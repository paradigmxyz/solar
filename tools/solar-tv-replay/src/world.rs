//! Independent, journaled EVM execution for explicit-world proofs.
//!
//! Rebuilds the original IR through the compiler rather than accepting the Python importer's
//! bytecode or instruction recipe. The existing evm2 engine executes nested calls, constructors,
//! precompiles, refunds and rollback. A custom entry handler avoids transaction fees,
//! nonce changes and intrinsic gas; an inspector supplies the explicit entry stack and budget.

use super::Request;
use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, TxResult,
    bytecode::Bytecode,
    ethereum::{TxEnvelope, execute_initial_frame, prepare_initial_frame, warm_base_accounts},
    evm::{AccountInfo, InMemoryDB, inspector::Inspector},
    interpreter::{Gas, GasTracker, InstrStop, Interpreter},
    registry::{TxRegistry, TxRequest},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{cell::RefCell, collections::BTreeMap, fs, process::Command, rc::Rc};

mod context;
mod mir;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Input {
    #[serde(default)]
    creation_gas: Vec<u64>,
    compiler: String,
    address: String,
    caller: String,
    origin: String,
    gas_price: String,
    chain_id: String,
    blob_hashes: Vec<String>,
    warm_addresses: Vec<String>,
    block: context::Block,
    memory: InitialMemory,
    gas: Option<u64>,
    #[serde(rename = "static")]
    is_static: bool,
    calldata: Vec<u8>,
    accounts: Vec<Account>,
    warm_slots: Vec<(String, String)>,
    #[serde(default)]
    gas_policy: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Account {
    address: String,
    nonce: u64,
    balance: String,
    code: Vec<u8>,
    storage: Vec<(String, String)>,
    #[serde(default)]
    transient: Vec<(String, String)>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct InitialMemory {
    data: Vec<u8>,
    size: u64,
}

fn word(request: &Request, text: &str) -> Result<U256, String> {
    let text = if let Some(name) = text.strip_prefix('$') {
        request.inputs.get(name).ok_or("missing symbolic world input")?
    } else {
        text
    };
    U256::from_str_radix(text, 10).map_err(|error| error.to_string())
}

fn address(request: &Request, text: &str) -> Result<Address, String> {
    let value = word(request, text)?;
    if value >> 160 != U256::ZERO {
        return Err("unclean world address".into());
    }
    Ok(Address::from_slice(&value.to_be_bytes::<32>()[12..]))
}

fn number(address: Address) -> String {
    U256::from_be_slice(address.as_slice()).to_string()
}

fn code(request: &Request, input: &Input, text: &str) -> Result<Vec<u8>, String> {
    let directory = tempfile::tempdir().map_err(|error| error.to_string())?;
    let path = directory
        .path()
        .join(format!("entry.{}", if request.kind == "mir" { "mir" } else { "evmir" }));
    fs::write(&path, text).map_err(|error| error.to_string())?;
    let mut command = Command::new(&input.compiler);
    command
        .arg(path)
        .args(["-Onone", "--emit=bin-runtime", "--allow=2264"])
        .arg(format!("--evm-version={}", request.evm_version));
    if request.kind == "evmir" {
        command.arg("-Zevm-ir-pipeline=none");
    }
    let output = command.output().map_err(|error| error.to_string())?;
    if !output.status.success() {
        return Err(format!(
            "world replay compiler rejected IR: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let stdout = String::from_utf8(output.stdout).map_err(|error| error.to_string())?;
    let artifact = stdout.lines().last().ok_or("missing world artifact")?;
    let value: Value = serde_json::from_str(artifact).map_err(|error| error.to_string())?;
    let artifacts = value["contracts"].as_object().ok_or("missing world runtime")?;
    if artifacts.len() != 1 {
        return Err("world replay requires exactly one runtime".into());
    }
    let encoded = artifacts.values().next().unwrap()["bin-runtime"]
        .as_str()
        .ok_or("missing world runtime bytes")?;
    hex::decode(encoded).map_err(|error| error.to_string())
}

#[derive(Default)]
struct Snapshot {
    gas: u64,
    refund: i64,
}

struct Observer {
    stack: Vec<U256>,
    gas: u64,
    is_static: bool,
    snapshot: Rc<RefCell<Snapshot>>,
}

impl Inspector<BaseEvmTypes> for Observer {
    fn initialize_interp(&mut self, interp: &mut Interpreter<'_, '_, BaseEvmTypes>) {
        if interp.message().depth == 0 {
            interp.set_gas(Gas::new(self.gas));
            interp.set_static(self.is_static);
            for value in self.stack.iter().rev() {
                interp.stack_mut().push(*value).expect("checked root stack height");
            }
        }
    }

    fn step_end(&mut self, interp: &mut Interpreter<'_, '_, BaseEvmTypes>) {
        if interp.message().depth == 0 {
            let mut snapshot = self.snapshot.borrow_mut();
            snapshot.gas = interp.gas().remaining();
            snapshot.refund = interp.gas().refunded();
        }
    }
}

pub(super) fn spec(text: &str) -> Result<SpecId, String> {
    Ok(match text {
        "istanbul" => SpecId::ISTANBUL,
        "berlin" => SpecId::BERLIN,
        "london" => SpecId::LONDON,
        "paris" => SpecId::MERGE,
        "shanghai" => SpecId::SHANGHAI,
        "cancun" => SpecId::CANCUN,
        "prague" => SpecId::PRAGUE,
        "osaka" => SpecId::OSAKA,
        "amsterdam" => SpecId::AMSTERDAM,
        _ => return Err("unsupported explicit-world fork".into()),
    })
}

pub(super) fn replay(request: &Request, input: &Input) -> Result<[String; 2], String> {
    if request.kind == "mir" {
        return mir::replay(request, input);
    }
    let budget = input.gas.ok_or("missing EVM entry gas budget")?;
    if !matches!(request.kind.as_str(), "mir" | "evmir")
        || budget > 30_000_000
        || input.calldata.len() > 65536
        || input.accounts.len() > 33
        || input.warm_slots.len() > 64
        || input.warm_addresses.len() > 64
        || input.blob_hashes.len() > 64
        || input.block.hashes.len() > 256
        || input.memory.size != 0
        || !input.memory.data.is_empty()
        || input.accounts.iter().any(|account| {
            account.code.len() > 65536
                || account.storage.len() > 64
                || !account.transient.is_empty()
        })
    {
        return Err("explicit-world replay limits exceeded".into());
    }
    let stack = super::words(request, "stack")?;
    if stack.len() > 1024 || (request.kind == "mir" && !stack.is_empty()) {
        return Err("invalid explicit-world entry stack".into());
    }
    let execute = |text: &str| -> Result<String, String> {
        let root = address(request, &input.address)?;
        let caller = address(request, &input.caller)?;
        let code = code(request, input, text)?;
        let block = context::block(request, input)?;
        let environment = context::transaction(request, input)?;
        let database = database(request, input, Some(&code))?;
        // The observation is the complete fixture world, including untouched accounts and
        // slots. Seed the overlay as well as its backing database so lazy reads cannot omit
        // those cells from the final snapshot.
        let initial = database.cache.clone();
        let spec = spec(&request.evm_version)?;
        let mut registry = TxRegistry::<BaseEvmTypes, TxResult>::new();
        let data = Bytes::copy_from_slice(&input.calldata);
        registry.register(
            0,
            |tx| match tx {
                TxEnvelope::Legacy(tx) => Some(tx),
                _ => None,
            },
            move |request: TxRequest<'_, '_, BaseEvmTypes, TxLegacy>| {
                warm_base_accounts(request.host, caller, TxKind::Call(root));
                let mut gas = GasTracker::new(budget);
                let frame = prepare_initial_frame(
                    request.host,
                    caller,
                    0,
                    TxKind::Call(root),
                    &data,
                    U256::ZERO,
                    &mut gas,
                )?;
                let result =
                    execute_initial_frame(request.host, &environment, frame, &mut gas, budget, 0);
                Ok(TxResult::<BaseEvmTypes> {
                    status: result.stop.is_success(),
                    stop: result.stop,
                    output: result.output,
                    ..Default::default()
                })
            },
        );
        let mut evm =
            Evm::<BaseEvmTypes>::new(spec, block, registry, database, Precompiles::base(spec));
        evm.overlay_db_mut().cache = initial;
        for addr in &input.warm_addresses {
            evm.state_mut().prewarm(&address(request, addr)?);
        }
        for (addr, slot) in &input.warm_slots {
            evm.state_mut().prewarm_storage_slot(&address(request, addr)?, word(request, slot)?);
        }
        let snapshot = Rc::new(RefCell::new(Snapshot { gas: budget, refund: 0 }));
        evm.set_inspector(Observer {
            stack: stack.clone(),
            gas: budget,
            is_static: input.is_static,
            snapshot: snapshot.clone(),
        });
        let tx = Recovered::new_unchecked(
            TxEnvelope::Legacy(TxLegacy {
                to: TxKind::Call(root),
                gas_limit: budget,
                ..Default::default()
            }),
            caller,
        );
        let outcome =
            evm.transact(&tx).map(evm2::ExecutedTx::commit).map_err(|error| error.to_string())?;
        let snapshot = snapshot.borrow();
        let stop = match outcome.stop {
            stop if stop.is_success() => "success",
            InstrStop::Revert => "revert",
            InstrStop::OutOfGas
            | InstrStop::MemoryOOG
            | InstrStop::MemoryLimitOOG
            | InstrStop::PrecompileOOG
            | InstrStop::InvalidOperandOOG
            | InstrStop::ReentrancySentryOOG
            | InstrStop::CreateInitCodeSizeLimit => "out-of-gas",
            _ => "exception",
        };
        let mut accounts = BTreeMap::new();
        for (&addr, account) in &evm.overlay_db().cache.accounts {
            let Some(account) = account else { continue };
            let slots = evm
                .overlay_db()
                .cache
                .storage
                .get(&addr)
                .map(|storage| {
                    storage
                        .slots
                        .iter()
                        .filter(|(_, value)| !value.is_zero())
                        .map(|(key, value)| (*key, *value))
                        .collect::<BTreeMap<_, _>>()
                })
                .unwrap_or_default();
            let code = if addr == root {
                Vec::new()
            } else {
                account
                    .code
                    .as_ref()
                    .or_else(|| evm.overlay_db().cache.contracts.get(&account.code_hash))
                    .map(|code| code.original_byte_slice().to_vec())
                    .unwrap_or_default()
            };
            accounts.insert(
                addr,
                json!([
                    number(addr),
                    account.nonce.to_string(),
                    account.balance.to_string(),
                    code,
                    slots
                        .iter()
                        .map(|(key, value)| [key.to_string(), value.to_string()])
                        .collect::<Vec<_>>()
                ]),
            );
        }
        let logs = outcome
            .logs
            .iter()
            .map(|log| {
                json!([
                    number(log.address),
                    log.data
                        .topics()
                        .iter()
                        .map(|topic| U256::from_be_slice(topic.as_slice()).to_string())
                        .collect::<Vec<_>>(),
                    log.data.data.to_vec()
                ])
            })
            .collect::<Vec<_>>();
        let paid = input.gas_policy.is_some();
        Ok(json!([
            stop,
            outcome.output.to_vec(),
            if paid {
                if matches!(stop, "exception" | "out-of-gas") { 0 } else { snapshot.gas }
            } else {
                0
            },
            if paid && stop == "success" { snapshot.refund } else { 0 },
            accounts.into_values().collect::<Vec<_>>(),
            logs
        ])
        .to_string())
    };
    Ok([execute(&request.before)?, execute(&request.after)?])
}

/// The explicit fixture database; root code is supplied only for EVM IR replay.
fn database(
    request: &Request,
    input: &Input,
    root_code: Option<&[u8]>,
) -> Result<InMemoryDB, String> {
    let root = address(request, &input.address)?;
    let mut database = InMemoryDB::default();
    for account in &input.accounts {
        let addr = address(request, &account.address)?;
        let bytes = if addr == root { root_code.unwrap_or(&account.code) } else { &account.code };
        database.insert_account_info(
            &addr,
            AccountInfo::default()
                .with_nonce(account.nonce)
                .with_balance(word(request, &account.balance)?)
                .with_code(if bytes.len() == 23 && bytes.starts_with(&[0xef, 0x01, 0x00]) {
                    Bytecode::new_eip7702(Address::from_slice(&bytes[3..]))
                } else {
                    Bytecode::new_legacy(Bytes::copy_from_slice(bytes))
                }),
        );
        for (key, value) in &account.storage {
            database.insert_account_storage(&addr, &word(request, key)?, &word(request, value)?);
        }
    }
    // Missing history is explicitly zero in the fixture. EmptyDB's synthetic hashes
    // are unsuitable here: seed the complete admissible BLOCKHASH window first.
    let current = word(request, &input.block.number)?;
    for distance in 1..=256u64 {
        if let Some(number) = current.checked_sub(U256::from(distance)) {
            database.insert_block_hash(&number, &Default::default());
        }
    }
    for (number, hash) in &input.block.hashes {
        database.insert_block_hash(&word(request, number)?, &word(request, hash)?.into());
    }
    Ok(database)
}

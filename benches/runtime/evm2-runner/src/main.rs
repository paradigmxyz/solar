//! Replays recorded transactions on evm2's interpreter and counts the host instructions each one
//! costs.
//!
//! The input is one JSON document: the chain ID, the node's hardfork, and a list of calls. Each
//! call carries the transaction, its block header, and the prestate it touched, as the node's
//! `eth_getTransactionByHash`, `eth_getBlockByNumber`, and `debug_traceTransaction` (with
//! `prestateTracer`) return them, plus the gas the node's receipt reports. Every call is rebuilt
//! from that state, checked against the receipt's gas, and executed repeatedly with its state
//! discarded after each run. The same transaction then runs against a target account whose code is
//! a single STOP, which measures the cost the contract's code does not cause.
//!
//! The output is one JSON document on stdout. Counts come from user-mode hardware counters of this
//! thread, so they are comparable only between runs of one binary.

use alloy_consensus::{TxEip1559, transaction::Recovered};
use alloy_primitives::{Address, B256, Bytes, TxKind, U64, U256};
use evm2::{
    BaseEvmTypes, Evm, ExecutionConfig, Inspector, Precompiles, SpecId, TxResult, Version,
    bytecode::Bytecode,
    env::BlockEnvExt,
    ethereum::{RecoveredTxEnvelope, TxEnvelope, ethereum_tx_registry},
    evm::{AccountInfo, InMemoryDB},
    interpreter::Interpreter,
};
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, process::ExitCode};

const EVM2_REV: &str = env!("EVM2_REV");

const USAGE: &str = "usage: evm2-runner INPUT.json | --check";

/// Timed batches per measurement; the result is the median batch.
const BATCHES: u64 = 7;

/// Minimum user-mode cycles per batch; sets how many repetitions a batch runs.
const BATCH_CYCLES: u64 = 200_000;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Input {
    chain_id: U64,
    hardfork: String,
    calls: Vec<Call>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Call {
    tx: RpcTx,
    block: RpcBlock,
    prestate: HashMap<Address, RpcAccount>,
    gas_used: U64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcTx {
    #[serde(rename = "type", default)]
    ty: U64,
    chain_id: U64,
    from: Address,
    to: Option<Address>,
    nonce: U64,
    gas: U64,
    value: U256,
    input: Bytes,
    max_fee_per_gas: U256,
    max_priority_fee_per_gas: U256,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RpcBlock {
    number: U64,
    timestamp: U64,
    gas_limit: U64,
    miner: Address,
    base_fee_per_gas: Option<U256>,
    mix_hash: Option<B256>,
    #[serde(default)]
    difficulty: U256,
}

#[derive(Deserialize)]
struct RpcAccount {
    #[serde(default)]
    balance: U256,
    #[serde(default)]
    nonce: u64,
    #[serde(default)]
    code: Bytes,
    #[serde(default)]
    storage: HashMap<U256, U256>,
}

#[derive(Serialize)]
struct Output {
    evm2_rev: &'static str,
    calls: Vec<CallOutput>,
}

#[derive(Serialize)]
#[serde(rename_all = "snake_case")]
enum CallOutput {
    Measurement(Measurement),
    Error(String),
}

/// The per-execution costs of one call. `net_instructions` is `instructions` minus the cost of the
/// same transaction on a STOP contract.
#[derive(Serialize)]
struct Measurement {
    instructions: u64,
    net_instructions: i64,
    cycles: u64,
    steps: u64,
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("evm2-runner: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), String> {
    let path = match std::env::args().nth(1).ok_or(USAGE)?.as_str() {
        "--check" => return Counters::new().map(drop),
        path => path.to_string(),
    };
    let input = serde_json::from_slice::<Input>(
        &std::fs::read(&path).map_err(|error| format!("cannot read {path}: {error}"))?,
    )
    .map_err(|error| format!("invalid input {path}: {error}"))?;
    let spec = spec_id(&input.hardfork)?;
    let counters = Counters::new()?;
    let calls = input
        .calls
        .iter()
        .map(|call| match measure(&input, spec, call, &counters) {
            Ok(measurement) => CallOutput::Measurement(measurement),
            Err(error) => CallOutput::Error(error),
        })
        .collect();
    println!("{}", serde_json::to_string(&Output { evm2_rev: EVM2_REV, calls }).unwrap());
    Ok(())
}

/// Maps the node's hardfork name to evm2's spec. Forks that only change blob parameters run the
/// spec they follow.
fn spec_id(hardfork: &str) -> Result<SpecId, String> {
    let name = hardfork.to_ascii_lowercase().replace(['_', '-', ' '], "");
    Ok(match name.as_str() {
        "frontier" => SpecId::FRONTIER,
        "homestead" => SpecId::HOMESTEAD,
        "tangerine" | "tangerinewhistle" => SpecId::TANGERINE,
        "spuriousdragon" => SpecId::SPURIOUS_DRAGON,
        "byzantium" => SpecId::BYZANTIUM,
        "constantinople" | "petersburg" | "constantinoplefix" => SpecId::PETERSBURG,
        "istanbul" => SpecId::ISTANBUL,
        "berlin" => SpecId::BERLIN,
        "london" => SpecId::LONDON,
        "merge" | "paris" => SpecId::MERGE,
        "shanghai" => SpecId::SHANGHAI,
        "cancun" => SpecId::CANCUN,
        "prague" => SpecId::PRAGUE,
        "osaka" | "bpo1" | "bpo2" | "bpo3" | "bpo4" | "bpo5" => SpecId::OSAKA,
        "amsterdam" => SpecId::AMSTERDAM,
        _ => return Err(format!("unsupported hardfork `{hardfork}`")),
    })
}

fn measure(
    input: &Input,
    spec: SpecId,
    call: &Call,
    counters: &Counters,
) -> Result<Measurement, String> {
    let target = call.tx.to.ok_or("contract creations are not replayed")?;
    let tx = recovered_tx(&call.tx)?;
    let mut evm = new_evm(input, spec, call, None)?;
    let result = evm.transact(&tx).map_err(|error| format!("{error:?}"))?.discard();
    check_receipt(&result, call)?;
    let mut steps = Steps(0);
    let inspected = evm.transact_with_inspector(&tx, &mut steps);
    drop(inspected.map_err(|error| format!("{error:?}"))?.discard());

    let (cycles, instructions) =
        counters.median(|| evm.transact(&tx).map(|r| drop(r.discard())))?;

    // The same transaction on a contract that stops at once.
    let mut stop_evm = new_evm(input, spec, call, Some(target))?;
    let (_, stop_instructions) =
        counters.median(|| stop_evm.transact(&tx).map(|r| drop(r.discard())))?;

    Ok(Measurement {
        instructions,
        net_instructions: instructions as i64 - stop_instructions as i64,
        cycles,
        steps: steps.0,
    })
}

/// A replay must reproduce what the node recorded, or its instruction count describes another
/// execution.
fn check_receipt(result: &TxResult<BaseEvmTypes>, call: &Call) -> Result<(), String> {
    let gas_used = call.gas_used.to::<u64>();
    if !result.status || result.tx_gas_used() != gas_used {
        return Err(format!(
            "replay used {} gas with status {}, the receipt has {gas_used} gas and status true",
            result.tx_gas_used(),
            result.status
        ));
    }
    Ok(())
}

/// Builds the EVM for one call. With `stop_target`, that account's code is a single STOP.
fn new_evm(
    input: &Input,
    spec: SpecId,
    call: &Call,
    stop_target: Option<Address>,
) -> Result<Evm<'static, BaseEvmTypes>, String> {
    let mut db = InMemoryDB::default();
    for (address, account) in &call.prestate {
        let mut info =
            AccountInfo::default().with_balance(account.balance).with_nonce(account.nonce);
        if stop_target == Some(*address) {
            info = info.with_code(Bytecode::new_legacy(Bytes::from_static(&[0x00])));
        } else if !account.code.is_empty() {
            let code = Bytecode::new_raw_checked(account.code.clone())
                .map_err(|error| format!("invalid code of {address}: {error:?}"))?;
            info = info.with_code(code);
        }
        db.insert_account_info(address, info);
        for (key, value) in &account.storage {
            db.insert_account_storage(address, key, value);
        }
    }

    // Like the node, which lifts the code size limits, and with a gas limit cap above the 80M the
    // calls use.
    let mut version = *Version::base(spec);
    version.chain_id = input.chain_id.to();
    version.max_code_size = usize::MAX;
    version.max_initcode_size = usize::MAX;
    version.tx_gas_limit_cap = u64::MAX;
    let block = &call.block;
    Ok(Evm::<BaseEvmTypes>::new_with_execution_config(
        ExecutionConfig::for_spec_and_version(spec, version),
        spec,
        BlockEnvExt {
            number: block.number.to(),
            beneficiary: block.miner,
            timestamp: block.timestamp.to(),
            gas_limit: block.gas_limit.to(),
            basefee: block.base_fee_per_gas.unwrap_or_default(),
            difficulty: block.difficulty,
            prevrandao: block.mix_hash.map(Into::into).unwrap_or_default(),
            ..Default::default()
        },
        ethereum_tx_registry(spec),
        db,
        Precompiles::base(spec),
    ))
}

/// Rebuilds the EIP-1559 transaction `cast send` makes.
fn recovered_tx(tx: &RpcTx) -> Result<RecoveredTxEnvelope, String> {
    if tx.ty.to::<u8>() != 2 {
        return Err(format!("unsupported transaction type {}", tx.ty));
    }
    let envelope = TxEnvelope::Eip1559(TxEip1559 {
        chain_id: tx.chain_id.to(),
        nonce: tx.nonce.to(),
        gas_limit: tx.gas.to(),
        max_fee_per_gas: tx.max_fee_per_gas.to(),
        max_priority_fee_per_gas: tx.max_priority_fee_per_gas.to(),
        to: tx.to.map_or(TxKind::Create, TxKind::Call),
        value: tx.value,
        access_list: Default::default(),
        input: tx.input.clone(),
    });
    Ok(Recovered::new_unchecked(envelope, tx.from))
}

/// Counts the executed instructions.
struct Steps(u64);

impl Inspector<BaseEvmTypes> for Steps {
    fn step(&mut self, _interp: &mut Interpreter<'_, '_, BaseEvmTypes>) {
        self.0 += 1;
    }
}

/// User-mode cycle and instruction counters for the calling thread.
struct Counters {
    cycles: i32,
    instructions: i32,
}

impl Counters {
    fn new() -> Result<Self, String> {
        Ok(Self { cycles: perf_counter(0)?, instructions: perf_counter(1)? })
    }

    fn read(fd: i32) -> u64 {
        let mut value = 0u64;
        let n = unsafe { libc::read(fd, (&raw mut value).cast(), 8) };
        assert_eq!(n, 8, "failed to read perf counter");
        value
    }

    fn measure(&self, f: impl FnOnce()) -> (u64, u64) {
        let (c0, i0) = (Self::read(self.cycles), Self::read(self.instructions));
        f();
        let (c1, i1) = (Self::read(self.cycles), Self::read(self.instructions));
        (c1 - c0, i1 - i0)
    }

    /// Returns the median (cycles, instructions) of one execution over the batches. One untimed
    /// execution warms the caches, then the repetitions per batch fill `BATCH_CYCLES`.
    fn median<E: std::fmt::Debug>(
        &self,
        mut f: impl FnMut() -> Result<(), E>,
    ) -> Result<(u64, u64), String> {
        f().map_err(|error| format!("{error:?}"))?;
        let (cycles, _) = self.measure(|| f().unwrap());
        let reps = (BATCH_CYCLES / cycles.max(1)).clamp(1, 10_000);
        let (mut cycle_samples, mut instruction_samples) = (vec![], vec![]);
        for _ in 0..BATCHES {
            let (cycles, instructions) = self.measure(|| {
                for _ in 0..reps {
                    f().unwrap();
                }
            });
            cycle_samples.push(cycles.div_ceil(reps));
            instruction_samples.push(instructions.div_ceil(reps));
        }
        let median = |samples: &mut Vec<u64>| {
            samples.sort_unstable();
            samples[samples.len() / 2]
        };
        Ok((median(&mut cycle_samples), median(&mut instruction_samples)))
    }
}

/// Opens a user-mode hardware counter (`PERF_TYPE_HARDWARE`) for the calling thread.
fn perf_counter(config: u64) -> Result<i32, String> {
    // `perf_event_attr`, version 8 (136 bytes): type and size, config, then flags at word 5.
    let mut attr = [0u64; 17];
    attr[0] = 136u64 << 32;
    attr[1] = config;
    // exclude_kernel | exclude_hv
    attr[5] = (1 << 5) | (1 << 6);
    let fd = unsafe { libc::syscall(libc::SYS_perf_event_open, attr.as_ptr(), 0, -1, -1, 0) };
    if fd < 0 {
        return Err(format!(
            "perf_event_open failed: {}; user-mode hardware counters need a CPU PMU and \
             perf_event_paranoid <= 2 or CAP_PERFMON",
            std::io::Error::last_os_error()
        ));
    }
    Ok(fd as i32)
}

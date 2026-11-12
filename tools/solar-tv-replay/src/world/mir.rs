//! Independent direct MIR state replay. Original IR is reparsed and interpreted,
//! never compiled to EVM or reconstructed from the Lean importer's model.

use super::{Input, Request, word};
use alloy_primitives::U256;
use serde_json::json;
use solar_config::EvmVersion;
use solar_evmir::op;
use solar_mir_interpreter::{
    Host, Options,
    replay::{StateInput, StateOutcome, run_state},
};
use std::{collections::BTreeMap, str::FromStr};

struct Context {
    storage: BTreeMap<U256, U256>,
    transient: BTreeMap<U256, U256>,
    reads: BTreeMap<u8, U256>,
    history: BTreeMap<U256, U256>,
    blobs: Vec<U256>,
    static_permission: bool,
}

impl Host for Context {
    fn storage(&mut self, slot: U256) -> U256 {
        self.storage.get(&slot).copied().unwrap_or_default()
    }
    fn transient(&mut self, slot: U256) -> U256 {
        self.transient.get(&slot).copied().unwrap_or_default()
    }
    fn is_static(&self) -> bool {
        self.static_permission
    }
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        match (opcode, operands) {
            (op::BLOCKHASH, &[number]) => {
                let current = self.reads[&op::NUMBER];
                Some(if number < current && current - number <= U256::from(256) {
                    self.history.get(&number).copied().unwrap_or_default()
                } else {
                    U256::ZERO
                })
            }
            (op::BLOBHASH, &[index]) => Some(
                usize::try_from(index)
                    .ok()
                    .and_then(|n| self.blobs.get(n))
                    .copied()
                    .unwrap_or_default(),
            ),
            (_, []) => self.reads.get(&opcode).copied(),
            _ => None,
        }
    }
}

pub(super) fn replay(request: &Request, input: &Input) -> Result<[String; 2], String> {
    if request.gas || input.gas_policy.is_some() || input.gas.is_some() {
        return Err("direct MIR replay does not assign EVM gas costs".into());
    }
    if input.memory.size > 65536
        || !input.memory.size.is_multiple_of(32)
        || input.memory.data.len() as u64 > input.memory.size
        || input.calldata.len() > 65536
        || input.accounts.len() > 33
        || input.accounts.iter().any(|account| {
            account.storage.len() > 64 || account.transient.len() > 64 || account.code.len() > 65536
        })
        || input.block.hashes.len() > 256
        || input.blob_hashes.len() > 64
    {
        return Err("direct MIR replay input limit".into());
    }
    let owner = word(request, &input.address)?;
    let root = input
        .accounts
        .iter()
        .find(|a| word(request, &a.address).ok() == Some(owner))
        .ok_or("missing MIR owner")?;
    let cells = |values: &[(String, String)]| {
        values
            .iter()
            .map(|(k, v)| Ok((word(request, k)?, word(request, v)?)))
            .collect::<Result<BTreeMap<_, _>, String>>()
    };
    let persistent = cells(&root.storage)?;
    let transient = cells(&root.transient)?;
    let version = EvmVersion::from_str(&request.evm_version).map_err(|e| e.to_string())?;
    let fields = [
        (op::ADDRESS, input.address.as_str()),
        (op::CALLER, input.caller.as_str()),
        (op::ORIGIN, input.origin.as_str()),
        (op::GASPRICE, input.gas_price.as_str()),
        (op::SELFBALANCE, root.balance.as_str()),
        (op::COINBASE, input.block.coinbase.as_str()),
        (op::TIMESTAMP, input.block.timestamp.as_str()),
        (op::NUMBER, input.block.number.as_str()),
        (
            op::PREVRANDAO,
            if version >= EvmVersion::Paris {
                input.block.prevrandao.as_str()
            } else {
                input.block.difficulty.as_str()
            },
        ),
        (op::GASLIMIT, input.block.gas_limit.as_str()),
        (op::CHAINID, input.chain_id.as_str()),
        (op::BASEFEE, input.block.base_fee.as_str()),
        (op::BLOBBASEFEE, input.block.blob_base_fee.as_str()),
        (op::SLOTNUM, input.block.slot.as_str()),
        (op::CALLVALUE, "0"),
    ];
    let reads = fields
        .into_iter()
        .map(|(op, v)| Ok((op, word(request, v)?)))
        .collect::<Result<BTreeMap<_, _>, String>>()?;
    let history = cells(&input.block.hashes)?;
    let blobs =
        input.blob_hashes.iter().map(|v| word(request, v)).collect::<Result<Vec<_>, _>>()?;
    let args = super::super::words(request, "arg")?;
    let execute = |text: &str| -> Result<String, String> {
        let mut host = Context {
            storage: persistent.clone(),
            transient: transient.clone(),
            reads: reads.clone(),
            history: history.clone(),
            blobs: blobs.clone(),
            static_permission: input.is_static,
        };
        let execution = run_state(
            text,
            StateInput {
                function: request.selector.as_deref(),
                arguments: &args,
                memory: &input.memory.data,
                memory_size: input.memory.size,
                calldata: &input.calldata,
            },
            &mut host,
            Options { evm_version: version, fuel: 1_000_000, depth: 128 },
        )?;
        let (status, values, data, memory, keep_transient) = match execution.outcome {
            StateOutcome::Return(values) => {
                ("return", values, Vec::new(), Some(execution.memory), true)
            }
            StateOutcome::Success(data) => ("success", Vec::new(), data, None, false),
            StateOutcome::Revert(data) => ("revert", Vec::new(), data, None, false),
            StateOutcome::Invalid => ("exception", Vec::new(), Vec::new(), None, false),
            StateOutcome::Unsupported(reason) => return Err(reason),
        };
        host.storage.extend(execution.storage);
        host.transient.extend(execution.transient);
        let normalize = |slots: BTreeMap<U256, U256>| {
            slots
                .into_iter()
                .filter(|(_, v)| !v.is_zero())
                .map(|(k, v)| [k.to_string(), v.to_string()])
                .collect::<Vec<_>>()
        };
        let mut accounts = BTreeMap::new();
        for account in &input.accounts {
            let address = word(request, &account.address)?;
            let slots =
                if address == owner { host.storage.clone() } else { cells(&account.storage)? };
            let transient = if !keep_transient {
                BTreeMap::new()
            } else if address == owner {
                host.transient.clone()
            } else {
                cells(&account.transient)?
            };
            accounts.insert(
                address,
                json!([
                    address.to_string(),
                    account.nonce.to_string(),
                    word(request, &account.balance)?.to_string(),
                    account.code,
                    normalize(slots),
                    normalize(transient)
                ]),
            );
        }
        let logs = execution
            .logs
            .iter()
            .map(|log| {
                json!([
                    owner.to_string(),
                    log.topics.iter().map(|v| v.to_string()).collect::<Vec<_>>(),
                    log.data
                ])
            })
            .collect::<Vec<_>>();
        Ok(json!({"outcome":status,"values":values.iter().map(|v|v.to_string()).collect::<Vec<_>>(),"data":data,"memory":memory,"accounts":accounts.into_values().collect::<Vec<_>>(),"logs":logs}).to_string())
    };
    Ok([execute(&request.before)?, execute(&request.after)?])
}

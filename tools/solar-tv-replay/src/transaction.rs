//! Independent checks of transaction admission and the Amsterdam gas meter.

use alloy_consensus::{TxEip7702, transaction::Recovered};
use alloy_primitives::{Address, Bytes, U256};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, Version,
    bytecode::Bytecode,
    env::BlockEnv,
    ethereum::{LazyTxEip7702, TxEnvelope, ethereum_tx_registry, initial_gas_and_reservoir},
    evm::{AccountInfo, InMemoryDB},
    interpreter::{GasTracker, InstrStop},
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::io::{self, Read};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    SetCode {
        fork: String,
        sender: Address,
        block_gas_limit: u64,
        base_fee: U256,
        accounts: Vec<Account>,
        inspect: Vec<Address>,
        tx: Box<TxEip7702>,
    },
    Reservoir {
        limit: u32,
        intrinsic: u32,
        charges: Vec<Debit>,
    },
    NestedReservoir {
        limit: u32,
        intrinsic: u32,
        actions: Vec<FrameAction>,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Account {
    address: Address,
    nonce: u64,
    balance: U256,
    code: Bytes,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Debit {
    Execution(u32),
    State(u32),
    Refill(u32),
    Rollback(()),
}

pub(super) fn replay() -> Result<Value, String> {
    let mut bytes = Vec::new();
    io::stdin().lock().take(131_073).read_to_end(&mut bytes).map_err(|e| e.to_string())?;
    if bytes.len() > 131_072 {
        return Err("transaction replay request exceeds 128 KiB".into());
    }
    match serde_json::from_slice(&bytes).map_err(|e| e.to_string())? {
        Input::SetCode { fork, sender, block_gas_limit, base_fee, accounts, inspect, tx } => {
            if accounts.len() > 32
                || inspect.len() > 32
                || tx.authorization_list.len() > 16
                || tx.input.len() > 4096
                || tx.access_list.len() > 32
                || accounts.iter().any(|a| a.code.len() > 4096)
            {
                return Err("transaction replay fixture exceeds limits".into());
            }
            let spec = super::world::spec(&fork)?;
            let mut db = InMemoryDB::default();
            for account in accounts {
                let code = if account.code.len() == 23 && account.code.starts_with(&[0xef, 1, 0]) {
                    Bytecode::new_eip7702(Address::from_slice(&account.code[3..]))
                } else {
                    Bytecode::new_legacy(account.code)
                };
                db.insert_account_info(
                    &account.address,
                    AccountInfo::default()
                        .with_nonce(account.nonce)
                        .with_balance(account.balance)
                        .with_code(code),
                );
            }
            let block = BlockEnv::<BaseEvmTypes> {
                gas_limit: U256::from(block_gas_limit),
                basefee: base_fee,
                ..Default::default()
            };
            let registry = ethereum_tx_registry(spec);
            let initial = db.cache.clone();
            let mut evm =
                Evm::<BaseEvmTypes>::new(spec, block, registry, db, Precompiles::base(spec));
            evm.overlay_db_mut().cache = initial;
            let tx = LazyTxEip7702::from_signed_authorizations(*tx);
            let recovered =
                tx.authorization_list.iter().map(|auth| auth.authority()).collect::<Vec<_>>();
            let tx = Recovered::new_unchecked(TxEnvelope::Eip7702(tx), sender);
            let result = match evm.transact(&tx) {
                Ok(executed) => executed.commit(),
                Err(error) => {
                    return Ok(json!({
                        "admitted":false,"reason":error.to_string(),"authorities":recovered,
                    }));
                }
            };
            let cache = &evm.overlay_db().cache;
            let accounts = inspect
                .iter()
                .map(|address| {
                    cache.accounts.get(address).and_then(Option::as_ref).map(|account| {
                        let code = account
                            .code
                            .as_ref()
                            .or_else(|| cache.contracts.get(&account.code_hash));
                        json!([
                            account.nonce,
                            code.map(|code| code.original_byte_slice()).unwrap_or_default()
                        ])
                    })
                })
                .collect::<Vec<_>>();
            Ok(json!({"admitted":true,"success":result.status,"output":result.output.to_vec(),
                "authorities":recovered,
                "spent":result.total_gas_spent,"refund":result.refunded,"accounts":accounts}))
        }
        Input::NestedReservoir { limit, intrinsic, actions } => nested(limit, intrinsic, actions),
        Input::Reservoir { limit, intrinsic, charges } => {
            if charges.len() > 64 {
                return Err("gas replay permits at most 64 charges".into());
            }
            let Some(mut gas) = begin(limit, intrinsic) else {
                return Ok(Value::Null);
            };
            let snapshot = |meter: &GasTracker| {
                json!([
                    meter.remaining(),
                    meter.reservoir(),
                    meter.state_gas_spent(),
                    meter.state_gas_spilled()
                ])
            };
            let mut trace = vec![snapshot(&gas)];
            for charge in charges {
                if charge.apply(&mut gas).is_err() {
                    trace.push(Value::Null);
                    break;
                }
                trace.push(snapshot(&gas));
            }
            Ok(json!(trace))
        }
    }
}

impl Debit {
    fn apply(self, gas: &mut GasTracker) -> Result<(), InstrStop> {
        match self {
            Self::Execution(cost) => gas.spend(cost.into()),
            Self::State(cost) => gas.spend_state(cost.into()),
            Self::Refill(amount) => {
                gas.refill_reservoir(amount.into());
                Ok(())
            }
            Self::Rollback(()) => {
                gas.rollback_state_gas();
                Ok(())
            }
        }
    }
}

#[derive(Deserialize)]
#[serde(untagged)]
enum FrameAction {
    Debit(Debit),
    Frame(FrameChange),
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum FrameChange {
    Refund(i32),
    Enter(ChildGas),
    Finish(Exit),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ChildGas {
    forward: u32,
    stipend: u32,
    reserved: u32,
}

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
enum Exit {
    Success,
    Revert,
    Halt,
}

fn begin(limit: u32, intrinsic: u32) -> Option<GasTracker> {
    let version = Version::base(SpecId::AMSTERDAM);
    let (limit, intrinsic) = (u64::from(limit), u64::from(intrinsic));
    if intrinsic > limit.min(version.tx_gas_limit_cap) {
        return None;
    }
    let (execution, reservoir) = initial_gas_and_reservoir(version, limit, intrinsic, 0);
    Some(GasTracker::new_with_execution_gas_and_reservoir(execution, reservoir))
}

fn nested(limit: u32, intrinsic: u32, actions: Vec<FrameAction>) -> Result<Value, String> {
    if actions.len() > 64 {
        return Err("nested gas replay permits at most 64 actions".into());
    }
    let Some(mut gas) = begin(limit, intrinsic) else {
        return Ok(Value::Null);
    };

    let snapshot = |meter: &GasTracker, depth| {
        json!([
            meter.remaining(),
            meter.reservoir(),
            meter.state_gas_spent(),
            meter.state_gas_spilled(),
            meter.refunded(),
            depth,
        ])
    };
    let mut trace = vec![snapshot(&gas, 0)];
    let mut callers = Vec::new();
    for action in actions {
        match action {
            FrameAction::Debit(debit) => {
                if debit.apply(&mut gas).is_err() {
                    trace.push(Value::Null);
                    break;
                }
            }
            FrameAction::Frame(FrameChange::Refund(amount)) => gas.record_refund(amount.into()),
            FrameAction::Frame(FrameChange::Enter(ChildGas { forward, stipend, reserved })) => {
                if gas.spend_state(reserved.into()).is_err() || gas.spend(forward.into()).is_err() {
                    trace.push(Value::Null);
                    break;
                }
                callers.push((gas, reserved));
                gas = GasTracker::new_with_execution_gas_and_reservoir(
                    u64::from(forward) + u64::from(stipend),
                    gas.reservoir(),
                );
            }
            FrameAction::Frame(FrameChange::Finish(exit)) => {
                let Some((mut parent, reserved)) = callers.pop() else {
                    trace.push(Value::Null);
                    break;
                };
                let stop = match exit {
                    Exit::Success => InstrStop::Stop,
                    Exit::Revert => InstrStop::Revert,
                    Exit::Halt => InstrStop::OutOfGas,
                };
                gas.settle_gas(stop);
                parent.merge_child_gas(gas, stop);
                if !stop.is_success() {
                    parent.refill_reservoir(reserved.into());
                }
                gas = parent;
            }
        }
        trace.push(snapshot(&gas, callers.len()));
    }
    Ok(json!(trace))
}

//! Independent checks of transaction admission and the Amsterdam gas meter.

use alloy_consensus::{TxEip7702, transaction::Recovered};
use alloy_primitives::{Address, Bytes, U256};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, Version,
    bytecode::Bytecode,
    env::BlockEnv,
    ethereum::{LazyTxEip7702, TxEnvelope, ethereum_tx_registry, initial_gas_and_reservoir},
    evm::{AccountInfo, InMemoryDB},
    interpreter::GasTracker,
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
        Input::Reservoir { limit, intrinsic, charges } => {
            if charges.len() > 64 {
                return Err("gas replay permits at most 64 charges".into());
            }
            let version = Version::base(SpecId::AMSTERDAM);
            let (limit, intrinsic) = (u64::from(limit), u64::from(intrinsic));
            if intrinsic > limit.min(version.tx_gas_limit_cap) {
                return Ok(Value::Null);
            }
            let (execution, reservoir) = initial_gas_and_reservoir(version, limit, intrinsic, 0);
            let mut gas = GasTracker::new_with_execution_gas_and_reservoir(execution, reservoir);
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
                let result = match charge {
                    Debit::Execution(cost) => gas.spend(cost.into()),
                    Debit::State(cost) => gas.spend_state(cost.into()),
                    Debit::Refill(amount) => {
                        gas.refill_reservoir(amount.into());
                        Ok(())
                    }
                    Debit::Rollback(()) => {
                        gas.rollback_state_gas();
                        Ok(())
                    }
                };
                if result.is_err() {
                    trace.push(Value::Null);
                    break;
                }
                trace.push(snapshot(&gas));
            }
            Ok(json!(trace))
        }
    }
}

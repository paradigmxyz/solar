//! Independent checks of transaction admission and the Amsterdam gas meter.

use alloy_consensus::{TxEip1559, TxEip7702, transaction::Recovered};
use alloy_primitives::{Address, Bytes, U256};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId, TxResult, Version,
    bytecode::Bytecode,
    env::BlockEnv,
    ethereum::{LazyTxEip7702, TxEnvelope, ethereum_tx_registry, initial_gas_and_reservoir},
    evm::{AccountInfo, InMemoryDB},
    interpreter::{GasTracker, InstrStop},
    registry::HandlerError,
};
use serde::Deserialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    io::{self, Read},
};

#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum Input {
    SetCode {
        fork: String,
        sender: Address,
        block_gas_limit: u64,
        base_fee: U256,
        #[serde(default)]
        beneficiary: Address,
        accounts: Vec<Account>,
        inspect: Vec<Address>,
        tx: Box<TxEip7702>,
    },
    AmsterdamDynamicFee {
        sender: Address,
        block_gas_limit: u64,
        base_fee: U256,
        #[serde(default)]
        beneficiary: Address,
        accounts: Vec<Account>,
        inspect: Vec<Address>,
        tx: Box<TxEip1559>,
    },
    AmsterdamDynamicFeeBlock {
        block_gas_limit: u64,
        header_gas_used: u64,
        base_fee: U256,
        #[serde(default)]
        beneficiary: Address,
        accounts: Vec<Account>,
        inspect: Vec<Address>,
        transactions: Vec<SignedMessage>,
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
struct SignedMessage {
    sender: Address,
    tx: TxEip1559,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Account {
    address: Address,
    nonce: u64,
    balance: U256,
    code: Bytes,
    #[serde(default)]
    storage: Vec<(U256, U256)>,
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
        Input::SetCode {
            fork,
            sender,
            block_gas_limit,
            base_fee,
            beneficiary,
            accounts,
            inspect,
            tx,
        } => {
            if accounts.len() > 32
                || inspect.len() > 32
                || tx.authorization_list.len() > 16
                || tx.input.len() > 4096
                || tx.access_list.len() > 32
                || accounts.iter().any(|a| a.code.len() > 4096 || !a.storage.is_empty())
            {
                return Err("transaction replay fixture exceeds limits".into());
            }
            let spec = super::world::spec(&fork)?;
            let mut evm = transaction_evm(spec, block_gas_limit, base_fee, beneficiary, accounts);
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
            let balance = |address: &Address| {
                cache
                    .accounts
                    .get(address)
                    .and_then(Option::as_ref)
                    .map_or(U256::ZERO, |account| account.balance)
            };
            Ok(json!({"admitted":true,"success":result.status,"output":result.output.to_vec(),
                "authorities":recovered,
                "spent":result.total_gas_spent,"refund":result.refunded,"accounts":accounts,
                "floor_gas":result.floor_gas,"gas_used":result.tx_gas_used(),
                "execution_gas":result.execution_gas_spent(),"state_gas":result.state_gas_spent(),
                "sender_balance":balance(&sender),"beneficiary_balance":balance(&beneficiary)}))
        }
        Input::AmsterdamDynamicFee {
            sender,
            block_gas_limit,
            base_fee,
            beneficiary,
            accounts,
            inspect,
            tx,
        } => {
            check_dynamic_fixture(&accounts, &inspect, [&*tx])?;
            let mut evm = transaction_evm(
                SpecId::AMSTERDAM,
                block_gas_limit,
                base_fee,
                beneficiary,
                accounts,
            );
            let Some(result) = transact_dynamic_message(&mut evm, sender, *tx)? else {
                return Ok(json!({"admitted": false}));
            };
            Ok(dynamic_observation(&evm, &result, sender, beneficiary, &inspect))
        }
        Input::AmsterdamDynamicFeeBlock {
            block_gas_limit,
            header_gas_used,
            base_fee,
            beneficiary,
            accounts,
            inspect,
            transactions,
        } => {
            if transactions.len() > 16 {
                return Err("Amsterdam block fixture supports at most 16 messages".into());
            }
            check_dynamic_fixture(&accounts, &inspect, transactions.iter().map(|row| &row.tx))?;
            let mut evm = transaction_evm(
                SpecId::AMSTERDAM,
                block_gas_limit,
                base_fee,
                beneficiary,
                accounts,
            );
            let mut execution = 0u64;
            let mut state = 0u64;
            let mut cumulative = 0u64;
            let mut receipts = Vec::with_capacity(transactions.len());
            for row in transactions {
                let limit = row.tx.gas_limit;
                // The pinned block runner bounds transaction inclusion separately on both axes.
                if limit.min(evm.version().tx_gas_limit_cap)
                    > block_gas_limit.saturating_sub(execution)
                    || limit > block_gas_limit.saturating_sub(state)
                {
                    return Ok(json!({"admitted": false}));
                }
                let Some(result) = transact_dynamic_message(&mut evm, row.sender, row.tx)? else {
                    return Ok(json!({"admitted": false}));
                };
                execution += result.execution_gas_spent();
                state += result.state_gas_spent();
                cumulative += result.tx_gas_used();
                let mut receipt = dynamic_receipt(&result);
                receipt["cumulative_gas"] = json!(cumulative);
                receipts.push(receipt);
            }
            let gas_used = execution.max(state);
            Ok(json!({
                "admitted": true,
                "execution_gas": execution,
                "state_gas": state,
                "cumulative_gas": cumulative,
                "header_gas_used": gas_used,
                "header_matches": gas_used == header_gas_used,
                "receipts": receipts,
                "accounts": dynamic_accounts(&evm, &inspect)
            }))
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

fn check_dynamic_fixture<'a>(
    accounts: &[Account],
    inspect: &[Address],
    transactions: impl IntoIterator<Item = &'a TxEip1559>,
) -> Result<(), String> {
    if accounts.len() > 32
        || inspect.len() > 32
        || accounts.iter().any(|a| a.code.len() > 4096 || a.storage.len() > 64)
        || transactions.into_iter().any(|tx| {
            tx.gas_limit >= 1 << 32
                || tx.input.len() > 4096
                || !matches!(tx.to, alloy_primitives::TxKind::Call(_))
                || tx.access_list.len() > 32
                || tx.access_list.storage_keys_count() > 128
        })
    {
        return Err("Amsterdam dynamic-fee fixture exceeds the message replay scope".into());
    }
    Ok(())
}

fn transact_dynamic_message(
    evm: &mut Evm<'_, BaseEvmTypes>,
    sender: Address,
    tx: TxEip1559,
) -> Result<Option<TxResult>, String> {
    let tx = Recovered::new_unchecked(TxEnvelope::Eip1559(tx), sender);
    match evm.transact(&tx) {
        Ok(executed) => Ok(Some(executed.commit())),
        Err(
            HandlerError::InvalidNonce { .. }
            | HandlerError::InvalidChainId { .. }
            | HandlerError::IntrinsicGasTooLow { .. }
            | HandlerError::InsufficientFunds
            | HandlerError::RejectCallerWithCode
            | HandlerError::NonceOverflow
            | HandlerError::GasLimitMoreThanBlock { .. }
            | HandlerError::TxGasLimitGreaterThanCap { .. }
            | HandlerError::FeeCapLessThanBaseFee { .. }
            | HandlerError::PriorityFeeGreaterThanMaxFee,
        ) => Ok(None),
        Err(error) => Err(error.to_string()),
    }
}

fn transaction_evm(
    spec: SpecId,
    block_gas_limit: u64,
    base_fee: U256,
    beneficiary: Address,
    accounts: Vec<Account>,
) -> Evm<'static, BaseEvmTypes> {
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
        for (slot, value) in account.storage {
            db.insert_account_storage(&account.address, &slot, &value);
        }
    }
    let block = BlockEnv::<BaseEvmTypes> {
        gas_limit: U256::from(block_gas_limit),
        basefee: base_fee,
        beneficiary,
        ..Default::default()
    };
    let registry = ethereum_tx_registry(spec);
    let initial = db.cache.clone();
    let mut evm = Evm::<BaseEvmTypes>::new(spec, block, registry, db, Precompiles::base(spec));
    evm.overlay_db_mut().cache = initial;
    evm
}

fn dynamic_observation(
    evm: &Evm<'_, BaseEvmTypes>,
    result: &TxResult,
    sender: Address,
    beneficiary: Address,
    inspect: &[Address],
) -> Value {
    let cache = &evm.overlay_db().cache;
    let balance = |address: &Address| {
        cache
            .accounts
            .get(address)
            .and_then(Option::as_ref)
            .map_or(U256::ZERO, |account| account.balance)
    };
    let mut observation = dynamic_receipt(result);
    observation["admitted"] = json!(true);
    observation["sender_balance"] = json!(balance(&sender));
    observation["beneficiary_balance"] = json!(balance(&beneficiary));
    observation["accounts"] = json!(dynamic_accounts(evm, inspect));
    observation
}

fn dynamic_accounts(evm: &Evm<'_, BaseEvmTypes>, inspect: &[Address]) -> Vec<Option<Value>> {
    let cache = &evm.overlay_db().cache;
    inspect
        .iter()
        .map(|address| {
            cache.accounts.get(address).and_then(Option::as_ref).map(|account| {
                let code =
                    account.code.as_ref().or_else(|| cache.contracts.get(&account.code_hash));
                let slots = cache
                    .storage
                    .get(address)
                    .map(|storage| {
                        storage
                            .slots
                            .iter()
                            .filter(|(_, value)| !value.is_zero())
                            .map(|(key, value)| (*key, *value))
                            .collect::<BTreeMap<_, _>>()
                    })
                    .unwrap_or_default();
                json!([
                    account.nonce,
                    account.balance,
                    code.map(|code| code.original_byte_slice()).unwrap_or_default(),
                    slots.into_iter().map(|(key, value)| [key, value]).collect::<Vec<_>>()
                ])
            })
        })
        .collect()
}

fn dynamic_receipt(result: &TxResult) -> Value {
    let logs = result
        .logs
        .iter()
        .map(|log| json!([log.address, log.data.topics(), log.data.data.to_vec()]))
        .collect::<Vec<_>>();
    json!({"stop":super::world::stop_name(result.stop),"output":result.output.to_vec(),
        "spent":result.total_gas_spent,"refund":result.refunded,"floor_gas":result.floor_gas,
        "gas_used":result.tx_gas_used(),"execution_gas":result.execution_gas_spent(),
        "state_gas":result.state_gas_spent(),"logs":logs})
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

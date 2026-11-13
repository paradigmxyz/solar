//! Deploys creation code in evm2 and reports the runtime code the deployment installs, for
//! checking a certified contract's bytecode against what a chain would hold.
//!
//! Reads one JSON request, `{"version": 1, "evm_version": "osaka", "initcode": "<hex>"}`, from
//! standard input. The creation transaction is an ordinary legacy transaction from a funded
//! account with no value and no constructor arguments, through evm2's Ethereum transaction
//! handler, so this does not share code with the compiler or with the Lean model. A failed
//! deployment is an error, never empty code.

use alloy_consensus::{TxLegacy, transaction::Recovered};
use alloy_primitives::{Address, Bytes, TxKind, U256, hex, keccak256};
use evm2::{
    BaseEvmTypes, Evm, Precompiles, SpecId,
    env::BlockEnv,
    ethereum::{TxEnvelope, ethereum_tx_registry},
    evm::{AccountInfo, InMemoryDB},
};
use serde::Deserialize;
use serde_json::{Value, json};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    version: u32,
    evm_version: String,
    initcode: String,
}

/// The account that sends the creation transaction.
const CALLER: Address = Address::repeat_byte(0x10);

/// Deploys the request's creation code and returns the deployed runtime code with its hashes.
pub(super) fn deploy(input: &[u8]) -> Result<Value, String> {
    let request: Request = serde_json::from_slice(input).map_err(|e| e.to_string())?;
    if request.version != 1 {
        return Err(format!("unsupported deploy request version {}", request.version));
    }
    let spec = super::world::spec(&request.evm_version)?;
    let initcode = hex::decode(request.initcode.trim().trim_start_matches("0x"))
        .map_err(|e| format!("invalid initcode: {e}"))?;
    let mut database = InMemoryDB::default();
    database.insert_account_info(&CALLER, AccountInfo::default().with_balance(U256::MAX));
    let mut evm = Evm::<BaseEvmTypes>::new(
        spec,
        BlockEnv::<BaseEvmTypes>::default(),
        ethereum_tx_registry(spec),
        database,
        Precompiles::base(spec),
    );
    let gas_limit = if spec >= SpecId::AMSTERDAM { 100_000_000 } else { 10_000_000 };
    let tx = Recovered::new_unchecked(
        TxEnvelope::Legacy(TxLegacy {
            nonce: 0,
            to: TxKind::Create,
            input: Bytes::from(initcode),
            gas_price: 0,
            value: U256::ZERO,
            chain_id: None,
            gas_limit,
        }),
        CALLER,
    );
    let result = evm
        .transact(&tx)
        .map(evm2::ExecutedTx::commit)
        .map_err(|e| format!("transaction rejected: {e}"))?;
    if !result.status {
        return Err(format!(
            "deployment failed with {:?}: 0x{}",
            result.stop,
            hex::encode(&result.output)
        ));
    }
    let address = result.created_address.ok_or("the deployment created no account")?;
    let cache = &evm.overlay_db().cache;
    let account = cache
        .accounts
        .get(&address)
        .and_then(Option::as_ref)
        .ok_or("the created account is not in the state")?;
    let code = account
        .code
        .as_ref()
        .or_else(|| cache.contracts.get(&account.code_hash))
        .map(|code| code.original_byte_slice().to_vec())
        .ok_or("the created account has no code")?;
    if keccak256(&code) != account.code_hash {
        return Err("the created account's code does not hash to its code hash".into());
    }
    Ok(json!({
        "address": address.to_string(),
        "runtime": hex::encode(&code),
        "code_hash": account.code_hash.to_string(),
    }))
}

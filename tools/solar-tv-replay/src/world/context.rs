//! Explicit environment inputs for independent world replay.
//!
//! These configure the engine's native block and transaction environments; replay never
//! substitutes the Lean opcode results. The custom entry begins after transaction admission.

use super::{Input, Request, address, word};
use alloy_primitives::U256;
use evm2::{
    BaseEvmTypes,
    env::{BlockEnv, TxEnv},
};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct Block {
    pub(super) number: String,
    coinbase: String,
    timestamp: String,
    gas_limit: String,
    difficulty: String,
    prevrandao: String,
    base_fee: String,
    blob_base_fee: String,
    slot: String,
    pub(super) hashes: Vec<(String, String)>,
}

pub(super) fn block(request: &Request, input: &Input) -> Result<BlockEnv, String> {
    let block = &input.block;
    let slot = word(request, &block.slot)?;
    if slot > U256::from(u64::MAX) {
        return Err("world slot exceeds uint64".into());
    }
    Ok(BlockEnv::<BaseEvmTypes> {
        number: word(request, &block.number)?,
        beneficiary: address(request, &block.coinbase)?,
        timestamp: word(request, &block.timestamp)?,
        gas_limit: word(request, &block.gas_limit)?,
        difficulty: word(request, &block.difficulty)?,
        prevrandao: word(request, &block.prevrandao)?,
        basefee: word(request, &block.base_fee)?,
        blob_basefee: word(request, &block.blob_base_fee)?,
        slot_num: slot,
        ..Default::default()
    })
}

pub(super) fn transaction(request: &Request, input: &Input) -> Result<TxEnv, String> {
    Ok(TxEnv::<BaseEvmTypes> {
        origin: address(request, &input.origin)?,
        gas_price: word(request, &input.gas_price)?,
        chain_id: word(request, &input.chain_id)?,
        blob_hashes: input
            .blob_hashes
            .iter()
            .map(|value| word(request, value))
            .collect::<Result<_, _>>()?,
        ..Default::default()
    })
}

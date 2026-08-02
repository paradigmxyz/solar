//! HIR-to-MIR lowering.
//!
//! This layer builds typed function bodies and records ABI shapes. Physical
//! calldata, memory, and return handling belongs to the MIR lowering passes.

mod contract;
mod function;
mod storage;
mod types;

use alloy_primitives::Bytes;
use solar_data_structures::map::FxHashMap;
use solar_sema::{Gcx, hir::ContractId};

use crate::mir::Module;

/// Lowers a contract from HIR to MIR.
pub fn lower_contract(gcx: Gcx<'_>, contract_id: ContractId) -> Module {
    lower_contract_with_bytecodes(gcx, contract_id, &FxHashMap::default())
}

/// Lowers a contract from HIR to MIR with child creation bytecodes available.
pub fn lower_contract_with_bytecodes(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    contract::lower(gcx, contract_id, child_bytecodes)
}

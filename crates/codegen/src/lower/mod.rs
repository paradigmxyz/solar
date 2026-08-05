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
    let empty = FxHashMap::default();
    lower_contract_with_bytecodes_and_runtime(gcx, contract_id, &empty, &empty)
}

/// Lowers a contract from HIR to MIR with child creation bytecodes available.
pub fn lower_contract_with_bytecodes(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    let empty = FxHashMap::default();
    contract::lower(gcx, contract_id, child_bytecodes, &empty)
}

/// Lowers a contract with child creation and runtime bytecodes available.
pub fn lower_contract_with_bytecodes_and_runtime(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Bytes>,
    child_runtime_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    contract::lower(gcx, contract_id, child_bytecodes, child_runtime_bytecodes)
}

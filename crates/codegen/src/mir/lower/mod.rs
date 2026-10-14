//! HIR-to-MIR lowering.
//!
//! This layer builds typed function bodies and records ABI shapes. Physical
//! calldata, memory, and return handling belongs to the MIR lowering passes.

mod contract;
mod data;
mod function;
mod storage;
mod types;

use solar_sema::{Gcx, hir::ContractId};

use crate::mir::Module;

pub(crate) use data::data_copy_cost;

/// Lowers a contract from HIR to MIR.
///
/// The bytecode of contracts that this contract embeds stays deferred data that
/// the backend links in during final assembly.
pub fn lower_contract(gcx: Gcx<'_>, contract_id: ContractId) -> Module {
    contract::lower(gcx, contract_id, false)
}

/// Lowers debug-only source observations without introducing executable uses.
pub(crate) fn lower_contract_with_debug_info(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    capture_variables: bool,
) -> Module {
    contract::lower(gcx, contract_id, capture_variables)
}

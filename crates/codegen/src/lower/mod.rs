//! HIR-to-MIR lowering.
//!
//! This layer builds typed function bodies and records ABI shapes. Physical
//! calldata, memory, and return handling belongs to the MIR lowering passes.

mod contract;
mod data;
mod function;
mod storage;
mod types;

use solar_data_structures::map::FxHashMap;
use solar_sema::{Gcx, hir::ContractId};

use crate::mir::Module;

pub use data::ContractBytecodes;
pub(crate) use data::data_copy_cost;

/// Lowers a contract from HIR to MIR.
pub fn lower_contract(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, ContractBytecodes>,
) -> Module {
    contract::lower(gcx, contract_id, child_bytecodes)
}

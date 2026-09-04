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
pub(crate) use data::{data_copy_cost, data_copy_gas, data_copy_is_profitable};

/// Lowers a contract from HIR to MIR.
///
/// `sema_errored` records whether the compilation had already failed when the
/// code generation phase started; a lowering bail-out is only reported when it
/// had not.
pub fn lower_contract(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, ContractBytecodes>,
    sema_errored: bool,
) -> Module {
    contract::lower(gcx, contract_id, child_bytecodes, sema_errored)
}

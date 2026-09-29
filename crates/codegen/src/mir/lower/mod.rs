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
pub use data::{ContractBytecodes, resolve_contract_code};

/// Lowers a contract from HIR to MIR.
///
/// `sema_errored` records whether the compilation had already failed when the
/// code generation phase started; a lowering bail-out is only reported when it
/// had not.
///
/// The bytecode of contracts that this contract creates stays deferred until
/// [`resolve_contract_code`] supplies it.
pub fn lower_contract(gcx: Gcx<'_>, contract_id: ContractId, sema_errored: bool) -> Module {
    contract::lower(gcx, contract_id, sema_errored)
}

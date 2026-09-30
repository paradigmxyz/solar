//! HIR-to-MIR lowering.
//!
//! This layer builds typed function bodies and records ABI shapes. Physical
//! calldata, memory, and return handling belongs to the MIR lowering passes.

mod contract;
mod data;
mod function;
mod storage;
mod types;

use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use solar_sema::{
    Gcx,
    hir::{ContractId, VariableId},
};

use crate::mir::Module;

pub use data::ContractBytecodes;
pub(crate) use data::data_copy_cost;

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

/// A state variable's storage location.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct StateVariableSlot {
    pub(crate) variable: VariableId,
    pub(crate) slot: U256,
    pub(crate) offset: u8,
    pub(crate) transient: bool,
}

/// Returns the storage locations of every state variable visible to a contract, ordered by
/// location.
pub(crate) fn state_variable_slots(
    gcx: Gcx<'_>,
    contract_id: ContractId,
) -> Vec<StateVariableSlot> {
    let layout = storage::StorageLayout::for_contract(gcx, contract_id);
    let mut slots = layout
        .variables()
        .map(|(variable, slot, offset, transient)| StateVariableSlot {
            variable,
            slot,
            offset,
            transient,
        })
        .collect::<Vec<_>>();
    slots.sort_by_key(|slot| (slot.transient, slot.slot, slot.offset));
    slots
}

//! Shared MIR utility helpers: the ones in `solar_mir::utils`, constant evaluation, call emission,
//! and the gas reserves of call lowering.

mod call;
pub(crate) mod eval;
mod gas;
pub(crate) use call::emit_call;
pub(crate) use gas::pre_tangerine_call_gas;
pub(crate) use solar_mir::utils::*;

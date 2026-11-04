//! Shared MIR utility helpers: the ones in `solar_mir::utils`, constant evaluation, and the gas
//! reserves of call lowering.

pub(crate) mod eval;
mod gas;
mod same_block;
pub(crate) use gas::{pre_tangerine_call_gas, precompile_gas};
pub(crate) use same_block::{SameBlock, merge_origins};
pub(crate) use solar_mir::utils::*;

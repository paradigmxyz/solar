//! Control-flow stack layout planning at the MIR-to-EVM boundary.
//!
//! Phi plans carry values across selected loops and joins. Global plans extend
//! those layouts to resident arguments and cross-block values. Selection compares
//! feasible layouts while respecting stack reach and writes to spill memory.

mod global;
mod phi;
mod select;

pub(in crate::backend::evm::codegen) use global::GlobalStackPlan;
pub(in crate::backend::evm::codegen) use phi::{
    StackPhiBranch, StackPhiEdge, StackPhiPlan, planned_entry_carries,
};

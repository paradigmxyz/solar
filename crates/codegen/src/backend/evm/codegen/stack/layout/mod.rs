//! Control-flow stack layout planning at the MIR-to-EVM boundary.
//!
//! Phi plans carry values across selected loops and joins. Global plans extend
//! those layouts to resident arguments and cross-block values. Selection compares
//! feasible layouts while respecting stack reach and writes to spill memory.
//! Emission may still reorder a loop's carried invariants to the stack that
//! enters it, before any block of the loop is emitted.

mod global;
mod phi;
mod rebind;
mod select;

pub(in crate::backend::evm::codegen) use global::GlobalStackPlan;
pub(in crate::backend::evm::codegen) use phi::{
    LIVE_JOIN_LAYOUT_LIMIT, StackPhiBranch, StackPhiEdge, StackPhiPlan, planned_entry_carries,
};

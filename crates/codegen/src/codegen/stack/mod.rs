//! Stack scheduling for EVM code generation.
//!
//! This module handles the translation from MIR's SSA values to EVM's stack-based model.
//! The key abstraction is `StackModel`, which tracks which MIR values are at which stack depths.
//!
//! ## Submodules
//!
//! - `model`: Core stack model tracking value positions
//! - `scheduler`: Stack scheduler for generating DUP/SWAP sequences
//! - `shuffler`: Optimal stack layout transitions (backward analysis)
//! - `spill`: Spill management for values beyond depth 16
//! - `scheduling`: Use-frequency analysis and scheduling hints

mod model;
mod scheduler;
pub mod scheduling;
pub mod shuffler;
mod spill;

pub use model::{StackModel, StackOp};
pub use scheduler::{ScheduledOp, StackScheduler};
pub use shuffler::{
    BlockStackLayout, LayoutAnalysis, ShuffleResult, StackShuffler, TargetSlot,
    combine_stack_layouts, estimate_shuffle_cost, ideal_binary_op_entry, ideal_operand_layout,
    ideal_unary_op_entry, is_freely_generable,
};
pub use spill::{SpillManager, SpillSlot};

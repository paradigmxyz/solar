//! MIR analysis passes.
//!
//! This module contains dataflow analysis passes for MIR, including:
//! - Liveness analysis for tracking value lifetimes
//! - Phi elimination for converting SSA to CSSA

mod liveness;
pub use liveness::{LiveSet, Liveness, LivenessInfo};

mod phi_elimination;
pub use phi_elimination::{eliminate_phis, BlockCopies, ParallelCopy, PhiEliminationResult};

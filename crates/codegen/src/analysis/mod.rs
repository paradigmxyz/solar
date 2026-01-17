//! MIR analysis passes.
//!
//! This module contains dataflow analysis passes for MIR, including:
//! - Liveness analysis for tracking value lifetimes
//! - Phi elimination for converting SSA to CSSA
//! - Loop analysis for detecting and analyzing natural loops

mod liveness;
pub use liveness::{LiveSet, Liveness, LivenessInfo};

mod phi_elimination;
pub use phi_elimination::{
    BlockCopies, CopyDest, CopySource, ParallelCopy, PhiEliminationResult, eliminate_phis,
};

mod loop_analysis;
pub use loop_analysis::{InductionVariable, Loop, LoopAnalyzer, LoopInfo};

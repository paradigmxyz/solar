//! MIR analysis passes.
//!
//! This module contains dataflow analysis passes for MIR, including:
//! - Liveness analysis for tracking value lifetimes
//! - Phi elimination for converting SSA to CSSA
//! - Loop analysis for detecting and analyzing natural loops

mod cfg;
pub use cfg::{CfgInfo, DominatorTree};

mod call_graph;
pub use call_graph::CallGraphInfo;

mod liveness;
pub use liveness::{LiveSet, Liveness, LivenessInfo};

mod phi_elimination;
pub use phi_elimination::{
    BlockCopies, CopyDest, CopySource, ParallelCopy, PhiEliminationResult, PhiEliminator,
    eliminate_phis,
};

mod loop_analysis;
pub use loop_analysis::{InductionVariable, Loop, LoopAnalyzer, LoopInfo};

mod scalar_evolution;
pub use scalar_evolution::{AffineExpr, AffineTerm, ScalarEvolution};

mod validator;
pub use validator::Validator;

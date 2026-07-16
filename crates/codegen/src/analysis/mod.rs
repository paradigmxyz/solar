//! MIR analysis passes.
//!
//! This module contains dataflow analysis passes for MIR, including:
//! - Liveness analysis for tracking value lifetimes
//! - Phi elimination for converting SSA to CSSA
//! - Loop analysis for detecting and analyzing natural loops

mod alias;
pub use alias::{
    Access, AddressSpace, AliasAnalysis, AliasResult, Location, LocationSize, MemoryAddress,
    MemoryBase, MemoryLocation, ModRef,
};

mod memory_summary;
pub use memory_summary::{FunctionMemorySummary, MemoryCallSummaries};

mod cfg;
pub(crate) use cfg::{CfgInfo, DominatorTree};

mod call_graph;
pub(crate) use call_graph::CallGraphInfo;

mod liveness;
pub(crate) use liveness::Liveness;

mod phi_elimination;
pub(crate) use phi_elimination::{CopyDest, CopySource, ParallelCopy, PhiEliminator};

mod loop_analysis;
pub(crate) use loop_analysis::{Loop, LoopAnalyzer};

mod scalar_evolution;
pub(crate) use scalar_evolution::{AffineExpr, ScalarEvolution};

mod validator;
pub(crate) use validator::validate;

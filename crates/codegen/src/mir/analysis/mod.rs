//! MIR analysis passes.
//!
//! This module contains dataflow analysis passes for MIR, including:
//! - Liveness analysis for tracking value lifetimes
//! - Phi elimination for converting SSA to CSSA
//! - Loop analysis for detecting and analyzing natural loops

pub(crate) mod integers;

mod alias;
pub(crate) use alias::{
    Access, AddressSpace, AliasAnalysis, AliasResult, Location, LocationSize, MemoryAddress,
    MemoryBase, MemoryLocation, ModRef,
};

mod memory_summary;
pub(crate) use memory_summary::{MemoryCallSummaries, may_observe_msize};

mod memory_restoration;

mod cfg;
pub(crate) use cfg::{CfgInfo, DominatorTree};

mod gas;
pub(crate) use gas::GasObservations;

mod call_graph;
pub(crate) use call_graph::CallGraphInfo;

mod liveness;
pub(crate) use liveness::Liveness;

mod phi_elimination;
pub(crate) use phi_elimination::{CopyDest, CopySource, ParallelCopy, PhiEliminator};

mod loop_analysis;
pub(crate) use loop_analysis::{InductionVariable, Loop, LoopAnalyzer, LoopInfo};

mod scalar_evolution;
pub(crate) use scalar_evolution::{AffineExpr, AffineTerm, ScalarEvolution};

mod validator;
pub(crate) use validator::{validate, validate_phase};

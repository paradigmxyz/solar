//! MIR analysis passes.
//!
//! This module contains dataflow analysis passes for MIR, including:
//! - Liveness analysis for tracking value lifetimes
//! - Loop analysis for detecting and analyzing natural loops

pub(crate) use solar_mir::analysis::{CallGraphInfo, CfgInfo, DominatorTree, direct_callees};

pub(crate) mod integers;

mod alias;
pub(crate) use alias::{
    Access, AddressSpace, AliasAnalysis, AliasResult, Location, LocationSize, MemoryAddress,
    MemoryBase, MemoryLocation, ModRef, ReadBounds,
};

mod memory_summary;
pub(crate) use memory_summary::{LocalSummaryCache, MemoryCallSummaries, returning_blocks};

mod memory_restoration;

mod gas;
pub(crate) use gas::GasObservations;

mod fresh_fmp;
pub(crate) use fresh_fmp::FreshFmpReads;

mod hotness;
pub(crate) use hotness::{HotnessAnalysis, aborts};

mod liveness;
pub(crate) use liveness::Liveness;

mod loop_analysis;
pub(crate) use loop_analysis::{InductionVariable, Loop, LoopAnalyzer, LoopInfo};

mod scalar_evolution;
pub(crate) use scalar_evolution::{AffineExpr, AffineTerm, ScalarEvolution};

mod absolute_addresses;
pub(crate) use absolute_addresses::{AddressInput, absolute_address_inputs};

mod heap_offsets;
pub(crate) use heap_offsets::{is_floor_clamp, pointer_bases};

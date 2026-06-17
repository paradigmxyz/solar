#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

extern crate derive_more as _;
extern crate tracing as _;

pub use solar_data_structures::map::FxHashMap;
pub use solar_sema as sema;

/// Constructor scratch memory used to stage immutable words before appending
/// them to runtime bytecode.
pub const IMMUTABLE_SCRATCH_BASE: u64 = 0x2000;

pub mod mir;
pub use mir::{
    BasicBlock, BlockId, Function, FunctionId, Immediate, InstId, InstKind, Instruction, MirType,
    Module, Terminator, Value, ValueId,
};

pub mod analysis;
pub use analysis::{InductionVariable, Liveness, LivenessInfo, Loop, LoopAnalyzer, LoopInfo};

pub mod backend;
pub use backend::{
    Backend,
    evm::{
        AssembledCode, Assembler, AssemblerConfig, EvmArtifact, EvmCodegen, EvmCodegenConfig,
        Label, PeepholeOptimizer, SpillManager, SpillSlot, StackModel, StackScheduler,
    },
};

pub mod lower;
pub use lower::Lowerer;

pub mod pass;
pub mod transform;
pub(crate) mod utils;
pub use transform::{
    CommonSubexprEliminator, DceStats, DeadCodeEliminator, FunctionInlineInfo, InlineAnalyzer,
    InlineConfig, InlineDecision, InlineStats, InstSimplifier, JumpThreader, JumpThreadingStats,
    LoopOptConfig, LoopOptStats, LoopOptimizer, OptLevel,
};

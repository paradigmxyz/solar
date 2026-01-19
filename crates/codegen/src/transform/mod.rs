//! Optimization and transformation passes for the Solar compiler.

pub mod constant_fold;
pub mod cse;
pub mod dce;
pub mod inline;
pub mod jump_threading;
pub mod loop_opt;

pub use constant_fold::{ConstantFolder, FoldResult};
pub use cse::CommonSubexprEliminator;
pub use dce::{DceStats, DeadCodeEliminator};
pub use inline::{
    FunctionInlineInfo, InlineAnalyzer, InlineConfig, InlineDecision, InlineStats, OptLevel,
};
pub use jump_threading::{JumpThreader, JumpThreadingStats};
pub use loop_opt::{LoopOptConfig, LoopOptStats, LoopOptimizer};

//! Optimization and transformation passes for the Solar compiler.

pub mod cfg_simplify;
pub mod constant_fold;
pub mod cse;
pub mod dce;
pub mod inline;
pub mod inst_simplify;
pub mod jump_threading;
pub mod loop_opt;
pub mod memory_dse;
pub mod sccp;
pub mod storage_promotion;

pub use cfg_simplify::{
    CallGraphAnalyzer, CfgSimplifier, CfgSimplifyStats, DeadFunctionEliminator,
    repair_reachability_phis, simplify_cfg, simplify_module_cfg,
};
pub use constant_fold::{ConstantFolder, FoldResult};
pub use cse::CommonSubexprEliminator;
pub use dce::{DceStats, DeadCodeEliminator};
pub use inline::{
    FunctionInlineInfo, InlineAnalyzer, InlineConfig, InlineDecision, InlineStats, MirInlineConfig,
    MirInlineStats, MirInliner, OptLevel,
};
pub use inst_simplify::InstSimplifier;
pub use jump_threading::{JumpThreader, JumpThreadingStats};
pub use loop_opt::{LoopOptConfig, LoopOptStats, LoopOptimizer};
pub use memory_dse::MemoryStoreEliminator;
pub use sccp::{SccpPass, SccpStats};
pub use storage_promotion::{StoragePromotionStats, StorageScalarPromoter};

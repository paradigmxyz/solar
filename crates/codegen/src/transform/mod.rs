//! Optimization and transformation passes for the Solar compiler.

pub mod cfg_simplify;
pub mod constant_fold;
pub mod cse;
pub mod dce;
pub mod frame_promotion;
pub mod inline;
pub mod inst_simplify;
pub mod jump_threading;
pub mod loop_opt;
pub mod memory_dse;
pub mod pure_eval;
pub mod sccp;
pub mod storage_load_cse;
pub mod storage_promotion;

pub use cfg_simplify::{
    CallGraphAnalyzer, CfgSimplifier, CfgSimplifyPass, CfgSimplifyStats, DeadFunctionEliminator,
    FunctionDcePass, repair_reachability_phis, simplify_cfg, simplify_module_cfg,
};
pub use constant_fold::{ConstantFolder, FoldResult};
pub use cse::{CommonSubexprEliminator, CsePass};
pub use dce::{DcePass, DceStats, DeadCodeEliminator};
pub use frame_promotion::{FramePromotionStats, FrameSlotPromoter, FrameSlotPromotionPass};
pub use inline::{
    FunctionInlineInfo, InlineAnalyzer, InlineConfig, InlineDecision, InlinePass, InlineStats,
    MirInlineConfig, MirInlineStats, MirInliner, OptLevel,
};
pub use inst_simplify::{InstSimplifier, InstSimplifyPass};
pub use jump_threading::{JumpThreader, JumpThreadingPass, JumpThreadingStats};
pub use loop_opt::{LicmPass, LoopOptConfig, LoopOptStats, LoopOptimizer};
pub use memory_dse::{MemoryDsePass, MemoryStoreEliminator};
pub use pure_eval::{PureEvalPass, PureEvalStats, PureEvaluator};
pub use sccp::{SccpPass, SccpStats, SccpTransformPass};
pub use storage_load_cse::{StorageLoadCse, StorageLoadCsePass};
pub use storage_promotion::{
    StoragePromotionStats, StorageScalarPromoter, StorageScalarPromotionPass,
};

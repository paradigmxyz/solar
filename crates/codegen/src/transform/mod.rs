//! Optimization and transformation passes for the Solar compiler.

pub mod adce;
pub mod cfg_simplify;
pub mod check_elim;
pub mod cse;
pub mod dce;
pub mod frame_promotion;
pub mod gvn;
pub mod indvar_simplify;
pub mod inline;
pub mod inst_simplify;
pub mod jump_threading;
pub mod load_pre;
pub mod loop_canonicalize;
pub mod loop_opt;
pub mod memory_dse;
pub mod pre;
pub mod pure_eval;
pub mod sccp;
pub mod storage_dse;
pub mod storage_load_cse;
pub mod storage_promotion;

pub use adce::{AdcePass, AdceStats, AggressiveDeadCodeEliminator};
pub use cfg_simplify::{
    CfgSimplifier, CfgSimplifyPass, CfgSimplifyStats, DeadFunctionEliminator, FunctionDcePass,
    simplify_cfg, simplify_module_cfg,
};
pub use check_elim::{CheckElimPass, CheckElimStats, CheckEliminator};
pub use cse::{CommonSubexprEliminator, CsePass};
pub use dce::{DcePass, DceStats, DeadCodeEliminator};
pub use frame_promotion::{FramePromotionStats, FrameSlotPromoter, FrameSlotPromotionPass};
pub use gvn::{GlobalValueNumberer, GvnPass};
pub use indvar_simplify::{IndVarSimplifier, IndVarSimplifyPass, IndVarSimplifyStats};
pub use inline::{
    FunctionInlineInfo, InlineAnalyzer, InlineConfig, InlineDecision, InlinePass, InlineStats,
    MirInlineConfig, MirInlineStats, MirInliner, OptLevel,
};
pub use inst_simplify::{InstSimplifier, InstSimplifyPass};
pub use jump_threading::{JumpThreader, JumpThreadingPass, JumpThreadingStats};
pub use load_pre::{LoadPrePass, LoadPreStats, LoadRedundancyEliminator};
pub use loop_canonicalize::{LoopCanonicalizePass, LoopCanonicalizeStats, LoopCanonicalizer};
pub use loop_opt::{LicmPass, LoopOptConfig, LoopOptStats, LoopOptimizer};
pub use memory_dse::{MemoryDsePass, MemoryStoreEliminator};
pub use pre::{PartialRedundancyEliminator, PrePass, PreStats};
pub use pure_eval::{PureEvalPass, PureEvalStats, PureEvaluator};
pub use sccp::{SccpPass, SccpStats, SccpTransformPass};
pub use storage_dse::{StorageDsePass, StorageStoreEliminator};
pub use storage_load_cse::{StorageLoadCse, StorageLoadCsePass};
pub use storage_promotion::{
    StoragePromotionStats, StorageScalarPromoter, StorageScalarPromotionPass,
};

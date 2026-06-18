//! Optimization and transformation passes for the Solar compiler.

use crate::mir::{Function, InstId, Value, ValueId};
use solar_data_structures::map::FxHashMap;

mod adce;
mod cfg_simplify;
mod check_elim;
mod cse;
mod dce;
mod frame_promotion;
mod gvn;
mod indvar_simplify;
mod inline;
mod inst_simplify;
mod jump_threading;
mod load_pre;
mod loop_canonicalize;
mod loop_opt;
mod memory_dse;
mod pre;
mod pure_eval;
mod sccp;
mod storage_dse;
mod storage_load_cse;
mod storage_promotion;

pub use adce::{AdcePass, AdceStats, AggressiveDeadCodeEliminator};
pub use cfg_simplify::{
    CfgSimplifier, CfgSimplifyPass, CfgSimplifyStats, DeadFunctionEliminator, FunctionDcePass,
    simplify_cfg, simplify_module_cfg,
};
pub use check_elim::{CheckElimPass, CheckElimStats, CheckEliminator};
pub use cse::{CommonSubexprEliminator, CsePass};
pub use dce::{DcePass, DceStats, DeadCodeEliminator};
pub use frame_promotion::{
    FramePromotionStats, FrameSlotPromoter, FrameSlotPromotionPass, PromotedSlot,
    PromotedSlotSummary,
};
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

fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
    let mut results = FxHashMap::default();
    results.reserve(func.instructions.len());
    for (value_id, value) in func.values.iter_enumerated() {
        if let Value::Inst(inst_id) = value {
            results.insert(*inst_id, value_id);
        }
    }
    results
}

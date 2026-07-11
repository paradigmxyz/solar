//! Optimization and transformation passes for the Solar compiler.

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
mod lower_abi;
mod lower_dispatch;
mod lower_evm_shaped;
mod memory_dse;
mod outline_reverts;
mod static_alloc;
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
pub use lower_abi::{LowerAbiPass, LowerAbiStats};
pub use lower_dispatch::{LowerDispatchPass, LowerDispatchStats};
pub use lower_evm_shaped::{LowerEvmShapedPass, LowerEvmShapedStats};
pub use memory_dse::{MemoryDsePass, MemoryStoreEliminator};
pub use outline_reverts::{OutlineRevertsPass, OutlineRevertsStats};
pub use static_alloc::StaticAllocPass;
pub use pre::{PartialRedundancyEliminator, PrePass, PreStats};
pub use pure_eval::{PureEvalPass, PureEvalStats, PureEvaluator};
pub use sccp::{SccpPass, SccpStats, SccpTransformPass};
pub use storage_dse::{StorageDsePass, StorageStoreEliminator};
pub use storage_load_cse::{StorageLoadCse, StorageLoadCsePass};
pub use storage_promotion::{
    StoragePromotionStats, StorageScalarPromoter, StorageScalarPromotionPass,
};

/// Whether an external entry must reject nonzero callvalue, mirroring the
/// backend dispatcher's rule.
pub(crate) fn rejects_callvalue(func: &crate::mir::Function) -> bool {
    use solar_sema::hir::StateMutability;
    matches!(
        func.attributes.state_mutability,
        StateMutability::NonPayable | StateMutability::View | StateMutability::Pure
    )
}

/// Whether the dispatch entry hoists a single callvalue check: every bodied
/// external entry (selector-bearing, receive, or fallback) rejects value.
///
/// [`LowerAbiPass`] and [`LowerDispatchPass`] both key off this: when it does
/// not hold, `lower-abi` injects the check into each rejecting wrapper's
/// prologue and `lower-dispatch` routes selector cases unguarded, so the two
/// passes MUST agree — a mismatch would leave a nonpayable entry unchecked.
pub(crate) fn dispatch_hoists_callvalue(module: &crate::mir::Module) -> bool {
    let mut any = false;
    for func in module.functions.iter() {
        let external = func.selector.is_some()
            || func.attributes.is_receive
            || func.attributes.is_fallback;
        if !external || func.blocks.is_empty() || func.attributes.is_constructor {
            continue;
        }
        if !rejects_callvalue(func) {
            return false;
        }
        any = true;
    }
    any
}

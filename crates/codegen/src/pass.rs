//! Pass infrastructure for MIR transformations and analyses.
//!
//! Transformation pipelines follow rustc MIR's pass-manager shape: passes
//! implement [`MirPass`] and pipelines are slices of trait-object references.
//! Analyses retain their LLVM/MLIR-style cache: read-only [`AnalysisPass`]es
//! produce results cached in an [`AnalysisManager`].
//!
//! # Usage
//!
//! ```ignore
//! // Read-only analysis pipeline (codegen):
//! let mut am = AnalysisManager::new();
//! let liveness = am.get_or_compute(&LivenessAnalysis, &func);
//!
//! let changed = run_passes(
//!     gcx,
//!     &mut module,
//!     &[&DCE_PASS],
//!     None,
//!     Optimizations::Allowed,
//! );
//! ```

pub use crate::pass_manager::{MirPass, Optimizations, run_passes, run_passes_no_validate};
use crate::{
    mir::{Function, MirPhase, Module},
    transform::{
        adce, cfg_simplify, check_elim, cse, dce, frame_promotion, gvn, indvar_simplify, inline,
        inst_simplify, jump_threading, load_pre, loop_canonicalize, loop_opt, lower_abi,
        lower_dispatch, lower_evm_shaped, lower_mapping_slots, memory_dse, outline_reverts, pre,
        pure_eval, sccp, static_alloc, storage_dse, storage_load_cse, storage_promotion,
    },
};
use solar_data_structures::map::FxHashMap;
use std::any::{Any, TypeId};

macro_rules! declare_passes {
    ($($vis:vis const $const_name:ident = $module:ident::$pass:ident;)+) => {
        $(
            $vis const $const_name: $module::$pass = $module::$pass;
        )+

        /// All known MIR passes exposed to `solar mir-opt`.
        pub static PASS_REGISTRY: &[&dyn MirPass] = &[$(&$const_name),+];
    };
}

declare_passes! {
    pub(crate) const INLINE_PASS = inline::Inline;
    pub(crate) const OUTLINE_REVERTS_PASS = outline_reverts::OutlineReverts;
    pub(crate) const FUNCTION_DCE_PASS = cfg_simplify::FunctionDce;
    pub(crate) const SCCP_PASS = sccp::Sccp;
    pub(crate) const PURE_EVAL_PASS = pure_eval::PureEval;
    pub(crate) const INST_SIMPLIFY_PASS = inst_simplify::InstSimplify;
    pub(crate) const CSE_PASS = cse::Cse;
    pub(crate) const PRE_PASS = pre::Pre;
    pub(crate) const GVN_PASS = gvn::Gvn;
    pub(crate) const STORAGE_LOAD_CSE_PASS = storage_load_cse::StorageLoadCse;
    pub(crate) const STORAGE_DSE_PASS = storage_dse::StorageDse;
    pub(crate) const LOAD_PRE_PASS = load_pre::LoadPre;
    pub(crate) const LOOP_CANONICALIZE_PASS = loop_canonicalize::LoopCanonicalize;
    pub(crate) const INDVAR_SIMPLIFY_PASS = indvar_simplify::IndVarSimplify;
    pub(crate) const STORAGE_PROMOTION_PASS = storage_promotion::StorageScalarPromotion;
    pub(crate) const LICM_PASS = loop_opt::Licm;
    pub(crate) const CHECK_ELIM_PASS = check_elim::CheckElim;
    pub(crate) const JUMP_THREADING_PASS = jump_threading::JumpThreading;
    pub(crate) const CFG_SIMPLIFY_PASS = cfg_simplify::CfgSimplify;
    pub(crate) const FRAME_SLOT_PROMOTION_PASS = frame_promotion::FrameSlotPromotion;
    pub(crate) const MEMORY_DSE_PASS = memory_dse::MemoryDse;
    pub(crate) const STATIC_ALLOC_PASS = static_alloc::StaticAlloc;
    pub(crate) const DCE_PASS = dce::Dce;
    pub(crate) const ADCE_PASS = adce::Adce;
    pub(crate) const LOWER_ABI_PASS = lower_abi::LowerAbi;
    pub(crate) const LOWER_DISPATCH_PASS = lower_dispatch::LowerDispatch;
    pub(crate) const LOWER_EVM_SHAPED_PASS = lower_evm_shaped::LowerEvmShaped;
    pub(crate) const LOWER_MAPPING_SLOTS_PASS = lower_mapping_slots::LowerMappingSlots;
}

/// Finds a pass in the global MIR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn MirPass> {
    PASS_REGISTRY.iter().copied().find(|pass| pass.name() == name)
}

/// The canonical MIR optimization pipeline used by EVM codegen.
pub static DEFAULT_PIPELINE: &[&dyn MirPass] = &[
    &INLINE_PASS,
    &FUNCTION_DCE_PASS,
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    // Reuse mapping slots before their scratch-memory expansion can obscure
    // the semantic expression from the remaining optimization passes.
    &LOWER_MAPPING_SLOTS_PASS,
    &GVN_PASS,
    &PRE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &STORAGE_DSE_PASS,
    &LOAD_PRE_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &LOOP_CANONICALIZE_PASS,
    &INDVAR_SIMPLIFY_PASS,
    &STORAGE_PROMOTION_PASS,
    &LICM_PASS,
    &CHECK_ELIM_PASS,
    &JUMP_THREADING_PASS,
    &CFG_SIMPLIFY_PASS,
    &MEMORY_DSE_PASS,
    &STATIC_ALLOC_PASS,
    &ADCE_PASS,
    &DCE_PASS,
];

/// Cleanup passes rerun after the primary pipeline until no pass changes MIR.
///
/// Keep this group focused on simplification and canonicalization. Structural
/// profitability passes such as inlining and storage promotion run once in
/// [`DEFAULT_PIPELINE`], while this loop cleans up opportunities exposed by
/// those transforms.
pub static DEFAULT_CLEANUP_PIPELINE: &[&dyn MirPass] = &[
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    &GVN_PASS,
    &PRE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &STORAGE_DSE_PASS,
    &LOAD_PRE_PASS,
    &CHECK_ELIM_PASS,
    &JUMP_THREADING_PASS,
    &CFG_SIMPLIFY_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &MEMORY_DSE_PASS,
    &ADCE_PASS,
    &DCE_PASS,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
///
/// This is a phase transition: the module comes out in `MirPhase::Optimized`.
/// Ad-hoc `solar mir-opt` pass lists deliberately do not advance the phase.
#[tracing::instrument(
    name = "mir_pipeline",
    level = "debug",
    skip_all,
    fields(module = %module.name),
)]
pub fn run_default_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
    let optimizations = Optimizations::for_gcx(gcx);
    let mut changed =
        run_passes(gcx, module, DEFAULT_PIPELINE, Some(MirPhase::Optimized), optimizations);
    changed |=
        run_cleanup_pipeline_to_fixpoint(gcx, module, DEFAULT_CLEANUP_PIPELINE, optimizations);
    changed
}

fn run_cleanup_pipeline_to_fixpoint(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    optimizations: Optimizations,
) -> bool {
    let mut changed = false;
    for _ in 1..=DEFAULT_CLEANUP_MAX_ROUNDS {
        let round_changed = run_passes(gcx, module, passes, None, optimizations);
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

/// A key identifying a particular analysis, derived from its result type.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct AnalysisKey(TypeId);

impl AnalysisKey {
    /// Creates a key from a type.
    pub(crate) fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// A read-only analysis pass.
///
/// Analysis passes inspect a function without modifying it and produce a
/// cacheable result that downstream passes can query via [`AnalysisManager`].
pub(crate) trait AnalysisPass {
    /// The result type produced by this analysis.
    type Result: 'static;

    /// Computes the analysis result for the given function.
    fn run(&self, func: &Function) -> Self::Result;
}

/// Runs a function-local transform over every bodied function in a module.
pub(crate) fn run_function_pass(
    module: &mut Module,
    mut run: impl FnMut(&mut Function) -> bool,
) -> bool {
    let mut changed = false;
    for func in &mut module.functions {
        changed |= run(func);
    }
    changed
}

/// Manages cached analysis results for a function.
///
/// Analyses are keyed by their result type via [`AnalysisKey`].
#[derive(Default)]
pub(crate) struct AnalysisManager {
    results: FxHashMap<AnalysisKey, Box<dyn Any>>,
}

impl AnalysisManager {
    /// Creates a new, empty analysis manager.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns the result of the analysis, computing and caching it if not already present.
    ///
    /// This is the recommended way to obtain analysis results, matching
    /// LLVM's `AnalysisManager::getResult<AnalysisT>(F)` pattern.
    pub(crate) fn get_or_compute<A: AnalysisPass>(
        &mut self,
        analysis: &A,
        func: &Function,
    ) -> &A::Result {
        let key = AnalysisKey::of::<A::Result>();
        self.results.entry(key).or_insert_with(|| {
            let result = analysis.run(func);
            Box::new(result)
        });
        self.results[&key].downcast_ref::<A::Result>().unwrap()
    }
}

/// Liveness analysis pass.
pub(crate) struct LivenessAnalysis;

impl AnalysisPass for LivenessAnalysis {
    type Result = crate::analysis::Liveness;

    fn run(&self, func: &Function) -> Self::Result {
        crate::analysis::Liveness::compute(func)
    }
}

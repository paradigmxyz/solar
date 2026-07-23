//! Pass infrastructure for MIR transformations and analyses.
//!
//! Transformation pipelines follow rustc MIR's pass-manager shape: passes
//! implement [`MirPass`] and pipelines are slices of trait-object references.
//! Analyses retain their LLVM/MLIR-style cache: read-only `AnalysisPass`es
//! produce results cached in an `AnalysisManager`.
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
//!     &[&dce::Dce],
//!     None,
//! );
//! ```

pub use crate::pass_manager::{MirPass, run_passes, run_passes_no_validate};
use crate::{
    mir::{Function, MirPhase, Module},
    transform::{
        adce, cfg_simplify, check_elim, copy_elision, cse, dce, frame_promotion, gvn,
        indvar_simplify, inline, inst_simplify, jump_threading, load_pre, loop_canonicalize,
        loop_opt, lower_abi, lower_abi_encode, lower_aggregates, lower_alloc, lower_dispatch,
        lower_evm_shaped, lower_mapping_slots, lower_memory_objects, lower_slices, memory_dse,
        outline_reverts, pre, pure_eval, sccp, sroa, static_alloc, storage_dse, storage_load_cse,
        storage_promotion,
    },
};
use solar_data_structures::map::FxHashMap;
use std::any::{Any, TypeId};

/// All known MIR passes exposed to `solar mir-opt`.
pub static ALL_PASSES: &[&dyn MirPass] = &[
    &inline::Inline,
    &outline_reverts::OutlineReverts,
    &cfg_simplify::FunctionDce,
    &sccp::Sccp,
    &pure_eval::PureEval,
    &inst_simplify::InstSimplify,
    &cse::Cse,
    &pre::Pre,
    &gvn::Gvn,
    &storage_load_cse::StorageLoadCse,
    &storage_dse::StorageDse,
    &load_pre::LoadPre,
    &loop_canonicalize::LoopCanonicalize,
    &indvar_simplify::IndVarSimplify,
    &storage_promotion::StorageScalarPromotion,
    &loop_opt::Licm,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &frame_promotion::FrameSlotPromotion,
    &memory_dse::MemoryDse,
    &static_alloc::StaticAlloc,
    &sroa::Sroa,
    &copy_elision::CopyElision,
    &dce::Dce,
    &adce::Adce,
    &lower_abi::LowerAbi,
    &lower_dispatch::LowerDispatch,
    &lower_evm_shaped::LowerEvmShaped,
    &lower_mapping_slots::LowerMappingSlots,
    &lower_abi_encode::LowerAbiEncode,
    &lower_aggregates::LowerAggregates,
    &lower_memory_objects::LowerMemoryObjects,
    &lower_slices::LowerSlices,
    &lower_alloc::LowerAlloc,
];

/// Finds a pass in the global MIR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn MirPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

/// The canonical MIR pipeline used by EVM codegen.
pub static DEFAULT_PIPELINE: &[&dyn MirPass] = &[
    &inline::Inline,
    &cfg_simplify::FunctionDce,
    &sccp::Sccp,
    &pure_eval::PureEval,
    &inst_simplify::InstSimplify,
    &cse::Cse,
    // Reuse mapping slots before their scratch-memory expansion can obscure
    // the semantic expression from the remaining optimization passes.
    &lower_mapping_slots::LowerMappingSlots,
    &gvn::Gvn,
    &pre::Pre,
    &storage_load_cse::StorageLoadCse,
    &storage_dse::StorageDse,
    &load_pre::LoadPre,
    &frame_promotion::FrameSlotPromotion,
    &loop_canonicalize::LoopCanonicalize,
    &indvar_simplify::IndVarSimplify,
    &storage_promotion::StorageScalarPromotion,
    &loop_opt::Licm,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &sroa::Sroa,
    &copy_elision::CopyElision,
    &memory_dse::MemoryDse,
    &adce::Adce,
    &dce::Dce,
];

/// Cleanup passes rerun after the primary pipeline until no pass changes MIR.
///
/// Keep this group focused on simplification and canonicalization. Structural
/// profitability passes such as inlining and storage promotion run once in
/// [`DEFAULT_PIPELINE`], while this loop cleans up opportunities exposed by
/// those transforms.
pub static DEFAULT_CLEANUP_PIPELINE: &[&dyn MirPass] = &[
    &sccp::Sccp,
    &pure_eval::PureEval,
    &inst_simplify::InstSimplify,
    &cse::Cse,
    &gvn::Gvn,
    &pre::Pre,
    &storage_load_cse::StorageLoadCse,
    &storage_dse::StorageDse,
    &load_pre::LoadPre,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &frame_promotion::FrameSlotPromotion,
    &sroa::Sroa,
    &copy_elision::CopyElision,
    &memory_dse::MemoryDse,
    &adce::Adce,
    &dce::Dce,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Runs the canonical MIR pipeline used by EVM codegen.
///
/// The optimization prefix advances the module to `MirPhase::Optimized` before
/// cleanup. The lowering suffix then advances it as far as the module permits.
/// Ad-hoc `solar mir-opt` pass lists do not advance the phase.
#[tracing::instrument(
    name = "mir_pipeline",
    level = "debug",
    skip_all,
    fields(module = %module.name),
)]
pub fn run_default_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
    let mut changed = run_passes(gcx, module, DEFAULT_PIPELINE, None);
    changed |= run_cleanup_pipeline_to_fixpoint(gcx, module, DEFAULT_CLEANUP_PIPELINE);
    run_passes(gcx, module, &[], Some(MirPhase::Optimized));
    changed
}

fn run_cleanup_pipeline_to_fixpoint(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
) -> bool {
    let mut changed = false;
    for _ in 1..=DEFAULT_CLEANUP_MAX_ROUNDS {
        let round_changed = run_passes(gcx, module, passes, None);
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

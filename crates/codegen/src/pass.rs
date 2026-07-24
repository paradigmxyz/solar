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
    analysis::{AliasAnalysis, CfgInfo, MemoryCallSummaries},
    mir::{Function, FunctionId, InstId, MirPhase, Module},
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
use std::{
    any::{Any, TypeId},
    rc::Rc,
    sync::Arc,
};

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

/// Finds a MIR pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn MirPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

struct SizeOnly<P>(P);

impl<P: MirPass> MirPass for SizeOnly<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        gcx.sess.opts.optimization.is_size() && self.0.is_enabled(gcx, module)
    }

    fn is_required(&self) -> bool {
        self.0.is_required()
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        self.0.run_pass(gcx, module, analyses)
    }
}

struct GasOnly<P>(P);

impl<P: MirPass> MirPass for GasOnly<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        gcx.sess.opts.optimization.is_gas() && self.0.is_enabled(gcx, module)
    }

    fn is_required(&self) -> bool {
        self.0.is_required()
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        self.0.run_pass(gcx, module, analyses)
    }
}

/// The canonical MIR pipeline used by EVM codegen.
pub static DEFAULT_PIPELINE: &[&dyn MirPass] = &[
    &inline::Inline,
    &cfg_simplify::FunctionDce,
    // Early frame scalarization improves size but can increase hot-path gas.
    &SizeOnly(cfg_simplify::CfgSimplify),
    &SizeOnly(frame_promotion::FrameSlotPromotion),
    &SizeOnly(sroa::Sroa),
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
    // MIR outlining remains profitable even though EVM IR can merge
    // equivalent terminal blocks: lowering and stack scheduling can
    // hide their shared semantic shape from the backend passes.
    &outline_reverts::OutlineReverts,
    // Outlining and late control-flow rewrites expose scalar simplifications.
    // Thread and clean the CFG first so the rest of this sequence observes the
    // simplified graph in one pass through the pipeline.
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &sccp::Sccp,
    &inst_simplify::InstSimplify,
    &cse::Cse,
    &gvn::Gvn,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &frame_promotion::FrameSlotPromotion,
    &memory_dse::MemoryDse,
    &adce::Adce,
    // Progressive lowering materializes ABI wrappers, selector routing, and
    // tail-call edges as MIR. Each pass bails without advancing the phase
    // when the module is outside its scope.
    &lower_abi::LowerAbi,
    &static_alloc::DeferAlloc,
    &lower_abi_encode::LowerAbiEncode,
    &lower_aggregates::LowerAggregates,
    &inst_simplify::InstSimplify,
    &cfg_simplify::CfgSimplify,
    &memory_dse::MemoryDse,
    // Late CSE reduces runtime gas after aggregate lowering, but can grow
    // bytecode through longer live ranges, so keep it out of `-Osize`.
    &GasOnly(cse::Cse),
    &dce::Dce,
    &lower_slices::LowerSlices,
    &lower_dispatch::LowerDispatch,
    &lower_memory_objects::LowerMemoryObjects,
    &lower_alloc::LowerAlloc,
    &lower_evm_shaped::LowerEvmShaped,
];

/// Runs the canonical MIR pipeline used by EVM codegen.
///
/// The optimization prefix advances the module to `MirPhase::Optimized`. The
/// lowering suffix then advances it as far as the module permits. Modules
/// already past optimization resume at the lowering suffix. Ad-hoc pass lists
/// passed to `solar mir-opt` do not advance the phase.
#[tracing::instrument(
    name = "mir_pipeline",
    level = "debug",
    skip_all,
    fields(module = %module.name),
)]
#[must_use]
pub fn run_default_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
    let lowering_start = DEFAULT_PIPELINE
        .iter()
        .position(|pass| pass.name() == lower_abi::LowerAbi.name())
        .expect("default pipeline must contain `lower-abi`");
    let (optimization_passes, lowering_passes) = DEFAULT_PIPELINE.split_at(lowering_start);
    let mut changed = false;
    if module.phase <= MirPhase::Optimized {
        changed |= run_passes(gcx, module, optimization_passes, Some(MirPhase::Optimized));
    }
    changed |= run_passes(gcx, module, lowering_passes, None);
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
#[must_use]
pub(crate) fn run_function_pass(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    mut run: impl FnMut(&mut Function, &FunctionAnalyses) -> bool,
) -> bool {
    let mut changed = false;
    for func_id in module.functions.indices() {
        if module.functions[func_id].blocks.is_empty() {
            continue;
        }
        changed |= run_function_pass_cached(analyses, module, func_id, &mut run);
    }
    analyses.preserved_by_pass = true;
    changed
}

/// Per-function analysis snapshots handed to a pass run.
pub(crate) struct FunctionAnalyses {
    /// Shared alias analysis; provenance and address memos build lazily.
    pub(crate) alias: Rc<AliasAnalysis>,
    /// Shared CFG snapshot; RPO, dominators, and reachability build lazily.
    pub(crate) cfg: Rc<CfgInfo>,
    /// Module call summaries for passes that consume them.
    pub(crate) call_summaries: Option<Arc<MemoryCallSummaries>>,
}

/// Cached per-function analyses shared by every pass in one pipeline run.
#[doc(hidden)]
#[derive(Default)]
pub struct ModuleAnalyses {
    alias: FxHashMap<FunctionId, Rc<AliasAnalysis>>,
    cfg: FxHashMap<FunctionId, Rc<CfgInfo>>,
    call_summaries: Option<Arc<MemoryCallSummaries>>,
    preserved_by_pass: bool,
}

impl ModuleAnalyses {
    pub(crate) fn begin_pass(&mut self) {
        self.preserved_by_pass = false;
    }

    pub(crate) fn finish_pass(&mut self, changed: bool) {
        if changed && !self.preserved_by_pass {
            self.invalidate_all();
        }
    }

    /// Returns the shared alias-analysis snapshot for a function.
    pub(crate) fn alias(&mut self, func_id: FunctionId) -> Rc<AliasAnalysis> {
        Rc::clone(self.alias.entry(func_id).or_insert_with(|| Rc::new(AliasAnalysis::empty())))
    }

    /// Returns the shared CFG snapshot for a function.
    pub(crate) fn cfg(&mut self, func_id: FunctionId, func: &Function) -> Rc<CfgInfo> {
        Rc::clone(self.cfg.entry(func_id).or_insert_with(|| Rc::new(CfgInfo::new(func))))
    }

    fn bundle(&mut self, func_id: FunctionId, func: &Function) -> FunctionAnalyses {
        FunctionAnalyses {
            alias: self.alias(func_id),
            cfg: self.cfg(func_id, func),
            call_summaries: self.call_summaries.clone(),
        }
    }

    /// Provides module call summaries to subsequent pass runs.
    pub(crate) fn set_call_summaries(&mut self, summaries: Arc<MemoryCallSummaries>) {
        self.call_summaries = Some(summaries);
    }

    /// Withdraws module call summaries after the consuming pass completes.
    pub(crate) fn clear_call_summaries(&mut self) {
        self.call_summaries = None;
    }

    fn retain(&mut self, func_id: FunctionId, keep_alias: bool, keep_cfg: bool) {
        if keep_alias {
            if let Some(alias) = self.alias.get(&func_id) {
                alias.clear_cached_addresses();
            }
        } else {
            self.alias.remove(&func_id);
        }
        if !keep_cfg {
            self.cfg.remove(&func_id);
        }
    }

    fn invalidate_all(&mut self) {
        self.alias.clear();
        self.cfg.clear();
        self.call_summaries = None;
    }
}

fn cfg_edges(func: &Function) -> Vec<(u32, u32)> {
    let mut edges = Vec::new();
    for (block_id, block) in func.blocks.iter_enumerated() {
        if let Some(terminator) = &block.terminator {
            for successor in terminator.successors() {
                edges.push((block_id.index() as u32, successor.index() as u32));
            }
        }
    }
    edges.sort_unstable();
    edges
}

fn verified_preservation(
    func: &Function,
    edges_before: &[(u32, u32)],
    insts_before: usize,
) -> (bool, bool) {
    let edges_after = cfg_edges(func);
    let keep_cfg = edges_after == edges_before;
    let no_new_side_effects = (insts_before..func.instructions.len())
        .map(InstId::from_usize)
        .all(|inst_id| !func.instructions[inst_id].kind.has_side_effects());
    let keep_alias = no_new_side_effects
        && (keep_cfg || edges_after.iter().all(|edge| edges_before.binary_search(edge).is_ok()));
    (keep_alias, keep_cfg)
}

#[must_use]
fn run_function_pass_cached(
    analyses: &mut ModuleAnalyses,
    module: &mut Module,
    func_id: FunctionId,
    run: &mut impl FnMut(&mut Function, &FunctionAnalyses) -> bool,
) -> bool {
    let bundle = analyses.bundle(func_id, &module.functions[func_id]);
    let func = &mut module.functions[func_id];
    let edges_before = cfg_edges(func);
    let insts_before = func.instructions.len();
    let changed = run(func, &bundle);
    if changed {
        let (keep_alias, keep_cfg) = verified_preservation(func, &edges_before, insts_before);
        analyses.retain(func_id, keep_alias, keep_cfg);
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

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
//!     None,
//! );
//! ```

use crate::{
    analysis::{AliasAnalysis, CfgInfo, MemoryCallSummaries},
    mir::{Function, FunctionId, InstId, MirPhase, Module},
    pass_manager::{
        PipelineState, mir_output_name, parse_pass_pipeline, print_checkpoint, print_pass_diff,
        print_pass_output, run_stage, run_stage_passes,
    },
    timing::StageId,
    transform::*,
};
use solar_data_structures::map::FxHashMap;
use std::{
    any::{Any, TypeId},
    rc::Rc,
    sync::Arc,
};

pub use crate::pass_manager::{MirPass, pipeline_label, run_passes, run_passes_no_validate};

/// All known MIR passes exposed by `-Zmir-pipeline`.
pub static ALL_PASSES: &[&dyn MirPass] = &[
    &inline::Inline,
    &inline::InlineTinyLeaves,
    &inline::SpecializeFunctionPointers,
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
    &function_compaction::DeadArgElim,
    &function_compaction::MergeEquivalentFunctions,
    &memory_dse::MemoryDse,
    &coalesce_allocs::CoalesceAllocs,
    &static_alloc::StaticAlloc,
    &sroa::Sroa,
    &copy_elision::CopyElision,
    &dce::Dce,
    &adce::Adce,
    &lower_abi::LowerAbi,
    &lower_dispatch::LowerDispatch,
    &lower_evm_shaped::LowerEvmShaped,
    &lower_immutables::LowerImmutables,
    &lower_intrinsics::LowerIntrinsics,
    &lower_mapping_slots::LowerMappingSlots,
    &lower_mcopy::LowerMCopy,
    &lower_abi_encode::LowerAbiEncode,
    &lower_aggregates::LowerAggregates,
    &lower_memory_objects::LowerMemoryObjects,
    &lower_slices::LowerSlices,
    &lower_alloc::LowerAlloc,
    &lower_memory_zero::LowerMemoryZero,
    &lower_target::LowerTarget,
    &static_alloc::DeferAlloc,
    &evm_inst_schedule::EvmInstSchedule,
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

/// Materializes ABI wrapper functions.
static LOWER_ABI_PASSES: &[&dyn MirPass] = &[&lower_abi::LowerAbi];

/// Cleans generated ABI wrappers before helper lowering.
static ABI_CLEANUP_PASSES: &[&dyn MirPass] =
    &[&function_compaction::DeadArgElim, &dce::Dce, &function_compaction::MergeEquivalentFunctions];

/// Selects allocations that the backend can place in static frames.
static ALLOCATION_PLANNING_PASSES: &[&dyn MirPass] = &[&static_alloc::DeferAlloc];

/// Optimizes typed source-level MIR while semantic mapping hashes remain intact.
static SEMANTIC_HASH_OPTIMIZE_PASSES: &[&dyn MirPass] = &[
    &cfg_simplify::FunctionDce,
    &SizeOnly(cfg_simplify::CfgSimplify),
    &SizeOnly(frame_promotion::FrameSlotPromotion),
    &SizeOnly(sroa::Sroa),
    &sccp::Sccp,
    &pure_eval::PureEval,
    &inst_simplify::InstSimplify,
    &cse::Cse,
];

/// Expands mapping hashes after semantic CSE has reused equal slots.
static LOWER_MAPPING_PASSES: &[&dyn MirPass] = &[&lower_mapping_slots::LowerMappingSlots];

/// Optimizes scratch-memory hashes and the remaining source-level MIR.
static SEMANTIC_OPTIMIZE_PASSES: &[&dyn MirPass] = &[
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
    &GasOnly(cfg_simplify::CfgSimplify),
    &sroa::Sroa,
    &copy_elision::CopyElision,
    &memory_dse::MemoryDse,
    &adce::Adce,
    &outline_reverts::OutlineReverts,
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &GasOnly(inline::InlineTinyLeaves),
    &inline::SpecializeFunctionPointers,
    &function_compaction::DeadArgElim,
    &cfg_simplify::FunctionDce,
    &sccp::Sccp,
    &inst_simplify::InstSimplify,
    &cse::Cse,
    &gvn::Gvn,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    &memory_dse::MemoryDse,
    &adce::Adce,
    &function_compaction::MergeEquivalentFunctions,
    &cfg_simplify::FunctionDce,
];

/// Lowers ABI codecs and optimizes their generated helpers.
static LOWER_CODECS_PASSES: &[&dyn MirPass] = &[
    &lower_abi_encode::LowerAbiEncode,
    &frame_promotion::FrameSlotPromotion,
    &lower_aggregates::LowerAggregates,
    &inst_simplify::InstSimplify,
    &cfg_simplify::CfgSimplify,
    &memory_dse::MemoryDse,
    &cse::Cse,
    &dce::Dce,
    &lower_slices::LowerSlices,
];

/// Materializes selector dispatch after generated helper cleanup.
static LOWER_DISPATCH_PASSES: &[&dyn MirPass] = &[&lower_dispatch::LowerDispatch];

/// Completes the semantic-intrinsic representation boundary.
static LOWER_INTRINSICS_PASSES: &[&dyn MirPass] = &[&lower_intrinsics::LowerIntrinsics];

/// Cleans memory and CFG exposed by intrinsic lowering.
static LOW_LEVEL_OPTIMIZE_PASSES: &[&dyn MirPass] =
    &[&gvn::Gvn, &copy_elision::CopyElision, &adce::Adce];

/// Folds arithmetic and CFG introduced by target-dependent lowering.
static TARGET_CLEANUP_PASSES: &[&dyn MirPass] = &[&sccp::Sccp];

/// Selects allocation placement, then applies target-dependent lowering.
static TARGET_LOWERING_PASSES: &[&dyn MirPass] = &[
    &lower_immutables::LowerImmutables,
    &coalesce_allocs::CoalesceAllocs,
    &lower_target::LowerTarget,
];

/// Makes non-returning call edges explicit for the backend.
static EVM_SHAPING_PASSES: &[&dyn MirPass] = &[&lower_evm_shaped::LowerEvmShaped];

/// Cleans target-lowered and EVM-shaped MIR before stack-oriented scheduling.
static FINAL_CLEANUP_PASSES: &[&dyn MirPass] = &[&dce::Dce];

/// Orders final MIR for the physical EVM stack scheduler.
static SCHEDULE_PASSES: &[&dyn MirPass] = &[&evm_inst_schedule::EvmInstSchedule];

/// Runs the configured MIR pipeline, substituting it for the canonical pipeline.
///
/// `name` overrides the module name in pass output. Named lowering passes advance representation
/// phases in both canonical and custom pipelines.
#[tracing::instrument(
    name = "mir_pipeline",
    level = "debug",
    skip_all,
    fields(module = %module.name),
)]
#[must_use]
pub fn run_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
    if module.phase >= MirPhase::TargetLowered
        && !lower_target::target_operations_are_lowered(
            module,
            gcx.sess.opts.evm_version.has_mcopy(),
        )
    {
        gcx.dcx()
            .err(format!(
                "MIR module claims the `{}` phase but still contains target-dependent operations",
                module.phase.name()
            ))
            .emit();
        return false;
    }
    if let Some(value) = gcx.sess.opts.unstable.mir_pipeline.as_deref() {
        let pipeline = match parse_pass_pipeline(gcx, value, "MIR", lookup_pass) {
            Ok(pipeline) => pipeline,
            Err(_) => return false,
        };
        if let Some(passes) = pipeline {
            let name = name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module));
            let mut changed = false;
            let mut state = PipelineState::default();
            let stage = StageId::new("custom", 1);
            let mut none_invocation = 0;
            for pass in passes {
                if let Some(pass) = pass {
                    let expected_phase =
                        pass.is_enabled(gcx, module).then(|| pass.output_phase()).flatten();
                    changed |= run_stage_passes(
                        gcx,
                        module,
                        &[pass],
                        expected_phase,
                        Some(&name),
                        true,
                        stage,
                        &mut state,
                    );
                    if gcx.dcx().has_errors().is_err() {
                        return changed;
                    }
                } else if gcx.sess.opts.unstable.pass_diff
                    && !gcx.sess.opts.unstable.print_after_stage
                {
                    none_invocation += 1;
                    let text = module.to_text();
                    print_pass_diff(
                        &name,
                        "MIR",
                        None,
                        state.pipeline_run(),
                        stage,
                        "none",
                        none_invocation,
                        false,
                        false,
                        false,
                        "skipped",
                        &text,
                        &text,
                    );
                } else if gcx.sess.opts.unstable.print_after_each {
                    none_invocation += 1;
                    print_pass_output(
                        &name,
                        "MIR",
                        None,
                        state.pipeline_run(),
                        stage,
                        "none",
                        none_invocation,
                        false,
                        "skipped",
                        false,
                        false,
                        module.to_text(),
                    );
                }
            }
            if gcx.sess.opts.unstable.print_after_stage {
                print_checkpoint(
                    &name,
                    "MIR",
                    None,
                    state.pipeline_run(),
                    stage,
                    "custom-output",
                    module.to_text(),
                );
            }
            return changed;
        }
    }

    let output_name = name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module));
    let mut changed = false;
    let mut state = PipelineState::default();
    if gcx.sess.opts.unstable.print_after_stage {
        let checkpoint = match module.phase {
            MirPhase::Built => "mir.fresh",
            MirPhase::Abi => "mir.abi-input",
            MirPhase::Dispatch => "mir.semantic-materialized-input",
            MirPhase::IntrinsicsLowered => "mir.intrinsics-lowered-input",
            MirPhase::TargetLowered => "mir.target-lowered-input",
            MirPhase::EvmShaped => "mir.evm-shaped-input",
        };
        print_checkpoint(
            &output_name,
            "MIR",
            None,
            state.pipeline_run(),
            StageId::new("input", 1),
            checkpoint,
            module.to_text(),
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }

    if module.phase == MirPhase::Built {
        changed |= run_stage(
            gcx,
            module,
            SEMANTIC_HASH_OPTIMIZE_PASSES,
            None,
            &output_name,
            StageId::new("optimize-semantic-hashes", 1),
            "mir.semantic-hashes-optimized",
            MirPhase::Built,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            LOWER_MAPPING_PASSES,
            None,
            &output_name,
            StageId::new("lower-mapping-hashes", 1),
            "mir.mapping-lowered",
            MirPhase::Built,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            SEMANTIC_OPTIMIZE_PASSES,
            None,
            &output_name,
            StageId::new("optimize-source-low-level", 1),
            "mir.source-optimized",
            MirPhase::Built,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    if module.phase == MirPhase::Built {
        changed |= run_stage(
            gcx,
            module,
            LOWER_ABI_PASSES,
            Some(MirPhase::Abi),
            &output_name,
            StageId::new("lower-abi", 1),
            "mir.abi",
            MirPhase::Abi,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    if module.phase == MirPhase::Abi {
        changed |= run_stage(
            gcx,
            module,
            ABI_CLEANUP_PASSES,
            None,
            &output_name,
            StageId::new("optimize-abi", 1),
            "mir.abi-optimized",
            MirPhase::Abi,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            ALLOCATION_PLANNING_PASSES,
            None,
            &output_name,
            StageId::new("plan-allocations", 1),
            "mir.allocations-planned",
            MirPhase::Abi,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            LOWER_CODECS_PASSES,
            None,
            &output_name,
            StageId::new("lower-codecs", 1),
            "mir.codecs-lowered",
            MirPhase::Abi,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            LOWER_DISPATCH_PASSES,
            Some(MirPhase::Dispatch),
            &output_name,
            StageId::new("lower-dispatch", 1),
            "mir.dispatch",
            MirPhase::Dispatch,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    if module.phase == MirPhase::Dispatch {
        changed |= run_stage(
            gcx,
            module,
            LOWER_INTRINSICS_PASSES,
            Some(MirPhase::IntrinsicsLowered),
            &output_name,
            StageId::new("lower-intrinsics", 1),
            "mir.intrinsics-lowered",
            MirPhase::IntrinsicsLowered,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    if module.phase == MirPhase::IntrinsicsLowered {
        changed |= run_stage(
            gcx,
            module,
            LOW_LEVEL_OPTIMIZE_PASSES,
            None,
            &output_name,
            StageId::new("optimize-low-level", 1),
            "mir.low-level-optimized",
            MirPhase::IntrinsicsLowered,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            TARGET_LOWERING_PASSES,
            Some(MirPhase::TargetLowered),
            &output_name,
            StageId::new("lower-target", 1),
            "mir.target-lowered",
            MirPhase::TargetLowered,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    if module.phase == MirPhase::TargetLowered {
        changed |= run_stage(
            gcx,
            module,
            TARGET_CLEANUP_PASSES,
            None,
            &output_name,
            StageId::new("optimize-target-generated", 1),
            "mir.target-optimized",
            MirPhase::TargetLowered,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            EVM_SHAPING_PASSES,
            Some(MirPhase::EvmShaped),
            &output_name,
            StageId::new("evm-shape", 1),
            "mir.evm-shaped",
            MirPhase::EvmShaped,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    if module.phase == MirPhase::EvmShaped {
        changed |= run_stage(
            gcx,
            module,
            FINAL_CLEANUP_PASSES,
            None,
            &output_name,
            StageId::new("final-cleanup", 1),
            "mir.final",
            MirPhase::EvmShaped,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        changed |= run_stage(
            gcx,
            module,
            SCHEDULE_PASSES,
            None,
            &output_name,
            StageId::new("schedule", 1),
            "mir.scheduled",
            MirPhase::EvmShaped,
            &mut state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
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

    pub(crate) fn invalidate(&mut self) {
        self.preserved_by_pass = false;
        self.invalidate_all();
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
    let no_new_side_effects = (insts_before..func.num_insts())
        .map(InstId::from_usize)
        .all(|inst_id| !func.inst(inst_id).kind.has_side_effects());
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
    let insts_before = func.num_insts();
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

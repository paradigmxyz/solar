//! Pass infrastructure for MIR transformations and analyses.
//!
//! Transformation pipelines follow rustc MIR's pass-manager shape: passes
//! implement [`MirPass`] and pipelines are slices of trait-object references.
//! [`ModuleAnalyses`] caches per-function CFG and alias analyses and module
//! call summaries between the passes of one pipeline run. A function-local pass
//! of a large module runs its functions on idle workers once no contract job
//! waits in the queue; the result and the caches match a serial run.
//!
//! # Usage
//!
//! ```ignore
//! let changed = run_passes(
//!     gcx,
//!     &mut module,
//!     &[&dce::Dce],
//!     None,
//! );
//! ```

use crate::{
    mir::{
        Function, FunctionId, InstId, MirPhase, Module,
        analysis::{AliasAnalysis, CfgInfo, LocalSummaryCache, MemoryCallSummaries},
        pass_manager::{mir_output_name, parse_pass_pipeline, print_pass_diff, run_passes_inner},
        transform::*,
    },
    target::Target,
};
use either::Either;
use smallvec::SmallVec;
use solar_config::OptimizationMode;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap, sync};
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_sema::Gcx;
use std::{any::TypeId, cell::Cell, convert::Infallible, rc::Rc, sync::Arc};

pub use crate::mir::pass_manager::{MirPass, pipeline_label, run_passes};

/// All known MIR passes exposed by `-Zmir-pipeline`.
static ALL_PASSES: &[&dyn MirPass] = &[
    &inline::Inline::Semantic,
    &inline::Inline::Tagged,
    &if_convert::IfConvert,
    &inline::SpecializeFunctionPointers,
    &specialize::Specialize,
    &call_cleanup::CallCleanup,
    &outline_reverts::OutlineReverts,
    &cfg_simplify::FunctionDce,
    &cfg_simplify::NoreturnCalls,
    &cfg_simplify::SimplifyTrivialPhis,
    &sccp::Sccp,
    &pure_eval::PureEval,
    &const_fold::ConstFold,
    &readonly_eval::ReadonlyEval,
    &cse::Cse::All,
    &pre::Pre::Pure,
    &element_cleanup::ElementCleanup,
    &egraph::Egraph,
    &word_sequence::WordSequence::Late,
    &byte_loops::ByteLoops,
    &byte_run::ByteRunLoads,
    &cse::Cse::Storage,
    &storage_dse::StorageDse,
    &pre::Pre::Reads,
    &loop_canonicalize::LoopCanonicalize,
    &loop_exit_remat::LoopExitRemat,
    &loop_reduce::LoopReduce,
    &loop_split::LoopSplit,
    &loop_unroll::LoopUnroll,
    &loop_version::LoopVersion,
    &merge_aborts::MergeAborts,
    &indvar_simplify::IndVarSimplify,
    &storage_promotion::StorageScalarPromotion,
    &loop_opt::Licm,
    &check_elim::CheckElim,
    &check_elim::LateCheckElim,
    &check_elim::IntegerCleanup,
    &check_elim::ImmutableCheckElim,
    &jump_threading::JumpThreading,
    &cfg_simplify::BranchSimplify,
    &cfg_simplify::CfgSimplify,
    &frame_promotion::FrameSlotPromotion,
    &function_compaction::DeadArgElim,
    &function_compaction::MergeEquivalentFunctions,
    &memory_dse::MemoryDse::Standard,
    &memory_dse::MemoryDse::WithUnreadAllocations,
    &coalesce_allocs::CoalesceAllocs,
    &static_alloc::DeferAlloc,
    &sroa::Sroa,
    &dce::Dce,
    &lower_arithmetic::LowerArithmetic,
    &lower_builtins::LowerBuiltins,
    &lower_checks::LowerChecks,
    &lower_abi::LowerAbi,
    &lower_dispatch::LowerDispatch,
    &lower_structs::LowerStructs,
    &lower_frame_slots::LowerFrameSlots,
    &lower_evm_shaped::LowerEvmShaped,
    &lower_immutables::LowerImmutables,
    &lower_integers::LowerIntegers,
    &lower_mcopy::LowerMCopy,
    &lower_abi_encode::LowerAbiEncode,
    &lower_mapping_slots::LowerMappingSlots,
    &lower_memory_objects::LowerMemoryObjects,
    &lower_slices::LowerSlices,
    &lower_alloc::LowerAlloc,
    &lower_memory_zero::LowerMemoryZero,
    &lower_shifts::LowerShifts,
    &evm_inst_schedule::EvmInstSchedule,
    &heap_floor::HeapFloor,
];

/// Finds a MIR pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn MirPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

struct SizeOnly<P>(P);

impl<P> SizeOnly<P> {
    const fn new(pass: P) -> Self {
        Self(pass)
    }
}

impl<P: MirPass> MirPass for SizeOnly<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        self.covers(module) && self.0.is_enabled(gcx, module)
    }

    fn is_contract_wide(&self) -> bool {
        self.0.is_contract_wide()
    }

    fn runs_on(&self, objective: OptimizationMode) -> bool {
        objective.is_size() && self.0.runs_on(objective)
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

impl<P> GasOnly<P> {
    const fn new(pass: P) -> Self {
        Self(pass)
    }
}

impl<P: MirPass> MirPass for GasOnly<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        self.covers(module) && self.0.is_enabled(gcx, module)
    }

    fn is_contract_wide(&self) -> bool {
        self.0.is_contract_wide()
    }

    fn runs_on(&self, objective: OptimizationMode) -> bool {
        objective.is_gas() && self.0.runs_on(objective)
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

/// Runs one pipeline slot at `-O none` too, for cheap cleanups whose bytecode savings repay their
/// compile time. Wrap single slots, not every occurrence of a pass.
struct RunsAtNone<P>(P);

impl<P> RunsAtNone<P> {
    const fn new(pass: P) -> Self {
        Self(pass)
    }
}

impl<P: MirPass> MirPass for RunsAtNone<P> {
    fn name(&self) -> &'static str {
        self.0.name()
    }

    fn is_enabled(&self, gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        self.covers(module) || self.0.is_enabled(gcx, module)
    }

    fn is_contract_wide(&self) -> bool {
        self.0.is_contract_wide()
    }

    fn runs_on(&self, objective: OptimizationMode) -> bool {
        objective.is_none() || self.0.runs_on(objective)
    }

    fn is_required(&self) -> bool {
        self.0.is_required()
    }

    fn cache_key(&self) -> Option<(&'static str, &'static str)> {
        self.0.cache_key()
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
static SEMANTIC_PIPELINE: &[&dyn MirPass] = &[
    // Clamp inline assembly's stores of absolute addresses to the free memory pointer before any
    // pass forwards the stored value to the pointer's loads.
    &heap_floor::HeapFloor,
    &cfg_simplify::FunctionDce,
    // A failure path through a revert helper ends at the call, so the code after it keeps the
    // facts the failing branch states.
    &cfg_simplify::NoreturnCalls,
    // HIR lowering leaves trivial phis for locals that loops never reassign.
    // They hide one object behind several SSA names, which alias analysis
    // joins to distinct symbolic pointers, so remove them before the loop and
    // memory passes. Only the phis: merging blocks this early hides the
    // short-circuit shapes jump threading recognizes after check elimination.
    &cfg_simplify::SimplifyTrivialPhis,
    // `@custom:solar-inline always` helpers join their callers before any pass reads
    // a loop, an object, or a bound through the call.
    &inline::Inline::Tagged,
    &sccp::Sccp,
    &pure_eval::PureEval,
    &egraph::Egraph,
    &word_sequence::WordSequence::Early,
    &pre::Pre::Pure,
    // Checked array loops reload one object's length before every bounds
    // check. Unify the dominated reads while object identity is explicit, so
    // load PRE and LICM see one loop-invariant load and the later check passes
    // compare against one bound.
    &cse::Cse::All,
    &pre::Pre::Reads,
    // Forwarded loads fold the checks of a helper that only fails, such as
    // `new uint256[](0)[0]`; end them, and the calls to it, before element
    // cleanup bounds the values that reach stores.
    &cfg_simplify::NoreturnCalls,
    // Element reads of arrays that only ever hold canonical words drop their
    // type masks while the accesses are still semantic and calls explicit.
    &element_cleanup::ElementCleanup,
    &frame_promotion::FrameSlotPromotion,
    &loop_canonicalize::LoopCanonicalize,
    // Replace byte-at-a-time loops with word operations while checks are single instructions,
    // before the inliner prices the helpers that spell them.
    &byte_loops::ByteLoops,
    // Evaluate recurrences before gas-bounded reasoning removes checks that
    // become reachable if a loop is replaced by a constant-time formula.
    &GasOnly::new(dce::Dce),
    &GasOnly::new(loop_reduce::LoopReduce),
    &indvar_simplify::IndVarSimplify,
    &storage_promotion::StorageScalarPromotion,
    &loop_opt::Licm,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    // A loop that stores into one array and reloads another's length gets a copy that reads
    // the lengths once, behind a run-time test that the stores cannot reach them. It runs on
    // the threaded graph, where the short-circuit conditions that cover a check dominate it;
    // the check elimination after memory lowering folds the copy's bounds.
    &GasOnly::new(loop_version::LoopVersion),
    &sroa::Sroa,
    &memory_dse::MemoryDse::Standard,
    &dce::Dce,
    // Late control-flow rewrites expose scalar simplifications.
    // Thread and clean the CFG first so the rest of this sequence observes the
    // simplified graph in one pass through the pipeline.
    &jump_threading::JumpThreading,
    &cfg_simplify::CfgSimplify,
    &GasOnly::new(inline::Inline::Semantic),
    &inline::SpecializeFunctionPointers,
    &specialize::Specialize,
    &function_compaction::DeadArgElim,
    &cfg_simplify::FunctionDce,
    &function_compaction::MergeEquivalentFunctions,
    &cfg_simplify::FunctionDce,
];

/// Expands semantic operations and makes the backend representation explicit.
static LOWERING_PIPELINE: &[&dyn MirPass] = &[
    &lower_checks::LowerChecks,
    &lower_builtins::LowerBuiltins,
    // Gas mode keeps arithmetic failure edges local for stack scheduling.
    &GasOnly::new(outline_reverts::OutlineReverts),
    &lower_arithmetic::LowerArithmetic,
    // Size mode shares arithmetic payloads too, before selecting stack layouts.
    &SizeOnly::new(outline_reverts::OutlineReverts),
    // Expansion exposes scalar checks and object copies to this bounded cleanup group.
    // Peel the last groups of lookahead loops so the main loop's bound folds
    // the body guards in the check elimination below.
    &GasOnly::new(loop_split::LoopSplit),
    &sccp::Sccp,
    &egraph::Egraph,
    &word_sequence::WordSequence::Early,
    &check_elim::CheckElim,
    &jump_threading::JumpThreading,
    // Runs at `-O none` too: it removes the empty blocks lowering leaves, which shrinks every
    // later pass's input and the bytecode.
    &RunsAtNone::new(cfg_simplify::CfgSimplify),
    // Small pure diamonds become selects once folded conditions are gone.
    &if_convert::IfConvert,
    &frame_promotion::FrameSlotPromotion,
    // Builtin expansion exposes buffers whose readers are gone.
    &memory_dse::MemoryDse::WithUnreadAllocations,
    &lower_abi::LowerAbi,
    // ABI lowering exposes canonical-word helpers to the same inlining policy.
    &inline::Inline::Semantic,
    // Getter inlining exposes runtime immutable widths after the general check passes.
    &check_elim::ImmutableCheckElim,
    &cfg_simplify::FunctionDce,
    // ABI lowering adds shared decoders whose callers often all pass the same
    // constant head; substitute it so dead-argument elimination can drop the
    // parameter.
    &specialize::Specialize,
    &function_compaction::DeadArgElim,
    &dce::Dce,
    &function_compaction::MergeEquivalentFunctions,
    &cfg_simplify::FunctionDce,
    &static_alloc::DeferAlloc,
    &lower_abi_encode::LowerAbiEncode,
    // Packed storage writes leave read-modify-write sequences. Forward whole
    // words before deleting the overwritten stores, then simplify masks.
    &cse::Cse::Storage,
    &storage_dse::StorageDse,
    &egraph::Egraph,
    &word_sequence::WordSequence::Late,
    &cfg_simplify::CfgSimplify,
    &memory_dse::MemoryDse::Standard,
    // Expose callee reads before late CSE.
    &inline::Inline::Semantic,
    &cse::Cse::All,
    // Forwarding exposes masks on stored words, such as ABI-encoded struct fields. Fold them
    // while integer types still exist.
    &egraph::Egraph,
    // Fold checks exposed by forwarding in late CSE. Gas-only: under `-Osize` it grows the
    // corpus's bytecode.
    &GasOnly::new(check_elim::CheckElim),
    &dce::Dce,
    // Forwarding has replaced the reads of allocation headers, which leaves their stores dead
    // while the allocations are still atomic.
    &memory_dse::MemoryDse::Standard,
    &lower_dispatch::LowerDispatch,
    &lower_structs::LowerStructs,
    // Hoisted ABI guards leave empty wrapper entries and unreachable reverts.
    &cfg_simplify::CfgSimplify,
    // Dispatch can hoist a common ABI head-size guard out of every selector
    // wrapper. Remove the dead per-wrapper comparisons before frame lowering.
    &dce::Dce,
    &lower_frame_slots::LowerFrameSlots,
    // Expand semantic mapping locations after ABI, dispatch, and frame
    // lowering, while keeping variable-size hash objects ahead of the memory
    // boundary.
    &lower_mapping_slots::LowerMappingSlots,
    &lower_memory_objects::LowerMemoryObjects,
    &cse::Cse::All,
    // Physical memory accesses let CSE unify semantic lengths with raw loads.
    // Repeated bounds checks then use the same condition as a dominating guard:
    // branch condition, body, exit; body: ...; branch condition, checked, panic
    // => body: ...; jump checked
    &check_elim::LateCheckElim,
    &inline::Inline::Physical,
    // Revisit allocations after semantic memory accesses become bounded raw
    // operations, so fixed-size hash buffers can use backend-known static
    // regions.
    &static_alloc::DeferAlloc,
    &lower_slices::LowerSlices,
    // Fuse straight-line constant-size allocations before their free-memory
    // pointer traffic is materialized; pointer values are preserved exactly.
    &coalesce_allocs::CoalesceAllocs,
    // Evaluate known allocation words before immutable initialization becomes
    // physical memory stores that conservatively alias heap pointers.
    &readonly_eval::ReadonlyEval,
    &lower_immutables::LowerImmutables,
    // Expand copies before `lower-alloc`, while allocations are still symbolic,
    // so provenance can prove more ranges disjoint and skip the runtime
    // direction check.
    &lower_mcopy::LowerMCopy,
    &lower_alloc::LowerAlloc,
    // Allocation lowering exposes zero fills and free-memory-pointer stores
    // that later stores in the same block can overwrite completely, and word
    // stores that later loads can forward.
    &memory_dse::MemoryDse::Standard,
    &lower_memory_zero::LowerMemoryZero,
    // Materialize integer cleanup before word-level extraction and scheduling.
    &lower_integers::LowerIntegers,
    // Carry proved argument widths across calls before simplifying word masks.
    &call_cleanup::CallCleanup,
    // Shared scalar ABI words and wrapper bodies become one CFG before extraction.
    &inline::Inline::Physical,
    &check_elim::IntegerCleanup,
    // Memory lowering materializes address arithmetic; number and simplify it
    // once more before the physical shape is fixed. The stack-aware cost keeps
    // rewrites from reaching for values the scheduler would have to keep alive.
    &egraph::Egraph,
    &word_sequence::WordSequence::Late,
    // Packing consecutive bytes out of one buffer reads each byte with its own
    // word read. The simplification above canonicalizes each read to one byte
    // extraction; fuse the run into the one read it is a field of, before the
    // loop passes below hoist and step addresses the run no longer uses.
    &byte_run::ByteRunLoads,
    // Memory lowering materializes each element access as `add base, 32`
    // plus an index term inside the loop that reads it. Hoist the invariant
    // base once the physical form is final, so a hot loop carries one word
    // instead of reloading its argument and re-adding the header every
    // iteration. Checked products are lowered by now, so this run also
    // replaces their in-loop divisions by one hoisted bound.
    &loop_opt::Licm,
    // With the base hoisted, each element address is `base + scale * index`
    // plus invariants; carry it as a pointer stepped on the latch instead of
    // rebuilding it from the index every iteration. Gas-only: under `-Osize` it grows
    // nearly every contract.
    &GasOnly::new(indvar_simplify::IndVarSimplify),
    // Counted loops test their bound once per two iterations once their
    // physical shape is final; the cleanup below drops the unread clones.
    // Size mode copies no loops; there it only counts up-counting loops from `-n` to zero, which
    // shrinks them.
    &loop_unroll::LoopUnroll,
    // The unrolled main loop's test bounds the first copy's counter, so its
    // overflow checks fold. Memory and integer lowering also leave guards that
    // fold here in every mode.
    &check_elim::CheckElim,
    // ABI and memory lowering leave dead guards and empty trampoline blocks.
    // Clean them before EVM shaping isolates phi copies on critical edges.
    &cfg_simplify::CfgSimplify,
    // With each straight-line run in one block, the checks that remain branch
    // once per run.
    &merge_aborts::MergeAborts,
    // A word-at-a-time loop is compact enough to consume at its sole call site.
    // This removes the internal frame protocol without duplicating the body;
    // the pass drops the consumed callee itself.
    &inline::Inline::Physical,
    &cfg_simplify::CfgSimplify,
    &lower_evm_shaped::LowerEvmShaped,
];

/// Optimizes lowered word SSA before physical stack scheduling.
static LOWERED_PIPELINE: &[&dyn MirPass] = &[
    &const_fold::ConstFold,
    &cfg_simplify::BranchSimplify,
    // Reconstruct old induction values on exits before selecting physical stack order.
    &loop_exit_remat::LoopExitRemat,
    // Late lowering can leave pure address and length calculations unused.
    // Remove their complete dependency chains before selecting physical stack order.
    &dce::Dce,
    // Targets without shift opcodes get arithmetic instead, so scheduling orders its operands.
    &lower_shifts::LowerShifts,
    &evm_inst_schedule::EvmInstSchedule,
];

/// Runs the configured MIR pipeline, substituting it for the canonical pipeline.
///
/// `name` overrides the module name in pass output. The canonical pipeline advances the module
/// through semantic optimization, representation conversion, and word optimization. Individual
/// passes preserve the phase except the checked completion of `lower-evm-shaped`.
#[tracing::instrument(name = "mir_pipeline", level = "debug", skip_all)]
#[must_use]
pub fn run_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
    if let Some(value) = gcx.sess.opts.unstable.mir_pipeline.as_deref() {
        let pipeline = match parse_pass_pipeline(gcx, value, "MIR", lookup_pass) {
            Ok(pipeline) => pipeline,
            Err(_) => return false,
        };
        if let Some(passes) = pipeline {
            let name = name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module));
            let mut changed = false;
            let mut remaining = passes.as_slice();
            while !remaining.is_empty() {
                let end = remaining.iter().position(Option::is_none).unwrap_or(remaining.len());
                let batch = remaining[..end].iter().copied().flatten().collect::<Vec<_>>();
                let (pass_changed, error) = run_passes_inner(gcx, module, &batch, Some(&name));
                changed |= pass_changed;
                if error.is_some() || end == remaining.len() {
                    return changed;
                }
                remaining = &remaining[end + 1..];
                if gcx.sess.opts.unstable.pass_diff {
                    let text = module.to_text();
                    print_pass_diff(&name, "none", &text, &text);
                } else if gcx.sess.opts.unstable.print_after_each {
                    println!("// === {name} (after none) ===");
                    print!("{}", module.to_text());
                }
            }
            return changed;
        }
    }

    let mut changed = false;
    if module.phase() == MirPhase::Semantic {
        let (pass_changed, error) = run_passes_inner(gcx, module, SEMANTIC_PIPELINE, None);
        changed |= pass_changed;
        if error.is_some() {
            return changed;
        }
        let (pass_changed, error) = run_passes_inner(gcx, module, LOWERING_PIPELINE, None);
        changed |= pass_changed;
        if error.is_some() {
            return changed;
        }
    }
    changed |= run_passes_inner(gcx, module, LOWERED_PIPELINE, None).0;
    changed
}

/// Runs a function-local transform over every bodied function in a module.
#[must_use]
pub(crate) fn run_function_pass(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with(module, analyses, FunctionAnalysisRequirements::NONE, &run)
}

/// Runs the function-local step of a pass that edits the module beyond the runner's bookkeeping:
/// bodyless functions first, then the bodied ones through [`run_function_pass`], so idle workers
/// can share them. The pass drops every cached analysis when it changes anything, as a serial
/// loop over the module does, and prices no creation code.
#[must_use]
pub(crate) fn run_lowering_function_pass(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    analyses.prices_creation = false;
    let mut changed = false;
    for id in module.functions.indices() {
        if module.functions[id].blocks.is_empty() {
            changed |= run_function_pass_cached(
                analyses,
                module,
                id,
                FunctionAnalysisRequirements::NONE,
                None,
                &run,
            );
        }
    }
    changed |= run_function_pass(module, analyses, run);
    analyses.preserved_by_pass = false;
    changed
}

/// Runs a function-local transform with alias analysis available on demand.
#[must_use]
pub(crate) fn run_function_pass_with_alias(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with(module, analyses, FunctionAnalysisRequirements::ALIAS, &run)
}

/// Runs a function-local transform with CFG analysis available on demand.
#[must_use]
pub(crate) fn run_function_pass_with_cfg(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with(module, analyses, FunctionAnalysisRequirements::CFG, &run)
}

/// Runs a function-local transform with alias and CFG analyses available.
#[must_use]
pub(crate) fn run_function_pass_with_alias_and_cfg(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with(module, analyses, FunctionAnalysisRequirements::ALL, &run)
}

fn run_function_pass_with(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    requirements: FunctionAnalysisRequirements,
    run: FunctionRun<'_>,
) -> bool {
    run_function_pass_with_cache(module, analyses, None, requirements, None, run)
}

fn run_function_pass_with_cache(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    selected: Option<&DenseBitSet<FunctionId>>,
    requirements: FunctionAnalysisRequirements,
    cache_key: Option<TypeId>,
    run: FunctionRun<'_>,
) -> bool {
    if let Some(changed) =
        run_parallel_function_pass(module, analyses, selected, requirements, cache_key, run)
    {
        return changed;
    }
    let functions = match selected {
        Some(selected) => Either::Left(selected.iter()),
        None => Either::Right(module.functions.indices()),
    };
    let mut changed = false;
    for func_id in functions {
        if module.functions[func_id].blocks.is_empty() || !analyses.runs_on(module, func_id) {
            continue;
        }
        changed |=
            run_function_pass_cached(analyses, module, func_id, requirements, cache_key, run);
    }
    analyses.preserved_by_pass = true;
    changed
}

/// Runs a transform only on selected functions, preserving unaffected analysis caches.
#[must_use]
pub(crate) fn run_selected_function_pass(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    selected: &DenseBitSet<FunctionId>,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with_cache(
        module,
        analyses,
        Some(selected),
        FunctionAnalysisRequirements::CFG,
        None,
        &run,
    )
}

/// Runs an alias- and CFG-aware transform only on selected functions.
#[must_use]
pub(crate) fn run_selected_function_pass_with_alias_and_cfg(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    selected: &DenseBitSet<FunctionId>,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with_cache(
        module,
        analyses,
        Some(selected),
        FunctionAnalysisRequirements::ALL,
        None,
        &run,
    )
}

/// Runs a CFG-aware function-local transform, skipping bodies on which the
/// same transform previously reported no change and no intervening local pass
/// has changed the body.
#[must_use]
pub(crate) fn run_selected_function_pass_cached<P: 'static>(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    selected: &DenseBitSet<FunctionId>,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with_cache(
        module,
        analyses,
        Some(selected),
        FunctionAnalysisRequirements::CFG,
        Some(TypeId::of::<P>()),
        &run,
    )
}

/// Runs an analysis-free function-local transform with unchanged-body reuse.
#[must_use]
pub(crate) fn run_selected_function_pass_without_analyses_cached<P: 'static>(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    selected: &DenseBitSet<FunctionId>,
    run: impl Fn(&mut Function, &FunctionAnalyses) -> bool + Sync,
) -> bool {
    run_function_pass_with_cache(
        module,
        analyses,
        Some(selected),
        FunctionAnalysisRequirements::NONE,
        Some(TypeId::of::<P>()),
        &run,
    )
}

/// A function-local transform. The runners take it as a trait object so that each pass does not
/// instantiate them again.
type FunctionRun<'a> = &'a (dyn Fn(&mut Function, &FunctionAnalyses) -> bool + Sync);

/// Per-function analysis snapshots handed to a pass run.
pub(crate) struct FunctionAnalyses {
    /// The function the transform runs on.
    function: FunctionId,
    /// Shared alias analysis; provenance and address memos build lazily.
    alias: Option<Rc<AliasAnalysis>>,
    /// Shared CFG snapshot; RPO, dominators, and reachability build lazily.
    cfg: Option<Rc<CfgInfo>>,
    /// Whether the transform edited the body while reporting no change.
    unreported_edit: Cell<bool>,
    /// Whether the function runs only during deployment, and no loop or recursion calls it.
    creation_only: bool,
    /// Whether, besides, the function has no loop of its own.
    straight_line_creation: bool,
    /// The objective the function's code is optimized for.
    optimization: OptimizationMode,
    /// Whether the function is cold: no call that succeeds runs it. The cost models below price a
    /// cold function by [`Target::priced_for_cold`].
    cold: bool,
}

impl FunctionAnalyses {
    fn new(
        function: FunctionId,
        alias: Option<Rc<AliasAnalysis>>,
        cfg: Option<Rc<CfgInfo>>,
        (creation_only, straight_line_creation): (bool, bool),
        optimization: OptimizationMode,
        cold: bool,
    ) -> Self {
        Self {
            function,
            alias,
            cfg,
            unreported_edit: Cell::new(false),
            creation_only,
            straight_line_creation,
            optimization,
            cold,
        }
    }

    /// Returns the id of the function the running transform edits.
    pub(crate) fn function(&self) -> FunctionId {
        self.function
    }

    /// Returns the alias analysis requested by the running transform.
    pub(crate) fn alias(&self) -> &Rc<AliasAnalysis> {
        self.alias.as_ref().expect("function pass must request alias analysis")
    }

    /// Returns the CFG analysis requested by the running transform.
    pub(crate) fn cfg(&self) -> &Rc<CfgInfo> {
        self.cfg.as_ref().expect("function pass must request CFG analysis")
    }

    /// The cost model for a transform that weighs loops itself: creation code that no loop or
    /// recursion calls is priced as such whether or not it loops, because the transform
    /// multiplies by trip counts.
    pub(crate) fn loop_aware_target(&self, gcx: Gcx<'_>) -> Target {
        self.runtime_target(gcx).priced_for(self.creation_only)
    }

    /// The cost model of runtime code under the module's objective, for code that a loop may run
    /// many times, wherever it is.
    pub(crate) fn runtime_target(&self, gcx: Gcx<'_>) -> Target {
        Target::new(gcx, self.optimization).priced_for_cold(self.cold)
    }

    /// The cost model for a transform that ranks code shapes without weighing blocks by how
    /// often they run: creation code is priced as such only when every instruction runs a
    /// bounded number of times, so without loops, recursion, or a looping caller.
    pub(crate) fn target(&self, gcx: Gcx<'_>) -> Target {
        self.runtime_target(gcx).priced_for(self.straight_line_creation)
    }

    /// Records an edit that the transform does not report as a change, such as removing dead
    /// instructions or annotating metadata, so the cached local memory summary drops the
    /// function.
    pub(crate) fn note_unreported_edit(&self) {
        self.unreported_edit.set(true);
    }
}

/// The fewest instructions, summed over the functions a pass runs on, that repay handing them
/// to idle workers.
const MIN_PARALLEL_INSTRUCTIONS: usize = 1024;

#[derive(Clone, Copy)]
struct FunctionAnalysisRequirements(u8);

impl FunctionAnalysisRequirements {
    const NONE: Self = Self(0);
    const ALIAS: Self = Self(1 << 0);
    const CFG: Self = Self(1 << 1);
    const ALL: Self = Self(Self::ALIAS.0 | Self::CFG.0);

    const fn alias(self) -> bool {
        self.0 & Self::ALIAS.0 != 0
    }

    const fn cfg(self) -> bool {
        self.0 & Self::CFG.0 != 0
    }
}

/// Cached per-function analyses shared by every pass in one pipeline run.
#[doc(hidden)]
#[derive(Default)]
pub struct ModuleAnalyses {
    /// A diagnostic emitted by this pipeline, independent of other modules.
    pub(crate) error: Option<ErrorGuaranteed>,
    alias: FxHashMap<FunctionId, Rc<AliasAnalysis>>,
    cfg: FxHashMap<FunctionId, Rc<CfgInfo>>,
    /// Function bodies on which a function-local pass most recently reported
    /// no change. Any intervening mutation of that body removes the entry.
    local_no_change: FxHashMap<TypeId, DenseBitSet<FunctionId>>,
    call_summaries: Option<Arc<MemoryCallSummaries>>,
    /// Local summaries of the functions no pass has changed since the last summary build.
    local_summaries: LocalSummaryCache,
    preserved_by_pass: bool,
    call_summaries_preserved: bool,
    local_summaries_preserved: bool,
    /// Whether the running pass may run its functions on several threads.
    parallel: bool,
    /// Whether the running pass edited a function without reporting a change.
    pub(crate) unreported_edit: bool,
    /// Whether the running pass prices creation code differently, which only the gas objective
    /// does.
    prices_creation: bool,
    /// Whether the running pass runs on code optimized for no objective, gas, and size, in that
    /// order. Function-local runners skip the functions whose objective it excludes. Unset, every
    /// function runs.
    objective_gate: Option<[bool; 3]>,
    /// The creation-only functions that no loop or recursion calls, and the subset without loops
    /// of their own. A pass that adds or removes calls and functions without preserving the
    /// analyses drops them.
    creation_functions: Option<(DenseBitSet<FunctionId>, DenseBitSet<FunctionId>)>,
}

impl ModuleAnalyses {
    /// Stops this pipeline after a pass emits a diagnostic.
    pub fn fail(&mut self, error: ErrorGuaranteed) {
        self.error = Some(error);
    }

    pub(crate) fn begin_pass(
        &mut self,
        prices_creation: bool,
        parallel: bool,
        objective_gate: [bool; 3],
    ) {
        self.objective_gate = Some(objective_gate);
        self.parallel = parallel;
        self.unreported_edit = false;
        self.preserved_by_pass = false;
        self.call_summaries_preserved = false;
        self.local_summaries_preserved = false;
        self.prices_creation = prices_creation;
    }

    /// Returns whether the running pass runs on `func`, whose code is optimized for its module's
    /// or its own tag's objective.
    pub(crate) fn runs_on(&self, module: &Module, func: FunctionId) -> bool {
        let objective = module.objective(func);
        self.objective_gate.is_none_or(|gate| {
            gate[usize::from(objective.is_gas()) + 2 * usize::from(objective.is_size())]
        })
    }

    pub(crate) fn finish_pass(&mut self, changed: bool) {
        if !changed {
            return;
        }
        if !self.preserved_by_pass {
            self.invalidate_all();
            self.creation_functions = None;
            if !self.local_summaries_preserved {
                self.local_summaries.clear();
            }
        }
        if !self.call_summaries_preserved {
            self.call_summaries = None;
        }
    }

    /// Returns the shared alias-analysis snapshot for a function, resolving
    /// internal calls through the module call summaries when the snapshot is
    /// first built. A snapshot cached by an earlier pass keeps its own view.
    fn alias_with_summaries(
        &mut self,
        func_id: FunctionId,
        summaries: Arc<MemoryCallSummaries>,
    ) -> Rc<AliasAnalysis> {
        Rc::clone(
            self.alias
                .entry(func_id)
                .or_insert_with(|| Rc::new(AliasAnalysis::empty_with_summaries(summaries))),
        )
    }

    /// Returns the shared CFG snapshot for a function.
    pub(crate) fn cfg(&mut self, func_id: FunctionId, func: &Function) -> Rc<CfgInfo> {
        Rc::clone(self.cfg.entry(func_id).or_insert_with(|| Rc::new(CfgInfo::new(func))))
    }

    fn bundle(
        &mut self,
        func_id: FunctionId,
        module: &Module,
        requirements: FunctionAnalysisRequirements,
    ) -> FunctionAnalyses {
        let pricing = self.creation_pricing(func_id, module);
        let alias = requirements.alias().then(|| {
            if let Some(alias) = self.alias.get(&func_id) {
                return Rc::clone(alias);
            }
            let summaries = self.call_summaries(module);
            self.alias_with_summaries(func_id, summaries)
        });
        let cfg = requirements.cfg().then(|| self.cfg(func_id, &module.functions[func_id]));
        let cold = module.functions[func_id].attributes.hotness.is_cold();
        FunctionAnalyses::new(func_id, alias, cfg, pricing, module.objective(func_id), cold)
    }

    /// Returns whether a function is creation-only and whether, besides, it has no loop, when
    /// the running pass prices creation code differently.
    fn creation_pricing(&mut self, func_id: FunctionId, module: &Module) -> (bool, bool) {
        if !self.prices_creation {
            return (false, false);
        }
        // A pass that adds functions, such as `lower-builtins` with its storage helpers, can
        // preserve analyses, so rebuild the sets when they no longer cover every function.
        if self
            .creation_functions
            .as_ref()
            .is_some_and(|(creation, _)| creation.domain_size() != module.functions.len())
        {
            self.creation_functions = None;
        }
        let (creation, straight) =
            self.creation_functions.get_or_insert_with(|| module.creation_pricing_functions());
        (creation.contains(func_id), straight.contains(func_id))
    }

    /// Returns the module call summaries, computing them on first use. A pass that changes
    /// the module drops them unless it calls [`Self::preserve_call_summaries`].
    pub(crate) fn call_summaries(&mut self, module: &Module) -> Arc<MemoryCallSummaries> {
        Arc::clone(self.call_summaries.get_or_insert_with(|| {
            Arc::new(MemoryCallSummaries::new_cached(module, &mut self.local_summaries))
        }))
    }

    /// Drops every cached local memory summary after a module pass edited bodies without
    /// reporting a change.
    pub(crate) fn note_unreported_module_edit(&mut self) {
        self.local_summaries.clear();
    }

    /// Declares that the running pass leaves the module call summaries valid.
    pub(crate) fn preserve_call_summaries(&mut self) {
        self.call_summaries_preserved = true;
    }

    /// Computes call summaries of the module as it is, from the cached local summaries, without
    /// replacing the shared ones.
    pub(crate) fn fresh_call_summaries(&mut self, module: &Module) -> MemoryCallSummaries {
        MemoryCallSummaries::new_cached(module, &mut self.local_summaries)
    }

    /// Drops the cached local summary of a function whose body the running pass changed.
    pub(crate) fn invalidate_local_summary(&mut self, func_id: FunctionId) {
        self.local_summaries.invalidate(func_id);
    }

    /// Renumbers the cached local summaries after the running pass kept only the functions in
    /// `keep`.
    pub(crate) fn retain_local_summaries(&mut self, keep: &DenseBitSet<FunctionId>) {
        self.local_summaries.retain_functions(keep);
    }

    /// Declares that the running pass invalidated the local summary of every function it
    /// changed, so they survive the pass.
    pub(crate) fn preserve_local_summaries(&mut self) {
        self.local_summaries_preserved = true;
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

    /// Returns the bodies on which the pass `key` last reported no change, emptied when the
    /// module's function count changed.
    fn no_change(&mut self, key: TypeId, functions: usize) -> &mut DenseBitSet<FunctionId> {
        let cached =
            self.local_no_change.entry(key).or_insert_with(|| DenseBitSet::new_empty(functions));
        if cached.domain_size() != functions {
            cached.clear_to(functions);
        }
        cached
    }

    fn function_cached(&mut self, key: TypeId, func_id: FunctionId, functions: usize) -> bool {
        self.no_change(key, functions).contains(func_id)
    }

    fn record_function_result(
        &mut self,
        func_id: FunctionId,
        functions: usize,
        cache_key: Option<TypeId>,
        changed: bool,
    ) {
        if changed {
            self.local_summaries.invalidate(func_id);
            for cached in self.local_no_change.values_mut() {
                if func_id.index() < cached.domain_size() {
                    cached.remove(func_id);
                }
            }
        } else if let Some(key) = cache_key {
            self.no_change(key, functions).insert(func_id);
        }
    }

    fn invalidate_all(&mut self) {
        self.alias.clear();
        self.cfg.clear();
        self.local_no_change.clear();
    }
}

fn verified_preservation(func: &Function, cfg: &CfgInfo, insts_before: usize) -> (bool, bool) {
    let mut keep_cfg = cfg.num_blocks() == func.blocks.len();
    let mut only_removed_edges = keep_cfg;
    if keep_cfg {
        for (block_id, block) in func.blocks.iter_enumerated() {
            let before = cfg.successors(block_id);
            let mut after =
                block.terminator.as_ref().map(|term| term.successors()).unwrap_or_default();
            if before == after.as_slice() {
                continue;
            }
            only_removed_edges &= after.iter().all(|edge| before.contains(edge));
            if keep_cfg {
                let mut before = SmallVec::<[_; 2]>::from_slice(before);
                before.sort_unstable();
                after.sort_unstable();
                keep_cfg = before == after;
            }
        }
    }
    let no_new_side_effects = (insts_before..func.num_insts())
        .map(InstId::from_usize)
        .all(|inst_id| !func.inst(inst_id).kind.has_side_effects());
    let keep_alias = no_new_side_effects && only_removed_edges;
    (keep_alias, keep_cfg)
}

#[must_use]
fn run_function_pass_cached(
    analyses: &mut ModuleAnalyses,
    module: &mut Module,
    func_id: FunctionId,
    requirements: FunctionAnalysisRequirements,
    cache_key: Option<TypeId>,
    run: FunctionRun<'_>,
) -> bool {
    if let Some(key) = cache_key
        && analyses.function_cached(key, func_id, module.functions.len())
    {
        return false;
    }
    let bundle = analyses.bundle(func_id, module, requirements);
    let func = &mut module.functions[func_id];
    let insts_before = func.num_insts();
    let changed = run(func, &bundle);
    if bundle.unreported_edit.get() {
        analyses.local_summaries.invalidate(func_id);
        analyses.unreported_edit = true;
    }
    if changed {
        if let Some(cfg) = &bundle.cfg {
            let (keep_alias, keep_cfg) = verified_preservation(func, cfg, insts_before);
            analyses.retain(func_id, keep_alias, keep_cfg);
        } else {
            analyses.retain(func_id, false, false);
        }
    }
    analyses.record_function_result(func_id, module.functions.len(), cache_key, changed);
    changed
}

/// What a function task hands back to the thread that owns the caches.
struct FunctionOutcome {
    changed: bool,
    unreported_edit: bool,
    keep_alias: bool,
    cfg: Option<CfgInfo>,
    alias: Option<AliasAnalysis>,
}

/// Runs a function-local transform on its functions with the help of idle workers, when the
/// pipeline is parallel and the functions are large enough. Returns `None` to run serially.
///
/// The results equal the serial ones: each task owns its function's analysis snapshots, and
/// the caches change in function order after the tasks finish.
fn run_parallel_function_pass(
    module: &mut Module,
    analyses: &mut ModuleAnalyses,
    selected: Option<&DenseBitSet<FunctionId>>,
    requirements: FunctionAnalysisRequirements,
    cache_key: Option<TypeId>,
    run: FunctionRun<'_>,
) -> Option<bool> {
    if !analyses.parallel {
        return None;
    }
    let functions = module.functions.len();
    let mut runnable = DenseBitSet::new_empty(functions);
    let mut work = 0;
    for (id, func) in module.functions.iter_enumerated() {
        if !func.blocks.is_empty()
            && analyses.runs_on(module, id)
            && selected.is_none_or(|selected| selected.contains(id))
            && !cache_key.is_some_and(|key| analyses.function_cached(key, id, functions))
        {
            runnable.insert(id);
            work += func.num_insts();
        }
    }
    if work < MIN_PARALLEL_INSTRUCTIONS || runnable.count() < 2 {
        return None;
    }
    // A snapshot that a pass still shares cannot move to a task.
    if requirements.alias()
        && runnable
            .iter()
            .any(|id| analyses.alias.get(&id).is_some_and(|alias| Rc::strong_count(alias) != 1))
    {
        return None;
    }

    // Creation pricing is the module's at the pass's start, as in a serial pass.
    let pricing = module
        .functions
        .indices()
        .map(|id| analyses.creation_pricing(id, module))
        .collect::<IndexVec<FunctionId, _>>();
    // A serial pass builds the call summaries when it first meets a function without an alias
    // snapshot, from the module as the functions before it left it.
    let first_missing = requirements
        .alias()
        .then(|| runnable.iter().find(|id| !analyses.alias.contains_key(id)))
        .flatten();
    let mut changed = false;
    if analyses.call_summaries.is_none()
        && let Some(first_missing) = first_missing
    {
        for id in runnable.iter().take_while(|&id| id < first_missing).collect::<Vec<_>>() {
            changed |= run_function_pass_cached(analyses, module, id, requirements, cache_key, run);
            runnable.remove(id);
        }
    }
    let summaries = first_missing.map(|_| analyses.call_summaries(module));
    let module_objective = module.optimization;

    let tasks = module
        .functions
        .iter_mut_enumerated()
        .filter(|&(id, _)| runnable.contains(id))
        .map(|(id, func)| {
            let cfg = requirements.cfg().then(|| {
                analyses.cfg.remove(&id).map_or_else(|| CfgInfo::new(func), Rc::unwrap_or_clone)
            });
            let alias = requirements.alias().then(|| {
                analyses.alias.remove(&id).map_or_else(
                    || AliasAnalysis::empty_with_summaries(Arc::clone(summaries.as_ref().unwrap())),
                    |alias| Rc::into_inner(alias).expect("alias snapshot is unshared"),
                )
            });
            let objective = func.attributes.optimization.unwrap_or(module_objective);
            (id, func, alias, cfg, pricing[id], objective)
        })
        .collect::<Vec<_>>();
    let results =
        sync::par_try_map(true, tasks, |(id, func, alias, cfg, pricing, optimization)| {
            let cold = func.attributes.hotness.is_cold();
            let bundle = FunctionAnalyses::new(
                id,
                alias.map(Rc::new),
                cfg.map(Rc::new),
                pricing,
                optimization,
                cold,
            );
            let insts_before = func.num_insts();
            let changed = run(func, &bundle);
            let (keep_alias, keep_cfg) = match &bundle.cfg {
                _ if !changed => (true, true),
                Some(cfg) => verified_preservation(func, cfg, insts_before),
                None => (false, false),
            };
            let alias = bundle.alias.filter(|_| keep_alias).map(|alias| {
                if changed {
                    alias.clear_cached_addresses();
                }
                Rc::into_inner(alias).expect("pass released its alias snapshot")
            });
            Ok::<_, Infallible>((
                id,
                FunctionOutcome {
                    changed,
                    unreported_edit: bundle.unreported_edit.get(),
                    keep_alias,
                    cfg: bundle.cfg.filter(|_| keep_cfg).map(Rc::unwrap_or_clone),
                    alias,
                },
            ))
        })
        .unwrap_or_else(|never| match never {});
    for (id, outcome) in results {
        if outcome.unreported_edit {
            analyses.local_summaries.invalidate(id);
            analyses.unreported_edit = true;
        }
        if outcome.changed {
            analyses.retain(id, outcome.keep_alias, false);
        }
        if let Some(cfg) = outcome.cfg {
            analyses.cfg.insert(id, Rc::new(cfg));
        }
        if let Some(alias) = outcome.alias {
            analyses.alias.insert(id, Rc::new(alias));
        }
        analyses.record_function_result(id, functions, cache_key, outcome.changed);
        changed |= outcome.changed;
    }
    analyses.preserved_by_pass = true;
    Some(changed)
}

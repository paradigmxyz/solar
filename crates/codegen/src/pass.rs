//! Pass infrastructure for MIR transformations and analyses.
//!
//! Inspired by LLVM/MLIR pass infrastructure:
//! - **Analysis passes** (`AnalysisPass`) are read-only and produce a cached result. They take
//!   `&Function` and store their result in `AnalysisManager`.
//! - **Module passes** (`ModulePass`) modify the IR at module scope. Function-local passes can
//!   implement `FunctionPass` and are automatically applied to each function.
//!
//! # Usage
//!
//! ```ignore
//! // Read-only analysis pipeline (codegen):
//! let mut am = AnalysisManager::new();
//! let liveness = am.get_or_compute(&LivenessAnalysis, &func);
//!
//! let changed = run_pass(gcx, &mut module, &DCE_PASS);
//! ```

use crate::{
    analysis::{AliasAnalysis, CfgInfo, MemoryCallSummaries},
    mir::{Function, FunctionId, MirPhase, Module, validate},
    timing::PassTimer,
    transform::{
        AdcePass, CfgSimplifyPass, CheckElimPass, CopyElisionPass, CsePass, DcePass,
        FrameSlotPromotionPass, FunctionDcePass, GvnPass, IndVarSimplifyPass, InlinePass,
        InstSimplifyPass, JumpThreadingPass, LicmPass, LoadPrePass, LoopCanonicalizePass,
        LowerAbiEncodePass, LowerAbiPass, LowerAggregatesPass, LowerAllocPass, LowerDispatchPass,
        LowerEvmShapedPass, LowerMappingSlotsPass, LowerMemoryObjectsPass, LowerSlicesPass,
        MemoryDsePass, OutlineRevertsPass, PrePass, PureEvalPass, SccpTransformPass, SroaPass,
        StaticAllocPass, StorageDsePass, StorageLoadCsePass, StorageScalarPromotionPass,
    },
};
use solar_data_structures::map::FxHashMap;
use solar_interface::diagnostics::DiagCtxt;
use solar_sema::Gcx;
use std::{
    any::{Any, TypeId},
    rc::Rc,
    sync::Arc,
};

type PassRunner = fn(Gcx<'_>, &mut Module, &mut ModuleAnalyses) -> bool;

/// Constructs a function-local pass object; only present for passes declared
/// `function` in `declare_passes!`, where the pipeline executor may schedule
/// them function-at-a-time instead of module-at-a-time. The executor builds
/// each chunk's pass objects once per worker and reuses them across that
/// worker's functions, so scratch collections held in pass fields amortize
/// their allocations.
type MakeFunctionPass = fn() -> Box<dyn FunctionPass + Send>;

/// Registry entry for a MIR transform pass.
#[derive(Clone, Copy, Debug)]
pub struct PassInfo {
    /// Command-line and pipeline name.
    pub name: &'static str,
    /// Human-readable help text.
    pub description: &'static str,
    /// Earliest [`MirPhase`] this pass may run on.
    min_phase: MirPhase,
    /// Latest [`MirPhase`] this pass may run on.
    max_phase: MirPhase,
    run_pass: PassRunner,
    /// Pass-object constructor for function-local passes; `None` for passes
    /// that transform the module as a whole.
    make_function_pass: Option<MakeFunctionPass>,
    /// Whether the pass consumes module call summaries; the chunk executor
    /// computes them once per chunk for such passes.
    wants_call_summaries: bool,
}

impl PassInfo {
    const fn new(name: &'static str, description: &'static str, run_pass: PassRunner) -> Self {
        Self {
            name,
            description,
            min_phase: MirPhase::Built,
            max_phase: MirPhase::EvmShaped,
            run_pass,
            make_function_pass: None,
            wants_call_summaries: false,
        }
    }

    /// Attaches the pass-object constructor for passes declared `function`.
    const fn function_runner(mut self, make: Option<MakeFunctionPass>) -> Self {
        self.make_function_pass = make;
        self
    }

    /// Marks the pass as consuming module call summaries.
    const fn with_call_summaries(mut self) -> Self {
        self.wants_call_summaries = true;
        self
    }

    /// Restricts the phases this pass may run on: the pass manager skips it,
    /// rather than running it, on modules outside the range.
    const fn phases(mut self, min: MirPhase, max: MirPhase) -> Self {
        self.min_phase = min;
        self.max_phase = max;
        self
    }

    /// Whether this pass's declared phase range admits the module's phase.
    #[must_use]
    fn admits(&self, module: &Module) -> bool {
        self.min_phase <= module.phase && module.phase <= self.max_phase
    }
}

macro_rules! function_pass_runner {
    (module $pass:expr) => {
        None
    };
    (function $pass:expr) => {
        Some((|| Box::new($pass) as Box<dyn FunctionPass + Send>) as MakeFunctionPass)
    };
}

macro_rules! declare_passes {
    ($(
        $(#[doc = $description:literal])+
        $vis:vis const $const_name:ident -> $name:literal = $kind:ident $pass:expr;
    )+) => {
        $(
            $(#[doc = $description])+
            $vis const $const_name: PassInfo = PassInfo::new(
                $name,
                concat!($($description, "\n"),+).trim_ascii(),
                |gcx, module, analyses| {
                    let pass = &mut $pass;
                    let changed = ModulePass::run(pass, gcx, module, analyses);
                    if changed && !ModulePass::maintains_analyses(pass) {
                        analyses.invalidate_all();
                    }
                    changed
                },
            ).function_runner(function_pass_runner!($kind $pass));
        )+
    };
}

declare_passes! {
    /// Internal MIR function inlining.
    pub(crate) const INLINE_PASS -> "inline" = module InlinePass;

    /// Outline duplicate constant revert blocks before backend lowering.
    pub(crate) const OUTLINE_REVERTS_PASS -> "outline-reverts" = module OutlineRevertsPass::default();

    /// Dead internal function elimination.
    pub(crate) const FUNCTION_DCE_PASS -> "function-dce" = module FunctionDcePass;

    /// Sparse Conditional Constant Propagation.
    pub(crate) const SCCP_PASS -> "sccp" = function SccpTransformPass;

    /// Bounded evaluator for closed pure MIR loops/functions.
    pub(crate) const PURE_EVAL_PASS -> "pure-eval" = function PureEvalPass;

    /// Local MIR instruction simplification.
    pub(crate) const INST_SIMPLIFY_PASS -> "inst-simplify" = function InstSimplifyPass;

    /// Common Subexpression Elimination (fixed-point).
    const CSE_PASS_BASE -> "cse" = function CsePass;

    /// Partial redundancy elimination for pure expressions.
    pub(crate) const PRE_PASS -> "pre" = function PrePass;

    /// Congruence-class global value numbering.
    pub(crate) const GVN_PASS -> "gvn" = function GvnPass;

    /// Reuse storage loads across definitely-disjoint stores.
    pub(crate) const STORAGE_LOAD_CSE_PASS -> "storage-load-cse" = function StorageLoadCsePass;

    /// Eliminate overwritten or repeated storage stores.
    pub(crate) const STORAGE_DSE_PASS -> "storage-dse" = function StorageDsePass;

    /// Availability-dataflow redundancy elimination and PRE for memory-dependent reads.
    pub(crate) const LOAD_PRE_PASS -> "load-pre" = function LoadPrePass;

    /// Canonicalize natural loops with explicit preheaders.
    pub(crate) const LOOP_CANONICALIZE_PASS -> "loop-canonicalize" = function LoopCanonicalizePass;

    /// Strength-reduce affine induction-variable address expressions.
    pub(crate) const INDVAR_SIMPLIFY_PASS -> "indvar-simplify" = function IndVarSimplifyPass;

    /// Promote simple loop-carried storage updates to memory.
    pub(crate) const STORAGE_PROMOTION_PASS -> "storage-promotion" = function StorageScalarPromotionPass;

    /// Loop-Invariant Code Motion.
    pub(crate) const LICM_PASS -> "licm" = function LicmPass;

    /// Range-based elimination of provably dead overflow-check branches.
    pub(crate) const CHECK_ELIM_PASS -> "check-elim" = function CheckElimPass;

    /// Jump Threading (fixed-point).
    pub(crate) const JUMP_THREADING_PASS -> "jump-threading" = function JumpThreadingPass;

    /// CFG Simplification (fixed-point).
    pub(crate) const CFG_SIMPLIFY_PASS -> "cfg-simplify" = function CfgSimplifyPass;

    /// Promote non-escaping compiler-local slots to SSA values.
    pub(crate) const FRAME_SLOT_PROMOTION_PASS -> "frame-slot-promotion" = function FrameSlotPromotionPass;

    /// Local dead memory-store elimination.
    pub(crate) const MEMORY_DSE_PASS -> "memory-dse" = function MemoryDsePass;

    /// Place provably local allocations at static frame addresses.
    pub(crate) const STATIC_ALLOC_PASS -> "static-alloc" = module StaticAllocPass;

    /// Scalar-replace non-escaping memory-object allocations.
    pub const SROA_PASS -> "sroa" = function SroaPass::default();

    /// Elide copies into write-only memory allocations.
    pub const COPY_ELISION_PASS -> "copy-elision" = function CopyElisionPass::default();

    /// Dead Code Elimination (fixed-point).
    pub(crate) const DCE_PASS -> "dce" = function DcePass::default();

    /// Aggressive dead-code elimination for dead control regions.
    pub(crate) const ADCE_PASS -> "adce" = function AdcePass;

    /// ABI phase lowering: external functions become self-decoding wrappers.
    const LOWER_ABI_PASS_BASE -> "lower-abi" = module LowerAbiPass::default();

    /// Dispatch phase lowering: synthesize the selector-switch `entry` function.
    const LOWER_DISPATCH_PASS_BASE -> "lower-dispatch" = module LowerDispatchPass::default();

    /// EVM-shape lowering: non-returning internal calls become tail calls.
    const LOWER_EVM_SHAPED_PASS_BASE -> "lower-evm-shaped" = module LowerEvmShapedPass::default();

    /// Lower mapping-slot hash builtins to memory operations.
    pub(crate) const LOWER_MAPPING_SLOTS_PASS -> "lower-mapping-slots" = function LowerMappingSlotsPass;

    /// Lower semantic ABI encoding to memory and slice operations.
    pub const LOWER_ABI_ENCODE_PASS -> "lower-abi-encode" = module LowerAbiEncodePass;

    /// Lower semantic memory/storage aggregate operations to word operations.
    pub const LOWER_AGGREGATES_PASS -> "lower-aggregates" = module LowerAggregatesPass;

    /// Lower semantic memory-object layouts and accesses to physical words.
    const LOWER_MEMORY_OBJECTS_PASS_BASE -> "lower-memory-objects" = module LowerMemoryObjectsPass::default();

    /// Lower logical slices to their pointer and length words.
    pub(crate) const LOWER_SLICES_PASS -> "lower-slices" = module LowerSlicesPass::default();

    /// Lower abstract allocation operations to free-memory-pointer updates.
    pub(crate) const LOWER_ALLOC_PASS -> "lower-alloc" = module LowerAllocPass;
}

/// Common subexpression elimination with its call-summary demand declared:
/// the chunk executor computes module call summaries once per chunk for it.
pub(crate) const CSE_PASS: PassInfo = CSE_PASS_BASE.with_call_summaries();

/// ABI phase lowering with its phase range declared: consumes
/// `built`/`optimized` MIR and produces the `abi` phase.
pub(crate) const LOWER_ABI_PASS: PassInfo =
    LOWER_ABI_PASS_BASE.phases(MirPhase::Built, MirPhase::Optimized);

/// Dispatch phase lowering with its phase range declared: consumes exactly
/// `abi`-phase MIR and produces the `dispatch` phase.
pub(crate) const LOWER_DISPATCH_PASS: PassInfo =
    LOWER_DISPATCH_PASS_BASE.phases(MirPhase::Abi, MirPhase::Abi);

/// Memory-object phase lowering: consumes up to `dispatch`-phase MIR and, for
/// a dispatched module, produces the `memory-lowered` phase.
pub const LOWER_MEMORY_OBJECTS_PASS: PassInfo =
    LOWER_MEMORY_OBJECTS_PASS_BASE.phases(MirPhase::Built, MirPhase::Dispatch);

/// EVM-shape lowering with its phase range declared: consumes exactly
/// `memory-lowered` MIR and produces the `evm-shaped` phase.
pub(crate) const LOWER_EVM_SHAPED_PASS: PassInfo =
    LOWER_EVM_SHAPED_PASS_BASE.phases(MirPhase::MemoryLowered, MirPhase::MemoryLowered);

/// All known MIR passes exposed to `solar mir-opt`.
pub const PASS_REGISTRY: &[PassInfo] = &[
    INLINE_PASS,
    FUNCTION_DCE_PASS,
    ADCE_PASS,
    DCE_PASS,
    INST_SIMPLIFY_PASS,
    CSE_PASS,
    GVN_PASS,
    PRE_PASS,
    STORAGE_LOAD_CSE_PASS,
    STORAGE_DSE_PASS,
    LOAD_PRE_PASS,
    LOOP_CANONICALIZE_PASS,
    INDVAR_SIMPLIFY_PASS,
    SCCP_PASS,
    PURE_EVAL_PASS,
    LICM_PASS,
    CHECK_ELIM_PASS,
    CFG_SIMPLIFY_PASS,
    JUMP_THREADING_PASS,
    FRAME_SLOT_PROMOTION_PASS,
    MEMORY_DSE_PASS,
    STATIC_ALLOC_PASS,
    SROA_PASS,
    COPY_ELISION_PASS,
    STORAGE_PROMOTION_PASS,
    LOWER_ABI_PASS,
    LOWER_DISPATCH_PASS,
    LOWER_EVM_SHAPED_PASS,
    OUTLINE_REVERTS_PASS,
    LOWER_MAPPING_SLOTS_PASS,
    LOWER_ABI_ENCODE_PASS,
    LOWER_AGGREGATES_PASS,
    LOWER_MEMORY_OBJECTS_PASS,
    LOWER_SLICES_PASS,
    LOWER_ALLOC_PASS,
];

/// Finds a pass in the global MIR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static PassInfo> {
    PASS_REGISTRY.iter().find(|pass| pass.name == name)
}

/// The canonical MIR optimization pipeline used by EVM codegen.
pub const DEFAULT_PIPELINE: &[PassInfo] = &[
    INLINE_PASS,
    FUNCTION_DCE_PASS,
    SCCP_PASS,
    PURE_EVAL_PASS,
    INST_SIMPLIFY_PASS,
    CSE_PASS,
    // Reuse mapping slots before their scratch-memory expansion can obscure
    // the semantic expression from the remaining optimization passes.
    LOWER_MAPPING_SLOTS_PASS,
    GVN_PASS,
    PRE_PASS,
    STORAGE_LOAD_CSE_PASS,
    STORAGE_DSE_PASS,
    LOAD_PRE_PASS,
    FRAME_SLOT_PROMOTION_PASS,
    LOOP_CANONICALIZE_PASS,
    INDVAR_SIMPLIFY_PASS,
    STORAGE_PROMOTION_PASS,
    LICM_PASS,
    CHECK_ELIM_PASS,
    JUMP_THREADING_PASS,
    CFG_SIMPLIFY_PASS,
    SROA_PASS,
    COPY_ELISION_PASS,
    MEMORY_DSE_PASS,
    // Keep allocation semantic through the optimization pipeline. The EVM
    // backend chooses static placement only after exact spill and helper-frame
    // addresses are known, then lowers the residual dynamic allocations.
    ADCE_PASS,
    DCE_PASS,
];

/// Cleanup passes rerun after the primary pipeline until no pass changes MIR.
///
/// Keep this group focused on simplification and canonicalization. Structural
/// profitability passes such as inlining and storage promotion run once in
/// [`DEFAULT_PIPELINE`], while this loop cleans up opportunities exposed by
/// those transforms.
pub const DEFAULT_CLEANUP_PIPELINE: &[PassInfo] = &[
    SCCP_PASS,
    PURE_EVAL_PASS,
    INST_SIMPLIFY_PASS,
    CSE_PASS,
    GVN_PASS,
    PRE_PASS,
    STORAGE_LOAD_CSE_PASS,
    STORAGE_DSE_PASS,
    LOAD_PRE_PASS,
    CHECK_ELIM_PASS,
    JUMP_THREADING_PASS,
    CFG_SIMPLIFY_PASS,
    FRAME_SLOT_PROMOTION_PASS,
    SROA_PASS,
    COPY_ELISION_PASS,
    MEMORY_DSE_PASS,
    ADCE_PASS,
    DCE_PASS,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Runs a named MIR pass over a module.
#[tracing::instrument(
    name = "mir_pass",
    level = "debug",
    skip_all,
    fields(module = %module.name, pass = pass.name),
)]
pub fn run_pass(gcx: solar_sema::Gcx<'_>, module: &mut Module, pass: &PassInfo) -> bool {
    run_pass_with(gcx, module, pass, &mut ModuleAnalyses::default())
}

/// Runs a named MIR pass over a module, sharing cached per-function analyses
/// with the other passes of one pipeline execution.
fn run_pass_with(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    pass: &PassInfo,
    analyses: &mut ModuleAnalyses,
) -> bool {
    // Passes declare which phases they operate on; the manager enforces it so a
    // pipeline entry cannot silently corrupt a module in the wrong phase.
    if !pass.admits(module) {
        return false;
    }
    if cfg!(debug_assertions) {
        validate_module_after_pass(module, "input");
    }
    let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
    let changed = (pass.run_pass)(gcx, module, analyses);
    timer.finish("MIR", module.name, pass.name, changed);
    if cfg!(debug_assertions) {
        validate_module_after_pass(module, pass.name);
    }
    changed
}

/// Whether per-pass observability flags force module-at-a-time execution,
/// where every pass application is individually timed and printable.
fn wants_pass_major(gcx: solar_sema::Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.time_passes || gcx.sess.opts.unstable.print_after_each
}

/// Runs a pass pipeline with a shared per-function analysis cache.
///
/// Maximal runs of consecutive function-local passes execute as one chunk,
/// function-at-a-time: every chunk pass runs on a function before the next
/// function is touched, so its analysis snapshots and cache lines stay hot
/// across the whole chunk. Module passes and per-pass observability flags
/// fall back to module-at-a-time execution.
fn run_pipeline_with(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    passes: &[PassInfo],
    analyses: &mut ModuleAnalyses,
) -> bool {
    let pass_major = wants_pass_major(gcx);
    let mut changed = false;
    let mut index = 0;
    while index < passes.len() {
        let chunk_len = if pass_major {
            0
        } else {
            passes[index..]
                .iter()
                .take_while(|pass| pass.make_function_pass.is_some() && pass.admits(module))
                .count()
        };
        if chunk_len > 1 {
            let chunk = &passes[index..index + chunk_len];
            changed |= run_local_chunk(gcx, module, chunk, analyses, 1);
            index += chunk_len;
            continue;
        }
        let pass = &passes[index];
        changed |= run_pass_with(gcx, module, pass, analyses);
        if gcx.sess.opts.unstable.print_after_each {
            println!("// === {} (after {}) ===", module.name, pass.name);
            print!("{}", module.to_text());
        }
        index += 1;
    }
    changed
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
///
/// This is a phase transition: the module comes out in `MirPhase::Optimized`.
/// Ad-hoc pass lists run through `run_pipeline`, such as `solar mir-opt`
/// invocations, deliberately do not advance the phase.
#[tracing::instrument(
    name = "mir_pipeline",
    level = "debug",
    skip_all,
    fields(module = %module.name),
)]
pub fn run_default_pipeline(gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
    // One analysis cache for the whole pipeline: lazily built per-function
    // snapshots survive every pass that does not change their function.
    let mut analyses = ModuleAnalyses::default();
    let mut changed = run_pipeline_with(gcx, module, DEFAULT_PIPELINE, &mut analyses);
    changed |= run_cleanup_pipeline_to_fixpoint(
        gcx,
        module,
        DEFAULT_CLEANUP_PIPELINE,
        "cleanup",
        &mut analyses,
    );
    module.advance_phase(crate::mir::MirPhase::Optimized);
    changed
}

fn run_cleanup_pipeline_to_fixpoint(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    passes: &[PassInfo],
    label: &str,
    analyses: &mut ModuleAnalyses,
) -> bool {
    // An all-function-local cleanup pipeline fixpoints one function at a time:
    // converged functions stop iterating while others continue, and each
    // function's analysis snapshots stay hot across all of its rounds. This is
    // equivalent to the module-wide round loop below because function-local
    // passes never read other functions.
    if !wants_pass_major(gcx)
        && passes.iter().all(|pass| pass.make_function_pass.is_some() && pass.admits(module))
    {
        return run_local_chunk(gcx, module, passes, analyses, DEFAULT_CLEANUP_MAX_ROUNDS);
    }
    // A pass only needs to rerun if the module changed since it last started:
    // rerunning a deterministic pass on identical input is a no-op. Tracking
    // that with a change generation keeps exactly the plain round loop's
    // optimization power — a changing pass is stamped with its *pre-run*
    // generation, so it always earns one confirming rerun, even when it is
    // not internally fixpointed — while the pure confirmation reruns of
    // unchanged passes, which otherwise dominate, are skipped.
    let mut generation = 0usize;
    let mut ran_at = vec![usize::MAX; passes.len()];
    let mut changed = false;
    for round in 1..=DEFAULT_CLEANUP_MAX_ROUNDS {
        let mut round_changed = false;
        for (index, pass) in passes.iter().enumerate() {
            if ran_at[index] == generation {
                continue;
            }
            let generation_before = generation;
            let pass_changed = run_pass_with(gcx, module, pass, analyses);
            if pass_changed {
                generation += 1;
                round_changed = true;
            }
            ran_at[index] = generation_before;
            if gcx.sess.opts.unstable.print_after_each {
                println!("// === {} (after {label}-{round}:{}) ===", module.name, pass.name);
                print!("{}", module.to_text());
            }
        }
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

/// Runs a chunk of function-local passes function-at-a-time.
///
/// With `max_rounds == 1` this applies each pass once, in pipeline order, to
/// one function before moving to the next; larger bounds run the
/// generation-tracked fixpoint loop per function. Each function runs against
/// a fresh [`FunctionCache`], sequentially or in parallel: functions are
/// independent under function-local passes, so parallel execution is
/// deterministic and byte-identical to the sequential order.
///
/// Module call summaries are computed once per chunk when a chunk pass
/// consumes them; within the chunk they can only go stale conservatively
/// (chunk passes remove or localize effects, they never invent writes to
/// fresh locations).
#[tracing::instrument(
    name = "mir_chunk",
    level = "debug",
    skip_all,
    fields(module = %module.name, passes = chunk.len()),
)]
fn run_local_chunk(
    gcx: solar_sema::Gcx<'_>,
    module: &mut Module,
    chunk: &[PassInfo],
    analyses: &mut ModuleAnalyses,
    max_rounds: usize,
) -> bool {
    if cfg!(debug_assertions) {
        validate_module_after_pass(module, "input");
    }
    let call_summaries = chunk
        .iter()
        .any(|pass| pass.wants_call_summaries)
        .then(|| Arc::new(MemoryCallSummaries::new(module)));
    let eligible = module.functions.iter().filter(|func| !func.blocks.is_empty()).count();
    let mut changed_ids = Vec::new();
    if gcx.sess.is_parallel() && eligible > 1 {
        use rayon::prelude::*;
        changed_ids = module
            .functions
            .raw
            .par_iter_mut()
            .enumerate()
            .filter(|(_, func)| !func.blocks.is_empty())
            .map_init(
                || ChunkWorker::new(chunk),
                |worker, (index, func)| {
                    run_chunk_on_function(func, worker, &call_summaries, max_rounds)
                        .then(|| FunctionId::from_usize(index))
                },
            )
            .filter_map(|id| id)
            .collect();
    } else {
        let mut worker = ChunkWorker::new(chunk);
        for func_id in module.functions.indices() {
            let func = &mut module.functions[func_id];
            if func.blocks.is_empty() {
                continue;
            }
            if run_chunk_on_function(func, &mut worker, &call_summaries, max_rounds) {
                changed_ids.push(func_id);
            }
        }
    }
    // The chunk ran against local caches; drop whatever the shared cache
    // still holds for the functions it changed.
    for &func_id in &changed_ids {
        analyses.retain(func_id, false, false);
    }
    if cfg!(debug_assertions) {
        validate_module_after_pass(module, "local-chunk");
    }
    !changed_ids.is_empty()
}

/// Per-worker execution state for one chunk: the chunk's pass objects and its
/// generation table, built once per worker and reused across that worker's
/// functions so scratch collections held in pass fields amortize their
/// allocations across the chunk.
struct ChunkWorker {
    passes: Vec<Box<dyn FunctionPass + Send>>,
    ran_at: Vec<usize>,
}

impl ChunkWorker {
    fn new(chunk: &[PassInfo]) -> Self {
        let passes: Vec<_> = chunk
            .iter()
            .map(|pass| (pass.make_function_pass.expect("chunk passes are function-local"))())
            .collect();
        Self { ran_at: vec![usize::MAX; passes.len()], passes }
    }
}

/// Runs every pass of a chunk on one function, iterating to a bounded
/// fixpoint with the same generation tracking as the module-wide cleanup
/// loop: a pass reruns only if the function changed since it last started.
fn run_chunk_on_function(
    func: &mut Function,
    worker: &mut ChunkWorker,
    call_summaries: &Option<Arc<MemoryCallSummaries>>,
    max_rounds: usize,
) -> bool {
    let mut cache = FunctionCache::default();
    worker.ran_at.fill(usize::MAX);
    let mut generation = 0usize;
    let mut changed = false;
    for _round in 1..=max_rounds {
        let mut round_changed = false;
        for index in 0..worker.passes.len() {
            if worker.ran_at[index] == generation {
                continue;
            }
            let generation_before = generation;
            let pass = &mut worker.passes[index];
            if run_function_pass_local(&mut cache, call_summaries, func, |func, bundle| {
                pass.run_on_function_cached(func, bundle)
            }) {
                generation += 1;
                round_changed = true;
            }
            worker.ran_at[index] = generation_before;
        }
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

/// A transformation pass that mutates a MIR module.
///
/// Module-level passes can inspect or transform more than one function. Function-local passes
/// should implement [`FunctionPass`] instead and use the blanket [`ModulePass`] implementation.
pub(crate) trait ModulePass {
    /// Runs the transformation on the given module.
    ///
    /// Returns true if the transform changed MIR.
    fn run(&mut self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool;

    /// Whether `run` keeps [`ModuleAnalyses`] consistent itself. The pass
    /// runner invalidates the whole cache after a changing run otherwise.
    fn maintains_analyses(&self) -> bool {
        false
    }
}

/// A transformation pass that mutates one function at a time.
pub(crate) trait FunctionPass {
    /// Runs the transformation on the given function.
    fn run_on_function(&mut self, func: &mut Function) -> bool;

    /// Runs the transformation on one function with its cached analyses.
    ///
    /// The default ignores the cache; passes that consult alias analysis or
    /// the CFG snapshot override this and read the shared instances instead of
    /// building their own, so lazily built provenance, address memos, and
    /// dominator trees amortize across every pass between two mutations of the
    /// function.
    fn run_on_function_cached(&mut self, func: &mut Function, analyses: &FunctionAnalyses) -> bool {
        let _ = analyses;
        self.run_on_function(func)
    }

    /// Runs the function pass over a module.
    ///
    /// Passes that need module-level summaries may override this hook; an
    /// override is responsible for invalidating the analyses of each function
    /// it changes. The default runs function-locally and invalidates the
    /// changed functions, retaining whichever snapshots the change verifiably
    /// preserved.
    fn run_on_module(&mut self, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass_over_module(analyses, module, self)
    }
}

/// Runs a function pass over every non-empty function of a module through the
/// verified-preservation cache. Module-level `run_on_module` overrides reuse
/// this loop around their extra setup.
pub(crate) fn run_function_pass_over_module<P: FunctionPass + ?Sized>(
    analyses: &mut ModuleAnalyses,
    module: &mut Module,
    pass: &mut P,
) -> bool {
    let mut changed = false;
    for func_id in module.functions.indices() {
        if module.functions[func_id].blocks.is_empty() {
            continue;
        }
        changed |= run_function_pass_cached(analyses, module, func_id, |func, bundle| {
            pass.run_on_function_cached(func, bundle)
        });
    }
    changed
}

impl<T: FunctionPass> ModulePass for T {
    fn run(&mut self, _gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        self.run_on_module(module, analyses)
    }

    fn maintains_analyses(&self) -> bool {
        true
    }
}

/// Per-function analysis snapshots handed to a pass run.
pub(crate) struct FunctionAnalyses {
    /// Shared alias analysis; provenance and address memos build lazily.
    pub(crate) alias: Rc<AliasAnalysis>,
    /// Shared CFG snapshot; RPO, dominators, and reachability build lazily.
    pub(crate) cfg: Rc<CfgInfo>,
    /// Module call summaries, present while a summary-consuming pass runs;
    /// provided by the executing chunk or the pass's own module override.
    pub(crate) call_summaries: Option<Arc<MemoryCallSummaries>>,
}

/// Cached per-function analyses shared by every pass in one pipeline run.
///
/// Entries are handed out as [`Rc`] snapshots and dropped when their function
/// changes; a pass holding the snapshot across its own mutations relies on the
/// same conservative-under-removal reasoning it would with a private copy.
#[derive(Default)]
pub(crate) struct ModuleAnalyses {
    alias: FxHashMap<FunctionId, Rc<AliasAnalysis>>,
    cfg: FxHashMap<FunctionId, Rc<CfgInfo>>,
    /// Module call summaries for summary-consuming passes, set for the
    /// duration of one chunk or module-override run.
    call_summaries: Option<Arc<MemoryCallSummaries>>,
}

impl ModuleAnalyses {
    /// Returns the shared alias-analysis snapshot for a function, creating an
    /// empty (lazily populated) one on first request.
    pub(crate) fn alias(&mut self, func_id: FunctionId) -> Rc<AliasAnalysis> {
        Rc::clone(self.alias.entry(func_id).or_insert_with(|| Rc::new(AliasAnalysis::empty())))
    }

    /// Returns the shared CFG snapshot for a function.
    pub(crate) fn cfg(&mut self, func_id: FunctionId, func: &Function) -> Rc<CfgInfo> {
        Rc::clone(self.cfg.entry(func_id).or_insert_with(|| Rc::new(CfgInfo::new(func))))
    }

    /// Returns both shared snapshots for one function.
    pub(crate) fn bundle(&mut self, func_id: FunctionId, func: &Function) -> FunctionAnalyses {
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

    /// Withdraws the module call summaries once their run completes.
    pub(crate) fn clear_call_summaries(&mut self) {
        self.call_summaries = None;
    }

    /// Drops the analyses a change did not verifiably preserve.
    fn retain(&mut self, func_id: FunctionId, keep_alias: bool, keep_cfg: bool) {
        if !keep_alias {
            self.alias.remove(&func_id);
        }
        if !keep_cfg {
            self.cfg.remove(&func_id);
        }
    }

    /// Drops every cached analysis after a module-level transformation.
    pub(crate) fn invalidate_all(&mut self) {
        self.alias.clear();
        self.cfg.clear();
        self.call_summaries = None;
    }
}

/// Snapshot of a function's CFG edge set for verified preservation checks.
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

/// Decides which analysis snapshots verifiably survived a changing pass run:
/// instead of trusting per-pass declarations, this compares the CFG edge set
/// before and after, and scans instructions the pass appended for side
/// effects.
///
/// - The CFG snapshot survives when the edge set is unchanged.
/// - The alias snapshot survives when no appended instruction has side effects (removals only leave
///   its facts conservative) and the edge set did not grow (removed edges only shrink reachability
///   and cycles, which also leaves the cached facts conservative).
///
/// Returns `(keep_alias, keep_cfg)`.
fn verified_preservation(
    func: &Function,
    edges_before: &[(u32, u32)],
    insts_before: usize,
) -> (bool, bool) {
    let edges_after = cfg_edges(func);
    let keep_cfg = edges_after == edges_before;
    let no_new_side_effects = (insts_before..func.instructions.len()).all(|index| {
        !func.instructions[crate::mir::InstId::from_usize(index)].kind.has_side_effects()
    });
    let keep_alias = no_new_side_effects
        && (keep_cfg || edges_after.iter().all(|edge| edges_before.binary_search(edge).is_ok()));
    (keep_alias, keep_cfg)
}

/// Per-function analysis cache for one chunk execution on one function; the
/// chunk executor creates it fresh per function, so parallel and sequential
/// execution share it by construction.
#[derive(Default)]
pub(crate) struct FunctionCache {
    alias: Option<Rc<AliasAnalysis>>,
    cfg: Option<Rc<CfgInfo>>,
}

impl FunctionCache {
    /// Returns the shared snapshots for the function, creating empty (lazily
    /// populated) ones on first request.
    fn bundle(
        &mut self,
        func: &Function,
        call_summaries: &Option<Arc<MemoryCallSummaries>>,
    ) -> FunctionAnalyses {
        FunctionAnalyses {
            alias: Rc::clone(self.alias.get_or_insert_with(|| Rc::new(AliasAnalysis::empty()))),
            cfg: Rc::clone(self.cfg.get_or_insert_with(|| Rc::new(CfgInfo::new(func)))),
            call_summaries: call_summaries.clone(),
        }
    }

    /// Drops the analyses a change did not verifiably preserve.
    fn retain(&mut self, keep_alias: bool, keep_cfg: bool) {
        if !keep_alias {
            self.alias = None;
        }
        if !keep_cfg {
            self.cfg = None;
        }
    }
}

/// Runs one function-local pass body against a per-function cache with
/// verified preservation.
fn run_function_pass_local(
    cache: &mut FunctionCache,
    call_summaries: &Option<Arc<MemoryCallSummaries>>,
    func: &mut Function,
    run: impl FnOnce(&mut Function, &FunctionAnalyses) -> bool,
) -> bool {
    let bundle = cache.bundle(func, call_summaries);
    let edges_before = cfg_edges(func);
    let insts_before = func.instructions.len();
    let changed = run(func, &bundle);
    if changed {
        let (keep_alias, keep_cfg) = verified_preservation(func, &edges_before, insts_before);
        cache.retain(keep_alias, keep_cfg);
    }
    changed
}

/// Runs one function-local pass body with the module-wide shared cache and
/// verified preservation; module-at-a-time pass runs go through this.
pub(crate) fn run_function_pass_cached(
    analyses: &mut ModuleAnalyses,
    module: &mut Module,
    func_id: FunctionId,
    run: impl FnOnce(&mut Function, &FunctionAnalyses) -> bool,
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

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let dcx = DiagCtxt::new_early();
    validate(&dcx, module);
    if dcx.has_errors().is_err() {
        panic!("MIR validation failed after `{pass_name}`");
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

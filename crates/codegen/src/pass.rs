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
//! let changed = run_pass(&mut module, &DCE_PASS, PipelineOptions::default());
//! ```

use crate::{
    mir::{Function, MirPhase, Module, validate},
    timing::PassTimer,
    transform::{
        AdcePass, CfgSimplifyPass, CheckElimPass, CsePass, DcePass, FrameSlotPromotionPass,
        FunctionDcePass, GvnPass, IndVarSimplifyPass, InlinePass, InstSimplifyPass,
        JumpThreadingPass, LicmPass, LoadPrePass, LoopCanonicalizePass, LowerAbiPass,
        LowerDispatchPass, LowerEvmShapedPass, LowerMappingSlotsPass, MemoryDsePass,
        OutlineRevertsPass, PrePass, PureEvalPass, SccpTransformPass, StaticAllocPass,
        StorageDsePass, StorageLoadCsePass, StorageScalarPromotionPass,
    },
};
use solar_data_structures::map::FxHashMap;
use solar_interface::diagnostics::DiagCtxt;
use std::any::{Any, TypeId};

type PassRunner = fn(&mut Module) -> bool;

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
}

impl PassInfo {
    const fn new(name: &'static str, description: &'static str, run_pass: PassRunner) -> Self {
        Self {
            name,
            description,
            min_phase: MirPhase::Built,
            max_phase: MirPhase::EvmShaped,
            run_pass,
        }
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

macro_rules! declare_passes {
    ($(
        $(#[doc = $description:literal])+
        $vis:vis const $const_name:ident -> $name:literal = $pass:expr;
    )+) => {
        $(
            $(#[doc = $description])+
            $vis const $const_name: PassInfo = PassInfo::new(
                $name,
                concat!($($description, "\n"),+).trim_ascii(),
                |module| ModulePass::run(&mut $pass, module),
            );
        )+
    };
}

declare_passes! {
    /// Internal MIR function inlining.
    pub(crate) const INLINE_PASS -> "inline" = InlinePass;

    /// Outline duplicate constant revert blocks before backend lowering.
    pub(crate) const OUTLINE_REVERTS_PASS -> "outline-reverts" = OutlineRevertsPass::default();

    /// Dead internal function elimination.
    pub(crate) const FUNCTION_DCE_PASS -> "function-dce" = FunctionDcePass;

    /// Sparse Conditional Constant Propagation.
    pub(crate) const SCCP_PASS -> "sccp" = SccpTransformPass;

    /// Bounded evaluator for closed pure MIR loops/functions.
    pub(crate) const PURE_EVAL_PASS -> "pure-eval" = PureEvalPass;

    /// Local MIR instruction simplification.
    pub(crate) const INST_SIMPLIFY_PASS -> "inst-simplify" = InstSimplifyPass;

    /// Common Subexpression Elimination (fixed-point).
    pub(crate) const CSE_PASS -> "cse" = CsePass;

    /// Partial redundancy elimination for pure expressions.
    pub(crate) const PRE_PASS -> "pre" = PrePass;

    /// Congruence-class global value numbering.
    pub(crate) const GVN_PASS -> "gvn" = GvnPass;

    /// Reuse storage loads across definitely-disjoint stores.
    pub(crate) const STORAGE_LOAD_CSE_PASS -> "storage-load-cse" = StorageLoadCsePass;

    /// Eliminate overwritten or repeated storage stores.
    pub(crate) const STORAGE_DSE_PASS -> "storage-dse" = StorageDsePass;

    /// Availability-dataflow redundancy elimination and PRE for memory-dependent reads.
    pub(crate) const LOAD_PRE_PASS -> "load-pre" = LoadPrePass;

    /// Canonicalize natural loops with explicit preheaders.
    pub(crate) const LOOP_CANONICALIZE_PASS -> "loop-canonicalize" = LoopCanonicalizePass;

    /// Strength-reduce affine induction-variable address expressions.
    pub(crate) const INDVAR_SIMPLIFY_PASS -> "indvar-simplify" = IndVarSimplifyPass;

    /// Promote simple loop-carried storage updates to memory.
    pub(crate) const STORAGE_PROMOTION_PASS -> "storage-promotion" = StorageScalarPromotionPass;

    /// Loop-Invariant Code Motion.
    pub(crate) const LICM_PASS -> "licm" = LicmPass;

    /// Range-based elimination of provably dead overflow-check branches.
    pub(crate) const CHECK_ELIM_PASS -> "check-elim" = CheckElimPass;

    /// Jump Threading (fixed-point).
    pub(crate) const JUMP_THREADING_PASS -> "jump-threading" = JumpThreadingPass;

    /// CFG Simplification (fixed-point).
    pub(crate) const CFG_SIMPLIFY_PASS -> "cfg-simplify" = CfgSimplifyPass;

    /// Promote non-escaping compiler-local slots to SSA values.
    pub(crate) const FRAME_SLOT_PROMOTION_PASS -> "frame-slot-promotion" = FrameSlotPromotionPass;

    /// Local dead memory-store elimination.
    pub(crate) const MEMORY_DSE_PASS -> "memory-dse" = MemoryDsePass;

    /// Place provably local fmp-bump allocations at static frame addresses.
    pub(crate) const STATIC_ALLOC_PASS -> "static-alloc" = StaticAllocPass;

    /// Dead Code Elimination (fixed-point).
    pub(crate) const DCE_PASS -> "dce" = DcePass;

    /// Aggressive dead-code elimination for dead control regions.
    pub(crate) const ADCE_PASS -> "adce" = AdcePass;

    /// ABI phase lowering: external functions become self-decoding wrappers.
    const LOWER_ABI_PASS_BASE -> "lower-abi" = LowerAbiPass::default();

    /// Dispatch phase lowering: synthesize the selector-switch `entry` function.
    const LOWER_DISPATCH_PASS_BASE -> "lower-dispatch" = LowerDispatchPass::default();

    /// EVM-shape lowering: non-returning internal calls become tail calls.
    const LOWER_EVM_SHAPED_PASS_BASE -> "lower-evm-shaped" = LowerEvmShapedPass::default();

    /// Lower mapping-slot hash builtins to memory operations.
    pub(crate) const LOWER_MAPPING_SLOTS_PASS -> "lower-mapping-slots" = LowerMappingSlotsPass;
}

/// ABI phase lowering with its phase range declared: consumes
/// `built`/`optimized` MIR and produces the `abi` phase.
pub(crate) const LOWER_ABI_PASS: PassInfo =
    LOWER_ABI_PASS_BASE.phases(MirPhase::Built, MirPhase::Optimized);

/// Dispatch phase lowering with its phase range declared: consumes exactly
/// `abi`-phase MIR and produces the `dispatch` phase.
pub(crate) const LOWER_DISPATCH_PASS: PassInfo =
    LOWER_DISPATCH_PASS_BASE.phases(MirPhase::Abi, MirPhase::Abi);

/// EVM-shape lowering with its phase range declared: consumes exactly
/// `dispatch`-phase MIR and produces the `evm-shaped` phase.
pub(crate) const LOWER_EVM_SHAPED_PASS: PassInfo =
    LOWER_EVM_SHAPED_PASS_BASE.phases(MirPhase::Dispatch, MirPhase::Dispatch);

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
    STORAGE_PROMOTION_PASS,
    LOWER_ABI_PASS,
    LOWER_DISPATCH_PASS,
    LOWER_EVM_SHAPED_PASS,
    OUTLINE_REVERTS_PASS,
    LOWER_MAPPING_SLOTS_PASS,
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
    MEMORY_DSE_PASS,
    STATIC_ALLOC_PASS,
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
    MEMORY_DSE_PASS,
    ADCE_PASS,
    DCE_PASS,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Options for running a MIR pass pipeline.
#[derive(Clone, Copy, Debug)]
pub struct PipelineOptions {
    /// Print the full module after every pass in the pipeline.
    pub print_after_each: bool,
    /// Print the time spent in each pass.
    pub time_passes: bool,
    /// Validate MIR after every pass.
    pub(crate) validate_after_each: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self {
            print_after_each: false,
            time_passes: false,
            validate_after_each: cfg!(debug_assertions),
        }
    }
}

/// Runs a named MIR pass over a module.
pub fn run_pass(module: &mut Module, pass: &PassInfo, options: PipelineOptions) -> bool {
    // Passes declare which phases they operate on; the manager enforces it so a
    // pipeline entry cannot silently corrupt a module in the wrong phase.
    if !pass.admits(module) {
        return false;
    }
    if options.validate_after_each {
        validate_module_after_pass(module, "input");
    }
    let timer = PassTimer::new(options.time_passes);
    let changed = (pass.run_pass)(module);
    timer.finish("MIR", module.name, pass.name, changed);
    if options.validate_after_each {
        validate_module_after_pass(module, pass.name);
    }
    changed
}

/// Runs a named MIR pass pipeline over a module.
fn run_pipeline(module: &mut Module, passes: &[PassInfo], options: PipelineOptions) -> bool {
    let mut changed = false;
    for pass in passes {
        changed |= run_pass(module, pass, options);
        if options.print_after_each {
            println!("// === {} (after {}) ===", module.name, pass.name);
            print!("{}", module.to_text());
        }
    }
    changed
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
///
/// This is a phase transition: the module comes out in `MirPhase::Optimized`.
/// Ad-hoc pass lists run through `run_pipeline`, such as `solar mir-opt`
/// invocations, deliberately do not advance the phase.
pub fn run_default_pipeline(module: &mut Module, options: PipelineOptions) -> bool {
    let mut changed = run_pipeline(module, DEFAULT_PIPELINE, options);
    changed |=
        run_cleanup_pipeline_to_fixpoint(module, DEFAULT_CLEANUP_PIPELINE, options, "cleanup");
    module.advance_phase(crate::mir::MirPhase::Optimized);
    changed
}

fn run_cleanup_pipeline_to_fixpoint(
    module: &mut Module,
    passes: &[PassInfo],
    options: PipelineOptions,
    label: &str,
) -> bool {
    let mut changed = false;
    for round in 1..=DEFAULT_CLEANUP_MAX_ROUNDS {
        let mut round_changed = false;
        for pass in passes {
            let pass_changed = run_pass(module, pass, options);
            round_changed |= pass_changed;
            if options.print_after_each {
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
    fn run(&mut self, module: &mut Module) -> bool;
}

/// A transformation pass that mutates one function at a time.
pub(crate) trait FunctionPass {
    /// Runs the transformation on the given function.
    fn run_on_function(&mut self, func: &mut Function) -> bool;
}

impl<T: FunctionPass> ModulePass for T {
    fn run(&mut self, module: &mut Module) -> bool {
        let mut changed = false;
        for func in module.functions.iter_mut().filter(|func| !func.blocks.is_empty()) {
            changed |= self.run_on_function(func);
        }
        changed
    }
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

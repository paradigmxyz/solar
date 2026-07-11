//! Pass infrastructure for MIR transformations and analyses.
//!
//! Inspired by LLVM/MLIR pass infrastructure:
//! - **Analysis passes** ([`AnalysisPass`]) are read-only and produce a cached result. They take
//!   `&Function` and store their result in [`AnalysisManager`].
//! - **Module passes** ([`ModulePass`]) modify the IR at module scope. Function-local passes can
//!   implement [`FunctionPass`] and are automatically applied to each function.
//!
//! # Usage
//!
//! ```ignore
//! // Read-only analysis pipeline (codegen):
//! let mut am = AnalysisManager::new();
//! let liveness = am.get_or_compute(&LivenessAnalysis, &func);
//!
//! // Transform pipeline:
//! let mut pm = PassManager::new();
//! pm.add_pass(Box::new(DcePass));
//! let changed = pm.run(&mut module).1;
//! ```

use crate::{
    analysis::Validator,
    mir::{Function, MirPhase, Module},
    transform::{
        AdcePass, CfgSimplifyPass, CheckElimPass, CsePass, DcePass, FrameSlotPromotionPass,
        FunctionDcePass, GvnPass, IndVarSimplifyPass, InlinePass, InstSimplifyPass,
        JumpThreadingPass, LicmPass, LoadPrePass, LoopCanonicalizePass, LowerAbiPass,
        LowerDispatchPass, LowerEvmShapedPass, MemoryDsePass, OutlineRevertsPass, PrePass,
        PureEvalPass, SccpTransformPass, StaticAllocPass, StorageDsePass, StorageLoadCsePass,
        StorageScalarPromotionPass,
    },
};
use solar_data_structures::map::FxHashMap;
use std::any::{Any, TypeId};

type PassFactory = fn() -> Box<dyn ModulePass>;

/// Registry entry for a MIR transform pass.
#[derive(Clone, Copy, Debug)]
pub struct PassInfo {
    /// Command-line and pipeline name.
    pub name: &'static str,
    /// Human-readable help text.
    pub description: &'static str,
    /// Earliest [`MirPhase`] this pass may run on.
    pub min_phase: MirPhase,
    /// Latest [`MirPhase`] this pass may run on.
    pub max_phase: MirPhase,
    make_pass: PassFactory,
}

impl PassInfo {
    const fn new(name: &'static str, description: &'static str, make_pass: PassFactory) -> Self {
        Self {
            name,
            description,
            min_phase: MirPhase::Built,
            max_phase: MirPhase::EvmShaped,
            make_pass,
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
    pub fn admits(&self, module: &Module) -> bool {
        self.min_phase <= module.phase && module.phase <= self.max_phase
    }

    fn make_pass(&self) -> Box<dyn ModulePass> {
        (self.make_pass)()
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
                || Box::new($pass),
            );
        )+
    };
}

declare_passes! {
    /// Internal MIR function inlining.
    pub const INLINE_PASS -> "inline" = InlinePass;

    /// Outline duplicate constant revert blocks into shared helpers.
    pub const OUTLINE_REVERTS_PASS -> "outline-reverts" = OutlineRevertsPass::default();

    /// Dead internal function elimination.
    pub const FUNCTION_DCE_PASS -> "function-dce" = FunctionDcePass;

    /// Sparse Conditional Constant Propagation.
    pub const SCCP_PASS -> "sccp" = SccpTransformPass;

    /// Bounded evaluator for closed pure MIR loops/functions.
    pub const PURE_EVAL_PASS -> "pure-eval" = PureEvalPass;

    /// Local MIR instruction simplification.
    pub const INST_SIMPLIFY_PASS -> "inst-simplify" = InstSimplifyPass;

    /// Common Subexpression Elimination (fixed-point).
    pub const CSE_PASS -> "cse" = CsePass;

    /// Partial redundancy elimination for pure expressions.
    pub const PRE_PASS -> "pre" = PrePass;

    /// Congruence-class global value numbering.
    pub const GVN_PASS -> "gvn" = GvnPass;

    /// Reuse storage loads across definitely-disjoint stores.
    pub const STORAGE_LOAD_CSE_PASS -> "storage-load-cse" = StorageLoadCsePass;

    /// Eliminate overwritten or repeated storage stores.
    pub const STORAGE_DSE_PASS -> "storage-dse" = StorageDsePass;

    /// Availability-dataflow redundancy elimination and PRE for memory-dependent reads.
    pub const LOAD_PRE_PASS -> "load-pre" = LoadPrePass;

    /// Canonicalize natural loops with explicit preheaders.
    pub const LOOP_CANONICALIZE_PASS -> "loop-canonicalize" = LoopCanonicalizePass;

    /// Strength-reduce affine induction-variable address expressions.
    pub const INDVAR_SIMPLIFY_PASS -> "indvar-simplify" = IndVarSimplifyPass;

    /// Promote simple loop-carried storage updates to memory.
    pub const STORAGE_PROMOTION_PASS -> "storage-promotion" = StorageScalarPromotionPass;

    /// Loop-Invariant Code Motion.
    pub const LICM_PASS -> "licm" = LicmPass;

    /// Range-based elimination of provably dead overflow-check branches.
    pub const CHECK_ELIM_PASS -> "check-elim" = CheckElimPass;

    /// Jump Threading (fixed-point).
    pub const JUMP_THREADING_PASS -> "jump-threading" = JumpThreadingPass;

    /// CFG Simplification (fixed-point).
    pub const CFG_SIMPLIFY_PASS -> "cfg-simplify" = CfgSimplifyPass;

    /// Promote non-escaping compiler-local slots to SSA values.
    pub const FRAME_SLOT_PROMOTION_PASS -> "frame-slot-promotion" = FrameSlotPromotionPass;

    /// Local dead memory-store elimination.
    pub const MEMORY_DSE_PASS -> "memory-dse" = MemoryDsePass;

    /// Place provably local fmp-bump allocations at static frame addresses.
    pub const STATIC_ALLOC_PASS -> "static-alloc" = StaticAllocPass;

    /// Dead Code Elimination (fixed-point).
    pub const DCE_PASS -> "dce" = DcePass;

    /// Aggressive dead-code elimination for dead control regions.
    pub const ADCE_PASS -> "adce" = AdcePass;

    /// ABI phase lowering: external functions become self-decoding wrappers.
    const LOWER_ABI_PASS_BASE -> "lower-abi" = LowerAbiPass::default();

    /// Dispatch phase lowering: synthesize the selector-switch `entry` function.
    const LOWER_DISPATCH_PASS_BASE -> "lower-dispatch" = LowerDispatchPass::default();

    /// EVM-shape lowering: non-returning internal calls become tail calls.
    const LOWER_EVM_SHAPED_PASS_BASE -> "lower-evm-shaped" = LowerEvmShapedPass::default();
}

/// ABI phase lowering with its phase range declared: consumes
/// `built`/`optimized` MIR and produces the `abi` phase.
pub const LOWER_ABI_PASS: PassInfo =
    LOWER_ABI_PASS_BASE.phases(MirPhase::Built, MirPhase::Optimized);

/// Dispatch phase lowering with its phase range declared: consumes exactly
/// `abi`-phase MIR and produces the `dispatch` phase.
pub const LOWER_DISPATCH_PASS: PassInfo =
    LOWER_DISPATCH_PASS_BASE.phases(MirPhase::Abi, MirPhase::Abi);

/// EVM-shape lowering with its phase range declared: consumes exactly
/// `dispatch`-phase MIR and produces the `evm-shaped` phase.
pub const LOWER_EVM_SHAPED_PASS: PassInfo =
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
    /// Validate MIR after every pass.
    pub validate_after_each: bool,
}

impl Default for PipelineOptions {
    fn default() -> Self {
        Self { print_after_each: false, validate_after_each: cfg!(debug_assertions) }
    }
}

/// Runs a named MIR pass over a module.
pub fn run_pass(module: &mut Module, pass: &PassInfo) -> bool {
    run_pass_with_options(module, pass, PipelineOptions::default())
}

fn run_pass_with_options(module: &mut Module, pass: &PassInfo, options: PipelineOptions) -> bool {
    // Passes declare which phases they operate on; the manager enforces it so a
    // pipeline entry cannot silently corrupt a module in the wrong phase.
    if !pass.admits(module) {
        return false;
    }
    let mut pm = PassManager::new();
    pm.set_validate_after_each(options.validate_after_each);
    pm.add_pass(pass.make_pass());
    pm.run(module).1
}

/// Runs a named MIR pass pipeline over a module.
pub fn run_pipeline(module: &mut Module, passes: &[PassInfo]) -> bool {
    let mut changed = false;
    for pass in passes {
        changed |= run_pass(module, pass);
    }
    changed
}

/// Runs a named MIR pass pipeline over a module with observer options.
pub fn run_pipeline_with_options(
    module: &mut Module,
    passes: &[PassInfo],
    options: PipelineOptions,
) -> bool {
    let mut changed = false;
    for pass in passes {
        changed |= run_pass_with_options(module, pass, options);
        if options.print_after_each {
            println!("// === {} (after {}) ===", module.name, pass.name);
            print!("{}", module.to_text());
        }
    }
    changed
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
pub fn run_default_pipeline(module: &mut Module) -> bool {
    run_default_pipeline_with_options(module, PipelineOptions::default())
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen with options.
///
/// This is a phase transition: the module comes out in [`MirPhase::Optimized`].
/// Ad-hoc pass lists run through [`run_pipeline`], such as `solar mir-opt`
/// invocations, deliberately do not advance the phase.
pub fn run_default_pipeline_with_options(module: &mut Module, options: PipelineOptions) -> bool {
    let mut changed = run_pipeline_with_options(module, DEFAULT_PIPELINE, options);
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
            let pass_changed = run_pass_with_options(module, pass, options);
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
pub struct AnalysisKey(TypeId);

impl AnalysisKey {
    /// Creates a key from a type.
    pub fn of<T: 'static>() -> Self {
        Self(TypeId::of::<T>())
    }
}

/// A read-only analysis pass.
///
/// Analysis passes inspect a function without modifying it and produce a
/// cacheable result that downstream passes can query via [`AnalysisManager`].
pub trait AnalysisPass {
    /// The result type produced by this analysis.
    type Result: 'static;

    /// The name of this analysis, for debugging and logging.
    fn name(&self) -> &str;

    /// Computes the analysis result for the given function.
    fn run(&self, func: &Function) -> Self::Result;
}

/// A transformation pass that mutates a MIR module.
///
/// Module-level passes can inspect or transform more than one function. Function-local passes
/// should implement [`FunctionPass`] instead and use the blanket [`ModulePass`] implementation.
pub trait ModulePass {
    /// The name of this pass, for debugging and logging.
    fn name(&self) -> &str;

    /// Runs the transformation on the given module.
    ///
    /// Returns true if the transform changed MIR.
    fn run(&mut self, module: &mut Module) -> bool;
}

/// A transformation pass that mutates one function at a time.
pub trait FunctionPass {
    /// The name of this pass, for debugging and logging.
    fn name(&self) -> &str;

    /// Runs the transformation on the given function.
    fn run_on_function(&mut self, func: &mut Function) -> bool;
}

impl<T: FunctionPass> ModulePass for T {
    fn name(&self) -> &str {
        FunctionPass::name(self)
    }

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
pub struct AnalysisManager {
    results: FxHashMap<AnalysisKey, Box<dyn Any>>,
}

impl AnalysisManager {
    /// Creates a new, empty analysis manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns the cached result for the given analysis type, if available.
    pub fn get<T: 'static>(&self) -> Option<&T> {
        let key = AnalysisKey::of::<T>();
        self.results.get(&key)?.downcast_ref::<T>()
    }

    /// Caches an analysis result.
    pub fn insert<T: 'static>(&mut self, result: T) {
        let key = AnalysisKey::of::<T>();
        self.results.insert(key, Box::new(result));
    }

    /// Returns the result of the analysis, computing and caching it if not already present.
    ///
    /// This is the recommended way to obtain analysis results, matching
    /// LLVM's `AnalysisManager::getResult<AnalysisT>(F)` pattern.
    pub fn get_or_compute<A: AnalysisPass>(&mut self, analysis: &A, func: &Function) -> &A::Result {
        let key = AnalysisKey::of::<A::Result>();
        self.results.entry(key).or_insert_with(|| {
            let result = analysis.run(func);
            Box::new(result)
        });
        self.results[&key].downcast_ref::<A::Result>().unwrap()
    }

    /// Invalidates all cached analysis results.
    pub fn invalidate_all(&mut self) {
        self.results.clear();
    }

    /// Invalidates a specific analysis result.
    pub fn invalidate<T: 'static>(&mut self) {
        let key = AnalysisKey::of::<T>();
        self.results.remove(&key);
    }
}

/// Orchestrates execution of [`ModulePass`]es on a module.
///
/// Each transform automatically invalidates all cached analyses after changing MIR.
pub struct PassManager {
    passes: Vec<Box<dyn ModulePass>>,
    validate_after_each: bool,
}

impl Default for PassManager {
    fn default() -> Self {
        Self { passes: Vec::new(), validate_after_each: cfg!(debug_assertions) }
    }
}

impl PassManager {
    /// Creates a new, empty pass manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a transformation pass to the pipeline.
    pub fn add_pass(&mut self, pass: Box<dyn ModulePass>) {
        self.passes.push(pass);
    }

    /// Enables or disables MIR validation after every pass.
    pub const fn set_validate_after_each(&mut self, enabled: bool) {
        self.validate_after_each = enabled;
    }

    /// Runs all transforms in order on the given module.
    /// Returns an [`AnalysisManager`] and whether any transform changed MIR.
    pub fn run(&mut self, module: &mut Module) -> (AnalysisManager, bool) {
        let mut am = AnalysisManager::new();
        let mut changed = false;
        if self.validate_after_each {
            validate_module_after_pass(module, "input");
        }
        for pass in &mut self.passes {
            let pass_name = pass.name().to_string();
            if pass.run(module) {
                changed = true;
                am.invalidate_all();
            }
            if self.validate_after_each {
                validate_module_after_pass(module, &pass_name);
            }
        }
        (am, changed)
    }
}

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let errors = Validator::validate_module(module);
    if errors.is_empty() {
        return;
    }

    let mut message = format!("MIR validation failed after `{pass_name}`");
    for error in errors {
        message.push_str("\n  ");
        message.push_str(&error.to_string());
    }
    panic!("{message}");
}

/// Liveness analysis pass.
pub struct LivenessAnalysis;

impl AnalysisPass for LivenessAnalysis {
    type Result = crate::analysis::Liveness;

    fn name(&self) -> &str {
        "liveness"
    }

    fn run(&self, func: &Function) -> Self::Result {
        crate::analysis::Liveness::compute(func)
    }
}

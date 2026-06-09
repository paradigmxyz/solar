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
    analysis::validate_module,
    mir::{Function, Module, module_to_text},
    transform::{
        CfgSimplifyPass, CsePass, DcePass, FrameSlotPromotionPass, FunctionDcePass,
        IndVarSimplifyPass, InlinePass, InstSimplifyPass, JumpThreadingPass, LicmPass,
        LoopCanonicalizePass, MemoryDsePass, PureEvalPass, SccpTransformPass, StorageLoadCsePass,
        StorageScalarPromotionPass,
    },
};
use solar_data_structures::map::FxHashMap;
use std::any::{Any, TypeId};

type PassFactory = fn() -> Box<dyn ModulePass>;

/// Registry entry for a MIR transform pass.
#[derive(Clone, Copy)]
pub struct PassInfo {
    /// Command-line and pipeline name.
    pub name: &'static str,
    /// Human-readable help text.
    pub description: &'static str,
    make_pass: PassFactory,
}

impl PassInfo {
    const fn new(name: &'static str, description: &'static str, make_pass: PassFactory) -> Self {
        Self { name, description, make_pass }
    }

    fn make_pass(&self) -> Box<dyn ModulePass> {
        (self.make_pass)()
    }
}

fn make_inline_pass() -> Box<dyn ModulePass> {
    Box::new(InlinePass)
}

fn make_function_dce_pass() -> Box<dyn ModulePass> {
    Box::new(FunctionDcePass)
}

fn make_sccp_pass() -> Box<dyn ModulePass> {
    Box::new(SccpTransformPass)
}

fn make_pure_eval_pass() -> Box<dyn ModulePass> {
    Box::new(PureEvalPass)
}

fn make_inst_simplify_pass() -> Box<dyn ModulePass> {
    Box::new(InstSimplifyPass)
}

fn make_cse_pass() -> Box<dyn ModulePass> {
    Box::new(CsePass)
}

fn make_storage_load_cse_pass() -> Box<dyn ModulePass> {
    Box::new(StorageLoadCsePass)
}

fn make_loop_canonicalize_pass() -> Box<dyn ModulePass> {
    Box::new(LoopCanonicalizePass)
}

fn make_indvar_simplify_pass() -> Box<dyn ModulePass> {
    Box::new(IndVarSimplifyPass)
}

fn make_storage_promotion_pass() -> Box<dyn ModulePass> {
    Box::new(StorageScalarPromotionPass)
}

fn make_licm_pass() -> Box<dyn ModulePass> {
    Box::new(LicmPass)
}

fn make_jump_threading_pass() -> Box<dyn ModulePass> {
    Box::new(JumpThreadingPass)
}

fn make_cfg_simplify_pass() -> Box<dyn ModulePass> {
    Box::new(CfgSimplifyPass)
}

fn make_frame_slot_promotion_pass() -> Box<dyn ModulePass> {
    Box::new(FrameSlotPromotionPass)
}

fn make_memory_dse_pass() -> Box<dyn ModulePass> {
    Box::new(MemoryDsePass)
}

fn make_dce_pass() -> Box<dyn ModulePass> {
    Box::new(DcePass)
}

pub const INLINE_PASS: PassInfo =
    PassInfo::new("inline", "Internal MIR function inlining", make_inline_pass);
pub const FUNCTION_DCE_PASS: PassInfo =
    PassInfo::new("function-dce", "Dead internal function elimination", make_function_dce_pass);
pub const SCCP_PASS: PassInfo =
    PassInfo::new("sccp", "Sparse Conditional Constant Propagation", make_sccp_pass);
pub const PURE_EVAL_PASS: PassInfo = PassInfo::new(
    "pure-eval",
    "Bounded evaluator for closed pure MIR loops/functions",
    make_pure_eval_pass,
);
pub const INST_SIMPLIFY_PASS: PassInfo =
    PassInfo::new("inst-simplify", "Local MIR instruction simplification", make_inst_simplify_pass);
pub const CSE_PASS: PassInfo =
    PassInfo::new("cse", "Common Subexpression Elimination (fixed-point)", make_cse_pass);
pub const STORAGE_LOAD_CSE_PASS: PassInfo = PassInfo::new(
    "storage-load-cse",
    "Reuse storage loads across definitely-disjoint stores",
    make_storage_load_cse_pass,
);
pub const LOOP_CANONICALIZE_PASS: PassInfo = PassInfo::new(
    "loop-canonicalize",
    "Canonicalize natural loops with explicit preheaders",
    make_loop_canonicalize_pass,
);
pub const INDVAR_SIMPLIFY_PASS: PassInfo = PassInfo::new(
    "indvar-simplify",
    "Strength-reduce affine induction-variable address expressions",
    make_indvar_simplify_pass,
);
pub const STORAGE_PROMOTION_PASS: PassInfo = PassInfo::new(
    "storage-promotion",
    "Promote simple loop-carried storage updates to memory",
    make_storage_promotion_pass,
);
pub const LICM_PASS: PassInfo = PassInfo::new("licm", "Loop-Invariant Code Motion", make_licm_pass);
pub const JUMP_THREADING_PASS: PassInfo =
    PassInfo::new("jump-threading", "Jump Threading (fixed-point)", make_jump_threading_pass);
pub const CFG_SIMPLIFY_PASS: PassInfo =
    PassInfo::new("cfg-simplify", "CFG Simplification (fixed-point)", make_cfg_simplify_pass);
pub const FRAME_SLOT_PROMOTION_PASS: PassInfo = PassInfo::new(
    "frame-slot-promotion",
    "Promote non-escaping compiler-local slots to SSA values",
    make_frame_slot_promotion_pass,
);
pub const MEMORY_DSE_PASS: PassInfo =
    PassInfo::new("memory-dse", "Local dead memory-store elimination", make_memory_dse_pass);
pub const DCE_PASS: PassInfo =
    PassInfo::new("dce", "Dead Code Elimination (fixed-point)", make_dce_pass);

/// All known MIR passes exposed to `solar mir-opt`.
pub const PASS_REGISTRY: &[&PassInfo] = &[
    &INLINE_PASS,
    &FUNCTION_DCE_PASS,
    &DCE_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &LOOP_CANONICALIZE_PASS,
    &INDVAR_SIMPLIFY_PASS,
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &LICM_PASS,
    &CFG_SIMPLIFY_PASS,
    &JUMP_THREADING_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &MEMORY_DSE_PASS,
    &STORAGE_PROMOTION_PASS,
];

/// Finds a pass in the global MIR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static PassInfo> {
    PASS_REGISTRY.iter().copied().find(|pass| pass.name == name)
}

/// The canonical MIR optimization pipeline used by EVM codegen.
pub const DEFAULT_PIPELINE: &[&PassInfo] = &[
    &INLINE_PASS,
    &FUNCTION_DCE_PASS,
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &LOOP_CANONICALIZE_PASS,
    &INDVAR_SIMPLIFY_PASS,
    &STORAGE_PROMOTION_PASS,
    &LICM_PASS,
    &JUMP_THREADING_PASS,
    &CFG_SIMPLIFY_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &MEMORY_DSE_PASS,
    &DCE_PASS,
];

/// Cleanup passes rerun after the primary pipeline until no pass changes MIR.
///
/// Keep this group focused on simplification and canonicalization. Structural
/// profitability passes such as inlining and storage promotion run once in
/// [`DEFAULT_PIPELINE`], while this loop cleans up opportunities exposed by
/// those transforms.
pub const DEFAULT_CLEANUP_PIPELINE: &[&PassInfo] = &[
    &SCCP_PASS,
    &PURE_EVAL_PASS,
    &INST_SIMPLIFY_PASS,
    &CSE_PASS,
    &STORAGE_LOAD_CSE_PASS,
    &JUMP_THREADING_PASS,
    &CFG_SIMPLIFY_PASS,
    &FRAME_SLOT_PROMOTION_PASS,
    &MEMORY_DSE_PASS,
    &DCE_PASS,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Options for running a MIR pass pipeline.
#[derive(Clone, Copy, Debug, Default)]
pub struct PipelineOptions {
    /// Print the full module after every pass in the pipeline.
    pub print_after_each: bool,
}

/// Runs a named MIR pass over a module.
pub fn run_pass(module: &mut Module, pass: &PassInfo) -> bool {
    let mut pm = PassManager::new();
    pm.add_pass(pass.make_pass());
    pm.run(module).1
}

/// Runs a named MIR pass pipeline over a module.
pub fn run_pipeline(module: &mut Module, passes: &[&PassInfo]) -> bool {
    let mut changed = false;
    for &pass in passes {
        changed |= run_pass(module, pass);
    }
    changed
}

/// Runs a named MIR pass pipeline over a module with observer options.
pub fn run_pipeline_with_options(
    module: &mut Module,
    passes: &[&PassInfo],
    options: PipelineOptions,
) -> bool {
    let mut changed = false;
    for &pass in passes {
        changed |= run_pass(module, pass);
        if options.print_after_each {
            println!("// === {} (after {}) ===", module.name, pass.name);
            print!("{}", module_to_text(module));
        }
    }
    changed
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
pub fn run_default_pipeline(module: &mut Module) -> bool {
    run_default_pipeline_with_options(module, PipelineOptions::default())
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen with options.
pub fn run_default_pipeline_with_options(module: &mut Module, options: PipelineOptions) -> bool {
    let mut changed = run_pipeline_with_options(module, DEFAULT_PIPELINE, options);
    changed |=
        run_cleanup_pipeline_to_fixpoint(module, DEFAULT_CLEANUP_PIPELINE, options, "cleanup");
    changed
}

fn run_cleanup_pipeline_to_fixpoint(
    module: &mut Module,
    passes: &[&PassInfo],
    options: PipelineOptions,
    label: &str,
) -> bool {
    let mut changed = false;
    for round in 1..=DEFAULT_CLEANUP_MAX_ROUNDS {
        let mut round_changed = false;
        for &pass in passes {
            let pass_changed = run_pass(module, pass);
            round_changed |= pass_changed;
            if options.print_after_each {
                println!("// === {} (after {label}-{round}:{}) ===", module.name, pass.name);
                print!("{}", module_to_text(module));
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
#[derive(Default)]
pub struct PassManager {
    passes: Vec<Box<dyn ModulePass>>,
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

    /// Runs all transforms in order on the given module.
    /// Returns an [`AnalysisManager`] and whether any transform changed MIR.
    pub fn run(&mut self, module: &mut Module) -> (AnalysisManager, bool) {
        let mut am = AnalysisManager::new();
        let mut changed = false;
        for pass in &mut self.passes {
            let pass_name = pass.name().to_string();
            if pass.run(module) {
                changed = true;
                am.invalidate_all();
            }
            validate_module_after_pass(module, &pass_name);
        }
        (am, changed)
    }
}

#[cfg(debug_assertions)]
fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let errors = validate_module(module);
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

#[cfg(not(debug_assertions))]
fn validate_module_after_pass(_module: &Module, _pass_name: &str) {}

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

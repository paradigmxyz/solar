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

/// A named MIR pass that can be used by the default codegen pipeline or `solar mir-opt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PassName {
    /// Internal MIR function inlining.
    Inline,
    /// Dead internal function elimination.
    FunctionDce,
    /// Sparse conditional constant propagation.
    Sccp,
    /// Bounded evaluator for closed pure MIR loops/functions.
    PureEval,
    /// Local MIR instruction simplification.
    InstSimplify,
    /// Local common subexpression elimination.
    Cse,
    /// Storage-load CSE across definitely-disjoint stores.
    StorageLoadCse,
    /// Loop canonicalization into LoopSimplify-style preheader form.
    LoopCanonicalize,
    /// Induction-variable simplification and strength reduction.
    IndVarSimplify,
    /// Loop-carried storage scalar promotion.
    StoragePromotion,
    /// Loop-invariant code motion.
    Licm,
    /// Jump threading.
    JumpThreading,
    /// CFG simplification.
    CfgSimplify,
    /// Compiler-local scalar promotion.
    FrameSlotPromotion,
    /// Local dead memory-store elimination.
    MemoryDse,
    /// Dead code elimination.
    Dce,
}

impl PassName {
    /// All known MIR passes exposed to `solar mir-opt`.
    pub const KNOWN: &'static [Self] = &[
        Self::Inline,
        Self::FunctionDce,
        Self::Dce,
        Self::InstSimplify,
        Self::Cse,
        Self::StorageLoadCse,
        Self::LoopCanonicalize,
        Self::IndVarSimplify,
        Self::Sccp,
        Self::PureEval,
        Self::Licm,
        Self::CfgSimplify,
        Self::JumpThreading,
        Self::FrameSlotPromotion,
        Self::MemoryDse,
        Self::StoragePromotion,
    ];

    /// The command-line name for this pass.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Inline => "inline",
            Self::FunctionDce => "function-dce",
            Self::Sccp => "sccp",
            Self::PureEval => "pure-eval",
            Self::InstSimplify => "inst-simplify",
            Self::Cse => "cse",
            Self::StorageLoadCse => "storage-load-cse",
            Self::LoopCanonicalize => "loop-canonicalize",
            Self::IndVarSimplify => "indvar-simplify",
            Self::StoragePromotion => "storage-promotion",
            Self::Licm => "licm",
            Self::JumpThreading => "jump-threading",
            Self::CfgSimplify => "cfg-simplify",
            Self::FrameSlotPromotion => "frame-slot-promotion",
            Self::MemoryDse => "memory-dse",
            Self::Dce => "dce",
        }
    }

    /// Human-readable description for help output.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Inline => "Internal MIR function inlining",
            Self::FunctionDce => "Dead internal function elimination",
            Self::Sccp => "Sparse Conditional Constant Propagation",
            Self::PureEval => "Bounded evaluator for closed pure MIR loops/functions",
            Self::InstSimplify => "Local MIR instruction simplification",
            Self::Cse => "Common Subexpression Elimination (fixed-point)",
            Self::StorageLoadCse => "Reuse storage loads across definitely-disjoint stores",
            Self::LoopCanonicalize => "Canonicalize natural loops with explicit preheaders",
            Self::IndVarSimplify => "Strength-reduce affine induction-variable address expressions",
            Self::StoragePromotion => "Promote simple loop-carried storage updates to memory",
            Self::Licm => "Loop-Invariant Code Motion",
            Self::JumpThreading => "Jump Threading (fixed-point)",
            Self::CfgSimplify => "CFG Simplification (fixed-point)",
            Self::FrameSlotPromotion => "Promote non-escaping compiler-local slots to SSA values",
            Self::MemoryDse => "Local dead memory-store elimination",
            Self::Dce => "Dead Code Elimination (fixed-point)",
        }
    }

    /// Parses a command-line pass name.
    pub fn parse(name: &str) -> Option<Self> {
        Some(match name {
            "inline" => Self::Inline,
            "function-dce" => Self::FunctionDce,
            "sccp" => Self::Sccp,
            "pure-eval" => Self::PureEval,
            "inst-simplify" => Self::InstSimplify,
            "cse" => Self::Cse,
            "storage-load-cse" => Self::StorageLoadCse,
            "loop-canonicalize" => Self::LoopCanonicalize,
            "indvar-simplify" => Self::IndVarSimplify,
            "storage-promotion" => Self::StoragePromotion,
            "licm" => Self::Licm,
            "jump-threading" => Self::JumpThreading,
            "cfg-simplify" => Self::CfgSimplify,
            "frame-slot-promotion" => Self::FrameSlotPromotion,
            "memory-dse" => Self::MemoryDse,
            "dce" => Self::Dce,
            _ => return None,
        })
    }
}

/// The canonical MIR optimization pipeline used by EVM codegen.
pub const DEFAULT_PIPELINE: &[PassName] = &[
    PassName::Inline,
    PassName::FunctionDce,
    PassName::Sccp,
    PassName::PureEval,
    PassName::InstSimplify,
    PassName::Cse,
    PassName::StorageLoadCse,
    PassName::LoopCanonicalize,
    PassName::IndVarSimplify,
    PassName::StoragePromotion,
    PassName::Licm,
    PassName::JumpThreading,
    PassName::CfgSimplify,
    PassName::FrameSlotPromotion,
    PassName::MemoryDse,
    PassName::Dce,
];

/// Cleanup passes rerun after the primary pipeline until no pass changes MIR.
///
/// Keep this group focused on simplification and canonicalization. Structural
/// profitability passes such as inlining and storage promotion run once in
/// [`DEFAULT_PIPELINE`], while this loop cleans up opportunities exposed by
/// those transforms.
pub const DEFAULT_CLEANUP_PIPELINE: &[PassName] = &[
    PassName::Sccp,
    PassName::PureEval,
    PassName::InstSimplify,
    PassName::Cse,
    PassName::StorageLoadCse,
    PassName::JumpThreading,
    PassName::CfgSimplify,
    PassName::FrameSlotPromotion,
    PassName::MemoryDse,
    PassName::Dce,
];

const DEFAULT_CLEANUP_MAX_ROUNDS: usize = 3;

/// Options for running a MIR pass pipeline.
#[derive(Clone, Copy, Debug, Default)]
pub struct PipelineOptions {
    /// Print the full module after every pass in the pipeline.
    pub print_after_each: bool,
}

/// Runs a named MIR pass over a module.
pub fn run_pass(module: &mut Module, pass: PassName) -> bool {
    let mut pm = PassManager::new();
    pm.add_pass(make_pass(pass));
    pm.run(module).1
}

/// Runs a named MIR pass pipeline over a module.
pub fn run_pipeline(module: &mut Module, passes: &[PassName]) -> bool {
    let mut changed = false;
    for &pass in passes {
        changed |= run_pass(module, pass);
    }
    changed
}

/// Runs a named MIR pass pipeline over a module with observer options.
pub fn run_pipeline_with_options(
    module: &mut Module,
    passes: &[PassName],
    options: PipelineOptions,
) -> bool {
    let mut changed = false;
    for &pass in passes {
        changed |= run_pass(module, pass);
        if options.print_after_each {
            println!("// === {} (after {}) ===", module.name, pass.as_str());
            println!("{}", module_to_text(module));
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
    passes: &[PassName],
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
                println!("// === {} (after {label}-{round}:{}) ===", module.name, pass.as_str());
                println!("{}", module_to_text(module));
            }
        }
        if !round_changed {
            break;
        }
        changed = true;
    }
    changed
}

fn make_pass(pass: PassName) -> Box<dyn ModulePass> {
    match pass {
        PassName::Inline => Box::new(InlinePass),
        PassName::FunctionDce => Box::new(FunctionDcePass),
        PassName::Sccp => Box::new(SccpTransformPass),
        PassName::PureEval => Box::new(PureEvalPass),
        PassName::InstSimplify => Box::new(InstSimplifyPass),
        PassName::Cse => Box::new(CsePass),
        PassName::StorageLoadCse => Box::new(StorageLoadCsePass),
        PassName::LoopCanonicalize => Box::new(LoopCanonicalizePass),
        PassName::IndVarSimplify => Box::new(IndVarSimplifyPass),
        PassName::StoragePromotion => Box::new(StorageScalarPromotionPass),
        PassName::Licm => Box::new(LicmPass),
        PassName::JumpThreading => Box::new(JumpThreadingPass),
        PassName::CfgSimplify => Box::new(CfgSimplifyPass),
        PassName::FrameSlotPromotion => Box::new(FrameSlotPromotionPass),
        PassName::MemoryDse => Box::new(MemoryDsePass),
        PassName::Dce => Box::new(DcePass),
    }
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
            if pass.run(module) {
                changed = true;
                am.invalidate_all();
            }
        }
        (am, changed)
    }
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

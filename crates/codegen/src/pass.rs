//! Pass infrastructure for MIR transformations and analyses.
//!
//! Inspired by LLVM/MLIR pass infrastructure:
//! - **Analysis passes** ([`AnalysisPass`]) are read-only and produce a cached result. They take
//!   `&Function` and store their result in [`AnalysisManager`].
//! - **Transformation passes** ([`TransformPass`]) modify the IR. They take `&mut Function` and
//!   should invalidate cached analyses.
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
//! pm.add_transform(Box::new(DcePass));
//! pm.run(&mut func);
//! ```

use crate::{
    mir::{Function, Module},
    transform::{DeadFunctionEliminator, MirInliner},
};
use rustc_hash::FxHashMap;
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
    /// Local MIR instruction simplification.
    InstSimplify,
    /// Local common subexpression elimination.
    Cse,
    /// Storage-load CSE across definitely-disjoint stores.
    StorageLoadCse,
    /// Loop-carried storage scalar promotion.
    StoragePromotion,
    /// Loop-invariant code motion.
    Licm,
    /// Jump threading.
    JumpThreading,
    /// CFG simplification.
    CfgSimplify,
    /// Internal-frame scalar promotion.
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
        Self::Sccp,
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
            Self::InstSimplify => "inst-simplify",
            Self::Cse => "cse",
            Self::StorageLoadCse => "storage-load-cse",
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
            Self::InstSimplify => "Local MIR instruction simplification",
            Self::Cse => "Common Subexpression Elimination (fixed-point)",
            Self::StorageLoadCse => "Reuse storage loads across definitely-disjoint stores",
            Self::StoragePromotion => "Promote simple loop-carried storage updates to memory",
            Self::Licm => "Loop-Invariant Code Motion",
            Self::JumpThreading => "Jump Threading (fixed-point)",
            Self::CfgSimplify => "CFG Simplification (fixed-point)",
            Self::FrameSlotPromotion => "Promote non-escaping internal-frame slots to SSA values",
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
            "inst-simplify" => Self::InstSimplify,
            "cse" => Self::Cse,
            "storage-load-cse" => Self::StorageLoadCse,
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
    PassName::InstSimplify,
    PassName::Cse,
    PassName::StorageLoadCse,
    PassName::StoragePromotion,
    PassName::Licm,
    PassName::JumpThreading,
    PassName::CfgSimplify,
    PassName::FrameSlotPromotion,
    PassName::MemoryDse,
    PassName::Dce,
];

/// Runs a named MIR pass over a module.
pub fn run_pass(module: &mut Module, pass: PassName) {
    match pass {
        PassName::Inline => {
            MirInliner::default().run(module);
        }
        PassName::FunctionDce => {
            DeadFunctionEliminator::new().run(module);
        }
        pass => {
            let Some(transform) = make_transform_pass(pass) else { return };
            let mut pm = PassManager::new();
            pm.add_transform(transform);
            for func in module.functions.iter_mut().filter(|func| !func.blocks.is_empty()) {
                pm.run(func);
            }
        }
    }
}

/// Runs a named MIR pass pipeline over a module.
pub fn run_pipeline(module: &mut Module, passes: &[PassName]) {
    for &pass in passes {
        run_pass(module, pass);
    }
}

/// Runs the canonical MIR optimization pipeline used by EVM codegen.
pub fn run_default_pipeline(module: &mut Module) {
    run_pipeline(module, DEFAULT_PIPELINE);
}

fn make_transform_pass(pass: PassName) -> Option<Box<dyn TransformPass>> {
    Some(match pass {
        PassName::Inline | PassName::FunctionDce => return None,
        PassName::Sccp => Box::new(SccpTransformPass),
        PassName::InstSimplify => Box::new(InstSimplifyPass),
        PassName::Cse => Box::new(CsePass),
        PassName::StorageLoadCse => Box::new(StorageLoadCsePass),
        PassName::StoragePromotion => Box::new(StorageScalarPromotionPass),
        PassName::Licm => Box::new(LicmPass),
        PassName::JumpThreading => Box::new(JumpThreadingPass),
        PassName::CfgSimplify => Box::new(CfgSimplifyPass),
        PassName::FrameSlotPromotion => Box::new(FrameSlotPromotionPass),
        PassName::MemoryDse => Box::new(MemoryDsePass),
        PassName::Dce => Box::new(DcePass),
    })
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

/// A transformation pass that mutates a function.
///
/// Transformation passes should call [`AnalysisManager::invalidate_all`]
/// after modifying the IR (or the [`PassManager`] does this automatically).
pub trait TransformPass {
    /// The name of this transform, for debugging and logging.
    fn name(&self) -> &str;

    /// Runs the transformation on the given function.
    fn run(&mut self, func: &mut Function);
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

/// Orchestrates execution of [`TransformPass`]es on a function.
///
/// Each transform automatically invalidates all cached analyses after running.
#[derive(Default)]
pub struct PassManager {
    passes: Vec<Box<dyn TransformPass>>,
}

impl PassManager {
    /// Creates a new, empty pass manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a transformation pass to the pipeline.
    pub fn add_transform(&mut self, pass: Box<dyn TransformPass>) {
        self.passes.push(pass);
    }

    /// Runs all transforms in order on the given function.
    /// Returns an [`AnalysisManager`] (empty after transforms invalidate everything).
    pub fn run(&mut self, func: &mut Function) -> AnalysisManager {
        let mut am = AnalysisManager::new();
        for pass in &mut self.passes {
            if func.blocks.is_empty() {
                break;
            }
            pass.run(func);
            am.invalidate_all();
        }
        am
    }
}

// === Concrete pass adapters ===

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

/// Dead code elimination transform.
///
/// Iterates `DeadCodeEliminator` to a fixed point internally so a single
/// `run` invocation removes all dead code reachable from the entry block.
pub struct DcePass;

impl TransformPass for DcePass {
    fn name(&self) -> &str {
        "dce"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::DeadCodeEliminator::new().run_to_fixpoint(func);
        crate::transform::repair_reachability_phis(func);
    }
}

/// CFG simplification transform.
///
/// Iterates `CfgSimplifier` to a fixed point so chained simplifications
/// (e.g., merging blocks made empty by previous merges) all happen.
pub struct CfgSimplifyPass;

impl TransformPass for CfgSimplifyPass {
    fn name(&self) -> &str {
        "cfg-simplify"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::CfgSimplifier::new().run_to_fixpoint(func);
    }
}

/// Jump threading transform.
///
/// Eliminates redundant unconditional jumps by threading control flow
/// through forwarder blocks. Iterates to a fixed point.
pub struct JumpThreadingPass;

impl TransformPass for JumpThreadingPass {
    fn name(&self) -> &str {
        "jump-threading"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::JumpThreader::new().run_to_fixpoint(func);
    }
}

/// Sparse Conditional Constant Propagation transform.
///
/// Propagates constants through the CFG using SSA def-use chains,
/// evaluates branch conditions to discover unreachable paths, and
/// folds phi nodes when all executable incoming values agree.
pub struct SccpTransformPass;

impl TransformPass for SccpTransformPass {
    fn name(&self) -> &str {
        "sccp"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::SccpPass::new().run(func);
        crate::transform::repair_reachability_phis(func);
    }
}

/// Local instruction simplification transform.
///
/// Removes exact algebraic no-ops and rewrites equivalent EVM instruction
/// patterns before local CSE and stack scheduling.
pub struct InstSimplifyPass;

impl TransformPass for InstSimplifyPass {
    fn name(&self) -> &str {
        "inst-simplify"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::InstSimplifier::new().run_to_fixpoint(func);
    }
}

/// Common subexpression elimination transform.
///
/// Eliminates redundant computations within each basic block (local CSE).
/// Handles commutative normalization and SLOAD caching (invalidated by SSTORE).
/// Iterates to a fixed point.
pub struct CsePass;

impl TransformPass for CsePass {
    fn name(&self) -> &str {
        "cse"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::CommonSubexprEliminator::new().run_to_fixpoint(func);
    }
}

/// Storage-load CSE transform.
///
/// Reuses `sload` results across definitely-disjoint storage stores on
/// straight-line paths.
pub struct StorageLoadCsePass;

impl TransformPass for StorageLoadCsePass {
    fn name(&self) -> &str {
        "storage-load-cse"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::StorageLoadCse::new().run_to_fixpoint(func);
    }
}

/// Local dead memory-store elimination transform.
///
/// Removes full-word memory stores that are overwritten in the same block
/// before memory, gas, or calls can observe them.
pub struct MemoryDsePass;

impl TransformPass for MemoryDsePass {
    fn name(&self) -> &str {
        "memory-dse"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::MemoryStoreEliminator::new().run_to_fixpoint(func);
    }
}

/// Internal-frame scalar promotion transform.
///
/// Promotes non-escaping full-word internal-frame slots to SSA values and
/// inserts phi nodes at loop headers/joins as needed.
pub struct FrameSlotPromotionPass;

impl TransformPass for FrameSlotPromotionPass {
    fn name(&self) -> &str {
        "frame-slot-promotion"
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::FrameSlotPromoter::new().run(func);
        crate::transform::repair_reachability_phis(func);
    }
}

/// Loop-carried storage scalar promotion transform.
///
/// Rewrites simple storage update loops so the loop updates a memory-backed
/// scalar and stores the final value once on clean loop exits.
pub struct StorageScalarPromotionPass;

impl TransformPass for StorageScalarPromotionPass {
    fn name(&self) -> &str {
        PassName::StoragePromotion.as_str()
    }

    fn run(&mut self, func: &mut Function) {
        crate::transform::StorageScalarPromoter::new().run(func);
    }
}

/// Loop-invariant code motion transform.
pub struct LicmPass;

impl TransformPass for LicmPass {
    fn name(&self) -> &str {
        "licm"
    }

    fn run(&mut self, func: &mut Function) {
        let config = crate::transform::LoopOptConfig {
            enable_licm: true,
            min_licm_profit: 3,
            max_licm_hoisted_insts: 4,
        };
        crate::transform::LoopOptimizer::new(config).optimize(func);
    }
}

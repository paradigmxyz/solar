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

use crate::mir::Function;
use rustc_hash::FxHashMap;
use std::any::{Any, TypeId};

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
        if !self.results.contains_key(&key) {
            let result = analysis.run(func);
            self.results.insert(key, Box::new(result));
        }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType, ValueId};
    use solar_interface::Ident;

    fn make_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn analysis_caching() {
        let mut am = AnalysisManager::new();
        am.insert(42u32);
        am.insert(String::from("hello"));
        assert_eq!(am.get::<u32>(), Some(&42));
        assert_eq!(am.get::<String>(), Some(&String::from("hello")));
        assert_eq!(am.get::<f64>(), None);
    }

    #[test]
    fn invalidate_all() {
        let mut am = AnalysisManager::new();
        am.insert(42u32);
        am.invalidate_all();
        assert_eq!(am.get::<u32>(), None);
    }

    #[test]
    fn invalidate_specific() {
        let mut am = AnalysisManager::new();
        am.insert(42u32);
        am.insert(String::from("kept"));
        am.invalidate::<u32>();
        assert_eq!(am.get::<u32>(), None);
        assert_eq!(am.get::<String>(), Some(&String::from("kept")));
    }

    #[test]
    fn get_or_compute_caches() {
        let mut func = make_func();
        {
            let mut b = FunctionBuilder::new(&mut func);
            let x = b.add_param(MirType::uint256());
            b.ret([x]);
        }

        let mut am = AnalysisManager::new();
        // First call computes.
        let liveness = am.get_or_compute(&LivenessAnalysis, &func);
        assert!(liveness.live_in(func.entry_block).contains(ValueId::from_usize(0)));

        // Second call hits the cache (same reference).
        let _liveness2 = am.get_or_compute(&LivenessAnalysis, &func);
        // If it cached correctly, the value is still there.
        assert!(am.get::<crate::analysis::Liveness>().is_some());
    }

    #[test]
    fn transform_pipeline_invalidates() {
        let mut func = make_func();
        {
            let mut b = FunctionBuilder::new(&mut func);
            let x = b.add_param(MirType::uint256());
            let y = b.imm_u64(1);
            let _unused = b.add(x, y);
            b.ret([x]);
        }

        let mut pm = PassManager::new();
        pm.add_transform(Box::new(DcePass));
        let am = pm.run(&mut func);

        // Transforms invalidate all caches.
        assert!(am.get::<crate::analysis::Liveness>().is_none());
    }
}

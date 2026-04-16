//! Pass infrastructure for MIR transformations and analyses.
//!
//! Inspired by LLVM/MLIR pass infrastructure:
//! - **Analysis passes** are read-only and produce a cached result.
//! - **Transformation passes** modify the IR and invalidate cached analyses.
//!
//! # Usage
//!
//! ```ignore
//! let mut pm = PassManager::new();
//! pm.add_transform(Box::new(DcePass));
//! pm.add_transform(Box::new(ConstFoldPass));
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

/// Manages cached analysis results for a function.
///
/// Analyses are keyed by their result type via [`AnalysisKey`].
/// Transformation passes should call [`invalidate_all`](Self::invalidate_all)
/// after modifying the IR.
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

/// A function-level pass (analysis or transformation).
pub trait FunctionPass {
    /// The name of this pass, for debugging and logging.
    fn name(&self) -> &str;

    /// Runs the pass on the given function.
    ///
    /// Analysis passes store results in `am`. Transformation passes mutate
    /// `func` and should call `am.invalidate_all()`.
    fn run_on_function(&mut self, func: &mut Function, am: &mut AnalysisManager);

    /// Returns `true` if this pass is an analysis (does not modify IR).
    fn is_analysis(&self) -> bool {
        false
    }
}

/// Orchestrates pass execution on a function.
#[derive(Default)]
pub struct PassManager {
    passes: Vec<Box<dyn FunctionPass>>,
}

impl PassManager {
    /// Creates a new, empty pass manager.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a pass to the pipeline.
    pub fn add_pass(&mut self, pass: Box<dyn FunctionPass>) {
        self.passes.push(pass);
    }

    /// Runs all passes in order on the given function.
    /// Returns the [`AnalysisManager`] with any cached results from the last pass(es).
    pub fn run(&mut self, func: &mut Function) -> AnalysisManager {
        let mut am = AnalysisManager::new();
        for pass in &mut self.passes {
            pass.run_on_function(func, &mut am);
        }
        am
    }
}

/// Adapter: wraps the existing [`Liveness`](crate::analysis::Liveness) as a [`FunctionPass`].
pub struct LivenessPass;

impl FunctionPass for LivenessPass {
    fn name(&self) -> &str {
        "liveness"
    }

    fn run_on_function(&mut self, func: &mut Function, am: &mut AnalysisManager) {
        let result = crate::analysis::Liveness::compute(func);
        am.insert(result);
    }

    fn is_analysis(&self) -> bool {
        true
    }
}

/// Adapter: wraps the existing [`DeadCodeEliminator`](crate::transform::DeadCodeEliminator) as a [`FunctionPass`].
pub struct DcePass;

impl FunctionPass for DcePass {
    fn name(&self) -> &str {
        "dce"
    }

    fn run_on_function(&mut self, func: &mut Function, am: &mut AnalysisManager) {
        let mut dce = crate::transform::DeadCodeEliminator::new();
        dce.run(func);
        am.invalidate_all();
    }
}

/// Adapter: wraps the existing [`CfgSimplifier`](crate::transform::CfgSimplifier) as a [`FunctionPass`].
pub struct CfgSimplifyPass;

impl FunctionPass for CfgSimplifyPass {
    fn name(&self) -> &str {
        "cfg-simplify"
    }

    fn run_on_function(&mut self, func: &mut Function, am: &mut AnalysisManager) {
        let mut simplifier = crate::transform::CfgSimplifier::new();
        simplifier.run(func);
        am.invalidate_all();
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
    fn liveness_pass_via_manager() {
        let mut func = make_func();
        {
            let mut b = FunctionBuilder::new(&mut func);
            let x = b.add_param(MirType::uint256());
            b.ret([x]);
        }

        let mut pm = PassManager::new();
        pm.add_pass(Box::new(LivenessPass));
        let am = pm.run(&mut func);

        let liveness = am.get::<crate::analysis::Liveness>().expect("liveness cached");
        assert!(liveness.live_in(func.entry_block).contains(ValueId::from_usize(0)));
    }

    #[test]
    fn transform_invalidates_analyses() {
        let mut func = make_func();
        {
            let mut b = FunctionBuilder::new(&mut func);
            let x = b.add_param(MirType::uint256());
            let y = b.imm_u64(1);
            let _unused = b.add(x, y); // Will be eliminated by DCE.
            b.ret([x]);
        }

        let mut pm = PassManager::new();
        pm.add_pass(Box::new(LivenessPass));
        pm.add_pass(Box::new(DcePass));
        let am = pm.run(&mut func);

        // After DCE, liveness should have been invalidated.
        assert!(am.get::<crate::analysis::Liveness>().is_none());
    }
}

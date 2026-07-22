//! Shared transformation pass infrastructure.

use solar_config::OptimizationMode;
use solar_sema::Gcx;

/// A transformation pass over `M`.
pub trait Pass<M>: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str;

    /// Human-readable help text.
    fn description(&self) -> &'static str;

    /// Returns whether this pass accepts the current module state.
    fn is_enabled(&self, _gcx: Gcx<'_>, _module: &M) -> bool {
        true
    }

    /// Runs the pass and returns whether it changed the module.
    fn run(&self, gcx: Gcx<'_>, module: &mut M) -> bool;
}

/// A statically registered pass backed by a factory function.
pub(crate) struct PassFactory<M> {
    name: &'static str,
    description: &'static str,
    is_enabled: for<'gcx> fn(Gcx<'gcx>, &M) -> bool,
    run: for<'gcx> fn(Gcx<'gcx>, &mut M) -> bool,
}

impl<M> PassFactory<M> {
    /// Creates a pass factory.
    pub(crate) const fn new(
        name: &'static str,
        description: &'static str,
        run: for<'gcx> fn(Gcx<'gcx>, &mut M) -> bool,
    ) -> Self {
        Self { name, description, is_enabled: optimizations_enabled, run }
    }

    /// Marks this pass as required independently of the optimization level.
    pub(crate) const fn required(mut self) -> Self {
        self.is_enabled = always_enabled;
        self
    }
}

impl<M> Pass<M> for PassFactory<M> {
    fn name(&self) -> &'static str {
        self.name
    }

    fn description(&self) -> &'static str {
        self.description
    }

    fn is_enabled(&self, gcx: Gcx<'_>, module: &M) -> bool {
        (self.is_enabled)(gcx, module)
    }

    fn run(&self, gcx: Gcx<'_>, module: &mut M) -> bool {
        (self.run)(gcx, module)
    }
}

fn optimizations_enabled<M>(gcx: Gcx<'_>, _module: &M) -> bool {
    gcx.sess.opts.optimization != OptimizationMode::None
}

fn always_enabled<M>(_gcx: Gcx<'_>, _module: &M) -> bool {
    true
}

impl<M> std::fmt::Debug for PassFactory<M> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PassFactory")
            .field("name", &self.name)
            .field("description", &self.description)
            .finish_non_exhaustive()
    }
}

/// Runs pass pipelines with IR-specific pass policy supplied by `run`.
pub(crate) struct PassManager<'gcx, M> {
    gcx: Gcx<'gcx>,
    run: for<'a> fn(Gcx<'a>, &mut M, &(dyn Pass<M> + 'static)) -> bool,
}

impl<'gcx, M> PassManager<'gcx, M> {
    /// Creates a pass manager.
    pub(crate) const fn new(
        gcx: Gcx<'gcx>,
        run: for<'a> fn(Gcx<'a>, &mut M, &(dyn Pass<M> + 'static)) -> bool,
    ) -> Self {
        Self { gcx, run }
    }

    /// Runs one pass.
    pub(crate) fn run_pass(&self, module: &mut M, pass: &(dyn Pass<M> + 'static)) -> bool {
        if !pass.is_enabled(self.gcx, module) {
            return false;
        }
        (self.run)(self.gcx, module, pass)
    }

    /// Runs a sequence of passes in order.
    pub(crate) fn run_passes(&self, module: &mut M, passes: &[&(dyn Pass<M> + 'static)]) -> bool {
        let mut changed = false;
        for &pass in passes {
            changed |= self.run_pass(module, pass);
        }
        changed
    }
}

/// Finds a pass by its command-line name.
pub(crate) fn find_pass<'a, M>(
    passes: &'a [&(dyn Pass<M> + 'static)],
    name: &str,
) -> Option<&'a (dyn Pass<M> + 'static)> {
    passes.iter().copied().find(|pass| pass.name() == name)
}

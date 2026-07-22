//! MIR pass execution, following rustc's MIR pass manager.

use crate::{
    mir::{MirPhase, Module, validate},
    timing::PassTimer,
};
use solar_config::OptimizationMode;
use solar_interface::diagnostics::DiagCtxt;
use solar_sema::Gcx;

// `foo::Bar<'a>` becomes `Bar`, matching rustc's default MIR pass naming.
const fn simplify_pass_type_name(name: &'static str) -> &'static str {
    let bytes = name.as_bytes();
    let mut i = bytes.len();
    while i > 0 && bytes[i - 1] != b':' {
        i -= 1;
    }
    let (_, bytes) = bytes.split_at(i);

    let mut i = 0;
    while i < bytes.len() && bytes[i] != b'<' {
        i += 1;
    }
    let (bytes, _) = bytes.split_at(i);

    match std::str::from_utf8(bytes) {
        Ok(name) => name,
        Err(_) => panic!(),
    }
}

/// A streamlined trait for a MIR transformation pass.
pub trait MirPass: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str {
        simplify_pass_type_name(std::any::type_name::<Self>())
    }

    /// Returns whether this pass is enabled with the current compiler flags and MIR phase.
    fn is_enabled(&self, _gcx: Gcx<'_>, _module: &Module) -> bool {
        true
    }

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }

    /// Runs the pass and returns whether it changed MIR.
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool;
}

/// Whether to allow non-required optimizations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Optimizations {
    /// Suppress passes that are not required for correctness.
    Suppressed,
    /// Allow optimization passes to run.
    Allowed,
}

impl Optimizations {
    /// Selects optimization suppression from the global compiler context.
    pub fn for_gcx(gcx: Gcx<'_>) -> Self {
        if matches!(gcx.sess.opts.optimization, OptimizationMode::None) {
            Self::Suppressed
        } else {
            Self::Allowed
        }
    }
}

/// Returns whether `pass` should run for this module and optimization mode.
pub(crate) fn should_run_pass<P>(
    gcx: Gcx<'_>,
    module: &Module,
    pass: &P,
    optimizations: Optimizations,
) -> bool
where
    P: MirPass + ?Sized,
{
    let suppressed = !pass.is_required() && matches!(optimizations, Optimizations::Suppressed);
    !suppressed && pass.is_enabled(gcx, module)
}

/// Runs a sequence of MIR passes without validating after each pass.
pub fn run_passes_no_validate(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
) -> bool {
    run_passes_inner(gcx, module, passes, phase_change, false, Optimizations::Allowed)
}

/// Runs a sequence of MIR passes, then applies `phase_change` when present.
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    optimizations: Optimizations,
) -> bool {
    run_passes_inner(gcx, module, passes, phase_change, true, optimizations)
}

fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    validate_each: bool,
    optimizations: Optimizations,
) -> bool {
    let mut changed = false;
    for pass in passes {
        let pass_name = pass.name();
        if !should_run_pass(gcx, module, *pass, optimizations) {
            continue;
        }

        let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
        let pass_changed = pass.run_pass(gcx, module);
        timer.finish("MIR", module.name, pass_name, pass_changed);
        changed |= pass_changed;

        if validate_each && cfg!(debug_assertions) {
            validate_module_after_pass(module, pass_name);
        }
        if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {} (after {pass_name}) ===", module.name);
            print!("{}", module.to_text());
        }
    }

    if let Some(new_phase) = phase_change {
        assert!(
            module.phase < new_phase,
            "invalid MIR phase transition from {} to {}",
            module.phase.name(),
            new_phase.name()
        );
        module.advance_phase(new_phase);
        if cfg!(debug_assertions) {
            validate_module_after_pass(module, new_phase.name());
        }
    }

    changed
}

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let dcx = DiagCtxt::new_early();
    validate(&dcx, module);
    if dcx.has_errors().is_err() {
        panic!("MIR validation failed after `{pass_name}`");
    }
}

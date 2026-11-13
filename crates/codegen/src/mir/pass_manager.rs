//! MIR pass execution, following rustc's MIR pass manager.
//!
//! Within one pipeline run, an exact pass instance that reported no change is
//! skipped until another pass mutates the module. Repeating a deterministic
//! transform on identical MIR cannot produce a new result, while avoiding that
//! work matters for the deliberately iterative optimization pipeline. The
//! cache stays module-wide because function-local results can depend on callee
//! summaries and other module state.

use crate::{
    mir::{Module, analysis::HotnessAnalysis, pass::ModuleAnalyses, should_validate_ir, validate},
    timing::PassTimer,
};
use solar_config::OptimizationMode;
use solar_data_structures::fmt::line_diff;
use solar_interface::{
    Result,
    diagnostics::{DiagCtxt, ErrorGuaranteed},
};
use solar_sema::Gcx;
use std::fmt::Display;

pub(crate) fn parse_pass_pipeline<P: Copy>(
    gcx: Gcx<'_>,
    value: &str,
    ir: &str,
    lookup: impl Fn(&str) -> Option<P>,
) -> Result<Option<Vec<Option<P>>>> {
    if value == "default" {
        return Ok(None);
    }
    let passes = value
        .split(',')
        .map(|name| match name {
            "none" => Ok(None),
            _ => lookup(name)
                .map(Some)
                .ok_or_else(|| gcx.dcx().err(format!("unknown {ir} pass: {name}")).emit()),
        })
        .collect::<Result<_>>()?;
    Ok(Some(passes))
}

/// Returns the display label for a configured IR pipeline.
pub fn pipeline_label(value: &str) -> &str {
    if value == "default" { "pipeline-default" } else { value }
}

pub(crate) fn pipeline_output_name(gcx: Gcx<'_>, fallback: impl Display) -> String {
    if !gcx.sess.opts.language.is_source()
        && let Some(source) = gcx.sources.first()
    {
        return source.file.name.display().to_string();
    }
    fallback.to_string()
}

pub(crate) fn mir_output_name(gcx: Gcx<'_>, module: &Module) -> String {
    if gcx.sess.opts.language.is_source()
        && let Some(contract_id) = gcx
            .hir
            .contract_ids()
            .find(|&contract_id| gcx.hir.contract(contract_id).name == module.name)
    {
        return gcx.contract_fully_qualified_name(contract_id).to_string();
    }
    pipeline_output_name(gcx, module.name)
}

/// A streamlined trait for a MIR transformation pass.
pub trait MirPass: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str;

    /// Returns whether this pass is enabled with the current compiler flags and MIR phase.
    fn is_enabled(&self, _gcx: Gcx<'_>, module: &Module) -> bool {
        self.covers(module)
    }

    /// Returns whether this pass runs on any of the module's code: a function-local pass on a
    /// function with an objective of its own, a contract-wide pass only on a contract whose own
    /// objective runs it.
    fn covers(&self, module: &Module) -> bool {
        if self.is_contract_wide() {
            self.runs_on(module.optimization)
        } else {
            module.runs_objective(|objective| self.runs_on(objective))
        }
    }

    /// Returns whether this pass rewrites functions together rather than one at a time, so that
    /// it follows the contract's objective, not that of a function with its own.
    fn is_contract_wide(&self) -> bool {
        false
    }

    /// Returns whether this pass runs on code optimized for `objective`: a function-local pass
    /// skips the functions it does not.
    fn runs_on(&self, objective: OptimizationMode) -> bool {
        self.is_required() || !objective.is_none()
    }

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }

    /// Cache only stateless optional passes, keyed by concrete type and pipeline name.
    /// Configured or required passes must run unless they provide their own safe cache.
    fn cache_key(&self) -> Option<(&'static str, &'static str)> {
        (!self.is_required() && std::mem::size_of_val(self) == 0)
            .then_some((std::any::type_name::<Self>(), self.name()))
    }

    /// Runs the pass and returns whether the module changed.
    ///
    /// Report an emitted error with [`ModuleAnalyses::fail`] to stop this pipeline.
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool;

    /// Returns whether names and instruction IDs still identify the same instructions after
    /// this pass. Debugify only certifies explicit drops when the pass preserves those sites;
    /// replacing a function can reuse its name and restart its instruction IDs.
    fn preserves_debugify_sites(&self) -> bool {
        false
    }
}

/// Runs a sequence of MIR passes, checking each changed result when verification is enabled.
#[must_use]
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    name: Option<&str>,
) -> bool {
    run_passes_inner(gcx, module, passes, name).0
}

#[must_use]
pub(crate) fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    name: Option<&str>,
) -> (bool, Option<ErrorGuaranteed>) {
    let output_name = if gcx.sess.opts.unstable.pass_diff || gcx.sess.opts.unstable.print_after_each
    {
        name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module))
    } else {
        String::new()
    };
    let explicit = name.is_some();
    let mut changed = false;
    let mut analyses = ModuleAnalyses::default();
    let mut unchanged = Vec::new();
    // Parsed or lowered MIR carries no trustworthy hotness.
    let mut hotness = HotnessAnalysis::new();
    hotness.run(gcx, module);
    if crate::debugify::enabled(gcx)
        && let Err(error) = crate::debugify::report_mir_input(gcx, module)
    {
        return (changed, Some(error));
    }
    for pass in passes {
        let pass_name = pass.name();
        let before =
            (explicit && gcx.sess.opts.unstable.pass_diff).then(|| module.to_text().to_string());
        let enabled = pass.is_enabled(gcx, module);
        if !enabled && !explicit {
            continue;
        }

        if enabled {
            let origins =
                crate::debugify::enabled(gcx).then(|| crate::debugify::Origins::mir(module));
            let key = pass.cache_key();
            let cached = key.is_some_and(|key| unchanged.contains(&key));
            let tv = crate::solar_tv::MirSnapshot::capture(gcx, cached, module);
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let pass_changed = if cached {
                false
            } else {
                // A queued contract job needs the workers more than this pass does.
                let parallel = gcx.sess.is_parallel_codegen() && !gcx.sess.has_queued_jobs();
                analyses.begin_pass(
                    module.runs_objective(OptimizationMode::is_gas),
                    parallel,
                    [OptimizationMode::None, OptimizationMode::Gas, OptimizationMode::Size]
                        .map(|objective| pass.runs_on(objective)),
                    module.functions.len(),
                );
                // Translation validation compares each function the pass changes with its
                // version before it.
                let validated = gcx
                    .sess
                    .opts
                    .unstable
                    .validate_passes
                    .as_ref()
                    .map(|dir| (dir, module.clone()));
                let phase = module.phase();
                // A pass that edits the module, reported or not, outdates the check, unless the
                // pass itself advanced the phase after its last edit.
                let lowered_checked = module.take_lowered_check();
                let pass_changed = tracing::trace_span!("mir_pass", pass = pass_name)
                    .in_scope(|| pass.run_pass(gcx, module, &mut analyses));
                module.restore_lowered_check(
                    lowered_checked && !pass_changed && !analyses.unreported_edit,
                );
                analyses.finish_pass(pass_changed);
                if let Err(error) = crate::lean_stages::mir_pass(gcx, pass_name, phase, module) {
                    return (changed | pass_changed, Some(error));
                }
                if pass_changed
                    && let Some((dir, before)) = validated
                    && let Err(error) = crate::mir::transform::validate_passes::write_queries(
                        pass_name,
                        &before,
                        module,
                        dir,
                        gcx.sess.opts.unstable.validate_passes_fault.as_deref(),
                    )
                {
                    let error = gcx
                        .dcx()
                        .err(format!(
                            "cannot write the pass validation queries to `{}`: {error}",
                            dir.display()
                        ))
                        .emit();
                    return (changed | pass_changed, Some(error));
                }
                if pass_changed || analyses.unreported_edit {
                    // Passes rename the storage alias bases they replace, but not the ones they
                    // delete with their last other use.
                    // sstore s, v !metadata(storage=offset(dead, n)) -> sstore s, v
                    for func in module.functions.iter_mut() {
                        func.drop_dangling_storage_aliases();
                    }
                    // Backend memory records describe the code generated for the MIR as it was.
                    module.backend_memory = None;
                    match analyses.changed_functions(module.functions.len()) {
                        Some(changed) => hotness.update(gcx, module, changed),
                        None => hotness.run(gcx, module),
                    }
                    debug_assert!(
                        hotness.is_current(gcx, module),
                        "stale MIR hotness after `{pass_name}`"
                    );
                }
                pass_changed
            };
            if pass_changed {
                unchanged.clear();
            } else if !cached && let Some(key) = key {
                unchanged.push(key);
            }
            timer.finish("MIR", module.name, pass_name, pass_changed);
            changed |= pass_changed;
            if analyses.error.is_some() {
                return (changed, analyses.error);
            }
            if let Some(tv) = tv
                && let Err(error) = tv.check(gcx, pass_name, module)
            {
                return (changed, Some(error));
            }
            if let Some(origins) = origins
                && let Err(error) = origins.check(
                    gcx,
                    "MIR",
                    module.name,
                    pass_name,
                    crate::debugify::Origins::mir_after(module, pass.preserves_debugify_sites()),
                )
            {
                return (changed, Some(error));
            }

            if pass_changed && should_validate_ir(gcx.sess) {
                validate_module_after_pass(module, pass_name);
            }
        }

        if let Some(before) = before {
            print_pass_diff(&output_name, pass_name, before, module.to_text());
        } else if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("; === {output_name} (after {pass_name}) ===");
            print!("{}", module.to_text());
        }
    }

    (changed, None)
}

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let dcx = DiagCtxt::new_early();
    validate(&dcx, module);
    if dcx.has_errors().is_err() {
        panic!("MIR validation failed after `{pass_name}`");
    }
}

pub(crate) fn print_pass_diff(
    name: impl Display,
    pass: impl Display,
    before: impl Display,
    after: impl Display,
) {
    let before = format!("; === {name} (before {pass}) ===\n{before}");
    let after = format!("; === {name} (after {pass}) ===\n{after}");
    print!("{}", line_diff(&before, &after));
}

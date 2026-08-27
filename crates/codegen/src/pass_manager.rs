//! MIR pass execution, following rustc's MIR pass manager.

use crate::{
    mir::{MirPhase, Module, validate},
    pass::ModuleAnalyses,
    timing::{PassTimer, StageId},
};
use solar_config::OptimizationMode;
use solar_data_structures::{fmt::line_diff, map::FxHashMap};
use solar_interface::{Result, diagnostics::DiagCtxt};
use solar_sema::Gcx;
use std::{
    fmt::Display,
    sync::atomic::{AtomicUsize, Ordering},
};

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

pub(crate) struct PipelineState {
    pipeline_run: usize,
    invocations: FxHashMap<&'static str, usize>,
}

static NEXT_PIPELINE_RUN: AtomicUsize = AtomicUsize::new(1);

impl Default for PipelineState {
    fn default() -> Self {
        Self {
            pipeline_run: NEXT_PIPELINE_RUN.fetch_add(1, Ordering::Relaxed),
            invocations: FxHashMap::default(),
        }
    }
}

impl PipelineState {
    pub(crate) fn pipeline_run(&self) -> usize {
        self.pipeline_run
    }

    pub(crate) fn next_invocation(&mut self, pass: &'static str) -> usize {
        let invocation = self.invocations.entry(pass).or_default();
        *invocation += 1;
        *invocation
    }
}

/// A streamlined trait for a MIR transformation pass.
pub trait MirPass: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str {
        simplify_pass_type_name(std::any::type_name::<Self>())
    }

    /// Returns whether this pass is enabled with the current compiler flags and MIR phase.
    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        self.is_required() || !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }

    /// Returns the phase an enabled, successful pass must produce.
    fn output_phase(&self) -> Option<MirPhase> {
        None
    }

    /// Runs the pass and returns whether it changed MIR.
    #[must_use]
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool;
}

/// Runs a sequence of MIR passes without validating after each pass.
#[must_use]
pub fn run_passes_no_validate(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
) -> bool {
    let mut state = PipelineState::default();
    run_passes_inner(
        gcx,
        module,
        passes,
        phase_change,
        false,
        None,
        false,
        StageId::new("custom", 1),
        &mut state,
    )
}

/// Runs a sequence of MIR passes and checks `expected_phase` when present.
///
/// Representation-lowering passes own phase transitions. The manager never
/// claims a phase on behalf of an incomplete transform.
#[must_use]
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    expected_phase: Option<MirPhase>,
    name: Option<&str>,
) -> bool {
    let mut state = PipelineState::default();
    run_passes_inner(
        gcx,
        module,
        passes,
        expected_phase,
        true,
        name,
        true,
        StageId::new("custom", 1),
        &mut state,
    )
}

/// Runs one named stage in the canonical MIR pipeline.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_stage(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    output_name: &str,
    stage: StageId,
    checkpoint: &'static str,
    checkpoint_phase: MirPhase,
    state: &mut PipelineState,
) -> bool {
    let changed =
        run_stage_passes(gcx, module, passes, phase_change, Some(output_name), false, stage, state);
    if gcx.sess.opts.unstable.print_after_stage
        && gcx.dcx().has_errors().is_ok()
        && module.phase >= checkpoint_phase
    {
        print_checkpoint(
            output_name,
            "MIR",
            None,
            state.pipeline_run(),
            stage,
            checkpoint,
            module.to_text(),
        );
    }
    changed
}

/// Runs passes with a shared stage and invocation state without printing a checkpoint.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub(crate) fn run_stage_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    name: Option<&str>,
    explicit: bool,
    stage: StageId,
    state: &mut PipelineState,
) -> bool {
    run_passes_inner(gcx, module, passes, phase_change, true, name, explicit, stage, state)
}

#[must_use]
#[allow(clippy::too_many_arguments)]
fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn MirPass],
    phase_change: Option<MirPhase>,
    validate_each: bool,
    name: Option<&str>,
    explicit: bool,
    stage: StageId,
    state: &mut PipelineState,
) -> bool {
    let output_name = name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module));
    let mut changed = false;
    let mut analyses = ModuleAnalyses::default();
    for (pass_index, pass) in passes.iter().enumerate() {
        let pass_name = pass.name();
        let enabled = pass.is_enabled(gcx, module);
        if !enabled && !explicit {
            continue;
        }
        let invocation = state.next_invocation(pass_name);
        let pass_diff =
            gcx.sess.opts.unstable.pass_diff && !gcx.sess.opts.unstable.print_after_stage;
        let inspect_change = gcx.sess.opts.unstable.time_passes
            || pass_diff
            || gcx.sess.opts.unstable.print_after_each;
        let before = inspect_change.then(|| module.to_text().to_string());
        let phase_before = module.phase;
        let mut ir_changed = false;
        let mut state_changed = false;
        let mut failed = false;

        if enabled {
            let errors_before = gcx.dcx().err_count();
            analyses.begin_pass();
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let pass_changed = pass.run_pass(gcx, module, &mut analyses);
            let timer = timer.stop();
            state_changed = phase_before != module.phase;
            if gcx.dcx().err_count() == errors_before
                && pass_index + 1 == passes.len()
                && let Some(expected_phase) = phase_change
                && module.phase != expected_phase
            {
                gcx.dcx()
                    .err(format!(
                        "MIR pipeline stopped at `{}`, expected `{}`",
                        module.phase.name(),
                        expected_phase.name()
                    ))
                    .emit();
            }
            failed = gcx.dcx().err_count() != errors_before;
            let after = before.as_ref().map(|_| module.to_text().to_string());
            ir_changed = match (&before, &after) {
                (Some(before), Some(after)) => !mir_body(before).eq(mir_body(after)),
                _ => pass_changed && !state_changed,
            };
            timer.finish(
                "MIR",
                &output_name,
                None,
                state.pipeline_run(),
                stage,
                pass_name,
                invocation,
                ir_changed,
                state_changed,
                if failed { "failed" } else { "ok" },
            );
            analyses.finish_pass(pass_changed);
            changed |= ir_changed || state_changed;

            if !failed && validate_each && cfg!(debug_assertions) {
                validate_module_after_pass(module, pass_name);
            }
        }

        if pass_diff {
            print_pass_diff(
                &output_name,
                "MIR",
                None,
                state.pipeline_run(),
                stage,
                pass_name,
                invocation,
                ir_changed,
                state_changed,
                enabled,
                if failed {
                    "failed"
                } else if enabled {
                    "ok"
                } else {
                    "skipped"
                },
                before.as_deref().unwrap_or_default(),
                module.to_text(),
            );
        } else if gcx.sess.opts.unstable.print_after_each {
            print_pass_output(
                &output_name,
                "MIR",
                None,
                state.pipeline_run(),
                stage,
                pass_name,
                invocation,
                enabled,
                if failed {
                    "failed"
                } else if enabled {
                    "ok"
                } else {
                    "skipped"
                },
                ir_changed,
                state_changed,
                module.to_text(),
            );
        }
        if gcx.dcx().has_errors().is_err() {
            break;
        }
    }

    if gcx.dcx().has_errors().is_ok()
        && let Some(expected_phase) = phase_change
        && module.phase != expected_phase
    {
        gcx.dcx()
            .err(format!(
                "MIR pipeline stopped at `{}`, expected `{}`",
                module.phase.name(),
                expected_phase.name()
            ))
            .emit();
    }

    changed
}

fn mir_body(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim_start().starts_with("@phase "))
}

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let dcx = DiagCtxt::new_early();
    validate(&dcx, module);
    if dcx.has_errors().is_err() {
        panic!("MIR validation failed after `{pass_name}`");
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn print_pass_diff(
    name: impl Display,
    ir: &str,
    artifact: Option<&str>,
    pipeline_run: usize,
    stage: StageId,
    pass: impl Display,
    invocation: usize,
    ir_changed: bool,
    state_changed: bool,
    executed: bool,
    outcome: &str,
    before: impl Display,
    after: impl Display,
) {
    let marker = if ir_changed || state_changed { '+' } else { ' ' };
    let name = name.to_string();
    let artifact = artifact.unwrap_or("-");
    println!(
        "{marker} // === {name} (after {pass}) [ir={ir} module={name:?} artifact={artifact} \
         pipeline_run={pipeline_run} stage={} round={} invocation={invocation} \
         executed={executed} outcome={outcome} ir_changed={ir_changed} \
         state_changed={state_changed}] ===",
        stage.name, stage.round
    );
    let before = before.to_string();
    let after = after.to_string();
    print!("{}", line_diff(&before, &after));
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn print_pass_output(
    name: impl Display,
    ir: &str,
    artifact: Option<&str>,
    pipeline_run: usize,
    stage: StageId,
    pass: impl Display,
    invocation: usize,
    executed: bool,
    outcome: &str,
    ir_changed: bool,
    state_changed: bool,
    text: impl Display,
) {
    let name = name.to_string();
    let artifact = artifact.unwrap_or("-");
    println!(
        "// === {name} (after {pass}) [ir={ir} module={name:?} artifact={artifact} \
         pipeline_run={pipeline_run} stage={} round={} invocation={invocation} \
         executed={executed} outcome={outcome} ir_changed={ir_changed} \
         state_changed={state_changed}] ===",
        stage.name, stage.round
    );
    print!("{text}");
}

pub(crate) fn print_checkpoint(
    name: impl Display,
    ir: &str,
    artifact: Option<&str>,
    pipeline_run: usize,
    stage: StageId,
    checkpoint: &str,
    text: impl Display,
) {
    let name = name.to_string();
    let artifact = artifact.unwrap_or("-");
    println!(
        "// === {name} [ir={ir} module={name:?} artifact={artifact} \
         pipeline_run={pipeline_run} stage={} round={} checkpoint={checkpoint}] ===",
        stage.name, stage.round
    );
    print!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_runs_have_distinct_ids() {
        let first = PipelineState::default();
        let retry = PipelineState::default();
        assert_ne!(first.pipeline_run(), retry.pipeline_run());
    }
}

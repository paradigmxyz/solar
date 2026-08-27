//! MIR pass execution, following rustc's MIR pass manager.

use crate::{
    mir::{
        Module, validate_for_evm, validate_phase_transition_for_evm, validate_structure_for_evm,
    },
    pass::ModuleAnalyses,
    timing::{PassInfo, PassOutcome, PassTimer},
};
use solar_config::OptimizationMode;
use solar_data_structures::{fmt::line_diff, map::FxHashMap};
use solar_interface::Result;
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
    observed: bool,
    observes_passes: bool,
}

static NEXT_PIPELINE_RUN: AtomicUsize = AtomicUsize::new(1);

impl PipelineState {
    pub(crate) fn new(observed: bool, observes_passes: bool) -> Self {
        Self {
            pipeline_run: if observed {
                NEXT_PIPELINE_RUN.fetch_add(1, Ordering::Relaxed)
            } else {
                0
            },
            invocations: FxHashMap::default(),
            observed,
            observes_passes,
        }
    }

    pub(crate) fn pipeline_run(&self) -> usize {
        self.pipeline_run
    }

    pub(crate) fn observed(&self) -> bool {
        self.observed
    }

    pub(crate) fn observes_passes(&self) -> bool {
        self.observes_passes
    }

    pub(crate) fn next_invocation(&mut self, pass: &'static str) -> usize {
        if !self.observes_passes {
            return 0;
        }
        let invocation = self.invocations.entry(pass).or_default();
        *invocation += 1;
        *invocation
    }
}

pub(crate) fn observes_pipeline(gcx: Gcx<'_>) -> bool {
    observes_passes(gcx) || gcx.sess.opts.unstable.print_after_stage
}

pub(crate) fn observes_passes(gcx: Gcx<'_>) -> bool {
    gcx.sess.opts.unstable.time_passes
        || (gcx.sess.opts.unstable.pass_diff && !gcx.sess.opts.unstable.print_after_stage)
        || gcx.sess.opts.unstable.print_after_each
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

    /// Runs the pass and returns whether it changed the MIR body.
    #[must_use]
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool;
}

/// Executes every MIR pass through one shared path.
pub(crate) struct MirPassManager {
    output_name: Option<String>,
    state: PipelineState,
    analyses: ModuleAnalyses,
    failed: bool,
}

impl MirPassManager {
    pub(crate) fn new(gcx: Gcx<'_>, module: &Module, name: Option<&str>) -> Self {
        let state = PipelineState::new(observes_pipeline(gcx), observes_passes(gcx));
        Self {
            output_name: state.observed().then(|| {
                name.map(ToOwned::to_owned).unwrap_or_else(|| mir_output_name(gcx, module))
            }),
            state,
            analyses: ModuleAnalyses::default(),
            failed: false,
        }
    }

    /// Runs one sequence of passes with shared analyses and observability state.
    #[must_use]
    pub(crate) fn run_passes<'a>(
        &mut self,
        gcx: Gcx<'_>,
        module: &mut Module,
        passes: impl IntoIterator<Item = Option<&'a dyn MirPass>>,
        explicit: bool,
        stage: &'static str,
    ) -> bool {
        let mut changed = false;
        for pass in passes {
            let pass_name = pass.map_or("none", MirPass::name);
            let enabled = pass.is_some_and(|pass| pass.is_enabled(gcx, module));
            if !enabled && !explicit {
                continue;
            }
            let invocation = self.state.next_invocation(pass_name);
            let pass_diff =
                gcx.sess.opts.unstable.pass_diff && !gcx.sess.opts.unstable.print_after_stage;
            let print_after = pass_diff || gcx.sess.opts.unstable.print_after_each;
            let before = pass_diff.then(|| module.to_text().to_string());
            let phase_before = module.phase;
            let mut ir_changed = false;
            let mut state_changed = false;
            let mut failed = false;
            let mut has_errors = false;
            let mut after = None;
            let mut timer = None;

            if let Some(pass) = pass.filter(|_| enabled) {
                let errors_before = self.state.observes_passes().then(|| gcx.dcx().err_count());
                self.analyses.begin_pass();
                let pass_timer = gcx.sess.opts.unstable.time_passes.then(PassTimer::new);
                let pass_changed = pass.run_pass(gcx, module, &mut self.analyses);
                timer = pass_timer.map(PassTimer::stop);
                state_changed = phase_before != module.phase;
                if let Some(errors_before) = errors_before {
                    let errors_after = gcx.dcx().err_count();
                    failed = errors_after != errors_before;
                    has_errors = errors_after != 0;
                }
                after = print_after.then(|| module.to_text().to_string());
                ir_changed = match (&before, &after) {
                    (Some(before), Some(after)) => !mir_body(before).eq(mir_body(after)),
                    _ => pass_changed,
                };
                self.analyses.finish_pass(pass_changed);
                changed |= ir_changed || state_changed;
            }

            if after.is_none() && print_after {
                after = Some(module.to_text().to_string());
            }

            if timer.is_some() || pass_diff || gcx.sess.opts.unstable.print_after_each {
                let info = PassInfo {
                    ir: "MIR",
                    module: self.output_name.as_deref().unwrap_or_default(),
                    artifact: None,
                    pipeline_run: self.state.pipeline_run(),
                    stage,
                    pass: pass_name,
                    invocation,
                    outcome: PassOutcome::new(enabled, failed),
                    ir_changed,
                    state_changed,
                };
                if let Some(timer) = timer {
                    timer.finish(info);
                }
                if pass_diff {
                    print_pass_diff(
                        info,
                        before.as_deref().unwrap_or_default(),
                        after.as_deref().unwrap_or_default(),
                    );
                } else if gcx.sess.opts.unstable.print_after_each {
                    print_pass_output(info, after.as_deref().unwrap_or_default());
                }
            }
            if has_errors {
                self.failed = true;
                break;
            }
        }
        changed
    }

    pub(crate) fn failed(&self) -> bool {
        self.failed
    }

    pub(crate) fn validate(&self, gcx: Gcx<'_>, module: &Module) {
        validate_for_evm(gcx.dcx(), module, gcx.sess.opts.evm_version);
    }

    pub(crate) fn validate_structure(&self, gcx: Gcx<'_>, module: &Module) {
        validate_structure_for_evm(gcx.dcx(), module, gcx.sess.opts.evm_version);
    }

    pub(crate) fn validate_phase_transition(&self, gcx: Gcx<'_>, module: &Module) {
        validate_phase_transition_for_evm(gcx.dcx(), module, gcx.sess.opts.evm_version);
    }

    pub(crate) fn print_checkpoint(
        &self,
        gcx: Gcx<'_>,
        module: &Module,
        stage: &'static str,
        checkpoint: impl Display,
    ) {
        if gcx.sess.opts.unstable.print_after_stage {
            print_checkpoint(
                self.output_name.as_deref().unwrap_or_default(),
                "MIR",
                None,
                self.state.pipeline_run(),
                stage,
                checkpoint,
                module.to_text(),
            );
        }
    }
}

fn mir_body(text: &str) -> impl Iterator<Item = &str> {
    text.lines().filter(|line| !line.trim_start().starts_with("@phase "))
}

pub(crate) fn print_pass_diff(info: PassInfo<'_>, before: &str, after: &str) {
    let marker = if info.ir_changed || info.state_changed { '+' } else { ' ' };
    print_pass_header(info, Some(marker));
    print!("{}", line_diff(before, after));
}

pub(crate) fn print_pass_output(info: PassInfo<'_>, text: impl Display) {
    print_pass_header(info, None);
    print!("{text}");
}

fn print_pass_header(info: PassInfo<'_>, marker: Option<char>) {
    if let Some(marker) = marker {
        print!("{marker} ");
    }
    let artifact = info.artifact.unwrap_or("-");
    println!(
        "// === {} (after {}) [ir={} module={:?} artifact={artifact} pipeline_run={} stage={} \
         invocation={} executed={} outcome={} ir_changed={} state_changed={}] ===",
        info.module,
        info.pass,
        info.ir,
        info.module,
        info.pipeline_run,
        info.stage,
        info.invocation,
        info.outcome.executed(),
        info.outcome,
        info.ir_changed,
        info.state_changed,
    );
}

pub(crate) fn print_checkpoint(
    name: impl Display,
    ir: &str,
    artifact: Option<&str>,
    pipeline_run: usize,
    stage: &'static str,
    checkpoint: impl Display,
    text: impl Display,
) {
    let name = name.to_string();
    let artifact = artifact.unwrap_or("-");
    println!(
        "// === {name} [ir={ir} module={name:?} artifact={artifact} \
         pipeline_run={pipeline_run} stage={stage} checkpoint={checkpoint}] ==="
    );
    print!("{text}");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pipeline_runs_have_distinct_ids() {
        let first = PipelineState::new(true, true);
        let retry = PipelineState::new(true, true);
        assert_ne!(first.pipeline_run(), retry.pipeline_run());
    }
}

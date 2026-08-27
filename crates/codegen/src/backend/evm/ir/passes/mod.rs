//! EVM IR optimization and layout passes.
//!
//! This module owns the pass list and canonical backend pipeline. Individual
//! transforms live in their own modules so their implementation and invariants
//! remain local, matching the organization of the MIR transforms.

mod block_cse;
mod block_layout;
mod cfg_simplify;
mod coalesce_copies;
pub(in crate::backend::evm) mod compact_pushes;
mod constant_data;
mod dce;
mod legalize_shifts;
mod outline;
mod peephole;
mod share_reverts;
mod tail_merge;
mod terminal_dedup;
pub(super) mod utils;

pub(in crate::backend::evm) use legalize_shifts::LEGACY_SHIFT_STACK_HEADROOM;

use super::Module;
use crate::{
    pass_manager::{
        PipelineState, observes_passes, observes_pipeline, parse_pass_pipeline,
        pipeline_output_name, print_checkpoint, print_pass_diff, print_pass_output,
    },
    timing::{PassInfo, PassOutcome, PassTimer},
};
use solar_config::OptimizationMode;
use solar_sema::Gcx;

pub use crate::pass_manager::pipeline_label;

/// A streamlined trait for an EVM IR transformation pass.
pub trait EvmPass: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str;

    /// Returns whether this pass is enabled with the current compiler flags.
    fn is_enabled(&self, gcx: Gcx<'_>, _module: &Module) -> bool {
        self.is_required() || !matches!(gcx.sess.opts.optimization, OptimizationMode::None)
    }

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }

    /// Runs the pass and returns whether it changed EVM IR.
    #[must_use]
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool;
}

/// All EVM IR passes exposed by `-Zevm-ir-pipeline`.
pub static ALL_PASSES: &[&dyn EvmPass] = &[
    &block_cse::BlockCse,
    &peephole::Peephole,
    &share_reverts::ShareReverts,
    &compact_pushes::CompactPushes,
    &coalesce_copies::CoalesceCopies,
    &constant_data::ConstantData,
    &dce::Dce,
    &legalize_shifts::LegalizeShifts,
    &cfg_simplify::CfgSimplify,
    &outline::Outline,
    &terminal_dedup::TerminalDedup,
    &tail_merge::TailMerge,
    &block_layout::BlockLayout,
];

static DEFAULT_PIPELINE: &[&dyn EvmPass] = &[
    &legalize_shifts::LegalizeShifts,
    &peephole::Peephole,
    &constant_data::ConstantData,
    &compact_pushes::CompactPushes,
    &cfg_simplify::CfgSimplify,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &outline::Outline,
    &compact_pushes::CompactPushes,
    &block_cse::BlockCse,
    &dce::Dce,
    &peephole::Peephole,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &tail_merge::TailMerge,
    &outline::Outline,
    &compact_pushes::CompactPushes,
    &peephole::Peephole,
    &block_cse::BlockCse,
    &dce::Dce,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
];

/// Finds an EVM IR pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

struct EvmPassManager<'a> {
    output_name: Option<String>,
    artifact: Option<&'a str>,
    state: PipelineState,
}

impl<'a> EvmPassManager<'a> {
    fn new(gcx: Gcx<'_>, module: &Module, name: Option<&str>, artifact: Option<&'a str>) -> Self {
        let state = PipelineState::new(observes_pipeline(gcx), observes_passes(gcx));
        let output_name = state.observed().then(|| {
            name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()))
        });
        Self { output_name, artifact, state }
    }

    fn output_name(&self) -> &str {
        self.output_name.as_deref().unwrap_or_default()
    }
}

/// Runs an EVM IR pass pipeline.
#[must_use]
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    name: Option<&str>,
) -> bool {
    let mut manager = EvmPassManager::new(gcx, module, name, None);
    manager.run_passes(gcx, module, passes.iter().copied().map(Some), true, "custom").changed
}

struct PassRun {
    changed: bool,
    failed: bool,
}

impl EvmPassManager<'_> {
    #[must_use]
    fn run_passes<'a>(
        &mut self,
        gcx: Gcx<'_>,
        module: &mut Module,
        passes: impl IntoIterator<Item = Option<&'a dyn EvmPass>>,
        explicit: bool,
        stage: &'static str,
    ) -> PassRun {
        let mut changed = false;
        let mut pipeline_failed = false;
        for pass in passes {
            let pass_name = pass.map_or("none", EvmPass::name);
            let enabled = pass.is_some_and(|pass| pass.is_enabled(gcx, module));
            if !enabled && !explicit {
                continue;
            }
            let invocation = self.state.next_invocation(pass_name);
            let pass_diff =
                gcx.sess.opts.unstable.pass_diff && !gcx.sess.opts.unstable.print_after_stage;
            let print_after = pass_diff || gcx.sess.opts.unstable.print_after_each;
            let before = pass_diff.then(|| module.to_text().to_string());
            let mut pass_changed = false;
            let mut failed = false;
            let mut has_errors = false;
            let mut after = None;
            let mut timer = None;

            if let Some(pass) = pass.filter(|_| enabled) {
                let errors_before = self.state.observes_passes().then(|| gcx.dcx().err_count());
                let pass_timer = gcx.sess.opts.unstable.time_passes.then(PassTimer::new);
                pass_changed = pass.run_pass(gcx, module);
                timer = pass_timer.map(PassTimer::stop);
                if let Some(errors_before) = errors_before {
                    let errors_after = gcx.dcx().err_count();
                    failed = errors_after != errors_before;
                    has_errors = errors_after != 0;
                } else {
                    has_errors = gcx.dcx().has_errors().is_err();
                }
                after = print_after.then(|| module.to_text().to_string());
                let ir_changed = match (&before, &after) {
                    (Some(before), Some(after)) => before != after,
                    _ => pass_changed,
                };
                pass_changed = ir_changed;
                changed |= ir_changed;
            }
            if after.is_none() && print_after {
                after = Some(module.to_text().to_string());
            }

            if timer.is_some() || pass_diff || gcx.sess.opts.unstable.print_after_each {
                let info = PassInfo {
                    ir: "EVM-IR",
                    module: self.output_name(),
                    artifact: self.artifact,
                    pipeline_run: self.state.pipeline_run(),
                    stage,
                    pass: pass_name,
                    invocation,
                    outcome: PassOutcome::new(enabled, failed),
                    ir_changed: pass_changed,
                    state_changed: false,
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
                pipeline_failed = true;
                break;
            }
        }
        PassRun { changed, failed: pipeline_failed }
    }
}

fn run_default_pipeline(
    gcx: Gcx<'_>,
    module: &mut Module,
    manager: &mut EvmPassManager<'_>,
) -> bool {
    let mut changed = false;
    let stage = "pipeline";
    for (index, &pass) in DEFAULT_PIPELINE.iter().enumerate() {
        let run = manager.run_passes(gcx, module, std::iter::once(Some(pass)), false, stage);
        changed |= run.changed;
        if run.failed {
            return changed;
        }
        if index == 0 {
            manager.print_checkpoint(gcx, module, stage, "evm.target-legal");
        }
    }
    manager.print_checkpoint(gcx, module, stage, "evm.final");
    changed
}

fn run_legalize(gcx: Gcx<'_>, module: &mut Module, manager: &mut EvmPassManager<'_>) -> PassRun {
    let stage = "custom";
    let run = manager.run_passes(
        gcx,
        module,
        std::iter::once(Some(&legalize_shifts::LegalizeShifts as &dyn EvmPass)),
        false,
        stage,
    );
    if !run.failed {
        manager.print_checkpoint(gcx, module, stage, "evm.target-legal");
    }
    run
}

impl EvmPassManager<'_> {
    fn print_checkpoint(
        &self,
        gcx: Gcx<'_>,
        module: &Module,
        stage: &'static str,
        checkpoint: &str,
    ) {
        if gcx.sess.opts.unstable.print_after_stage && gcx.dcx().has_errors().is_ok() {
            print_checkpoint(
                self.output_name(),
                "EVM-IR",
                self.artifact,
                self.state.pipeline_run(),
                stage,
                checkpoint,
                module.to_text(),
            );
        }
    }
}

/// Runs the configured EVM IR pipeline, or the canonical pipeline when none was provided.
///
/// `name` overrides the module name in pass output.
#[must_use]
pub fn run_pipeline(gcx: Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
    run_pipeline_inner(gcx, module, name, None)
}

/// Runs the EVM IR pipeline for one contract bytecode artifact.
#[must_use]
pub(in crate::backend::evm) fn run_pipeline_for_artifact(
    gcx: Gcx<'_>,
    module: &mut Module,
    name: Option<&str>,
    artifact: &str,
) -> bool {
    run_pipeline_inner(gcx, module, name, Some(artifact))
}

fn run_pipeline_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    name: Option<&str>,
    artifact: Option<&str>,
) -> bool {
    super::verify::validate_stack_ops_for_evm_version(gcx.dcx(), module, gcx.sess.opts.evm_version);
    if gcx.dcx().has_errors().is_err() {
        return false;
    }

    let mut manager = EvmPassManager::new(gcx, module, name, artifact);
    manager.print_checkpoint(gcx, module, "input", "evm.scheduled-input");

    let Some(value) = gcx.sess.opts.unstable.evm_ir_pipeline.as_deref() else {
        return run_default_pipeline(gcx, module, &mut manager);
    };
    let pipeline = match parse_pass_pipeline(gcx, value, "EVM IR", lookup_pass) {
        Ok(pipeline) => pipeline,
        Err(_) => return false,
    };
    let Some(passes) = pipeline else {
        return run_default_pipeline(gcx, module, &mut manager);
    };

    let stage = "custom";
    let run = manager.run_passes(gcx, module, passes, true, stage);
    let mut changed = run.changed;
    if run.failed {
        return changed;
    }
    manager.print_checkpoint(gcx, module, stage, "custom-output");
    let run = run_legalize(gcx, module, &mut manager);
    changed |= run.changed;
    if run.failed {
        return changed;
    }
    changed
}

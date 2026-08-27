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
        PipelineState, observes_pipeline, parse_pass_pipeline, pipeline_output_name,
        print_checkpoint, print_pass_diff, print_pass_output,
    },
    timing::{PassTimer, StageId},
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

struct EvmStage {
    id: StageId,
    checkpoint: &'static str,
    passes: &'static [&'static dyn EvmPass],
}

#[derive(Clone, Copy)]
struct EvmStageRun {
    id: StageId,
    checkpoint: &'static str,
    explicit: bool,
}

static DEFAULT_STAGES: &[EvmStage] = &[
    EvmStage {
        id: StageId::new("normalize", 1),
        checkpoint: "evm.normalized",
        passes: &[
            &peephole::Peephole,
            &constant_data::ConstantData,
            &compact_pushes::CompactPushes,
            &cfg_simplify::CfgSimplify,
            &block_layout::BlockLayout,
            &share_reverts::ShareReverts,
        ],
    },
    EvmStage {
        id: StageId::new("share-structure", 1),
        checkpoint: "evm.structure-shared",
        passes: &[
            &terminal_dedup::TerminalDedup,
            &cfg_simplify::CfgSimplify,
            &tail_merge::TailMerge,
            &cfg_simplify::CfgSimplify,
            &tail_merge::TailMerge,
            &outline::Outline,
        ],
    },
    EvmStage {
        id: StageId::new("regenerate", 1),
        checkpoint: "evm.regenerated",
        passes: &[
            &compact_pushes::CompactPushes,
            &block_cse::BlockCse,
            &dce::Dce,
            &peephole::Peephole,
            &block_layout::BlockLayout,
            &share_reverts::ShareReverts,
        ],
    },
    EvmStage {
        id: StageId::new("share-structure", 2),
        checkpoint: "evm.structure-reshared",
        passes: &[&tail_merge::TailMerge, &outline::Outline],
    },
    EvmStage {
        id: StageId::new("finalize", 1),
        checkpoint: "evm.final",
        passes: &[
            &compact_pushes::CompactPushes,
            &peephole::Peephole,
            &block_cse::BlockCse,
            &dce::Dce,
            &block_layout::BlockLayout,
            &share_reverts::ShareReverts,
        ],
    },
];

/// Finds an EVM IR pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

/// Runs an EVM IR pass pipeline.
#[must_use]
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    name: Option<&str>,
) -> bool {
    let output_name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let mut state = PipelineState::new(observes_pipeline(gcx));
    run_passes_inner(
        gcx,
        module,
        passes.iter().copied().map(Some),
        &output_name,
        None,
        true,
        StageId::new("custom", 1),
        &mut state,
    )
}

#[must_use]
#[allow(clippy::too_many_arguments)]
fn run_passes_inner<'a>(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: impl IntoIterator<Item = Option<&'a dyn EvmPass>>,
    output_name: &str,
    artifact: Option<&str>,
    explicit: bool,
    stage: StageId,
    state: &mut PipelineState,
) -> bool {
    let mut changed = false;
    for pass in passes {
        let pass_name = pass.map_or("none", EvmPass::name);
        let enabled = pass.is_some_and(|pass| pass.is_enabled(gcx, module));
        if !enabled && !explicit {
            continue;
        }
        let invocation = state.next_invocation(pass_name);
        let pass_diff =
            gcx.sess.opts.unstable.pass_diff && !gcx.sess.opts.unstable.print_after_stage;
        let inspect_change = pass_diff
            || gcx.sess.opts.unstable.print_after_each
            || (enabled && gcx.sess.opts.unstable.time_passes);
        let before = inspect_change.then(|| module.to_text().to_string());
        let mut pass_changed = false;
        let mut failed = false;
        let mut after = None;

        if let Some(pass) = pass.filter(|_| enabled) {
            let errors_before = gcx.dcx().err_count();
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            pass_changed = pass.run_pass(gcx, module);
            let timer = timer.stop();
            failed = gcx.dcx().err_count() != errors_before;
            after = inspect_change.then(|| module.to_text().to_string());
            let ir_changed = match (&before, &after) {
                (Some(before), Some(after)) => before != after,
                _ => pass_changed,
            };
            timer.finish(
                "EVM-IR",
                output_name,
                artifact,
                state.pipeline_run(),
                stage,
                pass_name,
                invocation,
                ir_changed,
                false,
                if failed { "failed" } else { "ok" },
            );
            pass_changed = ir_changed;
            changed |= ir_changed;
        }
        if after.is_none() && (pass_diff || gcx.sess.opts.unstable.print_after_each) {
            after = Some(module.to_text().to_string());
        }

        if pass_diff {
            print_pass_diff(
                output_name,
                "EVM-IR",
                artifact,
                state.pipeline_run(),
                stage,
                pass_name,
                invocation,
                pass_changed,
                false,
                enabled,
                if failed {
                    "failed"
                } else if enabled {
                    "ok"
                } else {
                    "skipped"
                },
                before.as_deref().unwrap_or_default(),
                after.as_deref().unwrap_or_default(),
            );
        } else if gcx.sess.opts.unstable.print_after_each {
            print_pass_output(
                output_name,
                "EVM-IR",
                artifact,
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
                pass_changed,
                false,
                after.as_deref().unwrap_or_default(),
            );
        }
        if gcx.dcx().has_errors().is_err() {
            break;
        }
    }
    changed
}

fn run_default_pipeline(
    gcx: Gcx<'_>,
    module: &mut Module,
    output_name: &str,
    artifact: Option<&str>,
    state: &mut PipelineState,
) -> bool {
    let mut changed = false;
    changed |= run_legalize_stage(gcx, module, output_name, artifact, state);
    if gcx.dcx().has_errors().is_err() {
        return changed;
    }
    for stage in DEFAULT_STAGES {
        changed |= run_stage(
            gcx,
            module,
            stage.passes.iter().copied().map(Some),
            output_name,
            artifact,
            EvmStageRun { id: stage.id, checkpoint: stage.checkpoint, explicit: false },
            state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
    }
    changed
}

fn run_legalize_stage(
    gcx: Gcx<'_>,
    module: &mut Module,
    output_name: &str,
    artifact: Option<&str>,
    state: &mut PipelineState,
) -> bool {
    run_stage(
        gcx,
        module,
        std::iter::once(Some(&legalize_shifts::LegalizeShifts as &dyn EvmPass)),
        output_name,
        artifact,
        EvmStageRun {
            id: StageId::new("legalize-target", 1),
            checkpoint: "evm.target-legal",
            explicit: false,
        },
        state,
    )
}

fn run_stage(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: impl IntoIterator<Item = Option<&'static dyn EvmPass>>,
    output_name: &str,
    artifact: Option<&str>,
    stage: EvmStageRun,
    state: &mut PipelineState,
) -> bool {
    let changed = run_passes_inner(
        gcx,
        module,
        passes,
        output_name,
        artifact,
        stage.explicit,
        stage.id,
        state,
    );
    if gcx.dcx().has_errors().is_ok() && gcx.sess.opts.unstable.print_after_stage {
        print_checkpoint(
            output_name,
            "EVM-IR",
            artifact,
            state.pipeline_run(),
            stage.id,
            stage.checkpoint,
            module.to_text(),
        );
    }
    changed
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

    let output_name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let mut state = PipelineState::new(observes_pipeline(gcx));
    if gcx.sess.opts.unstable.print_after_stage {
        print_checkpoint(
            &output_name,
            "EVM-IR",
            artifact,
            state.pipeline_run(),
            StageId::new("input", 1),
            "evm.scheduled-input",
            module.to_text(),
        );
    }

    let Some(value) = gcx.sess.opts.unstable.evm_ir_pipeline.as_deref() else {
        return run_default_pipeline(gcx, module, &output_name, artifact, &mut state);
    };
    let pipeline = match parse_pass_pipeline(gcx, value, "EVM IR", lookup_pass) {
        Ok(pipeline) => pipeline,
        Err(_) => return false,
    };
    let Some(passes) = pipeline else {
        return run_default_pipeline(gcx, module, &output_name, artifact, &mut state);
    };

    let stage = StageId::new("custom", 1);
    let mut changed = run_stage(
        gcx,
        module,
        passes,
        &output_name,
        artifact,
        EvmStageRun { id: stage, checkpoint: "custom-output", explicit: true },
        &mut state,
    );
    if gcx.dcx().has_errors().is_err() {
        return changed;
    }
    changed |= run_legalize_stage(gcx, module, &output_name, artifact, &mut state);
    if gcx.dcx().has_errors().is_err() {
        return changed;
    }
    changed
}

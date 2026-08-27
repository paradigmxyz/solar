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
        PipelineState, parse_pass_pipeline, pipeline_output_name, print_checkpoint,
        print_pass_diff, print_pass_output,
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

/// Normalizes local instructions and establishes the first physical layout.
static NORMALIZE_PASSES: &[&dyn EvmPass] = &[
    &peephole::Peephole,
    &constant_data::ConstantData,
    &compact_pushes::CompactPushes,
    &cfg_simplify::CfgSimplify,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
];

/// Canonicalizes and shares the explicit control-flow graph to a measured bound.
static SHARE_STRUCTURE_PASSES: &[&dyn EvmPass] = &[
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &outline::Outline,
];

/// Regenerates local stack code after global structure stops moving.
static REGENERATE_PASSES: &[&dyn EvmPass] = &[
    &compact_pushes::CompactPushes,
    &block_cse::BlockCse,
    &dce::Dce,
    &peephole::Peephole,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
];

/// Rechecks global sharing exposed by local regeneration.
static RESHARE_STRUCTURE_PASSES: &[&dyn EvmPass] = &[&tail_merge::TailMerge, &outline::Outline];

/// Finalizes local code and address-sensitive placement.
static FINALIZE_PASSES: &[&dyn EvmPass] = &[
    &compact_pushes::CompactPushes,
    &peephole::Peephole,
    &block_cse::BlockCse,
    &dce::Dce,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
];

struct EvmStage {
    id: StageId,
    checkpoint: &'static str,
    passes: &'static [&'static dyn EvmPass],
}

static DEFAULT_STAGES: &[EvmStage] = &[
    EvmStage {
        id: StageId::new("normalize", 1),
        checkpoint: "evm.normalized",
        passes: NORMALIZE_PASSES,
    },
    EvmStage {
        id: StageId::new("share-structure", 1),
        checkpoint: "evm.structure-shared",
        passes: SHARE_STRUCTURE_PASSES,
    },
    EvmStage {
        id: StageId::new("regenerate", 1),
        checkpoint: "evm.regenerated",
        passes: REGENERATE_PASSES,
    },
    EvmStage {
        id: StageId::new("share-structure", 2),
        checkpoint: "evm.structure-reshared",
        passes: RESHARE_STRUCTURE_PASSES,
    },
    EvmStage { id: StageId::new("finalize", 1), checkpoint: "evm.final", passes: FINALIZE_PASSES },
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
    let mut state = PipelineState::default();
    run_passes_inner(gcx, module, passes, name, None, true, StageId::new("custom", 1), &mut state)
}

#[must_use]
#[allow(clippy::too_many_arguments)]
fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    name: Option<&str>,
    artifact: Option<&str>,
    explicit: bool,
    stage: StageId,
    state: &mut PipelineState,
) -> bool {
    let output_name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let mut changed = false;
    for pass in passes {
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
        let mut pass_changed = false;
        let mut failed = false;

        if enabled {
            let errors_before = gcx.dcx().err_count();
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            pass_changed = pass.run_pass(gcx, module);
            let timer = timer.stop();
            failed = gcx.dcx().err_count() != errors_before;
            let after = before.as_ref().map(|_| module.to_text().to_string());
            let ir_changed = match (&before, &after) {
                (Some(before), Some(after)) => before != after,
                _ => pass_changed,
            };
            timer.finish(
                "EVM-IR",
                &output_name,
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

        if pass_diff {
            print_pass_diff(
                &output_name,
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
                module.to_text(),
            );
        } else if gcx.sess.opts.unstable.print_after_each {
            print_pass_output(
                &output_name,
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
                module.to_text(),
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
    changed |= run_passes_inner(
        gcx,
        module,
        &[&legalize_shifts::LegalizeShifts],
        Some(output_name),
        artifact,
        false,
        StageId::new("legalize-target", 1),
        state,
    );
    if gcx.dcx().has_errors().is_err() {
        return changed;
    }
    if gcx.sess.opts.unstable.print_after_stage {
        print_checkpoint(
            output_name,
            "EVM-IR",
            artifact,
            state.pipeline_run(),
            StageId::new("legalize-target", 1),
            "evm.target-legal",
            module.to_text(),
        );
    }
    for stage in DEFAULT_STAGES {
        changed |= run_passes_inner(
            gcx,
            module,
            stage.passes,
            Some(output_name),
            artifact,
            false,
            stage.id,
            state,
        );
        if gcx.dcx().has_errors().is_err() {
            return changed;
        }
        if gcx.sess.opts.unstable.print_after_stage {
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
    let mut state = PipelineState::default();
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

    let mut changed = false;
    let stage = StageId::new("custom", 1);
    let mut none_invocation = 0;
    for pass in passes {
        if let Some(pass) = pass {
            changed |= run_passes_inner(
                gcx,
                module,
                &[pass],
                Some(&output_name),
                artifact,
                true,
                stage,
                &mut state,
            );
            if gcx.dcx().has_errors().is_err() {
                return changed;
            }
        } else if gcx.sess.opts.unstable.pass_diff && !gcx.sess.opts.unstable.print_after_stage {
            none_invocation += 1;
            let text = module.to_text();
            print_pass_diff(
                &output_name,
                "EVM-IR",
                artifact,
                state.pipeline_run(),
                stage,
                "none",
                none_invocation,
                false,
                false,
                false,
                "skipped",
                &text,
                &text,
            );
        } else if gcx.sess.opts.unstable.print_after_each {
            none_invocation += 1;
            print_pass_output(
                &output_name,
                "EVM-IR",
                artifact,
                state.pipeline_run(),
                stage,
                "none",
                none_invocation,
                false,
                "skipped",
                false,
                false,
                module.to_text(),
            );
        }
    }
    if gcx.sess.opts.unstable.print_after_stage {
        print_checkpoint(
            &output_name,
            "EVM-IR",
            artifact,
            state.pipeline_run(),
            stage,
            "custom-output",
            module.to_text(),
        );
    }
    changed |= run_passes_inner(
        gcx,
        module,
        &[&legalize_shifts::LegalizeShifts],
        Some(&output_name),
        artifact,
        false,
        StageId::new("legalize-target", 1),
        &mut state,
    );
    if gcx.dcx().has_errors().is_err() {
        return changed;
    }
    if gcx.sess.opts.unstable.print_after_stage {
        print_checkpoint(
            &output_name,
            "EVM-IR",
            artifact,
            state.pipeline_run(),
            StageId::new("legalize-target", 1),
            "evm.target-legal",
            module.to_text(),
        );
    }
    changed
}

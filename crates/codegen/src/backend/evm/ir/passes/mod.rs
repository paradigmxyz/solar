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
pub(super) mod data;
mod dce;
mod legalize_shifts;
mod outline;
mod peephole;
mod reorder_pushes;
mod share_reverts;
mod stack_normalize;
mod tail_merge;
mod terminal_dedup;
pub(super) mod utils;

pub(in crate::backend::evm) use legalize_shifts::{LEGACY_SHIFT_STACK_HEADROOM, legalize_shifts};

use super::Module;
use crate::{
    pass_manager::{
        parse_pass_pipeline, pipeline_output_name, print_pass_diff, should_validate_ir,
    },
    timing::PassTimer,
};
use solar_config::{EvmVersion, OptimizationMode};
use solar_interface::diagnostics::DiagCtxt;
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
    &peephole::StopStackCleanup,
    &reorder_pushes::REORDER_PUSHES,
    &share_reverts::ShareReverts,
    &stack_normalize::StackDedup,
    &stack_normalize::StackNormalize,
    &compact_pushes::CompactPushes,
    &constant_data::ConstantData,
    &coalesce_copies::CoalesceCopies,
    &data::PackData,
    &dce::Dce,
    &legalize_shifts::LegalizeShifts,
    &cfg_simplify::CfgSimplify,
    &outline::Outline,
    &terminal_dedup::TerminalDedup,
    &tail_merge::TailMerge,
    &block_layout::BlockLayout,
];

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
static DEFAULT_PIPELINE: &[&dyn EvmPass] = &[
    // Normalize and establish the first physical layout.
    &peephole::Peephole,
    &coalesce_copies::CoalesceCopies,
    &cfg_simplify::CfgSimplify,
    &data::PackExistingData,
    &peephole::Cleanup(compact_pushes::CompactPushes),
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    // Simplify and merge the explicit control-flow graph.
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    // Outline only after straight-line paths and terminal tails are canonical.
    &outline::Outline,
    &cfg_simplify::CfgSimplify,
    // Stack allocation can leave `producer; push; swap1` when the producer was emitted first.
    // Reorder it only after structural sharing is fixed so local stack cleanup cannot perturb
    // outlining choices.
    &reorder_pushes::REORDER_PUSHES,
    &peephole::Peephole,
    // Regenerate only after structural sharing is fixed. Doing this before
    // tail merging can make otherwise-identical blocks context-dependent and
    // lose more shared bytes than the local CSE removes.
    &peephole::Cleanup(block_cse::BlockCse),
    &peephole::Cleanup(dce::Dce),
    // Stack normalization exposes local rewrites.
    &peephole::Cleanup(stack_normalize::StackNormalize),
    // Pack address-sensitive terminal blocks, then clean up any adjacent
    // revert branch that remains profitable in the final layout.
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &cfg_simplify::CfgSimplify,
    &block_layout::BlockLayout,
    // Block CSE and final placement can expose new equal tails whose addresses or predecessors
    // differed during the first structural sweep. Repeat the structural half to a fixed point at
    // pass granularity; each pass remains internally profitability-gated.
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &outline::Outline,
    &cfg_simplify::CfgSimplify,
    &reorder_pushes::FINAL_REORDER_PUSHES,
    &peephole::Peephole,
    &peephole::Cleanup(block_cse::BlockCse),
    &peephole::Cleanup(dce::Dce),
    &peephole::Cleanup(stack_normalize::StackNormalize),
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &cfg_simplify::CfgSimplify,
    &block_layout::BlockLayout,
    // Materialize constants and finalize the referenced data pool after all code transforms.
    &constant_data::ConstantData,
    &data::PackData,
    // Data packing can add compactable immediates and local stack shuffles.
    &compact_pushes::CompactPushes,
    &peephole::Peephole,
    &stack_normalize::StackDedup,
    &peephole::StopStackCleanup,
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
    run_passes_inner(gcx, module, passes, true, name)
}

/// Runs EVM IR passes without validating after each pass.
#[must_use]
pub fn run_passes_no_validate(gcx: Gcx<'_>, module: &mut Module, passes: &[&dyn EvmPass]) -> bool {
    run_passes_inner(gcx, module, passes, false, None)
}

#[must_use]
fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    validate_each: bool,
    name: Option<&str>,
) -> bool {
    let output_name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let explicit = name.is_some();
    let mut changed = false;
    for pass in passes {
        let pass_name = pass.name();
        let before =
            (explicit && gcx.sess.opts.unstable.pass_diff).then(|| module.to_text().to_string());
        let enabled = pass.is_enabled(gcx, module);
        if !enabled && !explicit {
            continue;
        }

        if enabled {
            let errors_before = gcx.dcx().err_count();
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let pass_changed = pass.run_pass(gcx, module);
            timer.finish("EVM IR", module.name(), pass_name, pass_changed);
            changed |= pass_changed;
            if gcx.dcx().err_count() != errors_before {
                return changed;
            }
            if validate_each && should_validate_ir(gcx) {
                validate_module_after_pass(module, pass_name);
            }
        }

        if let Some(before) = before {
            print_pass_diff(&output_name, pass_name, before, module.to_text());
        } else if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {output_name} (after {pass_name}) ===");
            print!("{}", module.to_text());
        }
    }
    changed
}

fn validate_module_after_pass(module: &Module, pass_name: &str) {
    let dcx = DiagCtxt::new_early();
    super::verify::Verifier::for_evm_version(&dcx, EvmVersion::Osaka).verify_module_shape(module);
    if dcx.has_errors().is_err() {
        panic!("EVM IR validation failed after `{pass_name}`");
    }
}

/// Runs the configured EVM IR pipeline, or the canonical pipeline when none was provided.
///
/// `name` overrides the module name in pass output.
#[must_use]
pub fn run_pipeline(gcx: Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
    super::verify::Verifier::new(gcx).verify_before_pipeline(module);
    if gcx.dcx().has_errors().is_err() {
        return false;
    }

    let Some(value) = gcx.sess.opts.unstable.evm_ir_pipeline.as_deref() else {
        return run_passes(gcx, module, DEFAULT_PIPELINE, None);
    };
    let pipeline = match parse_pass_pipeline(gcx, value, "EVM IR", lookup_pass) {
        Ok(pipeline) => pipeline,
        Err(_) => return false,
    };
    let Some(passes) = pipeline else {
        return run_passes(gcx, module, DEFAULT_PIPELINE, None);
    };

    let name =
        name.map(ToOwned::to_owned).unwrap_or_else(|| pipeline_output_name(gcx, module.name()));
    let mut changed = false;
    for pass in passes {
        if let Some(pass) = pass {
            changed |= run_passes(gcx, module, &[pass], Some(&name));
        } else if gcx.sess.opts.unstable.pass_diff {
            let text = module.to_text();
            print_pass_diff(&name, "none", &text, &text);
        } else if gcx.sess.opts.unstable.print_after_each {
            println!("// === {name} (after none) ===");
            print!("{}", module.to_text());
        }
    }
    changed
}

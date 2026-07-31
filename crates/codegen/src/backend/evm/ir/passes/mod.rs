//! EVM IR optimization and layout passes.
//!
//! This module owns the pass list and canonical backend pipeline. Individual
//! transforms live in their own modules so their implementation and invariants
//! remain local, matching the organization of the MIR transforms.

mod block_layout;
mod cfg_simplify;
mod compact_pushes;
mod outline;
mod peephole;
mod share_reverts;
mod tail_merge;
mod terminal_dedup;
pub(super) mod utils;

use super::Module;
use crate::{
    pass_manager::{parse_pass_pipeline, pipeline_output_name, print_pass_diff},
    timing::PassTimer,
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
    &peephole::Peephole,
    &share_reverts::ShareReverts,
    &compact_pushes::CompactPushes,
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
    &compact_pushes::CompactPushes,
    &peephole::Peephole,
    &cfg_simplify::CfgSimplify,
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
    &compact_pushes::CompactPushes,
    &peephole::Peephole,
    // Pack address-sensitive terminal blocks, then clean up any adjacent
    // revert branch that remains profitable in the final layout.
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    &cfg_simplify::CfgSimplify,
    &block_layout::BlockLayout,
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
    run_passes_inner(gcx, module, passes, name)
}

#[must_use]
fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
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
            let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
            let pass_changed = pass.run_pass(gcx, module);
            timer.finish("EVM IR", module.name(), pass_name, pass_changed);
            changed |= pass_changed;
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

/// Runs the configured EVM IR pipeline, or the canonical pipeline when none was provided.
///
/// `name` overrides the module name in pass output.
#[must_use]
pub fn run_pipeline(gcx: Gcx<'_>, module: &mut Module, name: Option<&str>) -> bool {
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

//! EVM IR optimization and layout passes.
//!
//! This module owns the pass list and canonical backend pipeline. Individual
//! transforms live in their own modules so their implementation and invariants
//! remain local, matching the organization of the MIR transforms.

mod block_layout;
mod cfg_simplify;
pub(in crate::backend::evm) mod compact_pushes;
mod outline;
mod peephole;
mod share_reverts;
mod tail_merge;
mod terminal_dedup;
pub(super) mod utils;

use super::Module;
use crate::timing::PassTimer;
use solar_config::OptimizationMode;
use solar_sema::Gcx;

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
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool;
}

/// All EVM IR passes exposed by `solar evm-opt`.
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
pub(crate) static DEFAULT_PIPELINE: &[&dyn EvmPass] = &[
    // Normalize and establish the first physical layout.
    &peephole::Peephole,
    &compact_pushes::CompactPushes,
    &cfg_simplify::CfgSimplify,
    &block_layout::BlockLayout,
    &share_reverts::ShareReverts,
    // Simplify and merge the explicit control-flow graph.
    &cfg_simplify::CfgSimplify,
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify,
    &tail_merge::TailMerge,
    &cfg_simplify::CfgSimplify,
    // Outline only after straight-line paths and terminal tails are canonical.
    &outline::Outline,
    &cfg_simplify::CfgSimplify,
    &terminal_dedup::TerminalDedup,
    &cfg_simplify::CfgSimplify,
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
pub fn run_passes(gcx: Gcx<'_>, module: &mut Module, passes: &[&dyn EvmPass]) -> bool {
    let mut changed = false;
    for pass in passes {
        let pass_name = pass.name();
        if !pass.is_enabled(gcx, module) {
            continue;
        }

        let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
        let pass_changed = pass.run_pass(gcx, module);
        timer.finish("EVM IR", module.name(), pass_name, pass_changed);
        changed |= pass_changed;

        if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {} (after {pass_name}) ===", module.name());
            print!("{}", module.to_text());
        }
    }
    changed
}

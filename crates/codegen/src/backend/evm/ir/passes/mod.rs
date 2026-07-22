//! EVM IR optimization and layout passes.
//!
//! This module owns the pass registry and canonical backend pipeline. Individual
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
use crate::{pass::Optimizations, timing::PassTimer};
use solar_sema::Gcx;

/// A streamlined trait for an EVM IR transformation pass.
pub trait EvmPass: Sync {
    /// Command-line and pipeline name.
    fn name(&self) -> &'static str;

    /// Returns whether this pass is enabled with the current compiler flags.
    fn is_enabled(&self, _gcx: Gcx<'_>, _module: &Module) -> bool {
        true
    }

    /// Returns whether this pass can be overridden by pass-selection options.
    fn can_be_overridden(&self) -> bool {
        true
    }

    /// Runs the pass and returns whether it changed EVM IR.
    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool;

    /// Returns whether this pass must run independently of the optimization level.
    fn is_required(&self) -> bool {
        false
    }
}

macro_rules! declare_passes {
    ($($vis:vis const $const_name:ident = $module:ident::$pass:ident;)+) => {
        $(
            $vis const $const_name: $module::$pass = $module::$pass;
        )+

        /// All EVM IR passes exposed by `solar evm-opt`.
        pub static PASS_REGISTRY: &[&dyn EvmPass] = &[$(&$const_name),+];
    };
}

declare_passes! {
    const PEEPHOLE_PASS = peephole::Peephole;
    const SHARE_REVERTS_PASS = share_reverts::ShareReverts;
    const COMPACT_PUSHES_PASS = compact_pushes::CompactPushes;
    const CFG_SIMPLIFY_PASS = cfg_simplify::CfgSimplify;
    const OUTLINE_PASS = outline::Outline;
    const TERMINAL_DEDUP_PASS = terminal_dedup::TerminalDedup;
    const TAIL_MERGE_PASS = tail_merge::TailMerge;
    const BLOCK_LAYOUT_PASS = block_layout::BlockLayout;
}

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
pub(crate) static DEFAULT_PIPELINE: &[&dyn EvmPass] = &[
    // Normalize and establish the first physical layout.
    &PEEPHOLE_PASS,
    &COMPACT_PUSHES_PASS,
    &CFG_SIMPLIFY_PASS,
    &BLOCK_LAYOUT_PASS,
    &SHARE_REVERTS_PASS,
    // Simplify and merge the explicit control-flow graph.
    &CFG_SIMPLIFY_PASS,
    &TERMINAL_DEDUP_PASS,
    &CFG_SIMPLIFY_PASS,
    &TAIL_MERGE_PASS,
    &CFG_SIMPLIFY_PASS,
    &TAIL_MERGE_PASS,
    &CFG_SIMPLIFY_PASS,
    // Outline only after straight-line paths and terminal tails are canonical.
    &OUTLINE_PASS,
    &CFG_SIMPLIFY_PASS,
    &TERMINAL_DEDUP_PASS,
    &CFG_SIMPLIFY_PASS,
    // Pack address-sensitive terminal blocks, then clean up any adjacent
    // revert branch that remains profitable in the final layout.
    &BLOCK_LAYOUT_PASS,
    &SHARE_REVERTS_PASS,
    &CFG_SIMPLIFY_PASS,
    &BLOCK_LAYOUT_PASS,
];

/// Finds a pass in the EVM IR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass> {
    PASS_REGISTRY.iter().copied().find(|pass| pass.name() == name)
}

/// Returns whether `pass` should run for this optimization mode.
pub(super) fn should_run_pass<P>(
    gcx: Gcx<'_>,
    module: &Module,
    pass: &P,
    optimizations: Optimizations,
) -> bool
where
    P: EvmPass + ?Sized,
{
    if !pass.can_be_overridden() {
        return pass.is_enabled(gcx, module);
    }

    let suppressed = !pass.is_required() && matches!(optimizations, Optimizations::Suppressed);
    !suppressed && pass.is_enabled(gcx, module)
}

/// Runs an EVM IR pass pipeline.
pub fn run_passes(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
    optimizations: Optimizations,
) -> bool {
    run_passes_inner(gcx, module, passes, optimizations)
}

fn run_passes_inner(
    gcx: Gcx<'_>,
    module: &mut Module,
    passes: &[&dyn EvmPass],
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
        timer.finish("EVM IR", module.name(), pass_name, pass_changed);
        changed |= pass_changed;

        if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
            println!("// === {} (after {pass_name}) ===", module.name());
            print!("{}", module.to_text());
        }
    }
    changed
}

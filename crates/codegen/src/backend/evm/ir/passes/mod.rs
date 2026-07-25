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
    #[must_use]
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

/// Finds an EVM IR pass by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static dyn EvmPass> {
    ALL_PASSES.iter().copied().find(|pass| pass.name() == name)
}

/// Runs an EVM IR pass pipeline.
#[must_use]
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

fn run_one(gcx: Gcx<'_>, module: &mut Module, pass: &'static dyn EvmPass) -> bool {
    run_passes(gcx, module, &[pass])
}

/// Runs the canonical EVM IR layout and code-size pipeline.
#[must_use]
pub(crate) fn run_default_pipeline(gcx: Gcx<'_>, module: &mut Module) -> bool {
    // Normalize the machine instructions and establish the first physical layout.
    let mut changed = run_passes(
        gcx,
        module,
        &[
            &peephole::Peephole,
            &compact_pushes::CompactPushes,
            &cfg_simplify::CfgSimplify,
            &block_layout::BlockLayout,
        ],
    );

    // CFG cleanup is only needed when terminal sharing changed an edge.
    let shared = run_one(gcx, module, &share_reverts::ShareReverts);
    changed |= shared;
    let deduplicated = run_one(gcx, module, &terminal_dedup::TerminalDedup);
    changed |= deduplicated;
    if shared || deduplicated {
        changed |= run_one(gcx, module, &cfg_simplify::CfgSimplify);
    }

    // Tail merging itself reaches a fixed point. Run it again only when CFG
    // cleanup exposes new cross-block suffixes.
    let tails_merged = run_one(gcx, module, &tail_merge::TailMerge);
    changed |= tails_merged;
    let mut second_tail_merge = false;
    if tails_merged {
        let cfg_changed = run_one(gcx, module, &cfg_simplify::CfgSimplify);
        changed |= cfg_changed;
        if cfg_changed {
            second_tail_merge = run_one(gcx, module, &tail_merge::TailMerge);
            changed |= second_tail_merge;
        }
    }

    // Outline after straight-line paths and terminal tails are canonical.
    let outlined = run_one(gcx, module, &outline::Outline);
    changed |= outlined;
    if second_tail_merge || outlined {
        changed |= run_one(gcx, module, &cfg_simplify::CfgSimplify);
    }
    changed |= run_one(gcx, module, &compact_pushes::CompactPushes);

    // Pack address-sensitive terminal blocks, then clean up a shared adjacent
    // revert path only when branch inversion changed the final layout.
    changed |= run_one(gcx, module, &block_layout::BlockLayout);
    let shared = run_one(gcx, module, &share_reverts::ShareReverts);
    changed |= shared;
    if shared {
        changed |=
            run_passes(gcx, module, &[&cfg_simplify::CfgSimplify, &block_layout::BlockLayout]);
    }

    changed
}

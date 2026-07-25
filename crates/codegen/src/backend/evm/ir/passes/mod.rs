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

use super::{BlockId, Instruction, Module};
use crate::timing::PassTimer;
use solar_config::OptimizationMode;
use solar_data_structures::bit_set::GrowableBitSet;
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
        changed |= run_pass_with(gcx, module, *pass, |module| pass.run_pass(gcx, module));
    }
    changed
}

#[must_use]
fn run_pass_with(
    gcx: Gcx<'_>,
    module: &mut Module,
    pass: &dyn EvmPass,
    run: impl FnOnce(&mut Module) -> bool,
) -> bool {
    if !pass.is_enabled(gcx, module) {
        return false;
    }

    let pass_name = pass.name();
    let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
    let changed = run(module);
    timer.finish("EVM IR", module.name(), pass_name, changed);

    if gcx.sess.opts.unstable.print_after_each && !gcx.sess.opts.unstable.pass_diff {
        println!("// === {} (after {pass_name}) ===", module.name());
        print!("{}", module.to_text());
    }
    changed
}

#[must_use]
fn run_one(gcx: Gcx<'_>, module: &mut Module, pass: &'static dyn EvmPass) -> bool {
    run_passes(gcx, module, &[pass])
}

#[must_use]
fn run_cfg(gcx: Gcx<'_>, module: &mut Module, state: &mut cfg_simplify::RunState) -> bool {
    run_pass_with(gcx, module, &cfg_simplify::CfgSimplify, |module| {
        cfg_simplify::simplify_cfg_with_state(module, state)
    })
}

#[must_use]
fn run_layout(gcx: Gcx<'_>, module: &mut Module, state: &mut block_layout::RunState) -> bool {
    run_pass_with(gcx, module, &block_layout::BlockLayout, |module| {
        block_layout::layout_blocks_with_state(gcx, module, state)
    })
}

#[must_use]
fn run_compact_pushes(gcx: Gcx<'_>, module: &mut Module, scratch: &mut Vec<Instruction>) -> bool {
    run_pass_with(gcx, module, &compact_pushes::CompactPushes, |module| {
        compact_pushes::compact_pushes_with_scratch(gcx, module, scratch)
    })
}

#[must_use]
fn run_share_reverts(
    gcx: Gcx<'_>,
    module: &mut Module,
    empty_reverts: &mut GrowableBitSet<BlockId>,
) -> bool {
    run_pass_with(gcx, module, &share_reverts::ShareReverts, |module| {
        share_reverts::share_reverts_with_scratch(module, empty_reverts)
    })
}

#[must_use]
fn run_tail_merge(gcx: Gcx<'_>, module: &mut Module, state: &mut tail_merge::RunState) -> bool {
    run_pass_with(gcx, module, &tail_merge::TailMerge, |module| {
        tail_merge::merge_tails_with_state(module, state)
    })
}

/// Runs the canonical EVM IR layout and code-size pipeline.
#[must_use]
pub(crate) fn run_default_pipeline(gcx: Gcx<'_>, module: &mut Module) -> bool {
    let mut cfg_state = cfg_simplify::RunState::default();
    let mut layout_state = block_layout::RunState::default();
    let mut compact_scratch = Vec::new();
    let mut empty_reverts = GrowableBitSet::new_empty();
    let mut tail_merge_state = tail_merge::RunState::default();

    // Normalize the machine instructions and establish the first physical layout.
    let mut changed = run_one(gcx, module, &peephole::Peephole);
    changed |= run_compact_pushes(gcx, module, &mut compact_scratch);
    changed |= run_cfg(gcx, module, &mut cfg_state);
    changed |= run_layout(gcx, module, &mut layout_state);

    // CFG cleanup is only needed when terminal sharing changed an edge.
    let shared = run_share_reverts(gcx, module, &mut empty_reverts);
    changed |= shared;
    let deduplicated = run_one(gcx, module, &terminal_dedup::TerminalDedup);
    changed |= deduplicated;
    if shared || deduplicated {
        changed |= run_cfg(gcx, module, &mut cfg_state);
    }

    // Tail merging itself reaches a fixed point. Run it again only when CFG
    // cleanup exposes new cross-block suffixes.
    let tails_merged = run_tail_merge(gcx, module, &mut tail_merge_state);
    changed |= tails_merged;
    let mut second_tail_merge = false;
    if tails_merged {
        let cfg_changed = run_cfg(gcx, module, &mut cfg_state);
        changed |= cfg_changed;
        if cfg_changed {
            second_tail_merge = run_tail_merge(gcx, module, &mut tail_merge_state);
            changed |= second_tail_merge;
        }
    }

    // Outline after straight-line paths and terminal tails are canonical.
    let outlined = run_one(gcx, module, &outline::Outline);
    changed |= outlined;
    if second_tail_merge || outlined {
        changed |= run_cfg(gcx, module, &mut cfg_state);
    }
    changed |= run_compact_pushes(gcx, module, &mut compact_scratch);

    // Pack address-sensitive terminal blocks, then clean up a shared adjacent
    // revert path only when branch inversion changed the final layout.
    changed |= run_layout(gcx, module, &mut layout_state);
    let shared = run_share_reverts(gcx, module, &mut empty_reverts);
    changed |= shared;
    if shared {
        changed |= run_cfg(gcx, module, &mut cfg_state);
        changed |= run_layout(gcx, module, &mut layout_state);
    }

    changed
}

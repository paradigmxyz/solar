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
use crate::{
    pass_manager::{Pass, PassFactory, PassManager, find_pass},
    timing::PassTimer,
};
use solar_sema::Gcx;

/// A dynamically dispatched EVM IR transformation pass.
pub type EvmPass = dyn Pass<Module>;

macro_rules! declare_passes {
    ($(
        $(#[doc = $description:literal])+
        $vis:vis const $const_name:ident -> $name:literal = $run_pass:path;
    )+) => {
        $(
            $(#[doc = $description])+
            $vis const $const_name: PassFactory<Module> = PassFactory::new(
                $name,
                concat!($($description, "\n"),+).trim_ascii(),
                $run_pass,
            );
        )+

        /// All EVM IR passes exposed by `solar evm-opt`.
        pub static PASS_REGISTRY: &[&EvmPass] = &[$(&$const_name),+];
    };
}

declare_passes! {
    /// Simplify local instruction sequences in scheduled EVM IR.
    pub(crate) const PEEPHOLE_PASS -> "peephole" = peephole::run;

    /// Share empty revert blocks and invert their conditional branches.
    pub(crate) const SHARE_REVERTS_PASS -> "share-reverts" = share_reverts::run;

    /// Select smaller instruction sequences for large immediate pushes.
    pub(crate) const COMPACT_PUSHES_PASS -> "compact-pushes" = compact_pushes::run;

    /// Redirect jump thunks, remove unreachable blocks, and coalesce linear control flow.
    pub(crate) const CFG_SIMPLIFY_PASS -> "cfg-simplify" = cfg_simplify::run;

    /// Outline repeated closed computations and large immediate pushes.
    pub(crate) const OUTLINE_PASS -> "outline" = outline::run;

    /// Redirect duplicate terminal block bodies to the first copy.
    pub(crate) const TERMINAL_DEDUP_PASS -> "terminal-dedup" = terminal_dedup::run;

    /// Merge profitable common suffixes of terminal blocks.
    pub(crate) const TAIL_MERGE_PASS -> "tail-merge" = tail_merge::run;

    /// Reorder blocks to maximize jumps assembled as physical fallthroughs.
    pub(crate) const BLOCK_LAYOUT_PASS -> "block-layout" = block_layout::run;
}

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
pub(crate) static DEFAULT_PIPELINE: &[&EvmPass] = &[
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
pub fn lookup_pass(name: &str) -> Option<&'static EvmPass> {
    find_pass(PASS_REGISTRY, name)
}

/// Runs a named EVM IR pass over a module.
#[tracing::instrument(
    name = "evm_ir_pass",
    level = "debug",
    skip_all,
    fields(pass = pass.name()),
)]
pub fn run_pass(gcx: Gcx<'_>, module: &mut Module, pass: &EvmPass) -> bool {
    PassManager::new(gcx, run_pass_inner).run_pass(module, pass)
}

fn run_pass_inner(gcx: Gcx<'_>, module: &mut Module, pass: &EvmPass) -> bool {
    let timer = PassTimer::new(gcx.sess.opts.unstable.time_passes);
    let changed = pass.run(gcx, module);
    timer.finish("EVM IR", module.name(), pass.name(), changed);
    changed
}

/// Runs an EVM IR pass pipeline.
pub(crate) fn run_passes(gcx: Gcx<'_>, module: &mut Module, passes: &[&EvmPass]) -> bool {
    PassManager::new(gcx, run_pass_inner).run_passes(module, passes)
}

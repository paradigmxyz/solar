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
use crate::{backend::evm::stack_schedule, timing::PassTimer};
use solar_config::{EvmVersion, OptimizationMode};

type PassRunner = fn(&mut Module, PassOptions) -> bool;

/// Registry entry for an EVM IR transform pass.
#[derive(Clone, Copy, Debug)]
pub struct PassInfo {
    /// Command-line and pipeline name.
    pub name: &'static str,
    /// Human-readable help text.
    pub description: &'static str,
    run_pass: PassRunner,
}

impl PassInfo {
    const fn new(name: &'static str, description: &'static str, run_pass: PassRunner) -> Self {
        Self { name, description, run_pass }
    }
}

macro_rules! declare_passes {
    ($(
        $(#[doc = $description:literal])+
        $vis:vis const $const_name:ident -> $name:literal = $run_pass:path;
    )+) => {
        $(
            $(#[doc = $description])+
            $vis const $const_name: PassInfo = PassInfo::new(
                $name,
                concat!($($description, "\n"),+).trim_ascii(),
                $run_pass,
            );
        )+
    };
}

declare_passes! {
    /// Materialize virtual instruction operands with physical stack operations.
    pub const STACK_SCHEDULE_PASS -> "stack-schedule" = run_stack_schedule;

    /// Simplify local instruction sequences in scheduled EVM IR.
    pub const PEEPHOLE_PASS -> "peephole" = peephole::run;

    /// Share empty revert blocks and invert their conditional branches.
    pub const SHARE_REVERTS_PASS -> "share-reverts" = share_reverts::run;

    /// Select smaller instruction sequences for large immediate pushes.
    pub const COMPACT_PUSHES_PASS -> "compact-pushes" = compact_pushes::run;

    /// Redirect jump thunks, remove unreachable blocks, and coalesce linear control flow.
    pub const CFG_SIMPLIFY_PASS -> "cfg-simplify" = cfg_simplify::run;

    /// Outline repeated closed computations and large immediate pushes.
    pub const OUTLINE_PASS -> "outline" = outline::run;

    /// Redirect duplicate terminal block bodies to the first copy.
    pub const TERMINAL_DEDUP_PASS -> "terminal-dedup" = terminal_dedup::run;

    /// Merge profitable common suffixes of terminal blocks.
    pub const TAIL_MERGE_PASS -> "tail-merge" = tail_merge::run;

    /// Reorder blocks to maximize jumps assembled as physical fallthroughs.
    pub const BLOCK_LAYOUT_PASS -> "block-layout" = block_layout::run;
}

/// Options for running an EVM IR pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassOptions {
    /// Print the time spent in the pass.
    pub time_passes: bool,
    /// EVM version used for target-dependent instruction sizing.
    pub evm_version: EvmVersion,
    /// Optimization mode used for profitability decisions.
    pub optimization: OptimizationMode,
}

/// All EVM IR passes exposed by `solar evm-opt`.
pub const PASS_REGISTRY: &[PassInfo] = &[
    STACK_SCHEDULE_PASS,
    PEEPHOLE_PASS,
    SHARE_REVERTS_PASS,
    COMPACT_PUSHES_PASS,
    CFG_SIMPLIFY_PASS,
    OUTLINE_PASS,
    TERMINAL_DEDUP_PASS,
    TAIL_MERGE_PASS,
    BLOCK_LAYOUT_PASS,
];

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
pub const DEFAULT_PIPELINE: &[PassInfo] = &[
    // Normalize and establish the first physical layout.
    PEEPHOLE_PASS,
    COMPACT_PUSHES_PASS,
    CFG_SIMPLIFY_PASS,
    BLOCK_LAYOUT_PASS,
    SHARE_REVERTS_PASS,
    // Simplify and merge the explicit control-flow graph.
    CFG_SIMPLIFY_PASS,
    TERMINAL_DEDUP_PASS,
    CFG_SIMPLIFY_PASS,
    TAIL_MERGE_PASS,
    CFG_SIMPLIFY_PASS,
    TAIL_MERGE_PASS,
    CFG_SIMPLIFY_PASS,
    // Outline only after straight-line paths and terminal tails are canonical.
    OUTLINE_PASS,
    CFG_SIMPLIFY_PASS,
    TERMINAL_DEDUP_PASS,
    CFG_SIMPLIFY_PASS,
    // Pack address-sensitive terminal blocks, then clean up any adjacent
    // revert branch that remains profitable in the final layout.
    BLOCK_LAYOUT_PASS,
    SHARE_REVERTS_PASS,
    CFG_SIMPLIFY_PASS,
    BLOCK_LAYOUT_PASS,
];

/// Finds a pass in the EVM IR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static PassInfo> {
    PASS_REGISTRY.iter().find(|pass| pass.name == name)
}

/// Runs a named EVM IR pass over a module.
pub fn run_pass(module: &mut Module, pass: &PassInfo, options: PassOptions) -> bool {
    let timer = PassTimer::new(options.time_passes);
    let changed = (pass.run_pass)(module, options);
    timer.finish("EVM IR", module.name(), pass.name, changed);
    changed
}

fn run_stack_schedule(module: &mut Module, _options: PassOptions) -> bool {
    stack_schedule::schedule_stack_ops(module)
}

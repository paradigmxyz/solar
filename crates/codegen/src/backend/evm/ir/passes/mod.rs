//! EVM IR optimization and layout passes.
//!
//! This module owns the pass registry and canonical backend pipeline. Individual
//! transforms live in their own modules so their implementation and invariants
//! remain local, matching the organization of the MIR transforms.

mod block_layout;
mod terminal_dedup;
pub(super) mod utils;

use super::Module;
use crate::{backend::evm::ir_stack_schedule, timing::PassTimer};

type PassRunner = fn(&mut Module) -> bool;

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
    pub const STACK_SCHEDULE_PASS -> "stack-schedule" = ir_stack_schedule::schedule_stack_ops;

    /// Replace duplicate terminal block bodies with jumps to the first copy when profitable.
    pub const TERMINAL_DEDUP_PASS -> "terminal-dedup" = terminal_dedup::run;

    /// Reorder blocks to maximize jumps assembled as physical fallthroughs.
    pub const BLOCK_LAYOUT_PASS -> "block-layout" = block_layout::run;
}

/// Options for running an EVM IR pass.
#[derive(Clone, Copy, Debug, Default)]
pub struct PassOptions {
    /// Print the time spent in the pass.
    pub time_passes: bool,
}

/// All EVM IR passes exposed by `solar evm-opt`.
pub const PASS_REGISTRY: &[PassInfo] =
    &[STACK_SCHEDULE_PASS, TERMINAL_DEDUP_PASS, BLOCK_LAYOUT_PASS];

/// The canonical EVM IR layout and code-size pipeline used by EVM codegen.
pub const DEFAULT_LAYOUT_PIPELINE: &[PassInfo] = &[TERMINAL_DEDUP_PASS, BLOCK_LAYOUT_PASS];

/// Finds a pass in the EVM IR pass registry by command-line name.
pub fn lookup_pass(name: &str) -> Option<&'static PassInfo> {
    PASS_REGISTRY.iter().find(|pass| pass.name == name)
}

/// Runs a named EVM IR pass over a module.
pub fn run_pass(module: &mut Module, pass: &PassInfo, options: PassOptions) -> bool {
    let timer = PassTimer::new(options.time_passes);
    let changed = (pass.run_pass)(module);
    timer.finish("EVM IR", module.name(), pass.name, changed);
    changed
}

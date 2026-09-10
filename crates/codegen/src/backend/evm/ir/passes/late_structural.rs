//! Conditional structural cleanup after constant and stack finalization.
//!
//! Late constant materialization, data packing, and stack cleanup can make terminal blocks equal.
//! This pass deduplicates those terminals and runs CFG simplification plus tail merging only when
//! deduplication changed the module. Those follow-up transforms remove the forwarding edges and
//! common tails created by terminal folding; without that trigger they only repeat analyses from
//! the earlier structural sweeps.
//!
//! The pass runs near the end of the EVM IR pipeline. Each underlying transform retains its own
//! safety and profitability checks. The conditional preserves the complete cleanup when a late
//! terminal opportunity exists while avoiding two whole-module analyses on unchanged code.

use super::{
    EvmPass, cfg_simplify::CfgSimplify, tail_merge::TailMerge, terminal_dedup::TerminalDedup,
};
use crate::backend::evm::ir::Module;
use solar_sema::Gcx;

pub(super) struct LateStructural;

impl EvmPass for LateStructural {
    fn name(&self) -> &'static str {
        "late-structural"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        if !TerminalDedup.run_pass(gcx, module) {
            return false;
        }

        // terminal_dedup; cfg_simplify; tail_merge
        let _ = CfgSimplify.run_pass(gcx, module);
        let _ = TailMerge.run_pass(gcx, module);
        true
    }
}

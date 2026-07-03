//! Dispatch phase lowering: materialize the selector switch as MIR.
//!
//! In `built`/`optimized` MIR there is no dispatcher: the backend synthesizes
//! the selector switch that routes an incoming call to the right external
//! function. This pass makes that routing an ordinary MIR function named
//! `entry` (the dispatch phase of the sketch in [`MirPhase`]).
//!
//! The synthesized `entry` function loads the 4-byte selector
//! (`calldataload(0) >> 224`) and switches on it to one argument-free
//! `internal_call` per external wrapper, defaulting to a `revert`. It is meant
//! to run after [`super::LowerAbiPass`], which turns external functions into the
//! argument-free self-decoding wrappers this switch routes to; that is why it
//! only routes selector-bearing functions that take no MIR arguments.
//!
//! It requires the `abi` phase: it routes to the argument-free wrappers that
//! [`super::LowerAbiPass`] produces, so it bails on `built`/`optimized` modules
//! rather than half-dispatching argument-taking functions.
//!
//! This is opt-in: it is not part of the default pipeline, and the backend does
//! not consume `dispatch`-phase modules. It is the staging ground for moving the
//! dispatcher out of the backend.

use crate::{
    mir::{Function, FunctionBuilder, FunctionId, MirPhase, Module, ValueId},
    pass::ModulePass,
};
use solar_interface::{Ident, Symbol};

/// Statistics from dispatch lowering.
#[derive(Clone, Debug, Default)]
pub struct LowerDispatchStats {
    /// Number of selector cases routed by the synthesized `entry` function.
    pub routed: usize,
}

/// Dispatch phase lowering pass.
#[derive(Debug, Default)]
pub struct LowerDispatchPass {
    stats: LowerDispatchStats,
}

impl LowerDispatchPass {
    /// Returns statistics for the most recent run.
    #[must_use]
    pub const fn stats(&self) -> &LowerDispatchStats {
        &self.stats
    }

    fn run(&mut self, module: &mut Module) -> bool {
        self.stats = LowerDispatchStats::default();

        // Idempotent: only build the dispatcher once.
        if module.phase >= MirPhase::Dispatch {
            return false;
        }

        // Dispatch routes to the argument-free ABI wrappers, so it requires the
        // ABI phase. Running on `built`/`optimized` MIR would leave
        // argument-taking external functions unroutable while still advancing
        // the phase; require the precondition and bail otherwise.
        if module.phase < MirPhase::Abi {
            return false;
        }

        // The ABI phase invariant is a hard precondition, not a debug-only
        // assumption. If a hand-written or stale MIR module claims `abi` while
        // still containing argument-taking selector functions, leave it
        // untouched instead of synthesizing an invalid dispatcher.
        if module.functions.iter().any(|func| {
            func.selector.is_some() && !func.blocks.is_empty() && !func.params.is_empty()
        }) {
            return false;
        }

        // Collect the routable external wrappers: a selector plus a body. After
        // the ABI phase every such wrapper is argument-free.
        let mut routes: Vec<(u32, FunctionId)> = Vec::new();
        for (id, func) in module.functions.iter_enumerated() {
            let Some(selector) = func.selector else { continue };
            if func.blocks.is_empty() {
                continue;
            }
            routes.push((u32::from_be_bytes(selector), id));
        }
        if routes.is_empty() {
            return false;
        }
        routes.sort_by_key(|(selector, _)| *selector);

        self.build_entry(module, &routes);
        self.stats.routed = routes.len();
        module.advance_phase(MirPhase::Dispatch);
        true
    }

    /// Synthesizes the `entry` dispatcher function and appends it to the module.
    fn build_entry(&self, module: &mut Module, routes: &[(u32, FunctionId)]) {
        let mut entry = Function::new(Ident::with_dummy_span(Symbol::intern("entry")));

        // One call block per selector, plus a shared revert default. Allocate
        // the blocks up front so the switch can target them.
        let case_blocks: Vec<_>;
        let default_block;
        let selector;
        {
            let mut builder = FunctionBuilder::new(&mut entry);
            selector = load_selector(&mut builder);
            case_blocks = routes.iter().map(|_| builder.create_block()).collect();
            default_block = builder.create_block();

            let cases = routes
                .iter()
                .zip(&case_blocks)
                .map(|((sel, _), block)| (builder.imm_u64(u64::from(*sel)), *block))
                .collect();
            builder.switch(selector, default_block, cases);

            // Default: no selector matched — revert with empty data.
            builder.switch_to_block(default_block);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);

            // Each case routes to its argument-free wrapper.
            //
            // TODO: this uses `internal_call` as a text-model placeholder. A
            // wrapper terminates externally (`RETURN`/`REVERT`), so this is not a
            // real backend-ready call edge: an internal call expects control to
            // return, but a wrapper never does. Making dispatch backend-ready
            // needs a tail-call/jump-like MIR edge to the wrapper, or dispatch
            // should target the body functions and encode returndata in the case
            // block itself. The trailing `stop` keeps the block well-formed for
            // the current model, where the backend does not yet consume this
            // phase.
            for ((_, target), block) in routes.iter().zip(&case_blocks) {
                builder.switch_to_block(*block);
                builder.internal_call_void(*target, Vec::new(), 0);
                builder.stop();
            }
        }

        module.add_function(entry);
    }
}

/// Emits `calldataload(0) >> 224`, the 4-byte function selector.
fn load_selector(builder: &mut FunctionBuilder<'_>) -> ValueId {
    let zero = builder.imm_u64(0);
    let word = builder.calldataload(zero);
    let shift = builder.imm_u64(224);
    builder.shr(shift, word)
}

impl ModulePass for LowerDispatchPass {
    fn name(&self) -> &str {
        "lower-dispatch"
    }

    fn run(&mut self, module: &mut Module) -> bool {
        Self::run(self, module)
    }
}

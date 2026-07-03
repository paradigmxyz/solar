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
        // Idempotent: only build the dispatcher once.
        if module.phase >= MirPhase::Dispatch {
            return false;
        }

        // Collect the routable external wrappers: a selector plus a body, and no
        // MIR arguments (the ABI phase has already absorbed them). Anything with
        // arguments is left for the backend, since this switch cannot supply
        // them.
        let mut routes: Vec<(u32, FunctionId)> = module
            .functions
            .iter_enumerated()
            .filter_map(|(id, func)| {
                let selector = func.selector?;
                (func.params.is_empty() && !func.blocks.is_empty())
                    .then(|| (u32::from_be_bytes(selector), id))
            })
            .collect();
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

            // Each case tail-calls its argument-free wrapper. The wrapper ends in
            // its own RETURN/REVERT, so the dispatcher never returns to the case
            // block; a `stop` keeps the block well-formed.
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

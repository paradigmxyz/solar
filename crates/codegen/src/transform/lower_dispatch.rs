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
use solar_interface::{Ident, sym};

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

        // Dispatch routes to the argument-free ABI wrappers, so it requires the
        // ABI phase. Running on `built`/`optimized` MIR would leave
        // argument-taking external functions unroutable while still advancing
        // the phase; require the precondition and bail otherwise.
        if module.phase < MirPhase::Abi {
            return false;
        }

        // Collect the routable external wrappers: a selector plus a body. After
        // the ABI phase every such wrapper is argument-free; assert that rather
        // than silently skipping, since a leftover argument-taking selector
        // function would mean the ABI invariant was violated.
        let mut routes: Vec<(u32, FunctionId)> = Vec::new();
        for (id, func) in module.functions.iter_enumerated() {
            let Some(selector) = func.selector else { continue };
            if func.blocks.is_empty() {
                continue;
            }
            debug_assert!(
                func.params.is_empty(),
                "dispatch after abi phase: selector function `{}` still takes arguments",
                func.name
            );
            routes.push((u32::from_be_bytes(selector), id));
        }
        routes.sort_by_key(|(selector, _)| *selector);

        // Receive/fallback entries, mirroring the backend dispatcher: only
        // bodied declarations participate in runtime dispatch. A fallback with
        // the `fallback(bytes) returns (bytes)` shape takes an argument this
        // switch cannot supply; bail all-or-nothing rather than half-routing.
        let special = |pick: fn(&Function) -> bool| {
            module
                .functions
                .iter_enumerated()
                .find(|(_, f)| pick(f) && !f.blocks.is_empty())
                .map(|(id, _)| id)
        };
        let receive = special(|f| f.attributes.is_receive);
        let fallback = special(|f| f.attributes.is_fallback);
        for id in [receive, fallback].into_iter().flatten() {
            if !module.function(id).params.is_empty() {
                return false;
            }
        }
        if routes.is_empty() && receive.is_none() && fallback.is_none() {
            return false;
        }

        // Hoist the callvalue check when every external entry rejects value,
        // exactly like the backend dispatcher does.
        let externals =
            routes.iter().map(|&(_, id)| id).chain(receive).chain(fallback).collect::<Vec<_>>();
        let hoist_callvalue = externals.iter().all(|&id| rejects_callvalue(module.function(id)));

        self.build_entry(module, &routes, receive, fallback, hoist_callvalue);
        self.stats.routed = routes.len();
        module.advance_phase(MirPhase::Dispatch);
        true
    }

    /// Synthesizes the `entry` dispatcher function and appends it to the module.
    ///
    /// Mirrors the backend dispatcher's semantics: an optional hoisted
    /// callvalue check when every entry rejects value, empty calldata routed to
    /// `receive`, then `fallback`, then revert, the selector switch defaulting
    /// to `fallback` or revert, and per-entry callvalue checks when the hoisted
    /// check does not apply. Short calldata (under 4 bytes but not empty) takes
    /// the selector path and falls through to the default, like the backend.
    fn build_entry(
        &self,
        module: &mut Module,
        routes: &[(u32, FunctionId)],
        receive: Option<FunctionId>,
        fallback: Option<FunctionId>,
        hoist_callvalue: bool,
    ) {
        let rejects: Vec<bool> =
            routes.iter().map(|&(_, id)| rejects_callvalue(module.function(id))).collect();
        let fallback_rejects = fallback.is_some_and(|id| rejects_callvalue(module.function(id)));

        let mut entry = Function::new(Ident::with_dummy_span(sym::entry));
        {
            let mut builder = FunctionBuilder::new(&mut entry);

            let size_block = builder.create_block();
            let empty_block = builder.create_block();
            let select_block = builder.create_block();
            let case_blocks: Vec<_> = routes.iter().map(|_| builder.create_block()).collect();
            let default_block = builder.create_block();
            let revert_block = builder.create_block();

            // Optional hoisted callvalue check.
            if hoist_callvalue {
                let value = builder.callvalue();
                builder.branch(value, revert_block, size_block);
            } else {
                builder.jump(size_block);
            }

            // Empty calldata: receive, else fallback, else revert.
            builder.switch_to_block(size_block);
            let size = builder.calldatasize();
            let zero = builder.imm_u64(0);
            let is_empty = builder.eq(size, zero);
            builder.branch(is_empty, empty_block, select_block);

            builder.switch_to_block(empty_block);
            match (receive, fallback) {
                (Some(target), _) => builder.tail_call(target, Vec::new()),
                (None, Some(target)) => {
                    self.guarded_tail_call(
                        &mut builder,
                        target,
                        fallback_rejects && !hoist_callvalue,
                        revert_block,
                    );
                }
                (None, None) => builder.jump(revert_block),
            }

            // Selector switch; the default goes to the fallback when present.
            builder.switch_to_block(select_block);
            let selector = load_selector(&mut builder);
            let cases = routes
                .iter()
                .zip(&case_blocks)
                .map(|((sel, _), block)| (builder.imm_u64(u64::from(*sel)), *block))
                .collect();
            builder.switch(selector, default_block, cases);

            builder.switch_to_block(default_block);
            if let Some(target) = fallback {
                self.guarded_tail_call(
                    &mut builder,
                    target,
                    fallback_rejects && !hoist_callvalue,
                    revert_block,
                );
            } else {
                builder.jump(revert_block);
            }

            // Each case tail-calls its argument-free wrapper, with a callvalue
            // check first when the entry rejects value and the check was not
            // hoisted.
            for (((_, target), block), rejects_value) in
                routes.iter().zip(&case_blocks).zip(&rejects)
            {
                builder.switch_to_block(*block);
                self.guarded_tail_call(
                    &mut builder,
                    *target,
                    *rejects_value && !hoist_callvalue,
                    revert_block,
                );
            }

            builder.switch_to_block(revert_block);
            let zero = builder.imm_u64(0);
            builder.revert(zero, zero);
        }

        module.add_function(entry);
    }

    /// Tail-calls `target`, first rejecting nonzero callvalue when `check`.
    fn guarded_tail_call(
        &self,
        builder: &mut FunctionBuilder<'_>,
        target: FunctionId,
        check: bool,
        revert_block: crate::mir::BlockId,
    ) {
        if check {
            let go = builder.create_block();
            let value = builder.callvalue();
            builder.branch(value, revert_block, go);
            builder.switch_to_block(go);
        }
        builder.tail_call(target, Vec::new());
    }
}

/// Whether an external entry must reject nonzero callvalue, mirroring the
/// backend dispatcher's rule.
fn rejects_callvalue(func: &Function) -> bool {
    use solar_sema::hir::StateMutability;
    matches!(
        func.attributes.state_mutability,
        StateMutability::NonPayable | StateMutability::View | StateMutability::Pure
    )
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

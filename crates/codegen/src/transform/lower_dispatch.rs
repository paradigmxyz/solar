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
//! to run after [`super::lower_abi::LowerAbi`], which turns external functions into the
//! argument-free self-decoding wrappers this switch routes to; that is why it
//! only routes selector-bearing functions that take no MIR arguments.
//!
//! It requires the `abi` phase: it routes to the argument-free wrappers that
//! [`super::lower_abi::LowerAbi`] produces, so it bails on `built`/`optimized` modules
//! rather than half-dispatching argument-taking functions.
//!
//! This pass runs after [`super::lower_abi::LowerAbi`] in the codegen pipeline. The
//! backend consumes the resulting `dispatch`-or-later module and uses its
//! `entry` function instead of synthesizing a dispatcher.

use crate::{
    mir::{Function, FunctionBuilder, FunctionId, MirPhase, Module, ValueId},
    pass::MirPass,
};
use solar_interface::{Ident, sym};

/// Dispatch phase lowering pass.
pub(crate) struct LowerDispatch;

impl MirPass for LowerDispatch {
    fn name(&self) -> &'static str {
        "lower-dispatch"
    }

    fn is_enabled(&self, _gcx: solar_sema::Gcx<'_>, module: &Module) -> bool {
        module.phase == MirPhase::Abi
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, _gcx: solar_sema::Gcx<'_>, module: &mut Module) -> bool {
        LowerDispatchCx::default().run(module)
    }
}

/// Statistics from dispatch lowering.
#[derive(Clone, Debug, Default)]
struct LowerDispatchStats {
    /// Number of selector cases routed by the synthesized `entry` function.
    routed: usize,
}

#[derive(Debug, Default)]
struct LowerDispatchCx {
    stats: LowerDispatchStats,
}

impl LowerDispatchCx {
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

        // Collect the routable external wrappers. After the ABI phase every
        // such wrapper is argument-free; assert that rather
        // than silently skipping, since a leftover argument-taking selector
        // function would mean the ABI invariant was violated.
        let mut routes: Vec<(u32, FunctionId)> = Vec::new();
        let mut receive = None;
        let mut fallback = None;
        let mut callvalue = super::utils::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if func.attributes.is_receive && receive.is_none() {
                receive = Some(id);
            }
            if func.attributes.is_fallback && fallback.is_none() {
                fallback = Some(id);
            }
            if let Some(selector) = func.selector {
                debug_assert!(
                    func.params.is_empty(),
                    "dispatch after abi phase: selector function `{}` still takes arguments",
                    func.name
                );
                routes.push((u32::from_be_bytes(selector), id));
            }
        }
        routes.sort_by_key(|(selector, _)| *selector);

        // A fallback with the `fallback(bytes) returns (bytes)` shape takes an
        // argument this switch cannot supply; bail all-or-nothing rather than half-routing.
        for id in [receive, fallback].into_iter().flatten() {
            if !module.function(id).params.is_empty() {
                return false;
            }
        }
        if routes.is_empty() && receive.is_none() && fallback.is_none() {
            return false;
        }

        // Hoist the callvalue check when every external entry rejects value,
        // exactly like the backend dispatcher does. When the hoist does not
        // apply, the selector cases route unguarded: `lower-abi` already
        // injected the check into each rejecting wrapper's prologue (the two
        // passes share this predicate).
        let hoist_callvalue = callvalue.hoists();

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
        let fallback_rejects =
            fallback.is_some_and(|id| super::utils::rejects_callvalue(module.function(id)));

        let mut entry = Function::new(Ident::with_dummy_span(sym::entry));
        {
            let mut builder = FunctionBuilder::new(&mut entry);

            let size_block = builder.create_block();
            // With no receive and no fallback there is no empty-calldata
            // entry: empty calldata branches straight to the revert, and
            // keeping `select_block` the fallthrough lets the backend invert
            // the size check into the shared revert stub — the backend
            // dispatcher's exact shape.
            let empty_block =
                (receive.is_some() || fallback.is_some()).then(|| builder.create_block());
            let select_block = builder.create_block();
            let case_blocks: Vec<_> = routes.iter().map(|_| builder.create_block()).collect();
            let default_block = fallback.map(|_| builder.create_block());
            let revert_block = builder.create_block();

            // Optional hoisted callvalue check.
            if hoist_callvalue {
                let value = builder.callvalue();
                builder.branch(value, revert_block, size_block);
            } else {
                builder.jump(size_block);
            }

            // Empty calldata: receive, else fallback, else revert. Nonzero
            // calldatasize is the branch condition itself; no comparison.
            builder.switch_to_block(size_block);
            let size = builder.calldatasize();
            builder.branch(size, select_block, empty_block.unwrap_or(revert_block));

            if let Some(empty_block) = empty_block {
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
                    (None, None) => unreachable!("empty_block exists without receive or fallback"),
                }
            }

            // Selector switch; the default goes to the fallback when present.
            builder.switch_to_block(select_block);
            let selector = load_selector(&mut builder);
            let cases = routes
                .iter()
                .zip(&case_blocks)
                .map(|((sel, _), block)| (builder.imm_u64(u64::from(*sel)), *block))
                .collect();
            builder.switch(selector, default_block.unwrap_or(revert_block), cases);

            if let Some(default_block) = default_block
                && let Some(target) = fallback
            {
                builder.switch_to_block(default_block);
                self.guarded_tail_call(
                    &mut builder,
                    target,
                    fallback_rejects && !hoist_callvalue,
                    revert_block,
                );
            }

            // Each case tail-calls its argument-free wrapper directly. A
            // rejecting wrapper carries its own callvalue check in its
            // prologue (injected by `lower-abi`) whenever the hoisted check
            // does not apply, so no per-case guard is needed.
            for ((_, target), block) in routes.iter().zip(&case_blocks) {
                builder.switch_to_block(*block);
                builder.tail_call(*target, Vec::new());
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

/// Emits `calldataload(0) >> 224`, the 4-byte function selector.
fn load_selector(builder: &mut FunctionBuilder<'_>) -> ValueId {
    let zero = builder.imm_u64(0);
    let word = builder.calldataload(zero);
    let shift = builder.imm_u64(224);
    builder.shr(shift, word)
}

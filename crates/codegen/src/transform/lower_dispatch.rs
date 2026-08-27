//! Dispatch phase lowering: materialize the selector switch as MIR.
//!
//! In `built` MIR, selector routing is still implicit. This pass
//! makes it an ordinary MIR function named `entry` (the dispatch phase of the
//! sketch in [`crate::mir::MirPhase`]).
//!
//! The synthesized `entry` function loads the 4-byte selector from
//! `calldataload(0)` and switches on it to one argument-free `tail_call`
//! per external wrapper, defaulting to a `revert`. It is meant
//! to run after [`super::lower_abi::LowerAbi`], which turns external functions into the
//! argument-free self-decoding wrappers this switch routes to; that is why it
//! only routes selector-bearing functions that take no MIR arguments.
//!
//! It requires the `abi` phase because it routes to the argument-free wrappers
//! that [`super::lower_abi::LowerAbi`] produces.
//!
//! This pass runs after [`super::lower_abi::LowerAbi`] in the codegen pipeline.
//! The backend only consumes the final `evm-shaped` module.

use crate::{
    mir::{Function, FunctionBuilder, FunctionId, Module, ValueId},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_interface::{Ident, sym};

/// Dispatch phase lowering pass.
pub(crate) struct LowerDispatch;

impl MirPass for LowerDispatch {
    fn name(&self) -> &'static str {
        "lower-dispatch"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let changed = LowerDispatchCx {
            has_bitwise_shifting: gcx.sess.opts.evm_version.has_bitwise_shifting(),
        }
        .run(module);
        module.advance_phase();
        changed
    }
}

#[derive(Debug)]
struct LowerDispatchCx {
    has_bitwise_shifting: bool,
}

impl LowerDispatchCx {
    fn run(&mut self, module: &mut Module) -> bool {
        // Collect the routable external wrappers.
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
                routes.push((u32::from_be_bytes(selector), id));
            }
        }
        routes.sort_by_key(|(selector, _)| *selector);

        if routes.is_empty() && receive.is_none() && fallback.is_none() && module.is_library {
            return false;
        }

        // Hoist the callvalue check when every external entry rejects value.
        // When the hoist does not apply, the selector cases route unguarded:
        // `lower-abi` already injected the check into each rejecting wrapper's
        // prologue (the two passes share this predicate).
        let hoist_callvalue = callvalue.hoists();

        self.build_entry(module, &routes, receive, fallback, hoist_callvalue);
        true
    }

    /// Synthesizes the `entry` routing function and appends it to the module.
    ///
    /// It includes an optional hoisted callvalue check when every entry rejects
    /// value, routes empty calldata to `receive`, rejects zero-padded short
    /// selector matches, and defaults the selector switch to `fallback` or
    /// revert.
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
        // `CALLDATALOAD(0)` right-pads short calldata with zeroes before the
        // selector extraction. A short input can therefore match a selector
        // only when its final byte is zero; guard all short inputs if any
        // route has that suffix. Solc emits this guard for every dispatch,
        // but the selective form avoids it when no route can collide.
        let needs_short_calldata_guard = routes.iter().any(|(selector, _)| selector & 0xff == 0);
        let needs_size_dispatch = receive.is_some() || needs_short_calldata_guard;

        let mut entry = Function::new(Ident::with_dummy_span(sym::entry));
        entry.attributes.is_dispatch_entry = true;
        {
            let mut builder = FunctionBuilder::new(&mut entry);

            let size_block = needs_size_dispatch.then(|| builder.create_block());
            let short_size_block =
                (receive.is_some() && needs_short_calldata_guard).then(|| builder.create_block());
            let receive_block = receive.map(|_| builder.create_block());
            let select_block = builder.create_block();
            let case_blocks: Vec<_> = routes.iter().map(|_| builder.create_block()).collect();
            let default_block = fallback.map(|_| builder.create_block());
            let revert_block = builder.create_block();
            let dispatch_block = size_block.unwrap_or(select_block);

            // Optional hoisted callvalue check.
            if hoist_callvalue {
                let value = builder.callvalue();
                builder.branch(value, revert_block, dispatch_block);
            } else {
                builder.jump(dispatch_block);
            }

            if let Some(size_block) = size_block {
                builder.switch_to_block(size_block);
                let size = builder.calldatasize();
                if receive.is_some() {
                    builder.branch(
                        size,
                        short_size_block.unwrap_or(select_block),
                        receive_block.expect("receive block must exist"),
                    );
                } else {
                    let selector_size = builder.imm_u64(4);
                    let short = builder.lt(size, selector_size);
                    builder.branch(short, default_block.unwrap_or(revert_block), select_block);
                }
            }

            if let Some(short_size_block) = short_size_block {
                builder.switch_to_block(short_size_block);
                let size = builder.calldatasize();
                let selector_size = builder.imm_u64(4);
                let short = builder.lt(size, selector_size);
                builder.branch(short, default_block.unwrap_or(revert_block), select_block);
            }

            if let Some(receive_block) = receive_block
                && let Some(target) = receive
            {
                builder.switch_to_block(receive_block);
                builder.tail_call(target, Vec::new());
            }

            // Selector switch; the default goes to the fallback when present.
            builder.switch_to_block(select_block);
            if routes.is_empty() {
                builder.jump(default_block.unwrap_or(revert_block));
            } else {
                let selector = self.load_selector(&mut builder);
                let cases = routes
                    .iter()
                    .zip(&case_blocks)
                    .map(|((sel, _), block)| (builder.imm_u64(u64::from(*sel)), *block))
                    .collect();
                builder.switch(selector, default_block.unwrap_or(revert_block), cases);
            }

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

    /// Loads the 4-byte function selector from the first calldata word.
    fn load_selector(&self, builder: &mut FunctionBuilder<'_>) -> ValueId {
        let zero = builder.imm_u64(0);
        let word = builder.calldataload(zero);
        if self.has_bitwise_shifting {
            let shift = builder.imm_u64(224);
            builder.shr(shift, word)
        } else {
            let divisor = builder.imm_u256(U256::from(1) << 224);
            builder.div(word, divisor)
        }
    }
}

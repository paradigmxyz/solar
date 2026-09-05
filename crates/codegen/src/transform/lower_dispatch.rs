//! Dispatch phase lowering: materialize the selector switch as MIR.
//!
//! In `built`/`optimized` MIR, selector routing is still implicit. This pass
//! makes it an ordinary MIR function named `entry` (the dispatch phase of the
//! sketch in [`MirPhase`]).
//!
//! The synthesized `entry` function loads the 4-byte selector through a
//! semantic calldata slice and switches on it to one argument-free `icall`
//! per external wrapper, defaulting to a `revert`. It is meant
//! to run after [`super::lower_abi::LowerAbi`], which turns external functions into the
//! argument-free self-decoding wrappers this switch routes to; that is why it
//! only routes selector-bearing functions that take no MIR arguments.
//!
//! It requires the `abi` phase: it routes to the argument-free wrappers that
//! [`super::lower_abi::LowerAbi`] produces, so it bails on `built`/`optimized` modules
//! rather than half-dispatching argument-taking functions.
//!
//! This pass runs after [`super::lower_abi::LowerAbi`] in the codegen pipeline.
//! The backend only consumes the final `evm-shaped` module.

use crate::{
    mir::{Function, FunctionBuilder, FunctionId, MirPhase, Module, RevertReason, ValueId},
    pass::MirPass,
};
use alloy_primitives::U256;
use solar_config::RevertStrings;
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

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        lower_dispatch(
            module,
            gcx.sess.opts.evm_version.has_bitwise_shifting(),
            gcx.sess.opts.revert_strings,
        )
    }
}

fn lower_dispatch(
    module: &mut Module,
    has_bitwise_shifting: bool,
    revert_strings: RevertStrings,
) -> bool {
    // Dispatch routes to the argument-free ABI wrappers, so it requires the
    // ABI phase. Running on `built`/`optimized` MIR would leave
    // argument-taking external functions unroutable while still advancing
    // the phase; require the precondition and bail otherwise.
    if module.phase != MirPhase::Abi {
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

    // Any fallback that still takes parameters was outside the ABI pass's
    // supported wrapper shapes; bail rather than routing it incorrectly.
    for id in [receive, fallback].into_iter().flatten() {
        if !module.function(id).params.is_empty() {
            return false;
        }
    }
    // Hoist the callvalue check when every external entry rejects value.
    // When the hoist does not apply, the selector cases route unguarded:
    // `lower-abi` already injected the check into each rejecting wrapper's
    // prologue (the two passes share this predicate).
    let hoist_callvalue = callvalue.hoists();

    build_entry(
        module,
        &routes,
        receive,
        fallback,
        hoist_callvalue,
        has_bitwise_shifting,
        revert_strings,
    );
    module.advance_phase(MirPhase::Dispatch);
    true
}

/// Synthesizes the `entry` routing function and appends it to the module.
///
/// It includes an optional hoisted callvalue check when every entry rejects
/// value, routes empty calldata to `receive`, rejects zero-padded short
/// selector matches, and defaults the selector switch to `fallback` or
/// revert.
fn build_entry(
    module: &mut Module,
    routes: &[(u32, FunctionId)],
    receive: Option<FunctionId>,
    fallback: Option<FunctionId>,
    hoist_callvalue: bool,
    has_bitwise_shifting: bool,
    revert_strings: RevertStrings,
) {
    // `CALLDATALOAD(0)` right-pads short calldata with zeroes before the
    // selector extraction. A short input can therefore match a selector
    // only when its final byte is zero; guard all short inputs if any
    // route has that suffix. Solc emits this guard for every dispatch,
    // but the selective form avoids it when no route can collide.
    let needs_short_calldata_guard = routes.iter().any(|(selector, _)| selector & 0xff == 0);

    let mut entry = Function::new(Ident::with_dummy_span(sym::entry));
    {
        let mut builder = FunctionBuilder::new(&mut entry).with_revert_strings(revert_strings);

        let receive_size_block = receive.map(|_| builder.create_block());
        let selector_size_block = needs_short_calldata_guard.then(|| builder.create_block());
        let receive_block = receive.map(|target| (target, builder.create_block()));
        let select_block = builder.create_block();
        let case_blocks: Vec<_> = routes.iter().map(|_| builder.create_block()).collect();
        let fallback_block = fallback.map(|target| (target, builder.create_block()));
        let revert_block = builder.create_block();
        // Rejected Ether and unknown selectors share one empty revert unless revert reasons are
        // encoded, in which case each gets its own message.
        let callvalue_revert_block =
            if revert_strings.is_debug() { builder.create_block() } else { revert_block };
        let default_block = fallback_block.as_ref().map_or(revert_block, |&(_, block)| block);
        let dispatch_block = receive_size_block.or(selector_size_block).unwrap_or(select_block);

        // Optional hoisted callvalue check.
        if hoist_callvalue {
            let value = builder.callvalue();
            builder.branch(value, callvalue_revert_block, dispatch_block);
        } else {
            builder.jump(dispatch_block);
        }

        if let Some(receive_size_block) = receive_size_block {
            builder.switch_to_block(receive_size_block);
            let size = builder.calldatasize();
            builder.branch(
                size,
                selector_size_block.unwrap_or(select_block),
                receive_block.expect("receive block must exist").1,
            );
        }

        if let Some(selector_size_block) = selector_size_block {
            builder.switch_to_block(selector_size_block);
            let size = builder.calldatasize();
            let selector_size = builder.imm(4);
            let short = builder.lt(size, selector_size);
            builder.branch(short, default_block, select_block);
        }

        if let Some((target, receive_block)) = receive_block {
            builder.switch_to_block(receive_block);
            builder.tail_call(target, Vec::new());
        }

        // Selector switch; the default goes to the fallback when present.
        builder.switch_to_block(select_block);
        if routes.is_empty() {
            builder.jump(default_block);
        } else {
            let selector = load_selector(&mut builder, has_bitwise_shifting);
            let cases = routes
                .iter()
                .zip(&case_blocks)
                .map(|((sel, _), block)| (builder.imm(u64::from(*sel)), *block))
                .collect();
            builder.switch(selector, default_block, cases);
        }

        if let Some((target, default_block)) = fallback_block {
            builder.switch_to_block(default_block);
            if !hoist_callvalue && super::utils::rejects_callvalue(module.function(target)) {
                let go = builder.create_block();
                let value = builder.callvalue();
                builder.branch(value, callvalue_revert_block, go);
                builder.switch_to_block(go);
            }
            builder.tail_call(target, Vec::new());
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
        // solc distinguishes a contract that can at least receive Ether from one that
        // rejects every call.
        builder.revert_with(if receive.is_some() {
            RevertReason::UnknownSelector
        } else {
            RevertReason::NoFallbackNorReceive
        });
        if callvalue_revert_block != revert_block {
            builder.switch_to_block(callvalue_revert_block);
            builder.revert_with(RevertReason::EtherSentToNonPayable);
        }
    }

    let entry = module.add_function(entry);
    module.set_dispatch_entry(entry);
}

/// Loads the 4-byte function selector from the first calldata word.
fn load_selector(builder: &mut FunctionBuilder<'_>, has_bitwise_shifting: bool) -> ValueId {
    let zero = builder.imm(0);
    let word = builder.calldataload(zero);
    if has_bitwise_shifting {
        let shift = builder.imm(224);
        builder.shr(shift, word)
    } else {
        let divisor = builder.imm(U256::from(1) << 224);
        builder.div(word, divisor)
    }
}

//! ABI phase lowering: materialize calldata decode / returndata encode as MIR.
//!
//! In `built` MIR an external function takes typed MIR arguments and
//! returns typed values; the calldata decode and returndata encode happen
//! implicitly in the backend. This pass makes that explicit, moving the ABI
//! boundary into MIR itself (the ABI phase of the sketch in [`MirPhase`]).
//!
//! For each external entry `f(x0: T0, .., xn: Tn)`, it:
//!
//! 1. copies the original into a fresh internal function `f.body` with its parameter list preserved
//!    when there are internal callers, and
//! 2. strips `f`'s MIR parameter list, keeping its selector and its `Value::Arg` entries. Scalar
//!    arguments remain lazy ABI head words; dynamic calldata arguments remain logical slices until
//!    `lower-slices` projects their pointer and length. Value-carrying returns are ABI-encoded
//!    according to the function's return layout and terminate with `returndata`.
//!
//! The wrapper keeps argument materialization lazy so values used after a
//! branch can still be rematerialized instead of spilled. Dynamic return
//! encoding becomes a semantic `abi_encode` operation here and lowers later;
//! static returns use the fixed low-memory return buffer directly. Internal
//! call sites that targeted a wrapped function are retargeted to its extracted
//! raw-return body, so internal calls to public functions keep their convention.
//!
//! Together with [`super::lower_dispatch::LowerDispatch`], which routes a selector switch
//! to these argument-free wrappers, this materializes the ABI boundary before
//! EVM codegen. Both passes must complete before the backend runs.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        BlockId, Function, FunctionBuilder, FunctionId, InstKind, MangledSymbol, MirPhase, Module,
        Terminator,
    },
    pass::MirPass,
};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_interface::{Span, Symbol};

/// ABI phase lowering pass.
pub(crate) struct LowerAbi;

impl MirPass for LowerAbi {
    fn name(&self) -> &'static str {
        "lower-abi"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        _analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        LowerAbiCx.run(module)
    }
}

struct LowerAbiCx;

impl LowerAbiCx {
    fn run(&mut self, module: &mut Module) -> bool {
        let mut targets = Vec::new();
        let mut internally_called = DenseBitSet::new_empty(module.functions.len());
        let mut callvalue = super::utils::DispatchCallvalue::default();
        for (id, func) in module.functions.iter_enumerated() {
            callvalue.observe(func);
            if is_wrappable_external(func) {
                targets.push(id);
            }
            for inst_id in func.instructions() {
                if let InstKind::InternalCall { function, .. } = func.inst(inst_id).kind {
                    internally_called.insert(function);
                }
            }
        }

        if targets.is_empty() {
            module.advance_phase(MirPhase::Abi);
            return false;
        }

        // Most external functions are never called internally. Only those
        // that are need a second, parameterized body; cloning every wrapper
        // needlessly grows the MIR consumed by all subsequent lowering and
        // backend passes.
        // When the dispatch entry cannot hoist a single callvalue check, each
        // rejecting wrapper carries its own. The check belongs to the wrapper's
        // prologue (falling through into the body) rather than to a guard block
        // in the selector switch, which would pay an extra jump per case.
        // `lower-dispatch` shares the predicate and routes selector cases
        // unguarded.
        let hoist_callvalue = callvalue.hoists();

        let mut changed = false;
        let mut body_of_wrapper = FxHashMap::default();
        for id in targets {
            let (body_id, wrapper_changed) =
                Self::wrap_function(module, id, internally_called.contains(id));
            changed |= wrapper_changed;
            if let Some(body_id) = body_id {
                body_of_wrapper.insert(id, body_id);
            }
            if !hoist_callvalue && super::utils::rejects_callvalue(module.function(id)) {
                Self::inject_callvalue_check(module.function_mut(id));
                changed = true;
            }
        }

        // Internal calls to a wrapped public/external function must keep the
        // original call semantics: retarget them to the extracted body. The
        // wrappers' own calls already target the bodies and are not affected.
        if !body_of_wrapper.is_empty() {
            for func in module.functions.iter_mut() {
                func.for_each_instruction_mut(|_, inst| {
                    if let InstKind::InternalCall { function, .. } = &mut inst.kind
                        && let Some(&body_id) = body_of_wrapper.get(function)
                    {
                        *function = body_id;
                    }
                });
            }
        }

        module.advance_phase(MirPhase::Abi);
        changed
    }

    /// Rewrites one external function into a self-decoding form, keeping a
    /// pristine copy for internal callers.
    ///
    /// The original function keeps its selector and loses its MIR parameter
    /// and return lists, but its `Value::Arg` entries stay in place. Scalar arguments
    /// continue to denote ABI head words, while logical calldata slices are
    /// projected by `lower-slices`; both forms preserve lazy per-use
    /// rematerialization, so wrapper arguments do not spill.
    /// Materializing the loads as eager MIR instructions instead was measured
    /// to cost real bytes: an instruction result is not rematerializable, so
    /// every multi-use or cross-block argument bought spill traffic the
    /// `Arg` form avoids. The explicit-decode representation returns when
    /// slices provide explicit high-level decode semantics without changing
    /// that backend property. Return values are ABI-encoded in place, and no
    /// internal call is introduced on the external path. When the function has
    /// internal callers, a pristine `.body` copy with raw returns and parameters
    /// preserved is appended and those callers are retargeted to it.
    fn wrap_function(
        module: &mut Module,
        wrapper_id: FunctionId,
        needs_body: bool,
    ) -> (Option<FunctionId>, bool) {
        let wrapper = module.function(wrapper_id);
        let changed = needs_body
            || !wrapper.params.is_empty()
            || !wrapper.returns.is_empty()
            || wrapper.abi_returns.is_some();

        // The copy must precede wrapper mutation and callvalue injection so
        // internal callers keep the original function semantics.
        let body_id = needs_body.then(|| {
            let mut body = module.function(wrapper_id).clone();
            body.name = MangledSymbol::new(Symbol::intern(&format!("{}.body", body.name.symbol)));
            body.name_span = Span::DUMMY;
            body.selector = None;
            body.abi_returns = None;
            body.attributes.visibility = solar_sema::hir::Visibility::Internal;
            module.add_function(body)
        });

        encode_live_returns(module.function_mut(wrapper_id));

        // The wrapper takes no MIR arguments; its `Arg` values now read the
        // calldata head words directly.
        let wrapper = module.function_mut(wrapper_id);
        wrapper.params.clear();
        wrapper.returns.clear();
        wrapper.abi_returns = None;
        (body_id, changed)
    }

    /// Prepends `if callvalue() != 0 { revert(0, 0) }` to a wrapper.
    ///
    /// The new guard block becomes the entry and falls through into the old
    /// body, so the check costs no extra jump. Injected after the `.body` copy
    /// is taken: internal callers never pay the check.
    fn inject_callvalue_check(func: &mut Function) {
        let old_entry = BlockId::ENTRY;
        let mut builder = FunctionBuilder::new(func);
        let guard = builder.create_block();
        let revert = builder.create_block();
        builder.switch_to_block(guard);
        let value = builder.callvalue();
        builder.branch(value, revert, old_entry);
        builder.switch_to_block(revert);
        let zero = builder.imm_u64(0);
        builder.revert(zero, zero);

        let order = std::iter::once(guard)
            .chain(func.blocks.indices().filter(|&block| block != guard))
            .collect::<Vec<_>>();
        crate::mir::utils::remap_block_order(func, &order);
    }
}

/// An external entry with a body and a selector — the shape a wrapper is built
/// for. Receive/fallback entries have no selector and need no ABI wrapper.
fn is_wrappable_external(func: &Function) -> bool {
    func.selector.is_some() && !func.attributes.is_constructor
}

/// Rewrites value-carrying returns into a semantic ABI encode followed by
/// `returndata(slice_ptr(encoded), slice_len(encoded))`.
fn encode_live_returns(func: &mut Function) -> usize {
    let layout = func.abi_returns.clone();
    let block_ids: Vec<_> = func.blocks.indices().collect();
    let mut encoded_returns = 0;
    for block_id in block_ids {
        let values = match func.blocks[block_id].terminator.take() {
            Some(Terminator::Return { values }) if !values.is_empty() => {
                values.into_vec().into_boxed_slice()
            }
            Some(terminator) => {
                func.blocks[block_id].terminator = Some(terminator);
                continue;
            }
            None => continue,
        };
        let layout = layout.as_ref().expect("value-returning ABI entry must have a return layout");
        assert_eq!(
            layout.types.len(),
            values.len(),
            "ABI return layout must match the returned values"
        );
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block_id);
        if layout.types.iter().any(crate::mir::AbiType::is_dynamic) {
            let encoded = builder.abi_encode(layout.clone(), None, values);
            let offset = builder.slice_ptr(encoded);
            let size = builder.slice_len(encoded);
            builder.ret_data(offset, size);
        } else {
            let offset = builder.imm_u64(EvmMemoryLayout::HEAP_START);
            let size = super::lower_abi_encode::encode_tuple(
                &mut builder,
                &values,
                &layout.types,
                offset,
                super::lower_abi_encode::AbiScratch { base: None, depth: 0 },
            );
            builder.ret_data(offset, size);
        }
        encoded_returns += 1;
    }
    encoded_returns
}

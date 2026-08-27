//! Progressive MIR lowering through one phase-driven pass.

use crate::{
    analysis::{CallGraphInfo, TailCallEligibility},
    memory::EvmMemoryLayout,
    mir::{
        BlockId, Function, FunctionBuilder, FunctionId, InstKind, MangledSymbol, MirPhase, Module,
        Terminator, ValueId, utils::repair_reachability_phis,
    },
    pass::{MirPass, ModuleAnalyses},
    transform::{
        cfg_simplify::remove_unreachable_blocks, lower_alloc::lower_alloc,
        lower_mcopy::lower_mcopy_module, lower_memory_objects::lower_memory_objects,
        lower_memory_zero::lower_memory_zero,
    },
};
use alloy_primitives::U256;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use solar_interface::{Ident, Span, Symbol, sym};
use solar_sema::Gcx;

/// Advances MIR through its next lowering phase.
pub(crate) struct Lower;

impl MirPass for Lower {
    fn name(&self) -> &'static str {
        "lower"
    }

    fn is_required(&self) -> bool {
        true
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        let phase = module.phase;
        let changed = match phase {
            MirPhase::Built => lower_abi(module),
            MirPhase::Abi => lower_dispatch(gcx, module),
            MirPhase::Dispatch => {
                let changed = lower_memory_objects(module);
                if changed {
                    analyses.invalidate();
                }
                changed
            }
            MirPhase::IntrinsicsLowered => {
                let mut changed = lower_alloc(module);
                changed |= lower_memory_zero(module);
                if !gcx.sess.opts.evm_version.has_mcopy() {
                    changed |= lower_mcopy_module(module);
                }
                changed
            }
            MirPhase::TargetLowered => lower_evm_shape(module),
            MirPhase::EvmShaped => panic!("cannot lower final MIR phase"),
        };
        module.advance_phase();
        changed
    }
}

fn lower_abi(module: &mut Module) -> bool {
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
        return false;
    }

    // Only functions called internally need a second, parameterized body.
    // When dispatch cannot hoist one callvalue check, each rejecting wrapper
    // carries its own guard.
    let hoist_callvalue = callvalue.hoists();
    let mut changed = false;
    let mut body_of_wrapper = FxHashMap::default();
    for id in targets {
        let (body_id, wrapper_changed) =
            wrap_abi_function(module, id, internally_called.contains(id));
        changed |= wrapper_changed;
        if let Some(body_id) = body_id {
            body_of_wrapper.insert(id, body_id);
        }
        if !hoist_callvalue && super::utils::rejects_callvalue(module.function(id)) {
            inject_callvalue_check(module.function_mut(id));
            changed = true;
        }
    }

    // Internal calls keep the original convention by targeting the body.
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

    changed
}

fn wrap_abi_function(
    module: &mut Module,
    wrapper_id: FunctionId,
    needs_body: bool,
) -> (Option<FunctionId>, bool) {
    let wrapper = module.function(wrapper_id);
    let changed = needs_body
        || !wrapper.params.is_empty()
        || !wrapper.returns.is_empty()
        || wrapper.abi_returns.is_some();

    // Clone before mutating the wrapper so internal callers retain the
    // original function semantics.
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

    // Argument values stay in place and become lazy calldata head reads.
    let wrapper = module.function_mut(wrapper_id);
    wrapper.params.clear();
    wrapper.returns.clear();
    wrapper.abi_returns = None;
    (body_id, changed)
}

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

    // Make the guard the entry without adding a jump to the old body.
    let order = std::iter::once(guard)
        .chain(func.blocks.indices().filter(|&block| block != guard))
        .collect::<Vec<_>>();
    crate::mir::utils::remap_block_order(func, &order);
}

fn is_wrappable_external(func: &Function) -> bool {
    func.selector.is_some() && !func.attributes.is_constructor
}

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

fn lower_dispatch(gcx: Gcx<'_>, module: &mut Module) -> bool {
    LowerDispatchCx { has_bitwise_shifting: gcx.sess.opts.evm_version.has_bitwise_shifting() }
        .run(module)
}

#[derive(Debug)]
struct LowerDispatchCx {
    has_bitwise_shifting: bool,
}

impl LowerDispatchCx {
    fn run(&mut self, module: &mut Module) -> bool {
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

        let hoist_callvalue = callvalue.hoists();
        self.build_entry(module, &routes, receive, fallback, hoist_callvalue);
        true
    }

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
        // CALLDATALOAD right-pads short input, so only selectors ending in zero
        // can match fewer than four calldata bytes.
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

    fn guarded_tail_call(
        &self,
        builder: &mut FunctionBuilder<'_>,
        target: FunctionId,
        check: bool,
        revert_block: BlockId,
    ) {
        if check {
            let go = builder.create_block();
            let value = builder.callvalue();
            builder.branch(value, revert_block, go);
            builder.switch_to_block(go);
        }
        builder.tail_call(target, Vec::new());
    }

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

fn lower_evm_shape(module: &mut Module) -> bool {
    // Most modules have no resultless internal call left to reshape.
    let has_candidate = module.functions.iter().any(|func| {
        func.instructions().any(|inst_id| {
            let inst = func.inst(inst_id);
            inst.result_ty.is_none() && matches!(inst.kind, InstKind::InternalCall { .. })
        })
    });
    if !has_candidate {
        return false;
    }

    let mut eligibility = TailCallEligibility::new(module);
    let mut changed = false;
    loop {
        let mut round_changed = false;
        let mut graph_changed = false;
        for index in 0..eligibility.callee_first().len() {
            let func_id = eligibility.callee_first()[index];
            let (function_changed, function_graph_changed) = {
                let function_count = module.functions.len();
                let func = &mut module.functions[func_id];
                let mut callees_before = None;
                let mut function_changed = false;
                for block_id in (0..func.blocks.len()).map(BlockId::from_usize) {
                    let insts = &func.blocks[block_id].instructions;
                    let Some(position) = insts.iter().position(|&inst_id| {
                        let inst = func.inst(inst_id);
                        inst.result_ty.is_none()
                            && matches!(
                                &inst.kind,
                                InstKind::InternalCall { function, .. }
                                    if eligibility.contains(func_id, *function)
                            )
                    }) else {
                        continue;
                    };
                    callees_before
                        .get_or_insert_with(|| CallGraphInfo::direct_callees(func, function_count));

                    let inst_id = func.blocks[block_id].instructions[position];
                    let InstKind::InternalCall { function, args, .. } = &func.inst(inst_id).kind
                    else {
                        unreachable!("position matched an internal call");
                    };
                    let (function, args) = (*function, args.iter().copied().collect());

                    // Control never returns, so the old continuation is dead.
                    func.blocks[block_id].instructions.truncate(position);
                    func.blocks[block_id].terminator =
                        Some(Terminator::TailCall { function, args });
                    function_changed = true;
                }
                if function_changed {
                    let _ = remove_unreachable_blocks(func);
                    let _ = repair_reachability_phis(func);
                }
                let function_graph_changed = callees_before.is_some_and(|callees_before| {
                    callees_before != CallGraphInfo::direct_callees(func, function_count)
                });
                (function_changed, function_graph_changed)
            };
            if function_changed {
                eligibility.refresh_callee(module, func_id);
                round_changed = true;
                graph_changed |= function_graph_changed;
            }
        }
        if !round_changed || !graph_changed {
            changed |= round_changed;
            break;
        }
        changed = true;

        let next = TailCallEligibility::new(module);
        if eligibility.same_eligible_calls(&next) {
            break;
        }
        eligibility = next;
    }

    changed
}

//! Internal call emission, preserved caller stacks, and result adoption.

use super::{
    BlockId, DenseBitSet, EffectKind, EvmCodegen, EvmMemoryLayout, Function, FunctionId, FxHashMap,
    FxHashSet, ICallStackEdge, InstKind, Label, Liveness, MAX_STACK_ACCESS, ScheduleCost, SmallVec,
    StackModel, StackOp, StackResultProjection, StackReturnPlan, StaticCallStackPlan, TargetSlot,
    U256, Value, ValueId, WORD_BYTES, op,
};

mod abi;
mod arguments;

impl<'gcx> EvmCodegen<'gcx> {
    /// Returns the first internal-call result only when it is consumed. The call itself remains
    /// effectful, and additional returns are staged separately in the multi-return buffer.
    fn live_icall_result(
        result: Option<ValueId>,
        returns: usize,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<ValueId> {
        result.filter(|&result| returns > 0 && !liveness.is_dead_after(result, block, inst_idx))
    }

    #[allow(clippy::too_many_arguments)]
    pub(in crate::backend::evm::codegen) fn emit_icall(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        callee: FunctionId,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let Some(&callee_label) = self.function_labels.get(&callee) else {
            return;
        };
        let return_label = self.asm.new_label();

        // A static-frame callee needs none of the frame-pointer or
        // free-pointer bookkeeping below: its addresses are compile-time
        // constants.
        if self.static_frame_functions.contains(callee) {
            let (preserved_words, argument_words) = self.emit_icall_static(
                func_id,
                func,
                callee,
                callee_label,
                return_label,
                args,
                returns,
                result,
                liveness,
                block,
                inst_idx,
            );
            self.icall_stack_edges.push(ICallStackEdge {
                caller: func_id,
                callee,
                preserved_words,
                argument_words,
            });
            return;
        }

        let resident_call_values: Vec<_> = if self.preserve_caller_stack {
            self.resident_stack_args(func_id)
                .into_iter()
                .flatten()
                .copied()
                .filter(|&value| {
                    self.scheduler.is_stack_only_value(value)
                        && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };

        // Frame layout: [reserved][saved frame ptr][args][returns][locals][spills].
        // The first slot is reserved (the return address used to live there;
        // it now travels on the EVM stack) so downstream offsets stay stable.
        // The spill suffix is only known after the callee body has emitted.
        let frame_size = self.asm.new_deferred_const();
        self.pending_frame_size_consts.push((frame_size, callee));

        // Spill values that are live after this call BEFORE consuming the
        // arguments. An argument that is also used later (e.g. a flag passed to
        // a helper and then stored, as in `tryAdd`) would otherwise be popped by
        // the arg-store loop below and then lost when the stack is cleared for
        // the call, leaving it unavailable at its later use.
        self.spill_live_stack_values(func_id, func, liveness, block, inst_idx);

        // The dynamic-frame base is an anonymous word kept on the physical stack while arguments
        // are stored. Give any argument that this extra word would bury beyond `DUP16` a memory
        // route first. Deep-spill recovery can move named MIR values out of the way, but it cannot
        // save an anonymous frame-base word after that word has already been pushed.
        self.materialize_deep_dynamic_call_args(func, args);

        self.emit_new_internal_frame_base_tracked();

        // The second frame word stores the previous frame pointer.
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MLOAD);
        self.scheduler.stack.push_unknown();
        self.emit_internal_frame_store_from_top_preserving_base(WORD_BYTES as u64);

        for (i, &arg) in args.iter().enumerate() {
            self.emit_operand(func, arg);
            self.emit_internal_frame_store_from_top_preserving_base(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (i as u64) * EvmMemoryLayout::WORD_SIZE,
            );
        }

        // current_frame = frame
        self.emit_store_frame_base_to_current_frame_slot();

        // free_ptr += frame_size
        self.emit_store_new_free_pointer_from_frame_base(frame_size);

        // Resident arguments have no frame home to reload after the nested
        // call. Keep their canonical prefix below the return address just as
        // the static-frame call path does. The callee's scheduler models only
        // words above this hidden prefix, and the whole-program stack-depth
        // validation accounts for the preserved words after emission.
        let caller_stack = if resident_call_values.is_empty() {
            None
        } else {
            self.pop_stack_values_not_needed_by(&resident_call_values);
            let target =
                resident_call_values.iter().copied().map(TargetSlot::Value).collect::<Vec<_>>();
            let shuffle = self.scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!(
                    "could not preserve resident arguments across a dynamic internal call in \
                     `{}`: stack={:?}, target={target:?}",
                    func.name, self.scheduler.stack
                )
            });
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }
            Some(self.scheduler.stack.clone())
        };
        let preserved_words = caller_stack.as_ref().map_or(0, StackModel::depth);
        self.icall_stack_edges.push(ICallStackEdge {
            caller: func_id,
            callee,
            preserved_words,
            argument_words: 0,
        });
        if caller_stack.is_none() {
            self.pop_all_stack_values();
        }
        self.scheduler.clear_stack();

        // The return address travels on the EVM stack, not in the frame: it is
        // pushed after the caller's stack is fully drained, so it is the only
        // physical value below the callee's execution. It is deliberately not
        // tracked by the scheduler — the model only describes the region above
        // it and every emitted DUP/SWAP/POP is model-relative, so nothing in
        // the callee can reach it. The callee's return consumes it with a bare
        // JUMP, and a tail call within the callee forwards it untouched.
        self.emit_push_label(return_label);

        self.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_continuation_label(return_label);
        if let Some(caller_stack) = caller_stack {
            self.scheduler.stack = caller_stack;
        } else {
            self.scheduler.clear_stack();
        }

        let live_result = Self::live_icall_result(result, returns, liveness, block, inst_idx);
        if let Some(result) = live_result {
            self.emit_current_internal_frame_addr(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            if returns <= 1 {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
            }
        }

        // Publish the callee's return area directly as the multi-return buffer.
        // MIR consumes every tail result immediately after the call, while the
        // callee frame is still intact. Copying those words to the unbumped
        // free-memory pointer can overwrite user assembly that is constructing
        // an object there.
        if returns > 1 {
            self.emit_current_internal_frame_addr(
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
        }

        // Deallocate the callee frame in strict LIFO order by restoring the
        // free memory pointer to the callee frame base. This must happen before
        // restoring the caller frame pointer because `emit_current_internal_frame_addr`
        // reads the internal-frame pointer slot. Do this only when the callee's declared
        // params/returns contain no memory pointer: memory pointer returns may
        // reference the callee's frame/heap region, and a memory pointer param lets
        // the callee install a fresh pointer into caller-visible memory. Solidity
        // allocation lowering zero-initializes new arrays/bytes/structs, so reclaimed
        // frame bytes need not be wiped.
        if self.restorable_internal_frames.contains(callee) {
            self.emit_current_internal_frame_addr(0);
            self.asm.emit_push(U256::from(EvmMemoryLayout::FMP_SLOT));
            self.asm.emit_op(op::MSTORE);
        }

        // Restore the caller frame pointer. If a result is on the stack, this leaves it there.
        self.emit_current_internal_frame_addr(WORD_BYTES as u64);
        self.asm.emit_op(op::MLOAD);
        self.asm.emit_push(U256::from(EvmMemoryLayout::INTERNAL_FRAME_PTR_SLOT));
        self.asm.emit_op(op::MSTORE);

        // Store a multi-return call's first result only after restoring the
        // caller frame pointer. The result is a caller value, so spilling it
        // while the callee frame is active can overwrite another return word.
        if returns > 1
            && let Some(result) = live_result
        {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn plan_static_call_stack(
        &self,
        func_id: FunctionId,
        func: &Function,
        callee: FunctionId,
        stack_mask: Option<&DenseBitSet<usize>>,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<StaticCallStackPlan> {
        let depth = self.scheduler.stack.depth();
        if !self.preserve_caller_stack
            || !(1..MAX_STACK_ACCESS).contains(&depth)
            || self.recursive_stack_functions.contains(func_id)
            || self.recursion_reaching_functions.contains(callee)
        {
            return None;
        }

        // Stack arguments are inserted above the hidden return label. A value duplicated from the
        // preserved caller prefix must remain addressable after the label and earlier arguments
        // have been pushed.
        if let Some(mask) = stack_mask {
            let mut words_above = 1;
            for (index, &arg) in args.iter().enumerate() {
                if !mask.contains(index) {
                    continue;
                }
                if self
                    .scheduler
                    .stack
                    .find(arg)
                    .is_some_and(|depth| depth + words_above + 1 > MAX_STACK_ACCESS)
                {
                    return None;
                }
                words_above += 1;
            }
        }

        // A call cannot observe words below its hidden return address. Keep an entirely live,
        // uniquely identified caller stack there without requiring the first post-call
        // instruction to consume the whole layout. The callee scheduler remains relative to its
        // own stack, and whole-program prefix validation retains the conservative spill fallback
        // when nested calls would exceed the physical EVM stack.
        let mut seen = FxHashSet::default();
        let opaque_prefix = self.scheduler.stack.iter().all(|word| {
            word.is_some_and(|value| {
                seen.insert(value) && liveness.is_used_at_or_after(value, block, inst_idx + 1)
            })
        });
        if opaque_prefix {
            return Some(StaticCallStackPlan {
                prepare_ops: Vec::new(),
                caller_stack: self.scheduler.stack.clone(),
            });
        }

        let mut retained = Vec::new();
        for value in self.scheduler.stack.iter().flatten() {
            if !retained.contains(&value)
                && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                && (self.scheduler.is_stack_only_value(value)
                    || (matches!(func.value(value), crate::mir::Value::Inst(_))
                        && Self::can_own_spill_slot(func, value)))
            {
                retained.push(value);
            }
        }
        if !retained.is_empty() {
            let is_stack_arg = |value: ValueId| {
                stack_mask.is_some_and(|mask| {
                    args.iter()
                        .enumerate()
                        .any(|(index, &arg)| mask.contains(index) && arg == value)
                })
            };
            for value in self.scheduler.stack.iter().flatten() {
                if !retained.contains(&value)
                    && is_stack_arg(value)
                    && matches!(func.value(value), Value::Inst(_))
                    && Self::can_own_spill_slot(func, value)
                {
                    retained.push(value);
                }
            }
        }
        if !retained.is_empty() {
            let mut caller_stack = self.scheduler.stack.clone();
            let mut prepare_ops = Vec::new();
            while let Some(depth) = {
                let mut remaining = Self::value_counts(retained.iter().copied());
                caller_stack.iter().enumerate().find_map(|(depth, word)| {
                    if let Some(value) = word
                        && let Some(count) = remaining.get_mut(&value)
                        && *count != 0
                    {
                        *count -= 1;
                        return None;
                    }
                    Some(depth)
                })
            } {
                if depth > MAX_STACK_ACCESS {
                    break;
                }
                if depth != 0 {
                    prepare_ops.push(StackOp::Swap(depth as u8));
                    caller_stack.swap(depth as u8);
                }
                prepare_ops.push(StackOp::Pop);
                caller_stack.pop();
            }
            if caller_stack.depth() == retained.len() {
                let stack_args_are_stable = stack_mask.is_none_or(|mask| {
                    args.iter().enumerate().all(|(index, &arg)| {
                        !mask.contains(index)
                            || !matches!(func.value(arg), crate::mir::Value::Inst(_))
                            || caller_stack.contains(arg)
                            || self.scheduler.reloadable_spill(arg).is_some()
                            || !self.scheduler.stack.contains(arg)
                    })
                });
                let fresh = retained
                    .iter()
                    .filter(|&&value| !self.scheduler.spills.is_stored(value))
                    .count();
                let spill_fallback_cost = depth + fresh * 3 + retained.len() * 2;
                if stack_args_are_stable && prepare_ops.len() < spill_fallback_cost {
                    return Some(StaticCallStackPlan { prepare_ops, caller_stack });
                }
            }
        }

        let &next_inst = func.blocks[block].instructions.get(inst_idx + 1)?;
        let orders = Self::static_call_operand_orders(&func.inst(next_inst).kind);
        let needed = orders.first()?;
        if self.first_stack_value_not_needed_by(needed).is_some() {
            return None;
        }

        let live_result = Self::live_icall_result(result, returns, liveness, block, inst_idx);
        let mut post_call = self.scheduler.clone();
        if let Some(result) = live_result {
            post_call.stack.push(result);
        }

        let cost_model = self.operand_cost_model();
        let mut drained = self.scheduler.clone();
        let mut drain_cost = ScheduleCost::stack_drain_lower_bound(depth);
        let mut stored = FxHashSet::default();
        for value in self.scheduler.stack.iter().flatten() {
            if !liveness.is_dead_after(value, block, inst_idx)
                && Self::can_own_spill_slot(func, value)
                && !drained.spills.is_stored(value)
                && stored.insert(value)
            {
                drained.spills.allocate(value);
                drained.spills.mark_stored(value);
                drain_cost = drain_cost.plus(ScheduleCost::memory_store(cost_model));
            }
        }
        drained.clear_stack();
        if let Some(result) = live_result {
            drained.stack.push(result);
        }

        let next_idx = inst_idx + 1;
        let mut preserve_cost = None;
        let mut drained_next_cost = None;
        for operands in &orders {
            let preserved =
                self.preserved_operands_for(&post_call, func, operands, liveness, block, next_idx);
            let Some(plan) = post_call.plan_operands(
                operands,
                &preserved,
                func,
                self.gcx.sess.opts.optimization,
                cost_model,
            ) else {
                continue;
            };
            let cost = plan.cost();
            if preserve_cost.is_none_or(|best: ScheduleCost| {
                cost.cmp_for(best, self.gcx.sess.opts.optimization).is_lt()
            }) {
                preserve_cost = Some(cost);
            }

            let preserved =
                self.preserved_operands_for(&drained, func, operands, liveness, block, next_idx);
            if let Some(plan) = drained.plan_operands(
                operands,
                &preserved,
                func,
                self.gcx.sess.opts.optimization,
                cost_model,
            ) {
                let cost = plan.cost();
                if drained_next_cost.is_none_or(|best: ScheduleCost| {
                    cost.cmp_for(best, self.gcx.sess.opts.optimization).is_lt()
                }) {
                    drained_next_cost = Some(cost);
                }
            }
        }

        let drain_cost = drain_cost.plus(drained_next_cost?);
        preserve_cost
            .filter(|cost| cost.cmp_for(drain_cost, self.gcx.sess.opts.optimization).is_lt())
            .map(|_| StaticCallStackPlan {
                prepare_ops: Vec::new(),
                caller_stack: self.scheduler.stack.clone(),
            })
    }

    fn static_call_operand_orders(kind: &InstKind) -> SmallVec<[SmallVec<[ValueId; 3]>; 2]> {
        let mut orders = SmallVec::new();
        if let Some(opcode) = kind.evm_opcode() {
            let operands = kind.operands();
            if !operands.is_empty()
                && op::stack_io(opcode).is_some_and(|(inputs, outputs)| {
                    outputs == 1 && usize::from(inputs) == operands.len()
                })
            {
                let mut stack_order = SmallVec::<[ValueId; 3]>::from_iter(operands.iter().copied());
                stack_order.reverse();
                orders.push(stack_order);
                if operands.len() == 2
                    && operands[0] != operands[1]
                    && op::swapped_binary_opcode(opcode).is_some()
                {
                    orders.push(SmallVec::from_iter(operands));
                }
            }
            return orders;
        }

        if let InstKind::Select(condition, if_true, if_false) = kind {
            orders.push(smallvec::smallvec![*if_false, *if_true, *condition]);
        }
        orders
    }

    /// Stores the top stack word into one static-frame argument slot.
    pub(in crate::backend::evm::codegen) fn emit_static_frame_arg_store(
        &mut self,
        callee: FunctionId,
        index: usize,
    ) {
        let addr = self.static_frame_addr(
            callee,
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_push_deferred(addr);
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
    }

    /// Call to a static-frame callee: arguments are stored at absolute
    /// addresses, the return address rides the EVM stack (same invariants as
    /// the dynamic path), and there is no frame-pointer save/update/restore
    /// and no free-pointer traffic — the callee's frame is a fixed region
    /// below the heap. Supported self-recursive Yul callees temporarily carry
    /// the suspended activation's live state on the EVM stack before reusing
    /// that region.
    #[allow(clippy::too_many_arguments)]
    fn emit_icall_static(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        callee: FunctionId,
        callee_label: Label,
        return_label: Label,
        args: &[ValueId],
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> (usize, usize) {
        let stack_mask =
            if self.runtime_stack_args { self.stack_arg_mask(callee).cloned() } else { None };
        let argument_words = stack_mask.as_ref().map_or(0, DenseBitSet::count);
        let recursive_reentry = self.recursive_frame_edges.contains(&(func_id, callee));
        let mut recursive_call_values = Vec::new();
        if recursive_reentry {
            // The callee is about to reuse a scratch frame that may belong to
            // an older activation in the same recursive component. Recover
            // every caller word needed after the call before argument stores
            // overwrite that frame, then keep those words below the hidden
            // return address for the duration of the nested activation.
            let mut seen = FxHashSet::default();
            for value in self
                .scheduler
                .stack
                .iter()
                .flatten()
                .chain(self.scheduler.spills.reloadable_values())
            {
                if Some(value) != result
                    && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                    && seen.insert(value)
                {
                    recursive_call_values.push(value);
                }
            }
            for value in func.live_values() {
                if Some(value) != result
                    && matches!(func.value(value), crate::mir::Value::Arg(_))
                    && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                    && seen.insert(value)
                {
                    recursive_call_values.push(value);
                }
            }
            for &value in &recursive_call_values {
                if !self.scheduler.stack.contains(value) {
                    self.emit_value(func, value);
                }
                self.scheduler.spills.invalidate_stored(value);
                if let Some(available) = &mut self.spill_available {
                    available.remove(&value);
                }
            }
        }
        let mut resident_call_values = recursive_call_values.clone();
        if recursive_reentry && let Some(mask) = &stack_mask {
            // Stack-passed actuals are installed after the memory arguments.
            // Snapshot them before any store can overwrite their source frame.
            for (index, &arg) in args.iter().enumerate() {
                if mask.contains(index) && !resident_call_values.contains(&arg) {
                    if !self.scheduler.stack.contains(arg) {
                        self.emit_value(func, arg);
                    }
                    resident_call_values.push(arg);
                }
            }
        }
        if self.preserve_caller_stack
            && let Some(resident) = self.resident_stack_args(func_id)
        {
            for &value in resident {
                if self.scheduler.is_stack_only_value(value)
                    && (liveness.is_used_at_or_after(value, block, inst_idx + 1)
                        || stack_mask.as_ref().is_some_and(|mask| {
                            args.iter()
                                .enumerate()
                                .any(|(index, &arg)| mask.contains(index) && arg == value)
                        }))
                    && !resident_call_values.contains(&value)
                {
                    resident_call_values.push(value);
                }
            }
        }
        let carries_resident_stack = !resident_call_values.is_empty();
        let caller_stack_plan = (!carries_resident_stack).then(|| {
            self.plan_static_call_stack(
                func_id,
                func,
                callee,
                stack_mask.as_ref(),
                args,
                returns,
                result,
                liveness,
                block,
                inst_idx,
            )
        });
        let caller_stack_plan = caller_stack_plan.flatten();
        if carries_resident_stack {
            for &value in &resident_call_values {
                let consumed = args
                    .iter()
                    .enumerate()
                    .filter(|&(index, &arg)| {
                        arg == value && stack_mask.as_ref().is_none_or(|mask| !mask.contains(index))
                    })
                    .count();
                while self.scheduler.stack.iter().filter(|slot| *slot == Some(value)).count()
                    <= consumed
                {
                    let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
                        if self.recover_lost_internal_stack_value(value) {
                            return 0;
                        }
                        panic!(
                            "resident argument {value:?} was lost before an internal call in `{}` \
                             at {block:?}:{inst_idx}; args={args:?}, mask={stack_mask:?}, \
                             resident={resident_call_values:?}, stack={:?}",
                            func.name, self.scheduler.stack
                        )
                    });
                    assert!(depth < MAX_STACK_ACCESS, "resident argument exceeded DUP16 reach");
                    self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
                }
            }
        }
        if !recursive_reentry && caller_stack_plan.is_none() {
            // The fallback drains the caller stack, so park every value needed after the call
            // before consuming arguments.
            self.spill_live_stack_values(func_id, func, liveness, block, inst_idx);
        }

        let memory_args = args
            .iter()
            .enumerate()
            .filter(|&(index, _)| stack_mask.as_ref().is_none_or(|mask| !mask.contains(index)))
            .map(|(index, &arg)| (index, arg))
            .collect::<Vec<_>>();
        if recursive_reentry {
            // The callee reuses this component's static frame. Materialize the
            // complete memory-passed tuple before the first destination write,
            // then consume the snapshots in reverse order. This is a parallel
            // copy even when arguments permute the caller's frame slots.
            for &(_, arg) in &memory_args {
                self.emit_operand(func, arg);
            }
        }
        if recursive_reentry {
            for (index, _) in memory_args.into_iter().rev() {
                self.emit_static_frame_arg_store(callee, index);
            }
        } else {
            for (index, arg) in memory_args {
                self.emit_operand(func, arg);
                self.emit_static_frame_arg_store(callee, index);
            }
        }

        let mut retention_plan = (!carries_resident_stack && caller_stack_plan.is_none())
            .then(|| {
                stack_mask.as_ref().and_then(|mask| self.plan_retained_stack_args(func, args, mask))
            })
            .flatten();

        // A retention plan is tied to the exact modeled stack it was built from. If any computed,
        // non-retained argument still needs materialization, reject retention before that
        // materialization can mutate the stack and use the conservative spill/drain/reload path.
        // This makes the ordering invariant structural instead of relying on
        // `spill_live_stack_values` having happened to store every such value already.
        if retention_plan.as_ref().is_some_and(|plan| {
            stack_mask.as_ref().is_some_and(|mask| {
                args.iter().enumerate().any(|(i, &arg)| {
                    mask.contains(i)
                        && !plan.retained.contains(i)
                        && matches!(func.value(arg), crate::mir::Value::Inst(_))
                        && self.scheduler.reloadable_spill(arg).is_none()
                })
            })
        }) {
            retention_plan = None;
        }

        // A computed argument not retained physically survives the drain in
        // its spill slot and is reloaded raw after it. Validate and retain the
        // exact slot before clearing the stack; failure is an invariant error
        // in every build instead of an unchecked MLOAD in release builds.
        let mut raw_spill_slots = vec![None; args.len()];
        if let Some(mask) = &stack_mask {
            for (i, &arg) in args.iter().enumerate() {
                if mask.contains(i)
                    && !retention_plan.as_ref().is_some_and(|plan| plan.retained.contains(i))
                    && !caller_stack_plan
                        .as_ref()
                        .is_some_and(|plan| plan.caller_stack.contains(arg))
                    && matches!(func.value(arg), crate::mir::Value::Inst(_))
                    && !Self::is_always_rematerializable_value(func, arg)
                {
                    let slot = if let Some(slot) = self.scheduler.reloadable_spill(arg) {
                        slot
                    } else {
                        self.emit_value(func, arg);
                        self.spill_value_if_needed(func, arg);
                        if self.scheduler.stack.top() == Some(arg) {
                            self.emit_stack_op(StackOp::Pop);
                        }
                        self.scheduler.reloadable_spill(arg).unwrap_or_else(|| {
                            panic!(
                                "computed stack argument {arg:?} is neither resident nor \
                                 runtime-reloadable in `{}`",
                                func.name
                            )
                        })
                    };
                    raw_spill_slots[i] = Some(slot);
                }
            }
        }

        let caller_stack = if carries_resident_stack {
            self.pop_stack_values_not_needed_by(&resident_call_values);
            let target =
                resident_call_values.iter().copied().map(TargetSlot::Value).collect::<Vec<_>>();
            let shuffle = self.scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!(
                    "could not preserve resident arguments across an internal call in `{}`: \
                     stack={:?}, target={target:?}",
                    func.name, self.scheduler.stack
                )
            });
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }
            Some(self.scheduler.stack.clone())
        } else {
            caller_stack_plan.map(|mut plan| {
                for op in plan.prepare_ops {
                    self.emit_stack_op(op);
                }
                debug_assert_eq!(plan.caller_stack.as_slice(), self.scheduler.stack.as_slice());
                plan.caller_stack.inherit_max_depth(self.scheduler.stack.max_depth());
                plan.caller_stack
            })
        };
        let preserved_words = caller_stack.as_ref().map_or(0, StackModel::depth);
        if let Some(plan) = &retention_plan {
            for &op in &plan.drain_ops {
                self.emit_stack_op(op);
            }
            debug_assert_eq!(self.scheduler.stack.depth(), plan.retained.count());
        } else if caller_stack.is_none() {
            self.pop_all_stack_values();
        }
        self.scheduler.clear_stack();

        self.emit_push_label(return_label);
        // Stack-passed arguments ride above the return address, untracked by
        // the model like the return address itself; the callee prologue
        // stores them into its frame before its body runs.
        if let Some(mask) = &stack_mask {
            let mut pushed_args = 0;
            for (i, &arg) in args.iter().enumerate() {
                if mask.contains(i)
                    && !retention_plan.as_ref().is_some_and(|plan| plan.retained.contains(i))
                {
                    self.emit_raw_stack_arg(
                        func,
                        arg,
                        raw_spill_slots[i],
                        caller_stack.as_ref(),
                        1 + pushed_args,
                    );
                    pushed_args += 1;
                }
            }
        }
        if let Some(plan) = &retention_plan {
            for &op in &plan.shuffle_ops {
                debug_assert!(matches!(op, StackOp::Swap(_)));
                self.asm.emit_stack_op(op);
            }
        }
        self.emit_push_label(callee_label);
        self.asm.emit_op(op::JUMP);

        self.asm.define_continuation_label(return_label);
        if let Some(caller_stack) = caller_stack {
            self.scheduler.stack = caller_stack;
        } else {
            self.scheduler.clear_stack();
        }

        // The nested activation is finished, so rebuild the caller's frame
        // homes from the words retained below its return address. This makes
        // later block entries and another recursive call see the caller's
        // state rather than the child activation's last stores.
        for &value in &recursive_call_values {
            if let crate::mir::Value::Arg(index) = func.value(value) {
                let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
                    panic!("recursive caller argument {value:?} was not preserved")
                });
                assert!(depth < MAX_STACK_ACCESS, "recursive caller argument exceeded DUP16 reach");
                self.emit_stack_op(StackOp::Dup((depth + 1) as u8));
                let addr = self.static_frame_addr(
                    func_id,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + index.index() as u64 * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(op::MSTORE);
                self.scheduler.instruction_executed(2, None);
            } else {
                self.spill_value_if_needed(func, value);
            }
        }

        if let Some(plan) = self.stack_return_plan(callee) {
            self.adopt_stack_call_results(
                func, callee, plan, returns, result, liveness, block, inst_idx,
            );
            return (preserved_words, argument_words);
        }

        if let Some(result) = Self::live_icall_result(result, returns, liveness, block, inst_idx) {
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MLOAD);
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        }

        // Publish the static callee's return area directly. Tail projections
        // are consumed before another call can reuse the overlaid frame.
        if returns > 1 {
            let addr = self.static_frame_addr(
                callee,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + (args.len() as u64) * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);
        }
        (preserved_words, argument_words)
    }

    /// Adopts a static callee's stack-native return tuple.
    ///
    /// MIR names only the first result; later results are consumed through the multi-return
    /// buffer. When the caller's protocol reads project cleanly, every returned word binds
    /// directly to its consumer and the buffer never materializes. Otherwise the callee leaves
    /// result `N - 1` on top, so stage those anonymous tail words in reverse order and then
    /// attach result zero to the caller's scheduler model.
    #[allow(clippy::too_many_arguments)]
    fn adopt_stack_call_results(
        &mut self,
        func: &Function,
        callee: FunctionId,
        plan: StackReturnPlan,
        returns: usize,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        assert_eq!(returns, plan.arity, "stack-return call arity changed after ABI planning");

        if plan.arity > 1 {
            if let Some(result) =
                Self::live_icall_result(result, returns, liveness, block, inst_idx)
                && let Some(projection) =
                    Self::plan_stack_result_projection(func, block, inst_idx, plan.arity)
            {
                // The words already sit in tuple order with result `N - 1` on
                // top; bind each to its protocol load and skip the republish
                // and the loads entirely.
                self.scheduler.stack.push(result);
                for &extra in &projection.extras {
                    self.scheduler.stack.push(extra);
                }
                self.elided_insts.extend(projection.elided);
                self.spill_adopted_call_result(func, liveness, block, inst_idx, result);
                for &extra in &projection.extras {
                    self.spill_adopted_call_result(func, liveness, block, inst_idx, extra);
                }
                return;
            }

            // Keep the ordinary return area for multiword stack callees as a compiler-owned
            // fallback buffer. A callee may legally leave slot `0x40` clobbered, so deriving this
            // address from the post-call free-memory pointer would turn a valid return into an
            // arbitrary write or OOG. The common direct-projection path never touches the buffer.
            let return_base = plan.local_base - plan.arity as u64 * EvmMemoryLayout::WORD_SIZE;
            let buffer = self.static_frame_addr(callee, return_base);
            self.asm.emit_push_deferred(buffer);
            self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
            self.asm.emit_op(op::MSTORE);

            for index in (1..plan.arity).rev() {
                self.asm.emit_push(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT));
                self.asm.emit_op(op::MLOAD);
                self.asm.emit_push(U256::from(index as u64 * EvmMemoryLayout::WORD_SIZE));
                self.asm.emit_op(op::ADD);
                // The address is on top of the anonymous return word.
                self.asm.emit_op(op::MSTORE);
            }
        }

        if let Some(result) = Self::live_icall_result(result, returns, liveness, block, inst_idx) {
            self.scheduler.stack.push(result);
            self.spill_top_value_if_live(func, liveness, block, inst_idx, result);
        } else {
            self.asm.emit_stack_op(StackOp::Pop);
        }
    }

    /// Plans direct adoption of a stack-returned tuple's anonymous tail words.
    ///
    /// MIR consumes returns `1..N` through the ephemeral buffer published at
    /// the scratch pointer slot. When the complete protocol — the pointer read
    /// and one offset load per extra return — follows the call with only pure
    /// instructions between, each load observes exactly the word the callee
    /// left on the stack, so the loads' results can adopt those words and the
    /// buffer never needs to exist. Any other consumer of the pointer or its
    /// offset addresses keeps the memory protocol.
    fn plan_stack_result_projection(
        func: &Function,
        block: BlockId,
        call_idx: usize,
        arity: usize,
    ) -> Option<StackResultProjection> {
        let tail = func.blocks[block].instructions.get(call_idx + 1..)?;

        // The first effectful instruction after the call must be the buffer
        // pointer read; nothing may intervene that could publish or clobber.
        let mut base = None;
        for (offset, &inst_id) in tail.iter().enumerate() {
            let inst = func.inst(inst_id);
            if let InstKind::MLoad(addr) = inst.kind
                && func.value_u64(addr) == Some(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT)
            {
                base = Some((offset, inst_id));
                break;
            }
            if inst.kind.effect_kind() != EffectKind::Pure {
                return None;
            }
        }
        let (base_offset, base_inst) = base?;
        let base_value = func.inst_result_value(base_inst)?;

        let mut elided = vec![base_inst];
        let mut addresses = FxHashMap::default();
        let mut extras = vec![None; arity - 1];
        for &inst_id in &tail[base_offset + 1..] {
            let inst = func.inst(inst_id);
            match &inst.kind {
                InstKind::Add(a, b) if *a == base_value || *b == base_value => {
                    let imm = if *a == base_value { *b } else { *a };
                    let index = func
                        .value_u64(imm)
                        .filter(|offset| offset % EvmMemoryLayout::WORD_SIZE == 0)
                        .map(|offset| (offset / EvmMemoryLayout::WORD_SIZE) as usize)?;
                    let address = func.inst_result_value(inst_id)?;
                    if !(1..arity).contains(&index) || addresses.insert(address, index).is_some() {
                        return None;
                    }
                    elided.push(inst_id);
                }
                InstKind::MLoad(addr) if addresses.contains_key(addr) => {
                    let result = func.inst_result_value(inst_id)?;
                    if extras[addresses[addr] - 1].replace(result).is_some() {
                        return None;
                    }
                    elided.push(inst_id);
                    if extras.iter().all(Option::is_some) {
                        break;
                    }
                }
                kind if kind.effect_kind() == EffectKind::Pure => {}
                _ => return None,
            }
        }
        let extras = extras.into_iter().collect::<Option<Vec<_>>>()?;

        // The pointer and its offset addresses must have no consumers beyond
        // the elided protocol; anything else still expects the buffer.
        let tracked = addresses.keys().copied().chain([base_value]).collect::<FxHashSet<_>>();
        let elided_set = elided.iter().copied().collect::<FxHashSet<_>>();
        for check_block in func.blocks.iter() {
            for &inst_id in &check_block.instructions {
                if !elided_set.contains(&inst_id)
                    && func.inst(inst_id).kind.operands().iter().any(|op| tracked.contains(op))
                {
                    return None;
                }
            }
            if let Some(terminator) = &check_block.terminator
                && terminator.operands().iter().any(|op| tracked.contains(op))
            {
                return None;
            }
        }

        Some(StackResultProjection { elided, extras })
    }

    /// Applies the eager-spill contract to a call result adopted mid-stack.
    ///
    /// Mirrors [`Self::spill_top_value_if_live`] without requiring the value
    /// on top: adopted tuple words sit in return order, so earlier results
    /// spill from beneath the later ones.
    fn spill_adopted_call_result(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        value: ValueId,
    ) {
        if self.scheduler.is_stack_only_value(value) || Self::is_rematerializable_value(func, value)
        {
            return;
        }
        let has_reserved_cross_block_slot = self.scheduler.spills.get(value).is_some();
        if liveness.is_dead_after(value, block, inst_idx) && !has_reserved_cross_block_slot {
            return;
        }
        if !self.spill_value_to_reserved_slot(func, value) {
            self.spill_value_if_needed(func, value);
        }
        if has_reserved_cross_block_slot {
            assert!(
                self.scheduler.reloadable_spill(value).is_some(),
                "reserved operand {value:?} was not stored before consumption in `{}`",
                func.name
            );
        }
    }

    fn spill_live_stack_values(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let stack_values: Vec<_> = self.scheduler.stack.iter().flatten().collect();
        for value in stack_values {
            if !liveness.is_dead_after(value, block, inst_idx) {
                self.materialize_stack_only_home(func_id, func, value);
                self.spill_value_if_needed(func, value);
            }
        }
    }
}

//! MIR instruction emission and physical opcode stack effects.

use super::{
    BlockId, EvmCodegen, Function, FunctionId, InstId, InstKind, Liveness, SmallVec, StackEffect,
    StackOp, StackPush, Terminator, ValueId, op,
};

impl<'gcx> EvmCodegen<'gcx> {
    // ==================== Stack-Aware Emitter API ====================
    //
    // These helpers ensure that all EVM stack mutations are tracked by the scheduler.
    // Any opcode that changes the EVM stack must be emitted through these methods
    // to keep the scheduler's StackModel in sync with the actual EVM stack.

    /// Emits a stack manipulation operation (DUP, SWAP, POP) and updates the scheduler.
    pub(super) fn emit_stack_op(&mut self, op: StackOp) {
        self.asm.emit_stack_op(op);
        self.scheduler.stack.apply(op);
    }

    /// Emits an opcode with known stack effects and updates the scheduler.
    ///
    /// This is the core method for stack-aware emission. After emitting the opcode:
    /// - `effect.pops` values are removed from the scheduler's stack model
    /// - Values are pushed according to `push`:
    ///   - `StackPush::None`: no value pushed (effect.pushes must be 0)
    ///   - `StackPush::Tracked(v)`: push a tracked ValueId (effect.pushes must be 1)
    ///   - `StackPush::Unknown`: push an untracked value (effect.pushes must be 1)
    pub(super) fn emit_op_with_effect(&mut self, opcode: u8, effect: StackEffect, push: StackPush) {
        #[cfg(debug_assertions)]
        let before = self.scheduler.depth();

        self.asm.emit_op(opcode);

        // Pop consumed values
        for _ in 0..effect.pops {
            self.scheduler.stack.pop();
        }

        // Push produced values
        match (effect.pushes, push) {
            (0, StackPush::None) => {}
            (1, StackPush::Tracked(v)) => self.scheduler.stack.push(v),
            (1, StackPush::Unknown) => self.scheduler.stack.push_unknown(),
            (n, _) if n > 1 => {
                // Multi-push: push unknown values
                for _ in 0..n {
                    self.scheduler.stack.push_unknown();
                }
            }
            _ => {}
        }

        #[cfg(debug_assertions)]
        {
            let expected = before + effect.pushes - effect.pops;
            debug_assert_eq!(
                self.scheduler.depth(),
                expected,
                "Stack model drift after opcode 0x{:02x}: expected depth {}, got {}",
                opcode,
                expected,
                self.scheduler.depth()
            );
        }
    }

    /// Generates bytecode for an instruction.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn generate_inst(
        &mut self,
        func_id: FunctionId,
        inst_id: InstId,
        func: &Function,
        kind: &InstKind,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
        result_value: Option<ValueId>,
    ) {
        // A resident stack ABI may carry this value by MIR identity, so it needs one tracked
        // physical definition. Without one, retain the cheaper emit-at-use behavior.
        if self.resident_stack_args(func_id).is_none()
            && result_value.is_some_and(|value| Self::is_always_rematerializable_value(func, value))
        {
            return;
        }

        let operands = kind.operands();
        self.materialize_lazy_stack_args(func_id, kind, block, inst_idx);
        let transient_growth = Self::instruction_transient_growth(kind, operands.len());
        self.materialize_deep_stack_args(func_id, func, transient_growth);

        // Calldata-backed global layouts can rematerialize a missing argument;
        // keep the old lazy-copy behavior for those non-stack-only values.
        for &operand in &operands {
            if self.global_stack_active
                && matches!(func.value(operand), crate::mir::Value::Arg(_))
                && !self.scheduler.is_stack_only_value(operand)
                && !self.scheduler.stack.contains(operand)
                && !liveness.is_dead_after(operand, block, inst_idx)
            {
                self.emit_value(func, operand);
            }
        }

        // Spill any operands that are live-out before they get consumed.
        // This ensures cross-block values are preserved in memory.
        self.spill_live_out_operands(func, liveness, block, &operands);

        match kind {
            kind if let Some(opcode) = kind.evm_opcode() => {
                self.emit_evm_opcode(
                    func,
                    &operands,
                    opcode,
                    result_value,
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Alloc { size, .. } => {
                debug_assert!(func.inst(inst_id).metadata.deferred_alloc());
                let size =
                    func.value_u64(*size).expect("deferred allocation must have a constant size");
                let alloc = self.asm.emit_deferred_alloc();
                self.pending_static_allocs.entry(func_id).or_default().push((alloc, size));
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::Fmp | InstKind::SetFmp(_) => {
                unreachable!("abstract allocation instruction reached EVM emission")
            }

            InstKind::StoreImmutable(..) => {
                unreachable!("immutable stores must be lowered before EVM codegen")
            }
            InstKind::LoadImmutable(id) => {
                self.emit_load_immutable(*id);
                self.scheduler.instruction_executed(0, result_value);
            }

            // Select is like a ternary conditional
            InstKind::Select(cond, true_val, false_val) => {
                // select(cond, t, f) = f + cond * (t - f)
                //
                // We emit all three values to the stack, then do inline computation.
                // Stack notation: rightmost = top (depth 0).
                // Stack after emit_value calls: [f, t, cond] with cond on top.

                if let Some(plan) = self.plan_operands(
                    func,
                    &[*false_val, *true_val, *cond],
                    liveness,
                    block,
                    inst_idx,
                ) {
                    self.emit_operand_plan(func, plan);
                } else {
                    self.preserve_stack_only_operands(
                        &[*false_val, *true_val, *cond],
                        liveness,
                        block,
                        inst_idx,
                    );
                    self.emit_value(func, *false_val); // Stack: [f]
                    self.emit_operand(func, *true_val); // Stack: [f, t]
                    self.emit_operand(func, *cond); // Stack: [f, t, cond]
                }

                // Now compute: f + cond * (t - f)
                // Stack is [f, t, cond] with cond on top (depth 0), t at depth 1, f at depth 2
                //
                // Step 1: get f -> [f, t, cond, f]
                self.emit_operand(func, *false_val);
                // Step 2: get t -> [f, t, cond, f, t]
                self.emit_operand(func, *true_val);
                // Step 3: SUB (top - second = t - f) -> [f, t, cond, t-f]
                self.emit_op_with_effect(
                    op::SUB,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 4: MUL (cond * (t-f)) -> [f, t, cond*(t-f)]
                self.emit_op_with_effect(
                    op::MUL,
                    StackEffect { pops: 2, pushes: 1 },
                    StackPush::Unknown,
                );
                // Step 5: SWAP1 -> [f, cond*(t-f), t]
                self.emit_stack_op(StackOp::Swap(1));
                // Step 6: POP (remove t) -> [f, cond*(t-f)]
                self.emit_stack_op(StackOp::Pop);
                // Step 7: ADD (cond*(t-f) + f = f + cond*(t-f)) -> [result]
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::ADD, StackEffect { pops: 2, pushes: 1 }, push);
            }

            // Phi nodes are skipped (handled by copies)
            InstKind::Phi(_) => {}

            // External calls
            //
            // These use emit_value_fresh to guarantee correct values regardless of scheduler
            // state. The stack-aware emit_op_with_effect ensures proper
            // tracking after emission.
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                // CALL(gas, addr, value, argsOffset, argsSize, retOffset, retSize)
                // EVM pops in order: gas (TOS), addr, value, argsOffset, argsSize, retOffset,
                // retSize So we push in reverse order: retSize first (deepest), gas
                // last (TOS)
                let operands =
                    [*gas, *addr, *value, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *value,
                    *addr,
                    *gas,
                ]);
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);

                // CALL consumes 7 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::CALL, StackEffect { pops: 7, pushes: 1 }, push);
            }

            InstKind::CallCode {
                gas,
                addr,
                value,
                args_offset,
                args_size,
                ret_offset,
                ret_size,
            } => {
                let operands =
                    [*gas, *addr, *value, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *value,
                    *addr,
                    *gas,
                ]);
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *value);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);

                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::CALLCODE, StackEffect { pops: 7, pushes: 1 }, push);
            }

            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                // STATICCALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                let operands = [*gas, *addr, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *addr,
                    *gas,
                ]);
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);
                // STATICCALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(op::STATICCALL, StackEffect { pops: 6, pushes: 1 }, push);
            }

            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                let operands = [*gas, *addr, *args_offset, *args_size, *ret_offset, *ret_size];
                self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);
                self.prepare_fresh_operands(func, &operands);
                self.stage_stack_only_fresh_operands(&[
                    *ret_size,
                    *ret_offset,
                    *args_size,
                    *args_offset,
                    *addr,
                    *gas,
                ]);
                // DELEGATECALL(gas, addr, argsOffset, argsSize, retOffset, retSize)
                self.emit_value_fresh(func, *ret_size);
                self.emit_value_fresh(func, *ret_offset);
                self.emit_value_fresh(func, *args_size);
                self.emit_value_fresh(func, *args_offset);
                self.emit_value_fresh(func, *addr);
                self.emit_gas_operand(func, *gas);
                // DELEGATECALL consumes 6 values and produces 1 (success bool)
                let push = result_value.map_or(StackPush::Unknown, StackPush::Tracked);
                self.emit_op_with_effect(
                    op::DELEGATECALL,
                    StackEffect { pops: 6, pushes: 1 },
                    push,
                );
            }

            InstKind::ICall { function, args } => {
                self.preserve_stack_only_operands(args, liveness, block, inst_idx);
                self.emit_icall(
                    func_id,
                    func,
                    *function,
                    args,
                    self.function_return_counts[*function],
                    result_value,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::InternalFrameAddr(offset) => {
                self.emit_own_frame_addr(*offset);
                if let Some(result) = result_value {
                    self.scheduler.stack.push(result);
                }
            }
            InstKind::ConstructorArgsBase => {
                self.emit_constructor_args_base();
                self.scheduler.instruction_executed(0, result_value);
            }
            InstKind::ConstructorArgsEnd => {
                self.emit_constructor_args_end();
                self.scheduler.instruction_executed(0, result_value);
            }

            // Log operations
            InstKind::Log0(offset, size) => {
                // LOG0(offset, size) - stack order: offset on top, then size
                self.emit_log(func, op::LOG0, &[*size, *offset], liveness, block, inst_idx);
            }
            InstKind::Log1(offset, size, topic1) => {
                // LOG1(offset, size, topic1) - stack order: offset, size, topic1
                self.emit_log(
                    func,
                    op::LOG1,
                    &[*topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log2(offset, size, topic1, topic2) => {
                // LOG2(offset, size, topic1, topic2) - stack order: offset, size, topic1,
                // topic2
                self.emit_log(
                    func,
                    op::LOG2,
                    &[*topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log3(offset, size, topic1, topic2, topic3) => {
                // LOG3(offset, size, topic1, topic2, topic3)
                self.emit_log(
                    func,
                    op::LOG3,
                    &[*topic3, *topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }
            InstKind::Log4(offset, size, topic1, topic2, topic3, topic4) => {
                // LOG4(offset, size, topic1, topic2, topic3, topic4)
                self.emit_log(
                    func,
                    op::LOG4,
                    &[*topic4, *topic3, *topic2, *topic1, *size, *offset],
                    liveness,
                    block,
                    inst_idx,
                );
            }

            // Memory copy operations
            InstKind::CalldataCopy(dest, offset, size) => {
                // CALLDATACOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::CALLDATACOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::DataCopy(data, dest, size) => {
                self.emit_data_copy(func, *data, *dest, *size, liveness, block, inst_idx);
            }

            InstKind::CodeCopy(dest, offset, size) => {
                // CODECOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::CODECOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::ReturnDataCopy(dest, offset, size) => {
                // RETURNDATACOPY(destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest],
                    op::RETURNDATACOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::MCopy(dest, src, size) => {
                // MCOPY(destOffset, srcOffset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *src, *dest],
                    op::MCOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::ExtCodeCopy(addr, dest, offset, size) => {
                // EXTCODECOPY(address, destOffset, offset, size)
                self.emit_copy_op_live_aware(
                    func,
                    &[*size, *offset, *dest, *addr],
                    op::EXTCODECOPY,
                    liveness,
                    block,
                    inst_idx,
                );
            }

            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::MappingSlotCalldata(_, _) => {
                unreachable!("mapping-slot builtins must be lowered before EVM codegen")
            }

            InstKind::MakeSlice { .. } | InstKind::SlicePtr(_) | InstKind::SliceLen(_) => {
                unreachable!(
                    "slice instructions must be lowered before EVM codegen: {kind:?} in `{}`",
                    func.name
                )
            }

            InstKind::MemoryObjectLen(_, _)
            | InstKind::SetMemoryObjectLen(_, _, _)
            | InstKind::MemoryObjectData(_, _)
            | InstKind::MemoryObjectFieldAddr { .. }
            | InstKind::MemoryObjectElementAddr { .. }
            | InstKind::Keccak256Bytes(_) => {
                unreachable!("memory-object instructions must be lowered before EVM codegen")
            }

            InstKind::MemoryZero(_, _) => {
                unreachable!("memory-zero instructions must be lowered before EVM codegen")
            }

            InstKind::AbiEncode { .. } => {
                unreachable!("ABI encoding must be lowered before EVM codegen")
            }

            InstKind::StorageToMemory { .. }
            | InstKind::MemoryToStorage { .. }
            | InstKind::ClearStorage { .. } => {
                unreachable!("aggregate operations must be lowered before EVM codegen")
            }
            _ => unreachable!("MIR instruction was not handled: {kind:?}"),
        }

        if let Some(result) = result_value
            && liveness.live_out(block).contains(result)
            && !self.is_stack_phi_source(block, result)
        {
            self.spill_value_if_needed(func, result);
        }

        // A constant-offset calldata load is the same physical word as the
        // corresponding external argument. Once its instruction result dies,
        // adopt a surviving stack copy as the argument instead of loading that
        // word again on the first planned edge.
        for operand in operands {
            if liveness.is_dead_after(operand, block, inst_idx)
                && let Some(&arg) = self.global_stack_aliases.get(&operand)
                && !liveness.is_dead_after(arg, block, inst_idx)
                && !self.scheduler.stack.contains(arg)
            {
                self.scheduler.stack.rename(operand, arg);
            }
        }

        // Drop dead values after the instruction
        let dead_ops = self.scheduler.drop_dead_values(liveness, block, inst_idx);
        for op in dead_ops {
            self.asm.emit_stack_op(op);
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn emit_evm_opcode(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let (inputs, outputs) = op::stack_io(opcode).expect("MIR opcode has no stack effect");
        assert_eq!(usize::from(inputs), operands.len(), "MIR opcode operand count mismatch");

        match (inputs, outputs) {
            (0, 1) => {
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(0, result);
            }
            (1, 1) => self.emit_unary_op_with_result(
                func,
                operands[0],
                opcode,
                result,
                liveness,
                block,
                inst_idx,
            ),
            (2, 1) => self.emit_binary_op_with_result(
                func,
                operands[0],
                operands[1],
                opcode,
                result,
                liveness,
                block,
                inst_idx,
            ),
            (2, 0) => self.emit_store_op_live_aware(
                func,
                operands[0],
                operands[1],
                opcode,
                liveness,
                block,
                inst_idx,
            ),
            (_, 1) => {
                let mut stack_order = SmallVec::<[ValueId; 8]>::from_slice(operands);
                stack_order.reverse();
                self.emit_nary_op(func, &stack_order, opcode, result, liveness, block, inst_idx);
            }
            _ => unreachable!("unsupported MIR opcode stack effect {inputs}->{outputs}"),
        }
    }

    /// Bounds how many words an instruction can place above a stack-only operand before reaching
    /// it. Ordinary operations consume one operand while arranging the rest, but an internal call
    /// also pushes its return label before emitting stack-passed arguments.
    pub(super) fn instruction_transient_growth(kind: &InstKind, operands: usize) -> usize {
        if matches!(kind, InstKind::ICall { .. }) {
            operands.max(1)
        } else {
            operands.saturating_sub(1).max(1)
        }
    }

    /// Bounds how many words a terminator can place above a stack-only operand before reaching it.
    /// Even a one-operand terminator needs the baseline check: an operand already below `DUP16`
    /// cannot be emitted without first materializing its frame fallback.
    pub(super) fn terminator_transient_growth(term: &Terminator) -> usize {
        let operands = term.operands().len();
        if operands == 0 { 0 } else { operands.saturating_sub(1).max(1) }
    }

    pub(super) fn emit_fresh_binary(
        &mut self,
        func: &Function,
        result: ValueId,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        commutative: bool,
    ) {
        if commutative {
            self.emit_value_fresh(func, a);
            self.emit_value_fresh(func, b);
        } else {
            // EVM binary opcodes consume `a` from the top of stack and `b`
            // from the word below, matching the normal binary emitter.
            self.emit_value_fresh(func, b);
            self.emit_value_fresh(func, a);
        }
        self.asm.emit_op(opcode);
        self.scheduler.stack.pop();
        self.scheduler.stack.pop();
        self.scheduler.stack.push(result);
    }

    /// Emits a binary operation with result tracking and liveness awareness.
    /// If an operand is still live after this instruction, we DUP it before it gets consumed.
    #[allow(clippy::too_many_arguments)]
    fn emit_binary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        b: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let mut selected =
            self.plan_operands(func, &[b, a], liveness, block, inst_idx).map(|plan| (opcode, plan));
        if a != b
            && selected.as_ref().is_none_or(|(_, plan)| !plan.is_free())
            && let Some(swapped_opcode) = op::swapped_binary_opcode(opcode)
            && let Some(swapped) = self.plan_operands(func, &[a, b], liveness, block, inst_idx)
            && selected.as_ref().is_none_or(|(_, current)| {
                swapped.cost().cmp_for(current.cost(), self.gcx.sess.opts.optimization).is_lt()
            })
        {
            selected = Some((swapped_opcode, swapped));
        }
        if let Some((opcode, plan)) = selected {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        self.preserve_stack_only_operands(&[a, b], liveness, block, inst_idx);

        // Check if operands are still live after this instruction.
        let a_is_live = !liveness.is_dead_after(a, block, inst_idx);

        // Special case: same operand used twice (e.g., a + a, a - a)
        if a == b {
            self.emit_value(func, a);
            if !self.block_local_copy_survives(liveness, block, a, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, a);
            }
            self.emit_operand(func, a);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Operands that already sit on top of the tracked stack are consumed
        // in place when they are dead afterwards and own no reserved spill
        // slot, instead of being re-emitted and the stale copy nipped later
        // (`DUP2 <op> ... SWAP1 POP` becomes `<op>`).
        let a_dead_free =
            liveness.is_dead_after(a, block, inst_idx) && self.scheduler.spills.get(a).is_none();
        let b_dead_free =
            liveness.is_dead_after(b, block, inst_idx) && self.scheduler.spills.get(b).is_none();
        if self.scheduler.stack.top() == Some(a)
            && self.scheduler.stack.peek(1) == Some(b)
            && a_dead_free
            && b_dead_free
        {
            // The stack is already [b, a].
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }
        if self.scheduler.stack.top() == Some(b)
            && b_dead_free
            && self.scheduler.can_emit_value(a, func)
        {
            // b is in place below; put a above it.
            self.emit_value(func, a);
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }
        if self.scheduler.stack.top() == Some(a)
            && a_dead_free
            && self.scheduler.can_emit_value(b, func)
        {
            // a is in place; emit b above it and swap into [b, a].
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_stack_op(StackOp::Swap(1));
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, result);
            return;
        }

        // Check if either operand is already on stack as an untracked value
        let a_can_emit = self.scheduler.can_emit_value(a, func);
        let b_can_emit = self.scheduler.can_emit_value(b, func);
        let has_untracked = self.scheduler.has_untracked_on_top();
        let has_untracked_at_1 = self.scheduler.has_untracked_at_depth(1);

        if !a_can_emit && b_can_emit && has_untracked {
            // a is an untracked value on top of stack, emit b, then SWAP
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_stack_op(StackOp::Swap(1));
        } else if a_can_emit && !b_can_emit && has_untracked {
            // b is an untracked value on top of stack, emit a on top
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0).
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
        } else if !a_can_emit && b_can_emit && has_untracked_at_1 {
            // a is an untracked value at depth 1, b is tracked on top
            // Stack is [b, a_untracked], need [a, b]
            self.emit_stack_op(StackOp::Swap(1));
        } else {
            // Normal case: emit b first (bottom), then a (top)
            self.emit_value(func, b);
            if !self.block_local_copy_survives(liveness, block, b, 1) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, b);
            }
            self.emit_value(func, a);
            // Spill a if live-after (it's now at depth 0).
            if a_is_live
                && !Self::is_rematerializable_value(func, a)
                && !self.block_local_copy_survives(liveness, block, a, 1)
            {
                self.spill_value_if_needed(func, a);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, result);
    }

    /// Emits a unary operation with result tracking and liveness awareness.
    /// If the operand is still live after this instruction, we spill it after emitting.
    #[allow(clippy::too_many_arguments)]
    fn emit_unary_op_with_result(
        &mut self,
        func: &Function,
        a: ValueId,
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if let Some(plan) = self.plan_operands(func, &[a], liveness, block, inst_idx) {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(1, result);
            return;
        }

        self.preserve_stack_only_operands(&[a], liveness, block, inst_idx);

        self.emit_value(func, a);
        if !self.block_local_copy_survives(liveness, block, a, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, a);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(1, result);
    }

    /// Emits a `LOG0`..=`LOG4` instruction. `operands` are given in stack order
    /// (deepest first, top last) and pushed in that order; the `LOG` then
    /// consumes all of them. Each operand still live after this instruction is
    /// spilled once it reaches the top, so a later use in the same block can
    /// reload it — the same operand-liveness handling as the arithmetic, store
    /// and copy paths. Without it, a topic value consumed by the `LOG` and used
    /// again later (e.g. an event that also stores its data word) would be lost.
    fn emit_log(
        &mut self,
        func: &Function,
        opcode: u8,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if let Some(plan) = self.plan_operands(func, operands, liveness, block, inst_idx) {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(operands.len(), None);
            return;
        }

        self.preserve_stack_only_operands(operands, liveness, block, inst_idx);

        for (i, &operand) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, operand);
            } else {
                // Repeated operands (e.g. duplicate topics) need their own stack item.
                self.emit_operand(func, operand);
            }
            // Occurrences of `operand` emitted so far, this one included: the
            // instruction consumes that many copies net of the occurrences
            // still to be pushed.
            let seen = operands[..=i].iter().filter(|&&op| op == operand).count();
            if !self.block_local_copy_survives(liveness, block, operand, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, operand);
            }
        }
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), None);
    }

    /// Emits a store while preserving operands used later on the stack or in spill slots.
    #[allow(clippy::too_many_arguments)]
    fn emit_store_op_live_aware(
        &mut self,
        func: &Function,
        addr: ValueId,
        val: ValueId,
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        // [..., val, addr] -> [...]
        if (self.is_stack_phi_source(block, val) || self.is_stack_phi_source(block, addr))
            && let Some(plan) = self.plan_operands(func, &[val, addr], liveness, block, inst_idx)
        {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(2, None);
            return;
        }
        self.preserve_stack_only_operands(&[addr, val], liveness, block, inst_idx);

        // Check if addr is still live after this instruction.
        let addr_is_live = !liveness.is_dead_after(addr, block, inst_idx);

        // Operands already sitting on top of the tracked stack are consumed
        // in place when they are dead afterwards and own no reserved spill
        // slot, instead of being re-emitted and the stale copies popped later
        // (`DUP2 DUP2 MSTORE ... POP POP` becomes `MSTORE`). Mirrors the
        // binary-op fast paths.
        let addr_dead_free = !addr_is_live && self.scheduler.spills.get(addr).is_none();
        let val_dead_free = liveness.is_dead_after(val, block, inst_idx)
            && self.scheduler.spills.get(val).is_none();
        if addr_dead_free && val_dead_free && self.scheduler.stack.depth() >= 2 {
            if self.scheduler.stack.top() == Some(addr) && self.scheduler.stack.peek(1) == Some(val)
            {
                // The stack is already [addr, val].
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(2, None);
                return;
            }
            if self.scheduler.stack.top() == Some(val) && self.scheduler.stack.peek(1) == Some(addr)
            {
                self.emit_stack_op(StackOp::Swap(1));
                self.asm.emit_op(opcode);
                self.scheduler.instruction_executed(2, None);
                return;
            }
        }

        // Emit val
        self.emit_value(func, val);
        if !self.block_local_copy_survives(liveness, block, val, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, val);
        }

        // Emit addr
        self.emit_operand(func, addr);
        // Spill addr if live-after (it's now at depth 0).
        let addr_consumed = if addr == val { 2 } else { 1 };
        if addr_is_live
            && !Self::is_rematerializable_value(func, addr)
            && !self.block_local_copy_survives(liveness, block, addr, addr_consumed)
        {
            self.spill_value_if_needed(func, addr);
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(2, None);
    }

    /// Emits a copy-style instruction (no result) with liveness awareness.
    /// `operands` are pushed in order, so the last one ends up on top of the
    /// stack; any operand still live after this instruction is spilled before
    /// the instruction consumes it, preserving it for later uses.
    fn emit_copy_op_live_aware(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        self.preserve_stack_only_operands(operands, liveness, block, inst_idx);

        for (i, &op) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, op);
            } else {
                // Repeated operands need their own stack item each.
                self.emit_operand(func, op);
            }
            // See `emit_log`: copies consumed net of occurrences still to come.
            let seen = operands[..=i].iter().filter(|&&o| o == op).count();
            if !self.block_local_copy_survives(liveness, block, op, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, op);
            }
        }

        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), None);
    }

    /// Emits a copy from relocatable module data to memory.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn emit_data_copy(
        &mut self,
        func: &Function,
        data: crate::mir::DataRef,
        dest: ValueId,
        size: ValueId,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        let operands = [size, dest];
        self.preserve_stack_only_operands(&operands, liveness, block, inst_idx);

        self.emit_value(func, size);
        if !self.block_local_copy_survives(liveness, block, size, 1) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, size);
        }

        // Keep `dest` within DUP16 reach before the anonymous relocation push.
        self.emit_operand(func, dest);
        let dest_consumed = if dest == size { 2 } else { 1 };
        if !self.block_local_copy_survives(liveness, block, dest, dest_consumed) {
            self.spill_top_value_if_live(func, liveness, block, inst_idx, dest);
        }

        self.asm.emit_push_data(data);
        self.scheduler.stack.push_unknown();
        self.emit_stack_op(StackOp::Swap(1));

        self.asm.emit_op(op::CODECOPY);
        self.scheduler.instruction_executed(3, None);
    }

    /// Emits an operation with liveness awareness.
    #[allow(clippy::too_many_arguments)]
    fn emit_nary_op(
        &mut self,
        func: &Function,
        operands: &[ValueId],
        opcode: u8,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) {
        if let Some(plan) = self.plan_operands(func, operands, liveness, block, inst_idx) {
            self.emit_operand_plan(func, plan);
            self.asm.emit_op(opcode);
            self.scheduler.instruction_executed(operands.len(), result);
            return;
        }

        self.preserve_stack_only_operands(operands, liveness, block, inst_idx);

        for (i, &operand) in operands.iter().enumerate() {
            if i == 0 {
                self.emit_value(func, operand);
            } else {
                self.emit_operand(func, operand);
            }
            let seen = operands[..=i].iter().filter(|&&op| op == operand).count();
            if !self.block_local_copy_survives(liveness, block, operand, seen) {
                self.spill_top_value_if_live(func, liveness, block, inst_idx, operand);
            }
        }
        self.asm.emit_op(opcode);
        self.scheduler.instruction_executed(operands.len(), result);
    }
}

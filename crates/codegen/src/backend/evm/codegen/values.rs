//! Value materialization and replay of operand schedules.

use super::{
    BlockId, EvmCodegen, EvmMemoryLayout, Function, InstKind, LateGasOperand, Liveness,
    OperandCostModel, OperandPlan, ScheduledOp, SmallVec, StackOp, StackScheduler, U256, Value,
    ValueId, WORD_BYTES, index_vec, op, rematerializable_nullary_opcode,
};

impl<'gcx> EvmCodegen<'gcx> {
    /// Emits a value to the stack.
    pub(super) fn emit_value(&mut self, func: &Function, val: ValueId) {
        self.emit_value_impl(func, val, true);
    }

    /// Emits a consuming operand occurrence to the stack.
    pub(super) fn emit_operand(&mut self, func: &Function, val: ValueId) {
        self.emit_value_impl(func, val, false);
    }

    /// Returns materialization costs for the active argument and spill addressing convention.
    pub(super) fn operand_cost_model(&self) -> OperandCostModel {
        if self.in_internal_function
            && self
                .current_internal_function
                .is_none_or(|func_id| !self.static_frame_functions.contains(func_id))
        {
            OperandCostModel::DYNAMIC_FRAME
        } else if self.in_constructor {
            OperandCostModel::CONSTRUCTOR
        } else {
            OperandCostModel::DIRECT
        }
    }

    /// Plans operand preparation for operations whose inputs remain valid while
    /// they are rearranged. Memory-mutating stores/copies and calls keep their
    /// freshness-aware emitters until the stack model represents value epochs.
    pub(super) fn plan_operands(
        &self,
        func: &Function,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Option<OperandPlan> {
        let preserved =
            self.preserved_operands_for(&self.scheduler, func, operands, liveness, block, inst_idx);
        self.scheduler.plan_operands(
            operands,
            &preserved,
            func,
            self.gcx.sess.opts.optimization,
            self.operand_cost_model(),
        )
    }

    pub(super) fn preserved_operands_for(
        &self,
        scheduler: &StackScheduler,
        func: &Function,
        operands: &[ValueId],
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> SmallVec<[ValueId; 8]> {
        let mut preserved = SmallVec::<[ValueId; 8]>::new();
        for value in scheduler.stack.iter().flatten() {
            if scheduler.is_stack_only_value(value)
                && liveness.is_used_at_or_after(value, block, inst_idx + 1)
                && !preserved.contains(&value)
            {
                preserved.push(value);
            }
        }
        for &value in operands {
            // A shallow reload can serve the next instruction through one DUP. Keeping a deeper
            // reload can cost more in shuffles than its later memory load.
            let used_by_next_instruction = scheduler.stack.depth() <= 1
                && func.blocks[block].instructions[inst_idx + 1..]
                    .first()
                    .is_some_and(|&inst| func.inst(inst).kind.operands().contains(&value));
            let alias_is_live = self
                .global_stack_aliases
                .get(&value)
                .is_some_and(|&alias| !liveness.is_dead_after(alias, block, inst_idx));
            let carried_arg_is_live = self.global_stack_active
                && matches!(func.value(value), crate::mir::Value::Arg(_))
                && !liveness.is_dead_after(value, block, inst_idx);
            let rematerializable = Self::is_rematerializable_value(func, value)
                || Self::is_always_rematerializable_value(func, value);
            if !preserved.contains(&value)
                && (!liveness.is_dead_after(value, block, inst_idx) || alias_is_live)
                && (!rematerializable || carried_arg_is_live)
                && (scheduler.reloadable_spill(value).is_none()
                    || scheduler.stack.contains(value)
                    || used_by_next_instruction)
            {
                preserved.push(value);
            }
        }
        preserved
    }

    pub(super) fn emit_operand_plan(&mut self, func: &Function, plan: OperandPlan) {
        let stack_depth = self.scheduler.depth();
        let ops = self.scheduler.apply_operand_plan(plan);
        self.record_scheduled_ops_peak(stack_depth, &ops);
        self.emit_scheduled_ops(func, ops);
    }

    pub(super) fn record_scheduled_ops_peak(&mut self, stack_depth: usize, ops: &[ScheduledOp]) {
        self.scheduler.observe_scheduled_ops_peak(stack_depth, ops, self.operand_cost_model());
    }

    pub(super) fn emit_scheduled_ops(
        &mut self,
        func: &Function,
        ops: impl IntoIterator<Item = ScheduledOp>,
    ) {
        for op in ops {
            match op {
                ScheduledOp::Stack(stack_op) => {
                    self.asm.emit_stack_op(stack_op);
                }
                ScheduledOp::PushImmediate(imm) => {
                    self.asm.emit_push(imm);
                }
                ScheduledOp::RematerializeNullary(opcode) => {
                    self.asm.emit_op(opcode);
                }
                ScheduledOp::LoadSpill(slot) => {
                    // PUSH slot_offset, MLOAD
                    self.emit_spill_load(func, slot);
                }
                ScheduledOp::LoadArg(index) => {
                    if self.in_internal_function {
                        self.emit_internal_arg_load(index);
                    } else if self.in_constructor {
                        self.emit_constructor_arg_load(index);
                    } else {
                        // Runtime function: load from calldata
                        // ABI encoding stores the selector in the first four bytes.
                        let offset = 4 + (index.index() as u64) * WORD_BYTES as u64;
                        self.asm.emit_push(U256::from(offset));
                        self.asm.emit_op(op::CALLDATALOAD);
                    }
                }
            }
        }
    }

    fn emit_fresh_scheduled_value(&mut self, func: &Function, value: ValueId, op: ScheduledOp) {
        self.record_scheduled_ops_peak(self.scheduler.depth(), &[op]);
        self.emit_scheduled_ops(func, [op]);
        self.scheduler.stack.push(value);
    }

    pub(super) fn emit_value_impl(&mut self, func: &Function, val: ValueId, claim_top: bool) {
        // Prefer the tracked definition while it is resident. Re-emitting beside that copy gives
        // one MIR identity two physical stack positions and invalidates resident layout plans.
        if Self::is_always_rematerializable_value(func, val)
            && self.scheduler.stack.find(val).is_none()
        {
            self.emit_value_fresh(func, val);
            return;
        }

        if self.scheduler.is_stack_only_value(val)
            && self.scheduler.stack.find(val).is_none()
            && self.scheduler.reloadable_spill(val).is_none()
            && self.recover_lost_internal_stack_value(val)
        {
            return;
        }
        if let Some(depth) = self.scheduler.stack.find(val)
            && depth >= self.stack_access_limit()
            && self.scheduler.reloadable_spill(val).is_none()
            && (self.scheduler.is_stack_only_value(val)
                || !matches!(
                    func.value(val),
                    crate::mir::Value::Immediate(_) | crate::mir::Value::Arg(_)
                ))
        {
            let slot = self.scheduler.spills.allocate(val);
            self.spill_deep_stack_value(func, val, slot, depth);
        }

        if self.scheduler.stack.find(val).is_none()
            && self.scheduler.should_recompute_unstored_spill(val)
        {
            self.emit_value_fresh(func, val);
            return;
        }

        let stack_depth = self.scheduler.depth();
        let ops = if claim_top {
            self.scheduler.ensure_on_top(val, func)
        } else {
            self.scheduler.ensure_operand_on_top(val, func)
        }
        .to_vec();
        self.record_scheduled_ops_peak(stack_depth, &ops);
        self.emit_scheduled_ops(func, ops);
    }

    /// Emits a value fresh, without trying to DUP from the stack.
    /// This is used for CALL operands where we need to guarantee correct values
    /// regardless of scheduler stack tracking state.
    pub(super) fn collect_late_gas_operands(&mut self, func: &Function) {
        self.late_gas_operands.clear();

        let mut use_counts = index_vec![0u32; func.num_values()];
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                for operand in func.inst(inst_id).kind.operands() {
                    use_counts[operand] += 1;
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    use_counts[operand] += 1;
                }
            }
        }

        for block in &func.blocks {
            for &inst_id in &block.instructions {
                let gas = match func.inst(inst_id).kind {
                    InstKind::Call { gas, .. }
                    | InstKind::CallCode { gas, .. }
                    | InstKind::StaticCall { gas, .. }
                    | InstKind::DelegateCall { gas, .. } => gas,
                    _ => continue,
                };
                if use_counts[gas] != 1 {
                    continue;
                }
                let Value::Inst(operand) = func.value(gas) else { continue };
                let (reading, subtracted) = match func.inst(*operand).kind {
                    InstKind::Gas => (*operand, None),
                    InstKind::Sub(lhs, rhs) => {
                        let Value::Inst(reading) = func.value(lhs) else { continue };
                        let Value::Immediate(imm) = func.value(rhs) else { continue };
                        let Some(subtracted) = imm.as_u256() else { continue };
                        if !matches!(func.inst(*reading).kind, InstKind::Gas)
                            || use_counts[lhs] != 1
                        {
                            continue;
                        }
                        (*reading, Some(subtracted))
                    }
                    _ => continue,
                };
                if !block.instructions.contains(&reading) || !block.instructions.contains(operand) {
                    continue;
                }
                self.elided_insts.insert(reading);
                self.elided_insts.insert(*operand);
                self.late_gas_operands.insert(gas, LateGasOperand { subtracted });
            }
        }
    }

    pub(super) fn emit_gas_operand(&mut self, func: &Function, gas: ValueId) {
        let Some(late) = self.late_gas_operands.get(&gas) else {
            self.emit_value_fresh(func, gas);
            return;
        };

        if let Some(subtracted) = late.subtracted {
            // push <reserve>
            // gas !metadata(keep_with_next)
            // sub !metadata(keep_with_next)
            //
            // Before EIP-150 a call asking for more gas than is left throws, so the reserve only
            // keeps solc's 10-gas margin while nothing but the `SUB` runs between the `GAS` and
            // the call. Keeping both with the next instruction stops every backend transform from
            // making that boundary a block boundary, and with it from inserting a jump.
            let keep_with_call = !self.gcx.sess.opts.evm_version.can_overcharge_gas_for_call();
            self.asm.emit_push(subtracted);
            self.asm.emit_op(op::GAS);
            if keep_with_call {
                self.asm.keep_last_with_next();
            }
            self.asm.emit_op(op::SUB);
            if keep_with_call {
                self.asm.keep_last_with_next();
            }
        } else {
            self.asm.emit_op(op::GAS);
        }
        self.scheduler.stack.push(gas);
    }

    pub(super) fn emit_value_fresh(&mut self, func: &Function, val: ValueId) {
        if let Some(op) = Self::always_rematerializable_op(func, val) {
            self.emit_fresh_scheduled_value(func, val, ScheduledOp::RematerializeNullary(op));
            return;
        }

        if self.scheduler.is_stack_only_value(val)
            && self.scheduler.stack.find(val).is_none()
            && self.scheduler.reloadable_spill(val).is_none()
            && self.recover_lost_internal_stack_value(val)
        {
            return;
        }
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                if let Some(u256) = imm.as_u256() {
                    self.emit_fresh_scheduled_value(func, val, ScheduledOp::PushImmediate(u256));
                }
            }
            crate::mir::Value::Arg(index) => {
                if self.scheduler.is_stack_only_value(val) {
                    let depth = self.scheduler.stack.find(val).unwrap_or_else(|| {
                        panic!(
                            "stack-only argument {val:?} was lost before fresh emission in `{}`",
                            func.name
                        )
                    });
                    assert!(
                        depth < self.stack_access_limit(),
                        "stack-only argument exceeded DUP reach"
                    );
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < self.stack_access_limit()
                {
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                self.emit_fresh_scheduled_value(func, val, ScheduledOp::LoadArg(*index));
            }
            crate::mir::Value::Inst(inst_id) => {
                // A value carried on the live stack is the current definition;
                // duplicate it instead of reloading or recomputing. A preserved
                // edge can carry a value that was never spilled, and
                // recomputing a definition such as an FMP load would observe
                // memory that changed since the definition executed.
                if let Some(depth) = self.scheduler.stack.find(val)
                    && depth < self.stack_access_limit()
                {
                    self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                    return;
                }
                // For instruction results, we need to check if they're spilled
                // or if they're instruction results that produce fresh values (like GAS, MLOAD)
                if let Some(slot) = self.scheduler.reloadable_spill(val) {
                    // Load from spill slot. Reloadable covers slots whose
                    // defining block is emitted later: the definition still
                    // executes before any use at runtime.
                    self.emit_fresh_scheduled_value(func, val, ScheduledOp::LoadSpill(slot));
                } else {
                    // Check if the instruction is one that we can "re-execute" to get a fresh value
                    // This handles GAS (which is always fresh) and MLOAD (which re-reads from
                    // memory)
                    let inst_kind = &func.inst(*inst_id).kind;
                    if let Some(opcode) = rematerializable_nullary_opcode(inst_kind).or_else(|| {
                        inst_kind.evm_opcode().filter(|_| matches!(inst_kind, InstKind::Gas))
                    }) {
                        self.emit_fresh_scheduled_value(
                            func,
                            val,
                            ScheduledOp::RematerializeNullary(opcode),
                        );
                    } else {
                        match inst_kind {
                            crate::mir::InstKind::LoadImmutable(id) if !self.in_constructor => {
                                self.emit_load_immutable(*id);
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::InternalFrameAddr(offset) => {
                                self.emit_own_frame_addr(*offset);
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::ConstructorArgsBase => {
                                self.emit_constructor_args_base();
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::ConstructorArgsEnd => {
                                self.emit_constructor_args_end();
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::MLoad(offset) => {
                                // Re-reading a constant scratch location is safe, but the
                                // free-memory-pointer word moves: a pointer defined as
                                // `mload(0x40)` must reach this point through its spill
                                // slot. A slot that is reloadable but not yet stored
                                // belongs to a defining block emitted after this point
                                // that still executes first at runtime.
                                if func.value_u64(*offset) == Some(EvmMemoryLayout::FMP_SLOT) {
                                    if let Some(slot) = self.scheduler.reloadable_spill(val) {
                                        self.emit_fresh_scheduled_value(
                                            func,
                                            val,
                                            ScheduledOp::LoadSpill(slot),
                                        );
                                        return;
                                    }
                                    panic!(
                                        "emit_value_fresh: rematerializing a stale \
                                     free-memory-pointer load: {val:?} in `{}`",
                                        func.name
                                    );
                                }
                                self.emit_value_fresh(func, *offset);
                                self.asm.emit_op(op::MLOAD);
                                // Pop offset, push result
                                self.scheduler.stack.pop();
                                self.scheduler.stack.push(val);
                            }
                            crate::mir::InstKind::CalldataLoad(offset) => {
                                // Calldata is immutable, so re-reading it is
                                // always safe once the address rematerializes.
                                self.emit_value_fresh(func, *offset);
                                self.asm.emit_op(op::CALLDATALOAD);
                                // Pop offset, push result
                                self.scheduler.stack.pop();
                                self.scheduler.stack.push(val);
                            }
                            kind if kind.evm_opcode().is_some_and(|opcode| {
                                matches!(
                                    opcode,
                                    op::KECCAK256
                                        | op::ADD
                                        | op::SUB
                                        | op::MUL
                                        | op::AND
                                        | op::OR
                                        | op::XOR
                                        | op::SHL
                                        | op::SHR
                                        | op::DIV
                                        | op::SDIV
                                        | op::MOD
                                        | op::SMOD
                                        | op::LT
                                        | op::GT
                                        | op::SLT
                                        | op::SGT
                                        | op::EQ
                                        | op::SAR
                                )
                            }) =>
                            {
                                let opcode = kind.evm_opcode().unwrap();
                                let operands = kind.operands();
                                debug_assert_eq!(operands.len(), 2);
                                self.emit_fresh_binary(
                                    func,
                                    val,
                                    operands[0],
                                    operands[1],
                                    opcode,
                                    op::is_commutative(opcode),
                                );
                            }
                            crate::mir::InstKind::SLoad(slot) => {
                                // Re-emit SLOAD. CALL operands are materialized in a
                                // tight sequence with no intervening store, so the
                                // storage slot reads the same value as the original
                                // load (same recompute contract as MLOAD above).
                                self.emit_value_fresh(func, *slot);
                                self.asm.emit_op(op::SLOAD);
                                self.scheduler.stack.pop();
                                self.scheduler.stack.push(val);
                            }
                            _ => {
                                // A value that cannot be re-executed (e.g. an
                                // internal-call result used to compute a CALL
                                // operand) is live on the stack: duplicate it rather
                                // than re-running it. If it is buried too deep to
                                // `DUP`, spill it to a reserved slot and reload.
                                if let Some(depth) = self.scheduler.stack.find(val) {
                                    if depth < self.stack_access_limit() {
                                        self.emit_stack_op(StackOp::Dup(depth as u8 + 1));
                                    } else {
                                        let slot = self.scheduler.spills.allocate(val);
                                        self.spill_deep_stack_value(func, val, slot, depth);
                                        self.emit_fresh_scheduled_value(
                                            func,
                                            val,
                                            ScheduledOp::LoadSpill(slot),
                                        );
                                    }
                                } else if let Some(slot) = self.scheduler.reloadable_spill(val) {
                                    // A defining block emitted later still stores
                                    // this slot before the load executes at runtime.
                                    self.emit_fresh_scheduled_value(
                                        func,
                                        val,
                                        ScheduledOp::LoadSpill(slot),
                                    );
                                } else {
                                    panic!(
                                        "emit_value_fresh: value {val:?} ({:?}) is neither on the \
                                     stack, spilled, nor re-executable",
                                        func.inst(*inst_id).kind
                                    );
                                }
                            }
                        }
                    }
                }
            }
            crate::mir::Value::Undef(_) => {
                // Undef values shouldn't appear in CALL operands
                panic!(
                    "emit_value_fresh: unexpected undef value {val:?}. \
                     CALL operands should be concrete values."
                );
            }
            crate::mir::Value::Error(_) => {
                // A lowering error fails compilation before codegen runs.
                panic!("emit_value_fresh: error sentinel {val:?} reached the backend");
            }
        }
    }
}

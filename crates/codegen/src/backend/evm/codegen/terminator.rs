//! Parallel copies, returns, and MIR terminator emission.

use super::{
    BlockId, CopyDest, CopySource, DebugFunctionExit, EvmCodegen, EvmMemoryLayout, Function,
    FxHashMap, Label, ParallelCopy, StackEffect, StackOp, StackPush, TargetSlot, Terminator, U256,
    ValueId, WORD_BYTES, op,
};

impl<'gcx> EvmCodegen<'gcx> {
    /// Generates a parallel copy.
    ///
    /// Phi copies move values from source to destination. The destination is typically
    /// a phi result that needs to be available in the successor block. We handle this
    /// by spilling the source value to the destination's spill slot.
    pub(super) fn generate_copy(
        &mut self,
        func: &Function,
        copy: &ParallelCopy,
        temps: &mut FxHashMap<u32, ValueId>,
    ) {
        // Handle source: either a MIR value or a temporary
        match &copy.src {
            CopySource::Value(val) => {
                self.emit_operand(func, *val);
            }
            CopySource::Temp(temp_id) => {
                // Temporaries are tracked in our temps map with their ValueId
                if let Some(&temp_val) = temps.get(temp_id) {
                    self.emit_operand(func, temp_val);
                }
            }
        }

        // Handle destination: either a MIR value or a temporary
        match &copy.dst {
            CopyDest::Value(dst_val) => {
                // Spill the value on top of stack to the destination's spill slot
                // This allows the successor block to reload it
                let slot = self.scheduler.spills.reserve(*dst_val);
                self.emit_spill_slot_addr(func, slot);
                self.scheduler.stack.push_unknown();
                self.asm.emit_op(op::MSTORE);
                self.scheduler.stack.pop(); // pop the untracked offset
                self.scheduler.stack.pop(); // pop the value
                self.scheduler.spills.mark_stored(*dst_val);
                if let Some(available) = &mut self.spill_available {
                    available.insert(*dst_val);
                }
            }
            CopyDest::Temp(temp_id) => {
                // Mark this temporary as defined - it's now on the stack
                // Get the ValueId of the value currently on top
                if let Some(val_on_top) = self.scheduler.stack.top() {
                    temps.insert(*temp_id, val_on_top);
                }
            }
        }
    }

    /// Pops all remaining values from the stack.
    /// This ensures the stack is empty before control flow transfer to another block.
    pub(super) fn pop_all_stack_values(&mut self) {
        while self.scheduler.stack_depth() > 0 {
            self.emit_stack_op(StackOp::Pop);
        }
    }

    fn emit_internal_return(&mut self, func: &Function, values: &[ValueId]) {
        if let Some(plan) =
            self.current_internal_function.and_then(|func_id| self.stack_return_plan(func_id))
        {
            assert_eq!(
                values.len(),
                plan.arity,
                "stack-return function `{}` changed return arity after ABI planning",
                func.name
            );
            self.pop_stack_values_not_needed_by(values);
            for value in Self::missing_stack_phi_sources(&self.scheduler.stack, values) {
                self.emit_operand(func, value);
            }
            // StackModel and the shuffler use top-to-bottom order. The physical ABI leaves the
            // last result on top so the caller can stage anonymous results N-1..1 before adopting
            // the MIR-visible first result.
            let target: Vec<_> = values.iter().rev().copied().map(TargetSlot::Value).collect();
            let Some(shuffle) = self.scheduler.shuffle_to_layout(&target) else {
                // A forwarded multi-result call leaves adopted copies of the
                // returned values on the stack; re-emitting them for the
                // return doubles every word and the bounded shuffler cannot
                // always drop the surplus mid-stack. Regenerate the runtime
                // with this function on the frame-backed return convention
                // instead of panicking.
                let func_id = self
                    .current_internal_function
                    .expect("stack-return plans only cover internal functions");
                self.disabled_stack_only_functions.insert(func_id);
                self.scheduler.clear_stack();
                return;
            };
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }

            // Rotate the untracked return address from below the result tuple to the top without
            // disturbing result order: SWAP1, SWAP2, ..., SWAPN maps
            // [return, r0, ..., rN] to [r0, ..., rN, return].
            for depth in 1..=plan.arity {
                self.asm.emit_stack_op(StackOp::Swap(depth as u8));
            }
            self.asm.emit_op(op::JUMP);
            self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            self.scheduler.clear_stack();
            return;
        }

        let return_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
            + (func.params.len() as u64) * EvmMemoryLayout::WORD_SIZE;
        for (i, &value) in values.iter().enumerate() {
            self.emit_operand(func, value);
            self.emit_own_frame_addr(return_base + (i as u64) * WORD_BYTES as u64);
            self.asm.emit_op(op::MSTORE);
            self.scheduler.stack.pop();
        }

        self.pop_all_stack_values();
        // The caller's return address is the untracked value at the bottom of
        // the stack; after popping every tracked value it is on top.
        self.asm.emit_op(op::JUMP);
        self.mark_debug_function_exit(func, DebugFunctionExit::Return);
    }

    fn emit_external_stop(&mut self, func: &Function) {
        if let Some(exit) = self.constructor_exit {
            self.emit_push_label(exit);
            self.asm.emit_op(op::JUMP);
        } else {
            self.asm.emit_op(op::STOP);
        }
        self.mark_debug_function_exit(func, DebugFunctionExit::Return);
    }

    fn emit_revert_returndata(&mut self) {
        if self.gcx.sess.opts.evm_version.supports_returndata() {
            // size = returndatasize
            // returndatacopy 0, 0, size
            // revert 0, returndatasize
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::RETURNDATASIZE,
                StackEffect { pops: 0, pushes: 1 },
                StackPush::Unknown,
            );
            self.emit_op_with_effect(
                op::RETURNDATACOPY,
                StackEffect { pops: 3, pushes: 0 },
                StackPush::None,
            );
            self.emit_op_with_effect(
                op::RETURNDATASIZE,
                StackEffect { pops: 0, pushes: 1 },
                StackPush::Unknown,
            );
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::REVERT,
                StackEffect { pops: 2, pushes: 0 },
                StackPush::None,
            );
        } else {
            // revert 0, 0
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.asm.emit_push(U256::ZERO);
            self.scheduler.stack.push_unknown();
            self.emit_op_with_effect(
                op::REVERT,
                StackEffect { pops: 2, pushes: 0 },
                StackPush::None,
            );
        }
    }

    pub(super) fn emit_push_label(&mut self, label: Label) {
        self.scheduler.stack.observe_peak(self.scheduler.depth().saturating_add(1));
        self.asm.emit_push_label(label);
    }

    pub(super) fn generate_terminator(
        &mut self,
        func: &Function,
        term: &Terminator,
        fallthrough: Option<BlockId>,
        preserve_stack: bool,
    ) {
        match term {
            Terminator::TailCall { function, args } => {
                // Control transfers to the target and never returns. A stack ABI reuses the
                // caller's inherited return label (if any) and places selected arguments above
                // it; otherwise arguments retain their compile-time frame homes. Fused external
                // bodies terminate directly and therefore need no hidden label.
                if !args.is_empty() {
                    // `lower-evm-shaped` only forms argument-carrying tail
                    // calls to callees the backend statically frames.
                    assert!(
                        self.static_frame_functions.contains(*function),
                        "argument-carrying tail call to a non-static-frame callee"
                    );
                    let stack_mask = self
                        .runtime_stack_args
                        .then(|| self.stack_arg_mask(*function).cloned())
                        .flatten();
                    let stack_args: Vec<_> = stack_mask
                        .as_ref()
                        .into_iter()
                        .flat_map(|mask| {
                            (0..args.len())
                                .rev()
                                .filter_map(|index| mask.contains(index).then_some(args[index]))
                        })
                        .collect();
                    let recursive_reentry = self.current_internal_function.is_some_and(|caller| {
                        self.recursive_frame_edges.contains(&(caller, *function))
                    });
                    let memory_args = args
                        .iter()
                        .enumerate()
                        .filter(|&(index, _)| {
                            stack_mask.as_ref().is_none_or(|mask| !mask.contains(index))
                        })
                        .map(|(index, &arg)| (index, arg))
                        .collect::<Vec<_>>();
                    if recursive_reentry {
                        // Keep stack-passed actuals above the caller frame, and
                        // stage all memory-passed actuals before overwriting any
                        // reused destination slot.
                        for &arg in &stack_args {
                            self.emit_operand(func, arg);
                        }
                        for &(_, arg) in &memory_args {
                            self.emit_operand(func, arg);
                        }
                        for (index, _) in memory_args.into_iter().rev() {
                            self.emit_static_frame_arg_store(*function, index);
                        }
                    } else {
                        for (index, arg) in memory_args {
                            self.emit_operand(func, arg);
                            self.emit_static_frame_arg_store(*function, index);
                        }
                    }
                    if !stack_args.is_empty() {
                        self.pop_stack_values_not_needed_by(&stack_args);
                        for value in
                            Self::missing_stack_phi_sources(&self.scheduler.stack, &stack_args)
                        {
                            self.emit_operand(func, value);
                        }
                        let target: Vec<_> =
                            stack_args.iter().copied().map(TargetSlot::Value).collect();
                        let Some(shuffle) = self.scheduler.shuffle_to_layout(&target) else {
                            // An unconstructible entry layout regenerates the
                            // runtime with the callee on the frame convention;
                            // the partially emitted attempt is discarded.
                            self.disabled_stack_only_functions.insert(*function);
                            return;
                        };
                        for op in shuffle.ops {
                            self.asm.emit_stack_op(op);
                        }
                    }
                }
                let label = self.function_labels[function];
                self.emit_push_label(label);
                self.asm.emit_op(op::JUMP);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }
            Terminator::Jump(target) => {
                // Pop any remaining values from the stack before jumping.
                // Each block normally starts with an empty stack, so we must
                // clean the stack before jumping — unless this edge preserves
                // its live stack into a single-predecessor target.
                if Some(*target) == fallthrough {
                    if !preserve_stack {
                        self.pop_all_stack_values();
                    }
                    return;
                }
                if !preserve_stack {
                    self.pop_all_stack_values();
                }
                self.emit_push_label(self.block_labels[target]);
                self.asm.emit_op(op::JUMP);
            }

            Terminator::Branch { condition, then_block, else_block } => {
                if preserve_stack {
                    self.emit_value(func, *condition);
                } else {
                    // Retain a resident condition while draining the rest. Materializing it first
                    // can duplicate an accessible copy only to swap and pop the original.
                    self.pop_stack_values_not_needed_by(&[*condition]);
                    self.emit_value(func, *condition);
                }

                match fallthrough {
                    Some(next) if *else_block == next => {
                        // JUMPI consumes the condition; false falls through to `else_block`.
                        self.emit_push_label(self.block_labels[then_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // condition consumed by JUMPI
                    }
                    Some(next) if *then_block == next => {
                        // Invert the condition so true falls through to `then_block`.
                        self.asm.emit_op(op::ISZERO);
                        self.scheduler.instruction_executed_untracked(1);
                        self.emit_push_label(self.block_labels[else_block]);
                        self.asm.emit_op(op::JUMPI);
                        self.scheduler.stack.pop(); // inverted condition consumed by JUMPI
                    }
                    _ => {
                        // Neither target falls through. Route the likely-hot
                        // edge through JUMPI (16 gas) and leave the cold
                        // revert path on the trailing unconditional jump,
                        // instead of paying JUMPI + JUMP (24 gas) on the hot
                        // path.
                        if self.block_is_cold(*then_block) && !self.block_is_cold(*else_block) {
                            self.asm.emit_op(op::ISZERO);
                            self.scheduler.instruction_executed_untracked(1);
                            self.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // inverted condition consumed by JUMPI

                            self.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMP);
                        } else {
                            // JUMPI consumes the condition
                            self.emit_push_label(self.block_labels[then_block]);
                            self.asm.emit_op(op::JUMPI);
                            self.scheduler.stack.pop(); // condition consumed by JUMPI

                            self.emit_push_label(self.block_labels[else_block]);
                            self.asm.emit_op(op::JUMP);
                        }
                    }
                }
            }

            Terminator::Switch { value, default, cases } => {
                self.emit_switch_terminator(
                    func,
                    *value,
                    *default,
                    cases,
                    fallthrough,
                    preserve_stack,
                );
            }

            Terminator::Return { values } => {
                if self.in_internal_function {
                    self.emit_internal_return(func, values);
                    return;
                }

                assert!(values.is_empty(), "external ABI returns with values must use ReturnData");
                self.emit_external_stop(func);
            }

            Terminator::Revert { offset, size } => {
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::REVERT);
                self.mark_debug_function_exit(func, DebugFunctionExit::Revert);
            }

            Terminator::RevertReturndata => self.emit_revert_returndata(),

            Terminator::ReturnData { offset, size } => {
                // Valid in internal functions too: a fused external body called
                // through an ABI wrapper returns straight to the external
                // caller, abandoning the internal frame.
                self.emit_value(func, *size);
                self.emit_operand(func, *offset);
                self.asm.emit_op(op::RETURN);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }

            Terminator::Stop => {
                // STOP
                self.asm.emit_op(op::STOP);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }

            Terminator::SelfDestruct { recipient } => {
                self.emit_value(func, *recipient);
                self.asm.emit_op(op::SELFDESTRUCT);
                self.mark_debug_function_exit(func, DebugFunctionExit::Return);
            }

            Terminator::Invalid => {
                self.asm.emit_op(op::INVALID);
            }
        }
    }
}

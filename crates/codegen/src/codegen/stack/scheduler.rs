//! Stack scheduler for generating DUP/SWAP sequences.
//!
//! The scheduler takes operands needed for an instruction and generates
//! the sequence of stack operations to arrange them on the stack.

use super::{
    model::{MAX_STACK_ACCESS, StackModel, StackOp},
    spill::{SpillManager, SpillSlot},
};
use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, ValueId},
};

/// Stack scheduler that generates stack manipulation operations.
pub struct StackScheduler {
    /// Current stack state.
    pub stack: StackModel,
    /// Spill manager for values beyond stack depth 16.
    pub spills: SpillManager,
    /// Operations to emit.
    ops: Vec<ScheduledOp>,
}

/// A scheduled operation to emit.
#[derive(Clone, Debug)]
pub enum ScheduledOp {
    /// Stack manipulation (DUP, SWAP, POP).
    Stack(StackOp),
    /// Push an immediate value.
    PushImmediate(alloy_primitives::U256),
    /// Load a spilled value from memory.
    LoadSpill(SpillSlot),
    /// Spill a value to memory.
    SaveSpill(SpillSlot),
    /// Load a function argument from calldata.
    /// Contains the argument index (0-based).
    LoadArg(u32),
}

impl StackScheduler {
    /// Creates a new stack scheduler.
    #[must_use]
    pub fn new() -> Self {
        Self { stack: StackModel::new(), spills: SpillManager::new(), ops: Vec::new() }
    }

    /// Clears the scheduled operations (after emitting them).
    pub fn clear_ops(&mut self) {
        self.ops.clear();
    }

    /// Takes the scheduled operations.
    pub fn take_ops(&mut self) -> Vec<ScheduledOp> {
        std::mem::take(&mut self.ops)
    }

    /// Ensures a value is on top of the stack.
    /// Returns the operations needed to achieve this.
    pub fn ensure_on_top(&mut self, value: ValueId, func: &Function) -> &[ScheduledOp] {
        self.ops.clear();

        if self.stack.is_on_top(value) {
            return &self.ops;
        }

        if let Some(depth) = self.stack.find(value) {
            if depth < MAX_STACK_ACCESS {
                // Value is accessible via DUP
                let dup_n = (depth + 1) as u8;
                self.ops.push(ScheduledOp::Stack(StackOp::Dup(dup_n)));
                self.stack.dup(dup_n);
            } else {
                // Value is too deep - need to spill something or load from spill
                // For now, we should have spilled it earlier
                if let Some(slot) = self.spills.get(value) {
                    self.ops.push(ScheduledOp::LoadSpill(slot));
                    self.stack.push(value);
                }
            }
        } else if let Some(slot) = self.spills.get(value) {
            // Value is spilled, load it
            self.ops.push(ScheduledOp::LoadSpill(slot));
            self.stack.push(value);
        } else {
            match func.value(value) {
                crate::mir::Value::Immediate(imm) => {
                    // It's an immediate, push it directly
                    if let Some(u256) = imm.as_u256() {
                        self.ops.push(ScheduledOp::PushImmediate(u256));
                        self.stack.push(value);
                    }
                }
                crate::mir::Value::Arg { index, .. } => {
                    // It's a function argument, load from calldata
                    self.ops.push(ScheduledOp::LoadArg(*index));
                    self.stack.push(value);
                }
                _ => {}
            }
        }

        &self.ops
    }

    /// Checks if we can emit a value (it's an immediate, arg, on stack, or spilled).
    /// Returns false for instruction results that aren't tracked.
    pub fn can_emit_value(&self, value: ValueId, func: &Function) -> bool {
        // Check if on stack
        if self.stack.find(value).is_some() {
            return true;
        }
        // Check if spilled
        if self.spills.get(value).is_some() {
            return true;
        }
        // Check value type
        matches!(func.value(value), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
    }

    /// Ensures multiple values are on top of the stack in order.
    /// The first value will be at the top, second below it, etc.
    pub fn ensure_on_top_many(&mut self, values: &[ValueId], func: &Function) -> Vec<ScheduledOp> {
        let mut all_ops = Vec::new();

        // Push in reverse order so first value ends up on top
        for &value in values.iter().rev() {
            self.ensure_on_top(value, func);
            all_ops.append(&mut self.ops);
        }

        all_ops
    }

    /// Brings a specific value to the top of the stack using SWAP.
    /// The value must already be on the stack within accessible range.
    pub fn bring_to_top(&mut self, value: ValueId) -> Option<StackOp> {
        if self.stack.is_on_top(value) {
            return None;
        }

        if let Some(depth) = self.stack.find(value)
            && depth < MAX_STACK_ACCESS
            && depth > 0
        {
            let swap_n = depth as u8;
            self.stack.swap(swap_n);
            return Some(StackOp::Swap(swap_n));
        }

        None
    }

    /// Records that an instruction consumed its operands and produced a result.
    /// This updates the stack model accordingly.
    pub fn instruction_executed(&mut self, consumed: usize, produced: Option<ValueId>) {
        // Pop consumed values
        for _ in 0..consumed {
            self.stack.pop();
        }

        // Push produced value
        if let Some(val) = produced {
            self.stack.push(val);
        }
    }

    /// Records that an instruction consumed inputs and produced an untracked output.
    /// The output is on the EVM stack but we don't track which ValueId it corresponds to.
    /// This is used for MLOAD where the value may become stale in loops.
    pub fn instruction_executed_untracked(&mut self, consumed: usize) {
        // Pop consumed values
        for _ in 0..consumed {
            self.stack.pop();
        }
        // Push an unknown value to keep stack depth correct
        self.stack.push_unknown();
    }

    /// Checks if there's an untracked value on top of the stack.
    pub fn has_untracked_on_top(&self) -> bool {
        self.stack.depth() > 0 && self.stack.top().is_none()
    }

    /// Checks if there's an untracked value at a specific depth.
    pub fn has_untracked_at_depth(&self, depth: usize) -> bool {
        self.stack.depth() > depth && self.stack.peek(depth).is_none()
    }

    /// Records that a SWAP1 was executed, updating the stack model.
    pub fn stack_swapped(&mut self) {
        self.stack.swap(1);
    }

    /// Drops dead values from the stack.
    /// Returns operations (SWAPs and POPs) to remove dead values.
    pub fn drop_dead_values(
        &mut self,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Vec<StackOp> {
        let mut ops = Vec::new();

        // First, pop dead values from the top
        while let Some(top_val) = self.stack.top() {
            if liveness.is_dead_after(top_val, block, inst_idx) {
                self.stack.pop();
                ops.push(StackOp::Pop);
            } else {
                break;
            }
        }

        // Then, look for dead values deeper in the stack (up to depth 16)
        // and swap them to the top to pop them
        let mut depth = 1usize;
        while depth < self.stack.depth().min(MAX_STACK_ACCESS) {
            if let Some(val) = self.stack.peek(depth)
                && liveness.is_dead_after(val, block, inst_idx)
            {
                // Swap this dead value to the top and pop it
                let swap_n = depth as u8;
                ops.push(StackOp::Swap(swap_n));
                self.stack.swap(swap_n);
                ops.push(StackOp::Pop);
                self.stack.pop();
                // Don't increment depth since we removed an element
                continue;
            }
            depth += 1;
        }

        ops
    }

    /// Spills values to memory to make room on the stack.
    /// This is needed when stack depth exceeds 16.
    pub fn spill_excess_values(&mut self) -> Vec<ScheduledOp> {
        let mut ops = Vec::new();

        if self.stack.depth() > MAX_STACK_ACCESS {
            // Find a value deep in the stack to spill
            if let Some(value) = self.stack.peek(MAX_STACK_ACCESS - 1) {
                let slot = self.spills.allocate(value);
                ops.push(ScheduledOp::SaveSpill(slot));
            }
        }

        ops
    }

    /// Returns the current stack depth.
    #[must_use]
    pub fn stack_depth(&self) -> usize {
        self.stack.depth()
    }

    /// Clears the stack model (used at block boundaries).
    pub fn clear_stack(&mut self) {
        self.stack.clear();
    }
}

impl Default for StackScheduler {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, Immediate, Value};
    use solar_interface::Ident;

    fn make_test_func() -> Function {
        let name = Ident::DUMMY;
        let mut func = Function::new(name);

        // Add some values
        func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(42))));
        func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(100))));

        func
    }

    #[test]
    fn test_ensure_on_top_already_there() {
        let func = make_test_func();
        let mut scheduler = StackScheduler::new();

        let v0 = ValueId::from_usize(0);
        scheduler.stack.push(v0);

        let ops = scheduler.ensure_on_top(v0, &func);
        assert!(ops.is_empty());
    }

    #[test]
    fn test_ensure_on_top_dup() {
        let func = make_test_func();
        let mut scheduler = StackScheduler::new();

        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        scheduler.stack.push(v0);
        scheduler.stack.push(v1);
        // Stack: [v1, v0]

        let ops = scheduler.ensure_on_top(v0, &func);
        // Should emit DUP2 to get v0 on top

        assert_eq!(ops.len(), 1);
        if let ScheduledOp::Stack(StackOp::Dup(n)) = &ops[0] {
            assert_eq!(*n, 2);
        } else {
            panic!("Expected DUP operation");
        }
    }
}

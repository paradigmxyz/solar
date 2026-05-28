//! Stack scheduler for generating DUP/SWAP sequences.
//!
//! The scheduler takes operands needed for an instruction and generates
//! the sequence of stack operations to arrange them on the stack.
//!
//! ## Backward Layout Optimization
//!
//! This scheduler now supports backward layout analysis:
//! 1. Define the desired exit layout for each instruction
//! 2. Compute the ideal entry layout that minimizes shuffling
//! 3. Use the shuffler to generate optimal DUP/SWAP/POP sequences

use super::{
    model::{MAX_STACK_ACCESS, StackModel, StackOp},
    shuffler::{BlockStackLayout, ShuffleResult, StackShuffler, TargetSlot, combine_stack_layouts},
    spill::{SpillManager, SpillSlot},
};
use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, ValueId},
};
use rustc_hash::FxHashMap;

/// Stack scheduler that generates stack manipulation operations.
pub struct StackScheduler {
    /// Current stack state.
    pub stack: StackModel,
    /// Spill manager for values beyond stack depth 16.
    pub spills: SpillManager,
    /// Operations to emit.
    ops: Vec<ScheduledOp>,
    /// Entry layouts for each block (computed from merge points).
    block_entry_layouts: FxHashMap<BlockId, BlockStackLayout>,
    /// Exit layouts for each block (computed after generating block).
    block_exit_layouts: FxHashMap<BlockId, BlockStackLayout>,
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
        Self {
            stack: StackModel::new(),
            spills: SpillManager::new(),
            ops: Vec::new(),
            block_entry_layouts: FxHashMap::default(),
            block_exit_layouts: FxHashMap::default(),
        }
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
                other => {
                    eprintln!(
                        "ERROR: Value {value:?} is not on stack, not spilled, and not an immediate/arg. \
                         Stack: {:?}, Spills: {:?}, Value kind: {other:?}",
                        self.stack, self.spills
                    );
                    debug_assert!(
                        false,
                        "Value {value:?} is not on stack, not spilled, and not an immediate/arg. \
                         This usually means a cross-block value wasn't spilled before the block exit. \
                         Value kind: {other:?}"
                    );
                }
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

        debug_assert!(self.stack.depth() <= 1024, "Stack overflow: depth {}", self.stack.depth());
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

    /// Returns the current stack depth (alias for `stack_depth`).
    #[must_use]
    pub fn depth(&self) -> usize {
        self.stack.depth()
    }

    /// Clears the stack model (used at block boundaries).
    pub fn clear_stack(&mut self) {
        self.stack.clear();
    }

    /// Shuffles the current stack to match the target layout.
    ///
    /// This uses the backward layout optimization approach:
    /// - Given a target layout (what we want the stack to look like)
    /// - Generate the minimal sequence of DUP/SWAP/POP operations
    ///
    /// Returns the shuffle result containing the operations to emit.
    pub fn shuffle_to_layout(&mut self, target: &[TargetSlot]) -> ShuffleResult {
        let shuffler = StackShuffler::new(&self.stack, target);
        let result = shuffler.shuffle();

        // Apply the operations to our stack model
        for op in &result.ops {
            match op {
                StackOp::Dup(n) => self.stack.dup(*n),
                StackOp::Swap(n) => self.stack.swap(*n),
                StackOp::Pop => {
                    self.stack.pop();
                }
            }
        }

        result
    }

    /// Prepares the stack for a binary operation.
    ///
    /// Given operands (a, b) where a should be on top and b below:
    /// - Computes the target layout [a, b, ...rest]
    /// - Shuffles current stack to match
    /// - Returns operations to emit
    pub fn prepare_binary_op(&mut self, a: ValueId, b: ValueId, _func: &Function) -> ShuffleResult {
        // Build target layout: [a, b]
        let target = [TargetSlot::Value(a), TargetSlot::Value(b)];

        // Check if we need to push values that aren't on stack
        let a_on_stack = self.stack.find(a).is_some();
        let b_on_stack = self.stack.find(b).is_some();

        if !a_on_stack || !b_on_stack {
            // Can't shuffle - values need to be pushed first
            // Fall back to regular ensure_on_top behavior
            return ShuffleResult::new();
        }

        self.shuffle_to_layout(&target)
    }

    /// Prepares the stack for a unary operation.
    ///
    /// Given operand that should be on top:
    /// - Computes the target layout [operand, ...rest]
    /// - Shuffles current stack to match
    /// - Returns operations to emit
    pub fn prepare_unary_op(&mut self, operand: ValueId, _func: &Function) -> ShuffleResult {
        let target = [TargetSlot::Value(operand)];

        if self.stack.find(operand).is_none() {
            // Can't shuffle - value needs to be pushed first
            return ShuffleResult::new();
        }

        self.shuffle_to_layout(&target)
    }

    /// Computes the ideal entry layout for a binary operation given the exit layout.
    ///
    /// For ADD(a, b) -> result, if exit layout is [result, x, y]:
    /// - Entry layout should be [a, b, x, y]
    pub fn compute_binary_entry_layout(
        a: ValueId,
        b: ValueId,
        result: Option<ValueId>,
        exit_layout: &[TargetSlot],
    ) -> Vec<TargetSlot> {
        super::shuffler::ideal_binary_op_entry(a, b, result, exit_layout)
    }

    /// Computes the ideal entry layout for a unary operation given the exit layout.
    pub fn compute_unary_entry_layout(
        operand: ValueId,
        result: Option<ValueId>,
        exit_layout: &[TargetSlot],
    ) -> Vec<TargetSlot> {
        super::shuffler::ideal_unary_op_entry(operand, result, exit_layout)
    }

    // ==================== Block Layout Management ====================
    //
    // These methods support phi node handling via stack layout merging.
    // Instead of always spilling values at block boundaries, we can pass
    // values through the stack by agreeing on a common layout at merge points.

    /// Sets the entry layout for a block.
    ///
    /// This is used to specify what the stack should look like when entering
    /// a block. Predecessors will shuffle their stacks to match this layout.
    pub fn set_block_entry_layout(&mut self, block: BlockId, layout: BlockStackLayout) {
        self.block_entry_layouts.insert(block, layout);
    }

    /// Gets the entry layout for a block, if one has been set.
    #[must_use]
    pub fn get_block_entry_layout(&self, block: BlockId) -> Option<&BlockStackLayout> {
        self.block_entry_layouts.get(&block)
    }

    /// Records the exit layout for a block (current stack state).
    ///
    /// This is called after generating a block's instructions, before the terminator.
    /// The layout is used to determine what values are on the stack when exiting.
    pub fn record_block_exit_layout(&mut self, block: BlockId) {
        let layout = BlockStackLayout::from_stack_model(&self.stack);
        self.block_exit_layouts.insert(block, layout);
    }

    /// Gets the exit layout for a block, if one has been recorded.
    #[must_use]
    pub fn get_block_exit_layout(&self, block: BlockId) -> Option<&BlockStackLayout> {
        self.block_exit_layouts.get(&block)
    }

    /// Computes the entry layout for a merge block from its predecessors' exit layouts.
    ///
    /// This is the key function for phi node handling. It finds a common stack layout
    /// that all predecessors can shuffle to with minimal cost.
    ///
    /// The function also considers the live-in values for the block to ensure
    /// all needed values are on the stack.
    pub fn compute_merge_layout(
        &self,
        predecessors: &[BlockId],
        live_in: impl IntoIterator<Item = ValueId>,
    ) -> BlockStackLayout {
        // Collect exit layouts from predecessors
        let mut pred_layouts: Vec<BlockStackLayout> = Vec::new();
        for &pred in predecessors {
            if let Some(layout) = self.block_exit_layouts.get(&pred) {
                pred_layouts.push(layout.clone());
            }
        }

        // If we have no predecessor layouts, create a layout from live-in values
        if pred_layouts.is_empty() {
            let live_in_values: Vec<_> = live_in.into_iter().collect();
            if live_in_values.is_empty() {
                return BlockStackLayout::new();
            }
            return BlockStackLayout::from_values(live_in_values);
        }

        // Combine the layouts to find a common one
        combine_stack_layouts(&pred_layouts).unwrap_or_default()
    }

    /// Shuffles the current stack to match a block's entry layout.
    ///
    /// Returns the shuffle operations needed. The caller is responsible for
    /// emitting the actual opcodes.
    pub fn shuffle_to_block_entry(&mut self, target_block: BlockId) -> ShuffleResult {
        if let Some(target_layout) = self.block_entry_layouts.get(&target_block) {
            let target_slots = target_layout.to_target_layout();
            self.shuffle_to_layout(&target_slots)
        } else {
            ShuffleResult::new()
        }
    }

    /// Initializes the stack from a block's entry layout.
    ///
    /// This is called when starting to generate a block that has an entry layout.
    /// It sets up the stack model to match the expected entry state.
    pub fn init_from_block_entry_layout(&mut self, block: BlockId) {
        self.stack.clear();
        if let Some(layout) = self.block_entry_layouts.get(&block).cloned() {
            // Push values in reverse order so first slot ends up on top
            for slot in layout.slots.iter().rev() {
                if let Some(val) = slot {
                    self.stack.push(*val);
                } else {
                    self.stack.push_unknown();
                }
            }
        }
    }

    /// Clears all block layout information.
    ///
    /// Called when starting to generate a new function.
    pub fn clear_block_layouts(&mut self) {
        self.block_entry_layouts.clear();
        self.block_exit_layouts.clear();
    }

    /// Returns true if a block has an entry layout set.
    #[must_use]
    pub fn has_block_entry_layout(&self, block: BlockId) -> bool {
        self.block_entry_layouts.contains_key(&block)
    }

    /// Checks if the current stack matches a block's entry layout.
    ///
    /// Returns true if the stack already matches the target layout (no shuffling needed).
    #[must_use]
    pub fn stack_matches_entry_layout(&self, target_block: BlockId) -> bool {
        if let Some(target_layout) = self.block_entry_layouts.get(&target_block) {
            let current = BlockStackLayout::from_stack_model(&self.stack);
            current == *target_layout
        } else {
            true // No layout specified, so any stack is fine
        }
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

    #[test]
    fn test_block_layout_set_and_get() {
        let mut scheduler = StackScheduler::new();
        let block_id = BlockId::from_usize(0);
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        scheduler.set_block_entry_layout(block_id, layout.clone());

        assert!(scheduler.has_block_entry_layout(block_id));
        assert_eq!(scheduler.get_block_entry_layout(block_id), Some(&layout));
    }

    #[test]
    fn test_record_block_exit_layout() {
        let mut scheduler = StackScheduler::new();
        let block_id = BlockId::from_usize(0);
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        scheduler.stack.push(v0);
        scheduler.stack.push(v1);
        // Stack: [v1, v0]

        scheduler.record_block_exit_layout(block_id);

        let exit_layout = scheduler.get_block_exit_layout(block_id);
        assert!(exit_layout.is_some());
        let layout = exit_layout.unwrap();
        assert_eq!(layout.get(0), Some(v1)); // v1 is on top
        assert_eq!(layout.get(1), Some(v0));
    }

    #[test]
    fn test_init_from_block_entry_layout() {
        let mut scheduler = StackScheduler::new();
        let block_id = BlockId::from_usize(0);
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        scheduler.set_block_entry_layout(block_id, layout);

        // Clear and reinitialize
        scheduler.stack.push(ValueId::from_usize(99)); // Put something on stack
        scheduler.init_from_block_entry_layout(block_id);

        // Stack should now match the entry layout
        assert_eq!(scheduler.stack.top(), Some(v0));
        assert_eq!(scheduler.stack.peek(1), Some(v1));
        assert_eq!(scheduler.stack.depth(), 2);
    }

    #[test]
    fn test_stack_matches_entry_layout() {
        let mut scheduler = StackScheduler::new();
        let block_id = BlockId::from_usize(0);
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let layout = BlockStackLayout::from_values([v0, v1]);
        scheduler.set_block_entry_layout(block_id, layout);

        // Stack doesn't match yet
        assert!(!scheduler.stack_matches_entry_layout(block_id));

        // Set up stack to match
        scheduler.stack.push(v1);
        scheduler.stack.push(v0);
        // Stack: [v0, v1]

        assert!(scheduler.stack_matches_entry_layout(block_id));
    }

    #[test]
    fn test_clear_block_layouts() {
        let mut scheduler = StackScheduler::new();
        let block_id = BlockId::from_usize(0);
        let v0 = ValueId::from_usize(0);

        let layout = BlockStackLayout::from_values([v0]);
        scheduler.set_block_entry_layout(block_id, layout);

        assert!(scheduler.has_block_entry_layout(block_id));

        scheduler.clear_block_layouts();

        assert!(!scheduler.has_block_entry_layout(block_id));
    }
}

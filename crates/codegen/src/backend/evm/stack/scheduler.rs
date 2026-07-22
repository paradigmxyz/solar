//! Local operand scheduler for EVM instructions.
//!
//! This module owns two related pieces of state:
//!
//! - [`StackScheduler`] tracks the physical [`StackModel`] and spill manager used by the MIR-to-EVM
//!   emitter.
//! - [`OperandPlan`] is an immutable, replayable proposal for arranging one instruction's operands
//!   without mutating that live state during search.
//!
//! ## Planning model
//!
//! Operands are supplied deepest-first, matching ordinary push order. Internally
//! the goal is reversed because [`StackModel`] stores the top at index zero. A
//! complete state must have that exact goal prefix and retain the requested
//! preservation multiplicities below the prefix.
//!
//! The search is a bounded Dijkstra traversal over modeled stack layouts. It can:
//!
//! - use `SWAP1..16` to consume accessible last uses in place;
//! - use `DUP1..16` when another copy must survive or an operand repeats;
//! - push an immediate with its hardfork-dependent encoded width;
//! - reload a stored spill; and
//! - reload a function argument using the active calling convention.
//!
//! Anonymous stack words are hard barriers for value-moving transitions. They
//! remain in the modeled layout, but the planner cannot claim that one is a MIR
//! operand or silently exchange its identity with a known value.
//!
//! Each transition accumulates [`ScheduleCost`]. Gas optimization orders plans
//! by static gas, spill pressure, encoded bytes, and action count. Size
//! optimization orders them by encoded bytes, spill pressure, static gas, and
//! action count. Queue serials make equal-cost traversal deterministic. Search
//! stops after [`MAX_OPERAND_SEARCH_STATES`]; returning `None` delegates to the
//! existing correctness-oriented emitter.
//!
//! ## Applying a plan
//!
//! [`StackScheduler::apply_operand_plan`] is the only operation that commits a
//! plan. It replays every action into the live model and returns the matching
//! physical operations for emission. Lowering then emits the EVM instruction
//! and calls [`StackScheduler::instruction_executed`] with its stack effect.
//!
//! Complete block-edge layouts use the separate shuffler through
//! [`StackScheduler::shuffle_to_layout`]. Keeping local operand preparation and
//! edge canonicalization separate avoids making the local search responsible
//! for CFG policy or stable cross-block spill placement.

use super::{
    model::{MAX_STACK_ACCESS, StackModel, StackOp},
    shuffler::{ShuffleResult, StackShuffler, TargetSlot},
    spill::{SpillManager, SpillSlot},
};
use crate::{
    analysis::Liveness,
    mir::{BlockId, Function, ValueId},
};
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::FxHashMap;
use std::{cmp::Ordering, collections::BinaryHeap};

const MAX_OPERAND_SEARCH_STATES: usize = 4096;

/// Stack scheduler that generates stack manipulation operations.
pub(crate) struct StackScheduler {
    /// Current stack state.
    pub stack: StackModel,
    /// Spill manager for values beyond stack depth 16.
    pub spills: SpillManager,
    /// Operations to emit.
    ops: Vec<ScheduledOp>,
}

/// A scheduled operation to emit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum ScheduledOp {
    /// Stack manipulation (DUP, SWAP, POP).
    Stack(StackOp),
    /// Push an immediate value.
    PushImmediate(alloy_primitives::U256),
    /// Load a spilled value from memory.
    LoadSpill(SpillSlot),
    /// Load a function argument from calldata.
    /// Contains the argument index (0-based).
    LoadArg(u32),
}

/// Estimated cost of an operand preparation plan.
///
/// Dynamic memory expansion is deliberately represented by `spill_words`
/// instead of folded into `static_gas`: the exact expansion cost depends on
/// the surrounding function's memory high-water mark.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScheduleCost {
    static_gas: u32,
    encoded_bytes: u32,
    spill_words: u32,
    actions: u32,
}

impl ScheduleCost {
    fn key(self, optimization: OptimizationMode) -> [u32; 4] {
        match optimization {
            OptimizationMode::Size => {
                [self.encoded_bytes, self.spill_words, self.static_gas, self.actions]
            }
            _ => [self.static_gas, self.spill_words, self.encoded_bytes, self.actions],
        }
    }

    /// Compares two costs under the selected optimization objective.
    pub(crate) fn cmp_for(self, other: Self, optimization: OptimizationMode) -> Ordering {
        self.key(optimization).cmp(&other.key(optimization))
    }

    fn with_op(mut self, op: &ScheduledOp, evm_version: EvmVersion) -> Self {
        let (static_gas, encoded_bytes) = match op {
            ScheduledOp::Stack(StackOp::Pop) => (2, 1),
            ScheduledOp::Stack(StackOp::Dup(_) | StackOp::Swap(_)) => (3, 1),
            ScheduledOp::PushImmediate(value) => {
                if value.is_zero() && evm_version.has_push0() {
                    (2, 1)
                } else {
                    let bytes = value.to_be_bytes::<32>();
                    let immediate_bytes =
                        bytes.iter().position(|&byte| byte != 0).map_or(1, |i| 32 - i);
                    (3, (immediate_bytes + 1) as u32)
                }
            }
            // A spill or argument load is a PUSH plus MLOAD/CALLDATALOAD. The
            // address width is finalized later, so use the normal PUSH2-sized
            // form here and track spill-area growth separately.
            ScheduledOp::LoadSpill(_) | ScheduledOp::LoadArg(_) => (6, 4),
        };
        self.static_gas += static_gas;
        self.encoded_bytes += encoded_bytes;
        self.actions += 1;
        self
    }
}

#[derive(Clone, Debug)]
struct PlannedAction {
    op: ScheduledOp,
    pushed: Option<ValueId>,
}

/// A complete, replayable operand preparation plan.
#[derive(Clone, Debug)]
pub(crate) struct OperandPlan {
    actions: Vec<PlannedAction>,
    cost: ScheduleCost,
}

impl OperandPlan {
    /// Returns the estimated plan cost.
    pub(crate) fn cost(&self) -> ScheduleCost {
        self.cost
    }
}

#[derive(Clone, Debug)]
struct SearchNode {
    stack: Vec<Option<ValueId>>,
    actions: Vec<PlannedAction>,
    cost: ScheduleCost,
}

#[derive(Clone, Debug)]
struct QueueEntry {
    key: [u32; 4],
    serial: usize,
    node: SearchNode,
}

impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.serial == other.serial
    }
}

impl Eq for QueueEntry {}

impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for QueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        // `BinaryHeap` is a max-heap; reverse the cost and insertion order so
        // the cheapest, earliest deterministic state is visited first.
        other.key.cmp(&self.key).then_with(|| other.serial.cmp(&self.serial))
    }
}

impl StackScheduler {
    /// Creates a new stack scheduler.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self { stack: StackModel::new(), spills: SpillManager::new(), ops: Vec::new() }
    }

    /// Plans an ordered operand head without mutating the current stack.
    ///
    /// `operands` are deepest-first, matching the order in which ordinary EVM
    /// emission pushes them. `preserved` contains values that need at least one
    /// copy below the operand head after the instruction consumes its inputs.
    pub(crate) fn plan_operands(
        &self,
        operands: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        optimization: OptimizationMode,
        evm_version: EvmVersion,
    ) -> Option<OperandPlan> {
        if matches!(optimization, OptimizationMode::None) {
            return None;
        }

        let goal: Vec<_> = operands.iter().rev().copied().collect();
        let mut preserve_counts = FxHashMap::default();
        for &value in preserved {
            preserve_counts.entry(value).or_insert(1usize);
        }

        let start = SearchNode {
            stack: self.stack.as_slice().to_vec(),
            actions: Vec::new(),
            cost: ScheduleCost::default(),
        };
        if Self::operand_goal_reached(&start.stack, &goal, &preserve_counts) {
            return Some(OperandPlan { actions: start.actions, cost: start.cost });
        }

        let mut queue = BinaryHeap::new();
        let mut visited = FxHashMap::default();
        let mut serial = 0usize;
        visited.insert(start.stack.clone(), start.cost.key(optimization));
        queue.push(QueueEntry { key: start.cost.key(optimization), serial, node: start });

        let mut expansions = 0usize;
        while let Some(QueueEntry { key: queued_key, node, .. }) = queue.pop() {
            if visited.get(&node.stack).is_some_and(|&best| best != queued_key) {
                continue;
            }
            if Self::operand_goal_reached(&node.stack, &goal, &preserve_counts) {
                return Some(OperandPlan { actions: node.actions, cost: node.cost });
            }
            expansions += 1;
            if expansions > MAX_OPERAND_SEARCH_STATES {
                break;
            }

            let mut next_actions = Vec::new();
            let max_swap = node.stack.len().saturating_sub(1).min(MAX_STACK_ACCESS);
            for depth in 1..=max_swap {
                if matches!((node.stack[0], node.stack[depth]), (Some(a), Some(b)) if a != b) {
                    next_actions.push(PlannedAction {
                        op: ScheduledOp::Stack(StackOp::Swap(depth as u8)),
                        pushed: None,
                    });
                }
            }

            let mut required_counts = preserve_counts.clone();
            for &value in &goal {
                *required_counts.entry(value).or_default() += 1;
            }
            for (&value, &required) in &required_counts {
                let current = node.stack.iter().filter(|&&slot| slot == Some(value)).count();
                if current < required
                    && let Some(depth) = node
                        .stack
                        .iter()
                        .take(MAX_STACK_ACCESS)
                        .position(|&slot| slot == Some(value))
                {
                    next_actions.push(PlannedAction {
                        op: ScheduledOp::Stack(StackOp::Dup((depth + 1) as u8)),
                        pushed: Some(value),
                    });
                }
            }

            for &value in &goal {
                if next_actions.iter().any(|action| action.pushed == Some(value)) {
                    continue;
                }
                let current = node.stack.iter().filter(|&&slot| slot == Some(value)).count();
                let required = required_counts.get(&value).copied().unwrap_or_default();
                let accessible =
                    node.stack.iter().take(MAX_STACK_ACCESS).any(|&slot| slot == Some(value));
                if (current < required || !accessible)
                    && let Some(op) = self.materialize_operand(value, func)
                {
                    next_actions.push(PlannedAction { op, pushed: Some(value) });
                }
            }

            for action in next_actions {
                let mut next = node.clone();
                match &action.op {
                    ScheduledOp::Stack(StackOp::Swap(depth)) => {
                        next.stack.swap(0, usize::from(*depth));
                    }
                    ScheduledOp::Stack(StackOp::Dup(depth)) => {
                        let value = next.stack[usize::from(*depth - 1)];
                        next.stack.insert(0, value);
                    }
                    ScheduledOp::Stack(StackOp::Pop) => {
                        next.stack.remove(0);
                    }
                    ScheduledOp::PushImmediate(_)
                    | ScheduledOp::LoadSpill(_)
                    | ScheduledOp::LoadArg(_) => {
                        next.stack.insert(0, action.pushed);
                    }
                }
                next.cost = next.cost.with_op(&action.op, evm_version);
                next.actions.push(action);

                let key = next.cost.key(optimization);
                if visited.get(&next.stack).is_some_and(|&old| old <= key) {
                    continue;
                }
                visited.insert(next.stack.clone(), key);
                serial += 1;
                queue.push(QueueEntry { key, serial, node: next });
            }
        }

        None
    }

    /// Applies a previously generated plan to the modeled stack and returns
    /// the physical operations for emission.
    pub(crate) fn apply_operand_plan(&mut self, plan: OperandPlan) -> Vec<ScheduledOp> {
        let mut ops = Vec::with_capacity(plan.actions.len());
        for action in plan.actions {
            match &action.op {
                ScheduledOp::Stack(StackOp::Dup(depth)) => self.stack.dup(*depth),
                ScheduledOp::Stack(StackOp::Swap(depth)) => self.stack.swap(*depth),
                ScheduledOp::Stack(StackOp::Pop) => {
                    self.stack.pop();
                }
                ScheduledOp::PushImmediate(_)
                | ScheduledOp::LoadSpill(_)
                | ScheduledOp::LoadArg(_) => {
                    self.stack.push(action.pushed.expect("materialization pushes a known value"));
                }
            }
            ops.push(action.op);
        }
        ops
    }

    fn operand_goal_reached(
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
    ) -> bool {
        if stack.len() < goal.len()
            || !stack.iter().zip(goal).all(|(&actual, &expected)| actual == Some(expected))
        {
            return false;
        }

        preserve_counts.iter().all(|(&value, &required)| {
            stack[goal.len()..].iter().filter(|&&slot| slot == Some(value)).count() >= required
        })
    }

    fn materialize_operand(&self, value: ValueId, func: &Function) -> Option<ScheduledOp> {
        if self.spills.is_reloadable(value)
            && let Some(slot) = self.spills.get(value)
        {
            return Some(ScheduledOp::LoadSpill(slot));
        }

        match func.value(value) {
            crate::mir::Value::Immediate(imm) => imm.as_u256().map(ScheduledOp::PushImmediate),
            crate::mir::Value::Arg { index, .. } => Some(ScheduledOp::LoadArg(*index)),
            _ => None,
        }
    }

    /// Ensures a value is on top of the stack.
    /// Returns the operations needed to achieve this.
    pub(crate) fn ensure_on_top(&mut self, value: ValueId, func: &Function) -> &[ScheduledOp] {
        self.ensure_on_top_impl(value, func, true)
    }

    /// Emits a fresh operand occurrence for a consuming instruction.
    ///
    /// If `value` is already on top, `ensure_on_top` can claim that existing stack item. That is
    /// correct for a single use, but wrong for instructions that consume the same MIR value more
    /// than once, such as `revert(x, x)` or `log1(x, x, x)`. In those cases every operand
    /// occurrence needs its own stack item, so a top-of-stack value must be duplicated.
    pub(crate) fn ensure_operand_on_top(
        &mut self,
        value: ValueId,
        func: &Function,
    ) -> &[ScheduledOp] {
        self.ensure_on_top_impl(value, func, false)
    }

    fn ensure_on_top_impl(
        &mut self,
        value: ValueId,
        func: &Function,
        claim_top: bool,
    ) -> &[ScheduledOp] {
        self.ops.clear();

        if self.stack.is_on_top(value) {
            if !claim_top {
                self.ops.push(ScheduledOp::Stack(StackOp::Dup(1)));
                self.stack.dup(1);
            }
            return &self.ops;
        }

        if let Some(depth) = self.stack.find(value) {
            if depth < MAX_STACK_ACCESS {
                // Value is accessible via DUP
                let dup_n = (depth + 1) as u8;
                self.ops.push(ScheduledOp::Stack(StackOp::Dup(dup_n)));
                self.stack.dup(dup_n);
                return &self.ops;
            }
            // Value is too deep for DUP. It must either be reloadable from a spill slot or
            // re-emittable below.
            if self.spills.is_reloadable(value)
                && let Some(slot) = self.spills.get(value)
            {
                self.ops.push(ScheduledOp::LoadSpill(slot));
                self.stack.push(value);
                return &self.ops;
            }
        } else if self.spills.is_reloadable(value)
            && let Some(slot) = self.spills.get(value)
        {
            // Value is spilled, load it
            self.ops.push(ScheduledOp::LoadSpill(slot));
            self.stack.push(value);
            return &self.ops;
        }

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
                panic!(
                    "Value {value:?} is not on stack, not spilled, and not an immediate/arg. \
                         This usually means a cross-block value wasn't spilled before the block exit. \
                         Stack: {:?}, Spills: {:?}. \
                         Value kind: {other:?}",
                    self.stack, self.spills
                );
            }
        }

        &self.ops
    }

    /// Checks if we can emit a value (it's an immediate, arg, on stack, or spilled).
    /// Returns false for instruction results that aren't tracked.
    pub(crate) fn can_emit_value(&self, value: ValueId, func: &Function) -> bool {
        // Check if on stack and reachable by DUP.
        if let Some(depth) = self.stack.find(value) {
            return depth < MAX_STACK_ACCESS || self.spills.is_reloadable(value);
        }
        // Check if spilled
        if self.spills.is_reloadable(value) {
            return true;
        }
        // Check value type
        matches!(func.value(value), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg { .. })
    }

    /// Records that an instruction consumed its operands and produced a result.
    /// This updates the stack model accordingly.
    pub(crate) fn instruction_executed(&mut self, consumed: usize, produced: Option<ValueId>) {
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
    pub(crate) fn instruction_executed_untracked(&mut self, consumed: usize) {
        // Pop consumed values
        for _ in 0..consumed {
            self.stack.pop();
        }
        // Push an unknown value to keep stack depth correct
        self.stack.push_unknown();
    }

    /// Checks if there's an untracked value on top of the stack.
    pub(crate) fn has_untracked_on_top(&self) -> bool {
        self.stack.depth() > 0 && self.stack.top().is_none()
    }

    /// Checks if there's an untracked value at a specific depth.
    pub(crate) fn has_untracked_at_depth(&self, depth: usize) -> bool {
        self.stack.depth() > depth && self.stack.peek(depth).is_none()
    }

    /// Records that a SWAP1 was executed, updating the stack model.
    pub(crate) fn stack_swapped(&mut self) {
        self.stack.swap(1);
    }

    /// Drops dead values from the stack.
    /// Returns operations (SWAPs and POPs) to remove dead values.
    pub(crate) fn drop_dead_values(
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

    /// Returns the current stack depth.
    #[must_use]
    pub(crate) fn stack_depth(&self) -> usize {
        self.stack.depth()
    }

    /// Returns the current stack depth (alias for `stack_depth`).
    #[must_use]
    pub(crate) fn depth(&self) -> usize {
        self.stack.depth()
    }

    /// Clears the stack model (used at block boundaries).
    pub(crate) fn clear_stack(&mut self) {
        self.stack.clear();
    }

    /// Shuffles the current stack to match the target layout.
    ///
    /// Returns the shuffle result containing the operations to emit.
    pub(crate) fn shuffle_to_layout(&mut self, target: &[TargetSlot]) -> Option<ShuffleResult> {
        let shuffler = StackShuffler::new(&self.stack, target);
        let result = shuffler.shuffle()?;

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

        Some(result)
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
    use crate::mir::{Function, Immediate, InstKind, Instruction, MirType, Value};
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
    fn test_deep_unspilled_inst_result_is_not_emittable() {
        let mut func = make_test_func();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let inst =
            func.alloc_inst(Instruction::new(InstKind::Add(v0, v1), Some(MirType::uint256())));
        let deep = func.alloc_value(Value::Inst(inst));
        let mut scheduler = StackScheduler::new();

        scheduler.stack.push(deep);
        for i in 0..MAX_STACK_ACCESS {
            scheduler.stack.push(ValueId::from_usize(100 + i));
        }

        assert_eq!(scheduler.stack.find(deep), Some(MAX_STACK_ACCESS));
        assert!(!scheduler.can_emit_value(deep, &func));

        scheduler.spills.allocate(deep);
        assert!(!scheduler.can_emit_value(deep, &func));

        scheduler.spills.mark_reloadable(deep);
        assert!(scheduler.can_emit_value(deep, &func));
    }

    #[test]
    fn operand_plan_consumes_aligned_last_uses() {
        let func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(b);
        scheduler.stack.push(a);

        let plan = scheduler
            .plan_operands(&[b, a], &[], &func, OptimizationMode::Gas, EvmVersion::Shanghai)
            .unwrap();
        assert!(plan.actions.is_empty());

        assert!(scheduler.apply_operand_plan(plan).is_empty());
        scheduler.instruction_executed(2, None);
        assert_eq!(scheduler.depth(), 0);
    }

    #[test]
    fn operand_plan_swaps_last_uses_instead_of_duping() {
        let func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(a);
        scheduler.stack.push(b);

        let plan = scheduler
            .plan_operands(&[b, a], &[], &func, OptimizationMode::Gas, EvmVersion::Shanghai)
            .unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Swap(1)));

        scheduler.apply_operand_plan(plan);
        assert_eq!(scheduler.stack.top(), Some(a));
        assert_eq!(scheduler.stack.peek(1), Some(b));
    }

    #[test]
    fn operand_plan_preserves_live_values() {
        let func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(b);
        scheduler.stack.push(a);

        let plan = scheduler
            .plan_operands(&[b, a], &[a, b], &func, OptimizationMode::Size, EvmVersion::Shanghai)
            .unwrap();
        scheduler.apply_operand_plan(plan);
        scheduler.instruction_executed(2, None);

        assert!(scheduler.stack.contains(a));
        assert!(scheduler.stack.contains(b));
    }

    #[test]
    fn operand_plan_handles_repeated_operands() {
        let func = make_test_func();
        let a = ValueId::from_usize(0);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(a);

        let plan = scheduler
            .plan_operands(&[a, a], &[], &func, OptimizationMode::Gas, EvmVersion::Shanghai)
            .unwrap();
        scheduler.apply_operand_plan(plan);

        assert_eq!(scheduler.stack.top(), Some(a));
        assert_eq!(scheduler.stack.peek(1), Some(a));
    }

    #[test]
    fn operand_plan_can_consume_swap16_value() {
        let mut func = make_test_func();
        let target = ValueId::from_usize(0);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        for i in 0..MAX_STACK_ACCESS {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(100 + i),
            )));
            scheduler.stack.push(filler);
        }
        assert_eq!(scheduler.stack.find(target), Some(MAX_STACK_ACCESS));

        let plan = scheduler
            .plan_operands(&[target], &[], &func, OptimizationMode::Gas, EvmVersion::Shanghai)
            .unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Swap(16)));
    }

    #[test]
    fn operand_plan_materializes_around_anonymous_words() {
        let func = make_test_func();
        let value = ValueId::from_usize(0);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push_unknown();

        let plan = scheduler
            .plan_operands(&[value], &[], &func, OptimizationMode::Gas, EvmVersion::Shanghai)
            .unwrap();
        scheduler.apply_operand_plan(plan);
        scheduler.instruction_executed(1, None);

        assert_eq!(scheduler.depth(), 1);
        assert!(scheduler.stack.top().is_none());
    }

    #[test]
    fn operand_plan_is_disabled_without_optimization() {
        let func = make_test_func();
        let value = ValueId::from_usize(0);
        let scheduler = StackScheduler::new();

        assert!(
            scheduler
                .plan_operands(&[value], &[], &func, OptimizationMode::None, EvmVersion::Shanghai,)
                .is_none()
        );
    }

    #[test]
    fn schedule_cost_honors_gas_and_size_objectives() {
        let gas_plan = ScheduleCost { static_gas: 3, encoded_bytes: 5, spill_words: 0, actions: 1 };
        let size_plan =
            ScheduleCost { static_gas: 6, encoded_bytes: 2, spill_words: 0, actions: 2 };

        assert!(gas_plan.cmp_for(size_plan, OptimizationMode::Gas).is_lt());
        assert!(size_plan.cmp_for(gas_plan, OptimizationMode::Size).is_lt());
    }
}

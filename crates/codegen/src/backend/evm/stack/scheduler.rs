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
//! complete state must have that exact goal prefix and retain a requested copy
//! of each preserved value below the prefix. It must not leave another copy of
//! a dead operand below the prefix: doing so only defers a `POP`, or a `SWAP`
//! plus `POP`, to the post-instruction cleanup.
//!
//! Plans use one cost and goal model at every tier. Exact prefix checks handle
//! the cheapest common case. Gas mode also uses allocation-free verified
//! one-action and unary plans before a lower-bound-certified deterministic walk;
//! bounded A* is reserved for layouts where those proofs do not succeed. Size
//! mode goes from the exact-prefix check to the deterministic/A* path so an
//! equal-cost local fast-path tie cannot leave a residual stack that costs more
//! to clean up after the instruction. The available actions are:
//!
//! - use `SWAP1..16` to consume accessible last uses in place;
//! - use `DUP1..16` when another copy must survive or an operand repeats;
//! - in gas mode, pop a redundant top copy when an accessible copy remains;
//! - push an immediate with its hardfork-dependent encoded width when the displaced copy remains
//!   live or is also consumed;
//! - reload a stored spill; and
//! - reload a function argument using the active calling convention.
//!
//! Anonymous stack words remain opaque in the modeled layout. The planner never
//! claims one as a MIR operand. It may move one by physical position while
//! arranging a known value, and known values below one can still be duplicated
//! when they are in reach.
//!
//! Each transition accumulates [`ScheduleCost`]. Direct and dynamic-frame loads
//! have separate costs because the latter also loads the frame pointer and adds
//! an offset. An admissible lower bound for missing copies and unavoidable
//! rearrangement proves the deterministic plan when its final cost matches the
//! bound. Otherwise the A* queue handles the ambiguous layout. Gas optimization
//! orders plans by static gas, encoded bytes, and action count. Size optimization
//! orders them by encoded bytes, static gas, and action count. Equal estimates
//! prefer deeper states, then queue serials make traversal deterministic. Search
//! stops after [`MAX_OPERAND_SEARCH_STATES`]; returning `None` delegates to the
//! existing correctness-oriented emitter. “Optimal” here means the least
//! estimated local preparation cost within this action model, not a
//! whole-function stack-allocation optimum.
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
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::FxHashMap;
use std::{cmp::Ordering, collections::BinaryHeap};

const MAX_OPERAND_SEARCH_STATES: usize = 4096;

type PlannedActions = SmallVec<[PlannedAction; 8]>;
type SearchStack = SmallVec<[Option<ValueId>; 24]>;

/// Tracks physical stack state and plans operand preparation.
pub(crate) struct StackScheduler {
    /// Current stack state.
    pub stack: StackModel,
    /// Spill slots and their current reloadability.
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
    /// Load a function argument through the active calling convention.
    ///
    /// Contains the argument index (0-based).
    LoadArg(u32),
}

/// Cost of materializing a spill or argument under the active frame convention.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OperandCostModel {
    load_static_gas: u32,
    load_encoded_bytes: u32,
}

impl OperandCostModel {
    /// A direct address push followed by `MLOAD` or `CALLDATALOAD`.
    pub(crate) const DIRECT: Self = Self { load_static_gas: 6, load_encoded_bytes: 4 };

    /// A frame-pointer load, offset addition, and final value load.
    pub(crate) const DYNAMIC_FRAME: Self = Self { load_static_gas: 15, load_encoded_bytes: 7 };
}

#[derive(Clone, Copy)]
struct OperandPlanningContext<'a> {
    func: &'a Function,
    optimization: OptimizationMode,
    evm_version: EvmVersion,
    cost_model: OperandCostModel,
}

/// Estimated cost of an operand preparation plan.
///
/// Plans may reload existing spill slots but never allocate new ones, so their
/// dynamic memory expansion is identical and does not participate in ordering.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct ScheduleCost {
    static_gas: u32,
    encoded_bytes: u32,
    actions: u32,
}

impl ScheduleCost {
    fn key(self, optimization: OptimizationMode) -> [u32; 3] {
        match optimization {
            OptimizationMode::Size => [self.encoded_bytes, self.static_gas, self.actions],
            _ => [self.static_gas, self.encoded_bytes, self.actions],
        }
    }

    /// Compares two costs under the selected optimization objective.
    pub(crate) fn cmp_for(self, other: Self, optimization: OptimizationMode) -> Ordering {
        self.key(optimization).cmp(&other.key(optimization))
    }

    fn with_op(
        mut self,
        op: &ScheduledOp,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Self {
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
            ScheduledOp::LoadSpill(_) | ScheduledOp::LoadArg(_) => {
                (cost_model.load_static_gas, cost_model.load_encoded_bytes)
            }
        };
        self.static_gas += static_gas;
        self.encoded_bytes += encoded_bytes;
        self.actions += 1;
        self
    }

    fn plus(self, other: Self) -> Self {
        Self {
            static_gas: self.static_gas.saturating_add(other.static_gas),
            encoded_bytes: self.encoded_bytes.saturating_add(other.encoded_bytes),
            actions: self.actions.saturating_add(other.actions),
        }
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
    actions: PlannedActions,
    cost: ScheduleCost,
}

impl OperandPlan {
    /// Returns the estimated plan cost.
    pub(crate) fn cost(&self) -> ScheduleCost {
        self.cost
    }

    /// Returns whether applying this plan emits no preparation operations.
    pub(crate) fn is_free(&self) -> bool {
        self.actions.is_empty()
    }
}

#[derive(Clone, Debug)]
struct SearchNode {
    stack: SearchStack,
    actions: PlannedActions,
    cost: ScheduleCost,
}

#[derive(Clone, Debug)]
struct QueueEntry {
    priority: [u32; 3],
    key: [u32; 3],
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
        // `BinaryHeap` is a max-heap; reverse the estimated cost so the most promising state is
        // visited first. Prefer deeper states when estimates tie, then preserve deterministic
        // insertion order.
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| self.node.cost.actions.cmp(&other.node.cost.actions))
            .then_with(|| other.serial.cmp(&self.serial))
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
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        if matches!(optimization, OptimizationMode::None) {
            return None;
        }

        let goal = operands.iter().rev().copied().collect::<SmallVec<[_; 8]>>();
        if Self::operand_goal_reached_direct(self.stack.as_slice(), &goal, preserved) {
            let plan =
                OperandPlan { actions: PlannedActions::new(), cost: ScheduleCost::default() };
            return Some(self.verify_operand_plan(plan, &goal, preserved));
        }
        // Size mode keeps the established search tie-breaking because equal local costs can leave
        // residual stacks with different cleanup costs after the instruction.
        if matches!(optimization, OptimizationMode::Gas) {
            if let Some(plan) = self.try_single_action_operand_plan(
                &goal,
                preserved,
                func,
                optimization,
                evm_version,
                cost_model,
            ) {
                return Some(self.verify_operand_plan(plan, &goal, preserved));
            }
            if let [value] = operands
                && let Some(plan) = self.try_unary_operand_plan(
                    *value,
                    preserved.contains(value),
                    func,
                    optimization,
                    evm_version,
                    cost_model,
                )
            {
                return Some(self.verify_operand_plan(plan, &goal, preserved));
            }
        }

        let mut preserve_counts = FxHashMap::default();
        for &value in preserved {
            preserve_counts.entry(value).or_insert(1usize);
        }

        let start = SearchNode {
            stack: self.stack.as_slice().iter().copied().collect(),
            actions: PlannedActions::new(),
            cost: ScheduleCost::default(),
        };
        let context = OperandPlanningContext { func, optimization, evm_version, cost_model };
        if let Some(plan) =
            self.try_goal_directed_operand_plan(start.clone(), &goal, &preserve_counts, context)
        {
            return Some(self.verify_operand_plan(plan, &goal, preserved));
        }

        let mut queue = BinaryHeap::new();
        let mut visited = FxHashMap::default();
        let mut serial = 0usize;
        let start_key = start.cost.key(optimization);
        let priority = self.operand_search_priority(&start, &goal, &preserve_counts, context);
        visited.insert(start.stack.clone(), start_key);
        queue.push(QueueEntry { priority, key: start_key, serial, node: start });

        let mut expansions = 0usize;
        while let Some(QueueEntry { key: queued_key, node, .. }) = queue.pop() {
            if visited.get(&node.stack).is_some_and(|&best| best != queued_key) {
                continue;
            }
            if Self::operand_goal_reached(&node.stack, &goal, &preserve_counts) {
                let plan = OperandPlan { actions: node.actions, cost: node.cost };
                return Some(self.verify_operand_plan(plan, &goal, preserved));
            }
            expansions += 1;
            if expansions > MAX_OPERAND_SEARCH_STATES {
                break;
            }

            for action in self.operand_search_actions(&node.stack, &goal, &preserve_counts, context)
            {
                let next = Self::apply_planned_action(&node, action, evm_version, cost_model);

                let key = next.cost.key(optimization);
                if visited.get(&next.stack).is_some_and(|&old| old <= key) {
                    continue;
                }
                visited.insert(next.stack.clone(), key);
                serial += 1;
                let priority =
                    self.operand_search_priority(&next, &goal, &preserve_counts, context);
                queue.push(QueueEntry { priority, key, serial, node: next });
            }
        }

        None
    }

    /// Replays every accepted planner tier in debug builds, matching the final goal check used by
    /// the full search.
    fn verify_operand_plan(
        &self,
        plan: OperandPlan,
        _goal: &[ValueId],
        _preserved: &[ValueId],
    ) -> OperandPlan {
        #[cfg(debug_assertions)]
        {
            let mut stack = self.stack.as_slice().to_vec();
            for action in &plan.actions {
                match action.op {
                    ScheduledOp::Stack(StackOp::Swap(depth)) => {
                        stack.swap(0, usize::from(depth));
                    }
                    ScheduledOp::Stack(StackOp::Dup(depth)) => {
                        let value = stack[usize::from(depth - 1)];
                        stack.insert(0, value);
                    }
                    ScheduledOp::Stack(StackOp::Pop) => {
                        stack.remove(0);
                    }
                    ScheduledOp::PushImmediate(_)
                    | ScheduledOp::LoadSpill(_)
                    | ScheduledOp::LoadArg(_) => {
                        stack.insert(0, action.pushed);
                    }
                }
            }
            debug_assert!(
                Self::operand_goal_reached_direct(&stack, _goal, _preserved),
                "operand plan does not reach its goal: stack={:?}, goal={:?}, \
                 preserved={:?}, plan={plan:?}",
                self.stack,
                _goal,
                _preserved
            );
        }
        plan
    }

    fn operand_goal_reached_direct(
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserved: &[ValueId],
    ) -> bool {
        stack.len() >= goal.len()
            && stack.iter().zip(goal).all(|(&actual, &expected)| actual == Some(expected))
            && preserved.iter().all(|value| stack[goal.len()..].contains(&Some(*value)))
            && stack[goal.len()..].iter().all(|&slot| {
                slot.is_none_or(|value| !goal.contains(&value) || preserved.contains(&value))
            })
    }

    fn operand_goal_reached_with(
        result_len: usize,
        goal: &[ValueId],
        preserved: &[ValueId],
        mut slot_at: impl FnMut(usize) -> Option<ValueId>,
    ) -> bool {
        result_len >= goal.len()
            && goal.iter().enumerate().all(|(i, &expected)| slot_at(i) == Some(expected))
            && preserved
                .iter()
                .all(|&value| (goal.len()..result_len).any(|i| slot_at(i) == Some(value)))
            && (goal.len()..result_len).all(|i| {
                slot_at(i).is_none_or(|value| !goal.contains(&value) || preserved.contains(&value))
            })
    }

    fn try_single_action_operand_plan(
        &self,
        goal: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        optimization: OptimizationMode,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        let stack = self.stack.as_slice();
        let mut best = None;
        let mut consider = |op: ScheduledOp, pushed| {
            let cost = ScheduleCost::default().with_op(&op, evm_version, cost_model);
            let plan =
                OperandPlan { actions: smallvec::smallvec![PlannedAction { op, pushed }], cost };
            if best
                .as_ref()
                .is_none_or(|old: &OperandPlan| plan.cost.cmp_for(old.cost, optimization).is_lt())
            {
                best = Some(plan);
            }
        };

        if matches!(optimization, OptimizationMode::Gas)
            && matches!(
                stack.first(),
                Some(Some(top))
                    if stack[1..]
                        .iter()
                        .take(MAX_STACK_ACCESS)
                        .any(|&slot| slot == Some(*top))
            )
            && Self::operand_goal_reached_with(stack.len() - 1, goal, preserved, |i| stack[i + 1])
        {
            consider(ScheduledOp::Stack(StackOp::Pop), None);
        }

        let max_swap = stack.len().saturating_sub(1).min(MAX_STACK_ACCESS);
        for depth in 1..=max_swap {
            if stack[0] != stack[depth]
                && (matches!(optimization, OptimizationMode::Gas)
                    || matches!((stack[0], stack[depth]), (Some(_), Some(_))))
                && Self::operand_goal_reached_with(stack.len(), goal, preserved, |i| {
                    if i == 0 {
                        stack[depth]
                    } else if i == depth {
                        stack[0]
                    } else {
                        stack[i]
                    }
                })
            {
                consider(ScheduledOp::Stack(StackOp::Swap(depth as u8)), None);
            }
        }

        let max_dup = stack.len().min(MAX_STACK_ACCESS);
        for depth in 0..max_dup {
            let Some(value) = stack[depth] else { continue };
            if Self::operand_goal_reached_with(stack.len() + 1, goal, preserved, |i| {
                if i == 0 { Some(value) } else { stack[i - 1] }
            }) {
                consider(ScheduledOp::Stack(StackOp::Dup((depth + 1) as u8)), Some(value));
            }
        }

        if let Some(&value) = goal.first()
            && Self::operand_goal_reached_with(stack.len() + 1, goal, preserved, |i| {
                if i == 0 { Some(value) } else { stack[i - 1] }
            })
            && let Some(op @ ScheduledOp::PushImmediate(_)) = self.materialize_operand(value, func)
        {
            let accessible = stack.iter().take(MAX_STACK_ACCESS).any(|&slot| slot == Some(value));
            if matches!(optimization, OptimizationMode::Gas) || !accessible {
                consider(op, Some(value));
            }
        }

        best
    }

    fn try_unary_operand_plan(
        &self,
        value: ValueId,
        preserve: bool,
        func: &Function,
        optimization: OptimizationMode,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        let stack = self.stack.as_slice();
        let copies = stack.iter().filter(|&&slot| slot == Some(value)).count();
        if stack.first() == Some(&Some(value))
            && ((preserve && stack[1..].contains(&Some(value)))
                || (!preserve && !stack[1..].contains(&Some(value))))
        {
            return Some(OperandPlan {
                actions: PlannedActions::new(),
                cost: ScheduleCost::default(),
            });
        }

        let mut candidates = SmallVec::<[(ScheduleCost, u8, PlannedActions); 4]>::new();
        let mut add_candidate = |priority: u8, actions: PlannedActions| {
            let cost = actions.iter().fold(ScheduleCost::default(), |cost, action| {
                cost.with_op(&action.op, evm_version, cost_model)
            });
            candidates.push((cost, priority, actions));
        };

        if let Some(depth) = stack.iter().position(|&slot| slot == Some(value)) {
            if preserve && depth < MAX_STACK_ACCESS {
                add_candidate(
                    1,
                    smallvec::smallvec![PlannedAction {
                        op: ScheduledOp::Stack(StackOp::Dup((depth + 1) as u8)),
                        pushed: Some(value),
                    }],
                );
            }

            if depth > 0
                && depth <= MAX_STACK_ACCESS
                && stack.first().is_some_and(|&top| top != Some(value))
                && ((!preserve && copies == 1) || (preserve && copies >= 2))
            {
                add_candidate(
                    0,
                    smallvec::smallvec![PlannedAction {
                        op: ScheduledOp::Stack(StackOp::Swap(depth as u8)),
                        pushed: None,
                    }],
                );
            } else if preserve
                && copies == 1
                && depth == MAX_STACK_ACCESS
                && stack.first().is_some_and(|&top| top != Some(value))
            {
                add_candidate(
                    2,
                    smallvec::smallvec![
                        PlannedAction {
                            op: ScheduledOp::Stack(StackOp::Swap(depth as u8)),
                            pushed: None,
                        },
                        PlannedAction {
                            op: ScheduledOp::Stack(StackOp::Dup(1)),
                            pushed: Some(value),
                        }
                    ],
                );
            }
        }

        if (preserve || copies == 0)
            && let Some(materialize) = self.materialize_operand(value, func)
        {
            let mut actions =
                smallvec::smallvec![PlannedAction { op: materialize.clone(), pushed: Some(value) }];
            if preserve && copies == 0 {
                let duplicate = ScheduledOp::Stack(StackOp::Dup(1));
                let duplicate_cost =
                    ScheduleCost::default().with_op(&duplicate, evm_version, cost_model);
                let materialize_cost =
                    ScheduleCost::default().with_op(&materialize, evm_version, cost_model);
                let op = if materialize_cost.cmp_for(duplicate_cost, optimization).is_lt() {
                    materialize
                } else {
                    duplicate
                };
                actions.push(PlannedAction { op, pushed: Some(value) });
            }
            add_candidate(3, actions);
        }

        candidates
            .into_iter()
            .min_by(|(a_cost, a_priority, a_actions), (b_cost, b_priority, b_actions)| {
                a_cost
                    .cmp_for(*b_cost, optimization)
                    .then(a_actions.len().cmp(&b_actions.len()))
                    .then(a_priority.cmp(b_priority))
            })
            .map(|(cost, _, actions)| OperandPlan { actions, cost })
    }

    fn try_goal_directed_operand_plan(
        &self,
        mut node: SearchNode,
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> Option<OperandPlan> {
        let OperandPlanningContext { optimization, evm_version, cost_model, .. } = context;
        let optimal_key = self.operand_search_priority(&node, goal, preserve_counts, context);
        let max_actions = self
            .operand_search_lower_bound(&node.stack, goal, preserve_counts, context)
            .actions as usize;

        for _ in 0..max_actions {
            let mut best = None;
            for action in self.operand_search_actions(&node.stack, goal, preserve_counts, context) {
                let next = Self::apply_planned_action(&node, action, evm_version, cost_model);
                let priority = self.operand_search_priority(&next, goal, preserve_counts, context);
                if best.as_ref().is_none_or(|(best_priority, _)| priority < *best_priority) {
                    best = Some((priority, next));
                }
            }
            node = best?.1;
        }

        (Self::operand_goal_reached(&node.stack, goal, preserve_counts)
            && node.cost.key(optimization) == optimal_key)
            .then_some(OperandPlan { actions: node.actions, cost: node.cost })
    }

    fn operand_search_actions(
        &self,
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> SmallVec<[PlannedAction; 24]> {
        let OperandPlanningContext { func, optimization, evm_version, cost_model } = context;
        let mut actions = SmallVec::<[PlannedAction; 24]>::new();
        if matches!(optimization, OptimizationMode::Gas)
            && Self::operand_pop_can_help(stack, goal, preserve_counts)
        {
            actions.push(PlannedAction { op: ScheduledOp::Stack(StackOp::Pop), pushed: None });
        }

        let max_swap = stack.len().saturating_sub(1).min(MAX_STACK_ACCESS);
        for depth in 1..=max_swap {
            if stack[0] != stack[depth] {
                actions.push(PlannedAction {
                    op: ScheduledOp::Stack(StackOp::Swap(depth as u8)),
                    pushed: None,
                });
            }
        }

        let mut required_counts = preserve_counts.clone();
        for &value in goal {
            *required_counts.entry(value).or_default() += 1;
        }
        for (&value, &required) in &required_counts {
            let current = stack.iter().filter(|&&slot| slot == Some(value)).count();
            let materialize = self.materialize_operand(value, func);
            let cheap_surplus_materialization = materialize.as_ref().is_some_and(|op| {
                let materialize_cost = ScheduleCost::default().with_op(op, evm_version, cost_model);
                let duplicate_cost = ScheduleCost::default().with_op(
                    &ScheduledOp::Stack(StackOp::Dup(1)),
                    evm_version,
                    cost_model,
                );
                materialize_cost.cmp_for(duplicate_cost, optimization).is_lt()
            });
            let cheap_surplus_copy_can_help = matches!(optimization, OptimizationMode::Gas)
                && preserve_counts.contains_key(&value)
                && cheap_surplus_materialization;
            if (current < required || cheap_surplus_copy_can_help)
                && let Some(depth) =
                    stack.iter().take(MAX_STACK_ACCESS).position(|&slot| slot == Some(value))
            {
                let duplicate = ScheduledOp::Stack(StackOp::Dup((depth + 1) as u8));
                let op = materialize
                    .filter(|materialize| {
                        let duplicate_cost =
                            ScheduleCost::default().with_op(&duplicate, evm_version, cost_model);
                        let materialize_cost =
                            ScheduleCost::default().with_op(materialize, evm_version, cost_model);
                        materialize_cost.cmp_for(duplicate_cost, optimization).is_lt()
                    })
                    .unwrap_or(duplicate);
                actions.push(PlannedAction { op, pushed: Some(value) });
            }
        }

        for &value in goal.iter().rev() {
            if actions.iter().any(|action| action.pushed == Some(value)) {
                continue;
            }
            let current = stack.iter().filter(|&&slot| slot == Some(value)).count();
            let required = required_counts.get(&value).copied().unwrap_or_default();
            let accessible = stack.iter().take(MAX_STACK_ACCESS).any(|&slot| slot == Some(value));
            if (current < required || !accessible)
                && let Some(op) = self.materialize_operand(value, func)
            {
                actions.push(PlannedAction { op, pushed: Some(value) });
            }
        }
        actions
    }

    fn operand_pop_can_help(
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
    ) -> bool {
        let Some(&Some(top)) = stack.first() else { return false };
        let required = goal.iter().filter(|&&value| value == top).count()
            + preserve_counts.get(&top).copied().unwrap_or_default();
        let current = stack.iter().filter(|&&slot| slot == Some(top)).count();
        current > required
            && stack[1..].iter().take(MAX_STACK_ACCESS).any(|&slot| slot == Some(top))
    }

    fn apply_planned_action(
        node: &SearchNode,
        action: PlannedAction,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> SearchNode {
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
            ScheduledOp::PushImmediate(_) | ScheduledOp::LoadSpill(_) | ScheduledOp::LoadArg(_) => {
                next.stack.insert(0, action.pushed);
            }
        }
        next.cost = next.cost.with_op(&action.op, evm_version, cost_model);
        next.actions.push(action);
        next
    }

    fn operand_search_priority(
        &self,
        node: &SearchNode,
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> [u32; 3] {
        let optimization = context.optimization;
        node.cost
            .plus(self.operand_search_lower_bound(&node.stack, goal, preserve_counts, context))
            .key(optimization)
    }

    fn operand_search_lower_bound(
        &self,
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> ScheduleCost {
        let OperandPlanningContext { func, optimization, evm_version, cost_model } = context;
        let mut required_counts = preserve_counts.clone();
        for &value in goal {
            *required_counts.entry(value).or_default() += 1;
        }

        let mut remaining = ScheduleCost::default();
        let mut has_missing_copies = false;
        let mut missing_counts = SmallVec::<[(ValueId, usize); 8]>::new();
        let mut total_missing = 0usize;
        for (value, required) in required_counts {
            let current = stack.iter().filter(|&&slot| slot == Some(value)).count();
            let missing = required.saturating_sub(current);
            if missing == 0 {
                continue;
            }
            has_missing_copies = true;
            missing_counts.push((value, missing));
            total_missing += missing;

            let duplicate = stack
                .iter()
                .take(MAX_STACK_ACCESS)
                .any(|&slot| slot == Some(value))
                .then_some(ScheduledOp::Stack(StackOp::Dup(1)));
            let materialize = self.materialize_operand(value, func);
            let first = match (duplicate, materialize.clone()) {
                (Some(duplicate), Some(materialize)) => {
                    let duplicate_cost =
                        ScheduleCost::default().with_op(&duplicate, evm_version, cost_model);
                    let materialize_cost =
                        ScheduleCost::default().with_op(&materialize, evm_version, cost_model);
                    if duplicate_cost.cmp_for(materialize_cost, optimization).is_le() {
                        duplicate
                    } else {
                        materialize
                    }
                }
                (Some(op), None) | (None, Some(op)) => op,
                (None, None) => continue,
            };
            remaining = remaining.with_op(&first, evm_version, cost_model);
            let subsequent = match materialize {
                Some(materialize) => {
                    let duplicate = ScheduledOp::Stack(StackOp::Dup(1));
                    let duplicate_cost =
                        ScheduleCost::default().with_op(&duplicate, evm_version, cost_model);
                    let materialize_cost =
                        ScheduleCost::default().with_op(&materialize, evm_version, cost_model);
                    if materialize_cost.cmp_for(duplicate_cost, optimization).is_lt() {
                        materialize
                    } else {
                        duplicate
                    }
                }
                None => ScheduledOp::Stack(StackOp::Dup(1)),
            };
            for _ in 1..missing {
                remaining = remaining.with_op(&subsequent, evm_version, cost_model);
            }
        }

        if has_missing_copies
            && !Self::operand_goal_reachable_by_missing_pushes(
                stack,
                goal,
                preserve_counts,
                &missing_counts,
                total_missing,
            )
        {
            let mut rearrange = ScheduleCost::default().with_op(
                &ScheduledOp::Stack(StackOp::Swap(1)),
                evm_version,
                cost_model,
            );
            if matches!(optimization, OptimizationMode::Gas)
                && Self::operand_pop_can_help(stack, goal, preserve_counts)
            {
                let pop = ScheduleCost::default().with_op(
                    &ScheduledOp::Stack(StackOp::Pop),
                    evm_version,
                    cost_model,
                );
                if pop.cmp_for(rearrange, optimization).is_lt() {
                    rearrange = pop;
                }
            }
            for &value in goal {
                let missing = missing_counts.iter().any(|&(missing, _)| missing == value);
                let accessible =
                    stack.iter().take(MAX_STACK_ACCESS).any(|&slot| slot == Some(value));
                let surplus_copy_can_help = matches!(optimization, OptimizationMode::Gas)
                    && preserve_counts.contains_key(&value);
                if (missing || accessible) && !surplus_copy_can_help {
                    continue;
                }
                if let Some(op) = self.materialize_operand(value, func) {
                    let cost = ScheduleCost::default().with_op(&op, evm_version, cost_model);
                    if cost.cmp_for(rearrange, optimization).is_lt() {
                        rearrange = cost;
                    }
                }
            }
            remaining = remaining.plus(rearrange);
        } else if !has_missing_copies && !Self::operand_goal_reached(stack, goal, preserve_counts) {
            let mut cheapest = None;
            let mut consider = |op: ScheduledOp| {
                let cost = ScheduleCost::default().with_op(&op, evm_version, cost_model);
                if cheapest.is_none_or(|old: ScheduleCost| cost.cmp_for(old, optimization).is_lt())
                {
                    cheapest = Some(cost);
                }
            };

            if let Some(&Some(top)) = stack.first()
                && stack
                    .iter()
                    .take(MAX_STACK_ACCESS + 1)
                    .skip(1)
                    .any(|&slot| slot.is_some_and(|value| value != top))
            {
                consider(ScheduledOp::Stack(StackOp::Swap(1)));
            }
            if matches!(optimization, OptimizationMode::Gas)
                && Self::operand_pop_can_help(stack, goal, preserve_counts)
            {
                consider(ScheduledOp::Stack(StackOp::Pop));
            }
            for &value in goal {
                let accessible =
                    stack.iter().take(MAX_STACK_ACCESS).any(|&slot| slot == Some(value));
                let surplus_copy_can_help = matches!(optimization, OptimizationMode::Gas)
                    && preserve_counts.contains_key(&value);
                if (!accessible || surplus_copy_can_help)
                    && let Some(op) = self.materialize_operand(value, func)
                {
                    consider(op);
                }
            }
            if let Some(cheapest) = cheapest {
                remaining = remaining.plus(cheapest);
            }
        }

        remaining
    }

    fn operand_goal_reachable_by_missing_pushes(
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        missing_counts: &[(ValueId, usize)],
        total_missing: usize,
    ) -> bool {
        let pushed_goal = total_missing.min(goal.len());
        for (i, &value) in goal[..pushed_goal].iter().enumerate() {
            let available = missing_counts
                .iter()
                .find_map(|&(candidate, count)| (candidate == value).then_some(count));
            let Some(available) = available else {
                return false;
            };
            if goal[..=i].iter().filter(|&&candidate| candidate == value).count() > available {
                return false;
            }
        }

        let consumed_from_stack = goal.len().saturating_sub(total_missing);
        if stack.len() < consumed_from_stack
            || !stack
                .iter()
                .take(consumed_from_stack)
                .zip(&goal[pushed_goal..])
                .all(|(&slot, &value)| slot == Some(value))
        {
            return false;
        }

        preserve_counts.iter().all(|(&value, &required)| {
            let pushed = missing_counts
                .iter()
                .find_map(|&(candidate, count)| (candidate == value).then_some(count))
                .unwrap_or_default();
            let pushed_into_goal =
                goal[..pushed_goal].iter().filter(|&&candidate| candidate == value).count();
            let pushed_tail = pushed.saturating_sub(pushed_into_goal);
            let stack_tail = stack[consumed_from_stack.min(stack.len())..]
                .iter()
                .filter(|&&slot| slot == Some(value))
                .count();
            pushed_tail + stack_tail >= required
        })
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
        }) && stack[goal.len()..].iter().all(|&slot| {
            slot.is_none_or(|value| !goal.contains(&value) || preserve_counts.contains_key(&value))
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
                // Load the function argument through the active calling convention.
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

        // Then, look for remaining dead values deeper in the stack and swap them to the top. A
        // contiguous run immediately below a live top needs only one SWAP followed by one POP per
        // dead value; removing the same values independently would need one SWAP per value.
        let mut depth = 1usize;
        while depth <= self.stack.depth().saturating_sub(1).min(MAX_STACK_ACCESS) {
            if let Some(val) = self.stack.peek(depth)
                && liveness.is_dead_after(val, block, inst_idx)
            {
                if depth == 1 {
                    let dead_run = (1..=self.stack.depth().saturating_sub(1).min(MAX_STACK_ACCESS))
                        .take_while(|&depth| {
                            self.stack
                                .peek(depth)
                                .is_some_and(|value| liveness.is_dead_after(value, block, inst_idx))
                        })
                        .count();
                    ops.push(StackOp::Swap(dead_run as u8));
                    self.stack.swap(dead_run as u8);
                    for _ in 0..dead_run {
                        ops.push(StackOp::Pop);
                        self.stack.pop();
                    }
                    continue;
                }

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
    use crate::mir::{
        BlockId, Function, FunctionBuilder, Immediate, InstKind, Instruction, MirType, Value,
    };
    use solar_interface::Ident;

    fn make_test_func() -> Function {
        let name = Ident::DUMMY;
        let mut func = Function::new(name);

        // Add some values
        func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(42))));
        func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(100))));

        func
    }

    fn exact_operand_cost(
        scheduler: &StackScheduler,
        operands: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        optimization: OptimizationMode,
        evm_version: EvmVersion,
    ) -> Option<OperandPlan> {
        let goal = operands.iter().rev().copied().collect::<Vec<_>>();
        let mut preserve_counts = FxHashMap::default();
        for &value in preserved {
            *preserve_counts.entry(value).or_default() += 1;
        }
        let context = OperandPlanningContext {
            func,
            optimization,
            evm_version,
            cost_model: OperandCostModel::DIRECT,
        };
        if StackScheduler::operand_goal_reached(scheduler.stack.as_slice(), &goal, &preserve_counts)
        {
            return Some(OperandPlan {
                actions: PlannedActions::new(),
                cost: ScheduleCost::default(),
            });
        }
        let start = SearchNode {
            stack: scheduler.stack.as_slice().iter().copied().collect(),
            actions: PlannedActions::new(),
            cost: ScheduleCost::default(),
        };
        // Keep the exhaustive test finite while allowing every requested and preserved occurrence
        // to introduce one positional copy beyond the starting layout.
        let max_stack_len = start.stack.len() + goal.len() + preserved.len();
        let mut queue = BinaryHeap::new();
        let mut visited = FxHashMap::default();
        let key = start.cost.key(optimization);
        visited.insert(start.stack.clone(), key);
        queue.push(QueueEntry { priority: key, key, serial: 0, node: start });

        let mut serial = 0;
        while let Some(QueueEntry { key: queued_key, node, .. }) = queue.pop() {
            if visited.get(&node.stack).is_some_and(|&best| best != queued_key) {
                continue;
            }
            if StackScheduler::operand_goal_reached(&node.stack, &goal, &preserve_counts) {
                return Some(OperandPlan { actions: node.actions, cost: node.cost });
            }

            let mut actions =
                scheduler.operand_search_actions(&node.stack, &goal, &preserve_counts, context);
            if matches!(optimization, OptimizationMode::Gas) {
                for &value in &goal {
                    let Some(op) = scheduler.materialize_operand(value, func) else { continue };
                    let materialize_cost =
                        ScheduleCost::default().with_op(&op, evm_version, OperandCostModel::DIRECT);
                    let duplicate_cost = ScheduleCost::default().with_op(
                        &ScheduledOp::Stack(StackOp::Dup(1)),
                        evm_version,
                        OperandCostModel::DIRECT,
                    );
                    if materialize_cost.cmp_for(duplicate_cost, optimization).is_lt()
                        && !actions.iter().any(|old| old.op == op)
                    {
                        actions.push(PlannedAction { op, pushed: Some(value) });
                    }
                }
            }

            for action in actions {
                let next = StackScheduler::apply_planned_action(
                    &node,
                    action,
                    evm_version,
                    OperandCostModel::DIRECT,
                );
                if next.stack.len() > max_stack_len {
                    continue;
                }
                let key = next.cost.key(optimization);
                if visited.get(&next.stack).is_some_and(|&old| old <= key) {
                    continue;
                }
                visited.insert(next.stack.clone(), key);
                serial += 1;
                queue.push(QueueEntry { priority: key, key, serial, node: next });
            }
        }

        None
    }

    fn sequences<T: Copy>(alphabet: &[T], max_len: usize) -> Vec<Vec<T>> {
        let mut all = vec![Vec::new()];
        let mut level = vec![Vec::new()];
        for _ in 0..max_len {
            let mut next = Vec::new();
            for prefix in &level {
                for &value in alphabet {
                    let mut sequence = prefix.clone();
                    sequence.push(value);
                    next.push(sequence);
                }
            }
            all.extend(next.iter().cloned());
            level = next;
        }
        all
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
            .plan_operands(
                &[b, a],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
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
            .plan_operands(
                &[b, a],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();
        assert_eq!(plan.actions.len(), 1);
        assert_eq!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Swap(1)));

        scheduler.apply_operand_plan(plan);
        assert_eq!(scheduler.stack.top(), Some(a));
        assert_eq!(scheduler.stack.peek(1), Some(b));
    }

    #[test]
    fn operand_plan_pops_redundant_top_copy() {
        let func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(b);
        scheduler.stack.push(a);
        scheduler.stack.push(b);

        let plan = scheduler
            .plan_operands(
                &[b, a],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Pop));
    }

    #[test]
    fn operand_plan_does_not_pop_unique_unrelated_value() {
        let func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(a);
        scheduler.stack.push(b);

        let plan = scheduler
            .plan_operands(
                &[a],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_ne!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Pop));
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
            .plan_operands(
                &[b, a],
                &[a, b],
                &func,
                OptimizationMode::Size,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
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
            .plan_operands(
                &[a, a],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();
        scheduler.apply_operand_plan(plan);

        assert_eq!(scheduler.stack.top(), Some(a));
        assert_eq!(scheduler.stack.peek(1), Some(a));
    }

    #[test]
    fn operand_plan_prefers_push0_for_repeated_zero() {
        let mut func = Function::new(Ident::DUMMY);
        let zero =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(zero);

        let plan = scheduler
            .plan_operands(
                &[zero, zero, zero],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.cost.static_gas, 4);
        assert_eq!(plan.actions.len(), 2);
        assert!(plan.actions.iter().all(|action| {
            action.op == ScheduledOp::PushImmediate(alloy_primitives::U256::ZERO)
        }));
    }

    #[test]
    fn operand_plan_does_not_defer_dead_zero_cleanup() {
        let mut func = Function::new(Ident::DUMMY);
        let zero =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let one =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let two =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(2))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(zero);
        scheduler.stack.push(two);
        scheduler.stack.push(one);

        let plan = scheduler
            .plan_operands(
                &[zero],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Swap(2)));
        assert_eq!(plan.cost.static_gas, 3);
    }

    #[test]
    fn operand_plan_prefers_push0_for_live_unary_value() {
        let mut func = Function::new(Ident::DUMMY);
        let zero =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let one =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(zero);
        scheduler.stack.push(one);

        let plan = scheduler
            .plan_operands(
                &[zero],
                &[zero],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.actions[0].op, ScheduledOp::PushImmediate(alloy_primitives::U256::ZERO));
        assert_eq!(plan.cost.static_gas, 2);
    }

    #[test]
    fn operand_plan_does_not_defer_multi_operand_cleanup() {
        let mut func = Function::new(Ident::DUMMY);
        let zero =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let one =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(zero);

        let plan = scheduler
            .plan_operands(
                &[zero, one],
                &[one],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.cost.static_gas, 9);
        assert_eq!(plan.actions[1].op, ScheduledOp::Stack(StackOp::Swap(1)));
    }

    #[test]
    fn operand_plan_uses_push0_for_live_multi_operand_value() {
        let mut func = Function::new(Ident::DUMMY);
        let zero =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let one =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(zero);
        scheduler.stack.push(zero);

        let plan = scheduler
            .plan_operands(
                &[zero, one, zero],
                &[zero],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.cost.static_gas, 5);
        assert_eq!(plan.actions[1].op, ScheduledOp::PushImmediate(alloy_primitives::U256::ZERO));
    }

    #[test]
    fn operand_search_matches_exact_cost_for_small_layouts() {
        let mut func = Function::new(Ident::DUMMY);
        let zero =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let one =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let layouts = sequences(&[None, Some(zero), Some(one)], 3);
        let operand_sets = sequences(&[zero, one], 3);
        let preserved_sets = [vec![], vec![zero], vec![one], vec![zero, one]];

        for layout in layouts {
            for operands in &operand_sets {
                if operands.is_empty() {
                    continue;
                }
                for preserved in &preserved_sets {
                    if preserved.iter().any(|value| !operands.contains(value)) {
                        continue;
                    }
                    for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
                        for evm_version in [EvmVersion::Paris, EvmVersion::Shanghai] {
                            let mut scheduler = StackScheduler::new();
                            for &slot in layout.iter().rev() {
                                if let Some(value) = slot {
                                    scheduler.stack.push(value);
                                } else {
                                    scheduler.stack.push_unknown();
                                }
                            }

                            let exact = exact_operand_cost(
                                &scheduler,
                                operands,
                                preserved,
                                &func,
                                optimization,
                                evm_version,
                            );
                            let actual = scheduler.plan_operands(
                                operands,
                                preserved,
                                &func,
                                optimization,
                                evm_version,
                                OperandCostModel::DIRECT,
                            );

                            match (actual, exact) {
                                (Some(actual), Some(exact)) => {
                                    assert_eq!(
                                        actual.cost.key(optimization),
                                        exact.cost.key(optimization),
                                        "layout={layout:?}, operands={operands:?}, \
                                         preserved={preserved:?}, optimization={optimization:?}, \
                                         evm_version={evm_version:?}, actual={actual:?}, \
                                         exact={exact:?}"
                                    );
                                }
                                (None, None) => {}
                                (actual, exact) => panic!(
                                    "plan mismatch for layout={layout:?}, operands={operands:?}, \
                                     preserved={preserved:?}, optimization={optimization:?}, \
                                     evm_version={evm_version:?}, actual={actual:?}, exact={exact:?}"
                                ),
                            }
                        }
                    }
                }
            }
        }
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
            .plan_operands(
                &[target],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
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
            .plan_operands(
                &[value],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();
        scheduler.apply_operand_plan(plan);
        scheduler.instruction_executed(1, None);

        assert_eq!(scheduler.depth(), 1);
        assert!(scheduler.stack.top().is_none());
    }

    #[test]
    fn operand_plan_uses_active_frame_reload_cost() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let inst = func.alloc_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let value = func.alloc_value(Value::Inst(inst));

        for (cost_model, expected_gas, expected_bytes) in
            [(OperandCostModel::DIRECT, 6, 4), (OperandCostModel::DYNAMIC_FRAME, 15, 7)]
        {
            let mut scheduler = StackScheduler::new();
            let slot = scheduler.spills.allocate(value);
            scheduler.spills.mark_reloadable(value);

            let plan = scheduler
                .plan_operands(
                    &[value],
                    &[],
                    &func,
                    OptimizationMode::Gas,
                    EvmVersion::Shanghai,
                    cost_model,
                )
                .unwrap();

            assert_eq!(plan.actions[0].op, ScheduledOp::LoadSpill(slot));
            assert_eq!(plan.cost.static_gas, expected_gas);
            assert_eq!(plan.cost.encoded_bytes, expected_bytes);
        }
    }

    #[test]
    fn operand_plan_preserves_resident_reloadable_value() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let inst = func.alloc_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let value = func.alloc_value(Value::Inst(inst));
        let mut scheduler = StackScheduler::new();
        scheduler.spills.allocate(value);
        scheduler.spills.mark_reloadable(value);
        scheduler.stack.push(value);

        let plan = scheduler
            .plan_operands(
                &[value],
                &[value],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(plan.actions[0].op, ScheduledOp::Stack(StackOp::Dup(1)));
        scheduler.apply_operand_plan(plan);
        scheduler.instruction_executed(1, None);
        assert_eq!(scheduler.stack.top(), Some(value));
    }

    #[test]
    fn operand_plan_materializes_high_arity_in_push_order() {
        let mut func = make_test_func();
        let mut operands = vec![ValueId::from_usize(0), ValueId::from_usize(1)];
        for value in 2..6 {
            operands.push(func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value),
            ))));
        }
        let mut scheduler = StackScheduler::new();

        let plan = scheduler
            .plan_operands(
                &operands,
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();
        let ops = scheduler.apply_operand_plan(plan);

        assert_eq!(ops.len(), operands.len());
        assert!(ops.iter().all(|op| matches!(op, ScheduledOp::PushImmediate(_))));
        assert!(scheduler.stack.iter().eq(operands.iter().rev().copied().map(Some)));
    }

    #[test]
    fn operand_plan_is_disabled_without_optimization() {
        let func = make_test_func();
        let value = ValueId::from_usize(0);
        let scheduler = StackScheduler::new();

        assert!(
            scheduler
                .plan_operands(
                    &[value],
                    &[],
                    &func,
                    OptimizationMode::None,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );
    }

    #[test]
    fn drops_contiguous_dead_values_with_one_swap() {
        let mut func = Function::new(Ident::DUMMY);
        let mut builder = FunctionBuilder::new(&mut func);
        let a = builder.add_param(MirType::uint256());
        let b = builder.add_param(MirType::uint256());
        let c = builder.add_param(MirType::uint256());
        let sum = builder.add(a, b);
        let result = builder.add(sum, c);
        builder.ret([result]);

        let liveness = Liveness::compute(&func);
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(c);
        scheduler.stack.push(b);
        scheduler.stack.push(a);
        scheduler.stack.push(sum);

        let ops = scheduler.drop_dead_values(&liveness, BlockId::ENTRY, 0);

        assert_eq!(ops, [StackOp::Swap(2), StackOp::Pop, StackOp::Pop]);
        assert!(scheduler.stack.iter().eq([Some(sum), Some(c)]));
    }

    #[test]
    fn schedule_cost_honors_gas_and_size_objectives() {
        let gas_plan = ScheduleCost { static_gas: 3, encoded_bytes: 5, actions: 1 };
        let size_plan = ScheduleCost { static_gas: 6, encoded_bytes: 2, actions: 2 };

        assert!(gas_plan.cmp_for(size_plan, OptimizationMode::Gas).is_lt());
        assert!(size_plan.cmp_for(gas_plan, OptimizationMode::Size).is_lt());
    }
}

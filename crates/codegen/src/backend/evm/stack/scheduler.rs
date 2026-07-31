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
//! the cheapest common case. Linear proofs cover distinct operands that all
//! require materialization, one resident last use among otherwise materialized
//! operands, and a binary operation whose only resident operand must survive.
//! Gas mode also uses verified one-action and unary plans before a
//! lower-bound-certified deterministic walk. Bounded A* is reserved for
//! layouts where those proofs do not succeed. Size mode uses the linear proofs
//! too, but skips the local one-action and unary fast paths because byte-cost
//! ties can leave different residual layouts that cost more to clean up after
//! the instruction. The available actions are:
//!
//! - use `SWAP1..16` to consume accessible last uses in place;
//! - use `DUP1..16` when another copy must survive or an operand repeats;
//! - in gas mode, pop a redundant top copy when an accessible copy remains;
//! - push an immediate with its hardfork-dependent encoded width when another required occurrence
//!   is missing or rematerialization is cheaper than duplicating a live copy;
//! - reload a spill known to hold the current runtime definition; and
//! - reload a function argument using the active calling convention.
//!
//! Spill freshness follows runtime control flow, which can differ from block emission order.
//! Non-recomputable live-ins may load a `reloadable` slot before its defining block has been
//! emitted because that block still stores the value first at runtime. An unstored cheap arithmetic
//! live-in is never exposed to this planner as a spill load. The fallback may recompute it when its
//! complete dependency tree ends in stable arguments, calldata, or transaction/block context;
//! otherwise it receives its own mandatory store when another block must reload it.
//! Constructor-staged immutable loads remain excluded because they are memory-backed until
//! deployment finishes. A value used only as a phi-edge source does not require a mandatory store
//! solely for that edge; a successfully preserved phi edge carries it on the stack.
//! Free-memory-pointer loads are stored at their definitions because the pointer may move before a
//! later use. Gas and unoptimized lowering give only cross-block values stable slots and reuse
//! block-local slots after emission. Size lowering keeps every free-memory-pointer slot stable
//! because the otherwise smaller local allocation increased generated size in corpus benchmarks.
//!
//! Anonymous stack words remain opaque in the modeled layout. The planner never
//! claims one as a MIR operand. It may move one by physical position while
//! arranging a known value, and known values below one can still be duplicated
//! when they are in reach.
//!
//! Each transition accumulates [`ScheduleCost`]. Direct and dynamic-frame loads
//! have separate costs because the latter also loads the frame pointer and adds
//! an offset. An admissible lower bound for missing copies and unavoidable rearrangement proves the
//! deterministic plan when its final cost matches the bound. A missing copy is priced as a `DUP`
//! whenever one already exists, even below the direct-access window; exposing that copy is
//! accounted for separately. The deterministic walk scores candidates by applying and undoing them
//! on one scratch layout, then records only the chosen action; it does not clone partial histories.
//! Otherwise the A* queue handles the ambiguous layout. A required value with no reload route below
//! `SWAP16` bypasses search so the fallback can expose and spill it. Size mode also bypasses every
//! dead operand copy below `SWAP16` because its action set cannot shorten the stack. Gas mode may
//! search only when an accessible surplus copy can be popped to expose the buried copy. Search
//! states retain parent links rather than full action histories, and separate per-search limits
//! bound expansions, created states, visited states, the open frontier, and estimated retained
//! bytes. Reaching a limit stops new expansion while already queued goals remain eligible. Searches
//! also share a function-wide A* expansion budget, and repeated capped failures stop later A*
//! attempts without disabling the cheaper planning tiers. Gas optimization orders plans by static
//! gas, encoded bytes, and action count. Size optimization orders them by encoded bytes, static
//! gas, and action count. Equal estimates prefer deeper states, then queue serials make traversal
//! deterministic. Returning `None` delegates to the existing correctness-oriented emitter.
//! Lower-bound-certified tiers and an A* result reached before pruning are least-cost within this
//! local action model, not whole-function stack-allocation optima. A goal already queued when a
//! limit is reached can still be validated and returned, but does not claim optimality over pruned
//! successors.
//!
//! ## Applying a plan
//!
//! [`StackScheduler::apply_operand_plan`] is the only operation that commits a
//! plan. Before that commit, every accepted planner tier is replayed against
//! the exact goal in all builds. Replay accepts only `DUP1..16` and `SWAP1..16`
//! and derives every immediate, argument, or spill load from the claimed MIR
//! value; an invalid plan falls back without changing state. Applying the
//! validated plan replays every action into the live model and returns the
//! matching physical operations for emission. Lowering then emits the EVM
//! instruction and calls [`StackScheduler::instruction_executed`] with its
//! stack effect.
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
    mir::{ArgIdx, BlockId, Function, ValueId},
};
use smallvec::SmallVec;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::{FxHashMap, StdEntry};
use std::{cell::Cell, cmp::Ordering, collections::BinaryHeap, mem::size_of};

const MAX_OPERAND_SEARCH_EXPANSIONS: usize = 1024;
const MAX_OPERAND_SEARCH_FUNCTION_EXPANSIONS: usize = 8 * MAX_OPERAND_SEARCH_EXPANSIONS;
const MAX_OPERAND_SEARCH_FUNCTION_LIMITS: usize = 4;
const MAX_OPERAND_SEARCH_CREATED_STATES: usize = 4096;
const MAX_OPERAND_SEARCH_VISITED_STATES: usize = 4096;
const MAX_OPERAND_SEARCH_OPEN_STATES: usize = 2048;
const MAX_OPERAND_SEARCH_RETAINED_BYTES: usize = 2 * 1024 * 1024;

type PlannedActions = SmallVec<[PlannedAction; 8]>;

// Keep the 17-word `SWAP16` window plus a ternary's three pushes inline.
const SEARCH_STACK_INLINE_CAPACITY: usize = MAX_STACK_ACCESS + 4;

type SearchStack = SmallVec<[Option<ValueId>; SEARCH_STACK_INLINE_CAPACITY]>;

/// Tracks physical stack state and plans operand preparation.
pub(crate) struct StackScheduler {
    /// Current stack state.
    pub stack: StackModel,
    /// Spill slots and their current reloadability.
    pub spills: SpillManager,
    /// Operations to emit.
    ops: Vec<ScheduledOp>,
    /// Remaining bounded-search work for this function.
    operand_search_budget: Cell<OperandSearchBudget>,
    #[cfg(test)]
    operand_search_stats: Cell<OperandSearchStats>,
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
    LoadArg(ArgIdx),
}

/// Cost of materializing a spill or argument under the active frame convention.
#[derive(Clone, Copy, Debug)]
pub(crate) struct OperandCostModel {
    load_static_gas: u32,
    load_encoded_bytes: u32,
}

impl OperandCostModel {
    /// A context-independent estimate for a direct address push followed by `MLOAD` or
    /// `CALLDATALOAD`.
    pub(crate) const DIRECT: Self = Self { load_static_gas: 6, load_encoded_bytes: 4 };

    /// A context-independent estimate for a frame-pointer load, offset addition, and final value
    /// load.
    pub(crate) const DYNAMIC_FRAME: Self = Self { load_static_gas: 15, load_encoded_bytes: 7 };
}

#[derive(Clone, Copy)]
struct OperandPlanningContext<'a> {
    func: &'a Function,
    required_counts: &'a FxHashMap<ValueId, usize>,
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
struct OperandSearchState {
    stack: SearchStack,
    cost: ScheduleCost,
    parent: Option<(usize, PlannedAction)>,
}

#[derive(Clone, Debug)]
struct OperandSearchQueueEntry {
    priority: [u32; 3],
    key: [u32; 3],
    serial: usize,
    state: usize,
    actions: u32,
}

impl PartialEq for OperandSearchQueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.serial == other.serial
    }
}

impl Eq for OperandSearchQueueEntry {}

impl PartialOrd for OperandSearchQueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for OperandSearchQueueEntry {
    fn cmp(&self, other: &Self) -> Ordering {
        other
            .priority
            .cmp(&self.priority)
            .then_with(|| self.actions.cmp(&other.actions))
            .then_with(|| other.serial.cmp(&self.serial))
    }
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default)]
struct OperandSearchStats {
    expansions: usize,
    created: usize,
    max_visited: usize,
    max_open: usize,
    retained_bytes: usize,
    unreachable_preflights: usize,
    limit_hit: bool,
    skipped_by_function_budget: bool,
}

#[derive(Clone, Copy, Debug)]
struct OperandSearchBudget {
    remaining_expansions: usize,
    limited_searches: usize,
}

impl Default for OperandSearchBudget {
    fn default() -> Self {
        Self { remaining_expansions: MAX_OPERAND_SEARCH_FUNCTION_EXPANSIONS, limited_searches: 0 }
    }
}

#[cfg(test)]
#[derive(Clone, Debug)]
struct QueueEntry {
    priority: [u32; 3],
    key: [u32; 3],
    serial: usize,
    node: SearchNode,
}

#[cfg(test)]
impl PartialEq for QueueEntry {
    fn eq(&self, other: &Self) -> bool {
        self.key == other.key && self.serial == other.serial
    }
}

#[cfg(test)]
impl Eq for QueueEntry {}

#[cfg(test)]
impl PartialOrd for QueueEntry {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
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
        Self {
            stack: StackModel::new(),
            spills: SpillManager::new(),
            ops: Vec::new(),
            operand_search_budget: Cell::new(OperandSearchBudget::default()),
            #[cfg(test)]
            operand_search_stats: Cell::new(OperandSearchStats::default()),
        }
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
        #[cfg(test)]
        self.operand_search_stats.set(OperandSearchStats::default());

        if matches!(optimization, OptimizationMode::None) {
            return None;
        }

        let goal = operands.iter().rev().copied().collect::<SmallVec<[_; 8]>>();
        if Self::operand_goal_reached_direct(self.stack.as_slice(), &goal, preserved) {
            let plan =
                OperandPlan { actions: PlannedActions::new(), cost: ScheduleCost::default() };
            return self.validate_operand_plan(plan, &goal, preserved, func);
        }
        if let Some(plan) = self.try_single_resident_operand_plan(
            operands,
            preserved,
            func,
            evm_version,
            cost_model,
        ) {
            return self.validate_operand_plan(plan, &goal, preserved, func);
        }
        if let Some(plan) = self.try_direct_materialization_operand_plan(
            operands,
            preserved,
            func,
            evm_version,
            cost_model,
        ) {
            return self.validate_operand_plan(plan, &goal, preserved, func);
        }
        if let Some(plan) = self.try_resident_nary_plan(
            &goal,
            preserved,
            func,
            optimization,
            evm_version,
            cost_model,
        ) {
            return self.validate_operand_plan(plan, &goal, preserved, func);
        }
        if let Some(plan) = self.try_preserved_resident_binary_plan(
            operands,
            preserved,
            func,
            optimization,
            evm_version,
            cost_model,
        ) {
            return self.validate_operand_plan(plan, &goal, preserved, func);
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
                return self.validate_operand_plan(plan, &goal, preserved, func);
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
                return self.validate_operand_plan(plan, &goal, preserved, func);
            }
        }

        let mut preserve_counts = FxHashMap::default();
        for &value in preserved {
            preserve_counts.entry(value).or_insert(1usize);
        }
        let mut required_counts = preserve_counts.clone();
        for &value in &goal {
            *required_counts.entry(value).or_default() += 1;
        }
        let stack = self.stack.as_slice();
        let inaccessible_required = required_counts.keys().any(|&value| {
            self.materialize_operand(value, func).is_none()
                && !stack.iter().take(MAX_STACK_ACCESS + 1).any(|&slot| slot == Some(value))
        });
        let inaccessible_dead_copy = goal.iter().any(|&value| {
            !preserve_counts.contains_key(&value)
                && stack.iter().skip(MAX_STACK_ACCESS + 1).any(|&slot| slot == Some(value))
        });
        let removable_accessible_surplus =
            stack.iter().take(MAX_STACK_ACCESS + 1).filter_map(|&slot| slot).any(|value| {
                let required = required_counts.get(&value).copied().unwrap_or_default();
                let current = stack.iter().filter(|&&slot| slot == Some(value)).count();
                current > required
                    && stack
                        .iter()
                        .take(MAX_STACK_ACCESS + 1)
                        .filter(|&&slot| slot == Some(value))
                        .count()
                        > 1
            });
        let inaccessible_dead_copy_is_removable =
            matches!(optimization, OptimizationMode::Gas) && removable_accessible_surplus;
        if inaccessible_required || (inaccessible_dead_copy && !inaccessible_dead_copy_is_removable)
        {
            #[cfg(test)]
            {
                let mut stats = self.operand_search_stats.get();
                stats.unreachable_preflights += 1;
                self.operand_search_stats.set(stats);
            }
            return None;
        }

        let start = SearchNode {
            stack: self.stack.as_slice().iter().copied().collect(),
            actions: PlannedActions::new(),
            cost: ScheduleCost::default(),
        };
        let context = OperandPlanningContext {
            func,
            required_counts: &required_counts,
            optimization,
            evm_version,
            cost_model,
        };
        if let Some(plan) =
            self.try_goal_directed_operand_plan(start.clone(), &goal, &preserve_counts, context)
        {
            return self.validate_operand_plan(plan, &goal, preserved, func);
        }

        let budget = self.operand_search_budget.get();
        if budget.remaining_expansions == 0
            || budget.limited_searches >= MAX_OPERAND_SEARCH_FUNCTION_LIMITS
        {
            #[cfg(test)]
            {
                let mut stats = self.operand_search_stats.get();
                stats.skipped_by_function_budget = true;
                self.operand_search_stats.set(stats);
            }
            return None;
        }
        let expansion_limit = MAX_OPERAND_SEARCH_EXPANSIONS.min(budget.remaining_expansions);
        let start_state = OperandSearchState { stack: start.stack, cost: start.cost, parent: None };
        let mut states = vec![start_state];
        let mut queue = BinaryHeap::new();
        let mut visited = FxHashMap::default();
        let mut serial = 0usize;
        let start_key = states[0].cost.key(optimization);
        let priority = self.operand_search_priority_parts(
            &states[0].stack,
            states[0].cost,
            &goal,
            &preserve_counts,
            context,
        );
        visited.insert(states[0].stack.clone(), start_key);
        queue.push(OperandSearchQueueEntry {
            priority,
            key: start_key,
            serial,
            state: 0,
            actions: 0,
        });
        let mut retained_bytes = Self::operand_search_state_bytes(&states[0].stack);
        let mut max_visited = visited.len();
        let mut max_open = queue.len();

        let mut expansions = 0usize;
        let mut limit_hit = false;
        while let Some(OperandSearchQueueEntry { key: queued_key, state: state_idx, .. }) =
            queue.pop()
        {
            let state = &states[state_idx];
            if visited.get(&state.stack).is_some_and(|&best| best != queued_key) {
                continue;
            }
            if Self::operand_goal_reached(&state.stack, &goal, &preserve_counts) {
                let plan = Self::operand_plan_from_search_state(&states, state_idx);
                #[cfg(test)]
                self.operand_search_stats.set(OperandSearchStats {
                    expansions,
                    created: states.len(),
                    max_visited,
                    max_open,
                    retained_bytes,
                    unreachable_preflights: 0,
                    limit_hit,
                    skipped_by_function_budget: false,
                });
                self.finish_operand_search(expansions, false);
                return self.validate_operand_plan(plan, &goal, preserved, func);
            }
            if expansions >= expansion_limit {
                limit_hit = true;
                continue;
            }
            expansions += 1;

            let stack = state.stack.clone();
            let cost = state.cost;
            for action in self.operand_search_actions(&stack, &goal, &preserve_counts, context) {
                if states.len() >= MAX_OPERAND_SEARCH_CREATED_STATES
                    || visited.len() >= MAX_OPERAND_SEARCH_VISITED_STATES
                    || queue.len() >= MAX_OPERAND_SEARCH_OPEN_STATES
                {
                    limit_hit = true;
                    break;
                }
                let mut next_stack = stack.clone();
                let _ = Self::apply_planned_stack_action(&mut next_stack, &action);
                let next_cost = cost.with_op(&action.op, evm_version, cost_model);
                let key = next_cost.key(optimization);
                let state_bytes = Self::operand_search_state_bytes(&next_stack);
                let next_stack = match visited.entry(next_stack) {
                    StdEntry::Occupied(mut entry) => {
                        if *entry.get() <= key {
                            continue;
                        }
                        if retained_bytes.saturating_add(state_bytes)
                            > MAX_OPERAND_SEARCH_RETAINED_BYTES
                        {
                            limit_hit = true;
                            break;
                        }
                        let next_stack = entry.key().clone();
                        entry.insert(key);
                        next_stack
                    }
                    StdEntry::Vacant(entry) => {
                        if retained_bytes.saturating_add(state_bytes)
                            > MAX_OPERAND_SEARCH_RETAINED_BYTES
                        {
                            limit_hit = true;
                            break;
                        }
                        let next_stack = entry.key().clone();
                        entry.insert(key);
                        next_stack
                    }
                };
                retained_bytes += state_bytes;
                serial += 1;
                let actions = next_cost.actions;
                let priority = self.operand_search_priority_parts(
                    &next_stack,
                    next_cost,
                    &goal,
                    &preserve_counts,
                    context,
                );
                let next_state = states.len();
                states.push(OperandSearchState {
                    stack: next_stack,
                    cost: next_cost,
                    parent: Some((state_idx, action)),
                });
                queue.push(OperandSearchQueueEntry {
                    priority,
                    key,
                    serial,
                    state: next_state,
                    actions,
                });
                max_visited = max_visited.max(visited.len());
                max_open = max_open.max(queue.len());
            }
        }

        #[cfg(test)]
        self.operand_search_stats.set(OperandSearchStats {
            expansions,
            created: states.len(),
            max_visited,
            max_open,
            retained_bytes,
            unreachable_preflights: 0,
            limit_hit,
            skipped_by_function_budget: false,
        });
        self.finish_operand_search(expansions, limit_hit);

        None
    }

    fn finish_operand_search(&self, expansions: usize, failed_due_to_limit: bool) {
        let mut budget = self.operand_search_budget.get();
        budget.remaining_expansions = budget.remaining_expansions.saturating_sub(expansions);
        if failed_due_to_limit {
            budget.limited_searches += 1;
        }
        self.operand_search_budget.set(budget);
    }

    /// Replays every accepted planner tier and rejects a malformed plan.
    fn validate_operand_plan(
        &self,
        plan: OperandPlan,
        goal: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
    ) -> Option<OperandPlan> {
        let mut stack = self.stack.clone();
        for action in &plan.actions {
            match action.op {
                ScheduledOp::Stack(StackOp::Swap(depth)) => {
                    if !(1..=MAX_STACK_ACCESS).contains(&usize::from(depth))
                        || usize::from(depth) >= stack.depth()
                    {
                        return None;
                    }
                    stack.swap(depth);
                }
                ScheduledOp::Stack(StackOp::Dup(depth)) => {
                    if !(1..=MAX_STACK_ACCESS).contains(&usize::from(depth)) {
                        return None;
                    }
                    let value = stack.peek(usize::from(depth - 1))?;
                    if !goal.contains(&value) {
                        return None;
                    }
                    stack.dup(depth);
                }
                ScheduledOp::Stack(StackOp::Pop) => {
                    let value = stack.top()?;
                    if !goal.contains(&value) && !stack.as_slice()[1..].contains(&Some(value)) {
                        return None;
                    }
                    stack.pop();
                }
                ScheduledOp::PushImmediate(_)
                | ScheduledOp::LoadSpill(_)
                | ScheduledOp::LoadArg(_) => {
                    let pushed = action.pushed?;
                    if !goal.contains(&pushed)
                        || self.materialize_operand(pushed, func).as_ref() != Some(&action.op)
                    {
                        return None;
                    }
                    stack.push(pushed);
                }
            }
        }
        Self::operand_goal_reached_direct(stack.as_slice(), goal, preserved).then_some(plan)
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

    /// Builds the optimal plan when one unique last-use operand is already on
    /// top and every other operand must be materialized.
    ///
    /// Every missing unique operand requires one materialization. If the
    /// resident value is not the deepest operand, at least one rearrangement is
    /// also necessary; pushing the surrounding operands in the derived order
    /// and swapping once reaches the exact goal at that lower bound.
    fn try_single_resident_operand_plan(
        &self,
        operands: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        if !preserved.is_empty() || operands.len() < 2 {
            return None;
        }
        let Some(&Some(resident)) = self.stack.as_slice().first() else {
            return None;
        };
        let resident_position = operands.iter().position(|&value| value == resident)?;
        if resident_position > MAX_STACK_ACCESS
            || operands.iter().enumerate().any(|(i, &value)| operands[i + 1..].contains(&value))
            || self.stack.as_slice()[1..]
                .iter()
                .any(|slot| slot.is_some_and(|value| operands.contains(&value)))
        {
            return None;
        }

        let mut actions = PlannedActions::new();
        let mut cost = ScheduleCost::default();
        let (materialize_order, swap_after) = if resident_position == 0 {
            (SmallVec::<[ValueId; 8]>::from_slice(&operands[1..]), None)
        } else {
            let mut order = SmallVec::<[ValueId; 8]>::new();
            order.extend_from_slice(&operands[1..resident_position]);
            order.push(operands[0]);
            let swap_after = Some(order.len());
            order.extend_from_slice(&operands[resident_position + 1..]);
            (order, swap_after)
        };

        for (i, value) in materialize_order.into_iter().enumerate() {
            let op = self.materialize_operand(value, func)?;
            cost = cost.with_op(&op, evm_version, cost_model);
            actions.push(PlannedAction { op, pushed: Some(value) });

            if swap_after == Some(i + 1) {
                let op = ScheduledOp::Stack(StackOp::Swap(resident_position as u8));
                cost = cost.with_op(&op, evm_version, cost_model);
                actions.push(PlannedAction { op, pushed: None });
            }
        }

        Some(OperandPlan { actions, cost })
    }

    /// Builds the only possible optimal plan when every distinct operand must be materialized.
    fn try_direct_materialization_operand_plan(
        &self,
        operands: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        if !preserved.is_empty()
            || operands.len() < 2
            || operands.iter().enumerate().any(|(i, &value)| operands[i + 1..].contains(&value))
            || self
                .stack
                .as_slice()
                .iter()
                .any(|slot| slot.is_some_and(|value| operands.contains(&value)))
        {
            return None;
        }

        let mut actions = PlannedActions::new();
        let mut cost = ScheduleCost::default();
        for &value in operands {
            let op = self.materialize_operand(value, func)?;
            cost = cost.with_op(&op, evm_version, cost_model);
            actions.push(PlannedAction { op, pushed: Some(value) });
        }
        Some(OperandPlan { actions, cost })
    }

    /// Builds the optimal plan when the first and penultimate goal values are the only resident
    /// operands and every value between or after them can be materialized.
    fn try_resident_nary_plan(
        &self,
        goal: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        optimization: OptimizationMode,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        let &[Some(top), Some(second), ..] = self.stack.as_slice() else { return None };
        let preserved = match preserved {
            [] => None,
            &[value] => Some(value),
            _ => return None,
        };
        if goal.len() < 3
            || goal.len() > MAX_STACK_ACCESS
            || goal.iter().enumerate().any(|(i, &value)| goal[i + 1..].contains(&value))
            || self.stack.as_slice()[2..]
                .iter()
                .any(|slot| slot.is_some_and(|value| goal.contains(&value)))
        {
            return None;
        }

        let first = goal[0];
        let penultimate = goal[goal.len() - 2];
        if self.materialize_operand(penultimate, func).is_some() {
            return None;
        }
        let retain_first = preserved == Some(first);
        if preserved.is_some() && !retain_first {
            return None;
        }
        let first_op = ScheduledOp::Stack(if top == first {
            StackOp::Dup(1)
        } else {
            StackOp::Swap((goal.len() - 1) as u8)
        });
        if self.materialize_operand(first, func).is_some_and(|materialize| {
            let resident_cost = ScheduleCost::default().with_op(&first_op, evm_version, cost_model);
            let materialize_cost =
                ScheduleCost::default().with_op(&materialize, evm_version, cost_model);
            materialize_cost.cmp_for(resident_cost, optimization).is_lt()
        }) {
            return None;
        }

        let mut actions = PlannedActions::new();
        let mut cost = ScheduleCost::default();
        let mut push = |op, pushed| {
            cost = cost.with_op(&op, evm_version, cost_model);
            actions.push(PlannedAction { op, pushed });
        };

        if top == first && second == penultimate && retain_first {
            push(ScheduledOp::Stack(StackOp::Dup(1)), Some(first));
            push(ScheduledOp::Stack(StackOp::Swap(2)), None);
            for &value in goal[1..goal.len() - 2].iter().rev() {
                push(self.materialize_operand(value, func)?, Some(value));
            }
            let trailing = goal[goal.len() - 1];
            push(self.materialize_operand(trailing, func)?, Some(trailing));
            push(ScheduledOp::Stack(StackOp::Swap((goal.len() - 1) as u8)), None);
        } else if top == penultimate && second == first && retain_first {
            let trailing = goal[goal.len() - 1];
            push(self.materialize_operand(trailing, func)?, Some(trailing));
            push(ScheduledOp::Stack(StackOp::Swap(1)), None);
            for &value in goal[1..goal.len() - 2].iter().rev() {
                push(self.materialize_operand(value, func)?, Some(value));
            }
            push(ScheduledOp::Stack(StackOp::Dup(goal.len() as u8)), Some(first));
        } else if top == penultimate && second == first && preserved.is_none() {
            for &value in goal[1..goal.len() - 2].iter().rev() {
                push(self.materialize_operand(value, func)?, Some(value));
            }
            let trailing = goal[goal.len() - 1];
            push(self.materialize_operand(trailing, func)?, Some(trailing));
            push(ScheduledOp::Stack(StackOp::Swap((goal.len() - 1) as u8)), None);
        } else {
            return None;
        }

        Some(OperandPlan { actions, cost })
    }

    /// Builds the optimal two-action plan for a preserved top-of-stack binary operand.
    fn try_preserved_resident_binary_plan(
        &self,
        operands: &[ValueId],
        preserved: &[ValueId],
        func: &Function,
        optimization: OptimizationMode,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> Option<OperandPlan> {
        let &[first, second] = operands else { return None };
        let &[preserved] = preserved else { return None };
        let Some(&Some(resident)) = self.stack.as_slice().first() else {
            return None;
        };
        if preserved != resident || first == second {
            return None;
        }

        let other = if first == resident {
            second
        } else if second == resident {
            first
        } else {
            return None;
        };
        if self.stack.as_slice()[1..].contains(&Some(other))
            || self.stack.as_slice()[1..].contains(&Some(resident))
        {
            return None;
        }
        let materialize = self.materialize_operand(other, func)?;
        let duplicate = ScheduledOp::Stack(StackOp::Dup(if first == resident { 1 } else { 2 }));
        let resident_op = self
            .materialize_operand(resident, func)
            .filter(|resident_op| {
                let materialize_cost =
                    ScheduleCost::default().with_op(resident_op, evm_version, cost_model);
                let duplicate_cost =
                    ScheduleCost::default().with_op(&duplicate, evm_version, cost_model);
                materialize_cost.cmp_for(duplicate_cost, optimization).is_lt()
            })
            .unwrap_or(duplicate);
        let resident_pushed = (!matches!(&resident_op, ScheduledOp::Stack(_))).then_some(resident);
        let ops = if first == resident {
            [(resident_op, resident_pushed), (materialize, Some(other))]
        } else {
            [(materialize, Some(other)), (resident_op, resident_pushed)]
        };

        let mut actions = PlannedActions::new();
        let mut cost = ScheduleCost::default();
        for (op, pushed) in ops {
            cost = cost.with_op(&op, evm_version, cost_model);
            actions.push(PlannedAction { op, pushed });
        }
        Some(OperandPlan { actions, cost })
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
        let lower_bound =
            self.operand_search_lower_bound(&node.stack, goal, preserve_counts, context);
        let optimal_key = node.cost.plus(lower_bound).key(optimization);
        let max_actions = lower_bound.actions as usize;

        for _ in 0..max_actions {
            let mut best = None;
            for action in self.operand_search_actions(&node.stack, goal, preserve_counts, context) {
                let popped = Self::apply_planned_stack_action(&mut node.stack, &action);
                let next_cost = node.cost.with_op(&action.op, evm_version, cost_model);
                let priority = next_cost
                    .plus(self.operand_search_lower_bound(
                        &node.stack,
                        goal,
                        preserve_counts,
                        context,
                    ))
                    .key(optimization);
                Self::undo_planned_stack_action(&mut node.stack, &action, popped);
                if best.as_ref().is_none_or(|(best_priority, _)| priority < *best_priority) {
                    best = Some((priority, action));
                }
            }
            let action = best?.1;
            let _ = Self::apply_planned_stack_action(&mut node.stack, &action);
            node.cost = node.cost.with_op(&action.op, evm_version, cost_model);
            node.actions.push(action);
        }

        let success = Self::operand_goal_reached(&node.stack, goal, preserve_counts)
            && node.cost.key(optimization) == optimal_key;
        success.then_some(OperandPlan { actions: node.actions, cost: node.cost })
    }

    fn operand_search_actions(
        &self,
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> SmallVec<[PlannedAction; 24]> {
        let OperandPlanningContext { func, required_counts, optimization, evm_version, cost_model } =
            context;
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

        for (&value, &required) in required_counts {
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

    #[cfg(test)]
    fn apply_planned_action(
        node: &SearchNode,
        action: PlannedAction,
        evm_version: EvmVersion,
        cost_model: OperandCostModel,
    ) -> SearchNode {
        let mut next = node.clone();
        let _ = Self::apply_planned_stack_action(&mut next.stack, &action);
        next.cost = next.cost.with_op(&action.op, evm_version, cost_model);
        next.actions.push(action);
        next
    }

    fn apply_planned_stack_action(
        stack: &mut SearchStack,
        action: &PlannedAction,
    ) -> Option<ValueId> {
        match &action.op {
            ScheduledOp::Stack(StackOp::Swap(depth)) => {
                stack.swap(0, usize::from(*depth));
                None
            }
            ScheduledOp::Stack(StackOp::Dup(depth)) => {
                let value = stack[usize::from(*depth - 1)];
                stack.insert(0, value);
                None
            }
            ScheduledOp::Stack(StackOp::Pop) => stack.remove(0),
            ScheduledOp::PushImmediate(_) | ScheduledOp::LoadSpill(_) | ScheduledOp::LoadArg(_) => {
                stack.insert(0, action.pushed);
                None
            }
        }
    }

    fn undo_planned_stack_action(
        stack: &mut SearchStack,
        action: &PlannedAction,
        popped: Option<ValueId>,
    ) {
        match action.op {
            ScheduledOp::Stack(StackOp::Swap(depth)) => {
                stack.swap(0, usize::from(depth));
            }
            ScheduledOp::Stack(StackOp::Dup(_))
            | ScheduledOp::PushImmediate(_)
            | ScheduledOp::LoadSpill(_)
            | ScheduledOp::LoadArg(_) => {
                stack.remove(0);
            }
            ScheduledOp::Stack(StackOp::Pop) => stack.insert(0, popped),
        }
    }

    fn operand_search_priority_parts(
        &self,
        stack: &[Option<ValueId>],
        cost: ScheduleCost,
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> [u32; 3] {
        let optimization = context.optimization;
        cost.plus(self.operand_search_lower_bound(stack, goal, preserve_counts, context))
            .key(optimization)
    }

    fn operand_plan_from_search_state(
        states: &[OperandSearchState],
        mut state: usize,
    ) -> OperandPlan {
        let cost = states[state].cost;
        let mut actions = PlannedActions::new();
        while let Some((parent, action)) = &states[state].parent {
            actions.push(action.clone());
            state = *parent;
        }
        actions.reverse();
        OperandPlan { actions, cost }
    }

    fn operand_search_state_bytes(stack: &SearchStack) -> usize {
        let heap_stack_bytes = if stack.spilled() {
            stack.capacity().saturating_mul(size_of::<Option<ValueId>>())
        } else {
            0
        };
        size_of::<OperandSearchState>()
            .saturating_add(size_of::<SearchStack>())
            .saturating_add(size_of::<OperandSearchQueueEntry>())
            .saturating_add(heap_stack_bytes.saturating_mul(2))
    }

    fn operand_search_lower_bound(
        &self,
        stack: &[Option<ValueId>],
        goal: &[ValueId],
        preserve_counts: &FxHashMap<ValueId, usize>,
        context: OperandPlanningContext<'_>,
    ) -> ScheduleCost {
        let OperandPlanningContext { func, required_counts, optimization, evm_version, cost_model } =
            context;

        let mut remaining = ScheduleCost::default();
        let mut has_missing_copies = false;
        let mut missing_counts = SmallVec::<[(ValueId, usize); 8]>::new();
        let mut total_missing = 0usize;
        for (&value, &required) in required_counts {
            let current = stack.iter().filter(|&&slot| slot == Some(value)).count();
            let missing = required.saturating_sub(current);
            if missing == 0 {
                continue;
            }
            has_missing_copies = true;
            missing_counts.push((value, missing));
            total_missing += missing;

            let duplicate =
                stack.contains(&Some(value)).then_some(ScheduledOp::Stack(StackOp::Dup(1)));
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

            if let Some(&top) = stack.first()
                && stack.iter().take(MAX_STACK_ACCESS + 1).skip(1).any(|&slot| slot != top)
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

    /// Returns whether an instruction result is cheap enough to recompute from its operands.
    pub(crate) fn is_cheap_recomputable_value(func: &Function, value: ValueId) -> bool {
        let crate::mir::Value::Inst(inst_id) = func.value(value) else {
            return false;
        };
        matches!(
            func.inst(*inst_id).kind,
            crate::mir::InstKind::Add(_, _)
                | crate::mir::InstKind::Sub(_, _)
                | crate::mir::InstKind::Mul(_, _)
                | crate::mir::InstKind::And(_, _)
                | crate::mir::InstKind::Or(_, _)
                | crate::mir::InstKind::Xor(_, _)
                | crate::mir::InstKind::Shl(_, _)
                | crate::mir::InstKind::Shr(_, _)
                | crate::mir::InstKind::Sar(_, _)
                | crate::mir::InstKind::ConstructorArgsBase
        )
    }

    /// Returns whether an unstored reserved slot must be recomputed instead of loaded.
    pub(crate) fn should_recompute_unstored_spill(&self, value: ValueId) -> bool {
        self.spills.get(value).is_some() && self.unstored_spill_requires_recompute(value)
    }

    fn unstored_spill_requires_recompute(&self, value: ValueId) -> bool {
        !self.spills.is_stored(value) && self.spills.is_recomputable(value)
    }

    /// Returns the value's spill slot when it can materialize it at this program point.
    pub(crate) fn reloadable_spill(&self, value: ValueId) -> Option<SpillSlot> {
        let slot = self.spills.get(value)?;
        (self.spills.is_reloadable(value) && !self.unstored_spill_requires_recompute(value))
            .then_some(slot)
    }

    fn materialize_operand(&self, value: ValueId, func: &Function) -> Option<ScheduledOp> {
        if let Some(slot) = self.reloadable_spill(value) {
            return Some(ScheduledOp::LoadSpill(slot));
        }

        match func.value(value) {
            crate::mir::Value::Immediate(imm) => imm.as_u256().map(ScheduledOp::PushImmediate),
            crate::mir::Value::Arg(index) => Some(ScheduledOp::LoadArg(*index)),
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
                // The value is accessible via DUP.
                let dup_n = (depth + 1) as u8;
                self.ops.push(ScheduledOp::Stack(StackOp::Dup(dup_n)));
                self.stack.dup(dup_n);
                return &self.ops;
            }
            // Value is too deep for DUP. It must either be reloadable from a spill slot or
            // re-emittable below.
            if let Some(slot) = self.reloadable_spill(value) {
                self.ops.push(ScheduledOp::LoadSpill(slot));
                self.stack.push(value);
                return &self.ops;
            }
        } else if let Some(slot) = self.reloadable_spill(value) {
            // The value is spilled, so load it.
            self.ops.push(ScheduledOp::LoadSpill(slot));
            self.stack.push(value);
            return &self.ops;
        }

        match func.value(value) {
            crate::mir::Value::Immediate(imm) => {
                // Push an immediate directly.
                if let Some(u256) = imm.as_u256() {
                    self.ops.push(ScheduledOp::PushImmediate(u256));
                    self.stack.push(value);
                }
            }
            crate::mir::Value::Arg(index) => {
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

    /// Returns whether a value is directly reachable, rematerializable, or runtime-reloadable.
    ///
    /// Returns false for an instruction result that is absent, too deep, or has only an unstored
    /// recomputable spill reservation.
    pub(crate) fn can_emit_value(&self, value: ValueId, func: &Function) -> bool {
        // Check if on stack and reachable by DUP.
        if let Some(depth) = self.stack.find(value) {
            return depth < MAX_STACK_ACCESS || self.reloadable_spill(value).is_some();
        }
        // Check whether the value is spilled.
        if self.reloadable_spill(value).is_some() {
            return true;
        }
        // Check the value type.
        matches!(func.value(value), crate::mir::Value::Immediate(_) | crate::mir::Value::Arg(_))
    }

    /// Records that an instruction consumed its operands and produced a result.
    /// This updates the stack model accordingly.
    pub(crate) fn instruction_executed(&mut self, consumed: usize, produced: Option<ValueId>) {
        // Pop consumed values.
        for _ in 0..consumed {
            self.stack.pop();
        }

        // Push the produced value.
        if let Some(val) = produced {
            self.stack.push(val);
        }

        debug_assert!(self.stack.depth() <= 1024, "Stack overflow: depth {}", self.stack.depth());
    }

    /// Records that a backend-synthesized instruction consumed inputs and produced an anonymous
    /// output with no MIR [`ValueId`].
    ///
    /// Branch inversion and switch comparisons use this for their temporary conditions.
    pub(crate) fn instruction_executed_untracked(&mut self, consumed: usize) {
        // Pop consumed values.
        for _ in 0..consumed {
            self.stack.pop();
        }
        // Push an unknown value to keep the stack depth correct.
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

    /// Drops reachable tracked values that are dead after an instruction.
    ///
    /// Returns the `SWAP` and `POP` operations used within the directly accessible window.
    pub(crate) fn drop_dead_values(
        &mut self,
        liveness: &Liveness,
        block: BlockId,
        inst_idx: usize,
    ) -> Vec<StackOp> {
        let mut ops = Vec::new();

        // First, pop dead values from the top.
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

                // Swap this dead value to the top and pop it.
                let swap_n = depth as u8;
                ops.push(StackOp::Swap(swap_n));
                self.stack.swap(swap_n);
                ops.push(StackOp::Pop);
                self.stack.pop();
                // Do not increment the depth since we removed an element.
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
    /// Returns the shuffle result containing the operations to emit. Failure leaves the live stack
    /// unchanged so callers can use their spill/reload fallback.
    pub(crate) fn shuffle_to_layout(&mut self, target: &[TargetSlot]) -> Option<ShuffleResult> {
        let shuffler = StackShuffler::new(&self.stack, target);
        let result = shuffler.shuffle()?;

        let mut next = self.stack.clone();
        for op in &result.ops {
            match op {
                StackOp::Dup(n) => next.dup(*n),
                StackOp::Swap(n) => next.swap(*n),
                StackOp::Pop => {
                    next.pop();
                }
            }
        }
        if next.depth() != target.len()
            || !next.iter().zip(target).all(|(actual, target)| match target {
                TargetSlot::Value(expected) => actual == Some(*expected),
            })
        {
            return None;
        }

        self.stack = next;
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

        // Add some values.
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
        let mut required_counts = preserve_counts.clone();
        for &value in &goal {
            *required_counts.entry(value).or_default() += 1;
        }
        let context = OperandPlanningContext {
            func,
            required_counts: &required_counts,
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
                let mut next = StackScheduler::apply_planned_action(
                    &node,
                    action,
                    evm_version,
                    OperandCostModel::DIRECT,
                );
                if next.stack.len() > max_stack_len {
                    continue;
                }
                let key = next.cost.key(optimization);
                match visited.entry(std::mem::take(&mut next.stack)) {
                    StdEntry::Occupied(mut entry) => {
                        if *entry.get() <= key {
                            continue;
                        }
                        next.stack.clone_from(entry.key());
                        entry.insert(key);
                    }
                    StdEntry::Vacant(entry) => {
                        next.stack.clone_from(entry.key());
                        entry.insert(key);
                    }
                }
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
    fn failed_layout_shuffle_preserves_live_stack() {
        let mut func = Function::new(Ident::DUMMY);
        let present =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let missing =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(present);

        assert!(scheduler.shuffle_to_layout(&[TargetSlot::Value(missing)]).is_none());
        assert_eq!(scheduler.stack.as_slice(), &[Some(present)]);
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
        // Should emit DUP2 to get v0 on top.

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
        let (_, deep) = func
            .alloc_value_inst(Instruction::new(InstKind::Add(v0, v1), Some(MirType::uint256())));
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
        scheduler.spills.mark_recomputable(deep);
        assert!(!scheduler.can_emit_value(deep, &func));

        scheduler.spills.mark_stored(deep);
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
    fn operand_plan_handles_resident_nary_layouts_without_search() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let (_, preserved) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let (_, second) =
            func.alloc_value_inst(Instruction::new(InstKind::Sub(a, b), Some(MirType::uint256())));
        let size = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(32))));
        let topic = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(256))));
        let trailing =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let operand_sets =
            [vec![trailing, second, preserved], vec![trailing, second, topic, size, preserved]];
        let cases = [
            ([preserved, second], true),
            ([second, preserved], true),
            ([second, preserved], false),
        ];

        for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
            for operands in &operand_sets {
                for (layout, retain) in &cases {
                    let mut scheduler = StackScheduler::new();
                    scheduler.spills.allocate(preserved);
                    scheduler.spills.mark_reloadable(preserved);
                    scheduler.spills.mark_stored(preserved);
                    for &value in layout.iter().rev() {
                        scheduler.stack.push(value);
                    }
                    let retained = [preserved];
                    let retained = if *retain { retained.as_slice() } else { &[] };
                    let exact = exact_operand_cost(
                        &scheduler,
                        operands,
                        retained,
                        &func,
                        optimization,
                        EvmVersion::Shanghai,
                    )
                    .unwrap();

                    let plan = scheduler
                        .plan_operands(
                            operands,
                            retained,
                            &func,
                            optimization,
                            EvmVersion::Shanghai,
                            OperandCostModel::DIRECT,
                        )
                        .unwrap();

                    assert_eq!(plan.cost, exact.cost);
                    let stats = scheduler.operand_search_stats.get();
                    assert_eq!(stats.expansions, 0);
                    assert_eq!(stats.created, 0);

                    scheduler.apply_operand_plan(plan);
                    scheduler.instruction_executed(operands.len(), None);
                    if *retain {
                        assert_eq!(scheduler.stack.as_slice(), &[Some(preserved)]);
                    } else {
                        assert_eq!(scheduler.stack.depth(), 0);
                    }
                }
            }
        }
    }

    #[test]
    fn operand_plan_handles_max_nary_layout_in_large_function_and_block() {
        const BIG_BLOCK_INSTRUCTIONS: usize = 4096;

        let mut func = Function::new(Ident::DUMMY);
        let (first, penultimate) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let zero = builder.imm_u64(0);
            let one = builder.imm_u64(1);
            for _ in 0..BIG_BLOCK_INSTRUCTIONS {
                builder.add(zero, one);
            }
            let first = builder.add(zero, one);
            let penultimate = builder.sub(one, zero);
            builder.stop();
            (first, penultimate)
        };
        assert_eq!(func.blocks[BlockId::ENTRY].instructions.len(), BIG_BLOCK_INSTRUCTIONS + 2);

        let middle = (0..MAX_STACK_ACCESS - 3)
            .map(|i| {
                func.alloc_value(Value::Immediate(Immediate::uint256(
                    alloy_primitives::U256::from(1000 + i),
                )))
            })
            .collect::<Vec<_>>();
        let trailing = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(2000))));
        let mut goal = Vec::with_capacity(MAX_STACK_ACCESS);
        goal.push(first);
        goal.extend(middle);
        goal.push(penultimate);
        goal.push(trailing);
        assert_eq!(goal.len(), MAX_STACK_ACCESS);
        let operands = goal.iter().rev().copied().collect::<Vec<_>>();

        let tail = (0..64)
            .map(|i| {
                func.alloc_value(Value::Immediate(Immediate::uint256(
                    alloy_primitives::U256::from(3000 + i),
                )))
            })
            .collect::<Vec<_>>();
        assert!(func.num_values() > BIG_BLOCK_INSTRUCTIONS);
        let cases = [
            ([first, penultimate], true),
            ([penultimate, first], true),
            ([penultimate, first], false),
        ];

        for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
            for (layout, retain) in &cases {
                let mut scheduler = StackScheduler::new();
                scheduler.spills.allocate(first);
                scheduler.spills.mark_reloadable(first);
                scheduler.spills.mark_stored(first);
                for &value in &tail {
                    scheduler.stack.push(value);
                }
                for &value in layout.iter().rev() {
                    scheduler.stack.push(value);
                }

                let retained = [first];
                let retained = if *retain { retained.as_slice() } else { &[] };
                let plan = scheduler
                    .plan_operands(
                        &operands,
                        retained,
                        &func,
                        optimization,
                        EvmVersion::Shanghai,
                        OperandCostModel::DIRECT,
                    )
                    .unwrap();

                let stats = scheduler.operand_search_stats.get();
                assert_eq!(stats.expansions, 0);
                assert_eq!(stats.created, 0);

                scheduler.apply_operand_plan(plan);
                scheduler.instruction_executed(operands.len(), None);
                let mut expected =
                    tail.iter().rev().copied().map(Some).collect::<Vec<Option<ValueId>>>();
                if *retain {
                    expected.insert(0, Some(first));
                }
                assert_eq!(scheduler.stack.as_slice(), expected);
            }
        }
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
    fn operand_search_byte_budget_counts_spilled_stacks() {
        let inline = SearchStack::new();
        let base_bytes = size_of::<OperandSearchState>()
            + size_of::<SearchStack>()
            + size_of::<OperandSearchQueueEntry>();
        assert_eq!(StackScheduler::operand_search_state_bytes(&inline), base_bytes);

        let mut spilled = SearchStack::new();
        spilled.resize(SEARCH_STACK_INLINE_CAPACITY + 1, None);
        assert!(spilled.spilled());
        assert_eq!(
            StackScheduler::operand_search_state_bytes(&spilled),
            base_bytes + 2 * spilled.capacity() * size_of::<Option<ValueId>>()
        );
    }

    #[test]
    fn operand_search_handles_anonymous_top_admissibly() {
        let mut func = Function::new(Ident::DUMMY);
        let a = func.alloc_param(MirType::uint256());
        let c = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(17))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(a);
        scheduler.stack.push(a);
        scheduler.stack.push_unknown();
        scheduler.stack.push(c);

        let plan = scheduler
            .plan_operands(
                &[a, c, a],
                &[a],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DIRECT,
            )
            .unwrap();

        assert_eq!(
            plan.actions.iter().map(|action| &action.op).collect::<Vec<_>>(),
            [
                &ScheduledOp::Stack(StackOp::Dup(3)),
                &ScheduledOp::Stack(StackOp::Swap(2)),
                &ScheduledOp::Stack(StackOp::Swap(3)),
            ]
        );
        assert_eq!(plan.cost.key(OptimizationMode::Gas), [9, 3, 3]);
        let stats = scheduler.operand_search_stats.get();
        assert!(stats.created > 0);
        assert!(stats.expansions <= MAX_OPERAND_SEARCH_EXPANSIONS);
        assert!(stats.created <= MAX_OPERAND_SEARCH_CREATED_STATES);
        assert!(stats.max_visited <= MAX_OPERAND_SEARCH_VISITED_STATES);
        assert!(stats.max_open <= MAX_OPERAND_SEARCH_OPEN_STATES);
        assert!(stats.retained_bytes <= MAX_OPERAND_SEARCH_RETAINED_BYTES);
    }

    #[test]
    fn operand_search_exhausts_function_budget_and_keeps_fast_paths() {
        let mut func = Function::new(Ident::DUMMY);
        let a = func.alloc_param(MirType::uint256());
        let c = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(17))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(a);
        scheduler.stack.push(a);
        scheduler.stack.push_unknown();
        scheduler.stack.push(c);
        scheduler
            .operand_search_budget
            .set(OperandSearchBudget { remaining_expansions: 1, limited_searches: 0 });

        assert!(
            scheduler
                .plan_operands(
                    &[a, c, a],
                    &[a],
                    &func,
                    OptimizationMode::Gas,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );
        let stats = scheduler.operand_search_stats.get();
        assert_eq!(stats.expansions, 1);
        assert!(stats.limit_hit);
        assert!(!stats.skipped_by_function_budget);
        assert_eq!(scheduler.operand_search_budget.get().remaining_expansions, 0);

        assert!(
            scheduler
                .plan_operands(
                    &[a, c, a],
                    &[a],
                    &func,
                    OptimizationMode::Gas,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );
        let stats = scheduler.operand_search_stats.get();
        assert_eq!(stats.expansions, 0);
        assert!(!stats.limit_hit);
        assert!(stats.skipped_by_function_budget);

        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(a);
        scheduler.operand_search_budget.set(OperandSearchBudget {
            remaining_expansions: 0,
            limited_searches: MAX_OPERAND_SEARCH_FUNCTION_LIMITS,
        });
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
        assert!(plan.is_free());
    }

    #[test]
    fn operand_search_lower_bound_is_admissible_with_anonymous_slots() {
        let mut func = Function::new(Ident::DUMMY);
        let a = func.alloc_param(MirType::uint256());
        let b = func.alloc_param(MirType::uint256());
        let layouts = sequences(&[None, Some(a), Some(b)], 4);
        let operand_sets = sequences(&[a, b], 2);
        let preserved_sets = [vec![], vec![a], vec![b]];

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
                        let mut scheduler = StackScheduler::new();
                        for &slot in layout.iter().rev() {
                            if let Some(value) = slot {
                                scheduler.stack.push(value);
                            } else {
                                scheduler.stack.push_unknown();
                            }
                        }
                        let Some(exact) = exact_operand_cost(
                            &scheduler,
                            operands,
                            preserved,
                            &func,
                            optimization,
                            EvmVersion::Shanghai,
                        ) else {
                            continue;
                        };
                        let goal = operands.iter().rev().copied().collect::<Vec<_>>();
                        let mut preserve_counts = FxHashMap::default();
                        for &value in preserved {
                            *preserve_counts.entry(value).or_default() += 1;
                        }
                        let mut required_counts = preserve_counts.clone();
                        for &value in &goal {
                            *required_counts.entry(value).or_default() += 1;
                        }
                        let context = OperandPlanningContext {
                            func: &func,
                            required_counts: &required_counts,
                            optimization,
                            evm_version: EvmVersion::Shanghai,
                            cost_model: OperandCostModel::DIRECT,
                        };
                        let lower = scheduler.operand_search_lower_bound(
                            scheduler.stack.as_slice(),
                            &goal,
                            &preserve_counts,
                            context,
                        );

                        assert!(
                            lower.key(optimization) <= exact.cost.key(optimization),
                            "layout={layout:?}, operands={operands:?}, preserved={preserved:?}, \
                             optimization={optimization:?}, lower={lower:?}, exact={exact:?}"
                        );
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
    fn operand_search_preflights_value_below_swap16() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let (_, target) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        for value in 0..=MAX_STACK_ACCESS {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value),
            )));
            scheduler.stack.push(filler);
        }
        assert_eq!(scheduler.stack.find(target), Some(MAX_STACK_ACCESS + 1));

        assert!(
            scheduler
                .plan_operands(
                    &[target],
                    &[],
                    &func,
                    OptimizationMode::Gas,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );
        let stats = scheduler.operand_search_stats.get();
        assert_eq!(stats.unreachable_preflights, 1);
        assert_eq!(stats.expansions, 0);
        assert_eq!(stats.created, 0);
        assert_eq!(stats.max_visited, 0);
        assert_eq!(stats.max_open, 0);
        assert_eq!(stats.retained_bytes, 0);
    }

    #[test]
    fn operand_search_preflights_dead_reloadable_copy_below_swap16() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let (_, target) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let (_, top) =
            func.alloc_value_inst(Instruction::new(InstKind::Sub(a, b), Some(MirType::uint256())));
        let mut scheduler = StackScheduler::new();
        scheduler.spills.allocate(target);
        scheduler.spills.mark_reloadable(target);
        scheduler.stack.push(target);
        for value in 0..MAX_STACK_ACCESS {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value),
            )));
            scheduler.stack.push(filler);
        }
        scheduler.stack.push(top);
        assert_eq!(scheduler.stack.find(target), Some(MAX_STACK_ACCESS + 1));

        assert!(
            scheduler
                .plan_operands(
                    &[top, target],
                    &[],
                    &func,
                    OptimizationMode::Gas,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );
        let stats = scheduler.operand_search_stats.get();
        assert_eq!(stats.unreachable_preflights, 1);
        assert_eq!(stats.expansions, 0);
    }

    #[test]
    fn size_operand_search_preflights_dead_copy_with_accessible_surplus() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let (_, target) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let (_, top) =
            func.alloc_value_inst(Instruction::new(InstKind::Sub(a, b), Some(MirType::uint256())));
        let surplus = func
            .alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(256))));
        let mut scheduler = StackScheduler::new();
        scheduler.spills.allocate(target);
        scheduler.spills.mark_reloadable(target);
        scheduler.stack.push(target);
        for value in 0..MAX_STACK_ACCESS - 2 {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value),
            )));
            scheduler.stack.push(filler);
        }
        scheduler.stack.push(surplus);
        scheduler.stack.push(surplus);
        scheduler.stack.push(top);
        assert_eq!(scheduler.stack.find(target), Some(MAX_STACK_ACCESS + 1));

        assert!(
            scheduler
                .plan_operands(
                    &[top, target],
                    &[],
                    &func,
                    OptimizationMode::Size,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );
        let stats = scheduler.operand_search_stats.get();
        assert_eq!(stats.unreachable_preflights, 1);
        assert_eq!(stats.expansions, 0);
    }

    #[test]
    fn operand_plan_validation_rejects_invalid_depths() {
        let mut func = Function::new(Ident::DUMMY);
        let target =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));

        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        let swap0 = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::Stack(StackOp::Swap(0)),
                pushed: None,
            }],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(swap0, &[target], &[], &func).is_none());

        for value in 0..=MAX_STACK_ACCESS {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value + 1),
            )));
            scheduler.stack.push(filler);
        }
        let swap17 = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::Stack(StackOp::Swap(17)),
                pushed: None,
            }],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(swap17, &[target], &[], &func).is_none());

        let scheduler = StackScheduler::new();
        let dup0 = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::Stack(StackOp::Dup(0)),
                pushed: None,
            }],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(dup0, &[target], &[], &func).is_none());

        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        for value in 0..MAX_STACK_ACCESS {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value + 100),
            )));
            scheduler.stack.push(filler);
        }
        let dup17 = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::Stack(StackOp::Dup(17)),
                pushed: Some(target),
            }],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(dup17, &[target], &[target], &func).is_none());
    }

    #[test]
    fn operand_plan_validation_rejects_forged_materializations() {
        let mut func = Function::new(Ident::DUMMY);
        let immediate =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let argument = func.alloc_param(MirType::uint256());
        let spilled = func.alloc_param(MirType::uint256());
        let mut scheduler = StackScheduler::new();
        let spill = scheduler.spills.allocate(spilled);
        scheduler.spills.mark_reloadable(spilled);

        let forged_immediate = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::PushImmediate(alloy_primitives::U256::from(1)),
                pushed: Some(immediate),
            }],
            cost: ScheduleCost::default(),
        };
        assert!(
            scheduler.validate_operand_plan(forged_immediate, &[immediate], &[], &func).is_none()
        );

        let forged_argument = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::LoadArg(ArgIdx::from_usize(1)),
                pushed: Some(argument),
            }],
            cost: ScheduleCost::default(),
        };
        assert!(
            scheduler.validate_operand_plan(forged_argument, &[argument], &[], &func).is_none()
        );

        let forged_spill = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::LoadSpill(SpillSlot { offset: spill.offset + 1 }),
                pushed: Some(spilled),
            }],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(forged_spill, &[spilled], &[], &func).is_none());
    }

    #[test]
    fn operand_plan_validation_preserves_non_operands() {
        let mut func = Function::new(Ident::DUMMY);
        let target =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let unrelated =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        scheduler.stack.push(unrelated);

        let forged_pop = OperandPlan {
            actions: smallvec::smallvec![PlannedAction {
                op: ScheduledOp::Stack(StackOp::Pop),
                pushed: None,
            }],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(forged_pop, &[target], &[], &func).is_none());
    }

    #[test]
    fn operand_plan_validation_can_drop_redundant_non_operand_copy() {
        let mut func = Function::new(Ident::DUMMY);
        let target =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::ZERO)));
        let unrelated =
            func.alloc_value(Value::Immediate(Immediate::uint256(alloy_primitives::U256::from(1))));
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        scheduler.stack.push(unrelated);
        scheduler.stack.push(unrelated);

        let plan = OperandPlan {
            actions: smallvec::smallvec![
                PlannedAction { op: ScheduledOp::Stack(StackOp::Pop), pushed: None },
                PlannedAction { op: ScheduledOp::Stack(StackOp::Swap(1)), pushed: None }
            ],
            cost: ScheduleCost::default(),
        };
        assert!(scheduler.validate_operand_plan(plan, &[target], &[], &func).is_some());
    }

    #[test]
    fn operand_plan_duplicates_value_below_dup16_reach() {
        let mut func = Function::new(Ident::DUMMY);
        let target = func.alloc_param(MirType::uint256());
        let mut scheduler = StackScheduler::new();
        scheduler.stack.push(target);
        for value in 0..MAX_STACK_ACCESS {
            let filler = func.alloc_value(Value::Immediate(Immediate::uint256(
                alloy_primitives::U256::from(value),
            )));
            scheduler.stack.push(filler);
        }
        assert_eq!(scheduler.stack.find(target), Some(MAX_STACK_ACCESS));

        let plan = scheduler
            .plan_operands(
                &[target, target],
                &[],
                &func,
                OptimizationMode::Gas,
                EvmVersion::Shanghai,
                OperandCostModel::DYNAMIC_FRAME,
            )
            .unwrap();

        assert_eq!(
            plan.actions.iter().map(|action| &action.op).collect::<Vec<_>>(),
            [&ScheduledOp::Stack(StackOp::Swap(16)), &ScheduledOp::Stack(StackOp::Dup(1))]
        );
        assert_eq!(plan.cost.key(OptimizationMode::Gas), [6, 2, 2]);
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
    fn operand_plan_rejects_unstored_recomputable_spill() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let (_, value) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
        let mut scheduler = StackScheduler::new();
        let slot = scheduler.spills.allocate(value);
        scheduler.spills.mark_reloadable(value);
        scheduler.spills.mark_recomputable(value);

        assert!(scheduler.should_recompute_unstored_spill(value));
        assert!(
            scheduler
                .plan_operands(
                    &[value],
                    &[],
                    &func,
                    OptimizationMode::Gas,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .is_none()
        );

        scheduler.spills.mark_stored(value);
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
        assert_eq!(plan.actions[0].op, ScheduledOp::LoadSpill(slot));
    }

    #[test]
    fn operand_plan_accepts_runtime_valid_unstored_spill() {
        let mut func = make_test_func();
        let (_, value) =
            func.alloc_value_inst(Instruction::new(InstKind::Gas, Some(MirType::uint256())));
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
                OperandCostModel::DIRECT,
            )
            .unwrap();
        assert_eq!(plan.actions[0].op, ScheduledOp::LoadSpill(slot));
    }

    #[test]
    fn operand_plan_uses_active_frame_reload_cost() {
        let mut func = make_test_func();
        let a = ValueId::from_usize(0);
        let b = ValueId::from_usize(1);
        let (_, value) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));

        for (cost_model, expected_gas, expected_bytes) in
            [(OperandCostModel::DIRECT, 6, 4), (OperandCostModel::DYNAMIC_FRAME, 15, 7)]
        {
            let mut scheduler = StackScheduler::new();
            let slot = scheduler.spills.allocate(value);
            scheduler.spills.mark_stored(value);

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
        let (_, value) =
            func.alloc_value_inst(Instruction::new(InstKind::Add(a, b), Some(MirType::uint256())));
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
    fn operand_plan_linearizes_single_resident_last_use() {
        let mut func = Function::new(Ident::DUMMY);
        let operands = (1..=5)
            .map(|value| {
                func.alloc_value(Value::Immediate(Immediate::uint256(
                    alloy_primitives::U256::from(value),
                )))
            })
            .collect::<Vec<_>>();

        for optimization in [OptimizationMode::Gas, OptimizationMode::Size] {
            let mut scheduler = StackScheduler::new();
            scheduler.stack.push(operands[1]);

            let plan = scheduler
                .plan_operands(
                    &operands,
                    &[],
                    &func,
                    optimization,
                    EvmVersion::Shanghai,
                    OperandCostModel::DIRECT,
                )
                .unwrap();
            assert_eq!(
                plan.actions.iter().map(|action| &action.op).collect::<Vec<_>>(),
                [
                    &ScheduledOp::PushImmediate(alloy_primitives::U256::from(1)),
                    &ScheduledOp::Stack(StackOp::Swap(1)),
                    &ScheduledOp::PushImmediate(alloy_primitives::U256::from(3)),
                    &ScheduledOp::PushImmediate(alloy_primitives::U256::from(4)),
                    &ScheduledOp::PushImmediate(alloy_primitives::U256::from(5)),
                ]
            );

            scheduler.apply_operand_plan(plan);
            assert!(scheduler.stack.iter().eq(operands.iter().rev().copied().map(Some)));
        }
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

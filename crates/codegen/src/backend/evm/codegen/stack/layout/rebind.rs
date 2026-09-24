//! Late binding of a loop's carried invariants to the stack that enters it.
//!
//! A planned loop header holds its phi results on top of the words the loop carries
//! unchanged. The planner orders those invariants from its model of the preheader's exit
//! stack, which lists a block's definitions newest first, while the emitter consumes last uses
//! in place and leaves the words in another order: entering the loop then pays a permutation
//! of words that no iteration moves. When the preheader is the only way into the loop and no
//! block of the loop has been emitted yet, the emitter can instead adopt the order its stack
//! already holds. Every planned layout inside the loop, every edge into one of its blocks and
//! every branch union inside it reorders those words the same way, so each edge inside the
//! loop keeps the shuffle it had; only the entry and the exits change. Each is priced the way
//! the edge emitters shape it, popping the words the target does not take, duplicating the
//! missing copies, pushing the constants the stack does not hold and shuffling the rest, and a
//! new order is taken only when the entry and the planned exits cost less together. The candidates
//! are the entering order, that order with the words an exit takes in the exit's own order, and
//! whatever pairwise exchanges of the best of them lower the total further. Words a resident
//! argument layout places, loops left by a planned jump, and layouts that already disagree about
//! the invariants' order leave the plan as it was.

use super::super::super::{
    BlockId, DenseBitSet, EvmCodegen, Function, FxHashMap, FxHashSet, GlobalStackPlan, InstKind,
    StackModel, StackOp, StackPhiEdge, StackPhiPlan, TargetSlot, Terminator, ValueId,
    lowered_stack_cost,
};
use crate::{backend::evm::codegen::stack::shuffler::StackShuffler, target::Target};

/// The loop's layouts under the new invariant order, with the gas of the loop's entry and
/// exit shuffles before and after.
struct Rebinding {
    entries: Vec<(BlockId, Vec<ValueId>)>,
    edges: Vec<(BlockId, StackPhiEdge)>,
    branches: Vec<(BlockId, Vec<ValueId>, StackPhiEdge, StackPhiEdge)>,
    before: usize,
    after: usize,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Rounds of pairwise exchanges tried on the best candidate order.
    const REBIND_ROUNDS: usize = 3;

    /// Reorders the invariants of the loop headed by `header`, entered only from `preheader`,
    /// to the order the current stack holds them in, when that makes the loop's entry and
    /// exits cheaper. Returns whether the plan changed.
    pub(in crate::backend::evm::codegen) fn rebind_loop_invariants(
        &self,
        func: &Function,
        plan: &mut StackPhiPlan,
        resident: &GlobalStackPlan,
        preheader: BlockId,
        header: BlockId,
        body: &DenseBitSet<BlockId>,
    ) -> bool {
        let Some(rebinding) = self.loop_rebinding(func, plan, resident, preheader, header, body)
        else {
            return false;
        };
        if rebinding.after >= rebinding.before {
            return false;
        }
        tracing::trace!(
            function = %func.name,
            ?header,
            before = rebinding.before,
            after = rebinding.after,
            "loop invariants rebound"
        );
        plan.entries.extend(rebinding.entries);
        plan.edges.extend(rebinding.edges);
        for (from, union, then_edge, else_edge) in rebinding.branches {
            let branch = plan.branch_edges.get_mut(&from).expect("branch was just read");
            branch.union = union;
            branch.then_edge = then_edge;
            branch.else_edge = else_edge;
        }
        true
    }

    fn loop_rebinding(
        &self,
        func: &Function,
        plan: &StackPhiPlan,
        resident: &GlobalStackPlan,
        preheader: BlockId,
        header: BlockId,
        body: &DenseBitSet<BlockId>,
    ) -> Option<Rebinding> {
        let edge = plan.edges.get(&preheader)?;
        let entry = plan.entries.get(&header)?;
        if edge.results != *entry || edge.sources.len() != entry.len() {
            return None;
        }
        let phis = func.blocks[header]
            .instructions
            .iter()
            .filter(|&&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
            .filter_map(|&inst| func.inst_result_value(inst))
            .collect::<FxHashSet<_>>();
        let pinned = body
            .iter()
            .chain([preheader])
            .filter_map(|block| resident.entry(block))
            .flatten()
            .copied()
            .collect::<FxHashSet<_>>();
        // header: [phi results..., invariants...], each invariant its own source
        let old = entry
            .iter()
            .zip(&edge.sources)
            .filter(|&(result, source)| {
                result == source && !phis.contains(result) && !pinned.contains(result)
            })
            .map(|(&result, _)| result)
            .collect::<Vec<_>>();
        if old.len() < 2
            || old.iter().any(|&value| entry.iter().filter(|&&other| other == value).count() != 1)
        {
            return None;
        }

        // The order the entering stack holds them in once the edge has popped what it does
        // not carry, top first.
        let mut trimmed = self.scheduler.stack.clone();
        pop_surplus(&mut trimmed, &edge.sources, &mut Vec::new())?;
        let mut held = Vec::with_capacity(old.len());
        for &value in &old {
            let mut positions = trimmed
                .iter()
                .enumerate()
                .filter(|&(_, slot)| slot == Some(value))
                .map(|(position, _)| position);
            let (Some(position), None) = (positions.next(), positions.next()) else {
                return None;
            };
            held.push((position, value));
        }
        held.sort_unstable();
        let held = held.into_iter().map(|(_, value)| value).collect::<Vec<_>>();
        let old_rank = rank(&old);
        let target = Target::new(self.gcx);
        let price =
            |source: &[Option<ValueId>], goal: &[ValueId]| edge_gas(func, source, goal, target);

        // The loop's branches, with the words each exit arm leaves with.
        let mut exits = Vec::new();
        for (&from, branch) in &plan.branch_edges {
            if !body.contains(from) {
                continue;
            }
            let Some(Terminator::Branch { then_block, else_block, .. }) =
                func.blocks[from].terminator
            else {
                return None;
            };
            for (arm, target) in [(&branch.then_edge, then_block), (&branch.else_edge, else_block)]
            {
                if !body.contains(target) && !arm.sources.is_empty() {
                    exits.push((branch.union.clone(), arm.sources.clone()));
                }
            }
        }

        // Candidates: the entering order, and that order with the words an exit takes put in
        // the exit's own order, so the exit leaves without permuting them back.
        let mut candidates = vec![held.clone()];
        for (_, sources) in &exits {
            let taken = sources.iter().filter(|value| old_rank.contains_key(value)).copied();
            let mut taken = taken.collect::<Vec<_>>();
            taken.dedup();
            let mut candidate = held.clone();
            let slots = candidate
                .iter()
                .enumerate()
                .filter(|(_, value)| taken.contains(value))
                .map(|(slot, _)| slot)
                .collect::<Vec<_>>();
            if slots.len() == taken.len() {
                for (&slot, value) in slots.iter().zip(taken) {
                    candidate[slot] = value;
                }
                if !candidates.contains(&candidate) {
                    candidates.push(candidate);
                }
            }
        }
        candidates.retain(|order| *order != old);
        if candidates.is_empty() {
            return None;
        }
        let entering = self.scheduler.stack.iter().collect::<Vec<_>>();
        let mut before = price(&entering, &edge.sources)?;
        for (union, sources) in &exits {
            let union = union.iter().copied().map(Some).collect::<Vec<_>>();
            before += price(&union, sources)?;
        }
        let mut best: Option<(Vec<ValueId>, Rebinding)> = None;
        for order in candidates {
            if let Some(rebinding) = self.rebind_to(func, plan, body, preheader, &old, &order)
                && best.as_ref().is_none_or(|(_, best)| rebinding.after < best.after)
            {
                best = Some((order, rebinding));
            }
        }
        // Exchanging two words of the best order can still spare the entry and the exits
        // shuffles that pull in opposite directions; keep any exchange that lowers the total.
        let (mut order, mut rebinding) = best?;
        for _ in 0..Self::REBIND_ROUNDS {
            let mut improved = false;
            for first in 0..order.len() {
                for second in first + 1..order.len() {
                    let mut exchanged = order.clone();
                    exchanged.swap(first, second);
                    if exchanged == old {
                        continue;
                    }
                    if let Some(candidate) =
                        self.rebind_to(func, plan, body, preheader, &old, &exchanged)
                        && candidate.after < rebinding.after
                    {
                        (order, rebinding) = (exchanged, candidate);
                        improved = true;
                    }
                }
            }
            if !improved {
                break;
            }
        }
        rebinding.before = before;
        Some(rebinding)
    }

    /// The loop's layouts with its invariants, `old` top first, moved to `order`, and the gas of
    /// the entry and exit shuffles under them.
    fn rebind_to(
        &self,
        func: &Function,
        plan: &StackPhiPlan,
        body: &DenseBitSet<BlockId>,
        preheader: BlockId,
        old: &[ValueId],
        order: &[ValueId],
    ) -> Option<Rebinding> {
        let target = Target::new(self.gcx);
        let price =
            |source: &[Option<ValueId>], goal: &[ValueId]| edge_gas(func, source, goal, target);
        let (old_rank, new_rank) = (rank(old), rank(order));
        // layout: [..., a, ..., b, ...] with a before b in the old order
        //   => [..., b, ..., a, ...] where the new order puts b first
        let reorder = |layout: &[ValueId]| -> Option<Vec<ValueId>> {
            let slots = layout
                .iter()
                .enumerate()
                .filter(|(_, value)| old_rank.contains_key(value))
                .map(|(slot, _)| slot)
                .collect::<Vec<_>>();
            let mut members = slots.iter().map(|&slot| layout[slot]).collect::<Vec<_>>();
            if !members.is_sorted_by_key(|value| old_rank[value]) {
                return None;
            }
            members.sort_by_key(|value| new_rank[value]);
            let mut reordered = layout.to_vec();
            for (&slot, value) in slots.iter().zip(members) {
                reordered[slot] = value;
            }
            Some(reordered)
        };
        // An edge's sources move with its results wherever it carries an invariant.
        let reorder_edge = |edge: &StackPhiEdge| -> Option<StackPhiEdge> {
            let results = reorder(&edge.results)?;
            let mut sources = edge.sources.clone();
            for (slot, (&before, &after)) in edge.results.iter().zip(&results).enumerate() {
                if old_rank.contains_key(&before) {
                    if sources[slot] != before {
                        return None;
                    }
                    sources[slot] = after;
                }
            }
            Some(StackPhiEdge { sources, results })
        };

        let mut entries = Vec::new();
        for block in body.iter() {
            if let Some(layout) = plan.entries.get(&block) {
                entries.push((block, reorder(layout)?));
            }
        }
        let mut edges = Vec::new();
        for (&from, planned) in &plan.edges {
            let terminator = func.blocks[from].terminator.as_ref()?;
            match *terminator {
                Terminator::Jump(target) if body.contains(target) => {
                    edges.push((from, reorder_edge(planned)?));
                }
                // A planned edge out of the loop leaves from a stack no layout here describes.
                _ if body.contains(from) => return None,
                _ if terminator.successors().iter().any(|&target| body.contains(target)) => {
                    return None;
                }
                _ => {}
            }
        }
        let entering = self.scheduler.stack.iter().collect::<Vec<_>>();
        let rebound = &edges.iter().find(|(from, _)| *from == preheader)?.1;
        let mut after = price(&entering, &rebound.sources)?;

        let mut branches = Vec::new();
        for (&from, branch) in &plan.branch_edges {
            let Some(Terminator::Branch { then_block, else_block, .. }) =
                func.blocks[from].terminator
            else {
                return None;
            };
            if !body.contains(from) {
                if body.contains(then_block) || body.contains(else_block) {
                    return None;
                }
                continue;
            }
            let union = reorder(&branch.union)?;
            let new_union = union.iter().copied().map(Some).collect::<Vec<_>>();
            let mut arms = [branch.then_edge.clone(), branch.else_edge.clone()];
            for (arm, target) in arms.iter_mut().zip([then_block, else_block]) {
                if body.contains(target) {
                    *arm = reorder_edge(arm)?;
                } else if !arm.sources.is_empty() {
                    // union => exit layout, on leaving the loop
                    after += price(&new_union, &arm.sources)?;
                }
            }
            let [then_edge, else_edge] = arms;
            branches.push((from, union, then_edge, else_edge));
        }
        Some(Rebinding { entries, edges, branches, before: 0, after })
    }
}

/// Each value's position in `order`.
fn rank(order: &[ValueId]) -> FxHashMap<ValueId, usize> {
    order.iter().enumerate().map(|(rank, &value)| (value, rank)).collect()
}

/// The gas of the stack operations an edge emits to turn `source`, top first, into `goal`:
/// the words it does not need are popped from the top down, missing copies are duplicated
/// onto the top, constants the stack does not hold are pushed, and the shuffler arranges the
/// rest, as the edge emitters do.
fn edge_gas(
    func: &Function,
    source: &[Option<ValueId>],
    goal: &[ValueId],
    target: Target,
) -> Option<usize> {
    let evm_version = target.evm_version();
    let mut stack = StackModel::from_top_to_bottom(source.iter().copied());
    let mut ops = Vec::new();
    pop_surplus(&mut stack, goal, &mut ops)?;
    // dup(depth) for each missing copy and push(value) for each missing constant, in goal order
    let mut present = FxHashMap::<ValueId, usize>::default();
    for value in stack.iter().flatten() {
        *present.entry(value).or_default() += 1;
    }
    let mut pushed = 0;
    for &value in goal {
        let count = present.entry(value).or_default();
        if *count > 0 {
            *count -= 1;
            continue;
        }
        if stack.find(value).is_none()
            && let Some(constant) = func.value(value).as_immediate().and_then(|imm| imm.as_u256())
        {
            pushed += target.push(constant).gas as usize;
            stack.push(value);
            continue;
        }
        let depth = u8::try_from(stack.find(value)? + 1).ok()?;
        if !StackOp::Dup(depth).is_valid() {
            return None;
        }
        ops.push(StackOp::Dup(depth));
        stack.apply(StackOp::Dup(depth));
    }
    let goal = goal.iter().copied().map(TargetSlot::Value).collect::<Vec<_>>();
    let shuffle = StackShuffler::for_evm_version(&stack, &goal, evm_version)
        .with_wide_permutation_search(true)
        .shuffle()?;
    ops.extend(shuffle.ops);
    if ops.iter().any(|op| op.lowering(evm_version).is_none()) {
        return None;
    }
    Some(lowered_stack_cost(&ops, evm_version).1 + pushed)
}

/// Pops every word of `stack` that `goal` has no use for, shallowest first, swapping each up
/// from its depth as the edge emitters do.
fn pop_surplus(stack: &mut StackModel, goal: &[ValueId], ops: &mut Vec<StackOp>) -> Option<()> {
    // swap(depth), pop for each word the goal has no use for
    loop {
        let mut remaining = FxHashMap::<ValueId, usize>::default();
        for &value in goal {
            *remaining.entry(value).or_default() += 1;
        }
        let surplus = stack.iter().position(|slot| {
            let Some(value) = slot else { return true };
            match remaining.get_mut(&value) {
                Some(count) if *count > 0 => {
                    *count -= 1;
                    false
                }
                _ => true,
            }
        });
        let Some(depth) = surplus else { return Some(()) };
        if depth > 0 {
            let depth = u8::try_from(depth).ok()?;
            if !StackOp::Swap(depth).is_valid() {
                return None;
            }
            ops.push(StackOp::Swap(depth));
            stack.apply(StackOp::Swap(depth));
        }
        ops.push(StackOp::Pop);
        stack.apply(StackOp::Pop);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType};
    use alloy_primitives::U256;
    use solar_config::{EvmVersion, OptimizationMode};
    use solar_interface::Ident;

    #[test]
    fn edge_gas_pushes_missing_constants() {
        let mut function = Function::new(Ident::DUMMY);
        let mut builder = FunctionBuilder::new(&mut function);
        let a = builder.add_param(MirType::I256);
        let b = builder.add_param(MirType::I256);
        let zero = builder.imm(0);
        let target = Target::with(
            EvmVersion::Cancun,
            OptimizationMode::Gas,
            Target::DEFAULT_EXPECTED_EXECUTIONS,
        );
        let push = target.push(U256::ZERO).gas as usize;
        let swap = lowered_stack_cost(&[StackOp::Swap(1)], EvmVersion::Cancun).1;
        let source = [Some(a), Some(b)];
        // [a, b] => [0, a, b]: push 0
        assert_eq!(edge_gas(&function, &source, &[zero, a, b], target), Some(push));
        // [a, b] => [a, 0, b]: push 0, swap 1
        assert_eq!(edge_gas(&function, &source, &[a, zero, b], target), Some(push + swap));
    }
}

//! Physical stack run simplification and self-contained expression ordering.
//!
//! Symbolic execution tracks equal stack identities through DUP/SWAP/EXCHANGE.
//! Normalization compares the checked private scheduler, a prefix-first placement,
//! a cycle decomposition for unique permutations, and a search bounded to three
//! operations over four incoming words. On legacy forks, a complete unique
//! permutation already has a minimum-cost cycle schedule, so the bounded search
//! is unnecessary. Extended forks retain the search for EXCHANGE. It accepts
//! only a smaller equivalent sequence with no increased input access and a peak
//! within proved stack capacity. A canonical leading literal can move after a
//! SWAP/EXCHANGE-only run when its distinct symbolic identity finishes on top.
//! The same cycle solver then permutes only the original words; complete input,
//! net height, peak and cost checks must also improve the existing incumbent.
//! No observed value, effect or control instruction is crossed. Literal sinking
//! additionally requires the caller's module-wide permission for new code/gas
//! changes; ordinary schedule costing leaves this extension disabled. The final
//! resident trial also prices query-only copies with it enabled, without granting
//! that permission to the executable module.
//! Reordering moves a literal across only
//! a self-contained pure expression to remove its final swap. Unknown effects
//! and noncanonical metadata bound each local analysis; no CFG edge is crossed.
//! Store address preparation stays canonical for later literal-data packing and
//! outlining; gas mode still removes swaps in zero initialization. Size mode
//! also moves compact literals before independent producers
//! to leave room for their temporary words during materialization.

use super::{
    super::{InstKind, Instruction, immediate, verify},
    canonical, discardable_push, pure, rewrite, stack_usage,
};
use crate::backend::evm::{op, scheduler::Stack};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::collections::VecDeque;

pub(super) fn stack_step(values: &mut Vec<usize>, kind: &InstKind) -> bool {
    let len = values.len();
    match *kind {
        InstKind::Dup(depth) if usize::from(depth) <= len && depth > 0 => {
            values.push(values[len - usize::from(depth)])
        }
        InstKind::Swap(depth) if usize::from(depth) < len && depth > 0 => {
            values.swap(len - 1, len - 1 - usize::from(depth))
        }
        InstKind::Exchange(a, b) if usize::from(a.max(b)) < len => {
            values.swap(len - 1 - usize::from(a), len - 1 - usize::from(b))
        }
        InstKind::Op(op::POP) if len > 0 => {
            values.pop();
        }
        _ => return false,
    }
    true
}

pub(super) fn normalize(
    insts: &mut Vec<Instruction>,
    version: EvmVersion,
    entry_max: Option<usize>,
    allow_sink_literal: bool,
) -> bool {
    let mut changed = false;
    let mut start = 0;
    while start < insts.len() {
        let mut end = start;
        let mut values = (0..version.reachable_stack_depth() + 1).collect::<Vec<_>>();
        let initial = values.clone();
        while end < insts.len()
            && canonical(&insts[end])
            && stack_step(&mut values, &insts[end].kind)
        {
            end += 1;
        }
        if end > start {
            let original = &insts[start..end];
            let room = entry_max.and_then(|entry| {
                let (_, delta, _) = stack_usage(&insts[..start])?;
                1024i64.checked_sub(entry as i64 + delta)
            });
            let mut best = original.to_vec();
            let mut consider = |candidate: Option<Vec<Instruction>>| {
                if let Some(candidate) = candidate
                    && stack_usage(&candidate).zip(stack_usage(original)).is_some_and(
                        |(new, old)| {
                            new.0 <= old.0
                                && (new.2 <= old.2 || room.is_some_and(|room| new.2 <= room))
                        },
                    )
                {
                    let new = immediate::cost(version, &candidate);
                    let old = immediate::cost(version, &best);
                    if new.0 <= old.0 && new.1 <= old.1 && new != old {
                        best = candidate;
                    }
                }
            };
            let mut stack = Stack::new(initial.clone());
            consider(stack.reconcile(&values, 0, version).ok());
            consider(place_then_pop(&initial, &values, version));
            let permutation = cycle_permutation(initial.len(), &values, version);
            let search = permutation.is_none() || version.has_extended_stack_ops();
            consider(permutation);
            // Legacy SWAPs have equal cost, and cycles minimize their count.
            // A three-operation length-preserving search using DUP/POP has at
            // most one SWAP; it cannot improve a unique permutation's cycle.
            if search {
                consider(short_plan(original, version, room));
            }
            let mut rewrite_start = start;
            if allow_sink_literal
                && start > 0
                && let Some(candidate) =
                    sink_literal(&insts[start - 1], original, &best, &values, version)
            {
                best = candidate;
                rewrite_start -= 1;
            }
            if rewrite_start != start || best != original {
                let len = best.len();
                // <optional literal>; <stack run> -> <cheaper equivalent schedule>
                changed |= rewrite(insts, rewrite_start, end - rewrite_start, best);
                end = rewrite_start + len;
            }
        }
        start = end.max(start + 1);
    }
    changed
}

/// Sinks a leading literal that a pure permutation leaves on top.
fn sink_literal(
    literal: &Instruction,
    original: &[Instruction],
    incumbent: &[Instruction],
    desired: &[usize],
    version: EvmVersion,
) -> Option<Vec<Instruction>> {
    let literal_id = desired.len().checked_sub(1)?;
    if !canonical(literal)
        || !matches!(literal.kind, InstKind::Push(_))
        || desired.last() != Some(&literal_id)
        || original.iter().any(|inst| {
            !canonical(inst) || !matches!(inst.kind, InstKind::Swap(_) | InstKind::Exchange(..))
        })
    {
        return None;
    }
    // push <literal>; <permutation> -> <original-word permutation>; push <literal>
    let mut candidate = cycle_permutation(literal_id, &desired[..literal_id], version)?;
    candidate.push(literal.clone());
    let new_usage = stack_usage(&candidate)?;
    for previous in [original, incumbent] {
        let (required, net, peak) = stack_usage(previous)?;
        // A canonical PUSH supplies one input and raises every later height by one.
        let old_usage = ((required - 1).max(0), net + 1, peak + 1);
        if new_usage.0 > old_usage.0 || new_usage.1 != old_usage.1 || new_usage.2 > old_usage.2 {
            return None;
        }
    }
    let new_cost = immediate::cost(version, &candidate);
    let (bytes, gas) = immediate::cost(version, incumbent);
    let (push_bytes, push_gas) = immediate::cost(version, std::slice::from_ref(literal));
    let old_cost = (bytes + push_bytes, gas + push_gas);
    (new_cost.0 <= old_cost.0 && new_cost.1 <= old_cost.1 && new_cost != old_cost)
        .then_some(candidate)
}

pub(super) fn dedup_stack(insts: &mut Vec<Instruction>, version: EvmVersion) -> bool {
    let mut zeros = FxHashSet::default();
    let mut values = Vec::new();
    let mut next = 0;
    let mut changed = false;
    let mut index = 0;
    while index < insts.len() {
        let before = values.clone();
        if let InstKind::Dup(depth) = insts[index].kind
            && canonical(&insts[index])
            && version.has_push0()
            && usize::from(depth) <= values.len()
            && depth > 0
            && zeros.contains(&values[values.len() - usize::from(depth)])
        {
            // dup <known zero> -> push0
            insts[index].kind = InstKind::Push(U256::ZERO);
            changed = true;
        }
        let inst = &insts[index];
        if !canonical(inst) {
            values.clear();
        } else if stack_step(&mut values, &inst.kind) {
            if before == values && matches!(inst.kind, InstKind::Swap(_) | InstKind::Exchange(..)) {
                // <permutation of equal identities> -> identity
                insts.remove(index);
                changed = true;
                continue;
            }
        } else if matches!(inst.kind, InstKind::Dup(_) | InstKind::Swap(_) | InstKind::Exchange(..))
        {
            // An inaccessible physical permutation can replace every tracked
            // suffix identity with an unknown incoming word.
            values.clear();
        } else if let Some((inputs, outputs)) = verify::effect(&inst.kind) {
            if usize::from(inputs) > values.len() {
                values.clear();
            } else {
                values.truncate(values.len() - usize::from(inputs));
            }
            for _ in 0..outputs {
                if matches!(inst.kind, InstKind::Push(value) if value.is_zero())
                    || matches!(inst.kind, InstKind::Op(op::PUSH0))
                {
                    zeros.insert(next);
                }
                values.push(next);
                next += 1;
            }
        } else {
            values.clear();
        }
        index += 1;
    }
    changed
}

pub(super) fn reorder(
    insts: &mut Vec<Instruction>,
    version: EvmVersion,
    entry_max: Option<usize>,
    mode: OptimizationMode,
) -> bool {
    let copies_only = mode.is_size() && !version.has_extended_stack_ops();
    let mut changed = false;
    let mut index = 2;
    while index < insts.len() {
        if canonical(&insts[index])
            && matches!(insts[index].kind, InstKind::Swap(1))
            && canonical(&insts[index - 1])
            && movable_push(&insts[index - 1].kind)
            && ((mode.is_gas()
                && matches!(insts[index - 1].kind, InstKind::Push(value) if value.is_zero()))
                || !insts.get(index + 1).is_some_and(|inst| {
                    matches!(inst.kind, InstKind::Op(op::MSTORE | op::MSTORE8))
                }))
        {
            let mut start = index - 1;
            let mut required = 1;
            while start > 0 && required > 0 {
                let inst = &insts[start - 1];
                if !canonical(inst) {
                    break;
                }
                if movable_push(&inst.kind) || matches!(inst.kind, InstKind::Dup(_)) {
                    required -= 1;
                } else if let InstKind::Op(code) = inst.kind
                    && movable_read(code)
                    && let Some((inputs, 1)) = op::stack_io(code)
                {
                    required = required - 1 + usize::from(inputs);
                } else {
                    break;
                }
                start -= 1;
            }
            if required == 0
                && (!copies_only
                    || insts[start..index - 1]
                        .iter()
                        .any(|inst| matches!(inst.kind, InstKind::Dup(_)))
                    || matches!(insts[index - 1].kind, InstKind::Push(value)
                        if immediate::materialize(version, value).len() > 1))
            {
                let mut replacement = vec![insts[index - 1].clone()];
                let mut local_height = 0usize;
                let mut valid = true;
                for inst in &insts[start..index - 1] {
                    let mut inst = inst.clone();
                    if let InstKind::Dup(depth) = &mut inst.kind
                        && usize::from(*depth) > local_height
                    {
                        *depth += 1;
                        valid &= usize::from(*depth) <= version.reachable_stack_depth();
                    }
                    if let Some((inputs, outputs)) = verify::effect(&inst.kind) {
                        local_height =
                            local_height.saturating_sub(usize::from(inputs)) + usize::from(outputs);
                    }
                    replacement.push(inst);
                }
                let original_peak = relative_usage(&insts[start..=index]).map(|(_, peak)| peak);
                let candidate_peak = relative_usage(&replacement).map(|(_, peak)| peak);
                let headroom = entry_max
                    .and_then(|entry| {
                        let (delta, _) = relative_usage(&insts[..start])?;
                        1024i64.checked_sub(i64::try_from(entry).ok()?.checked_add(delta)?)
                    })
                    .or_else(|| {
                        let (_, peak) = relative_usage(insts)?;
                        let (delta, _) = relative_usage(&insts[..start])?;
                        Some(peak - delta)
                    })
                    .or(original_peak);
                if valid && candidate_peak.zip(headroom).is_some_and(|(peak, room)| peak <= room) {
                    // <producer>; push b; swap1 -> push b; <producer with rebased prefix reads>
                    insts.splice(start..=index, replacement);
                    changed = true;
                    index = start + 1;
                }
            }
        }
        index += 1;
    }
    changed
}

fn movable_push(kind: &InstKind) -> bool {
    discardable_push(kind) || matches!(kind, InstKind::PushDeferred(_))
}

fn movable_read(opcode: u8) -> bool {
    pure(opcode)
        || matches!(
            opcode,
            op::ADDRESS
                | op::BALANCE
                | op::ORIGIN
                | op::CALLER
                | op::CALLVALUE
                | op::CALLDATALOAD
                | op::CALLDATASIZE
                | op::CODESIZE
                | op::GASPRICE
                | op::EXTCODESIZE
                | op::RETURNDATASIZE
                | op::EXTCODEHASH
                | op::BLOCKHASH
                | op::COINBASE
                | op::TIMESTAMP
                | op::NUMBER
                | op::PREVRANDAO
                | op::GASLIMIT
                | op::CHAINID
                | op::SELFBALANCE
                | op::BASEFEE
                | op::BLOBHASH
                | op::BLOBBASEFEE
                | op::SLOTNUM
                | op::MLOAD
                | op::SLOAD
                | op::TLOAD
                | op::MSIZE
                | op::KECCAK256
        )
}

fn relative_usage(insts: &[Instruction]) -> Option<(i64, i64)> {
    stack_usage(insts).map(|(_, delta, peak)| (delta, peak))
}

/// Decomposes a permutation of ascending identities into cycles through the top.
///
/// Each swap places the top identity at its final position. When the top is
/// already correct, one swap opens another nontrivial cycle. This uses the
/// minimum number of top swaps for unique identities, without extra stack words.
fn cycle_permutation(
    initial_len: usize,
    desired: &[usize],
    version: EvmVersion,
) -> Option<Vec<Instruction>> {
    if desired.len() != initial_len {
        return None;
    }
    let mut sorted = desired.to_vec();
    sorted.sort_unstable();
    if sorted.iter().copied().ne(0..desired.len()) {
        return None;
    }
    let mut values = sorted;
    let top = values.len().checked_sub(1)?;
    let mut output = Vec::new();
    while values != desired {
        let index = if values[top] != desired[top] {
            desired.iter().position(|&value| value == values[top])?
        } else {
            values.iter().zip(desired).position(|(value, wanted)| value != wanted)?
        };
        let depth = top - index;
        if depth > version.reachable_stack_depth() {
            return None;
        }
        // <top identity>; swap its final depth
        values.swap(index, top);
        output.push(InstKind::Swap(depth as u16).into());
    }
    Some(output)
}

/// Places the surviving prefix first, so a discarded suffix needs only POPs.
fn place_then_pop(
    initial: &[usize],
    desired: &[usize],
    version: EvmVersion,
) -> Option<Vec<Instruction>> {
    let mut values = initial.to_vec();
    let mut output = Vec::new();
    for (index, &value) in desired.iter().enumerate() {
        if values.get(index) == Some(&value) {
            continue;
        }
        if let Some(source) = values
            .iter()
            .enumerate()
            .skip(index)
            .find_map(|(i, &other)| (other == value).then_some(i))
        {
            // swap source; swap destination
            for position in [source, index] {
                let top = values.len() - 1;
                let depth = top - position;
                if depth > version.reachable_stack_depth() {
                    return None;
                }
                if depth > 0 {
                    values.swap(top, position);
                    output.push(InstKind::Swap(depth as u16).into());
                }
            }
        } else {
            let source = values.iter().rposition(|&other| other == value)?;
            let depth = values.len() - source;
            if depth > version.reachable_stack_depth() || values.len() == 1024 {
                return None;
            }
            // dup source
            // swap destination (when filling a hole rather than appending)
            values.push(value);
            output.push(InstKind::Dup(depth as u16).into());
            let top = values.len() - 1;
            if top > index {
                if top - index > version.reachable_stack_depth() {
                    return None;
                }
                values.swap(top, index);
                output.push(InstKind::Swap((top - index) as u16).into());
            }
        }
    }
    // pop <discarded suffix>
    while values.len() > desired.len() {
        values.pop();
        output.push(InstKind::Op(op::POP).into());
    }
    (values == desired).then_some(output)
}

/// Searches at most three instructions over at most four incoming words.
fn short_plan(
    original: &[Instruction],
    version: EvmVersion,
    room: Option<i64>,
) -> Option<Vec<Instruction>> {
    let (required, _, peak) = stack_usage(original)?;
    let required = usize::try_from(required).ok()?;
    if required > 4 || original.len() < 2 {
        return None;
    }
    let initial = (0..required).collect::<Vec<_>>();
    let mut desired = initial.clone();
    for inst in original {
        if !stack_step(&mut desired, &inst.kind) {
            return None;
        }
    }
    let max_height = required + usize::try_from(room.unwrap_or(peak).min(3).max(peak)).ok()?;
    let old_cost = immediate::cost(version, original);
    let mut queue = VecDeque::from([(initial.clone(), Vec::<Instruction>::new())]);
    let mut seen = FxHashMap::default();
    seen.insert(initial, (0, 0));
    let mut best = None::<Vec<Instruction>>;
    while let Some((state, prefix)) = queue.pop_front() {
        if state == desired {
            let cost = immediate::cost(version, &prefix);
            if cost.0 <= old_cost.0
                && cost.1 <= old_cost.1
                && cost != old_cost
                && best.as_ref().is_none_or(|best| cost < immediate::cost(version, best))
            {
                best = Some(prefix);
            }
            continue;
        }
        if prefix.len() == 3 {
            continue;
        }
        let mut operations = Vec::new();
        if !state.is_empty() {
            operations.push(InstKind::Op(op::POP));
        }
        for depth in 1..=state.len().min(version.reachable_stack_depth()) {
            if state.len() < max_height {
                operations.push(InstKind::Dup(depth as u16));
            }
            if depth < state.len() {
                operations.push(InstKind::Swap(depth as u16));
            }
        }
        if version.has_extended_stack_ops() {
            for a in 1..state.len() {
                for b in a + 1..state.len() {
                    if op::encode_exchange(a as u16, b as u16).is_some() {
                        operations.push(InstKind::Exchange(a as u16, b as u16));
                    }
                }
            }
        }
        for operation in operations {
            let mut next = state.clone();
            if !stack_step(&mut next, &operation) {
                continue;
            }
            let mut sequence = prefix.clone();
            // <search prefix>; <one legal physical stack operation>
            sequence.push(operation.into());
            let cost = immediate::cost(version, &sequence);
            if cost.0 > old_cost.0
                || cost.1 > old_cost.1
                || seen.get(&next).is_some_and(|&prior| prior <= cost)
            {
                continue;
            }
            seen.insert(next.clone(), cost);
            queue.push_back((next, sequence));
        }
    }
    best
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn legacy_cycles_dominate_three_operation_search() {
        // Enumerate every legal DUP/SWAP/POP path independently of stack_step,
        // including temporary words and POP's lower gas price. Every result
        // retaining the complete unique input domain must cost at least its
        // cycle schedule. This is precisely the bounded search we skip.
        let mut permutations = 0;
        for len in 1..=4 {
            let initial = (0..len).collect::<Vec<_>>();
            let mut pending = VecDeque::from([(initial, 0usize, 0u32)]);
            while let Some((state, count, gas)) = pending.pop_front() {
                let mut sorted = state.clone();
                sorted.sort_unstable();
                if sorted.iter().copied().eq(0..len) {
                    let cycle = cycle_permutation(len, &state, EvmVersion::Osaka).unwrap();
                    assert!(cycle.len() <= count);
                    assert!(3 * cycle.len() as u32 <= gas);
                    permutations += 1;
                }
                if count == 3 {
                    continue;
                }
                if !state.is_empty() {
                    let mut popped = state.clone();
                    popped.pop();
                    pending.push_back((popped, count + 1, gas + 2));
                    for depth in 1..=state.len() {
                        let top = state.len() - 1;
                        let mut copied = state.clone();
                        copied.push(state[state.len() - depth]);
                        pending.push_back((copied, count + 1, gas + 3));
                        if depth <= top {
                            let mut swapped = state.clone();
                            swapped.swap(top, top - depth);
                            pending.push_back((swapped, count + 1, gas + 3));
                        }
                    }
                }
            }
        }
        assert!(permutations > 100);
    }

    #[test]
    fn cycle_permutations_match_exhaustive_shortest_paths() {
        let initial = (0..6).collect::<Vec<_>>();
        let mut shortest = FxHashMap::from_iter([(initial.clone(), 0)]);
        let mut pending = VecDeque::from([initial.clone()]);
        while let Some(state) = pending.pop_front() {
            let distance = shortest[&state];
            for index in 0..5 {
                let mut next = state.clone();
                next.swap(index, 5);
                if !shortest.contains_key(&next) {
                    shortest.insert(next.clone(), distance + 1);
                    pending.push_back(next);
                }
            }
        }
        assert_eq!(shortest.len(), 720);
        for (desired, distance) in shortest {
            let code = cycle_permutation(initial.len(), &desired, EvmVersion::Osaka).unwrap();
            let mut actual = initial.clone();
            for inst in &code {
                assert!(stack_step(&mut actual, &inst.kind));
            }
            assert_eq!(actual, desired);
            assert_eq!(code.len(), distance);
        }
        assert!(cycle_permutation(3, &[0, 0, 2], EvmVersion::Osaka).is_none());
        assert!(cycle_permutation(3, &[0, 1], EvmVersion::Osaka).is_none());
        let desired = (0..18).rev().collect::<Vec<_>>();
        assert!(cycle_permutation(18, &desired, EvmVersion::Osaka).is_none());
        assert!(cycle_permutation(18, &desired, EvmVersion::Amsterdam).is_some());
    }
}

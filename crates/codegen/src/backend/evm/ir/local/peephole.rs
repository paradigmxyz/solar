//! Adjacent physical rewrites and terminal stack cleanup.
//!
//! Patterns preserve EVM pop order and stop at noncanonical stack metadata.
//! Algebraic identities precede exact constant folding through the retained word
//! evaluator. Memory patterns only remove already-observed identical accesses;
//! extra copies require a proved stack-capacity bound and mutable observations
//! never move. Terminal cleanup discards a pure suffix only when the terminator
//! cannot observe it. Terminal bodies may leave discarded prefix words underneath
//! their operands when the additional stack height remains safe. A SWAP1/POP pair
//! is redundant when the suffix observes at most its unchanged top incoming word.
//! These transforms run on blocks before assembly.

use super::super::{Block, InstKind, Instruction, TerminatorKind, immediate};
use super::{canonical, discardable_push, pure, rewrite, stack_usage, swapped};
use crate::{backend::evm::op, utils::eval::eval_opcode};
use alloy_primitives::U256;
use solar_config::EvmVersion;

pub(super) fn peephole(
    insts: &mut Vec<Instruction>,
    version: EvmVersion,
    entry_max: Option<usize>,
) -> bool {
    let mut changed = false;
    let mut index = 0;
    while index < insts.len() {
        let mut replacement = None;
        let tail = &insts[index..];
        if tail.len() >= 4 && tail[..4].iter().all(canonical) {
            match (&tail[0].kind, &tail[1].kind, &tail[2].kind, &tail[3].kind) {
                // dup2; binary; swap1; pop -> [swap1]; binary
                (
                    InstKind::Dup(2),
                    InstKind::Op(code),
                    InstKind::Swap(1),
                    InstKind::Op(op::POP),
                ) if op::stack_io(*code) == Some((2, 1)) => {
                    let sequence = if let Some(code) = swapped(*code) {
                        vec![InstKind::Op(code).into()]
                    } else {
                        vec![InstKind::Swap(1).into(), InstKind::Op(*code).into()]
                    };
                    replacement = Some((4, sequence));
                }
                // dup1; push address; store; pop -> push address; store
                (
                    InstKind::Dup(1),
                    InstKind::Push(_),
                    InstKind::Op(code),
                    InstKind::Op(op::POP),
                ) if matches!(*code, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE)
                    && !(tail.len() >= 6
                        && *code == op::MSTORE
                        && tail[1].kind == tail[4].kind
                        && matches!(tail[5].kind, InstKind::Op(op::MLOAD))) =>
                {
                    replacement = Some((4, vec![tail[1].clone(), tail[2].clone()]));
                }
                // push p; mstore; push p; mload -> dup1; push p; mstore
                (
                    InstKind::Push(a),
                    InstKind::Op(op::MSTORE),
                    InstKind::Push(b),
                    InstKind::Op(op::MLOAD),
                ) if a == b
                    && stack_usage(&insts[..index]).is_some_and(|(_, delta, _)| {
                        entry_max.is_some_and(|entry| entry as i64 + delta + 2 <= 1024)
                            || stack_usage(insts).is_some_and(|(_, _, peak)| delta + 2 <= peak)
                    }) =>
                {
                    replacement =
                        Some((4, vec![InstKind::Dup(1).into(), tail[0].clone(), tail[1].clone()]));
                }
                // iszero; iszero; push target; jumpi -> push target; jumpi
                (
                    InstKind::Op(op::ISZERO),
                    InstKind::Op(op::ISZERO),
                    InstKind::PushLabel(_),
                    InstKind::Op(op::JUMPI),
                ) => {
                    replacement = Some((4, vec![tail[2].clone(), tail[3].clone()]));
                }
                // eq; iszero; push target; jumpi -> sub; push target; jumpi
                (
                    InstKind::Op(op::EQ),
                    InstKind::Op(op::ISZERO),
                    InstKind::PushLabel(_),
                    InstKind::Op(op::JUMPI),
                ) => {
                    replacement = Some((
                        4,
                        vec![InstKind::Op(op::SUB).into(), tail[2].clone(), tail[3].clone()],
                    ));
                }
                _ => {}
            }
        }
        if replacement.is_none() && tail.len() >= 6 && tail[..6].iter().all(canonical) {
            match (
                &tail[0].kind,
                &tail[1].kind,
                &tail[2].kind,
                &tail[3].kind,
                &tail[4].kind,
                &tail[5].kind,
            ) {
                // dup1; push p; mstore; dup1; push p; mstore -> dup1; push p; mstore
                (
                    InstKind::Dup(1),
                    InstKind::Push(a),
                    InstKind::Op(op::MSTORE),
                    InstKind::Dup(1),
                    InstKind::Push(b),
                    InstKind::Op(op::MSTORE),
                ) if a == b => {
                    replacement = Some((6, tail[..3].to_vec()));
                }
                // dup1; push p; mstore; pop; push p; mload -> dup1; push p; mstore
                (
                    InstKind::Dup(1),
                    InstKind::Push(a),
                    InstKind::Op(op::MSTORE),
                    InstKind::Op(op::POP),
                    InstKind::Push(b),
                    InstKind::Op(op::MLOAD),
                ) if a == b => {
                    replacement = Some((6, tail[..3].to_vec()));
                }
                _ => {}
            }
        }
        if replacement.is_none()
            && tail.len() >= 5
            && tail[..5].iter().all(canonical)
            && let (
                InstKind::Push(a),
                InstKind::Op(op::MLOAD),
                InstKind::Dup(1),
                InstKind::Push(b),
                InstKind::Op(op::MSTORE),
            ) = (&tail[0].kind, &tail[1].kind, &tail[2].kind, &tail[3].kind, &tail[4].kind)
            && a == b
        {
            // push p; mload; dup1; push p; mstore -> push p; mload
            replacement = Some((5, tail[..2].to_vec()));
        }
        if replacement.is_none()
            && let InstKind::Swap(depth) = tail[0].kind
        {
            let pops = tail[1..]
                .iter()
                .take_while(|inst| canonical(inst) && matches!(inst.kind, InstKind::Op(op::POP)))
                .count();
            if pops > usize::from(depth) {
                // swap depth; pop * (depth + 1) -> pop * (depth + 1)
                replacement = Some((1, vec![]));
            } else if pops > 0
                && tail.len() > pops + 2
                && canonical(&tail[pops + 1])
                && canonical(&tail[pops + 2])
                && matches!(tail[pops + 2].kind, InstKind::Op(op::POP))
                && let InstKind::Swap(other) = tail[pops + 1].kind
                && usize::from(depth) == pops
                && usize::from(other) + pops <= version.reachable_stack_depth()
            {
                // swap p; pop * p; swap q; pop -> swap (p + q); pop * (p + 1)
                let mut sequence = vec![InstKind::Swap(depth + other).into()];
                sequence.extend_from_slice(&tail[1..=pops]);
                sequence.push(InstKind::Op(op::POP).into());
                replacement = Some((pops + 3, sequence));
            }
        }
        if replacement.is_none() && tail.len() >= 3 && tail[..3].iter().all(canonical) {
            match (&tail[0].kind, &tail[1].kind, &tail[2].kind) {
                // swap a; swap b; swap a -> exchange a, b
                (InstKind::Swap(a), InstKind::Swap(b), InstKind::Swap(c))
                    if a == c
                        && a != b
                        && (!version.has_extended_stack_ops()
                            || op::encode_exchange((*a).min(*b), (*a).max(*b)).is_some()) =>
                {
                    replacement =
                        Some((3, vec![InstKind::Exchange((*a).min(*b), (*a).max(*b)).into()]));
                }
                // iszero; iszero; iszero -> iszero
                (InstKind::Op(op::ISZERO), InstKind::Op(op::ISZERO), InstKind::Op(op::ISZERO)) => {
                    replacement = Some((3, vec![tail[0].clone()]))
                }
                // dup2; sink; pop -> swap1; sink
                (InstKind::Dup(2), InstKind::Op(code), InstKind::Op(op::POP))
                    if op::stack_io(*code) == Some((2, 0)) =>
                {
                    replacement = Some((3, vec![InstKind::Swap(1).into(), tail[1].clone()]));
                }
                _ => {}
            }
        }
        if replacement.is_none() && tail.len() >= 2 && tail[..2].iter().all(canonical) {
            match (&tail[0].kind, &tail[1].kind) {
                // not; not -> identity
                (InstKind::Op(op::NOT), InstKind::Op(op::NOT)) => replacement = Some((2, vec![])),
                // push / dup; pop -> identity
                (kind, InstKind::Op(op::POP))
                    if discardable_push(kind) || matches!(kind, InstKind::Dup(_)) =>
                {
                    replacement = Some((2, vec![]))
                }
                // swap a; swap a -> identity
                (InstKind::Swap(a), InstKind::Swap(b)) if a == b => replacement = Some((2, vec![])),
                // exchange a,b; swap a -> swap a; swap b (and symmetrically for b)
                (InstKind::Exchange(a, b), InstKind::Swap(depth)) if depth == a || depth == b => {
                    let other = if depth == a { *b } else { *a };
                    replacement = Some((
                        2,
                        vec![InstKind::Swap(*depth).into(), InstKind::Swap(other).into()],
                    ));
                }
                // swap a; exchange a,b -> swap b; swap a (and symmetrically for b)
                (InstKind::Swap(depth), InstKind::Exchange(a, b)) if depth == a || depth == b => {
                    let other = if depth == a { *b } else { *a };
                    replacement = Some((
                        2,
                        vec![InstKind::Swap(other).into(), InstKind::Swap(*depth).into()],
                    ));
                }
                // exchange a,b; exchange a,b -> identity
                (InstKind::Exchange(a, b), InstKind::Exchange(c, d)) if a == c && b == d => {
                    replacement = Some((2, vec![]))
                }
                // dup a; swap a -> dup a
                (InstKind::Dup(a), InstKind::Swap(b)) if a == b => {
                    replacement = Some((2, vec![tail[0].clone()]))
                }
                // swap1; symmetric/reversible operation -> operation with reversed operands
                (InstKind::Swap(1), InstKind::Op(code)) if swapped(*code).is_some() => {
                    replacement = Some((2, vec![InstKind::Op(swapped(*code).unwrap()).into()]))
                }
                // push zero; identity operation -> identity
                (InstKind::Push(value), InstKind::Op(code))
                    if value.is_zero()
                        && matches!(
                            *code,
                            op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR
                        ) =>
                {
                    replacement = Some((2, vec![]))
                }
                // push zero; eq -> iszero
                (InstKind::Push(value), InstKind::Op(op::EQ)) if value.is_zero() => {
                    replacement = Some((2, vec![InstKind::Op(op::ISZERO).into()]))
                }
                // push one; mul -> identity
                (InstKind::Push(value), InstKind::Op(op::MUL)) if *value == U256::ONE => {
                    replacement = Some((2, vec![]))
                }
                _ => {}
            }
        }
        if let Some((len, replacement)) = replacement {
            changed |= rewrite(insts, index, len, replacement);
            index = index.saturating_sub(4);
            continue;
        }
        if let InstKind::Op(opcode) = insts[index].kind
            && canonical(&insts[index])
            && let Some((inputs, 1)) = op::stack_io(opcode)
            && (1..=3).contains(&inputs)
            && index >= usize::from(inputs)
        {
            let start = index - usize::from(inputs);
            let operands = insts[start..index]
                .iter()
                .rev()
                .map(|inst| {
                    if canonical(inst)
                        && let InstKind::Push(value) = inst.kind
                    {
                        Some(value)
                    } else {
                        None
                    }
                })
                .collect::<Option<Vec<_>>>();
            if let Some(operands) = operands
                && let Some(value) = eval_opcode(opcode, &operands)
            {
                let candidate = immediate::materialize_bounded(version, value, usize::from(inputs));
                if immediate::cost(version, &candidate)
                    <= immediate::cost(version, &insts[start..=index])
                    && rewrite(insts, start, usize::from(inputs) + 1, candidate)
                {
                    changed = true;
                    index = start.saturating_sub(4);
                    continue;
                }
            }
        }
        index += 1;
    }
    changed
}

pub(super) fn dead_tail(
    insts: &mut Vec<Instruction>,
    terminator: &TerminatorKind,
    entry_max: Option<usize>,
) -> bool {
    let mut required = match terminator {
        TerminatorKind::Stop | TerminatorKind::Invalid | TerminatorKind::Unreachable => 0,
        TerminatorKind::Return | TerminatorKind::Revert => 2,
        TerminatorKind::SelfDestruct => 1,
        _ => return false,
    };
    let mut index = insts.len();
    while index > 0 {
        let inst = &insts[index - 1];
        if !canonical(inst) {
            break;
        }
        if required > 0 {
            if discardable_push(&inst.kind) {
                required -= 1;
            } else {
                break;
            }
        } else if !discardable_push(&inst.kind)
            && !matches!(
                inst.kind,
                InstKind::Dup(_)
                    | InstKind::Swap(_)
                    | InstKind::Exchange(..)
                    | InstKind::Op(op::POP)
            )
            && !matches!(inst.kind, InstKind::Op(code) if pure(code))
        {
            break;
        }
        index -= 1;
    }
    let preserve = match terminator {
        TerminatorKind::Return | TerminatorKind::Revert => 2,
        TerminatorKind::SelfDestruct => 1,
        _ => 0,
    };
    if required == 0 && index + preserve < insts.len() {
        let Some((_, _, old_peak)) = stack_usage(insts) else { return false };
        let mut candidate = insts.clone();
        candidate.drain(index..insts.len() - preserve);
        let Some((_, _, new_peak)) = stack_usage(&candidate) else { return false };
        if new_peak > old_peak && !entry_max.is_some_and(|entry| entry as i64 + new_peak <= 1024) {
            return false;
        }
        // <dead stack suffix>; <explicit terminal operands> -> <explicit terminal operands>
        *insts = candidate;
        true
    } else {
        false
    }
}

/// Leaves discarded prefix words below a terminal suffix with bounded input access.
pub(super) fn terminal_pops(
    insts: &mut Vec<Instruction>,
    terminator: &TerminatorKind,
    entry_max: Option<usize>,
) -> bool {
    let terminal_inputs = match terminator {
        TerminatorKind::Stop | TerminatorKind::Invalid | TerminatorKind::Unreachable => 0,
        TerminatorKind::Return | TerminatorKind::Revert => 2,
        TerminatorKind::SelfDestruct => 1,
        _ => return false,
    };
    let mut changed = false;
    let mut index = insts.len();
    while index > 0 {
        index -= 1;
        if !canonical(&insts[index]) || !matches!(insts[index].kind, InstKind::Op(op::POP)) {
            continue;
        }
        let pair = index > 0
            && canonical(&insts[index - 1])
            && matches!(insts[index - 1].kind, InstKind::Swap(1));
        let start = index - usize::from(pair);
        let survivors = i64::from(pair);
        let suffix = &insts[index + 1..];
        if !suffix.iter().all(|inst| {
            canonical(inst)
                && !matches!(
                    inst.kind,
                    InstKind::Op(op::JUMP | op::JUMPI | op::JUMPDEST | op::PC | op::GAS)
                )
        }) {
            continue;
        }
        let Some((required, delta, _)) = stack_usage(suffix) else { continue };
        if required > survivors || delta + survivors < terminal_inputs {
            continue;
        }
        let Some((_, _, old_peak)) = stack_usage(insts) else { continue };
        let mut candidate = insts.clone();
        candidate.drain(start..=index);
        let Some((_, _, new_peak)) = stack_usage(&candidate) else { continue };
        if new_peak <= old_peak || entry_max.is_some_and(|entry| entry as i64 + new_peak <= 1024) {
            // [swap1]; pop dead_prefix; <suffix using at most the retained top>; terminate
            // -> <same suffix and retained top>; terminate
            *insts = candidate;
            index = start;
            changed = true;
        }
    }
    changed
}

/// Returns the peak of a terminal body that does not read any incoming word.
pub(super) fn self_contained_terminal_peak(block: &Block) -> Option<i64> {
    let inputs = match block.terminator.kind {
        TerminatorKind::Stop | TerminatorKind::Invalid | TerminatorKind::Unreachable => 0,
        TerminatorKind::Return | TerminatorKind::Revert => 2,
        TerminatorKind::SelfDestruct => 1,
        _ => return None,
    };
    if block.insts.len() > 64 || !block.insts.iter().all(|inst| canonical(inst) && !matches!(
        inst.kind, InstKind::Op(op::JUMP | op::JUMPI | op::JUMPDEST | op::PC | op::GAS)
    )) {
        return None;
    }
    let (required, delta, peak) = stack_usage(&block.insts)?;
    (required == 0 && delta >= inputs).then_some(peak)
}

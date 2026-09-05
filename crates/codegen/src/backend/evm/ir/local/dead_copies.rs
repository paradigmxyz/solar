//! Elimination of saved stack copies across straight-line effects.
//!
//! Starting at a DUP, the analysis marks one occurrence as omitted and executes
//! the original instructions symbolically. The candidate retains the same values
//! and ordered effects while the private scheduler adjusts copies and permutations
//! around the omitted occurrence. A matching POP closes the proof. Consuming the
//! omitted occurrence as an ordinary operand, opaque metadata, control flow or
//! inaccessible aliases abort the candidate. We accept only a byte/gas Pareto
//! improvement with no increase in relative stack peak. The search is local to a
//! block and bounded; it never removes an ordinary effect or a deferred push.

use super::{
    super::{InstKind, Instruction, immediate, verify},
    canonical, rewrite, stack_usage,
};
use crate::backend::evm::{op, scheduler::Stack};
use solar_config::EvmVersion;

/// Removes redundant saved occurrences whose only final use is a discard.
pub(super) fn eliminate(insts: &mut Vec<Instruction>, version: EvmVersion) -> bool {
    let mut changed = false;
    let mut start = 0;
    while start < insts.len() {
        if let InstKind::Dup(depth) = insts[start].kind
            && canonical(&insts[start])
            && let Some((end, replacement)) = candidate(&insts[start..], depth, version)
        {
            // dup saved; <effects>; pop saved -> <same effects with adjusted stack access>
            changed |= rewrite(insts, start, end, replacement);
            start = start.saturating_sub(1);
        } else {
            start += 1;
        }
    }
    changed
}

/// An occurrence carries a value identity separately from its physical copy identity.
#[derive(Clone, Copy)]
struct Occurrence {
    value: usize,
    copy: usize,
}

fn candidate(
    input: &[Instruction],
    depth: u16,
    version: EvmVersion,
) -> Option<(usize, Vec<Instruction>)> {
    let prefix = version.reachable_stack_depth() + 1;
    let depth = usize::from(depth);
    if depth == 0 || depth > prefix {
        return None;
    }
    let mut logical =
        (0..prefix).map(|value| Occurrence { value, copy: value }).collect::<Vec<_>>();
    let omitted = prefix - depth;
    logical.push(Occurrence { value: omitted, copy: prefix });
    let mut next = prefix + 1;
    let initial = (0..prefix).collect::<Vec<_>>();
    let mut actual = Stack::new(initial);
    let desired = values_without(&logical, omitted);
    let mut output = actual.reconcile(&desired, 0, version).ok()?;
    let mut original_peak = prefix + 1;
    let mut candidate_peak = prefix.max(desired.len());
    for (index, inst) in input.iter().enumerate().skip(1).take(128) {
        if !canonical(inst) {
            return None;
        }
        let len = logical.len();
        let mut closed = false;
        let stack_only = match inst.kind {
            InstKind::Dup(depth) if depth > 0 && usize::from(depth) <= len => {
                let value = logical[len - usize::from(depth)].value;
                logical.push(Occurrence { value, copy: next });
                next += 1;
                true
            }
            InstKind::Swap(depth) if depth > 0 && usize::from(depth) < len => {
                logical.swap(len - 1, len - 1 - usize::from(depth));
                true
            }
            InstKind::Exchange(a, b) if usize::from(a.max(b)) < len => {
                logical.swap(len - 1 - usize::from(a), len - 1 - usize::from(b));
                true
            }
            InstKind::Op(op::POP) => {
                closed = logical.pop()?.copy == omitted;
                true
            }
            _ => false,
        };
        original_peak = original_peak.max(logical.len());
        if stack_only {
            let target = values_without(&logical, omitted);
            let before = actual.values().len();
            let adjustment = actual.reconcile(&target, 0, version).ok()?;
            candidate_peak = candidate_peak.max(stack_peak(before, &adjustment)?);
            // <original stack operation> -> <access adjusted around omitted occurrence>
            output.extend(adjustment);
        } else {
            if matches!(
                inst.kind,
                InstKind::Dup(_)
                    | InstKind::Swap(_)
                    | InstKind::Exchange(..)
                    | InstKind::Op(
                        op::JUMP
                            | op::JUMPI
                            | op::JUMPDEST
                            | op::CALLF
                            | op::JUMPF
                            | op::RETF
                            | op::PC
                            | op::GAS
                    )
            ) {
                return None;
            }
            let (inputs, outputs) = verify::effect(&inst.kind).or(inst.stack_effect)?;
            let first = logical.len().checked_sub(usize::from(inputs))?;
            if logical[first..].iter().any(|value| value.copy == omitted) {
                return None;
            }
            logical.truncate(first);
            actual.truncate(actual.values().len().checked_sub(usize::from(inputs))?);
            for _ in 0..outputs {
                logical.push(Occurrence { value: next, copy: next });
                actual.push(next);
                next += 1;
            }
            // <effect and operands> -> <same effect and operands>
            output.push(inst.clone());
            original_peak = original_peak.max(logical.len());
            candidate_peak = candidate_peak.max(actual.values().len());
        }
        if closed {
            let original_cost = immediate::cost(version, &input[..=index]);
            let candidate_cost = immediate::cost(version, &output);
            return (stack_usage(&output)?.0 <= stack_usage(&input[..=index])?.0
                && candidate_peak <= original_peak
                && candidate_cost.0 <= original_cost.0
                && candidate_cost.1 <= original_cost.1
                && candidate_cost != original_cost)
                .then_some((index + 1, output));
        }
    }
    None
}

fn values_without(logical: &[Occurrence], omitted: usize) -> Vec<usize> {
    logical.iter().filter(|value| value.copy != omitted).map(|value| value.value).collect()
}

fn stack_peak(mut height: usize, instructions: &[Instruction]) -> Option<usize> {
    let mut peak = height;
    for inst in instructions {
        let (inputs, outputs) = verify::effect(&inst.kind)?;
        height = height.checked_sub(usize::from(inputs))?.checked_add(usize::from(outputs))?;
        peak = peak.max(height);
    }
    Some(peak)
}

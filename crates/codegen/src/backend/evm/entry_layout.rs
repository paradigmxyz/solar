//! Successor-aware preservation of live physical stack values.
//!
//! The last opcode feeding a branch can retain its other live values in the
//! order needed by the unique cyclic successor. This avoids rotating loop values
//! after the branch merely because a new result was appended at the stack top.
//! We compare the existing checked schedule with one checked alternative, scoring
//! preparation together with the remaining successor permutation through shared
//! physical peepholes. Both static bytes and gas must improve or remain equal.
//!
//! The fixed prefix, complete live-value set and operand pop order are identical
//! in both candidates. Missing successor sources or an inaccessible permutation
//! leave the first candidate selected. Selection is atomic; this helper never
//! materializes values or changes a MIR instruction or control-flow edge.

use super::{
    ir,
    scheduler::{Stack, StackError},
};
use solar_config::EvmVersion;

/// Prepares operands while considering the next block's canonical live-value order.
pub(crate) fn prepare<T: Copy + Eq>(
    stack: &mut Stack<T>,
    operands: &[T],
    preferred: &[T],
    prefix: usize,
    version: EvmVersion,
    opcode: u8,
    live: impl Fn(T) -> bool,
) -> Result<Vec<ir::Instruction>, StackError> {
    let mut original = stack.clone();
    let original_ops = original.prepare(operands, prefix, version, &live)?;
    let original_values = original.values();
    let retained_end = original_values.len() - operands.len();
    let retained = &original_values[..retained_end];
    let mut desired = retained[..prefix].to_vec();
    for &value in preferred.iter().chain(&retained[prefix..]) {
        if retained[prefix..].contains(&value) && !desired[prefix..].contains(&value) {
            desired.push(value);
        }
    }
    // <fixed prefix>; <preferred live values>; <operands in reverse pop order>
    desired.extend(operands.iter().rev().copied());
    let mut alternative = stack.clone();
    if desired != original_values
        && let Ok(alternative_ops) = alternative.reconcile(&desired, prefix, version)
        && let Some(old_cost) =
            score(&original, &original_ops, operands.len(), preferred, prefix, version, opcode)
        && let Some(new_cost) = score(
            &alternative,
            &alternative_ops,
            operands.len(),
            preferred,
            prefix,
            version,
            opcode,
        )
        && new_cost.0 <= old_cost.0
        && new_cost.1 <= old_cost.1
        && new_cost != old_cost
    {
        *stack = alternative;
        Ok(alternative_ops)
    } else {
        *stack = original;
        Ok(original_ops)
    }
}

fn score<T: Copy + Eq>(
    prepared: &Stack<T>,
    preparation: &[ir::Instruction],
    operands: usize,
    preferred: &[T],
    prefix: usize,
    version: EvmVersion,
    opcode: u8,
) -> Option<(usize, usize)> {
    let mut body = preparation.to_vec();
    // <prepared operands>; opcode; <branch consumes result>
    body.push(ir::InstKind::Op(opcode).into());
    let body_cost = ir::scheduling_cost(version, &body);
    let mut remaining = prepared.clone();
    remaining.truncate(remaining.values().len().checked_sub(operands)?);
    let edge = remaining.reconcile(preferred, prefix, version).ok()?;
    let edge_cost = ir::scheduling_cost(version, &edge);
    Some((body_cost.0 + edge_cost.0, body_cost.1 + edge_cost.1))
}

//! Reuses one repeated negated literal within an already scheduled block.
//!
//! After compact-pushes has selected concrete encodings, a Gas-only caller may
//! retain one value from adjacent PUSH/NOT pairs with at least three occurrences.
//! Actual paid transport decides admission; gross bytes select one numeric value,
//! breaking ties by its first occurrence; failed trials do not try another value.
//! No MIR identities, materialization search, CFG changes or memory homes are used.
//!
//! The original block's minimum input requirement defines its active stack suffix.
//! At the first pair, descending SWAPs park the literal below that suffix and DUP
//! supplies the original operand. Ordinary instructions then see exactly their
//! original operands above the cache. Middle pairs become DUPs; ascending SWAPs
//! consume the cache at its last use and restore the exact original exit stack.
//! An arbitrary deeper prefix is never inspected or moved. All new accesses must
//! fit the legacy sixteen-word reach; opaque effects, glue and raw control refuse.
//!
//! Admission compares complete physical bodies: required inputs and net height
//! must match, encoded bytes must decrease, and static gas cannot increase. A larger
//! relative peak requires the caller's existing absolute entry bound; without it,
//! the old peak cannot increase. The caller also supplies its existing module-wide
//! code/gas-observer and computed-entry permission. Unchanged operations retain
//! their order and operands, including memory accesses; only literal transport
//! changes. Planning and checks finish before the instruction vector is replaced.

use super::{canonical, immediate, inherit_debug, stack_usage, verify};
use crate::backend::evm::{
    ir::{DebugMetadata, InstKind, Instruction},
    op,
};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use std::cmp::Reverse;

pub(super) fn reuse(
    insts: &mut Vec<Instruction>,
    version: EvmVersion,
    entry_max: Option<usize>,
    debug_info_tracked: bool,
) -> bool {
    let Some((value, count)) = select(insts, version) else { return false };
    if insts.iter().any(|inst| {
        !canonical(inst)
            || inst.stack_effect.is_some()
            || matches!(
                inst.kind,
                InstKind::PushLabel(_)
                    | InstKind::PushData { .. }
                    | InstKind::PushDeferred(_)
                    | InstKind::PushImmutable { .. }
                    | InstKind::Op(
                        op::STOP | op::RETURN | op::REVERT | op::INVALID | op::SELFDESTRUCT
                    )
            )
    }) {
        return false;
    }
    let Some(old) = stack_usage(insts) else { return false };
    let Some(candidate) = plan(insts, value, count, old.0, debug_info_tracked) else {
        return false;
    };
    let Some(new) = stack_usage(&candidate) else { return false };
    let capacity = new.2 <= old.2
        || entry_max.is_some_and(|entry| {
            entry.checked_add(new.2 as usize).is_some_and(|peak| peak <= 1024)
        });
    let old_cost = immediate::cost(version, insts);
    let new_cost = immediate::cost(version, &candidate);
    if new.0 != old.0
        || new.1 != old.1
        || !capacity
        || new_cost.0 >= old_cost.0
        || new_cost.1 > old_cost.1
    {
        return false;
    }
    // <original entry>; <cached literal transport>; <exact original exit>
    *insts = candidate;
    true
}

/// Selects by gross removed bytes before paying the actual transport cost.
fn select(insts: &[Instruction], version: EvmVersion) -> Option<(U256, usize)> {
    let mut counts = FxHashMap::<U256, (usize, usize)>::default();
    for (index, pair) in insts.windows(2).enumerate() {
        if let Some(value) = literal(pair) {
            let entry = counts.entry(value).or_insert((0, index));
            entry.0 += 1;
        }
    }
    counts
        .into_iter()
        .filter(|(_, (count, _))| *count >= 3)
        .max_by_key(|(value, (count, first))| {
            ((count - 1) * op::push_len(version, *value), Reverse(*first))
        })
        .map(|(value, (count, _))| (value, count))
}

fn literal(pair: &[Instruction]) -> Option<U256> {
    let [push, not] = pair else { return None };
    if let InstKind::Push(value) = push.kind
        && not.kind == InstKind::Op(op::NOT)
    {
        Some(value)
    } else {
        None
    }
}

fn plan(
    insts: &[Instruction],
    value: U256,
    count: usize,
    mut height: i64,
    debug_info_tracked: bool,
) -> Option<Vec<Instruction>> {
    let mut output = Vec::with_capacity(insts.len());
    let mut remaining = count;
    let mut cursor = 0;
    while cursor < insts.len() {
        if insts.get(cursor..cursor + 2).and_then(literal) == Some(value) {
            let depth = u16::try_from(height).ok()?;
            if depth > 16 || (remaining != 1 && depth == 16) {
                return None;
            }
            if remaining == count {
                // S; push value; not; swap k ... swap 1; dup (k + 1)
                // -> literal_cache; S; literal_operand
                output.extend_from_slice(&insts[cursor..cursor + 2]);
                for depth in (1..=depth).rev() {
                    output.push(helper(InstKind::Swap(depth), debug_info_tracked));
                }
                output.push(helper(InstKind::Dup(depth + 1), debug_info_tracked));
            } else if remaining == 1 {
                // literal_cache; S; swap 1 ... swap k -> S; literal_operand
                let release = output.len();
                for depth in 1..=depth {
                    output.push(helper(InstKind::Swap(depth), debug_info_tracked));
                }
                // NOTE: Release helpers have no source span. A nonempty release keeps
                // its first invocation at a real opcode; an empty release drops it.
                // An exit on the removed NOT is not a transfer and is not moved.
                if let Some(first) = output.get_mut(release)
                    && let Some(invoke) =
                        insts[cursor].debug.as_deref().and_then(|debug| debug.function_invoke)
                {
                    first.debug = Some(Box::new(DebugMetadata {
                        function_invoke: Some(invoke),
                        ..Default::default()
                    }));
                }
            } else {
                // literal_cache; S; dup (k + 1) -> literal_cache; S; literal_operand
                let mut duplicate = InstKind::Dup(depth + 1).into();
                inherit_debug(&insts[cursor..cursor + 2], std::slice::from_mut(&mut duplicate));
                output.push(duplicate);
            }
            remaining -= 1;
            height += 1;
            cursor += 2;
        } else {
            let (inputs, outputs) = verify::effect(&insts[cursor].kind)?;
            height = height.checked_sub(i64::from(inputs))?.checked_add(i64::from(outputs))?;
            // <same original operands>; <same original instruction>
            output.push(insts[cursor].clone());
            cursor += 1;
        }
    }
    Some(output)
}

/// Marks generated transport as deliberately source-unknown when debug is tracked.
fn helper(kind: InstKind, debug_info_tracked: bool) -> Instruction {
    // <generated DUP or SWAP>
    let mut inst = Instruction::from(kind);
    if debug_info_tracked {
        inst.debug = Some(Box::new(DebugMetadata { dropped: true, ..Default::default() }));
    }
    inst
}

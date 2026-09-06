//! Retains words across adjacent fixed-home restoration and backup runs.
//!
//! A run stores incoming words into distinct constant addresses, then reloads them in reverse
//! address order. Duplicating each original word for its store leaves the final stack unchanged
//! without the reloads. Every store keeps its exact value, address and execution order; removed
//! loads cannot expand memory because their complete words were just stored. The matcher also
//! accepts the innermost store/load pair already folded into DUP1 followed by a store.
//!
//! Windows contain at most sixteen words, canonical metadata and nonoverlapping word addresses.
//! The additional transient stack word must fit a known entry bound or the original block peak.
//! Both encoded bytes and static gas must decrease. The caller supplies the module-wide permission
//! excluding code/gas observations and unproved computed entries. This is a block peephole, with
//! no change to frame ownership, memory-alias analysis or the order of mutable effects.

use super::{super::immediate, canonical, stack_usage};
use crate::backend::evm::{
    ir::{InstKind, Instruction},
    op,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::EvmVersion;

pub(super) fn replacement(
    insts: &[Instruction],
    start: usize,
    version: EvmVersion,
    entry_max: Option<usize>,
) -> Option<(usize, Vec<Instruction>)> {
    let tail = &insts[start..];
    let first = tail.get(..2)?;
    let (InstKind::Push(address), InstKind::Op(op::MSTORE)) = (&first[0].kind, &first[1].kind)
    else {
        return None;
    };
    if !first.iter().all(canonical) {
        return None;
    }
    let mut stores = SmallVec::<[(U256, &Instruction); 16]>::new();
    stores.push((*address, &first[0]));
    let mut cursor = 2;
    while stores.len() < 16
        && let Some(pair) = tail.get(cursor..cursor + 2)
        && pair.iter().all(canonical)
        && let (InstKind::Push(address), InstKind::Op(op::MSTORE)) = (&pair[0].kind, &pair[1].kind)
    {
        stores.push((*address, &pair[0]));
        cursor += 2;
    }
    let reloads = stores.len();
    if stores.len() < 16
        && let Some(center) = tail.get(cursor..cursor + 3)
        && center.iter().all(canonical)
        && let (InstKind::Dup(1), InstKind::Push(address), InstKind::Op(op::MSTORE)) =
            (&center[0].kind, &center[1].kind, &center[2].kind)
    {
        stores.push((*address, &center[1]));
        cursor += 3;
    }
    if stores.len() < 2 {
        return None;
    }
    for (index, &(address, _)) in stores.iter().enumerate() {
        let end = address.checked_add(U256::from(32))?;
        if stores[..index].iter().any(|(previous, _)| {
            *previous < end && previous.checked_add(U256::from(32)).is_none_or(|end| address < end)
        }) {
            return None;
        }
    }
    for (address, _) in stores[..reloads].iter().rev() {
        let pair = tail.get(cursor..cursor + 2)?;
        if !pair.iter().all(canonical)
            || pair[0].kind != InstKind::Push(*address)
            || pair[1].kind != InstKind::Op(op::MLOAD)
        {
            return None;
        }
        cursor += 2;
    }
    // <original words>; dup1; push homeN; mstore; dup2; push homeN-1; mstore; ...
    let mut candidate = Vec::with_capacity(stores.len() * 3);
    for (index, (_, address)) in stores.into_iter().enumerate() {
        candidate.extend([
            InstKind::Dup(index as u16 + 1).into(),
            address.clone(),
            InstKind::Op(op::MSTORE).into(),
        ]);
    }
    let old = stack_usage(&tail[..cursor])?;
    let new = stack_usage(&candidate)?;
    let (_, prefix, _) = stack_usage(&insts[..start])?;
    let capacity = entry_max.is_some_and(|entry| entry as i64 + prefix + new.2 <= 1024)
        || stack_usage(insts).is_some_and(|(_, _, peak)| prefix + new.2 <= peak);
    let old_cost = immediate::cost(version, &tail[..cursor]);
    let new_cost = immediate::cost(version, &candidate);
    (new.0 <= old.0
        && new.1 == old.1
        && capacity
        && new_cost.0 < old_cost.0
        && new_cost.1 < old_cost.1)
        .then_some((cursor, candidate))
}

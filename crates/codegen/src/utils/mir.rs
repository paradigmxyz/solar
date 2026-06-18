//! Shared MIR utility helpers.

use crate::mir::{
    BlockId, Function, InstId, InstKind, MemoryRegion, StorageAlias, Terminator, Value, ValueId,
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Which state-access instructions should receive storage-alias metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageAliasScope {
    /// Annotate persistent storage accesses only.
    Storage,
    /// Annotate persistent and transient storage accesses.
    StorageAndTransient,
}

/// Returns a map from each instruction to its result value.
pub(crate) fn inst_results(func: &Function) -> FxHashMap<InstId, ValueId> {
    let mut results =
        FxHashMap::with_capacity_and_hasher(func.instructions.len(), Default::default());
    for (value_id, value) in func.values.iter_enumerated() {
        if let Value::Inst(inst_id) = value {
            results.insert(*inst_id, value_id);
        }
    }
    results
}

/// Returns a map from each instruction to the block containing it.
pub(crate) fn inst_blocks(func: &Function) -> FxHashMap<InstId, BlockId> {
    let mut inst_blocks =
        FxHashMap::with_capacity_and_hasher(func.instructions.len(), Default::default());
    for (block_id, block) in func.blocks.iter_enumerated() {
        for &inst_id in &block.instructions {
            inst_blocks.insert(inst_id, block_id);
        }
    }
    inst_blocks
}

/// Returns predecessors with duplicate CFG edges collapsed.
pub(crate) fn unique_predecessors(func: &Function, block: BlockId) -> Vec<BlockId> {
    let mut predecessors = Vec::new();
    for &pred in &func.blocks[block].predecessors {
        if !predecessors.contains(&pred) {
            predecessors.push(pred);
        }
    }
    predecessors
}

/// Returns true if the block contains any phi instruction.
pub(crate) fn block_has_phi(func: &Function, block: BlockId) -> bool {
    func.blocks[block]
        .instructions
        .iter()
        .any(|&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
}

/// Returns true if every instruction in the block is a phi instruction.
pub(crate) fn block_has_only_phis(func: &Function, block: BlockId) -> bool {
    func.blocks[block]
        .instructions
        .iter()
        .all(|&inst_id| matches!(func.instructions[inst_id].kind, InstKind::Phi(_)))
}

/// Returns the result values produced by phi instructions in the block.
pub(crate) fn block_phi_results(func: &Function, block: BlockId) -> FxHashSet<ValueId> {
    func.blocks[block]
        .instructions
        .iter()
        .filter_map(|&inst_id| {
            matches!(func.instructions[inst_id].kind, InstKind::Phi(_))
                .then(|| func.inst_result_value(inst_id))
                .flatten()
        })
        .collect()
}

/// Resolves a value through a replacement map until it reaches its canonical value.
pub(crate) fn resolve_replacement(
    mut value: ValueId,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> ValueId {
    while let Some(&replacement) = replacements.get(&value) {
        if replacement == value {
            break;
        }
        value = replacement;
    }
    value
}

/// Replaces all value uses according to a one-step replacement map.
pub(crate) fn replace_uses(func: &mut Function, replacements: &FxHashMap<ValueId, ValueId>) {
    replace_uses_with(func, replacements, |value, replacements| {
        replacements.get(&value).copied().unwrap_or(value)
    });
}

/// Replaces all value uses according to a canonicalized replacement map.
pub(crate) fn replace_uses_canonicalized(
    func: &mut Function,
    replacements: &FxHashMap<ValueId, ValueId>,
) {
    replace_uses_with(func, replacements, resolve_replacement);
}

/// Replaces instruction operands according to a one-step replacement map.
pub(crate) fn replace_inst_uses(
    kind: &mut InstKind,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> usize {
    replace_inst_operands(kind, replacements, |value, replacements| {
        replacements.get(&value).copied().unwrap_or(value)
    })
}

/// Replaces instruction operands according to a canonicalized replacement map.
pub(crate) fn replace_inst_uses_canonicalized(
    kind: &mut InstKind,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> usize {
    replace_inst_operands(kind, replacements, resolve_replacement)
}

/// Replaces terminator operands according to a one-step replacement map.
pub(crate) fn replace_terminator_uses(
    term: &mut Terminator,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> usize {
    replace_terminator_operands(term, replacements, |value, replacements| {
        replacements.get(&value).copied().unwrap_or(value)
    })
}

/// Replaces terminator operands according to a canonicalized replacement map.
pub(crate) fn replace_terminator_uses_canonicalized(
    term: &mut Terminator,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> usize {
    replace_terminator_operands(term, replacements, resolve_replacement)
}

/// Annotates storage-alias metadata for state-access instructions.
pub(crate) fn annotate_storage_aliases(func: &mut Function, scope: StorageAliasScope) {
    let inst_ids: Vec<_> =
        func.instructions.iter_enumerated().map(|(inst_id, _)| inst_id).collect();
    for inst_id in inst_ids {
        let slot = match func.instructions[inst_id].kind {
            InstKind::SLoad(slot) | InstKind::SStore(slot, _) => Some(slot),
            InstKind::TLoad(slot) | InstKind::TStore(slot, _)
                if scope == StorageAliasScope::StorageAndTransient =>
            {
                Some(slot)
            }
            _ => None,
        };
        let alias = slot.map(|slot| StorageAlias::for_value(func, slot));
        func.instructions[inst_id].metadata.set_storage_alias(alias);
    }
}

/// Returns stored storage-alias metadata, or computes a conservative alias key.
pub(crate) fn storage_alias(func: &Function, inst_id: InstId, slot: ValueId) -> StorageAlias {
    func.instructions[inst_id]
        .metadata
        .storage_alias()
        .unwrap_or_else(|| StorageAlias::for_value(func, slot))
}

/// Returns storage-alias metadata after applying value replacements.
pub(crate) fn storage_alias_after_replacements(
    func: &Function,
    inst_id: InstId,
    slot: ValueId,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> StorageAlias {
    let original_slot = slot;
    let slot = resolve_replacement(slot, replacements);
    if slot == original_slot {
        storage_alias(func, inst_id, slot)
    } else {
        StorageAlias::for_value(func, slot)
    }
}

/// Converts a U256 to a u64 when lossless.
pub(crate) fn u256_to_u64(value: U256) -> Option<u64> {
    value.try_into().ok()
}

/// Returns an immediate value as U256.
pub(crate) fn value_u256(func: &Function, value: ValueId) -> Option<U256> {
    let Value::Immediate(imm) = func.value(value) else { return None };
    imm.as_u256()
}

/// Returns an immediate value as u64 when lossless.
pub(crate) fn value_u64(func: &Function, value: ValueId) -> Option<u64> {
    value_u256(func, value).and_then(u256_to_u64)
}

/// Returns a possibly replaced immediate value as U256.
pub(crate) fn value_u256_after_replacements(
    func: &Function,
    value: ValueId,
    replacements: &FxHashMap<ValueId, ValueId>,
) -> Option<U256> {
    value_u256(func, resolve_replacement(value, replacements))
}

/// Returns true if two possibly-overflowing byte ranges overlap.
pub(crate) fn ranges_overlap(a_start: u64, a_size: u64, b_start: u64, b_size: u64) -> bool {
    let Some(a_end) = a_start.checked_add(a_size) else {
        return true;
    };
    let Some(b_end) = b_start.checked_add(b_size) else {
        return true;
    };
    a_start < b_end && b_start < a_end
}

/// Returns the statically known memory region for an address value.
pub(crate) fn memory_region_for_addr(func: &Function, addr: ValueId) -> MemoryRegion {
    match func.value(addr) {
        Value::Immediate(imm) if imm.as_u256().is_some_and(|value| value < U256::from(0x80)) => {
            MemoryRegion::Scratch
        }
        _ => MemoryRegion::Unknown,
    }
}

/// Returns true for instructions whose operands derive memory metadata.
pub(crate) fn is_memory_inst(kind: &InstKind) -> bool {
    matches!(
        kind,
        InstKind::MLoad(_)
            | InstKind::MStore(_, _)
            | InstKind::MStore8(_, _)
            | InstKind::MCopy(_, _, _)
            | InstKind::CalldataCopy(_, _, _)
            | InstKind::CodeCopy(_, _, _)
            | InstKind::ReturnDataCopy(_, _, _)
            | InstKind::ExtCodeCopy(_, _, _, _)
            | InstKind::Keccak256(_, _)
    )
}

fn replace_uses_with(
    func: &mut Function,
    replacements: &FxHashMap<ValueId, ValueId>,
    replacement: impl Fn(ValueId, &FxHashMap<ValueId, ValueId>) -> ValueId,
) {
    if replacements.is_empty() {
        return;
    }

    for inst in func.instructions.iter_mut() {
        replace_inst_operands(&mut inst.kind, replacements, &replacement);
    }
    for block in func.blocks.iter_mut() {
        if let Some(term) = &mut block.terminator {
            replace_terminator_operands(term, replacements, &replacement);
        }
    }
}

fn replace_inst_operands(
    kind: &mut InstKind,
    replacements: &FxHashMap<ValueId, ValueId>,
    replacement: impl Fn(ValueId, &FxHashMap<ValueId, ValueId>) -> ValueId,
) -> usize {
    let mut replaced = 0;
    kind.visit_operands_mut(|value| {
        let new_value = replacement(*value, replacements);
        if new_value != *value {
            *value = new_value;
            replaced += 1;
        }
    });
    replaced
}

fn replace_terminator_operands(
    term: &mut Terminator,
    replacements: &FxHashMap<ValueId, ValueId>,
    replacement: impl Fn(ValueId, &FxHashMap<ValueId, ValueId>) -> ValueId,
) -> usize {
    let mut replaced = 0;
    let mut replace = |value: &mut ValueId| {
        let new_value = replacement(*value, replacements);
        if new_value != *value {
            *value = new_value;
            replaced += 1;
        }
    };

    match term {
        Terminator::Jump(_) | Terminator::Stop | Terminator::Invalid => {}
        Terminator::Branch { condition, .. } => replace(condition),
        Terminator::Switch { value, cases, .. } => {
            replace(value);
            for (case_value, _) in cases {
                replace(case_value);
            }
        }
        Terminator::Return { values } => {
            for value in values {
                replace(value);
            }
        }
        Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
            replace(offset);
            replace(size);
        }
        Terminator::SelfDestruct { recipient } => replace(recipient),
    }
    replaced
}

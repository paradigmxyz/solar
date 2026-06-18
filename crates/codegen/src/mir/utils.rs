//! Shared MIR utility helpers.

use crate::mir::{
    BasicBlock, BlockId, Function, InstId, InstKind, MemoryRegion, StorageAlias, Terminator, Value,
    ValueId,
};
use alloy_primitives::U256;
use smallvec::smallvec;
use solar_data_structures::map::FxHashMap;

/// Which state-access instructions should receive storage-alias metadata.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum StorageAliasScope {
    /// Annotate persistent storage accesses only.
    Storage,
    /// Annotate persistent and transient storage accesses.
    StorageAndTransient,
}

/// Splits the CFG edge from `pred` to `succ` by inserting a fresh block that
/// contains no instructions and jumps straight to `succ`. Returns the new
/// block.
///
/// A terminator that targets `succ` several times — a branch with both arms to
/// `succ`, or a switch with several cases plus the default — is one logical
/// edge: every occurrence is retargeted to the same new block. Phi incoming
/// lists are keyed per predecessor block, so two distinct split blocks for one
/// predecessor would force conflicting phi entries.
///
/// Self-loops (`pred == succ`) are supported: the new block takes over the
/// backedge and `succ`'s phis are rekeyed from `pred` to the new block.
///
/// `succ` cannot be the entry block, which has no predecessors by definition.
pub(crate) fn split_edge(func: &mut Function, pred: BlockId, succ: BlockId) -> BlockId {
    debug_assert_ne!(succ, func.entry_block, "the entry block has no predecessor edges to split");

    let new_block = func.blocks.push(BasicBlock {
        instructions: Vec::new(),
        terminator: Some(Terminator::Jump(succ)),
        predecessors: smallvec![pred],
    });

    // Retarget every occurrence of `succ` among `pred`'s successors.
    let mut retargeted = false;
    let mut retarget = |block: &mut BlockId| {
        if *block == succ {
            *block = new_block;
            retargeted = true;
        }
    };
    match func.blocks[pred].terminator.as_mut().expect("predecessor must have a terminator") {
        Terminator::Jump(target) => retarget(target),
        Terminator::Branch { then_block, else_block, .. } => {
            retarget(then_block);
            retarget(else_block);
        }
        Terminator::Switch { default, cases, .. } => {
            retarget(default);
            for (_, case_block) in cases {
                retarget(case_block);
            }
        }
        term => panic!("terminator `{}` has no successor edges to split", term.mnemonic()),
    }
    assert!(retargeted, "bb{} does not branch to bb{}", pred.index(), succ.index());

    // Replace `pred` with the new block in `succ`'s predecessor list. A
    // multi-occurrence edge may have been recorded once per occurrence;
    // they all collapse into the single split block.
    let predecessors = &mut func.blocks[succ].predecessors;
    let first = predecessors
        .iter()
        .position(|&block| block == pred)
        .expect("successor must list pred as a predecessor");
    predecessors[first] = new_block;
    predecessors.retain(|&mut block| block != pred);

    // Rekey `succ`'s phi incoming entries from `pred` to the new block.
    for &inst_id in &func.blocks[succ].instructions {
        if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
            for (block, _) in incoming.iter_mut() {
                if *block == pred {
                    *block = new_block;
                }
            }
        }
    }

    new_block
}

/// Rebuilds CFG edge lists from terminators and drops phi inputs from blocks
/// that are no longer predecessors. Returns true if any phi input was dropped.
pub(crate) fn repair_reachability_phis(func: &mut Function) -> bool {
    let mut edges = Vec::new();
    for (block, bb) in func.blocks.iter_enumerated() {
        if let Some(term) = &bb.terminator {
            edges.push((block, term.successors()));
        }
    }

    for block in func.blocks.iter_mut() {
        block.predecessors.clear();
    }

    for (block, successors) in edges {
        for succ in successors {
            func.blocks[succ].predecessors.push(block);
        }
    }

    let mut changed = false;
    for block_id in func.blocks.indices() {
        let predecessors = func.blocks[block_id].predecessors.clone();
        for &inst_id in &func.blocks[block_id].instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
                let len_before = incoming.len();
                incoming.retain(|(pred, _)| predecessors.contains(pred));
                changed |= incoming.len() != len_before;
            }
        }
    }
    changed
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

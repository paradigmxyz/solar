//! Shared MIR utility helpers.

use crate::mir::{BasicBlock, BlockId, Function, InstKind, Terminator, ValueId};
use alloy_primitives::U256;
use smallvec::smallvec;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

pub(crate) fn remap_block_order(
    func: &mut Function,
    order: &[BlockId],
) -> IndexVec<BlockId, BlockId> {
    debug_assert_eq!(order.len(), func.blocks.len());
    let remap = remap_blocks(func, order);
    debug_assert!(!remap.contains(&BlockId::MAX));
    remap
}

pub(crate) fn retain_blocks(func: &mut Function, order: &[BlockId]) {
    debug_assert!(order.len() <= func.blocks.len());
    remap_blocks(func, order);
}

fn remap_blocks(func: &mut Function, order: &[BlockId]) -> IndexVec<BlockId, BlockId> {
    let mut remap = index_vec![BlockId::MAX; func.blocks.len()];
    let mut old_blocks =
        std::mem::take(&mut func.blocks).into_iter().map(Some).collect::<IndexVec<BlockId, _>>();
    let mut blocks = IndexVec::with_capacity(order.len());
    for &old_block in order {
        let block = old_blocks[old_block].take().expect("duplicate block in order");
        let new_block = blocks.push(block);
        remap[old_block] = new_block;
    }
    func.blocks = blocks;

    let mut retained_instructions = Vec::new();
    for block in &mut func.blocks {
        block.predecessors.retain(|predecessor| remap[*predecessor] != BlockId::MAX);
        for predecessor in &mut block.predecessors {
            *predecessor = remap[*predecessor];
        }
        if let Some(terminator) = &mut block.terminator {
            remap_terminator_blocks(terminator, &remap);
        }
        retained_instructions.extend_from_slice(&block.instructions);
    }
    for inst_id in retained_instructions {
        let inst = &mut func.instructions[inst_id];
        if let InstKind::Phi(incoming) = &mut inst.kind {
            incoming.retain(|(block, _)| remap[*block] != BlockId::MAX);
            for (block, _) in incoming {
                *block = remap[*block];
            }
        }
    }
    remap
}

fn remap_terminator_blocks(terminator: &mut Terminator, remap: &IndexVec<BlockId, BlockId>) {
    let remap_block = |block: &mut BlockId| {
        let remapped = remap[*block];
        assert_ne!(remapped, BlockId::MAX, "terminator target must be retained");
        *block = remapped;
    };
    match terminator {
        Terminator::Jump(target) => remap_block(target),
        Terminator::Branch { then_block, else_block, .. } => {
            remap_block(then_block);
            remap_block(else_block);
        }
        Terminator::Switch { default, cases, .. } => {
            remap_block(default);
            for (_, target) in cases {
                remap_block(target);
            }
        }
        Terminator::Return { .. }
        | Terminator::Revert { .. }
        | Terminator::ReturnData { .. }
        | Terminator::Stop
        | Terminator::SelfDestruct { .. }
        | Terminator::TailCall { .. }
        | Terminator::Invalid => {}
    }
}

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
pub(crate) fn split_edge(func: &mut Function, pred: BlockId, succ: BlockId) -> BlockId {
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
#[must_use]
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
    for block in &mut func.blocks {
        for &inst_id in &block.instructions {
            if let InstKind::Phi(incoming) = &mut func.instructions[inst_id].kind {
                let len_before = incoming.len();
                incoming.retain(|(pred, _)| block.predecessors.contains(pred));
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

/// Converts a U256 to a u64 when lossless.
pub(crate) fn u256_to_u64(value: U256) -> Option<u64> {
    value.try_into().ok()
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
            | InstKind::MappingSlotMemory(_, _)
    )
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
        Terminator::TailCall { args, .. } => {
            for arg in args {
                replace(arg);
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

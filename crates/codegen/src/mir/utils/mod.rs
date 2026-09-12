//! Shared MIR utility helpers.
//!
//! CFG rewrites must maintain terminators, predecessor lists, and phi inputs together. Prefer the
//! local edge helpers below. New edges need explicit phi inputs; none of these helpers updates
//! cached analyses or reconstructs SSA after moving definitions.

use crate::mir::{BasicBlock, BlockId, Function, InstKind, Terminator, ValueId};
use alloy_primitives::U256;
use smallvec::smallvec;
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
};

pub(crate) mod eval;
mod gas;
pub(crate) use gas::{pre_tangerine_call_gas, precompile_gas};

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
        let inst = func.inst_mut(inst_id);
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
        | Terminator::RevertReturndata
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
    // pred -> new_block -> succ !metadata(pred terminator)
    let new_block = func.blocks.push(BasicBlock {
        instructions: Vec::new(),
        terminator: Some(Terminator::Jump(succ)),
        terminator_metadata: func.blocks[pred].terminator_metadata.clone(),
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
    let instruction_count = func.blocks[succ].instructions.len();
    for index in 0..instruction_count {
        let inst_id = func.blocks[succ].instructions[index];
        if let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind {
            for (block, _) in incoming.iter_mut() {
                if *block == pred {
                    *block = new_block;
                }
            }
        }
    }

    new_block
}

/// Removes a logical edge's predecessor entry and phi inputs after its last occurrence is gone.
fn remove_predecessor(func: &mut Function, block: BlockId, predecessor: BlockId) {
    func.blocks[block].predecessors.retain(|pred| *pred != predecessor);
    // phi [..., predecessor: value, ...] -> phi [...]
    let count = func.blocks[block].instructions.len();
    for index in 0..count {
        let id = func.blocks[block].instructions[index];
        if let InstKind::Phi(incoming) = &mut func.inst_mut(id).kind {
            incoming.retain(|&(pred, _)| pred != predecessor);
        }
    }
}

/// Replaces a terminator while maintaining the affected predecessor lists and phi inputs.
///
/// Before adding an edge to a block with phis, the caller must supply that predecessor's phi
/// inputs. Removed edges lose their inputs; retained edges keep theirs, including duplicate
/// branch arms and switch cases. Debug metadata is unchanged; cached CFG analyses are not updated.
pub(crate) fn replace_terminator(func: &mut Function, block: BlockId, terminator: Terminator) {
    let mut old =
        func.blocks[block].terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    let mut new = terminator.successors();
    old.sort_unstable();
    old.dedup();
    new.sort_unstable();
    new.dedup();
    for &successor in &new {
        if !old.contains(&successor) {
            for &id in &func.blocks[successor].instructions {
                if let InstKind::Phi(incoming) = &func.inst(id).kind {
                    assert!(
                        incoming.iter().any(|&(pred, _)| pred == block),
                        "new CFG edges require explicit phi inputs"
                    );
                }
            }
        }
    }
    // block: old_terminator -> terminator !metadata(old terminator)
    func.blocks[block].terminator = Some(terminator);
    for successor in old {
        if !new.contains(&successor) {
            remove_predecessor(func, successor, block);
        }
    }
    for successor in new {
        let predecessors = &mut func.blocks[successor].predecessors;
        let mut seen = false;
        predecessors.retain(|pred| *pred != block || !std::mem::replace(&mut seen, true));
        if !seen {
            predecessors.push(block);
        }
    }
}

/// Folds a terminator to one of its existing successors and updates affected CFG links and phis.
///
/// The kept edge retains its phi values even when several branch arms or switch cases target it.
/// This cannot introduce an edge: callers that redirect control must supply the new phi values.
/// Terminator debug metadata is preserved. Cached CFG analyses must still be invalidated.
pub(crate) fn fold_terminator_to_jump(func: &mut Function, block: BlockId, target: BlockId) {
    assert!(
        func.blocks[block].terminator.as_ref().is_some_and(|term| term.has_successor(target)),
        "folded target must be an existing successor"
    );
    // branch/switch ..., target, ... -> jump target
    replace_terminator(func, block, Terminator::Jump(target));
}

/// Clears a block proven unreachable and removes its outgoing CFG links and phi inputs.
///
/// Incoming edges remain until their source terminators change, including edges from other dead
/// blocks. Keeping those backlinks makes this operation independent of block deletion order.
/// Cached CFG analyses must still be invalidated. Returns whether the block changed.
#[must_use]
pub(crate) fn invalidate_unreachable_block(func: &mut Function, block: BlockId) -> bool {
    if func.blocks[block].instructions.is_empty()
        && matches!(func.blocks[block].terminator, Some(Terminator::Invalid))
    {
        return false;
    }
    let mut successors =
        func.blocks[block].terminator.as_ref().map(Terminator::successors).unwrap_or_default();
    successors.sort_unstable();
    successors.dedup();
    // unreachable block -> invalid
    func.blocks[block].instructions.clear();
    func.blocks[block].terminator = Some(Terminator::Invalid);
    for successor in successors {
        remove_predecessor(func, successor, block);
    }
    true
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
            | InstKind::MemoryZero(_, _)
            | InstKind::MCopy(_, _, _)
            | InstKind::CalldataCopy(_, _, _)
            | InstKind::DataCopy(_, _, _)
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
        Terminator::Jump(_)
        | Terminator::RevertReturndata
        | Terminator::Stop
        | Terminator::Invalid => {}
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

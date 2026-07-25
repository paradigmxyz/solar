//! Phi elimination for MIR.
//!
//! Converts SSA phi nodes into parallel copies inserted at predecessor block exits.
//! This is necessary because the EVM cannot directly execute phi nodes.
//!
//! The algorithm:
//! 1. For each phi node in block B with incoming value V from predecessor P, insert a copy from V
//!    to the phi's destination at the end of P.
//! 2. Handle cycles by detecting when copies form a cycle and using a temporary.
//! 3. Remove the phi instructions after copies are inserted.

use crate::mir::{BlockId, Function, InstKind, MirType, ValueId};
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::collections::BTreeSet;

/// Source for a parallel copy - either a regular value or a temporary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CopySource {
    /// A regular MIR value.
    Value(ValueId),
    /// A temporary created during cycle breaking (identified by index).
    Temp(u32),
}

/// Destination for a parallel copy - either a regular value or a temporary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum CopyDest {
    /// A regular MIR value.
    Value(ValueId),
    /// A temporary created during cycle breaking (identified by index).
    Temp(u32),
}

/// A parallel copy operation: copy from source to destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ParallelCopy {
    /// Source value to copy from.
    pub src: CopySource,
    /// Destination value (the phi result).
    pub dst: CopyDest,
    /// Type of the copy.
    pub ty: MirType,
}

/// Copies to insert at the end of a block (before the terminator).
#[derive(Clone, Debug, Default)]
pub(crate) struct BlockCopies {
    /// Parallel copies to execute at this block's exit.
    pub copies: Vec<ParallelCopy>,
}

/// Result of phi elimination.
#[derive(Debug)]
pub(crate) struct PhiEliminationResult {
    /// Copies to insert at each predecessor block.
    pub block_copies: FxHashMap<BlockId, BlockCopies>,
    /// Phi instructions to remove (block, instruction index).
    pub phis_to_remove: Vec<(BlockId, usize)>,
}

/// Phi elimination query.
pub(crate) struct PhiEliminator;

impl PhiEliminator {
    /// Analyzes phi nodes and returns parallel copies to insert at predecessor exits.
    ///
    /// The caller is responsible for modifying the function.
    #[must_use]
    pub(crate) fn analyze(func: &Function) -> PhiEliminationResult {
        let mut block_copies: FxHashMap<BlockId, BlockCopies> = FxHashMap::default();
        let mut phis_to_remove = Vec::new();

        // Process each block looking for phi instructions
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                let inst = func.inst(inst_id);

                if let InstKind::Phi(incoming) = &inst.kind
                    && let Some(dst) = func.inst_result_value(inst_id)
                {
                    let ty = func.value(dst).ty();

                    // For each predecessor, insert a copy
                    for &(pred_block, src_val) in incoming {
                        block_copies.entry(pred_block).or_default().copies.push(ParallelCopy {
                            src: CopySource::Value(src_val),
                            dst: CopyDest::Value(dst),
                            ty,
                        });
                    }

                    phis_to_remove.push((block_id, inst_idx));
                }
            }
        }

        // Sequentialize parallel copies to handle cycles
        let mut temp_counter = 0u32;
        for copies in block_copies.values_mut() {
            sequentialize_copies(&mut copies.copies, &mut temp_counter);
        }

        PhiEliminationResult { block_copies, phis_to_remove }
    }
}

/// Helper to extract ValueId from CopySource if it's a value (not a temp).
fn src_value(src: &CopySource) -> Option<ValueId> {
    match src {
        CopySource::Value(v) => Some(*v),
        CopySource::Temp(_) => None,
    }
}

/// Helper to extract ValueId from CopyDest if it's a value (not a temp).
fn dst_value(dst: &CopyDest) -> Option<ValueId> {
    match dst {
        CopyDest::Value(v) => Some(*v),
        CopyDest::Temp(_) => None,
    }
}

/// Sequentializes parallel copies to handle dependencies and cycles.
///
/// A chain like: a = b, c = a needs to be ordered as: c = a, a = b
/// (read from a before writing to a)
///
/// A cycle like: a = b, b = a needs a temporary: tmp = a, a = b, b = tmp
///
/// Uses the algorithm from "Practical Improvements to the Construction and
/// Destruction of Static Single Assignment Form" by Briggs et al.
fn sequentialize_copies(copies: &mut Vec<ParallelCopy>, temp_counter: &mut u32) {
    if copies.len() <= 1 {
        return;
    }

    // Build the copy graph
    let pending: Vec<ParallelCopy> = std::mem::take(copies);
    let mut result: Vec<ParallelCopy> = Vec::with_capacity(pending.len() + 2);

    // Map from value to index of copy that writes to it
    let mut writes_to: FxHashMap<ValueId, usize> = FxHashMap::default();
    for (i, copy) in pending.iter().enumerate() {
        if let Some(dst) = dst_value(&copy.dst) {
            writes_to.insert(dst, i);
        }
    }

    // For each copy, count how many other copies need to read its destination
    // before we can safely write to it
    let mut blocked_by: Vec<usize> = vec![0; pending.len()];
    for (i, copy) in pending.iter().enumerate() {
        if let Some(src) = src_value(&copy.src)
            && let Some(&writer_idx) = writes_to.get(&src)
            && writer_idx != i
        {
            blocked_by[writer_idx] += 1;
        }
    }

    let mut emitted = DenseBitSet::new_empty(pending.len());
    let mut ready = blocked_by
        .iter()
        .enumerate()
        .filter_map(|(index, &blocked)| (blocked == 0).then_some(index))
        .collect::<BTreeSet<_>>();

    // Emit the acyclic portion in the same scan order as the simple iterative
    // algorithm, without revisiting blocked copies on each sweep.
    let mut cursor = 0;
    while !ready.is_empty() {
        let index = if let Some(&index) = ready.range(cursor..).next() {
            index
        } else {
            *ready.first().expect("ready is not empty")
        };
        ready.remove(&index);
        result.push(pending[index].clone());
        emitted.insert(index);
        cursor = index + 1;

        // Unblock the writer whose destination this copy just finished reading.
        if let Some(src) = src_value(&pending[index].src)
            && let Some(&blocked_writer) = writes_to.get(&src)
            && blocked_writer != index
            && !emitted.contains(blocked_writer)
        {
            blocked_by[blocked_writer] -= 1;
            if blocked_by[blocked_writer] == 0 {
                ready.insert(blocked_writer);
            }
        }
    }

    // Every remaining component is a cycle: each node is blocked, and every
    // copy has at most one dependency. Break each one at its first index.
    let mut cycle_members = DenseBitSet::new_empty(pending.len());
    let mut cycle_state = CycleState {
        emitted: &mut emitted,
        cycle_members: &mut cycle_members,
        blocked_by: &mut blocked_by,
    };
    for start_idx in 0..pending.len() {
        if !cycle_state.emitted.contains(start_idx) {
            break_cycle(
                start_idx,
                &pending,
                &mut cycle_state,
                &writes_to,
                &mut result,
                temp_counter,
            );
        }
    }

    *copies = result;
}

struct CycleState<'a> {
    emitted: &'a mut DenseBitSet<usize>,
    cycle_members: &'a mut DenseBitSet<usize>,
    blocked_by: &'a mut [usize],
}

/// Breaks cycles in the remaining copies by inserting a temporary.
///
/// For a cycle a -> b -> a, we:
/// 1. Pick one copy in the cycle (say b = a)
/// 2. Save its source to a temporary: tmp = a
/// 3. Emit all copies in the cycle normally: a = b
/// 4. Replace the broken copy's source with temp: b = tmp
fn break_cycle(
    start_idx: usize,
    pending: &[ParallelCopy],
    state: &mut CycleState<'_>,
    writes_to: &FxHashMap<ValueId, usize>,
    result: &mut Vec<ParallelCopy>,
    temp_counter: &mut u32,
) {
    let CycleState { emitted, cycle_members, blocked_by } = state;

    // Trace the cycle to find all participants
    let mut cycle_indices = vec![start_idx];
    cycle_members.insert(start_idx);
    let mut current = start_idx;

    while let Some(src) = src_value(&pending[current].src) {
        // Find the copy that writes to our source (the predecessor in the cycle)
        if let Some(&pred_idx) = writes_to.get(&src) {
            if emitted.contains(pred_idx) {
                break;
            }
            if pred_idx == start_idx {
                // We've completed the cycle
                break;
            }
            if !cycle_members.insert(pred_idx) {
                // Hit part of the cycle we've already seen
                break;
            }
            cycle_indices.push(pred_idx);
            current = pred_idx;
        } else {
            // Not a true cycle (shouldn't happen if blocked_by > 0)
            break;
        }
    }
    for &index in &cycle_indices {
        cycle_members.remove(index);
    }

    // Pick the first copy in the cycle to break
    let break_idx = cycle_indices[0];
    let break_copy = &pending[break_idx];

    // Allocate a temporary ID
    let temp_id = *temp_counter;
    *temp_counter += 1;

    // Step 1: Save the source to temporary
    result.push(ParallelCopy {
        src: break_copy.src.clone(),
        dst: CopyDest::Temp(temp_id),
        ty: break_copy.ty,
    });

    // Step 2: Emit all other copies in the cycle (they can now proceed)
    // The copy at break_idx is blocked, so unblock its writer
    if let Some(src) = src_value(&break_copy.src)
        && let Some(&blocked_writer) = writes_to.get(&src)
        && blocked_writer != break_idx
    {
        blocked_by[blocked_writer] = blocked_by[blocked_writer].saturating_sub(1);
    }

    // Emit copies that are now unblocked (in the cycle)
    for &idx in &cycle_indices[1..] {
        if !emitted.contains(idx) && blocked_by[idx] == 0 {
            result.push(pending[idx].clone());
            emitted.insert(idx);

            // Unblock the writer of our source
            if let Some(src) = src_value(&pending[idx].src)
                && let Some(&blocked_writer) = writes_to.get(&src)
                && !emitted.contains(blocked_writer)
            {
                blocked_by[blocked_writer] = blocked_by[blocked_writer].saturating_sub(1);
            }
        }
    }

    // Step 3: Emit the broken copy with temp as source
    result.push(ParallelCopy {
        src: CopySource::Temp(temp_id),
        dst: break_copy.dst.clone(),
        ty: break_copy.ty,
    });
    emitted.insert(break_idx);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copy(src: usize, dst: usize) -> ParallelCopy {
        ParallelCopy {
            src: CopySource::Value(ValueId::from_usize(src)),
            dst: CopyDest::Value(ValueId::from_usize(dst)),
            ty: MirType::uint256(),
        }
    }

    fn has_temp(copies: &[ParallelCopy]) -> bool {
        copies.iter().any(|c| matches!(c.src, CopySource::Temp(_)))
            || copies.iter().any(|c| matches!(c.dst, CopyDest::Temp(_)))
    }

    #[test]
    fn test_no_cycle() {
        let mut copies = vec![copy(0, 1), copy(2, 3)];
        let mut temp_counter = 0;
        sequentialize_copies(&mut copies, &mut temp_counter);
        assert_eq!(copies.len(), 2);
        assert!(!has_temp(&copies));
    }

    #[test]
    fn test_chain() {
        // a = b, c = a -> should read from 'a' before writing to 'a'
        let mut copies = vec![copy(1, 0), copy(0, 2)];
        let mut temp_counter = 0;
        sequentialize_copies(&mut copies, &mut temp_counter);

        // Find the positions
        let write_to_a_idx =
            copies.iter().position(|c| matches!(c.dst, CopyDest::Value(v) if v.index() == 0));
        let read_from_a_idx =
            copies.iter().position(|c| matches!(c.src, CopySource::Value(v) if v.index() == 0));
        assert!(read_from_a_idx.unwrap() < write_to_a_idx.unwrap());
    }

    #[test]
    fn test_simple_cycle() {
        // a = b, b = a (swap) requires a temporary
        let mut copies = vec![copy(1, 0), copy(0, 1)];
        let mut temp_counter = 0;
        sequentialize_copies(&mut copies, &mut temp_counter);

        // Should have extra copies for the temporary
        // Expected: tmp = a, a = b, b = tmp  OR  tmp = b, b = a, a = tmp
        assert!(copies.len() >= 3, "Cycle should introduce temporary copies");
        assert!(has_temp(&copies), "Should use temporaries for cycles");
        assert!(temp_counter >= 1, "Should allocate at least one temp");
    }

    #[test]
    fn test_three_way_cycle() {
        // a = b, b = c, c = a (rotate) requires a temporary
        let mut copies = vec![copy(1, 0), copy(2, 1), copy(0, 2)];
        let mut temp_counter = 0;
        sequentialize_copies(&mut copies, &mut temp_counter);

        // Should handle the 3-way rotation
        assert!(copies.len() >= 4, "3-way cycle should introduce temporary copies");
        assert!(has_temp(&copies), "Should use temporaries for cycles");
    }

    #[test]
    fn test_independent_copies() {
        // Completely independent copies: a = x, b = y, c = z
        let mut copies = vec![copy(10, 0), copy(11, 1), copy(12, 2)];
        let mut temp_counter = 0;
        sequentialize_copies(&mut copies, &mut temp_counter);

        // Should remain as 3 copies with no temporaries
        assert_eq!(copies.len(), 3);
        assert!(!has_temp(&copies));
    }
}

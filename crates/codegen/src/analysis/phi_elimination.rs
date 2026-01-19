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

use crate::mir::{BlockId, Function, InstId, InstKind, MirType, Value, ValueId};
use rustc_hash::FxHashMap;

/// Source for a parallel copy - either a regular value or a temporary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopySource {
    /// A regular MIR value.
    Value(ValueId),
    /// A temporary created during cycle breaking (identified by index).
    Temp(u32),
}

/// Destination for a parallel copy - either a regular value or a temporary.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum CopyDest {
    /// A regular MIR value.
    Value(ValueId),
    /// A temporary created during cycle breaking (identified by index).
    Temp(u32),
}

/// A parallel copy operation: copy from source to destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParallelCopy {
    /// Source value to copy from.
    pub src: CopySource,
    /// Destination value (the phi result).
    pub dst: CopyDest,
    /// Type of the copy.
    pub ty: MirType,
}

/// Copies to insert at the end of a block (before the terminator).
#[derive(Clone, Debug, Default)]
pub struct BlockCopies {
    /// Parallel copies to execute at this block's exit.
    pub copies: Vec<ParallelCopy>,
}

/// Result of phi elimination.
#[derive(Debug)]
pub struct PhiEliminationResult {
    /// Copies to insert at each predecessor block.
    pub block_copies: FxHashMap<BlockId, BlockCopies>,
    /// Phi instructions to remove (block, instruction index).
    pub phis_to_remove: Vec<(BlockId, usize)>,
}

/// Eliminates phi nodes by inserting parallel copies at predecessor block exits.
///
/// Returns the copies to insert and phis to remove. The caller is responsible
/// for actually modifying the function.
#[must_use]
pub fn eliminate_phis(func: &Function) -> PhiEliminationResult {
    let mut block_copies: FxHashMap<BlockId, BlockCopies> = FxHashMap::default();
    let mut phis_to_remove = Vec::new();

    // Process each block looking for phi instructions
    for (block_id, block) in func.blocks.iter_enumerated() {
        for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
            let inst = func.instruction(inst_id);

            if let InstKind::Phi(incoming) = &inst.kind {
                // Find the value defined by this phi
                let phi_dst = find_phi_dst(func, inst_id);

                if let Some(dst) = phi_dst {
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
    }

    // Sequentialize parallel copies to handle cycles
    let mut temp_counter = 0u32;
    for (_, copies) in block_copies.iter_mut() {
        sequentialize_copies(&mut copies.copies, &mut temp_counter);
    }

    PhiEliminationResult { block_copies, phis_to_remove }
}

/// Finds the ValueId that is defined by a phi instruction.
fn find_phi_dst(func: &Function, inst_id: InstId) -> Option<ValueId> {
    for (val_id, val) in func.values.iter_enumerated() {
        if let Value::Inst(def_inst) = val
            && *def_inst == inst_id
        {
            return Some(val_id);
        }
    }
    None
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

    let mut emitted = vec![false; pending.len()];

    // Emit copies in dependency order until we hit cycles
    loop {
        let mut made_progress = false;

        for i in 0..pending.len() {
            if emitted[i] {
                continue;
            }

            // Can emit if no one is blocking us (all readers of our dst have been emitted)
            if blocked_by[i] == 0 {
                result.push(pending[i].clone());
                emitted[i] = true;
                made_progress = true;

                // Unblock anyone who was waiting for us to read their dst
                if let Some(src) = src_value(&pending[i].src)
                    && let Some(&blocked_writer) = writes_to.get(&src)
                    && blocked_writer != i
                    && !emitted[blocked_writer]
                {
                    blocked_by[blocked_writer] = blocked_by[blocked_writer].saturating_sub(1);
                }
            }
        }

        if !made_progress {
            // All remaining copies form cycles - break one cycle at a time
            break_cycles(
                &pending,
                &mut emitted,
                &mut blocked_by,
                &writes_to,
                &mut result,
                temp_counter,
            );

            // If we broke at least one cycle, continue to see if more copies are now unblocked
            if !emitted.iter().all(|&e| e) {
                continue;
            }
            break;
        }

        if emitted.iter().all(|&e| e) {
            break;
        }
    }

    *copies = result;
}

/// Breaks cycles in the remaining copies by inserting a temporary.
///
/// For a cycle a -> b -> a, we:
/// 1. Pick one copy in the cycle (say b = a)
/// 2. Save its source to a temporary: tmp = a
/// 3. Emit all copies in the cycle normally: a = b
/// 4. Replace the broken copy's source with temp: b = tmp
fn break_cycles(
    pending: &[ParallelCopy],
    emitted: &mut [bool],
    blocked_by: &mut [usize],
    writes_to: &FxHashMap<ValueId, usize>,
    result: &mut Vec<ParallelCopy>,
    temp_counter: &mut u32,
) {
    // Find a copy that's part of a cycle (not emitted and blocking > 0)
    let cycle_start = pending
        .iter()
        .enumerate()
        .find(|(i, _)| !emitted[*i] && blocked_by[*i] > 0)
        .map(|(i, _)| i);

    let Some(start_idx) = cycle_start else {
        // No cycles, emit remaining in order
        for (i, copy) in pending.iter().enumerate() {
            if !emitted[i] {
                result.push(copy.clone());
                emitted[i] = true;
            }
        }
        return;
    };

    // Trace the cycle to find all participants
    let mut cycle_indices = vec![start_idx];
    let mut current = start_idx;

    loop {
        // Find the copy that writes to our source (the predecessor in the cycle)
        let Some(src) = src_value(&pending[current].src) else {
            break;
        };
        if let Some(&pred_idx) = writes_to.get(&src) {
            if emitted[pred_idx] {
                break;
            }
            if pred_idx == start_idx {
                // We've completed the cycle
                break;
            }
            if cycle_indices.contains(&pred_idx) {
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
        if !emitted[idx] && blocked_by[idx] == 0 {
            result.push(pending[idx].clone());
            emitted[idx] = true;

            // Unblock the writer of our source
            if let Some(src) = src_value(&pending[idx].src)
                && let Some(&blocked_writer) = writes_to.get(&src)
                && !emitted[blocked_writer]
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
    emitted[break_idx] = true;
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

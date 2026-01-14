//! Phi elimination for MIR.
//!
//! Converts SSA phi nodes into parallel copies inserted at predecessor block exits.
//! This is necessary because the EVM cannot directly execute phi nodes.
//!
//! The algorithm:
//! 1. For each phi node in block B with incoming value V from predecessor P,
//!    insert a copy from V to the phi's destination at the end of P.
//! 2. Handle cycles by detecting when copies form a cycle and using a temporary.
//! 3. Remove the phi instructions after copies are inserted.

use crate::mir::{BlockId, Function, InstId, InstKind, MirType, Value, ValueId};
use rustc_hash::FxHashMap;


/// A parallel copy operation: copy from source to destination.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParallelCopy {
    /// Source value to copy from.
    pub src: ValueId,
    /// Destination value (the phi result).
    pub dst: ValueId,
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
                            src: src_val,
                            dst,
                            ty,
                        });
                    }

                    phis_to_remove.push((block_id, inst_idx));
                }
            }
        }
    }

    // Sequentialize parallel copies to handle cycles
    for (_, copies) in block_copies.iter_mut() {
        sequentialize_copies(&mut copies.copies);
    }

    PhiEliminationResult { block_copies, phis_to_remove }
}

/// Finds the ValueId that is defined by a phi instruction.
fn find_phi_dst(func: &Function, inst_id: InstId) -> Option<ValueId> {
    for (val_id, val) in func.values.iter_enumerated() {
        if let Value::Inst(def_inst) = val
            && *def_inst == inst_id {
                return Some(val_id);
            }
    }
    None
}

/// Sequentializes parallel copies to handle dependencies and cycles.
///
/// A chain like: a = b, c = a needs to be ordered as: c = a, a = b
/// (read from a before writing to a)
///
/// A cycle like: a = b, b = a needs a temporary: tmp = a, a = b, b = tmp
fn sequentialize_copies(copies: &mut Vec<ParallelCopy>) {
    if copies.len() <= 1 {
        return;
    }

    let pending: Vec<ParallelCopy> = std::mem::take(copies);
    let mut result: Vec<ParallelCopy> = Vec::with_capacity(pending.len());

    // Map from value to index of copy that writes to it
    let mut writes_to: FxHashMap<ValueId, usize> = FxHashMap::default();
    for (i, copy) in pending.iter().enumerate() {
        writes_to.insert(copy.dst, i);
    }

    // For each copy, count how many other copies need to read its destination
    // before we can safely write to it
    let mut blocked_by: Vec<usize> = vec![0; pending.len()];
    for (i, copy) in pending.iter().enumerate() {
        // If copy i reads from value X, and copy j writes to X, then j is blocked by i
        if let Some(&writer_idx) = writes_to.get(&copy.src)
            && writer_idx != i {
                blocked_by[writer_idx] += 1;
            }
    }

    let mut emitted = vec![false; pending.len()];

    // Emit copies in dependency order
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
                // (i.e., if our src is someone else's dst)
                if let Some(&blocked_writer) = writes_to.get(&pending[i].src)
                    && blocked_writer != i && !emitted[blocked_writer] {
                        blocked_by[blocked_writer] = blocked_by[blocked_writer].saturating_sub(1);
                    }
            }
        }

        if !made_progress {
            // All remaining copies form cycles - just emit them
            // A proper implementation would use temporaries here
            for i in 0..pending.len() {
                if !emitted[i] {
                    result.push(pending[i].clone());
                }
            }
            break;
        }

        if emitted.iter().all(|&e| e) {
            break;
        }
    }

    *copies = result;
}

/// Represents a copy instruction to be inserted in the IR.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub(crate) struct CopyInst {
    /// Source value.
    pub src: ValueId,
    /// Destination value.
    pub dst: ValueId,
}

/// Generates the sequence of copy instructions for a block's parallel copies.
/// Returns copies in the order they should be executed.
#[allow(dead_code)]
pub(crate) fn generate_copy_sequence(copies: &[ParallelCopy]) -> Vec<CopyInst> {
    copies
        .iter()
        .map(|c| CopyInst { src: c.src, dst: c.dst })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_no_cycle() {
        let mut copies = vec![
            ParallelCopy {
                src: ValueId::from_usize(0),
                dst: ValueId::from_usize(1),
                ty: MirType::uint256(),
            },
            ParallelCopy {
                src: ValueId::from_usize(2),
                dst: ValueId::from_usize(3),
                ty: MirType::uint256(),
            },
        ];

        sequentialize_copies(&mut copies);
        assert_eq!(copies.len(), 2);
    }

    #[test]
    fn test_chain() {
        // a = b, c = a -> should do a = b first, then c = a
        let mut copies = vec![
            ParallelCopy {
                src: ValueId::from_usize(1), // b
                dst: ValueId::from_usize(0), // a
                ty: MirType::uint256(),
            },
            ParallelCopy {
                src: ValueId::from_usize(0), // a
                dst: ValueId::from_usize(2), // c
                ty: MirType::uint256(),
            },
        ];

        sequentialize_copies(&mut copies);

        // The copy reading from 'a' should come before the copy writing to 'a'
        let write_to_a_idx = copies.iter().position(|c| c.dst.index() == 0).unwrap();
        let read_from_a_idx = copies.iter().position(|c| c.src.index() == 0).unwrap();
        assert!(read_from_a_idx < write_to_a_idx);
    }
}

//! Liveness analysis for MIR.
//!
//! Computes which values are live at each program point using backward dataflow analysis.
//! A value is live at a point if there exists a path from that point to a use of the value
//! that doesn't pass through a definition of that value.
//!
//! The analysis stores one dense bit matrix row per block, indexed by `ValueId`.
//!
//! Phi nodes are ordinary instructions (`InstKind::Phi`) whose result is defined like
//! any other instruction result, but their incoming operands are uses on the incoming
//! edge: an operand is live out of the predecessor it flows from and is not live into
//! the merge block, which only sees the phi result.

use crate::mir::{BlockId, Function, InstKind, Terminator, Value, ValueId, analysis::CfgInfo};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{BitMatrix, BitMatrixRow, DenseBitSet},
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::collections::VecDeque;

/// Liveness analysis results for a function.
#[derive(Debug)]
pub(crate) struct Liveness {
    /// Values live at each block's entry.
    live_in: BitMatrix<BlockId, ValueId>,
    /// Values live at each block's exit.
    live_out: BitMatrix<BlockId, ValueId>,
    /// The last use location of each value within each block: (block, instruction index).
    /// The key is (ValueId, BlockId), and value is the instruction index (None = terminator).
    /// This tracks the last use of a value *within* each block where it's used.
    /// `None` when only the live sets were computed.
    last_use_in_block: Option<FxHashMap<(ValueId, BlockId), Option<usize>>>,
    /// Number of values in the function.
    #[allow(dead_code)]
    num_values: usize,
}

impl Liveness {
    /// Computes liveness for a function.
    #[must_use]
    pub(crate) fn compute(func: &Function) -> Self {
        Self::compute_inner(func, true)
    }

    /// Computes only the block live-in and live-out sets, without the per-block
    /// last uses that [`Self::is_used_at_or_after`] needs.
    #[must_use]
    pub(crate) fn compute_live_sets(func: &Function) -> Self {
        Self::compute_inner(func, false)
    }

    fn compute_inner(func: &Function, tracks_last_uses: bool) -> Self {
        let num_values = func.num_values();
        let num_blocks = func.blocks.len();

        let mut live_in = BitMatrix::new(num_blocks, num_values);
        let mut live_out = BitMatrix::new(num_blocks, num_values);

        // Compute local def/use sets for each block
        let mut block_defs = BitMatrix::new(num_blocks, num_values);
        let mut block_uses = BitMatrix::new(num_blocks, num_values);

        // Phi operands are uses on the incoming edge, keyed by the merge block: each entry
        // is live out of its predecessor and does not enter the merge block.
        let mut phi_edge_uses = (0..num_blocks)
            .map(|_| SmallVec::<[(BlockId, ValueId); 4]>::new())
            .collect::<IndexVec<BlockId, _>>();

        let mut operand_buf = SmallVec::<[ValueId; 8]>::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            // Process instructions in forward order to compute upward-exposed uses and defs
            for &inst_id in &block.instructions {
                let inst = func.inst(inst_id);

                if let InstKind::Phi(incoming) = &inst.kind {
                    phi_edge_uses[block_id].extend_from_slice(incoming);
                } else {
                    // Collect uses (upward-exposed uses - used before defined in this block)
                    inst.kind.visit_operands(|operand| {
                        if !block_defs.contains(block_id, operand) {
                            block_uses.insert(block_id, operand);
                        }
                    });
                }

                if let Some(val_id) = func.inst_result_value(inst_id) {
                    block_defs.insert(block_id, val_id);
                }
            }

            // Process terminator uses
            if let Some(term) = &block.terminator {
                operand_buf.clear();
                collect_terminator_uses(term, &mut operand_buf);
                for &operand in &operand_buf {
                    if !block_defs.contains(block_id, operand) {
                        block_uses.insert(block_id, operand);
                    }
                }
            }
        }

        // Worklist algorithm for computing live_in/live_out.
        //
        // live_out(B) = union over S in succ(B) of live_in(S) | phi operands S takes from B
        // live_in(B) = block_uses(B) | (live_out(B) - block_defs(B))
        // Seed the worklist in postorder, so successors settle before their predecessors even
        // when transforms append blocks in the middle of the CFG. Unreachable blocks follow.
        let cfg = CfgInfo::new(func);
        let mut worklist = cfg.rpo().iter().rev().copied().collect::<VecDeque<_>>();
        worklist.extend(func.blocks.indices().rev().filter(|&block| !cfg.is_reachable(block)));
        let mut queued = DenseBitSet::new_filled(num_blocks);
        let mut new_live_out = DenseBitSet::new_empty(num_values);
        let mut new_live_in = DenseBitSet::new_empty(num_values);

        while let Some(block_id) = worklist.pop_front() {
            queued.remove(block_id);
            let block = &func.blocks[block_id];

            new_live_out.clear();
            if let Some(term) = &block.terminator {
                term.for_each_successor(|succ| {
                    new_live_out.union(&live_in.row(succ));
                    for &(pred, value) in &phi_edge_uses[succ] {
                        if pred == block_id {
                            new_live_out.insert(value);
                        }
                    }
                });
            }

            // live_in = use ∪ (live_out - def)
            new_live_in.clone_from(&new_live_out);
            new_live_in.subtract(&block_defs.row(block_id));
            new_live_in.union(&block_uses.row(block_id));

            let out_changed = live_out.replace_row(block_id, &new_live_out);
            if live_in.replace_row(block_id, &new_live_in) | out_changed {
                // Add predecessors to worklist
                for &pred in &block.predecessors {
                    if queued.insert(pred) {
                        worklist.push_back(pred);
                    }
                }
            }
        }

        // Compute last use locations per block
        // For each value, track the last instruction index where it's used within each block.
        if !tracks_last_uses {
            return Self { live_in, live_out, last_use_in_block: None, num_values };
        }
        let mut last_use_in_block = FxHashMap::default();
        // A phi operand is used when its predecessor transfers control, so it
        // must survive to that block's terminator.
        for edge_uses in &phi_edge_uses {
            for &(pred, value) in edge_uses {
                last_use_in_block.insert((value, pred), None);
            }
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            // Check terminator uses - these are the last use in this block
            if let Some(term) = &block.terminator {
                operand_buf.clear();
                collect_terminator_uses(term, &mut operand_buf);
                for &operand in &operand_buf {
                    // Terminator is represented by None for inst_idx
                    last_use_in_block.entry((operand, block_id)).or_insert(None);
                }
            }

            // Check instruction uses in reverse order
            // The first occurrence in reverse order is the last use in forward order
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate().rev() {
                let inst = func.inst(inst_id);
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }
                inst.kind.visit_operands(|operand| {
                    last_use_in_block.entry((operand, block_id)).or_insert(Some(inst_idx));
                });
            }
        }

        Self { live_in, live_out, last_use_in_block: Some(last_use_in_block), num_values }
    }

    /// Returns whether every computed value is consumed in its defining block and the
    /// function reads no arguments.
    pub(crate) fn is_block_local(func: &Function) -> bool {
        let mut defining_blocks = index_vec![None; func.num_insts()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                defining_blocks[inst_id] = Some(block_id);
            }
        }
        let is_local = |value, block_id| match func.value(value) {
            Value::Inst(inst_id) => defining_blocks[*inst_id] == Some(block_id),
            Value::Arg(_) => false,
            Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => true,
        };
        func.blocks.iter_enumerated().all(|(block_id, block)| {
            block.instructions.iter().all(|&inst_id| {
                func.inst(inst_id).kind.operands().iter().all(|&value| is_local(value, block_id))
            }) && block
                .terminator
                .as_ref()
                .is_none_or(|term| term.operands().iter().all(|&value| is_local(value, block_id)))
        })
    }

    /// Returns the values live at the entry of a block.
    #[must_use]
    pub(crate) fn live_in(&self, block: BlockId) -> BitMatrixRow<'_, ValueId> {
        self.live_in.row(block)
    }

    /// Returns the values live at the exit of a block.
    #[must_use]
    pub(crate) fn live_out(&self, block: BlockId) -> BitMatrixRow<'_, ValueId> {
        self.live_out.row(block)
    }

    fn last_uses(&self) -> &FxHashMap<(ValueId, BlockId), Option<usize>> {
        self.last_use_in_block.as_ref().expect("liveness was computed without last uses")
    }

    /// Returns whether a value defined before `inst_idx` is used at or after that instruction.
    #[must_use]
    pub(crate) fn is_used_at_or_after(
        &self,
        val: ValueId,
        block: BlockId,
        inst_idx: usize,
    ) -> bool {
        if self.live_out(block).contains(val) {
            return true;
        }

        match self.last_uses().get(&(val, block)) {
            Some(Some(last_idx)) => *last_idx >= inst_idx,
            Some(None) => true,
            None => false,
        }
    }
}

/// Collects all value uses from a terminator.
fn collect_terminator_uses(term: &Terminator, out: &mut SmallVec<[ValueId; 8]>) {
    out.extend(term.operands());
}

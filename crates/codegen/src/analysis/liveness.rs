//! Liveness analysis for MIR.
//!
//! Computes which values are live at each program point using backward dataflow analysis.
//! A value is live at a point if there exists a path from that point to a use of the value
//! that doesn't pass through a definition of that value.
//!
//! The analysis uses dense bitsets indexed by `ValueId` for efficiency.

use crate::mir::{BlockId, Function, Terminator, Value, ValueId};
use rustc_hash::FxHashMap;
use std::collections::VecDeque;

/// A dense bitset for tracking live values.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LiveSet {
    /// Bit vector where bit i indicates whether value i is live.
    bits: Vec<u64>,
}

impl LiveSet {
    /// Creates a new empty live set with capacity for `n` values.
    #[must_use]
    pub fn with_capacity(n: usize) -> Self {
        let words = n.div_ceil(64);
        Self { bits: vec![0; words] }
    }

    /// Returns true if the value is in the set.
    #[must_use]
    pub fn contains(&self, val: ValueId) -> bool {
        let idx = val.index();
        let word = idx / 64;
        let bit = idx % 64;
        word < self.bits.len() && (self.bits[word] & (1u64 << bit)) != 0
    }

    /// Adds a value to the set. Returns true if the value was not already present.
    pub fn insert(&mut self, val: ValueId) -> bool {
        let idx = val.index();
        let word = idx / 64;
        let bit = idx % 64;
        if word >= self.bits.len() {
            self.bits.resize(word + 1, 0);
        }
        let mask = 1u64 << bit;
        let was_absent = (self.bits[word] & mask) == 0;
        self.bits[word] |= mask;
        was_absent
    }

    /// Removes a value from the set.
    pub fn remove(&mut self, val: ValueId) {
        let idx = val.index();
        let word = idx / 64;
        let bit = idx % 64;
        if word < self.bits.len() {
            self.bits[word] &= !(1u64 << bit);
        }
    }

    /// Unions this set with another, returning true if this set changed.
    pub fn union_with(&mut self, other: &Self) -> bool {
        let mut changed = false;
        if self.bits.len() < other.bits.len() {
            self.bits.resize(other.bits.len(), 0);
        }
        for (i, &word) in other.bits.iter().enumerate() {
            let old = self.bits[i];
            self.bits[i] |= word;
            if self.bits[i] != old {
                changed = true;
            }
        }
        changed
    }

    /// Returns an iterator over all values in the set.
    pub fn iter(&self) -> impl Iterator<Item = ValueId> + '_ {
        self.bits.iter().enumerate().flat_map(|(word_idx, &word)| {
            (0..64).filter_map(move |bit| {
                if (word & (1u64 << bit)) != 0 {
                    Some(ValueId::from_usize(word_idx * 64 + bit))
                } else {
                    None
                }
            })
        })
    }

    /// Returns the number of live values.
    #[must_use]
    pub fn count(&self) -> usize {
        self.bits.iter().map(|w| w.count_ones() as usize).sum()
    }

    /// Clears the set.
    pub fn clear(&mut self) {
        for word in &mut self.bits {
            *word = 0;
        }
    }
}

/// Per-instruction liveness information.
#[derive(Clone, Debug)]
pub struct LivenessInfo {
    /// Values that are live before this instruction.
    pub live_before: LiveSet,
    /// Values that are live after this instruction.
    pub live_after: LiveSet,
}

/// Per-block liveness results.
#[derive(Clone, Debug)]
struct BlockLiveness {
    /// Values live at block entry (live_in).
    live_in: LiveSet,
    /// Values live at block exit (live_out).
    live_out: LiveSet,
}

/// Liveness analysis results for a function.
#[derive(Debug)]
pub struct Liveness {
    /// Per-block liveness information (indexed by block index).
    block_liveness: Vec<BlockLiveness>,
    /// The last use location of each value within each block: (block, instruction index).
    /// The key is (ValueId, BlockId), and value is the instruction index (None = terminator).
    /// This tracks the last use of a value *within* each block where it's used.
    last_use_in_block: FxHashMap<(ValueId, BlockId), Option<usize>>,
    /// Number of values in the function.
    #[allow(dead_code)]
    num_values: usize,
}

impl Liveness {
    /// Computes liveness for a function.
    #[must_use]
    pub fn compute(func: &Function) -> Self {
        let num_values = func.values.len();
        let num_blocks = func.blocks.len();

        // Initialize per-block liveness
        let mut block_liveness: Vec<BlockLiveness> = (0..num_blocks)
            .map(|_| BlockLiveness {
                live_in: LiveSet::with_capacity(num_values),
                live_out: LiveSet::with_capacity(num_values),
            })
            .collect();

        // Compute local def/use sets for each block
        let mut block_defs: Vec<LiveSet> =
            (0..num_blocks).map(|_| LiveSet::with_capacity(num_values)).collect();
        let mut block_uses: Vec<LiveSet> =
            (0..num_blocks).map(|_| LiveSet::with_capacity(num_values)).collect();

        let mut operand_buf = Vec::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            let bidx = block_id.index();
            // Process instructions in forward order to compute upward-exposed uses and defs
            for &inst_id in &block.instructions {
                let inst = func.instruction(inst_id);

                // Collect uses (upward-exposed uses - used before defined in this block)
                operand_buf.clear();
                inst.kind.collect_operands(&mut operand_buf);
                for &operand in &operand_buf {
                    if !block_defs[bidx].contains(operand) {
                        block_uses[bidx].insert(operand);
                    }
                }

                // Record definition
                if let Value::Inst(def_inst) = func.value(ValueId::from_usize(inst_id.index()))
                    && *def_inst == inst_id
                {
                    block_defs[bidx].insert(ValueId::from_usize(inst_id.index()));
                }

                // Handle the result value
                // The instruction defines a value if it has a result
                if inst.result_ty.is_some() {
                    // Find the ValueId that corresponds to this instruction
                    for (val_id, val) in func.values.iter_enumerated() {
                        if let Value::Inst(inst) = val
                            && *inst == inst_id
                        {
                            block_defs[bidx].insert(val_id);
                            break;
                        }
                    }
                }
            }

            // Process terminator uses
            if let Some(term) = &block.terminator {
                let mut term_uses = Vec::new();
                collect_terminator_uses(term, &mut term_uses);
                for operand in term_uses {
                    if !block_defs[bidx].contains(operand) {
                        block_uses[bidx].insert(operand);
                    }
                }
            }
        }

        // Worklist algorithm for computing live_in/live_out
        // live_out(B) = ∪ live_in(S) for all successors S of B
        // live_in(B) = use(B) ∪ (live_out(B) - def(B))
        let mut worklist: VecDeque<BlockId> = func.blocks.indices().collect();

        // Initialize live_in for all blocks from their upward-exposed uses.
        // This is necessary because the worklist algorithm only updates live_in
        // when live_out changes, but initially live_out is empty everywhere.
        for bidx in 0..num_blocks {
            block_liveness[bidx].live_in = block_uses[bidx].clone();
        }

        while let Some(block_id) = worklist.pop_front() {
            let bidx = block_id.index();
            let block = &func.blocks[block_id];

            // Compute live_out as union of live_in of successors
            let mut new_live_out = LiveSet::with_capacity(num_values);
            for &succ in &block.successors {
                new_live_out.union_with(&block_liveness[succ.index()].live_in);
            }

            // Check if live_out changed
            if new_live_out != block_liveness[bidx].live_out {
                block_liveness[bidx].live_out = new_live_out;

                // Compute live_in = use ∪ (live_out - def)
                let mut new_live_in = block_uses[bidx].clone();
                for val in block_liveness[bidx].live_out.iter() {
                    if !block_defs[bidx].contains(val) {
                        new_live_in.insert(val);
                    }
                }

                if new_live_in != block_liveness[bidx].live_in {
                    block_liveness[bidx].live_in = new_live_in;

                    // Add predecessors to worklist
                    for &pred in &block.predecessors {
                        worklist.push_back(pred);
                    }
                }
            }
        }

        // Compute last use locations per block
        // For each value, track the last instruction index where it's used within each block.
        let mut last_use_in_block: FxHashMap<(ValueId, BlockId), Option<usize>> =
            FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            // Check terminator uses - these are the last use in this block
            if let Some(term) = &block.terminator {
                let mut term_uses = Vec::new();
                collect_terminator_uses(term, &mut term_uses);
                for operand in term_uses {
                    // Terminator is represented by None for inst_idx
                    last_use_in_block.entry((operand, block_id)).or_insert(None);
                }
            }

            // Check instruction uses in reverse order
            // The first occurrence in reverse order is the last use in forward order
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate().rev() {
                let inst = func.instruction(inst_id);
                operand_buf.clear();
                inst.kind.collect_operands(&mut operand_buf);
                for &operand in &operand_buf {
                    last_use_in_block.entry((operand, block_id)).or_insert(Some(inst_idx));
                }
            }
        }

        Self { block_liveness, last_use_in_block, num_values }
    }

    /// Returns the values live at the entry of a block.
    #[must_use]
    pub fn live_in(&self, block: BlockId) -> &LiveSet {
        &self.block_liveness[block.index()].live_in
    }

    /// Returns the values live at the exit of a block.
    #[must_use]
    pub fn live_out(&self, block: BlockId) -> &LiveSet {
        &self.block_liveness[block.index()].live_out
    }

    /// Computes liveness at a specific instruction within a block.
    /// Returns values live before and after the instruction.
    #[must_use]
    pub fn live_at_inst(
        &self,
        func: &Function,
        block_id: BlockId,
        inst_idx: usize,
    ) -> LivenessInfo {
        let bidx = block_id.index();
        let block = &func.blocks[block_id];

        // Start with live_out of the block
        let mut live = self.block_liveness[bidx].live_out.clone();

        // Process terminator (kills uses, adds no defs for our purposes)
        if let Some(term) = &block.terminator {
            let mut term_uses = Vec::new();
            collect_terminator_uses(term, &mut term_uses);
            for operand in term_uses {
                live.insert(operand);
            }
        }

        // Process instructions in reverse order from end to inst_idx
        let mut operand_buf = Vec::new();
        let mut live_after = None;

        for (idx, &inst_id) in block.instructions.iter().enumerate().rev() {
            if idx == inst_idx {
                live_after = Some(live.clone());
            }

            let inst = func.instruction(inst_id);

            // Remove definition (value becomes dead before this instruction)
            for (val_id, val) in func.values.iter_enumerated() {
                if let Value::Inst(def_inst) = val
                    && *def_inst == inst_id
                {
                    live.remove(val_id);
                }
            }

            // Add uses (values become live before this instruction)
            operand_buf.clear();
            inst.kind.collect_operands(&mut operand_buf);
            for &operand in &operand_buf {
                live.insert(operand);
            }

            if idx == inst_idx {
                return LivenessInfo { live_before: live, live_after: live_after.unwrap() };
            }
        }

        // Should not reach here
        LivenessInfo { live_before: live.clone(), live_after: live }
    }

    /// Returns the last use location of a value within a specific block, if any.
    /// Returns `Some(Some(inst_idx))` if last used at an instruction,
    /// `Some(None)` if last used in a terminator,
    /// `None` if the value is not used in this block.
    #[must_use]
    pub fn last_use_in_block(&self, val: ValueId, block: BlockId) -> Option<Option<usize>> {
        self.last_use_in_block.get(&(val, block)).copied()
    }

    /// Returns true if the value is dead after the given instruction in the given block.
    ///
    /// A value is dead after an instruction if:
    /// 1. The instruction is the last use of the value within this block, AND
    /// 2. The value is NOT in live_out (meaning no successor blocks use it)
    #[must_use]
    pub fn is_dead_after(&self, val: ValueId, block: BlockId, inst_idx: usize) -> bool {
        // If the value is in live_out, it's used by successor blocks, so it's not dead
        if self.block_liveness[block.index()].live_out.contains(val) {
            return false;
        }

        // Check if this instruction is the last use within this block
        match self.last_use_in_block.get(&(val, block)) {
            Some(&Some(last_idx)) => last_idx == inst_idx,
            // Last use is in terminator - not dead after any instruction
            Some(&None) => false,
            // Value not used in this block at all - should not happen if we're asking
            // but conservatively say it's dead
            None => true,
        }
    }
}

/// Collects all value uses from a terminator.
fn collect_terminator_uses(term: &Terminator, out: &mut Vec<ValueId>) {
    match term {
        Terminator::Jump(_) => {}
        Terminator::Branch { condition, .. } => {
            out.push(*condition);
        }
        Terminator::Switch { value, cases, .. } => {
            out.push(*value);
            for (case_val, _) in cases {
                out.push(*case_val);
            }
        }
        Terminator::Return { values } => {
            out.extend(values.iter().copied());
        }
        Terminator::Revert { offset, size } => {
            out.push(*offset);
            out.push(*size);
        }
        Terminator::Stop | Terminator::Invalid => {}
        Terminator::SelfDestruct { recipient } => {
            out.push(*recipient);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_liveset_basic() {
        let mut set = LiveSet::with_capacity(100);
        let v0 = ValueId::from_usize(0);
        let v42 = ValueId::from_usize(42);
        let v99 = ValueId::from_usize(99);

        assert!(!set.contains(v0));
        assert!(!set.contains(v42));

        assert!(set.insert(v0));
        assert!(set.contains(v0));
        assert!(!set.insert(v0)); // Already present

        assert!(set.insert(v42));
        assert!(set.contains(v42));

        assert!(set.insert(v99));
        assert_eq!(set.count(), 3);

        set.remove(v42);
        assert!(!set.contains(v42));
        assert_eq!(set.count(), 2);
    }

    #[test]
    fn test_liveset_union() {
        let mut set1 = LiveSet::with_capacity(64);
        let mut set2 = LiveSet::with_capacity(64);

        set1.insert(ValueId::from_usize(1));
        set1.insert(ValueId::from_usize(3));
        set2.insert(ValueId::from_usize(2));
        set2.insert(ValueId::from_usize(3));

        assert!(set1.union_with(&set2));
        assert!(set1.contains(ValueId::from_usize(1)));
        assert!(set1.contains(ValueId::from_usize(2)));
        assert!(set1.contains(ValueId::from_usize(3)));
        assert_eq!(set1.count(), 3);

        // Union again should not change
        assert!(!set1.union_with(&set2));
    }
}

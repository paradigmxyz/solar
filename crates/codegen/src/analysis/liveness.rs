//! Liveness analysis for MIR.
//!
//! Computes which values are live at each program point using backward dataflow analysis.
//! A value is live at a point if there exists a path from that point to a use of the value
//! that doesn't pass through a definition of that value.
//!
//! The analysis uses dense bitsets indexed by `ValueId` for efficiency.
//!
//! Phi nodes are ordinary instructions (`InstKind::Phi`): their incoming operands are
//! treated as uses at the phi instruction in the merge block, and the phi result is
//! defined like any other instruction result.

use crate::mir::{BlockId, Function, InstId, Terminator, Value, ValueId};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::{IndexVec, index_vec},
    map::FxHashMap,
};
use std::collections::VecDeque;

/// A dense bitset for tracking live values.
pub(crate) type LiveSet = GrowableBitSet<ValueId>;

#[cfg(test)]
#[derive(Clone, Debug)]
struct LivenessInfo {
    live_before: LiveSet,
    live_after: LiveSet,
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
pub(crate) struct Liveness {
    /// Per-block liveness information (indexed by block index).
    block_liveness: IndexVec<BlockId, BlockLiveness>,
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
    pub(crate) fn compute(func: &Function) -> Self {
        let num_values = func.values.len();
        let num_blocks = func.blocks.len();

        // Precompute InstId → ValueId mapping.
        // This replaces the O(n) linear scans that were previously done per-instruction.
        let mut inst_to_value: FxHashMap<InstId, ValueId> = FxHashMap::default();
        for (val_id, val) in func.values.iter_enumerated() {
            if let Value::Inst(inst_id) = val {
                inst_to_value.insert(*inst_id, val_id);
            }
        }

        // Initialize per-block liveness
        let mut block_liveness: IndexVec<BlockId, BlockLiveness> = (0..num_blocks)
            .map(|_| BlockLiveness {
                live_in: LiveSet::with_capacity(num_values),
                live_out: LiveSet::with_capacity(num_values),
            })
            .collect();

        // Compute local def/use sets for each block
        let mut block_defs: IndexVec<BlockId, LiveSet> =
            (0..num_blocks).map(|_| LiveSet::with_capacity(num_values)).collect();
        let mut block_uses: IndexVec<BlockId, LiveSet> =
            (0..num_blocks).map(|_| LiveSet::with_capacity(num_values)).collect();

        let mut operand_buf = SmallVec::<[ValueId; 8]>::new();

        for (block_id, block) in func.blocks.iter_enumerated() {
            // Process instructions in forward order to compute upward-exposed uses and defs
            for &inst_id in &block.instructions {
                let inst = func.instruction(inst_id);

                // Collect uses (upward-exposed uses - used before defined in this block)
                operand_buf.clear();
                inst.kind.collect_operands(&mut operand_buf);
                for &operand in &operand_buf {
                    if !block_defs[block_id].contains(operand) {
                        block_uses[block_id].insert(operand);
                    }
                }

                // Record definition using precomputed map (O(1) instead of O(n)).
                if let Some(&val_id) = inst_to_value.get(&inst_id) {
                    block_defs[block_id].insert(val_id);
                }
            }

            // Process terminator uses
            if let Some(term) = &block.terminator {
                operand_buf.clear();
                collect_terminator_uses(term, &mut operand_buf);
                for &operand in &operand_buf {
                    if !block_defs[block_id].contains(operand) {
                        block_uses[block_id].insert(operand);
                    }
                }
            }
        }

        // Worklist algorithm for computing live_in/live_out.
        //
        // live_out(B) = union over S in succ(B) of live_in(S)
        // live_in(B) = block_uses(B) | (live_out(B) - block_defs(B))
        let mut worklist: VecDeque<BlockId> = func.blocks.indices().rev().collect();
        let mut queued = DenseBitSet::new_filled(num_blocks);
        let mut new_live_out = LiveSet::with_capacity(num_values);
        let mut new_live_in = LiveSet::with_capacity(num_values);

        while let Some(block_id) = worklist.pop_front() {
            queued.remove(block_id);
            let block = &func.blocks[block_id];

            new_live_out.clear();
            let successors =
                block.terminator.as_ref().map(Terminator::successors).unwrap_or_default();
            for succ in successors {
                new_live_out.union(&block_liveness[succ].live_in);
            }

            // live_in = use ∪ (live_out - def)
            new_live_in.clone_from(&new_live_out);
            new_live_in.subtract(&block_defs[block_id]);
            new_live_in.union(&block_uses[block_id]);

            if new_live_out != block_liveness[block_id].live_out
                || new_live_in != block_liveness[block_id].live_in
            {
                std::mem::swap(&mut block_liveness[block_id].live_out, &mut new_live_out);
                std::mem::swap(&mut block_liveness[block_id].live_in, &mut new_live_in);

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
        let mut last_use_in_block: FxHashMap<(ValueId, BlockId), Option<usize>> =
            FxHashMap::default();
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

    /// Computes the subset of liveness needed by codegen when every computed
    /// value is consumed in its defining block and the function has no
    /// arguments. Returns `None` when the function needs full dataflow.
    ///
    /// Immediate and undefined values are rematerializable, so they do not
    /// need live-in/live-out tracking. Instruction results still retain exact
    /// last-use information for stack scheduling.
    pub(crate) fn compute_block_local_for_codegen(func: &Function) -> Option<Self> {
        let num_values = func.values.len();
        for val in &func.values {
            match val {
                Value::Inst(_) => {}
                Value::Arg { .. } => return None,
                Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => {}
            }
        }

        let mut defining_blocks = index_vec![None; func.instructions.len()];
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                defining_blocks[inst_id] = Some(block_id);
            }
        }

        let is_local = |value, block_id| match func.value(value) {
            Value::Inst(inst_id) => defining_blocks[*inst_id] == Some(block_id),
            Value::Arg { .. } => false,
            Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => true,
        };
        let mut operands = SmallVec::<[ValueId; 8]>::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                operands.clear();
                func.instruction(inst_id).kind.collect_operands(&mut operands);
                if operands.iter().any(|&value| !is_local(value, block_id)) {
                    return None;
                }
            }
            if let Some(term) = &block.terminator {
                operands.clear();
                collect_terminator_uses(term, &mut operands);
                if operands.iter().any(|&value| !is_local(value, block_id)) {
                    return None;
                }
            }
        }

        let block_liveness: IndexVec<BlockId, _> = (0..func.blocks.len())
            .map(|_| BlockLiveness {
                live_in: LiveSet::new_empty(),
                live_out: LiveSet::new_empty(),
            })
            .collect();
        let mut last_use_in_block = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            if let Some(term) = &block.terminator {
                operands.clear();
                collect_terminator_uses(term, &mut operands);
                for &operand in &operands {
                    last_use_in_block.entry((operand, block_id)).or_insert(None);
                }
            }
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate().rev() {
                operands.clear();
                func.instruction(inst_id).kind.collect_operands(&mut operands);
                for &operand in &operands {
                    last_use_in_block.entry((operand, block_id)).or_insert(Some(inst_idx));
                }
            }
        }

        Some(Self { block_liveness, last_use_in_block, num_values })
    }

    /// Returns the values live at the entry of a block.
    #[must_use]
    pub(crate) fn live_in(&self, block: BlockId) -> &LiveSet {
        &self.block_liveness[block].live_in
    }

    /// Returns the values live at the exit of a block.
    #[must_use]
    pub(crate) fn live_out(&self, block: BlockId) -> &LiveSet {
        &self.block_liveness[block].live_out
    }

    #[cfg(test)]
    fn live_at_inst(&self, func: &Function, block_id: BlockId, inst_idx: usize) -> LivenessInfo {
        let block = &func.blocks[block_id];
        let inst_to_value = func.inst_results();
        let mut live = self.block_liveness[block_id].live_out.clone();

        if let Some(term) = &block.terminator {
            let mut term_uses = SmallVec::<[ValueId; 8]>::new();
            collect_terminator_uses(term, &mut term_uses);
            for operand in term_uses {
                live.insert(operand);
            }
        }

        let mut operand_buf = SmallVec::<[ValueId; 8]>::new();
        for (idx, &inst_id) in block.instructions.iter().enumerate().rev() {
            let live_after = (idx == inst_idx).then(|| live.clone());
            if let Some(&value) = inst_to_value.get(&inst_id) {
                live.remove(value);
            }
            operand_buf.clear();
            func.instruction(inst_id).kind.collect_operands(&mut operand_buf);
            for &operand in &operand_buf {
                live.insert(operand);
            }
            if let Some(live_after) = live_after {
                return LivenessInfo { live_before: live, live_after };
            }
        }

        LivenessInfo { live_before: live.clone(), live_after: live }
    }

    #[cfg(test)]
    fn last_use_in_block(&self, val: ValueId, block: BlockId) -> Option<Option<usize>> {
        self.last_use_in_block.get(&(val, block)).copied()
    }

    /// Returns whether a value defined before `inst_idx` is used at or after that instruction.
    #[must_use]
    pub(crate) fn is_used_at_or_after(
        &self,
        val: ValueId,
        block: BlockId,
        inst_idx: usize,
    ) -> bool {
        if self.block_liveness[block].live_out.contains(val) {
            return true;
        }

        match self.last_use_in_block.get(&(val, block)) {
            Some(Some(last_idx)) => *last_idx >= inst_idx,
            Some(None) => true,
            None => false,
        }
    }

    /// Returns true if the value is dead after the given instruction in the given block.
    ///
    /// A value is dead after an instruction if:
    /// 1. The instruction is the last use of the value within this block, AND
    /// 2. The value is NOT in live_out (meaning no successor blocks use it)
    #[must_use]
    pub(crate) fn is_dead_after(&self, val: ValueId, block: BlockId, inst_idx: usize) -> bool {
        // If the value is in live_out, it's used by successor blocks, so it's not dead
        if self.block_liveness[block].live_out.contains(val) {
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
fn collect_terminator_uses(term: &Terminator, out: &mut SmallVec<[ValueId; 8]>) {
    out.extend(term.operands());
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

        assert!(set1.union(&set2));
        assert!(set1.contains(ValueId::from_usize(1)));
        assert!(set1.contains(ValueId::from_usize(2)));
        assert!(set1.contains(ValueId::from_usize(3)));
        assert_eq!(set1.count(), 3);

        // Union again should not change
        assert!(!set1.union(&set2));
    }

    #[test]
    fn test_liveset_boundary() {
        let mut set = LiveSet::with_capacity(200);
        for i in [0, 1, 62, 63, 64, 65, 126, 127, 128, 129, 199] {
            assert!(set.insert(ValueId::from_usize(i)));
            assert!(set.contains(ValueId::from_usize(i)));
        }
        assert_eq!(set.count(), 11);
    }

    #[test]
    fn test_liveset_clear() {
        let mut set = LiveSet::with_capacity(128);
        set.insert(ValueId::from_usize(0));
        set.insert(ValueId::from_usize(63));
        set.insert(ValueId::from_usize(64));
        set.insert(ValueId::from_usize(127));
        assert_eq!(set.count(), 4);
        set.clear();
        assert_eq!(set.count(), 0);
        assert!(!set.contains(ValueId::from_usize(0)));
    }

    #[test]
    fn test_liveset_iter() {
        let mut set = LiveSet::with_capacity(200);
        let indices = [0, 5, 63, 64, 127, 128, 199];
        for &i in &indices {
            set.insert(ValueId::from_usize(i));
        }
        let collected: Vec<usize> = set.iter().map(|v| v.index()).collect();
        assert_eq!(collected, indices);
    }

    // === Liveness algorithm tests ===

    use crate::mir::{Function, FunctionBuilder, MirType};
    use solar_interface::Ident;

    fn make_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn test_linear_code() {
        // bb0: v2 = add v0, v1; ret v2
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let x = b.add_param(MirType::uint256());
        let one = b.imm_u64(1);
        let sum = b.add(x, one);
        b.ret([sum]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        // x and one are used by add, so live-in to entry.
        assert!(liveness.live_in(entry).contains(x));
        assert!(liveness.live_in(entry).contains(one));
        // sum is defined in entry and consumed by ret; not live-out.
        assert!(!liveness.live_out(entry).contains(sum));
        // Nothing escapes a single return block.
        assert_eq!(liveness.live_out(entry).count(), 0);
    }

    #[test]
    fn test_diamond_cfg() {
        // entry: branch cond, then_bb, else_bb
        // then_bb: v_then = add x, c1; jump merge
        // else_bb: v_else = sub x, c2; jump merge
        // merge: ret x  (x used across branches)
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let x = b.add_param(MirType::uint256());
        let cond = b.add_param(MirType::Bool);

        let then_bb = b.create_block();
        let else_bb = b.create_block();
        let merge = b.create_block();

        b.branch(cond, then_bb, else_bb);

        b.switch_to_block(then_bb);
        let c1 = b.imm_u64(1);
        let v_then = b.add(x, c1);
        b.jump(merge);

        b.switch_to_block(else_bb);
        let c2 = b.imm_u64(1);
        let v_else = b.sub(x, c2);
        b.jump(merge);

        // merge: return both values via x (simplified: just ret x)
        b.switch_to_block(merge);
        b.ret([v_then]);

        let liveness = Liveness::compute(&func);

        // x must be live-in to then_bb and else_bb (used in add/sub).
        assert!(liveness.live_in(then_bb).contains(x));
        assert!(liveness.live_in(else_bb).contains(x));
        // x must be live-out of entry (flows to successors).
        assert!(liveness.live_out(func.entry_block).contains(x));
        // v_then must be live-out of then_bb (used in merge's ret).
        assert!(liveness.live_out(then_bb).contains(v_then));
        // v_else should NOT be live-out of else_bb (merge returns v_then, not v_else).
        assert!(!liveness.live_out(else_bb).contains(v_else));
    }

    #[test]
    fn test_simple_loop() {
        // entry: jump header
        // header: cond = lt(i, limit); branch cond, body, exit
        // body: i_next = add(i, step); jump header
        // exit: ret i
        //
        // Note: without phis, i is the param. This tests cross-block liveness.
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let i = b.add_param(MirType::uint256());
        let limit = b.imm_u64(10);

        let header = b.create_block();
        let body = b.create_block();
        let exit = b.create_block();

        b.jump(header);

        b.switch_to_block(header);
        let cond = b.lt(i, limit);
        b.branch(cond, body, exit);

        b.switch_to_block(body);
        let step = b.imm_u64(1);
        let _i_next = b.add(i, step);
        b.jump(header);

        b.switch_to_block(exit);
        b.ret([i]);

        let liveness = Liveness::compute(&func);

        // i must be live through the entire loop.
        assert!(liveness.live_in(header).contains(i), "i live-in to header");
        assert!(liveness.live_in(body).contains(i), "i live-in to body");
        assert!(liveness.live_in(exit).contains(i), "i live-in to exit");
        assert!(liveness.live_out(header).contains(i), "i live-out of header");
        assert!(liveness.live_out(body).contains(i), "i live-out of body");
    }

    #[test]
    fn test_dead_instruction_result() {
        // A dead *instruction result* (not just an immediate).
        // The add result is never used — liveness should not track it
        // beyond its definition point.
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let x = b.add_param(MirType::uint256());
        let y = b.add_param(MirType::uint256());
        let dead = b.add(x, y); // result never used
        b.ret([x]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        assert!(liveness.live_in(entry).contains(x));
        // dead instruction result must not be live-out.
        assert!(!liveness.live_out(entry).contains(dead));
        // dead after its own instruction.
        assert!(liveness.is_dead_after(dead, entry, 0), "dead inst result should be dead");
    }

    #[test]
    fn test_unused_param() {
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let _x = b.add_param(MirType::uint256()); // Unused.
        let y = b.add_param(MirType::uint256());
        b.ret([y]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        assert!(!liveness.live_in(entry).contains(_x), "unused param not live");
        assert!(liveness.live_in(entry).contains(y), "used param is live");
    }

    #[test]
    fn test_value_used_in_two_successors() {
        // entry: branch cond, left, right
        // left: ret x
        // right: ret x
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let x = b.add_param(MirType::uint256());
        let cond = b.add_param(MirType::Bool);

        let left = b.create_block();
        let right = b.create_block();

        b.branch(cond, left, right);

        b.switch_to_block(left);
        b.ret([x]);

        b.switch_to_block(right);
        b.ret([x]);

        let liveness = Liveness::compute(&func);

        assert!(liveness.live_in(left).contains(x));
        assert!(liveness.live_in(right).contains(x));
        assert!(liveness.live_out(func.entry_block).contains(x));
    }

    #[test]
    fn test_empty_function() {
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        b.stop();

        let liveness = Liveness::compute(&func);
        assert_eq!(liveness.live_in(func.entry_block).count(), 0);
        assert_eq!(liveness.live_out(func.entry_block).count(), 0);
    }

    #[test]
    fn test_side_effect_op_keeps_operands_live() {
        // sstore(slot, val); loaded = sload(slot); ret loaded
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let slot = b.add_param(MirType::uint256());
        let val = b.add_param(MirType::uint256());
        b.sstore(slot, val);
        let loaded = b.sload(slot);
        b.ret([loaded]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        // Before sstore (inst 0): slot and val must be live.
        let info_0 = liveness.live_at_inst(&func, entry, 0);
        assert!(info_0.live_before.contains(slot), "slot live before sstore");
        assert!(info_0.live_before.contains(val), "val live before sstore");

        // Before sload (inst 1): slot must be live, val no longer needed.
        let info_1 = liveness.live_at_inst(&func, entry, 1);
        assert!(info_1.live_before.contains(slot), "slot live before sload");
        assert!(!info_1.live_before.contains(val), "val not live before sload");
    }

    #[test]
    fn test_live_at_inst() {
        // bb0: v2 = add v0, v1; v3 = mul v2, v0; ret v3
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let v0 = b.add_param(MirType::uint256());
        let v1 = b.add_param(MirType::uint256());
        let v2 = b.add(v0, v1);
        let v3 = b.mul(v2, v0);
        b.ret([v3]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        // Before add (inst 0): v0 and v1 are live.
        let info_0 = liveness.live_at_inst(&func, entry, 0);
        assert!(info_0.live_before.contains(v0));
        assert!(info_0.live_before.contains(v1));
        assert!(!info_0.live_before.contains(v2));
        assert!(!info_0.live_before.contains(v3));

        // After add (inst 0): v0 and v2 are live (v0 used again by mul).
        assert!(info_0.live_after.contains(v0));
        assert!(info_0.live_after.contains(v2));
        assert!(!info_0.live_after.contains(v1), "v1 dead after add");

        // Before mul (inst 1): v0 and v2 are live.
        let info_1 = liveness.live_at_inst(&func, entry, 1);
        assert!(info_1.live_before.contains(v0));
        assert!(info_1.live_before.contains(v2));

        // After mul (inst 1): only v3 is live (used by ret).
        assert!(info_1.live_after.contains(v3));
        assert!(!info_1.live_after.contains(v0));
        assert!(!info_1.live_after.contains(v2));
    }

    #[test]
    fn test_is_dead_after() {
        // bb0: v2 = add v0, v1; ret v2
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let v0 = b.add_param(MirType::uint256());
        let v1 = b.add_param(MirType::uint256());
        let v2 = b.add(v0, v1);
        b.ret([v2]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        // v0 and v1 are dead after the add (inst 0) — their last use is at inst 0.
        assert!(liveness.is_dead_after(v0, entry, 0), "v0 dead after add");
        assert!(liveness.is_dead_after(v1, entry, 0), "v1 dead after add");
        // v2 is NOT dead after add — it's used by ret (terminator).
        assert!(!liveness.is_dead_after(v2, entry, 0), "v2 alive after add");
    }

    #[test]
    fn test_last_use_in_block() {
        // bb0: v2 = add v0, v1; v3 = mul v2, v0; ret v3
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let v0 = b.add_param(MirType::uint256());
        let v1 = b.add_param(MirType::uint256());
        let _v2 = b.add(v0, v1);
        let v3 = b.mul(_v2, v0);
        b.ret([v3]);

        let liveness = Liveness::compute(&func);
        let entry = func.entry_block;

        // v0 last used at inst 1 (mul).
        assert_eq!(liveness.last_use_in_block(v0, entry), Some(Some(1)));
        // v1 last used at inst 0 (add).
        assert_eq!(liveness.last_use_in_block(v1, entry), Some(Some(0)));
        // v3 last used at terminator (ret).
        assert_eq!(liveness.last_use_in_block(v3, entry), Some(None));
    }

    #[test]
    fn test_value_live_across_multiple_blocks() {
        // entry: jump bb1
        // bb1: v2 = add(v0, v1); jump bb2
        // bb2: ret v0  (v0 must be live through bb1 and into bb2)
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let v0 = b.add_param(MirType::uint256());
        let v1 = b.add_param(MirType::uint256());

        let bb1 = b.create_block();
        let bb2 = b.create_block();

        b.jump(bb1);

        b.switch_to_block(bb1);
        let _v2 = b.add(v0, v1);
        b.jump(bb2);

        b.switch_to_block(bb2);
        b.ret([v0]);

        let liveness = Liveness::compute(&func);

        // v0 must be live-out of bb1 (used in bb2).
        assert!(liveness.live_out(bb1).contains(v0));
        // v1 is NOT live-out of bb1 (only used locally in bb1's add).
        assert!(!liveness.live_out(bb1).contains(v1));
        // v0 must be live-in to bb2.
        assert!(liveness.live_in(bb2).contains(v0));
    }

    #[test]
    fn test_multiple_returns() {
        // entry: branch cond, left, right
        // left: ret v0
        // right: ret v1
        let mut func = make_func();
        let mut b = FunctionBuilder::new(&mut func);
        let v0 = b.add_param(MirType::uint256());
        let v1 = b.add_param(MirType::uint256());
        let cond = b.add_param(MirType::Bool);

        let left = b.create_block();
        let right = b.create_block();

        b.branch(cond, left, right);

        b.switch_to_block(left);
        b.ret([v0]);

        b.switch_to_block(right);
        b.ret([v1]);

        let liveness = Liveness::compute(&func);

        // v0 is live-in to left, NOT to right.
        assert!(liveness.live_in(left).contains(v0));
        assert!(!liveness.live_in(right).contains(v0));
        // v1 is live-in to right, NOT to left.
        assert!(liveness.live_in(right).contains(v1));
        assert!(!liveness.live_in(left).contains(v1));
    }

    #[test]
    fn test_phi_liveness() {
        // Phi nodes are ordinary instructions (`InstKind::Phi`): their incoming
        // operands are uses at the phi instruction in the merge block, and the
        // phi result is defined like any other instruction result.
        //
        // entry: jump header
        // header: phi_val = phi [(entry, init), (body, updated)]
        //         branch cond, body, exit
        // body:   updated = add phi_val, step; jump header
        // exit:   ret phi_val
        let mut func = make_func();

        // Phase 1: build the CFG skeleton without the phi (to avoid borrow conflicts).
        let entry;
        let header;
        let body;
        let exit;
        let init;
        let updated;
        let phi_placeholder; // ValueId slot we'll point at the phi instruction.
        {
            let mut b = FunctionBuilder::new(&mut func);
            init = b.imm_u64(0);
            entry = b.current_block();
            header = b.create_block();
            body = b.create_block();
            exit = b.create_block();

            b.jump(header);

            b.switch_to_block(header);
            // Allocate a placeholder for the phi result (an undef that we'll replace).
            phi_placeholder = b.undef(MirType::uint256());
            let limit = b.imm_u64(10);
            let cond = b.lt(phi_placeholder, limit);
            b.branch(cond, body, exit);

            b.switch_to_block(body);
            let step = b.imm_u64(1);
            updated = b.add(phi_placeholder, step);
            b.jump(header);

            b.switch_to_block(exit);
            b.ret([phi_placeholder]);
        }

        // Phase 2: insert the phi instruction at the head of `header` and point
        // the placeholder value at its result.
        let phi_val = phi_placeholder;
        let phi_inst = func.alloc_inst(crate::mir::Instruction::new(
            crate::mir::InstKind::Phi(vec![(entry, init), (body, updated)]),
            Some(MirType::uint256()),
        ));
        func.blocks[header].instructions.insert(0, phi_inst);
        func.values[phi_val] = crate::mir::Value::Inst(phi_inst);

        let liveness = Liveness::compute(&func);

        // init must be live-out of entry (used by the phi in header).
        assert!(liveness.live_out(entry).contains(init), "init live-out of entry");
        // updated must be live-out of body (flows back to the phi via the back-edge).
        assert!(liveness.live_out(body).contains(updated), "updated live-out of body");
        // phi_val must be live-in to body and exit (used by add and ret).
        assert!(liveness.live_in(body).contains(phi_val), "phi_val live-in to body");
        assert!(liveness.live_in(exit).contains(phi_val), "phi_val live-in to exit");
        // phi_val should NOT be live-in to entry (it's defined in header).
        assert!(!liveness.live_in(entry).contains(phi_val), "phi_val not live-in to entry");
    }
}

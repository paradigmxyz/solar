//! Loop analysis for MIR.
//!
//! This module provides loop detection and analysis for optimization passes:
//! - Natural loop detection using dominance-based algorithm
//! - Loop variable identification
//! - Loop-invariant computation detection
//! - Loop bound analysis

use crate::mir::{BlockId, Function, InstId, InstKind, Terminator, Value, ValueId};
use rustc_hash::{FxHashMap, FxHashSet};
use smallvec::SmallVec;

/// A natural loop in the control flow graph.
#[derive(Clone, Debug)]
pub struct Loop {
    /// The header block (entry point with back edge).
    pub header: BlockId,
    /// All blocks in the loop body (including header).
    pub blocks: FxHashSet<BlockId>,
    /// Back edges: blocks that jump back to the header.
    pub back_edges: SmallVec<[BlockId; 2]>,
    /// Exit blocks: blocks outside the loop that are successors of loop blocks.
    pub exit_blocks: SmallVec<[BlockId; 2]>,
    /// Preheader block (if exists): unique predecessor of header outside the loop.
    pub preheader: Option<BlockId>,
    /// Loop induction variables.
    pub induction_vars: Vec<InductionVariable>,
    /// Instructions that are invariant (don't change within the loop).
    pub invariant_insts: FxHashSet<InstId>,
    /// Optional: constant trip count if statically known.
    pub trip_count: Option<u64>,
}

/// An induction variable in a loop.
#[derive(Clone, Debug)]
pub struct InductionVariable {
    /// The value ID of the induction variable (typically a phi node in header).
    pub value: ValueId,
    /// Initial value before loop entry.
    pub init: ValueId,
    /// Step/stride per iteration.
    pub step: ValueId,
    /// The instruction that computes the next value.
    pub update_inst: Option<InstId>,
}

/// Result of loop analysis for a function.
#[derive(Clone, Debug, Default)]
pub struct LoopInfo {
    /// All loops in the function, keyed by header block.
    pub loops: FxHashMap<BlockId, Loop>,
    /// Mapping from block to the innermost loop containing it.
    pub block_to_loop: FxHashMap<BlockId, BlockId>,
}

impl LoopInfo {
    /// Returns true if the block is in any loop.
    #[must_use]
    pub fn is_in_loop(&self, block: BlockId) -> bool {
        self.block_to_loop.contains_key(&block)
    }

    /// Returns the loop containing the given block, if any.
    #[must_use]
    pub fn get_loop(&self, block: BlockId) -> Option<&Loop> {
        self.block_to_loop.get(&block).and_then(|header| self.loops.get(header))
    }

    /// Returns all loops in the function.
    pub fn all_loops(&self) -> impl Iterator<Item = &Loop> {
        self.loops.values()
    }
}

/// Loop analyzer that detects and analyzes loops in MIR functions.
#[derive(Debug, Default)]
pub struct LoopAnalyzer {
    /// Dominators: for each block, the set of blocks that dominate it.
    dominators: FxHashMap<BlockId, FxHashSet<BlockId>>,
}

impl LoopAnalyzer {
    /// Creates a new loop analyzer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Analyzes loops in a function.
    pub fn analyze(&mut self, func: &Function) -> LoopInfo {
        let mut info = LoopInfo::default();

        self.compute_dominators(func);
        let loops = self.find_natural_loops(func);

        for mut loop_info in loops {
            self.find_exit_blocks(func, &mut loop_info);
            self.find_preheader(func, &mut loop_info);
            self.analyze_induction_vars(func, &mut loop_info);
            self.find_invariant_instructions(func, &mut loop_info);
            self.analyze_trip_count(func, &mut loop_info);

            for &block in &loop_info.blocks {
                info.block_to_loop.insert(block, loop_info.header);
            }
            info.loops.insert(loop_info.header, loop_info);
        }

        info
    }

    fn compute_dominators(&mut self, func: &Function) {
        self.dominators.clear();
        let all_blocks: FxHashSet<BlockId> = func.blocks.indices().collect();

        for (block_id, _) in func.blocks.iter_enumerated() {
            if block_id == func.entry_block {
                let mut doms = FxHashSet::default();
                doms.insert(block_id);
                self.dominators.insert(block_id, doms);
            } else {
                self.dominators.insert(block_id, all_blocks.clone());
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for (block_id, block) in func.blocks.iter_enumerated() {
                if block_id == func.entry_block {
                    continue;
                }

                let mut new_doms: Option<FxHashSet<BlockId>> = None;
                for &pred in &block.predecessors {
                    if let Some(pred_doms) = self.dominators.get(&pred) {
                        match &mut new_doms {
                            Some(doms) => doms.retain(|b| pred_doms.contains(b)),
                            None => new_doms = Some(pred_doms.clone()),
                        }
                    }
                }

                let mut new_doms = new_doms.unwrap_or_default();
                new_doms.insert(block_id);

                if self.dominators.get(&block_id) != Some(&new_doms) {
                    self.dominators.insert(block_id, new_doms);
                    changed = true;
                }
            }
        }
    }

    fn find_natural_loops(&self, func: &Function) -> Vec<Loop> {
        let mut loops: FxHashMap<BlockId, Loop> = FxHashMap::default();

        for (block_id, block) in func.blocks.iter_enumerated() {
            if let Some(term) = &block.terminator {
                for succ in term.successors() {
                    if let Some(doms) = self.dominators.get(&block_id) {
                        if doms.contains(&succ) {
                            let loop_info = loops.entry(succ).or_insert_with(|| Loop {
                                header: succ,
                                blocks: FxHashSet::default(),
                                back_edges: SmallVec::new(),
                                exit_blocks: SmallVec::new(),
                                preheader: None,
                                induction_vars: Vec::new(),
                                invariant_insts: FxHashSet::default(),
                                trip_count: None,
                            });
                            loop_info.back_edges.push(block_id);
                            self.collect_loop_blocks(func, succ, block_id, &mut loop_info.blocks);
                        }
                    }
                }
            }
        }

        loops.into_values().collect()
    }

    fn collect_loop_blocks(
        &self,
        func: &Function,
        header: BlockId,
        back_edge_src: BlockId,
        blocks: &mut FxHashSet<BlockId>,
    ) {
        blocks.insert(header);
        let mut worklist = vec![back_edge_src];
        while let Some(block) = worklist.pop() {
            if blocks.insert(block) {
                for &pred in &func.blocks[block].predecessors {
                    if !blocks.contains(&pred) {
                        worklist.push(pred);
                    }
                }
            }
        }
    }

    fn find_exit_blocks(&self, func: &Function, loop_info: &mut Loop) {
        for &block_id in &loop_info.blocks {
            if let Some(term) = &func.blocks[block_id].terminator {
                for succ in term.successors() {
                    if !loop_info.blocks.contains(&succ) && !loop_info.exit_blocks.contains(&succ) {
                        loop_info.exit_blocks.push(succ);
                    }
                }
            }
        }
    }

    fn find_preheader(&self, func: &Function, loop_info: &mut Loop) {
        let header_preds: Vec<BlockId> = func.blocks[loop_info.header]
            .predecessors
            .iter()
            .filter(|&&pred| !loop_info.blocks.contains(&pred))
            .copied()
            .collect();

        if header_preds.len() == 1 {
            loop_info.preheader = Some(header_preds[0]);
        }
    }

    fn analyze_induction_vars(&self, func: &Function, loop_info: &mut Loop) {
        for &inst_id in &func.blocks[loop_info.header].instructions {
            let inst = &func.instructions[inst_id];

            if let InstKind::Phi(incoming) = &inst.kind {
                let mut init_value: Option<ValueId> = None;
                let mut step_value: Option<ValueId> = None;

                for &(block, value) in incoming {
                    if loop_info.blocks.contains(&block) {
                        step_value = Some(value);
                    } else {
                        init_value = Some(value);
                    }
                }

                if let (Some(init), Some(step_val)) = (init_value, step_value) {
                    let phi_value = self.find_result_value(func, inst_id);
                    if let Some(phi_val) = phi_value {
                        if let Some(update_inst) =
                            self.find_update_instruction(func, phi_val, step_val)
                        {
                            if let Some(step_amount) =
                                self.get_step_amount(func, update_inst, phi_val)
                            {
                                loop_info.induction_vars.push(InductionVariable {
                                    value: phi_val,
                                    init,
                                    step: step_amount,
                                    update_inst: Some(update_inst),
                                });
                            }
                        }
                    }
                }
            }
        }
    }

    fn find_update_instruction(
        &self,
        func: &Function,
        phi_val: ValueId,
        step_val: ValueId,
    ) -> Option<InstId> {
        if let Value::Inst(inst_id) = &func.values[step_val] {
            let inst = &func.instructions[*inst_id];
            match &inst.kind {
                InstKind::Add(a, b) if *a == phi_val || *b == phi_val => return Some(*inst_id),
                InstKind::Sub(a, _) if *a == phi_val => return Some(*inst_id),
                _ => {}
            }
        }
        None
    }

    fn get_step_amount(
        &self,
        func: &Function,
        inst_id: InstId,
        phi_val: ValueId,
    ) -> Option<ValueId> {
        let inst = &func.instructions[inst_id];
        match &inst.kind {
            InstKind::Add(a, b) => Some(if *a == phi_val { *b } else { *a }),
            InstKind::Sub(_, b) => Some(*b),
            _ => None,
        }
    }

    fn find_invariant_instructions(&self, func: &Function, loop_info: &mut Loop) {
        let mut invariant_values: FxHashSet<ValueId> = FxHashSet::default();

        for (value_id, value) in func.values.iter_enumerated() {
            match value {
                Value::Immediate(_) | Value::Arg { .. } => {
                    invariant_values.insert(value_id);
                }
                Value::Inst(inst_id) => {
                    let in_loop = loop_info
                        .blocks
                        .iter()
                        .any(|&block| func.blocks[block].instructions.contains(inst_id));
                    if !in_loop {
                        invariant_values.insert(value_id);
                    }
                }
                Value::Phi { .. } | Value::Undef(_) => {}
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for &block_id in &loop_info.blocks {
                for &inst_id in &func.blocks[block_id].instructions {
                    let inst = &func.instructions[inst_id];

                    if loop_info.invariant_insts.contains(&inst_id) {
                        continue;
                    }
                    if inst.kind.has_side_effects() {
                        continue;
                    }
                    if matches!(inst.kind, InstKind::Phi(_)) {
                        continue;
                    }

                    let operands = inst.kind.operands();
                    if operands.iter().all(|op| invariant_values.contains(op)) {
                        loop_info.invariant_insts.insert(inst_id);
                        if let Some(result) = self.find_result_value(func, inst_id) {
                            invariant_values.insert(result);
                        }
                        changed = true;
                    }
                }
            }
        }
    }

    fn analyze_trip_count(&self, func: &Function, loop_info: &mut Loop) {
        if loop_info.induction_vars.len() != 1 {
            return;
        }

        let iv = &loop_info.induction_vars[0];

        let init = match &func.values[iv.init] {
            Value::Immediate(imm) => imm.as_u256(),
            _ => return,
        };

        let step = match &func.values[iv.step] {
            Value::Immediate(imm) => imm.as_u256(),
            _ => return,
        };

        let (Some(init), Some(step)) = (init, step) else { return };

        if step.is_zero() {
            return;
        }

        if let Some(bound) = self.find_loop_bound(func, loop_info, iv.value) {
            if bound >= init {
                let diff = bound - init;
                let trip = diff / step;
                loop_info.trip_count = trip.try_into().ok();
            }
        }
    }

    fn find_loop_bound(
        &self,
        func: &Function,
        loop_info: &Loop,
        iv_value: ValueId,
    ) -> Option<alloy_primitives::U256> {
        for &block_id in &loop_info.blocks {
            if let Some(Terminator::Branch { condition, .. }) = &func.blocks[block_id].terminator {
                if let Value::Inst(cond_inst) = &func.values[*condition] {
                    let inst = &func.instructions[*cond_inst];
                    match &inst.kind {
                        InstKind::Lt(a, b) if *a == iv_value => {
                            if let Value::Immediate(imm) = &func.values[*b] {
                                return imm.as_u256();
                            }
                        }
                        InstKind::Gt(a, b) if *b == iv_value => {
                            if let Value::Immediate(imm) = &func.values[*a] {
                                return imm.as_u256();
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        None
    }

    fn find_result_value(&self, func: &Function, inst_id: InstId) -> Option<ValueId> {
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(id) = value {
                if *id == inst_id {
                    return Some(value_id);
                }
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, Immediate, Value};
    use solar_interface::Ident;

    fn make_test_func() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn test_simple_loop_detection() {
        let mut func = make_test_func();

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let exit = func.alloc_block();

        func.blocks[entry].terminator = Some(Terminator::Jump(header));
        func.blocks[entry].successors.push(header);
        func.blocks[header].predecessors.push(entry);

        let cond = func.alloc_value(Value::Immediate(Immediate::bool(true)));
        func.blocks[header].terminator =
            Some(Terminator::Branch { condition: cond, then_block: body, else_block: exit });
        func.blocks[header].successors.push(body);
        func.blocks[header].successors.push(exit);
        func.blocks[body].predecessors.push(header);
        func.blocks[exit].predecessors.push(header);

        func.blocks[body].terminator = Some(Terminator::Jump(header));
        func.blocks[body].successors.push(header);
        func.blocks[header].predecessors.push(body);

        func.blocks[exit].terminator = Some(Terminator::Stop);

        let mut analyzer = LoopAnalyzer::new();
        let info = analyzer.analyze(&func);

        assert_eq!(info.loops.len(), 1);
        let loop_info = info.loops.get(&header).expect("Loop should have header as key");
        assert!(loop_info.blocks.contains(&header));
        assert!(loop_info.blocks.contains(&body));
        assert!(!loop_info.blocks.contains(&exit));
    }
}

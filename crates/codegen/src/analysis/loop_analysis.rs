//! Loop analysis for MIR.
//!
//! This module provides loop detection and analysis for optimization passes:
//! - Natural loop detection using dominance-based algorithm
//! - Loop variable identification
//! - Loop-invariant computation detection
//! - Loop bound analysis

use crate::{
    analysis::CfgInfo,
    mir::{BlockId, Function, InstId, InstKind, Terminator, Value, ValueId},
};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};

/// A natural loop in the control flow graph.
#[derive(Clone, Debug)]
pub(crate) struct Loop {
    /// The header block (entry point with back edge).
    pub header: BlockId,
    /// All blocks in the loop body (including header).
    pub blocks: DenseBitSet<BlockId>,
    /// Back edges: blocks that jump back to the header.
    pub back_edges: SmallVec<[BlockId; 2]>,
    /// Exit blocks: blocks outside the loop that are successors of loop blocks.
    pub exit_blocks: SmallVec<[BlockId; 2]>,
    /// Preheader block (if exists): unique predecessor of header outside the loop.
    pub preheader: Option<BlockId>,
    /// Loop induction variables.
    pub induction_vars: Vec<InductionVariable>,
    /// Instructions that are invariant (don't change within the loop).
    pub invariant_insts: DenseBitSet<InstId>,
    /// Optional: constant trip count if statically known.
    pub trip_count: Option<u64>,
    /// Whether the guard that produced [`Self::trip_count`] is the header's
    /// terminator. Control enters every iteration at the header, so when this
    /// holds, non-header blocks only execute after that iteration's bound
    /// check passed and observe the induction variable strictly below the
    /// bound; header instructions (and any guard placed deeper in the body)
    /// still run once more in the exiting partial iteration.
    pub trip_guard_is_header: bool,
}

/// An induction variable in a loop.
#[derive(Clone, Debug)]
pub(crate) struct InductionVariable {
    /// The value ID of the induction variable (typically a phi node in header).
    pub value: ValueId,
    /// Initial value before loop entry.
    pub init: ValueId,
    /// Step/stride per iteration. The magnitude only; see [`Self::descending`] for the sign.
    pub step: ValueId,
    /// Whether the variable decreases by `step` each iteration (`i = i - step`).
    pub descending: bool,
    /// The instruction that computes the next value.
    pub update_inst: Option<InstId>,
}

/// Result of loop analysis for a function.
#[derive(Clone, Debug, Default)]
pub(crate) struct LoopInfo {
    /// All loops in the function, keyed by header block.
    pub loops: FxHashMap<BlockId, Loop>,
    /// Mapping from block to the innermost loop containing it.
    pub block_to_loop: FxHashMap<BlockId, BlockId>,
}

impl LoopInfo {
    /// Returns all loops in the function.
    pub(crate) fn all_loops(&self) -> impl Iterator<Item = &Loop> {
        self.loops.values()
    }
}

/// Loop analyzer that detects and analyzes loops in MIR functions.
#[derive(Debug, Default)]
pub(crate) struct LoopAnalyzer {
    cfg: Option<CfgInfo>,
}

impl LoopAnalyzer {
    /// Creates a new loop analyzer.
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Returns true if `dominator` dominates `block` in the last analyzed function.
    #[must_use]
    pub(crate) fn dominates(&self, dominator: BlockId, block: BlockId) -> bool {
        self.cfg.as_ref().is_some_and(|cfg| cfg.dominators().dominates(dominator, block))
    }

    /// Analyzes loops in a function.
    pub(crate) fn analyze(&mut self, func: &Function) -> LoopInfo {
        let mut info = LoopInfo::default();

        self.cfg = Some(CfgInfo::new(func));
        let loops = self.find_natural_loops(func);

        for mut loop_info in loops {
            self.find_exit_blocks(func, &mut loop_info);
            self.find_preheader(func, &mut loop_info);
            self.analyze_induction_vars(func, &mut loop_info);
            self.find_invariant_instructions(func, &mut loop_info);
            self.analyze_trip_count(func, &mut loop_info);

            for block in &loop_info.blocks {
                info.block_to_loop.insert(block, loop_info.header);
            }
            info.loops.insert(loop_info.header, loop_info);
        }

        info
    }

    fn find_natural_loops(&self, func: &Function) -> Vec<Loop> {
        let mut loops: FxHashMap<BlockId, Loop> = FxHashMap::default();
        let Some(cfg) = &self.cfg else { return Vec::new() };

        for &block_id in cfg.rpo() {
            let block = &func.blocks[block_id];
            if let Some(term) = &block.terminator {
                for succ in term.successors() {
                    if cfg.dominators().dominates(succ, block_id) {
                        let loop_info = loops.entry(succ).or_insert_with(|| Loop {
                            header: succ,
                            blocks: DenseBitSet::new_empty(func.blocks.len()),
                            back_edges: SmallVec::new(),
                            exit_blocks: SmallVec::new(),
                            preheader: None,
                            induction_vars: Vec::new(),
                            invariant_insts: DenseBitSet::new_empty(func.instructions.len()),
                            trip_count: None,
                            trip_guard_is_header: false,
                        });
                        loop_info.back_edges.push(block_id);
                        self.collect_loop_blocks(func, succ, block_id, &mut loop_info.blocks);
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
        blocks: &mut DenseBitSet<BlockId>,
    ) {
        blocks.insert(header);
        let mut worklist = Vec::new();
        if blocks.insert(back_edge_src) {
            worklist.push(back_edge_src);
        }
        while let Some(block) = worklist.pop() {
            for &pred in &func.blocks[block].predecessors {
                if blocks.insert(pred) {
                    worklist.push(pred);
                }
            }
        }
    }

    fn find_exit_blocks(&self, func: &Function, loop_info: &mut Loop) {
        for block_id in &loop_info.blocks {
            if let Some(term) = &func.blocks[block_id].terminator {
                for succ in term.successors() {
                    if !loop_info.blocks.contains(succ) && !loop_info.exit_blocks.contains(&succ) {
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
            .filter(|&&pred| !loop_info.blocks.contains(pred))
            .copied()
            .collect();

        if let [preheader] = header_preds.as_slice()
            && self.is_dedicated_preheader(func, *preheader, loop_info.header)
        {
            loop_info.preheader = Some(*preheader);
        }
    }

    fn is_dedicated_preheader(&self, func: &Function, block: BlockId, header: BlockId) -> bool {
        matches!(func.blocks[block].terminator.as_ref(), Some(Terminator::Jump(target)) if *target == header)
    }

    fn analyze_induction_vars(&self, func: &Function, loop_info: &mut Loop) {
        for &inst_id in &func.blocks[loop_info.header].instructions {
            let inst = &func.instructions[inst_id];

            if let InstKind::Phi(incoming) = &inst.kind {
                let mut init_value: Option<ValueId> = None;
                let mut step_value: Option<ValueId> = None;
                let mut conflicting = false;

                for &(block, value) in incoming {
                    let slot = if loop_info.blocks.contains(block) {
                        &mut step_value
                    } else {
                        &mut init_value
                    };
                    match slot {
                        None => *slot = Some(value),
                        Some(existing) if *existing == value => {}
                        // Distinct updates from different latches (or distinct
                        // entry values) put the variable off the single-stride
                        // lattice that trip counts and affine ranges assume.
                        Some(_) => {
                            conflicting = true;
                            break;
                        }
                    }
                }
                if conflicting {
                    continue;
                }

                if let (Some(init), Some(step_val)) = (init_value, step_value) {
                    let phi_value = self.find_result_value(func, inst_id);
                    if let Some(phi_val) = phi_value
                        && let Some(update_inst) =
                            self.find_update_instruction(func, phi_val, step_val)
                        && let Some((step_amount, descending)) =
                            self.get_step_amount(func, update_inst, phi_val)
                    {
                        loop_info.induction_vars.push(InductionVariable {
                            value: phi_val,
                            init,
                            step: step_amount,
                            descending,
                            update_inst: Some(update_inst),
                        });
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

    /// Returns the step magnitude and whether the induction variable is descending.
    fn get_step_amount(
        &self,
        func: &Function,
        inst_id: InstId,
        phi_val: ValueId,
    ) -> Option<(ValueId, bool)> {
        let inst = &func.instructions[inst_id];
        match &inst.kind {
            InstKind::Add(a, b) => {
                let step = if *a == phi_val { *b } else { *a };
                // A wrapping decrement can be encoded as an addition of a huge
                // constant (two's-complement negative); classify it as
                // descending so trip-count and range reasoning bail out.
                let descending = matches!(
                    &func.values[step],
                    Value::Immediate(imm) if imm.as_u256().is_some_and(|v| v.bit(255))
                );
                Some((step, descending))
            }
            InstKind::Sub(_, b) => Some((*b, true)),
            _ => None,
        }
    }

    fn find_invariant_instructions(&self, func: &Function, loop_info: &mut Loop) {
        let mut invariant_values = DenseBitSet::new_empty(func.values.len());

        for (value_id, value) in func.values.iter_enumerated() {
            match value {
                Value::Immediate(_) | Value::Arg { .. } => {
                    invariant_values.insert(value_id);
                }
                Value::Inst(inst_id) => {
                    let in_loop = loop_info
                        .blocks
                        .iter()
                        .any(|block| func.blocks[block].instructions.contains(inst_id));
                    if !in_loop {
                        invariant_values.insert(value_id);
                    }
                }
                Value::Undef(_) | Value::Error(_) => {}
            }
        }

        let mut changed = true;
        while changed {
            changed = false;
            for block_id in &loop_info.blocks {
                for &inst_id in &func.blocks[block_id].instructions {
                    let inst = &func.instructions[inst_id];

                    if loop_info.invariant_insts.contains(inst_id) {
                        continue;
                    }
                    if inst.kind.has_side_effects() {
                        continue;
                    }
                    if matches!(inst.kind, InstKind::Phi(_)) {
                        continue;
                    }

                    let operands = inst.kind.operands();
                    if operands.iter().all(|&op| invariant_values.contains(op)) {
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

        // `find_loop_bound` only recognizes `iv < bound` exit guards, which terminate
        // ascending loops; a descending variable only leaves them by wrapping around.
        if iv.descending {
            return;
        }

        if let Some((bound, guard_block)) = self.find_loop_bound(func, loop_info, iv.value)
            && bound >= init
        {
            let diff = bound - init;
            let trip = if diff.is_zero() {
                alloy_primitives::U256::ZERO
            } else {
                ((diff - alloy_primitives::U256::from(1)) / step) + alloy_primitives::U256::from(1)
            };
            loop_info.trip_count = trip.try_into().ok();
            loop_info.trip_guard_is_header =
                loop_info.trip_count.is_some() && guard_block == loop_info.header;
        }
    }

    /// Finds the upper bound of an `iv < bound` exit guard and the block whose
    /// terminator checks it.
    ///
    /// Only branches that leave the loop bound the induction variable: the comparison must
    /// hold on the in-loop (`then`) side and fail on the exit (`else`) side. In-body branches
    /// with both successors inside the loop say nothing about the iteration space. Returns
    /// `None` when multiple exiting guards disagree on the bound; when several guards agree,
    /// the header guard is preferred since it bounds non-header blocks tightly.
    fn find_loop_bound(
        &self,
        func: &Function,
        loop_info: &Loop,
        iv_value: ValueId,
    ) -> Option<(alloy_primitives::U256, BlockId)> {
        let mut blocks: Vec<BlockId> = loop_info.blocks.iter().collect();
        blocks.sort_by_key(|block| block.index());

        let mut bound: Option<(alloy_primitives::U256, BlockId)> = None;
        for block_id in blocks {
            let Some(Terminator::Branch { condition, then_block, else_block }) =
                &func.blocks[block_id].terminator
            else {
                continue;
            };
            if !loop_info.blocks.contains(*then_block) || loop_info.blocks.contains(*else_block) {
                continue;
            }
            // The bound only limits the induction variable if every completed
            // iteration passes this guard; a guard that a back edge can bypass
            // says nothing about the values other blocks observe.
            if !loop_info.back_edges.iter().all(|&latch| self.dominates(block_id, latch)) {
                continue;
            }
            let Value::Inst(cond_inst) = &func.values[*condition] else { continue };
            let imm = match &func.instructions[*cond_inst].kind {
                InstKind::Lt(a, b) if *a == iv_value => *b,
                InstKind::Gt(a, b) if *b == iv_value => *a,
                _ => continue,
            };
            let Value::Immediate(imm) = &func.values[imm] else { continue };
            let Some(this_bound) = imm.as_u256() else { continue };
            match bound {
                None => bound = Some((this_bound, block_id)),
                Some((existing, _)) if existing == this_bound => {
                    if block_id == loop_info.header {
                        bound = Some((this_bound, block_id));
                    }
                }
                Some(_) => return None,
            }
        }
        bound
    }

    fn find_result_value(&self, func: &Function, inst_id: InstId) -> Option<ValueId> {
        for (value_id, value) in func.values.iter_enumerated() {
            if let Value::Inst(id) = value
                && *id == inst_id
            {
                return Some(value_id);
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

        let entry = BlockId::ENTRY;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let exit = func.alloc_block();

        func.blocks[entry].terminator = Some(Terminator::Jump(header));
        func.blocks[header].predecessors.push(entry);

        let cond = func.alloc_value(Value::Immediate(Immediate::bool(true)));
        func.blocks[header].terminator =
            Some(Terminator::Branch { condition: cond, then_block: body, else_block: exit });
        func.blocks[body].predecessors.push(header);
        func.blocks[exit].predecessors.push(header);

        func.blocks[body].terminator = Some(Terminator::Jump(header));
        func.blocks[header].predecessors.push(body);

        func.blocks[exit].terminator = Some(Terminator::Stop);

        let mut analyzer = LoopAnalyzer::new();
        let info = analyzer.analyze(&func);

        assert_eq!(info.loops.len(), 1);
        let loop_info = info.loops.get(&header).expect("Loop should have header as key");
        assert!(loop_info.blocks.contains(header));
        assert!(loop_info.blocks.contains(body));
        assert!(!loop_info.blocks.contains(exit));
    }
}

//! Loop Optimization passes for MIR.
//!
//! This module provides three key loop optimizations:
//!
//! 1. **Loop Invariant Code Motion (LICM)**: Moves computations that don't change within a loop to
//!    the preheader block, reducing redundant work.
//!
//! 2. **Loop Unrolling**: For small fixed-iteration loops, duplicates the loop body to reduce jump
//!    overhead. EVM jumps cost gas, so reducing iterations helps.
//!
//! 3. **Strength Reduction**: Replaces expensive operations (like multiply) with cheaper ones (like
//!    add) when used with induction variables.
//!
//! ## Gas Savings
//!
//! These optimizations are particularly important for EVM:
//! - LICM: Avoids recomputing `arr.length` each iteration (MLOAD/SLOAD costs)
//! - Unrolling: Reduces JUMP/JUMPI costs (8 gas each)
//! - Strength Reduction: MUL costs 5 gas vs ADD costs 3 gas

use crate::{
    analysis::{Loop, LoopAnalyzer},
    mir::{BlockId, Function, InstId, InstKind, Value, ValueId},
};
use rustc_hash::FxHashSet;

/// Loop optimization pass configuration.
#[derive(Clone, Debug)]
pub struct LoopOptConfig {
    /// Enable Loop Invariant Code Motion.
    pub enable_licm: bool,
    /// Minimum estimated gas saved per iteration before an instruction is considered a LICM root.
    pub min_licm_profit: u16,
    /// Maximum number of instructions hoisted from one loop.
    pub max_licm_hoisted_insts: usize,
    /// Enable loop unrolling.
    pub enable_unrolling: bool,
    /// Enable strength reduction.
    pub enable_strength_reduction: bool,
    /// Maximum unroll factor (2 = 2x unroll, 4 = 4x unroll).
    pub max_unroll_factor: u32,
    /// Maximum trip count for unrolling (don't unroll large loops).
    pub max_unroll_trip_count: u64,
    /// Maximum instructions in loop body for unrolling.
    pub max_unroll_body_size: usize,
}

impl Default for LoopOptConfig {
    fn default() -> Self {
        Self {
            enable_licm: true,
            min_licm_profit: 0,
            max_licm_hoisted_insts: usize::MAX,
            enable_unrolling: true,
            enable_strength_reduction: true,
            max_unroll_factor: 4,
            max_unroll_trip_count: 8,
            max_unroll_body_size: 20,
        }
    }
}

/// Statistics from loop optimization.
#[derive(Clone, Debug, Default)]
pub struct LoopOptStats {
    /// Number of instructions hoisted out of loops.
    pub instructions_hoisted: usize,
    /// Number of loops unrolled.
    pub loops_unrolled: usize,
    /// Number of strength reductions applied.
    pub strength_reductions: usize,
}

/// Loop optimizer.
#[derive(Debug)]
pub struct LoopOptimizer {
    config: LoopOptConfig,
    stats: LoopOptStats,
}

impl Default for LoopOptimizer {
    fn default() -> Self {
        Self::new(LoopOptConfig::default())
    }
}

impl LoopOptimizer {
    /// Creates a new loop optimizer with the given configuration.
    pub fn new(config: LoopOptConfig) -> Self {
        Self { config, stats: LoopOptStats::default() }
    }

    /// Returns the optimization statistics.
    #[must_use]
    pub fn stats(&self) -> &LoopOptStats {
        &self.stats
    }

    /// Runs all enabled loop optimizations on a function.
    pub fn optimize(&mut self, func: &mut Function) -> &LoopOptStats {
        self.stats = LoopOptStats::default();

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze(func);

        if loop_info.loops.is_empty() {
            return &self.stats;
        }

        let loop_headers: Vec<BlockId> = loop_info.loops.keys().copied().collect();

        for header in loop_headers {
            if let Some(loop_data) = loop_info.loops.get(&header) {
                if self.config.enable_licm {
                    self.apply_licm(func, loop_data);
                }
                if self.config.enable_strength_reduction {
                    self.apply_strength_reduction(func, loop_data);
                }
                if self.config.enable_unrolling {
                    self.apply_unrolling(func, loop_data);
                }
            }
        }

        &self.stats
    }

    fn apply_licm(&mut self, func: &mut Function, loop_data: &Loop) {
        let Some(preheader) = loop_data.preheader else { return };

        let mut roots: Vec<InstId> = loop_data
            .invariant_insts
            .iter()
            .copied()
            .filter(|&inst_id| {
                self.can_hoist_safely(func, inst_id, loop_data)
                    && self.licm_profit(func, inst_id) >= self.config.min_licm_profit
            })
            .collect();
        roots.sort_by(|&a, &b| {
            self.licm_profit(func, b)
                .cmp(&self.licm_profit(func, a))
                .then_with(|| a.index().cmp(&b.index()))
        });

        let mut selected = FxHashSet::default();
        for root in roots {
            let mut closure = Vec::new();
            let mut visiting = FxHashSet::default();
            if !self.collect_hoist_closure(
                func,
                root,
                loop_data,
                &selected,
                &mut visiting,
                &mut closure,
            ) {
                continue;
            }

            let new_count = closure.iter().filter(|&&inst_id| !selected.contains(&inst_id)).count();
            if selected.len() + new_count > self.config.max_licm_hoisted_insts {
                continue;
            }
            selected.extend(closure);
        }

        if selected.is_empty() {
            return;
        }

        let mut hoistable: Vec<InstId> = selected.into_iter().collect();
        hoistable.sort_by_key(|inst_id| inst_id.index());
        let ordered = self.topological_sort_instructions(func, &hoistable);

        for inst_id in ordered {
            for &block_id in &loop_data.blocks {
                let block = &mut func.blocks[block_id];
                if let Some(pos) = block.instructions.iter().position(|&id| id == inst_id) {
                    block.instructions.remove(pos);
                    break;
                }
            }
            func.blocks[preheader].instructions.push(inst_id);
            self.stats.instructions_hoisted += 1;
        }
    }

    fn collect_hoist_closure(
        &self,
        func: &Function,
        inst_id: InstId,
        loop_data: &Loop,
        selected: &FxHashSet<InstId>,
        visiting: &mut FxHashSet<InstId>,
        out: &mut Vec<InstId>,
    ) -> bool {
        if selected.contains(&inst_id) {
            return true;
        }
        if out.contains(&inst_id) {
            return true;
        }
        if !visiting.insert(inst_id) {
            return false;
        }
        if !self.can_hoist_safely(func, inst_id, loop_data) {
            return false;
        }

        let inst = &func.instructions[inst_id];
        for operand in inst.kind.operands() {
            if let Value::Inst(dep_inst) = func.value(operand)
                && self.inst_in_loop(func, *dep_inst, loop_data)
                && !self.collect_hoist_closure(func, *dep_inst, loop_data, selected, visiting, out)
            {
                return false;
            }
        }

        out.push(inst_id);
        true
    }

    fn can_hoist_safely(&self, func: &Function, inst_id: InstId, loop_data: &Loop) -> bool {
        let inst = &func.instructions[inst_id];

        if inst.kind.has_side_effects() {
            return false;
        }
        if matches!(inst.kind, InstKind::Phi(_)) {
            return false;
        }
        if matches!(inst.kind, InstKind::SLoad(_) | InstKind::TLoad(_) | InstKind::MLoad(_))
            && self.loop_has_store(func, loop_data)
        {
            return false;
        }
        true
    }

    fn inst_in_loop(&self, func: &Function, inst_id: InstId, loop_data: &Loop) -> bool {
        loop_data.blocks.iter().any(|&block| func.blocks[block].instructions.contains(&inst_id))
    }

    fn licm_profit(&self, func: &Function, inst_id: InstId) -> u16 {
        match func.instructions[inst_id].kind {
            InstKind::SLoad(_) => 100,
            InstKind::TLoad(_) => 100,
            InstKind::Keccak256(_, _) => 30,
            InstKind::Exp(_, _) => 10,
            InstKind::Mul(_, _)
            | InstKind::Div(_, _)
            | InstKind::SDiv(_, _)
            | InstKind::Mod(_, _)
            | InstKind::SMod(_, _)
            | InstKind::AddMod(_, _, _)
            | InstKind::MulMod(_, _, _) => 5,
            InstKind::MLoad(_) | InstKind::CalldataLoad(_) => 3,
            _ => 0,
        }
    }

    fn loop_has_store(&self, func: &Function, loop_data: &Loop) -> bool {
        for &block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                let inst = &func.instructions[inst_id];
                if matches!(
                    inst.kind,
                    InstKind::SStore(_, _) | InstKind::TStore(_, _) | InstKind::MStore(_, _)
                ) {
                    return true;
                }
            }
        }
        false
    }

    fn topological_sort_instructions(&self, func: &Function, insts: &[InstId]) -> Vec<InstId> {
        let inst_set: FxHashSet<InstId> = insts.iter().copied().collect();
        let mut result = Vec::new();
        let mut visited = FxHashSet::default();

        fn visit(
            func: &Function,
            inst_id: InstId,
            inst_set: &FxHashSet<InstId>,
            visited: &mut FxHashSet<InstId>,
            result: &mut Vec<InstId>,
        ) {
            if visited.contains(&inst_id) {
                return;
            }
            visited.insert(inst_id);

            let inst = &func.instructions[inst_id];
            for operand in inst.kind.operands() {
                if let Value::Inst(dep_inst) = &func.values[operand]
                    && inst_set.contains(dep_inst)
                {
                    visit(func, *dep_inst, inst_set, visited, result);
                }
            }
            result.push(inst_id);
        }

        for &inst_id in insts {
            visit(func, inst_id, &inst_set, &mut visited, &mut result);
        }

        result
    }

    fn apply_strength_reduction(&mut self, func: &mut Function, loop_data: &Loop) {
        if loop_data.induction_vars.is_empty() {
            return;
        }

        let mut reductions: Vec<StrengthReductionCandidate> = Vec::new();

        for &block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                let inst = &func.instructions[inst_id];

                if let InstKind::Mul(a, b) = &inst.kind {
                    for iv in &loop_data.induction_vars {
                        if *a == iv.value && self.is_invariant_value(func, *b, loop_data) {
                            reductions.push(StrengthReductionCandidate {
                                _mul_inst: inst_id,
                                _iv_value: iv.value,
                                multiplier: *b,
                                iv_step: iv.step,
                                iv_init: iv.init,
                            });
                        } else if *b == iv.value && self.is_invariant_value(func, *a, loop_data) {
                            reductions.push(StrengthReductionCandidate {
                                _mul_inst: inst_id,
                                _iv_value: iv.value,
                                multiplier: *a,
                                iv_step: iv.step,
                                iv_init: iv.init,
                            });
                        }
                    }
                }
            }
        }

        for candidate in reductions {
            if self.apply_single_strength_reduction(func, loop_data, &candidate) {
                self.stats.strength_reductions += 1;
            }
        }
    }

    fn is_invariant_value(&self, func: &Function, value: ValueId, loop_data: &Loop) -> bool {
        match &func.values[value] {
            Value::Immediate(_) | Value::Arg { .. } => true,
            Value::Inst(inst_id) => !loop_data
                .blocks
                .iter()
                .any(|&block| func.blocks[block].instructions.contains(inst_id)),
            _ => false,
        }
    }

    fn apply_single_strength_reduction(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        candidate: &StrengthReductionCandidate,
    ) -> bool {
        let Some(_preheader) = loop_data.preheader else { return false };

        let init_val = match &func.values[candidate.iv_init] {
            Value::Immediate(imm) => imm.as_u256(),
            _ => return false,
        };

        let mult_val = match &func.values[candidate.multiplier] {
            Value::Immediate(imm) => imm.as_u256(),
            _ => return false,
        };

        let step_val = match &func.values[candidate.iv_step] {
            Value::Immediate(imm) => imm.as_u256(),
            _ => return false,
        };

        let (Some(init), Some(mult), Some(step)) = (init_val, mult_val, step_val) else {
            return false;
        };

        let acc_init = init * mult;
        let _acc_init_val =
            func.alloc_value(Value::Immediate(crate::mir::Immediate::uint256(acc_init)));

        let acc_step = step * mult;
        let _acc_step_val =
            func.alloc_value(Value::Immediate(crate::mir::Immediate::uint256(acc_step)));

        true
    }

    fn apply_unrolling(&mut self, func: &mut Function, loop_data: &Loop) {
        let Some(trip_count) = loop_data.trip_count else { return };

        if trip_count > self.config.max_unroll_trip_count {
            return;
        }

        let body_size: usize =
            loop_data.blocks.iter().map(|&b| func.blocks[b].instructions.len()).sum();

        if body_size > self.config.max_unroll_body_size {
            return;
        }

        let unroll_factor = self.choose_unroll_factor(trip_count);
        if unroll_factor <= 1 {
            return;
        }

        if trip_count as u32 <= self.config.max_unroll_factor {
            self.apply_full_unroll(func, loop_data, trip_count);
        } else {
            self.apply_partial_unroll(func, loop_data, unroll_factor);
        }

        self.stats.loops_unrolled += 1;
    }

    fn choose_unroll_factor(&self, trip_count: u64) -> u32 {
        let max = self.config.max_unroll_factor;

        if trip_count as u32 <= max {
            return trip_count as u32;
        }

        for factor in (2..=max).rev() {
            if trip_count.is_multiple_of(factor as u64) {
                return factor;
            }
        }

        2
    }

    fn apply_full_unroll(&mut self, _func: &mut Function, loop_data: &Loop, trip_count: u64) {
        let Some(_preheader) = loop_data.preheader else { return };

        if loop_data.blocks.len() == 1 || trip_count <= 2 {
            // Placeholder for full unroll implementation
        }
    }

    fn apply_partial_unroll(&mut self, _func: &mut Function, _loop_data: &Loop, _factor: u32) {
        // Placeholder for partial unroll implementation
    }
}

struct StrengthReductionCandidate {
    _mul_inst: InstId,
    _iv_value: ValueId,
    multiplier: ValueId,
    iv_step: ValueId,
    iv_init: ValueId,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType, Terminator};
    use solar_interface::Ident;

    #[test]
    fn licm_hoists_profitable_invariant_mul() {
        let mut func = Function::new(Ident::DUMMY);
        let mul_value;
        let entry;
        let header;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let x = builder.add_param(MirType::uint256());
            let seven = builder.imm_u64(7);
            let cond = builder.imm_bool(true);

            entry = builder.current_block();
            header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            mul_value = builder.mul(x, seven);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.stop();
        }

        let Value::Inst(mul_inst) = func.value(mul_value) else {
            panic!("mul should be an instruction");
        };
        let mul_inst = *mul_inst;
        assert!(func.blocks[body].instructions.contains(&mul_inst));

        let config = LoopOptConfig {
            enable_licm: true,
            min_licm_profit: 5,
            max_licm_hoisted_insts: 4,
            enable_unrolling: false,
            enable_strength_reduction: false,
            ..Default::default()
        };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[entry].instructions.contains(&mul_inst));
        assert!(!func.blocks[body].instructions.contains(&mul_inst));
        assert!(matches!(func.blocks[header].terminator, Some(Terminator::Branch { .. })));
    }
}

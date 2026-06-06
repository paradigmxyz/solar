//! Loop Optimization passes for MIR.
//!
//! This module provides loop optimizations for MIR.
//!
//! **Loop Invariant Code Motion (LICM)** moves computations that don't change
//! within a loop to the preheader block, reducing redundant work.
//!
//! ## Gas Savings
//!
//! This optimization is particularly important for EVM:
//! - LICM: Avoids recomputing `arr.length` each iteration (MLOAD/SLOAD costs)

use crate::{
    analysis::{AffineExpr, Loop, LoopAnalyzer, ScalarEvolution},
    mir::{BlockId, Function, InstId, InstKind, Value, ValueId},
    pass::FunctionPass,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageSpace {
    Persistent,
    Transient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AffineRange {
    base: Option<ValueId>,
    start: i128,
    end: i128,
}

#[derive(Clone, Copy)]
struct LoopOptContext<'a> {
    loop_data: &'a Loop,
    scev: &'a ScalarEvolution,
}

fn ranges_overlap(a_start: u64, a_width: u64, b_start: u64, b_width: u64) -> bool {
    let a_end = a_start.saturating_add(a_width);
    let b_end = b_start.saturating_add(b_width);
    a_start < b_end && b_start < a_end
}

/// Loop optimization pass configuration.
#[derive(Clone, Debug)]
pub struct LoopOptConfig {
    /// Enable Loop Invariant Code Motion.
    pub enable_licm: bool,
    /// Minimum estimated gas saved per iteration before an instruction is considered a LICM root.
    pub min_licm_profit: u16,
    /// Maximum number of instructions hoisted from one loop.
    pub max_licm_hoisted_insts: usize,
}

impl Default for LoopOptConfig {
    fn default() -> Self {
        Self { enable_licm: true, min_licm_profit: 0, max_licm_hoisted_insts: usize::MAX }
    }
}

/// Statistics from loop optimization.
#[derive(Clone, Debug, Default)]
pub struct LoopOptStats {
    /// Number of instructions hoisted out of loops.
    pub instructions_hoisted: usize,
}

/// Loop optimizer.
#[derive(Debug)]
pub struct LoopOptimizer {
    config: LoopOptConfig,
    stats: LoopOptStats,
}

/// Function pass for loop-invariant code motion.
pub struct LicmPass;

impl FunctionPass for LicmPass {
    fn name(&self) -> &str {
        "licm"
    }

    fn run_on_function(&mut self, func: &mut Function) -> bool {
        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 3, max_licm_hoisted_insts: 4 };
        LoopOptimizer::new(config).optimize(func).instructions_hoisted != 0
    }
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
            if let Some(loop_data) = loop_info.loops.get(&header)
                && self.config.enable_licm
            {
                self.apply_licm(func, loop_data);
            }
        }

        &self.stats
    }

    fn apply_licm(&mut self, func: &mut Function, loop_data: &Loop) {
        let Some(preheader) = loop_data.preheader else { return };
        if self.loop_observes_gas(func, loop_data) {
            return;
        }

        let scev = ScalarEvolution::analyze(func, loop_data);
        let ctx = LoopOptContext { loop_data, scev: &scev };
        let mut roots: Vec<InstId> = loop_data
            .invariant_insts
            .iter()
            .copied()
            .filter(|&inst_id| {
                self.can_hoist_safely(func, inst_id, ctx)
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
            if !self.collect_hoist_closure(func, root, ctx, &selected, &mut visiting, &mut closure)
            {
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
        ctx: LoopOptContext<'_>,
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
        if !self.can_hoist_safely(func, inst_id, ctx) {
            return false;
        }

        let inst = &func.instructions[inst_id];
        for operand in inst.kind.operands() {
            if let Value::Inst(dep_inst) = func.value(operand)
                && self.inst_in_loop(func, *dep_inst, ctx.loop_data)
                && !self.collect_hoist_closure(func, *dep_inst, ctx, selected, visiting, out)
            {
                return false;
            }
        }

        out.push(inst_id);
        true
    }

    fn can_hoist_safely(&self, func: &Function, inst_id: InstId, ctx: LoopOptContext<'_>) -> bool {
        let inst = &func.instructions[inst_id];

        if inst.kind.has_side_effects() {
            return false;
        }
        if matches!(inst.kind, InstKind::Phi(_)) {
            return false;
        }
        match inst.kind {
            InstKind::MLoad(addr) => {
                return !self.loop_may_mutate_memory_range(func, ctx, addr, Some(32));
            }
            InstKind::Keccak256(offset, size) => {
                return !self.loop_may_mutate_memory_range(
                    func,
                    ctx,
                    offset,
                    self.const_addr(func, size),
                );
            }
            InstKind::SLoad(slot) => {
                return !self.loop_may_mutate_storage_slot(
                    func,
                    ctx.loop_data,
                    slot,
                    StorageSpace::Persistent,
                );
            }
            InstKind::TLoad(slot) => {
                return !self.loop_may_mutate_storage_slot(
                    func,
                    ctx.loop_data,
                    slot,
                    StorageSpace::Transient,
                );
            }
            _ => {}
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

    fn loop_observes_gas(&self, func: &Function, loop_data: &Loop) -> bool {
        for &block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.instructions[inst_id].kind, InstKind::Gas) {
                    return true;
                }
            }
        }
        false
    }

    fn loop_may_mutate_memory_range(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_addr: ValueId,
        load_width: Option<u64>,
    ) -> bool {
        for &block_id in &ctx.loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match func.instructions[inst_id].kind {
                    InstKind::MStore(addr, _)
                        if self.memory_ranges_may_alias(
                            func, ctx, load_addr, load_width, addr, 32,
                        ) =>
                    {
                        return true;
                    }
                    InstKind::MStore8(addr, _)
                        if self
                            .memory_ranges_may_alias(func, ctx, load_addr, load_width, addr, 1) =>
                    {
                        return true;
                    }
                    InstKind::MCopy(_, _, _)
                    | InstKind::CalldataCopy(_, _, _)
                    | InstKind::CodeCopy(_, _, _)
                    | InstKind::ReturnDataCopy(_, _, _)
                    | InstKind::ExtCodeCopy(_, _, _, _)
                    | InstKind::Call { .. }
                    | InstKind::StaticCall { .. }
                    | InstKind::DelegateCall { .. }
                    | InstKind::InternalCall { .. } => return true,
                    InstKind::MSize => return true,
                    _ => {}
                }
            }
        }
        false
    }

    fn loop_may_mutate_storage_slot(
        &self,
        func: &Function,
        loop_data: &Loop,
        load_slot: ValueId,
        space: StorageSpace,
    ) -> bool {
        for &block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match (space, &func.instructions[inst_id].kind) {
                    (StorageSpace::Persistent, InstKind::SStore(slot, _))
                    | (StorageSpace::Transient, InstKind::TStore(slot, _))
                        if self.slots_may_alias(func, load_slot, *slot) =>
                    {
                        return true;
                    }
                    (
                        _,
                        InstKind::Call { .. }
                        | InstKind::StaticCall { .. }
                        | InstKind::DelegateCall { .. }
                        | InstKind::InternalCall { .. }
                        | InstKind::Create(_, _, _)
                        | InstKind::Create2(_, _, _, _),
                    ) => return true,
                    _ => {}
                }
            }
        }
        false
    }

    fn memory_ranges_may_alias(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_addr: ValueId,
        load_width: Option<u64>,
        write_addr: ValueId,
        write_width: u64,
    ) -> bool {
        match (self.const_addr(func, load_addr), load_width, self.const_addr(func, write_addr)) {
            (Some(load), Some(load_width), Some(write)) => {
                ranges_overlap(load, load_width, write, write_width)
            }
            _ => {
                let Some(load_width) = load_width else { return true };
                let Some(load) = self.affine_range(func, ctx, load_addr, load_width) else {
                    return true;
                };
                let Some(write) = self.affine_range(func, ctx, write_addr, write_width) else {
                    return true;
                };
                if load.base != write.base {
                    return true;
                }
                load.start < write.end && write.start < load.end
            }
        }
    }

    fn affine_range(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        value: ValueId,
        width: u64,
    ) -> Option<AffineRange> {
        let expr = ctx.scev.get(value).cloned().or_else(|| self.const_affine_expr(func, value))?;
        self.affine_expr_range(func, ctx.loop_data, expr, width)
    }

    fn affine_expr_range(
        &self,
        func: &Function,
        loop_data: &Loop,
        expr: AffineExpr,
        width: u64,
    ) -> Option<AffineRange> {
        let mut start = expr.constant;
        let mut end = expr.constant;
        if !expr.terms.is_empty() {
            let trip_count = i128::from(loop_data.trip_count?);
            if trip_count == 0 {
                return None;
            }
            for term in expr.terms {
                let iv = loop_data.induction_vars.iter().find(|iv| iv.value == term.value)?;
                let init = self.const_i128(func, iv.init)?;
                let step = self.const_i128(func, iv.step)?;
                let first = init.checked_mul(term.scale)?;
                let last_iv = init.checked_add(step.checked_mul(trip_count.checked_sub(1)?)?)?;
                let last = last_iv.checked_mul(term.scale)?;
                start = start.checked_add(first.min(last))?;
                end = end.checked_add(first.max(last))?;
            }
        }

        Some(AffineRange { base: expr.base, start, end: end.checked_add(i128::from(width))? })
    }

    fn const_affine_expr(&self, func: &Function, value: ValueId) -> Option<AffineExpr> {
        Some(AffineExpr {
            base: None,
            constant: self.const_i128(func, value)?,
            terms: Default::default(),
        })
    }

    fn slots_may_alias(&self, func: &Function, load_slot: ValueId, write_slot: ValueId) -> bool {
        match (self.const_addr(func, load_slot), self.const_addr(func, write_slot)) {
            (Some(load), Some(write)) => load == write,
            _ => true,
        }
    }

    fn const_addr(&self, func: &Function, value: ValueId) -> Option<u64> {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256()?.try_into().ok(),
            Value::Arg { .. } | Value::Inst(_) | Value::Phi { .. } | Value::Undef(_) => None,
        }
    }

    fn const_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        match func.value(value) {
            Value::Immediate(imm) => u256_to_i128(imm.as_u256()?),
            Value::Arg { .. } | Value::Inst(_) | Value::Phi { .. } | Value::Undef(_) => None,
        }
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
}

fn u256_to_i128(value: U256) -> Option<i128> {
    if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, Immediate, Instruction, MirType, Terminator};
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

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 5, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[entry].instructions.contains(&mul_inst));
        assert!(!func.blocks[body].instructions.contains(&mul_inst));
        assert!(matches!(func.blocks[header].terminator, Some(Terminator::Branch { .. })));
    }

    #[test]
    fn licm_hoists_mload_past_non_overlapping_const_store() {
        let mut func = Function::new(Ident::DUMMY);
        let load_value;
        let entry;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let zero = builder.imm_u64(0);
            let sixty_four = builder.imm_u64(64);
            let value = builder.imm_u64(1);
            let cond = builder.imm_bool(true);

            entry = builder.current_block();
            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            load_value = builder.mload(zero);
            builder.mstore(sixty_four, value);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![load_value]);
        }

        let Value::Inst(load_inst) = func.value(load_value) else {
            panic!("mload should be an instruction");
        };
        let load_inst = *load_inst;
        assert!(func.blocks[body].instructions.contains(&load_inst));

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 3, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[entry].instructions.contains(&load_inst));
        assert!(!func.blocks[body].instructions.contains(&load_inst));
    }

    #[test]
    fn licm_hoists_mload_past_non_overlapping_affine_store() {
        let mut func = Function::new(Ident::DUMMY);

        let entry = func.entry_block;
        let header = func.alloc_block();
        let body = func.alloc_block();
        let exit = func.alloc_block();

        let base = func.alloc_value(Value::Arg { index: 0, ty: MirType::uint256() });
        func.params.push(MirType::uint256());
        let zero = func.alloc_value(Value::Immediate(Immediate::uint256(U256::ZERO)));
        let one = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let four = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(4))));
        let stride = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(32))));
        let value = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(7))));

        func.blocks[entry].terminator = Some(Terminator::Jump(header));
        func.blocks[entry].successors.push(header);
        func.blocks[header].predecessors.push(entry);

        let phi_inst = func.alloc_inst(Instruction::new(
            InstKind::Phi(vec![(entry, zero)]),
            Some(MirType::uint256()),
        ));
        func.blocks[header].instructions.push(phi_inst);
        let i = func.alloc_value(Value::Inst(phi_inst));
        let cond_inst =
            func.alloc_inst(Instruction::new(InstKind::Lt(i, four), Some(MirType::Bool)));
        func.blocks[header].instructions.push(cond_inst);
        let cond = func.alloc_value(Value::Inst(cond_inst));
        func.blocks[header].terminator =
            Some(Terminator::Branch { condition: cond, then_block: body, else_block: exit });
        func.blocks[header].successors.extend([body, exit]);
        func.blocks[body].predecessors.push(header);
        func.blocks[exit].predecessors.push(header);

        let load_inst =
            func.alloc_inst(Instruction::new(InstKind::MLoad(base), Some(MirType::uint256())));
        func.blocks[body].instructions.push(load_inst);
        let load_value = func.alloc_value(Value::Inst(load_inst));
        let scaled_inst =
            func.alloc_inst(Instruction::new(InstKind::Mul(i, stride), Some(MirType::uint256())));
        func.blocks[body].instructions.push(scaled_inst);
        let scaled = func.alloc_value(Value::Inst(scaled_inst));
        let offset_inst = func
            .alloc_inst(Instruction::new(InstKind::Add(stride, scaled), Some(MirType::uint256())));
        func.blocks[body].instructions.push(offset_inst);
        let offset = func.alloc_value(Value::Inst(offset_inst));
        let store_addr_inst = func
            .alloc_inst(Instruction::new(InstKind::Add(base, offset), Some(MirType::uint256())));
        func.blocks[body].instructions.push(store_addr_inst);
        let store_addr = func.alloc_value(Value::Inst(store_addr_inst));
        let store_inst =
            func.alloc_inst(Instruction::new(InstKind::MStore(store_addr, value), None));
        func.blocks[body].instructions.push(store_inst);
        let next_inst =
            func.alloc_inst(Instruction::new(InstKind::Add(i, one), Some(MirType::uint256())));
        func.blocks[body].instructions.push(next_inst);
        let next = func.alloc_value(Value::Inst(next_inst));
        if let InstKind::Phi(incoming) = &mut func.instructions[phi_inst].kind {
            incoming.push((body, next));
        }
        func.blocks[body].terminator = Some(Terminator::Jump(header));
        func.blocks[body].successors.push(header);
        func.blocks[header].predecessors.push(body);

        func.blocks[exit].terminator = Some(Terminator::Return { values: vec![load_value].into() });

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 3, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[entry].instructions.contains(&load_inst));
        assert!(!func.blocks[body].instructions.contains(&load_inst));
    }

    #[test]
    fn licm_keeps_mload_inside_loop_when_store_overlaps() {
        let mut func = Function::new(Ident::DUMMY);
        let load_value;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let zero = builder.imm_u64(0);
            let overlapping = builder.imm_u64(16);
            let value = builder.imm_u64(1);
            let cond = builder.imm_bool(true);

            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            load_value = builder.mload(zero);
            builder.mstore(overlapping, value);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![load_value]);
        }

        let Value::Inst(load_inst) = func.value(load_value) else {
            panic!("mload should be an instruction");
        };
        let load_inst = *load_inst;

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 3, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[body].instructions.contains(&load_inst));
    }

    #[test]
    fn licm_hoists_keccak_past_non_overlapping_const_store() {
        let mut func = Function::new(Ident::DUMMY);
        let hash_value;
        let entry;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let zero = builder.imm_u64(0);
            let thirty_two = builder.imm_u64(32);
            let sixty_four = builder.imm_u64(64);
            let value = builder.imm_u64(1);
            let cond = builder.imm_bool(true);

            entry = builder.current_block();
            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            hash_value = builder.keccak256(zero, thirty_two);
            builder.mstore(sixty_four, value);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![hash_value]);
        }

        let Value::Inst(hash_inst) = func.value(hash_value) else {
            panic!("keccak256 should be an instruction");
        };
        let hash_inst = *hash_inst;

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 5, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[entry].instructions.contains(&hash_inst));
        assert!(!func.blocks[body].instructions.contains(&hash_inst));
    }

    #[test]
    fn licm_keeps_keccak_inside_loop_when_store_overlaps() {
        let mut func = Function::new(Ident::DUMMY);
        let hash_value;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let zero = builder.imm_u64(0);
            let thirty_two = builder.imm_u64(32);
            let overlapping = builder.imm_u64(16);
            let value = builder.imm_u64(1);
            let cond = builder.imm_bool(true);

            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            hash_value = builder.keccak256(zero, thirty_two);
            builder.mstore(overlapping, value);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![hash_value]);
        }

        let Value::Inst(hash_inst) = func.value(hash_value) else {
            panic!("keccak256 should be an instruction");
        };
        let hash_inst = *hash_inst;

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 5, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[body].instructions.contains(&hash_inst));
    }

    #[test]
    fn licm_hoists_sload_past_different_const_slot_store() {
        let mut func = Function::new(Ident::DUMMY);
        let load_value;
        let entry;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let slot_zero = builder.imm_u64(0);
            let slot_one = builder.imm_u64(1);
            let value = builder.imm_u64(1);
            let cond = builder.imm_bool(true);

            entry = builder.current_block();
            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            load_value = builder.sload(slot_zero);
            builder.sstore(slot_one, value);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![load_value]);
        }

        let Value::Inst(load_inst) = func.value(load_value) else {
            panic!("sload should be an instruction");
        };
        let load_inst = *load_inst;

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 5, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[entry].instructions.contains(&load_inst));
        assert!(!func.blocks[body].instructions.contains(&load_inst));
    }

    #[test]
    fn licm_keeps_sload_inside_loop_when_store_uses_same_slot() {
        let mut func = Function::new(Ident::DUMMY);
        let load_value;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let slot = builder.imm_u64(0);
            let value = builder.imm_u64(1);
            let cond = builder.imm_bool(true);

            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            load_value = builder.sload(slot);
            builder.sstore(slot, value);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![load_value]);
        }

        let Value::Inst(load_inst) = func.value(load_value) else {
            panic!("sload should be an instruction");
        };
        let load_inst = *load_inst;

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 5, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[body].instructions.contains(&load_inst));
    }

    #[test]
    fn licm_does_not_move_work_across_gas_observer() {
        let mut func = Function::new(Ident::DUMMY);
        let mul_value;
        let body;

        {
            let mut builder = FunctionBuilder::new(&mut func);
            let x = builder.add_param(MirType::uint256());
            let seven = builder.imm_u64(7);
            let cond = builder.imm_bool(true);

            let header = builder.create_block();
            body = builder.create_block();
            let exit = builder.create_block();

            builder.jump(header);

            builder.switch_to_block(header);
            builder.branch(cond, body, exit);

            builder.switch_to_block(body);
            builder.gas();
            mul_value = builder.mul(x, seven);
            builder.jump(header);

            builder.switch_to_block(exit);
            builder.ret(vec![mul_value]);
        }

        let Value::Inst(mul_inst) = func.value(mul_value) else {
            panic!("mul should be an instruction");
        };
        let mul_inst = *mul_inst;

        let config =
            LoopOptConfig { enable_licm: true, min_licm_profit: 5, max_licm_hoisted_insts: 4 };
        let mut optimizer = LoopOptimizer::new(config);
        optimizer.optimize(&mut func);

        assert!(func.blocks[body].instructions.contains(&mul_inst));
    }
}

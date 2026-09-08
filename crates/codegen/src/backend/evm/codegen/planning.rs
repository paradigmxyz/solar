//! Bounded physical planning and equivalent-expression selection.
//!
//! ISLE proposes word-equivalent expressions, such as equality versus an
//! available subtraction/XOR tested for zero, or arithmetic and bitwise
//! combinations of resident intersections, unions and differences. Each candidate is planned with
//! the real scheduler, including argument/spill reloads, opcode cost and dead
//! word cleanup. Accept only a target-cost improvement that leaves the same
//! residual stack and does not increase its high-water mark. Definitions from
//! previous loop iterations are excluded by same-block availability checks.
//!
//! Equivalent operand orders can leave different residual stacks. Comparing
//! only the current instruction misses the DUPs, SWAPs and reloads those layouts
//! cause at the next use. Replay both plans on cloned scheduler states through
//! up to two following pure instructions, including dead-word cleanup and
//! materialization through the active spill/argument convention. Compare total
//! gas and bytes only when both trials finish at the same residual layout.
//!
//! This is a target selection decision at the lowering boundary; no physical
//! stack state enters MIR. Unsupported operations, pending result spills, live
//! exports and global stack aliases retain the ordinary local planner. The
//! window is bounded independently of function size and never crosses effects
//! or CFG edges. Speculation neither emits instructions nor consumes the real
//! scheduler's search budget.
//! Keep intermediate layouts so a useful one-instruction window survives an
//! unsupported second instruction. Prefer the longest common window that
//! rejoins; a partial trial cannot be compared with a longer baseline.

use super::{
    EvmCodegen, OperandPlan, ScheduleCost,
    select::{self, OpcodeLowering},
};
use crate::{
    backend::evm::op,
    mir::{BlockId, EffectKind, Function, Op, Value, ValueId, analysis::Liveness},
    target::{Cost, Target},
};
use smallvec::SmallVec;

mod isle;

struct ExpressionPlan {
    opcode: u8,
    arity: usize,
    plan: OperandPlan,
    cost: Cost,
    layout: Vec<Option<ValueId>>,
    peak: usize,
}

struct WindowPlan {
    cost: Cost,
    layout: SmallVec<[Option<ValueId>; 8]>,
}

impl EvmCodegen<'_> {
    /// Selects one equivalent expression only when its complete physical plan
    /// improves cost and rejoins the baseline stack without increasing its peak.
    pub(super) fn emit_stack_expression(
        &mut self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
        index: usize,
    ) -> bool {
        if self.global_stack_active || !self.global_stack_aliases.is_empty() {
            return false;
        }
        let inst = func.blocks[block].instructions[index];
        let op = func.inst(inst).kind.op();
        let alternatives = isle::alternatives(func, &self.scheduler.stack, block, index, &op);
        if alternatives.is_empty() {
            return false;
        }
        let Some(result) = func.inst_result_value(inst) else { return false };
        if self.scheduler.spills.get(result).is_some() || liveness.live_out(block).contains(result)
        {
            return false;
        }
        let Some(baseline) = self.expression_plan(func, &op, result, liveness, block, index) else {
            return false;
        };
        let target = Target::new(self.gcx);
        let mut best = None;
        for op in alternatives {
            if let Some(candidate) = self.expression_plan(func, &op, result, liveness, block, index)
                && candidate.layout == baseline.layout
                && candidate.peak <= baseline.peak
                && target.cmp(candidate.cost, baseline.cost).is_lt()
                && best
                    .as_ref()
                    .is_none_or(|old: &ExpressionPlan| target.cmp(candidate.cost, old.cost).is_lt())
            {
                best = Some(candidate);
            }
        }
        let Some(best) = best else { return false };
        tracing::trace!(target: "solar::codegen::evm::planning", function = %func.name,
            before = ?baseline.cost, after = ?best.cost, "select equivalent expression");
        // prepare equivalent operands; selected_opcode -> original_result
        self.emit_operand_plan(func, best.plan);
        self.asm.emit_op(best.opcode);
        self.scheduler.instruction_executed(best.arity, Some(result));
        true
    }

    #[allow(clippy::too_many_arguments)]
    fn expression_plan(
        &self,
        func: &Function,
        op: &Op,
        result: ValueId,
        liveness: &Liveness,
        block: BlockId,
        index: usize,
    ) -> Option<ExpressionPlan> {
        let lowering = select::opcode_lowering(op)?;
        if !matches!(lowering, OpcodeLowering::Unary { .. } | OpcodeLowering::Binary { .. }) {
            return None;
        }
        let mut operands = SmallVec::<[ValueId; 8]>::new();
        let _ = op.map_values(|value| {
            operands.push(value);
            value
        });
        operands.reverse();
        let target = Target::new(self.gcx);
        let mut scheduler = self.scheduler.clone();
        let preserved =
            self.preserved_operands_for(&scheduler, func, &operands, liveness, block, index);
        let mut opcode = lowering.opcode();
        let mut plan = scheduler.plan_operands(
            &operands,
            &preserved,
            func,
            target.optimization(),
            self.operand_cost_model(),
        )?;
        if operands.len() == 2
            && operands[0] != operands[1]
            && let Some(swapped_opcode) = op::swapped_binary_opcode(opcode)
            && let Some(swapped) = scheduler.plan_operands(
                &[operands[1], operands[0]],
                &preserved,
                func,
                target.optimization(),
                self.operand_cost_model(),
            )
            && self.prefer_binary_plan(func, &plan, &swapped, Some(result), liveness, block, index)
        {
            opcode = swapped_opcode;
            plan = swapped;
        }
        let mut cost = plan.cost().target_cost().plus(target.opcode(opcode));
        // prepare operands; opcode; identical dead-word cleanup
        scheduler.apply_operand_plan(plan.clone());
        scheduler.instruction_executed(operands.len(), Some(result));
        for op in scheduler.drop_dead_values(liveness, block, index) {
            cost += ScheduleCost::stack_op(op, target.evm_version()).target_cost();
        }
        Some(ExpressionPlan {
            opcode,
            arity: operands.len(),
            plan,
            cost,
            layout: scheduler.stack.iter().collect(),
            peak: scheduler.stack.max_depth(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub(super) fn prefer_binary_plan(
        &self,
        func: &Function,
        current: &OperandPlan,
        candidate: &OperandPlan,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        index: usize,
    ) -> bool {
        if current.is_free() && candidate.is_free() {
            return false;
        }
        let target = Target::new(self.gcx);
        if let Some(old) = self.binary_window(func, current, result, liveness, block, index)
            && let Some(new) = self.binary_window(func, candidate, result, liveness, block, index)
        {
            for (old, new) in old.iter().zip(&new).rev() {
                if old.layout == new.layout {
                    return target.cmp(new.cost, old.cost).is_lt();
                }
            }
        }
        candidate.cost().cmp_for(current.cost(), target.optimization()).is_lt()
    }

    #[allow(clippy::too_many_arguments)]
    fn binary_window(
        &self,
        func: &Function,
        first: &OperandPlan,
        result: Option<ValueId>,
        liveness: &Liveness,
        block: BlockId,
        index: usize,
    ) -> Option<SmallVec<[WindowPlan; 2]>> {
        if self.global_stack_active
            || self.scheduler.stack.depth() > 8
            || !self.global_stack_aliases.is_empty()
        {
            return None;
        }
        let result = result?;
        if self.scheduler.spills.get(result).is_some() || liveness.live_out(block).contains(result)
        {
            return None;
        }
        let target = Target::new(self.gcx);
        let mut scheduler = self.scheduler.clone();
        let mut cost = first.cost().target_cost();
        // prepare operands; binary_op; drop dead words
        scheduler.apply_operand_plan(first.clone());
        scheduler.instruction_executed(2, Some(result));
        for op in scheduler.drop_dead_values(liveness, block, index) {
            cost += ScheduleCost::stack_op(op, target.evm_version()).target_cost();
        }
        let mut windows = SmallVec::new();
        for (offset, &inst) in
            func.blocks[block].instructions[index + 1..].iter().take(2).enumerate()
        {
            let index = index + 1 + offset;
            let kind = &func.inst(inst).kind;
            let Some(result) = func.inst_result_value(inst) else { break };
            if kind.effect_kind() != EffectKind::Pure
                || scheduler.spills.get(result).is_some()
                || liveness.live_out(block).contains(result)
            {
                break;
            }
            let Some(lowering) = select::opcode_lowering(&kind.op()) else { break };
            if !matches!(lowering, OpcodeLowering::Unary { .. } | OpcodeLowering::Binary { .. }) {
                break;
            }
            let operands = kind.operands();
            if operands.iter().any(|&value| {
                liveness.live_out(block).contains(value)
                    || matches!(func.value(value), Value::Arg(_))
                        && !scheduler.stack.contains(value)
            }) {
                break;
            }
            let order: SmallVec<[ValueId; 8]> = operands.iter().rev().copied().collect();
            let preserved =
                self.preserved_operands_for(&scheduler, func, &order, liveness, block, index);
            let Some(mut plan) = scheduler.plan_operands(
                &order,
                &preserved,
                func,
                target.optimization(),
                self.operand_cost_model(),
            ) else {
                break;
            };
            if order.len() == 2
                && order[0] != order[1]
                && op::swapped_binary_opcode(lowering.opcode()).is_some()
                && let Some(swapped) = scheduler.plan_operands(
                    &[order[1], order[0]],
                    &preserved,
                    func,
                    target.optimization(),
                    self.operand_cost_model(),
                )
                && swapped.cost().cmp_for(plan.cost(), target.optimization()).is_lt()
            {
                plan = swapped;
            }
            cost += plan.cost().target_cost();
            // prepare operands; equivalent opcode; drop dead words
            scheduler.apply_operand_plan(plan);
            scheduler.instruction_executed(operands.len(), Some(result));
            for op in scheduler.drop_dead_values(liveness, block, index) {
                cost += ScheduleCost::stack_op(op, target.evm_version()).target_cost();
            }
            windows.push(WindowPlan { cost, layout: scheduler.stack.iter().collect() });
        }
        (!windows.is_empty()).then_some(windows)
    }
}

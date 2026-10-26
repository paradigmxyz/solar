//! Closed forms for pure additive loops before gas-bounded check elimination.
//!
//! A straight-line loop counting from zero to an invariant unsigned bound
//! by one executes exactly `n` iterations, even at the word's maximum value.
//! Each carried word must add or subtract an invariant, or another carried word
//! that itself adds or subtracts an invariant. Linear recurrences become
//! `a + n * b`; quadratic ones become `a + n * b + n * (n - 1) / 2 * c`, with
//! subtraction signs preserved. Divide the even factor before multiplying:
//! `(n >> 1) * ((n - 1) | 1)` computes the triangular term modulo 2^256 without
//! losing the product's high bit, including for zero iterations.
//!
//! Require only phis and the guard in the header, only recurrence updates in
//! the body, no effects, no other entries or exits, and no escaping body values.
//! Narrow values, checked arithmetic, higher-degree recurrences, and conditional
//! updates are deliberately excluded. The target prices the formulas against
//! the original loop using its trip-count estimate; gas mode alone enables this
//! pass. It runs after loop canonicalization, but before induction-variable
//! rewriting and check elimination: those passes may rely on gas limiting the
//! number of iterations, which no longer holds after replacing a loop. Later
//! CFG and dead-code cleanup remove the old loop and unused final values.
//!
//! NOTE: Formulas summarize several iterations, so their debug locations stay
//! unknown rather than inheriting an arbitrary iteration's source checkpoint.

use crate::{
    backend::evm::op,
    mir::{
        BlockId, Function, FunctionBuilder, InstId, InstKind, MirType, Module, Terminator, Value,
        ValueId,
        analysis::{Loop, LoopAnalyzer},
        pass::{MirPass, ModuleAnalyses, run_function_pass},
        utils::{fold_terminator_to_jump, invalidate_unreachable_block},
    },
    target::{Cost, Target},
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

/// Evaluates linear and quadratic additive recurrences at loop exits.
pub(crate) struct LoopReduce;

impl MirPass for LoopReduce {
    fn name(&self) -> &'static str {
        "loop-reduce"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            run_function(func, analyses.loop_aware_target(gcx))
        })
    }
}

struct Recurrence {
    value: ValueId,
    init: ValueId,
    step: ValueId,
    subtract: bool,
    update: InstId,
}

struct ClosedForm {
    header: BlockId,
    body: Vec<BlockId>,
    preheader: BlockId,
    exit: BlockId,
    count: ValueId,
    recurrences: Vec<Recurrence>,
}

fn run_function(func: &mut Function, target: Target) -> bool {
    let mut changed = false;
    loop {
        let loops = LoopAnalyzer::new().analyze_structure(func);
        let Some(plan) = loops.all_loops().find_map(|l| plan(func, l, target)) else { break };
        rewrite(func, plan);
        let _ = super::cfg_simplify::remove_unreachable_blocks(func);
        changed = true;
    }
    changed
}

fn plan(func: &Function, l: &Loop, target: Target) -> Option<ClosedForm> {
    let header = l.header;
    let [latch] = l.back_edges.as_slice() else { return None };
    let preheader = l.preheader?;
    let [exit] = l.exit_blocks.as_slice() else { return None };
    if !matches!(func.blocks[preheader].terminator, Some(Terminator::Jump(b)) if b == header) {
        return None;
    }
    let body: Vec<_> = l.blocks.iter().filter(|&block| block != header).collect();
    for &block in &body {
        if func.blocks[block].predecessors.len() != 1
            || !matches!(func.blocks[block].terminator,
                Some(Terminator::Jump(next)) if l.blocks.contains(next))
        {
            return None;
        }
    }
    let [phis @ .., guard] = func.blocks[header].instructions.as_slice() else { return None };
    let InstKind::Lt(counter, count) = func.inst(*guard).kind else { return None };
    let condition = func.inst_result_value(*guard)?;
    if !matches!(func.blocks[header].terminator,
        Some(Terminator::Branch { condition: c, then_block, else_block })
        if c == condition && body.contains(&then_block) && else_block == *exit)
        || !defined_outside(func, l, count)
        || escapes(func, l, condition)
    {
        return None;
    }
    let mut recurrences = Vec::new();
    for &phi in phis {
        let InstKind::Phi(incoming) = &func.inst(phi).kind else { return None };
        if incoming.len() != 2 {
            return None;
        }
        let value = func.inst_result_value(phi)?;
        let init = incoming.iter().find(|(block, _)| *block == preheader)?.1;
        let next = incoming.iter().find(|(block, _)| block == latch)?.1;
        let Value::Inst(update) = *func.value(next) else { return None };
        if func.value_ty(value) != Some(MirType::I256)
            || !defined_outside(func, l, init)
            || !body.iter().any(|&block| func.blocks[block].instructions.contains(&update))
            || escapes(func, l, next)
        {
            return None;
        }
        let (step, subtract) = match func.inst(update).kind {
            InstKind::Add(a, b) if a == value => (b, false),
            InstKind::Add(a, b) if b == value => (a, false),
            InstKind::Sub(a, b) if a == value => (b, true),
            _ => return None,
        };
        recurrences.push(Recurrence { value, init, step, subtract, update });
    }
    let counter = recurrences.iter().find(|r| r.value == counter)?;
    if func.value_u64(counter.init) != Some(0)
        || func.value_u64(counter.step) != Some(1)
        || counter.subtract
        || body
            .iter()
            .flat_map(|&block| &func.blocks[block].instructions)
            .any(|inst| !recurrences.iter().any(|r| r.update == *inst))
    {
        return None;
    }
    let invariant = |value| defined_outside(func, l, value);
    let mut quadratic = 0;
    for r in &recurrences {
        if !invariant(r.step) {
            let source = recurrences.iter().find(|source| source.value == r.step)?;
            if !invariant(source.step) {
                return None;
            }
            quadratic += 1;
        }
    }

    let linear_cost = target.opcode(op::MUL) + target.opcode(op::ADD) + target.dup().times(2);
    let mut after =
        linear_cost.times(recurrences.len() as u32) + linear_cost.times(quadratic) + target.jump();
    if quadratic != 0 {
        after += target.opcode(op::SHR)
            + target.opcode(op::SUB)
            + target.opcode(op::OR)
            + target.opcode(op::MUL)
            + target.push(U256::from(1)).times(3)
            + target.dup();
    }
    let header_cost = target.block_code_estimate(func, header);
    let body_cost = body.iter().map(|&block| target.block_code_estimate(func, block)).sum::<Cost>();
    let trips = func.value_u64(count).unwrap_or(Target::UNCOUNTED_LOOP_ITERATIONS);
    let looped = (header_cost + body_cost).times(u32::try_from(trips).unwrap_or(u32::MAX));
    let before =
        Cost::new(looped.gas.saturating_add(header_cost.gas), header_cost.bytes + body_cost.bytes)
            .with_steps(looped.steps.saturating_add(header_cost.steps));
    if !target.cmp_lifetime(after, before).is_lt() {
        return None;
    }
    Some(ClosedForm { header, body, preheader, exit: *exit, count, recurrences })
}

fn rewrite(func: &mut Function, plan: ClosedForm) {
    let mut replacements = FxHashMap::default();
    let mut builder = FunctionBuilder::new(func);
    builder.switch_to_block(plan.preheader);
    let mut triangular = None;
    for r in &plan.recurrences {
        let source = plan.recurrences.iter().find(|source| source.value == r.step);
        let step = source.map_or(r.step, |source| source.init);
        // delta = mul n, step
        let mut delta = builder.mul(plan.count, step);
        if let Some(source) = source {
            // half = shr n, 1
            // previous = sub n, 1
            // odd = or previous, 1
            // triangular = mul half, odd
            let triangular = *triangular.get_or_insert_with(|| {
                let one = builder.imm(1);
                let half = builder.shr(plan.count, one);
                let previous = builder.sub(plan.count, one);
                let odd = builder.or(previous, one);
                builder.mul(half, odd)
            });
            // drift = mul triangular, source_step
            // delta = add/sub delta, drift
            let drift = builder.mul(triangular, source.step);
            delta =
                if source.subtract { builder.sub(delta, drift) } else { builder.add(delta, drift) };
        }
        // final = add/sub init, delta
        let value =
            if r.subtract { builder.sub(r.init, delta) } else { builder.add(r.init, delta) };
        replacements.insert(r.value, value);
    }
    func.replace_uses(&replacements);
    // header: jump exit
    fold_terminator_to_jump(func, plan.header, plan.exit);
    for block in plan.body {
        let _ = invalidate_unreachable_block(func, block);
    }
    func.blocks[plan.header].instructions.clear();
}

fn defined_outside(func: &Function, l: &Loop, value: ValueId) -> bool {
    let Value::Inst(inst) = *func.value(value) else { return true };
    !l.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst))
}

fn escapes(func: &Function, l: &Loop, value: ValueId) -> bool {
    func.blocks.iter_enumerated().any(|(block, contents)| {
        if l.blocks.contains(block) {
            return false;
        }
        let mut found = false;
        for &inst in &contents.instructions {
            func.inst(inst).kind.visit_operands(|operand| found |= operand == value);
        }
        if let Some(term) = &contents.terminator {
            term.visit_operands(|operand| found |= operand == value);
        }
        found
    })
}

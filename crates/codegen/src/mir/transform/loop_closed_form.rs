//! Evaluate small, effect-free additive loops at their exit.
//!
//! Match a zero-based unsigned counter with unit stride, a header containing
//! only word phis and its bound comparison, and one straight-line latch whose
//! only instructions add each phi's increment. Increments may be loop invariant
//! or another phi with an invariant increment. A phi may instead take the same
//! invariant value on every latch; its exit is that value when `n != 0`, or its
//! initial value when `n == 0`. The loop then has exactly `n`
//! iterations, even when `n` is the largest word, and each additive exit value is a
//! polynomial of degree at most two in `n`, evaluated modulo 2^256.
//!
//! Compute `n*(n-1)/2` by halving the even factor before multiplying. A wrapped
//! product followed by division would lose a bit. Zero trips need no special
//! branch: both polynomial terms vanish. Checked arithmetic, extra exits,
//! memory or environment observations, higher-degree recurrences, and latch
//! values used outside their own phis prevent the rewrite.
//!
//! Run after late scalar and CFG cleanup, before EVM shaping. Only gas mode
//! enables this bounded tradeoff: up to four recurrences replace repeated loop
//! work with a fixed setup whose code size can exceed the original loop.
//! This setup can cost more for short or zero-trip loops. Conversely, a large
//! finite loop that would exhaust gas may now finish; exact gas-exhaustion
//! outcomes are not preserved across optimization settings.

use crate::mir::{
    BlockId, Function, FunctionBuilder, InstId, InstKind, InstructionMetadata, MirType, Module,
    Terminator, Value, ValueId,
    pass::{MirPass, ModuleAnalyses, run_function_pass},
    utils::{fold_terminator_to_jump, invalidate_unreachable_block},
};
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

pub(crate) struct LoopClosedForm;

impl MirPass for LoopClosedForm {
    fn name(&self) -> &'static str {
        "loop-closed-form"
    }

    fn is_enabled(&self, gcx: Gcx<'_>, _: &Module) -> bool {
        gcx.sess.opts.optimization.is_gas()
    }

    fn run_pass(&self, _: Gcx<'_>, module: &mut Module, analyses: &mut ModuleAnalyses) -> bool {
        run_function_pass(module, analyses, |func, _| {
            let mut changed = false;
            for header in func.blocks.indices() {
                if let Some(candidate) = recognize(func, header) {
                    rewrite(func, candidate);
                    changed = true;
                }
            }
            changed
        })
    }
}

struct Recurrence {
    value: ValueId,
    initial: ValueId,
    step: Step,
}

#[derive(Clone, Copy)]
enum Step {
    Add { increment: ValueId, update: InstId },
    Assign(ValueId),
}

struct Candidate {
    header: BlockId,
    body: BlockId,
    exit: BlockId,
    preheader: BlockId,
    bound: ValueId,
    recurrences: Vec<Recurrence>,
}

fn recognize(func: &Function, header: BlockId) -> Option<Candidate> {
    let block = &func.blocks[header];
    let Terminator::Branch { condition, then_block: body, else_block: exit } =
        block.terminator.as_ref()?
    else {
        return None;
    };
    let (body, exit) = (*body, *exit);
    if body == header
        || exit == header
        || exit == body
        || func.blocks[body].predecessors.as_slice() != [header]
        || func.blocks[body].terminator != Some(Terminator::Jump(header))
        || block.predecessors.len() != 2
        || !(3..=5).contains(&block.instructions.len())
        || block.instructions.iter().any(|&inst| {
            func.inst(inst)
                .metadata
                .effect()
                .is_some_and(|effect| effect != func.inst(inst).kind.effect_kind())
        })
    {
        return None;
    }
    let preheader = *block.predecessors.iter().find(|&&pred| pred != body)?;
    if func.blocks[preheader].terminator != Some(Terminator::Jump(header)) {
        return None;
    }
    let Value::Inst(compare) = func.value(*condition) else { return None };
    if block.instructions.last() != Some(compare) {
        return None;
    }
    let InstKind::Lt(counter, bound) = func.inst(*compare).kind else { return None };
    let invariant = |value| match func.value(value) {
        Value::Inst(inst) => {
            !block.instructions.contains(inst) && !func.blocks[body].instructions.contains(inst)
        }
        _ => true,
    };
    if !invariant(bound) || func.value_ty(bound) != Some(MirType::I256) {
        return None;
    }
    let mut recurrences = Vec::new();
    for &inst in &block.instructions[..block.instructions.len() - 1] {
        let instruction = func.inst(inst);
        let InstKind::Phi(incoming) = &instruction.kind else { return None };
        if incoming.len() != 2 || instruction.result_ty != Some(MirType::I256) {
            return None;
        }
        let value = func.inst_result_value(inst)?;
        let initial = incoming.iter().find(|&&(pred, _)| pred == preheader)?.1;
        let next = incoming.iter().find(|&&(pred, _)| pred == body)?.1;
        if !invariant(initial) {
            return None;
        }
        if invariant(next) {
            recurrences.push(Recurrence { value, initial, step: Step::Assign(next) });
            continue;
        }
        let Value::Inst(update) = func.value(next) else { return None };
        if !func.blocks[body].instructions.contains(update) {
            return None;
        }
        let InstKind::Add(a, b) = func.inst(*update).kind else { return None };
        let increment = if a == value {
            b
        } else if b == value {
            a
        } else {
            return None;
        };
        recurrences.push(Recurrence {
            value,
            initial,
            step: Step::Add { increment, update: *update },
        });
    }
    if func.blocks[body].instructions.len()
        != recurrences.iter().filter(|r| matches!(r.step, Step::Add { .. })).count()
        || func.blocks[body].instructions.iter().any(|inst| {
            !recurrences.iter().any(
                |recurrence| matches!(recurrence.step, Step::Add { update, .. } if update == *inst),
            ) || func
                .inst(*inst)
                .metadata
                .effect()
                .is_some_and(|effect| effect != func.inst(*inst).kind.effect_kind())
        })
    {
        return None;
    }
    let index = recurrences.iter().find(|recurrence| recurrence.value == counter)?;
    if func.value_u64(index.initial) != Some(0)
        || !matches!(index.step, Step::Add { increment, .. } if func.value_u64(increment) == Some(1))
    {
        return None;
    }
    for recurrence in &recurrences {
        if let Step::Add { increment, .. } = recurrence.step
            && !invariant(increment)
            && !recurrences.iter().any(|other| {
                other.value == increment
                    && matches!(other.step, Step::Add { increment, .. } if invariant(increment))
            })
        {
            return None;
        }
    }
    // Only header phis may escape: the latch does not execute on the zero-trip path.
    for (id, outside) in func.blocks.iter_enumerated() {
        if id == header || id == body {
            continue;
        }
        let forbidden = |value| {
            matches!(func.value(value), Value::Inst(inst)
                if *inst == *compare || func.blocks[body].instructions.contains(inst))
        };
        if outside
            .instructions
            .iter()
            .any(|&inst| func.inst(inst).operands().into_iter().any(forbidden))
            || outside
                .terminator
                .as_ref()
                .is_some_and(|term| term.operands().into_iter().any(forbidden))
        {
            return None;
        }
    }
    Some(Candidate { header, body, exit, preheader, bound, recurrences })
}

fn rewrite(func: &mut Function, candidate: Candidate) {
    let mut replacements = FxHashMap::default();
    {
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(candidate.preheader);
        // NOTE: Closed forms combine loop iterations, so their source origin is unknown.
        let mut metadata = InstructionMetadata::EMPTY;
        metadata.mark_debug_info_dropped();
        builder.set_debug_context(&metadata);
        let one = builder.imm(1);
        let mut triangular = None;
        for recurrence in &candidate.recurrences {
            let increment = match recurrence.step {
                Step::Assign(next) => {
                    let result = builder.select(candidate.bound, next, recurrence.initial);
                    replacements.insert(recurrence.value, result);
                    continue;
                }
                Step::Add { increment, .. } => increment,
            };
            let dependent = candidate.recurrences.iter().find(|r| r.value == increment);
            let increment = dependent.map_or(increment, |r| r.initial);
            let linear = builder.mul(candidate.bound, increment);
            let mut result = builder.add(recurrence.initial, linear);
            if let Some(dependent) = dependent {
                let triangular = *triangular.get_or_insert_with(|| {
                    let half = builder.shr(one, candidate.bound);
                    let parity = builder.and(candidate.bound, one);
                    let previous = builder.sub(candidate.bound, one);
                    let odd = builder.add(previous, parity);
                    builder.mul(half, odd)
                });
                let Step::Add { increment, .. } = dependent.step else { unreachable!() };
                let quadratic = builder.mul(triangular, increment);
                result = builder.add(result, quadratic);
            }
            replacements.insert(recurrence.value, result);
        }
    }
    func.replace_uses(&replacements);
    fold_terminator_to_jump(func, candidate.header, candidate.exit);
    let _ = invalidate_unreachable_block(func, candidate.body);
    func.blocks[candidate.header].instructions.clear();
}

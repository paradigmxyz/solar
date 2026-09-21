//! Combine surviving checks that branch to the same failure block.
//!
//! After all lowering and check elimination, scan straight-line chains of conditional
//! branches with a common failure successor. Keep each predicate, combine them in execution
//! order, and branch only at the end of the group. This works for signed and unsigned arithmetic
//! of every width without recognizing arithmetic recipes or encoded panic payloads. Eliminating
//! redundant checks first avoids retaining their computations inside a partially known OR.
//!
//! Each continuation must have one predecessor and contain only pure, nontrapping computations,
//! with no phis. Calls, memory and environment reads, writes, semantic checks, and control-flow
//! joins stop the group. The common failure block must end in a revert or tail call and have no
//! phis: postponing its execution must not change which incoming values it observes. Different
//! panic codes have different targets and cannot combine. Failure-on-false predicates receive
//! a boolean inversion before all failure predicates combine with OR.
//!
//! The target prices each removed branch against an OR, accumulator stack traffic, and a possible
//! inversion. Groups contain at most four branches; each continuation has at most eight instructions.
//! Do not extend an accumulator across an edge with more than four live nonconstant values: extra
//! stack shuffling can outweigh the saved branches. Cyclic blocks retain their checks because
//! loop-carried stack layouts can regress even below this limit. These bounds limit speculative
//! work and live ranges. CFG cleanup runs only in changed functions and merges unconditional links.
//! Empty-revert paths stay separate: combining their compact ABI guards can increase stack
//! pressure and interfere with later ABI lowering. Outlined single-block payload helpers qualify.
//! The default pipeline enables this only for gas: fewer branches can still grow bytecode when
//! the accumulated predicate needs spills. Original computations keep their debug contexts; the
//! combined branch has no unique origin.

use super::cfg_simplify::simplify_function;
use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, FunctionId, Immediate, InstKind, Instruction, MirType,
        Module, Terminator, Value, ValueId,
        analysis::{CfgInfo, Liveness},
        pass::{MirPass, run_function_pass_with_cfg},
        utils::{fold_terminator_to_jump, replace_terminator},
    },
    target::Target,
};
use solar_data_structures::bit_set::DenseBitSet;
use std::cell::OnceCell;

pub(crate) struct CheckedAggregate;

impl MirPass for CheckedAggregate {
    fn name(&self) -> &'static str {
        "checked-aggregate"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let target = Target::new(gcx);
        let mut payload_helpers = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if func.blocks.len() == 1
                && !func.blocks[BlockId::ENTRY].instructions.is_empty()
                && matches!(func.blocks[BlockId::ENTRY].terminator, Some(Terminator::Revert { .. }))
            {
                payload_helpers.insert(id);
            }
        }
        run_function_pass_with_cfg(module, analyses, |func, analyses| {
            aggregate(func, &payload_helpers, target, analyses.cfg())
        })
    }
}

#[derive(Clone, Copy)]
struct Check {
    block: BlockId,
    condition: ValueId,
    failure: BlockId,
    continuation: BlockId,
    failure_on_true: bool,
}

fn check(
    func: &Function,
    block: BlockId,
    payload_helpers: &DenseBitSet<FunctionId>,
) -> Option<Check> {
    let Terminator::Branch { condition, then_block, else_block } =
        *func.blocks[block].terminator.as_ref()?
    else {
        return None;
    };
    if then_block == else_block {
        return None;
    }
    for (failure, continuation, failure_on_true) in
        [(then_block, else_block, true), (else_block, then_block, false)]
    {
        if failure != block
            && !func.block_has_phi(failure)
            && match &func.blocks[failure].terminator {
                Some(Terminator::Revert { .. }) => !func.blocks[failure].instructions.is_empty(),
                Some(Terminator::TailCall { function, .. }) => payload_helpers.contains(*function),
                _ => false,
            }
        {
            return Some(Check { block, condition, failure, continuation, failure_on_true });
        }
    }
    None
}

fn aggregate(
    func: &mut Function,
    payload_helpers: &DenseBitSet<FunctionId>,
    target: Target,
    cfg: &CfgInfo,
) -> bool {
    let liveness = OnceCell::new();
    let mut claimed = DenseBitSet::new_empty(func.blocks.len());
    let mut changed = false;
    for block in func.blocks.indices() {
        if claimed.contains(block) || cfg.cyclic_blocks().contains(block) {
            continue;
        }
        let Some(first) = check(func, block, payload_helpers) else { continue };
        let mut chain = vec![first];
        let mut current = first;
        while chain.len() < 4 {
            let next = current.continuation;
            if next == block
                || claimed.contains(next)
                || func.blocks[next].predecessors.as_slice() != [current.block]
                || func.blocks[next].instructions.len() > 8
                || !func.blocks[next].instructions.iter().all(|&id| {
                    let kind = &func.inst(id).kind;
                    !matches!(kind, InstKind::Phi(_))
                        && !kind.has_side_effects()
                        && kind.effect_kind() == EffectKind::Pure
                        && func.inst_result_value(id).is_some()
                })
            {
                break;
            }
            let Some(next) = check(func, next, payload_helpers) else { break };
            if next.failure != first.failure {
                break;
            }
            if liveness
                .get_or_init(|| Liveness::compute(func))
                .live_out(current.block)
                .iter()
                .filter(|&value| func.value(value).as_immediate().is_none())
                .count()
                > 4
            {
                break;
            }
            chain.push(next);
            current = next;
        }
        if chain.len() < 2 {
            continue;
        }
        let branches = (chain.len() - 1) as u32;
        let inversions = chain.iter().filter(|check| !check.failure_on_true).count() as u32;
        let before = target.opcode(op::JUMPI).plus(target.opcode(op::PUSH2)).times(branches);
        let after = target
            .opcode(op::OR)
            .plus(target.dup())
            .times(branches)
            .plus(target.opcode(op::ISZERO).times(inversions));
        if target.cmp(after, before).is_ge() {
            continue;
        }
        let mut accumulated = failure_condition(func, first);
        for (index, node) in chain.iter().enumerate().skip(1) {
            let condition = failure_condition(func, *node);
            // accumulated = accumulated | condition
            // branch accumulated, failure, continuation
            let kind = InstKind::Or(accumulated, condition);
            let (id, value) = func.alloc_value_inst(
                Instruction::new(kind, Some(MirType::I1)).with_debug_info_dropped(),
            );
            func.blocks[node.block].instructions.push(id);
            replace_terminator(
                func,
                node.block,
                Terminator::Branch {
                    condition: value,
                    then_block: node.failure,
                    else_block: node.continuation,
                },
            );
            // NOTE: Several checks now share this branch. Keep no arbitrary source location.
            func.blocks[node.block].terminator_metadata.mark_debug_info_dropped();
            accumulated = value;
            let previous = chain[index - 1];
            // branch condition, failure, continuation => jump continuation
            fold_terminator_to_jump(func, previous.block, previous.continuation);
            func.blocks[previous.block].terminator_metadata.mark_debug_info_dropped();
        }
        for node in chain {
            claimed.insert(node.block);
        }
        changed = true;
    }
    if changed {
        simplify_function(func);
    }
    changed
}

fn failure_condition(func: &mut Function, check: Check) -> ValueId {
    if check.failure_on_true {
        return check.condition;
    }
    // failure = condition == false
    let zero = func.alloc_value(Value::Immediate(Immediate::I1(false)));
    let (id, value) = func.alloc_value_inst(
        Instruction::new(InstKind::Eq(check.condition, zero), Some(MirType::I1))
            .with_debug_info_dropped(),
    );
    func.blocks[check.block].instructions.push(id);
    value
}

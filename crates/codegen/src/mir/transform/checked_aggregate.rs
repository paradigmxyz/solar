//! Combine surviving checks that branch to the same failure block.
//!
//! After arithmetic lowering and check elimination, scan straight-line chains of conditional
//! branches with a common failure successor. Keep each overflow predicate, OR them in execution
//! order, and branch only at the end of the group. This works for signed and unsigned arithmetic
//! of every width without recognizing arithmetic recipes or encoded panic payloads. Eliminating
//! redundant checks first avoids retaining their computations inside a partially known OR.
//!
//! Each continuation must have one predecessor and contain only pure, nontrapping computations,
//! with no phis. Calls, memory and environment reads, writes, semantic checks, and control-flow
//! joins stop the group. The common failure block must end in a revert or tail call and have no
//! phis: postponing its execution must not change which incoming values it observes. Different
//! panic codes have different targets and cannot combine. Only failure-on-nonzero branches
//! qualify; division checks and other inverted conditions remain untouched.
//!
//! The target prices each removed branch against an OR and accumulator stack traffic. Groups
//! contain at most four branches and each continuation at most eight instructions to bound
//! speculative work and live ranges. The following CFG cleanup merges the unconditional links.
//! Empty-revert paths stay separate: combining their compact ABI guards can increase stack
//! pressure and interfere with later ABI lowering. Outlined single-block payload helpers qualify.
//! The default pipeline enables this only for gas: fewer branches can still grow bytecode when
//! the accumulated predicate needs spills. Original computations keep their debug contexts; the
//! combined branch has no unique origin.

use crate::{
    backend::evm::op,
    mir::{
        BlockId, EffectKind, Function, FunctionId, InstKind, Instruction, MirType, Module,
        Terminator, ValueId,
        pass::{MirPass, run_function_pass},
        utils::{fold_terminator_to_jump, replace_terminator},
    },
    target::Target,
};
use solar_data_structures::bit_set::DenseBitSet;

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
        let before = target.opcode(op::JUMPI).plus(target.opcode(op::PUSH2));
        let after = target.opcode(op::OR).plus(target.dup());
        if target.cmp(after, before).is_ge() {
            return false;
        }
        let mut payload_helpers = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if func.blocks.len() == 1
                && !func.blocks[BlockId::ENTRY].instructions.is_empty()
                && matches!(func.blocks[BlockId::ENTRY].terminator, Some(Terminator::Revert { .. }))
            {
                payload_helpers.insert(id);
            }
        }
        run_function_pass(module, analyses, |func, _| aggregate(func, &payload_helpers))
    }
}

#[derive(Clone, Copy)]
struct Check {
    block: BlockId,
    condition: ValueId,
    failure: BlockId,
    continuation: BlockId,
}

fn check(
    func: &Function,
    block: BlockId,
    payload_helpers: &DenseBitSet<FunctionId>,
) -> Option<Check> {
    let Terminator::Branch { condition, then_block: failure, else_block: continuation } =
        func.blocks[block].terminator.as_ref()?
    else {
        return None;
    };
    if failure == continuation
        || *failure == block
        || func.block_has_phi(*failure)
        || !match &func.blocks[*failure].terminator {
            Some(Terminator::Revert { .. }) => !func.blocks[*failure].instructions.is_empty(),
            Some(Terminator::TailCall { function, .. }) => payload_helpers.contains(*function),
            _ => false,
        }
    {
        return None;
    }
    Some(Check { block, condition: *condition, failure: *failure, continuation: *continuation })
}

fn aggregate(func: &mut Function, payload_helpers: &DenseBitSet<FunctionId>) -> bool {
    let mut claimed = DenseBitSet::new_empty(func.blocks.len());
    let mut changed = false;
    for block in func.blocks.indices() {
        if claimed.contains(block) {
            continue;
        }
        let Some(first) = check(func, block, payload_helpers) else { continue };
        let mut chain = vec![first];
        let mut current = first;
        while chain.len() < 4 {
            let next = current.continuation;
            if next == block
                || claimed.contains(next)
                || func.unique_predecessors(next).as_slice() != [current.block]
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
            chain.push(next);
            current = next;
        }
        if chain.len() < 2 {
            continue;
        }
        let mut accumulated = first.condition;
        for (index, node) in chain.iter().enumerate().skip(1) {
            // accumulated = accumulated | condition
            // branch accumulated, failure, continuation
            let (id, value) = func.alloc_value_inst(
                Instruction::new(
                    InstKind::Or(accumulated, node.condition),
                    Some(MirType::uint256()),
                )
                .with_debug_info_dropped(),
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
    changed
}

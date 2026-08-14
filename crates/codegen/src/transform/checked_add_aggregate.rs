//! Checked `uint256` addition-chain aggregation.
//!
//! This pass recognizes consecutive MIR blocks containing only those primitive
//! operations and combines their wrap tests. The additions remain in their
//! original order, but only the final block branches to the shared panic block.

use crate::{
    mir::{
        BlockId, Function, FunctionId, InstKind, Instruction, MirType, Module, Terminator, ValueId,
        utils::repair_reachability_phis,
    },
    pass::{MirPass, run_function_pass},
};
use alloy_primitives::U256;
use solar_data_structures::bit_set::DenseBitSet;

/// Aggregates consecutive checked `uint256` addition overflow checks.
pub(crate) struct CheckedAddAggregate;

impl MirPass for CheckedAddAggregate {
    fn name(&self) -> &'static str {
        "checked-add-aggregate"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        let mut panic_stubs = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if is_arithmetic_panic_stub(func) {
                panic_stubs.insert(id);
            }
        }
        run_function_pass(module, analyses, |func, _| {
            let changed = aggregate_checked_adds(func, &panic_stubs);
            changed | repair_reachability_phis(func)
        })
    }
}

#[derive(Clone, Copy)]
struct CheckedAddBlock {
    block: BlockId,
    sum: ValueId,
    carry: ValueId,
    panic_stub: FunctionId,
    continuation: BlockId,
}

fn checked_add_block(
    func: &Function,
    block: BlockId,
    panic_stubs: &DenseBitSet<FunctionId>,
) -> Option<CheckedAddBlock> {
    let bb = &func.blocks[block];
    let [.., add_inst, carry_inst] = bb.instructions.as_slice() else { return None };
    let Terminator::Branch { condition, then_block, else_block } = bb.terminator.as_ref()? else {
        return None;
    };
    let InstKind::Add(lhs, rhs) = func.inst(*add_inst).kind else { return None };
    let sum = func.inst_result_value(*add_inst)?;
    let InstKind::Lt(result, previous) = func.inst(*carry_inst).kind else { return None };
    let carry = func.inst_result_value(*carry_inst)?;
    let panic_stub = arithmetic_panic_target(func, *then_block, panic_stubs)?;

    if *condition != carry
        || result != sum
        || (previous != lhs && previous != rhs)
        || func.value_ty(sum) != Some(MirType::uint256())
        || func.value_ty(lhs) != Some(MirType::uint256())
        || func.value_ty(rhs) != Some(MirType::uint256())
    {
        return None;
    }

    Some(CheckedAddBlock { block, sum, carry, panic_stub, continuation: *else_block })
}

fn is_arithmetic_panic_stub(func: &Function) -> bool {
    if func.blocks.len() != 1 {
        return false;
    }
    let block = &func.blocks[BlockId::ENTRY];
    let [selector_store, code_store] = block.instructions.as_slice() else { return false };
    let InstKind::MStore(selector_offset, selector) = func.inst(*selector_store).kind else {
        return false;
    };
    let InstKind::MStore(code_offset, code) = func.inst(*code_store).kind else { return false };
    let Some(Terminator::Revert { offset, size }) = block.terminator.as_ref() else { return false };
    let panic_selector = U256::from(0x4e487b71_u64) << 224;

    func.value_u256(selector_offset) == Some(U256::ZERO)
        && func.value_u256(selector) == Some(panic_selector)
        && func.value_u256(code_offset) == Some(U256::from(4))
        && func.value_u256(code) == Some(U256::from(0x11))
        && func.value_u256(*offset) == Some(U256::ZERO)
        && func.value_u256(*size) == Some(U256::from(36))
}

fn arithmetic_panic_target(
    func: &Function,
    block: BlockId,
    panic_stubs: &DenseBitSet<FunctionId>,
) -> Option<FunctionId> {
    let block = &func.blocks[block];
    let Terminator::TailCall { function, args } = block.terminator.as_ref()? else { return None };
    (block.instructions.is_empty() && args.is_empty() && panic_stubs.contains(*function))
        .then_some(*function)
}

fn aggregate_checked_adds(func: &mut Function, panic_stubs: &DenseBitSet<FunctionId>) -> bool {
    let mut chains = Vec::new();
    let mut claimed = DenseBitSet::new_empty(func.blocks.len());
    for block in func.blocks.indices() {
        if claimed.contains(block) {
            continue;
        }
        let Some(first) = checked_add_block(func, block, panic_stubs) else { continue };
        let mut chain = vec![first];
        let mut current = first;

        loop {
            let next_block = current.continuation;
            if claimed.contains(next_block)
                || func.unique_predecessors(next_block).as_slice() != [current.block]
                || func.blocks[next_block].instructions.len() != 2
            {
                break;
            }
            let Some(next) = checked_add_block(func, next_block, panic_stubs) else { break };
            let InstKind::Add(lhs, rhs) = func.inst(func.blocks[next_block].instructions[0]).kind
            else {
                break;
            };
            if next.panic_stub != first.panic_stub || (lhs != current.sum && rhs != current.sum) {
                break;
            }
            chain.push(next);
            current = next;
        }

        if chain.len() >= 2 {
            for node in &chain {
                claimed.insert(node.block);
            }
            chains.push(chain);
        }
    }

    if chains.is_empty() {
        return false;
    }

    for chain in chains {
        let mut accumulated = chain[0].carry;
        for (index, node) in chain.iter().copied().enumerate().skip(1) {
            let (or_inst, or_value) = func.alloc_value_inst(Instruction::new(
                InstKind::Or(accumulated, node.carry),
                Some(MirType::uint256()),
            ));
            func.blocks[node.block].instructions.push(or_inst);
            let Terminator::Branch { condition, .. } =
                func.blocks[node.block].terminator.as_mut().expect("matched branch")
            else {
                unreachable!("matched branch changed during aggregation")
            };
            *condition = or_value;
            accumulated = or_value;

            let previous = chain[index - 1];
            func.blocks[previous.block].terminator = Some(Terminator::Jump(previous.continuation));
        }
    }
    true
}

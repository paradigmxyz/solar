//! Bounded whole-block trials for private retained-value ordering.
//!
//! Entry ordering places earlier last uses nearer the top and pays both its
//! incoming permutation and outgoing phi reconciliation. Operand ordering instead
//! places dying operands in pop order while retaining other values in any order,
//! then restores the exact original post-instruction stack. The established unary
//! trial is compared first; the multi-operand trial must improve on it. This pays
//! all later shuffles and leaves terminator lowering and outgoing edges unchanged.
//!
//! Both trials replay the actual opcode emitter and share physical gas/byte and
//! input/net/peak checks. Selection is atomic on failure and adds no labels. At
//! most 32 direct instructions in a spill-free block qualify; calls, compound
//! lowering, and code/gas observations are excluded. Entry ordering additionally
//! requires pure opcodes and an unconditional jump. Opcode/effect order and the
//! fixed return-label prefix never change. Later outlining/layout interactions
//! still require complete generated-code measurements. Operand trials run only
//! for gas optimization; size mode keeps canonical bodies for existing outlining.
//! Their leading stack-only prefix must match the original, preserving the
//! demonstrated cancellation with predecessor shuffles. This conservative
//! boundary guard is not a guarantee about every later CFG/layout interaction.

use super::{Context, Slot, edge_values, lower_opcode, materialize, prefix};
use crate::{
    backend::evm::{ir, op, scheduler::Stack},
    mir,
};
use solar_config::OptimizationMode;
use std::cmp::Reverse;

/// Allowed departure from canonical operand preparation during one block replay.
#[derive(Clone, Copy)]
pub(super) enum OperandOrder {
    Canonical,
    DeadUnary,
    DeadOperands,
}

impl OperandOrder {
    pub(super) fn allows(self, arity: usize) -> bool {
        match self {
            Self::Canonical => false,
            Self::DeadUnary => arity == 1,
            Self::DeadOperands => arity > 0,
        }
    }
}

pub(super) fn choose(
    context: &Context<'_>,
    block_id: mir::BlockId,
    target: mir::BlockId,
    original_stack: &Stack<Slot>,
    original_insts: &[ir::Instruction],
) -> Option<(Stack<Slot>, Vec<ir::Instruction>)> {
    if !eligible(
        context,
        block_id,
        |opcode| matches!(opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::CLZ),
    ) {
        return None;
    }
    let block = &context.function.blocks[block_id];
    let incoming = &context.layout.entries[block_id];
    let mut preferred = incoming.clone();
    preferred[prefix(context)..].sort_by_key(|slot| {
        let last = match *slot {
            Slot::Value(value) if !context.layout.live.live_out(block_id).contains(value) => block
                .instructions
                .iter()
                .rposition(|&id| context.function.inst(id).kind.operands().contains(&value))
                .unwrap_or(usize::MAX),
            _ => usize::MAX,
        };
        Reverse(last)
    });
    if preferred == *incoming {
        return None;
    }
    let mut stack = Stack::new(incoming.clone());
    // <canonical entry>; <paid permutation into last-use order>
    let mut insts = stack.reconcile(&preferred, prefix(context), context.version).ok()?;
    replay(context, block_id, &mut stack, &mut insts, OperandOrder::Canonical)?;
    let desired = edge_values(context, block_id, target).ok()?;
    finish(context, &mut stack, &mut insts, &desired)?;
    let mut original = original_insts.to_vec();
    finish(context, &mut original_stack.clone(), &mut original, &desired)?;
    improves(context, &original, &insts).then_some((stack, insts))
}

pub(super) fn choose_operands(
    context: &Context<'_>,
    block_id: mir::BlockId,
    original_stack: &Stack<Slot>,
    original_insts: &[ir::Instruction],
) -> Option<Vec<ir::Instruction>> {
    if !matches!(context.optimization, OptimizationMode::Gas)
        || !eligible(context, block_id, |opcode| {
            op::stack_io(opcode).is_some()
                && !matches!(
                    opcode,
                    op::CALL
                        | op::CALLCODE
                        | op::DELEGATECALL
                        | op::STATICCALL
                        | op::EXTCALL
                        | op::EXTDELEGATECALL
                        | op::EXTSTATICCALL
                        | op::CREATE
                        | op::CREATE2
                        | op::EOFCREATE
                        | op::GAS
                        | op::PC
                        | op::CODESIZE
                        | op::CODECOPY
                        | op::EXTCODESIZE
                        | op::EXTCODECOPY
                        | op::EXTCODEHASH
                )
        })
    {
        return None;
    }
    let mut best = original_insts.to_vec();
    for order in [OperandOrder::DeadUnary, OperandOrder::DeadOperands] {
        let mut stack = Stack::new(context.layout.entries[block_id].clone());
        let mut insts = Vec::new();
        // <same prepared operands>; <same opcodes>; <paid exact original exit order>
        if replay(context, block_id, &mut stack, &mut insts, order).is_some()
            && finish(context, &mut stack, &mut insts, original_stack.values()).is_some()
            && stack_prefix(original_insts) == stack_prefix(&insts)
            && insts != best
            && improves(context, &best, &insts)
        {
            best = insts;
        }
    }
    (best != original_insts).then_some(best)
}

fn stack_prefix(insts: &[ir::Instruction]) -> &[ir::Instruction] {
    let end = insts
        .iter()
        .position(|inst| matches!(inst.kind, ir::InstKind::Op(code) if code != op::POP))
        .unwrap_or(insts.len());
    &insts[..end]
}

fn eligible(context: &Context<'_>, block: mir::BlockId, allowed: impl Fn(u8) -> bool) -> bool {
    !matches!(context.optimization, OptimizationMode::None)
        && context.layout.spills.homes.is_empty()
        && context.function.blocks[block].instructions.len() <= 32
        && context.function.blocks[block].instructions.iter().all(|&id| {
            let kind = &context.function.inst(id).kind;
            matches!(kind, mir::InstKind::Phi(_)) || kind.evm_opcode().is_some_and(&allowed)
        })
}

fn replay(
    context: &Context<'_>,
    block: mir::BlockId,
    stack: &mut Stack<Slot>,
    insts: &mut Vec<ir::Instruction>,
    operand_order: OperandOrder,
) -> Option<()> {
    for (position, &id) in context.function.blocks[block].instructions.iter().enumerate() {
        if let Some(opcode) = context.function.inst(id).kind.evm_opcode() {
            // <same prepared operands>; <same direct opcode>
            lower_opcode(context, block, position, id, opcode, stack, insts, operand_order).ok()?;
        }
    }
    Some(())
}

fn finish(
    context: &Context<'_>,
    stack: &mut Stack<Slot>,
    insts: &mut Vec<ir::Instruction>,
    desired: &[Slot],
) -> Option<()> {
    // <live values and simultaneous phi sources>; <required boundary order>
    materialize(context, stack, insts, desired).ok()?;
    insts.extend(stack.reconcile(desired, prefix(context), context.version).ok()?);
    Some(())
}

fn improves(
    context: &Context<'_>,
    original: &[ir::Instruction],
    candidate: &[ir::Instruction],
) -> bool {
    let Some(old) = ir::scheduling_usage(original) else { return false };
    let Some(new) = ir::scheduling_usage(candidate) else { return false };
    if new.0 > old.0 || new.1 != old.1 || new.2 > old.2 {
        return false;
    }
    let old = ir::scheduling_cost(context.version, original);
    let new = ir::scheduling_cost(context.version, candidate);
    new.0 <= old.0 && new.1 <= old.1 && new != old
}

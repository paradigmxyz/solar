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
//! The original unary, multi-operand, argument-carry, and entry-order choices run
//! before the final materialized-operand trial. A chosen entry order returns
//! immediately, conservatively retaining its incoming boundary and successor
//! stack even when its prefix happens to match. Otherwise materialization must
//! preserve the complete old winner's leading stack-only prefix. The older
//! operand trials still compare their prefix with the original. These guards
//! preserve demonstrated predecessor cancellation, but do not guarantee every
//! later CFG/layout interaction.
//!
//! One additional Gas candidate materializes a repeated immutable argument once
//! across two direct binary instructions. It requires an empty entry and an exact
//! complete emitted body; the commutative producer and ordered consumer retain
//! their output identities and relative peak. Its distinct materialization prefix
//! is confined to this candidate. Later CFG sharing still needs corpus validation.

use super::{Context, Slot, debug, edge_values, lower_opcode, materialize, prefix};
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
    MaterializedOperands,
}

impl OperandOrder {
    pub(super) fn allows(self, arity: usize) -> bool {
        match self {
            Self::Canonical => false,
            Self::DeadUnary => arity == 1,
            Self::DeadOperands | Self::MaterializedOperands => arity > 0,
        }
    }
}

fn choose_entry(
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

pub(super) fn choose(
    context: &Context<'_>,
    block_id: mir::BlockId,
    original_stack: &Stack<Slot>,
    original_insts: &[ir::Instruction],
) -> Option<(Stack<Slot>, Vec<ir::Instruction>)> {
    let entry = |insts: &[ir::Instruction]| {
        let mir::Terminator::Jump(target) =
            context.function.blocks[block_id].terminator.as_ref()?
        else {
            return None;
        };
        choose_entry(context, block_id, *target, original_stack, insts)
    };
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
        return entry(original_insts);
    }
    let mut best = original_insts.to_vec();
    let consider = |order, best: &mut Vec<ir::Instruction>| {
        let boundary = if matches!(order, OperandOrder::MaterializedOperands) {
            best.as_slice()
        } else {
            original_insts
        };
        let mut stack = Stack::new(context.layout.entries[block_id].clone());
        let mut insts = Vec::new();
        // <same prepared operands>; <same opcodes>; <paid exact original exit order>
        if replay(context, block_id, &mut stack, &mut insts, order).is_some()
            && finish(context, &mut stack, &mut insts, original_stack.values()).is_some()
            && stack_prefix(boundary) == stack_prefix(&insts)
            && insts != *best
            && improves(context, best, &insts)
        {
            *best = insts;
        }
    };
    for order in [OperandOrder::DeadUnary, OperandOrder::DeadOperands] {
        consider(order, &mut best);
    }
    if let Some(candidate) = carry_argument(context, block_id, original_stack, original_insts)
        && improves(context, &best, &candidate)
    {
        best = candidate;
    }
    if let Some(selected) = entry(&best) {
        return Some(selected);
    }
    consider(OperandOrder::MaterializedOperands, &mut best);
    (best != original_insts).then(|| (original_stack.clone(), best))
}

/// Tries one self-contained materialization pair; ordinary replay prefix guards stay intact.
/// A carried immutable argument replaces its second load with DUP/SWAP at equal gas.
/// Exact identity/peak checks do not prove later literal-arm sharing or CFG profitability.
fn carry_argument(
    context: &Context<'_>,
    block: mir::BlockId,
    original_stack: &Stack<Slot>,
    original: &[ir::Instruction],
) -> Option<Vec<ir::Instruction>> {
    let function = context.function;
    let [producer, consumer] = function.blocks[block].instructions.as_slice() else { return None };
    if !context.layout.entries[block].is_empty() || !super::external_argument(function) {
        return None;
    }
    let first = function.inst(*producer);
    let second = function.inst(*consumer);
    let producer_op = first.kind.evm_opcode()?;
    let consumer_op = second.kind.evm_opcode()?;
    let (arg, literal) = first.kind.reorderable_binary_operands()?;
    let p = function.inst_result_value(*producer)?;
    let c = function.inst_result_value(*consumer)?;
    if !matches!(producer_op, op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ)
        || !matches!(consumer_op, op::ADD..=op::SIGNEXTEND | op::LT..=op::SAR)
        || op::stack_io(consumer_op) != Some((2, 1))
        || !matches!(function.value(arg), mir::Value::Arg(_))
        || second.kind.operands().as_slice() != [p, arg]
        || original_stack.values() != [Slot::Value(p), Slot::Value(c)]
    {
        return None;
    }
    let mir::Value::Immediate(value) = function.value(literal) else { return None };
    let value = value.as_u256()?;
    let [constant, offset, read, produce, reload_offset, reload, duplicate, consume] = original
    else {
        return None;
    };
    if constant.kind != ir::InstKind::Push(value)
        || !matches!(offset.kind, ir::InstKind::Push(value) if !value.is_zero())
        || offset.kind != reload_offset.kind
        || read.kind != ir::InstKind::Op(op::CALLDATALOAD)
        || read.kind != reload.kind
        || produce.kind != ir::InstKind::Op(producer_op)
        || duplicate.kind != ir::InstKind::Dup(2)
        || consume.kind != ir::InstKind::Op(consumer_op)
        || original.iter().any(|inst| inst.stack_effect.is_some() || inst.keep_with_next)
    {
        return None;
    }
    // load arg; dup1; push literal; producer
    // swap1; dup2; consumer
    // Both complete bodies leave [producer_result, consumer_result], with peak three.
    let mut candidate = vec![
        offset.clone(),
        read.clone(),
        ir::InstKind::Dup(1).into(),
        constant.clone(),
        produce.clone(),
        ir::InstKind::Swap(1).into(),
        duplicate.clone(),
        consume.clone(),
    ];
    debug::instructions(context, &first.metadata, &mut candidate[..5]);
    debug::instructions(context, &second.metadata, &mut candidate[5..]);
    Some(candidate)
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
        && !context.layout.uses_spill_protocol()
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
            lower_opcode(context, (block, position), id, opcode, stack, insts, operand_order)
                .ok()?;
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

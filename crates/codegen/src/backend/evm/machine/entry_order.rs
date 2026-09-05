//! Local retained-value ordering for short, pure MIR blocks.
//!
//! A single trial places incoming values with earlier last uses nearer the top.
//! The actual checked opcode emitter schedules both versions. We pay the initial
//! permutation inside the block and the complete outgoing phi reconciliation,
//! leaving every predecessor and successor stack interface unchanged. Shared
//! physical simplification scores the complete instruction streams, and both
//! bytes and gas must improve or remain equal without increasing input access or
//! peak height. Selection adds no labels and is atomic on any scheduling failure.
//!
//! Only spill-free blocks with at most 32 pure direct opcodes and an unconditional
//! jump qualify. Calls, memory writers, observations of code or gas, and compound
//! lowering protocols remain outside this bounded trial. Opcode order, operands,
//! and the fixed return-label prefix never change.

use super::{Context, Slot, edge_values, lower_opcode, materialize, prefix};
use crate::{
    backend::evm::{ir, op, scheduler::Stack},
    mir,
};
use solar_config::OptimizationMode;
use std::cmp::Reverse;

pub(super) fn choose(
    context: &Context<'_>,
    block_id: mir::BlockId,
    target: mir::BlockId,
    original_stack: &Stack<Slot>,
    original_insts: &[ir::Instruction],
) -> Option<(Stack<Slot>, Vec<ir::Instruction>)> {
    let block = &context.function.blocks[block_id];
    if matches!(context.optimization, OptimizationMode::None)
        || !context.layout.spills.homes.is_empty()
        || block.instructions.len() > 32
        || !block.instructions.iter().all(|&id| {
            let kind = &context.function.inst(id).kind;
            matches!(kind, mir::InstKind::Phi(_))
                || kind.evm_opcode().is_some_and(
                    |opcode| matches!(opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::CLZ),
                )
        })
    {
        return None;
    }
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
    for (position, &id) in block.instructions.iter().enumerate() {
        if let Some(opcode) = context.function.inst(id).kind.evm_opcode() {
            // <same prepared operands>; <same direct opcode>
            lower_opcode(context, block_id, position, id, opcode, &mut stack, &mut insts).ok()?;
        }
    }
    let desired = edge_values(context, block_id, target).ok()?;
    let finish = |stack: &mut Stack<Slot>, insts: &mut Vec<ir::Instruction>| {
        // <live values and simultaneous phi sources>; <canonical successor order>
        materialize(context, stack, insts, &desired).ok()?;
        insts.extend(stack.reconcile(&desired, prefix(context), context.version).ok()?);
        Some(())
    };
    finish(&mut stack, &mut insts)?;
    let mut original = original_insts.to_vec();
    finish(&mut original_stack.clone(), &mut original)?;
    let old = ir::scheduling_usage(&original)?;
    let new = ir::scheduling_usage(&insts)?;
    if new.0 > old.0 || new.1 != old.1 || new.2 > old.2 {
        return None;
    }
    let old = ir::scheduling_cost(context.version, &original);
    let new = ir::scheduling_cost(context.version, &insts);
    (new.0 <= old.0 && new.1 <= old.1 && new != old).then_some((stack, insts))
}

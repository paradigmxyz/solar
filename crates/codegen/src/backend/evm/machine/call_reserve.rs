//! Schedules the pre-EIP-150 gas reserve together with its external call.
//!
//! Match adjacent MIR `gas; sub(gas, constant); call` instructions whose intermediate
//! results have no other uses. Prepare all call operands and save live homes before
//! reading gas, then emit an indivisible `GAS; SUB; CALL` sequence. Pre-EIP-150 calls
//! fail when their gas operand exceeds the remaining gas after the call surcharge;
//! even an extra stack shuffle can exhaust the deliberately small reserve.
//!
//! No observable MIR instruction moves across another. Other gas expressions keep
//! ordinary scheduling. The physical adjacency flag prevents later block passes from
//! inserting transfers inside the sequence; it is semantic metadata, independent of
//! optional source provenance. Matching is constant work per encountered GAS.

use super::{
    Context, Slot, debug, materialize, prepare, record_result, restore_writer_homes,
    save_writer_homes, schedule_error,
};
use crate::{
    backend::evm::{ir, op, scheduler::Stack},
    mir,
};

pub(super) fn lower(
    context: &Context<'_>,
    (block, position): (mir::BlockId, usize),
    stack: &mut Stack<Slot>,
    output: &mut Vec<ir::Instruction>,
) -> Result<bool, String> {
    if context.version.can_overcharge_gas_for_call() {
        return Ok(false);
    }
    let function = context.function;
    let instructions = &function.blocks[block].instructions;
    let Some(&[gas_id, sub_id, call_id]) = instructions.get(position..position + 3) else {
        return Ok(false);
    };
    let mir::InstKind::Sub(gas, reserve) = function.inst(sub_id).kind else {
        return Ok(false);
    };
    let call = function.inst(call_id);
    let Some(opcode @ (op::CALL | op::CALLCODE | op::DELEGATECALL | op::STATICCALL)) =
        call.kind.evm_opcode()
    else {
        return Ok(false);
    };
    let operands = call.kind.operands();
    let Some(forwarded) = function.inst_result_value(sub_id) else { return Ok(false) };
    let live = |value| context.layout.live.is_used_at_or_after(value, block, position + 3);
    if function.inst_result_value(gas_id) != Some(gas)
        || function.value(reserve).as_immediate().is_none()
        || operands[0] != forwarded
        || operands[1..].iter().any(|&value| value == gas || value == forwarded)
        || live(gas)
        || live(forwarded)
    {
        return Ok(false);
    }
    let start = output.len();
    let saved = save_writer_homes(
        context,
        (block, position + 2),
        stack,
        output,
        live,
        operands.len() + 1,
        || super::control_live_after(context, block, position + 2),
    )?;
    let mut prepared = operands;
    prepared[0] = reserve;
    if let Some(fixed_prefix) = saved.protected_prefix {
        let slots = prepared.iter().copied().map(Slot::Value).collect::<Vec<_>>();
        materialize(context, stack, output, &slots)?;
        // <protected homes>; <call operands in reverse order>; reserve
        output.extend(
            stack
                .prepare(&slots, fixed_prefix, context.version, |_| false)
                .map_err(schedule_error)?,
        );
    } else {
        // <retained live values>; <call operands in reverse order>; reserve
        prepare(context, stack, output, &prepared, live)?;
    }
    if stack.values().len() == 1024 {
        return Err("EVM gas reserve exceeds the 1024-word stack limit".into());
    }
    debug::instructions(context, &call.metadata, &mut output[start..]);
    // <remaining call operands>; reserve
    // gas
    // sub
    // call
    for (id, opcode, keep_with_next) in
        [(gas_id, op::GAS, true), (sub_id, op::SUB, true), (call_id, opcode, false)]
    {
        let mut instruction = ir::Instruction::from(ir::InstKind::Op(opcode));
        instruction.keep_with_next = keep_with_next;
        debug::instructions(
            context,
            &function.inst(id).metadata,
            std::slice::from_mut(&mut instruction),
        );
        output.push(instruction);
    }
    stack.truncate(stack.values().len() - prepared.len() - saved.tracked);
    // <saved homes>; call_result
    // restore homes, preserving call_result
    restore_writer_homes(&saved, output, true);
    record_result(context, call_id, stack, output, true)?;
    Ok(true)
}

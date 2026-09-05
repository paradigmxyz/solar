//! Bounded selection of canonical stacks around MIR control-flow cycles.
//!
//! Candidate layouts are simulated with the same physical scheduler used by
//! lowering, including live operand preservation and simultaneous phi transfers.
//! A reversed cycle-wide candidate avoids a coordinate-descent local minimum;
//! subsequent block reversals and adjacent exchanges refine it. At most 64
//! candidates are evaluated for functions with at most 128 blocks and 1024
//! instructions. Cyclic blocks and edges receive weight eight, while cold exits
//! receive weight one. Both weighted stack gas and static stack bytes must improve
//! or remain equal. This is a scheduling estimate, not a runtime frequency proof.
//!
//! Calls, non-opcode lowering, returning functions and spill protocols deliberately
//! keep their original layouts, including the private return-address prefix. No
//! MIR instruction or physical EVM IR is changed by this analysis.

use super::{ir, op, scheduler::Stack};
use crate::{analysis::{CfgInfo, Liveness}, mir};
use solar_config::EvmVersion;
use solar_data_structures::index::IndexVec;

/// Chooses canonical value orders using bounded whole-function scheduling trials.
pub(crate) fn improve(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    version: EvmVersion,
    returning: bool,
    entries: &mut IndexVec<mir::BlockId, Vec<mir::ValueId>>,
    stored: impl Fn(mir::ValueId) -> bool,
) {
    if returning || function.blocks.len() > 128 || function.instructions().count() > 1024
        || cfg.cyclic_blocks().is_empty() {
        return;
    }
    let score = |entries: &IndexVec<mir::BlockId, Vec<mir::ValueId>>| {
        score(function, live, cfg, version, entries, &stored)
    };
    let Some(mut best) = score(entries) else { return };
    let ids = cfg.cyclic_blocks().iter().filter(|&id| (2..=6).contains(&entries[id].len())).collect::<Vec<_>>();
    if ids.is_empty() { return; }
    let mut attempts = 0;
    let mut consider = |candidate: IndexVec<mir::BlockId, Vec<mir::ValueId>>, entries: &mut IndexVec<mir::BlockId, Vec<mir::ValueId>>| {
        if attempts == 64 { return; }
        attempts += 1;
        if let Some(cost) = score(&candidate)
            && cost.0 <= best.0 && cost.1 <= best.1 && cost != best {
            *entries = candidate;
            best = cost;
        }
    };
    if ids.len() <= 6 && ids.iter().all(|&id| entries[id].len() == 2) {
        let original = entries.clone();
        for mask in 1usize..1 << ids.len() {
            let mut candidate = original.clone();
            for (bit, &id) in ids.iter().enumerate() {
                if mask & (1 << bit) != 0 { candidate[id].reverse(); }
            }
            consider(candidate, entries);
        }
        return;
    }
    let mut reversed = entries.clone();
    for &id in &ids { reversed[id].reverse(); }
    consider(reversed, entries);
    for _ in 0..2 {
        for &id in &ids {
            let mut candidate = entries.clone();
            candidate[id].reverse();
            consider(candidate, entries);
            for position in 1..entries[id].len() {
                let mut candidate = entries.clone();
                candidate[id].swap(position - 1, position);
                consider(candidate, entries);
            }
        }
    }
}

fn score(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    version: EvmVersion,
    entries: &IndexVec<mir::BlockId, Vec<mir::ValueId>>,
    stored: &impl Fn(mir::ValueId) -> bool,
) -> Option<(usize, usize)> {
    let mut gas = 0;
    let mut bytes = 0;
    for &id in cfg.rpo() {
        let block = &function.blocks[id];
        let weight = if cfg.cyclic_blocks().contains(id) { 8 } else { 1 };
        let mut stack = Stack::new(entries[id].clone());
        let mut operations = Vec::new();
        for (position, &inst) in block.instructions.iter().enumerate() {
            let kind = &function.inst(inst).kind;
            if matches!(kind, mir::InstKind::Phi(_)) { continue; }
            let opcode = kind.evm_opcode()?;
            let operands = kind.operands();
            prepare(function, &mut stack, &mut operations, &operands, version, stored, |value| {
                live.is_used_at_or_after(value, id, position + 1)
            })?;
            // <scheduled operands>; opcode
            operations.push(ir::InstKind::Op(opcode).into());
            stack.truncate(stack.values().len().checked_sub(operands.len())?);
            if let Some(result) = function.inst_result_value(inst) {
                stack.push(result);
            } else if op::stack_io(opcode)?.1 != 0 {
                operations.push(ir::InstKind::Op(op::POP).into());
            }
        }
        match block.terminator.as_ref()? {
            mir::Terminator::Branch { condition, .. } => {
                prepare(function, &mut stack, &mut operations, &[*condition], version, stored, |value| live.live_out(id).contains(value))?;
                stack.truncate(stack.values().len().checked_sub(1)?);
            }
            mir::Terminator::ReturnData { offset, size } | mir::Terminator::Revert { offset, size } => {
                prepare(function, &mut stack, &mut operations, &[*offset, *size], version, stored, |_| false)?;
            }
            mir::Terminator::SelfDestruct { recipient } => {
                prepare(function, &mut stack, &mut operations, &[*recipient], version, stored, |_| false)?;
            }
            mir::Terminator::Jump(_) | mir::Terminator::Stop | mir::Terminator::Invalid => {}
            _ => return None,
        }
        let block_cost = ir::scheduling_cost(version, &operations);
        gas += block_cost.0 * weight;
        bytes += block_cost.1;
        for &successor in cfg.successors(id) {
            let mut target = entries[successor].clone();
            for value in &mut target {
                if let mir::Value::Inst(inst) = function.value(*value)
                    && function.blocks[successor].instructions.contains(inst)
                    && let mir::InstKind::Phi(incoming) = &function.inst(*inst).kind {
                    *value = incoming.iter().find(|(predecessor, _)| *predecessor == id)?.1;
                }
            }
            let mut edge = stack.clone();
            let mut operations = Vec::new();
            materialize(function, &mut edge, &mut operations, &target, stored)?;
            operations.extend(edge.reconcile(&target, 0, version).ok()?);
            let cost = ir::scheduling_cost(version, &operations);
            gas += cost.0 * if cfg.cyclic_blocks().contains(id) && cfg.cyclic_blocks().contains(successor) { 8 } else { 1 };
            bytes += cost.1;
        }
    }
    Some((gas, bytes))
}

fn prepare(
    function: &mir::Function,
    stack: &mut Stack<mir::ValueId>,
    operations: &mut Vec<ir::Instruction>,
    operands: &[mir::ValueId],
    version: EvmVersion,
    stored: &impl Fn(mir::ValueId) -> bool,
    live: impl Fn(mir::ValueId) -> bool,
) -> Option<()> {
    materialize(function, stack, operations, operands, stored)?;
    operations.extend(stack.prepare(operands, 0, version, |value| stored(value) && live(value)).ok()?);
    Some(())
}

fn materialize(
    function: &mir::Function,
    stack: &mut Stack<mir::ValueId>,
    operations: &mut Vec<ir::Instruction>,
    operands: &[mir::ValueId],
    stored: &impl Fn(mir::ValueId) -> bool,
) -> Option<()> {
    for &value in operands.iter().rev() {
        if stack.values().contains(&value) { continue; }
        if stored(value) { return None; }
        match function.value(value) {
            mir::Value::Immediate(immediate) => operations.push(ir::InstKind::Push(immediate.as_u256()?).into()),
            mir::Value::Undef(_) => operations.push(ir::InstKind::Push(alloy_primitives::U256::ZERO).into()),
            mir::Value::Arg(index) => {
                operations.push(ir::InstKind::Push(alloy_primitives::U256::from(4 + index.index() * 32)).into());
                operations.push(ir::InstKind::Op(op::CALLDATALOAD).into());
            }
            _ => return None,
        }
        stack.push(value);
    }
    Some(())
}


//! Reuse of live physical expression results and known memory words.
//!
//! Interned expression keys distinguish literals, relocations, operand order and
//! effect epochs. A repeated expression is replaced only when its result remains
//! directly accessible on the stack and its adjacent operand copies can be
//! removed. Known constant-address memory words survive disjoint word stores;
//! overlapping stores, opaque operations and calls invalidate affected facts.
//! Volatile environment queries are never commoned. This pass stays within a
//! block and does not rematerialize values or raise the operand stack peak.

use super::super::{InstKind, Instruction, verify};
use super::{canonical, discardable_push, pure, stack::stack_step, swapped};
use crate::backend::evm::op;
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;

/// A physical expression key; mutable reads carry the current effect epoch.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Expression {
    Push(InstKind),
    Op(u8, Vec<usize>, usize),
}

pub(super) fn common_expressions(insts: &mut Vec<Instruction>, version: EvmVersion) -> bool {
    let original = std::mem::take(insts);
    let mut stack = Vec::new();
    let mut expressions = FxHashMap::<Expression, usize>::default();
    let mut constants = FxHashMap::<usize, U256>::default();
    let mut memory = FxHashMap::<U256, usize>::default();
    let mut next = 0;
    let mut epoch = 0;
    for inst in &original {
        if !canonical(inst) {
            stack.clear();
            memory.clear();
            epoch += 1;
            insts.push(inst.clone());
            continue;
        }
        if stack_step(&mut stack, &inst.kind) {
            insts.push(inst.clone());
            continue;
        }
        if matches!(inst.kind, InstKind::Dup(_) | InstKind::Swap(_) | InstKind::Exchange(..)) {
            stack.clear();
            insts.push(inst.clone());
            continue;
        }
        let Some((inputs, outputs)) = verify::effect(&inst.kind).or(inst.stack_effect) else {
            stack.clear();
            memory.clear();
            epoch += 1;
            insts.push(inst.clone());
            continue;
        };
        while stack.len() < usize::from(inputs) {
            stack.insert(0, next);
            next += 1;
        }
        let arguments = stack.drain(stack.len() - usize::from(inputs)..).rev().collect::<Vec<_>>();
        let address = arguments.first().and_then(|id| constants.get(id)).copied();
        let tail_copies = insts.len() >= usize::from(inputs)
            && insts[insts.len() - usize::from(inputs)..].iter().all(|inst| {
                canonical(inst)
                    && (discardable_push(&inst.kind) || matches!(inst.kind, InstKind::Dup(_)))
            });
        if let InstKind::Op(op::MSTORE) = inst.kind {
            if let Some(address) = address
                && memory.get(&address) == arguments.get(1)
                && tail_copies
            {
                // <address and value copies>; mstore -> identity
                insts.truncate(insts.len() - 2);
                continue;
            }
        }
        let key = match &inst.kind {
            InstKind::Push(_)
            | InstKind::PushLabel(_)
            | InstKind::PushData { .. }
            | InstKind::PushImmutable { .. } => Some(Expression::Push(inst.kind.clone())),
            InstKind::Op(code)
                if pure(*code)
                    || stable(*code)
                    || matches!(*code, op::MLOAD | op::SLOAD | op::TLOAD) =>
            {
                let mut operands = arguments.clone();
                if swapped(*code) == Some(*code) {
                    operands.sort_unstable();
                }
                Some(Expression::Op(
                    *code,
                    operands,
                    if matches!(*code, op::MLOAD | op::SLOAD | op::TLOAD) { epoch } else { 0 },
                ))
            }
            _ => None,
        };
        let known = if matches!(inst.kind, InstKind::Op(op::MLOAD)) {
            address.and_then(|address| memory.get(&address).copied())
        } else {
            None
        };
        let existing = known.or_else(|| key.as_ref().and_then(|key| expressions.get(key).copied()));
        let reuse = !matches!(inst.kind, InstKind::Push(value) if value.is_zero())
            && !matches!(inst.kind, InstKind::PushImmutable { id, .. } if id.index() == 0);
        let reused = if outputs == 1
            && tail_copies
            && reuse
            && let Some(value) = existing
            && let Some(index) = stack.iter().rposition(|&other| other == value)
            && stack.len() - index <= version.reachable_stack_depth()
        {
            // <operand copies>; <repeated expression> -> dup existing_value
            insts.truncate(insts.len() - usize::from(inputs));
            insts.push(InstKind::Dup((stack.len() - index) as u16).into());
            stack.push(value);
            true
        } else {
            false
        };
        if !reused {
            insts.push(inst.clone());
            for _ in 0..outputs {
                let value = if outputs == 1 { existing.unwrap_or(next) } else { next };
                if value == next {
                    next += 1;
                }
                if let Some(key) = &key {
                    expressions.insert(key.clone(), value);
                }
                if let InstKind::Push(constant) = inst.kind {
                    constants.insert(value, constant);
                }
                stack.push(value);
            }
        }
        if let InstKind::Op(code) = inst.kind
            && !pure(code)
            && !stable(code)
            && !matches!(code, op::MLOAD | op::SLOAD | op::TLOAD | op::POP)
        {
            epoch += 1;
            if code == op::MSTORE
                && let Some(address) = address
                && let Some(end) = address.checked_add(U256::from(32))
            {
                memory.retain(|&other, _| {
                    other
                        .checked_add(U256::from(32))
                        .is_some_and(|other_end| other_end <= address || end <= other)
                });
                memory.insert(address, arguments[1]);
            } else {
                memory.clear();
            }
        }
    }
    *insts != original
}

fn stable(opcode: u8) -> bool {
    matches!(
        opcode,
        op::ADDRESS
            | op::ORIGIN
            | op::CALLER
            | op::CALLVALUE
            | op::CALLDATASIZE
            | op::CALLDATALOAD
            | op::CODESIZE
            | op::GASPRICE
            | op::COINBASE
            | op::TIMESTAMP
            | op::NUMBER
            | op::PREVRANDAO
            | op::GASLIMIT
            | op::CHAINID
            | op::BASEFEE
            | op::BLOBBASEFEE
            | op::BLOBHASH
            | op::SLOTNUM
    )
}

//! Block-local common-subexpression regeneration over scheduled EVM IR.

use super::EvmPass;
use crate::backend::evm::{
    ir::{Instruction, Module, PushValue, default_instruction_stack_effect},
    op,
};
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;
use solar_sema::Gcx;

pub(super) struct BlockCse;

impl EvmPass for BlockCse {
    fn name(&self) -> &'static str {
        "block-cse"
    }

    fn run_pass(&self, _gcx: Gcx<'_>, module: &mut Module) -> bool {
        let mut changed = false;
        for block in &mut module.blocks {
            changed |= regenerate_block(&mut block.instructions);
        }
        changed
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum Expr {
    Push(u8, u8, PushValue),
    Op(u8, SmallVec<[usize; 3]>),
    Read(u8, u64, SmallVec<[usize; 3]>),
}

#[derive(Clone, Copy)]
struct StackValue {
    expr: usize,
    /// A closed instruction interval that produces only this value.
    span: Option<(usize, usize)>,
}

fn regenerate_block(instructions: &mut Vec<Instruction>) -> bool {
    if !may_regenerate(instructions) {
        return false;
    }

    let original = std::mem::take(instructions);
    instructions.reserve(original.len());

    let mut stack = Vec::<StackValue>::new();
    let mut expressions = FxHashMap::<Expr, usize>::default();
    let mut next_expr = 0usize;
    let mut memory_epoch = 0u64;
    let mut storage_epoch = 0u64;
    let mut changed = false;

    for inst in original {
        if inst.is_encoded_push() {
            let Some(value) = inst.value else {
                append_unknown(inst, instructions, &mut stack, &mut next_expr);
                continue;
            };
            let expr = intern(
                Expr::Push(inst.opcode, inst.encoding, value),
                &mut expressions,
                &mut next_expr,
            );
            if inst.deferred_push().is_none()
                && inst.pushed_value().is_some_and(|value| !value.is_zero())
                && let Some(depth) = stack.iter().rev().position(|value| value.expr == expr)
                && depth < 16
            {
                let mut duplicate = Instruction::opcode(op::dup((depth + 1) as u8));
                duplicate.metadata = inst.metadata;
                duplicate.metadata.stack = None;
                instructions.push(duplicate);
                stack.push(StackValue { expr, span: None });
                changed = true;
                continue;
            }
            let start = instructions.len();
            instructions.push(inst);
            stack.push(StackValue { expr, span: Some((start, start + 1)) });
            continue;
        }

        let opcode = inst.opcode;
        if (op::DUP1..=op::DUP16).contains(&opcode) {
            let depth = usize::from(opcode - op::DUP1 + 1);
            ensure_depth(&mut stack, depth, &mut next_expr);
            let value = stack[stack.len() - depth];
            instructions.push(inst);
            stack.push(StackValue { expr: value.expr, span: None });
            continue;
        }
        if (op::SWAP1..=op::SWAP16).contains(&opcode) {
            let depth = usize::from(opcode - op::SWAP1 + 1);
            ensure_depth(&mut stack, depth + 1, &mut next_expr);
            let top = stack.len() - 1;
            stack.swap(top, top - depth);
            instructions.push(inst);
            continue;
        }
        if opcode == op::POP {
            ensure_depth(&mut stack, 1, &mut next_expr);
            stack.pop();
            instructions.push(inst);
            continue;
        }

        if let Some((inputs, read_epoch)) = expression_inputs(opcode, memory_epoch, storage_epoch) {
            ensure_depth(&mut stack, inputs, &mut next_expr);
            let mut operands = SmallVec::<[StackValue; 3]>::new();
            for _ in 0..inputs {
                operands.push(stack.pop().expect("stack depth was extended"));
            }
            let mut operand_exprs: SmallVec<[usize; 3]> =
                operands.iter().map(|value| value.expr).collect();
            if is_commutative(opcode) {
                operand_exprs.sort_unstable();
            }
            let expression = if let Some(epoch) = read_epoch {
                Expr::Read(opcode, epoch, operand_exprs)
            } else {
                Expr::Op(opcode, operand_exprs)
            };
            let expr = intern(expression, &mut expressions, &mut next_expr);

            let closed_span = closed_operand_span(&operands, instructions.len());
            if let Some((start, _)) = closed_span
                && instructions.len() + 1 - start > 1
                && let Some(depth) = stack.iter().rev().position(|value| value.expr == expr)
                && depth < 16
            {
                // The removed interval contains at least one pure opcode (gas
                // >= DUPn) plus another instruction. It is therefore strictly
                // smaller and cannot cost more runtime gas than the DUPn.
                instructions.truncate(start);
                let mut duplicate = Instruction::opcode(op::dup((depth + 1) as u8));
                duplicate.metadata = inst.metadata;
                duplicate.metadata.stack = None;
                instructions.push(duplicate);
                stack.push(StackValue { expr, span: None });
                changed = true;
            } else {
                let start = closed_span.map(|span| span.0);
                instructions.push(inst);
                stack.push(StackValue {
                    expr,
                    span: start.map(|start| (start, instructions.len())),
                });
            }
            continue;
        }

        let effect = default_instruction_stack_effect(&inst);
        instructions.push(inst);
        if clobbers_memory(opcode) {
            memory_epoch = memory_epoch.wrapping_add(1);
        }
        if clobbers_storage(opcode) {
            storage_epoch = storage_epoch.wrapping_add(1);
        }
        if let Some(effect) = effect {
            let inputs = usize::from(effect.inputs);
            ensure_depth(&mut stack, inputs, &mut next_expr);
            stack.truncate(stack.len() - inputs);
            for _ in 0..effect.outputs {
                stack.push(StackValue { expr: fresh(&mut next_expr), span: None });
            }
        } else {
            // An unknown stack effect is a hard analysis boundary. Values
            // below it may still exist physically, but cannot safely satisfy
            // a later expression lookup.
            stack.clear();
        }
    }

    changed
}

/// Returns whether a block contains two operations that could possibly intern
/// to the same expression. This linear, allocation-free screen avoids building
/// expression tables for the common case where regeneration cannot change the
/// block.
fn may_regenerate(instructions: &[Instruction]) -> bool {
    let mut seen = [false; 256];
    for inst in instructions {
        let candidate = if inst.is_encoded_push() {
            inst.deferred_push().is_none()
                && inst.pushed_value().is_some_and(|value| !value.is_zero())
        } else {
            expression_inputs(inst.opcode, 0, 0).is_some()
        };
        if candidate {
            let opcode = usize::from(inst.opcode);
            if seen[opcode] {
                return true;
            }
            seen[opcode] = true;
        }
    }
    false
}

fn append_unknown(
    inst: Instruction,
    instructions: &mut Vec<Instruction>,
    stack: &mut Vec<StackValue>,
    next_expr: &mut usize,
) {
    instructions.push(inst);
    stack.push(StackValue { expr: fresh(next_expr), span: None });
}

fn closed_operand_span(operands: &[StackValue], instruction_end: usize) -> Option<(usize, usize)> {
    let mut spans: SmallVec<[(usize, usize); 3]> =
        operands.iter().map(|value| value.span).collect::<Option<_>>()?;
    spans.sort_unstable();
    let start = spans.first()?.0;
    let mut cursor = start;
    for (span_start, span_end) in spans {
        if span_start != cursor {
            return None;
        }
        cursor = span_end;
    }
    (cursor == instruction_end).then_some((start, instruction_end + 1))
}

fn ensure_depth(stack: &mut Vec<StackValue>, depth: usize, next_expr: &mut usize) {
    let missing = depth.saturating_sub(stack.len());
    if missing != 0 {
        stack.splice(0..0, (0..missing).map(|_| StackValue { expr: fresh(next_expr), span: None }));
    }
}

fn intern(expr: Expr, expressions: &mut FxHashMap<Expr, usize>, next_expr: &mut usize) -> usize {
    if let Some(&id) = expressions.get(&expr) {
        return id;
    }
    let id = fresh(next_expr);
    expressions.insert(expr, id);
    id
}

fn fresh(next_expr: &mut usize) -> usize {
    let id = *next_expr;
    *next_expr += 1;
    id
}

const fn expression_inputs(
    opcode: u8,
    memory_epoch: u64,
    storage_epoch: u64,
) -> Option<(usize, Option<u64>)> {
    let pure = match opcode {
        op::ISZERO | op::NOT => Some(1),
        op::ADD
        | op::MUL
        | op::SUB
        | op::DIV
        | op::SDIV
        | op::MOD
        | op::SMOD
        | op::EXP
        | op::SIGNEXTEND
        | op::LT
        | op::GT
        | op::SLT
        | op::SGT
        | op::EQ
        | op::AND
        | op::OR
        | op::XOR
        | op::BYTE
        | op::SHL
        | op::SHR
        | op::SAR => Some(2),
        op::ADDMOD | op::MULMOD => Some(3),
        _ => None,
    };
    if let Some(inputs) = pure {
        return Some((inputs, None));
    }
    match opcode {
        op::MLOAD => Some((1, Some(memory_epoch << 1))),
        op::KECCAK256 => Some((2, Some(memory_epoch << 1))),
        op::SLOAD | op::TLOAD => Some((1, Some((storage_epoch << 1) | 1))),
        op::CALLDATALOAD => Some((1, Some(0))),
        _ => None,
    }
}

const fn clobbers_memory(opcode: u8) -> bool {
    matches!(
        opcode,
        op::MSTORE
            | op::MSTORE8
            | op::MCOPY
            | op::CALLDATACOPY
            | op::CODECOPY
            | op::EXTCODECOPY
            | op::RETURNDATACOPY
            | op::CALL
            | op::CALLCODE
            | op::DELEGATECALL
            | op::STATICCALL
    )
}

const fn clobbers_storage(opcode: u8) -> bool {
    matches!(
        opcode,
        op::SSTORE
            | op::TSTORE
            | op::CALL
            | op::CALLCODE
            | op::DELEGATECALL
            | op::STATICCALL
            | op::CREATE
            | op::CREATE2
    )
}

const fn is_commutative(opcode: u8) -> bool {
    matches!(opcode, op::ADD | op::MUL | op::EQ | op::AND | op::OR | op::XOR)
}

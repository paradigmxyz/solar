//! Local peephole optimization over scheduled EVM IR.

use crate::backend::evm::{
    ir::{Instruction, Module, Operand},
    opcode as op,
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        if block.instructions.iter().any(|inst| {
            inst.result.is_some() || (!inst.is_encoded_push() && !inst.operands.is_empty())
        }) {
            continue;
        }
        changed |= optimize(&mut block.instructions, &mut scratch) != 0;
    }
    changed
}

fn optimize(instructions: &mut Vec<Instruction>, scratch: &mut Vec<Instruction>) -> usize {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());
    let mut rewrites = 0;
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while let Some(rewrite) = try_peephole(instructions) {
            instructions.truncate(instructions.len() - rewrite.skip);
            instructions.extend(rewrite.replacement);
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole(instructions: &[Instruction]) -> Option<Rewrite> {
    let stack = InstStack::new(instructions);

    if stack.len() >= 3
        && is_removable_push(&stack[2])
        && let (Some(value), Some(opcode)) = (push_value(&stack[1]), raw_opcode(&stack[0]))
    {
        if value.is_zero()
            && matches!(
                opcode,
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT
            )
        {
            return Some(Rewrite::replace(3, [push(U256::ZERO)]));
        }
        if value == U256::ONE && opcode == op::EXP {
            return Some(Rewrite::replace(3, [push(U256::ONE)]));
        }
    }

    if stack.len() >= 2
        && let (Some(value), Some(opcode)) = (push_value(&stack[1]), raw_opcode(&stack[0]))
    {
        if value.is_zero() {
            return match opcode {
                op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => {
                    Some(Rewrite::delete(2))
                }
                op::EQ => Some(Rewrite::replace(2, [raw(op::ISZERO)])),
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                    Some(Rewrite::replace(2, [raw(op::POP), push(U256::ZERO)]))
                }
                _ => None,
            };
        }
        if value == U256::ONE {
            return match opcode {
                op::MUL => Some(Rewrite::delete(2)),
                op::EXP => Some(Rewrite::replace(2, [raw(op::POP), push(U256::ONE)])),
                _ => None,
            };
        }
    }

    if stack.len() >= 2 && is_removable_push(&stack[1]) && raw_opcode(&stack[0]) == Some(op::POP) {
        return Some(Rewrite::delete(2));
    }

    if stack.len() >= 2
        && let (Some(a), Some(b)) = (raw_opcode(&stack[1]), raw_opcode(&stack[0]))
        && ((a, b) == (op::NOT, op::NOT)
            || (b == op::POP && (op::DUP1..=op::DUP16).contains(&a))
            || (a == b && (op::SWAP1..=op::SWAP16).contains(&a)))
    {
        return Some(Rewrite::delete(2));
    }

    if stack.len() >= 3
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[1]) == Some(op::ISZERO)
        && raw_opcode(&stack[0]) == Some(op::ISZERO)
    {
        return Some(Rewrite::replace(3, [raw(op::ISZERO)]));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::POP)
        && raw_opcode(&stack[1]) == Some(op::SWAP1)
        && raw_opcode(&stack[3]) == Some(op::DUP2)
        && let Some(binop) = raw_opcode(&stack[2])
    {
        if matches!(binop, op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ) {
            return Some(Rewrite::replace(4, [raw(binop)]));
        }
        if matches!(
            binop,
            op::SUB
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
                | op::BYTE
                | op::SHL
                | op::SHR
                | op::SAR
                | op::KECCAK256
        ) {
            return Some(Rewrite::replace(4, [raw(op::SWAP1), raw(binop)]));
        }
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::POP)
        && raw_opcode(&stack[1]) == Some(op::SWAP1)
        && raw_opcode(&stack[2]) == Some(op::POP)
        && raw_opcode(&stack[3]) == Some(op::SWAP1)
    {
        return Some(Rewrite::replace(4, [raw(op::SWAP2), raw(op::POP), raw(op::POP)]));
    }

    if stack.len() >= 6
        && raw_opcode(&stack[0]) == Some(op::MSTORE)
        && raw_opcode(&stack[2]) == Some(op::DUP1)
        && raw_opcode(&stack[3]) == Some(op::MSTORE)
        && raw_opcode(&stack[5]) == Some(op::DUP1)
        && let (Some(a), Some(b)) = (push_value(&stack[1]), push_value(&stack[4]))
        && a == b
    {
        return Some(Rewrite::replace(6, [stack[5].clone(), stack[4].clone(), stack[3].clone()]));
    }

    if stack.len() >= 6
        && raw_opcode(&stack[0]) == Some(op::MLOAD)
        && raw_opcode(&stack[2]) == Some(op::POP)
        && raw_opcode(&stack[3]) == Some(op::MSTORE)
        && raw_opcode(&stack[5]) == Some(op::DUP1)
        && let (Some(a), Some(b)) = (push_value(&stack[1]), push_value(&stack[4]))
        && a == b
    {
        return Some(Rewrite::replace(6, [stack[5].clone(), stack[4].clone(), stack[3].clone()]));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::JUMPI)
        && is_block_push(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[3]) == Some(op::ISZERO)
    {
        return Some(Rewrite::replace(4, [stack[1].clone(), raw(op::JUMPI)]));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::JUMPI)
        && is_block_push(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[3]) == Some(op::EQ)
    {
        return Some(Rewrite::replace(4, [raw(op::SUB), stack[1].clone(), raw(op::JUMPI)]));
    }

    None
}

fn raw(opcode: u8) -> Instruction {
    Instruction::opcode(opcode)
}

fn push(value: U256) -> Instruction {
    Instruction::push(Operand::Immediate(value))
}

fn raw_opcode(inst: &Instruction) -> Option<u8> {
    (!inst.is_encoded_push()).then_some(inst.opcode)
}

fn push_value(inst: &Instruction) -> Option<U256> {
    if !inst.is_encoded_push() || inst.is_deferred_push() || inst.is_immutable_push() {
        return None;
    }
    match inst.operands.as_slice() {
        [Operand::Immediate(value)] => Some(*value),
        _ => None,
    }
}

fn is_block_push(inst: &Instruction) -> bool {
    inst.is_encoded_push() && matches!(inst.operands.as_slice(), [Operand::Block(_)])
}

fn is_removable_push(inst: &Instruction) -> bool {
    inst.is_encoded_push() && !inst.is_deferred_push()
}

struct Rewrite {
    skip: usize,
    replacement: ArrayVec<Instruction, 3>,
}

impl Rewrite {
    fn delete(skip: usize) -> Self {
        Self { skip, replacement: ArrayVec::new() }
    }

    fn replace<const N: usize>(skip: usize, replacement: [Instruction; N]) -> Self {
        debug_assert!(N <= skip);
        Self { skip, replacement: replacement.into_iter().collect() }
    }
}

#[derive(Clone, Copy)]
struct InstStack<'a> {
    instructions: &'a [Instruction],
}

impl<'a> InstStack<'a> {
    fn new(instructions: &'a [Instruction]) -> Self {
        Self { instructions }
    }

    fn len(self) -> usize {
        self.instructions.len()
    }
}

impl std::ops::Index<usize> for InstStack<'_> {
    type Output = Instruction;

    fn index(&self, index: usize) -> &Self::Output {
        &self.instructions[self.instructions.len() - 1 - index]
    }
}

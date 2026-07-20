//! Local peephole optimization over scheduled EVM IR.

use crate::backend::evm::{
    ir::{Instruction, Module, Operand},
    opcode as op,
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;
use solar_interface::Symbol;
use std::fmt;
use tracing::trace;

const TRACE_TARGET: &str = "solar::codegen::evm_ir::peephole";
const RULES: &[&str] = &[
    "zero_absorbing_dead_lhs",
    "one_exp_dead_lhs",
    "zero_identity",
    "zero_eq",
    "zero_absorbing",
    "one_mul",
    "one_exp",
    "pop_push",
    "double_not",
    "dup_pop",
    "double_swap",
    "triple_iszero",
    "dup2_commutative_pop",
    "dup2_binop_pop",
    "dup2_sink_pop",
    "double_swap_pop",
    "duplicate_mstore",
    "store_load_pop",
    "double_iszero_jumpi",
    "eq_iszero_jumpi",
];

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
    trace!(
        target: TRACE_TARGET,
        rules = %RuleSequence(RULES),
        "evm_ir_peephole_rules"
    );
    let mut changed = false;
    let mut scratch = Vec::new();
    for block in &mut module.blocks {
        if block.instructions.iter().any(|inst| {
            inst.result.is_some() || (!inst.is_encoded_push() && !inst.operands.is_empty())
        }) {
            continue;
        }
        trace!(
            target: TRACE_TARGET,
            module = %module.name,
            block = block.label,
            instructions = %InstructionSequence(&block.instructions),
            "evm_ir_peephole_input"
        );
        changed |= optimize(&mut block.instructions, &mut scratch, module.name, block.label) != 0;
    }
    changed
}

fn optimize(
    instructions: &mut Vec<Instruction>,
    scratch: &mut Vec<Instruction>,
    module: Symbol,
    block: u32,
) -> usize {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());
    let mut rewrites = 0;
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while let Some(rewrite) = try_peephole(instructions) {
            let input_start = instructions.len() - rewrite.skip;
            trace!(
                target: TRACE_TARGET,
                module = %module,
                block,
                rule = rewrite.rule,
                input = %InstructionSequence(&instructions[input_start..]),
                output = %InstructionSequence(&rewrite.replacement),
                "evm_ir_peephole_rewrite"
            );
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
        && let Some(opcode) = raw_opcode(&stack[0])
        && let Some(value) = push_value(&stack[1])
        && is_removable_push(&stack[2])
    {
        if value.is_zero()
            && matches!(
                opcode,
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT
            )
        {
            return Some(Rewrite::replace("zero_absorbing_dead_lhs", 3, [push(U256::ZERO)]));
        }
        if value == U256::ONE && opcode == op::EXP {
            return Some(Rewrite::replace("one_exp_dead_lhs", 3, [push(U256::ONE)]));
        }
    }

    if stack.len() >= 2
        && let Some(opcode) = raw_opcode(&stack[0])
        && let Some(value) = push_value(&stack[1])
    {
        if value.is_zero() {
            return match opcode {
                op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => {
                    Some(Rewrite::delete("zero_identity", 2))
                }
                op::EQ => Some(Rewrite::replace("zero_eq", 2, [raw(op::ISZERO)])),
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                    Some(Rewrite::replace("zero_absorbing", 2, [raw(op::POP), push(U256::ZERO)]))
                }
                _ => None,
            };
        }
        if value == U256::ONE {
            return match opcode {
                op::MUL => Some(Rewrite::delete("one_mul", 2)),
                op::EXP => Some(Rewrite::replace("one_exp", 2, [raw(op::POP), push(U256::ONE)])),
                _ => None,
            };
        }
    }

    if stack.len() >= 2 && raw_opcode(&stack[0]) == Some(op::POP) && is_removable_push(&stack[1]) {
        return Some(Rewrite::delete("pop_push", 2));
    }

    if stack.len() >= 2
        && let Some(b) = raw_opcode(&stack[0])
        && let Some(a) = raw_opcode(&stack[1])
        && ((a, b) == (op::NOT, op::NOT)
            || (b == op::POP && (op::DUP1..=op::DUP16).contains(&a))
            || (a == b && (op::SWAP1..=op::SWAP16).contains(&a)))
    {
        let rule = if (a, b) == (op::NOT, op::NOT) {
            "double_not"
        } else if b == op::POP {
            "dup_pop"
        } else {
            "double_swap"
        };
        return Some(Rewrite::delete(rule, 2));
    }

    if stack.len() >= 3
        && raw_opcode(&stack[0]) == Some(op::ISZERO)
        && raw_opcode(&stack[1]) == Some(op::ISZERO)
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
    {
        return Some(Rewrite::replace("triple_iszero", 3, [raw(op::ISZERO)]));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::POP)
        && raw_opcode(&stack[1]) == Some(op::SWAP1)
        && let Some(binop) = raw_opcode(&stack[2])
        && raw_opcode(&stack[3]) == Some(op::DUP2)
    {
        if matches!(binop, op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ) {
            return Some(Rewrite::replace("dup2_commutative_pop", 4, [raw(binop)]));
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
            return Some(Rewrite::replace("dup2_binop_pop", 4, [raw(op::SWAP1), raw(binop)]));
        }
    }

    if stack.len() >= 3
        && raw_opcode(&stack[0]) == Some(op::POP)
        && let Some(opcode) = raw_opcode(&stack[1])
        && matches!(opcode, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE | op::LOG0)
        && raw_opcode(&stack[2]) == Some(op::DUP2)
    {
        return Some(Rewrite::replace("dup2_sink_pop", 3, [raw(op::SWAP1), raw(opcode)]));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::POP)
        && raw_opcode(&stack[1]) == Some(op::SWAP1)
        && raw_opcode(&stack[2]) == Some(op::POP)
        && raw_opcode(&stack[3]) == Some(op::SWAP1)
    {
        return Some(Rewrite::replace(
            "double_swap_pop",
            4,
            [raw(op::SWAP2), raw(op::POP), raw(op::POP)],
        ));
    }

    if stack.len() >= 6
        && raw_opcode(&stack[0]) == Some(op::MSTORE)
        && let Some(a) = push_value(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::DUP1)
        && raw_opcode(&stack[3]) == Some(op::MSTORE)
        && let Some(b) = push_value(&stack[4])
        && raw_opcode(&stack[5]) == Some(op::DUP1)
        && a == b
    {
        return Some(Rewrite::replace(
            "duplicate_mstore",
            6,
            [stack[5].clone(), stack[4].clone(), stack[3].clone()],
        ));
    }

    if stack.len() >= 6
        && raw_opcode(&stack[0]) == Some(op::MLOAD)
        && let Some(a) = push_value(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::POP)
        && raw_opcode(&stack[3]) == Some(op::MSTORE)
        && let Some(b) = push_value(&stack[4])
        && raw_opcode(&stack[5]) == Some(op::DUP1)
        && a == b
    {
        return Some(Rewrite::replace(
            "store_load_pop",
            6,
            [stack[5].clone(), stack[4].clone(), stack[3].clone()],
        ));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::JUMPI)
        && is_block_push(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[3]) == Some(op::ISZERO)
    {
        return Some(Rewrite::replace(
            "double_iszero_jumpi",
            4,
            [stack[1].clone(), raw(op::JUMPI)],
        ));
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::JUMPI)
        && is_block_push(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[3]) == Some(op::EQ)
    {
        return Some(Rewrite::replace(
            "eq_iszero_jumpi",
            4,
            [raw(op::SUB), stack[1].clone(), raw(op::JUMPI)],
        ));
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
    rule: &'static str,
    skip: usize,
    replacement: ArrayVec<Instruction, 3>,
}

impl Rewrite {
    fn delete(rule: &'static str, skip: usize) -> Self {
        Self { rule, skip, replacement: ArrayVec::new() }
    }

    fn replace<const N: usize>(
        rule: &'static str,
        skip: usize,
        replacement: [Instruction; N],
    ) -> Self {
        debug_assert!(N <= skip);
        Self { rule, skip, replacement: replacement.into_iter().collect() }
    }
}

struct InstructionSequence<'a>(&'a [Instruction]);

struct RuleSequence<'a>(&'a [&'a str]);

impl fmt::Display for RuleSequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, rule) in self.0.iter().enumerate() {
            if index != 0 {
                f.write_str(",")?;
            }
            f.write_str(rule)?;
        }
        Ok(())
    }
}

impl fmt::Display for InstructionSequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, inst) in self.0.iter().enumerate() {
            if index != 0 {
                f.write_str(",")?;
            }
            if let Some(value) = push_value(inst) {
                if value.is_zero() {
                    f.write_str("PUSH0")?;
                } else if value == U256::ONE {
                    f.write_str("PUSH1")?;
                } else {
                    f.write_str("PUSH")?;
                }
            } else if inst.is_encoded_push() {
                f.write_str("PUSH_REF")?;
            } else if let Some(mnemonic) = op::mnemonic(inst.opcode) {
                f.write_str(mnemonic)?;
            } else {
                write!(f, "0x{:02x}", inst.opcode)?;
            }
        }
        Ok(())
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

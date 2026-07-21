//! Local peephole optimization over scheduled EVM IR.

use crate::backend::evm::{
    ir::{Instruction, Module, Operand},
    opcode as op,
};
use alloy_primitives::U256;
use solar_interface::Symbol;
use std::fmt;
use tracing::trace;

const TRACE_TARGET: &str = "solar::codegen::evm_ir::peephole";

pub(super) fn run(module: &mut Module, _options: super::PassOptions) -> bool {
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
            instructions = %PatternSequence(&block.instructions),
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
        while try_peephole(instructions, module, block) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole(instructions: &mut Vec<Instruction>, module: Symbol, block: u32) -> bool {
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
            return rewrite(instructions, 3, Edit::RemoveFirstKeepOne, module, block);
        }
        if value == U256::ONE && opcode == op::EXP {
            return rewrite(instructions, 3, Edit::RemoveFirstKeepOne, module, block);
        }
    }

    if stack.len() >= 2
        && let Some(opcode) = raw_opcode(&stack[0])
        && let Some(value) = push_value(&stack[1])
    {
        if value.is_zero() {
            return match opcode {
                op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => {
                    rewrite(instructions, 2, Edit::Keep(0), module, block)
                }
                op::EQ => {
                    rewrite(instructions, 2, Edit::RemoveFirstOverwrite(op::ISZERO), module, block)
                }
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                    rewrite(instructions, 2, Edit::SwapOverwrite(op::POP), module, block)
                }
                _ => false,
            };
        }
        if value == U256::ONE {
            return match opcode {
                op::MUL => rewrite(instructions, 2, Edit::Keep(0), module, block),
                op::EXP => rewrite(instructions, 2, Edit::SwapOverwrite(op::POP), module, block),
                _ => false,
            };
        }
    }

    if stack.len() >= 2 && raw_opcode(&stack[0]) == Some(op::POP) && is_removable_push(&stack[1]) {
        return rewrite(instructions, 2, Edit::Keep(0), module, block);
    }

    if stack.len() >= 2
        && let Some(b) = raw_opcode(&stack[0])
        && let Some(a) = raw_opcode(&stack[1])
        && ((a, b) == (op::NOT, op::NOT)
            || (b == op::POP && (op::DUP1..=op::DUP16).contains(&a))
            || (a == b && (op::SWAP1..=op::SWAP16).contains(&a)))
    {
        return rewrite(instructions, 2, Edit::Keep(0), module, block);
    }

    if stack.len() >= 3
        && raw_opcode(&stack[0]) == Some(op::ISZERO)
        && raw_opcode(&stack[1]) == Some(op::ISZERO)
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
    {
        return rewrite(instructions, 3, Edit::OverwriteOne(op::ISZERO), module, block);
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::POP)
        && raw_opcode(&stack[1]) == Some(op::SWAP1)
        && let Some(binop) = raw_opcode(&stack[2])
        && raw_opcode(&stack[3]) == Some(op::DUP2)
    {
        if matches!(binop, op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ) {
            return rewrite(instructions, 4, Edit::OverwriteOne(binop), module, block);
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
            return rewrite(instructions, 4, Edit::OverwriteTwo(binop), module, block);
        }
    }

    if stack.len() >= 3
        && raw_opcode(&stack[0]) == Some(op::POP)
        && let Some(opcode) = raw_opcode(&stack[1])
        && matches!(opcode, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE | op::LOG0)
        && raw_opcode(&stack[2]) == Some(op::DUP2)
    {
        return rewrite(instructions, 3, Edit::OverwriteTwo(opcode), module, block);
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::POP)
        && raw_opcode(&stack[1]) == Some(op::SWAP1)
    {
        for depth in 1..16 {
            let previous = depth + 2;
            if stack.len() <= previous {
                break;
            }
            if (2..previous).all(|index| raw_opcode(&stack[index]) == Some(op::POP))
                && raw_opcode(&stack[previous]) == Some(op::swap(depth as u8))
            {
                let depth = depth + 1;
                return rewrite(
                    instructions,
                    depth + 2,
                    Edit::MergeSwapPop(depth as u8),
                    module,
                    block,
                );
            }
        }
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
        return rewrite(instructions, 6, Edit::Keep(3), module, block);
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
        return rewrite(instructions, 6, Edit::Keep(3), module, block);
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::JUMPI)
        && is_block_push(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[3]) == Some(op::ISZERO)
    {
        return rewrite(instructions, 4, Edit::DropDoubleIszero, module, block);
    }

    if stack.len() >= 4
        && raw_opcode(&stack[0]) == Some(op::JUMPI)
        && is_block_push(&stack[1])
        && raw_opcode(&stack[2]) == Some(op::ISZERO)
        && raw_opcode(&stack[3]) == Some(op::EQ)
    {
        return rewrite(instructions, 4, Edit::EqIszeroJumpi, module, block);
    }

    false
}

// Keep trace formatting out of the hot matcher's stack frame.
#[inline(never)]
fn rewrite(
    instructions: &mut Vec<Instruction>,
    skip: usize,
    edit: Edit,
    module: Symbol,
    block: u32,
) -> bool {
    let start = instructions.len() - skip;
    let input = tracing::enabled!(target: TRACE_TARGET, tracing::Level::TRACE)
        .then(|| InstructionSequence(&instructions[start..]).to_string());
    edit.apply(instructions, start);
    if let Some(input) = input {
        trace!(
            target: TRACE_TARGET,
            module = %module,
            block,
            input,
            output = %InstructionSequence(&instructions[start..]),
            "evm_ir_peephole_rewrite"
        );
    }
    true
}

#[derive(Clone, Copy)]
enum Edit {
    Keep(u8),
    RemoveFirstKeepOne,
    RemoveFirstOverwrite(u8),
    SwapOverwrite(u8),
    OverwriteOne(u8),
    OverwriteTwo(u8),
    MergeSwapPop(u8),
    DropDoubleIszero,
    EqIszeroJumpi,
}

impl Edit {
    fn apply(self, instructions: &mut Vec<Instruction>, start: usize) {
        match self {
            Self::Keep(len) => instructions.truncate(start + usize::from(len)),
            Self::RemoveFirstKeepOne => {
                instructions.remove(start);
                instructions.truncate(start + 1);
            }
            Self::RemoveFirstOverwrite(opcode) => {
                instructions.remove(start);
                overwrite_raw(&mut instructions[start], opcode);
            }
            Self::SwapOverwrite(opcode) => {
                instructions.swap(start, start + 1);
                overwrite_raw(&mut instructions[start], opcode);
            }
            Self::OverwriteOne(opcode) => {
                overwrite_raw(&mut instructions[start], opcode);
                instructions.truncate(start + 1);
            }
            Self::OverwriteTwo(opcode) => {
                overwrite_raw(&mut instructions[start], op::SWAP1);
                overwrite_raw(&mut instructions[start + 1], opcode);
                instructions.truncate(start + 2);
            }
            Self::MergeSwapPop(depth) => {
                let end = instructions.len();
                overwrite_raw(&mut instructions[start], op::swap(depth));
                overwrite_raw(&mut instructions[end - 2], op::POP);
                instructions.truncate(end - 1);
            }
            Self::DropDoubleIszero => {
                instructions.drain(start..start + 2);
                overwrite_raw(&mut instructions[start + 1], op::JUMPI);
            }
            Self::EqIszeroJumpi => {
                overwrite_raw(&mut instructions[start], op::SUB);
                instructions.remove(start + 1);
                overwrite_raw(&mut instructions[start + 2], op::JUMPI);
            }
        }
    }
}

fn overwrite_raw(inst: &mut Instruction, opcode: u8) {
    debug_assert!(raw_opcode(inst).is_some());
    debug_assert!(inst.result.is_none() && inst.operands.is_empty());
    inst.opcode = opcode;
    inst.metadata.stack = None;
    inst.metadata.attrs.clear();
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

struct InstructionSequence<'a>(&'a [Instruction]);
struct PatternSequence<'a>(&'a [Instruction]);

impl fmt::Display for InstructionSequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, inst) in self.0.iter().enumerate() {
            if index != 0 {
                f.write_str(",")?;
            }
            if inst.is_deferred_push() {
                f.write_str("PUSH_DEFERRED")?;
            } else if inst.is_immutable_push() {
                f.write_str("PUSH_IMMUTABLE")?;
            } else if let Some(value) = push_value(inst) {
                if value.is_zero() {
                    f.write_str("PUSH0")?;
                } else {
                    write!(f, "PUSH{}({value:#x})", value.byte_len())?;
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

impl fmt::Display for PatternSequence<'_> {
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

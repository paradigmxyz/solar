//! Local peephole optimization over scheduled EVM IR.

use crate::backend::evm::{
    ir::{Instruction, Module, Operand},
    opcode as op,
};
use alloy_primitives::U256;
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
        changed |= optimize(&mut block.instructions, &mut scratch, block.label) != 0;
    }
    changed
}

fn optimize(
    instructions: &mut Vec<Instruction>,
    scratch: &mut Vec<Instruction>,
    block: u32,
) -> usize {
    scratch.clear();
    std::mem::swap(instructions, scratch);
    instructions.reserve(scratch.len());
    let mut rewrites = 0;
    for inst in scratch.drain(..) {
        instructions.push(inst);
        while try_peephole(instructions, block) {
            rewrites += 1;
        }
    }
    rewrites
}

fn try_peephole(instructions: &mut Vec<Instruction>, block: u32) -> bool {
    // `PUSH x PUSH 0 OP -> PUSH 0`.
    // `PUSH x PUSH 1 EXP -> PUSH 1`.
    if let [.., lhs, pushed, instruction] = instructions.as_slice()
        && is_removable_push(lhs)
        && let Some(value) = push_value(pushed)
        && let Some(opcode) = raw_opcode(instruction)
    {
        if value.is_zero()
            && matches!(
                opcode,
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT
            )
        {
            return rewrite(instructions, 3, Edit::RemoveFirstKeepOne, block);
        }
        if value == U256::ONE && opcode == op::EXP {
            return rewrite(instructions, 3, Edit::RemoveFirstKeepOne, block);
        }
    }

    // `PUSH 0 OP -> ∅`.
    // `PUSH 0 EQ -> ISZERO`.
    // `PUSH 0 OP -> POP PUSH 0`.
    // `PUSH 1 MUL -> ∅`.
    // `PUSH 1 EXP -> POP PUSH 1`.
    if let [.., pushed, instruction] = instructions.as_slice()
        && let Some(value) = push_value(pushed)
        && let Some(opcode) = raw_opcode(instruction)
    {
        if value.is_zero() {
            return match opcode {
                op::ADD | op::OR | op::XOR | op::SHL | op::SHR | op::SAR => {
                    rewrite(instructions, 2, Edit::Keep(0), block)
                }
                op::EQ => rewrite(instructions, 2, Edit::RemoveFirstOverwrite(op::ISZERO), block),
                op::MUL | op::DIV | op::SDIV | op::MOD | op::SMOD | op::AND | op::GT => {
                    rewrite(instructions, 2, Edit::SwapOverwrite(op::POP), block)
                }
                _ => false,
            };
        }
        if value == U256::ONE {
            return match opcode {
                op::MUL => rewrite(instructions, 2, Edit::Keep(0), block),
                op::EXP => rewrite(instructions, 2, Edit::SwapOverwrite(op::POP), block),
                _ => false,
            };
        }
    }

    // `PUSH x POP -> ∅`.
    if let [.., pushed, pop] = instructions.as_slice()
        && is_removable_push(pushed)
        && raw_opcode(pop) == Some(op::POP)
    {
        return rewrite(instructions, 2, Edit::Keep(0), block);
    }

    // `NOT NOT -> ∅`, `DUPn POP -> ∅`, or `SWAPn SWAPn -> ∅`.
    if let [.., first, second] = instructions.as_slice()
        && let Some(a) = raw_opcode(first)
        && let Some(b) = raw_opcode(second)
        && ((a, b) == (op::NOT, op::NOT)
            || (b == op::POP && (op::DUP1..=op::DUP16).contains(&a))
            || (a == b && (op::SWAP1..=op::SWAP16).contains(&a)))
    {
        return rewrite(instructions, 2, Edit::Keep(0), block);
    }

    // `ISZERO ISZERO ISZERO -> ISZERO`.
    if let [.., first, second, third] = instructions.as_slice()
        && raw_opcode(first) == Some(op::ISZERO)
        && raw_opcode(second) == Some(op::ISZERO)
        && raw_opcode(third) == Some(op::ISZERO)
    {
        return rewrite(instructions, 3, Edit::OverwriteOne(op::ISZERO), block);
    }

    // `DUP2 OP SWAP1 POP -> OP`.
    // `DUP2 OP SWAP1 POP -> SWAP1 OP`.
    if let [.., dup, binop, swap, pop] = instructions.as_slice()
        && raw_opcode(dup) == Some(op::DUP2)
        && let Some(binop) = raw_opcode(binop)
        && raw_opcode(swap) == Some(op::SWAP1)
        && raw_opcode(pop) == Some(op::POP)
    {
        if matches!(binop, op::ADD | op::MUL | op::AND | op::OR | op::XOR | op::EQ) {
            return rewrite(instructions, 4, Edit::OverwriteOne(binop), block);
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
            return rewrite(instructions, 4, Edit::OverwriteTwo(binop), block);
        }
    }

    // `DUP2 SINK POP -> SWAP1 SINK`.
    if let [.., dup, sink, pop] = instructions.as_slice()
        && raw_opcode(dup) == Some(op::DUP2)
        && let Some(opcode) = raw_opcode(sink)
        && matches!(opcode, op::MSTORE | op::MSTORE8 | op::SSTORE | op::TSTORE | op::LOG0)
        && raw_opcode(pop) == Some(op::POP)
    {
        return rewrite(instructions, 3, Edit::OverwriteTwo(opcode), block);
    }

    // `SWAPn POP*n SWAP1 POP -> SWAP(n+1) POP*(n+1)`.
    for depth in 1..16 {
        let input_len = depth + 3;
        let Some(start) = instructions.len().checked_sub(input_len) else {
            break;
        };
        if raw_opcode(&instructions[start]) == Some(op::swap(depth as u8))
            && instructions[start + 1..instructions.len() - 2]
                .iter()
                .all(|inst| raw_opcode(inst) == Some(op::POP))
            && raw_opcode(&instructions[instructions.len() - 2]) == Some(op::SWAP1)
            && raw_opcode(&instructions[instructions.len() - 1]) == Some(op::POP)
        {
            let merged_depth = depth + 1;
            return rewrite(instructions, input_len, Edit::MergeSwapPop(merged_depth as u8), block);
        }
    }

    // `DUP1 PUSH x MSTORE DUP1 PUSH x MSTORE -> DUP1 PUSH x MSTORE`.
    if let [.., dup_a, push_a, store_a, dup_b, push_b, store_b] = instructions.as_slice()
        && raw_opcode(dup_a) == Some(op::DUP1)
        && let Some(a) = push_value(push_a)
        && raw_opcode(store_a) == Some(op::MSTORE)
        && raw_opcode(dup_b) == Some(op::DUP1)
        && let Some(b) = push_value(push_b)
        && raw_opcode(store_b) == Some(op::MSTORE)
        && a == b
    {
        return rewrite(instructions, 6, Edit::Keep(3), block);
    }

    // `DUP1 PUSH x MSTORE POP PUSH x MLOAD -> DUP1 PUSH x MSTORE`.
    if let [.., dup, pushed, store, pop, loaded, load] = instructions.as_slice()
        && raw_opcode(dup) == Some(op::DUP1)
        && let Some(a) = push_value(pushed)
        && raw_opcode(store) == Some(op::MSTORE)
        && raw_opcode(pop) == Some(op::POP)
        && let Some(b) = push_value(loaded)
        && raw_opcode(load) == Some(op::MLOAD)
        && a == b
    {
        return rewrite(instructions, 6, Edit::Keep(3), block);
    }

    // `ISZERO ISZERO PUSH_REF JUMPI -> PUSH_REF JUMPI`.
    if let [.., first, second, target, jump] = instructions.as_slice()
        && raw_opcode(first) == Some(op::ISZERO)
        && raw_opcode(second) == Some(op::ISZERO)
        && is_block_push(target)
        && raw_opcode(jump) == Some(op::JUMPI)
    {
        return rewrite(instructions, 4, Edit::DropDoubleIszero, block);
    }

    // `EQ ISZERO PUSH_REF JUMPI -> SUB PUSH_REF JUMPI`.
    if let [.., eq, iszero, target, jump] = instructions.as_slice()
        && raw_opcode(eq) == Some(op::EQ)
        && raw_opcode(iszero) == Some(op::ISZERO)
        && is_block_push(target)
        && raw_opcode(jump) == Some(op::JUMPI)
    {
        return rewrite(instructions, 4, Edit::EqIszeroJumpi, block);
    }

    false
}

// Keep trace formatting out of the hot matcher's stack frame.
#[inline(never)]
fn rewrite(instructions: &mut Vec<Instruction>, skip: usize, edit: Edit, block: u32) -> bool {
    let start = instructions.len() - skip;
    let input = tracing::enabled!(target: TRACE_TARGET, tracing::Level::TRACE)
        .then(|| instructions[start..].to_vec());
    edit.apply(instructions, start);
    if let Some(input) = input {
        trace!(
            target: TRACE_TARGET,
            block,
            input = %format_args!("\"{}\"", InstructionSequence(&input)),
            output = %format_args!("\"{}\"", InstructionSequence(&instructions[start..])),
            "rewrite"
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

impl fmt::Display for InstructionSequence<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (index, inst) in self.0.iter().enumerate() {
            if index != 0 {
                f.write_str(" ")?;
            }
            if inst.is_deferred_push() {
                f.write_str("push_deferred")?;
            } else if inst.is_immutable_push() {
                f.write_str("push_immutable")?;
            } else if let Some(value) = push_value(inst) {
                write!(f, "push {value:#x}")?;
            } else if inst.is_encoded_push() {
                f.write_str("push_ref")?;
            } else if let Some(mnemonic) = op::mnemonic(inst.opcode) {
                f.write_str(mnemonic)?;
            } else {
                write!(f, "0x{:02x}", inst.opcode)?;
            }
        }
        Ok(())
    }
}

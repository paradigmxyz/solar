//! Duplicate terminal block elimination.
//!
//! Terminal blocks with alpha-equivalent instruction bodies can share one
//! implementation because execution never returns to their callers. This pass
//! keeps the first profitable body and replaces later copies with jumps to it,
//! accounting for the byte cost of the replacement before changing the IR.

use super::utils::is_evm_terminal;
use crate::backend::evm::ir::{
    Block, BlockId, Instruction, InstructionKind, Module, Operand, Terminator, TerminatorKind,
    ValueId,
};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;

pub(super) fn run(module: &mut Module) -> bool {
    let mut canonical = Vec::<(TerminalBlockKey, BlockId)>::new();
    let mut changed = false;

    let block_ids: Vec<_> = module.blocks.indices().collect();
    for block_id in block_ids {
        let block = &module.blocks[block_id];
        if !terminal_block_dedup_is_profitable(block) {
            continue;
        }
        let Some(key) = terminal_block_key(block) else { continue };
        if let Some((_, target)) = canonical.iter().find(|(known, _)| *known == key) {
            module.blocks[block_id].instructions.clear();
            module.blocks[block_id].terminator =
                Some(Terminator::new(TerminatorKind::Jump(*target)));
            changed = true;
        } else {
            canonical.push((key, block_id));
        }
    }

    changed
}

fn terminal_block_dedup_is_profitable(block: &Block) -> bool {
    let Some(term) = &block.terminator else { return false };
    if !is_evm_terminal(&term.kind) {
        return false;
    }
    // A replacement block still needs `JUMPDEST PUSH2(label) JUMP`. Avoid
    // rewriting tiny revert blocks where size is equal and revert-path gas
    // would get worse.
    let current_size = 1
        + block.instructions.iter().map(estimated_instruction_size).sum::<usize>()
        + estimated_terminator_size(&term.kind);
    let replacement_size = 1 + 3 + 1;
    current_size > replacement_size
}

fn estimated_instruction_size(inst: &Instruction) -> usize {
    match &inst.kind {
        InstructionKind::Stack(_) => 1,
        InstructionKind::Operation(mnemonic) if mnemonic == "push" => {
            match inst.operands.as_slice() {
                [operand] => estimated_push_size(operand),
                _ => 1,
            }
        }
        InstructionKind::Operation(mnemonic) if mnemonic == "push_immutable" => 33,
        InstructionKind::Operation(_) => 1,
    }
}

fn estimated_terminator_size(kind: &TerminatorKind) -> usize {
    let operand_pushes = |operands: &[&Operand]| {
        operands.iter().map(|operand| estimated_push_size(operand)).sum::<usize>() + 1
    };
    match kind {
        TerminatorKind::Return { offset, size } | TerminatorKind::Revert { offset, size } => {
            operand_pushes(&[offset, size])
        }
        TerminatorKind::SelfDestruct { recipient } => operand_pushes(&[recipient]),
        TerminatorKind::Stop | TerminatorKind::Invalid | TerminatorKind::RawOpcode(_) => 1,
        TerminatorKind::Jump(_) | TerminatorKind::Branch { .. } | TerminatorKind::Switch { .. } => {
            0
        }
    }
}

fn estimated_push_size(operand: &Operand) -> usize {
    match operand {
        Operand::Immediate(value) if *value == U256::ZERO => 1,
        Operand::Immediate(value) => value.byte_len() + 1,
        Operand::Block(_) | Operand::Symbol(_) => 3,
        Operand::Value(_) => 0,
    }
}

fn terminal_block_key(block: &Block) -> Option<TerminalBlockKey> {
    let mut locals = FxHashMap::default();
    let mut instructions = Vec::with_capacity(block.instructions.len());

    for inst in &block.instructions {
        let operands =
            inst.operands.iter().map(|operand| terminal_operand_key(operand, &locals)).collect();
        let result = inst.result.map(|value| {
            let index = locals.len();
            locals.insert(value, index);
            index
        });
        instructions.push(TerminalInstructionKey { result, kind: inst.kind.clone(), operands });
    }

    let term = block.terminator.as_ref()?;
    Some(TerminalBlockKey {
        instructions,
        terminator: terminal_terminator_key(&term.kind, &locals),
    })
}

fn terminal_operand_key(
    operand: &Operand,
    locals: &FxHashMap<ValueId, usize>,
) -> TerminalOperandKey {
    match operand {
        Operand::Value(value) => locals
            .get(value)
            .copied()
            .map(TerminalOperandKey::LocalValue)
            .unwrap_or(TerminalOperandKey::ExternalValue(*value)),
        Operand::Immediate(value) => TerminalOperandKey::Immediate(*value),
        Operand::Block(block) => TerminalOperandKey::Block(*block),
        Operand::Symbol(symbol) => TerminalOperandKey::Symbol(symbol.clone()),
    }
}

fn terminal_terminator_key(
    kind: &TerminatorKind,
    locals: &FxHashMap<ValueId, usize>,
) -> TerminalTerminatorKey {
    match kind {
        TerminatorKind::Jump(target) => TerminalTerminatorKey::Jump(*target),
        TerminatorKind::Branch { condition, then_block, else_block } => {
            TerminalTerminatorKey::Branch {
                condition: terminal_operand_key(condition, locals),
                then_block: *then_block,
                else_block: *else_block,
            }
        }
        TerminatorKind::Switch { value, default, cases } => TerminalTerminatorKey::Switch {
            value: terminal_operand_key(value, locals),
            default: *default,
            cases: cases
                .iter()
                .map(|(case, target)| (terminal_operand_key(case, locals), *target))
                .collect(),
        },
        TerminatorKind::Return { offset, size } => TerminalTerminatorKey::Return {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        TerminatorKind::Revert { offset, size } => TerminalTerminatorKey::Revert {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        TerminatorKind::Stop => TerminalTerminatorKey::Stop,
        TerminatorKind::Invalid => TerminalTerminatorKey::Invalid,
        TerminatorKind::SelfDestruct { recipient } => TerminalTerminatorKey::SelfDestruct {
            recipient: terminal_operand_key(recipient, locals),
        },
        TerminatorKind::RawOpcode(opcode) => TerminalTerminatorKey::RawOpcode(*opcode),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalBlockKey {
    instructions: Vec<TerminalInstructionKey>,
    terminator: TerminalTerminatorKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalInstructionKey {
    result: Option<usize>,
    kind: InstructionKind,
    operands: Vec<TerminalOperandKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalTerminatorKey {
    Jump(BlockId),
    Branch {
        condition: TerminalOperandKey,
        then_block: BlockId,
        else_block: BlockId,
    },
    Switch {
        value: TerminalOperandKey,
        default: BlockId,
        cases: Vec<(TerminalOperandKey, BlockId)>,
    },
    Return {
        offset: TerminalOperandKey,
        size: TerminalOperandKey,
    },
    Revert {
        offset: TerminalOperandKey,
        size: TerminalOperandKey,
    },
    Stop,
    Invalid,
    SelfDestruct {
        recipient: TerminalOperandKey,
    },
    RawOpcode(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalOperandKey {
    LocalValue(usize),
    ExternalValue(ValueId),
    Immediate(U256),
    Block(BlockId),
    Symbol(String),
}

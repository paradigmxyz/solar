//! Duplicate terminal block elimination.
//!
//! Terminal blocks with alpha-equivalent instruction bodies can share one
//! implementation because execution never returns to their callers. This pass
//! keeps the first body and redirects later copies to it. CFG simplification
//! then redirects references and removes the temporary jump thunks.

use super::utils::is_evm_terminal;
use crate::backend::evm::ir::{
    Block, BlockId, Module, Operand, Terminator, TerminatorKind, ValueId,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::map::{FxHashMap, StdEntry};

#[derive(Default)]
pub(super) struct RunState {
    canonical: FxHashMap<TerminalBlockKey, BlockId>,
    locals: FxHashMap<ValueId, usize>,
    redirects: Vec<(BlockId, BlockId)>,
}

pub(super) fn run(
    module: &mut Module,
    _options: super::PassOptions,
    pass_state: &mut super::PassState,
) -> bool {
    let state = &mut pass_state.terminal_dedup;
    state.canonical.clear();
    state.redirects.clear();

    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        let Some(key) = terminal_block_key(block, &mut state.locals) else { continue };
        match state.canonical.entry(key) {
            StdEntry::Occupied(entry) => state.redirects.push((block_id, *entry.get())),
            StdEntry::Vacant(entry) => {
                entry.insert(block_id);
            }
        }
    }

    let changed = !state.redirects.is_empty();
    for (block, target) in state.redirects.drain(..) {
        module.blocks[block].instructions.clear();
        module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(target)));
    }
    changed
}

fn terminal_block_key(
    block: &Block,
    locals: &mut FxHashMap<ValueId, usize>,
) -> Option<TerminalBlockKey> {
    if !block.terminator.as_ref().is_some_and(|term| is_evm_terminal(&term.kind)) {
        return None;
    }
    locals.clear();
    let mut instructions = Vec::with_capacity(block.instructions.len());

    for inst in &block.instructions {
        let operands =
            inst.operands.iter().map(|operand| terminal_operand_key(operand, locals)).collect();
        let result = inst.result.map(|value| {
            let index = locals.len();
            locals.insert(value, index);
            index
        });
        instructions.push(TerminalInstructionKey {
            result,
            opcode: inst.opcode,
            encoding: inst.encoding,
            operands,
        });
    }

    let term = block.terminator.as_ref()?;
    Some(TerminalBlockKey { instructions, terminator: terminal_terminator_key(&term.kind, locals) })
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TerminalBlockKey {
    instructions: Vec<TerminalInstructionKey>,
    terminator: TerminalTerminatorKey,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TerminalInstructionKey {
    result: Option<usize>,
    opcode: u8,
    encoding: u8,
    operands: SmallVec<[TerminalOperandKey; 1]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
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
        cases: SmallVec<[(TerminalOperandKey, BlockId); 2]>,
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

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum TerminalOperandKey {
    LocalValue(usize),
    ExternalValue(ValueId),
    Immediate(U256),
    Block(BlockId),
}

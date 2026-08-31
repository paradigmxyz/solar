//! Duplicate terminal block elimination.
//!
//! Terminal blocks with identical machine instruction bodies can share one
//! implementation because execution never returns to their callers. This pass
//! keeps the first body and redirects later copies to it. CFG simplification
//! then redirects references and removes the temporary jump thunks.

use super::{
    EvmPass,
    utils::{StackDepths, is_terminal_boundary, relative_stack_high_water},
};
use crate::backend::evm::ir::{
    Block, BlockId, Hotness, Module, PushValue, Terminator, TerminatorKind,
};
use solar_data_structures::map::{FxHashMap, StdEntry};
use solar_sema::Gcx;

pub(super) struct TerminalDedup;

impl EvmPass for TerminalDedup {
    fn name(&self) -> &'static str {
        "terminal-dedup"
    }

    fn run_pass(&self, gcx: Gcx<'_>, module: &mut Module) -> bool {
        deduplicate_terminals(gcx, module)
    }
}

#[derive(Default)]
struct RunState {
    canonical: FxHashMap<TerminalBlockKey, BlockId>,
    redirects: Vec<(BlockId, BlockId)>,
}

fn deduplicate_terminals(_gcx: Gcx<'_>, module: &mut Module) -> bool {
    let depths = StackDepths::new(module);
    let mut state = RunState::default();
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        let Some(key) = terminal_block_key(block) else { continue };
        match state.canonical.entry(key) {
            StdEntry::Occupied(entry) => {
                let fits = body_provides_jump_headroom(block)
                    || depths.as_ref().is_some_and(|depths| depths.has_headroom(block_id, 0, 1));
                if fits {
                    state.redirects.push((block_id, *entry.get()));
                }
            }
            StdEntry::Vacant(entry) => {
                entry.insert(block_id);
            }
        }
    }

    let changed = !state.redirects.is_empty();
    for (block, target) in state.redirects.drain(..) {
        if !module.blocks[block].metadata.hotness.is_cold() {
            module.blocks[target].metadata.hotness = Hotness::Hot;
        }
        module.blocks[block].instructions.clear();
        module.blocks[block].terminator = Some(Terminator::new(TerminatorKind::Jump(target)));
    }
    changed
}

fn body_provides_jump_headroom(block: &Block) -> bool {
    relative_stack_high_water(&block.instructions).is_some_and(|peak| peak >= 1)
}

fn terminal_block_key(block: &Block) -> Option<TerminalBlockKey> {
    let terminator = &block.terminator.as_ref()?.kind;
    if !is_terminal_boundary(terminator) {
        return None;
    }
    let instructions = block
        .instructions
        .iter()
        .map(|inst| TerminalInstructionKey {
            opcode: inst.opcode,
            encoding: inst.encoding,
            value: inst.value,
            stack_op: inst.as_stack_op(),
        })
        .collect();
    Some(TerminalBlockKey { instructions, terminator: terminator.clone() })
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TerminalBlockKey {
    instructions: Vec<TerminalInstructionKey>,
    terminator: TerminatorKind,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct TerminalInstructionKey {
    opcode: u8,
    encoding: u8,
    value: Option<PushValue>,
    stack_op: Option<crate::backend::evm::op::StackOp>,
}

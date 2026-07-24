//! Duplicate terminal block elimination.
//!
//! Terminal blocks with identical machine instruction bodies can share one
//! implementation because execution never returns to their callers. This pass
//! keeps the first body and redirects later copies to it. CFG simplification
//! then redirects references and removes the temporary jump thunks.

use super::{EvmPass, utils::is_evm_terminal};
use crate::backend::evm::ir::{Block, BlockId, Module, PushValue, Terminator, TerminatorKind};
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
    let mut state = RunState::default();

    for block_id in module.block_ids() {
        let block = module.block(block_id);
        let Some(key) = terminal_block_key(block) else { continue };
        match state.canonical.entry(key) {
            StdEntry::Occupied(entry) => state.redirects.push((block_id, *entry.get())),
            StdEntry::Vacant(entry) => {
                entry.insert(block_id);
            }
        }
    }

    let changed = !state.redirects.is_empty();
    for (block, target) in state.redirects.drain(..) {
        module.block_mut(block).instructions.clear();
        module.block_mut(block).terminator = Some(Terminator::new(TerminatorKind::Jump(target)));
    }
    changed
}

fn terminal_block_key(block: &Block) -> Option<TerminalBlockKey> {
    let terminator = &block.terminator.as_ref()?.kind;
    if !is_evm_terminal(terminator) {
        return None;
    }
    let instructions = block
        .instructions
        .iter()
        .map(|inst| TerminalInstructionKey {
            opcode: inst.opcode,
            encoding: inst.encoding,
            value: inst.value,
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
}

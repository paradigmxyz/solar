//! Duplicate terminal block elimination.
//!
//! Terminal blocks with identical machine instruction bodies can share one
//! implementation because execution never returns to their callers. This pass
//! keeps the first body and redirects later copies to it. CFG simplification
//! then redirects references and removes the temporary jump thunks. Block hotness does not affect
//! equivalence; a hot redirect promotes the shared body so later layout keeps it on the hot path.
//! The shared body inherits loop membership from every copy. Debug origins merge across the
//! whole group, preserving known function events only when no copy conflicts. Debug metadata
//! never participates in the body key or changes the redirects.
//!
//! The body key includes each instruction's `keep_with_next` flag, so the surviving copy cannot
//! drop a constraint one of the redirected copies carried. A shared body is entered at its own
//! block boundary, which is legal by construction, so the pass needs no other split check.

use super::{EvmPass, utils::is_terminal_boundary};
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
    let mut state = RunState::default();
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        let Some(key) = terminal_block_key(block) else { continue };
        match state.canonical.entry(key) {
            StdEntry::Occupied(entry) => state.redirects.push((block_id, *entry.get())),
            StdEntry::Vacant(entry) => {
                entry.insert(block_id);
            }
        }
    }

    let changed = !state.redirects.is_empty();
    state.redirects.sort_unstable_by_key(|&(block, target)| (target, block));
    for group in state.redirects.chunk_by(|a, b| a.1 == b.1) {
        merge_debug_origins(module, group);
        for &(block, target) in group {
            if !module.blocks[block].metadata.hotness.is_cold() {
                module.blocks[target].metadata.hotness = Hotness::Hot;
            }
            module.blocks[target].metadata.in_loop |= module.blocks[block].metadata.in_loop;
            // duplicate terminal body -> jump canonical body
            module.blocks[block].instructions.clear();
            let mut terminator = Terminator::new(TerminatorKind::Jump(target));
            terminator.metadata.mark_debug_info_dropped();
            module.blocks[block].terminator = Some(terminator);
        }
    }
    changed
}

fn merge_debug_origins(module: &mut Module, redirects: &[(BlockId, BlockId)]) {
    let target = redirects[0].1;
    for index in 0..module.blocks[target].instructions.len() {
        let mut metadata = module.blocks[target].instructions[index].metadata.clone();
        metadata.merge_equivalent_debug_info(
            redirects.iter().map(|&(block, _)| &module.blocks[block].instructions[index].metadata),
        );
        module.blocks[target].instructions[index].metadata = metadata;
    }
    let mut metadata = module.blocks[target].terminator.as_ref().unwrap().metadata.clone();
    metadata.merge_equivalent_debug_info(
        redirects
            .iter()
            .map(|&(block, _)| &module.blocks[block].terminator.as_ref().unwrap().metadata),
    );
    module.blocks[target].terminator.as_mut().unwrap().metadata = metadata;
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
            keep_with_next: inst.keeps_with_next(),
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
    keep_with_next: bool,
}

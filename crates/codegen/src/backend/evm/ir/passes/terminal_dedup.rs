//! Duplicate terminal block elimination.
//!
//! Terminal blocks with identical machine instruction bodies can share one
//! implementation because execution never returns to their callers. This pass
//! keeps the first body and redirects later copies to it. CFG simplification
//! then redirects references and removes the temporary jump thunks. Block hotness does not affect
//! equivalence; a hot redirect promotes the shared body so later layout keeps it on the hot path.
//!
//! The body key includes each instruction's `keep_with_next` flag, so the surviving copy cannot
//! drop a constraint one of the redirected copies carried. A shared body is entered at its own
//! block boundary, which is legal by construction, so the pass needs no other split check.
//!
//! Nonterminal blocks can also share identical bodies when all incoming edges already require a
//! jump. A physical fallthrough excludes the block, as does an ordinary address-taken label whose
//! numeric identity must remain observable. Compiler-generated return continuations explicitly
//! permit their addresses to be redirected. These restrictions let CFG simplification bypass the
//! duplicate's temporary thunk without adding a transfer to any incoming path. Blocks entered by
//! fallthrough remain with tail merging's separate profitability policy.
//!
//! Shared code keeps source origins but drops function events that disagree between paths. Debug
//! metadata does not participate in candidate selection or prevent executable-code sharing.

use super::{EvmPass, cfg_simplify::is_direct_jump_label, utils::is_terminal_boundary};
use crate::backend::evm::ir::{
    Block, BlockId, Hotness, Module, PushValue, Terminator, TerminatorKind,
};
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, StdEntry},
};
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
    let mut unshareable = DenseBitSet::new_empty(module.blocks.len());
    unshareable.insert(BlockId::ENTRY);
    for (id, block) in module.blocks.iter_enumerated() {
        if let Some(next) = module.next_block(id)
            && let Some(term) = &block.terminator
            && matches!(term.kind, TerminatorKind::Jump(to) if to == next)
        {
            unshareable.insert(next);
        }
        if let Some(next) = module.next_block(id)
            && let Some(term) = &block.terminator
            && matches!(term.kind, TerminatorKind::JumpI { then_block, else_block }
                if then_block == next || else_block == next)
        {
            unshareable.insert(next);
        }
        for (at, inst) in block.instructions.iter().enumerate() {
            if !is_direct_jump_label(block, at)
                && let Some(to) = inst.pushed_block()
                && !module.blocks[to].metadata.is_continuation
            {
                unshareable.insert(to);
            }
        }
    }
    for block_id in module.blocks.indices() {
        let block = &module.blocks[block_id];
        let redirectable = !unshareable.contains(block_id);
        let Some(key) = terminal_block_key(block, redirectable) else { continue };
        match state.canonical.entry(key) {
            StdEntry::Occupied(entry) => state.redirects.push((block_id, *entry.get())),
            StdEntry::Vacant(entry) => {
                entry.insert(block_id);
            }
        }
    }

    let changed = !state.redirects.is_empty();
    for (block, target) in state.redirects.drain(..) {
        merge_debug_origins(module, block, target);
        module.blocks[target].metadata.in_loop |= module.blocks[block].metadata.in_loop;
        if !module.blocks[block].metadata.hotness.is_cold() {
            module.blocks[target].metadata.hotness = Hotness::Hot;
        }
        // duplicate: body; terminator -> duplicate: jump canonical
        module.blocks[block].instructions.clear();
        let mut terminator = Terminator::new(TerminatorKind::Jump(target));
        terminator.metadata.mark_debug_info_dropped();
        module.blocks[block].terminator = Some(terminator);
    }
    changed
}

fn merge_debug_origins(module: &mut Module, block: BlockId, target: BlockId) {
    let function_invoke = module.blocks[block].metadata.function_invoke;
    let instruction_metadata = module.blocks[block]
        .instructions
        .iter()
        .map(|inst| inst.metadata.clone())
        .collect::<Vec<_>>();
    let terminator_metadata =
        module.blocks[block].terminator.as_ref().map(|terminator| terminator.metadata.clone());
    let target = &mut module.blocks[target];
    // NOTE: A shared block's entry cannot name a function activated on only one incoming path.
    if target.metadata.function_invoke != function_invoke {
        target.metadata.function_invoke = None;
    }
    debug_assert_eq!(target.instructions.len(), instruction_metadata.len());
    for (instruction, metadata) in target.instructions.iter_mut().zip(&instruction_metadata) {
        instruction.metadata.merge_shared_debug_info(metadata);
    }
    if let Some(metadata) = &terminator_metadata
        && let Some(terminator) = &mut target.terminator
    {
        terminator.metadata.merge_shared_debug_info(metadata);
    }
}

fn terminal_block_key(block: &Block, redirectable: bool) -> Option<TerminalBlockKey> {
    let terminator = &block.terminator.as_ref()?.kind;
    if !is_terminal_boundary(terminator) && !redirectable {
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

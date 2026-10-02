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

use super::{
    EvmPass,
    cfg_simplify::is_direct_jump_label,
    utils::{MachineInstKey, is_terminal_boundary},
};
use crate::backend::evm::ir::{Block, BlockId, Hotness, Module, Terminator, TerminatorKind};
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHasher},
};
use solar_sema::Gcx;
use std::hash::{Hash, Hasher};

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
    /// First body of each shape, bucketed by the hash of its instruction keys and terminator.
    canonical: FxHashMap<u64, SmallVec<[BlockId; 1]>>,
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
        let Some(hash) = terminal_block_hash(block, redirectable) else { continue };
        let bodies = state.canonical.entry(hash).or_default();
        if let Some(&canonical) =
            bodies.iter().find(|&&other| same_terminal_body(&module.blocks[other], block))
        {
            state.redirects.push((block_id, canonical));
        } else {
            bodies.push(block_id);
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

/// Hashes a candidate body's machine instructions and terminator; `None` when the block cannot
/// share its body.
fn terminal_block_hash(block: &Block, redirectable: bool) -> Option<u64> {
    let terminator = &block.terminator.as_ref()?.kind;
    if !is_terminal_boundary(terminator) && !redirectable {
        return None;
    }
    let mut hasher = FxHasher::default();
    block.instructions.len().hash(&mut hasher);
    for inst in &block.instructions {
        MachineInstKey::new(inst).hash(&mut hasher);
    }
    terminator.hash(&mut hasher);
    Some(hasher.finish())
}

/// Whether two candidate bodies have the same machine instructions and terminator.
fn same_terminal_body(a: &Block, b: &Block) -> bool {
    a.instructions.len() == b.instructions.len()
        && a.instructions
            .iter()
            .zip(&b.instructions)
            .all(|(a, b)| MachineInstKey::new(a) == MachineInstKey::new(b))
        && a.terminator.as_ref().map(|term| &term.kind)
            == b.terminator.as_ref().map(|term| &term.kind)
}

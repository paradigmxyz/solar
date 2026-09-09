//! Recognize leaf functions that restore their temporary memory writes.
//!
//! All writes must target one exact SSA address. A dominating, non-loop load
//! saves its initial word, and a two-state forward analysis requires every
//! returning path to restore that word. Joins retain both clean and dirty states;
//! a restore on one arm cannot cover a dirty return on another. Saving the word
//! inside a loop is excluded because a later iteration could save modified data.
//!
//! Only pure instructions, word loads, and the matched word stores are accepted.
//! Calls, partial writes, allocation, environment observations, and other exits
//! are excluded. This keeps temporary writes unobservable outside the function.
//! Reads and FMP/MSIZE/capture flags remain conservative in the caller summary:
//! restoring bytes does not undo memory expansion or erase pointer observations.
//! No instructions are removed here; callers may reuse memory facts across the
//! call. The analysis is capped at 32 blocks and 128 live instructions.

use super::CfgInfo;
use crate::mir::{BlockId, EffectKind, Function, InstKind, Terminator, Value};
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec};
use std::collections::VecDeque;

pub(super) fn restores_memory(func: &Function) -> bool {
    if func.blocks.is_empty()
        || func.blocks.len() > 32
        || func.returns.len() > 1
        || func.instructions().take(129).count() > 128
    {
        return false;
    }
    let mut stores = Vec::new();
    for (block, body) in func.blocks.iter_enumerated() {
        if !matches!(
            body.terminator,
            Some(
                Terminator::Jump(_)
                    | Terminator::Branch { .. }
                    | Terminator::Return { .. }
                    | Terminator::Stop
            )
        ) {
            return false;
        }
        for (position, &inst) in body.instructions.iter().enumerate() {
            let instruction = func.inst(inst);
            if instruction
                .metadata
                .effect()
                .is_some_and(|effect| effect != instruction.kind.effect_kind())
            {
                return false;
            }
            match &instruction.kind {
                InstKind::MStore(address, value) => {
                    stores.push((block, position, *address, *value))
                }
                InstKind::MLoad(_) => {}
                kind if kind.effect_kind() == EffectKind::Pure => {}
                _ => return false,
            }
        }
    }
    let Some(&(_, _, address, _)) = stores.first() else { return false };
    if stores.iter().any(|&(_, _, other, _)| other != address) {
        return false;
    }
    let Some((saved, load)) = stores.iter().find_map(|&(_, _, _, value)| {
        if let Value::Inst(inst) = func.value(value)
            && matches!(func.inst(*inst).kind, InstKind::MLoad(other) if other == address)
        {
            Some((value, *inst))
        } else {
            None
        }
    }) else {
        return false;
    };
    let Some((save_block, save_position)) =
        func.blocks.iter_enumerated().find_map(|(id, block)| {
            block.instructions.iter().position(|&inst| inst == load).map(|position| (id, position))
        })
    else {
        return false;
    };
    let cfg = CfgInfo::new(func);
    if cfg.cyclic_blocks().contains(save_block)
        || stores.iter().any(|&(block, position, _, _)| {
            !cfg.dominators().dominates(save_block, block)
                || (block == save_block && position <= save_position)
        })
    {
        return false;
    }

    const CLEAN: u8 = 1;
    const DIRTY: u8 = 2;
    let mut incoming = index_vec![0u8; func.blocks.len()];
    incoming[BlockId::ENTRY] = CLEAN;
    let mut worklist = VecDeque::from([BlockId::ENTRY]);
    let mut queued = DenseBitSet::new_empty(func.blocks.len());
    queued.insert(BlockId::ENTRY);
    while let Some(block) = worklist.pop_front() {
        queued.remove(block);
        let mut state = incoming[block];
        for &inst in &func.blocks[block].instructions {
            if let InstKind::MStore(_, value) = func.inst(inst).kind {
                state = if value == saved { CLEAN } else { DIRTY };
            }
        }
        if matches!(
            func.blocks[block].terminator,
            Some(Terminator::Return { .. } | Terminator::Stop)
        ) && state & DIRTY != 0
        {
            return false;
        }
        for &successor in cfg.successors(block) {
            let merged = incoming[successor] | state;
            if merged != incoming[successor] {
                incoming[successor] = merged;
                if queued.insert(successor) {
                    worklist.push_back(successor);
                }
            }
        }
    }
    true
}

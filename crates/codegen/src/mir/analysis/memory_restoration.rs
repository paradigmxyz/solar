//! Recognize leaf functions that restore their temporary memory writes.
//!
//! All writes must target at most eight saved words with pairwise disjoint
//! word ranges. A dominating, non-loop load saves each initial word, and a forward
//! dirty-bit analysis requires every returning path to restore every word.
//! Joins retain the union of possibly dirty words;
//! a restore on one arm cannot cover a dirty return on another. Saving the word
//! inside a loop is excluded because a later iteration could save modified data.
//!
//! Byte stores may dirty a saved word when canonical address analysis proves
//! the byte lies wholly within that word. Only a full store of the original
//! saved value marks the word clean again. Pure instructions and word loads are
//! also accepted. Calls, allocation, environment observations, and other exits
//! are excluded. This keeps temporary writes unobservable outside the function.
//! Reads and FMP/MSIZE/capture flags remain conservative in the caller summary:
//! restoring bytes does not undo memory expansion or erase pointer observations.
//! No instructions are removed here; callers may reuse memory facts across the
//! call. The analysis is capped at 32 blocks and 128 live instructions.

use super::{AliasAnalysis, CfgInfo, LocationSize};
use crate::mir::{BlockId, EffectKind, Function, InstKind, Terminator, Value};
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec, map::FxHashMap};
use std::collections::VecDeque;

pub(super) fn restores_memory(func: &Function, aa: &AliasAnalysis) -> bool {
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
                    stores.push((block, position, *address, *value, 32))
                }
                InstKind::MStore8(address, value) => {
                    stores.push((block, position, *address, *value, 1))
                }
                InstKind::MLoad(_) => {}
                kind if kind.effect_kind() == EffectKind::Pure => {}
                _ => return false,
            }
        }
    }
    if stores.is_empty() {
        return false;
    }
    let mut words = Vec::new();
    let mut locations = Vec::new();
    let cfg = CfgInfo::new(func);
    for &(_, _, address, _, width) in &stores {
        if width != 32 {
            continue;
        }
        if words.iter().any(|&(other, _, _, _)| other == address) {
            continue;
        }
        if words.len() == 8 {
            return false;
        }
        let Some(location) = aa.bare_memory_location(func, address, LocationSize::Const(32)) else {
            return false;
        };
        if locations.iter().any(|&other| aa.memory_alias(location, other).may_alias()) {
            return false;
        }
        let Some((saved, load)) = stores.iter().find_map(|&(_, _, store_address, value, width)| {
            if width == 32
                && store_address == address
                && let Value::Inst(inst) = func.value(value)
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
                block
                    .instructions
                    .iter()
                    .position(|&inst| inst == load)
                    .map(|position| (id, position))
            })
        else {
            return false;
        };
        if cfg.cyclic_blocks().contains(save_block) {
            return false;
        }
        words.push((address, saved, save_block, save_position));
        locations.push(location);
    }

    // Every write belongs to one saved word, and its original load must precede
    // every possible mutation. Unknown offsets and bytes outside saved ranges bail.
    let mut store_slots = FxHashMap::default();
    for &(block, position, address, _, width) in &stores {
        let Some(location) = aa.bare_memory_location(func, address, LocationSize::Const(width))
        else {
            return false;
        };
        let Some(slot) = locations.iter().position(|saved| {
            saved.address.base == location.address.base
                && location
                    .address
                    .offset
                    .checked_sub(saved.address.offset)
                    .is_some_and(|offset| offset <= 32 - width)
        }) else {
            return false;
        };
        let (_, _, save_block, save_position) = words[slot];
        if !cfg.dominators().dominates(save_block, block)
            || (block == save_block && position <= save_position)
        {
            return false;
        }
        store_slots.insert(func.blocks[block].instructions[position], slot);
    }

    // Bit zero distinguishes a reachable clean state from an unvisited block.
    // Bits one through eight record words dirty on at least one incoming path.
    const REACHABLE: u16 = 1;
    let mut incoming = index_vec![0u16; func.blocks.len()];
    incoming[BlockId::ENTRY] = REACHABLE;
    let mut worklist = VecDeque::from([BlockId::ENTRY]);
    let mut queued = DenseBitSet::new_empty(func.blocks.len());
    queued.insert(BlockId::ENTRY);
    while let Some(block) = worklist.pop_front() {
        queued.remove(block);
        let mut state = incoming[block];
        for &inst in &func.blocks[block].instructions {
            if let Some(&slot) = store_slots.get(&inst) {
                let bit = 1 << (slot + 1);
                if matches!(func.inst(inst).kind, InstKind::MStore(address, value)
                    if address == words[slot].0 && value == words[slot].1)
                {
                    state &= !bit;
                } else {
                    state |= bit;
                }
            }
        }
        if matches!(
            func.blocks[block].terminator,
            Some(Terminator::Return { .. } | Terminator::Stop)
        ) && state & !REACHABLE != 0
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

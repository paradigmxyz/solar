//! Recognize leaf functions that restore their temporary memory writes.
//!
//! All writes must target at most four exact SSA addresses with pairwise disjoint
//! word ranges. A dominating, non-loop load saves each initial word, and a forward
//! dirty-bit analysis requires every returning path to restore every word.
//! Joins retain the union of possibly dirty words;
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

use super::{AliasAnalysis, CfgInfo, LocationSize};
use crate::mir::{BlockId, EffectKind, Function, InstKind, Terminator, Value};
use solar_data_structures::{bit_set::DenseBitSet, index::index_vec};
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
                    stores.push((block, position, *address, *value))
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
    for &(_, _, address, _) in &stores {
        if words.iter().any(|&(other, _)| other == address) {
            continue;
        }
        if words.len() == 4 {
            return false;
        }
        let Some(location) = aa.bare_memory_location(func, address, LocationSize::Const(32)) else {
            return false;
        };
        if locations.iter().any(|&other| aa.memory_alias(location, other).may_alias()) {
            return false;
        }
        let Some((saved, load)) = stores.iter().find_map(|&(_, _, store_address, value)| {
            if store_address == address
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
        if cfg.cyclic_blocks().contains(save_block)
            || stores.iter().any(|&(block, position, other, _)| {
                other == address
                    && (!cfg.dominators().dominates(save_block, block)
                        || (block == save_block && position <= save_position))
            })
        {
            return false;
        }
        words.push((address, saved));
        locations.push(location);
    }

    // Bit zero distinguishes a reachable clean state from an unvisited block.
    // Bits one through four record words dirty on at least one incoming path.
    const REACHABLE: u8 = 1;
    let mut incoming = index_vec![0u8; func.blocks.len()];
    incoming[BlockId::ENTRY] = REACHABLE;
    let mut worklist = VecDeque::from([BlockId::ENTRY]);
    let mut queued = DenseBitSet::new_empty(func.blocks.len());
    queued.insert(BlockId::ENTRY);
    while let Some(block) = worklist.pop_front() {
        queued.remove(block);
        let mut state = incoming[block];
        for &inst in &func.blocks[block].instructions {
            if let InstKind::MStore(address, value) = func.inst(inst).kind {
                let slot = words.iter().position(|&(other, _)| other == address).unwrap();
                let bit = 1 << (slot + 1);
                if value == words[slot].1 {
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

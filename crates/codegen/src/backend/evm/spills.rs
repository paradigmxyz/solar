//! Stack-pressure planning and activation-local value homes.
//!
//! The planner bounds each block's live stack plus operand preparation against the target DUP
//! window. Functions above that bound receive homes for active stored values, avoiding value
//! rematerialization across arbitrary memory effects. A separate temporary region permits phi
//! edge copies to read every source before writing any destination, including cyclic transfers.
//! Ordinary low-pressure functions retain stack-only values. Address placement and dynamic-frame
//! lifetime remain in storage planning; this module emits no physical instructions.

use crate::{
    analysis::{Access, AddressSpace, CfgInfo, Liveness, Location, MemoryBase, ModRef},
    mir,
};
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::{cmp::Reverse, collections::BinaryHeap};

#[derive(Default)]
pub(crate) struct SpillPlan {
    pub(crate) homes: FxHashMap<mir::ValueId, usize>,
    pub(crate) phi_scratch: usize,
    pub(crate) words: usize,
}

impl SpillPlan {
    pub(crate) fn new(
        function: &mir::Function,
        live: &Liveness,
        cfg: &CfgInfo,
        returning: bool,
        version: EvmVersion,
        stored: impl Fn(mir::ValueId) -> bool,
    ) -> Self {
        if !exceeds_stack_window(function, live, cfg, returning, version, &stored) {
            return Self::default();
        }
        let mut local = DenseBitSet::new_empty(function.num_values());
        for (block_id, block) in function.blocks.iter_enumerated() {
            for (position, pair) in block.instructions.windows(2).enumerate() {
                let next = &function.inst(pair[1]).kind;
                if let Some(value) = function.inst_result_value(pair[0])
                    && !matches!(function.inst(pair[0]).kind, mir::InstKind::Phi(_))
                    && !next.has_side_effects()
                    && !matches!(next, mir::InstKind::Phi(_))
                    && next.operands().contains(&value)
                    && !live.is_used_at_or_after(value, block_id, position + 2)
                {
                    local.insert(value);
                }
            }
        }
        let (homes, phi_scratch) = assign_homes(function, live, cfg, |value| stored(value) && !local.contains(value));
        let scratch_count = function
            .blocks
            .iter()
            .map(|block| {
                block
                    .instructions
                    .iter()
                    .filter(|&&inst| matches!(function.inst(inst).kind, mir::InstKind::Phi(_)))
                    .count()
            })
            .max()
            .unwrap_or(0);
        Self { homes, phi_scratch, words: phi_scratch + scratch_count }
    }
}

/// Reuses words whose conservative live intervals do not overlap in physical block order.
fn assign_homes(function: &mir::Function, live: &Liveness, cfg: &CfgInfo, stored: impl Fn(mir::ValueId) -> bool) -> (FxHashMap<mir::ValueId, usize>, usize) {
    let mut intervals = FxHashMap::<mir::ValueId, (usize, usize)>::default();
    let mut touch = |value, position| {
        if stored(value) {
            let range = intervals.entry(value).or_insert((position, position));
            range.0 = range.0.min(position);
            range.1 = range.1.max(position);
        }
    };
    let mut start = 0;
    for (block_id, block) in function.blocks.iter_enumerated() {
        if !cfg.is_reachable(block_id) { continue; }
        for value in live.live_in(block_id).iter() { touch(value, start); }
        for (position, &inst) in block.instructions.iter().enumerate() {
            let point = start + 2 * position;
            if !matches!(function.inst(inst).kind, mir::InstKind::Phi(_)) {
                for value in function.inst(inst).kind.operands() { touch(value, point); }
            }
            if let Some(value) = function.inst_result_value(inst) {
                touch(value, if matches!(function.inst(inst).kind, mir::InstKind::Phi(_)) { start } else { point + 1 });
            }
        }
        start += 2 * block.instructions.len() + 2;
        for value in live.live_out(block_id).iter() { touch(value, start - 1); }
        if let Some(term) = &block.terminator { term.for_each_operand(|value| touch(value, start - 1)); }
    }
    let mut intervals = intervals.into_iter().collect::<Vec<_>>();
    intervals.sort_unstable_by_key(|&(value, (start, _))| (start, value));
    let mut homes = FxHashMap::default();
    let mut active = BinaryHeap::<Reverse<(usize, usize)>>::new();
    let mut free = BinaryHeap::<Reverse<usize>>::new();
    let mut words = 0;
    for (value, (start, end)) in intervals {
        while let Some(&Reverse((end, home))) = active.peek() {
            if end >= start { break; }
            active.pop();
            free.push(Reverse(home));
        }
        let home = free.pop().map(|Reverse(home)| home).unwrap_or_else(|| { let home = words; words += 1; home });
        homes.insert(value, home);
        active.push(Reverse((end, home)));
    }
    (homes, words)
}

/// Conservatively reserves homes when an activation cannot keep all live values within DUP reach.
fn exceeds_stack_window(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    returning: bool,
    version: EvmVersion,
    stored: impl Fn(mir::ValueId) -> bool,
) -> bool {
    let limit = version.reachable_stack_depth();
    for (block_id, block) in function.blocks.iter_enumerated() {
        if !cfg.is_reachable(block_id) {
            continue;
        }
        let mut active = live.live_in(block_id).iter().filter(|&v| stored(v)).collect::<Vec<_>>();
        for &inst in &block.instructions {
            if matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                && let Some(v) = function.inst_result_value(inst)
            {
                active.push(v);
            }
        }
        active.sort_unstable();
        active.dedup();
        if active.len() + usize::from(returning) > limit {
            return true;
        }
        for (position, &inst) in block.instructions.iter().enumerate() {
            if matches!(function.inst(inst).kind, mir::InstKind::Phi(_)) {
                continue;
            }
            let operands = function.inst(inst).kind.operands();
            if active.len() + operands.len() + usize::from(returning) > limit {
                return true;
            }
            active.retain(|&v| live.is_used_at_or_after(v, block_id, position + 1));
            if let Some(v) = function.inst_result_value(inst) {
                active.push(v);
            }
        }
    }
    false
}

/// Tests writes against compiler-owned words without treating source memory as scratch space.
pub(crate) fn may_overlap(effects: &ModRef, home: super::storage::FrameAddress) -> bool {
    accesses_overlap(effects.writes(), home)
}

pub(crate) fn accesses_overlap(accesses: &[Access], home: super::storage::FrameAddress) -> bool {
    accesses.iter().any(|access| match access {
        Access::Any(AddressSpace::Memory) => true,
        Access::Location(Location::Memory(location)) => {
            if location.size.as_const() == Some(0) {
                return false;
            }
            if let Some(size) = location.size.as_const() {
                let comparable = match (home, location.address.base) {
                    (super::storage::FrameAddress::Absolute(home), MemoryBase::Absolute) => {
                        Some(home)
                    }
                    (super::storage::FrameAddress::Relative(home), MemoryBase::InternalFrame) => {
                        Some(home)
                    }
                    _ => None,
                };
                if let Some(home) = comparable {
                    return location.address.offset.checked_add(size).is_none_or(|end| end > home)
                        && home.checked_add(32).is_none_or(|end| end > location.address.offset);
                }
            }
            !matches!(
                location.address.region,
                mir::MemoryRegion::Heap
                    | mir::MemoryRegion::AbiReturn
                    | mir::MemoryRegion::InternalFrame
            )
        }
        _ => false,
    })
}

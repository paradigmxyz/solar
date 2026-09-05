//! Stack-pressure planning and activation-local value homes.
//!
//! The planner simulates the physical scheduler over block entries, operand preparation, calls,
//! and phi edges to find functions that exceed the target DUP window. Dying operands are consumed
//! in place instead of being counted twice. Those functions receive reusable memory homes for
//! overlapping live intervals; short-lived expression temporaries, dying branch conditions and
//! single internal return values remain on the stack. Calls, memory writers and wider terminal
//! protocols stop the bounded local window. A separate
//! temporary region permits phi edge copies to read every source before writing any destination,
//! including cyclic transfers. Ordinary low-pressure functions retain stack-only values. Address
//! placement and dynamic-frame lifetime remain in storage planning; this module emits no physical
//! instructions.

use super::scheduler::Stack;
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
        // At most one result is defined per instruction. Bounding each temporary's lifetime bounds
        // their simultaneous stack occupancy; reserve room for the return label, three operands,
        // and the two additional words used by select lowering.
        let window = version.reachable_stack_depth().saturating_sub(6).min(8);
        for (block_id, block) in function.blocks.iter_enumerated() {
            for (position, &inst) in block.instructions.iter().enumerate() {
                if let Some(value) = function.inst_result_value(inst)
                    && !matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                {
                    for (next_position, next_inst) in block.instructions.iter().copied().map(Some)
                        .chain([None]).enumerate().skip(position + 1).take(window)
                    {
                        let Some(next_inst) = next_inst else {
                            let terminal_use = match &block.terminator {
                                Some(mir::Terminator::Branch { condition, .. }) => *condition == value,
                                Some(mir::Terminator::Return { values }) if returning => {
                                    values.as_slice() == [value]
                                }
                                _ => false,
                            };
                            if terminal_use && !live.live_out(block_id).contains(value) {
                                local.insert(value);
                            }
                            break;
                        };
                        let next = &function.inst(next_inst).kind;
                        if next.has_side_effects() || matches!(next, mir::InstKind::Phi(_)) {
                            break;
                        }
                        if next.operands().contains(&value)
                            && !live.is_used_at_or_after(value, block_id, next_position + 1)
                        {
                            local.insert(value);
                            break;
                        }
                    }
                }
            }
        }
        let (homes, phi_scratch) =
            assign_homes(function, live, cfg, |value| stored(value) && !local.contains(value));
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
fn assign_homes(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    stored: impl Fn(mir::ValueId) -> bool,
) -> (FxHashMap<mir::ValueId, usize>, usize) {
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
        if !cfg.is_reachable(block_id) {
            continue;
        }
        for value in live.live_in(block_id).iter() {
            touch(value, start);
        }
        for (position, &inst) in block.instructions.iter().enumerate() {
            let point = start + 2 * position;
            if !matches!(function.inst(inst).kind, mir::InstKind::Phi(_)) {
                for value in function.inst(inst).kind.operands() {
                    touch(value, point);
                }
            }
            if let Some(value) = function.inst_result_value(inst) {
                touch(
                    value,
                    if matches!(function.inst(inst).kind, mir::InstKind::Phi(_)) {
                        start
                    } else {
                        point + 1
                    },
                );
            }
        }
        start += 2 * block.instructions.len() + 2;
        for value in live.live_out(block_id).iter() {
            touch(value, start - 1);
        }
        if let Some(term) = &block.terminator {
            term.for_each_operand(|value| touch(value, start - 1));
        }
    }
    let mut intervals = intervals.into_iter().collect::<Vec<_>>();
    intervals.sort_unstable_by_key(|&(value, (start, _))| (start, value));
    let mut homes = FxHashMap::default();
    let mut active = BinaryHeap::<Reverse<(usize, usize)>>::new();
    let mut free = BinaryHeap::<Reverse<usize>>::new();
    let mut words = 0;
    for (value, (start, end)) in intervals {
        while let Some(&Reverse((end, home))) = active.peek() {
            if end >= start {
                break;
            }
            active.pop();
            free.push(Reverse(home));
        }
        let home = free.pop().map(|Reverse(home)| home).unwrap_or_else(|| {
            let home = words;
            words += 1;
            home
        });
        homes.insert(value, home);
        active.push(Reverse((end, home)));
    }
    (homes, words)
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PressureSlot {
    Value(mir::ValueId),
    ReturnAddress,
    Continuation,
}

/// Uses the physical scheduler itself to account for dying operands and simultaneous edge copies.
fn exceeds_stack_window(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    returning: bool,
    version: EvmVersion,
    stored: impl Fn(mir::ValueId) -> bool,
) -> bool {
    let prefix = usize::from(returning);
    let entries = function
        .blocks
        .iter_enumerated()
        .map(|(id, block)| {
            let mut values = live.live_in(id).iter().filter(|&v| stored(v)).collect::<Vec<_>>();
            for &inst in &block.instructions {
                if matches!(function.inst(inst).kind, mir::InstKind::Phi(_))
                    && let Some(v) = function.inst_result_value(inst)
                {
                    values.push(v);
                }
            }
            values.sort_unstable();
            values.dedup();
            let mut slots = if returning { vec![PressureSlot::ReturnAddress] } else { Vec::new() };
            slots.extend(values.into_iter().map(PressureSlot::Value));
            slots
        })
        .collect::<solar_data_structures::index::IndexVec<mir::BlockId, _>>();
    for (block_id, block) in function.blocks.iter_enumerated() {
        if !cfg.is_reachable(block_id) {
            continue;
        }
        if entries[block_id].len() > version.reachable_stack_depth() + prefix {
            return true;
        }
        let mut stack = Stack::new(entries[block_id].clone());
        for (position, &inst) in block.instructions.iter().enumerate() {
            let kind = &function.inst(inst).kind;
            if matches!(kind, mir::InstKind::Phi(_)) {
                continue;
            }
            let operands = kind.operands();
            let prepares = kind.evm_opcode().is_some()
                || matches!(
                    kind,
                    mir::InstKind::Select(..)
                        | mir::InstKind::DataCopy(..)
                        | mir::InstKind::InternalCall { .. }
                );
            if prepares
                && !prepare_pressure(&mut stack, &operands, prefix, version, &stored, |v| {
                    live.is_used_at_or_after(v, block_id, position + 1)
                })
            {
                return true;
            }
            if matches!(kind, mir::InstKind::InternalCall { .. }) {
                let caller = stack.values()[..stack.values().len() - operands.len()].to_vec();
                let mut desired = caller.clone();
                desired.push(PressureSlot::Continuation);
                desired.extend(operands.iter().rev().copied().map(PressureSlot::Value));
                stack.push(PressureSlot::Continuation);
                if stack.reconcile(&desired, prefix, version).is_err() {
                    return true;
                }
                stack = Stack::new(caller);
            } else if prepares {
                stack.truncate(stack.values().len() - operands.len());
            }
            if let Some(value) = function.inst_result_value(inst) {
                stack.push(PressureSlot::Value(value));
            }
        }
        if let Some(term) = &block.terminator {
            match term {
                mir::Terminator::Branch { condition, .. } => {
                    if !prepare_pressure(&mut stack, &[*condition], prefix, version, &stored, |v| {
                        live.live_out(block_id).contains(v)
                    }) {
                        return true;
                    }
                    stack.truncate(stack.values().len() - 1);
                }
                mir::Terminator::Switch { value, .. } => {
                    if !prepare_pressure(&mut stack, &[*value], prefix, version, &stored, |v| {
                        live.live_out(block_id).contains(v)
                    }) {
                        return true;
                    }
                    stack.truncate(stack.values().len() - 1);
                }
                mir::Terminator::Return { values } if returning => {
                    for &value in values.iter().skip(1) {
                        if !prepare_pressure(&mut stack, &[value], prefix, version, &stored, |v| {
                            values.contains(&v)
                        }) {
                            return true;
                        }
                        stack.truncate(stack.values().len() - 1);
                    }
                    let mut desired = values
                        .first()
                        .copied()
                        .map(PressureSlot::Value)
                        .into_iter()
                        .collect::<Vec<_>>();
                    desired.push(PressureSlot::ReturnAddress);
                    if !materialize_pressure(&mut stack, &desired, &stored)
                        || stack.reconcile(&desired, 0, version).is_err()
                    {
                        return true;
                    }
                }
                mir::Terminator::SelfDestruct { recipient } => {
                    if !prepare_pressure(
                        &mut stack,
                        &[*recipient],
                        prefix,
                        version,
                        &stored,
                        |_| false,
                    ) {
                        return true;
                    }
                }
                mir::Terminator::TailCall { args, .. } => {
                    if !prepare_pressure(&mut stack, args, prefix, version, &stored, |_| false) {
                        return true;
                    }
                }
                mir::Terminator::ReturnData { offset, size }
                | mir::Terminator::Revert { offset, size } => {
                    if !prepare_pressure(
                        &mut stack,
                        &[*offset, *size],
                        prefix,
                        version,
                        &stored,
                        |_| false,
                    ) {
                        return true;
                    }
                }
                _ => {}
            }
            for to in term.successors() {
                let mut desired = entries[to].clone();
                for slot in &mut desired {
                    if let PressureSlot::Value(value) = slot
                        && let mir::Value::Inst(inst) = function.value(*value)
                        && function.blocks[to].instructions.contains(inst)
                        && let mir::InstKind::Phi(incoming) = &function.inst(*inst).kind
                    {
                        let Some((_, source)) = incoming.iter().find(|(from, _)| *from == block_id)
                        else {
                            return true;
                        };
                        *value = *source;
                    }
                }
                let mut edge = stack.clone();
                if !materialize_pressure(&mut edge, &desired, &stored)
                    || edge.reconcile(&desired, prefix, version).is_err()
                {
                    return true;
                }
            }
        }
    }
    false
}

fn materialize_pressure(
    stack: &mut Stack<PressureSlot>,
    desired: &[PressureSlot],
    stored: impl Fn(mir::ValueId) -> bool,
) -> bool {
    for &slot in desired.iter().rev() {
        if !stack.values().contains(&slot) {
            if let PressureSlot::Value(value) = slot
                && !stored(value)
            {
                stack.push(slot);
            } else {
                return false;
            }
        }
    }
    true
}

fn prepare_pressure(
    stack: &mut Stack<PressureSlot>,
    operands: &[mir::ValueId],
    prefix: usize,
    version: EvmVersion,
    stored: impl Fn(mir::ValueId) -> bool,
    live: impl Fn(mir::ValueId) -> bool,
) -> bool {
    let slots = operands.iter().copied().map(PressureSlot::Value).collect::<Vec<_>>();
    materialize_pressure(stack, &slots, &stored)
        && stack
            .prepare(&slots, prefix, version, |slot| match slot {
                PressureSlot::Value(value) => stored(value) && live(value),
                PressureSlot::ReturnAddress => true,
                PressureSlot::Continuation => false,
            })
            .is_ok()
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

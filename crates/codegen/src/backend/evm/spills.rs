//! Stack-pressure planning and activation-local value homes.
//!
//! The planner simulates the physical scheduler over block entries, operand preparation, calls,
//! and phi edges to find functions that exceed the target DUP window. Dying operands are consumed
//! in place instead of being counted twice. Those functions receive reusable memory homes for
//! overlapping live intervals; short-lived expression temporaries, dying branch conditions and
//! single internal return values remain on the stack. A temporary may also end at an
//! allocation-local or low-memory store whose writes are disjoint from every compiler-owned word.
//! Calls, other writers and wider terminal protocols stop the bounded local window. A separate
//! temporary region permits phi edge copies to read every source before writing any destination,
//! including cyclic transfers. Ordinary low-pressure functions retain stack-only values.
//! A bounded set of additional non-Phi values can remain on the stack across blocks when their
//! conservative live intervals fit the same budget. Direct memory writers keep live operands in
//! homes; dying operands can remain resident when all values, operands and protocol backups fit
//! sixteen words. Larger windows freeze other residents below explicit backups. Calls and other
//! writers retain memory homes, and the scheduler validates each proposed mixed layout. Address
//! placement and dynamic-frame lifetime remain in storage planning; this module emits no physical
//! instructions.

use super::scheduler::Stack;
use crate::{
    analysis::{AddressSpace, AliasAnalysis, CfgInfo, Liveness},
    mir,
};
use overlap::disjoint_frame_write;
use solar_config::EvmVersion;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::{cmp::Reverse, collections::BinaryHeap};

mod overlap;

pub(crate) use overlap::{accesses_overlap, may_overlap};

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
        alias: &AliasAnalysis,
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
                    for (next_position, next_inst) in block
                        .instructions
                        .iter()
                        .copied()
                        .map(Some)
                        .chain([None])
                        .enumerate()
                        .skip(position + 1)
                        .take(window)
                    {
                        let Some(next_inst) = next_inst else {
                            let terminal_use = match &block.terminator {
                                Some(mir::Terminator::Branch { condition, .. }) => {
                                    *condition == value
                                }
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
                            if matches!(
                                next,
                                mir::InstKind::MStore(..) | mir::InstKind::MStore8(..)
                            ) && next.operands().contains(&value)
                                && !live.is_used_at_or_after(value, block_id, next_position + 1)
                                && disjoint_frame_write(
                                    function,
                                    &alias.instruction_mod_ref(function, next_inst),
                                )
                            {
                                local.insert(value);
                            }
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
        let intervals = live_intervals(function, live, cfg, &stored);
        let proposed = resident_candidates(function, live, cfg, alias, &intervals, window, &local);
        if proposed != local
            && !exceeds_stack_window(function, live, cfg, returning, version, |value| {
                stored(value) && proposed.contains(value)
            })
        {
            local = proposed;
        }
        let (homes, phi_scratch) = assign_homes(
            intervals.into_iter().filter(|(value, _)| !local.contains(*value)).collect(),
        );
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

/// Conservative intervals shared by stack-residence selection and memory-home reuse.
fn live_intervals(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    stored: impl Fn(mir::ValueId) -> bool,
) -> Vec<(mir::ValueId, (usize, usize))> {
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
    intervals.into_iter().collect()
}

/// Reuses words whose conservative live intervals do not overlap in physical block order.
fn assign_homes(
    mut intervals: Vec<(mir::ValueId, (usize, usize))>,
) -> (FxHashMap<mir::ValueId, usize>, usize) {
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

/// Adds whole-lifetime residents without changing the existing local exemptions or call protocol.
fn resident_candidates(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    alias: &AliasAnalysis,
    intervals: &[(mir::ValueId, (usize, usize))],
    words: usize,
    local: &DenseBitSet<mir::ValueId>,
) -> DenseBitSet<mir::ValueId> {
    let mut mandatory = DenseBitSet::new_empty(function.num_values());
    // Argument entry materialization has a separate cost from instruction result residence.
    for &(value, _) in intervals {
        if matches!(function.value(value), mir::Value::Arg(_)) {
            mandatory.insert(value);
        }
    }
    let mut uses = IndexVec::<mir::ValueId, usize>::from_vec(vec![0; function.num_values()]);
    for (block_id, block) in function.blocks.iter_enumerated() {
        if !cfg.is_reachable(block_id) {
            continue;
        }
        let mut before = DenseBitSet::new_empty(function.num_values());
        for value in live.live_out(block_id).iter() {
            before.insert(value);
        }
        if let Some(term) = &block.terminator {
            term.for_each_operand(|value| {
                before.insert(value);
                uses[value] += 1;
            });
            if matches!(term, mir::Terminator::TailCall { .. }) {
                mandatory.union(&before);
            }
        }
        for (position, &inst) in block.instructions.iter().enumerate().rev() {
            let kind = &function.inst(inst).kind;
            if let Some(value) = function.inst_result_value(inst) {
                before.remove(value);
                if matches!(kind, mir::InstKind::Phi(_)) {
                    mandatory.insert(value);
                }
            }
            let operands = kind.operands();
            for &value in &operands {
                uses[value] += 1;
                if matches!(kind, mir::InstKind::Phi(_)) {
                    mandatory.insert(value);
                } else {
                    before.insert(value);
                }
            }
            let effects = alias.instruction_mod_ref(function, inst);
            if matches!(kind, mir::InstKind::InternalCall { .. })
                || (effects.writes_space(AddressSpace::Memory)
                    && !disjoint_frame_write(function, &effects))
            {
                // Larger windows load writer operands above frozen residents and saved homes.
                // Reserve a return word, three protocol backups and three address temporaries.
                if direct_writer(kind) && before.count() + operands.len() + 7 <= 1024 {
                    // A dying operand may move through the backups only when the complete
                    // window fits legacy SWAP reach: values, operand copies, a return label,
                    // and three protocol backups. Literals are charged by operand arity.
                    let movable = before
                        .iter()
                        .filter(|&value| {
                            matches!(
                                function.value(value),
                                mir::Value::Inst(_) | mir::Value::Arg(_)
                            )
                        })
                        .take(17)
                        .count()
                        + operands.len()
                        + 4
                        <= 16;
                    for &operand in &operands {
                        if !movable || live.is_used_at_or_after(operand, block_id, position + 1) {
                            mandatory.insert(operand);
                        }
                    }
                } else {
                    mandatory.union(&before);
                }
            }
        }
    }
    if local.iter().any(|value| mandatory.contains(value)) {
        return local.clone();
    }
    let mut occupancy =
        vec![0usize; intervals.iter().map(|(_, (_, end))| end + 1).max().unwrap_or(0)];
    for &(value, (start, end)) in intervals {
        if local.contains(value) {
            for count in &mut occupancy[start..=end] {
                *count += 1;
            }
        }
    }
    let mut candidates = intervals
        .iter()
        .copied()
        .filter(|(value, _)| !local.contains(*value) && !mandatory.contains(*value))
        .collect::<Vec<_>>();
    candidates
        .sort_unstable_by_key(|&(value, (start, end))| (Reverse(uses[value]), end - start, value));
    let mut proposed = local.clone();
    for (value, (start, end)) in candidates {
        if occupancy[start..=end].iter().all(|&count| count < words) {
            proposed.insert(value);
            for count in &mut occupancy[start..=end] {
                *count += 1;
            }
        }
    }
    proposed
}

/// Zero-result source writers whose operands can load above a frozen activation prefix.
pub(super) fn direct_writer(kind: &mir::InstKind) -> bool {
    matches!(
        kind,
        mir::InstKind::MStore(..)
            | mir::InstKind::MStore8(..)
            | mir::InstKind::MCopy(..)
            | mir::InstKind::CalldataCopy(..)
            | mir::InstKind::CodeCopy(..)
            | mir::InstKind::ReturnDataCopy(..)
            | mir::InstKind::ExtCodeCopy(..)
    )
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
                    && stored(v)
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
            if let Some(value) = function.inst_result_value(inst)
                && stored(value)
            {
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

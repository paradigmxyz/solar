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
//! instructions. Pressure checks include the possible saved protocol prefix. The initial bound
//! describes an unspilled activation; resident validation uses the bound after a spill can disable
//! stack arguments. Protected words are retained through preparation and removed before results.
//!
//! After ordinary planning and frame reservation, an optional Phi-only proposal can retire
//! existing homes within the same eight-word lifetime budget. It preserves argument, call and
//! writer floors and all original residents. The machine layer admits this single interval-ranked
//! proposal through actual mixed-edge and writer emission, falling back to the ordinary plan on
//! failure. Single-use arithmetic sources immediately feeding a same-home, unpromoted Phi keep
//! their original stores when the remaining predecessor suffix is pure arithmetic. Retiring those
//! sources would only move the stores onto the edge, potentially adding stack permutations without
//! removing memory traffic or writer backups. Other uses and intervening observations retain the
//! proposal. It does not change home addresses, reservations or the stack-only fast path.
//!
//! When the Phi proposal is absent, a no-Phi owner with at most 256 allocated values can
//! instead start with each single-use optional home resident. This limits new operand-copy
//! costs beyond the ordinary eight-word budget; original residents keep their existing policy.
//! Up to eight physical pressure scans
//! restore one old home from the first failing site's conservative identity pool, preferring
//! longer lifetimes. Existing floors and original residents never change.
//! This only selects one emitted trial; actual writer costs and scheduling remain authoritative.
//! The value bound limits analysis; it alone proves neither complete activation capacity nor
//! writer cost. Caller exclusions and checked physical emission remain necessary.

use super::{op, scheduler::Stack};
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
        (returning, protocol_words): (bool, [usize; 2]),
        version: EvmVersion,
        stored: impl Fn(mir::ValueId) -> bool,
    ) -> Self {
        if !exceeds_stack_window(
            function,
            live,
            cfg,
            alias,
            (returning, protocol_words[0]),
            version,
            &stored,
            None,
        ) {
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
        let proposed =
            resident_candidates(function, live, cfg, alias, &intervals, window, (&local, None));
        if proposed != local
            && !exceeds_stack_window(
                function,
                live,
                cfg,
                alias,
                (returning, protocol_words[1]),
                version,
                |value| stored(value) && proposed.contains(value),
                None,
            )
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

    /// Proposes retiring only Phi-related homes, leaving the ordinary allocation intact.
    pub(crate) fn phi_residents(
        &self,
        function: &mir::Function,
        live: &Liveness,
        cfg: &CfgInfo,
        alias: &AliasAnalysis,
        version: EvmVersion,
        stored: impl Fn(mir::ValueId) -> bool,
    ) -> Option<DenseBitSet<mir::ValueId>> {
        if self.homes.is_empty() {
            return None;
        }
        let mut eligible = DenseBitSet::new_empty(function.num_values());
        for (block_id, block) in function.blocks.iter_enumerated() {
            if cfg.is_reachable(block_id) {
                for &inst in &block.instructions {
                    if let mir::InstKind::Phi(incoming) = &function.inst(inst).kind
                        && let Some(result) = function.inst_result_value(inst)
                    {
                        eligible.insert(result);
                        for &(_, value) in incoming {
                            eligible.insert(value);
                        }
                    }
                }
            }
        }
        if eligible.is_empty() {
            return None;
        }
        let intervals = live_intervals(function, live, cfg, stored);
        let mut resident = DenseBitSet::new_empty(function.num_values());
        for &(value, _) in &intervals {
            if !self.homes.contains_key(&value) {
                resident.insert(value);
            }
        }
        let window = version.reachable_stack_depth().saturating_sub(6).min(8);
        let mut proposed = resident_candidates(
            function,
            live,
            cfg,
            alias,
            &intervals,
            window,
            (&resident, Some(PhiCandidates { values: &eligible, homes: &self.homes })),
        );
        proposed.subtract(&resident);
        // Keep the original spill protocol without introducing a dummy home binding.
        (!proposed.is_empty() && proposed.count() < self.homes.len()).then_some(proposed)
    }

    /// Retires optional homes using at most eight first-failure pressure scans.
    /// The caller retains the static, nonreturning, no-hidden-prefix transaction.
    pub(crate) fn failure_residents(
        &self,
        function: &mir::Function,
        live: &Liveness,
        cfg: &CfgInfo,
        alias: &AliasAnalysis,
        version: EvmVersion,
        stored: impl Fn(mir::ValueId) -> bool,
    ) -> Option<DenseBitSet<mir::ValueId>> {
        if self.homes.is_empty()
            || function.num_values() > 256
            || function
                .instructions()
                .any(|id| matches!(function.inst(id).kind, mir::InstKind::Phi(_)))
        {
            return None;
        }
        let intervals = live_intervals(function, live, cfg, &stored);
        let (mandatory, uses) =
            residence_constraints(function, live, cfg, alias, &intervals, false);
        let mut original = DenseBitSet::new_empty(function.num_values());
        let mut proposed = DenseBitSet::new_empty(function.num_values());
        for &(value, _) in &intervals {
            if !self.homes.contains_key(&value) {
                original.insert(value);
            }
            if (!mandatory.contains(value) && uses[value] == 1) || original.contains(value) {
                proposed.insert(value);
            }
        }
        if original.iter().any(|value| mandatory.contains(value)) || proposed == original {
            return None;
        }
        let mut failure = DenseBitSet::new_empty(function.num_values());
        for _ in 0..8 {
            failure.clear();
            // Static, nonreturning owners need no protocol prefix here. Actual emission
            // separately checks saved home banks and every physical preparation.
            if !exceeds_stack_window(
                function,
                live,
                cfg,
                alias,
                (false, 0),
                version,
                |value| stored(value) && proposed.contains(value),
                Some(&mut failure),
            ) {
                proposed.subtract(&original);
                // Retain the ordinary spill protocol without a dummy home binding.
                return (!proposed.is_empty() && proposed.count() < self.homes.len())
                    .then_some(proposed);
            }
            let &(value, _) = intervals
                .iter()
                .filter(|(value, _)| {
                    failure.contains(*value)
                        && proposed.contains(*value)
                        && self.homes.contains_key(value)
                })
                .min_by_key(|&&(value, (start, end))| (Reverse(end - start), value))?;
            proposed.remove(value);
        }
        None
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

/// Optional Phi-related bindings considered after ordinary home allocation.
struct PhiCandidates<'a> {
    values: &'a DenseBitSet<mir::ValueId>,
    homes: &'a FxHashMap<mir::ValueId, usize>,
}

/// Shared argument, control-transfer and source-writer residence floors.
fn residence_constraints(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    alias: &AliasAnalysis,
    intervals: &[(mir::ValueId, (usize, usize))],
    allow_phi: bool,
) -> (DenseBitSet<mir::ValueId>, IndexVec<mir::ValueId, usize>) {
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
                if !allow_phi && matches!(kind, mir::InstKind::Phi(_)) {
                    mandatory.insert(value);
                }
            }
            let operands = kind.operands();
            for &value in &operands {
                uses[value] += 1;
                if matches!(kind, mir::InstKind::Phi(_)) {
                    if !allow_phi {
                        mandatory.insert(value);
                    }
                } else {
                    before.insert(value);
                }
            }
            let effects = alias.instruction_mod_ref(function, inst);
            if matches!(kind, mir::InstKind::ICall { .. })
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
    (mandatory, uses)
}

/// Adds whole-lifetime residents without changing the existing local exemptions or call protocol.
fn resident_candidates(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    alias: &AliasAnalysis,
    intervals: &[(mir::ValueId, (usize, usize))],
    words: usize,
    (local, phi_candidates): (&DenseBitSet<mir::ValueId>, Option<PhiCandidates<'_>>),
) -> DenseBitSet<mir::ValueId> {
    let (mandatory, uses) =
        residence_constraints(function, live, cfg, alias, intervals, phi_candidates.is_some());
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
        .filter(|(value, _)| {
            !local.contains(*value)
                && !mandatory.contains(*value)
                && phi_candidates
                    .as_ref()
                    .is_none_or(|candidates| candidates.values.contains(*value))
        })
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
    if let Some(candidates) = phi_candidates {
        retain_same_home_sources(function, cfg, candidates.homes, &uses, &mut proposed);
    }
    proposed
}

/// Keeps stores that residence would merely relocate from a pure suffix onto its outgoing edge.
fn retain_same_home_sources(
    function: &mir::Function,
    cfg: &CfgInfo,
    homes: &FxHashMap<mir::ValueId, usize>,
    uses: &IndexVec<mir::ValueId, usize>,
    proposed: &mut DenseBitSet<mir::ValueId>,
) {
    let mut sources = FxHashMap::default();
    for (predecessor, block) in function.blocks.iter_enumerated() {
        if cfg.is_reachable(predecessor)
            && let Some(mir::Terminator::Jump(target)) = block.terminator
        {
            for &inst in block.instructions.iter().rev() {
                let instruction = function.inst(inst);
                if instruction
                    .metadata
                    .effect()
                    .is_some_and(|effect| effect != mir::EffectKind::Pure)
                    || !instruction.kind.evm_opcode().is_some_and(
                        |opcode| matches!(opcode, op::ADD..=op::SIGNEXTEND | op::LT..=op::CLZ),
                    )
                {
                    break;
                }
                if let Some(value) = function.inst_result_value(inst)
                    && proposed.contains(value)
                    && uses[value] == 1
                    && homes.contains_key(&value)
                {
                    sources.insert(value, (predecessor, target));
                }
            }
        }
    }
    if sources.is_empty() {
        return;
    }
    for (target, block) in function.blocks.iter_enumerated() {
        if cfg.is_reachable(target) {
            for &inst in &block.instructions {
                if let mir::InstKind::Phi(incoming) = &function.inst(inst).kind
                    && let Some(result) = function.inst_result_value(inst)
                    && !proposed.contains(result)
                    && let Some(destination) = homes.get(&result)
                {
                    for &(predecessor, value) in incoming {
                        if sources.get(&value) == Some(&(predecessor, target))
                            && homes.get(&value) == Some(destination)
                        {
                            // value = arithmetic; store home, value; jump target
                            // result = phi [...: value] with the same retained home
                            proposed.remove(value);
                        }
                    }
                }
            }
        }
    }
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
    Protected(u8),
}

/// Uses the physical scheduler itself to account for dying operands and simultaneous edge copies.
#[expect(
    clippy::too_many_arguments,
    reason = "the existing pressure simulation accepts an optional failure pool without a separate context"
)]
fn exceeds_stack_window(
    function: &mir::Function,
    live: &Liveness,
    cfg: &CfgInfo,
    alias: &AliasAnalysis,
    (returning, protocol_words): (bool, usize),
    version: EvmVersion,
    stored: impl Fn(mir::ValueId) -> bool,
    mut failure: Option<&mut DenseBitSet<mir::ValueId>>,
) -> bool {
    // Collect only on failure, including requested operands not yet materialized.
    // The current stack may include tentative materializations; this is a conservative
    // site pool, not the precise inaccessible value after scheduler planning.
    let mut failed =
        |current: &[PressureSlot], requested: &[PressureSlot], operands: &[mir::ValueId]| {
            if let Some(values) = failure.as_deref_mut() {
                for slot in current.iter().chain(requested) {
                    if let PressureSlot::Value(value) = *slot {
                        values.insert(value);
                    }
                }
                for &value in operands {
                    values.insert(value);
                }
            }
            true
        };
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
            return failed(&entries[block_id], &[], &[]);
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
                        | mir::InstKind::ICall { .. }
                );
            let protected = if prepares
                && protocol_words != 0
                && kind.has_side_effects()
                && !matches!(kind, mir::InstKind::DataCopy(..))
            {
                overlap::protocol_words(&alias.instruction_mod_ref(function, inst), protocol_words)
            } else {
                0
            };
            if protected != 0 {
                let headroom = if matches!(kind, mir::InstKind::ICall { .. }) { 8 } else { 3 };
                if stack.values().len() + protected + operands.len() + headroom > 1024 {
                    return failed(stack.values(), &[], &operands);
                }
                // <current residents>; <saved protocol words>; <operand preparation follows>
                for index in 0..protected {
                    stack.push(PressureSlot::Protected(index as u8));
                }
            }
            if prepares
                && !prepare_pressure(&mut stack, &operands, prefix, version, &stored, |v| {
                    live.is_used_at_or_after(v, block_id, position + 1)
                })
            {
                return failed(stack.values(), &[], &operands);
            }
            if matches!(kind, mir::InstKind::ICall { .. }) {
                let caller = stack.values()[..stack.values().len() - operands.len()].to_vec();
                let mut desired = caller.clone();
                desired.push(PressureSlot::Continuation);
                desired.extend(operands.iter().rev().copied().map(PressureSlot::Value));
                stack.push(PressureSlot::Continuation);
                if stack.reconcile(&desired, prefix, version).is_err() {
                    return failed(stack.values(), &desired, &[]);
                }
                stack = Stack::new(caller);
            } else if prepares {
                stack.truncate(stack.values().len() - operands.len());
            }
            // <retained residents>; <restored protocol words are consumed>; <result follows>
            if protected != 0 {
                stack.truncate(stack.values().len() - protected);
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
                        return failed(stack.values(), &[], &[*condition]);
                    }
                    stack.truncate(stack.values().len() - 1);
                }
                mir::Terminator::Switch { value, .. } => {
                    if !prepare_pressure(&mut stack, &[*value], prefix, version, &stored, |v| {
                        live.live_out(block_id).contains(v)
                    }) {
                        return failed(stack.values(), &[], &[*value]);
                    }
                    stack.truncate(stack.values().len() - 1);
                }
                mir::Terminator::Return { values } if returning => {
                    for &value in values.iter().skip(1) {
                        if !prepare_pressure(&mut stack, &[value], prefix, version, &stored, |v| {
                            values.contains(&v)
                        }) {
                            return failed(stack.values(), &[], &[value]);
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
                        return failed(stack.values(), &desired, &[]);
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
                        return failed(stack.values(), &[], &[*recipient]);
                    }
                }
                mir::Terminator::TailCall { args, .. } => {
                    if !prepare_pressure(&mut stack, args, prefix, version, &stored, |_| false) {
                        return failed(stack.values(), &[], args);
                    }
                }
                mir::Terminator::ReturnData { offset, size }
                | mir::Terminator::Revert { offset, size }
                    if !prepare_pressure(
                        &mut stack,
                        &[*offset, *size],
                        prefix,
                        version,
                        &stored,
                        |_| false,
                    ) =>
                {
                    return failed(stack.values(), &[], &[*offset, *size]);
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
                            return failed(stack.values(), &[], &[]);
                        };
                        *value = *source;
                    }
                }
                let mut edge = stack.clone();
                if !materialize_pressure(&mut edge, &desired, &stored)
                    || edge.reconcile(&desired, prefix, version).is_err()
                {
                    return failed(edge.values(), &desired, &[]);
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
                PressureSlot::ReturnAddress | PressureSlot::Protected(_) => true,
                PressureSlot::Continuation => false,
            })
            .is_ok()
}

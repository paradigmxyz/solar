//! Bounded shared-expression contraction after ordinary MIR instruction scheduling.
//!
//! A ready-node traversal moves canonical native pure descendants of in-region mutable reads
//! inside hard-boundary regions. Reads and writes retain their order, instructions are emitted
//! exactly once, and SSA identities remain unchanged. The incumbent scheduler output is retained
//! unless exact region liveness improves at a pressured write without increasing any write's live
//! count or either the distinct-live peak or its result-before-input-retirement bound. CFG liveness
//! supplies region exits, including successor phi uses. These are SSA pressure measures, not a
//! physical stack-capacity or bytecode profitability proof. Eligibility also requires fewer shared
//! mutable captures live across that same improved write, matching storage, transient storage or
//! memory. Unrelated constant and calldata-only expressions keep their positions as fixed anchors.
//!
//! Only regions with more shared mutable captures live across a matching write than the target
//! can reach, and at most 128 instructions, are considered. Other live values do not qualify a
//! smaller captured-state bank: their rematerialization can make the incumbent cheaper. Readiness
//! scans are quadratic in that fixed bound. Liveness is computed once for eligible functions; a
//! reversible touched-value log compares orders without cloning or clearing function-sized sets per
//! region. This runs last so optional segment scheduling cannot invalidate the checked MIR order.

use super::{EvmInstSchedule, ScheduleScratch};
use crate::mir::{EffectKind, Function, InstId, InstKind, Value, ValueId, analysis::Liveness};
use solar_data_structures::bit_set::DenseBitSet;

const MAX_REGION: usize = 128;

pub(super) fn contract(
    func: &mut Function,
    reach: usize,
    shared: &DenseBitSet<InstId>,
    scratch: &mut ScheduleScratch,
) -> bool {
    // Keep the incumbent order; only pressure-driven shared regions need CFG liveness.
    let mut blocks = Vec::new();
    for (id, block) in func.blocks.iter_enumerated() {
        if !block
            .instructions
            .split(|&id| EvmInstSchedule::is_contraction_boundary(func.inst(id)))
            .any(|region| eligible(func, region, reach, shared))
        {
            continue;
        }
        scratch.clear_segment();
        for &inst in &block.instructions {
            scratch.members.insert(inst);
            scratch.active_members.push(inst);
        }
        if EvmInstSchedule::has_pressure(
            func,
            &block.instructions,
            block.terminator.as_ref(),
            reach,
            scratch,
        ) {
            blocks.push(id);
        }
        scratch.clear_segment();
    }
    if blocks.is_empty() {
        return false;
    }
    // Compute while every instruction list is intact. Only order changes below, so block
    // live-out and edge-specific phi uses remain valid for every later region.
    let liveness = Liveness::compute(func);
    let mut live = Live::new(func.num_values());
    let mut changed = false;
    for block in blocks {
        live.values.clear();
        live.count = 0;
        for value in liveness.live_out(block).iter() {
            live.use_value(func, value, false);
        }
        if let Some(term) = &func.blocks[block].terminator {
            term.for_each_operand(|value| live.use_value(func, value, false));
        }
        let mut instructions = std::mem::take(&mut func.blocks[block].instructions);
        let mut end = instructions.len();
        while end > 0 {
            if EvmInstSchedule::is_contraction_boundary(func.inst(instructions[end - 1])) {
                live.step(func, instructions[end - 1], false);
                end -= 1;
                continue;
            }
            let start = instructions[..end]
                .iter()
                .rposition(|&id| EvmInstSchedule::is_contraction_boundary(func.inst(id)))
                .map_or(0, |index| index + 1);
            let region = &mut instructions[start..end];
            let order = eligible(func, region, reach, shared)
                .then(|| schedule(func, region, scratch))
                .flatten();
            let captures = if order.is_some() {
                region
                    .iter()
                    .copied()
                    .filter(|&id| shared.contains(id) && mutable_read(func, id))
                    .collect::<Vec<_>>()
            } else {
                Vec::new()
            };
            scratch.clear_segment();
            let candidate = order
                .as_ref()
                .filter(|order| order.as_slice() != region)
                .map(|order| live.profile(func, order, &captures, true));
            live.restore();
            let Some(candidate) = candidate else {
                for &id in region.iter().rev() {
                    live.step(func, id, false);
                }
                end = start;
                continue;
            };
            let original = live.profile(func, region, &captures, false);
            if let Some(order) = order
                && candidate.peak <= original.peak
                && candidate.overlap <= original.overlap
                && candidate.writes.len() == original.writes.len()
                && candidate
                    .writes
                    .iter()
                    .zip(&original.writes)
                    .all(|(a, b)| a.0 == b.0 && a.1 <= b.1)
                && candidate
                    .writes
                    .iter()
                    .zip(&original.writes)
                    .any(|(a, b)| b.2 > reach && a.1 < b.1 && a.2 < b.2)
            {
                // fixed anchors; shared pure DAG -> earliest-ready pure DAG; same fixed anchors
                region.copy_from_slice(&order);
                changed = true;
            }
            end = start;
        }
        func.blocks[block].instructions = instructions;
    }
    changed
}

fn mutable_read(func: &Function, id: InstId) -> bool {
    let inst = func.inst(id);
    matches!(inst.kind, InstKind::SLoad(_) | InstKind::TLoad(_) | InstKind::MLoad(_))
        && inst.metadata.effect().is_none_or(|effect| effect == inst.kind.effect_kind())
}

fn matching_write(read: &InstKind, write: &InstKind) -> bool {
    matches!(
        (read, write),
        (InstKind::SLoad(_), InstKind::SStore(..))
            | (InstKind::TLoad(_), InstKind::TStore(..))
            | (InstKind::MLoad(_), InstKind::MStore(..) | InstKind::MStore8(..))
    )
}

fn eligible(
    func: &Function,
    region: &[InstId],
    reach: usize,
    shared: &DenseBitSet<InstId>,
) -> bool {
    region.len() > reach
        && region.len() <= MAX_REGION
        && region.iter().filter(|&&id| shared.contains(id) && mutable_read(func, id)).count()
            > reach
        && region.iter().any(|&read| {
            shared.contains(read)
                && mutable_read(func, read)
                && region.iter().any(|&write| {
                    EvmInstSchedule::is_source_write(func.inst(write))
                        && matching_write(&func.inst(read).kind, &func.inst(write).kind)
                })
        })
}

fn schedule(
    func: &Function,
    region: &[InstId],
    scratch: &mut ScheduleScratch,
) -> Option<Vec<InstId>> {
    scratch.clear_segment();
    for &id in region {
        scratch.members.insert(id);
        scratch.active_members.push(id);
        if func
            .inst(id)
            .kind
            .operands()
            .iter()
            .any(|&v| matches!(func.value(v), Value::Undef(_) | Value::Error(_)))
        {
            return None;
        }
    }
    let pure = |id| {
        let inst = func.inst(id);
        inst.kind.effect_kind() == EffectKind::Pure
            && inst.metadata.effect().is_none_or(|effect| effect == EffectKind::Pure)
            && inst.kind.evm_opcode().is_some()
            && func.inst_result_value(id).is_some()
    };
    // Mutable reads seed ancestry but stay fixed. Only their pure descendants can move;
    // independent constant address chains must not be pulled ahead of their original anchors.
    for &id in region {
        if mutable_read(func, id)
            || (pure(id) && func.inst(id).kind.operands().iter().any(|&value|
                matches!(func.value(value), Value::Inst(def) if scratch.consumer_roots.contains(*def))))
        {
            scratch.consumer_roots.insert(id);
        }
    }
    let movable = |id, scratch: &ScheduleScratch| pure(id) && scratch.consumer_roots.contains(id);
    let ready = |id, scratch: &ScheduleScratch| {
        func.inst(id).kind.operands().iter().all(|&v| {
        !matches!(func.value(v), Value::Inst(def) if scratch.members.contains(*def) && !scratch.visited.contains(*def))
    })
    };
    let mut order = Vec::with_capacity(region.len());
    let mut anchor = 0;
    // ready pure nodes; next fixed read/write; newly ready pure nodes; ...
    while order.len() < region.len() {
        let id = if let Some(&id) = region.iter().find(|&&id| {
            !scratch.visited.contains(id) && movable(id, scratch) && ready(id, scratch)
        }) {
            id
        } else {
            while region.get(anchor).is_some_and(|&id| movable(id, scratch)) {
                anchor += 1;
            }
            let &id = region.get(anchor)?;
            anchor += 1;
            if !ready(id, scratch) {
                return None;
            }
            id
        };
        if !scratch.visited.insert(id) {
            return None;
        }
        order.push(id);
    }
    Some(order)
}

struct Profile {
    peak: usize,
    overlap: usize,
    writes: Vec<(InstId, usize, usize)>,
}

struct Live {
    values: DenseBitSet<ValueId>,
    count: usize,
    undo: Vec<(ValueId, bool)>,
}

impl Live {
    fn new(values: usize) -> Self {
        Self { values: DenseBitSet::new_empty(values), count: 0, undo: Vec::new() }
    }

    fn set(&mut self, value: ValueId, present: bool, record: bool) {
        let changed = if present { self.values.insert(value) } else { self.values.remove(value) };
        if changed {
            if present {
                self.count += 1;
            } else {
                self.count -= 1;
            }
            if record {
                self.undo.push((value, !present));
            }
        }
    }

    fn use_value(&mut self, func: &Function, value: ValueId, record: bool) {
        if matches!(func.value(value), Value::Inst(_) | Value::Arg(_)) {
            self.set(value, true, record);
        }
    }

    fn step(&mut self, func: &Function, id: InstId, record: bool) -> usize {
        let result = func.inst_result_value(id);
        if let Some(value) = result {
            self.set(value, false, record);
        }
        // Phi incoming operands are edge uses already supplied by CFG liveness.
        if !matches!(func.inst(id).kind, InstKind::Phi(_)) {
            for value in func.inst(id).kind.operands() {
                self.use_value(func, value, record);
            }
        }
        // Include even a dead result's temporary word before retiring its inputs.
        self.count + usize::from(result.is_some())
    }

    fn profile(
        &mut self,
        func: &Function,
        order: &[InstId],
        captures: &[InstId],
        record: bool,
    ) -> Profile {
        let mut profile = Profile { peak: self.count, overlap: self.count, writes: Vec::new() };
        for &id in order.iter().rev() {
            let writer = EvmInstSchedule::is_source_write(func.inst(id));
            // Count live-after, excluding reads used only as write operands. Backward definition
            // kills and fixed read/write order ensure each counted local capture precedes the
            // write.
            let crossing = if writer {
                captures
                    .iter()
                    .filter(|&&read| {
                        matching_write(&func.inst(read).kind, &func.inst(id).kind)
                            && func
                                .inst_result_value(read)
                                .is_some_and(|value| self.values.contains(value))
                    })
                    .count()
            } else {
                0
            };
            profile.overlap = profile.overlap.max(self.step(func, id, record));
            profile.peak = profile.peak.max(self.count);
            if writer {
                profile.writes.push((id, self.count, crossing));
            }
        }
        profile
    }

    fn restore(&mut self) {
        while let Some((value, present)) = self.undo.pop() {
            self.set(value, present, false);
        }
    }
}

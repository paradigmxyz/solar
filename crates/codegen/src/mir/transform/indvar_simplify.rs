//! Induction-variable simplification and strength reduction.
//!
//! This pass recognizes loop-local address expressions of the form
//! `base + iv * stride + constant` and replaces their loop uses with a
//! loop-carried pointer phi:
//!
//! ```text
//! ptr = phi [preheader: base + init * stride + constant], [latch: ptr + step * stride]
//! ```
//!
//! The initial implementation is deliberately narrow. It requires canonical
//! loops with a dedicated preheader, a single latch, and a single additive
//! induction-variable update. That gives later loop optimizations a real
//! ScalarEvolution-backed transform without guessing from ad hoc instruction
//! patterns.
//!
//! A counter that only some paths around the loop advance is reduced the same
//! way. A merge of two sorted inputs steps `i` on the arms that consume `a[i]`
//! and leaves it alone on the others, so the header phi's latch value reaches
//! it through the merge phis of those arms, each leaf being the counter or the
//! counter plus a constant. The pointer mirrors that shape: one phi beside
//! every merge phi and one add beside every leaf, so it holds
//! `base + iv * stride + constant` wherever the counter is in scope. Address
//! uses in the blocks the header dominates, such as the start of a loop that
//! copies the remainder, are replaced as well, since the pointer is valid
//! there and they would otherwise keep the counter alive past its exit test.
//!
//! The address may scale the induction variable negatively (a pointer walking
//! down from the end of an array) and may add scaled loop invariants, such as
//! `base + 32 * length - 32 * i`: the pointer's start is computed once in the
//! preheader and the latch adds or subtracts the stride. All arithmetic is the
//! same modular word arithmetic as the expression it replaces.
//!
//! Addresses that differ only by a constant, such as `a[j]` and `a[j - 1]`,
//! share one pointer: the most used one becomes the phi and each sibling's
//! definition is rewritten in place to the pointer plus its offset, so the loop
//! carries one word instead of one per offset.
//!
//! A rewrite pays when the operations it removes from every iteration, the
//! index duplication, scaling, base duplication and additions, outweigh the one
//! duplication per pointer use, two per sibling use, the latch update, and the
//! extra carried word it adds. The pass runs on the semantic MIR and once more
//! in gas mode after memory lowering, where element addresses become explicit.
//!
//! When the header's exit test compares the counter with an invariant bound
//! and the counter has no other use than the addresses being replaced, the
//! test is replaced by comparing an ascending pointer with its value at the
//! bound, computed once in the preheader, and the counter's phi and update
//! die. Then every address family is reduced at once, since removing the
//! counter pays for a pointer that alone would only break even. The pointer
//! must not wrap between the start and the bound: its base is a heap address,
//! bounded by the memory a call can afford, and a loop cannot run
//! `2^MAX_TRIP_COUNT_BITS` iterations, the trip-count assumption the loop split
//! also makes, so a scaled bound below that stays far from the word size.
//! A comparison of the counter elsewhere, such as the second half of
//! `i < a.length && j < b.length`, is taken over the same way, but its bound
//! is not a trip count the loop could not reach anyway, so only an object's
//! length word qualifies: every allocation keeps that far below where a
//! scaled pointer could wrap.
//!
//! Safety contract:
//! - require canonical loops with a preheader; several latches must pass one value
//! - rewrite only affine address expressions derived from the recognized induction variable
//! - preserve the original address value when it is still used outside the loop
//! - recognize checked unsigned word updates while retaining their failure checks.
//! - add only one scaled address counter when the original update must stay live.

use super::egraph::max_bits_with_args;
use crate::mir::{
    ArithmeticKind, BlockId, CheckedOp, Function, Immediate, InstId, InstKind, Instruction,
    MemoryRegion, MirType, Module, Terminator, Value, ValueId,
    analysis::{
        AffineTerm, AliasAnalysis, CfgInfo, Loop, LoopAnalyzer, MemoryBase, ScalarEvolution,
    },
    memory::EvmMemoryLayout,
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
    utils as mir_utils,
};
use alloy_primitives::U256;
use solar_data_structures::{
    bit_set::DenseBitSet,
    map::{FxHashMap, FxHashSet},
};
use std::rc::Rc;

/// Function pass for induction-variable simplification and strength reduction.
pub(crate) struct IndVarSimplify;

impl MirPass for IndVarSimplify {
    fn name(&self) -> &'static str {
        "indvar-simplify"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut selected = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if !func.blocks.is_empty() && !analyses.cfg(id, func).cyclic_blocks().is_empty() {
                selected.insert(id);
            }
        }
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                IndVarSimplifier::new(Rc::clone(analyses.alias()))
                    .run(func, Rc::clone(analyses.cfg()))
                    .total()
                    != 0
            },
        )
    }
}

/// Statistics from induction-variable simplification.
#[derive(Clone, Debug, Default)]
struct IndVarSimplifyStats {
    /// Number of loop-carried pointer phis inserted.
    pointer_phis_inserted: usize,
    /// Number of loop-local address uses replaced.
    address_uses_replaced: usize,
}

impl IndVarSimplifyStats {
    /// Returns the total number of MIR changes performed.
    #[must_use]
    const fn total(&self) -> usize {
        self.pointer_phis_inserted + self.address_uses_replaced
    }
}

/// Performs conservative induction-variable strength reduction.
#[derive(Debug)]
struct IndVarSimplifier {
    stats: IndVarSimplifyStats,
    alias: Rc<AliasAnalysis>,
}

/// The control-flow region in which one loop counter can be reduced.
#[derive(Clone, Copy)]
struct LoopRegion<'a> {
    loop_data: &'a Loop,
    blocks: &'a DenseBitSet<BlockId>,
    preheader: BlockId,
    latches: &'a [BlockId],
}

/// A comparison of the counter, or of one of its updates, with an invariant
/// bound: `lt`, `gt` or `eq` with the subject on either side.
#[derive(Clone, Copy)]
struct ExitTest {
    condition: InstId,
    /// The counter or an update of it.
    subject: ValueId,
    bound: ValueId,
    subject_first: bool,
    comparison: Comparison,
}

/// The unsigned comparison an exit test makes; a pointer that grows with the
/// counter satisfies the same one against its end.
#[derive(Clone, Copy)]
enum Comparison {
    Lt,
    Gt,
    Eq,
}

/// What one update adds to the counter.
#[derive(Clone, Copy, Debug)]
enum Step {
    /// A signed constant.
    Constant(i128),
    /// A small word, such as the condition an if-converted arm turned into
    /// `i += (u < v)`; `negative` when the update subtracts it.
    Value { value: ValueId, negative: bool },
}

impl Step {
    /// Whether the update can change the counter.
    fn moves(self) -> bool {
        !matches!(self, Self::Constant(0))
    }
}

/// A loop counter: a header phi that every path around the loop advances by
/// a constant or a small value, or leaves alone. The latch value reaches the
/// phi through merge phis inside the loop; each leaf is `counter + step`, or
/// the counter itself.
#[derive(Clone, Debug)]
struct Counter {
    /// The header phi.
    value: ValueId,
    /// The value entering from the preheader.
    init: ValueId,
    /// The latch's incoming value: a leaf result or a merge phi.
    latch_value: ValueId,
    /// The merge phis between the leaves and the header phi, all in the loop.
    phis: Vec<InstId>,
    /// The updates `counter + step` with their steps.
    leaves: Vec<(InstId, Step)>,
}

impl Counter {
    /// The phis and updates that together define the counter.
    fn definers(&self, func: &Function) -> FxHashSet<InstId> {
        let Value::Inst(phi) = *func.value(self.value) else { return FxHashSet::default() };
        [phi]
            .into_iter()
            .chain(self.phis.iter().copied())
            .chain(self.leaves.iter().map(|&(inst_id, _)| inst_id))
            .collect()
    }

    /// The results of the updates.
    fn update_values<'a>(&'a self, func: &'a Function) -> impl Iterator<Item = ValueId> + 'a {
        self.leaves.iter().filter_map(|&(inst_id, _)| func.inst_result_value(inst_id))
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct AddressKey {
    /// The unscaled invariant base, if any.
    base: Option<ValueId>,
    /// Scaled invariant terms, ordered by value.
    invariants: Vec<AffineTerm>,
    iv: ValueId,
    scale: i128,
    constant: i128,
}

impl AddressKey {
    /// The key without its constant: addresses sharing it walk in step and can
    /// share one pointer.
    fn family(&self) -> Self {
        Self { constant: 0, ..self.clone() }
    }

    /// Operations one use of the expression costs inside the loop. The offset
    /// is free when the address is built from the variable's own update,
    /// `a[--i]` or `a[j - 1]` beside `--j`, which the loop computes anyway. A
    /// scaled invariant costs its scaling and an add; an unscaled one only
    /// the add, and often not even that when the loop shares the sum with a
    /// bounds check.
    fn use_cost(&self, offset_shared: bool) -> usize {
        let scaling = if self.scale.abs() == 1 { 0 } else { 2 };
        let base = if self.base.is_some() { 2 } else { 0 };
        let invariants = self
            .invariants
            .iter()
            .map(|term| if term.scale.abs() == 1 { 2 } else { 4 })
            .sum::<usize>();
        let offset = if self.constant != 0 && !offset_shared { 2 } else { 0 };
        1 + scaling + base + invariants + offset
    }
}

impl IndVarSimplifier {
    /// Creates a new induction-variable simplifier.
    #[must_use]
    fn new(alias: Rc<AliasAnalysis>) -> Self {
        Self { stats: IndVarSimplifyStats::default(), alias }
    }

    /// Runs induction-variable simplification once over `func`.
    fn run(&mut self, func: &mut Function, cfg: Rc<CfgInfo>) -> &IndVarSimplifyStats {
        self.stats = IndVarSimplifyStats::default();

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze_with_cfg(func, cfg);
        let loops: Vec<_> = loop_info.loops.values().cloned().collect();

        // A loop that copies the remainder starts its counter from the merge
        // loop's final one: a phi use that keeps the first loop's counter alive
        // until the second loop is reduced and reads it through an address
        // instead. Reducing in rounds resolves such chains in either order; a
        // round that changes nothing ends them.
        for _ in 0..Self::ROUNDS {
            let before = self.stats.total();
            for loop_data in &loops {
                self.run_loop(func, &analyzer, loop_data);
            }
            if self.stats.total() == before {
                break;
            }
        }

        &self.stats
    }

    /// Rounds over the loops of one function.
    const ROUNDS: usize = 3;

    fn run_loop(&mut self, func: &mut Function, analyzer: &LoopAnalyzer, loop_data: &Loop) {
        let Some(preheader) = loop_data.preheader else { return };
        // Several latches are one when they all pass the same value, the way
        // the arms of a conditional store each jump back to the header.
        let latches = loop_data.back_edges.as_slice();
        if latches.is_empty() {
            return;
        }
        // The pointers are header phis, so they are in scope in every block the
        // header dominates: the loop, and the code after it that reads the
        // counter's final value through the same addresses.
        let mut region = DenseBitSet::new_empty(func.blocks.len());
        for block in func.blocks.indices() {
            if analyzer.dominates(loop_data.header, block) {
                region.insert(block);
            }
        }
        // A loop may step more than one counter, each walking its own addresses:
        // a codec reading an input and writing an output steps both, and taking
        // only the single-counter case left every such loop rebuilding both
        // address families from scratch every iteration. Reduce them one at a
        // time. A reduction only retypes instructions, adds phis with their
        // updates, and deletes arithmetic it just made dead, so the blocks,
        // preheader and back edge analyzed here stay valid for the next.
        let lengths = self.stored_lengths(func, analyzer, loop_data.header);
        // The pointer's end for a bound is built in the preheader, so a bound
        // must be defined where the preheader can read it.
        let mut before = DenseBitSet::new_empty(func.blocks.len());
        for block in func.blocks.indices() {
            if analyzer.dominates(block, preheader) {
                before.insert(block);
            }
        }
        let region = LoopRegion { loop_data, blocks: &region, preheader, latches };
        for counter in Self::counters(func, loop_data, preheader, latches) {
            self.reduce_counter(func, region, &before, &lengths, counter);
        }
    }

    /// The words stored as an object's length before `header`: a length the
    /// function computed itself, `(n / 4) * 3` say, is as bounded as one read
    /// back from the object, since the allocation checked it.
    fn stored_lengths(
        &self,
        func: &Function,
        analyzer: &LoopAnalyzer,
        header: BlockId,
    ) -> FxHashSet<ValueId> {
        let mut lengths = FxHashSet::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            if !analyzer.dominates(block_id, header) {
                continue;
            }
            for &inst_id in &block.instructions {
                let (slot, length) = match func.inst(inst_id).kind {
                    InstKind::MStore(slot, length) => (slot, length),
                    InstKind::SetMemoryObjectLen(object, length, _) => (object, length),
                    _ => continue,
                };
                if !self.is_length_slot(func, slot) {
                    continue;
                }
                lengths.insert(length);
                // `n + 29` stored as a length bounds `n` as well: an
                // allocation with slack is cut back to the length it computed.
                if let Some(&InstKind::Add(a, b)) = inst_kind(func, length) {
                    if func.value_u256(b).is_some() {
                        lengths.insert(a);
                    } else if func.value_u256(a).is_some() {
                        lengths.insert(b);
                    }
                }
            }
        }
        lengths
    }

    /// Whether `slot` is the first word of a memory object: a memory
    /// reference, or the start of an allocation, which after allocation
    /// lowering is a read of the free-memory pointer.
    fn is_length_slot(&self, func: &Function, slot: ValueId) -> bool {
        if func.value_ty(slot).is_some_and(MirType::is_memory_reference) {
            return true;
        }
        let Some(address) = self.alias.memory_address(func, slot) else { return false };
        if address.region != MemoryRegion::Heap || address.offset != 0 {
            return false;
        }
        match address.base {
            MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_) => true,
            MemoryBase::Value(base) => {
                let Value::Inst(inst_id) = func.value(base) else { return false };
                match func.inst(*inst_id).kind {
                    InstKind::Fmp => true,
                    InstKind::MLoad(fmp) => func.value_u64(fmp) == Some(EvmMemoryLayout::FMP_SLOT),
                    _ => false,
                }
            }
            _ => false,
        }
    }

    /// The header phis every path around the loop advances by a constant or
    /// leaves alone, found by walking each latch value down through the merge
    /// phis inside the loop to the updates of the phi itself.
    fn counters(
        func: &Function,
        loop_data: &Loop,
        preheader: BlockId,
        latches: &[BlockId],
    ) -> Vec<Counter> {
        let header = loop_data.header;
        let in_loop = |inst_id: InstId| {
            loop_data.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
        };
        let mut counters = Vec::new();
        for &inst_id in &func.blocks[header].instructions {
            let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
            let Some(value) = func.inst_result_value(inst_id) else { continue };
            let mut init = None;
            let mut latch_value = None;
            let mut agreed = true;
            for &(block, incoming_value) in incoming {
                if block == preheader {
                    init = Some(incoming_value);
                } else if latches.contains(&block) {
                    agreed &= latch_value.is_none_or(|value| value == incoming_value);
                    latch_value = Some(incoming_value);
                }
            }
            let (Some(init), Some(latch_value)) = (init, latch_value) else { continue };
            if !agreed {
                continue;
            }

            let mut phis = Vec::new();
            let mut leaves = Vec::new();
            let mut pending = vec![latch_value];
            let mut visited = FxHashSet::default();
            let mut recognized = true;
            while let Some(pending_value) = pending.pop() {
                // The counter itself: a path that leaves it alone.
                if pending_value == value || !visited.insert(pending_value) {
                    continue;
                }
                let Value::Inst(definer) = *func.value(pending_value) else {
                    recognized = false;
                    break;
                };
                if !in_loop(definer) {
                    recognized = false;
                    break;
                }
                match &func.inst(definer).kind {
                    // Another header phi would make this counter a function of
                    // that one; a merge phi below the header is a step's join.
                    InstKind::Phi(merged) => {
                        if func.blocks[header].instructions.contains(&definer) {
                            recognized = false;
                            break;
                        }
                        phis.push(definer);
                        pending.extend(merged.iter().map(|&(_, merged_value)| merged_value));
                    }
                    kind => match Self::step_of(func, kind, value) {
                        Some(step) => leaves.push((definer, step)),
                        None => {
                            recognized = false;
                            break;
                        }
                    },
                }
            }
            if recognized && leaves.iter().any(|&(_, step)| step.moves()) {
                counters.push(Counter { value, init, latch_value, phis, leaves });
            }
        }
        counters
    }

    /// The step `kind` adds to `counter`, when it is an update of it: a signed
    /// constant, or a value of at most [`Self::VALUE_STEP_BITS`] bits, which
    /// keeps a counter bounded by the trip count from wrapping.
    fn step_of(func: &Function, kind: &InstKind, counter: ValueId) -> Option<Step> {
        let step = |value: ValueId, negative: bool| match func.value(value) {
            Value::Immediate(imm) => {
                let constant = u256_to_i128(imm.as_u256()?)?;
                Some(Step::Constant(if negative { constant.checked_neg()? } else { constant }))
            }
            _ => (max_bits_with_args(func, value, Self::VALUE_STEP_DEPTH, &|_| 256)
                <= Self::VALUE_STEP_BITS)
                .then_some(Step::Value { value, negative }),
        };
        match *kind {
            InstKind::Add(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs: a,
                rhs: b,
            } if a == counter => step(b, false),
            InstKind::Add(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs: a,
                rhs: b,
            } if b == counter => step(a, false),
            InstKind::Sub(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Sub,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs: a,
                rhs: b,
            } if a == counter => step(b, true),
            _ => None,
        }
    }

    /// The widest value step accepted, a byte: a rune length from a table or
    /// a condition, never a word that could carry the counter around.
    const VALUE_STEP_BITS: u32 = 8;

    /// How far the width of a value step is traced.
    const VALUE_STEP_DEPTH: u32 = 4;

    /// Replaces one counter's address expressions with carried pointers.
    fn reduce_counter(
        &mut self,
        func: &mut Function,
        loop_region: LoopRegion<'_>,
        before: &DenseBitSet<BlockId>,
        lengths: &FxHashSet<ValueId>,
        counter: Counter,
    ) {
        let LoopRegion { loop_data, blocks: region, preheader, .. } = loop_region;
        // Reducing an earlier counter can delete this one's updates as dead
        // address arithmetic, leaving the recorded instructions outside the loop.
        let in_loop = |func: &Function, inst_id: InstId| {
            loop_data.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
        };
        if counter.leaves.iter().any(|&(inst_id, _)| !in_loop(func, inst_id)) {
            return;
        }
        let must_keep_update = counter
            .leaves
            .iter()
            .any(|&(inst_id, _)| func.inst(inst_id).kind.effects().must_execute(false));

        // The pointer is valid in the whole region, so the affine analysis
        // covers it: an address computed after the loop from the counter's
        // final value is the same expression, not a new invariant base.
        let mut region_loop = loop_data.clone();
        region_loop.blocks = region.clone();
        let scev = ScalarEvolution::analyze_with_counters(func, &region_loop, &[counter.value]);
        let carried = Self::carried_words(func, loop_data);
        let update_values = counter.update_values(func).collect::<Vec<_>>();
        let mut candidates: FxHashMap<AddressKey, Vec<ValueId>> = FxHashMap::default();
        let mut offset_shared = FxHashSet::default();
        let mut loop_addresses = FxHashSet::default();

        for block in region.iter() {
            for &inst_id in &func.blocks[block].instructions {
                let Some(value) = func.inst_result_value(inst_id) else { continue };
                if !self.is_reducible_result(func, inst_id) {
                    continue;
                }
                let Some(key) = self.address_key(&scev, value, counter.value) else {
                    continue;
                };
                // A checked update must remain live, so a second plain-add counter saves no work.
                if must_keep_update && key.scale == 1 {
                    continue;
                }
                let scalable = |step: Step| match step {
                    Step::Constant(constant) => key.scale.checked_mul(constant).is_some(),
                    Step::Value { .. } => true,
                };
                if !counter.leaves.iter().all(|&(_, step)| scalable(step))
                    || !self.has_non_address_use(func, &region_loop, value)
                {
                    continue;
                }
                if update_values
                    .iter()
                    .any(|&update| Self::depends_on(func, &region_loop, value, update, 0))
                {
                    offset_shared.insert(value);
                }
                if loop_data.blocks.contains(block) {
                    loop_addresses.insert(value);
                }
                candidates.entry(key).or_default().push(value);
            }
        }

        // Checked updates cannot die when their address uses disappear. Limit the additional
        // loop-carried state instead of creating a counter for every field address.
        if loop_addresses.is_empty() || (must_keep_update && candidates.len() != 1) {
            return;
        }

        let addresses = candidates.values().flatten().copied().collect::<FxHashSet<_>>();
        let mut families: FxHashMap<AddressKey, Vec<(AddressKey, Vec<ValueId>)>> =
            FxHashMap::default();
        for (key, values) in candidates {
            families.entry(key.family()).or_default().push((key, values));
        }
        let mut families = families.into_values().collect::<Vec<_>>();
        // A family read only after the loop saves nothing per iteration.
        families.retain(|members| {
            members
                .iter()
                .flat_map(|(_, values)| values)
                .any(|value| loop_addresses.contains(value))
        });
        for members in &mut families {
            // The most used offset carries the pointer; ties go to the smallest offset.
            members.sort_by_key(|(key, values)| {
                let uses = values.iter().filter(|value| loop_addresses.contains(value)).count();
                (std::cmp::Reverse(uses), key.constant.abs())
            });
        }
        families.sort_by_key(|members| members[0].0.family().constant);

        // With the counter free once every address is a pointer, an ascending pointer
        // that cannot wrap takes over the exit test and the counter dies; that credit
        // is weighed across all families at once.
        let exit_tests = self.counter_exit_tests(func, loop_data, region, lengths, &counter);
        let definers = counter.definers(func);
        let counter_free = !must_keep_update
            && !exit_tests.is_empty()
            && Self::counter_only_feeds(func, region, &counter, &exit_tests, &definers, &addresses);
        let test_family = if counter_free {
            families.iter().position(|members| {
                let key = &members[0].0;
                key.scale > 0
                    && key.invariants.is_empty()
                    && key.base.is_some_and(|base| self.is_heap_address(func, base))
            })
        } else {
            None
        };
        let reduce_all = test_family.is_some() && {
            let before = families
                .iter()
                .map(|members| Self::family_cost_before(members, &offset_shared, &loop_addresses))
                .sum::<usize>();
            let after = families
                .iter()
                .map(|members| Self::family_cost_after(members, carried, &loop_addresses, &counter))
                .sum::<usize>();
            before + Self::COUNTER_COST > after
        };

        let mut replacements = FxHashMap::default();
        let mut siblings = Vec::new();
        let mut test_pointer = None;
        for (index, members) in families.iter().enumerate() {
            let (primary, primary_values) = &members[0];
            // ptr = phi [preheader: start], [latch: ptr + delta]
            // costs one update per iteration plus a carried word the scheduler
            // must keep resident; a sibling offset costs an add at its definition.
            let pays = reduce_all
                || Self::reduction_pays_off(
                    members,
                    &offset_shared,
                    carried,
                    &loop_addresses,
                    &counter,
                );
            tracing::trace!(
                function = %func.name,
                header = ?loop_data.header,
                family = ?members.iter().map(|(key, values)| (key.constant, values.len())).collect::<Vec<_>>(),
                scale = primary.scale,
                base = ?primary.base,
                invariants = primary.invariants.len(),
                carried,
                counter_free,
                test_family = test_family == Some(index),
                base_region = ?primary
                    .base
                    .and_then(|base| self.alias.memory_address(func, base))
                    .map(|address| (address.region, address.base)),
                pays,
                "pointer family"
            );
            if !pays {
                continue;
            }
            let derived = Self::derived_values(func, region, counter.value);
            let subjects = exit_tests
                .iter()
                .filter_map(|test| Some((test.subject, *derived.get(&test.subject)?)))
                .collect::<FxHashMap<_, _>>();
            let Some((pointer, mirrors)) =
                self.materialize_pointer_phi(func, loop_region, &counter, primary, &subjects)
            else {
                tracing::trace!(
                    function = %func.name,
                    header = ?loop_data.header,
                    ?primary,
                    "pointer start not materializable"
                );
                continue;
            };
            if reduce_all && test_family == Some(index) {
                test_pointer = Some((pointer, primary.clone(), mirrors));
            }
            for &value in primary_values {
                replacements.insert(value, pointer);
            }
            for (key, values) in &members[1..] {
                let Some(offset) = key.constant.checked_sub(primary.constant) else { continue };
                for &value in values {
                    siblings.push((value, pointer, offset));
                }
            }
        }

        // exit: lt counter, bound  =>  lt ptr, end   with end = ptr's value at the bound
        // and the same for gt and eq
        let mut test_rewritten = false;
        if let Some((_, key, mirrors)) = test_pointer.as_ref()
            && replacements.len() + siblings.len() == addresses.len()
        {
            let mut ends = FxHashMap::default();
            for test in &exit_tests {
                let Some(&subject) = mirrors.get(&test.subject) else { continue };
                let end = match ends.get(&test.bound) {
                    Some(&end) => end,
                    None => {
                        // A bound the preheader can read is priced there
                        // once; one defined after the loop, a length read
                        // back for a later check, gets its end beside it.
                        let bound_block = match func.value(test.bound) {
                            Value::Inst(inst_id) => region
                                .iter()
                                .find(|&block| func.blocks[block].instructions.contains(inst_id)),
                            _ => None,
                        };
                        let end = match bound_block {
                            Some(block) if !before.contains(block) => {
                                let position = func.blocks[block]
                                    .instructions
                                    .iter()
                                    .position(|&inst_id| {
                                        func.inst_result_value(inst_id) == Some(test.bound)
                                    })
                                    .map_or(0, |position| position + 1);
                                self.pointer_at_position(func, block, position, key, test.bound)
                            }
                            _ => self.pointer_at(func, preheader, key, test.bound),
                        };
                        let Some(end) = end else { continue };
                        ends.insert(test.bound, end);
                        end
                    }
                };
                let (a, b) = if test.subject_first { (subject, end) } else { (end, subject) };
                func.inst_mut(test.condition).kind = match test.comparison {
                    Comparison::Lt => InstKind::Lt(a, b),
                    Comparison::Gt => InstKind::Gt(a, b),
                    Comparison::Eq => InstKind::Eq(a, b),
                };
                self.stats.address_uses_replaced += 1;
                test_rewritten = true;
            }
        }

        // An address after the loop reads the pointer only once the counter
        // dies with it; while the counter lives on, rebuilding the address from
        // it there is cheaper than carrying the pointer out of the loop as well.
        if !test_rewritten {
            siblings.retain(|(value, ..)| loop_addresses.contains(value));
            replacements.retain(|value, _| loop_addresses.contains(value));
        }

        // sibling = ptr + offset, in place of its old address arithmetic
        for (value, pointer, offset) in siblings {
            let Value::Inst(inst_id) = *func.value(value) else { continue };
            let Some(magnitude) = offset.checked_abs() else { continue };
            let Some(magnitude) = self.offset_value(func, magnitude) else { continue };
            func.inst_mut(inst_id).kind = if offset >= 0 {
                InstKind::Add(pointer, magnitude)
            } else {
                InstKind::Sub(pointer, magnitude)
            };
            self.stats.address_uses_replaced += 1;
        }

        if replacements.is_empty() {
            return;
        }

        self.stats.address_uses_replaced += self.replace_uses(func, region, &replacements);
        // The replaced addresses and the index arithmetic only they read are dead now;
        // remove them here so the counter's remaining reads are visible below.
        self.remove_dead_address_arithmetic(func, region);
        if test_rewritten {
            self.remove_dead_counter(func, region, &counter);
        }
    }

    /// Removes the region's pure address arithmetic whose results nothing reads any more,
    /// following each removal to the operands it leaves unread.
    fn remove_dead_address_arithmetic(&self, func: &mut Function, region: &DenseBitSet<BlockId>) {
        let mut uses = FxHashMap::<ValueId, usize>::default();
        for block in &func.blocks {
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst_id| func.inst(inst_id).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                *uses.entry(operand).or_default() += 1;
            }
        }
        let in_region = |func: &Function, inst_id: InstId| {
            region.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
        };
        let mut pending = Vec::new();
        for block in region.iter() {
            for &inst_id in &func.blocks[block].instructions {
                if Self::is_address_builder(&func.inst(inst_id).kind)
                    && func
                        .inst_result_value(inst_id)
                        .is_some_and(|result| uses.get(&result).copied().unwrap_or_default() == 0)
                {
                    pending.push(inst_id);
                }
            }
        }
        while let Some(inst_id) = pending.pop() {
            let operands = func.inst(inst_id).kind.operands();
            for block in region.iter() {
                func.blocks[block].instructions.retain(|&other| other != inst_id);
            }
            for operand in operands {
                let Some(count) = uses.get_mut(&operand) else { continue };
                *count = count.saturating_sub(1);
                if *count == 0
                    && let Value::Inst(definer) = *func.value(operand)
                    && in_region(func, definer)
                    && Self::is_address_builder(&func.inst(definer).kind)
                {
                    pending.push(definer);
                }
            }
        }
    }

    /// Operations the counter's phi and update cost per iteration: the increment
    /// and its carried word.
    const COUNTER_COST: usize = 4;

    /// The comparisons of the counter with an invariant bound that the pointer
    /// can take over. The header's compares against the loop's trip bound. A
    /// test elsewhere, the second half of `i < a.length && j < b.length`, is
    /// the counter's exit test too, but its bound is not a trip count that the
    /// loop could not reach anyway, so it is accepted only when the bound is
    /// an object's length, a word every allocation this compiler emits keeps
    /// far below where a scaled pointer could wrap.
    fn counter_exit_tests(
        &self,
        func: &Function,
        loop_data: &Loop,
        region: &DenseBitSet<BlockId>,
        lengths: &FxHashSet<ValueId>,
        counter: &Counter,
    ) -> Vec<ExitTest> {
        // `i + 4 <= n` tests the update, and the tail after a loop tests
        // `i + 1 < n`; each such value gets a mirror beside its definition.
        let subjects = Self::derived_values(func, region, counter.value);
        // A bound is any value the loop does not change: one the preheader
        // can read, or one defined after the loop for a test there.
        let invariant = |value: ValueId| match func.value(value) {
            Value::Immediate(_) | Value::Arg(_) => true,
            Value::Inst(inst_id) => !loop_data
                .blocks
                .iter()
                .any(|block| func.blocks[block].instructions.contains(inst_id)),
            Value::Undef(_) | Value::Error(_) => false,
        };
        let object_length = |value: ValueId| match func.value(value) {
            Value::Immediate(_) => true,
            Value::Inst(inst_id) => {
                lengths.contains(&value)
                    || match func.inst(*inst_id).kind {
                        InstKind::MemoryObjectLen(..) => true,
                        InstKind::MLoad(slot) => self.is_length_slot(func, slot),
                        _ => false,
                    }
            }
            Value::Arg(_) | Value::Undef(_) | Value::Error(_) => false,
        };
        let mut tests = Vec::new();
        for block in region.iter() {
            let in_header = block == loop_data.header;
            for &condition in &func.blocks[block].instructions {
                let (a, b, comparison) = match func.inst(condition).kind {
                    InstKind::Lt(a, b) => (a, b, Comparison::Lt),
                    InstKind::Gt(a, b) => (a, b, Comparison::Gt),
                    InstKind::Eq(a, b) => (a, b, Comparison::Eq),
                    _ => continue,
                };
                let (subject, bound, subject_first) = if subjects.contains_key(&a) && invariant(b) {
                    (a, b, true)
                } else if subjects.contains_key(&b) && invariant(a) {
                    (b, a, false)
                } else {
                    continue;
                };
                if in_header && !matches!(comparison, Comparison::Eq) || object_length(bound) {
                    tests.push(ExitTest { condition, subject, bound, subject_first, comparison });
                }
            }
        }
        tests
    }

    /// The counter and every `counter + c` in the region, each with its
    /// offset, whether an update of the loop or a derived index after it.
    fn derived_values(
        func: &Function,
        region: &DenseBitSet<BlockId>,
        counter: ValueId,
    ) -> FxHashMap<ValueId, i128> {
        let mut derived = FxHashMap::default();
        derived.insert(counter, 0);
        for block in region.iter() {
            for &inst_id in &func.blocks[block].instructions {
                let Some(result) = func.inst_result_value(inst_id) else { continue };
                if let Some(Step::Constant(offset)) =
                    Self::step_of(func, &func.inst(inst_id).kind, counter)
                {
                    derived.insert(result, offset);
                }
            }
        }
        derived
    }

    /// Whether the counter is read only by its exit tests, the phis and updates
    /// that define it, and the address arithmetic in `addresses` that the
    /// pointers replace.
    fn counter_only_feeds(
        func: &Function,
        region: &DenseBitSet<BlockId>,
        counter: &Counter,
        tests: &[ExitTest],
        definers: &FxHashSet<InstId>,
        addresses: &FxHashSet<ValueId>,
    ) -> bool {
        // The updates are read like the counter: an update kept alive by
        // another read keeps the whole counter.
        let mut pending = vec![counter.value];
        pending.extend(counter.update_values(func));
        let mut visited = FxHashSet::default();
        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }
            for (block_id, block) in func.blocks.iter_enumerated() {
                let in_region = region.contains(block_id);
                if block.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                    return false;
                }
                for &inst_id in &block.instructions {
                    let inst = func.inst(inst_id);
                    if !inst.kind.operands().contains(&value) {
                        continue;
                    }
                    if tests.iter().any(|test| test.condition == inst_id)
                        || definers.contains(&inst_id)
                    {
                        continue;
                    }
                    let Some(result) = func.inst_result_value(inst_id) else { return false };
                    if !in_region || matches!(inst.kind, InstKind::Phi(_)) {
                        return false;
                    }
                    if addresses.contains(&result) {
                        continue;
                    }
                    if !Self::is_address_builder(&inst.kind) {
                        return false;
                    }
                    pending.push(result);
                }
            }
        }
        true
    }

    /// Removes the counter's phis and updates once nothing else reads any of
    /// them: the plain DCE that runs later keeps a cycle that only feeds itself.
    fn remove_dead_counter(
        &self,
        func: &mut Function,
        region: &DenseBitSet<BlockId>,
        counter: &Counter,
    ) {
        let definers = counter.definers(func);
        let defined = definers
            .iter()
            .filter_map(|&inst_id| func.inst_result_value(inst_id))
            .collect::<Vec<_>>();
        let reads_defined =
            |operands: &[ValueId]| operands.iter().any(|value| defined.contains(value));
        let read_elsewhere = func.blocks.iter().any(|block| {
            block.terminator.as_ref().is_some_and(|term| reads_defined(&term.operands()))
                || block.instructions.iter().any(|&inst_id| {
                    !definers.contains(&inst_id)
                        && reads_defined(&func.inst(inst_id).kind.operands())
                })
        });
        if read_elsewhere {
            return;
        }
        for block in region.iter() {
            func.blocks[block].instructions.retain(|inst_id| !definers.contains(inst_id));
        }
    }

    /// Whether `value` addresses memory, so scaling a bounded index onto it
    /// cannot wrap: a heap address, or an offset from a memory reference, an
    /// argument or a call result the lowered MIR no longer classifies by region.
    fn is_heap_address(&self, func: &Function, value: ValueId) -> bool {
        self.alias.memory_address(func, value).is_some_and(|address| {
            address.region == MemoryRegion::Heap
                || matches!(address.base, MemoryBase::Value(base)
                    if func.value_ty(base).is_some_and(MirType::is_memory_reference))
        })
    }

    /// Appends to `block` the pointer's value at `index`:
    /// `base + sum(invariant * scale) + index * scale + constant`.
    fn pointer_at(
        &self,
        func: &mut Function,
        block: BlockId,
        key: &AddressKey,
        index: ValueId,
    ) -> Option<ValueId> {
        let mut value = key.base;
        for term in &key.invariants {
            let scaled = self.scale_value(func, block, term.value, term.scale)?;
            value = Some(self.add_values(func, block, value, scaled));
        }
        let scaled = self.scale_value(func, block, index, key.scale)?;
        let value = self.add_values(func, block, value, scaled);
        self.add_signed_offset(func, block, value, key.constant)
    }

    /// Inserts the pointer's value at `index` at `at` in `block`, the way
    /// [`Self::pointer_at`] appends it.
    fn pointer_at_position(
        &self,
        func: &mut Function,
        block: BlockId,
        mut at: usize,
        key: &AddressKey,
        index: ValueId,
    ) -> Option<ValueId> {
        let mut value = key.base;
        for term in &key.invariants {
            let (scaled, next) = self.insert_scaled(func, block, at, term.value, term.scale)?;
            at = next;
            value = Some(match value {
                Some(acc) => {
                    let sum = self.insert_inst_value(func, block, at, InstKind::Add(acc, scaled));
                    at += 1;
                    sum
                }
                None => scaled,
            });
        }
        let (scaled, next) = self.insert_scaled(func, block, at, index, key.scale)?;
        at = next;
        let value = match value {
            Some(acc) => {
                let sum = self.insert_inst_value(func, block, at, InstKind::Add(acc, scaled));
                at += 1;
                sum
            }
            None => scaled,
        };
        self.insert_signed_offset(func, block, at, value, key.constant)
    }

    /// Appends `acc + value` to `block`, or starts the sum with `value`.
    fn add_values(
        &self,
        func: &mut Function,
        block: BlockId,
        acc: Option<ValueId>,
        value: ValueId,
    ) -> ValueId {
        match acc {
            Some(acc) => {
                self.append_inst_value(func, block, InstKind::Add(acc, value), Some(MirType::I256))
            }
            None => value,
        }
    }

    /// Operations a family's addresses cost per iteration today; addresses
    /// computed after the loop run once and count for nothing.
    fn family_cost_before(
        members: &[(AddressKey, Vec<ValueId>)],
        offset_shared: &FxHashSet<ValueId>,
        loop_addresses: &FxHashSet<ValueId>,
    ) -> usize {
        members
            .iter()
            .flat_map(|(key, values)| {
                values
                    .iter()
                    .filter(|value| loop_addresses.contains(value))
                    .map(move |value| key.use_cost(offset_shared.contains(value)))
            })
            .sum::<usize>()
    }

    /// Operations a family's pointer costs per iteration: one duplication per
    /// primary use, an add per sibling use, the latch update, and the carried word.
    fn family_cost_after(
        members: &[(AddressKey, Vec<ValueId>)],
        carried: usize,
        loop_addresses: &FxHashSet<ValueId>,
        counter: &Counter,
    ) -> usize {
        let carry = 2 + carried.saturating_sub(4);
        let key = &members[0].0;
        let byte_pointer = if key.scale.abs() == 1 && key.constant == 0 && key.invariants.is_empty()
        {
            2
        } else {
            0
        };
        // A value step is scaled before it is added, on every path that takes it.
        let scaling =
            counter.leaves.iter().filter(|(_, step)| matches!(step, Step::Value { .. })).count()
                * 2;
        members
            .iter()
            .enumerate()
            .map(|(index, (_, values))| {
                let uses = values.iter().filter(|value| loop_addresses.contains(value)).count();
                uses * if index == 0 { 1 } else { 3 }
            })
            .sum::<usize>()
            + 2
            + carry
            + byte_pointer
            + scaling
    }

    /// Whether carrying one pointer for a family of addresses saves more per
    /// iteration than it costs. Every use of an expression duplicates the
    /// index, scales it, duplicates and adds the base and every scaled
    /// invariant, and adds the offset; the pointer costs one duplication per
    /// use of the primary offset, an add per use of a sibling offset, a latch
    /// update of two operations, and one loop-carried word beside the counter
    /// its exit test keeps alive. That word costs two operations, so a
    /// single scaled use only breaks even and is left alone (the `copy` loop
    /// lost 1.2% carrying two such pointers, and charging one in loops with
    /// three carried words still cost every sorting kernel a few tenths of a
    /// percent), plus one for every word past four the loop already carries,
    /// since each deepens the accesses to all the others. A byte pointer, one
    /// whose index is unscaled and added to a bare base, is charged two more:
    /// its address is one add away from words the loop holds anyway, and the
    /// `replace` search loop lost 1.3% carrying one. With an offset or an
    /// invariant in the address the base is not that word, so the charge
    /// stays with the expression it prices.
    fn reduction_pays_off(
        members: &[(AddressKey, Vec<ValueId>)],
        offset_shared: &FxHashSet<ValueId>,
        carried: usize,
        loop_addresses: &FxHashSet<ValueId>,
        counter: &Counter,
    ) -> bool {
        // A pointer beside a counter that stays alive doubles every merge of
        // the counter: each mirror phi is one more word the arms must line up
        // (`uniquifySorted` lost 3.7% carrying one beside its write index).
        let merges = counter.phis.len() * Self::MERGE_COST;
        Self::family_cost_before(members, offset_shared, loop_addresses)
            > Self::family_cost_after(members, carried, loop_addresses, counter) + merges
    }

    /// Operations one mirror phi costs per iteration beside a live counter.
    const MERGE_COST: usize = 3;

    /// Whether `value` is computed from `target` through in-loop operands, at
    /// most four instructions deep. Phis are not traversed: their incoming
    /// values are edge uses, and the header phi's backedge would otherwise
    /// reach the update from every address.
    fn depends_on(
        func: &Function,
        loop_data: &Loop,
        value: ValueId,
        target: ValueId,
        depth: usize,
    ) -> bool {
        if value == target {
            return true;
        }
        if depth >= 4 {
            return false;
        }
        let Value::Inst(inst_id) = func.value(value) else { return false };
        let kind = &func.inst(*inst_id).kind;
        !matches!(kind, InstKind::Phi(_))
            && loop_data
                .blocks
                .iter()
                .any(|block| func.blocks[block].instructions.contains(inst_id))
            && kind
                .operands()
                .iter()
                .any(|&operand| Self::depends_on(func, loop_data, operand, target, depth + 1))
    }

    /// The words the backend carries through the loop: the header's phis and
    /// the instruction results defined outside that the loop reads.
    fn carried_words(func: &Function, loop_data: &Loop) -> usize {
        let header = &func.blocks[loop_data.header];
        let mut count = header
            .instructions
            .iter()
            .filter(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .count();
        let mut defined_inside = DenseBitSet::new_empty(func.num_insts());
        for block in loop_data.blocks.iter() {
            for &inst in &func.blocks[block].instructions {
                defined_inside.insert(inst);
            }
        }
        let mut seen = DenseBitSet::new_empty(func.num_values());
        for block in loop_data.blocks.iter() {
            let block = &func.blocks[block];
            for operand in block
                .instructions
                .iter()
                .filter(|&&inst_id| !matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .flat_map(|&inst_id| func.inst(inst_id).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                let Value::Inst(inst_id) = func.value(operand) else { continue };
                if !defined_inside.contains(*inst_id) && seen.insert(operand) {
                    count += 1;
                }
            }
        }
        count
    }

    fn address_key(
        &self,
        scev: &ScalarEvolution,
        value: ValueId,
        iv_value: ValueId,
    ) -> Option<AddressKey> {
        let expr = scev.get(value)?;
        if expr.base.is_none() && expr.invariants.is_empty() {
            return None;
        }
        let [term] = expr.terms.as_slice() else { return None };
        if term.value != iv_value || term.scale == 0 {
            return None;
        }
        let mut invariants = expr.invariants.to_vec();
        invariants.sort_unstable_by_key(|term| term.value.index());
        Some(AddressKey {
            base: expr.base,
            invariants,
            iv: iv_value,
            scale: term.scale,
            constant: expr.constant,
        })
    }

    fn materialize_pointer_phi(
        &mut self,
        func: &mut Function,
        loop_region: LoopRegion<'_>,
        counter: &Counter,
        key: &AddressKey,
        subjects: &FxHashMap<ValueId, i128>,
    ) -> Option<(ValueId, FxHashMap<ValueId, ValueId>)> {
        let LoopRegion { loop_data, blocks: region, preheader, latches } = loop_region;
        for &(_, step) in &counter.leaves {
            if let Step::Constant(constant) = step {
                constant.checked_mul(key.scale)?;
            }
        }

        // preheader: start = base + sum(invariant * scale) + init * scale + constant
        // A constant start folds into the offset; a loop-invariant start such as an
        // enclosing counter is scaled in the preheader like an invariant term.
        let initial = if let Some(init) = self.value_i128(func, counter.init) {
            let mut value = key.base;
            for term in &key.invariants {
                let scaled = self.scale_value(func, preheader, term.value, term.scale)?;
                value = Some(self.add_values(func, preheader, value, scaled));
            }
            let offset = key.constant.checked_add(init.checked_mul(key.scale)?)?;
            self.add_signed_offset(func, preheader, value?, offset)?
        } else {
            self.pointer_at(func, preheader, key, counter.init)?
        };
        let (phi_inst, phi_value) = func.alloc_value_inst(
            Instruction::new(InstKind::Phi(vec![(preheader, initial)]), Some(MirType::I256))
                .with_debug_info_dropped(),
        );
        self.insert_header_phi(func, loop_data.header, phi_inst);

        // The pointer follows the counter's own definition: a phi beside every
        // merge phi and an add beside every update, so it holds the counter's
        // address wherever the counter is in scope.
        //   ptr = phi [preheader: start], [latch: mirror(latch value)]
        //   mirror(counter) = ptr
        //   mirror(phi [b: v]...) = phi [b: mirror(v)]...
        //   mirror(counter + step) = mirror-site: add ptr, step * scale
        // The merge phis may form a cycle through an inner loop's header, so
        // every mirror phi exists before any is filled in.
        let mut mirrors = FxHashMap::default();
        mirrors.insert(counter.value, phi_value);
        let mut mirror_phis = Vec::new();
        for &merge in &counter.phis {
            let merged = func.inst_result_value(merge)?;
            let block = loop_data
                .blocks
                .iter()
                .find(|&block| func.blocks[block].instructions.contains(&merge))?;
            let (mirror_inst, mirror) = func.alloc_value_inst(
                Instruction::new(InstKind::Phi(Vec::new()), Some(MirType::I256))
                    .with_debug_info_dropped(),
            );
            self.insert_header_phi(func, block, mirror_inst);
            mirrors.insert(merged, mirror);
            mirror_phis.push((merge, mirror_inst));
        }
        for &(leaf, step) in &counter.leaves {
            let updated = func.inst_result_value(leaf)?;
            // A counter with one plain update steps its pointer at the end of
            // the latch; the mirror of an update below a merge, or of one an
            // exit test compares, sits beside the update.
            let (block, position) = loop_data.blocks.iter().find_map(|block| {
                func.blocks[block]
                    .instructions
                    .iter()
                    .position(|&inst_id| inst_id == leaf)
                    .map(|position| (block, position))
            })?;
            let (block, at) = match latches {
                [latch] if counter.phis.is_empty() && !subjects.contains_key(&updated) => {
                    (*latch, func.blocks[*latch].instructions.len())
                }
                _ => (block, position + 1),
            };
            let mirror = match step {
                Step::Constant(constant) => {
                    let delta = constant.checked_mul(key.scale)?;
                    self.insert_signed_offset(func, block, at, phi_value, delta)?
                }
                // ptr + (zext(value) << log2 scale), or ptr - it for a
                // subtracting update. The step is narrower than a word, so it
                // is widened before it is scaled onto the pointer.
                Step::Value { value, negative } => {
                    let scale = if negative { key.scale.checked_neg()? } else { key.scale };
                    let (value, at) = self.widen(func, block, at, value);
                    let (scaled, at) = self.insert_scaled(func, block, at, value, scale)?;
                    self.insert_inst_value(func, block, at, InstKind::Add(phi_value, scaled))
                }
            };
            mirrors.insert(updated, mirror);
        }
        for (merge, mirror_inst) in mirror_phis {
            let InstKind::Phi(incoming) = &func.inst(merge).kind else { return None };
            let mirrored = incoming
                .iter()
                .map(|&(pred, merged)| Some((pred, *mirrors.get(&merged)?)))
                .collect::<Option<Vec<_>>>()?;
            func.inst_mut(mirror_inst).kind = InstKind::Phi(mirrored);
        }

        let latch_mirror = *mirrors.get(&counter.latch_value)?;
        let InstKind::Phi(incoming) = &mut func.inst_mut(phi_inst).kind else {
            return None;
        };
        incoming.extend(latches.iter().map(|&latch| (latch, latch_mirror)));
        self.stats.pointer_phis_inserted += 1;

        // A tested value derived from the counter elsewhere in the region,
        // `i + 1` after the loop, gets its mirror beside its definition too.
        //   mirror(counter + c) = add ptr, c * scale
        for (&subject, &offset) in subjects {
            if mirrors.contains_key(&subject) {
                continue;
            }
            let Value::Inst(definer) = *func.value(subject) else { continue };
            let Some((block, position)) = region.iter().find_map(|block| {
                func.blocks[block]
                    .instructions
                    .iter()
                    .position(|&inst_id| inst_id == definer)
                    .map(|position| (block, position))
            }) else {
                continue;
            };
            let delta = offset.checked_mul(key.scale)?;
            let mirror = self.insert_signed_offset(func, block, position + 1, phi_value, delta)?;
            mirrors.insert(subject, mirror);
        }
        Some((phi_value, mirrors))
    }

    /// Inserts `value + offset` at `at` in `block`, as an add or a subtraction
    /// by the magnitude.
    fn insert_signed_offset(
        &self,
        func: &mut Function,
        block: BlockId,
        at: usize,
        value: ValueId,
        offset: i128,
    ) -> Option<ValueId> {
        if offset == 0 {
            return Some(value);
        }
        let magnitude = self.offset_value(func, offset.checked_abs()?)?;
        let kind = if offset > 0 {
            InstKind::Add(value, magnitude)
        } else {
            InstKind::Sub(value, magnitude)
        };
        Some(self.insert_inst_value(func, block, at, kind))
    }

    /// Widens a narrow step to a word at `at` in `block`, returning the word
    /// and the position after it. A value that is already a word is returned
    /// as it is.
    fn widen(
        &self,
        func: &mut Function,
        block: BlockId,
        at: usize,
        value: ValueId,
    ) -> (ValueId, usize) {
        if func.value_ty(value) == Some(MirType::I256) {
            return (value, at);
        }
        // word = zext value to i256
        (self.insert_inst_value(func, block, at, InstKind::Zext(value)), at + 1)
    }

    /// Inserts `value * scale` at `at` in `block` the way [`Self::scale_value`]
    /// appends it, returning the scaled value and the position after it.
    fn insert_scaled(
        &self,
        func: &mut Function,
        block: BlockId,
        mut at: usize,
        value: ValueId,
        scale: i128,
    ) -> Option<(ValueId, usize)> {
        let magnitude = scale.checked_abs()?.unsigned_abs();
        let scaled = if magnitude == 1 {
            value
        } else if magnitude.is_power_of_two() {
            let shift = self.offset_value(func, i128::from(magnitude.trailing_zeros()))?;
            let scaled = self.insert_inst_value(func, block, at, InstKind::Shl(shift, value));
            at += 1;
            scaled
        } else {
            let factor = self.offset_value(func, i128::try_from(magnitude).ok()?)?;
            let scaled = self.insert_inst_value(func, block, at, InstKind::Mul(value, factor));
            at += 1;
            scaled
        };
        if scale > 0 {
            return Some((scaled, at));
        }
        let zero = self.offset_value(func, 0)?;
        let negated = self.insert_inst_value(func, block, at, InstKind::Sub(zero, scaled));
        Some((negated, at + 1))
    }

    /// Inserts a word instruction at `at` in `block`.
    fn insert_inst_value(
        &self,
        func: &mut Function,
        block: BlockId,
        at: usize,
        kind: InstKind,
    ) -> ValueId {
        let (inst, value) = func.alloc_value_inst(
            Instruction::new(kind, Some(MirType::I256)).with_debug_info_dropped(),
        );
        func.blocks[block].instructions.insert(at, inst);
        value
    }

    /// Appends `value + offset` to `block`, as an add or a subtraction by the magnitude.
    fn add_signed_offset(
        &self,
        func: &mut Function,
        block: BlockId,
        value: ValueId,
        offset: i128,
    ) -> Option<ValueId> {
        if offset == 0 {
            return Some(value);
        }
        let magnitude = self.offset_value(func, offset.checked_abs()?)?;
        let kind = if offset > 0 {
            InstKind::Add(value, magnitude)
        } else {
            InstKind::Sub(value, magnitude)
        };
        Some(self.append_inst_value(func, block, kind, Some(MirType::I256)))
    }

    /// Appends `value * scale` to `block`: a shift for a power of two, a
    /// multiplication otherwise, negated by subtraction from zero.
    fn scale_value(
        &self,
        func: &mut Function,
        block: BlockId,
        value: ValueId,
        scale: i128,
    ) -> Option<ValueId> {
        let magnitude = scale.checked_abs()?.unsigned_abs();
        let scaled = if magnitude == 1 {
            value
        } else if magnitude.is_power_of_two() {
            let shift = self.offset_value(func, i128::from(magnitude.trailing_zeros()))?;
            self.append_inst_value(func, block, InstKind::Shl(shift, value), Some(MirType::I256))
        } else {
            let factor = self.offset_value(func, i128::try_from(magnitude).ok()?)?;
            self.append_inst_value(func, block, InstKind::Mul(value, factor), Some(MirType::I256))
        };
        if scale > 0 {
            return Some(scaled);
        }
        let zero = self.offset_value(func, 0)?;
        Some(self.append_inst_value(func, block, InstKind::Sub(zero, scaled), Some(MirType::I256)))
    }

    fn offset_value(&self, func: &mut Function, offset: i128) -> Option<ValueId> {
        if offset < 0 {
            return None;
        }
        Some(func.alloc_value(Value::Immediate(Immediate::I256(U256::from(offset as u128)))))
    }

    fn append_inst_value(
        &self,
        func: &mut Function,
        block: BlockId,
        kind: InstKind,
        ty: Option<MirType>,
    ) -> ValueId {
        let (inst, value) =
            func.alloc_value_inst(Instruction::new(kind, ty).with_debug_info_dropped());
        func.blocks[block].instructions.push(inst);
        value
    }

    fn insert_header_phi(&self, func: &mut Function, header: BlockId, phi_inst: InstId) {
        let insert_pos = func.blocks[header]
            .instructions
            .iter()
            .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .count();
        func.blocks[header].instructions.insert(insert_pos, phi_inst);
    }

    fn is_reducible_result(&self, func: &Function, inst_id: InstId) -> bool {
        if func.inst(inst_id).result_ty != Some(MirType::I256) {
            return false;
        }
        matches!(
            func.inst(inst_id).kind,
            InstKind::Add(_, _) | InstKind::Sub(_, _) | InstKind::Mul(_, _) | InstKind::Shl(_, _)
        )
    }

    fn value_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        match func.value(value) {
            Value::Immediate(imm) => u256_to_i128(imm.as_u256()?),
            _ => None,
        }
    }

    fn has_non_address_use(&self, func: &Function, loop_data: &Loop, value: ValueId) -> bool {
        for block in &loop_data.blocks {
            for &inst_id in &func.blocks[block].instructions {
                let kind = &func.inst(inst_id).kind;
                if kind.operands().contains(&value) && !Self::is_address_builder(kind) {
                    return true;
                }
            }
            if func.blocks[block]
                .terminator
                .as_ref()
                .is_some_and(|term| term.operands().contains(&value))
            {
                return true;
            }
        }
        false
    }

    fn is_address_builder(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Add(_, _) | InstKind::Sub(_, _) | InstKind::Mul(_, _) | InstKind::Shl(_, _)
        )
    }

    fn replace_uses(
        &self,
        func: &mut Function,
        region: &DenseBitSet<BlockId>,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> usize {
        let mut replaced = 0;
        for block in region.iter() {
            let instruction_count = func.blocks[block].instructions.len();
            for index in 0..instruction_count {
                let inst_id = func.blocks[block].instructions[index];
                replaced += mir_utils::replace_inst_uses(func.inst_mut(inst_id), replacements);
            }
            if let Some(term) = &mut func.blocks[block].terminator {
                replaced += mir_utils::replace_terminator_uses(term, replacements);
            }
        }
        replaced
    }
}

fn inst_kind(func: &Function, value: ValueId) -> Option<&InstKind> {
    match func.value(value) {
        Value::Inst(inst_id) => Some(&func.inst(*inst_id).kind),
        _ => None,
    }
}

fn u256_to_i128(value: U256) -> Option<i128> {
    if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
}

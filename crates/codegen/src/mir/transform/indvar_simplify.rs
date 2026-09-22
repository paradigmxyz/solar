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
//!
//! Safety contract:
//! - require canonical loops with a preheader and a single latch
//! - rewrite only affine address expressions derived from the recognized induction variable
//! - preserve the original address value when it is still used outside the loop
//! - recognize checked unsigned word updates while retaining their failure checks.
//! - add only one scaled address counter when the original update must stay live.

use crate::mir::{
    ArithmeticKind, BlockId, CheckedOp, Function, Immediate, InstId, InstKind, Instruction,
    MemoryRegion, MirType, Module, Terminator, Value, ValueId,
    analysis::{
        AffineTerm, AliasAnalysis, CfgInfo, InductionVariable, Loop, LoopAnalyzer, ScalarEvolution,
    },
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

/// The header's exit test, `lt counter, bound` or `lt bound, counter`.
#[derive(Clone, Copy)]
struct ExitTest {
    condition: InstId,
    bound: ValueId,
    counter_first: bool,
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

        for loop_data in loops {
            self.run_loop(func, &loop_data);
        }

        &self.stats
    }

    fn run_loop(&mut self, func: &mut Function, loop_data: &Loop) {
        let Some(preheader) = loop_data.preheader else { return };
        let [latch] = loop_data.back_edges.as_slice() else { return };
        let latch = *latch;
        // A loop may step more than one counter, each walking its own addresses:
        // a codec reading an input and writing an output steps both, and taking
        // only the single-counter case left every such loop rebuilding both
        // address families from scratch every iteration. Reduce them one at a
        // time. A reduction only retypes instructions, adds one header phi with
        // its latch update, and deletes arithmetic it just made dead, so the
        // blocks, preheader and back edge analyzed here stay valid for the next.
        for iv in loop_data.induction_vars.clone() {
            self.reduce_induction_variable(func, loop_data, preheader, latch, iv);
        }
    }

    /// Replaces one counter's loop address expressions with carried pointers.
    fn reduce_induction_variable(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        preheader: BlockId,
        latch: BlockId,
        iv: InductionVariable,
    ) {
        // Reducing an earlier counter can delete this one's update as dead
        // address arithmetic, leaving the recorded instruction outside the loop.
        if !loop_data
            .blocks
            .iter()
            .any(|block| func.blocks[block].instructions.contains(&iv.update_inst))
        {
            return;
        }
        let Some(step) = self.additive_step(func, iv.value, Some(iv.update_inst)) else {
            return;
        };
        let must_keep_update = func.inst(iv.update_inst).kind.effects().must_execute(false);

        let scev = ScalarEvolution::analyze(func, loop_data);
        let carried = Self::carried_words(func, loop_data);
        let update_value = func.inst_result_value(iv.update_inst);
        let mut candidates: FxHashMap<AddressKey, Vec<ValueId>> = FxHashMap::default();
        let mut offset_shared = FxHashSet::default();

        for block in &loop_data.blocks {
            for &inst_id in &func.blocks[block].instructions {
                let Some(value) = func.inst_result_value(inst_id) else { continue };
                if !self.is_reducible_result(func, inst_id) {
                    continue;
                }
                let Some(key) = self.address_key(&scev, value, iv.value) else {
                    continue;
                };
                // A checked update must remain live, so a second plain-add counter saves no work.
                if must_keep_update && key.scale == 1 {
                    continue;
                }
                let Some(delta) = key.scale.checked_mul(step) else { continue };
                if delta == 0 || !self.has_non_address_loop_use(func, loop_data, value) {
                    continue;
                }
                if update_value
                    .is_some_and(|update| Self::depends_on(func, loop_data, value, update, 0))
                {
                    offset_shared.insert(value);
                }
                candidates.entry(key).or_default().push(value);
            }
        }

        // Checked updates cannot die when their address uses disappear. Limit the additional
        // loop-carried state instead of creating a counter for every field address.
        if candidates.is_empty() || (must_keep_update && candidates.len() != 1) {
            return;
        }

        let addresses = candidates.values().flatten().copied().collect::<FxHashSet<_>>();
        let mut families: FxHashMap<AddressKey, Vec<(AddressKey, Vec<ValueId>)>> =
            FxHashMap::default();
        for (key, values) in candidates {
            families.entry(key.family()).or_default().push((key, values));
        }
        let mut families = families.into_values().collect::<Vec<_>>();
        for members in &mut families {
            // The most used offset carries the pointer; ties go to the smallest offset.
            members
                .sort_by_key(|(key, values)| (std::cmp::Reverse(values.len()), key.constant.abs()));
        }
        families.sort_by_key(|members| members[0].0.family().constant);

        // With the counter free once every address is a pointer, an ascending pointer
        // that cannot wrap takes over the exit test and the counter dies; that credit
        // is weighed across all families at once.
        let exit_test = self.counter_exit_test(func, loop_data, iv.value);
        let counter_free = !must_keep_update
            && exit_test.is_some_and(|test| {
                Self::counter_only_feeds(
                    func,
                    loop_data,
                    iv.value,
                    test.condition,
                    Some(iv.update_inst),
                    &addresses,
                )
            });
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
                .map(|members| Self::family_cost_before(members, &offset_shared))
                .sum::<usize>();
            let after = families
                .iter()
                .map(|members| Self::family_cost_after(members, carried))
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
            let pays = reduce_all || Self::reduction_pays_off(members, &offset_shared, carried);
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
            let Some(pointer) =
                self.materialize_pointer_phi(func, loop_data, preheader, latch, primary)
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
                test_pointer = Some((pointer, primary.clone()));
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
        let mut test_rewritten = false;
        if let (Some(test), Some((pointer, key))) = (exit_test, test_pointer.as_ref())
            && replacements.len() + siblings.len() == addresses.len()
            && let Some(end) = self.pointer_at(func, preheader, key, test.bound)
        {
            func.inst_mut(test.condition).kind = if test.counter_first {
                InstKind::Lt(*pointer, end)
            } else {
                InstKind::Lt(end, *pointer)
            };
            self.stats.address_uses_replaced += 1;
            test_rewritten = true;
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

        self.stats.address_uses_replaced += self.replace_loop_uses(func, loop_data, &replacements);
        // The replaced addresses and the index arithmetic only they read are dead now;
        // remove them here so the counter's remaining reads are visible below.
        self.remove_dead_address_arithmetic(func, loop_data);
        if test_rewritten {
            self.remove_dead_counter(func, loop_data, iv.value, Some(iv.update_inst));
        }
    }

    /// Removes the loop's pure address arithmetic whose results nothing reads any more,
    /// following each removal to the operands it leaves unread.
    fn remove_dead_address_arithmetic(&self, func: &mut Function, loop_data: &Loop) {
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
        let in_loop = |func: &Function, inst_id: InstId| {
            loop_data.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
        };
        let mut pending = Vec::new();
        for block in loop_data.blocks.iter() {
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
            for block in loop_data.blocks.iter() {
                func.blocks[block].instructions.retain(|&other| other != inst_id);
            }
            for operand in operands {
                let Some(count) = uses.get_mut(&operand) else { continue };
                *count = count.saturating_sub(1);
                if *count == 0
                    && let Value::Inst(definer) = *func.value(operand)
                    && in_loop(func, definer)
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

    /// The header's exit test when it compares the counter with an invariant.
    fn counter_exit_test(
        &self,
        func: &Function,
        loop_data: &Loop,
        iv: ValueId,
    ) -> Option<ExitTest> {
        let Some(Terminator::Branch { condition, .. }) = &func.blocks[loop_data.header].terminator
        else {
            return None;
        };
        let Value::Inst(condition) = *func.value(*condition) else { return None };
        let InstKind::Lt(a, b) = func.inst(condition).kind else { return None };
        let invariant = |value: ValueId| match func.value(value) {
            Value::Immediate(_) | Value::Arg(_) => true,
            Value::Inst(inst_id) => !loop_data
                .blocks
                .iter()
                .any(|block| func.blocks[block].instructions.contains(inst_id)),
            Value::Undef(_) | Value::Error(_) => false,
        };
        if a == iv && invariant(b) {
            Some(ExitTest { condition, bound: b, counter_first: true })
        } else if b == iv && invariant(a) {
            Some(ExitTest { condition, bound: a, counter_first: false })
        } else {
            None
        }
    }

    /// Whether the counter is read only by its exit test, its update, and the
    /// address arithmetic in `addresses` that the pointers replace.
    fn counter_only_feeds(
        func: &Function,
        loop_data: &Loop,
        iv: ValueId,
        condition: InstId,
        update: Option<InstId>,
        addresses: &FxHashSet<ValueId>,
    ) -> bool {
        let mut pending = vec![iv];
        let mut visited = FxHashSet::default();
        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }
            for (block_id, block) in func.blocks.iter_enumerated() {
                let in_loop = loop_data.blocks.contains(block_id);
                if block.terminator.as_ref().is_some_and(|term| term.operands().contains(&value)) {
                    return false;
                }
                for &inst_id in &block.instructions {
                    let inst = func.inst(inst_id);
                    if !inst.kind.operands().contains(&value) {
                        continue;
                    }
                    if inst_id == condition || Some(inst_id) == update {
                        continue;
                    }
                    let Some(result) = func.inst_result_value(inst_id) else { return false };
                    if !in_loop || matches!(inst.kind, InstKind::Phi(_)) {
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

    /// Removes the counter phi and its update once nothing else reads either: the plain
    /// DCE that runs later keeps a cycle that only feeds itself.
    fn remove_dead_counter(
        &self,
        func: &mut Function,
        loop_data: &Loop,
        counter: ValueId,
        update: Option<InstId>,
    ) {
        let Some(update) = update else { return };
        let Value::Inst(phi) = *func.value(counter) else { return };
        let Some(next) = func.inst_result_value(update) else { return };
        let read_elsewhere = |value: ValueId, except: InstId| {
            func.blocks.iter().any(|block| {
                block.terminator.as_ref().is_some_and(|term| term.operands().contains(&value))
                    || block.instructions.iter().any(|&inst_id| {
                        inst_id != except && func.inst(inst_id).kind.operands().contains(&value)
                    })
            })
        };
        if read_elsewhere(counter, update) || read_elsewhere(next, phi) {
            return;
        }
        for block in loop_data.blocks.iter() {
            func.blocks[block].instructions.retain(|&inst_id| inst_id != phi && inst_id != update);
        }
    }

    /// Whether `value` has proven heap provenance, so scaling a bounded index cannot wrap.
    fn is_heap_address(&self, func: &Function, value: ValueId) -> bool {
        self.alias
            .memory_address(func, value)
            .is_some_and(|address| address.region == MemoryRegion::Heap)
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

    /// Operations a family's addresses cost per iteration today.
    fn family_cost_before(
        members: &[(AddressKey, Vec<ValueId>)],
        offset_shared: &FxHashSet<ValueId>,
    ) -> usize {
        members
            .iter()
            .flat_map(|(key, values)| {
                values.iter().map(move |value| key.use_cost(offset_shared.contains(value)))
            })
            .sum::<usize>()
    }

    /// Operations a family's pointer costs per iteration: one duplication per
    /// primary use, an add per sibling use, the latch update, and the carried word.
    fn family_cost_after(members: &[(AddressKey, Vec<ValueId>)], carried: usize) -> usize {
        let carry = 2 + carried.saturating_sub(4);
        let byte_pointer = if members[0].0.scale.abs() == 1 { 2 } else { 0 };
        members
            .iter()
            .enumerate()
            .map(|(index, (_, values))| values.len() * if index == 0 { 1 } else { 3 })
            .sum::<usize>()
            + 2
            + carry
            + byte_pointer
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
    /// whose index is unscaled, is charged two more: its address is one add
    /// away from words the loop holds anyway, and the `replace` search loop
    /// lost 1.3% carrying one.
    fn reduction_pays_off(
        members: &[(AddressKey, Vec<ValueId>)],
        offset_shared: &FxHashSet<ValueId>,
        carried: usize,
    ) -> bool {
        Self::family_cost_before(members, offset_shared) > Self::family_cost_after(members, carried)
    }

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

    fn additive_step(
        &self,
        func: &Function,
        iv_value: ValueId,
        update_inst: Option<InstId>,
    ) -> Option<i128> {
        let update_inst = update_inst?;
        match func.inst(update_inst).kind {
            InstKind::Add(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs: a,
                rhs: b,
            } if a == iv_value => self.value_i128(func, b),
            InstKind::Add(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs: a,
                rhs: b,
            } if b == iv_value => self.value_i128(func, a),
            InstKind::Sub(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Sub,
                arithmetic: ArithmeticKind::Unsigned(256),
                lhs: a,
                rhs: b,
            } if a == iv_value => self.value_i128(func, b)?.checked_neg(),
            _ => None,
        }
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
        loop_data: &Loop,
        preheader: BlockId,
        latch: BlockId,
        key: &AddressKey,
    ) -> Option<ValueId> {
        let iv = loop_data.induction_vars.iter().find(|iv| iv.value == key.iv)?;
        let delta =
            self.additive_step(func, key.iv, Some(iv.update_inst))?.checked_mul(key.scale)?;
        if delta == 0 {
            return None;
        }

        // preheader: start = base + sum(invariant * scale) + init * scale + constant
        // A constant start folds into the offset; a loop-invariant start such as an
        // enclosing counter is scaled in the preheader like an invariant term.
        let initial = if let Some(init) = self.value_i128(func, iv.init) {
            let mut value = key.base;
            for term in &key.invariants {
                let scaled = self.scale_value(func, preheader, term.value, term.scale)?;
                value = Some(self.add_values(func, preheader, value, scaled));
            }
            let offset = key.constant.checked_add(init.checked_mul(key.scale)?)?;
            self.add_signed_offset(func, preheader, value?, offset)?
        } else {
            self.pointer_at(func, preheader, key, iv.init)?
        };
        let (phi_inst, phi_value) = func.alloc_value_inst(
            Instruction::new(InstKind::Phi(vec![(preheader, initial)]), Some(MirType::I256))
                .with_debug_info_dropped(),
        );
        self.insert_header_phi(func, loop_data.header, phi_inst);

        // latch: next = ptr + delta, or ptr - |delta| for a pointer walking down
        let next = self.add_signed_offset(func, latch, phi_value, delta)?;
        let InstKind::Phi(incoming) = &mut func.inst_mut(phi_inst).kind else {
            return None;
        };
        incoming.push((latch, next));
        self.stats.pointer_phis_inserted += 1;
        Some(phi_value)
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
        let value = match func.value(value) {
            Value::Immediate(imm) => imm.as_u256()?,
            _ => return None,
        };
        if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
    }

    fn has_non_address_loop_use(&self, func: &Function, loop_data: &Loop, value: ValueId) -> bool {
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

    fn replace_loop_uses(
        &self,
        func: &mut Function,
        loop_data: &Loop,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> usize {
        let mut replaced = 0;
        for block in &loop_data.blocks {
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

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
//! A pointer with any other base, such as a calldata array element, may wrap,
//! so it takes over the test only from a counter that starts at a literal below
//! `2^64`, steps by one, and keeps the loop running while `i < bound` holds, and
//! tests `ptr != end` instead. Two pointers agree
//! exactly when their counters agree modulo `2^(256 - k)`, where `2^k` is the
//! largest power of two dividing the scale, so the pointer first reaches `end`
//! when the counter reaches a bound between the start and that modulus. A bound
//! below the start is raised to it, entering no iteration. An odd scale, or a
//! check on every path into the loop such as an ABI decoder's comparison of a
//! length with the remaining calldata, keeps the bound below the modulus; any
//! other bound is first lowered to `2^65`, which the counter reaches only after
//! more iterations than a loop can run.
//!
//! The counter may still be read after the loop, as a search returns where it
//! stopped. Once the pointer takes over the exit test, each such read is rebuilt
//! from the pointer as `init + (ptr - start) / scale`, a shift for a power-of-two
//! scale, at the top of the reading block, or at the end of the block a phi reads
//! it from. Only the equality exit takes over from such a counter: it counts up
//! and its clamped `end` cannot wrap, while a heap pointer's `ptr < end` would leave
//! at once for a bound near the word size and return a counter the loop never
//! reached. A phi reading it over a critical exit edge, from a loop block into a
//! join, needs a block of its own on that edge: the pass splits those edges after
//! visiting every loop of the function and then visits those loops again. Only
//! the counter itself may be read after the loop; a value the loop derives from
//! it keeps the counter, and so does a counter narrower than a word, which the
//! rebuild would replace with a word.
//!
//! Safety contract:
//! - require canonical loops with a preheader and a single latch
//! - rewrite only affine address expressions derived from the recognized induction variable
//! - preserve the original address value when it is still used outside the loop
//! - recognize checked unsigned updates at every width and retain their failure checks.
//! - add only one scaled address counter when the original update must stay live.

use crate::{
    mir::{
        ArithmeticKind, BlockId, CheckedOp, Function, FunctionBuilder, Immediate, InstId, InstKind,
        Instruction, MemoryRegion, MirType, Module, Terminator, Value, ValueId,
        analysis::{
            AffineTerm, AliasAnalysis, CfgInfo, InductionVariable, Loop, LoopAnalyzer,
            ScalarEvolution,
        },
        pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
        utils as mir_utils,
    },
    target::Target,
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
                let insts = func.num_insts();
                let changed = IndVarSimplifier::new(Rc::clone(analyses.alias()))
                    .run(func, Rc::clone(analyses.cfg()))
                    .total()
                    != 0;
                // NOTE: A pointer phi that fails to materialize leaves the instructions it
                // already inserted in place without reporting a change.
                if !changed && func.num_insts() != insts {
                    analyses.note_unreported_edit();
                }
                changed
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
    /// Number of loop exit edges split to rebuild the counter there.
    exit_edges_split: usize,
}

impl IndVarSimplifyStats {
    /// Returns the total number of MIR changes performed.
    #[must_use]
    const fn total(&self) -> usize {
        self.pointer_phis_inserted + self.address_uses_replaced + self.exit_edges_split
    }
}

/// Performs conservative induction-variable strength reduction.
#[derive(Debug)]
struct IndVarSimplifier {
    stats: IndVarSimplifyStats,
    alias: Rc<AliasAnalysis>,
    /// Exit edges, from a loop block to a join outside it, whose phis read a counter that
    /// could otherwise die, by the loop header.
    exit_splits: Vec<(BlockId, BlockId, BlockId)>,
}

/// Reads of a counter phi after its loop, which the counter's pointer rebuilds once the
/// pointer takes over the exit test.
#[derive(Default)]
struct ExitReads {
    /// Blocks outside the loop whose instructions or terminator read the counter.
    blocks: Vec<BlockId>,
    /// Phis outside the loop with the counter incoming from a block outside the loop.
    phis: Vec<(InstId, BlockId)>,
    /// Exit edges, from a loop block to a join outside it, whose phis read the counter.
    edges: Vec<(BlockId, BlockId)>,
}

impl ExitReads {
    fn is_empty(&self) -> bool {
        self.blocks.is_empty() && self.phis.is_empty() && self.edges.is_empty()
    }
}

/// The header's exit test, `lt counter, bound` or `lt bound, counter`.
#[derive(Clone, Copy)]
struct ExitTest {
    condition: InstId,
    bound: ValueId,
    counter_first: bool,
    /// Whether the loop continues where the test holds and leaves where it fails.
    continues_on_true: bool,
}

/// How a pointer that replaces the counter tests the loop's exit.
#[derive(Clone, Copy, PartialEq, Eq)]
enum PointerExit {
    /// `ptr < end`: an ascending heap pointer cannot wrap before its value at the bound.
    Below,
    /// `ptr != end`, with `end` taken at the bound clamped where it could wrap: the counter
    /// starts at this literal and steps by one.
    Reaches(U256),
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
        Self { stats: IndVarSimplifyStats::default(), alias, exit_splits: Vec::new() }
    }

    /// Runs induction-variable simplification once over `func`.
    fn run(&mut self, func: &mut Function, cfg: Rc<CfgInfo>) -> &IndVarSimplifyStats {
        self.stats = IndVarSimplifyStats::default();

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze_with_cfg(func, Rc::clone(&cfg));
        let loops: Vec<_> = loop_info.loops.values().cloned().collect();

        for loop_data in loops {
            self.run_loop(func, &cfg, loop_data);
        }

        // A counter read after its loop through a phi on a critical exit edge can die only
        // once the edge has a block of its own to rebuild it in: split those edges, then run
        // the loops that asked for it again over the new control flow.
        if !self.exit_splits.is_empty() {
            let splits = std::mem::take(&mut self.exit_splits);
            let mut headers = FxHashSet::default();
            for &(header, from, to) in &splits {
                // from -> to  =>  from -> split -> to
                if func.blocks[from].terminator.as_ref().is_some_and(|term| term.has_successor(to))
                {
                    mir_utils::split_edge(func, from, to);
                    self.stats.exit_edges_split += 1;
                }
                headers.insert(header);
            }
            let cfg = Rc::new(CfgInfo::new(func));
            let loop_info = LoopAnalyzer::new().analyze_with_cfg(func, Rc::clone(&cfg));
            let loops: Vec<_> = loop_info
                .loops
                .values()
                .filter(|loop_data| headers.contains(&loop_data.header))
                .cloned()
                .collect();
            for loop_data in loops {
                self.run_loop(func, &cfg, loop_data);
            }
            // NOTE: A second request for the same loop is dropped: its edges were split once.
            self.exit_splits.clear();
        }

        &self.stats
    }

    fn run_loop(&mut self, func: &mut Function, cfg: &CfgInfo, mut loop_data: Loop) {
        let Some(preheader) = loop_data.preheader else { return };
        let [latch] = loop_data.back_edges.as_slice() else { return };
        let latch = *latch;
        // Reducing an earlier loop can rewrite this loop's entry values, such as with a
        // counter rebuilt after that loop, so read them again from the header phis.
        for iv in &mut loop_data.induction_vars {
            if let Some(init) = preheader_value(func, iv.value, preheader) {
                iv.init = init;
            }
        }
        // A loop may step more than one counter, each walking its own addresses:
        // a codec reading an input and writing an output steps both, and taking
        // only the single-counter case left every such loop rebuilding both
        // address families from scratch every iteration. Reduce them one at a
        // time. A reduction only retypes instructions, adds one header phi with
        // its latch update, and deletes arithmetic it just made dead, so the
        // blocks, preheader and back edge analyzed here stay valid for the next.
        for iv in loop_data.induction_vars.clone() {
            self.reduce_induction_variable(func, cfg, &loop_data, preheader, latch, iv);
        }
    }

    /// Replaces one counter's loop address expressions with carried pointers.
    fn reduce_induction_variable(
        &mut self,
        func: &mut Function,
        cfg: &CfgInfo,
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
        // The pointer rebuilds a counter read after the loop as a word, so a narrower counter
        // read there keeps its test until integer lowering widens it.
        let exit_reads = exit_test
            .filter(|_| !must_keep_update)
            .and_then(|test| {
                Self::counter_only_feeds(
                    func,
                    loop_data,
                    iv.value,
                    test.condition,
                    Some(iv.update_inst),
                    &addresses,
                )
            })
            .filter(|reads| reads.is_empty() || func.value_ty(iv.value) == Some(MirType::I256));
        let counter_free = exit_reads.is_some();
        // A counter that starts at a literal and steps by one leaves the loop exactly when an
        // ascending pointer reaches its value at the bound, clamped where that value could
        // wrap onto an earlier one, whatever the base: unlike `ptr < end`, equality needs no
        // proof that the pointer does not wrap.
        let literal_start =
            func.value_u256(iv.init).filter(|&start| step == 1 && start <= U256::from(u64::MAX));
        // A counter read after the loop is rebuilt from where its pointer stopped, so the pointer
        // must stop where the counter would: a heap pointer's unclamped `end` wraps for a bound
        // near the word size, leaving at once where the counter runs out of gas, and a descending
        // counter does not count up from its start. Only the clamped equality exit rebuilds it.
        let read_after = exit_reads.as_ref().is_some_and(|reads| !reads.is_empty());
        let test_family = if counter_free {
            families
                .iter()
                .position(|members| {
                    let key = &members[0].0;
                    key.scale > 0
                        && key.invariants.is_empty()
                        && key.base.is_some_and(|base| self.is_heap_address(func, base))
                })
                .filter(|_| !read_after)
                .map(|index| (index, PointerExit::Below))
                .or_else(|| {
                    let start = literal_start?;
                    // `ptr != end` holds until the counter first reaches the bound, which is
                    // `i < bound` only for a loop that runs while the test holds: one that runs
                    // while `i >= bound` would leave a step past its start.
                    exit_test.filter(|test| test.counter_first && test.continues_on_true)?;
                    let index = families.iter().position(|members| {
                        let key = &members[0].0;
                        key.scale > 0
                            && key.scale <= i128::from(u32::MAX)
                            && key.invariants.is_empty()
                    })?;
                    Some((index, PointerExit::Reaches(start)))
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

        // The counter's reads on critical exit edges need blocks of their own first.
        if reduce_all
            && let Some(reads) = &exit_reads
            && !reads.edges.is_empty()
        {
            self.exit_splits
                .extend(reads.edges.iter().map(|&(from, to)| (loop_data.header, from, to)));
            return;
        }

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
                test_family = test_family.is_some_and(|(test_index, _)| test_index == index),
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
            if reduce_all
                && let Some((test_index, exit)) = test_family
                && test_index == index
            {
                test_pointer = Some((pointer, primary.clone(), exit));
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

        let mut test_rewritten = false;
        if let (Some(test), Some((pointer, key, exit))) = (exit_test, test_pointer.as_ref())
            && replacements.len() + siblings.len() == addresses.len()
        {
            let bound = match *exit {
                PointerExit::Below => Some(test.bound),
                PointerExit::Reaches(start) => {
                    Some(self.clamped_bound(func, cfg, preheader, test.bound, start, key.scale))
                }
            };
            if let Some(bound) = bound
                && let Some(end) = self.pointer_at(func, preheader, key, bound)
            {
                func.inst_mut(test.condition).kind = match (*exit, test.counter_first) {
                    // exit: lt counter, bound  =>  lt ptr, end   with end = ptr's value at the
                    // bound
                    (PointerExit::Below, true) => InstKind::Lt(*pointer, end),
                    (PointerExit::Below, false) => InstKind::Lt(end, *pointer),
                    // exit: lt counter, bound  =>  ne ptr, end   with end = ptr's value at the
                    // bound clamped to [start, 2^65]
                    (PointerExit::Reaches(_), _) => InstKind::Ne(*pointer, end),
                };
                self.stats.address_uses_replaced += 1;
                test_rewritten = true;
            }
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
        if test_rewritten
            && let Some(reads) = &exit_reads
            && let Some((pointer, key, _)) = &test_pointer
        {
            self.rebuild_exit_reads(func, preheader, &iv, *pointer, key.scale, reads);
        }
        if test_rewritten {
            self.remove_dead_counter(func, loop_data, iv.value, Some(iv.update_inst));
        }
    }

    /// Removes the loop's pure address arithmetic whose results nothing reads any more,
    /// following each removal to the operands it leaves unread.
    fn remove_dead_address_arithmetic(&self, func: &mut Function, loop_data: &Loop) {
        let mut uses = FxHashMap::<ValueId, usize>::default();
        for block in &func.blocks {
            for &inst_id in &block.instructions {
                func.inst(inst_id)
                    .kind
                    .visit_operands(|operand| *uses.entry(operand).or_default() += 1);
            }
            if let Some(term) = &block.terminator {
                term.visit_operands(|operand| *uses.entry(operand).or_default() += 1);
            }
        }
        // The loop's instructions, less those removed below.
        let mut in_loop = DenseBitSet::new_empty(func.num_insts());
        let mut pending = Vec::new();
        for block in loop_data.blocks.iter() {
            for &inst_id in &func.blocks[block].instructions {
                in_loop.insert(inst_id);
                if Self::is_address_builder(&func.inst(inst_id).kind)
                    && func
                        .inst_result_value(inst_id)
                        .is_some_and(|result| uses.get(&result).copied().unwrap_or_default() == 0)
                {
                    pending.push(inst_id);
                }
            }
        }
        let mut removed = false;
        while let Some(inst_id) = pending.pop() {
            removed |= in_loop.remove(inst_id);
            func.inst(inst_id).kind.visit_operands(|operand| {
                let Some(count) = uses.get_mut(&operand) else { return };
                *count = count.saturating_sub(1);
                if *count == 0
                    && let Value::Inst(definer) = *func.value(operand)
                    && in_loop.contains(definer)
                    && Self::is_address_builder(&func.inst(definer).kind)
                {
                    pending.push(definer);
                }
            });
        }
        if removed {
            for block in loop_data.blocks.iter() {
                func.blocks[block].instructions.retain(|&inst_id| in_loop.contains(inst_id));
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
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            &func.blocks[loop_data.header].terminator
        else {
            return None;
        };
        let continues_on_true =
            loop_data.blocks.contains(*then_block) && !loop_data.blocks.contains(*else_block);
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
            Some(ExitTest { condition, bound: b, counter_first: true, continues_on_true })
        } else if b == iv && invariant(a) {
            Some(ExitTest { condition, bound: a, counter_first: false, continues_on_true })
        } else {
            None
        }
    }

    /// The counter's reads after the loop when, inside it, only its exit test, its update,
    /// and the address arithmetic in `addresses` that the pointers replace read it. After
    /// the loop, only the counter itself may be read.
    fn counter_only_feeds(
        func: &Function,
        loop_data: &Loop,
        iv: ValueId,
        condition: InstId,
        update: Option<InstId>,
        addresses: &FxHashSet<ValueId>,
    ) -> Option<ExitReads> {
        let mut pending = Vec::new();
        pending.push(iv);
        let mut visited = DenseBitSet::new_empty(func.num_values());
        while let Some(value) = pending.pop() {
            if !visited.insert(value) {
                continue;
            }
            for block_id in loop_data.blocks.iter() {
                let block = &func.blocks[block_id];
                if block.terminator.as_ref().is_some_and(|term| term.reads(value)) {
                    return None;
                }
                for &inst_id in &block.instructions {
                    let inst = func.inst(inst_id);
                    if !inst.kind.reads(value) {
                        continue;
                    }
                    if inst_id == condition || Some(inst_id) == update {
                        continue;
                    }
                    let result = func.inst_result_value(inst_id)?;
                    if matches!(inst.kind, InstKind::Phi(_)) {
                        return None;
                    }
                    if addresses.contains(&result) {
                        continue;
                    }
                    if !Self::is_address_builder(&inst.kind) {
                        return None;
                    }
                    pending.push(result);
                }
            }
        }
        // Outside the loop, besides the exit test and the update, only the counter itself may
        // be read, which its pointer can rebuild there.
        let mut reads = ExitReads::default();
        let derived = |operand: ValueId| operand != iv && visited.contains(operand);
        for (block_id, block) in func.blocks.iter_enumerated() {
            if loop_data.blocks.contains(block_id) {
                continue;
            }
            let mut reads_counter = false;
            if let Some(term) = &block.terminator {
                if term.any_operand(derived) {
                    return None;
                }
                reads_counter |= term.reads(iv);
            }
            for &inst_id in &block.instructions {
                if inst_id == condition || Some(inst_id) == update {
                    continue;
                }
                let kind = &func.inst(inst_id).kind;
                if kind.any_operand(derived) {
                    return None;
                }
                if let InstKind::Phi(incoming) = kind {
                    for &(from, value) in incoming {
                        if value != iv {
                            continue;
                        }
                        if loop_data.blocks.contains(from) {
                            reads.edges.push((from, block_id));
                        } else {
                            reads.phis.push((inst_id, from));
                        }
                    }
                } else {
                    reads_counter |= kind.reads(iv);
                }
            }
            if reads_counter {
                reads.blocks.push(block_id);
            }
        }
        Some(reads)
    }

    /// Rebuilds the counter's reads after the loop from the pointer that took over its exit
    /// test: `init + (ptr - start) / scale`, with `start` the pointer's value on entry. The
    /// pointer moves `scale` bytes for every step of the counter, and the counter travels
    /// below `2^64` steps, so the difference never wraps past the word.
    fn rebuild_exit_reads(
        &self,
        func: &mut Function,
        preheader: BlockId,
        iv: &InductionVariable,
        pointer: ValueId,
        scale: i128,
        reads: &ExitReads,
    ) {
        // The rebuild counts up from the start, as the equality exit's counter does; a counter
        // left unrebuilt stays alive for its readers.
        if iv.descending {
            return;
        }
        let Some(start) = preheader_value(func, pointer, preheader) else { return };
        let Ok(magnitude) = u128::try_from(scale) else { return };
        // counter = init + (ptr - start) >> log2(scale)   or   / scale
        let rebuild = |this: &Self, func: &mut Function, block: BlockId| -> ValueId {
            let delta = this.append_inst_value(
                func,
                block,
                InstKind::Sub(pointer, start),
                Some(MirType::I256),
            );
            let steps = if magnitude == 1 {
                delta
            } else if magnitude.is_power_of_two() {
                let shift = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(
                    magnitude.trailing_zeros(),
                ))));
                this.append_inst_value(
                    func,
                    block,
                    InstKind::Shr(shift, delta),
                    Some(MirType::I256),
                )
            } else {
                let divisor =
                    func.alloc_value(Value::Immediate(Immediate::I256(U256::from(magnitude))));
                this.append_inst_value(
                    func,
                    block,
                    InstKind::Div(delta, divisor),
                    Some(MirType::I256),
                )
            };
            if func.value_u256(iv.init).is_some_and(|init| init.is_zero()) {
                steps
            } else {
                this.append_inst_value(
                    func,
                    block,
                    InstKind::Add(iv.init, steps),
                    Some(MirType::I256),
                )
            }
        };
        for &(phi, from) in &reads.phis {
            // from: ...; counter = rebuild; jump  ->  phi [from: counter]
            let counter = rebuild(self, func, from);
            if let InstKind::Phi(incoming) = &mut func.inst_mut(phi).kind {
                for (block, value) in incoming.iter_mut() {
                    if *block == from && *value == iv.value {
                        *value = counter;
                    }
                }
            }
        }
        for &block in &reads.blocks {
            // block: phis; counter = rebuild; ...reads of counter
            let phis = func.blocks[block]
                .instructions
                .iter()
                .take_while(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .count();
            let body = func.blocks[block].instructions.split_off(phis);
            let counter = rebuild(self, func, block);
            let rebuilt = func.blocks[block].instructions.split_off(phis);
            func.blocks[block]
                .instructions
                .extend(rebuilt.iter().copied().chain(body.iter().copied()));
            for &inst_id in &body {
                func.inst_mut(inst_id).kind.visit_operands_mut(|operand| {
                    if *operand == iv.value {
                        *operand = counter;
                    }
                });
            }
            if let Some(term) = func.blocks[block].terminator.as_mut() {
                term.visit_operands_mut(|operand| {
                    if *operand == iv.value {
                        *operand = counter;
                    }
                });
            }
        }
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
                block.terminator.as_ref().is_some_and(|term| term.reads(value))
                    || block
                        .instructions
                        .iter()
                        .any(|&inst_id| inst_id != except && func.inst(inst_id).kind.reads(value))
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
        // A literal index folds into the offset, as the pointer's start does.
        if let Some(offset) = self
            .value_i128(func, index)
            .and_then(|index| index.checked_mul(key.scale)?.checked_add(key.constant))
        {
            return match value {
                Some(value) => self.add_signed_offset(func, block, value, offset),
                None => self.offset_value(func, offset),
            };
        }
        let scaled = self.scale_value(func, block, index, key.scale)?;
        let value = self.add_values(func, block, value, scaled);
        self.add_signed_offset(func, block, value, key.constant)
    }

    /// Appends to `block` the bound clamped to `[start, 2^65]` where the pointer could reach
    /// `end` before the counter reaches the bound.
    ///
    /// The pointers are equal when the counters agree modulo `2^(256 - k)`, where `2^k` is the
    /// largest power of two dividing the scale. A bound below that modulus, as an odd scale or
    /// a check on every path into the loop guarantees, is reached first exactly when the
    /// counter reaches it. A larger bound is clamped to `2^65`, which a counter starting below
    /// `2^64` reaches only after more than the `2^MAX_TRIP_COUNT_BITS` iterations a loop can
    /// run. A bound at or below the start enters no iteration, as `ptr == end` then holds at
    /// once.
    fn clamped_bound(
        &self,
        func: &mut Function,
        cfg: &CfgInfo,
        block: BlockId,
        bound: ValueId,
        start: U256,
        scale: i128,
    ) -> ValueId {
        let limit = U256::from(1) << (Target::MAX_TRIP_COUNT_BITS + 1);
        if let Some(value) = func.value_u256(bound) {
            let clamped = value.max(start).min(limit);
            return func.alloc_value(Value::Immediate(Immediate::I256(clamped)));
        }
        let exact = U256::MAX >> scale.trailing_zeros();
        let mut clamped = bound;
        if exact != U256::MAX && !bounded_on_entry(func, cfg, block, bound, exact) {
            // above = gt bound, 2^65; bound = select above, 2^65, bound
            let limit = func.alloc_value(Value::Immediate(Immediate::I256(limit)));
            let above =
                self.append_inst_value(func, block, InstKind::Gt(bound, limit), Some(MirType::I1));
            clamped = self.append_inst_value(
                func,
                block,
                InstKind::Select(above, limit, bound),
                Some(MirType::I256),
            );
        }
        if !start.is_zero() {
            // below = lt bound, start; bound = select below, start, bound
            let start = func.alloc_value(Value::Immediate(Immediate::I256(start)));
            let below = self.append_inst_value(
                func,
                block,
                InstKind::Lt(clamped, start),
                Some(MirType::I1),
            );
            clamped = self.append_inst_value(
                func,
                block,
                InstKind::Select(below, start, clamped),
                Some(MirType::I256),
            );
        }
        clamped
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
                let acc = self.word_value(func, block, acc);
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
                arithmetic: ArithmeticKind::Unsigned(_),
                lhs: a,
                rhs: b,
            } if a == iv_value => self.value_i128(func, b),
            InstKind::Add(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Add,
                arithmetic: ArithmeticKind::Unsigned(_),
                lhs: a,
                rhs: b,
            } if b == iv_value => self.value_i128(func, a),
            InstKind::Sub(a, b)
            | InstKind::CheckedBinary {
                op: CheckedOp::Sub,
                arithmetic: ArithmeticKind::Unsigned(_),
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
        let value = self.word_value(func, block, value);
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
        let value = self.word_value(func, block, value);
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

    fn word_value(&self, func: &mut Function, block: BlockId, value: ValueId) -> ValueId {
        let mut builder = FunctionBuilder::new(func);
        builder.switch_to_block(block);
        builder.cast_word(value)
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
            InstKind::Add(_, _)
                | InstKind::Sub(_, _)
                | InstKind::Mul(_, _)
                | InstKind::Shl(_, _)
                | InstKind::Zext(_)
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
                if kind.reads(value) && !Self::is_address_builder(kind) {
                    return true;
                }
            }
            if func.blocks[block].terminator.as_ref().is_some_and(|term| term.reads(value)) {
                return true;
            }
        }
        false
    }

    fn is_address_builder(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Add(_, _)
                | InstKind::Sub(_, _)
                | InstKind::Mul(_, _)
                | InstKind::Shl(_, _)
                | InstKind::Zext(_)
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

/// How deep [`bounded_on_entry`] follows a branch condition's operands and a bound's
/// definition.
const MAX_FACT_DEPTH: usize = 8;

/// Whether every path into `block` takes a branch edge that keeps `value` at most a word
/// that cannot exceed `limit`, such as the false edge of an ABI decoder's
/// `length > (calldatasize - offset) >> 5` check.
fn bounded_on_entry(
    func: &Function,
    cfg: &CfgInfo,
    block: BlockId,
    value: ValueId,
    limit: U256,
) -> bool {
    cfg.dominators().self_and_dominators(block).into_iter().any(|block| {
        // An edge decides a fact on entry only when it is the block's only way in.
        let &[pred] = cfg.predecessors(block) else { return false };
        let Some(Terminator::Branch { condition, then_block, .. }) = &func.blocks[pred].terminator
        else {
            return false;
        };
        implies_at_most(func, *condition, *then_block == block, value, limit, MAX_FACT_DEPTH)
    })
}

/// Whether `condition` being nonzero exactly when `nonzero` holds keeps `value` at most a word
/// that cannot exceed `limit`.
fn implies_at_most(
    func: &Function,
    condition: ValueId,
    nonzero: bool,
    value: ValueId,
    limit: U256,
    depth: usize,
) -> bool {
    let Some(depth) = depth.checked_sub(1) else { return false };
    let Value::Inst(inst_id) = *func.value(condition) else { return false };
    let implies =
        |condition, nonzero| implies_at_most(func, condition, nonzero, value, limit, depth);
    let is_zero = |operand| func.value_u256(operand).is_some_and(|operand| operand.is_zero());
    match (&func.inst(inst_id).kind, nonzero) {
        // value <= x, or value < x
        (&InstKind::Gt(a, x), false)
        | (&InstKind::Lt(x, a), false)
        | (&InstKind::Lt(a, x), true)
        | (&InstKind::Gt(x, a), true)
            if a == value =>
        {
            upper_bound(func, x, depth).is_some_and(|bound| bound <= limit)
        }
        // A zero `or` has zero operands, and a nonzero `and` nonzero ones.
        (&InstKind::Or(a, b), false) | (&InstKind::And(a, b), true) => {
            implies(a, nonzero) || implies(b, nonzero)
        }
        (&InstKind::Eq(a, b), _) if is_zero(b) => implies(a, !nonzero),
        (&InstKind::Ne(a, b), _) if is_zero(b) => implies(a, nonzero),
        (&InstKind::Zext(source), _) => implies(source, nonzero),
        _ => false,
    }
}

/// The largest value a word can take by its definition: literals, comparisons, masks, right
/// shifts, and quotients by a literal.
fn upper_bound(func: &Function, value: ValueId, depth: usize) -> Option<U256> {
    if let Some(constant) = func.value_u256(value) {
        return Some(constant);
    }
    let depth = depth.checked_sub(1)?;
    let Value::Inst(inst_id) = *func.value(value) else { return None };
    match func.inst(inst_id).kind {
        InstKind::Lt(..)
        | InstKind::Gt(..)
        | InstKind::SLt(..)
        | InstKind::SGt(..)
        | InstKind::Eq(..)
        | InstKind::Ne(..) => Some(U256::from(1)),
        InstKind::Zext(source) => upper_bound(func, source, depth),
        InstKind::And(a, b) => match (upper_bound(func, a, depth), upper_bound(func, b, depth)) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (bound, None) | (None, bound) => bound,
        },
        InstKind::Shr(shift, source) => {
            let shift = usize::try_from(func.value_u256(shift)?).ok()?;
            let source = upper_bound(func, source, depth).unwrap_or(U256::MAX);
            Some(source.checked_shr(shift).unwrap_or(U256::ZERO))
        }
        InstKind::Div(_, divisor) => {
            let divisor = func.value_u256(divisor)?;
            Some(U256::MAX.checked_div(divisor).unwrap_or(U256::ZERO))
        }
        _ => None,
    }
}

/// The value the header phi `phi` takes on entry from `preheader`.
fn preheader_value(func: &Function, phi: ValueId, preheader: BlockId) -> Option<ValueId> {
    let Value::Inst(inst_id) = *func.value(phi) else { return None };
    let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { return None };
    incoming.iter().find_map(|&(from, value)| (from == preheader).then_some(value))
}

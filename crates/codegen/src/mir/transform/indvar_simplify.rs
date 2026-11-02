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
//! It requires canonical loops with a dedicated preheader and a single latch,
//! and counters with a single additive update, and takes the address
//! expressions from scalar evolution rather than from instruction patterns.
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
//! and its clamped `end` cannot wrap, while a heap pointer's `ptr < end` would
//! leave at once for a bound near the word size and return a counter the loop
//! never reached. A phi reading the counter over a critical edge, from a loop
//! block or from a block after the loop that also branches elsewhere, needs a
//! block of its own on that edge, so that only the path into the phi pays for the
//! rebuild: the pass splits those edges after visiting every loop of the function
//! and then visits those loops again. Only the counter itself may be read after
//! the loop; a value the loop derives from it keeps the counter, and so does a
//! counter narrower than a word, which the rebuild would replace with a word.
//!
//! A loop that leaves on what it reads, such as a partition's scan
//! `do ++i; while (a[i] < y)` once its bounds checks are gone, tests no counter
//! at all: the counter only addresses the element and tells the code after the
//! loop where it stopped. With one address family built from the counter's
//! update, the pointer then steps beside that update: its phi holds the address
//! of the counter's previous value and `next = ptr + delta`, right after the
//! update, the address the iteration reads, which replaces every use of the
//! family's address, after the loop too. Both copies of the value live across
//! the back edge as one word, so the latch's branch closes the loop without a
//! pop. The loop's one exit block, which only the loop enters and the update
//! dominates, rebuilds the counter's last value as
//! `start + (next - ptr_start) / scale`, exact whatever the start, since the
//! pointer moves by whole steps and fewer than `2^MAX_TRIP_COUNT_BITS` of them,
//! and the counter dies. It pays when the address, the counter's update and the
//! word an address read after the loop keeps across the exit cost more than the
//! pointer's update and carried word.
//!
//! When the counter starts from a phi of an enclosing block whose input on some
//! edge is the loop's own update, as each scan of a partition resumes where its
//! last run stopped, that block gets a pointer phi too: the pointer's last `next`
//! on those edges, which addresses the same element, and the pointer at the
//! phi's input, computed at the end of the edge's block, on the others. When the
//! two phis have the same two edges, the counter equals its entry value plus the
//! pointer's whole steps from its own entry value at every run, so the rebuild
//! counts from there; an enclosing counter that then only feeds its own rebuild
//! dies with it, and a rebuild that only one join's phis read on the edge from
//! the exit moves onto that edge once every loop is visited, so the iterations
//! that stay in the enclosing loop skip it.
//!
//! Safety contract:
//! - require canonical loops with a preheader and a single latch
//! - rewrite only affine address expressions derived from the recognized induction variable
//! - preserve the original address value when it is still used outside the loop, unless a pointer
//!   stepping beside the update replaces it, which equals it wherever it is read
//! - recognize checked unsigned updates at every width and retain their failure checks.
//! - add only one scaled address counter when the original update must stay live.

use super::egraph::max_bits_with_args;
use crate::{
    mir::{
        ArithmeticKind, BlockId, CheckedOp, Function, FunctionBuilder, Immediate, InstId, InstKind,
        Instruction, MemoryRegion, MirType, Module, Terminator, Value, ValueId,
        analysis::{
            AffineTerm, AliasAnalysis, CfgInfo, InductionVariable, Loop, LoopAnalyzer,
            ScalarEvolution, integers::unsigned_bounds,
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
                    .run(func, Rc::clone(analyses.cfg()));
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

/// Performs conservative induction-variable strength reduction.
#[derive(Debug)]
struct IndVarSimplifier {
    /// Whether a pointer phi was inserted or an edge split.
    changed: bool,
    alias: Rc<AliasAnalysis>,
    /// Exit edges, from a loop block to a join outside it, whose phis read a counter that
    /// could otherwise die, by the loop header.
    exit_splits: Vec<(BlockId, BlockId, BlockId)>,
    /// Counter rebuilds that only a join's phis read, on one edge from the block they sit in:
    /// the block, the join, and the rebuild's instructions, which move onto that edge once every
    /// loop is visited.
    rebuild_moves: Vec<(BlockId, BlockId, Vec<InstId>)>,
    /// The loop's instructions, refilled by each user.
    in_loop: DenseBitSet<InstId>,
    /// The values the loop's non-address instructions and terminators read.
    read_by_non_address: DenseBitSet<ValueId>,
    /// Values already counted or visited by one scan.
    seen: DenseBitSet<ValueId>,
}

/// Reads of a counter phi after its loop, which the counter's pointer rebuilds once the
/// pointer takes over the exit test.
#[derive(Default)]
struct ExitReads {
    /// Blocks outside the loop whose instructions or terminator read the counter.
    blocks: Vec<BlockId>,
    /// Phis outside the loop with the counter incoming from a block outside the loop.
    phis: Vec<(InstId, BlockId)>,
    /// Edges into a join outside the loop whose phis read the counter, from a loop block or
    /// from a block that also branches elsewhere: each needs a block of its own to rebuild in.
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
        Self {
            changed: false,
            alias,
            exit_splits: Vec::new(),
            rebuild_moves: Vec::new(),
            in_loop: DenseBitSet::new_empty(0),
            read_by_non_address: DenseBitSet::new_empty(0),
            seen: DenseBitSet::new_empty(0),
        }
    }

    /// Runs induction-variable simplification once over `func` and returns whether it changed.
    fn run(&mut self, func: &mut Function, cfg: Rc<CfgInfo>) -> bool {
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
                    self.changed = true;
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

        // block -> join  =>  block -> rebuild -> join, with the rebuild moved off the block
        for (block, join, rebuild) in std::mem::take(&mut self.rebuild_moves) {
            let values = rebuild.iter().filter_map(|&inst| func.inst_result_value(inst));
            let values = values.collect::<Vec<_>>();
            if !func.blocks[block].terminator.as_ref().is_some_and(|term| term.has_successor(join))
                || !rebuild.iter().all(|inst| func.blocks[block].instructions.contains(inst))
                || Self::only_join_reading(func, block, &values) != Some(join)
            {
                continue;
            }
            let edge = mir_utils::split_edge(func, block, join);
            self.changed = true;
            func.blocks[block].instructions.retain(|inst| !rebuild.contains(inst));
            func.blocks[edge].instructions.extend(rebuild);
        }

        self.changed
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
        if Self::position(func, loop_data, iv.update_inst).is_none() {
            return;
        }
        let Some(step) = self.additive_step(func, iv.value, iv.update_inst) else {
            return;
        };
        let must_keep_update = func.inst(iv.update_inst).kind.has_side_effects();

        let scev = ScalarEvolution::analyze(func, loop_data);
        // The loop's instructions, and the values its other instructions read.
        self.in_loop.clear_to(func.num_insts());
        self.read_by_non_address.clear_to(func.num_values());
        for block in loop_data.blocks.iter() {
            for &inst_id in &func.blocks[block].instructions {
                self.in_loop.insert(inst_id);
                let kind = &func.inst(inst_id).kind;
                if !Self::is_address_builder(kind) {
                    kind.visit_operands(|operand| {
                        self.read_by_non_address.insert(operand);
                    });
                }
            }
            if let Some(term) = &func.blocks[block].terminator {
                term.visit_operands(|operand| {
                    self.read_by_non_address.insert(operand);
                });
            }
        }
        let carried = Self::carried_words(func, loop_data, &self.in_loop, &mut self.seen);
        let update_value = func.inst_result_value(iv.update_inst);
        let mut candidates: FxHashMap<AddressKey, Vec<ValueId>> = FxHashMap::default();
        let mut offset_shared = FxHashSet::default();

        for block in &loop_data.blocks {
            for &inst_id in &func.blocks[block].instructions {
                let Some(value) = func.inst_result_value(inst_id) else { continue };
                let inst = func.inst(inst_id);
                if inst.result_ty != Some(MirType::I256) || !Self::is_address_builder(&inst.kind) {
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
                if delta == 0 || !self.read_by_non_address.contains(value) {
                    continue;
                }
                if update_value
                    .is_some_and(|update| Self::depends_on(func, &self.in_loop, value, update, 0))
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
            members.sort_by_key(|(key, values)| {
                (std::cmp::Reverse(values.len()), key.constant.abs(), values.iter().min().copied())
            });
        }
        // Hash iteration order follows value numbering; order families by their first value so
        // the chosen pointers do not change when unrelated instructions shift the numbering.
        families
            .sort_by_key(|members| members.iter().flat_map(|(_, values)| values).min().copied());

        // With the counter free once every address is a pointer, an ascending pointer
        // that cannot wrap takes over the exit test and the counter dies; that credit
        // is weighed across all families at once.
        let exit_test = Self::counter_exit_test(func, loop_data, &self.in_loop, iv.value);
        // A loop that leaves on what it reads, with no test of its counter, keeps the counter
        // only to address elements and to tell the code after it where it stopped.
        if exit_test.is_none()
            && !must_keep_update
            && let [members] = families.as_slice()
            && self.reduce_untested_counter(
                func,
                cfg,
                loop_data,
                preheader,
                latch,
                &iv,
                step,
                members,
                &offset_shared,
                carried,
            )
        {
            return;
        }
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
                    iv.update_inst,
                    &addresses,
                    &mut self.seen,
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
        // NOTE: A small trip count does not bound an arbitrary start or bound. Keep strict
        // pointer comparisons only when both fit well below the wrap point; the clamped
        // equality fallback below handles large bounds without making that assumption.
        let test_family = if counter_free {
            families
                .iter()
                .position(|members| {
                    let key = &members[0].0;
                    key.scale > 0
                        && key.scale <= 1 << 62
                        && key.invariants.is_empty()
                        && key.base.is_some_and(|base| self.is_heap_address(func, base))
                        && step > 0
                        && step <= 1 << 64
                        && max_bits_with_args(func, iv.init, 4, &|_| 256) <= 64
                        && exit_test.is_some_and(|test| {
                            max_bits_with_args(func, test.bound, 4, &|_| 256) <= 128
                        })
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
            if !pays {
                continue;
            }
            let Some(pointer) =
                self.materialize_pointer_phi(func, loop_data, preheader, latch, &iv, step, primary)
            else {
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
                test_rewritten = true;
            }
        }

        // sibling = ptr + offset, in place of its old address arithmetic
        for (value, pointer, offset) in siblings {
            self.rewrite_sibling(func, value, pointer, offset);
        }

        if replacements.is_empty() {
            return;
        }

        Self::replace_block_uses(func, loop_data.blocks.iter(), &replacements);
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
            self.remove_dead_counter(func, loop_data, iv.value, iv.update_inst);
        }
    }

    /// Removes the loop's pure address arithmetic whose results nothing reads any more,
    /// following each removal to the operands it leaves unread.
    fn remove_dead_address_arithmetic(&mut self, func: &mut Function, loop_data: &Loop) {
        let mut uses = super::egraph::use_counts(func);
        // The loop's instructions, less those removed below.
        let in_loop = &mut self.in_loop;
        in_loop.clear_to(func.num_insts());
        let mut pending = Vec::new();
        for block in loop_data.blocks.iter() {
            for &inst_id in &func.blocks[block].instructions {
                in_loop.insert(inst_id);
                if Self::is_address_builder(&func.inst(inst_id).kind)
                    && func.inst_result_value(inst_id).is_some_and(|result| uses[result] == 0)
                {
                    pending.push(inst_id);
                }
            }
        }
        let mut removed = false;
        while let Some(inst_id) = pending.pop() {
            removed |= in_loop.remove(inst_id);
            func.inst(inst_id).kind.visit_operands(|operand| {
                let count = &mut uses[operand];
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

    /// Operations an address the code after the loop reads costs per iteration beyond its
    /// rebuild: the word it keeps live past the exit branch, which the back edge pops.
    const ESCAPE_COST: usize = 2;

    /// Replaces a counter that no test reads, as in a scan that leaves on the element it reads,
    /// with a pointer that steps beside the counter's update: the phi holds the address of the
    /// counter's previous value and `next = ptr + delta`, right after the update, the address of
    /// the updated counter, which replaces every use of the family's address, after the loop
    /// too. The exit block rebuilds the counter's last value from how far the pointer moved,
    /// `start + (next - ptr_start) / scale`, exact for any start since a loop moves its pointer
    /// fewer than `2^MAX_TRIP_COUNT_BITS` steps, and the counter dies. Returns whether it did.
    ///
    /// The counter's phi may feed only its update, the update only the family's address
    /// arithmetic and the phi, and the code after the loop only the counter or its update,
    /// behind the loop's one exit block, which the loop alone enters.
    #[allow(clippy::too_many_arguments)]
    fn reduce_untested_counter(
        &mut self,
        func: &mut Function,
        cfg: &CfgInfo,
        loop_data: &Loop,
        preheader: BlockId,
        latch: BlockId,
        iv: &InductionVariable,
        step: i128,
        members: &[(AddressKey, Vec<ValueId>)],
        offset_shared: &FxHashSet<ValueId>,
        carried: usize,
    ) -> bool {
        let Some(next) = func.inst_result_value(iv.update_inst) else { return false };
        let &[exit] = loop_data.exit_blocks.as_slice() else { return false };
        if cfg.predecessors(exit).iter().any(|&pred| !loop_data.blocks.contains(pred))
            || func.blocks[exit]
                .instructions
                .iter()
                .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
            || func.value_ty(iv.value) != Some(MirType::I256)
        {
            return false;
        }
        let (primary, primary_values) = &members[0];
        let Some(update_position) = Self::position(func, loop_data, iv.update_inst) else {
            return false;
        };
        // Every way out of the loop passes the update, so the exit can rebuild from its pointer.
        if !cfg.dominators().dominates(update_position.0, exit) {
            return false;
        }
        let follows_update = |value: ValueId| {
            let Value::Inst(inst) = *func.value(value) else { return false };
            Self::position(func, loop_data, inst).is_some_and(|(block, index)| {
                if block == update_position.0 {
                    update_position.1 < index
                } else {
                    cfg.dominators().dominates(update_position.0, block)
                }
            })
        };
        // The pointer replaces the family's addresses from the update on.
        if !members.iter().flat_map(|(_, values)| values).all(|&value| follows_update(value)) {
            return false;
        }
        // In the loop, the counter feeds its update and the update the address arithmetic of
        // the family, whose members the pointer replaces, and the phi.
        let addresses = members.iter().flat_map(|(_, values)| values.iter().copied());
        let addresses = addresses.collect::<FxHashSet<_>>();
        let mut derived = FxHashSet::default();
        let mut pending = vec![next];
        while let Some(value) = pending.pop() {
            for block in loop_data.blocks.iter() {
                if func.blocks[block].terminator.as_ref().is_some_and(|term| term.reads(value)) {
                    return false;
                }
                for &inst in &func.blocks[block].instructions {
                    let kind = &func.inst(inst).kind;
                    if !kind.reads(value) {
                        continue;
                    }
                    if let InstKind::Phi(incoming) = kind {
                        // Only the counter's own phi takes the update, on the back edge.
                        if func.inst_result_value(inst) != Some(iv.value)
                            || incoming.iter().any(|&(from, read)| read == value && from != latch)
                        {
                            return false;
                        }
                        continue;
                    }
                    let Some(result) = func.inst_result_value(inst) else { return false };
                    if addresses.contains(&result) {
                        continue;
                    }
                    if !Self::is_address_builder(kind) || !derived.insert(result) {
                        return false;
                    }
                    pending.push(result);
                }
            }
        }
        for block in loop_data.blocks.iter() {
            for &inst in &func.blocks[block].instructions {
                if inst != iv.update_inst && func.inst(inst).kind.reads(iv.value) {
                    return false;
                }
            }
            if func.blocks[block].terminator.as_ref().is_some_and(|term| term.reads(iv.value)) {
                return false;
            }
        }
        // After the loop, only the counter, its update, and the family's addresses are read.
        let reads_outside = |value: ValueId| Self::read_after_loop(func, loop_data, value);
        if derived.iter().any(|&value| reads_outside(value))
            || members[1..].iter().flat_map(|(_, values)| values).any(|&value| reads_outside(value))
        {
            return false;
        }
        let escaping = primary_values.iter().any(|&value| reads_outside(value));
        let reads_counter = reads_outside(iv.value);
        let before = Self::family_cost_before(members, offset_shared)
            + Self::COUNTER_COST
            + if escaping { Self::ESCAPE_COST } else { 0 };
        if before <= Self::family_cost_after(members, carried) {
            return false;
        }
        let Some(magnitude) =
            primary.scale.checked_abs().and_then(|scale| u128::try_from(scale).ok())
        else {
            return false;
        };
        if !magnitude.is_power_of_two() {
            return false;
        }
        let Some((pointer, stepped)) = self
            .materialize_stepped_pointer(func, cfg, loop_data, preheader, latch, iv, step, primary)
        else {
            return false;
        };
        let Some(start) = preheader_value(func, pointer, preheader) else { return false };
        // The counter and the pointer at the run's start, or, when an enclosing loop carries both
        // from one entry, where the enclosing loop entered: each later run starts where the last
        // stopped, the counter and the pointer having moved together by whole steps since.
        let (counter_start, start) =
            Self::run_anchor(func, iv.init, start, next).unwrap_or((iv.init, start));
        // sibling = next + offset
        let mut replacements = FxHashMap::default();
        for &value in primary_values {
            replacements.insert(value, stepped);
        }
        for (key, values) in &members[1..] {
            let Some(offset) = key.constant.checked_sub(primary.constant) else { continue };
            let Some(magnitude) = offset.checked_abs() else { continue };
            let Some(magnitude) = self.offset_value(func, magnitude) else { continue };
            for &value in values {
                let Value::Inst(inst) = *func.value(value) else { continue };
                func.inst_mut(inst).kind = offset_kind(stepped, magnitude, offset >= 0);
            }
        }
        func.replace_uses(&replacements);
        self.remove_dead_address_arithmetic(func, loop_data);
        // exit: moved = next - ptr_start; steps = moved >> log2(|scale|); counter' = start + steps
        // (a pointer stepping down: moved = ptr_start - next; a counter stepping down:
        // counter' = start - steps)
        let ascending = primary.scale.checked_mul(step).is_some_and(|product| product > 0);
        // The exit has no phis, so its instructions follow the rebuild.
        let body = std::mem::take(&mut func.blocks[exit].instructions);
        let moved =
            if ascending { InstKind::Sub(stepped, start) } else { InstKind::Sub(start, stepped) };
        let moved = self.append_inst_value(func, exit, moved, Some(MirType::I256));
        let steps = self.append_steps(func, exit, moved, magnitude);
        let last = self.append_inst_value(
            func,
            exit,
            offset_kind(counter_start, steps, step > 0),
            Some(MirType::I256),
        );
        // The counter itself is one step behind its update.
        let amount =
            func.alloc_value(Value::Immediate(Immediate::I256(U256::from(step.unsigned_abs()))));
        let previous = offset_kind(last, amount, step <= 0);
        let previous = reads_counter
            .then(|| self.append_inst_value(func, exit, previous, Some(MirType::I256)));
        func.blocks[exit].instructions.extend(body);
        let mut rebuilt = FxHashMap::default();
        rebuilt.insert(next, last);
        if let Some(previous) = previous {
            rebuilt.insert(iv.value, previous);
        }
        // ...reads of the counter after the loop  =>  ...reads of counter'
        let outside = func.blocks.indices().filter(|&block| !loop_data.blocks.contains(block));
        Self::replace_block_uses(func, outside, &rebuilt);
        self.remove_dead_counter(func, loop_data, iv.value, iv.update_inst);
        // A counter that starts from an enclosing phi fed only by this rebuild, as a partition's
        // scan up that the enclosing loop no longer compares once it carries the pointer, keeps a
        // cycle that only feeds itself: remove the phi and the rebuild.
        let mut cycle = vec![moved, steps, last];
        cycle.extend(previous);
        cycle.push(iv.init);
        Self::remove_dead_cycle(func, &cycle);
        // A rebuild that only one join's phis read on the edge from the exit, as the index a
        // partition returns once its loop carries pointers, moves onto that edge, so the
        // iterations that stay in an enclosing loop skip it.
        let rebuild = [Some(moved), (steps != moved).then_some(steps), Some(last), previous]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>();
        if let Some(join) = Self::only_join_reading(func, exit, &rebuild) {
            let insts = rebuild
                .iter()
                .filter_map(|&value| match *func.value(value) {
                    Value::Inst(inst) => Some(inst),
                    _ => None,
                })
                .collect();
            self.rebuild_moves.push((exit, join, insts));
        }
        true
    }

    /// The one block whose phis are the only readers of `values`, all on the edge from `from`,
    /// when that block has other predecessors, so the values need an edge block of their own.
    fn only_join_reading(func: &Function, from: BlockId, values: &[ValueId]) -> Option<BlockId> {
        let mut join = None;
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst in &block.instructions {
                let kind = &func.inst(inst).kind;
                if values.iter().any(|&value| func.inst_result_value(inst) == Some(value)) {
                    // The rebuild's own instructions read each other.
                    continue;
                }
                match kind {
                    InstKind::Phi(incoming) => {
                        for &(pred, input) in incoming {
                            if values.contains(&input) {
                                if pred != from || join.is_some_and(|join| join != block_id) {
                                    return None;
                                }
                                join = Some(block_id);
                            }
                        }
                    }
                    _ if values.iter().any(|&value| kind.reads(value)) => return None,
                    _ => {}
                }
            }
            if block.terminator.as_ref().is_some_and(|term| values.iter().any(|&v| term.reads(v))) {
                return None;
            }
        }
        join.filter(|&join| func.blocks[join].predecessors.len() > 1)
    }

    /// The counter and the pointer where an enclosing loop entered, when the run's starts are
    /// the phis `counter = phi [entry: c0], [back: next]` and `ptr = phi [entry: p0], [back:
    /// ...]` of one block with the same two edges: the counter then equals `c0` plus the
    /// pointer's whole steps from `p0` at every run, so a rebuild can count from the entry.
    fn run_anchor(
        func: &Function,
        counter: ValueId,
        pointer: ValueId,
        next: ValueId,
    ) -> Option<(ValueId, ValueId)> {
        let phi_inputs = |value: ValueId| match *func.value(value) {
            Value::Inst(inst) => match &func.inst(inst).kind {
                InstKind::Phi(incoming) if incoming.len() == 2 => Some(incoming.clone()),
                _ => None,
            },
            _ => None,
        };
        let counters = phi_inputs(counter)?;
        let pointers = phi_inputs(pointer)?;
        let &(back, _) = counters.iter().find(|&&(_, input)| input == next)?;
        let &(entry, counter_entry) = counters.iter().find(|&&(from, _)| from != back)?;
        let &(_, pointer_entry) = pointers.iter().find(|&&(from, _)| from == entry)?;
        pointers.iter().any(|&(from, _)| from == back).then_some((counter_entry, pointer_entry))
    }

    /// Removes the instructions defining `values` that nothing outside them reads, phis included,
    /// as the plain dead-code elimination keeps a cycle that only feeds itself.
    fn remove_dead_cycle(func: &mut Function, values: &[ValueId]) {
        let defining = values
            .iter()
            .filter_map(|&value| match *func.value(value) {
                Value::Inst(inst) => Some((value, inst)),
                _ => None,
            })
            .collect::<FxHashMap<_, _>>();
        let mut live = FxHashSet::default();
        // A value is live when an instruction or terminator outside the candidates reads it, or
        // a live candidate does.
        for block in &func.blocks {
            for &inst in &block.instructions {
                if defining.values().any(|&candidate| candidate == inst) {
                    continue;
                }
                func.inst(inst).kind.visit_operands(|operand| {
                    if defining.contains_key(&operand) {
                        live.insert(operand);
                    }
                });
            }
            if let Some(term) = &block.terminator {
                term.visit_operands(|operand| {
                    if defining.contains_key(&operand) {
                        live.insert(operand);
                    }
                });
            }
        }
        let mut pending = live.iter().copied().collect::<Vec<_>>();
        while let Some(value) = pending.pop() {
            func.inst(defining[&value]).kind.visit_operands(|operand| {
                if defining.contains_key(&operand) && live.insert(operand) {
                    pending.push(operand);
                }
            });
        }
        let dead = defining
            .iter()
            .filter(|&(value, _)| !live.contains(value))
            .map(|(_, &inst)| inst)
            .collect::<FxHashSet<_>>();
        if dead.is_empty() {
            return;
        }
        for block in func.blocks.iter_mut() {
            block.instructions.retain(|inst| !dead.contains(inst));
        }
    }

    /// The header's exit test when it compares the counter with an invariant.
    fn counter_exit_test(
        func: &Function,
        loop_data: &Loop,
        in_loop: &DenseBitSet<InstId>,
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
            Value::Inst(inst_id) => !in_loop.contains(*inst_id),
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
        update: InstId,
        addresses: &FxHashSet<ValueId>,
        visited: &mut DenseBitSet<ValueId>,
    ) -> Option<ExitReads> {
        let mut pending = Vec::new();
        pending.push(iv);
        visited.clear_to(func.num_values());
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
                    if inst_id == condition || inst_id == update {
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
                if inst_id == condition || inst_id == update {
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
                        // The rebuild runs at the end of the block the phi reads it from, so
                        // every path out of that block would pay for it.
                        let branches_elsewhere =
                            func.blocks[from].terminator.as_ref().is_some_and(|term| {
                                term.successors().iter().any(|&successor| successor != block_id)
                            });
                        if loop_data.blocks.contains(from) || branches_elsewhere {
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
            let steps = this.append_steps(func, block, delta, magnitude);
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
            func.blocks[block].instructions.extend_from_slice(&body);
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
        update: InstId,
    ) {
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
    /// single scaled use only breaks even and is left alone (the `mcopy` loop
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
        in_loop: &DenseBitSet<InstId>,
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
            && in_loop.contains(*inst_id)
            && kind
                .operands()
                .iter()
                .any(|&operand| Self::depends_on(func, in_loop, operand, target, depth + 1))
    }

    /// The words the backend carries through the loop: the header's phis and
    /// the instruction results defined outside that the loop reads.
    fn carried_words(
        func: &Function,
        loop_data: &Loop,
        in_loop: &DenseBitSet<InstId>,
        seen: &mut DenseBitSet<ValueId>,
    ) -> usize {
        let header = &func.blocks[loop_data.header];
        let mut count = header
            .instructions
            .iter()
            .filter(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .count();
        seen.clear_to(func.num_values());
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
                if !in_loop.contains(*inst_id) && seen.insert(operand) {
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
        update_inst: InstId,
    ) -> Option<i128> {
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

    #[allow(clippy::too_many_arguments)]
    fn materialize_pointer_phi(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        preheader: BlockId,
        latch: BlockId,
        iv: &InductionVariable,
        step: i128,
        key: &AddressKey,
    ) -> Option<ValueId> {
        let delta = step.checked_mul(key.scale)?;
        // The latch's step below needs the magnitude, which must exist before the phi does.
        if delta == 0 || delta.checked_abs().is_none() {
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
        self.changed = true;
        Some(phi_value)
    }

    /// Materializes a pointer that steps beside the counter: the phi holds the address of the
    /// counter's previous value, and `next`, inserted just after the counter's update, the
    /// address of the updated counter, which is the value `key` describes for the iteration
    /// and the phi's input from the latch. Returns the phi and `next`.
    ///
    /// A counter that starts where its last run stopped, from a phi of an enclosing block whose
    /// input on some edge is the loop's own last update, as a partition's scans start from their
    /// last positions, starts its pointer from a phi of that block too: on those edges it takes the
    /// pointer's last `next`, which addresses the same element, and on the others the pointer at
    /// the phi's input, computed at the end of the edge's block. The enclosing loop then carries
    /// the pointer instead of rebuilding it from the counter on every entry.
    #[allow(clippy::too_many_arguments)]
    fn materialize_stepped_pointer(
        &mut self,
        func: &mut Function,
        cfg: &CfgInfo,
        loop_data: &Loop,
        preheader: BlockId,
        latch: BlockId,
        iv: &InductionVariable,
        step: i128,
        key: &AddressKey,
    ) -> Option<(ValueId, ValueId)> {
        let delta = step.checked_mul(key.scale)?;
        if delta == 0 {
            return None;
        }
        let update_inst = iv.update_inst;
        let update_value = func.inst_result_value(update_inst)?;
        // The address of the counter one step back: `key` with the step taken off.
        let previous = AddressKey { constant: key.constant.checked_sub(delta)?, ..key.clone() };
        // Every step that can fail runs before the phi and `next` enter their blocks, so a
        // failure leaves no phi without its latch input.
        let (update_block, update_index) = Self::position(func, loop_data, update_inst)?;
        let magnitude = self.offset_value(func, delta.checked_abs()?)?;
        let carried =
            self.carried_start(func, cfg, loop_data, preheader, iv.init, update_value, &previous);
        // preheader: start = base + sum(invariant * scale) + init * scale + constant - delta
        let initial = if carried.is_some() {
            // Replaced by the carried phi below.
            iv.init
        } else if let Some(init) = self.value_i128(func, iv.init) {
            let mut value = previous.base;
            for term in &previous.invariants {
                let scaled = self.scale_value(func, preheader, term.value, term.scale)?;
                value = Some(self.add_values(func, preheader, value, scaled));
            }
            let offset = previous.constant.checked_add(init.checked_mul(previous.scale)?)?;
            self.add_signed_offset(func, preheader, value?, offset)?
        } else {
            self.pointer_at(func, preheader, &previous, iv.init)?
        };
        let (phi_inst, phi_value) = func.alloc_value_inst(
            Instruction::new(InstKind::Phi(vec![(preheader, initial)]), Some(MirType::I256))
                .with_debug_info_dropped(),
        );
        // update block: ...; counter' = update; next = ptr + delta; ...
        let (next_inst, next) = func.alloc_value_inst(
            Instruction::new(offset_kind(phi_value, magnitude, delta > 0), Some(MirType::I256))
                .with_debug_info_dropped(),
        );
        // block: start = phi [from: next], ..., [other: base + sum(...) + input * scale + ...]
        let start = match carried {
            Some((block, inputs)) => {
                let mut starts = Vec::with_capacity(inputs.len());
                for (from, input) in inputs {
                    let start = if input == update_value {
                        next
                    } else {
                        self.pointer_at(func, from, &previous, input)?
                    };
                    starts.push((from, start));
                }
                let (start_inst, start) = func.alloc_value_inst(
                    Instruction::new(InstKind::Phi(starts), Some(MirType::I256))
                        .with_debug_info_dropped(),
                );
                Some((block, start_inst, start))
            }
            None => None,
        };
        // `next` goes in first: the phi may enter the update's own block, ahead of the update.
        func.blocks[update_block].instructions.insert(update_index + 1, next_inst);
        if let Some((block, start_inst, start)) = start {
            self.insert_header_phi(func, block, start_inst);
            let InstKind::Phi(incoming) = &mut func.inst_mut(phi_inst).kind else {
                unreachable!("the pointer is a phi")
            };
            incoming[0].1 = start;
        }
        let InstKind::Phi(incoming) = &mut func.inst_mut(phi_inst).kind else {
            unreachable!("the pointer is a phi")
        };
        incoming.push((latch, next));
        self.insert_header_phi(func, loop_data.header, phi_inst);
        self.changed = true;
        Some((phi_value, next))
    }

    /// The block and inputs of the phi a counter starts from, when some of its inputs are the
    /// loop's own update, so the pointer's last value can start the next run: the phi's block
    /// dominates the preheader, the loop's header every block that passes the update, and every
    /// other input's pointer can be computed at the end of its block.
    #[allow(clippy::too_many_arguments)]
    fn carried_start(
        &self,
        func: &Function,
        cfg: &CfgInfo,
        loop_data: &Loop,
        preheader: BlockId,
        init: ValueId,
        update: ValueId,
        key: &AddressKey,
    ) -> Option<(BlockId, Vec<(BlockId, ValueId)>)> {
        let Value::Inst(init_inst) = *func.value(init) else { return None };
        let InstKind::Phi(incoming) = &func.inst(init_inst).kind else { return None };
        if !incoming.iter().any(|&(_, input)| input == update) {
            return None;
        }
        let block = func
            .blocks
            .indices()
            .find(|&block| func.blocks[block].instructions.contains(&init_inst))?;
        let dominators = cfg.dominators();
        if !dominators.dominates(block, preheader) {
            return None;
        }
        // The key's base and invariants, available wherever their definitions dominate.
        let available = |value: ValueId, at: BlockId| match func.value(value) {
            Value::Inst(inst) => func.blocks.indices().any(|defining| {
                func.blocks[defining].instructions.contains(inst)
                    && dominators.dominates(defining, at)
            }),
            _ => true,
        };
        for &(from, input) in incoming {
            let ok = if input == update {
                dominators.dominates(loop_data.header, from)
            } else {
                key.base.is_none_or(|base| available(base, from))
                    && key.invariants.iter().all(|term| available(term.value, from))
            };
            if !ok {
                return None;
            }
        }
        Some((block, incoming.clone()))
    }

    /// The block of the loop holding `inst`, and its index there.
    fn position(func: &Function, loop_data: &Loop, inst: InstId) -> Option<(BlockId, usize)> {
        loop_data.blocks.iter().find_map(|block| {
            let index = func.blocks[block].instructions.iter().position(|&other| other == inst)?;
            Some((block, index))
        })
    }

    /// Whether a block outside the loop reads `value`.
    fn read_after_loop(func: &Function, loop_data: &Loop, value: ValueId) -> bool {
        func.blocks.iter_enumerated().any(|(block_id, block)| {
            !loop_data.blocks.contains(block_id)
                && (block.instructions.iter().any(|&inst_id| func.inst(inst_id).kind.reads(value))
                    || block.terminator.as_ref().is_some_and(|term| term.reads(value)))
        })
    }

    /// Replaces every use of the keys of `replacements` in `blocks` with their values.
    fn replace_block_uses(
        func: &mut Function,
        blocks: impl Iterator<Item = BlockId>,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) {
        for block in blocks {
            for index in 0..func.blocks[block].instructions.len() {
                let inst_id = func.blocks[block].instructions[index];
                mir_utils::replace_inst_uses(func.inst_mut(inst_id), replacements);
            }
            if let Some(term) = &mut func.blocks[block].terminator {
                mir_utils::replace_terminator_uses(term, replacements);
            }
        }
    }

    /// Rewrites the address `value` in place to `pointer + offset`.
    fn rewrite_sibling(&self, func: &mut Function, value: ValueId, pointer: ValueId, offset: i128) {
        let Value::Inst(inst_id) = *func.value(value) else { return };
        let Some(magnitude) = offset.checked_abs() else { return };
        let Some(magnitude) = self.offset_value(func, magnitude) else { return };
        func.inst_mut(inst_id).kind = offset_kind(pointer, magnitude, offset >= 0);
    }

    /// Appends `delta / magnitude` to `block`: a shift for a power of two, a division otherwise.
    fn append_steps(
        &self,
        func: &mut Function,
        block: BlockId,
        delta: ValueId,
        magnitude: u128,
    ) -> ValueId {
        if magnitude == 1 {
            delta
        } else if magnitude.is_power_of_two() {
            let shift = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(
                magnitude.trailing_zeros(),
            ))));
            self.append_inst_value(func, block, InstKind::Shr(shift, delta), Some(MirType::I256))
        } else {
            let divisor =
                func.alloc_value(Value::Immediate(Immediate::I256(U256::from(magnitude))));
            self.append_inst_value(func, block, InstKind::Div(delta, divisor), Some(MirType::I256))
        }
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
        let kind = offset_kind(value, magnitude, offset > 0);
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

    fn value_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        i128::try_from(func.value_u256(value)?).ok()
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
}

/// The instruction `base + magnitude`, or `base - magnitude` when `add` is false.
fn offset_kind(base: ValueId, magnitude: ValueId, add: bool) -> InstKind {
    if add { InstKind::Add(base, magnitude) } else { InstKind::Sub(base, magnitude) }
}

/// How deep [`bounded_on_entry`] follows a branch condition's operands.
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
            unsigned_bounds(func, x).1 <= limit
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

/// The value the header phi `phi` takes on entry from `preheader`.
fn preheader_value(func: &Function, phi: ValueId, preheader: BlockId) -> Option<ValueId> {
    let Value::Inst(inst_id) = *func.value(phi) else { return None };
    let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { return None };
    incoming.iter().find_map(|&(from, value)| (from == preheader).then_some(value))
}

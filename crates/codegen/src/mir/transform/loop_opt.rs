//! Loop-invariant code motion (`licm`).
//!
//! Moves instructions whose operands do not change inside a natural loop to the loop's
//! preheader, so each runs once per entry instead of once per iteration. The pass needs the
//! dedicated preheader `loop-canonicalize` inserts, and moves an instruction together with its
//! in-loop operands, in dependence order, as one closure.
//!
//! Safety. An instruction moves only when running it early, and possibly on a zero-trip
//! entry, is unobservable. Pure operations move freely. Memory, storage, transient and
//! immutable reads move only when no loop instruction may write what they read, judged by the
//! shared alias analysis with module call summaries, so a call to a memory-clean helper is not
//! a barrier; scalar evolution separates affine element ranges of one base that the
//! value-local analysis cannot. Every read additionally needs an execution guarantee: its block
//! dominates every live exit, or the loop runs a verified first iteration that reaches it, and
//! no semantic check or call that may exit without a CFG edge runs before it in the loop.
//! A semantic length read of an existing heap object (a fresh allocation or an object
//! argument) reads allocated memory and needs no execution guarantee. Fixed-width slot hashes
//! move with their scratch writes only when nothing else in the function touches scratch.
//! Environment reads that calls or creates change (balances, code size and hash, return data
//! size) stay in loops that call or create, and nothing moves out of a loop that reads the
//! remaining gas. Frame loads and the free memory pointer never move.
//!
//! Profitability. An instruction is a hoisting root when its estimated per-iteration saving
//! reaches the pass threshold, or when it is the invariant base of an affine address the loop
//! reads or writes. In gas mode, pure word arithmetic that executes on every iteration is a
//! root as well: the backend carries a preheader value on the stack through the loop or
//! rebuilds it at its uses, so hoisting only widens its choice. Those below-threshold hoists
//! are held to a budget of loop-carried words, the header's phis plus the values the loop reads
//! from outside, computed per loop of the nest containing the
//! instruction and updated as roots are accepted: past the budget, the backend's layouts spill loop
//! values to frame slots, and a hoisted base costs more in reloads of the words it displaces than
//! its in-loop recomputation saved. At most a fixed number of instructions leave one loop.
//!
//! The pass runs once on the semantic MIR and once more after memory lowering,
//! which materializes each element access as `add base, 32` plus an index term inside the
//! loop that reads it; the late run hoists that base so a hot loop carries one word instead
//! of reloading its argument and re-adding the header on every iteration. An access at an index
//! the frontend spells first, `(base + index term) + 32`, is reassociated in gas mode into
//! `(base + 32) + index term`, with `base + 32` computed once in the preheader of the outermost
//! loop `base` is fixed in, together with every such sum that preheader dominates, when those
//! sums are the base's last readers: the new word then replaces the base among those the loops
//! carry. A heap's sift, a sort's partition scan, or a hash table's probe, whose index no affine
//! recurrence of the loop describes, saves one addition per access; an affine index stays for
//! `indvar-simplify`, which steps a pointer instead.
//!
//! An unsigned checked product inside a loop whose factor `y` is defined before
//! the loop, lowered as `y == 0 || (x * y) / y == x` or, with `y` first, as a check that
//! divides by `x`, becomes `x <= MAX / y - (y == 0)`, with the bound computed once in the
//! preheader. A nonzero `y` keeps the product below `2^256`
//! exactly when `x <= MAX / y`. A zero `y` turns the bound into `0 - 1 = MAX`, which every
//! `x` meets. Each iteration then compares instead of dividing. The rewrite is skipped in loops
//! that check the gas left, since the bound runs first.
//!
//! When the check only feeds its branch, the comparison becomes the branch condition itself, with
//! the edges swapped, so no `iszero` follows it and `merge-aborts` can join it with a following
//! abort test. No later pass flips a branch on `eq overflows, 0`. In size mode the bound is one
//! more word the loop carries, and its saving depends on the stack schedule, not on the opcodes: a
//! loop carrying seven words grew by two bytes, while loops carrying two and four shrank by one and
//! four. Size mode therefore rewrites a factor's checks only while every loop around them stays
//! within [`LOOP_CARRY_BUDGET`] with the bound.

use crate::mir::{
    BlockId, EffectKind, Function, FunctionBuilder, Immediate, ImmutableId, InstId, InstKind,
    Instruction, MirType, Module, OpTraits, PtrLocation, StorageAlias, Terminator, Value, ValueId,
    analysis::{
        AddressSpace, AffineExpr, AliasAnalysis, AliasResult, CfgInfo, DominatorTree, Location,
        LocationSize, Loop, LoopAnalyzer, MemoryAddress, MemoryLocation, ModRef, ScalarEvolution,
    },
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
    utils as mir_utils,
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use std::{cmp::Reverse, rc::Rc};

/// Most words a loop may carry after a below-threshold hoist or, in size mode, a product check's
/// hoisted bound. The count is the header's phis plus the values defined outside the loop that
/// its body reads, which the backend keeps on the stack through the loop when they fit. Past this
/// many, its layouts spill loop values to frame slots, and a hoisted base then costs more in
/// reloads of the words it displaced than its in-loop recomputation saved: the hash-probe loop of
/// `hasDuplicate` lost 2.4% carrying eight, while `reverse` with four gained 8%.
const LOOP_CARRY_BUDGET: usize = 5;

/// Function pass for loop-invariant code motion.
pub(crate) struct Licm;

impl MirPass for Licm {
    fn name(&self) -> &'static str {
        "licm"
    }

    fn run_pass(
        &self,
        gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        let mut selected = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if func.blocks.is_empty() {
                continue;
            }
            if !analyses.cfg(id, func).cyclic_blocks().is_empty() {
                selected.insert(id);
            }
        }
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let mut optimizer = LoopOptimizer {
                    hoist_cheap: analyses.runtime_target(gcx).optimization().is_gas(),
                    alias: Rc::clone(analyses.alias()),
                    hoisted: 0,
                    annotated_aliases: false,
                };
                let changed = optimizer.optimize(func, Rc::clone(analyses.cfg())) != 0;
                if optimizer.annotated_aliases {
                    analyses.note_unreported_edit();
                }
                changed
            },
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AffineRange {
    base: Option<ValueId>,
    start: i128,
    end: i128,
}

/// What a hoist candidate's load reads: its extent and, when affine, its range.
#[derive(Clone, Copy)]
struct LoadRange {
    location: Option<MemoryLocation>,
    affine: Option<AffineRange>,
}

#[derive(Clone, Copy)]
struct LoopOptContext<'a> {
    loop_data: &'a Loop,
    scev: &'a ScalarEvolution,
    dominators: &'a DominatorTree,
    inst_blocks: &'a IndexVec<InstId, Option<BlockId>>,
    /// The loop's blocks from which it can actually exit.
    exiting: &'a [BlockId],
}

impl LoopOptContext<'_> {
    /// The loop block holding `inst`, if the loop holds it.
    fn loop_block(&self, inst: InstId) -> Option<BlockId> {
        self.inst_blocks[inst].filter(|&block| self.loop_data.blocks.contains(block))
    }

    /// Whether the loop block holding `inst` dominates every back edge.
    fn dominates_backedges(&self, inst: InstId) -> bool {
        let Some(block) = self.loop_block(inst) else { return false };
        self.loop_data.back_edges.iter().all(|&latch| self.dominators.dominates(block, latch))
    }
}

/// Sets and lists that each loop clears and refills instead of reallocating.
struct Scratch {
    selected: DenseBitSet<InstId>,
    closure_set: DenseBitSet<InstId>,
    visiting: DenseBitSet<InstId>,
    closure: Vec<InstId>,
    values: DenseBitSet<ValueId>,
}

impl Scratch {
    fn new() -> Self {
        Self {
            selected: DenseBitSet::new_empty(0),
            closure_set: DenseBitSet::new_empty(0),
            visiting: DenseBitSet::new_empty(0),
            closure: Vec::new(),
            values: DenseBitSet::new_empty(0),
        }
    }
}

/// Minimum estimated gas saved per iteration before an instruction is considered a LICM root.
const MIN_LICM_PROFIT: u16 = 3;

/// Maximum number of instructions hoisted from one loop.
const MAX_LICM_HOISTED_INSTS: usize = 8;

/// Loop optimizer.
struct LoopOptimizer {
    /// Whether pure arithmetic executed on every iteration is hoisted even when its per-iteration
    /// saving is below [`MIN_LICM_PROFIT`]. The backend carries a preheader value on the stack
    /// through a loop or rebuilds it at its use sites, whichever its pricing prefers, so
    /// hoisting only widens its choice; the cost is bytecode when the value is spilled, which
    /// gas mode accepts.
    hoist_cheap: bool,
    alias: Rc<AliasAnalysis>,
    /// Number of instructions hoisted or rewritten.
    hoisted: usize,
    /// Whether storage-alias annotation changed metadata, which is not reported as a change.
    annotated_aliases: bool,
}

impl LoopOptimizer {
    /// Runs loop-invariant code motion on a function and returns the number of instructions
    /// hoisted or rewritten.
    fn optimize(&mut self, func: &mut Function, cfg: Rc<CfgInfo>) -> usize {
        self.annotated_aliases |=
            func.annotate_storage_aliases(mir_utils::StorageAliasScope::StorageAndTransient);

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze_with_cfg(func, Rc::clone(&cfg));

        if loop_info.loops.is_empty() {
            return 0;
        }

        let loops = loop_info.loops.values().cloned().collect::<Vec<_>>();
        let mut scratch = Scratch::new();
        for loop_data in &loops {
            // The product's limit runs before the loop, so whatever in the loop checks the gas
            // left would see what the limit cost before the first check paid for it.
            if !self.loop_checks_gas_left(func, loop_data) {
                self.hoisted += reduce_product_checks(
                    func,
                    loop_data,
                    &loops,
                    self.hoist_cheap,
                    &mut scratch.values,
                );
            }
        }
        let inst_blocks = func.inst_block_table();
        let mut carried = loops
            .iter()
            .map(|loop_data| {
                let words = Self::carried_words(func, loop_data, &inst_blocks, &mut scratch.values);
                (loop_data.header, words)
            })
            .collect::<FxHashMap<_, _>>();
        if self.hoist_cheap {
            // Outer loops first, so a sum an inner loop shares with its outer loop is rewritten
            // with the outer loop's view of the index.
            let mut nest = loops.iter().collect::<Vec<_>>();
            nest.sort_by_key(|loop_data| Reverse(loop_data.blocks.count()));
            for loop_data in nest {
                self.hoisted +=
                    reassociate_invariant_offsets(func, loop_data, &loops, cfg.dominators());
            }
        }
        for loop_data in &loops {
            self.apply_licm(func, loop_data, cfg.dominators(), &loops, &mut carried, &mut scratch);
        }

        self.hoisted
    }

    /// The words the backend carries through a loop: the header's phis and the values defined
    /// outside the loop that its non-phi instructions read. Immediates, arguments, and nullary
    /// rematerializable reads are rebuilt where used and cost no word.
    fn carried_words(
        func: &Function,
        loop_data: &Loop,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        seen: &mut DenseBitSet<ValueId>,
    ) -> usize {
        let header = &func.blocks[loop_data.header];
        let mut count = header
            .instructions
            .iter()
            .filter(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .count();
        seen.clear_to(func.num_values());
        let mut visit = |operand| {
            if Self::is_carried_operand(func, loop_data, inst_blocks, operand)
                && seen.insert(operand)
            {
                count += 1;
            }
        };
        for block in loop_data.blocks.iter() {
            let block = &func.blocks[block];
            for &inst_id in &block.instructions {
                let kind = &func.inst(inst_id).kind;
                if !matches!(kind, InstKind::Phi(_)) {
                    kind.visit_operands(&mut visit);
                }
            }
            if let Some(term) = &block.terminator {
                term.visit_operands(&mut visit);
            }
        }
        count
    }

    /// Whether a value read inside `loop_data` occupies a carried word: an instruction result
    /// outside the loop that is not a nullary rematerializable read.
    fn is_carried_operand(
        func: &Function,
        loop_data: &Loop,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        value: ValueId,
    ) -> bool {
        let Value::Inst(inst_id) = func.value(value) else { return false };
        let kind = &func.inst(*inst_id).kind;
        inst_blocks
            .get(*inst_id)
            .copied()
            .flatten()
            .is_none_or(|block| !loop_data.blocks.contains(block))
            && !(kind.operands().is_empty()
                && kind.op_def().traits.contains(OpTraits::REMATERIALIZABLE))
    }

    /// The change in carried words of loop `inner` (the hoisting loop or one nested in it) when
    /// `closure` moves to the preheader: every closure result still read elsewhere becomes a
    /// carried word, and every outside operand the closure was the loop's only reader of stops
    /// being one.
    fn carried_delta(
        func: &Function,
        inner: &Loop,
        closure: &[InstId],
        closure_set: &DenseBitSet<InstId>,
        inst_blocks: &IndexVec<InstId, Option<BlockId>>,
        released: &mut DenseBitSet<ValueId>,
    ) -> isize {
        let reads_outside_closure = |value: ValueId, block: BlockId| {
            let block = &func.blocks[block];
            block.instructions.iter().any(|&inst_id| {
                !closure_set.contains(inst_id) && func.inst(inst_id).kind.reads(value)
            }) || block.terminator.as_ref().is_some_and(|term| term.reads(value))
        };
        let mut delta = 0isize;
        for &inst_id in closure {
            if let Some(result) = func.inst_result_value(inst_id)
                && func.blocks.indices().any(|block| reads_outside_closure(result, block))
            {
                delta += 1;
            }
        }
        released.clear_to(func.num_values());
        for operand in closure.iter().flat_map(|&inst_id| func.inst(inst_id).kind.operands()) {
            if Self::is_carried_operand(func, inner, inst_blocks, operand)
                && !released.contains(operand)
                && !inner.blocks.iter().any(|block| reads_outside_closure(operand, block))
            {
                released.insert(operand);
                delta -= 1;
            }
        }
        delta
    }

    fn apply_licm(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        dominators: &DominatorTree,
        loops: &[Loop],
        carried: &mut FxHashMap<BlockId, usize>,
        scratch: &mut Scratch,
    ) {
        let Some(preheader) = loop_data.preheader else { return };
        if loop_data.invariant_insts.is_empty()
            || self.loop_contains(func, loop_data, InstKind::observes_gas)
        {
            return;
        }

        let scev = ScalarEvolution::analyze(func, loop_data);
        let inst_blocks = func.inst_block_table();
        let exiting = Self::live_exiting_blocks(func, loop_data);
        let ctx = LoopOptContext {
            loop_data,
            scev: &scev,
            dominators,
            inst_blocks: &inst_blocks,
            exiting: &exiting,
        };
        let mut roots: Vec<InstId> = loop_data
            .invariant_insts
            .iter()
            .filter(|&inst_id| {
                self.can_hoist_safely(func, inst_id, ctx)
                    && self.is_profitable_licm_root(func, inst_id, ctx)
            })
            .collect();
        if roots.is_empty() {
            return;
        }
        roots.sort_unstable_by_key(|&inst_id| {
            (Reverse(self.licm_profit(func, inst_id)), inst_id.index())
        });

        let Scratch { selected, closure_set, visiting, closure, values } = scratch;
        selected.clear_to(func.num_insts());
        closure_set.clear_to(func.num_insts());
        visiting.clear_to(func.num_insts());
        for root in roots {
            closure.clear();
            visiting.clear();
            if !self.collect_hoist_closure(func, root, ctx, selected, visiting, closure) {
                continue;
            }

            let new_count = closure.iter().filter(|&&inst_id| !selected.contains(inst_id)).count();
            if selected.count() + new_count > MAX_LICM_HOISTED_INSTS {
                continue;
            }
            // The root leaves every loop of this nest that contains it; each of those loops
            // carries the closure's results from now on. A hoist that only pays through the
            // backend's residency must keep every one of them within the carry budget.
            closure_set.clear();
            for &inst_id in closure.iter() {
                closure_set.insert(inst_id);
            }
            let Some(root_block) = inst_blocks.get(root).copied().flatten() else { continue };
            let nest = loops
                .iter()
                .filter(|inner| {
                    inner.blocks.contains(root_block) && loop_data.blocks.superset(&inner.blocks)
                })
                .map(|inner| {
                    (
                        inner.header,
                        Self::carried_delta(
                            func,
                            inner,
                            closure,
                            closure_set,
                            &inst_blocks,
                            values,
                        ),
                    )
                })
                .collect::<Vec<_>>();
            if self.licm_profit(func, root) < MIN_LICM_PROFIT
                && nest.iter().any(|&(header, delta)| {
                    carried[&header].saturating_add_signed(delta) > LOOP_CARRY_BUDGET
                })
            {
                continue;
            }
            for (header, delta) in nest {
                let count = carried.get_mut(&header).expect("every loop has a carried count");
                *count = count.saturating_add_signed(delta);
            }
            for &inst_id in closure.iter() {
                selected.insert(inst_id);
            }
        }

        if selected.is_empty() {
            return;
        }

        visiting.clear();
        let ordered = self.topological_sort_instructions(func, selected, visiting);

        for inst_id in ordered {
            // An enclosing loop's earlier hoist may have already moved the
            // instruction out of these blocks; pushing it again would schedule
            // the same instruction in two blocks.
            if let Some(block) = ctx.loop_block(inst_id) {
                func.blocks[block].instructions.retain(|&id| id != inst_id);
                func.blocks[preheader].instructions.push(inst_id);
                self.hoisted += 1;
            }
        }
    }

    fn collect_hoist_closure(
        &self,
        func: &Function,
        inst_id: InstId,
        ctx: LoopOptContext<'_>,
        selected: &DenseBitSet<InstId>,
        visiting: &mut DenseBitSet<InstId>,
        out: &mut Vec<InstId>,
    ) -> bool {
        if selected.contains(inst_id) {
            return true;
        }
        if out.contains(&inst_id) {
            return true;
        }
        if !visiting.insert(inst_id) {
            return false;
        }
        if !self.can_hoist_safely(func, inst_id, ctx) {
            return false;
        }

        let inst = func.inst(inst_id);
        for operand in inst.kind.operands() {
            if let Value::Inst(dep_inst) = func.value(operand)
                && ctx.loop_block(*dep_inst).is_some()
                && !self.collect_hoist_closure(func, *dep_inst, ctx, selected, visiting, out)
            {
                return false;
            }
        }

        out.push(inst_id);
        true
    }

    fn can_hoist_safely(&self, func: &Function, inst_id: InstId, ctx: LoopOptContext<'_>) -> bool {
        let inst = func.inst(inst_id);

        if matches!(inst.kind, InstKind::StorageArrayDataSlot(_) | InstKind::MappingSlot(..)) {
            return self.slot_hash_scratch_is_private(func, inst_id)
                && self.hoist_execution_guaranteed(func, inst_id, ctx);
        }
        if inst.kind.has_side_effects() {
            return false;
        }
        if matches!(inst.kind, InstKind::Phi(_)) {
            return false;
        }
        match inst.kind {
            // Hoisting memory reads expands memory earlier (and unconditionally); on top of the
            // dependence checks they must also be guaranteed to execute so a zero-trip loop
            // cannot start trapping (OOG from speculated memory expansion) or paying for work
            // it never did.
            InstKind::Load(addr, PtrLocation::Memory) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_memory_range(func, ctx, addr, Some(32));
            }
            // A frame slot's address is implicit until frame lowering. Keep the load in place so a
            // store in the loop cannot be missed by the dependence check above.
            InstKind::FrameLoad { .. } => return false,
            InstKind::Keccak256(offset, size) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_memory_range(func, ctx, offset, func.value_u64(size));
            }
            InstKind::Load(slot, location @ (PtrLocation::Storage | PtrLocation::Transient)) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_storage_slot(func, ctx, inst_id, slot, location);
            }
            InstKind::LoadImmutable(id) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_assign_immutable(func, ctx.loop_data, id);
            }
            // The free memory pointer moves with every allocation.
            InstKind::Fmp => return false,
            // Every call or create rewrites the return-data buffer. Also require guaranteed
            // execution here and below: a zero-trip loop must not pay for a speculated read,
            // and a cold account read costs as much as a storage read.
            InstKind::ReturnDataSize => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_contains(func, ctx.loop_data, |kind| {
                        matches!(
                            kind.effect_kind(),
                            EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
                        )
                    });
            }
            // Balances move with value transfers, and code changes on deploy and selfdestruct.
            _ if inst.kind.reads_account_state() => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_contains(
                        func,
                        ctx.loop_data,
                        InstKind::may_change_account_state,
                    );
            }
            _ => {}
        }
        inst.kind.effects().can_speculate()
    }

    /// Returns true if hoisting `inst_id` into the preheader cannot make it execute when the
    /// original loop would not have executed it.
    ///
    /// This holds when the instruction's block dominates every (live) exiting block, or when
    /// the loop is known to complete at least one iteration that executes the instruction:
    /// a verified trip count of at least one, a single exiting block (so the trip-count guard
    /// is the only way out), and the instruction dominating every backedge.
    fn hoist_execution_guaranteed(
        &self,
        func: &Function,
        inst_id: InstId,
        ctx: LoopOptContext<'_>,
    ) -> bool {
        let loop_data = ctx.loop_data;
        let Some(inst_block) = ctx.loop_block(inst_id) else { return false };

        // Semantic checks and calls can exit without a CFG edge. The candidate must execute
        // before each such operation, including those earlier in its own block.
        for block_id in &loop_data.blocks {
            if block_id != inst_block && ctx.dominators.dominates(inst_block, block_id) {
                continue;
            }
            if func.blocks[block_id]
                .instructions
                .iter()
                .take_while(|&&other| other != inst_id)
                .any(|&other| func.inst(other).kind.effects().control.any())
            {
                return false;
            }
        }

        // No live exit means the loop only terminates by running out of gas,
        // which consumes the entire gas budget regardless of what executes
        // beforehand, so any placement is observationally equivalent.
        if ctx.exiting.is_empty() {
            return true;
        }
        if ctx.exiting.iter().all(|&block| ctx.dominators.dominates(inst_block, block)) {
            return true;
        }

        loop_data.trip_count.is_some_and(|trip| trip >= 1)
            && ctx.exiting.len() == 1
            && loop_data.back_edges.iter().all(|&latch| ctx.dominators.dominates(inst_block, latch))
    }

    /// Returns the in-loop blocks from which the loop can actually exit.
    ///
    /// Branches whose condition is a constant that always picks the in-loop successor cannot
    /// leave the loop and are ignored.
    fn live_exiting_blocks(func: &Function, loop_data: &Loop) -> Vec<BlockId> {
        let mut exiting = Vec::new();
        for block_id in &loop_data.blocks {
            let Some(term) = &func.blocks[block_id].terminator else { continue };
            let escapes = match term {
                Terminator::Branch { condition, then_block, else_block } => {
                    match func.value_u256(*condition).map(|condition| !condition.is_zero()) {
                        Some(true) => !loop_data.blocks.contains(*then_block),
                        Some(false) => !loop_data.blocks.contains(*else_block),
                        None => {
                            !loop_data.blocks.contains(*then_block)
                                || !loop_data.blocks.contains(*else_block)
                        }
                    }
                }
                _ => term.successors().iter().any(|&succ| !loop_data.blocks.contains(succ)),
            };
            if escapes {
                exiting.push(block_id);
            }
        }
        exiting
    }

    fn loop_contains(
        &self,
        func: &Function,
        loop_data: &Loop,
        predicate: impl Fn(&InstKind) -> bool,
    ) -> bool {
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id]
                .instructions
                .iter()
                .any(|&inst_id| predicate(&func.inst(inst_id).kind))
        })
    }

    fn licm_profit(&self, func: &Function, inst_id: InstId) -> u16 {
        match func.inst(inst_id).kind {
            InstKind::Load(_, PtrLocation::Storage | PtrLocation::Transient) => 100,
            InstKind::Keccak256(_, _) => 30,
            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotBytes(_, _)
            | InstKind::StorageArrayDataSlot(_)
            | InstKind::StorageArrayElementSlot { .. } => 30,
            InstKind::Exp(_, _) => 10,
            InstKind::Mul(_, _)
            | InstKind::Div(_, _)
            | InstKind::SDiv(_, _)
            | InstKind::Mod(_, _)
            | InstKind::SMod(_, _)
            | InstKind::AddMod(_, _, _)
            | InstKind::MulMod(_, _, _)
            | InstKind::Clz(_) => 5,
            InstKind::Load(..) => 3,
            _ => 0,
        }
    }

    fn is_profitable_licm_root(
        &self,
        func: &Function,
        inst_id: InstId,
        ctx: LoopOptContext<'_>,
    ) -> bool {
        self.licm_profit(func, inst_id) >= MIN_LICM_PROFIT
            || (ctx.loop_data.trip_count.is_some_and(|trip_count| trip_count > 1)
                && self.is_affine_address_base_used_in_loop(func, inst_id, ctx))
            || (ctx.dominates_backedges(inst_id)
                && (self.is_affine_address_base_used_in_loop(func, inst_id, ctx)
                    || self.is_cheap_invariant_arithmetic(func, inst_id)))
    }

    /// Whether `inst_id` is pure word arithmetic whose result the backend can carry or rebuild:
    /// no memory or storage read, no side effect, and a result value. Hoisted only when it runs
    /// on every iteration, so the preheader never pays for work a taken exit would have skipped.
    fn is_cheap_invariant_arithmetic(&self, func: &Function, inst_id: InstId) -> bool {
        let inst = func.inst(inst_id);
        self.hoist_cheap
            && inst.kind.effect_kind() == EffectKind::Pure
            && func.inst_result_value(inst_id).is_some()
    }

    /// Whether the loop checks the gas left: a `gas` reading or a call that observes it, or a
    /// persistent storage write, which fails when its sentry finds 2300 gas or less.
    fn loop_checks_gas_left(&self, func: &Function, loop_data: &Loop) -> bool {
        let aa = &self.alias;
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                let mod_ref = aa.instruction_mod_ref(func, inst_id);
                func.inst(inst_id).kind.observes_gas()
                    || mod_ref.observes_gas()
                    || mod_ref.writes_space(AddressSpace::Storage)
            })
        })
    }

    fn loop_may_mutate_memory_range(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_addr: ValueId,
        load_width: Option<u64>,
    ) -> bool {
        let aa = &self.alias;
        // The hoist candidate's address is loop-invariant, so its extent and affine range hold
        // for every store the loop makes.
        let load = load_width.map(|width| LoadRange {
            location: aa.bare_memory_location(func, load_addr, LocationSize::Const(width)),
            affine: self.affine_range(func, ctx, load_addr, width, None),
        });
        for block_id in &ctx.loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match func.inst(inst_id).kind {
                    InstKind::Store(addr, _, PtrLocation::Memory) => {
                        if self.memory_ranges_may_alias(func, ctx, load, addr, 32, block_id) {
                            return true;
                        }
                    }
                    InstKind::Store8(addr, _) => {
                        if self.memory_ranges_may_alias(func, ctx, load, addr, 1, block_id) {
                            return true;
                        }
                    }
                    _ if aa
                        .instruction_mod_ref(func, inst_id)
                        .writes_space(AddressSpace::Memory) =>
                    {
                        return true;
                    }
                    _ => {}
                }
            }
        }
        false
    }

    fn loop_may_mutate_storage_slot(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_inst: InstId,
        load_slot: ValueId,
        location: PtrLocation,
    ) -> bool {
        let aa = &self.alias;
        let Some(load_alias) = self.storage_alias_for_loop_value(func, load_inst, load_slot, ctx)
        else {
            return true;
        };
        let Some(load) = Location::slot(location, load_alias) else { return true };
        if !Self::can_use_storage_alias_for_licm(load_alias, ctx.loop_data) {
            return true;
        }

        for block_id in &ctx.loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match &func.inst(inst_id).kind {
                    InstKind::Store(slot, _, store_location) if *store_location == location => {
                        let Some(store_alias) =
                            self.storage_alias_for_loop_value(func, inst_id, *slot, ctx)
                        else {
                            return true;
                        };
                        if !Self::can_use_storage_alias_for_licm(store_alias, ctx.loop_data) {
                            return true;
                        }
                        let Some(store) = Location::slot(location, store_alias) else {
                            return true;
                        };
                        if aa.alias(load, store).may_alias() {
                            return true;
                        }
                    }
                    _ => {
                        if aa.instruction_mod_ref(func, inst_id).may_write(aa, load) {
                            return true;
                        }
                    }
                }
            }
        }
        false
    }

    fn loop_may_assign_immutable(
        &self,
        func: &Function,
        loop_data: &Loop,
        load_id: ImmutableId,
    ) -> bool {
        let aa = &self.alias;
        let load = Location::Immutable(load_id);
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id]
                .instructions
                .iter()
                .any(|&inst_id| aa.instruction_mod_ref(func, inst_id).may_write(aa, load))
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn memory_ranges_may_alias(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load: Option<LoadRange>,
        write_addr: ValueId,
        write_width: u64,
        write_block: BlockId,
    ) -> bool {
        let Some(load) = load else { return true };
        let aa = &self.alias;
        if let Some(location) = load.location
            && let Some(write) =
                aa.bare_memory_location(func, write_addr, LocationSize::Const(write_width))
        {
            match aa.memory_alias(func, location, write) {
                AliasResult::NoAlias => return false,
                AliasResult::MustAlias | AliasResult::PartialAlias => return true,
                AliasResult::MayAlias => {}
            }
        }

        // Scalar evolution can prove disjointness for affine loop addresses beyond value-local
        // BasicAA.
        let Some(load) = load.affine else { return true };
        let Some(write) = self.affine_range(func, ctx, write_addr, write_width, Some(write_block))
        else {
            return true;
        };
        if load.base != write.base {
            return true;
        }
        load.start < write.end && write.start < load.end
    }

    fn affine_range(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        value: ValueId,
        width: u64,
        inst_block: Option<BlockId>,
    ) -> Option<AffineRange> {
        let expr = ctx.scev.get(value).cloned().or_else(|| self.const_affine_expr(func, value))?;
        // Non-header blocks only execute after the header guard passed in
        // their iteration, so they observe the induction variable strictly
        // below the bound; everything else (header instructions, deeper
        // guards, unknown position) also runs in the exiting partial
        // iteration and sees one more stride.
        let tight = ctx.loop_data.trip_guard_is_header
            && inst_block.is_some_and(|block| block != ctx.loop_data.header);
        self.affine_expr_range(func, ctx.loop_data, expr, width, tight)
    }

    fn affine_expr_range(
        &self,
        func: &Function,
        loop_data: &Loop,
        expr: AffineExpr,
        width: u64,
        tight: bool,
    ) -> Option<AffineRange> {
        // A scaled invariant has no known range here.
        if !expr.invariants.is_empty() {
            return None;
        }
        let mut start = expr.constant;
        let mut end = expr.constant;
        if !expr.terms.is_empty() {
            let trip_count = i128::from(loop_data.trip_count?);
            if trip_count == 0 {
                return None;
            }
            let strides = if tight { trip_count.checked_sub(1)? } else { trip_count };
            for term in expr.terms {
                let iv = loop_data.induction_vars.iter().find(|iv| iv.value == term.value)?;
                // `last_iv` below assumes the variable grows from `init`; a descending
                // variable instead shrinks (and may wrap), so its range is unknown here.
                if iv.descending {
                    return None;
                }
                let init = self.const_i128(func, iv.init)?;
                let step = self.const_i128(func, iv.step)?;
                let first = init.checked_mul(term.scale)?;
                let last_iv = init.checked_add(step.checked_mul(strides)?)?;
                let last = last_iv.checked_mul(term.scale)?;
                start = start.checked_add(first.min(last))?;
                end = end.checked_add(first.max(last))?;
            }
        }

        Some(AffineRange { base: expr.base, start, end: end.checked_add(i128::from(width))? })
    }

    fn const_affine_expr(&self, func: &Function, value: ValueId) -> Option<AffineExpr> {
        Some(AffineExpr {
            base: None,
            invariants: Default::default(),
            constant: self.const_i128(func, value)?,
            terms: Default::default(),
        })
    }

    fn const_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        i128::try_from(func.value_u256(value)?).ok()
    }

    fn storage_alias_for_loop_value(
        &self,
        func: &Function,
        inst_id: InstId,
        value: ValueId,
        ctx: LoopOptContext<'_>,
    ) -> Option<StorageAlias> {
        let alias = func.storage_alias(inst_id, value);
        if let Some(base) = alias.symbolic_base()
            && Self::value_defined_in_loop(func, base, ctx)
        {
            return None;
        }
        Some(alias)
    }

    fn can_use_storage_alias_for_licm(alias: StorageAlias, loop_data: &Loop) -> bool {
        matches!(alias, StorageAlias::Slot(_)) || loop_data.trip_count.is_some_and(|trip| trip > 1)
    }

    fn value_defined_in_loop(func: &Function, value: ValueId, ctx: LoopOptContext<'_>) -> bool {
        match func.value(value) {
            Value::Inst(inst_id) => ctx.loop_block(*inst_id).is_some(),
            Value::Undef(_) | Value::Error(_) => true,
            Value::Arg(_) | Value::Immediate(_) => false,
        }
    }

    fn is_affine_address_base_used_in_loop(
        &self,
        func: &Function,
        inst_id: InstId,
        ctx: LoopOptContext<'_>,
    ) -> bool {
        let Some(result) = func.inst_result_value(inst_id) else { return false };
        for block_id in &ctx.loop_data.blocks {
            for &user_inst in &func.blocks[block_id].instructions {
                let kind = &func.inst(user_inst).kind;
                let mut address_operands = ArrayVec::<ValueId, 2>::new();
                match kind {
                    // Storage and transient slots are not memory ranges but are checked as
                    // addresses too.
                    InstKind::Load(addr, _) | InstKind::Store(addr, ..) => {
                        address_operands.push(*addr);
                    }
                    InstKind::MappingSlotBytes(key, _)
                        if func.value_ty(*key) == Some(MirType::MEMPTR) =>
                    {
                        address_operands.push(*key);
                    }
                    // Calls, creates, logs, and `MemoryZero` do not count as address uses.
                    InstKind::MemoryZero(..) => continue,
                    _ if kind.effect_kind().is_memory() => {
                        kind.visit_raw_memory(|_, addr, _| address_operands.push(addr));
                    }
                    _ => continue,
                }

                for address in address_operands {
                    if self.value_feeds_affine_address(func, ctx, result, address, 0) {
                        return true;
                    }
                }
            }
        }
        false
    }

    fn value_feeds_affine_address(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        needle: ValueId,
        value: ValueId,
        depth: usize,
    ) -> bool {
        if value == needle {
            return true;
        }
        if depth >= 4 || ctx.scev.get(value).is_none() {
            return false;
        }

        let Value::Inst(inst_id) = func.value(value) else { return false };
        if ctx.loop_block(*inst_id).is_none() {
            return false;
        }
        func.inst(*inst_id)
            .kind
            .operands()
            .iter()
            .copied()
            .any(|operand| self.value_feeds_affine_address(func, ctx, needle, operand, depth + 1))
    }

    fn topological_sort_instructions(
        &self,
        func: &Function,
        inst_set: &DenseBitSet<InstId>,
        visited: &mut DenseBitSet<InstId>,
    ) -> Vec<InstId> {
        let mut result = Vec::new();

        fn visit(
            func: &Function,
            inst_id: InstId,
            inst_set: &DenseBitSet<InstId>,
            visited: &mut DenseBitSet<InstId>,
            result: &mut Vec<InstId>,
        ) {
            if !visited.insert(inst_id) {
                return;
            }

            let inst = func.inst(inst_id);
            inst.kind.visit_operands(|operand| {
                if let Value::Inst(dep_inst) = func.value(operand)
                    && inst_set.contains(*dep_inst)
                {
                    visit(func, *dep_inst, inst_set, visited, result);
                }
            });
            result.push(inst_id);
        }

        for inst_id in inst_set.iter() {
            visit(func, inst_id, inst_set, visited, &mut result);
        }

        result
    }

    /// Moving a fixed-width hash also moves its scratch writes and memory expansion. Require
    /// no other observer or clobber of scratch in the whole function, including calls; retain
    /// the existing must-execute proof so zero-trip loops do not acquire a hash.
    fn slot_hash_scratch_is_private(&self, func: &Function, candidate: InstId) -> bool {
        let aa = &self.alias;
        let scratch = Location::Memory(MemoryLocation::new(
            MemoryAddress::absolute(0),
            LocationSize::Const(crate::mir::memory::EvmMemoryLayout::FMP_SLOT),
        ));
        let private = |effects: ModRef| {
            !effects.observes_gas()
                && !effects.may_read(aa, scratch)
                && !effects.may_write(aa, scratch)
        };
        func.instructions()
            .filter(|&inst| inst != candidate)
            .all(|inst| private(aa.instruction_mod_ref(func, inst)))
            && func
                .blocks
                .iter()
                .filter_map(|block| block.terminator.as_ref())
                .all(|terminator| private(aa.terminator_mod_ref(func, terminator)))
    }
}

/// Rewrites each check of a product `x * y` with a factor `y` defined before the loop, lowered as
/// `d == 0 || (x * y) / d == q` for either order of the factors, into `x <= MAX / y - (y == 0)`,
/// computing the bound once in the preheader. Returns the number of checks rewritten.
fn reduce_product_checks(
    func: &mut Function,
    loop_data: &Loop,
    loops: &[Loop],
    gas: bool,
    seen: &mut DenseBitSet<ValueId>,
) -> usize {
    let Some(preheader) = loop_data.preheader else { return 0 };
    let inst_blocks = func.inst_block_table();
    let outside = |func: &Function, value: ValueId| match *func.value(value) {
        Value::Arg(_) => true,
        Value::Inst(inst) => {
            inst_blocks.get(inst).copied().flatten().is_some_and(|b| !loop_data.blocks.contains(b))
        }
        _ => false,
    };
    let mut checks = Vec::new();
    for block in loop_data.blocks.iter() {
        for &inst in &func.blocks[block].instructions {
            let InstKind::Or(a, b) = func.inst(inst).kind else { continue };
            let Some((divisor, other, zero)) =
                product_check(func, a, b).or_else(|| product_check(func, b, a))
            else {
                continue;
            };
            // Overflow does not depend on which factor the check divides by.
            if outside(func, divisor) {
                checks.push((block, inst, other, divisor, Some(zero)));
            } else if outside(func, other) {
                checks.push((block, inst, divisor, other, None));
            }
        }
    }
    if !gas && !checks.is_empty() {
        // The bound is carried through every loop from this one down to each of its checks.
        let mut worst = FxHashMap::<ValueId, usize>::default();
        for &(block, _, _, y, _) in &checks {
            let carried = loops
                .iter()
                .filter(|inner| {
                    loop_data.blocks.contains(inner.header) && inner.blocks.contains(block)
                })
                .map(|inner| LoopOptimizer::carried_words(func, inner, &inst_blocks, seen))
                .max()
                .unwrap_or_default();
            let worst = worst.entry(y).or_default();
            *worst = (*worst).max(carried);
        }
        let mut bounds = 0;
        let mut admitted = FxHashMap::default();
        checks.retain(|&(.., y, _)| {
            *admitted.entry(y).or_insert_with(|| {
                let fits = worst[&y] + bounds < LOOP_CARRY_BUDGET;
                bounds += usize::from(fits);
                fits
            })
        });
    }
    if checks.is_empty() {
        return 0;
    }
    let uses = super::egraph::use_counts(func);
    let mut limits = FxHashMap::default();
    for &(block, inst, x, y, zero) in &checks {
        let context = func.inst(inst).metadata.debug_context();
        let limit = *limits.entry(y).or_insert_with(|| {
            // preheader: zero = eq y, 0
            //            limit = sub (div MAX, y), (zext zero)
            let mut builder = FunctionBuilder::new(func);
            builder.switch_to_block(preheader);
            builder.set_debug_context(&context);
            let zero = match zero {
                Some(zero) if outside(builder.func(), zero) => zero,
                _ => builder.eq_zero(y),
            };
            let max = builder.imm(U256::MAX);
            let quotient = builder.div(max, y);
            let wide = builder.cast(zero, MirType::I256);
            builder.sub(quotient, wide)
        });
        if flip_check(func, block, inst, &uses) {
            // overflows = gt x, limit
            // jumpi overflows, abort, next
            func.inst_mut(inst).kind = InstKind::Gt(x, limit);
            continue;
        }
        // overflows = gt x, limit
        let mut overflows = Instruction::new(InstKind::Gt(x, limit), Some(MirType::I1));
        overflows.metadata.copy_debug_context(&func.inst(inst).metadata);
        let (overflows_inst, overflows) = func.alloc_value_inst(overflows);
        let position = func.blocks[block].instructions.iter().position(|&id| id == inst);
        let position = position.expect("the check is in its block");
        func.blocks[block].instructions.insert(position, overflows_inst);
        // valid = eq overflows, 0
        let zero = func.alloc_value(Value::Immediate(Immediate::I1(false)));
        func.inst_mut(inst).kind = InstKind::Eq(overflows, zero);
    }
    checks.len()
}

/// Swaps the edges of the branch that tests only `inst`, so the check's result becomes the
/// condition that aborts.
fn flip_check(
    func: &mut Function,
    block: BlockId,
    inst: InstId,
    uses: &IndexVec<ValueId, u32>,
) -> bool {
    let Some(check) = func.inst_result_value(inst) else { return false };
    if uses[check] != 1 {
        return false;
    }
    let Some(Terminator::Branch { condition, then_block, else_block }) =
        &mut func.blocks[block].terminator
    else {
        return false;
    };
    if *condition != check {
        return false;
    }
    std::mem::swap(then_block, else_block);
    true
}

/// Matches `d == 0` and `(x * y) / d == q`, where `d` and `q` are the factors `x` and `y` in
/// either order, as the two operands of a product's check, returning `d`, `q`, and the zero test.
fn product_check(
    func: &Function,
    zero_test: ValueId,
    exact: ValueId,
) -> Option<(ValueId, ValueId, ValueId)> {
    let def = |value: ValueId| match *func.value(value) {
        Value::Inst(inst) => Some(&func.inst(inst).kind),
        _ => None,
    };
    let InstKind::Eq(l, r) = *def(zero_test)? else { return None };
    let divisor = match (func.value_u256(l), func.value_u256(r)) {
        (None, Some(zero)) if zero.is_zero() => l,
        (Some(zero), None) if zero.is_zero() => r,
        _ => return None,
    };
    let InstKind::Eq(l, r) = *def(exact)? else { return None };
    [(l, r), (r, l)].into_iter().find_map(|(quotient, other)| {
        let &InstKind::Div(product, by) = def(quotient)? else { return None };
        let &InstKind::Mul(a, b) = def(product)? else { return None };
        let factors = (a == other && b == divisor) || (a == divisor && b == other);
        let words = [divisor, other].iter().all(|&v| func.value_ty(v) == Some(MirType::I256));
        (by == divisor && factors && words).then_some((divisor, other, zero_test))
    })
}

/// Rewrites `(x + y) + c` in a loop, with `x` from outside the loop, `y` from inside, and `c` a
/// constant, as `t + y` with `t = x + c` computed once before the loop. Memory lowering spells an
/// element address `(base + (index << 5)) + 32`, so a loop whose index no affine recurrence
/// describes, as a heap's sift or a sort's partition scan, rebuilt both sums on every access. An
/// affine `y` stays: `indvar-simplify` steps a pointer for it, and prices that by the sums it
/// removes. Word addition wraps, so any association computes the same word.
///
/// `t` goes to the preheader of the outermost loop `x` is fixed in, so a nested loop does not
/// rebuild it on every outer iteration, and replaces `x` there: every sum with the same `x + c`
/// that preheader dominates is rewritten, each inner sum must have no other reader, and nothing
/// else the preheader dominates may read `x`. A word kept for other readers would make the loops
/// carry one more, which costs more than the sums save when a loop runs once or twice, as a heap
/// push's sift or a hash table's probe does.
fn reassociate_invariant_offsets(
    func: &mut Function,
    loop_data: &Loop,
    loops: &[Loop],
    dominators: &DominatorTree,
) -> usize {
    let inst_blocks = func.inst_block_table();
    let block_of = |value: ValueId| match *func.value(value) {
        Value::Inst(inst) => inst_blocks.get(inst).copied().flatten(),
        _ => None,
    };
    // An affine index is `indvar-simplify`'s, which steps a pointer instead.
    let scev = ScalarEvolution::analyze(func, loop_data);
    let readers = super::egraph::use_counts(func);
    // The sum `outer = (x + y) + c` with its single-reader inner sum, as `(x, c, outer, inner, y)`.
    let split = |outer: InstId| {
        let InstKind::Add(first, second) = func.inst(outer).kind else { return None };
        let (sum, c) = [(first, second), (second, first)]
            .into_iter()
            .find(|&(sum, c)| func.value_u256(c).is_some() && func.value_u256(sum).is_none())?;
        let Value::Inst(inner) = *func.value(sum) else { return None };
        let InstKind::Add(a, b) = func.inst(inner).kind else { return None };
        (readers[sum] == 1
            && func.value_ty(sum) == Some(MirType::I256)
            && func.inst(outer).result_ty == Some(MirType::I256))
        .then_some((a, b, c, inner))
    };
    // Each `x + c` with an in-loop site whose `y` changes in the loop but not affinely.
    let mut keys = Vec::<(ValueId, U256, ValueId)>::new();
    for block in loop_data.blocks.iter() {
        for &outer in &func.blocks[block].instructions {
            let Some((a, b, c, _)) = split(outer) else { continue };
            for (x, y) in [(a, b), (b, a)] {
                if func.value_u256(x).is_none()
                    && block_of(x).is_none_or(|def| !loop_data.blocks.contains(def))
                    && block_of(y).is_some_and(|def| loop_data.blocks.contains(def))
                    && scev.get(y).is_none()
                {
                    let constant = func.value_u256(c).expect("the offset is a constant");
                    if !keys.iter().any(|&(kx, kc, _)| kx == x && kc == constant) {
                        keys.push((x, constant, c));
                    }
                    break;
                }
            }
        }
    }
    // Plan every rewrite before changing the function.
    let mut plans = Vec::new();
    let mut claimed = FxHashSet::default();
    for (x, constant, c) in keys {
        // The outermost loop around this one that `x` is fixed in.
        let Some(target) = loops
            .iter()
            .filter(|outer| {
                outer.blocks.superset(&loop_data.blocks)
                    && block_of(x).is_none_or(|def| !outer.blocks.contains(def))
            })
            .max_by_key(|outer| outer.blocks.count())
        else {
            continue;
        };
        let Some(preheader) = target.preheader else { continue };
        if block_of(x).is_some_and(|def| def != preheader && !dominators.dominates(def, preheader))
        {
            continue;
        }
        // Every `x + c` sum the preheader dominates, and whether anything else there reads `x`.
        let mut sites = Vec::new();
        let mut read_elsewhere = false;
        for block in func.blocks.indices() {
            if block == preheader || !dominators.dominates(preheader, block) {
                continue;
            }
            for &inst in &func.blocks[block].instructions {
                if let Some((a, b, sum_c, inner)) = split(inst)
                    && func.value_u256(sum_c) == Some(constant)
                    && let Some(y) = if a == x {
                        Some(b)
                    } else if b == x {
                        Some(a)
                    } else {
                        None
                    }
                    && func.value_u256(y).is_none()
                {
                    sites.push((inst, inner, y));
                }
            }
            let block = &func.blocks[block];
            if block.terminator.as_ref().is_some_and(|term| term.reads(x)) {
                read_elsewhere = true;
            }
        }
        let inners = sites.iter().map(|&(_, inner, _)| inner).collect::<Vec<_>>();
        read_elsewhere |= func.blocks.indices().any(|block| {
            block != preheader
                && dominators.dominates(preheader, block)
                && func.blocks[block]
                    .instructions
                    .iter()
                    .any(|&inst| !inners.contains(&inst) && func.inst(inst).kind.reads(x))
        });
        if read_elsewhere
            || sites.is_empty()
            || sites.iter().any(|&(outer, _, _)| claimed.contains(&outer))
        {
            continue;
        }
        claimed.extend(sites.iter().map(|&(outer, _, _)| outer));
        plans.push((preheader, x, c, sites));
    }
    let mut rewritten = 0;
    for (preheader, x, c, sites) in plans {
        // preheader: t = add x, c
        let mut sum = Instruction::new(InstKind::Add(x, c), Some(MirType::I256));
        sum.metadata.copy_debug_context(&func.inst(sites[0].0).metadata);
        let (sum_inst, t) = func.alloc_value_inst(sum);
        func.blocks[preheader].instructions.push(sum_inst);
        for (outer, inner, y) in sites {
            // outer = add t, y
            func.inst_mut(outer).kind = InstKind::Add(t, y);
            let block = inst_blocks[inner].expect("the inner sum is in a block");
            func.blocks[block].instructions.retain(|&inst| inst != inner);
            rewritten += 1;
        }
    }
    rewritten
}

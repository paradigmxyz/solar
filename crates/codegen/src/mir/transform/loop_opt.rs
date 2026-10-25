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
//! from outside, computed per loop of the nest containing the instruction and updated as roots
//! are accepted: past the budget, the backend's layouts spill loop values to frame slots, and a
//! hoisted base costs more in reloads of the words it displaces than its in-loop recomputation
//! saved. At most a fixed number of instructions leave one loop.
//!
//! The pass runs once on the semantic MIR and once more after memory lowering,
//! which materializes each element access as `add base, 32` plus an index term inside the
//! loop that reads it; the late run hoists that base so a hot loop carries one word instead
//! of reloading its argument and re-adding the header on every iteration.
//!
//! An unsigned checked product inside a loop whose factor `y` is defined before
//! the loop, lowered as `y == 0 || (x * y) / y == x` or, with `y` first, as a check that
//! divides by `x`, becomes `x <= MAX / y - (y == 0)`, with the bound computed once in the
//! preheader. A nonzero `y` keeps the product below `2^256`
//! exactly when `x <= MAX / y`. A zero `y` turns the bound into `0 - 1 = MAX`, which every
//! `x` meets. Each iteration then compares instead of dividing. The rewrite is skipped in loops
//! that check the gas left, since the bound runs first.
//!
//! In gas mode the check keeps its branch, which panics when the comparison fails: flipping the
//! branch so the check could merge with the add overflow check after it made the loop carry more
//! words, and it measured slower. In size mode the comparison is the branch condition itself,
//! with the edges swapped, so `merge-aborts` joins it with the next check's test and no
//! `iszero` remains. The bound is one more word the loop carries, and it replaces a division
//! whose operands the schedule shuffles: size mode rewrites only when the target prices the bound
//! and the comparisons below the checks they replace, and the loop stays within
//! [`LOOP_CARRY_BUDGET`]. A loop carrying seven words grew by two bytes, while loops carrying
//! two and four shrank by one and four.

use crate::{
    mir::{
        BlockId, EffectKind, Function, FunctionBuilder, Immediate, ImmutableId, InstId, InstKind,
        Instruction, MirType, Module, OpTraits, PtrLocation, StorageAlias, Terminator, Value,
        ValueId,
        analysis::{
            AddressSpace, AffineExpr, AliasAnalysis, AliasResult, CfgInfo, Location, LocationSize,
            Loop, LoopAnalyzer, MemoryAddress, MemoryLocation, ModRef, ScalarEvolution,
        },
        pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
        utils as mir_utils,
    },
    target::Target,
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::rc::Rc;

/// Most words a loop may carry after a below-threshold hoist. The count is the header's phis
/// plus the values defined outside the loop that its body reads, which the backend keeps on
/// the stack through the loop when they fit. Past this many, its layouts spill loop values to
/// frame slots, and a hoisted base then costs more in reloads of the words it displaced than
/// its in-loop recomputation saved: the hash-probe loop of `hasDuplicate` lost 2.4% carrying
/// eight, while `reverse` with four gained 8%.
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
        let hoist_cheap = gcx.sess.opts.optimization.is_gas();
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
                    hoist_cheap,
                    target: Target::new(gcx),
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
    analyzer: &'a LoopAnalyzer,
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
    target: Target,
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
        let loop_info = analyzer.analyze_with_cfg(func, cfg);

        if loop_info.loops.is_empty() {
            return 0;
        }

        let loops = loop_info.loops.values().cloned().collect::<Vec<_>>();
        for loop_data in &loops {
            // The product's limit runs before the loop, so whatever in the loop checks the gas
            // left would see what the limit cost before the first check paid for it.
            if !self.loop_checks_gas_left(func, loop_data) {
                self.hoisted +=
                    reduce_product_checks(func, loop_data, &self.target, |func, blocks| {
                        Self::carried_words(func, loop_data, blocks)
                    });
            }
        }
        let inst_blocks = func.inst_block_table();
        let mut carried = loops
            .iter()
            .map(|loop_data| (loop_data.header, Self::carried_words(func, loop_data, &inst_blocks)))
            .collect::<FxHashMap<_, _>>();
        for loop_data in &loops {
            self.apply_licm(func, loop_data, &analyzer, &loops, &mut carried);
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
    ) -> usize {
        let header = &func.blocks[loop_data.header];
        let mut count = header
            .instructions
            .iter()
            .filter(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .count();
        let mut seen = DenseBitSet::new_empty(func.num_values());
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
        let mut released = DenseBitSet::new_empty(func.num_values());
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

    fn alias(&self) -> &AliasAnalysis {
        &self.alias
    }

    fn apply_licm(
        &mut self,
        func: &mut Function,
        loop_data: &Loop,
        analyzer: &LoopAnalyzer,
        loops: &[Loop],
        carried: &mut FxHashMap<BlockId, usize>,
    ) {
        let Some(preheader) = loop_data.preheader else { return };
        if loop_data.invariant_insts.is_empty() || self.loop_observes_gas(func, loop_data) {
            return;
        }

        let scev = ScalarEvolution::analyze(func, loop_data);
        let ctx = LoopOptContext { loop_data, scev: &scev, analyzer };
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
        roots.sort_unstable_by(|&a, &b| {
            self.licm_profit(func, b)
                .cmp(&self.licm_profit(func, a))
                .then_with(|| a.index().cmp(&b.index()))
        });

        let inst_blocks = func.inst_block_table();
        let mut selected = DenseBitSet::new_empty(func.num_insts());
        let mut closure = Vec::new();
        let mut closure_set = DenseBitSet::new_empty(func.num_insts());
        let mut visiting = DenseBitSet::new_empty(func.num_insts());
        for root in roots {
            closure.clear();
            visiting.clear();
            if !self.collect_hoist_closure(func, root, ctx, &selected, &mut visiting, &mut closure)
            {
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
            for &inst_id in &closure {
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
                        Self::carried_delta(func, inner, &closure, &closure_set, &inst_blocks),
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
            for &inst_id in &closure {
                selected.insert(inst_id);
            }
        }

        if selected.is_empty() {
            return;
        }

        let ordered = self.topological_sort_instructions(func, &selected);

        for inst_id in ordered {
            // An enclosing loop's earlier hoist may have already moved the
            // instruction out of these blocks; pushing it again would schedule
            // the same instruction in two blocks.
            let mut removed = false;
            for block_id in &loop_data.blocks {
                let block = &mut func.blocks[block_id];
                if let Some(pos) = block.instructions.iter().position(|&id| id == inst_id) {
                    block.instructions.remove(pos);
                    removed = true;
                    break;
                }
            }
            if removed {
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
                && self.inst_in_loop(func, *dep_inst, ctx.loop_data)
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
        let Some(inst_block) = loop_data
            .blocks
            .iter()
            .find(|&block| func.blocks[block].instructions.contains(&inst_id))
        else {
            return false;
        };

        // Semantic checks and calls can exit without a CFG edge. The candidate must execute
        // before each such operation, including those earlier in its own block.
        for block_id in &loop_data.blocks {
            if block_id != inst_block && ctx.analyzer.dominates(inst_block, block_id) {
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

        let exiting = self.live_exiting_blocks(func, loop_data);
        // No live exit means the loop only terminates by running out of gas,
        // which consumes the entire gas budget regardless of what executes
        // beforehand, so any placement is observationally equivalent.
        if exiting.is_empty() {
            return true;
        }
        if exiting.iter().all(|&block| ctx.analyzer.dominates(inst_block, block)) {
            return true;
        }

        loop_data.trip_count.is_some_and(|trip| trip >= 1)
            && exiting.len() == 1
            && loop_data.back_edges.iter().all(|&latch| ctx.analyzer.dominates(inst_block, latch))
    }

    /// Returns the in-loop blocks from which the loop can actually exit.
    ///
    /// Branches whose condition is a constant that always picks the in-loop successor cannot
    /// leave the loop and are ignored.
    fn live_exiting_blocks(&self, func: &Function, loop_data: &Loop) -> Vec<BlockId> {
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

    fn inst_in_loop(&self, func: &Function, inst_id: InstId, loop_data: &Loop) -> bool {
        loop_data.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
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
            || (self.loop_has_known_multiple_iterations(ctx.loop_data)
                && self.is_affine_address_base_used_in_loop(func, inst_id, ctx))
            || (self.inst_dominates_loop_backedges(func, inst_id, ctx.loop_data, ctx.analyzer)
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

    fn loop_has_known_multiple_iterations(&self, loop_data: &Loop) -> bool {
        loop_data.trip_count.is_some_and(|trip_count| trip_count > 1)
    }

    fn loop_observes_gas(&self, func: &Function, loop_data: &Loop) -> bool {
        for block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                if func.inst(inst_id).kind.observes_gas() {
                    return true;
                }
            }
        }
        false
    }

    /// Whether the loop checks the gas left: a `gas` reading or a call that observes it, or a
    /// persistent storage write, which fails when its sentry finds 2300 gas or less.
    fn loop_checks_gas_left(&self, func: &Function, loop_data: &Loop) -> bool {
        let aa = self.alias();
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                let mod_ref = aa.instruction_mod_ref(func, inst_id);
                func.inst(inst_id).kind.observes_gas()
                    || mod_ref.observes_gas()
                    || mod_ref.writes_space(AddressSpace::Storage)
            })
        })
    }

    fn inst_dominates_loop_backedges(
        &self,
        func: &Function,
        inst_id: InstId,
        loop_data: &Loop,
        analyzer: &LoopAnalyzer,
    ) -> bool {
        let Some(inst_block) = loop_data
            .blocks
            .iter()
            .find(|&block| func.blocks[block].instructions.contains(&inst_id))
        else {
            return false;
        };
        loop_data.back_edges.iter().all(|&latch| analyzer.dominates(inst_block, latch))
    }

    fn loop_may_mutate_memory_range(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_addr: ValueId,
        load_width: Option<u64>,
    ) -> bool {
        let aa = self.alias();
        // The hoist candidate's address is loop-invariant, so its extent and affine range hold
        // for every store the loop makes.
        let load = load_width.map(|width| LoadRange {
            location: aa.bare_access_extent(func, load_addr, LocationSize::Const(width)),
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
        let aa = self.alias();
        let Some(load_alias) =
            self.storage_alias_for_loop_value(func, load_inst, load_slot, ctx.loop_data)
        else {
            return true;
        };
        let Some(load) = Location::slot(location, load_alias) else { return true };
        if !self.can_use_storage_alias_for_licm(load_alias, ctx.loop_data) {
            return true;
        }

        for block_id in &ctx.loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match &func.inst(inst_id).kind {
                    InstKind::Store(slot, _, store_location) if *store_location == location => {
                        let Some(store_alias) =
                            self.storage_alias_for_loop_value(func, inst_id, *slot, ctx.loop_data)
                        else {
                            return true;
                        };
                        if !self.can_use_storage_alias_for_licm(store_alias, ctx.loop_data) {
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
        let aa = self.alias();
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
        let aa = self.alias();
        if let Some(location) = load.location
            && let Some(write) =
                aa.bare_access_extent(func, write_addr, LocationSize::Const(write_width))
        {
            match aa.memory_alias(location, write) {
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
        loop_data: &Loop,
    ) -> Option<StorageAlias> {
        let alias = self.alias().storage_alias(func, inst_id, value);
        if let Some(base) = alias.symbolic_base()
            && self.value_defined_in_loop(func, base, loop_data)
        {
            return None;
        }
        Some(alias)
    }

    fn can_use_storage_alias_for_licm(&self, alias: StorageAlias, loop_data: &Loop) -> bool {
        matches!(alias, StorageAlias::Slot(_)) || self.loop_has_known_multiple_iterations(loop_data)
    }

    fn value_defined_in_loop(&self, func: &Function, value: ValueId, loop_data: &Loop) -> bool {
        match func.value(value) {
            Value::Inst(inst_id) => self.inst_in_loop(func, *inst_id, loop_data),
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
                    InstKind::Load(addr, _)
                    | InstKind::Store(addr, ..)
                    | InstKind::Store8(addr, _)
                    | InstKind::Keccak256(addr, _)
                    | InstKind::Copy(addr, _, _, PtrLocation::Calldata | PtrLocation::Returndata)
                    | InstKind::DataCopy(_, addr, _)
                    | InstKind::CodeCopy(addr, _, _)
                    | InstKind::ExtCodeCopy(_, addr, _, _) => address_operands.push(*addr),
                    InstKind::MappingSlotBytes(key, _)
                        if func.value_ty(*key) == Some(MirType::MEMPTR) =>
                    {
                        address_operands.push(*key);
                    }
                    InstKind::Copy(dst, src, _, PtrLocation::Memory) => {
                        address_operands.push(*dst);
                        address_operands.push(*src);
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
        if !self.inst_in_loop(func, *inst_id, ctx.loop_data) {
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
    ) -> Vec<InstId> {
        let mut result = Vec::new();
        let mut visited = DenseBitSet::new_empty(func.num_insts());

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
            visit(func, inst_id, inst_set, &mut visited, &mut result);
        }

        result
    }

    /// Moving a fixed-width hash also moves its scratch writes and memory expansion. Require
    /// no other observer or clobber of scratch in the whole function, including calls; retain
    /// the existing must-execute proof so zero-trip loops do not acquire a hash.
    fn slot_hash_scratch_is_private(&self, func: &Function, candidate: InstId) -> bool {
        let aa = self.alias();
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
    target: &Target,
    carried_words: impl FnOnce(&Function, &IndexVec<InstId, Option<BlockId>>) -> usize,
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
    let gas = target.optimization().is_gas();
    if !gas && !checks.is_empty() {
        let carried = carried_words(func, &inst_blocks);
        // Each bound is one more carried word and pays only when enough checks share it.
        let mut groups = Vec::<(ValueId, u32)>::new();
        for &(.., y, _) in &checks {
            match groups.iter_mut().find(|(factor, _)| *factor == y) {
                Some((_, count)) => *count += 1,
                None => groups.push((y, 1)),
            }
        }
        let mut accepted = Vec::new();
        for (y, count) in groups {
            if carried + accepted.len() < LOOP_CARRY_BUDGET
                && target.product_bound_is_profitable(count)
            {
                accepted.push(y);
            }
        }
        checks.retain(|&(.., y, _)| accepted.contains(&y));
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
        if !gas && flip_check(func, block, inst, &uses) {
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

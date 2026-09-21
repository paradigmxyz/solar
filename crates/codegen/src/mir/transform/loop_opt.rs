//! Loop Optimization passes for MIR.
//!
//! This module provides loop optimizations for MIR.
//!
//! **Loop Invariant Code Motion (LICM)** moves computations that don't change
//! within a loop to the preheader block, reducing redundant work.
//!
//! Memory reads hoist only when no loop instruction may write what they read,
//! judged by the shared alias analysis with module call summaries, so a call
//! to a memory-clean helper is not a barrier. A semantic length read of an
//! existing heap object (a fresh allocation or an object argument) needs no
//! execution guarantee: its header is allocated memory, so reading it early
//! cannot expand memory or trap on a zero-trip loop.
//!
//! An instruction is a hoisting root when its estimated per-iteration saving reaches the
//! pass threshold, or when it is the invariant base of an affine address the loop reads or
//! writes. In gas mode, pure word arithmetic that executes on every iteration is a root as
//! well: the backend carries a preheader value on the stack through the loop or rebuilds it
//! at its uses, so hoisting only widens its choice. Those below-threshold hoists are held to
//! a budget of loop-carried words, the header's phis plus the values the loop reads from
//! outside, computed per loop of the nest containing the instruction and updated as roots
//! are accepted: past the budget, the backend's layouts spill loop values to frame slots,
//! and a hoisted base costs more in reloads of the words it displaces than its in-loop
//! recomputation saved.
//!
//! The pass runs once on the semantic MIR and once more in gas mode after memory lowering,
//! restricted to loops with physical memory or storage accesses by `memory-licm`. Lowering
//! materializes each element access as `add base, 32` plus an index term inside the
//! loop that reads it; the late run hoists that base so a hot loop carries one word instead
//! of reloading its argument and re-adding the header on every iteration.
//!
//! ## Gas Savings
//!
//! This optimization is particularly important for EVM:
//! - LICM: Avoids recomputing `arr.length` each iteration (MLOAD/SLOAD costs)
//!
//! Hoisting loads requires both independence and an execution guarantee. Semantic checks
//! and calls may exit before a load without an explicit CFG edge; their control effects
//! therefore constrain the guarantee even when a constant loop bound is known.

use crate::mir::{
    BlockId, Callee, EffectKind, Function, ImmutableId, InstId, InstKind, Module, OpTraits,
    StorageAlias, Terminator, Value, ValueId,
    analysis::{
        Access, AddressSpace, AffineExpr, AliasAnalysis, AliasResult, CfgInfo, Location,
        LocationSize, Loop, LoopAnalyzer, ScalarEvolution,
    },
    pass::{MirPass, run_selected_function_pass_with_alias_and_cfg},
    utils as mir_utils,
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;
use solar_data_structures::{bit_set::DenseBitSet, map::FxHashMap};
use std::rc::Rc;

/// Most words a loop may carry after a below-threshold hoist. The count is the header's phis
/// plus the values defined outside the loop that its body reads, which the backend keeps on
/// the stack through the loop when they fit. Past this many, its layouts spill loop values to
/// frame slots, and a hoisted base then costs more in reloads of the words it displaced than
/// its in-loop recomputation saved: the hash-probe loop of `hasDuplicate` lost 2.4% carrying
/// eight, while `reverse` with four gained 8%.
const LOOP_CARRY_BUDGET: usize = 5;

/// Function pass for loop-invariant code motion.
pub(crate) enum Licm {
    All,
    MemoryLoops,
}

impl MirPass for Licm {
    fn name(&self) -> &'static str {
        match self {
            Self::All => "licm",
            Self::MemoryLoops => "memory-licm",
        }
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
            let cfg = analyses.cfg(id, func);
            let cycles = cfg.cyclic_blocks();
            if !cycles.is_empty()
                && (matches!(self, Self::All)
                    || cycles.iter().any(|block| {
                        func.blocks[block].instructions.iter().any(|&inst| {
                            matches!(
                                func.inst(inst).kind,
                                InstKind::MLoad(_)
                                    | InstKind::MStore(..)
                                    | InstKind::SLoad(..)
                                    | InstKind::SStore(..)
                                    | InstKind::TLoad(..)
                                    | InstKind::TStore(..)
                            )
                        })
                    }))
            {
                selected.insert(id);
            }
        }
        run_selected_function_pass_with_alias_and_cfg(
            module,
            analyses,
            &selected,
            |func, analyses| {
                let mut optimizer = LoopOptimizer::with_limits(3, 8);
                optimizer.hoist_cheap = hoist_cheap;
                optimizer.alias = Some(Rc::clone(analyses.alias()));
                optimizer.optimize(func, Rc::clone(analyses.cfg())).instructions_hoisted != 0
            },
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum StorageSpace {
    Persistent,
    Transient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AffineRange {
    base: Option<ValueId>,
    start: i128,
    end: i128,
}

#[derive(Clone, Copy)]
struct LoopOptContext<'a> {
    loop_data: &'a Loop,
    scev: &'a ScalarEvolution,
    analyzer: &'a LoopAnalyzer,
}

/// Loop optimizer.
#[derive(Debug)]
struct LoopOptimizer {
    /// Minimum estimated gas saved per iteration before an instruction is considered a LICM root.
    min_licm_profit: u16,
    /// Maximum number of instructions hoisted from one loop.
    max_licm_hoisted_insts: usize,
    /// Whether pure arithmetic executed on every iteration is hoisted even when its per-iteration
    /// saving is below `min_licm_profit`. The backend carries a preheader value on the stack
    /// through a loop or rebuilds it at its use sites, whichever its pricing prefers, so
    /// hoisting only widens its choice; the cost is bytecode when the value is spilled, which
    /// gas mode accepts.
    hoist_cheap: bool,
    stats: LoopOptStats,
    alias: Option<Rc<AliasAnalysis>>,
}

impl Default for LoopOptimizer {
    fn default() -> Self {
        Self {
            min_licm_profit: 0,
            max_licm_hoisted_insts: usize::MAX,
            hoist_cheap: false,
            stats: LoopOptStats::default(),
            alias: None,
        }
    }
}

/// Statistics from loop optimization.
#[derive(Clone, Debug, Default)]
struct LoopOptStats {
    /// Number of instructions hoisted out of loops.
    instructions_hoisted: usize,
}

impl LoopOptimizer {
    fn with_limits(min_licm_profit: u16, max_licm_hoisted_insts: usize) -> Self {
        Self {
            min_licm_profit,
            max_licm_hoisted_insts,
            hoist_cheap: false,
            stats: LoopOptStats::default(),
            alias: None,
        }
    }

    /// Runs loop-invariant code motion on a function.
    fn optimize(&mut self, func: &mut Function, cfg: Rc<CfgInfo>) -> &LoopOptStats {
        self.stats = LoopOptStats::default();
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::StorageAndTransient);
        if self.alias.is_none() {
            self.alias = Some(Rc::new(AliasAnalysis::new(func)));
        }

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze_with_cfg(func, cfg);

        if loop_info.loops.is_empty() {
            return &self.stats;
        }

        let loops = loop_info.loops.values().cloned().collect::<Vec<_>>();
        let inst_blocks = func.inst_blocks();
        let mut carried = loops
            .iter()
            .map(|loop_data| (loop_data.header, Self::carried_words(func, loop_data, &inst_blocks)))
            .collect::<FxHashMap<_, _>>();
        for loop_data in &loops {
            self.apply_licm(func, loop_data, &analyzer, &loops, &mut carried);
        }

        &self.stats
    }

    /// The words the backend carries through a loop: the header's phis and the values defined
    /// outside the loop that its non-phi instructions read. Immediates and nullary
    /// rematerializable reads are rebuilt where used and cost no word.
    fn carried_words(
        func: &Function,
        loop_data: &Loop,
        inst_blocks: &FxHashMap<InstId, BlockId>,
    ) -> usize {
        let header = &func.blocks[loop_data.header];
        let mut count = header
            .instructions
            .iter()
            .filter(|&&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
            .count();
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
                if Self::is_carried_operand(func, loop_data, inst_blocks, operand)
                    && seen.insert(operand)
                {
                    count += 1;
                }
            }
        }
        count
    }

    /// Whether a value read inside `loop_data` occupies a carried word: an argument or an
    /// instruction result outside the loop that is not a nullary rematerializable read.
    fn is_carried_operand(
        func: &Function,
        loop_data: &Loop,
        inst_blocks: &FxHashMap<InstId, BlockId>,
        value: ValueId,
    ) -> bool {
        if matches!(func.value(value), Value::Arg(_)) {
            return true;
        }
        let Value::Inst(inst_id) = func.value(value) else { return false };
        let kind = &func.inst(*inst_id).kind;
        inst_blocks.get(inst_id).is_none_or(|block| !loop_data.blocks.contains(*block))
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
        inst_blocks: &FxHashMap<InstId, BlockId>,
    ) -> isize {
        let reads_outside_closure = |value: ValueId, block: BlockId| {
            let block = &func.blocks[block];
            block
                .instructions
                .iter()
                .filter(|&&inst_id| !closure_set.contains(inst_id))
                .flat_map(|&inst_id| func.inst(inst_id).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
                .any(|operand| operand == value)
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
        self.alias.as_ref().expect("loop optimizer alias snapshot is initialized")
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
        if self.loop_observes_gas(func, loop_data) {
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
        roots.sort_unstable_by(|&a, &b| {
            self.licm_profit(func, b)
                .cmp(&self.licm_profit(func, a))
                .then_with(|| a.index().cmp(&b.index()))
        });

        let inst_blocks = func.inst_blocks();
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
            if selected.count() + new_count > self.max_licm_hoisted_insts {
                continue;
            }
            // The root leaves every loop of this nest that contains it; each of those loops
            // carries the closure's results from now on. A hoist that only pays through the
            // backend's residency must keep every one of them within the carry budget.
            closure_set.clear();
            for &inst_id in &closure {
                closure_set.insert(inst_id);
            }
            let Some(&root_block) = inst_blocks.get(&root) else { continue };
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
            if self.licm_profit(func, root) < self.min_licm_profit
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
                self.stats.instructions_hoisted += 1;
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

        if inst.must_execute(false) {
            return false;
        }
        if matches!(inst.kind, InstKind::Phi(_)) {
            return false;
        }
        match inst.kind {
            // Hoisting memory reads expands memory earlier (and unconditionally), which any
            // MSIZE in the function could observe; on top of the dependence checks they must
            // also be guaranteed to execute so a zero-trip loop cannot start trapping (OOG
            // from speculated memory expansion) or paying for work it never did.
            InstKind::MLoad(addr) => {
                return !self.function_observes_msize(func)
                    && self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_memory_range(func, ctx, addr, Some(32));
            }
            // A semantic length read lowers to one word load of the object's
            // header. Element, byte, and word stores address the payload that
            // follows the header, so alias analysis can prove the loop leaves
            // the length alone while the object identity is still explicit.
            // Nominal object types do not prove that the header is allocated.
            // As with raw loads, require execution on every path through the loop.
            InstKind::MemoryObjectLen(..) => {
                return !self.function_observes_msize(func)
                    && self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_write_read_locations(func, ctx, inst_id);
            }
            // These semantic memory reads lower to `mload` after LICM. Keep them in
            // place until their physical address and width are explicit so a store
            // in the loop cannot be missed by the dependence check above.
            InstKind::MemoryObjectLoadField { .. }
            | InstKind::MemoryObjectLoadElement { .. }
            | InstKind::MemoryObjectLoadByte { .. }
            | InstKind::MemorySliceLoadWord { .. }
            | InstKind::Keccak256Bytes(_)
            | InstKind::FrameLoad { .. } => return false,
            InstKind::Keccak256(offset, size) => {
                return !self.function_observes_msize(func)
                    && self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_memory_range(
                        func,
                        ctx,
                        offset,
                        self.const_addr(func, size),
                    );
            }
            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::MappingSlotCalldata(_, _)
            | InstKind::StorageArrayDataSlot(_)
            | InstKind::StorageArrayElementSlot { .. } => return false,
            InstKind::SLoad(slot) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_storage_slot(
                        func,
                        ctx,
                        inst_id,
                        slot,
                        StorageSpace::Persistent,
                    );
            }
            InstKind::TLoad(slot) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_mutate_storage_slot(
                        func,
                        ctx,
                        inst_id,
                        slot,
                        StorageSpace::Transient,
                    );
            }
            InstKind::LoadImmutable(id) => {
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_may_assign_immutable(func, ctx.loop_data, id);
            }
            // MSIZE observes every memory expansion, including from other hoisted
            // instructions; never move it.
            InstKind::MSize | InstKind::Fmp => return false,
            // Environment reads that calls or creates can change: balances move with value
            // transfers, code size/hash change on deploy/selfdestruct, and every external
            // call rewrites the return-data buffer.
            InstKind::Balance(_)
            | InstKind::SelfBalance
            | InstKind::ExtCodeSize(_)
            | InstKind::ExtCodeHash(_)
            | InstKind::ReturnDataSize => {
                // Also require guaranteed execution: speculating a cold
                // BALANCE/EXTCODESIZE/EXTCODEHASH into the preheader of a
                // zero-trip loop wastes 2600 gas.
                return self.hoist_execution_guaranteed(func, inst_id, ctx)
                    && !self.loop_contains_call_or_create(func, ctx.loop_data);
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
                    match self.const_condition(func, *condition) {
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

    fn function_observes_msize(&self, func: &Function) -> bool {
        self.alias().may_observe_msize(func)
    }

    fn loop_contains_call_or_create(&self, func: &Function, loop_data: &Loop) -> bool {
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                matches!(
                    func.inst(inst_id).kind.effect_kind(),
                    EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
                )
            })
        })
    }

    fn inst_in_loop(&self, func: &Function, inst_id: InstId, loop_data: &Loop) -> bool {
        loop_data.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
    }

    fn licm_profit(&self, func: &Function, inst_id: InstId) -> u16 {
        match func.inst(inst_id).kind {
            InstKind::SLoad(_) => 100,
            InstKind::TLoad(_) => 100,
            InstKind::Keccak256(_, _) => 30,
            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::MappingSlotCalldata(_, _)
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
            InstKind::MLoad(_) | InstKind::CalldataLoad(_) | InstKind::MemoryObjectLen(_, _) => 3,
            _ => 0,
        }
    }

    fn is_profitable_licm_root(
        &self,
        func: &Function,
        inst_id: InstId,
        ctx: LoopOptContext<'_>,
    ) -> bool {
        self.licm_profit(func, inst_id) >= self.min_licm_profit
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
            && inst.kind.effect_kind() == crate::mir::EffectKind::Pure
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
        for block_id in &ctx.loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match func.inst(inst_id).kind {
                    InstKind::MStore(addr, _) => {
                        if self.memory_ranges_may_alias(
                            func, ctx, load_addr, load_width, addr, 32, block_id,
                        ) {
                            return true;
                        }
                    }
                    InstKind::MStore8(addr, _) => {
                        if self.memory_ranges_may_alias(
                            func, ctx, load_addr, load_width, addr, 1, block_id,
                        ) {
                            return true;
                        }
                    }
                    _ if aa
                        .instruction_mod_ref(func, inst_id)
                        .writes_space(AddressSpace::Memory) =>
                    {
                        return true;
                    }
                    InstKind::MSize => return true,
                    _ => {}
                }
            }
        }
        false
    }

    /// Returns true if any loop instruction or terminator may write a location
    /// that `load_inst` reads, or if that read is not a bounded location.
    fn loop_may_write_read_locations(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_inst: InstId,
    ) -> bool {
        let aa = self.alias();
        let mut locations = ArrayVec::<Location, 4>::new();
        for &access in aa.instruction_mod_ref(func, load_inst).reads() {
            match access {
                Access::Location(location) if !locations.is_full() => locations.push(location),
                Access::Location(_) | Access::Any(_) => return true,
            }
        }
        for block_id in &ctx.loop_data.blocks {
            let block = &func.blocks[block_id];
            for &inst_id in &block.instructions {
                let effects = aa.instruction_mod_ref(func, inst_id);
                if locations.iter().any(|&location| effects.may_write(aa, location)) {
                    return true;
                }
            }
            if let Some(terminator) = &block.terminator {
                let effects = aa.terminator_mod_ref(func, terminator);
                if locations.iter().any(|&location| effects.may_write(aa, location)) {
                    return true;
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
        space: StorageSpace,
    ) -> bool {
        let aa = self.alias();
        let Some(load_alias) =
            self.storage_alias_for_loop_value(func, load_inst, load_slot, ctx.loop_data)
        else {
            return true;
        };
        if !self.can_use_storage_alias_for_licm(load_alias, ctx.loop_data) {
            return true;
        }

        for block_id in &ctx.loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                match (space, &func.inst(inst_id).kind) {
                    (StorageSpace::Persistent, InstKind::SStore(slot, _))
                    | (StorageSpace::Transient, InstKind::TStore(slot, _)) => {
                        let Some(store_alias) =
                            self.storage_alias_for_loop_value(func, inst_id, *slot, ctx.loop_data)
                        else {
                            return true;
                        };
                        if !self.can_use_storage_alias_for_licm(store_alias, ctx.loop_data) {
                            return true;
                        }
                        let (load, store) = match space {
                            StorageSpace::Persistent => {
                                (Location::Storage(load_alias), Location::Storage(store_alias))
                            }
                            StorageSpace::Transient => {
                                (Location::Transient(load_alias), Location::Transient(store_alias))
                            }
                        };
                        if aa.alias(load, store).may_alias() {
                            return true;
                        }
                    }
                    _ => {
                        let location = match space {
                            StorageSpace::Persistent => Location::Storage(load_alias),
                            StorageSpace::Transient => Location::Transient(load_alias),
                        };
                        if aa.instruction_mod_ref(func, inst_id).may_write(aa, location) {
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
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                matches!(
                    func.inst(inst_id).kind,
                    InstKind::StoreImmutable(id, _) if id == load_id
                ) || matches!(
                    func.inst(inst_id).kind,
                    InstKind::ICall { function: Callee::Function(_), .. }
                )
            })
        })
    }

    #[allow(clippy::too_many_arguments)]
    fn memory_ranges_may_alias(
        &self,
        func: &Function,
        ctx: LoopOptContext<'_>,
        load_addr: ValueId,
        load_width: Option<u64>,
        write_addr: ValueId,
        write_width: u64,
        write_block: BlockId,
    ) -> bool {
        let Some(load_width) = load_width else { return true };
        let aa = self.alias();
        if let (Some(load), Some(write)) = (
            aa.bare_memory_location(func, load_addr, LocationSize::Const(load_width)),
            aa.bare_memory_location(func, write_addr, LocationSize::Const(write_width)),
        ) {
            match aa.memory_alias(load, write) {
                AliasResult::NoAlias => return false,
                AliasResult::MustAlias | AliasResult::PartialAlias => return true,
                AliasResult::MayAlias => {}
            }
        }

        // The hoist candidate's address is loop-invariant, so its position
        // never tightens the range. Scalar evolution can prove disjointness
        // for affine loop addresses beyond value-local BasicAA.
        let Some(load) = self.affine_range(func, ctx, load_addr, load_width, None) else {
            return true;
        };
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

    fn const_addr(&self, func: &Function, value: ValueId) -> Option<u64> {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256()?.try_into().ok(),
            Value::Arg(_) | Value::Inst(_) | Value::Undef(_) | Value::Error(_) => None,
        }
    }

    fn const_condition(&self, func: &Function, value: ValueId) -> Option<bool> {
        match func.value(value) {
            Value::Immediate(imm) => Some(!imm.as_u256()?.is_zero()),
            Value::Arg(_) | Value::Inst(_) | Value::Undef(_) | Value::Error(_) => None,
        }
    }

    fn const_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        match func.value(value) {
            Value::Immediate(imm) => u256_to_i128(imm.as_u256()?),
            Value::Arg(_) | Value::Inst(_) | Value::Undef(_) | Value::Error(_) => None,
        }
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
                    InstKind::MLoad(addr)
                    | InstKind::MStore(addr, _)
                    | InstKind::MStore8(addr, _)
                    | InstKind::SLoad(addr)
                    | InstKind::SStore(addr, _)
                    | InstKind::TLoad(addr)
                    | InstKind::TStore(addr, _)
                    | InstKind::CalldataLoad(addr)
                    | InstKind::Keccak256(addr, _)
                    | InstKind::MappingSlotMemory(addr, _)
                    | InstKind::CalldataCopy(addr, _, _)
                    | InstKind::DataCopy(_, addr, _)
                    | InstKind::CodeCopy(addr, _, _)
                    | InstKind::ReturnDataCopy(addr, _, _)
                    | InstKind::ExtCodeCopy(_, addr, _, _) => address_operands.push(*addr),
                    InstKind::MCopy(dst, src, _) => {
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
            for operand in inst.kind.operands() {
                if let Value::Inst(dep_inst) = func.value(operand)
                    && inst_set.contains(*dep_inst)
                {
                    visit(func, *dep_inst, inst_set, visited, result);
                }
            }
            result.push(inst_id);
        }

        for inst_id in inst_set.iter() {
            visit(func, inst_id, inst_set, &mut visited, &mut result);
        }

        result
    }
}

fn u256_to_i128(value: U256) -> Option<i128> {
    if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
}

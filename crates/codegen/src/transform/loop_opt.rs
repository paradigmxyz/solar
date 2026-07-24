//! Loop Optimization passes for MIR.
//!
//! This module provides loop optimizations for MIR.
//!
//! **Loop Invariant Code Motion (LICM)** moves computations that don't change
//! within a loop to the preheader block, reducing redundant work.
//!
//! ## Gas Savings
//!
//! This optimization is particularly important for EVM:
//! - LICM: Avoids recomputing `arr.length` each iteration (MLOAD/SLOAD costs)

use crate::{
    analysis::{
        AddressSpace, AffineExpr, AliasAnalysis, AliasResult, Location, LocationSize, Loop,
        LoopAnalyzer, ScalarEvolution,
    },
    mir::{
        BlockId, Function, InstId, InstKind, Module, StorageAlias, Terminator, Value, ValueId,
        utils as mir_utils,
    },
    pass::{MirPass, run_function_pass},
};
use alloy_primitives::U256;
use arrayvec::ArrayVec;
use solar_data_structures::bit_set::DenseBitSet;
use std::rc::Rc;

/// Function pass for loop-invariant code motion.
pub(crate) struct Licm;

impl MirPass for Licm {
    fn name(&self) -> &'static str {
        "licm"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass(module, analyses, |func, analyses| {
            let mut optimizer = LoopOptimizer::with_limits(3, 8);
            optimizer.alias = Some(Rc::clone(&analyses.alias));
            optimizer.optimize(func).instructions_hoisted != 0
        })
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
    stats: LoopOptStats,
    alias: Option<Rc<AliasAnalysis>>,
}

impl Default for LoopOptimizer {
    fn default() -> Self {
        Self {
            min_licm_profit: 0,
            max_licm_hoisted_insts: usize::MAX,
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
            stats: LoopOptStats::default(),
            alias: None,
        }
    }

    /// Runs loop-invariant code motion on a function.
    fn optimize(&mut self, func: &mut Function) -> &LoopOptStats {
        self.stats = LoopOptStats::default();
        func.annotate_storage_aliases(mir_utils::StorageAliasScope::StorageAndTransient);
        if self.alias.is_none() {
            self.alias = Some(Rc::new(AliasAnalysis::new(func)));
        }

        let mut analyzer = LoopAnalyzer::new();
        let loop_info = analyzer.analyze(func);

        if loop_info.loops.is_empty() {
            return &self.stats;
        }

        let loop_headers: Vec<BlockId> = loop_info.loops.keys().copied().collect();

        for header in loop_headers {
            if let Some(loop_data) = loop_info.loops.get(&header) {
                self.apply_licm(func, loop_data, &analyzer);
            }
        }

        &self.stats
    }

    fn alias(&self) -> &AliasAnalysis {
        self.alias.as_ref().expect("loop optimizer alias snapshot is initialized")
    }

    fn apply_licm(&mut self, func: &mut Function, loop_data: &Loop, analyzer: &LoopAnalyzer) {
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
        roots.sort_by(|&a, &b| {
            self.licm_profit(func, b)
                .cmp(&self.licm_profit(func, a))
                .then_with(|| a.index().cmp(&b.index()))
        });

        let mut selected = DenseBitSet::new_empty(func.instructions.len());
        let mut closure = Vec::new();
        let mut visiting = DenseBitSet::new_empty(func.instructions.len());
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
            for &inst_id in &closure {
                selected.insert(inst_id);
            }
        }

        if selected.is_empty() {
            return;
        }

        let mut hoistable: Vec<InstId> = selected.iter().collect();
        hoistable.sort_by_key(|inst_id| inst_id.index());
        let ordered = self.topological_sort_instructions(func, &hoistable);

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

        let inst = &func.instructions[inst_id];
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
        let inst = &func.instructions[inst_id];

        if inst.kind.has_side_effects() {
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
            | InstKind::MappingSlotCalldata(_, _) => return false,
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
        true
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
        func.instructions()
            .any(|inst_id| matches!(func.instructions[inst_id].kind, InstKind::MSize))
    }

    fn loop_contains_call_or_create(&self, func: &Function, loop_data: &Loop) -> bool {
        loop_data.blocks.iter().any(|block_id| {
            func.blocks[block_id].instructions.iter().any(|&inst_id| {
                matches!(
                    func.instructions[inst_id].kind,
                    InstKind::Call { .. }
                        | InstKind::StaticCall { .. }
                        | InstKind::DelegateCall { .. }
                        | InstKind::InternalCall { .. }
                        | InstKind::Create(_, _, _)
                        | InstKind::Create2(_, _, _, _)
                )
            })
        })
    }

    fn inst_in_loop(&self, func: &Function, inst_id: InstId, loop_data: &Loop) -> bool {
        loop_data.blocks.iter().any(|block| func.blocks[block].instructions.contains(&inst_id))
    }

    fn licm_profit(&self, func: &Function, inst_id: InstId) -> u16 {
        match func.instructions[inst_id].kind {
            InstKind::SLoad(_) => 100,
            InstKind::TLoad(_) => 100,
            InstKind::Keccak256(_, _) => 30,
            InstKind::MappingSlot(_, _)
            | InstKind::MappingSlotMemory(_, _)
            | InstKind::MappingSlotCalldata(_, _) => 30,
            InstKind::Exp(_, _) => 10,
            InstKind::Mul(_, _)
            | InstKind::Div(_, _)
            | InstKind::SDiv(_, _)
            | InstKind::Mod(_, _)
            | InstKind::SMod(_, _)
            | InstKind::AddMod(_, _, _)
            | InstKind::MulMod(_, _, _) => 5,
            InstKind::MLoad(_) | InstKind::CalldataLoad(_) => 3,
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
                && self.is_affine_address_base_used_in_loop(func, inst_id, ctx))
    }

    fn loop_has_known_multiple_iterations(&self, loop_data: &Loop) -> bool {
        loop_data.trip_count.is_some_and(|trip_count| trip_count > 1)
    }

    fn loop_observes_gas(&self, func: &Function, loop_data: &Loop) -> bool {
        for block_id in &loop_data.blocks {
            for &inst_id in &func.blocks[block_id].instructions {
                if matches!(func.instructions[inst_id].kind, InstKind::Gas) {
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
                match func.instructions[inst_id].kind {
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
                match (space, &func.instructions[inst_id].kind) {
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
            constant: self.const_i128(func, value)?,
            terms: Default::default(),
        })
    }

    fn const_addr(&self, func: &Function, value: ValueId) -> Option<u64> {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256()?.try_into().ok(),
            Value::Arg { .. } | Value::Inst(_) | Value::Undef(_) | Value::Error(_) => None,
        }
    }

    fn const_condition(&self, func: &Function, value: ValueId) -> Option<bool> {
        match func.value(value) {
            Value::Immediate(imm) => Some(!imm.as_u256()?.is_zero()),
            Value::Arg { .. } | Value::Inst(_) | Value::Undef(_) | Value::Error(_) => None,
        }
    }

    fn const_i128(&self, func: &Function, value: ValueId) -> Option<i128> {
        match func.value(value) {
            Value::Immediate(imm) => u256_to_i128(imm.as_u256()?),
            Value::Arg { .. } | Value::Inst(_) | Value::Undef(_) | Value::Error(_) => None,
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
            Value::Arg { .. } | Value::Immediate(_) => false,
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
                let kind = &func.instructions[user_inst].kind;
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
        func.instructions[*inst_id]
            .kind
            .operands()
            .iter()
            .copied()
            .any(|operand| self.value_feeds_affine_address(func, ctx, needle, operand, depth + 1))
    }

    fn topological_sort_instructions(&self, func: &Function, insts: &[InstId]) -> Vec<InstId> {
        let mut inst_set = DenseBitSet::new_empty(func.instructions.len());
        for &inst_id in insts {
            inst_set.insert(inst_id);
        }
        let mut result = Vec::new();
        let mut visited = DenseBitSet::new_empty(func.instructions.len());

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

            let inst = &func.instructions[inst_id];
            for operand in inst.kind.operands() {
                if let Value::Inst(dep_inst) = &func.values[operand]
                    && inst_set.contains(*dep_inst)
                {
                    visit(func, *dep_inst, inst_set, visited, result);
                }
            }
            result.push(inst_id);
        }

        for &inst_id in insts {
            visit(func, inst_id, &inst_set, &mut visited, &mut result);
        }

        result
    }
}

fn u256_to_i128(value: U256) -> Option<i128> {
    if value <= U256::from(i128::MAX as u128) { Some(value.to::<u128>() as i128) } else { None }
}

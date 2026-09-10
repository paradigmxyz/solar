//! Function emission, phi edge splitting, and cold-block layout.

use super::{
    BlockId, CfgInfo, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function, FunctionId, FxHashMap,
    FxHashSet, GlobalStackPlan, InstId, InstKind, Label, Liveness, Module, OnceCell,
    OptimizationMode, PhiEliminator, STACK_PHI_LAYOUT_LIMIT, StackModel, StackPhiPlan, Terminator,
    Value, ValueId, cross_block_values, planned_entry_carries,
};
use std::rc::Rc;

impl<'gcx> EvmCodegen<'gcx> {
    /// Splits phi-carrying edges out of multi-successor predecessors when a
    /// phi destination is still read before or on a sibling path.
    ///
    /// A phi's parallel copies are emitted in the predecessor before its
    /// terminator, so a conditional predecessor runs them on the edge that
    /// does not reach the phi as well. That is harmless for ordinary join
    /// phis, whose destinations are dead before the terminator and on the
    /// sibling path, but a loop header may test its old phi result in the
    /// latch terminator or keep it live on the exit. Writing the backedge copy
    /// early then makes the branch observe the next iteration's value.
    /// Rerouting such edges through a fresh jump-only block gives the copies
    /// an unconditional home after the predecessor's branch. Required stack phis also split
    /// switch edges so each tuple shuffle has a jump-only predecessor.
    pub(super) fn split_phi_critical_edges(func: &mut Function, stack_phis: bool) {
        let has_phis = func.blocks.iter().any(|block| {
            block
                .instructions
                .iter()
                .any(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
        });
        if !has_phis {
            return;
        }
        let liveness = Liveness::compute(func);

        let mut splits: Vec<(BlockId, BlockId)> = Vec::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &inst_id in &block.instructions {
                let InstKind::Phi(incoming) = &func.inst(inst_id).kind else { continue };
                let Some(dst) = func.inst_result_value(inst_id) else { continue };
                for &(pred, src) in incoming {
                    let split_switch = stack_phis
                        && matches!(func.blocks[pred].terminator, Some(Terminator::Switch { .. }));
                    if (src == dst && !split_switch) || splits.contains(&(pred, block_id)) {
                        continue;
                    }
                    let Some(terminator) = func.blocks[pred].terminator.as_ref() else { continue };
                    let successors = terminator.successors();
                    if split_switch
                        || terminator.operands().contains(&dst)
                        || successors
                            .iter()
                            .any(|&succ| succ != block_id && liveness.live_in(succ).contains(dst))
                    {
                        splits.push((pred, block_id));
                    }
                }
            }
        }

        // pred -> edge -> succ
        // succ: phi [..., edge: source, ...]
        for (pred, succ) in splits {
            let edge = func.alloc_block();
            func.blocks[edge].terminator = Some(Terminator::Jump(succ));
            func.blocks[edge].predecessors.push(pred);
            match func.blocks[pred].terminator.as_mut() {
                Some(Terminator::Branch { then_block, else_block, .. }) => {
                    if *then_block == succ {
                        *then_block = edge;
                    }
                    if *else_block == succ {
                        *else_block = edge;
                    }
                }
                Some(Terminator::Switch { default, cases, .. }) => {
                    if *default == succ {
                        *default = edge;
                    }
                    for (_, target) in cases {
                        if *target == succ {
                            *target = edge;
                        }
                    }
                }
                _ => continue,
            }
            for pred_entry in &mut func.blocks[succ].predecessors {
                if *pred_entry == pred {
                    *pred_entry = edge;
                }
            }
            let phi_insts: Vec<InstId> = func.blocks[succ]
                .instructions
                .iter()
                .copied()
                .filter(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::Phi(_)))
                .collect();
            for inst_id in phi_insts {
                if let InstKind::Phi(incoming) = &mut func.inst_mut(inst_id).kind {
                    for (incoming_pred, _) in incoming {
                        if *incoming_pred == pred {
                            *incoming_pred = edge;
                        }
                    }
                }
            }
        }
    }

    /// Generates the body of a function.
    pub(super) fn generate_function_body(&mut self, func_id: FunctionId, func: &Function) {
        if self.uses_recursive_stack_abi(func_id) {
            self.asm.require_source_memory(true);
        }
        self.forwarding_scratch_observable = self.msize_observed_functions.contains(func_id);
        let stack_only_disabled_at_entry = self.stack_only_function_disabled(func_id);
        let report_missing_spill_home = self.gcx.sess.opts.unstable.assert_planned_edge_spill_home;
        let block_local_liveness =
            self.emitting_entry.then(|| Liveness::compute_block_local_for_codegen(func)).flatten();
        let whole_function_liveness = block_local_liveness.is_none();
        let liveness = block_local_liveness.unwrap_or_else(|| Liveness::compute(func));
        let liveness = &liveness;
        let cross_block_live = OnceCell::new();

        self.spill_hazard_insts = self.compute_spill_hazard_insts(func);

        // Calldata and immutable initcode provide reloads independent of source memory.
        let hazard_recomputable = if self.spill_hazard_insts.is_empty()
            && !self.asm.source_memory_required()
            && !(self.in_constructor && self.preserve_caller_stack)
        {
            DenseBitSet::new_empty(func.num_values())
        } else {
            cross_block_values(func, |value| {
                !matches!(func.value(value), Value::Arg(_))
                    || (!self.in_internal_function
                        && (!self.in_constructor || self.asm.source_memory_required()))
            })
        };

        // Eliminate phis.
        self.block_copies.clear();
        self.elided_insts.clear();
        self.collect_late_gas_operands(func);
        let phi_result = PhiEliminator::analyze(func);
        let has_phis = !phi_result.block_copies.is_empty();
        for (block_id, copies) in phi_result.block_copies {
            self.block_copies.insert(block_id, copies.copies);
        }
        // Stack-phi planning starts with loop analysis, but cannot produce a
        // plan without a phi. Avoid that analysis for the overwhelmingly
        // common phi-free function.
        // The cached plan is keyed on whole-function liveness; a block-local entry gets its own.
        let phi_plan = if !has_phis {
            None
        } else if whole_function_liveness {
            Some(self.stack_phi_plan(func_id, func, liveness))
        } else {
            Some(Rc::new(StackPhiPlan::analyze(func, liveness, &self.cold_functions)))
        };
        let mut stack_phi_plan =
            phi_plan.as_deref().map_or_else(StackPhiPlan::default, StackPhiPlan::clone);
        let resident_stack_plan = self.resident_stack_plan(func_id).cloned();
        let mut hazard_cross_block_values = self.spill_hazard_cross_block_values(
            func_id,
            func,
            liveness,
            &cross_block_live,
            &hazard_recomputable,
        );
        self.spill_hazard_values = DenseBitSet::new_empty(func.num_values());
        for &value in &hazard_cross_block_values {
            self.spill_hazard_values.insert(value);
        }
        // Keep block-local values mandatory in the scheduler and across calls, but
        // only values crossing CFG edges need a global entry layout.
        if !hazard_cross_block_values.is_empty() {
            let cross_block =
                cross_block_live.get_or_init(|| Self::cross_block_live_values(func, liveness));
            hazard_cross_block_values.retain(|value| cross_block.contains(*value));
        }
        let resident_carries_hazards = resident_stack_plan.as_ref().is_some_and(|plan| {
            self.stack_plan_carries_spill_hazards(func, liveness, plan, &hazard_cross_block_values)
        });
        let mut protected_stack_values =
            self.resident_stack_args(func_id).map_or_else(Vec::new, |values| values.to_vec());
        for &value in &hazard_cross_block_values {
            if !protected_stack_values.contains(&value) {
                protected_stack_values.push(value);
            }
        }
        let hazard_stack_layout = (!hazard_cross_block_values.is_empty()
            && !resident_carries_hazards)
            .then(|| {
                self.compute_spill_hazard_stack_layout(func, liveness, &protected_stack_values)
            })
            .flatten();
        if !hazard_cross_block_values.is_empty()
            && hazard_stack_layout.is_none()
            && !resident_carries_hazards
        {
            self.gcx
                .dcx()
                .err(format!(
                    "codegen cannot preserve values without compiler memory in `{}`",
                    func.name
                ))
                .emit();
            return;
        }
        let hazard_stack_values = hazard_stack_layout.as_ref().map(|(values, _)| values.as_slice());
        let hazard_stack_plan =
            hazard_stack_layout.as_ref().map(|(_, plan)| plan.clone()).or_else(|| {
                if resident_carries_hazards { resident_stack_plan.clone() } else { None }
            });
        let has_hazard_stack_plan = hazard_stack_plan.is_some();
        let required_stack_plan = resident_stack_plan.is_some() || hazard_stack_plan.is_some();
        let mut global_stack_plan = hazard_stack_plan
            .clone()
            .or(resident_stack_plan)
            .unwrap_or_else(|| GlobalStackPlan::analyze(func, liveness, &stack_phi_plan));
        let mut stack_phi_sources = stack_phi_plan.edge_sources();
        if required_stack_plan {
            if !stack_phi_plan.merge_resident(func, &global_stack_plan) {
                // Selection preflights this exact composition. If a future transform invalidates
                // that proof, regenerate the runtime with the ordinary frame-backed convention
                // instead of emitting a partial stack ABI or panicking.
                self.disabled_stack_only_functions.insert(func_id);
                return;
            }
            stack_phi_sources = stack_phi_plan.edge_sources();
        } else if global_stack_plan.is_empty()
            && let Some((values, plan)) =
                self.compute_cross_block_stack_layout(
                    func,
                    liveness,
                    &cross_block_live,
                    phi_plan,
                )
            // Phi layouts own their incoming stack on planned joins. Adopt the layout only when
            // that composition is proven, mirroring the resident arm.
            && stack_phi_plan.merge_resident(func, &plan)
        {
            global_stack_plan = plan;
            // An early spill store can be omitted only when every physical successor layout
            // carries the value; an edge-specific cleanup may otherwise discard the sole stack
            // copy before a later block reloads its reserved slot. The reserved spill slot
            // remains available if edge emission ever falls back to the memory convention.
            stack_phi_sources = stack_phi_plan.edge_sources();
            for (block_id, block) in func.blocks.iter_enumerated() {
                let Some(term) = block.terminator.as_ref() else { continue };
                let carried = global_stack_plan.uniformly_carried_values(func, term);
                let sources = stack_phi_sources.entry(block_id).or_default();
                for &value in &values {
                    if carried.contains(&value) && !sources.contains(&value) {
                        sources.push(value);
                    }
                }
            }
        }
        // A planned edge consumes its stack sources, so the emitter skips
        // their spill stores. A source with uses beyond the edge's phis —
        // live past the successor or read by one of its own instructions —
        // loses its only stack copy to the edge, and a later block reloads
        // its reserved slot: the store must stay unless a planned successor
        // layout keeps the value stack-resident. A materialization loop's
        // base pointer feeding a second copy loop reloaded uninitialized
        // memory this way. Phi operands count as uses in the merge block, so
        // `live_in` alone cannot separate edge-only sources from these.
        //
        // A successor's entry layout only proves one hop: the value must be
        // carried by the planned entry of EVERY block where it stays live, or
        // an edge cleanup on some uncovered path drops the sole stack copy
        // and a later block reloads its reserved slot uninitialized (a
        // diamond over a multi-return value followed by an internal call read
        // stale decode scratch this way).
        let carried_at = |block: BlockId, value: ValueId| {
            stack_phi_plan.entries.get(&block).is_some_and(|entry| entry.contains(&value))
                || global_stack_plan.entries.get(&block).is_some_and(|entry| entry.contains(&value))
        };
        let mut carried_everywhere = FxHashMap::<ValueId, bool>::default();
        for (block_id, sources) in &mut stack_phi_sources {
            let Some(term) = func.blocks[*block_id].terminator.as_ref() else { continue };
            let successors = term.successors();
            sources.retain(|&value| {
                successors.iter().all(|&succ| {
                    if carried_at(succ, value)
                        && *carried_everywhere.entry(value).or_insert_with(|| {
                            func.blocks.indices().all(|block| {
                                !liveness.live_in(block).contains(value) || carried_at(block, value)
                            })
                        })
                    {
                        return true;
                    }
                    let block = &func.blocks[succ];
                    let used_past_phis = liveness.live_out(succ).contains(value)
                        || block.instructions.iter().any(|&inst_id| {
                            let inst = func.inst(inst_id);
                            !matches!(inst.kind, InstKind::Phi(_))
                                && inst.kind.operands().contains(&value)
                        })
                        || block
                            .terminator
                            .as_ref()
                            .is_some_and(|term| term.operands().contains(&value));
                    !used_past_phis
                })
            });
        }
        if has_hazard_stack_plan {
            for (block_id, block) in func.blocks.iter_enumerated() {
                let Some(term) = block.terminator.as_ref() else { continue };
                let sources = stack_phi_sources.entry(block_id).or_default();
                for successor in term.successors() {
                    for &value in global_stack_plan.entry(successor).into_iter().flatten() {
                        if !sources.contains(&value) {
                            sources.push(value);
                        }
                    }
                }
            }
        }
        self.stack_phi_sources = stack_phi_sources;
        self.global_stack_active = !global_stack_plan.is_empty();
        self.global_stack_aliases = global_stack_plan.aliases.clone();

        // Reset scheduler
        self.scheduler.reset();
        self.spill_addr_consts.clear();
        self.spill_stores.clear();
        self.spill_loads.clear();
        self.early_spill_removals.clear();
        self.function_ir_block_start = self.asm.block_count();

        // Cross-block rematerialization is selected during spill preallocation. Record every
        // argument without a frame home before that analysis so an expression depending on one is
        // stored instead of later being rebuilt after its only physical copy was consumed.
        let mut initial_stack_only_values = self.stack_only_values(func_id, true);
        initial_stack_only_values.extend(hazard_stack_values.into_iter().flatten().copied());
        self.scheduler.set_stack_only_values(func.num_values(), initial_stack_only_values);

        self.preallocate_cross_block_spills(func, liveness, &cross_block_live);
        for value in &hazard_recomputable {
            if matches!(func.value(value), Value::Inst(_)) {
                self.scheduler.spills.mark_recompute_only(value);
            }
        }

        self.cold_blocks = self.collect_cold_blocks(func);

        // Create labels for each block
        self.block_labels.clear();
        for block_id in func.blocks.indices() {
            let label = self.asm.new_label();
            if self.block_is_cold(block_id) {
                self.asm.mark_label_cold(label);
            }
            self.block_labels.insert(block_id, label);
        }

        // Generate each block.
        let block_order = self.block_layout_order(func);
        let block_pos: FxHashMap<BlockId, usize> =
            block_order.iter().enumerate().map(|(pos, &b)| (b, pos)).collect();
        // Stack layout a block must start with when it is reached by a stack-
        // preserving jump from its single predecessor (recorded by that
        // predecessor, restored here).
        let mut block_entry_stacks: FxHashMap<BlockId, StackModel> = FxHashMap::default();
        let mut preserved_fallthrough: Option<BlockId> = None;
        // A spill store is a path fact, not a function fact: a store emitted
        // in one branch arm must not satisfy reloads on paths that bypass it.
        // Track store availability per emitted block — a slot is trustworthy
        // at a block only when every forward predecessor makes it available —
        // and drop the scheduler's stored guarantee where a live value is not
        // available, so that path stores again before any reload.
        let store_cfg = CfgInfo::new(func);
        let mut spill_avail_out: FxHashMap<BlockId, FxHashSet<ValueId>> = FxHashMap::default();
        for (pos, &block_id) in block_order.iter().enumerate() {
            let block = &func.blocks[block_id];
            let fallthrough = block_order.get(pos + 1).copied();
            if self.capture_debug_info {
                let modifier_depth = block
                    .instructions
                    .first()
                    .map(|&inst_id| func.inst(inst_id).metadata.modifier_depth())
                    .unwrap_or(0);
                self.asm.set_modifier_depth(modifier_depth);
            }
            let entered_by_preserved_fallthrough = preserved_fallthrough == Some(block_id);
            preserved_fallthrough = None;
            let label = self.block_labels[&block_id];
            if !entered_by_preserved_fallthrough && !block.predecessors.is_empty() {
                self.asm.define_label(label);
            }

            // Reset stack at block entry unless the block is reached with a
            // known live stack: a physical fallthrough carries the scheduler's
            // stack directly, and a stack-preserving jump from a single
            // predecessor restores the recorded layout. All other cross-block
            // values live in spill slots.
            if !entered_by_preserved_fallthrough {
                if let Some(entry_stack) = block_entry_stacks.remove(&block_id) {
                    let max_depth = self.scheduler.stack.max_depth();
                    self.scheduler.stack = entry_stack;
                    self.scheduler.stack.inherit_max_depth(max_depth);
                    // Live-ins not on the carried stack still arrive in memory.
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = stack_phi_plan.entries.get(&block_id) {
                    self.set_stack_to_values(entry);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else if let Some(entry) = global_stack_plan.entry(block_id) {
                    self.set_stack_to_values(entry);
                    self.mark_live_in_spills(func, liveness, block_id);
                } else {
                    self.scheduler.clear_stack();
                    self.mark_live_in_spills(func, liveness, block_id);
                }
            }
            // A spill store is a path fact, not a function fact: a store
            // emitted in a sibling branch arm sets the global `stored` flag,
            // which must not suppress this path's own store. Intersect store
            // availability over emitted forward predecessors (loop back edges
            // are exempt: a value redefined around a loop is handled by the
            // carried-phi invalidation, and its pre-loop store stays valid; a
            // predecessor emitted later stores every value live across the edge
            // before jumping here, including the ones a preserved stack edge
            // would otherwise leave to this block).
            let mut avail_in: Option<FxHashSet<ValueId>> = None;
            for &pred in func.blocks[block_id].predecessors.iter() {
                if store_cfg.dominators().dominates(block_id, pred) {
                    continue;
                }
                let pred_avail = spill_avail_out.get(&pred);
                match (&mut avail_in, pred_avail) {
                    (None, Some(pred_avail)) => avail_in = Some(pred_avail.clone()),
                    (Some(set), Some(pred_avail)) => {
                        set.retain(|value| pred_avail.contains(value));
                    }
                    (_, None) => {}
                }
            }
            self.spill_available = avail_in;
            // A store emitted by a sibling branch arm also marked its value
            // reloadable, so a copy carried in on the stack could be dropped
            // in favor of a slot this path never wrote. Forget stores that are
            // not available on every emitted forward predecessor.
            if let Some(available) = &self.spill_available {
                let stale: Vec<ValueId> = self
                    .scheduler
                    .spills
                    .reloadable_values()
                    .filter(|&value| {
                        !available.contains(&value) && self.scheduler.stack.contains(value)
                    })
                    .collect();
                for value in stale {
                    self.scheduler.spills.invalidate_stored(value);
                }
            }
            self.invalidate_carried_phi_spills(func, block_id);
            if block_id == BlockId::ENTRY
                && let Some(values) =
                    self.resident_stack_args(func_id).map(|values| values.to_vec())
            {
                debug_assert_eq!(self.scheduler.stack.depth(), 0);
                self.set_stack_to_values(&values);
            } else if block_id == BlockId::ENTRY
                && let Some(values) = self.direct_stack_args(func_id).map(|values| values.to_vec())
            {
                debug_assert_eq!(self.scheduler.stack.depth(), 0);
                self.set_stack_to_values(&values);
            } else if block_id == BlockId::ENTRY
                && let Some(plan) = self.lazy_stack_args(func_id).cloned()
            {
                debug_assert_eq!(self.scheduler.stack.depth(), 0);
                self.set_stack_to_values(&plan.values().collect::<Vec<_>>());
            }
            // Resident and direct arguments have no frame fallback.
            let mut stack_only_values = self.stack_only_values(func_id, block_id == BlockId::ENTRY);
            if block_id == BlockId::ENTRY {
                self.scheduler
                    .set_stack_only_values(func.num_values(), stack_only_values.iter().copied());
                for &value in hazard_stack_values.into_iter().flatten() {
                    if matches!(func.value(value), Value::Arg(_))
                        && !self.scheduler.stack.contains(value)
                    {
                        // push frame(arg); mload
                        self.emit_value(func, value);
                    }
                }
            }
            stack_only_values.extend(self.spill_hazard_values.iter());
            self.scheduler.set_stack_only_values(func.num_values(), stack_only_values);
            if block_id != BlockId::ENTRY
                && self.resident_stack_args(func_id).is_some()
                && !stack_phi_plan.entries.contains_key(&block_id)
            {
                let live_in = liveness.live_in(block_id);
                let needed: Vec<_> = self
                    .scheduler
                    .stack
                    .iter()
                    .flatten()
                    .filter(|value| live_in.contains(*value))
                    .collect();
                self.pop_stack_values_not_needed_by(&needed);
            }

            // Generate instructions
            let mut pinned_hazard_values = FxHashSet::<ValueId>::default();
            for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                let inst = func.inst(inst_id);

                // Skip phi instructions (they're handled by copies)
                if matches!(inst.kind, InstKind::Phi(_)) {
                    continue;
                }

                // Skip multi-return protocol instructions already satisfied
                // from adopted stack-return words.
                if self.elided_insts.remove(&inst_id) {
                    continue;
                }

                if self.capture_debug_info {
                    self.asm.set_source_spans(inst.metadata.source_spans());
                    self.asm.set_modifier_depth(inst.metadata.modifier_depth());
                }

                // A whole-calldata-forwarding clobber overwrites the low memory
                // the spill area lives in. Reload every value live across it
                // onto the stack while the slot is still valid, and drop the
                // stored flag so nothing reloads the clobbered slot. The slot is
                // NOT re-stored here: the clobbered range is the very memory the
                // following forward reads, so writing it back would corrupt the
                // forwarded input. The value rides the stack, which the write
                // never touches; live-out values re-store once at block end.
                // Only values still needed past the clobber are pinned: a phi
                // source or an operand the copy itself consumes has its last
                // recorded use in this block at or before it, and reloading it
                // would only deepen the stack with a dead word.
                if self.spill_hazard_insts.contains(&inst_id) {
                    let at_risk: Vec<ValueId> = self
                        .scheduler
                        .spills
                        .reloadable_values()
                        .filter(|&value| {
                            liveness.is_used_at_or_after(value, block_id, inst_idx + 1)
                        })
                        .collect();
                    for value in at_risk {
                        let recomputable = self.scheduler.spills.is_recomputable(value);
                        if !recomputable {
                            if !self.scheduler.stack.contains(value) {
                                self.emit_value(func, value);
                            }
                            pinned_hazard_values.insert(value);
                        }
                        self.scheduler.spills.invalidate_stored(value);
                        if let Some(available) = &mut self.spill_available {
                            available.remove(&value);
                        }
                    }
                }

                // Find the value ID that corresponds to this instruction (if any)
                let result_value = func.inst_result_value(inst_id);

                // Generate the instruction
                self.generate_inst(
                    func_id,
                    inst_id,
                    func,
                    &inst.kind,
                    liveness,
                    block_id,
                    inst_idx,
                    result_value,
                );
                if !stack_only_disabled_at_entry && self.stack_only_function_disabled(func_id) {
                    return;
                }
                if let Some(result) = result_value {
                    self.spill_reserved_result_if_live(func, liveness, block_id, inst_idx, result);
                    // A free-memory-pointer load cannot be rematerialized once
                    // the pointer moves. Park every FMP load at its
                    // definition so later uses reload the original value —
                    // whether the definition crosses a block on a preserved
                    // edge or is re-materialized between two allocations in
                    // its own block.
                    if matches!(
                        inst.kind,
                        InstKind::MLoad(addr)
                            if func.value_u64(addr) == Some(EvmMemoryLayout::FMP_SLOT)
                    ) {
                        self.spill_value_if_needed(func, result);
                    }
                }
            }
            if self.capture_debug_info {
                self.asm.set_source_span(None);
            }

            // Re-store only values a successor reloads; carried values retain their stack copy.
            if !pinned_hazard_values.is_empty() {
                let live_out = liveness.live_out(block_id);
                let hazard_carried = block.terminator.as_ref().map_or_else(Vec::new, |term| {
                    hazard_stack_plan
                        .as_ref()
                        .map_or_else(Vec::new, |plan| plan.uniformly_carried_values(func, term))
                });
                for value in std::mem::take(&mut pinned_hazard_values) {
                    if live_out.contains(value) && !hazard_carried.contains(&value) {
                        self.spill_value_if_needed(func, value);
                    }
                }
            }

            // A preserved stack edge hands its carried values to the successor,
            // which stores the ones it drops. An already-emitted successor
            // chose those stores without seeing this path, so it cannot take
            // the obligation over: store everything live across the edge here,
            // which is what the store-availability intersection assumes a
            // later-emitted predecessor does. Back edges are exempt for the
            // same reason that intersection skips them.
            //
            // Only a planned edge can preserve into an already-emitted block:
            // its successor rebuilds the layout from the plan at its own entry,
            // whichever order the two were emitted in. Fallthrough, jump, and
            // branch preservation all require a single-predecessor target that
            // is still ahead, other than an arm whose live-ins are immediates,
            // and an unpreserved terminator spills every live-out value at
            // block end regardless, so storing here would only move that store
            // earlier at the cost of a deeper `dup`.
            //
            //   dup depth(value)
            //   push slot(value)
            //   mstore
            let planned_stack_edge = stack_phi_plan.edges.contains_key(&block_id)
                || stack_phi_plan.branch_edges.contains_key(&block_id)
                || block.terminator.as_ref().is_some_and(|term| {
                    global_stack_plan.edge_layout(func, term).is_some()
                        || global_stack_plan.branch_layouts(term).is_some()
                        || global_stack_plan.switch_layouts(term).is_some()
                });
            let emitted_successors = block
                .terminator
                .as_ref()
                .filter(|_| planned_stack_edge)
                .map(Terminator::successors)
                .unwrap_or_default();
            for successor in emitted_successors {
                if block_pos.get(&successor).is_some_and(|&target| target < pos)
                    && !store_cfg.dominators().dominates(successor, block_id)
                {
                    let live_in = liveness.live_in(successor);
                    for value in liveness.live_out(block_id) {
                        if live_in.contains(value) {
                            // `spill_value_if_needed` stores nothing for a value that is
                            // neither on the stack nor validly stored, and re-emitting one
                            // here is not an option: the pushed word would deepen the
                            // stack the preserved layout is built from. It never has to.
                            // Such a value is one the plan carries into the successor,
                            // which rebuilds it from its recorded entry layout and wants no
                            // memory home; anything that has to travel in memory is still
                            // on the stack, already stored, or holds its slot on every
                            // emitted path into this block at this point.
                            //
                            // A value that arrives only from a forward predecessor further
                            // down the stream is the exception: that predecessor stores its
                            // live-out values when it is emitted, later in the stream but
                            // earlier at runtime, so the home is owed rather than missing
                            // and there is nothing to check here yet.
                            //
                            // This is a description of the common cases and not an
                            // invariant the scheduler maintains, so it is logged rather
                            // than asserted. A value that is on this block's stack is
                            // stored right below, and one that is not cannot get a home
                            // here whatever the reason is: pushing it again would deepen
                            // the stack the preserved layout is built from. The store
                            // obligation is then not this block's, and stating whose it is
                            // needs the availability record of the paths this emission
                            // order has not walked yet.
                            //
                            // The check itself walks predecessors with dominance queries,
                            // so it only runs when something can observe its result.
                            if (report_missing_spill_home
                                || tracing::enabled!(tracing::Level::DEBUG))
                                && !self.has_spill_home(func, value)
                                && !planned_entry_carries(
                                    &stack_phi_plan,
                                    &global_stack_plan,
                                    successor,
                                    value,
                                )
                                && Self::forward_predecessors_emitted(
                                    func, &store_cfg, &block_pos, block_id, pos,
                                )
                            {
                                assert!(
                                    !report_missing_spill_home,
                                    "{value:?} lives across the edge from {block_id:?} to \
                                     the already-emitted {successor:?} with no home"
                                );
                                tracing::debug!(
                                    ?value,
                                    ?block_id,
                                    ?successor,
                                    "value lives across the edge to an already-emitted \
                                     block with no home"
                                );
                            }
                            self.spill_value_if_needed(func, value);
                        }
                    }
                }
            }

            let terminator_growth =
                block.terminator.as_ref().map_or(0, Self::terminator_transient_growth);
            self.materialize_deep_stack_args(func_id, func, terminator_growth);
            if !stack_only_disabled_at_entry && self.stack_only_function_disabled(func_id) {
                return;
            }

            let stack_phi_preserved = stack_phi_plan.edges.get(&block_id).is_some_and(|edge| {
                if !self.can_prepare_stack_phi_edge(func, edge) {
                    return false;
                }
                self.spill_live_out_values_except(func, liveness, block_id, &edge.results);
                self.pop_stack_values_not_needed_by(&edge.sources);
                self.try_emit_stack_phi_edge(func, edge)
            });
            let stack_phi_branch_preserved = block
                .terminator
                .as_ref()
                .and_then(|term| {
                    let Terminator::Branch { condition, .. } = term else { return None };
                    stack_phi_plan.branch_edges.get(&block_id).map(|branch| (*condition, branch))
                })
                .is_some_and(|(condition, branch)| {
                    self.can_prepare_stack_phi_branch(func, condition, branch)
                });

            // Insert phi copies before terminator. If the edge was materialized
            // as a stack-resident phi layout, the copies for this unconditional
            // predecessor are represented by the edge stack itself.
            if stack_phi_preserved || stack_phi_branch_preserved {
                self.block_copies.remove(&block_id);
            } else if let Some(copies) = self.block_copies.remove(&block_id) {
                let mut temps = FxHashMap::default();
                for copy in &copies {
                    self.generate_copy(func, copy, &mut temps);
                }
            }

            let global_branch_layouts = block.terminator.as_ref().and_then(|term| {
                global_stack_plan.branch_layouts(term).and_then(|(then_layout, else_layout)| {
                    (then_layout != else_layout)
                        .then(|| (then_layout.to_vec(), else_layout.to_vec()))
                })
            });
            let global_switch_layouts = block.terminator.as_ref().and_then(|term| {
                global_stack_plan.switch_layouts(term).and_then(|layouts| {
                    let first = layouts.first()?.1;
                    layouts.iter().any(|(_, layout)| *layout != first).then(|| {
                        layouts
                            .into_iter()
                            .map(|(target, layout)| (target, layout.to_vec()))
                            .collect::<Vec<_>>()
                    })
                })
            });
            let has_edge_specific_global =
                global_branch_layouts.is_some() || global_switch_layouts.is_some();

            let preserve_stack_to_fallthrough = !has_edge_specific_global
                && self.can_preserve_stack_fallthrough(func, block_id, fallthrough);

            // A jump to a single-predecessor target that is emitted later can
            // keep its live stack instead of spilling: the target has exactly
            // one entry stack (this block's exit), so it can be restored there.
            let preserve_jump_target = (!has_edge_specific_global
                && !preserve_stack_to_fallthrough)
                .then(|| self.single_pred_jump_target(func, block_id, fallthrough))
                .flatten()
                .filter(|target| block_pos.get(target).copied() > Some(pos));

            // A conditional branch whose other arm is a cold revert can carry
            // its single freshly-computed live-out on the stack into the hot
            // arm, which restores it as its recorded entry layout.
            let preserve_branch_targets = if !has_edge_specific_global
                && !preserve_stack_to_fallthrough
                && preserve_jump_target.is_none()
                && !stack_phi_branch_preserved
            {
                self.branch_preserve_targets(func, liveness, block_id, pos, &block_pos)
            } else {
                Vec::new()
            };
            if !preserve_branch_targets.is_empty()
                && let Some(Terminator::Branch { condition, .. }) = block.terminator.as_ref()
                && liveness.live_out(block_id).contains(*condition)
            {
                // JUMPI consumes the condition while the preserved successor layout omits it.
                // Save an instruction result that remains live so either successor can reload
                // the same definition instead of observing an unwritten reserved spill slot.
                self.spill_value_if_needed(func, *condition);
            }
            if !preserve_branch_targets.is_empty() {
                self.remove_dead_carried_spill_stores(
                    func,
                    liveness,
                    block_id,
                    &preserve_branch_targets,
                );
            }

            let global_branch_preserved = if !stack_phi_preserved
                && !stack_phi_branch_preserved
                && let Some((then_layout, else_layout)) = &global_branch_layouts
                && let Some(Terminator::Branch { condition, .. }) = block.terminator.as_ref()
            {
                let union = Self::global_branch_union(then_layout, else_layout);
                self.spill_live_out_values_except(func, liveness, block_id, &union);
                self.try_emit_global_stack_branch(func, *condition, then_layout, else_layout)
            } else {
                None
            };

            let global_switch_preserved = if !stack_phi_preserved
                && !stack_phi_branch_preserved
                && global_branch_preserved.is_none()
                && let Some(layouts) = &global_switch_layouts
                && let Some(term @ Terminator::Switch { .. }) = block.terminator.as_ref()
            {
                let union = Self::global_switch_union(layouts);
                self.spill_live_out_values_except(func, liveness, block_id, &union);
                self.try_emit_global_stack_edge(func, term, &union).then_some(union)
            } else {
                None
            };

            let global_stack_preserved = if global_branch_preserved.is_none()
                && global_switch_preserved.is_none()
                && !preserve_stack_to_fallthrough
                && preserve_jump_target.is_none()
                && preserve_branch_targets.is_empty()
                && !stack_phi_preserved
                && !stack_phi_branch_preserved
                && let Some(term) = block.terminator.as_ref()
                && let Some(layout) = global_stack_plan.edge_layout(func, term)
            {
                self.spill_live_out_values_except(func, liveness, block_id, layout);
                self.try_emit_global_stack_edge(func, term, layout)
            } else {
                false
            };

            let preserve_stack = preserve_stack_to_fallthrough
                || preserve_jump_target.is_some()
                || !preserve_branch_targets.is_empty()
                || stack_phi_preserved
                || stack_phi_branch_preserved
                || global_branch_preserved.is_some()
                || global_switch_preserved.is_some()
                || global_stack_preserved;

            // Spill all live-out values before the terminator so they can be reloaded in successor
            // blocks. For a preserved edge, keep stack values live instead.
            if stack_phi_branch_preserved
                && let Some(branch) = stack_phi_plan.branch_edges.get(&block_id)
                && let Some(Terminator::Branch { then_block, else_block, .. }) =
                    block.terminator.as_ref()
            {
                let arms = [(*then_block, &branch.then_edge), (*else_block, &branch.else_edge)];
                let exempt = branch
                    .union
                    .iter()
                    .copied()
                    .filter(|value| {
                        arms.iter().all(|(arm, edge)| {
                            !liveness.live_in(*arm).contains(*value) || edge.results.contains(value)
                        })
                    })
                    .collect::<Vec<_>>();
                self.spill_live_out_values_except(func, liveness, block_id, &exempt);
            } else if !preserve_stack {
                self.spill_live_out_values(func, liveness, block_id);
            }

            // Generate terminator. An edge-specific resident branch owns its cleanup and jumps.
            if self.capture_debug_info {
                let metadata = &block.terminator_metadata;
                self.asm.set_source_spans(metadata.source_spans());
                self.asm.set_modifier_depth(metadata.modifier_depth());
            }
            if let (
                Some(union),
                Some((then_layout, else_layout)),
                Some(Terminator::Branch { condition, then_block, else_block }),
            ) = (&global_branch_preserved, &global_branch_layouts, &block.terminator)
            {
                self.emit_global_stack_branch(
                    func,
                    *condition,
                    *then_block,
                    *else_block,
                    then_layout,
                    else_layout,
                    union,
                    fallthrough,
                );
            } else if let (
                Some(union),
                Some(layouts),
                Some(Terminator::Switch { value, default, cases }),
            ) = (&global_switch_preserved, &global_switch_layouts, &block.terminator)
            {
                self.emit_global_stack_switch(func, *value, *default, cases, layouts, union);
            } else if stack_phi_branch_preserved
                && let Some(Terminator::Branch { condition, then_block, else_block }) =
                    block.terminator.as_ref()
                && let Some(branch) = stack_phi_plan.branch_edges.get(&block_id)
            {
                self.emit_stack_phi_branch(
                    func,
                    *condition,
                    *then_block,
                    *else_block,
                    branch,
                    fallthrough,
                );
            } else if let Some(term) = &block.terminator {
                self.generate_terminator(func, term, fallthrough, preserve_stack);
            }
            if self.capture_debug_info {
                self.asm.set_source_span(None);
                self.asm.set_modifier_depth(0);
            }
            self.scheduler.spills.release_block_locals();
            if preserve_stack_to_fallthrough {
                preserved_fallthrough = fallthrough;
            } else if let Some(target) = preserve_jump_target {
                block_entry_stacks.insert(target, self.scheduler.stack.clone());
            }
            for target in preserve_branch_targets {
                let mut entry_stack = self.scheduler.stack.clone();
                // The branch has one physical exit stack, but a cold successor need not retain
                // identities used only by its hot sibling. Keep hot layouts exact: anonymizing
                // their dead slots can turn a loop-carried stack hit into a reload every iteration.
                if self.block_is_cold(target) {
                    let live_in = liveness.live_in(target);
                    entry_stack.forget_values_not_matching(|value| live_in.contains(value));
                }
                block_entry_stacks.insert(target, entry_stack);
            }

            spill_avail_out.insert(
                block_id,
                self.spill_available
                    .clone()
                    .unwrap_or_else(|| self.scheduler.spills.stored_values().collect()),
            );
        }

        let mut peak = self.scheduler.stack.max_depth();
        if self.direct_stack_args(func_id).is_none()
            && self.resident_stack_args(func_id).is_none()
            && self.lazy_stack_args(func_id).is_none()
            && let Some(mask) = self.stack_arg_mask(func_id)
        {
            peak = peak.max(mask.count());
        }
        self.function_stack_peaks.insert(func_id, peak);
        self.remove_dead_spill_stores();
        self.assign_ranked_spill_addrs(func_id);
    }

    /// Returns the target of a stack-preservable jump: the block ends in
    /// `Jump(T)` to a non-fallthrough, single-predecessor block with no phis
    /// (whose copies would otherwise interfere with the carried layout).
    fn single_pred_jump_target(
        &self,
        func: &Function,
        block_id: BlockId,
        fallthrough: Option<BlockId>,
    ) -> Option<BlockId> {
        let Some(Terminator::Jump(target)) = func.blocks[block_id].terminator.as_ref() else {
            return None;
        };
        if Some(*target) == fallthrough
            || func.blocks[*target].predecessors.as_slice() != [block_id]
        {
            return None;
        }
        let has_phi = func.blocks[*target]
            .instructions
            .iter()
            .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
        (!has_phi).then_some(*target)
    }

    /// Returns branch successors that can receive the current stack layout.
    ///
    /// This handles loop headers after stack-resident phi planning: the header
    /// computes the branch condition while the carried phi values remain below
    /// it. If both successors are private, later blocks, we can leave those
    /// values on the stack for both edges instead of spilling them before every
    /// loop condition.
    fn branch_preserve_targets(
        &self,
        func: &Function,
        liveness: &Liveness,
        block_id: BlockId,
        pos: usize,
        block_pos: &FxHashMap<BlockId, usize>,
    ) -> Vec<BlockId> {
        let Some(Terminator::Branch { condition, then_block, else_block }) =
            func.blocks[block_id].terminator.as_ref()
        else {
            return Vec::new();
        };

        if self.scheduler.stack.depth() <= 1 || self.scheduler.stack.top() != Some(*condition) {
            return Vec::new();
        }

        let Some(carried) = self
            .scheduler
            .stack
            .iter()
            .skip(1)
            .map(|slot| {
                let value = slot?;
                liveness.live_out(block_id).contains(value).then_some(value)
            })
            .collect::<Option<Vec<_>>>()
        else {
            return Vec::new();
        };
        if carried.len() > STACK_PHI_LAYOUT_LIMIT {
            return Vec::new();
        }

        // Consuming the branch condition removes the top stack word. A resident argument has no
        // frame fallback, so every stack-only live-out must already have another carried copy
        // below the condition. Otherwise let the global stack-edge planner duplicate and arrange
        // the condition together with the successor layout.
        if liveness
            .live_out(block_id)
            .iter()
            .any(|value| self.scheduler.is_stack_only_value(value) && !carried.contains(&value))
        {
            return Vec::new();
        }

        let targets = [*then_block, *else_block];
        let mut live_in_any_target = DenseBitSet::new_empty(func.num_values());
        for target in targets {
            for value in liveness.live_in(target) {
                live_in_any_target.insert(value);
            }
        }
        if carried.iter().any(|&value| !live_in_any_target.contains(value)) {
            return Vec::new();
        }

        let mut preserved = Vec::with_capacity(2);
        for target in targets {
            if target == block_id {
                return Vec::new();
            }
            let has_phi = func.blocks[target]
                .instructions
                .iter()
                .any(|&inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
            if func.blocks[target].predecessors.as_slice() == [block_id]
                && block_pos.get(&target).copied() > Some(pos)
                && !has_phi
            {
                preserved.push(target);
                continue;
            }
            if !has_phi && self.is_junk_tolerant_terminal(func, liveness, target) {
                continue;
            }
            return Vec::new();
        }

        preserved
    }

    fn is_junk_tolerant_terminal(
        &self,
        func: &Function,
        liveness: &Liveness,
        block: BlockId,
    ) -> bool {
        liveness.live_in(block).iter().all(|value| matches!(func.value(value), Value::Immediate(_)))
            && match func.blocks[block].terminator {
                Some(
                    Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid,
                ) => true,
                Some(Terminator::TailCall { function, .. }) => {
                    self.cold_functions.contains(function)
                }
                _ => false,
            }
    }

    /// Finds functions whose reachable exits all abort, including cold call chains.
    pub(super) fn collect_cold_functions(module: &Module) -> DenseBitSet<FunctionId> {
        crate::mir::analysis::CallGraphInfo::collect_cold_functions(module)
    }

    /// Finds blocks that abort directly or can only reach other cold blocks.
    fn collect_cold_blocks(&self, func: &Function) -> DenseBitSet<BlockId> {
        let mut cold = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = Vec::new();
        for block_id in func.blocks.indices() {
            if self.block_aborts(func, block_id) {
                cold.insert(block_id);
                worklist.push(block_id);
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return cold;
        }

        while let Some(block_id) = worklist.pop() {
            for &predecessor in &func.blocks[block_id].predecessors {
                if cold.contains(predecessor) {
                    continue;
                }
                let Some(term) = func.blocks[predecessor].terminator.as_ref() else {
                    continue;
                };
                let successors = term.successors();
                if !successors.is_empty()
                    && successors.iter().all(|&successor| cold.contains(successor))
                {
                    cold.insert(predecessor);
                    worklist.push(predecessor);
                }
            }
        }
        cold
    }

    /// Returns true when a block aborts directly or calls a function whose
    /// reachable exits all abort.
    fn block_aborts(&self, func: &Function, block_id: BlockId) -> bool {
        let block = &func.blocks[block_id];
        matches!(
            block.terminator,
            Some(Terminator::Revert { .. } | Terminator::RevertReturndata | Terminator::Invalid)
        ) || matches!(
            block.terminator,
            Some(Terminator::TailCall { function, .. })
                if self.cold_functions.contains(function)
        ) || block.instructions.iter().any(|&inst_id| {
            matches!(
                func.inst(inst_id).kind,
                InstKind::ICall { function, .. }
                    if self.cold_functions.contains(function)
            )
        })
    }

    pub(super) fn block_is_cold(&self, block_id: BlockId) -> bool {
        self.cold_blocks.contains(block_id)
    }

    pub(super) fn new_function_label(&mut self, function: FunctionId) -> Label {
        let label = self.asm.new_label();
        if self.cold_functions.contains(function) {
            self.asm.mark_label_cold(label);
        }
        label
    }

    fn block_layout_order(&self, func: &Function) -> Vec<BlockId> {
        // Layout only initializes reachability; RPO, dominators, and
        // transitive reachability remain unevaluated.
        let cfg = CfgInfo::new(func);
        let reachable = cfg.reachable();
        let mut order = Vec::with_capacity(func.blocks.len());
        let mut placed = DenseBitSet::new_empty(func.blocks.len());

        self.append_layout_chain(func, BlockId::ENTRY, reachable, &mut placed, &mut order);
        for block_id in func.blocks.indices() {
            if reachable.contains(block_id) {
                self.append_layout_chain(func, block_id, reachable, &mut placed, &mut order);
            }
        }

        order
    }

    fn append_layout_chain(
        &self,
        func: &Function,
        mut block_id: BlockId,
        reachable: &DenseBitSet<BlockId>,
        placed: &mut DenseBitSet<BlockId>,
        order: &mut Vec<BlockId>,
    ) {
        loop {
            if !reachable.contains(block_id) || !placed.insert(block_id) {
                return;
            }
            order.push(block_id);

            let target = match func.blocks[block_id].terminator.as_ref() {
                Some(Terminator::Jump(target))
                    if func.blocks[*target].predecessors.as_slice() == [block_id] =>
                {
                    *target
                }
                Some(Terminator::Branch { then_block, else_block, .. })
                    if !matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) =>
                {
                    match (self.block_is_cold(*then_block), self.block_is_cold(*else_block)) {
                        (true, false) => *else_block,
                        (false, true) => *then_block,
                        _ => return,
                    }
                }
                _ => return,
            };
            if placed.contains(target) {
                return;
            }

            block_id = target;
        }
    }
}

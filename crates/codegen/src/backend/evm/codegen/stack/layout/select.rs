//! Selection of profitable cross-block stack layouts under spill hazards.
//!
//! Gas mode also retains a calldata loop bound beneath three or four stack-resident phis when
//! the loop has one pure, straight-line latch of at most sixteen instructions. The resident
//! argument planner proves incoming layouts and liveness; the phi planner proves that its
//! changing words fit above the invariant. Calls and uncomposable layouts keep reloads.
//! This replaces repeated calldata loads with DUPs, trading a small setup and bytecode cost
//! for cheaper iterations. It assumes repeated traversal for profitability, not correctness:
//! zero-iteration calls can cost more. Size mode retains rematerialization. This decision
//! stays at the scheduling boundary and neither changes the MIR recurrence nor moves loads
//! across external calls.

use super::super::super::{
    BlockId, CanonicalArgValues, CfgInfo, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function,
    FunctionId, FxHashMap, FxHashSet, GLOBAL_STACK_LAYOUT_LIMIT, GlobalStackPlan, InstKind,
    Liveness, LoopAnalyzer, Module, OnceCell, OperandCostModel, OptimizationMode,
    ResidentSearchContext, ScheduleCost, StackOp, StackPhiPlan, Terminator, Value, ValueId,
};
use crate::target::Target;
use std::rc::Rc;

impl<'gcx> EvmCodegen<'gcx> {
    /// Carries a calldata loop bound beneath a small pure loop's changing words.
    pub(in crate::backend::evm::codegen) fn compute_loop_bound_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        phi_plan: &StackPhiPlan,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if !self.gcx.sess.opts.optimization.is_gas() || !Self::is_external_entry(func) {
            return None;
        }
        for (header, block) in func.blocks.iter_enumerated() {
            if let Some(Terminator::Branch { condition, then_block: body, .. }) = &block.terminator
                && let Value::Inst(cond) = func.value(*condition)
                && let InstKind::Lt(index, bound) = func.inst(*cond).kind
                && matches!(func.value(bound), Value::Arg(_))
                && let Some(layout) = phi_plan.entries.get(&header)
                && (3..=4).contains(&layout.len())
                && layout.contains(&index)
                && func.blocks[*body].predecessors.as_slice() == [header]
                && matches!(func.blocks[*body].terminator, Some(Terminator::Jump(to)) if to == header)
                && func.blocks[*body].instructions.len() <= 16
                && func.blocks[*body]
                    .instructions
                    .iter()
                    .all(|&inst| func.inst(inst).kind.effect_kind() == crate::mir::EffectKind::Pure)
            {
                // preheader: carry(bound, initial phis...)
                // header: compare(index, bound)
                // latch: carry(bound, next phis...)
                let values = vec![bound];
                let plan = GlobalStackPlan::analyze_resident_args(func, liveness, &values, false)?;
                return Some((values, plan));
            }
        }
        None
    }

    /// Returns the stack-phi plan for a function, computing it on first use.
    pub(in crate::backend::evm::codegen) fn stack_phi_plan(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        liveness: &Liveness,
    ) -> Rc<StackPhiPlan> {
        let cold_functions = &self.cold_functions;
        Rc::clone(self.stack_phi_plans.entry(func_id).or_insert_with(|| {
            Rc::new(StackPhiPlan::analyze(func, liveness, cold_functions, Target::new(self.gcx)))
        }))
    }

    /// Collects the canonical identity of each used static-callee argument once for the stack
    /// argument analyses below. Gas codegen canonicalizes argument operands before runtime
    /// planning, so every active occurrence of one argument must use the same value identity.
    pub(in crate::backend::evm::codegen) fn collect_canonical_stack_arg_values(
        &self,
        module: &Module,
    ) -> FxHashMap<FunctionId, CanonicalArgValues> {
        let mut all_values = FxHashMap::default();
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return all_values;
        }

        for func_id in self.static_frame_functions.iter() {
            let func = &module.functions[func_id];
            if func.params.is_empty() {
                continue;
            }
            let mut values = CanonicalArgValues::from_vec(vec![None; func.params.len()]);
            for value in func.live_values() {
                let crate::mir::Value::Arg(index) = func.value(value) else { continue };
                let canonical = &mut values[*index];
                debug_assert!(canonical.is_none_or(|existing| existing == value));
                *canonical = Some(value);
            }
            if values.iter().any(Option::is_some) {
                all_values.insert(func_id, values);
            }
        }
        all_values
    }

    /// Builds the subset-invariant analyses shared by one resident-layout
    /// search. The exhaustive subset loop evaluates up to `2^8` candidates;
    /// recomputing the CFG, its dominators, the phi plan, or operand counts
    /// per candidate made the search quadratic on large functions.
    pub(in crate::backend::evm::codegen) fn resident_search_context(
        &self,
        func: &Function,
        values: &[ValueId],
        phi_plan: Option<Rc<StackPhiPlan>>,
    ) -> ResidentSearchContext {
        let mut value_uses = FxHashMap::default();
        for block in &func.blocks {
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst| func.inst(inst).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                if values.contains(&operand) {
                    *value_uses.entry(operand).or_insert(0usize) += 1;
                }
            }
        }
        ResidentSearchContext { phi_plan, cfg: CfgInfo::new(func), value_uses }
    }

    pub(in crate::backend::evm::codegen) fn analyze_resident_subset(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        preserve_across_calls: bool,
        context: &ResidentSearchContext,
    ) -> Option<(GlobalStackPlan, ScheduleCost)> {
        let plan =
            GlobalStackPlan::analyze_resident_args(func, liveness, values, preserve_across_calls)?;
        if let Some(phi_plan) = &context.phi_plan {
            // One physical word cannot be both a phi input and an invariant resident prefix word.
            // `merge_resident` would otherwise extend only the result side of that edge, leaving a
            // non-square layout and a phantom word at the successor entry. Reject the complete or
            // candidate subset here and retain the frame home for those arguments.
            let resident_is_phi_source = phi_plan.edges.iter().any(|(&pred, edge)| {
                let term = func.blocks[pred]
                    .terminator
                    .as_ref()
                    .expect("stack-phi predecessor has no terminator");
                plan.edge_layout(func, term)
                    .is_some_and(|layout| layout.iter().any(|value| edge.sources.contains(value)))
            });
            if resident_is_phi_source {
                return None;
            }
            // A resident prefix on a planned backedge pays its shuffle on every loop iteration
            // and the composed emission is not yet correct for loop-carried prefixes: lifting
            // this gate miscompiled the nitro cold-prover paths (deep nested-loop deserialize
            // callees) and cost gas even where output stayed correct. Keep loop phis on the
            // established layout until the planner has execution-frequency-aware costing and
            // the loop composition is fixed; acyclic join edges compose without that
            // multiplier.
            let carries_planned_backedge =
                phi_plan.edges.keys().chain(phi_plan.branch_edges.keys()).any(|&pred| {
                    func.blocks[pred].terminator.as_ref().is_some_and(|term| {
                        term.successors().into_iter().any(|target| {
                            (matches!(term, Terminator::Jump(_)) || target != pred)
                                && plan.entry(target).is_some()
                                && context.cfg.dominators().dominates(target, pred)
                        })
                    })
                });
            if carries_planned_backedge {
                return None;
            }
        }

        let padding = plan
            .entries
            .iter()
            .map(|(block, entry)| {
                if GlobalStackPlan::is_terminal_block(func, *block)
                    || matches!(
                        func.blocks[*block].terminator,
                        Some(Terminator::TailCall { function, .. })
                            if self.cold_functions.contains(function)
                    )
                {
                    return 0;
                }
                entry.iter().filter(|&&value| !liveness.live_in(*block).contains(value)).count()
            })
            .sum::<usize>();
        // Without execution frequencies, the exact opcode cost below still cannot account for
        // padding paid repeatedly on hot edges. Require enough static uses to amortize every
        // padded word before comparing otherwise realizable layouts.
        let uses = values
            .iter()
            .map(|value| context.value_uses.get(value).copied().unwrap_or_default())
            .sum::<usize>();
        if uses < padding * 2 {
            return None;
        }
        let evm_version = self.gcx.sess.opts.evm_version;
        let padded_entry = ScheduleCost::stack_op(StackOp::Swap(1), evm_version)
            .plus(ScheduleCost::stack_op(StackOp::Pop, evm_version));
        let mut overhead = padded_entry.times(padding);
        for block in &func.blocks {
            match block.terminator.as_ref() {
                Some(term @ Terminator::Branch { then_block, else_block, .. }) => {
                    let Some((then_layout, else_layout)) = plan.branch_layouts(term) else {
                        continue;
                    };
                    let union = Self::global_branch_union(then_layout, else_layout);
                    let terminal_cleanup = (then_layout.is_empty()
                        && else_layout == union
                        && GlobalStackPlan::is_terminal_block(func, *then_block))
                        || (else_layout.is_empty()
                            && then_layout == union
                            && GlobalStackPlan::is_terminal_block(func, *else_block));
                    if then_layout != else_layout && !terminal_cleanup {
                        overhead = overhead.plus(ScheduleCost::control_flow_jump());
                    }
                }
                Some(term @ Terminator::Switch { .. }) => {
                    let Some(layouts) = plan.switch_layouts(term) else { continue };
                    let union = layouts.iter().fold(Vec::new(), |mut union, (_, layout)| {
                        for &value in *layout {
                            if !union.contains(&value) {
                                union.push(value);
                            }
                        }
                        union
                    });
                    let trampolines = layouts.iter().filter(|(_, layout)| *layout != union).count();
                    overhead = overhead.plus(
                        ScheduleCost::control_flow_jump()
                            .plus(ScheduleCost::jumpdest())
                            .times(trampolines),
                    );
                }
                _ => {}
            }
        }
        Some((plan, overhead))
    }

    /// Finds the least-cost realizable resident layout. This is deliberately exhaustive: the
    /// static ABI is capped at eight values, so all subsets are cheap to evaluate and a difficult
    /// argument need not force every independent word back into memory.
    pub(in crate::backend::evm::codegen) fn select_resident_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        preserve_across_calls: bool,
        phi_plan: Option<Rc<StackPhiPlan>>,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        debug_assert!(values.len() <= GLOBAL_STACK_LAYOUT_LIMIT);
        let mut use_counts = FxHashMap::default();
        let mut use_blocks = FxHashMap::default();
        for block in &func.blocks {
            let mut used_here = FxHashSet::default();
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst| func.inst(inst).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                if values.contains(&operand) {
                    *use_counts.entry(operand).or_insert(0usize) += 1;
                    used_here.insert(operand);
                }
            }
            for operand in used_here {
                *use_blocks.entry(operand).or_insert(0usize) += 1;
            }
        }

        let frame_store = ScheduleCost::memory_store(OperandCostModel::DIRECT);
        let frame_load = ScheduleCost::memory_load(OperandCostModel::DIRECT);
        let resident_access =
            ScheduleCost::stack_op(StackOp::Dup(1), self.gcx.sess.opts.evm_version);
        let memory_cost = |value: ValueId| {
            let uses = use_counts.get(&value).copied().unwrap_or_default();
            let blocks = use_blocks.get(&value).copied().unwrap_or_default();
            frame_store
                .plus(frame_load.times(blocks))
                .plus(resident_access.times(uses.saturating_sub(blocks)))
        };
        let baseline = values
            .iter()
            .fold(ScheduleCost::default(), |cost, &value| cost.plus(memory_cost(value)));
        let target = Target::new(self.gcx);
        let context = self.resident_search_context(func, values, phi_plan);
        let mut best = Option::<(ScheduleCost, Vec<ValueId>, GlobalStackPlan)>::None;
        for bits in 1usize..(1usize << values.len()) {
            let subset = values
                .iter()
                .enumerate()
                .filter_map(|(index, &value)| ((bits >> index) & 1 != 0).then_some(value))
                .collect::<Vec<_>>();
            let Some((plan, mut candidate)) = self.analyze_resident_subset(
                func,
                liveness,
                &subset,
                preserve_across_calls,
                &context,
            ) else {
                continue;
            };

            // A resident word is accessed with a DUP/SWAP-class stack operation instead of a
            // direct-address load. Charge every use rather than assuming the last one is free;
            // this is conservative when a return shuffle consumes the final copy. A padded entry
            // pays both the exposure swap and final pop that codegen may need on its dead arm.
            for &value in values {
                candidate = candidate.plus(if subset.contains(&value) {
                    resident_access.times(
                        use_counts.get(&value).copied().unwrap_or_default().saturating_sub(1),
                    )
                } else {
                    memory_cost(value)
                });
            }
            if !candidate.cmp_lifetime_for(baseline, target).is_lt() {
                continue;
            }
            if best.as_ref().is_none_or(|(best_cost, best_values, _)| {
                candidate.cmp_lifetime_for(*best_cost, target).is_lt()
                    || (candidate == *best_cost && subset.len() > best_values.len())
            }) {
                best = Some((candidate, subset, plan));
            }
        }
        best.map(|(_, values, plan)| (values, plan))
    }

    /// Retains profitable computed words across acyclic joins while reserving ordinary spill slots
    /// as an edge-time fallback. The current global argument planner owns external entry layouts
    /// when it applies, so this incremental planner runs only when that plan is empty and limits
    /// its search to eight cross-block definitions.
    pub(in crate::backend::evm::codegen) fn compute_cross_block_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        cross_block_live: &OnceCell<DenseBitSet<ValueId>>,
        phi_plan: Option<Rc<StackPhiPlan>>,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None)
            || !Self::is_external_entry(func)
            || func.blocks.len() < 3
        {
            return None;
        }

        let inst_blocks = func.inst_blocks();
        let cross_block =
            cross_block_live.get_or_init(|| Self::cross_block_live_values(func, liveness));
        let mut uses =
            FxHashMap::<ValueId, (BlockId, usize, FxHashSet<BlockId>, bool, bool, bool)>::default();
        for value in cross_block {
            let crate::mir::Value::Inst(inst_id) = func.value(value) else { continue };
            if matches!(func.inst(*inst_id).kind, InstKind::Phi(_))
                || Self::is_cross_block_recomputable_inst(func, value)
            {
                continue;
            }
            let Some(&definition) = inst_blocks.get(inst_id) else { continue };
            uses.insert(value, (definition, 0, FxHashSet::default(), false, false, false));
        }
        for (block_id, block) in func.blocks.iter_enumerated() {
            for &user in &block.instructions {
                let phi = matches!(func.inst(user).kind, InstKind::Phi(_));
                for operand in func.inst(user).kind.operands() {
                    if let Some((definition, count, blocks, used_in_definition, used_by_phi, _)) =
                        uses.get_mut(&operand)
                    {
                        *used_in_definition |= block_id == *definition;
                        *used_by_phi |= phi;
                        *count += 1;
                        blocks.insert(block_id);
                    }
                }
            }
            for operand in block.terminator.iter().flat_map(Terminator::operands) {
                if let Some((definition, count, blocks, used_in_definition, _, _)) =
                    uses.get_mut(&operand)
                {
                    *used_in_definition |= block_id == *definition;
                    *count += 1;
                    blocks.insert(block_id);
                }
            }
            if block.predecessors.len() > 1 {
                for value in liveness.live_in(block_id) {
                    if let Some((_, _, _, _, _, crosses_join)) = uses.get_mut(&value) {
                        *crosses_join = true;
                    }
                }
            }
        }

        let mut ranked = Vec::new();
        for (value, (_, use_count, use_blocks, used_in_definition, used_by_phi, crosses_join)) in
            uses
        {
            // Definition-block and phi-edge uses need position-sensitive accounting. Leave those
            // to the existing spill/phi planners until this plan models individual program points.
            if used_in_definition || used_by_phi || !crosses_join || use_count < 2 {
                continue;
            }
            ranked.push((value, use_blocks.len(), use_count));
        }
        ranked.sort_unstable_by_key(|&(value, blocks, uses)| {
            (std::cmp::Reverse(blocks), std::cmp::Reverse(uses), value.index())
        });
        let values = ranked
            .into_iter()
            .take(GLOBAL_STACK_LAYOUT_LIMIT)
            .map(|(value, _, _)| value)
            .collect::<Vec<_>>();
        self.select_cross_block_stack_layout(func, liveness, &values, phi_plan)
    }

    /// Keeps values that survive a low-memory calldata copy in a canonical
    /// stack layout until their uses are complete. The copied range is
    /// dynamic, so no fixed spill address is safe: a decompressor can grow its
    /// output through every compiler-owned low-memory slot before forwarding
    /// that output to a call.
    pub(in crate::backend::evm::codegen) fn compute_spill_hazard_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        stack_phi_plan: &StackPhiPlan,
        values: &[ValueId],
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if self.spill_hazard_insts.is_empty() {
            return None;
        }

        if values.is_empty() {
            return None;
        }

        let mut plan = GlobalStackPlan::analyze_resident_args(
            func,
            liveness,
            values,
            self.preserve_caller_stack,
        )?;
        // Phi operands are edge uses, not unchanged target live-ins. Full
        // liveness conservatively includes them at the header; remove those
        // incoming identities from the resident prefix so the phi edge can
        // replace each source with its result instead of trying to carry both.
        for (&pred, edge) in &stack_phi_plan.edges {
            let Some(Terminator::Jump(target)) = func.blocks[pred].terminator.as_ref() else {
                continue;
            };
            if let Some(entry) = plan.entries.get_mut(target) {
                entry.retain(|value| !edge.sources.contains(value));
            }
        }
        plan.entries.retain(|_, entry| !entry.is_empty());
        Some((values.to_vec(), plan))
    }

    /// Values that need a successor after a forwarding-buffer clobber.
    pub(in crate::backend::evm::codegen) fn spill_hazard_cross_block_values(
        &self,
        func: &Function,
        liveness: &Liveness,
        cross_block_live: &OnceCell<DenseBitSet<ValueId>>,
        recomputable: &DenseBitSet<ValueId>,
    ) -> Vec<ValueId> {
        if self.spill_hazard_is_repeated_low_phi(func) {
            return cross_block_live
                .get_or_init(|| Self::cross_block_live_values(func, liveness))
                .iter()
                .filter(|&value| {
                    Self::can_own_spill_slot(func, value) && !recomputable.contains(value)
                })
                .collect();
        }

        let inst_blocks = func.inst_blocks();
        let mut values = DenseBitSet::new_empty(func.num_values());
        for inst in &self.spill_hazard_insts {
            let Some(&block) = inst_blocks.get(inst) else { continue };
            for value in liveness.live_out(block) {
                if Self::can_own_spill_slot(func, value) && !recomputable.contains(value) {
                    values.insert(value);
                }
            }
        }
        values.iter().collect()
    }

    /// Whether a low forwarding destination is a loop-carried pointer. Every
    /// cross-block value participates in the loop's canonical stack shape;
    /// selecting only the values live out of the copy can omit phi companions
    /// needed to preserve that shape on the backedge.
    fn spill_hazard_is_repeated_low_phi(&self, func: &Function) -> bool {
        let inst_blocks = func.inst_blocks();
        let mut loop_analyzer = LoopAnalyzer::new();
        let loop_info = loop_analyzer.analyze(func);
        loop_info.all_loops().any(|loop_info| {
            self.spill_hazard_insts.iter().any(|inst| {
                inst_blocks.get(inst).is_some_and(|block| loop_info.blocks.contains(*block))
                    && Self::dynamic_spill_write_dest(func, *inst).is_some_and(|dest| {
                        matches!(func.value(dest), Value::Inst(definition)
                        if matches!(&func.inst(*definition).kind, InstKind::Phi(incoming)
                            if incoming.iter().any(|&(_, value)| {
                                func.value_u64(value).is_some_and(|address| {
                                    address < EvmMemoryLayout::HEAP_START
                                })
                            })))
                    })
            })
        })
    }

    /// Whether an existing resident plan carries every at-risk live-out.
    pub(in crate::backend::evm::codegen) fn stack_plan_carries_spill_hazards(
        &self,
        func: &Function,
        liveness: &Liveness,
        plan: &GlobalStackPlan,
        hazard_values: &[ValueId],
    ) -> bool {
        let inst_blocks = func.inst_blocks();
        self.spill_hazard_insts.iter().all(|inst| {
            let Some(&block_id) = inst_blocks.get(inst) else { return false };
            let Some(term) = func.blocks[block_id].terminator.as_ref() else { return false };
            let carried = plan.uniformly_carried_values(func, term);
            hazard_values
                .iter()
                .filter(|&&value| liveness.live_out(block_id).contains(value))
                .all(|value| carried.contains(value))
        })
    }

    fn select_cross_block_stack_layout(
        &self,
        func: &Function,
        liveness: &Liveness,
        values: &[ValueId],
        phi_plan: Option<Rc<StackPhiPlan>>,
    ) -> Option<(Vec<ValueId>, GlobalStackPlan)> {
        if values.is_empty() {
            return None;
        }
        debug_assert!(values.len() <= GLOBAL_STACK_LAYOUT_LIMIT);

        let mut use_counts = FxHashMap::default();
        let mut use_blocks = FxHashMap::<ValueId, FxHashSet<BlockId>>::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for operand in block
                .instructions
                .iter()
                .flat_map(|&inst| func.inst(inst).kind.operands())
                .chain(block.terminator.iter().flat_map(Terminator::operands))
            {
                if values.contains(&operand) {
                    *use_counts.entry(operand).or_insert(0usize) += 1;
                    use_blocks.entry(operand).or_default().insert(block_id);
                }
            }
        }

        let spill_store = ScheduleCost::memory_store(OperandCostModel::DIRECT);
        let spill_load = ScheduleCost::memory_load(OperandCostModel::DIRECT);
        let resident_access =
            ScheduleCost::stack_op(StackOp::Dup(1), self.gcx.sess.opts.evm_version);
        let memory_cost = |value: ValueId| {
            let uses = use_counts.get(&value).copied().unwrap_or_default();
            let blocks = use_blocks.get(&value).map_or(0, FxHashSet::len);
            spill_store
                .plus(spill_load.times(blocks))
                .plus(resident_access.times(uses.saturating_sub(blocks)))
        };
        let baseline = values
            .iter()
            .fold(ScheduleCost::default(), |cost, &value| cost.plus(memory_cost(value)));
        let target = Target::new(self.gcx);
        let context = self.resident_search_context(func, values, phi_plan);
        let mut best = Option::<(ScheduleCost, Vec<ValueId>, GlobalStackPlan)>::None;
        for bits in 1usize..(1usize << values.len()) {
            let subset = values
                .iter()
                .enumerate()
                .filter_map(|(index, &value)| ((bits >> index) & 1 != 0).then_some(value))
                .collect::<Vec<_>>();
            let Some((plan, mut candidate)) = self.analyze_resident_subset(
                func,
                liveness,
                &subset,
                self.preserve_caller_stack,
                &context,
            ) else {
                continue;
            };
            if plan.entries.iter().any(|(&block, _)| {
                func.blocks[block]
                    .predecessors
                    .iter()
                    .any(|&pred| context.cfg.dominators().dominates(block, pred))
            }) {
                continue;
            }

            for &value in values {
                candidate = candidate.plus(if subset.contains(&value) {
                    let uses = use_counts.get(&value).copied().unwrap_or_default();
                    // A carried SSA copy can be consumed on its final use; all earlier uses retain
                    // it with a stack operation. Its spill store is emitted only if an edge falls
                    // back to memory, so it does not belong to the selected layout's hot cost.
                    resident_access.times(uses.saturating_sub(1))
                } else {
                    memory_cost(value)
                });
            }
            if !candidate.cmp_lifetime_for(baseline, target).is_lt() {
                continue;
            }
            if best.as_ref().is_none_or(|(best_cost, best_values, _)| {
                candidate.cmp_lifetime_for(*best_cost, target).is_lt()
                    || (candidate == *best_cost && subset.len() > best_values.len())
            }) {
                best = Some((candidate, subset, plan));
            }
        }
        best.map(|(_, values, plan)| (values, plan))
    }
}

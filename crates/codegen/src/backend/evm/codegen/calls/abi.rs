//! Selection of stack argument and return conventions for internal calls.

use super::super::{
    ArgIdx, CallGraphInfo, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function, FunctionId,
    FxHashMap, GlobalStackPlan, IndexVec, InstKind, LazyStackArgPlan, Liveness, MAX_STACK_ACCESS,
    Module, OptimizationMode, StackReturnPlan, StaticCallAbi, StaticCallEntry, Terminator, ValueId,
    index_vec,
};

impl<'gcx> EvmCodegen<'gcx> {
    pub(in crate::backend::evm::codegen) fn uses_recursive_stack_abi(
        &self,
        id: FunctionId,
    ) -> bool {
        self.static_call_abis.get(&id).is_some_and(|abi| abi.recursive_stack)
    }

    /// Selects frame-free recursive components with bounded word signatures and stack-planned phis.
    /// Suspended activations retain their live words below the callee's return address.
    /// Acyclic callees use their ordinary stack convention: they cannot reenter this component.
    pub(in crate::backend::evm::codegen) fn compute_recursive_stack_abis(
        &mut self,
        module: &Module,
        calls: &CallGraphInfo,
    ) {
        let cold_functions = Self::collect_cold_functions(module);
        let mut visited = DenseBitSet::new_empty(module.functions.len());
        for id in module.functions.indices() {
            if !calls.is_recursive(id) || visited.contains(id) {
                continue;
            }
            let component = calls.recursive_component(id);
            visited.union(&component);
            if !component.iter().any(|id| {
                self.stack_only_memory_functions.contains(id)
                    || (module.functions[id].attributes.is_yul
                        && module.functions[id].returns.len() > 1)
            }) {
                continue;
            }
            let members = component;
            let mut plans = Vec::new();
            for id in members.iter() {
                let func = &module.functions[id];
                if self.disabled_stack_only_functions.contains(id)
                    || func.internal_frame_size != 0
                    || func.params.len() > MAX_STACK_ACCESS
                    || func.returns.len() > MAX_STACK_ACCESS
                    || func
                        .instructions()
                        .any(|inst| matches!(func.inst(inst).kind, InstKind::InternalFrameAddr(_)))
                    || func.blocks.iter().any(|block| {
                        matches!(
                            block.terminator.as_ref(),
                            Some(Terminator::TailCall { function, args })
                                if !args.is_empty() || !cold_functions.contains(*function)
                        ) || (matches!(block.terminator, Some(Terminator::Stop))
                            && !func.returns.is_empty())
                    })
                {
                    break;
                }
                let uses = func.arg_uses();
                if uses.iter().any(|values| {
                    values.first().is_some_and(|first| values.iter().any(|value| value != first))
                }) {
                    break;
                }
                let values = uses
                    .iter()
                    .rev()
                    .filter_map(|values| values.first().copied())
                    .collect::<Vec<_>>();
                let liveness = Liveness::compute(func);
                let layout = if values.is_empty() {
                    GlobalStackPlan::default()
                } else if let Some(layout) =
                    GlobalStackPlan::analyze_resident_args(func, &liveness, &values, true)
                {
                    layout
                } else {
                    break;
                };
                if func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Phi(_)))
                {
                    let phi_plan = self.stack_phi_plan(id, func, &liveness);
                    let mut composed = phi_plan.as_ref().clone();
                    if func.blocks.iter_enumerated().any(|(block, data)| {
                        data.instructions.iter().any(|&inst| {
                            matches!(func.inst(inst).kind, InstKind::Phi(_))
                                && func.inst_result_value(inst).is_none_or(|value| {
                                    !phi_plan
                                        .entries
                                        .get(&block)
                                        .is_some_and(|entry| entry.contains(&value))
                                })
                        })
                    }) || !composed.merge_resident(func, &layout)
                    {
                        break;
                    }
                }
                let mut abi = StaticCallAbi::new(func.params.len());
                abi.recursive_stack = true;
                for index in 0..func.params.len() {
                    if uses[ArgIdx::new(index)].is_empty() {
                        abi.ignored_args.insert(index);
                    } else {
                        abi.stack_args.insert(index);
                    }
                }
                abi.entry = StaticCallEntry::Resident { values, layout };
                abi.returns = Some(StackReturnPlan {
                    arity: func.returns.len(),
                    local_base: EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + ((func.params.len() + func.returns.len()) as u64)
                            * EvmMemoryLayout::WORD_SIZE,
                    preserve_source_scratch: true,
                });
                plans.push((id, abi));
            }
            if plans.len() == members.count() {
                self.static_call_abis.extend(plans);
            }
        }
    }

    pub(in crate::backend::evm::codegen) fn report_private_memory_required(
        &self,
        func: &Function,
        context: &str,
    ) {
        self.gcx
            .dcx()
            .err(format!(
                "codegen cannot preserve {context} without compiler memory in `{}`",
                func.name,
            ))
            .emit();
    }

    pub(in crate::backend::evm::codegen) fn static_call_abi_mut(
        &mut self,
        func_id: FunctionId,
        arg_count: usize,
    ) -> &mut StaticCallAbi {
        self.static_call_abis.entry(func_id).or_insert_with(|| StaticCallAbi::new(arg_count))
    }

    /// Rejects argument and return conventions that violate source memory ownership.
    pub(in crate::backend::evm::codegen) fn validate_memory_call_abis(
        &self,
        module: &Module,
        internal_targets: &DenseBitSet<FunctionId>,
    ) {
        for func_id in internal_targets.iter() {
            let func = &module.functions[func_id];
            // Yul tuples must not publish a return-buffer pointer in source scratch memory.
            if func.attributes.is_yul
                && func.returns.len() > 1
                && self
                    .stack_return_plan(func_id)
                    .is_none_or(|plan| plan.arity != func.returns.len())
            {
                self.report_private_memory_required(func, "Yul tuple returns");
            }
            if !self.stack_only_memory_functions.contains(func_id) {
                continue;
            }
            let func = &module.functions[func_id];
            if Self::is_external_entry(func) {
                continue;
            }
            let Some(abi) = self.static_call_abis.get(&func_id) else {
                self.report_private_memory_required(func, "internal arguments");
                continue;
            };
            let complete_args = abi.stack_args.count() + abi.ignored_args.count()
                == func.params.len()
                && abi.stack_args.iter().all(|index| !abi.ignored_args.contains(index));
            let memory_free_entry = abi.stack_args.is_empty()
                || match &abi.entry {
                    StaticCallEntry::Stored => false,
                    StaticCallEntry::Direct(values) | StaticCallEntry::Resident { values, .. } => {
                        values.len() == abi.stack_args.count()
                    }
                    StaticCallEntry::Lazy(plan) => {
                        plan.frame_values.is_empty() && plan.args.len() == abi.stack_args.count()
                    }
                };
            if (!self.static_frame_functions.contains(func_id)
                && !self.uses_recursive_stack_abi(func_id))
                || self.disabled_stack_only_functions.contains(func_id)
                || !complete_args
                || !memory_free_entry
            {
                self.report_private_memory_required(func, "internal arguments");
            }
            if func.blocks.iter().any(|block| {
                matches!(&block.terminator, Some(Terminator::Return { values }) if !values.is_empty())
            }) && abi.returns.is_none_or(|plan| plan.arity != func.returns.len())
            {
                self.report_private_memory_required(func, "internal returns");
            }
        }
    }

    pub(in crate::backend::evm::codegen) fn stack_arg_mask(
        &self,
        func_id: FunctionId,
    ) -> Option<&DenseBitSet<usize>> {
        self.static_call_abis
            .get(&func_id)
            .map(|abi| &abi.stack_args)
            .filter(|mask| !mask.is_empty())
    }

    pub(in crate::backend::evm::codegen) fn direct_stack_args(
        &self,
        func_id: FunctionId,
    ) -> Option<&[ValueId]> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Direct(values) => Some(values),
            _ => None,
        }
    }

    pub(in crate::backend::evm::codegen) fn resident_stack_args(
        &self,
        func_id: FunctionId,
    ) -> Option<&[ValueId]> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Resident { values, .. } => Some(values),
            _ => None,
        }
    }

    pub(in crate::backend::evm::codegen) fn resident_stack_plan(
        &self,
        func_id: FunctionId,
    ) -> Option<&GlobalStackPlan> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Resident { layout, .. } => Some(layout),
            _ => None,
        }
    }

    pub(in crate::backend::evm::codegen) fn lazy_stack_args(
        &self,
        func_id: FunctionId,
    ) -> Option<&LazyStackArgPlan> {
        match &self.static_call_abis.get(&func_id)?.entry {
            StaticCallEntry::Lazy(plan) => Some(plan),
            _ => None,
        }
    }

    /// Returns arguments without a valid frame home at this point in the callee.
    pub(in crate::backend::evm::codegen) fn stack_only_values(
        &self,
        func_id: FunctionId,
        entry: bool,
    ) -> Vec<ValueId> {
        self.resident_stack_args(func_id)
            .into_iter()
            .flatten()
            .copied()
            .chain(
                entry
                    .then(|| self.direct_stack_args(func_id))
                    .flatten()
                    .into_iter()
                    .flatten()
                    .copied(),
            )
            .chain(
                entry
                    .then(|| self.lazy_stack_args(func_id))
                    .flatten()
                    .into_iter()
                    .flat_map(LazyStackArgPlan::values),
            )
            .collect()
    }

    pub(in crate::backend::evm::codegen) fn stack_return_plan(
        &self,
        func_id: FunctionId,
    ) -> Option<StackReturnPlan> {
        self.static_call_abis.get(&func_id)?.returns
    }

    /// Selects static-frame helpers that can return their complete tuple on the EVM stack.
    ///
    /// Tail-call edges keep the memory convention because an external dispatch path does not
    /// necessarily carry an internal return address. Calls whose MIR return arity disagrees with
    /// the callee are also excluded defensively. Every optimized mode uses the convention: the
    /// bounded tuple shuffle replaces callee stores and caller loads, and any function that cannot
    /// realize its plan is regenerated with its ordinary frame-backed return area.
    pub(in crate::backend::evm::codegen) fn compute_stack_return_plans(&mut self, module: &Module) {
        let cold_functions = Self::collect_cold_functions(module);
        for abi in self.static_call_abis.values_mut() {
            abi.returns = None;
        }
        for (func_id, func) in module.functions.iter_enumerated() {
            let arity = func.returns.len();
            let source_scratch_live = func.attributes.is_yul && arity > 1;
            if !self.stack_returns_enabled && !source_scratch_live {
                continue;
            }
            // Non-returning helpers can use this convention too: their callers must not
            // manufacture a frame load after a call that always aborts.
            let has_consistent_returns = func.blocks.iter().all(|block| match &block.terminator {
                Some(Terminator::Return { values }) => values.len() == arity,
                // The backend treats `stop` in an internal function as a void return, which is
                // incompatible with a non-empty stack-return convention.
                Some(Terminator::Stop) => false,
                _ => true,
            });
            if self.static_frame_functions.contains(func_id)
                && (!matches!(self.gcx.sess.opts.optimization, OptimizationMode::None)
                    || (self.in_constructor && self.preserve_caller_stack)
                    || self.stack_only_memory_functions.contains(func_id)
                    || source_scratch_live)
                && !self.disabled_stack_only_functions.contains(func_id)
                && !self.recursive_frame_functions.contains(func_id)
                && (1..=MAX_STACK_ACCESS).contains(&arity)
                && has_consistent_returns
            {
                let local_base = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    + ((func.params.len() + arity) as u64) * EvmMemoryLayout::WORD_SIZE;
                self.static_call_abi_mut(func_id, func.params.len()).returns =
                    Some(StackReturnPlan {
                        arity,
                        local_base,
                        preserve_source_scratch: source_scratch_live,
                    });
            }
        }

        for (caller, func) in module.functions.iter_enumerated() {
            for inst_id in func.instructions() {
                if let InstKind::ICall { function, returns, .. } = &func.inst(inst_id).kind
                    && self
                        .stack_return_plan(*function)
                        .is_some_and(|plan| *returns as usize != plan.arity)
                    && let Some(abi) = self.static_call_abis.get_mut(function)
                {
                    abi.returns = None;
                }
            }
            for block in &func.blocks {
                if let Some(Terminator::TailCall { function, .. }) = &block.terminator {
                    if !cold_functions.contains(*function)
                        && let Some(abi) = self.static_call_abis.get_mut(&caller)
                    {
                        abi.returns = None;
                    }
                    if let Some(abi) = self.static_call_abis.get_mut(function) {
                        abi.returns = None;
                    }
                }
            }
        }
    }

    /// Computes which arguments of each static-frame callee pass on the
    /// stack. A site can deliver a stack argument through raw re-emission after
    /// the drain for immediates, position-independently reloadable caller
    /// arguments, and always-rematerializable reads, or through a
    /// freshness-validated spill reload for other computed values.
    /// Only callers emitted in the current deployment or runtime artifact constrain the ABI.
    /// The per-argument choice is scored across those sites — raw and
    /// already-stored (cross-block) values save the four-byte frame store,
    /// while a fresh block-local value must first pay its own spill — and an
    /// argument passes on the stack when the sites' savings outweigh the
    /// callee's one-time prologue store. Tail calls use the same entry tuple without pushing a new
    /// return label; an internal caller reuses its inherited label, while fused external bodies do
    /// not return through one.
    pub(in crate::backend::evm::codegen) fn compute_stack_arg_masks(
        &mut self,
        module: &Module,
        callers: &DenseBitSet<FunctionId>,
    ) {
        self.static_call_abis.clear();
        if self.static_frame_functions.is_empty() {
            return;
        }

        let mut scores = FxHashMap::<FunctionId, IndexVec<ArgIdx, Option<i64>>>::default();
        let mut excluded = DenseBitSet::new_empty(module.functions.len());
        for caller_id in callers.iter() {
            let func = &module.functions[caller_id];
            let mut has_candidate_call = false;
            for block in func.blocks.iter() {
                has_candidate_call |= block.instructions.iter().any(|&inst_id| {
                    matches!(
                        &func.inst(inst_id).kind,
                        InstKind::ICall { function, .. }
                            if self.static_frame_functions.contains(*function)
                    )
                });
                has_candidate_call |= matches!(
                    &block.terminator,
                    Some(Terminator::TailCall { function, .. })
                        if self.static_frame_functions.contains(*function)
                );
            }
            if !has_candidate_call {
                continue;
            }

            let caller_is_entry = Self::is_external_entry(func);
            let caller_static = self.static_frame_functions.contains(caller_id);
            let raw_leaves_ok = caller_is_entry
                || caller_static
                || (self.in_constructor
                    && func.attributes.is_constructor
                    && self.asm.source_memory_required());
            // Where each instruction result is defined, to spot cross-block
            // arguments (already stored at their definition).
            let mut inst_block = index_vec![None; func.num_insts()];
            let mut use_counts: FxHashMap<ValueId, usize> = FxHashMap::default();
            for (block_idx, block) in func.blocks.iter().enumerate() {
                for &inst_id in &block.instructions {
                    inst_block[inst_id] = Some(block_idx);
                    for operand in func.inst(inst_id).kind.operands() {
                        *use_counts.entry(operand).or_default() += 1;
                    }
                }
                if let Some(term) = &block.terminator {
                    for operand in term.operands() {
                        *use_counts.entry(operand).or_default() += 1;
                    }
                }
            }
            for (block_idx, block) in func.blocks.iter().enumerate() {
                for &inst_id in &block.instructions {
                    let InstKind::ICall { function, args, .. } = &func.inst(inst_id).kind else {
                        continue;
                    };
                    if !self.static_frame_functions.contains(*function) {
                        continue;
                    }
                    let score = scores
                        .entry(*function)
                        .or_insert_with(|| IndexVec::from_vec(vec![Some(0); args.len()]));
                    if score.len() != args.len() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (i, &arg) in args.iter().enumerate() {
                        let index = ArgIdx::new(i);
                        let Some(current) = score[index] else { continue };
                        let benefit = if Self::raw_arg_emittable(func, raw_leaves_ok, arg) {
                            // The frame store disappears outright.
                            Some(4)
                        } else if !Self::stack_arg_site_eligible(func, raw_leaves_ok, arg) {
                            // This site can neither emit the argument raw nor
                            // reload it through the computed-value spill path,
                            // so the argument must stay frame-passed everywhere.
                            None
                        } else {
                            Some(match func.value(arg) {
                                crate::mir::Value::Inst(def)
                                    if inst_block[*def] != Some(block_idx) =>
                                {
                                    // Cross-block values are stored at their
                                    // definition; the site keeps only the
                                    // slot reload it would have paid anyway.
                                    4
                                }
                                crate::mir::Value::Inst(_)
                                    if use_counts.get(&arg).copied().unwrap_or(0) > 1 =>
                                {
                                    // Multi-use block-local values usually
                                    // have a stack copy; the extra spill is
                                    // partially amortized.
                                    1
                                }
                                // A fresh single-use value pays a spill it
                                // did not need before.
                                _ => -5,
                            })
                        };
                        score[index] = benefit.map(|benefit| current.saturating_add(benefit));
                    }
                }
                if let Some(Terminator::TailCall { function, args }) = &block.terminator {
                    if !self.static_frame_functions.contains(*function) {
                        continue;
                    }
                    let score = scores
                        .entry(*function)
                        .or_insert_with(|| IndexVec::from_vec(vec![Some(0); args.len()]));
                    if score.len() != args.len() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (i, &arg) in args.iter().enumerate() {
                        let index = ArgIdx::new(i);
                        let Some(current) = score[index] else { continue };
                        let benefit = if Self::raw_arg_emittable(func, raw_leaves_ok, arg) {
                            Some(4)
                        } else if !Self::stack_arg_site_eligible(func, raw_leaves_ok, arg) {
                            None
                        } else {
                            Some(match func.value(arg) {
                                crate::mir::Value::Inst(def)
                                    if inst_block[*def] != Some(block_idx) =>
                                {
                                    4
                                }
                                crate::mir::Value::Inst(_)
                                    if use_counts.get(&arg).copied().unwrap_or(0) > 1 =>
                                {
                                    1
                                }
                                _ => -5,
                            })
                        };
                        score[index] = benefit.map(|benefit| current.saturating_add(benefit));
                    }
                }
            }
        }
        scores.retain(|func_id, _| {
            self.static_frame_functions.contains(*func_id)
                && !self.recursive_frame_functions.contains(*func_id)
                && !excluded.contains(*func_id)
                && !self.disabled_stack_only_functions.contains(*func_id)
        });
        let mut masks = FxHashMap::default();
        for (func_id, score) in scores {
            // The callee prologue pays one store per stack argument.
            let mut mask = DenseBitSet::new_empty(score.len());
            for (index, benefit) in score.iter_enumerated() {
                if benefit.is_some_and(|benefit| {
                    benefit > 4 || self.stack_only_memory_functions.contains(func_id)
                }) {
                    mask.insert(index.index());
                }
            }
            // The tail-call emitter shuffles the selected tuple into an exact
            // entry layout, so a mask beyond DUP16/SWAP16 reach could never be
            // constructed.
            if !mask.is_empty() && mask.count() <= MAX_STACK_ACCESS {
                masks.insert(func_id, mask);
            }
        }
        for (func_id, stack_args) in masks {
            self.static_call_abis.insert(
                func_id,
                StaticCallAbi {
                    recursive_stack: false,
                    ignored_args: DenseBitSet::new_empty(stack_args.domain_size()),
                    stack_args,
                    entry: StaticCallEntry::Stored,
                    returns: None,
                },
            );
        }
    }
}

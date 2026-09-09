//! Memory provenance and writes that can clobber compiler spill slots.

use super::{
    super::{
        AliasAnalysis, ArgIdx, BlockId, CallGraphInfo, DenseBitSet, EffectKind, EvmCodegen,
        EvmMemoryLayout, Function, FunctionId, FxHashMap, FxHashSet, IndexVec, InstId, InstKind,
        MemoryBase, MemoryRegion, MirType, Module, Terminator, U256, Value, ValueId,
    },
    SPILL_HAZARD_BOUND,
};

struct LowFmpState {
    entry: DenseBitSet<FunctionId>,
    returns: DenseBitSet<FunctionId>,
    pointer_returns: DenseBitSet<FunctionId>,
    args: FxHashMap<FunctionId, DenseBitSet<ArgIdx>>,
}

impl<'gcx> EvmCodegen<'gcx> {
    /// Finds functions whose temporary memory expansion could reach an `msize` observation.
    /// Internal callers, callees, and sibling calls share memory. Separate dispatcher arms do
    /// not, so the synthetic dispatcher is excluded from the roots. The order of calls and
    /// observations is deliberately conservative.
    pub(in crate::backend::evm::codegen) fn collect_msize_observed_functions(
        module: &Module,
        call_graph: &CallGraphInfo,
    ) -> DenseBitSet<FunctionId> {
        let mut observers = DenseBitSet::new_empty(module.functions.len());
        for (id, func) in module.functions.iter_enumerated() {
            if func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::MSize)) {
                observers.insert(id);
            }
        }
        let mut observed = observers.clone();
        if observers.is_empty() {
            return observed;
        }
        for id in module.functions.indices() {
            if Some(id) == module.dispatch_entry() {
                continue;
            }
            let mut reachable = call_graph.reachable_callees_from([id]);
            reachable.insert(id);
            if observers.iter().any(|observer| reachable.contains(observer)) {
                observed.union(&reachable);
            }
        }
        observed
    }

    /// Tracks explicit heap resets to `HEAP_START` or below through blocks and internal calls.
    /// A saved pointer loaded before a reset can restore the heap, but pointers derived from
    /// a low load remain low after restoration. Unknown writes retain the current state;
    /// this analysis does not discover arbitrary computed resets.
    pub(in crate::backend::evm::codegen) fn collect_low_fmp_functions(
        &self,
        module: &Module,
        call_graph: &CallGraphInfo,
    ) -> DenseBitSet<FunctionId> {
        let explicit_reset = module.functions.iter().any(|func| {
            func.instructions().any(|inst| match func.inst(inst).kind {
                InstKind::SetFmp(value) => {
                    func.value_u64(value).is_some_and(|value| value <= EvmMemoryLayout::HEAP_START)
                }
                InstKind::MStore(address, value) => {
                    func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                        && func
                            .value_u64(value)
                            .is_some_and(|value| value <= EvmMemoryLayout::HEAP_START)
                }
                _ => false,
            })
        });
        if !explicit_reset {
            return DenseBitSet::new_empty(module.functions.len());
        }
        let mut state = LowFmpState {
            entry: DenseBitSet::new_empty(module.functions.len()),
            returns: DenseBitSet::new_empty(module.functions.len()),
            pointer_returns: DenseBitSet::new_empty(module.functions.len()),
            args: FxHashMap::default(),
        };
        let mut heap_clobbers = state.entry.clone();
        let mut block_low = module
            .functions
            .iter()
            .map(|func| DenseBitSet::new_empty(func.blocks.len()))
            .collect::<IndexVec<FunctionId, _>>();
        let mut reads_low = module
            .functions
            .iter()
            .map(|func| DenseBitSet::new_empty(func.num_insts()))
            .collect::<IndexVec<FunctionId, _>>();
        let aliases =
            module.functions.iter().map(AliasAnalysis::new).collect::<IndexVec<FunctionId, _>>();
        loop {
            let mut changed = false;
            for (id, func) in module.functions.iter_enumerated() {
                if state.entry.contains(id) {
                    changed |= block_low[id].insert(BlockId::ENTRY);
                }
                for (block_id, block) in func.blocks.iter_enumerated() {
                    let mut low = block_low[id].contains(block_id);
                    for &inst in &block.instructions {
                        let kind = &func.inst(inst).kind;
                        let heap_write = (func.inst(inst).metadata.memory_region()
                            == Some(MemoryRegion::Heap)
                            && kind.effect_kind() == EffectKind::MemoryWrite)
                            || match *kind {
                                InstKind::MStore(dest, _)
                                | InstKind::MStore8(dest, _)
                                | InstKind::CalldataCopy(dest, _, _)
                                | InstKind::CodeCopy(dest, _, _)
                                | InstKind::ReturnDataCopy(dest, _, _)
                                | InstKind::MCopy(dest, _, _) => {
                                    !self.write_dest_may_reach_spills(func, &aliases[id], dest)
                                }
                                _ => false,
                            };
                        let low_destination = match *kind {
                            InstKind::MStore(dest, _)
                            | InstKind::MStore8(dest, _)
                            | InstKind::CalldataCopy(dest, _, _)
                            | InstKind::CodeCopy(dest, _, _)
                            | InstKind::ReturnDataCopy(dest, _, _)
                            | InstKind::MCopy(dest, _, _)
                                if heap_write && !low =>
                            {
                                self.fmp_value_may_be_low(
                                    func,
                                    &aliases[id],
                                    dest,
                                    &reads_low[id],
                                    id,
                                    &state,
                                ) == Some(true)
                            }
                            _ => false,
                        };
                        if heap_write && (low || low_destination) {
                            changed |= heap_clobbers.insert(id);
                        }
                        match kind {
                            InstKind::Fmp => {
                                if low {
                                    changed |= reads_low[id].insert(inst);
                                }
                            }
                            InstKind::MLoad(address)
                                if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                            {
                                if low {
                                    changed |= reads_low[id].insert(inst);
                                }
                            }
                            InstKind::SetFmp(value) => {
                                low = self
                                    .fmp_value_may_be_low(
                                        func,
                                        &aliases[id],
                                        *value,
                                        &reads_low[id],
                                        id,
                                        &state,
                                    )
                                    .unwrap_or(low);
                            }
                            InstKind::MStore(address, value)
                                if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                            {
                                low = self
                                    .fmp_value_may_be_low(
                                        func,
                                        &aliases[id],
                                        *value,
                                        &reads_low[id],
                                        id,
                                        &state,
                                    )
                                    .unwrap_or(low);
                            }
                            InstKind::ICall { function, args, .. } => {
                                for (index, &arg) in args.iter().enumerate() {
                                    if func.value_u64(arg).is_none()
                                        && self.fmp_value_may_be_low(
                                            func,
                                            &aliases[id],
                                            arg,
                                            &reads_low[id],
                                            id,
                                            &state,
                                        ) == Some(true)
                                    {
                                        changed |= state
                                            .args
                                            .entry(*function)
                                            .or_insert_with(|| {
                                                DenseBitSet::new_empty(
                                                    module.functions[*function].params.len(),
                                                )
                                            })
                                            .insert(ArgIdx::from_usize(index));
                                    }
                                }
                                if low {
                                    changed |= state.entry.insert(*function);
                                }
                                low |= state.returns.contains(*function);
                            }
                            _ => {}
                        }
                    }
                    if let Some(term) = &block.terminator {
                        if let Terminator::Return { values } = term
                            && values.iter().any(|&value| {
                                func.value_u64(value).is_none()
                                    && self.fmp_value_may_be_low(
                                        func,
                                        &aliases[id],
                                        value,
                                        &reads_low[id],
                                        id,
                                        &state,
                                    ) == Some(true)
                            })
                        {
                            changed |= state.pointer_returns.insert(id);
                        }
                        if let Terminator::TailCall { function, args } = term {
                            for (index, &arg) in args.iter().enumerate() {
                                if func.value_u64(arg).is_none()
                                    && self.fmp_value_may_be_low(
                                        func,
                                        &aliases[id],
                                        arg,
                                        &reads_low[id],
                                        id,
                                        &state,
                                    ) == Some(true)
                                {
                                    changed |= state
                                        .args
                                        .entry(*function)
                                        .or_insert_with(|| {
                                            DenseBitSet::new_empty(
                                                module.functions[*function].params.len(),
                                            )
                                        })
                                        .insert(ArgIdx::from_usize(index));
                                }
                            }
                            if state.pointer_returns.contains(*function) {
                                changed |= state.pointer_returns.insert(id);
                            }
                            if low {
                                changed |= state.entry.insert(*function);
                            }
                            low |= state.returns.contains(*function);
                        }
                        if low {
                            for next in term.successors() {
                                changed |= block_low[id].insert(next);
                            }
                            if matches!(
                                term,
                                Terminator::Return { .. }
                                    | Terminator::Stop
                                    | Terminator::TailCall { .. }
                            ) {
                                changed |= state.returns.insert(id);
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
        let mut affected = DenseBitSet::new_empty(module.functions.len());
        if !heap_clobbers.is_empty() {
            for id in module.functions.indices() {
                if Some(id) == module.dispatch_entry() {
                    continue;
                }
                let mut component = call_graph.reachable_callees_from([id]);
                component.insert(id);
                if heap_clobbers.iter().any(|callee| component.contains(callee)) {
                    affected.union(&component);
                }
            }
        }
        affected
    }

    fn fmp_value_may_be_low(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        reads_low: &DenseBitSet<InstId>,
        func_id: FunctionId,
        state: &LowFmpState,
    ) -> Option<bool> {
        if func.value_u64(value).is_some_and(|value| value <= EvmMemoryLayout::HEAP_START) {
            return Some(true);
        }
        let mut visiting = DenseBitSet::new_empty(func.num_values());
        let mut memo = FxHashMap::default();
        let mut rooted =
            self.heap_pointer_provenance(func, aa, value, &mut visiting, &mut memo) == Some(true);
        let mut pending = vec![value];
        visiting.clear();
        while let Some(value) = pending.pop() {
            // Heap-typed arguments do not prove the incoming pointer is above fixed frames.
            if let Value::Arg(arg) = func.value(value) {
                if state.args.get(&func_id).is_some_and(|args| args.contains(*arg)) {
                    return Some(true);
                }
                rooted = false;
            }
            if visiting.insert(value)
                && let Value::Inst(inst) = func.value(value)
            {
                if reads_low.contains(*inst) {
                    return Some(true);
                }
                if let InstKind::ICall { function, .. } = func.inst(*inst).kind
                    && (state.entry.contains(function)
                        || state.returns.contains(function)
                        || state.pointer_returns.contains(function))
                {
                    return Some(true);
                }
                pending.extend(func.inst(*inst).kind.operands());
            }
        }
        rooted.then_some(false)
    }

    /// Propagates low-memory clobbers that can return to an internal caller. A copy followed
    /// only by revert/returndata exits the entire call and cannot invalidate caller live-outs.
    pub(in crate::backend::evm::codegen) fn collect_spill_clobber_functions(
        &mut self,
        module: &Module,
    ) -> DenseBitSet<FunctionId> {
        self.spill_clobber_args.clear();
        let mut clobbers = DenseBitSet::new_empty(module.functions.len());
        let mut direct_args = IndexVec::<FunctionId, _>::with_capacity(module.functions.len());
        let mut callees = IndexVec::<FunctionId, Vec<(FunctionId, Option<InstId>)>>::with_capacity(
            module.functions.len(),
        );
        for (id, func) in module.functions.iter_enumerated() {
            let mut returning = DenseBitSet::new_empty(func.blocks.len());
            let mut pending = func
                .blocks
                .iter_enumerated()
                .filter_map(|(id, block)| {
                    matches!(
                        block.terminator,
                        Some(
                            Terminator::Return { .. }
                                | Terminator::Stop
                                | Terminator::TailCall { .. }
                        )
                    )
                    .then_some(id)
                })
                .collect::<Vec<_>>();
            while let Some(block) = pending.pop() {
                if returning.insert(block) {
                    pending.extend(func.blocks[block].predecessors.iter().copied());
                }
            }
            let hazards = self.compute_spill_hazard_insts(func);
            let aa = AliasAnalysis::new(func);
            let mut returning_callees = Vec::new();
            let mut clobber_args = DenseBitSet::new_empty(func.params.len());
            let mut only_args = true;
            for block in returning.iter() {
                for &inst in &func.blocks[block].instructions {
                    if hazards.contains(&inst) {
                        clobbers.insert(id);
                        if let Some(dest) = Self::dynamic_spill_write_dest(func, inst)
                            && let Some(args) = self.spill_destination_args(func, &aa, dest)
                        {
                            clobber_args.union(&args);
                        } else {
                            only_args = false;
                        }
                    }
                    if let InstKind::ICall { function, .. } = func.inst(inst).kind {
                        returning_callees.push((function, Some(inst)));
                    }
                }
                if let Some(Terminator::TailCall { function, .. }) = func.blocks[block].terminator {
                    returning_callees.push((function, None));
                }
            }
            if clobbers.contains(id) && only_args && returning_callees.is_empty() {
                self.spill_clobber_args.insert(id, clobber_args.clone());
            }
            direct_args.push(only_args.then_some(clobber_args));
            callees.push(returning_callees);
        }
        loop {
            let mut changed = false;
            for (id, callees) in callees.iter_enumerated() {
                if callees.iter().any(|&(callee, inst)| {
                    clobbers.contains(callee)
                        && inst.is_none_or(|inst| {
                            self.call_may_clobber_spills(&module.functions[id], inst)
                        })
                }) {
                    changed |= clobbers.insert(id);
                }
            }
            if !changed {
                break;
            }
        }
        loop {
            let mut changed = false;
            for (id, calls) in callees.iter_enumerated() {
                if !clobbers.contains(id) || self.spill_clobber_args.contains_key(&id) {
                    continue;
                }
                let Some(mut args) = direct_args[id].clone() else { continue };
                let func = &module.functions[id];
                let aa = AliasAnalysis::new(func);
                let known = calls.iter().all(|&(callee, inst)| {
                    if !clobbers.contains(callee) {
                        return true;
                    }
                    let Some(destinations) = self.spill_clobber_args.get(&callee) else {
                        return false;
                    };
                    let Some(inst) = inst else { return false };
                    let InstKind::ICall { args: actuals, .. } = &func.inst(inst).kind else {
                        return false;
                    };
                    destinations.iter().all(|arg| {
                        if let Some(&dest) = actuals.get(arg.index())
                            && let Some(required) = self.spill_destination_args(func, &aa, dest)
                        {
                            args.union(&required);
                            true
                        } else {
                            false
                        }
                    })
                });
                if known {
                    self.spill_clobber_args.insert(id, args);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        clobbers
    }

    /// Finds a sufficient set of heap-pointer arguments for a symbolic destination.
    /// Start with every argument, then remove assumptions whose absence still proves the pointer.
    fn spill_destination_args(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        dest: ValueId,
    ) -> Option<DenseBitSet<ArgIdx>> {
        let mut args = DenseBitSet::new_filled(func.params.len());
        let prove = |args: &DenseBitSet<ArgIdx>| {
            let mut visiting = DenseBitSet::new_empty(func.num_values());
            let mut memo = FxHashMap::default();
            Self::heap_pointer_provenance_with_helpers(
                func,
                aa,
                dest,
                &self.heap_pointer_return_functions,
                Some(args),
                &mut visiting,
                &mut memo,
            ) == Some(true)
        };
        if !prove(&args) {
            return None;
        }
        for arg in args.iter().collect::<Vec<_>>() {
            args.remove(arg);
            if !prove(&args) {
                args.insert(arg);
            }
        }
        Some(args)
    }

    /// A copy helper only clobbers its destination arguments. Prove those actuals
    /// are heap pointers before propagating its low-memory hazard into a caller.
    fn call_may_clobber_spills(&self, func: &Function, inst: InstId) -> bool {
        let InstKind::ICall { function, args, .. } = &func.inst(inst).kind else { return true };
        let Some(destinations) = self.spill_clobber_args.get(function) else { return true };
        let aa = AliasAnalysis::new(func);
        destinations.iter().any(|arg| {
            args.get(arg.index())
                .is_none_or(|&dest| self.write_dest_may_reach_spills(func, &aa, dest))
        })
    }

    /// Returns the destination of a symbolic memory write that can cover a
    /// compiler spill slot. Constant ranges are handled by the static memory
    /// high-water mark instead.
    pub(in crate::backend::evm::codegen) fn dynamic_spill_write_dest(
        func: &Function,
        inst_id: InstId,
    ) -> Option<ValueId> {
        if matches!(
            func.inst(inst_id).metadata.memory_region(),
            Some(MemoryRegion::AbiReturn | MemoryRegion::Heap | MemoryRegion::InternalFrame)
        ) {
            return None;
        }
        let dynamic_range = |dest, size| {
            // Fixed-width copies have the same explicit-memory contract as
            // `mstore`: arbitrary destinations in hand-written assembly may
            // alias compiler memory. The forwarding-buffer protocol is for
            // variable-length writes that can sweep over every spill slot.
            if func.value_u64(size).is_some() {
                return None;
            }
            let below_spills = func.value_u64(dest).is_some_and(|dest| {
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                Self::value_u64_upper_bound(func, size, &mut visiting)
                    .and_then(|size| dest.checked_add(size))
                    .is_some_and(|end| end <= EvmMemoryLayout::HEAP_START)
            });
            (!below_spills).then_some(dest)
        };
        match func.inst(inst_id).kind {
            InstKind::CalldataCopy(dest, _, size)
            | InstKind::DataCopy(_, dest, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ExtCodeCopy(_, dest, _, size)
            | InstKind::MCopy(dest, _, size) => dynamic_range(dest, size),
            InstKind::ReturnDataCopy(dest, offset, size) => {
                // `returndatacopy(_, returndatasize(), n)` is an OOG guard:
                // `n == 0` writes nothing, while every nonzero length is out
                // of bounds and traps before memory is modified.
                let starts_at_end = Self::is_current_returndata_size(func, offset, inst_id);
                if starts_at_end || Self::returndata_copy_below_spills(func, inst_id, dest) {
                    None
                } else {
                    dynamic_range(dest, size)
                }
            }
            InstKind::Call { ret_offset: dest, ret_size: size, .. }
            | InstKind::CallCode { ret_offset: dest, ret_size: size, .. }
            | InstKind::StaticCall { ret_offset: dest, ret_size: size, .. }
            | InstKind::DelegateCall { ret_offset: dest, ret_size: size, .. }
                if func.value_u64(size) != Some(0) =>
            {
                dynamic_range(dest, size)
            }
            _ => None,
        }
    }

    /// Proves that a guarded return-data buffer cannot reach spill slots. Only a direct
    /// switch predecessor is considered, and neither block may replace the observed buffer.
    fn returndata_copy_below_spills(func: &Function, copy: InstId, dest: ValueId) -> bool {
        let Some(dest) = func.value_u64(dest) else { return false };
        let Some((block_id, block, position)) =
            func.blocks.iter_enumerated().find_map(|(id, block)| {
                block
                    .instructions
                    .iter()
                    .position(|&inst| inst == copy)
                    .map(|position| (id, block, position))
            })
        else {
            return false;
        };
        let [predecessor] = block.predecessors.as_slice() else { return false };
        let predecessor = &func.blocks[*predecessor];
        let Some(Terminator::Switch { value, default, cases }) = &predecessor.terminator else {
            return false;
        };
        if *default == block_id
            || !Self::returndata_size_is_current(func, *value, &predecessor.instructions)
            || block.instructions[..position]
                .iter()
                .any(|&inst| Self::replaces_returndata(func, inst))
        {
            return false;
        }
        let mut matched = false;
        for &(value, target) in cases {
            if target == block_id {
                matched = true;
                if func
                    .value_u64(value)
                    .and_then(|size| dest.checked_add(size))
                    .is_none_or(|end| end > EvmMemoryLayout::HEAP_START)
                {
                    return false;
                }
            }
        }
        matched
    }

    /// Proves that a size observation still describes the current return-data buffer.
    /// Restrict the proof to one block; calls and creations can replace that buffer.
    fn is_current_returndata_size(func: &Function, value: ValueId, before: InstId) -> bool {
        func.blocks.iter().any(|block| {
            block.instructions.iter().position(|&inst| inst == before).is_some_and(|position| {
                Self::returndata_size_is_current(func, value, &block.instructions[..position])
            })
        })
    }

    fn returndata_size_is_current(
        func: &Function,
        value: ValueId,
        instructions: &[InstId],
    ) -> bool {
        let Value::Inst(definition) = func.value(value) else { return false };
        if !matches!(func.inst(*definition).kind, InstKind::ReturnDataSize) {
            return false;
        }
        for &inst in instructions.iter().rev() {
            if inst == *definition {
                return true;
            }
            if Self::replaces_returndata(func, inst) {
                return false;
            }
        }
        false
    }

    fn replaces_returndata(func: &Function, inst: InstId) -> bool {
        matches!(
            func.inst(inst).kind.effect_kind(),
            EffectKind::ExternalCall | EffectKind::ICall | EffectKind::Create
        )
    }

    /// Returns a conservative upper bound for a small integer expression.
    fn value_u64_upper_bound(
        func: &Function,
        value: ValueId,
        visiting: &mut DenseBitSet<ValueId>,
    ) -> Option<u64> {
        if let Some(value) = func.value_u64(value) {
            return Some(value);
        }
        if !visiting.insert(value) {
            return None;
        }
        let bound = match func.value(value) {
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::Select(_, if_true, if_false) => Some(
                    Self::value_u64_upper_bound(func, if_true, visiting)?
                        .max(Self::value_u64_upper_bound(func, if_false, visiting)?),
                ),
                _ => None,
            },
            Value::Arg(_) | Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => None,
        };
        visiting.remove(value);
        bound
    }

    /// Collects symbolic low-memory clobbers that can overwrite the spill
    /// area. This includes variable-length copy opcodes and call return
    /// buffers. Alias analysis excludes destinations rooted at the free-memory
    /// pointer, allocations, or internal frames. Single-word stores do not
    /// need the forwarding-buffer protocol: their exact runtime address does
    /// not create an unbounded clobber range.
    pub(in crate::backend::evm::codegen) fn compute_spill_hazard_insts(
        &self,
        func: &Function,
    ) -> FxHashSet<InstId> {
        let mut hazards = func
            .instructions()
            .filter(|&inst| {
                matches!(func.inst(inst).kind, InstKind::ICall { function, .. }
                if self.spill_clobber_functions.contains(function) && self.call_may_clobber_spills(func, inst))
            })
            .collect::<FxHashSet<_>>();
        let candidates = func
            .instructions()
            .filter_map(|inst_id| {
                Self::dynamic_spill_write_dest(func, inst_id).map(|dest| (inst_id, dest))
            })
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return hazards;
        }
        let aa = AliasAnalysis::new(func);
        for (inst_id, dest) in candidates {
            if self.write_dest_may_reach_spills(func, &aa, dest) {
                hazards.insert(inst_id);
            }
        }
        hazards
    }

    /// Whether a dynamic-length write's destination may overlap the spill area.
    /// A free-memory-pointer, allocation, or internal-frame destination stays
    /// in compiler-owned high memory; a symbolic low base (raw
    /// `returndatasize()` addressing) or a low absolute base can reach the
    /// fixed low-memory spill slots.
    fn write_dest_may_reach_spills(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        dest: ValueId,
    ) -> bool {
        let Some(address) = aa.memory_address(func, dest) else {
            return true;
        };
        if matches!(address.region, MemoryRegion::Heap | MemoryRegion::InternalFrame) {
            return false;
        }
        match address.base {
            MemoryBase::Allocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::InternalFrame => false,
            MemoryBase::Absolute => {
                address.offset < EvmMemoryLayout::HEAP_START.saturating_add(SPILL_HAZARD_BOUND)
            }
            MemoryBase::Value(value) => {
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                self.heap_pointer_provenance(func, aa, value, &mut visiting, &mut memo)
                    != Some(true)
            }
        }
    }

    /// Finds leaf helpers that return a pointer rooted at the free-memory pointer.
    /// Calls through these helpers lose alias provenance in MIR, so remember the
    /// narrow interprocedural fact needed by forwarding-buffer hazard analysis.
    pub(in crate::backend::evm::codegen) fn collect_heap_pointer_return_functions(
        module: &Module,
    ) -> DenseBitSet<FunctionId> {
        let mut functions = DenseBitSet::new_empty(module.functions.len());
        let no_helpers = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if func.instructions().any(|inst_id| {
                matches!(func.inst(inst_id).kind, InstKind::ICall { .. } | InstKind::SetFmp(_))
                    || matches!(
                        func.inst(inst_id).kind,
                        InstKind::MStore(address, _)
                            if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT)
                    )
            }) {
                continue;
            }

            let aa = AliasAnalysis::new(func);
            let mut saw_return = false;
            let mut valid = true;
            for block in &func.blocks {
                let Some(Terminator::Return { values }) = &block.terminator else { continue };
                saw_return = true;
                if values.len() != 1 {
                    valid = false;
                    break;
                }
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                if Self::heap_pointer_provenance_with_helpers(
                    func,
                    &aa,
                    values[0],
                    &no_helpers,
                    None,
                    &mut visiting,
                    &mut memo,
                ) != Some(true)
                {
                    valid = false;
                    break;
                }
            }
            if saw_return && valid {
                functions.insert(func_id);
            }
        }
        functions
    }

    /// Returns `Some(grounded)` for a heap-pointer derivation. Recursive phi
    /// edges are provisionally valid but ungrounded; every accepted cycle must
    /// also contain a concrete FMP, allocation, or qualified-helper origin.
    fn heap_pointer_provenance(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, bool>,
    ) -> Option<bool> {
        Self::heap_pointer_provenance_with_helpers(
            func,
            aa,
            value,
            &self.heap_pointer_return_functions,
            None,
            visiting,
            memo,
        )
    }

    fn heap_pointer_provenance_with_helpers(
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        helper_returns: &DenseBitSet<FunctionId>,
        heap_args: Option<&DenseBitSet<ArgIdx>>,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, bool>,
    ) -> Option<bool> {
        if let Some(&grounded) = memo.get(&value) {
            return Some(grounded);
        }
        if !visiting.insert(value) {
            return Some(false);
        }

        let aligned_mask = |value: ValueId| {
            let mask = func.value_u256(value).or_else(|| {
                let Value::Inst(inst) = func.value(value) else { return None };
                let InstKind::Not(operand) = func.inst(*inst).kind else { return None };
                func.value_u256(operand).map(|value| !value)
            });
            mask.is_some_and(|mask| {
                mask == U256::MAX - U256::from(31)
                    || mask == U256::from(u64::MAX.saturating_sub(31))
            })
        };
        let derive = |value, visiting: &mut DenseBitSet<ValueId>, memo: &mut FxHashMap<_, _>| {
            Self::heap_pointer_provenance_with_helpers(
                func,
                aa,
                value,
                helper_returns,
                heap_args,
                visiting,
                memo,
            )
        };

        let provenance = aa
            .memory_address(func, value)
            .and_then(|address| matches!(address.region, MemoryRegion::Heap).then_some(true))
            .or_else(|| {
                if let Value::Arg(arg) = func.value(value)
                    && (func.value_ty(value).is_some_and(MirType::is_memory_reference)
                        || heap_args.is_some_and(|args| args.contains(*arg)))
                {
                    return Some(true);
                }
                let Value::Inst(inst_id) = func.value(value) else { return None };
                match &func.inst(*inst_id).kind {
                    InstKind::Fmp | InstKind::Alloc { .. } => Some(true),
                    InstKind::MLoad(address)
                        if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                    {
                        Some(true)
                    }
                    InstKind::ICall { function, returns: 1, .. }
                        if helper_returns.contains(*function) =>
                    {
                        Some(true)
                    }
                    InstKind::Add(first, second) => {
                        derive(*first, visiting, memo).or_else(|| derive(*second, visiting, memo))
                    }
                    InstKind::Sub(base, _) => derive(*base, visiting, memo),
                    InstKind::And(first, second) if aligned_mask(*second) => {
                        derive(*first, visiting, memo)
                    }
                    InstKind::And(first, second) if aligned_mask(*first) => {
                        derive(*second, visiting, memo)
                    }
                    InstKind::MemoryObjectData(object, _)
                    | InstKind::MemoryObjectFieldAddr { object, .. }
                    | InstKind::MemoryObjectElementAddr { object, .. } => {
                        derive(*object, visiting, memo)
                    }
                    InstKind::Phi(incoming) => {
                        let mut grounded = false;
                        for &(_, incoming) in incoming {
                            grounded |= derive(incoming, visiting, memo)?;
                        }
                        Some(grounded)
                    }
                    InstKind::Select(_, then_value, else_value) => Some(
                        derive(*then_value, visiting, memo)? | derive(*else_value, visiting, memo)?,
                    ),
                    _ => None,
                }
            });
        visiting.remove(value);
        if let Some(grounded) = provenance {
            memo.insert(value, grounded);
        }
        provenance
    }

    /// Returns whether a function can read, write, or observe the reserved free-memory-pointer
    /// word. Unknown offsets and lengths are conservatively overlapping; constant ranges proven
    /// disjoint from `[0x40, 0x60)` keep the lazy entry initialization optimization.
    pub(in crate::backend::evm::codegen) fn function_may_observe_free_memory_slot(
        func: &Function,
    ) -> bool {
        let overlaps = |offset, size| {
            Self::constant_memory_range_may_overlap_fmp(
                func.value_u64(offset),
                func.value_u64(size),
            )
        };
        let overlaps_const = |offset, size| {
            Self::constant_memory_range_may_overlap_fmp(func.value_u64(offset), Some(size))
        };
        if func.instructions().any(|inst_id| match &func.inst(inst_id).kind {
            InstKind::MLoad(offset) | InstKind::MStore(offset, _) => {
                overlaps_const(*offset, EvmMemoryLayout::WORD_SIZE)
            }
            InstKind::MStore8(offset, _) => overlaps_const(*offset, 1),
            InstKind::MemoryZero(offset, size)
            | InstKind::Keccak256(offset, size)
            | InstKind::CalldataCopy(offset, _, size)
            | InstKind::DataCopy(_, offset, size)
            | InstKind::CodeCopy(offset, _, size)
            | InstKind::ReturnDataCopy(offset, _, size)
            | InstKind::ExtCodeCopy(_, offset, _, size) => overlaps(*offset, *size),
            InstKind::MCopy(dest, src, size) => overlaps(*dest, *size) || overlaps(*src, *size),
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                overlaps(*args_offset, *args_size) || overlaps(*ret_offset, *ret_size)
            }
            InstKind::Create(_, offset, size) | InstKind::Create2(_, offset, size, _) => {
                overlaps(*offset, *size)
            }
            InstKind::Log0(offset, size)
            | InstKind::Log1(offset, size, _)
            | InstKind::Log2(offset, size, _, _)
            | InstKind::Log3(offset, size, _, _, _)
            | InstKind::Log4(offset, size, _, _, _, _) => overlaps(*offset, *size),
            InstKind::MSize | InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc { .. } => true,
            // These semantic memory operations are normally gone by the `evm-shaped` phase. If
            // one remains, its complete accessed range is not represented as physical operands
            // here, so retain the Solidity memory invariant conservatively.
            InstKind::MemoryObjectLen(_, _)
            | InstKind::SetMemoryObjectLen(_, _, _)
            | InstKind::MemoryObjectData(_, _)
            | InstKind::MemoryObjectFieldAddr { .. }
            | InstKind::MemoryObjectElementAddr { .. }
            | InstKind::AbiEncode { .. }
            | InstKind::StorageToMemory { .. }
            | InstKind::MemoryToStorage { .. }
            | InstKind::Keccak256Bytes(_)
            | InstKind::MappingSlotMemory(_, _) => true,
            _ => false,
        }) {
            return true;
        }

        func.blocks.iter().any(|block| match block.terminator.as_ref() {
            Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size }) => {
                overlaps(*offset, *size)
            }
            _ => false,
        })
    }

    pub(in crate::backend::evm::codegen) fn constant_memory_range_may_overlap_fmp(
        offset: Option<u64>,
        size: Option<u64>,
    ) -> bool {
        if size == Some(0) {
            return false;
        }
        let Some(offset) = offset else { return true };
        let start = EvmMemoryLayout::FMP_SLOT;
        let end = start + EvmMemoryLayout::WORD_SIZE;
        if offset >= end {
            return false;
        }
        let Some(size) = size else { return true };
        offset.checked_add(size).is_none_or(|range_end| range_end > start)
    }
}

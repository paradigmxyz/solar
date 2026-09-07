//! Argument residency, lazy materialization, and retained call operands.

use super::super::{
    ArgIdx, BlockId, CanonicalArgValues, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function,
    FunctionId, FxHashMap, GLOBAL_STACK_LAYOUT_LIMIT, GlobalStackPlan, InstKind, LazyStackArgPlan,
    Liveness, MAX_STACK_ACCESS, Module, OptimizationMode, SpillSlot, StackArgRetentionPlan,
    StackArgUseInfo, StackModel, StackOp, StackScheduler, StaticCallEntry, StaticCallStackWord,
    TargetSlot, Terminator, U256, ValueId, WORD_BYTES, op, rematerializable_nullary_value,
};

const STACK_ARG_ROTATION_LIMIT: usize = 16;

impl<'gcx> EvmCodegen<'gcx> {
    /// Promotes a profitable subset of stack arguments to a callee-wide physical layout. The
    /// ordinary stack-argument convention pays one
    /// prologue `MSTORE` and subsequent frame `MLOAD`s. A proven resident
    /// layout removes both, including across joins and loops.
    pub(in crate::backend::evm::codegen) fn compute_resident_stack_args(
        &mut self,
        module: &Module,
        arg_values: &FxHashMap<FunctionId, CanonicalArgValues>,
    ) {
        for abi in self.static_call_abis.values_mut() {
            if matches!(abi.entry, StaticCallEntry::Resident { .. }) {
                abi.entry = StaticCallEntry::Stored;
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        // Start with every used canonical argument of an eligible function.
        // A computed actual can use the same validated spill/reload fallback
        // as ordinary stack arguments. Residency removes the callee prologue
        // store and every later frame load, so it can amortize that caller-side
        // copy even when the actual is not directly rematerializable.
        let mut candidates = FxHashMap::default();
        for (&func_id, values) in arg_values {
            if self.disabled_stack_only_functions.contains(func_id)
                || !self.static_frame_functions.contains(func_id)
                || self.recursive_stack_functions.contains(func_id)
                || self.recursion_reaching_functions.contains(func_id)
            {
                continue;
            }
            let func = &module.functions[func_id];
            let mut mask = DenseBitSet::new_empty(func.params.len());
            for index in 0..func.params.len() {
                if values[ArgIdx::new(index)].is_some() {
                    mask.insert(index);
                }
            }
            if !mask.is_empty() {
                candidates.insert(func_id, mask);
            }
        }

        let mut seen = DenseBitSet::new_empty(module.functions.len());
        let mut excluded = DenseBitSet::new_empty(module.functions.len());
        for (caller_id, caller) in module.functions.iter_enumerated() {
            let raw_leaves_ok =
                Self::is_external_entry(caller) || self.static_frame_functions.contains(caller_id);
            for block in &caller.blocks {
                for &inst_id in &block.instructions {
                    let InstKind::ICall { function, args, .. } = &caller.inst(inst_id).kind else {
                        continue;
                    };
                    let Some(mask) = candidates.get_mut(function) else { continue };
                    seen.insert(*function);
                    if args.len() != mask.domain_size() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (index, &arg) in args.iter().enumerate() {
                        if !Self::stack_arg_site_eligible(caller, raw_leaves_ok, arg) {
                            mask.remove(index);
                        }
                    }
                }
                if let Some(Terminator::TailCall { function, args }) = &block.terminator
                    && let Some(mask) = candidates.get_mut(function)
                {
                    seen.insert(*function);
                    if args.len() != mask.domain_size() {
                        excluded.insert(*function);
                        continue;
                    }
                    for (index, &arg) in args.iter().enumerate() {
                        if !Self::raw_arg_emittable(caller, raw_leaves_ok, arg)
                            && !matches!(caller.value(arg), crate::mir::Value::Inst(_))
                        {
                            mask.remove(index);
                        }
                    }
                }
            }
        }

        for (func_id, mut mask) in candidates {
            if !seen.contains(func_id) || excluded.contains(func_id) || mask.is_empty() {
                continue;
            }
            if mask.count() > GLOBAL_STACK_LAYOUT_LIMIT {
                let retained: Vec<_> = mask.iter().take(GLOBAL_STACK_LAYOUT_LIMIT).collect();
                mask.clear();
                for index in retained {
                    mask.insert(index);
                }
            }

            let func = &module.functions[func_id];
            let arg_values = &arg_values[&func_id];
            let mut values = Vec::with_capacity(mask.count());
            let mut eligible = true;
            for index in mask.iter() {
                let Some(value) = arg_values[ArgIdx::new(index)] else {
                    eligible = false;
                    break;
                };
                values.push(value);
            }
            if !eligible {
                continue;
            }
            // The layout keeps arguments in descending index order.
            values.reverse();

            // Reject shapes the resident-layout analysis cannot represent before paying for
            // whole-function liveness. Single-block leaves have no inter-block layout to solve.
            if func.blocks.iter().any(|block| {
                !self.preserve_caller_stack
                    && block
                        .instructions
                        .iter()
                        .any(|&inst_id| matches!(func.inst(inst_id).kind, InstKind::ICall { .. }))
            }) {
                continue;
            }
            let has_phis =
                func.instructions().any(|inst| matches!(func.inst(inst).kind, InstKind::Phi(_)));
            let liveness = (func.blocks.len() != 1 || has_phis).then(|| Liveness::compute(func));
            let plan = if let Some(liveness) = &liveness {
                let context = self.resident_search_context(func, liveness, &values, has_phis);
                if let Some((plan, _)) = self.analyze_resident_subset(
                    func,
                    liveness,
                    &values,
                    self.preserve_caller_stack,
                    &context,
                ) {
                    // Preserve the established full-tuple layout when it passes the structural
                    // amortization guard. Costed subset selection is a fallback for tuples where
                    // one difficult value would otherwise disable every independent resident
                    // argument.
                    plan
                } else if let Some((subset, plan)) = self.select_resident_layout(
                    func,
                    liveness,
                    &values,
                    self.preserve_caller_stack,
                    has_phis,
                ) {
                    values = subset;
                    mask.clear();
                    for &value in &values {
                        let crate::mir::Value::Arg(index) = func.value(value) else {
                            unreachable!("resident candidates are canonical arguments")
                        };
                        mask.insert(index.index());
                    }
                    plan
                } else {
                    continue;
                }
            } else {
                GlobalStackPlan {
                    entries: FxHashMap::default(),
                    aliases: FxHashMap::default(),
                    terminal_sensitive: true,
                }
            };
            let abi = self.static_call_abi_mut(func_id, func.params.len());
            let mut stack_args = DenseBitSet::new_empty(func.params.len());
            for index in abi.stack_args.iter().chain(mask.iter()) {
                if arg_values[ArgIdx::new(index)].is_some() {
                    stack_args.insert(index);
                }
            }
            abi.stack_args = stack_args;
            abi.entry = StaticCallEntry::Resident { values, layout: plan };
        }
    }

    /// Collects operand-use facts shared by lazy and direct stack-argument selection.
    pub(in crate::backend::evm::codegen) fn collect_stack_arg_uses(
        &self,
        module: &Module,
    ) -> FxHashMap<FunctionId, StackArgUseInfo> {
        let mut all_uses = FxHashMap::default();
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return all_uses;
        }

        for (&func_id, abi) in &self.static_call_abis {
            if matches!(abi.entry, StaticCallEntry::Resident { .. }) || abi.stack_args.is_empty() {
                continue;
            }
            let func = &module.functions[func_id];
            let mut info = StackArgUseInfo {
                use_counts: FxHashMap::default(),
                non_entry_uses: DenseBitSet::new_empty(func.num_values()),
                call_uses: DenseBitSet::new_empty(func.num_values()),
                entry_first_uses: FxHashMap::default(),
                first_entry_call: None,
            };
            for (block_id, block) in func.blocks.iter_enumerated() {
                for (inst_idx, &inst_id) in block.instructions.iter().enumerate() {
                    let kind = &func.inst(inst_id).kind;
                    let is_call = matches!(kind, InstKind::ICall { .. });
                    if block_id == BlockId::ENTRY && info.first_entry_call.is_none() && is_call {
                        info.first_entry_call = Some(inst_idx);
                    }
                    for operand in kind.operands() {
                        *info.use_counts.entry(operand).or_insert(0) += 1;
                        if block_id == BlockId::ENTRY {
                            info.entry_first_uses.entry(operand).or_insert(inst_idx);
                        } else {
                            info.non_entry_uses.insert(operand);
                        }
                        if is_call {
                            info.call_uses.insert(operand);
                        }
                    }
                }
                if let Some(term) = &block.terminator {
                    let is_call = matches!(term, Terminator::TailCall { .. });
                    for operand in term.operands() {
                        *info.use_counts.entry(operand).or_insert(0) += 1;
                        if block_id != BlockId::ENTRY {
                            info.non_entry_uses.insert(operand);
                        }
                        if is_call {
                            info.call_uses.insert(operand);
                        }
                    }
                }
            }
            all_uses.insert(func_id, info);
        }
        all_uses
    }

    /// Selects stack-passed arguments that the callee can consume directly.
    ///
    /// Every selected argument must have one canonical value identity and all of its uses must stay
    /// in the entry block. The local scheduler can then retain or duplicate that physical word for
    /// as many operations as need it without giving it a memory home. Arguments consumed by a
    /// nested call remain frame-passed until call layouts can carry stack-only values through the
    /// nested edge. Requiring the entire stack-argument mask to qualify lets the prologue omit
    /// every store without shuffling around partially materialized words.
    pub(in crate::backend::evm::codegen) fn compute_direct_stack_args(
        &mut self,
        module: &Module,
        arg_values: &FxHashMap<FunctionId, CanonicalArgValues>,
        use_info: &FxHashMap<FunctionId, StackArgUseInfo>,
    ) {
        for abi in self.static_call_abis.values_mut() {
            if matches!(abi.entry, StaticCallEntry::Direct(_)) {
                abi.entry = StaticCallEntry::Stored;
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        let candidates: Vec<_> = self
            .static_call_abis
            .iter()
            .filter(|(_, abi)| matches!(abi.entry, StaticCallEntry::Stored))
            .map(|(&func_id, abi)| (func_id, abi.stack_args.clone()))
            .collect();
        for (func_id, mask) in candidates {
            if self.disabled_stack_only_functions.contains(func_id) {
                continue;
            }
            let func = &module.functions[func_id];
            if mask.domain_size() != func.params.len() {
                continue;
            }
            if mask.count() > 4 {
                continue;
            }
            // A direct stack argument has no frame home and cannot own a spill slot, so a
            // stack-draining internal or tail call between the entry stack and a later use would
            // drop it and reload it from its never-written frame slot. `use_info` only rejects
            // arguments consumed *by* a call, not ones merely live across one, so exclude any
            // callee that makes a call at all. Resident and lazy selection already
            // cover the callee shapes that survive a drain.
            if func.blocks.iter().any(|block| {
                block
                    .instructions
                    .iter()
                    .any(|&inst| matches!(func.inst(inst).kind, InstKind::ICall { .. }))
                    || matches!(
                        block.terminator,
                        Some(Terminator::TailCall { .. } | Terminator::Switch { .. })
                    )
            }) {
                continue;
            }
            let Some(arg_values) = arg_values.get(&func_id) else { continue };
            let Some(info) = use_info.get(&func_id) else { continue };

            let mut values = Vec::with_capacity(mask.count());
            let mut eligible = true;
            for index in mask.iter() {
                let Some(value) = arg_values[ArgIdx::new(index)] else {
                    eligible = false;
                    break;
                };
                if info.use_counts.get(&value).copied().unwrap_or(0) == 0
                    || info.non_entry_uses.contains(value)
                    || info.call_uses.contains(value)
                {
                    eligible = false;
                    break;
                }
                values.push(value);
            }
            // The entry layout keeps arguments in descending index order.
            values.reverse();
            if eligible && !values.is_empty() {
                self.static_call_abi_mut(func_id, func.params.len()).entry =
                    StaticCallEntry::Direct(values);
            }
        }
    }

    /// Selects stack arguments whose first memory materialization can move past their first use.
    ///
    /// The whole mask must qualify because the incoming words are contiguous above the return
    /// address. Each selected argument needs an identity used by the entry block's first
    /// instruction. A repeated argument gets a frame home immediately before that instruction; a
    /// single-use argument is consumed directly from the incoming stack. This restriction keeps
    /// the rewrite local and prevents it from changing later stack scheduling or CFG layout.
    pub(in crate::backend::evm::codegen) fn compute_lazy_stack_args(
        &mut self,
        module: &Module,
        arg_values: &FxHashMap<FunctionId, CanonicalArgValues>,
        use_info: &FxHashMap<FunctionId, StackArgUseInfo>,
    ) {
        for abi in self.static_call_abis.values_mut() {
            if matches!(abi.entry, StaticCallEntry::Lazy(_)) {
                abi.entry = StaticCallEntry::Stored;
            }
        }
        if matches!(self.gcx.sess.opts.optimization, OptimizationMode::None) {
            return;
        }

        let candidates: Vec<_> = self
            .static_call_abis
            .iter()
            .filter(|(_, abi)| matches!(abi.entry, StaticCallEntry::Stored))
            .map(|(&func_id, abi)| (func_id, abi.stack_args.clone()))
            .collect();
        for (func_id, mask) in candidates {
            if self.disabled_stack_only_functions.contains(func_id) {
                continue;
            }
            if mask.count() > MAX_STACK_ACCESS {
                continue;
            }
            let func = &module.functions[func_id];
            if mask.domain_size() != func.params.len() {
                continue;
            }

            let Some(arg_values) = arg_values.get(&func_id) else { continue };
            let Some(info) = use_info.get(&func_id) else { continue };
            let mut args = Vec::with_capacity(mask.count());
            let mut frame_values = DenseBitSet::new_empty(func.num_values());
            let mut eligible = true;
            for index in mask.iter() {
                let Some(value) = arg_values[ArgIdx::new(index)] else {
                    eligible = false;
                    break;
                };
                let Some(&first_use) = info.entry_first_uses.get(&value) else {
                    eligible = false;
                    break;
                };
                if info.first_entry_call.is_some_and(|call| first_use >= call) {
                    eligible = false;
                    break;
                }
                if first_use != 0 {
                    eligible = false;
                    break;
                }
                args.push((ArgIdx::new(index), value));
                let total_uses = info.use_counts.get(&value).copied().unwrap_or(0);
                if total_uses > 1 {
                    frame_values.insert(value);
                }
            }
            // Materialization emits in descending index order.
            args.reverse();
            if eligible && !args.is_empty() {
                self.static_call_abi_mut(func_id, func.params.len()).entry =
                    StaticCallEntry::Lazy(LazyStackArgPlan { args, frame_values });
            }
        }
    }

    /// Returns true when the caller can re-emit `val` raw (untracked) after
    /// its stack drain.
    pub(in crate::backend::evm::codegen) fn raw_arg_emittable(
        func: &Function,
        raw_leaves_ok: bool,
        val: ValueId,
    ) -> bool {
        match func.value(val) {
            crate::mir::Value::Immediate(imm) => imm.as_u256().is_some(),
            crate::mir::Value::Arg(_) => raw_leaves_ok,
            crate::mir::Value::Inst(_) => rematerializable_nullary_value(func, val).is_some(),
            _ => false,
        }
    }

    /// Returns whether one call site can participate in a stack-argument convention. Computed
    /// values reload from a validated spill after draining the modeled stack; this remains valid
    /// for a dynamic-frame caller because a static call does not replace its frame pointer and the
    /// reload is emitted before control transfers to the callee. Caller arguments do not own spill
    /// slots, so they still require the position-independent raw path. Both ordinary and resident
    /// selection use this predicate to keep their call-site eligibility invariant identical.
    pub(in crate::backend::evm::codegen) fn stack_arg_site_eligible(
        func: &Function,
        raw_leaves_ok: bool,
        val: ValueId,
    ) -> bool {
        Self::raw_arg_emittable(func, raw_leaves_ok, val)
            || matches!(func.value(val), crate::mir::Value::Inst(_))
    }

    /// Emits a mask-qualified argument without touching the scheduler model:
    /// the value lands on the physical stack for the callee prologue, below
    /// everything the caller's model describes.
    pub(in crate::backend::evm::codegen) fn emit_raw_stack_arg(
        &mut self,
        func: &Function,
        val: ValueId,
        spill_slot: Option<SpillSlot>,
        caller_stack: Option<&StackModel>,
        words_above: usize,
    ) {
        if let Some(op) = Self::always_rematerializable_op(func, val) {
            self.asm.emit_op(op);
            return;
        }

        if let crate::mir::Value::Immediate(imm) = func.value(val)
            && imm.as_u256() == Some(U256::ZERO)
            && self.gcx.sess.opts.evm_version.has_push0()
        {
            self.asm.emit_push(U256::ZERO);
            return;
        }

        if let Some(depth) = caller_stack.and_then(|stack| stack.find(val)) {
            let dup = depth + words_above + 1;
            assert!(
                dup <= MAX_STACK_ACCESS,
                "resident caller argument exceeded DUP16 reach at an internal call"
            );
            self.asm.emit_stack_op(StackOp::Dup(dup as u8));
            return;
        }

        match func.value(val) {
            crate::mir::Value::Immediate(imm) => {
                self.asm.emit_push(imm.as_u256().expect("mask requires a word immediate"));
            }
            crate::mir::Value::Arg(index) => {
                if self.in_internal_function {
                    let func_id = self
                        .current_internal_function
                        .expect("internal caller has a current function");
                    let addr = self.static_frame_addr(
                        func_id,
                        EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                            + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
                    );
                    self.asm.emit_push_deferred(addr);
                    self.asm.emit_op(op::MLOAD);
                } else {
                    self.asm.emit_push(U256::from(4 + (index.index() as u64) * WORD_BYTES as u64));
                    self.asm.emit_op(op::CALLDATALOAD);
                }
            }
            crate::mir::Value::Inst(_) => {
                let slot = spill_slot.expect("computed stack argument has a validated spill slot");
                self.emit_spill_load(func, slot);
            }
            other => unreachable!("stack-arg mask admitted an unsupported value: {other:?}"),
        }
    }

    /// Stores the stack-passed arguments of `func_id` into their frame slots.
    /// Arguments were pushed in index order, so the highest index is on top;
    /// after the last store only the return address remains above the
    /// caller's drained stack.
    pub(in crate::backend::evm::codegen) fn emit_stack_arg_prologue(
        &mut self,
        func_id: FunctionId,
        func: &Function,
    ) {
        if !self.runtime_stack_args {
            return;
        }
        if self.direct_stack_args(func_id).is_some() || self.lazy_stack_args(func_id).is_some() {
            return;
        }
        let Some(mask) = self.stack_arg_mask(func_id).cloned() else { return };
        if mask.domain_size() != func.params.len() {
            return;
        }
        // A selective resident ABI may leave only part of the incoming tuple on-stack. Store the
        // other words even when resident words sit above them, then leave exactly the layout that
        // `generate_function_body` adopts. The hidden return address remains immediately below it.
        let stack_indices = mask.iter().collect::<Vec<_>>();
        if let Some(resident) = self.resident_stack_args(func_id).map(|values| values.to_vec()) {
            let mut args = CanonicalArgValues::from_vec(vec![None; func.params.len()]);
            for value in func.live_values() {
                if let crate::mir::Value::Arg(index) = func.value(value) {
                    args[*index] = Some(value);
                }
            }
            let mut incoming = StackModel::new();
            for &index in &stack_indices {
                incoming.push(args[ArgIdx::new(index)].expect("stack argument has no identity"));
            }
            for &index in stack_indices.iter().rev() {
                let value = args[ArgIdx::new(index)].expect("stack argument has no identity");
                if resident.contains(&value) {
                    continue;
                }
                let depth = incoming
                    .find(value)
                    .expect("non-resident stack argument disappeared in the callee prologue");
                if depth != 0 {
                    assert!(depth <= MAX_STACK_ACCESS, "stack argument exceeded SWAP16 reach");
                    self.asm.emit_stack_op(StackOp::Swap(depth as u8));
                    incoming.swap(depth as u8);
                }
                let addr = self.static_frame_addr(
                    func_id,
                    EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                        + index as u64 * EvmMemoryLayout::WORD_SIZE,
                );
                self.asm.emit_push_deferred(addr);
                self.asm.emit_op(op::MSTORE);
                incoming.pop();
            }
            let target: Vec<_> = resident.iter().copied().map(TargetSlot::Value).collect();
            let mut scheduler = StackScheduler::for_evm_version(self.gcx.sess.opts.evm_version);
            scheduler.stack = incoming;
            let shuffle = scheduler.shuffle_to_layout(&target).unwrap_or_else(|| {
                panic!("could not construct selective resident entry layout for `{}`", func.name)
            });
            for op in shuffle.ops {
                self.asm.emit_stack_op(op);
            }
            debug_assert_eq!(
                scheduler.stack.as_slice(),
                resident.iter().copied().map(Some).collect::<Vec<_>>().as_slice(),
                "selective resident prologue produced the wrong entry layout"
            );
            return;
        }

        for i in stack_indices.into_iter().rev() {
            let addr = self.static_frame_addr(
                func_id,
                EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE + i as u64 * EvmMemoryLayout::WORD_SIZE,
            );
            self.asm.emit_push_deferred(addr);
            self.asm.emit_op(op::MSTORE);
        }
    }

    /// Gives a stack-passed argument a valid frame home while retaining its stack copy.
    fn materialize_stack_arg(&mut self, func_id: FunctionId, index: ArgIdx, value: ValueId) {
        if !self.scheduler.is_stack_only_value(value) {
            return;
        }
        let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
            panic!("stack argument {value:?} was lost before frame materialization")
        });
        assert!(depth < MAX_STACK_ACCESS, "stack argument exceeded DUP16 reach");
        self.emit_stack_op(StackOp::Dup((depth + 1) as u8));

        let addr = self.static_frame_addr(
            func_id,
            EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                + (index.index() as u64) * EvmMemoryLayout::WORD_SIZE,
        );
        self.asm.emit_push_deferred(addr);
        self.scheduler.stack.push_unknown();
        self.asm.emit_op(op::MSTORE);
        self.scheduler.instruction_executed(2, None);
        self.scheduler.materialize_stack_only_value(value);
    }

    /// Gives a stack-only value a memory home before a fallback drains the physical stack.
    pub(in crate::backend::evm::codegen) fn materialize_stack_only_home(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        value: ValueId,
    ) {
        if !self.scheduler.is_stack_only_value(value) {
            return;
        }
        match func.value(value) {
            crate::mir::Value::Arg(index) => self.materialize_stack_arg(func_id, *index, value),
            crate::mir::Value::Inst(_) => {
                let depth = self.scheduler.stack.find(value).unwrap_or_else(|| {
                    panic!("stack-only value {value:?} was lost before memory materialization")
                });
                let slot = self.scheduler.spills.allocate(value);
                if depth >= self.stack_access_limit() {
                    self.spill_deep_stack_value(func, value, slot, depth);
                } else {
                    self.spill_accessible_stack_value(func, value, slot, depth);
                }
                self.scheduler.materialize_stack_only_value(value);
            }
            crate::mir::Value::Immediate(_)
            | crate::mir::Value::Undef(_)
            | crate::mir::Value::Error(_) => unreachable!("unsupported stack-only value"),
        }
    }

    /// Gives resident arguments a frame fallback before an emission stage can bury their last
    /// stack copy beyond `DUP16` reach. `transient_growth` bounds the words pushed before the stage
    /// reaches a resident operand, or the one result left by an ordinary MIR instruction.
    pub(in crate::backend::evm::codegen) fn materialize_deep_stack_args(
        &mut self,
        func_id: FunctionId,
        func: &Function,
        transient_growth: usize,
    ) {
        if transient_growth == 0 {
            return;
        }
        let materialize_depth = MAX_STACK_ACCESS.saturating_sub(transient_growth);
        let mut disabled_residency = false;
        loop {
            let entry = self.scheduler.stack.iter().enumerate().find_map(|(depth, value)| {
                value
                    .filter(|&value| {
                        depth >= materialize_depth && self.scheduler.is_stack_only_value(value)
                    })
                    .map(|value| (depth, value))
            });
            let Some((_, value)) = entry else { break };
            disabled_residency |= matches!(func.value(value), crate::mir::Value::Arg(_));
            self.materialize_stack_only_home(func_id, func, value);
        }
        if disabled_residency {
            self.disabled_stack_only_functions.insert(func_id);
        }
    }

    /// Materializes repeated arguments immediately before the entry block's first instruction.
    pub(in crate::backend::evm::codegen) fn materialize_lazy_stack_args(
        &mut self,
        func_id: FunctionId,
        kind: &InstKind,
        block: BlockId,
        inst_idx: usize,
    ) {
        if block != BlockId::ENTRY || inst_idx != 0 {
            return;
        }
        let Some(plan) = self.lazy_stack_args(func_id).cloned() else { return };
        let operands = kind.operands();
        for (index, value) in plan.args {
            debug_assert!(operands.contains(&value));
            if plan.frame_values.contains(value) {
                self.materialize_stack_arg(func_id, index, value);
            }
        }
    }

    /// Plans a bounded rotation that keeps computed arguments on the physical
    /// stack while the rest of the caller stack is drained. The resulting
    /// layout matches the existing stack-argument convention: selected
    /// arguments in descending index order above the return address.
    pub(in crate::backend::evm::codegen) fn plan_retained_stack_args(
        &self,
        func: &Function,
        args: &[ValueId],
        mask: &DenseBitSet<usize>,
    ) -> Option<StackArgRetentionPlan> {
        let selected = mask.count();
        if mask.domain_size() != args.len()
            || selected == 0
            || selected > STACK_ARG_ROTATION_LIMIT
            || self.scheduler.stack.depth() > STACK_ARG_ROTATION_LIMIT + 1
        {
            return None;
        }

        // One physical word cannot fill two argument positions. Repeated
        // values keep the spill-reload path, which materializes each
        // occurrence independently.
        let mut selected_value_counts = FxHashMap::default();
        for (i, &arg) in args.iter().enumerate() {
            if mask.contains(i) && matches!(func.value(arg), crate::mir::Value::Inst(_)) {
                *selected_value_counts.entry(arg).or_insert(0usize) += 1;
            }
        }
        let candidates: Vec<_> = args
            .iter()
            .enumerate()
            .filter_map(|(i, &arg)| {
                (mask.contains(i)
                    && selected_value_counts.get(&arg) == Some(&1)
                    && self.scheduler.stack.contains(arg))
                .then_some(i)
            })
            .collect();
        if candidates.is_empty() {
            return None;
        }
        self.build_stack_arg_retention_plan(args, mask, &candidates)
    }

    fn build_stack_arg_retention_plan(
        &self,
        args: &[ValueId],
        mask: &DenseBitSet<usize>,
        retained_indices: &[usize],
    ) -> Option<StackArgRetentionPlan> {
        let mut keep = FxHashMap::default();
        for &index in retained_indices {
            keep.insert(args[index], index);
        }

        let mut stack = self.scheduler.stack.as_slice().to_vec();
        let mut drain_ops = Vec::new();
        while stack.len() > keep.len() {
            let depth = stack.iter().position(|word| match word {
                Some(value) if keep.contains_key(value) => {
                    stack.iter().filter(|other| **other == *word).count() > 1
                }
                _ => true,
            })?;
            if depth > STACK_ARG_ROTATION_LIMIT {
                return None;
            }
            if depth != 0 {
                drain_ops.push(StackOp::Swap(depth as u8));
                stack.swap(0, depth);
            }
            drain_ops.push(StackOp::Pop);
            stack.remove(0);
        }

        let mut layout = Vec::with_capacity(mask.count() + 1);
        for word in stack {
            layout.push(StaticCallStackWord::Argument(*keep.get(&word?)?));
        }
        layout.insert(0, StaticCallStackWord::ReturnAddress);
        for i in mask.iter() {
            if !retained_indices.contains(&i) {
                layout.insert(0, StaticCallStackWord::Argument(i));
            }
        }

        let mut target: Vec<_> = mask.iter().map(StaticCallStackWord::Argument).collect();
        target.reverse();
        target.push(StaticCallStackWord::ReturnAddress);
        if layout.len() != target.len() || layout.len() > STACK_ARG_ROTATION_LIMIT + 1 {
            return None;
        }

        let mut shuffle_ops = Vec::new();
        for target_depth in (1..layout.len()).rev() {
            if layout[target_depth] == target[target_depth] {
                continue;
            }
            let source_depth =
                layout[..=target_depth].iter().position(|&word| word == target[target_depth])?;
            if source_depth != 0 {
                shuffle_ops.push(StackOp::Swap(source_depth as u8));
                layout.swap(0, source_depth);
            }
            shuffle_ops.push(StackOp::Swap(target_depth as u8));
            layout.swap(0, target_depth);
        }
        debug_assert_eq!(layout, target);

        // Baseline drains every tracked word and reloads each computed stack
        // argument through at least PUSH1+MLOAD. A value without a stored slot
        // also pays at least DUP+PUSH1+MSTORE. Deferred addresses can only make
        // that baseline larger, so this is a conservative byte gate.
        let fresh = retained_indices
            .iter()
            .filter(|&&index| !self.scheduler.spills.is_stored(args[index]))
            .count();
        let baseline_cost = self.scheduler.stack.depth() + retained_indices.len() * 3 + fresh * 4;
        let planned_cost = drain_ops.len() + shuffle_ops.len();
        if planned_cost >= baseline_cost {
            return None;
        }

        let mut retained = DenseBitSet::new_empty(args.len());
        for &index in retained_indices {
            retained.insert(index);
        }
        Some(StackArgRetentionPlan { retained, drain_ops, shuffle_ops })
    }
}

//! Memory provenance and writes that can clobber compiler spill slots.
//!
//! Heap-returning helpers are proved conditionally on a valid incoming free-memory pointer.
//! A separate call-graph walk excludes helpers reached after arbitrary resets. Returning-path
//! summaries ignore terminal failure paths, while caller-state propagation still visits them.
//! Allocation lowering retains its valid-bump proof; dynamic stores need bounded arithmetic,
//! a completed copy, or a loop whose preceding store bounds the next cursor. Unknown writes
//! remain conservative, including cycles without a grounded heap origin.

use super::{
    super::{
        AliasAnalysis, BlockId, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function, FunctionId,
        FxHashMap, FxHashSet, InstId, InstKind, MemoryBase, MemoryRegion, MirType, Module,
        Terminator, U256, Value, ValueId,
    },
    SPILL_HAZARD_BOUND,
};
use crate::mir::{
    Callee,
    analysis::{CallGraphInfo, CfgInfo},
};
use std::cell::OnceCell;

impl<'gcx> EvmCodegen<'gcx> {
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
                let starts_at_end = matches!(
                    func.value(offset),
                    Value::Inst(inst) if matches!(func.inst(*inst).kind, InstKind::ReturnDataSize)
                );
                if starts_at_end { None } else { dynamic_range(dest, size) }
            }
            InstKind::Call { ret_offset: dest, ret_size: size, .. }
            | InstKind::CallCode { ret_offset: dest, ret_size: size, .. }
            | InstKind::StaticCall { ret_offset: dest, ret_size: size, .. }
            | InstKind::DelegateCall { ret_offset: dest, ret_size: size, .. }
                if func.value_u64(size) != Some(0) =>
            {
                dynamic_range(dest, size)
            }
            // A fixed-width store through a loop-carried pointer that starts below the spill
            // area sweeps every slot the loop reaches; across the iterations it is as unbounded
            // as a variable-length copy.
            InstKind::MStore(dest, _) | InstKind::MStore8(dest, _)
                if Self::is_low_sweeping_pointer(func, dest) =>
            {
                Some(dest)
            }
            _ => None,
        }
    }

    /// Whether `pointer` is a phi that enters from an address below the spill area and is
    /// advanced by its own increment on another edge.
    fn is_low_sweeping_pointer(func: &Function, pointer: ValueId) -> bool {
        let Value::Inst(inst_id) = func.value(pointer) else { return false };
        let InstKind::Phi(incoming) = &func.inst(*inst_id).kind else { return false };
        let starts_low = incoming.iter().any(|&(_, value)| {
            func.value_u64(value).is_some_and(|address| address < EvmMemoryLayout::HEAP_START)
        });
        let advances = incoming.iter().any(|&(_, value)| {
            matches!(func.value(value), Value::Inst(step)
                if matches!(func.inst(*step).kind, InstKind::Add(lhs, rhs)
                    if lhs == pointer || rhs == pointer))
        });
        starts_low && advances
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
    /// pointer, allocations, or internal frames. A single-word store does not
    /// need the forwarding-buffer protocol: its exact runtime address does
    /// not create an unbounded clobber range, unless it is repeated through a
    /// loop-carried pointer that starts below the spill area and sweeps it.
    pub(in crate::backend::evm::codegen) fn compute_spill_hazard_insts(
        &self,
        func: &Function,
    ) -> FxHashSet<InstId> {
        let mut hazards = FxHashSet::default();
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
            | MemoryBase::Param(_)
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

    /// Finds helpers returning heap pointers, including chains of already-proven helpers.
    /// Unknown calls and non-heap writes to the free-memory pointer exclude a function.
    /// Iteration only adds proven functions, so recursive call cycles remain unknown.
    pub(in crate::backend::evm::codegen) fn collect_heap_pointer_return_functions(
        module: &Module,
    ) -> DenseBitSet<FunctionId> {
        let mut functions = DenseBitSet::new_empty(module.functions.len());
        let mut preserves_fmp = DenseBitSet::new_empty(module.functions.len());
        loop {
            let mut changed = false;
            for (func_id, func) in module.functions.iter_enumerated() {
                if functions.contains(func_id) {
                    continue;
                }
                let mut returning_blocks = DenseBitSet::new_empty(func.blocks.len());
                let mut worklist = func
                    .blocks
                    .iter_enumerated()
                    .filter_map(|(block, data)| {
                        matches!(
                            data.terminator,
                            Some(Terminator::Return { .. } | Terminator::TailCall { .. })
                        )
                        .then_some(block)
                    })
                    .collect::<Vec<_>>();
                while let Some(block) = worklist.pop() {
                    if returning_blocks.insert(block) {
                        worklist.extend(func.blocks[block].predecessors.iter().copied());
                    }
                }
                let inst_blocks = func.inst_blocks();
                let resets = Self::fmp_reset_insts(func, &functions, &preserves_fmp);
                if resets.iter().any(|inst| returning_blocks.contains(inst_blocks[inst]))
                    || func.blocks.iter().any(|block| matches!(block.terminator,
                        Some(Terminator::TailCall { function, .. }) if !preserves_fmp.contains(function)))
                {
                    continue;
                }
                let aa = AliasAnalysis::new(func);
                let is_heap_pointer = |value| {
                    Self::heap_pointer_provenance_with_helpers(
                        func,
                        &aa,
                        value,
                        &functions,
                        true,
                        &mut DenseBitSet::new_empty(func.num_values()),
                        &mut FxHashMap::default(),
                    ) == Some(true)
                };
                changed |= preserves_fmp.insert(func_id);
                let mut saw_return = false;
                let mut valid = !func
                    .blocks
                    .iter()
                    .any(|block| matches!(block.terminator, Some(Terminator::TailCall { .. })));
                for block in &func.blocks {
                    let Some(Terminator::Return { values }) = &block.terminator else { continue };
                    saw_return = true;
                    if values.len() != 1 {
                        valid = false;
                        break;
                    }
                    if !is_heap_pointer(values[0]) {
                        valid = false;
                        break;
                    }
                }
                if saw_return && valid {
                    functions.insert(func_id);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        // A helper's FMP-relative return needs a valid incoming heap. Seed unknown
        // entry states only at calls reached after a reset, then propagate them.
        let mut unknown_fmp = DenseBitSet::new_empty(module.functions.len());
        for (_, func) in module.functions.iter_enumerated() {
            let resets = Self::fmp_reset_insts(func, &functions, &preserves_fmp);
            if resets.is_empty() {
                continue;
            }
            let mut unknown_blocks = DenseBitSet::new_empty(func.blocks.len());
            loop {
                let mut changed = false;
                for (block_id, block) in func.blocks.iter_enumerated() {
                    let mut unknown = unknown_blocks.contains(block_id);
                    for &inst in &block.instructions {
                        if unknown
                            && let InstKind::ICall { function: Callee::Function(callee), .. } =
                                func.inst(inst).kind
                        {
                            unknown_fmp.insert(callee);
                        }
                        unknown |= resets.contains(&inst);
                    }
                    if unknown && let Some(terminator) = &block.terminator {
                        if let Terminator::TailCall { function, .. } = terminator {
                            unknown_fmp.insert(*function);
                        }
                        for successor in terminator.successors() {
                            changed |= unknown_blocks.insert(successor);
                        }
                    }
                }
                if !changed {
                    break;
                }
            }
        }
        let graph = CallGraphInfo::new(module);
        unknown_fmp.union(&graph.reachable_callees_from(unknown_fmp.iter()));
        functions.subtract(&unknown_fmp);
        functions
    }

    /// Instructions that can invalidate an initially compiler-owned free-memory pointer.
    fn fmp_reset_insts(
        func: &Function,
        functions: &DenseBitSet<FunctionId>,
        preserves_fmp: &DenseBitSet<FunctionId>,
    ) -> FxHashSet<InstId> {
        let aa = AliasAnalysis::new(func);
        let is_heap_pointer = |value| {
            Self::heap_pointer_provenance_with_helpers(
                func,
                &aa,
                value,
                functions,
                true,
                &mut DenseBitSet::new_empty(func.num_values()),
                &mut FxHashMap::default(),
            ) == Some(true)
        };
        let guard_facts = OnceCell::new();
        let is_safe_update = |inst, value| {
            is_heap_pointer(value) || {
                let facts = guard_facts.get_or_init(|| HeapWriteProof::new(func));
                Self::guarded_heap_pointer(
                    func,
                    &facts.cfg,
                    facts.inst_blocks[&inst],
                    value,
                    &is_heap_pointer,
                )
            }
        };
        func.instructions()
            .filter(|&inst_id| !func.inst(inst_id).metadata.preserves_valid_fmp())
            .filter(|&inst_id| match func.inst(inst_id).kind {
                InstKind::ICall { function: Callee::Function(callee), .. } => {
                    !preserves_fmp.contains(callee)
                }
                InstKind::ICall { .. } => true,
                InstKind::SetFmp(value) => !is_safe_update(inst_id, value),
                InstKind::MStore(address, value)
                    if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                {
                    !is_safe_update(inst_id, value)
                }
                InstKind::MStore(dest, _)
                | InstKind::MStore8(dest, _)
                | InstKind::MCopy(dest, _, _)
                | InstKind::CalldataCopy(dest, _, _)
                | InstKind::DataCopy(_, dest, _)
                | InstKind::CodeCopy(dest, _, _)
                | InstKind::ReturnDataCopy(dest, _, _)
                | InstKind::ExtCodeCopy(_, dest, _, _)
                | InstKind::Call { ret_offset: dest, .. }
                | InstKind::CallCode { ret_offset: dest, .. }
                | InstKind::StaticCall { ret_offset: dest, .. }
                | InstKind::DelegateCall { ret_offset: dest, .. }
                    if is_heap_pointer(dest)
                        || guard_facts
                            .get_or_init(|| HeapWriteProof::new(func))
                            .heap_destination(inst_id, dest, &is_heap_pointer, 0) =>
                {
                    false
                }
                _ => aa.instruction_may_reset_fmp(func, inst_id),
            })
            .collect()
    }

    /// Recognizes allocator updates guarded against wraparound and addresses above 64 bits.
    fn guarded_heap_pointer(
        func: &Function,
        cfg: &CfgInfo,
        write_block: BlockId,
        value: ValueId,
        is_heap_pointer: &impl Fn(ValueId) -> bool,
    ) -> bool {
        let mut lower = false;
        let mut upper = false;
        for (block_id, block) in func.blocks.iter_enumerated() {
            let Some(Terminator::Branch { condition, then_block, else_block }) = block.terminator
            else {
                continue;
            };
            if then_block == else_block {
                continue;
            }
            for (successor, truth) in [(then_block, true), (else_block, false)] {
                if func.blocks[successor].predecessors.as_slice() != [block_id]
                    || !cfg.dominators().dominates(successor, write_block)
                {
                    continue;
                }
                let mut conditions = vec![(condition, truth)];
                let mut seen = FxHashSet::default();
                while let Some((condition, truth)) = conditions.pop() {
                    if !seen.insert((condition, truth)) {
                        continue;
                    }
                    let Value::Inst(inst) = func.value(condition) else { continue };
                    match func.inst(*inst).kind {
                        InstKind::Zext(inner) => conditions.push((inner, truth)),
                        InstKind::Ne(inner, zero) if func.value_u64(zero) == Some(0) => {
                            conditions.push((inner, truth));
                        }
                        InstKind::Ne(zero, inner) if func.value_u64(zero) == Some(0) => {
                            conditions.push((inner, truth));
                        }
                        InstKind::Eq(inner, zero) if func.value_u64(zero) == Some(0) => {
                            conditions.push((inner, !truth));
                        }
                        InstKind::Eq(zero, inner) if func.value_u64(zero) == Some(0) => {
                            conditions.push((inner, !truth));
                        }
                        InstKind::Or(a, b) if !truth => {
                            conditions.extend([(a, false), (b, false)]);
                        }
                        InstKind::And(a, b) if truth => {
                            conditions.extend([(a, true), (b, true)]);
                        }
                        InstKind::Lt(a, b) if !truth => {
                            lower |= a == value && is_heap_pointer(b);
                            upper |= b == value && func.value_u64(a).is_some();
                        }
                        InstKind::Gt(a, b) if !truth => {
                            lower |= b == value && is_heap_pointer(a);
                            upper |= a == value && func.value_u64(b).is_some();
                        }
                        InstKind::Shr(bits, a) if !truth && a == value => {
                            upper |= func.value_u64(bits).is_some_and(|bits| bits <= 64);
                        }
                        _ => {}
                    }
                }
            }
        }
        lower && upper
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
            false,
            visiting,
            memo,
        )
    }

    /// Context-free return summaries require bounded offsets. Local write analysis also
    /// accepts the memory-reference contracts supplied by typed arguments.
    fn heap_pointer_provenance_with_helpers(
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        helper_returns: &DenseBitSet<FunctionId>,
        bounded: bool,
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
            func.value_u256(value).is_some_and(|mask| {
                mask == U256::MAX - U256::from(31)
                    || (!bounded && mask == U256::from(u64::MAX.saturating_sub(31)))
            })
        };
        let derive = |value, visiting: &mut DenseBitSet<ValueId>, memo: &mut FxHashMap<_, _>| {
            Self::heap_pointer_provenance_with_helpers(
                func,
                aa,
                value,
                helper_returns,
                bounded,
                visiting,
                memo,
            )
        };

        let provenance = aa
            .memory_address(func, value)
            .filter(|address| matches!(address.region, MemoryRegion::Heap))
            .filter(|_| !bounded || !AliasAnalysis::range_may_overlap_fmp(func, value, None))
            .map(|_| true)
            .or_else(|| {
                if !bounded
                    && matches!(func.value(value), Value::Arg(_))
                    && func.value_ty(value).is_some_and(MirType::is_memory_reference)
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
                    InstKind::ICall { function: Callee::Function(function), .. }
                        if helper_returns.contains(*function) =>
                    {
                        Some(true)
                    }
                    InstKind::Add(first, second) if !bounded => {
                        derive(*first, visiting, memo).or_else(|| derive(*second, visiting, memo))
                    }
                    InstKind::Sub(base, _) if !bounded => derive(*base, visiting, memo),
                    InstKind::Add(first, second) if func.value_u64(*second).is_some() => {
                        derive(*first, visiting, memo)
                    }
                    InstKind::Add(first, second) if func.value_u64(*first).is_some() => {
                        derive(*second, visiting, memo)
                    }
                    InstKind::PtrToInt(base, 256)
                    | InstKind::IntToPtr(base)
                    | InstKind::Bitcast(base) => derive(*base, visiting, memo),
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
            // These semantic memory operations are normally gone by the `lowered` phase. If
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

/// Uses executed memory accesses and dominating bounds to prove heap writes cannot wrap low.
struct HeapWriteProof<'a> {
    func: &'a Function,
    cfg: CfgInfo,
    inst_blocks: FxHashMap<InstId, BlockId>,
}

impl<'a> HeapWriteProof<'a> {
    fn new(func: &'a Function) -> Self {
        Self { func, cfg: CfgInfo::new(func), inst_blocks: func.inst_blocks() }
    }

    fn precedes(&self, first: InstId, second: InstId) -> bool {
        let a = self.inst_blocks[&first];
        let b = self.inst_blocks[&second];
        if a != b {
            return self.cfg.dominators().dominates(a, b);
        }
        let insts = &self.func.blocks[a].instructions;
        insts.iter().position(|&i| i == first) < insts.iter().position(|&i| i == second)
    }

    fn completed_copy(&self, before: InstId, base: ValueId, size: ValueId) -> bool {
        self.func.instructions().any(|inst| {
            matches!(self.func.inst(inst).kind,
                InstKind::MCopy(dest, _, length) | InstKind::CalldataCopy(dest, _, length)
                | InstKind::CodeCopy(dest, _, length) | InstKind::ReturnDataCopy(dest, _, length)
                if dest == base && length == size)
                && self.precedes(inst, before)
        })
    }

    fn upper_bound(&self, before: InstId, value: ValueId, depth: usize) -> Option<U256> {
        if depth > 16 {
            return None;
        }
        if let Some(value) = self.func.value_u256(value) {
            return Some(value);
        }
        // A successful word access expands memory, so its address fits the EVM memory limit.
        if self.func.instructions().any(|inst| {
            matches!(self.func.inst(inst).kind,
                InstKind::MStore(dest, _) | InstKind::MStore8(dest, _) | InstKind::MLoad(dest)
                if dest == value)
                && self.precedes(inst, before)
        }) {
            return Some(U256::from(u64::MAX));
        }
        let block = self.inst_blocks[&before];
        for (id, data) in self.func.blocks.iter_enumerated() {
            if let Some(Terminator::Branch { condition, then_block, else_block }) = data.terminator
                && then_block != else_block
                && self.func.blocks[else_block].predecessors.as_slice() == [id]
                && self.cfg.dominators().dominates(else_block, block)
                && let Value::Inst(inst) = self.func.value(condition)
                && let InstKind::Gt(lhs, rhs) = self.func.inst(*inst).kind
                && lhs == value
                && let Some(bound) = self.func.value_u256(rhs)
            {
                return Some(bound);
            }
        }
        let Value::Inst(inst) = self.func.value(value) else {
            return None;
        };
        match self.func.inst(*inst).kind {
            InstKind::Add(a, b) => {
                for (base, size) in [(a, b), (b, a)] {
                    if self.completed_copy(before, base, size) {
                        return Some(
                            self.upper_bound(before, base, depth + 1)?.max(U256::from(u64::MAX)),
                        );
                    }
                }
                self.upper_bound(before, a, depth + 1)?.checked_add(self.upper_bound(
                    before,
                    b,
                    depth + 1,
                )?)
            }
            InstKind::And(a, b) => match (
                self.upper_bound(before, a, depth + 1),
                self.upper_bound(before, b, depth + 1),
            ) {
                (Some(a), Some(b)) => Some(a.min(b)),
                (a, b) => a.or(b),
            },
            InstKind::Bitcast(value)
            | InstKind::IntToPtr(value)
            | InstKind::PtrToInt(value, 256) => self.upper_bound(before, value, depth + 1),
            _ => None,
        }
    }

    fn heap_destination(
        &self,
        before: InstId,
        dest: ValueId,
        is_heap: &impl Fn(ValueId) -> bool,
        depth: usize,
    ) -> bool {
        if depth > 16 {
            return false;
        }
        if is_heap(dest) {
            return true;
        }
        let Value::Inst(inst) = self.func.value(dest) else {
            return false;
        };
        let InstKind::Add(a, b) = self.func.inst(*inst).kind else {
            return false;
        };
        for (base, offset) in [(a, b), (b, a)] {
            if !self.heap_destination(before, base, is_heap, depth + 1) {
                continue;
            }
            // A completed copy either has length zero or establishes a non-wrapping end.
            if self.completed_copy(before, base, offset)
                || self
                    .upper_bound(before, base, 0)
                    .zip(self.upper_bound(before, offset, 0))
                    .is_some_and(|(base, offset)| base.checked_add(offset).is_some())
                || self.inductive_store(before, base, offset)
            {
                return true;
            }
        }
        false
    }

    fn inductive_store(&self, store: InstId, base: ValueId, offset: ValueId) -> bool {
        let (InstKind::MStore(dest, _) | InstKind::MStore8(dest, _)) = self.func.inst(store).kind
        else {
            return false;
        };
        let Value::Inst(dest) = self.func.value(dest) else {
            return false;
        };
        if !matches!(self.func.inst(*dest).kind, InstKind::Add(a, b) if (a == base && b == offset) || (b == base && a == offset))
        {
            return false;
        }
        let Value::Inst(phi) = self.func.value(offset) else {
            return false;
        };
        let InstKind::Phi(incoming) = &self.func.inst(*phi).kind else {
            return false;
        };
        let header = self.inst_blocks[phi];
        let body = self.inst_blocks[&store];
        let dom = self.cfg.dominators();
        if !dom.dominates(header, body) {
            return false;
        }
        let mut invariant = base;
        if let Value::Inst(inst) = self.func.value(base)
            && self.inst_blocks[inst] == header
            && let InstKind::Phi(incoming) = &self.func.inst(*inst).kind
        {
            let mut external =
                incoming.iter().map(|&(_, value)| value).filter(|&value| value != base);
            let Some(first) = external.next() else {
                return false;
            };
            if !external.all(|value| value == first) {
                return false;
            }
            invariant = first;
        }
        if let Value::Inst(base) = self.func.value(invariant) {
            let block = self.inst_blocks[base];
            if block == header || !dom.dominates(block, header) {
                return false;
            }
        }
        let mut grounded = false;
        for &(pred, value) in incoming {
            if self.func.value_u64(value) == Some(0) {
                grounded = true;
                continue;
            }
            let Value::Inst(step) = self.func.value(value) else {
                return false;
            };
            let InstKind::Add(a, b) = self.func.inst(*step).kind else {
                return false;
            };
            if !((a == offset && self.func.value_u64(b).is_some())
                || (b == offset && self.func.value_u64(a).is_some()))
                || !dom.dominates(body, pred)
            {
                return false;
            }
        }
        // The first store uses the base itself. Every backedge has already executed that
        // store, bounding its address before the next constant increment can take effect.
        grounded
    }
}

//! Memory provenance and writes that can clobber compiler spill slots.

use super::{
    super::{
        AliasAnalysis, ArgIdx, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function, FunctionId,
        FxHashMap, FxHashSet, IndexVec, InstId, InstKind, MemoryBase, MemoryRegion, MirType,
        Module, Terminator, U256, Value, ValueId,
    },
    PtrLocation,
};
use crate::mir::{
    Callee,
    analysis::{direct_callees, is_floor_clamp, pointer_bases},
};

impl<'gcx> EvmCodegen<'gcx> {
    /// Returns the destination of a memory write that can cover a compiler spill slot.
    ///
    /// The spill area's bounds are only fixed at frame layout, after the stackifier decides
    /// which values to keep out of it, so a write is protected unless its range is disjoint from
    /// the area whatever the layout: below the heap start, or a constant range, which layout
    /// keeps the area clear of, except in the constructor, whose layout only keeps it above
    /// `constructor_floor`. A copy of a constant length is as unbounded as one of any other
    /// length when an input picks its destination, so a pass that folds the length changes
    /// nothing. [`Self::write_dest_may_reach_spills`] drops the destinations that provably lie
    /// on the heap or in a frame.
    pub(in crate::backend::evm::codegen) fn spill_write_dest(
        func: &Function,
        inst_id: InstId,
        constructor_floor: Option<u64>,
    ) -> Option<ValueId> {
        if matches!(
            func.inst(inst_id).metadata.memory_region(),
            Some(MemoryRegion::AbiReturn | MemoryRegion::Heap | MemoryRegion::InternalFrame)
        ) {
            return None;
        }
        // `length` is the exact length when constant, and `longest` a bound on it.
        let protected = |dest: ValueId, length: Option<u64>, longest: Option<u64>| {
            if length == Some(0) {
                return None;
            }
            let Some(start) = func.value_u64(dest) else { return Some(dest) };
            // A constant range is a fixed image that layout keeps the runtime spill area clear
            // of, and the constructor's keeps clear of what lies below its floor. One of unknown
            // length reaches every address above its start, so only a range that ends below the
            // heap start is disjoint.
            let end = longest.and_then(|longest| start.checked_add(longest));
            let floor = match (length, constructor_floor) {
                (Some(_), None) => return None,
                (Some(_), Some(floor)) => floor,
                (None, _) => EvmMemoryLayout::HEAP_START,
            };
            (!end.is_some_and(|end| end <= floor)).then_some(dest)
        };
        let symbolic_size = |dest: ValueId, size: ValueId| {
            let longest = Self::value_u64_upper_bound(
                func,
                size,
                &mut DenseBitSet::new_empty(func.num_values()),
            );
            protected(dest, func.value_u64(size), longest)
        };
        match func.inst(inst_id).kind {
            InstKind::Copy(dest, _, size, PtrLocation::Calldata | PtrLocation::Memory)
            | InstKind::DataCopy(_, dest, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ExtCodeCopy(_, dest, _, size)
            | InstKind::MemoryZero(dest, size) => symbolic_size(dest, size),
            InstKind::Copy(dest, offset, size, PtrLocation::Returndata) => {
                // `returndatacopy(_, returndatasize(), n)` is an OOG guard:
                // `n == 0` writes nothing, while every nonzero length is out
                // of bounds and traps before memory is modified.
                let starts_at_end = matches!(
                    func.value(offset),
                    Value::Inst(inst) if matches!(func.inst(*inst).kind, InstKind::ReturnDataSize)
                );
                if starts_at_end { None } else { symbolic_size(dest, size) }
            }
            InstKind::Call { ret_offset: dest, ret_size: size, .. }
            | InstKind::CallCode { ret_offset: dest, ret_size: size, .. }
            | InstKind::StaticCall { ret_offset: dest, ret_size: size, .. }
            | InstKind::DelegateCall { ret_offset: dest, ret_size: size, .. } => {
                symbolic_size(dest, size)
            }
            InstKind::Store(dest, _, PtrLocation::Memory) => protected(dest, Some(32), Some(32)),
            InstKind::Store8(dest, _) => protected(dest, Some(1), Some(1)),
            _ => None,
        }
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

    /// Collects the writes that can overwrite the spill area: stores, zeroing, copies of any
    /// length, and call return buffers whose destination [`Self::write_dest_may_reach_spills`]
    /// cannot place on the heap or in a frame. A module without inline assembly keeps only the
    /// writes to constant destinations, since the compiler places its own writes.
    pub(in crate::backend::evm::codegen) fn compute_spill_hazard_insts(
        &self,
        func_id: FunctionId,
        func: &Function,
        constructor_floor: Option<u64>,
        inline_assembly: bool,
    ) -> FxHashSet<InstId> {
        let mut hazards = FxHashSet::default();
        let candidates = func
            .instructions()
            .filter_map(|inst_id| {
                Self::spill_write_dest(func, inst_id, constructor_floor).map(|dest| (inst_id, dest))
            })
            // Without inline assembly, the compiler places every write it makes at a computed
            // address on the heap or in a frame.
            .filter(|&(_, dest)| inline_assembly || func.value_u64(dest).is_some())
            .collect::<Vec<_>>();
        if candidates.is_empty() {
            return hazards;
        }
        let aa = AliasAnalysis::new(func);
        for (inst_id, dest) in candidates {
            if self.write_dest_may_reach_spills(func_id, func, &aa, dest) {
                hazards.insert(inst_id);
            }
        }
        hazards
    }

    /// Whether a write's destination may overlap the spill area, unless it provably lies on the
    /// heap or in an internal frame: an allocation, a pointer rooted at the free-memory pointer
    /// that only offsets that cannot wrap move, or a frame address. Any other destination may
    /// reach the fixed low-memory spill slots.
    fn write_dest_may_reach_spills(
        &self,
        func_id: FunctionId,
        func: &Function,
        aa: &AliasAnalysis,
        dest: ValueId,
    ) -> bool {
        let Some(address) = aa.memory_address(func, dest) else {
            return true;
        };
        if address.region == MemoryRegion::InternalFrame {
            return false;
        }
        match address.base {
            MemoryBase::Allocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::InternalFrame => false,
            MemoryBase::Absolute => true,
            MemoryBase::Value(value) => {
                let mut visiting = DenseBitSet::new_empty(func.num_values());
                let mut memo = FxHashMap::default();
                self.heap_pointer_provenance(func_id, func, value, &mut visiting, &mut memo)
                    != Some(true)
            }
        }
    }

    /// Finds helpers that return a pointer rooted at the free-memory pointer,
    /// and arguments that every internal call site passes such a pointer. Calls
    /// lose alias provenance in MIR, so remember the narrow interprocedural
    /// facts needed by forwarding-buffer hazard analysis.
    ///
    /// Both sets start from every candidate and shrink until no return or call
    /// site refutes them, since each can depend on the other: a helper may
    /// return its argument, and a call site may pass a helper's result.
    /// Lowering erases pointer types and inline assembly can retag any word as
    /// a memory reference, so an argument is trusted only through its callers.
    pub(in crate::backend::evm::codegen) fn collect_heap_pointer_facts(
        module: &Module,
    ) -> (DenseBitSet<FunctionId>, IndexVec<FunctionId, DenseBitSet<ArgIdx>>) {
        // A function that returns on no path, as a revert helper does, hands no result back, nor
        // does one that only tail-calls such functions.
        let mut never_returns = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            if !func
                .blocks
                .iter()
                .any(|block| matches!(block.terminator, Some(Terminator::Return { .. })))
            {
                never_returns.insert(func_id);
            }
        }
        loop {
            let mut changed = false;
            for (func_id, func) in module.functions.iter_enumerated() {
                if never_returns.contains(func_id)
                    && func.blocks.iter().any(|block| matches!(block.terminator, Some(Terminator::TailCall { function, .. }) if !never_returns.contains(function)))
                {
                    never_returns.remove(func_id);
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }
        let mut helpers = DenseBitSet::new_empty(module.functions.len());
        for (func_id, func) in module.functions.iter_enumerated() {
            let mut returns = func.blocks.iter().filter_map(|block| match &block.terminator {
                Some(Terminator::Return { values }) => Some(values.len()),
                _ => None,
            });
            // A tail call hands the result to a function the candidate does not vouch for.
            let tails = func.blocks.iter().any(|block| {
                matches!(block.terminator, Some(Terminator::TailCall { function, .. }) if !never_returns.contains(function))
            });
            if !tails && returns.next() == Some(1) && returns.all(|len| len == 1) {
                helpers.insert(func_id);
            }
        }
        // Every parameter of a function some call reaches starts as a candidate: lowering erases
        // pointer types, and a call that passes anything else refutes it.
        let mut called = DenseBitSet::new_empty(module.functions.len());
        for callee in module.functions.iter().flat_map(direct_callees) {
            called.insert(callee);
        }
        let mut args = module
            .functions
            .iter_enumerated()
            .map(|(func_id, func)| {
                let mut args = DenseBitSet::new_empty(func.arg_indices().count());
                if called.contains(func_id) && !Self::is_external_entry(func) {
                    for arg in func.arg_indices() {
                        args.insert(arg);
                    }
                }
                args
            })
            .collect::<IndexVec<FunctionId, _>>();

        loop {
            let mut refuted_helpers = Vec::new();
            let mut refuted_args = Vec::new();
            for (func_id, func) in module.functions.iter_enumerated() {
                let is_heap_pointer = |value| {
                    let mut visiting = DenseBitSet::new_empty(func.num_values());
                    let mut memo = FxHashMap::default();
                    Self::heap_pointer_provenance_with_helpers(
                        func,
                        value,
                        &helpers,
                        &args[func_id],
                        // A step back that a helper or a call chain repeats has no bound.
                        0,
                        &mut visiting,
                        &mut memo,
                    ) == Some(true)
                };
                if helpers.contains(func_id)
                    && !func.blocks.iter().all(|block| match &block.terminator {
                        Some(Terminator::Return { values }) => is_heap_pointer(values[0]),
                        _ => true,
                    })
                {
                    refuted_helpers.push(func_id);
                }
                let calls = func
                    .instructions()
                    .filter_map(|inst_id| match &func.inst(inst_id).kind {
                        InstKind::ICall { function: Callee::Function(function), args } => {
                            Some((*function, &args[..]))
                        }
                        _ => None,
                    })
                    .chain(func.blocks.iter().filter_map(|block| match &block.terminator {
                        Some(Terminator::TailCall { function, args }) => {
                            Some((*function, &args[..]))
                        }
                        _ => None,
                    }));
                for (callee, values) in calls {
                    for (index, &value) in values.iter().enumerate() {
                        let arg = ArgIdx::new(index);
                        if args[callee].contains(arg) && !is_heap_pointer(value) {
                            refuted_args.push((callee, arg));
                        }
                    }
                }
            }
            if refuted_helpers.is_empty() && refuted_args.is_empty() {
                return (helpers, args);
            }
            for func_id in refuted_helpers {
                helpers.remove(func_id);
            }
            for (func_id, arg) in refuted_args {
                args[func_id].remove(arg);
            }
        }
    }

    /// Returns `Some(grounded)` for a heap-pointer derivation. Recursive phi
    /// edges are provisionally valid but ungrounded; every accepted cycle must
    /// also contain a root the match below accepts: the free memory pointer, an allocation, the
    /// heap floor, a qualified helper's result, or a trusted argument.
    ///
    /// A pointer plus an offset stays on the heap only through the operands [`pointer_bases`]
    /// accepts.
    fn heap_pointer_provenance(
        &self,
        func_id: FunctionId,
        func: &Function,
        value: ValueId,
        visiting: &mut DenseBitSet<ValueId>,
        memo: &mut FxHashMap<ValueId, bool>,
    ) -> Option<bool> {
        Self::heap_pointer_provenance_with_helpers(
            func,
            value,
            &self.heap_pointer_return_functions,
            &self.heap_pointer_args[func_id],
            u64::MAX,
            visiting,
            memo,
        )
    }

    fn heap_pointer_provenance_with_helpers(
        func: &Function,
        value: ValueId,
        helper_returns: &DenseBitSet<FunctionId>,
        heap_args: &DenseBitSet<ArgIdx>,
        max_step_back: u64,
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
                    || mask == U256::from(u64::MAX.saturating_sub(31))
            })
        };
        let derive = |value, visiting: &mut DenseBitSet<ValueId>, memo: &mut FxHashMap<_, _>| {
            Self::heap_pointer_provenance_with_helpers(
                func,
                value,
                helper_returns,
                heap_args,
                max_step_back,
                visiting,
                memo,
            )
        };

        let provenance = (|| {
            let inst_id = match func.value(value) {
                Value::Arg(arg) if heap_args.contains(*arg) => return Some(true),
                Value::Inst(inst_id) => inst_id,
                _ => return None,
            };
            match &func.inst(*inst_id).kind {
                InstKind::Fmp | InstKind::Alloc { .. } | InstKind::HeapFloor => Some(true),
                InstKind::Load(address, PtrLocation::Memory)
                    if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
                {
                    Some(true)
                }
                InstKind::ICall { function: Callee::Function(function), .. }
                    if helper_returns.contains(*function) =>
                {
                    Some(true)
                }
                InstKind::Add(..) | InstKind::PtrAdd(..) | InstKind::Sub(..) => {
                    // The heap prefix guard keeps a constant step back from a heap pointer clear
                    // of the spill area.
                    let bases = pointer_bases(func, value, max_step_back);
                    bases.into_iter().find_map(|base| derive(base, visiting, memo))
                }
                InstKind::IntToPtr(base) | InstKind::PtrToInt(base, 256) => {
                    derive(*base, visiting, memo)
                }
                InstKind::And(first, second) if aligned_mask(*second) => {
                    derive(*first, visiting, memo)
                }
                InstKind::And(first, second) if aligned_mask(*first) => {
                    derive(*second, visiting, memo)
                }
                InstKind::Phi(incoming) => {
                    let mut grounded = false;
                    for &(_, incoming) in incoming {
                        grounded |= derive(incoming, visiting, memo)?;
                    }
                    Some(grounded)
                }
                InstKind::Select(cond, then_value, else_value) => {
                    if is_floor_clamp(func, *cond, *then_value, *else_value) {
                        return Some(true);
                    }
                    Some(
                        derive(*then_value, visiting, memo)? | derive(*else_value, visiting, memo)?,
                    )
                }
                InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    aggregate: slice,
                    index: MirType::SLICE_PTR_FIELD,
                } => match func.value(*slice) {
                    Value::Inst(slice) => match &func.inst(*slice).kind {
                        InstKind::MakeSlice { ptr, location: PtrLocation::Memory, .. } => {
                            derive(*ptr, visiting, memo)
                        }
                        InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. } => Some(true),
                        _ => None,
                    },
                    _ => None,
                },
                _ => None,
            }
        })();
        visiting.remove(value);
        if let Some(grounded) = provenance {
            memo.insert(value, grounded);
        }
        provenance
    }

    /// Returns whether a function can read or observe the reserved free-memory-pointer word, or
    /// write part of it. Unknown offsets and lengths are conservatively overlapping; constant
    /// ranges proven disjoint from `[0x40, 0x60)` keep the lazy entry initialization optimization.
    /// A full-word store to the slot overwrites the initialization without observing it, and a
    /// `revert` or `return` range in the block of such a store reads the stored word.
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
            InstKind::Store(..) if func.is_fmp_slot_store(inst_id) => false,
            InstKind::Load(offset, PtrLocation::Memory)
            | InstKind::Store(offset, _, PtrLocation::Memory) => {
                overlaps_const(*offset, EvmMemoryLayout::WORD_SIZE)
            }
            InstKind::Store8(offset, _) => overlaps_const(*offset, 1),
            InstKind::MemoryZero(offset, size)
            | InstKind::Keccak256(offset, size)
            | InstKind::Copy(offset, _, size, PtrLocation::Calldata | PtrLocation::Returndata)
            | InstKind::DataCopy(_, offset, size)
            | InstKind::CodeCopy(offset, _, size)
            | InstKind::ExtCodeCopy(_, offset, _, size) => overlaps(*offset, *size),
            InstKind::Copy(dest, src, size, PtrLocation::Memory) => {
                overlaps(*dest, *size) || overlaps(*src, *size)
            }
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                overlaps(*args_offset, *args_size) || overlaps(*ret_offset, *ret_size)
            }
            InstKind::Create(_, offset, size) | InstKind::Create2(_, offset, size, _) => {
                overlaps(*offset, *size)
            }
            InstKind::Log { offset, size, .. } => overlaps(*offset, *size),
            InstKind::Fmp | InstKind::SetFmp(_) | InstKind::Alloc { .. } => true,
            // These semantic memory operations are normally gone by the `lowered` phase. If
            // one remains, its complete accessed range is not represented as physical operands
            // here, so retain the Solidity memory invariant conservatively.
            InstKind::AbiEncode { .. } | InstKind::MappingSlotBytes(_, _) => true,
            _ => false,
        }) {
            return true;
        }

        func.blocks.iter().any(|block| match block.terminator.as_ref() {
            Some(Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size }) => {
                overlaps(*offset, *size) && !func.block_stores_fmp_slot(block)
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

//! Memory provenance and writes that can clobber compiler spill slots.

use super::{
    super::{
        AliasAnalysis, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function, FunctionId, FxHashMap,
        FxHashSet, InstId, InstKind, MemoryBase, MemoryRegion, MirType, Module, Terminator, U256,
        Value, ValueId,
    },
    SPILL_HAZARD_BOUND,
};
use crate::mir::Callee;

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
            visiting,
            memo,
        )
    }

    fn heap_pointer_provenance_with_helpers(
        func: &Function,
        aa: &AliasAnalysis,
        value: ValueId,
        helper_returns: &DenseBitSet<FunctionId>,
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
                aa,
                value,
                helper_returns,
                visiting,
                memo,
            )
        };

        let provenance = aa
            .memory_address(func, value)
            .and_then(|address| matches!(address.region, MemoryRegion::Heap).then_some(true))
            .or_else(|| {
                if matches!(func.value(value), Value::Arg(_))
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

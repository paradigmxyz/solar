//! Memory provenance and writes that can clobber compiler spill slots.

use super::{
    super::{
        AliasAnalysis, ArtifactKind, DenseBitSet, EvmCodegen, EvmMemoryLayout, Function,
        FunctionId, FxHashMap, FxHashSet, InstId, InstKind, MemoryBase, MemoryRegion, MirType,
        Module, Terminator, U256, Value, ValueId, spill_base::WriteSize,
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
        let (dest, size) = Self::memory_write(func, inst_id)?;
        match (&func.inst(inst_id).kind, size) {
            // A fixed-width store through a loop-carried pointer that starts below the spill
            // area sweeps every slot the loop reaches; across the iterations it is as unbounded
            // as a variable-length copy.
            (InstKind::MStore(..) | InstKind::MStore8(..), _) => {
                Self::is_low_sweeping_pointer(func, dest).then_some(dest)
            }
            // Fixed-width copies have the same explicit-memory contract as `mstore`: arbitrary
            // destinations in hand-written assembly may alias compiler memory. The
            // forwarding-buffer protocol is for variable-length writes that can sweep over every
            // spill slot.
            (_, WriteSize::Const(_)) => None,
            (_, WriteSize::Value(size)) => {
                let below_spills = func.value_u64(dest).is_some_and(|dest| {
                    Self::value_upper_bound(func, size)
                        .or_else(|| Self::guarded_upper_bound(func, inst_id, size))
                        .and_then(|size| dest.checked_add(size))
                        .is_some_and(|end| end <= EvmMemoryLayout::HEAP_START)
                });
                (!below_spills).then_some(dest)
            }
        }
    }

    /// Returns the bound on `value` set by the switch case or equality branch that is the only
    /// way into the block of `inst`, through a chain of single-predecessor blocks. For example,
    /// solmate copies `returndatasize()` bytes only under `case 32`.
    fn guarded_upper_bound(func: &Function, inst: InstId, value: ValueId) -> Option<u64> {
        let (mut block, _) =
            func.blocks.iter_enumerated().find(|(_, block)| block.instructions.contains(&inst))?;
        let mut visited = DenseBitSet::new_empty(func.blocks.len());
        while visited.insert(block) {
            let &[pred] = func.blocks[block].predecessors.as_slice() else { return None };
            match func.blocks[pred].terminator.as_ref()? {
                Terminator::Switch { value: scrutinee, default, cases }
                    if *scrutinee == value && *default != block =>
                {
                    return cases
                        .iter()
                        .filter(|&&(_, target)| target == block)
                        .map(|&(case, _)| func.value_u64(case))
                        .try_fold(0, |bound, case| Some(bound.max(case?)));
                }
                Terminator::Branch { condition, then_block, else_block }
                    if *then_block == block && *else_block != block =>
                {
                    if let Value::Inst(def) = func.value(*condition)
                        && let InstKind::Eq(left, right) = func.inst(*def).kind
                        && let Some(other) = (left == value)
                            .then_some(right)
                            .or_else(|| (right == value).then_some(left))
                        && let Some(bound) = func.value_u64(other)
                    {
                        return Some(bound);
                    }
                }
                _ => {}
            }
            block = pred;
        }
        None
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
    pub(in crate::backend::evm::codegen) fn value_upper_bound(
        func: &Function,
        value: ValueId,
    ) -> Option<u64> {
        let mut visiting = DenseBitSet::new_empty(func.num_values());
        Self::value_u64_upper_bound(func, value, &mut visiting)
    }

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
                InstKind::Add(left, right) => Self::value_u64_upper_bound(func, left, visiting)?
                    .checked_add(Self::value_u64_upper_bound(func, right, visiting)?),
                InstKind::And(left, right) => {
                    match (
                        Self::value_u64_upper_bound(func, left, visiting),
                        Self::value_u64_upper_bound(func, right, visiting),
                    ) {
                        (Some(left), Some(right)) => Some(left.min(right)),
                        (bound, None) | (None, bound) => bound,
                    }
                }
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
    /// A call to a function that may make such a write counts as one too.
    pub(in crate::backend::evm::codegen) fn compute_spill_hazard_insts(
        &self,
        func: &Function,
    ) -> FxHashSet<InstId> {
        let mut hazards = self.direct_spill_hazard_insts(func, &AliasAnalysis::new(func));
        // Creation code runs with empty calldata, so a copy of `calldatasize()` bytes writes
        // nothing.
        if self.asm.artifact_kind == ArtifactKind::Constructor {
            hazards.retain(|&inst| {
                !matches!(Self::memory_write(func, inst), Some((_, WriteSize::Value(size)))
                    if matches!(func.value(size), Value::Inst(def)
                        if matches!(func.inst(*def).kind, InstKind::CalldataSize)))
            });
        }
        if !self.spill_hazard_functions.is_empty() {
            hazards.extend(func.instructions().filter(|&inst_id| {
                matches!(func.inst(inst_id).kind, InstKind::ICall {
                    function: Callee::Function(callee), ..
                } if callee.index() < self.spill_hazard_functions.domain_size()
                    && self.spill_hazard_functions.contains(callee))
            }));
        }
        hazards
    }

    /// Collects the writes of `func` itself that [`Self::compute_spill_hazard_insts`] reports.
    pub(in crate::backend::evm::codegen) fn direct_spill_hazard_insts(
        &self,
        func: &Function,
        aa: &AliasAnalysis,
    ) -> FxHashSet<InstId> {
        func.instructions()
            .filter(|&inst_id| {
                Self::dynamic_spill_write_dest(func, inst_id)
                    .is_some_and(|dest| self.write_dest_may_reach_spills(func, aa, dest))
            })
            .collect()
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
    pub(in crate::backend::evm::codegen) fn heap_pointer_provenance(
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
                    InstKind::Sub(base, _)
                    | InstKind::IntToPtr(base)
                    | InstKind::PtrToInt(base, 256)
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

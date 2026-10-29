//! Removal of writes into allocations that nothing reads, for `memory-dse`.
//!
//! An allocation whose contents are never observed is dead no matter what fills it: the stores and
//! copies into it produce no effect. The allocation stays, since its free-memory-pointer bump and
//! failure behavior are observable on their own. Such buffers appear after other passes strip
//! their readers: a scalar-replaced or decoded struct, an inlined helper whose result is dropped, a
//! re-encoded argument that is never used.
//!
//! One walk finds the writes. It collects every value derived from the allocation (`ptradd`, `add`,
//! `sub`, casts, slice pointers) and gives up on the first use that may read the allocation: a
//! load, a hash, a copy source, a slice length, an escape through a store or terminator, or any
//! other instruction taking a derived value. What remains are the stores, fills, and copies whose
//! destination is derived. A write that can fail on its own, such as `returndatacopy` past the end
//! of the return data, always stays.
//!
//! Alias analysis then proves the walk's result safe to remove. The pointer must not escape, no
//! instruction or terminator may read an address that may alias the allocation's extent, and
//! every write must lie inside its constant-size extent. An indexed element store also qualifies
//! when its sole incoming edge proves an unsigned index bound whose last word fits. Other dynamic
//! addresses stay conservative. An allocation that runs once reads the free-memory pointer before
//! its object exists, so that read does not count; one in a loop reads it again after the previous
//! instance's writes and keeps them. Every other read of the slot still counts. Callers' later
//! reads of the slot cannot see the removed writes: allocations start at or above the heap floor
//! (see `heap-floor`), above the slot. Unlike the dead-allocation-store rule, which removes single
//! constant-offset stores by reachability, this one removes every write of an allocation that is
//! never read, including copies, fills, and bounded indexed stores.
//!
//! Only `memory-dse-unread` runs this rule; the other `memory-dse` runs skip its whole-function
//! scans. It runs when the cheaper rewrites settle, and the fixpoint then continues, so removing a
//! copy that read one allocation lets the next round remove the writes into that allocation.

use super::{MemoryStoreEliminator, remove_dead};
use crate::mir::{
    BlockId, Function, InstId, InstKind, MirType, PtrLocation, RawMemoryAccess, Terminator, Value,
    ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, Location, LocationSize, MemoryAddress, MemoryBase,
        MemoryLocation,
    },
    memory::EvmMemoryLayout,
    utils::IndexLists,
};
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec};
use std::cell::OnceCell;

impl MemoryStoreEliminator {
    /// Removes the writes into allocations that nothing reads and returns how many.
    pub(super) fn remove_write_only_allocation_writes(&mut self, func: &mut Function) -> usize {
        let allocations: Vec<_> = func
            .instructions()
            .filter_map(|inst| match func.inst(inst).kind {
                InstKind::Alloc { size, .. } => Some((inst, size, func.inst_result_value(inst)?)),
                _ => None,
            })
            .collect();
        if allocations.is_empty() {
            return 0;
        }

        let uses = WriteOnlyUses::new(func);
        let mut dead = DenseBitSet::new_empty(func.num_insts());
        for (alloc, size, object) in allocations {
            if let Some(writes) = uses.write_only_writes(func, object)
                && let Some(address) = self.alias.memory_address(func, object)
                && uses.writes_fit_allocation(func, &self.alias, object, address, size, &writes)
                && !self.alias.value_escapes(func, object)
                && !self.allocation_may_be_read(
                    func,
                    alloc,
                    // The allocation's own extent is in bounds, including when its size is
                    // dynamic.
                    MemoryLocation::new(address, self.alias.location_size(func, size)),
                )
            {
                for write in writes {
                    dead.insert(write);
                }
            }
        }

        remove_dead(func, &dead)
    }

    fn allocation_may_be_read(
        &self,
        func: &Function,
        alloc_inst: InstId,
        allocation: MemoryLocation,
    ) -> bool {
        let alias = &self.alias;
        // An allocation that runs once reads the free-memory pointer before its object exists.
        // A loop allocation's next run may read a previous instance, so it keeps the read.
        let own_fmp_read = allocation.address.base == MemoryBase::Allocation(alloc_inst);
        let reads_allocation = |access: Access| match access {
            Access::Any(AddressSpace::Memory) => true,
            // A runtime-indexed access wraps, and so may land in any object.
            Access::Location(Location::Memory(location))
                if location.size == LocationSize::Unknown =>
            {
                true
            }
            Access::Location(Location::Memory(location)) => {
                alias.memory_alias(func, allocation, location).may_alias()
            }
            _ => false,
        };
        func.instructions().any(|inst| {
            !(own_fmp_read && inst == alloc_inst)
                && alias
                    .instruction_mod_ref(func, inst)
                    .reads()
                    .iter()
                    .copied()
                    .any(reads_allocation)
        }) || func.blocks.iter().filter_map(|block| block.terminator.as_ref()).any(|terminator| {
            alias.terminator_mod_ref(func, terminator).reads().iter().copied().any(reads_allocation)
        })
    }
}

/// Operand users of one function, shared by the proofs of all its allocations.
struct WriteOnlyUses {
    /// Instruction users grouped by operand value.
    users: IndexLists<ValueId, InstId>,
    /// Values used by terminators.
    terminator_uses: DenseBitSet<ValueId>,
    /// The block of each instruction, built when a guarded element store needs it.
    inst_blocks: OnceCell<IndexVec<InstId, Option<BlockId>>>,
}

impl WriteOnlyUses {
    fn new(func: &Function) -> Self {
        let mut pairs = Vec::new();
        let mut terminator_uses = DenseBitSet::new_empty(func.num_values());
        for block in &func.blocks {
            for &inst in &block.instructions {
                func.inst(inst).visit_operands(|operand| pairs.push((operand, inst)));
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    terminator_uses.insert(operand);
                }
            }
        }
        Self {
            users: IndexLists::new(func.num_values(), pairs.iter().copied()),
            terminator_uses,
            inst_blocks: OnceCell::new(),
        }
    }

    /// If every access to the allocation writes it, returns the write
    /// instructions to remove; returns `None` if the allocation is read.
    fn write_only_writes(&self, func: &Function, object: ValueId) -> Option<Vec<InstId>> {
        // Address values derived from the allocation. Follow only indexed uses
        // instead of rescanning every instruction for each derived value.
        let mut derived = DenseBitSet::new_empty(func.num_values());
        derived.insert(object);
        let mut worklist = vec![object];
        let mut seen = DenseBitSet::new_empty(func.num_insts());
        while let Some(value) = worklist.pop() {
            for &inst_id in self.users.get(value) {
                if !seen.insert(inst_id) {
                    continue;
                }
                let kind = &func.inst(inst_id).kind;
                let propagates = match kind {
                    InstKind::Add(first, second)
                    | InstKind::Sub(first, second)
                    | InstKind::PtrAdd(first, second) => {
                        derived.contains(*first) || derived.contains(*second)
                    }
                    InstKind::PtrToInt(value, 256)
                    | InstKind::IntToPtr(value)
                    | InstKind::ExtractValue {
                        ty: MirType::Slice(_),
                        aggregate: value,
                        index: MirType::SLICE_PTR_FIELD,
                    }
                    | InstKind::MakeSlice { ptr: value, .. } => derived.contains(*value),
                    _ => false,
                };
                if propagates
                    && let Some(result) = func.inst_result_value(inst_id)
                    && derived.insert(result)
                {
                    worklist.push(result);
                }
            }
        }

        let mut writes = Vec::new();
        seen.clear();
        for value in derived.iter() {
            for &inst_id in self.users.get(value) {
                if !seen.insert(inst_id) {
                    continue;
                }
                let inst = func.inst(inst_id);
                match &inst.kind {
                    // Storing an interior address elsewhere is a read/escape.
                    InstKind::Store(_, value, PtrLocation::Memory) | InstKind::Store8(_, value)
                        if derived.contains(*value) =>
                    {
                        return None;
                    }
                    InstKind::ExtractValue {
                        ty: MirType::Slice(_),
                        aggregate: addr,
                        index: MirType::SLICE_LEN_FIELD,
                    } => {
                        if derived.contains(*addr) {
                            return None;
                        }
                    }
                    // Address-derivation instructions are the closure itself.
                    InstKind::Add(..)
                    | InstKind::Sub(..)
                    | InstKind::PtrAdd(..)
                    | InstKind::PtrToInt(_, 256)
                    | InstKind::IntToPtr(_)
                    | InstKind::ExtractValue {
                        ty: MirType::Slice(_),
                        index: MirType::SLICE_PTR_FIELD,
                        ..
                    }
                    | InstKind::MakeSlice { .. }
                    | InstKind::Alloc { .. } => {}
                    // Any other use of a derived address is treated as a read.
                    kind => {
                        // Memory accesses are judged by their raw ranges. Calls, creates, logs,
                        // and operations without a range read every derived operand.
                        let mut ranged = false;
                        let mut read = false;
                        if kind.effect_kind().is_memory() {
                            kind.visit_raw_memory(|access, addr, _| {
                                ranged = true;
                                if derived.contains(addr) {
                                    match access {
                                        RawMemoryAccess::Write => writes.push(inst_id),
                                        RawMemoryAccess::Read | RawMemoryAccess::PartialWrite => {
                                            read = true;
                                        }
                                    }
                                }
                            });
                        }
                        if read || !ranged && kind.operands().iter().any(|&op| derived.contains(op))
                        {
                            return None;
                        }
                    }
                }
            }
        }

        // Terminators never read a non-escaping allocation (that would escape),
        // but guard defensively.
        if derived.iter().any(|value| self.terminator_uses.contains(value)) {
            return None;
        }

        // A write that can fail on its own, such as `returndatacopy` past the end of the return
        // data, stays even though its destination is unread.
        writes.retain(|&inst_id| !func.inst(inst_id).kind.effects().control.any());
        (!writes.is_empty()).then_some(writes)
    }

    fn writes_fit_allocation(
        &self,
        func: &Function,
        alias: &AliasAnalysis,
        object: ValueId,
        address: MemoryAddress,
        size: ValueId,
        writes: &[InstId],
    ) -> bool {
        if let Some(size) = func.value_u64(size)
            && let Some(end) = address.offset.checked_add(size)
        {
            writes.iter().all(|&inst| {
                if self.guarded_element_end(func, object, inst).is_some_and(|end| end <= size) {
                    return true;
                }
                alias.instruction_mod_ref(func, inst).writes().iter().all(|access| {
                    if let Access::Location(Location::Memory(location)) = access
                        && location.address.base == address.base
                        && location.address.offset >= address.offset
                        && let Some(size) = location.size.as_const()
                        && let Some(write_end) = location.address.offset.checked_add(size)
                    {
                        write_end <= end
                    } else {
                        false
                    }
                })
            })
        } else {
            false
        }
    }

    fn guarded_element_end(&self, func: &Function, object: ValueId, inst: InstId) -> Option<u64> {
        let InstKind::Store(address, _, PtrLocation::Memory) = func.inst(inst).kind else {
            return None;
        };
        // A dynamic object's elements follow its length word; a fixed one's start at it.
        let (target, data_offset, index) = func.element_address(address)?;
        let block = self.inst_blocks.get_or_init(|| func.inst_block_table())[inst]?;
        if target == object
            && let [pred] = func.blocks[block].predecessors.as_slice()
            && let Some(Terminator::Branch { condition, then_block, else_block }) =
                func.blocks[*pred].terminator
            && then_block == block
            && else_block != block
            && let Value::Inst(condition) = *func.value(condition)
            && let InstKind::Lt(value, bound) = func.inst(condition).kind
            && value == index
        {
            let bound = func.value_u64(bound).filter(|&bound| bound > 0)?;
            data_offset.checked_add(bound.checked_mul(EvmMemoryLayout::WORD_SIZE)?)
        } else {
            None
        }
    }
}

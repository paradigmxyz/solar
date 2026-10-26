//! Elision of copies into write-only memory allocations.
//!
//! An allocation that never escapes and is never read is dead no matter what is
//! written into it: the copies and stores that fill it produce no observable
//! effect. A pointer to an allocation that is only stored into other dead
//! allocations does not escape either, so a decoded struct, the nested structs and
//! `bytes` objects it points to, and the pointers between them die together: the
//! pass finds the largest set of allocations whose every use is a write into the
//! set, a store of a member's pointer into a member, or nothing else. A member
//! that is read, escapes, or has its pointer stored anywhere else keeps itself and
//! every allocation whose pointer it holds. The allocation itself remains because its
//! free-memory-pointer bump and failure behavior are independently observable. This arises after
//! other passes strip the readers of a materialized buffer — a scalar-replaced
//! struct, an inlined helper whose result is discarded, a re-encoded argument
//! that is dropped — leaving a copy whose destination no one observes.
//!
//! Ordinary dead-store elimination keeps such copies because they write
//! memory; proving the destination allocation is unread lets them go. The pass
//! is conservative: any read of the allocation (`mload`, `keccak256`, a copy
//! that reads it, or an escape into a call/return) keeps every write, and a write that can fail
//! on its own, such as `returndatacopy`, always stays. Writes must fit a known
//! allocation extent; writes outside it can affect other objects, even when this object is unread.
//! Indexed element stores also qualify when their sole incoming edge proves an unsigned index
//! bound whose final word fits the allocation. Other dynamic addresses remain conservative.
//! An allocation that runs once reads the free-memory pointer before its object exists, so that
//! read does not count; an allocation in a loop reads it again after the previous instance's
//! writes and keeps them. Every other read of the pointer's slot still counts. Callers' later
//! reads of the slot cannot see the removed writes: allocations start at or above the heap floor
//! (see `heap-floor`), above the slot.
//!
//! Before that, the pass redirects a copy out of a buffer that one calldata copy filled. When
//! `calldatacopy buf, src, m` dominates `mcopy dst, buf + s, n`, the copied range lies inside
//! the filled one, and no instruction on a path between them may write it, the `mcopy` becomes
//! `calldatacopy dst, src + (s - buf offset), n`: calldata never changes, so the bytes are the
//! ones the fill wrote. The buffer is then often unread. Sizes compare when both are constants
//! or when a dominating branch edge proves a size equal to one, such as the `revert` of a
//! length check. A buffer that `returndatacopy` filled does not qualify, because return data
//! changes with every call.

use crate::mir::{
    BlockId, Function, Immediate, InstId, InstKind, Instruction, MirType, Module, PtrLocation,
    Terminator, Value, ValueId,
    analysis::{
        Access, AddressSpace, AliasAnalysis, CfgInfo, Location, LocationSize, MemoryAddress,
        MemoryBase, MemoryLocation,
    },
    memory::EvmMemoryLayout,
    pass::{MirPass, run_function_pass_with_alias_and_cfg},
};
use alloy_primitives::U256;
use solar_data_structures::map::{FxHashMap, FxHashSet};

/// Copy-elision pass over write-only memory allocations.
pub(crate) struct CopyElision;

impl MirPass for CopyElision {
    fn name(&self) -> &'static str {
        "copy-elision"
    }

    fn run_pass(
        &self,
        _gcx: solar_sema::Gcx<'_>,
        module: &mut Module,
        analyses: &mut crate::mir::pass::ModuleAnalyses,
    ) -> bool {
        run_function_pass_with_alias_and_cfg(module, analyses, |func, analyses| {
            CopyElisionCx::default().run(func, analyses.alias(), analyses.cfg())
        })
    }
}

#[derive(Debug, Default)]
struct CopyElisionCx {
    /// Number of write-only allocations eliminated.
    eliminated: usize,
    /// Instruction uses indexed by operand value.
    uses: FxHashMap<ValueId, Vec<InstId>>,
    /// Blocks containing indexed element stores that may need an edge bounds proof.
    indexed_stores: FxHashMap<InstId, BlockId>,
    /// Values used by terminators.
    terminator_uses: FxHashSet<ValueId>,
    /// The block of each instruction.
    inst_positions: FxHashMap<InstId, (BlockId, usize)>,
}

/// How a function uses one allocation, when no use may read it.
#[derive(Debug)]
struct ObjectUses {
    object: ValueId,
    /// Address values derived from the allocation.
    derived: FxHashSet<ValueId>,
    /// Writes into the allocation that may be removed.
    writes: Vec<InstId>,
    /// Addresses the allocation's pointer is stored at.
    stored_at: Vec<ValueId>,
}

/// A copy that fills part of an allocation from calldata.
#[derive(Debug)]
struct CalldataFill {
    block: BlockId,
    index: usize,
    site: InstId,
    offset: u64,
    size: ValueId,
    source: ValueId,
}

/// A fixed-size copy out of an allocation.
#[derive(Debug)]
struct MemoryCopy {
    inst: InstId,
    block: BlockId,
    index: usize,
    site: InstId,
    offset: u64,
    size: ValueId,
    /// The size when it is a known constant.
    constant: Option<u64>,
}

impl CopyElisionCx {
    fn run(&mut self, func: &mut Function, alias: &AliasAnalysis, cfg: &CfgInfo) -> bool {
        let allocs: Vec<ValueId> = func
            .instructions()
            .filter_map(|inst_id| {
                matches!(func.inst(inst_id).kind, InstKind::Alloc { .. })
                    .then(|| func.inst_result_value(inst_id))
                    .flatten()
            })
            .collect();
        if allocs.is_empty() {
            return false;
        }
        let mut changed = self.forward_calldata_copies(func, alias, cfg);
        loop {
            self.uses.clear();
            self.indexed_stores.clear();
            self.terminator_uses.clear();
            self.inst_positions.clear();
            self.index_uses(func);

            let dead = self.dead_writes(func, alias, cfg, &allocs);
            if dead.is_empty() {
                break;
            }
            // stores confined to an unread allocation -> removed
            for block in func.blocks.iter_mut() {
                block.instructions.retain(|inst| !dead.contains(inst));
            }
            changed = true;
        }
        changed
    }

    /// Returns the writes into the largest set of allocations that nothing reads.
    ///
    /// An allocation joins the set when no use may read it and its writes fit its extent. It
    /// stays when its pointer is stored anywhere but into a member, so a member's pointer never
    /// reaches memory that is read.
    fn dead_writes(
        &mut self,
        func: &Function,
        alias: &AliasAnalysis,
        cfg: &CfgInfo,
        allocs: &[ValueId],
    ) -> FxHashSet<InstId> {
        let mut members = Vec::new();
        for &object in allocs {
            let Some(uses) = self.object_uses(func, object) else { continue };
            // A stored pointer escapes unless the set keeps it, which the fixpoint below decides.
            if (uses.stored_at.is_empty() && alias.value_escapes(func, object))
                || self.allocation_may_be_read(func, alias, cfg, object)
                || !self.writes_fit_allocation(func, alias, cfg, object, &uses.writes)
            {
                continue;
            }
            members.push(uses);
        }
        let mut owners = FxHashMap::default();
        for (index, member) in members.iter().enumerate() {
            owners.extend(member.derived.iter().map(|&value| (value, index)));
        }
        // Drop members whose pointer is stored outside the set until none changes.
        let mut dropped = FxHashSet::default();
        while let Some(member) = members.iter().find(|member| {
            !dropped.contains(&member.object)
                && member.stored_at.iter().any(|address| {
                    owners
                        .get(address)
                        .is_none_or(|&owner| dropped.contains(&members[owner].object))
                })
        }) {
            dropped.insert(member.object);
        }
        let mut dead = FxHashSet::default();
        for member in members.iter().filter(|member| !dropped.contains(&member.object)) {
            if !member.writes.is_empty() {
                self.eliminated += 1;
            }
            dead.extend(member.writes.iter().copied());
        }
        dead
    }

    fn writes_fit_allocation(
        &self,
        func: &Function,
        alias: &AliasAnalysis,
        cfg: &CfgInfo,
        object: ValueId,
        writes: &[InstId],
    ) -> bool {
        if let Value::Inst(alloc) = func.value(object)
            && let InstKind::Alloc { size, .. } = func.inst(*alloc).kind
            && let Some(size) = func.value_u64(size)
            && let Some(address) = alias.memory_address(func, object)
            && let Some(end) = address.offset.checked_add(size)
        {
            // A write of unknown extent names the allocation's pointer as a fresh base; with a
            // constant width its range starts at the same offset.
            let is_unbounded_view = |base| base == MemoryBase::FreeMemory(*alloc);
            writes.iter().all(|&inst| {
                if self.guarded_element_end(func, object, inst).is_some_and(|end| end <= size) {
                    return true;
                }
                alias.instruction_mod_ref(func, inst).writes().iter().all(|access| {
                    if let Access::Location(Location::Memory(location)) = access
                        && (location.address.base == address.base
                            || is_unbounded_view(location.address.base))
                        && location.address.offset >= address.offset
                        && let Some(size) = self.constant_size(func, cfg, inst, location.size)
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
        let block = *self.indexed_stores.get(&inst)?;
        let InstKind::Store(address, _, PtrLocation::Memory) = func.inst(inst).kind else {
            return None;
        };
        // A dynamic object's elements follow its length word; a fixed one's start at it.
        let (target, data_offset, index) = func.element_address(address)?;
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

    fn allocation_may_be_read(
        &self,
        func: &Function,
        alias: &AliasAnalysis,
        cfg: &CfgInfo,
        object: ValueId,
    ) -> bool {
        let (allocation, alloc_inst) = if let Value::Inst(inst) = func.value(object)
            && let InstKind::Alloc { size, .. } = func.inst(*inst).kind
            && let Some(address) = alias.memory_address(func, object)
        {
            // The allocation's own extent is in bounds, including when its size is dynamic.
            (MemoryLocation::new(address, alias.location_size(func, size)), *inst)
        } else {
            return true;
        };
        // An allocation that runs once reads the free-memory pointer before its object exists.
        // A loop allocation's next run may read a previous instance, so it keeps the read.
        let own_fmp_read = allocation.address.base == MemoryBase::Allocation(alloc_inst);
        let reads_allocation = |access: Access| match access {
            Access::Any(AddressSpace::Memory) => true,
            // A runtime-indexed access wraps, and so may land in any object.
            Access::Location(Location::Memory(location)) => {
                location.size == LocationSize::Unknown
                    || alias.memory_alias(allocation, location).may_alias()
            }
            _ => false,
        };
        // Every write to a unique allocation follows it, so a read that no path runs after the
        // allocation cannot observe a write.
        let runs_after = |inst: InstId| {
            !own_fmp_read
                || self.runs_after(cfg, alloc_inst, self.inst_positions.get(&inst).copied())
        };
        func.instructions().any(|inst| {
            !(own_fmp_read && inst == alloc_inst)
                && runs_after(inst)
                && alias
                    .instruction_mod_ref(func, inst)
                    .reads()
                    .iter()
                    .copied()
                    .any(reads_allocation)
        }) || func.blocks.iter_enumerated().any(|(block, data)| {
            data.terminator.as_ref().is_some_and(|terminator| {
                (!own_fmp_read
                    || self.runs_after(cfg, alloc_inst, Some((block, data.instructions.len()))))
                    && alias
                        .terminator_mod_ref(func, terminator)
                        .reads()
                        .iter()
                        .copied()
                        .any(reads_allocation)
            })
        })
    }

    /// Returns whether a path can run the instruction at `position` after `alloc`.
    fn runs_after(&self, cfg: &CfgInfo, alloc: InstId, position: Option<(BlockId, usize)>) -> bool {
        let (Some((alloc_block, alloc_index)), Some((block, index))) =
            (self.inst_positions.get(&alloc).copied(), position)
        else {
            return true;
        };
        if block == alloc_block && index > alloc_index {
            return true;
        }
        cfg.transitive_reachability(alloc_block).is_none_or(|row| row.contains(block))
    }

    fn index_uses(&mut self, func: &Function) {
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                self.inst_positions.insert(inst_id, (block_id, index));
                let inst = func.inst(inst_id);
                if let InstKind::Store(address, _, PtrLocation::Memory) = inst.kind
                    && func.element_address(address).is_some()
                {
                    self.indexed_stores.insert(inst_id, block_id);
                }
                inst.visit_operands(|operand| {
                    self.uses.entry(operand).or_default().push(inst_id);
                });
            }
        }
        for block in &func.blocks {
            if let Some(terminator) = &block.terminator {
                self.terminator_uses.extend(terminator.operands());
            }
        }
    }

    /// If every use of the allocation writes it or stores its pointer, returns the write
    /// instructions to remove and the addresses its pointer is stored at; returns `None` if
    /// anything else uses it.
    fn object_uses(&self, func: &Function, object: ValueId) -> Option<ObjectUses> {
        // Address values derived from the allocation. Follow only indexed uses
        // instead of rescanning every instruction for each derived value.
        let mut derived = FxHashSet::default();
        derived.insert(object);
        let mut worklist = Vec::new();
        worklist.push(object);
        let mut seen = FxHashSet::default();
        while let Some(value) = worklist.pop() {
            for &inst_id in self.uses.get(&value).into_iter().flatten() {
                if !seen.insert(inst_id) {
                    continue;
                }
                let kind = &func.inst(inst_id).kind;
                let propagates = match kind {
                    InstKind::Add(first, second)
                    | InstKind::Sub(first, second)
                    | InstKind::PtrAdd(first, second) => {
                        derived.contains(first) || derived.contains(second)
                    }
                    InstKind::PtrToInt(value, 256)
                    | InstKind::IntToPtr(value)
                    | InstKind::ExtractValue {
                        ty: MirType::Slice(_),
                        aggregate: value,
                        index: MirType::SLICE_PTR_FIELD,
                    }
                    | InstKind::MakeSlice { ptr: value, .. } => derived.contains(value),
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
        let mut stored_at = Vec::new();
        let mut seen = FxHashSet::default();
        for value in &derived {
            for &inst_id in self.uses.get(value).into_iter().flatten() {
                if !seen.insert(inst_id) {
                    continue;
                }
                let inst = func.inst(inst_id);
                match &inst.kind {
                    // Writes to the allocation: the address is a derived value.
                    InstKind::Store(addr, value, PtrLocation::Memory) => {
                        // Storing an interior address elsewhere is an escape unless that memory is
                        // dead too.
                        if derived.contains(value) {
                            stored_at.push(*addr);
                        }
                        if derived.contains(addr) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::Store8(addr, value) => {
                        if derived.contains(value) {
                            return None;
                        }
                        if derived.contains(addr) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::MemoryZero(addr, _) => {
                        if derived.contains(addr) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::Copy(dest, _, _, PtrLocation::Calldata | PtrLocation::Returndata)
                    | InstKind::DataCopy(_, dest, _)
                    | InstKind::CodeCopy(dest, _, _) => {
                        if derived.contains(dest) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::ExtCodeCopy(_, dest, _, _) => {
                        if derived.contains(dest) {
                            writes.push(inst_id);
                        }
                    }
                    InstKind::Copy(dest, source, _, PtrLocation::Memory) => {
                        if derived.contains(source) {
                            return None; // Read as a copy source.
                        }
                        if derived.contains(dest) {
                            writes.push(inst_id);
                        }
                    }
                    // Reads of the allocation keep every write.
                    InstKind::Load(addr, PtrLocation::Memory)
                    | InstKind::ExtractValue {
                        ty: MirType::Slice(_),
                        aggregate: addr,
                        index: MirType::SLICE_LEN_FIELD,
                    } => {
                        if derived.contains(addr) {
                            return None;
                        }
                    }
                    InstKind::Keccak256(offset, _) => {
                        if derived.contains(offset) {
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
                        if kind.operands().iter().any(|op| derived.contains(op)) {
                            return None;
                        }
                    }
                }
            }
        }

        // Terminators never read a non-escaping allocation (that would escape),
        // but guard defensively.
        if self.terminator_uses.iter().any(|value| derived.contains(value)) {
            return None;
        }

        // A write that can fail on its own, such as `returndatacopy` past the end of the return
        // data, stays even though its destination is unread.
        writes.retain(|&inst_id| !func.inst(inst_id).kind.effects().control.any());
        Some(ObjectUses { object, derived, writes, stored_at })
    }

    /// Returns the constant width of an access by `inst`, including a size that a dominating
    /// branch proves constant.
    fn constant_size(
        &self,
        func: &Function,
        cfg: &CfgInfo,
        inst: InstId,
        size: LocationSize,
    ) -> Option<u64> {
        match size {
            LocationSize::Const(size) => Some(size),
            LocationSize::Dynamic(size) => {
                Self::known_constant(func, cfg, self.inst_positions.get(&inst)?.0, size)
            }
            LocationSize::Unknown => None,
        }
    }

    /// Returns the constant `value` equals in `block`: an immediate, or the other side of an
    /// equality that the only edge into a dominating block proves.
    fn known_constant(
        func: &Function,
        cfg: &CfgInfo,
        block: BlockId,
        value: ValueId,
    ) -> Option<u64> {
        if let Some(constant) = func.value_u64(value) {
            return Some(constant);
        }
        let dominators = cfg.dominators();
        let mut current = block;
        loop {
            // The edge into `current` holds `value == constant` when it is the equal side of the
            // branch on `eq`/`ne`.
            if let [pred] = cfg.predecessors(current)
                && let Some(Terminator::Branch { condition, then_block, else_block }) =
                    func.blocks[*pred].terminator
                && then_block != else_block
                && let Value::Inst(condition) = *func.value(condition)
                && let Some((first, second)) = match func.inst(condition).kind {
                    InstKind::Eq(first, second) if current == then_block => Some((first, second)),
                    InstKind::Ne(first, second) if current == else_block => Some((first, second)),
                    _ => None,
                }
                && let Some(constant) =
                    [(first, second), (second, first)].into_iter().find_map(|(operand, other)| {
                        (operand == value).then(|| func.value_u64(other))?
                    })
            {
                return Some(constant);
            }
            current = dominators.idom(current).filter(|&idom| idom != current)?;
        }
    }

    /// Redirects `mcopy` out of a calldata-filled buffer to read calldata itself.
    fn forward_calldata_copies(
        &mut self,
        func: &mut Function,
        alias: &AliasAnalysis,
        cfg: &CfgInfo,
    ) -> bool {
        let mut fills = Vec::new();
        let mut copies = Vec::new();
        for (block_id, block) in func.blocks.iter_enumerated() {
            for (index, &inst) in block.instructions.iter().enumerate() {
                match func.inst(inst).kind {
                    InstKind::Copy(dest, source, size, PtrLocation::Calldata) => {
                        if let Some(MemoryAddress {
                            base: MemoryBase::Allocation(site),
                            offset,
                            ..
                        }) = alias.memory_address(func, dest)
                        {
                            fills.push(CalldataFill {
                                block: block_id,
                                index,
                                site,
                                offset,
                                size,
                                source,
                            });
                        }
                    }
                    InstKind::Copy(_, source, size, PtrLocation::Memory) => {
                        if let Some(MemoryAddress {
                            base: MemoryBase::Allocation(site),
                            offset,
                            ..
                        }) = alias.memory_address(func, source)
                        {
                            let constant = Self::known_constant(func, cfg, block_id, size);
                            copies.push(MemoryCopy {
                                inst,
                                block: block_id,
                                index,
                                site,
                                offset,
                                size,
                                constant,
                            });
                        }
                    }
                    _ => {}
                }
            }
        }
        if fills.is_empty() || copies.is_empty() {
            return false;
        }

        let mut rewrites = Vec::new();
        for copy in &copies {
            let InstKind::Copy(_, source, _, _) = func.inst(copy.inst).kind else { continue };
            let Some(address) = alias.memory_address(func, source) else { continue };
            let read = MemoryLocation::new(address, alias.location_size(func, copy.size));
            let fill = fills.iter().find(|fill| {
                fill.site == copy.site
                    && fill.offset <= copy.offset
                    && Self::fits_fill(func, cfg, fill, copy)
                    && Self::dominates(cfg, fill.block, fill.index, copy.block, copy.index)
                    && !Self::written_between(func, alias, cfg, fill, copy, read)
            });
            if let Some(fill) = fill {
                rewrites.push((copy.inst, copy.block, fill.source, copy.offset - fill.offset));
            }
        }

        let changed = !rewrites.is_empty();
        for (inst, block, source, shift) in rewrites {
            let InstKind::Copy(dest, _, size, _) = func.inst(inst).kind else { continue };
            let source = if shift == 0 {
                source
            } else {
                let offset = func.alloc_value(Value::Immediate(Immediate::I256(U256::from(shift))));
                // add src, shift
                let (add, source) = func.alloc_value_inst(
                    Instruction::new(InstKind::Add(source, offset), Some(MirType::I256))
                        .with_debug_info_dropped(),
                );
                let position = func.blocks[block].instructions.iter().position(|&id| id == inst);
                let Some(position) = position else { continue };
                func.blocks[block].instructions.insert(position, add);
                source
            };
            // calldatacopy dest, source, size
            let instruction = func.inst_mut(inst);
            instruction.kind = InstKind::Copy(dest, source, size, PtrLocation::Calldata);
            // The flag says the `mcopy` source and destination do not overlap.
            instruction.metadata.set_disjoint(false);
        }
        changed
    }

    /// Returns whether the copied range lies inside the filled one: both sizes are constants, the
    /// fill's perhaps proven at the copy, and the range ends in the fill; or the copy starts where
    /// the fill does and takes the fill's size value.
    fn fits_fill(func: &Function, cfg: &CfgInfo, fill: &CalldataFill, copy: &MemoryCopy) -> bool {
        match (copy.constant, Self::known_constant(func, cfg, copy.block, fill.size)) {
            (Some(copy_size), Some(fill_size)) => (copy.offset - fill.offset)
                .checked_add(copy_size)
                .is_some_and(|end| end <= fill_size),
            _ => copy.size == fill.size && copy.offset == fill.offset,
        }
    }

    /// Returns whether the first instruction runs before the second on every path.
    fn dominates(
        cfg: &CfgInfo,
        first_block: BlockId,
        first_index: usize,
        second_block: BlockId,
        second_index: usize,
    ) -> bool {
        if first_block == second_block {
            first_index < second_index
        } else {
            cfg.dominators().dominates(first_block, second_block)
        }
    }

    /// Returns whether an instruction on a path from the fill to the copy may write `read`.
    ///
    /// Blocks that lie on such a path count whole, and a block that loops back to itself counts
    /// whole as the copy's block even where the copy comes first.
    fn written_between(
        func: &Function,
        alias: &AliasAnalysis,
        cfg: &CfgInfo,
        fill: &CalldataFill,
        copy: &MemoryCopy,
        read: MemoryLocation,
    ) -> bool {
        let writes = |inst| {
            alias.instruction_mod_ref(func, inst).writes().iter().any(|&access| match access {
                Access::Any(AddressSpace::Memory) => true,
                Access::Location(Location::Memory(write)) => {
                    alias.memory_alias(read, write).may_alias()
                }
                _ => false,
            })
        };
        let instructions = |block: BlockId| func.blocks[block].instructions.iter().copied();
        if fill.block == copy.block {
            return instructions(fill.block)
                .skip(fill.index + 1)
                .take(copy.index - fill.index - 1)
                .any(writes);
        }
        if instructions(fill.block).skip(fill.index + 1).any(writes)
            || instructions(copy.block).take(copy.index).any(writes)
        {
            return true;
        }
        if cfg.cyclic_blocks().contains(copy.block)
            && instructions(copy.block).skip(copy.index + 1).any(writes)
        {
            return true;
        }
        let Some(from_fill) = cfg.transitive_reachability(fill.block) else { return true };
        // Blocks reachable from the fill that reach the copy.
        func.blocks.indices().any(|block| {
            block != fill.block
                && block != copy.block
                && from_fill.contains(block)
                && cfg.transitive_reachability(block).is_none_or(|row| row.contains(copy.block))
                && instructions(block).any(writes)
        })
    }
}

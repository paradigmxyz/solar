//! Shared MIR alias and ModRef analysis.
//!
//! Pointer provenance follows SSA definitions through address arithmetic,
//! slices, selects, and control-flow joins. Fresh allocations retain unique
//! identities, while incompatible incoming paths conservatively join to a
//! symbolic pointer. The analysis also keeps compiler-owned regions disjoint
//! and exposes the memory, storage, and transient-storage effects of each
//! instruction.

use super::MemoryCallSummaries;
use crate::{
    memory::{EvmMemoryLayout, MemoryLayoutPolicy},
    mir::{
        AbiType, BlockId, Function, InstId, InstKind, MemoryObjectKind, MemoryRegion,
        SliceLocation, StorageAlias, Terminator, Value, ValueId,
    },
};
use smallvec::SmallVec;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use std::{cell::RefCell, collections::VecDeque, sync::Arc};

/// An address space tracked by ModRef analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AddressSpace {
    /// EVM linear memory.
    Memory,
    /// Persistent contract storage.
    Storage,
    /// Transaction-scoped transient storage.
    Transient,
}

/// The canonical base of a memory address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MemoryBase {
    /// An absolute EVM memory address.
    Absolute,
    /// The current function's internal-call frame.
    InternalFrame,
    /// One fresh abstract heap allocation.
    Allocation(InstId),
    /// An allocation instruction with multiple dynamic loop instances.
    DynamicAllocation(InstId),
    /// A symbolic MIR value.
    Value(ValueId),
}

/// A canonical memory address represented as a base plus byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MemoryAddress {
    /// Coarse compiler-owned memory region.
    pub region: MemoryRegion,
    /// Canonical address base.
    pub base: MemoryBase,
    /// Constant byte offset from `base`.
    pub offset: u64,
}

impl MemoryAddress {
    /// Creates an absolute memory address.
    #[must_use]
    pub(crate) const fn absolute(offset: u64) -> Self {
        let region = if EvmMemoryLayout::is_reserved(offset) {
            MemoryRegion::Scratch
        } else {
            MemoryRegion::Unknown
        };
        Self { region, base: MemoryBase::Absolute, offset }
    }

    /// Creates an address in the current internal-call frame.
    #[must_use]
    pub(crate) const fn internal_frame(offset: u64) -> Self {
        Self { region: MemoryRegion::InternalFrame, base: MemoryBase::InternalFrame, offset }
    }

    /// Creates a symbolic address.
    #[must_use]
    pub(crate) const fn symbolic(value: ValueId, region: MemoryRegion) -> Self {
        Self { region, base: MemoryBase::Value(value), offset: 0 }
    }

    /// Returns the absolute address, if known.
    #[must_use]
    pub(crate) const fn as_absolute(self) -> Option<u64> {
        match self.base {
            MemoryBase::Absolute => Some(self.offset),
            MemoryBase::InternalFrame
            | MemoryBase::Allocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::Value(_) => None,
        }
    }

    /// Returns the internal-frame byte offset, if known.
    #[must_use]
    pub(crate) const fn as_internal_frame_offset(self) -> Option<u64> {
        match self.base {
            MemoryBase::InternalFrame => Some(self.offset),
            MemoryBase::Absolute
            | MemoryBase::Allocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::Value(_) => None,
        }
    }

    /// Returns this address advanced by `offset`, if it fits.
    #[must_use]
    pub(crate) fn checked_add(self, offset: u64) -> Option<Self> {
        Some(Self { offset: self.offset.checked_add(offset)?, ..self })
    }
}

/// The byte width of a memory location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum LocationSize {
    /// A compile-time byte width.
    Const(u64),
    /// A dynamic width represented by a MIR value.
    Dynamic(ValueId),
    /// An unknown byte width.
    Unknown,
}

impl LocationSize {
    /// Returns the constant byte width, if known.
    #[must_use]
    pub(crate) const fn as_const(self) -> Option<u64> {
        match self {
            Self::Const(size) => Some(size),
            Self::Dynamic(_) | Self::Unknown => None,
        }
    }
}

/// A canonical memory byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct MemoryLocation {
    /// Start address.
    pub address: MemoryAddress,
    /// Range width.
    pub size: LocationSize,
}

impl MemoryLocation {
    /// Creates a memory location.
    #[must_use]
    pub(crate) const fn new(address: MemoryAddress, size: LocationSize) -> Self {
        Self { address, size }
    }
}

/// A location in one of the stateful EVM address spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Location {
    /// Linear-memory byte range.
    Memory(MemoryLocation),
    /// One persistent storage slot.
    Storage(StorageAlias),
    /// One transient storage slot.
    Transient(StorageAlias),
}

impl Location {
    /// Returns this location's address space.
    #[must_use]
    pub(crate) const fn address_space(self) -> AddressSpace {
        match self {
            Self::Memory(_) => AddressSpace::Memory,
            Self::Storage(_) => AddressSpace::Storage,
            Self::Transient(_) => AddressSpace::Transient,
        }
    }
}

/// Alias relationship between two locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
// LLVM's canonical NoAlias/MayAlias/MustAlias naming.
#[allow(clippy::enum_variant_names)]
pub(crate) enum AliasResult {
    /// The locations cannot overlap.
    NoAlias,
    /// The locations may overlap, but the analysis cannot prove how.
    MayAlias,
    /// The locations denote exactly the same range or slot.
    MustAlias,
    /// The locations overlap but are not identical.
    PartialAlias,
}

impl AliasResult {
    /// Returns whether the locations may overlap.
    #[must_use]
    pub(crate) const fn may_alias(self) -> bool {
        !matches!(self, Self::NoAlias)
    }
}

/// One exact or address-space-wide instruction access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Access {
    /// Access to one canonical location.
    Location(Location),
    /// Conservative access to an entire address space.
    Any(AddressSpace),
}

impl Access {
    /// Returns this access's address space.
    #[must_use]
    pub(crate) const fn address_space(self) -> AddressSpace {
        match self {
            Self::Location(location) => location.address_space(),
            Self::Any(space) => space,
        }
    }
}

/// Memory and state accesses performed by one MIR operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModRef {
    reads: SmallVec<[Access; 4]>,
    writes: SmallVec<[Access; 4]>,
    observes_memory_size: bool,
    observes_gas: bool,
}

impl ModRef {
    /// Returns exact and address-space-wide reads.
    #[must_use]
    pub(crate) fn reads(&self) -> &[Access] {
        &self.reads
    }

    /// Returns exact and address-space-wide writes.
    #[must_use]
    pub(crate) fn writes(&self) -> &[Access] {
        &self.writes
    }

    /// Returns whether the operation observes the active memory size.
    #[must_use]
    pub(crate) const fn observes_memory_size(&self) -> bool {
        self.observes_memory_size
    }

    /// Returns whether the operation observes remaining gas.
    #[must_use]
    pub(crate) const fn observes_gas(&self) -> bool {
        self.observes_gas
    }

    /// Returns whether any access reads `space`.
    #[must_use]
    pub(crate) fn reads_space(&self, space: AddressSpace) -> bool {
        self.reads.iter().any(|access| access.address_space() == space)
    }

    /// Returns whether an address-space-wide access reads `space`.
    #[must_use]
    pub(crate) fn reads_anywhere(&self, space: AddressSpace) -> bool {
        self.reads.contains(&Access::Any(space))
    }

    /// Returns whether any access writes `space`.
    #[must_use]
    pub(crate) fn writes_space(&self, space: AddressSpace) -> bool {
        self.writes.iter().any(|access| access.address_space() == space)
    }

    /// Returns whether an address-space-wide access writes `space`.
    #[must_use]
    pub(crate) fn writes_anywhere(&self, space: AddressSpace) -> bool {
        self.writes.contains(&Access::Any(space))
    }

    /// Returns whether this operation may write `location`.
    #[must_use]
    pub(crate) fn may_write(&self, aa: &AliasAnalysis, location: Location) -> bool {
        self.writes.iter().any(|&access| aa.access_may_alias(access, location))
    }

    fn read(&mut self, access: Access) {
        self.reads.push(access);
    }

    fn write(&mut self, access: Access) {
        self.writes.push(access);
    }

    fn read_any(&mut self, space: AddressSpace) {
        self.read(Access::Any(space));
    }

    fn write_any(&mut self, space: AddressSpace) {
        self.write(Access::Any(space));
    }
}

fn add_storage_range(effects: &mut ModRef, base: StorageAlias, slots: u64, write: bool) {
    for offset in 0..slots {
        let offset = alloy_primitives::U256::from_limbs([offset, 0, 0, 0]);
        let access = Access::Location(Location::Storage(base.offset_by(offset)));
        if write {
            effects.write(access);
        } else {
            effects.read(access);
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct AllocationProvenance {
    dynamic: bool,
    unique: bool,
}

#[derive(Debug, Default)]
struct PointerProvenance {
    allocations: FxHashMap<InstId, AllocationProvenance>,
    addresses: RefCell<FxHashMap<ValueId, Option<MemoryAddress>>>,
    visiting: RefCell<FxHashSet<ValueId>>,
}

impl PointerProvenance {
    fn new(
        func: &Function,
        call_summaries: Option<&MemoryCallSummaries>,
        may_reset_fmp: impl Fn(&Function, InstId, Option<&MemoryCallSummaries>) -> bool,
    ) -> Self {
        if func.blocks.is_empty() {
            return Self::default();
        }
        let cyclic = cyclic_blocks(func);
        let block_resets: Vec<_> = func
            .blocks
            .iter()
            .map(|block| {
                block.instructions.iter().any(|&inst| may_reset_fmp(func, inst, call_summaries))
            })
            .collect();

        // `poisoned[block]` means at least one path into the block may have
        // recycled the FMP. The monotone OR lattice handles joins and loops.
        let mut reachable = vec![false; func.blocks.len()];
        let mut poisoned = vec![false; func.blocks.len()];
        let mut worklist = VecDeque::from([func.entry_block]);
        reachable[func.entry_block.index()] = true;
        while let Some(block) = worklist.pop_front() {
            let out = poisoned[block.index()] || block_resets[block.index()];
            let Some(terminator) = &func.blocks[block].terminator else { continue };
            for successor in terminator.successors() {
                let index = successor.index();
                let changed = !reachable[index] || (out && !poisoned[index]);
                reachable[index] = true;
                poisoned[index] |= out;
                if changed {
                    worklist.push_back(successor);
                }
            }
        }

        let mut allocations = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            let mut reset = poisoned[block_id.index()];
            for &inst_id in &block.instructions {
                if matches!(
                    func.instructions[inst_id].kind,
                    InstKind::Alloc { .. } | InstKind::AbiEncode { .. }
                ) {
                    allocations.insert(
                        inst_id,
                        AllocationProvenance {
                            dynamic: cyclic[block_id.index()],
                            unique: reachable[block_id.index()]
                                && !cyclic[block_id.index()]
                                && !reset,
                        },
                    );
                }
                reset |= may_reset_fmp(func, inst_id, call_summaries);
            }
        }

        Self {
            allocations,
            addresses: RefCell::new(FxHashMap::default()),
            visiting: RefCell::new(FxHashSet::default()),
        }
    }
}

/// Shared cached, control-flow-aware provenance, alias, and ModRef analysis.
///
/// One instance is an immutable snapshot of a function. Recompute it after a
/// transform mutates definitions or CFG edges.
#[derive(Debug)]
pub(crate) struct AliasAnalysis {
    provenance: PointerProvenance,
    call_summaries: Option<Arc<MemoryCallSummaries>>,
}

impl AliasAnalysis {
    /// Computes a function-local snapshot with conservative internal calls.
    #[must_use]
    pub(crate) fn new(func: &Function) -> Self {
        Self::with_optional_summaries(func, None)
    }

    /// Computes a snapshot using module-level internal-call summaries.
    #[must_use]
    pub(crate) fn with_call_summaries(
        func: &Function,
        summaries: Arc<MemoryCallSummaries>,
    ) -> Self {
        Self::with_optional_summaries(func, Some(summaries))
    }

    /// Drops value-address memoization after instruction operands are rewritten.
    pub(crate) fn clear_cached_addresses(&self) {
        self.provenance.addresses.borrow_mut().clear();
        self.provenance.visiting.borrow_mut().clear();
    }

    fn with_optional_summaries(
        func: &Function,
        call_summaries: Option<Arc<MemoryCallSummaries>>,
    ) -> Self {
        let provenance = PointerProvenance::new(
            func,
            call_summaries.as_deref(),
            Self::instruction_may_reset_fmp_with_summaries,
        );
        Self { provenance, call_summaries }
    }

    /// Canonicalizes a MIR value used as a memory address.
    #[must_use]
    pub(crate) fn memory_address(&self, func: &Function, value: ValueId) -> Option<MemoryAddress> {
        self.memory_address_with_depth(func, value, 0)
    }

    /// Creates a memory location, using instruction metadata to refine its region.
    #[must_use]
    pub(crate) fn memory_location(
        &self,
        func: &Function,
        inst_id: InstId,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        let mut address = self.memory_address(func, address)?;
        if let Some(region) = func.instructions[inst_id].metadata.memory_region()
            && region != MemoryRegion::Unknown
        {
            address.region = region;
        }
        Some(MemoryLocation::new(address, size))
    }

    /// Returns the physical word holding a semantic memory object's length.
    #[must_use]
    pub(crate) fn memory_object_length_location(
        &self,
        func: &Function,
        inst_id: InstId,
        object: ValueId,
        kind: MemoryObjectKind,
    ) -> Option<MemoryLocation> {
        let offset = EvmMemoryLayout::object_length_offset(kind)?;
        let mut address = self.memory_address(func, object)?.checked_add(offset)?;
        if let Some(region) = func.instructions[inst_id].metadata.memory_region()
            && region != MemoryRegion::Unknown
        {
            address.region = region;
        }
        Some(MemoryLocation::new(address, LocationSize::Const(EvmMemoryLayout::WORD_SIZE)))
    }

    /// Creates a memory location without instruction metadata.
    #[must_use]
    pub(crate) fn bare_memory_location(
        &self,
        func: &Function,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        Some(MemoryLocation::new(self.memory_address(func, address)?, size))
    }

    /// Converts a MIR size operand to a canonical location size.
    #[must_use]
    pub(crate) fn location_size(&self, func: &Function, value: ValueId) -> LocationSize {
        func.value_u64(value).map_or(LocationSize::Dynamic(value), LocationSize::Const)
    }

    /// Returns the canonical storage alias for an instruction operand.
    #[must_use]
    pub(crate) fn storage_alias(
        &self,
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
    ) -> StorageAlias {
        Self::storage_alias_at(func, inst_id, slot)
    }

    /// Returns a canonical storage alias without pointer-provenance state.
    #[must_use]
    pub(crate) fn storage_alias_at(
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
    ) -> StorageAlias {
        func.storage_alias(inst_id, slot)
    }

    /// Canonicalizes a storage-slot value without pointer-provenance state.
    #[must_use]
    pub(crate) fn storage_alias_for_value_at(func: &Function, slot: ValueId) -> StorageAlias {
        StorageAlias::for_value(func, slot)
    }

    /// Returns the canonical storage alias after value replacements.
    #[must_use]
    pub(crate) fn storage_alias_after_replacements(
        &self,
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> StorageAlias {
        Self::storage_alias_after_replacements_at(func, inst_id, slot, replacements)
    }

    /// Canonicalizes a replaced storage operand without pointer-provenance state.
    #[must_use]
    pub(crate) fn storage_alias_after_replacements_at(
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> StorageAlias {
        func.storage_alias_after_replacements(inst_id, slot, replacements)
    }

    /// Returns whether a pointer-derived value can escape its function.
    ///
    /// Address-only uses are non-capturing. Stores of the pointer value,
    /// returns, and arguments to capturing internal-call parameters escape.
    /// Unsupported uses stay conservative.
    #[must_use]
    pub(crate) fn value_escapes(&self, func: &Function, root: ValueId) -> bool {
        let mut derived = FxHashSet::default();
        derived.insert(root);
        loop {
            let mut changed = false;
            for (value_id, value) in func.values.iter_enumerated() {
                let Value::Inst(inst_id) = value else { continue };
                let propagates = match &func.instructions[*inst_id].kind {
                    InstKind::Add(first, second)
                    | InstKind::Sub(first, second)
                    | InstKind::MakeSlice { ptr: first, len: second, .. } => {
                        derived.contains(first) || derived.contains(second)
                    }
                    InstKind::Phi(incoming) => {
                        incoming.iter().any(|(_, value)| derived.contains(value))
                    }
                    InstKind::Select(_, first, second) => {
                        derived.contains(first) || derived.contains(second)
                    }
                    InstKind::SlicePtr(value)
                    | InstKind::MemoryObjectData(value, _)
                    | InstKind::MemoryObjectFieldAddr { object: value, .. } => {
                        derived.contains(value)
                    }
                    InstKind::MemoryObjectElementAddr { object, .. } => derived.contains(object),
                    _ => false,
                };
                if propagates && derived.insert(value_id) {
                    changed = true;
                }
            }
            if !changed {
                break;
            }
        }

        for block in &func.blocks {
            for &inst_id in &block.instructions {
                let kind = &func.instructions[inst_id].kind;
                for operand in kind.operands() {
                    if derived.contains(&operand) && self.instruction_operand_escapes(kind, operand)
                    {
                        return true;
                    }
                }
            }
            if let Some(terminator) = &block.terminator {
                for operand in terminator.operands() {
                    if !derived.contains(&operand) {
                        continue;
                    }
                    match terminator {
                        Terminator::Revert { offset, .. }
                        | Terminator::ReturnData { offset, .. }
                            if operand == *offset => {}
                        Terminator::TailCall { function, args } => {
                            let summary = self
                                .call_summaries
                                .as_deref()
                                .and_then(|summaries| summaries.get(*function));
                            if summary.is_none_or(|summary| {
                                args.iter().enumerate().any(|(index, &arg)| {
                                    arg == operand && summary.captures_param(index)
                                })
                            }) {
                                return true;
                            }
                        }
                        _ => return true,
                    }
                }
            }
        }
        false
    }

    fn instruction_operand_escapes(&self, kind: &InstKind, operand: ValueId) -> bool {
        match kind {
            InstKind::Add(_, _)
            | InstKind::Sub(_, _)
            | InstKind::Phi(_)
            | InstKind::Select(_, _, _)
            | InstKind::MakeSlice { .. }
            | InstKind::SlicePtr(_)
            | InstKind::MemoryObjectData(_, _)
            | InstKind::MemoryObjectFieldAddr { .. }
            | InstKind::MemoryObjectElementAddr { .. } => false,
            InstKind::MLoad(address)
            | InstKind::MappingSlotMemory(address, _)
            | InstKind::MemoryObjectLen(address, _) => operand != *address,
            InstKind::MStore(address, _)
            | InstKind::MStore8(address, _)
            | InstKind::SetMemoryObjectLen(address, _, _) => operand != *address,
            InstKind::MCopy(dest, source, _)
            | InstKind::StorageToMemory { memory: dest, storage: source, .. } => {
                operand != *dest && operand != *source
            }
            InstKind::MemoryToStorage { memory, .. } => operand != *memory,
            InstKind::CalldataCopy(dest, _, _)
            | InstKind::CodeCopy(dest, _, _)
            | InstKind::ReturnDataCopy(dest, _, _)
            | InstKind::Keccak256(dest, _)
            | InstKind::Log0(dest, _) => operand != *dest,
            InstKind::ExtCodeCopy(_, dest, _, _) => operand != *dest,
            InstKind::Log1(address, _, _)
            | InstKind::Log2(address, _, _, _)
            | InstKind::Log3(address, _, _, _, _)
            | InstKind::Log4(address, _, _, _, _, _) => operand != *address,
            InstKind::Call { args_offset, ret_offset, .. }
            | InstKind::StaticCall { args_offset, ret_offset, .. }
            | InstKind::DelegateCall { args_offset, ret_offset, .. } => {
                operand != *args_offset && operand != *ret_offset
            }
            InstKind::Create(_, offset, _) | InstKind::Create2(_, offset, _, _) => {
                operand != *offset
            }
            InstKind::AbiEncode { args, layout, .. } => args
                .iter()
                .zip(layout.types.iter())
                .filter(|(arg, _)| **arg == operand)
                .any(|(_, ty)| !Self::abi_type_reads_memory(ty)),
            InstKind::InternalCall { function, args, .. } => self
                .call_summaries
                .as_deref()
                .and_then(|summaries| summaries.get(*function))
                .is_none_or(|summary| {
                    args.iter()
                        .enumerate()
                        .any(|(index, &arg)| arg == operand && summary.captures_param(index))
                }),
            _ => true,
        }
    }

    /// Computes the alias relationship between two locations.
    #[must_use]
    pub(crate) fn alias(&self, first: Location, second: Location) -> AliasResult {
        Self::alias_locations(first, second)
    }

    /// Computes a location relationship without function-specific provenance.
    #[must_use]
    /// An allocation-site base as `(site, is_loop_instance)`, or `None` for a
    /// non-allocation base.
    fn allocation_base(base: MemoryBase) -> Option<(InstId, bool)> {
        match base {
            MemoryBase::Allocation(id) => Some((id, false)),
            MemoryBase::DynamicAllocation(id) => Some((id, true)),
            _ => None,
        }
    }

    pub(crate) fn alias_locations(first: Location, second: Location) -> AliasResult {
        match (first, second) {
            (Location::Memory(first), Location::Memory(second)) => {
                Self::memory_alias_locations(first, second)
            }
            (Location::Storage(first), Location::Storage(second))
            | (Location::Transient(first), Location::Transient(second)) => {
                if first == second {
                    AliasResult::MustAlias
                } else if first.may_alias(second) {
                    AliasResult::MayAlias
                } else {
                    AliasResult::NoAlias
                }
            }
            _ => AliasResult::NoAlias,
        }
    }

    /// Computes the ModRef effects of one instruction.
    #[must_use]
    pub(crate) fn instruction_mod_ref(&self, func: &Function, inst_id: InstId) -> ModRef {
        self.instruction_mod_ref_with_replacements(func, inst_id, &FxHashMap::default())
    }

    /// Computes instruction ModRef effects after applying value replacements.
    #[must_use]
    pub(crate) fn instruction_mod_ref_with_replacements(
        &self,
        func: &Function,
        inst_id: InstId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> ModRef {
        let kind = &func.instructions[inst_id].kind;
        let resolve = |value| crate::mir::utils::resolve_replacement(value, replacements);
        let mut effects = ModRef::default();
        let read_memory = |effects: &mut ModRef, address, size| {
            if let Some(location) = self.memory_location(
                func,
                inst_id,
                resolve(address),
                self.resolved_location_size(func, size, replacements),
            ) {
                effects.read(Access::Location(Location::Memory(location)));
            } else {
                effects.read_any(AddressSpace::Memory);
            }
        };
        let write_memory = |effects: &mut ModRef, address, size| {
            if let Some(location) = self.memory_location(
                func,
                inst_id,
                resolve(address),
                self.resolved_location_size(func, size, replacements),
            ) {
                effects.write(Access::Location(Location::Memory(location)));
            } else {
                effects.write_any(AddressSpace::Memory);
            }
        };

        match *kind {
            InstKind::MLoad(address) => {
                read_memory(&mut effects, address, SizeOperand::Const(32));
            }
            InstKind::MStore(address, _) => {
                write_memory(&mut effects, address, SizeOperand::Const(32));
            }
            InstKind::MStore8(address, _) => {
                write_memory(&mut effects, address, SizeOperand::Const(1));
            }
            InstKind::Fmp => {
                effects.read(Access::Location(Location::Memory(Self::fmp_location())));
            }
            InstKind::SetFmp(_) => {
                effects.write(Access::Location(Location::Memory(Self::fmp_location())));
            }
            InstKind::MemoryObjectLen(object, kind) => {
                if let Some(location) =
                    self.memory_object_length_location(func, inst_id, object, kind)
                {
                    effects.read(Access::Location(Location::Memory(location)));
                } else {
                    effects.read_any(AddressSpace::Memory);
                }
            }
            InstKind::SetMemoryObjectLen(object, _, kind) => {
                if let Some(location) =
                    self.memory_object_length_location(func, inst_id, object, kind)
                {
                    effects.write(Access::Location(Location::Memory(location)));
                } else {
                    effects.write_any(AddressSpace::Memory);
                }
            }
            InstKind::Alloc { size, semantics, .. } => {
                let fmp = Access::Location(Location::Memory(Self::fmp_location()));
                effects.read(fmp);
                effects.write(fmp);
                if semantics.initialization == crate::mir::AllocationInitialization::Zeroed
                    && let Some(ptr) = func.inst_result_value(inst_id)
                {
                    let size = match semantics.alignment {
                        crate::mir::AllocationAlignment::Exact => SizeOperand::Value(size),
                        crate::mir::AllocationAlignment::Word => func
                            .value_u64(size)
                            .and_then(EvmMemoryLayout::align_word)
                            .map_or(SizeOperand::Unknown, SizeOperand::Const),
                    };
                    write_memory(&mut effects, ptr, size);
                }
            }
            InstKind::AbiEncode { .. } => {
                let InstKind::AbiEncode { args, layout, .. } = kind else { unreachable!() };
                for (&arg, ty) in args.iter().zip(layout.types.iter()) {
                    if Self::abi_type_reads_memory(ty) {
                        read_memory(&mut effects, arg, SizeOperand::Unknown);
                    }
                }
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::StorageToMemory { .. } => {
                let InstKind::StorageToMemory { storage, memory, layout } = kind else {
                    unreachable!()
                };
                let base =
                    self.storage_alias_after_replacements(func, inst_id, *storage, replacements);
                add_storage_range(&mut effects, base, layout.storage_slots(), false);
                write_memory(&mut effects, *memory, SizeOperand::Const(layout.memory_words() * 32));
                if layout.has_nested_layout() {
                    effects.write_any(AddressSpace::Memory);
                    let fmp = Access::Location(Location::Memory(Self::fmp_location()));
                    effects.read(fmp);
                    effects.write(fmp);
                }
            }
            InstKind::MemoryToStorage { .. } => {
                let InstKind::MemoryToStorage { memory, storage, layout } = kind else {
                    unreachable!()
                };
                if layout.has_nested_layout() {
                    effects.read_any(AddressSpace::Memory);
                } else {
                    read_memory(
                        &mut effects,
                        *memory,
                        SizeOperand::Const(layout.memory_words() * 32),
                    );
                }
                let base =
                    self.storage_alias_after_replacements(func, inst_id, *storage, replacements);
                add_storage_range(&mut effects, base, layout.storage_slots(), true);
            }
            InstKind::ClearStorage { .. } => {
                let InstKind::ClearStorage { storage, layout } = kind else { unreachable!() };
                let base =
                    self.storage_alias_after_replacements(func, inst_id, *storage, replacements);
                add_storage_range(&mut effects, base, layout.storage_slots(), true);
            }
            InstKind::MCopy(dest, source, size) => {
                read_memory(&mut effects, source, SizeOperand::Value(size));
                write_memory(&mut effects, dest, SizeOperand::Value(size));
            }
            InstKind::CalldataCopy(dest, _, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ReturnDataCopy(dest, _, size) => {
                write_memory(&mut effects, dest, SizeOperand::Value(size));
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                write_memory(&mut effects, dest, SizeOperand::Value(size));
            }
            InstKind::Keccak256(address, size) | InstKind::Log0(address, size) => {
                read_memory(&mut effects, address, SizeOperand::Value(size));
            }
            InstKind::Log1(address, size, _)
            | InstKind::Log2(address, size, _, _)
            | InstKind::Log3(address, size, _, _, _)
            | InstKind::Log4(address, size, _, _, _, _) => {
                read_memory(&mut effects, address, SizeOperand::Value(size));
            }
            InstKind::MappingSlotMemory(address, _) => {
                read_memory(&mut effects, address, SizeOperand::Unknown);
            }
            InstKind::MSize => effects.observes_memory_size = true,
            InstKind::Gas => effects.observes_gas = true,
            InstKind::SLoad(slot) => effects.read(Access::Location(Location::Storage(
                self.storage_alias_after_replacements(func, inst_id, slot, replacements),
            ))),
            InstKind::SStore(slot, _) => effects.write(Access::Location(Location::Storage(
                self.storage_alias_after_replacements(func, inst_id, slot, replacements),
            ))),
            InstKind::TLoad(slot) => effects.read(Access::Location(Location::Transient(
                self.storage_alias_after_replacements(func, inst_id, slot, replacements),
            ))),
            InstKind::TStore(slot, _) => effects.write(Access::Location(Location::Transient(
                self.storage_alias_after_replacements(func, inst_id, slot, replacements),
            ))),
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::StaticCall { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::DelegateCall { args_offset, args_size, ret_offset, ret_size, .. } => {
                read_memory(&mut effects, args_offset, SizeOperand::Value(args_size));
                write_memory(&mut effects, ret_offset, SizeOperand::Value(ret_size));
                effects.read_any(AddressSpace::Storage);
                effects.read_any(AddressSpace::Transient);
                if !matches!(kind, InstKind::StaticCall { .. }) {
                    effects.write_any(AddressSpace::Storage);
                    effects.write_any(AddressSpace::Transient);
                }
            }
            InstKind::InternalCall { function, .. } => {
                if let Some(summary) =
                    self.call_summaries.as_deref().and_then(|summaries| summaries.get(function))
                {
                    for space in
                        [AddressSpace::Memory, AddressSpace::Storage, AddressSpace::Transient]
                    {
                        if summary.reads(space) {
                            effects.read_any(space);
                        }
                        if summary.writes(space) {
                            effects.write_any(space);
                        }
                    }
                } else {
                    effects.read_any(AddressSpace::Memory);
                    effects.write_any(AddressSpace::Memory);
                    effects.read_any(AddressSpace::Storage);
                    effects.write_any(AddressSpace::Storage);
                    effects.read_any(AddressSpace::Transient);
                    effects.write_any(AddressSpace::Transient);
                }
            }
            InstKind::Create(_, offset, size) | InstKind::Create2(_, offset, size, _) => {
                read_memory(&mut effects, offset, SizeOperand::Value(size));
                effects.read_any(AddressSpace::Storage);
                effects.write_any(AddressSpace::Storage);
                effects.read_any(AddressSpace::Transient);
                effects.write_any(AddressSpace::Transient);
            }
            _ => {}
        }
        effects
    }

    /// Computes ModRef effects of one terminator.
    #[must_use]
    pub(crate) fn terminator_mod_ref(&self, func: &Function, terminator: &Terminator) -> ModRef {
        let mut effects = ModRef::default();
        match *terminator {
            Terminator::Revert { offset, size } | Terminator::ReturnData { offset, size } => {
                if let Some(location) =
                    self.bare_memory_location(func, offset, self.location_size(func, size))
                {
                    effects.read(Access::Location(Location::Memory(location)));
                } else {
                    effects.read_any(AddressSpace::Memory);
                }
            }
            Terminator::TailCall { function, .. } => {
                if let Some(summary) =
                    self.call_summaries.as_deref().and_then(|summaries| summaries.get(function))
                {
                    for space in
                        [AddressSpace::Memory, AddressSpace::Storage, AddressSpace::Transient]
                    {
                        if summary.reads(space) {
                            effects.read_any(space);
                        }
                        if summary.writes(space) {
                            effects.write_any(space);
                        }
                    }
                } else {
                    effects.read_any(AddressSpace::Memory);
                    effects.write_any(AddressSpace::Memory);
                    effects.read_any(AddressSpace::Storage);
                    effects.write_any(AddressSpace::Storage);
                    effects.read_any(AddressSpace::Transient);
                    effects.write_any(AddressSpace::Transient);
                }
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::Stop
            | Terminator::Invalid
            | Terminator::SelfDestruct { .. } => {}
        }
        effects
    }

    /// Returns the canonical free-memory-pointer word location.
    #[must_use]
    pub(crate) const fn fmp_location() -> MemoryLocation {
        MemoryLocation::new(
            MemoryAddress {
                region: MemoryRegion::Scratch,
                base: MemoryBase::Absolute,
                offset: EvmMemoryLayout::FMP_SLOT,
            },
            LocationSize::Const(32),
        )
    }

    fn access_may_alias(&self, access: Access, location: Location) -> bool {
        match access {
            Access::Any(space) => space == location.address_space(),
            Access::Location(other) => self.alias(other, location).may_alias(),
        }
    }

    /// Computes the alias relationship between two memory ranges.
    #[must_use]
    pub(crate) fn memory_alias(
        &self,
        first: MemoryLocation,
        second: MemoryLocation,
    ) -> AliasResult {
        Self::memory_alias_locations(first, second)
    }

    /// Computes a memory-range relationship after addresses are canonicalized.
    #[must_use]
    pub(crate) fn memory_alias_locations(
        first: MemoryLocation,
        second: MemoryLocation,
    ) -> AliasResult {
        if matches!(first.size, LocationSize::Const(0))
            || matches!(second.size, LocationSize::Const(0))
        {
            return AliasResult::NoAlias;
        }
        let first_region = first.address.region;
        let second_region = second.address.region;
        if first_region != MemoryRegion::Unknown
            && second_region != MemoryRegion::Unknown
            && first_region != second_region
        {
            return AliasResult::NoAlias;
        }
        // Two accesses based on distinct allocation sites never overlap: each
        // `alloc` bumps the free-memory pointer into a fresh region, even
        // across loop iterations, so a loop-instance allocation is still
        // disjoint from every other allocation site. Two accesses to the same
        // loop-instance allocation may hit the same or different instances, so
        // they stay `MayAlias`; a dynamic allocation against a
        // non-allocation base is likewise `MayAlias`.
        let first_alloc = Self::allocation_base(first.address.base);
        let second_alloc = Self::allocation_base(second.address.base);
        match (first_alloc, second_alloc) {
            (Some((a, a_dynamic)), Some((b, b_dynamic))) => {
                if a != b {
                    return AliasResult::NoAlias;
                }
                if a_dynamic || b_dynamic {
                    return AliasResult::MayAlias;
                }
                // Same unique static allocation: compare offsets below.
            }
            (Some((_, true)), _) | (_, Some((_, true))) => return AliasResult::MayAlias,
            _ => {}
        }
        if first.address.base != second.address.base {
            return AliasResult::MayAlias;
        }
        match (first.size, second.size) {
            (LocationSize::Const(first_size), LocationSize::Const(second_size)) => {
                let Some(first_end) = first.address.offset.checked_add(first_size) else {
                    return AliasResult::MayAlias;
                };
                let Some(second_end) = second.address.offset.checked_add(second_size) else {
                    return AliasResult::MayAlias;
                };
                if first.address.offset >= second_end || second.address.offset >= first_end {
                    AliasResult::NoAlias
                } else if first.address.offset == second.address.offset && first_size == second_size
                {
                    AliasResult::MustAlias
                } else {
                    AliasResult::PartialAlias
                }
            }
            (LocationSize::Dynamic(first_size), LocationSize::Dynamic(second_size))
                if first_size == second_size && first.address.offset == second.address.offset =>
            {
                AliasResult::MustAlias
            }
            _ => AliasResult::MayAlias,
        }
    }

    fn memory_address_with_depth(
        &self,
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        if let Some(cached) = self.provenance.addresses.borrow().get(&value).copied() {
            return cached;
        }
        if depth > 8 {
            return Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)));
        }
        if !self.provenance.visiting.borrow_mut().insert(value) {
            return Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)));
        }
        let address = (|| match func.value(value) {
            Value::Immediate(immediate) => {
                Some(MemoryAddress::absolute(immediate.as_u256()?.try_into().ok()?))
            }
            Value::Arg { ty, .. } => Some(MemoryAddress::symbolic(
                value,
                if matches!(ty, crate::mir::MirType::MemoryObject(_)) {
                    MemoryRegion::Heap
                } else {
                    MemoryRegion::Unknown
                },
            )),
            Value::Undef(_) | Value::Error(_) => None,
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::InternalFrameAddr(offset) => Some(MemoryAddress::internal_frame(offset)),
                InstKind::Alloc { .. } => Some(if self.allocation_is_dynamic(*inst_id) {
                    MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::DynamicAllocation(*inst_id),
                        offset: 0,
                    }
                } else if self.allocation_has_unique_provenance(*inst_id) {
                    MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::Allocation(*inst_id),
                        offset: 0,
                    }
                } else {
                    MemoryAddress::symbolic(value, MemoryRegion::Heap)
                }),
                InstKind::Add(first, second) => self
                    .address_add(func, first, second, depth)
                    .or_else(|| self.address_add(func, second, first, depth))
                    .or_else(|| {
                        Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)))
                    }),
                InstKind::Sub(base, offset) => {
                    self.address_sub(func, base, offset, depth).or_else(|| {
                        Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)))
                    })
                }
                InstKind::SlicePtr(slice) => self.slice_pointer_address(func, slice, depth),
                InstKind::MemoryObjectData(object, kind) => self
                    .memory_address_with_depth(func, object, depth + 1)?
                    .checked_add(EvmMemoryLayout::object_data_offset(kind)),
                InstKind::MemoryObjectFieldAddr { object, layout, field } => self
                    .memory_address_with_depth(func, object, depth + 1)?
                    .checked_add(EvmMemoryLayout::field_offset(layout, field)?),
                InstKind::MemoryObjectElementAddr { object, layout, index } => {
                    let base = self
                        .memory_address_with_depth(func, object, depth + 1)?
                        .checked_add(EvmMemoryLayout::object_data_offset(layout.kind()))?;
                    let Some(index) = func.value_u64(index) else {
                        return Some(MemoryAddress::symbolic(value, base.region));
                    };
                    base.checked_add(index.checked_mul(EvmMemoryLayout::element_stride(layout)?)?)
                }
                InstKind::Phi(ref incoming) => self.join_pointer_paths(
                    func,
                    value,
                    incoming.iter().map(|(_, value)| *value),
                    depth,
                ),
                InstKind::Select(_, first, second) => {
                    self.join_pointer_paths(func, value, [first, second], depth)
                }
                _ => Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0))),
            },
        })();
        self.provenance.visiting.borrow_mut().remove(&value);
        self.provenance.addresses.borrow_mut().insert(value, address);
        address
    }

    fn address_add(
        &self,
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        self.memory_address_with_depth(func, base, depth + 1)?.checked_add(func.value_u64(offset)?)
    }

    fn address_sub(
        &self,
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        let mut address = self.memory_address_with_depth(func, base, depth + 1)?;
        address.offset = address.offset.checked_sub(func.value_u64(offset)?)?;
        Some(address)
    }

    fn slice_pointer_address(
        &self,
        func: &Function,
        slice: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        let Value::Inst(inst_id) = func.value(slice) else {
            return Some(MemoryAddress::symbolic(slice, MemoryRegion::Unknown));
        };
        match &func.instructions[*inst_id].kind {
            InstKind::MakeSlice { ptr, location, .. } => match location {
                SliceLocation::Memory => self.memory_address_with_depth(func, *ptr, depth + 1),
                // Calldata and returndata pointers index their own address
                // spaces, not memory, so they carry no memory provenance.
                SliceLocation::Calldata | SliceLocation::Returndata => None,
            },
            InstKind::AbiEncode { .. } => Some(if self.allocation_is_dynamic(*inst_id) {
                MemoryAddress {
                    region: MemoryRegion::Heap,
                    base: MemoryBase::DynamicAllocation(*inst_id),
                    offset: 0,
                }
            } else if self.allocation_has_unique_provenance(*inst_id) {
                MemoryAddress {
                    region: MemoryRegion::Heap,
                    base: MemoryBase::Allocation(*inst_id),
                    offset: 0,
                }
            } else {
                MemoryAddress::symbolic(slice, MemoryRegion::Heap)
            }),
            _ => Some(MemoryAddress::symbolic(slice, MemoryRegion::Unknown)),
        }
    }

    fn join_pointer_paths(
        &self,
        func: &Function,
        result: ValueId,
        incoming: impl IntoIterator<Item = ValueId>,
        depth: usize,
    ) -> Option<MemoryAddress> {
        let mut incoming = incoming.into_iter();
        let first = self.memory_address_with_depth(func, incoming.next()?, depth + 1)?;
        let mut region = first.region;
        let mut all_same = true;
        for value in incoming {
            let Some(address) = self.memory_address_with_depth(func, value, depth + 1) else {
                return Some(MemoryAddress::symbolic(result, MemoryRegion::Unknown));
            };
            all_same &= address == first;
            if address.region != region {
                region = MemoryRegion::Unknown;
            }
        }
        if all_same { Some(first) } else { Some(MemoryAddress::symbolic(result, region)) }
    }

    fn pointer_region(&self, func: &Function, value: ValueId, depth: usize) -> MemoryRegion {
        if depth > 8 {
            return MemoryRegion::Unknown;
        }
        let Value::Inst(inst_id) = func.value(value) else {
            return match func.value(value) {
                Value::Arg { ty: crate::mir::MirType::MemoryObject(_), .. } => MemoryRegion::Heap,
                _ => MemoryRegion::Unknown,
            };
        };
        match func.instructions[*inst_id].kind {
            InstKind::InternalFrameAddr(_) => MemoryRegion::InternalFrame,
            InstKind::Fmp | InstKind::Alloc { .. } => MemoryRegion::Heap,
            InstKind::MLoad(address)
                if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
            {
                MemoryRegion::Heap
            }
            InstKind::Add(first, second) => {
                let first = self.pointer_region(func, first, depth + 1);
                if first != MemoryRegion::Unknown {
                    first
                } else {
                    self.pointer_region(func, second, depth + 1)
                }
            }
            InstKind::Sub(base, _)
            | InstKind::MemoryObjectData(base, _)
            | InstKind::MemoryObjectFieldAddr { object: base, .. }
            | InstKind::MemoryObjectElementAddr { object: base, .. } => {
                self.pointer_region(func, base, depth + 1)
            }
            InstKind::Phi(ref incoming) => {
                self.join_pointer_regions(func, incoming.iter().map(|(_, value)| *value), depth)
            }
            InstKind::Select(_, first, second) => {
                self.join_pointer_regions(func, [first, second], depth)
            }
            InstKind::SlicePtr(slice) => {
                let Value::Inst(slice_inst) = func.value(slice) else {
                    return MemoryRegion::Unknown;
                };
                match &func.instructions[*slice_inst].kind {
                    InstKind::MakeSlice { location: SliceLocation::Memory, .. }
                    | InstKind::AbiEncode { .. } => MemoryRegion::Heap,
                    _ => MemoryRegion::Unknown,
                }
            }
            _ => MemoryRegion::Unknown,
        }
    }

    fn join_pointer_regions(
        &self,
        func: &Function,
        incoming: impl IntoIterator<Item = ValueId>,
        depth: usize,
    ) -> MemoryRegion {
        let mut incoming = incoming.into_iter();
        let Some(first) = incoming.next() else { return MemoryRegion::Unknown };
        let first = self.pointer_region(func, first, depth + 1);
        if first == MemoryRegion::Unknown {
            return first;
        }
        if incoming.all(|value| self.pointer_region(func, value, depth + 1) == first) {
            first
        } else {
            MemoryRegion::Unknown
        }
    }

    fn allocation_has_unique_provenance(&self, target: InstId) -> bool {
        self.provenance.allocations.get(&target).is_some_and(|facts| facts.unique)
    }

    fn allocation_is_dynamic(&self, target: InstId) -> bool {
        self.provenance.allocations.get(&target).is_some_and(|facts| facts.dynamic)
    }

    /// Returns whether an instruction may recycle or arbitrarily replace the FMP.
    #[must_use]
    pub(crate) fn instruction_may_reset_fmp(&self, func: &Function, inst: InstId) -> bool {
        Self::instruction_may_reset_fmp_with_summaries(func, inst, self.call_summaries.as_deref())
    }

    fn instruction_may_reset_fmp_with_summaries(
        func: &Function,
        inst: InstId,
        call_summaries: Option<&MemoryCallSummaries>,
    ) -> bool {
        match func.instructions[inst].kind {
            InstKind::SetFmp(_) => true,
            InstKind::InternalCall { function, .. } => call_summaries
                .and_then(|summaries| summaries.get(function))
                .is_none_or(|summary| summary.may_reset_fmp()),
            InstKind::MStore(address, _) => Self::range_may_overlap_fmp(func, address, Some(32)),
            InstKind::MStore8(address, _) => Self::range_may_overlap_fmp(func, address, Some(1)),
            InstKind::MCopy(dest, _, size)
            | InstKind::CalldataCopy(dest, _, size)
            | InstKind::CodeCopy(dest, _, size)
            | InstKind::ReturnDataCopy(dest, _, size) => {
                Self::range_may_overlap_fmp(func, dest, func.value_u64(size))
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                Self::range_may_overlap_fmp(func, dest, func.value_u64(size))
            }
            InstKind::Call { ret_offset, ret_size, .. }
            | InstKind::StaticCall { ret_offset, ret_size, .. }
            | InstKind::DelegateCall { ret_offset, ret_size, .. } => {
                Self::range_may_overlap_fmp(func, ret_offset, func.value_u64(ret_size))
            }
            _ => false,
        }
    }

    fn range_may_overlap_fmp(func: &Function, address: ValueId, size: Option<u64>) -> bool {
        if size == Some(0) {
            return false;
        }
        if let Some(address) = func.value_u64(address) {
            let Some(size) = size else { return true };
            let Some(end) = address.checked_add(size) else { return true };
            let fmp_end = EvmMemoryLayout::FMP_SLOT + EvmMemoryLayout::WORD_SIZE;
            return address < fmp_end && end > EvmMemoryLayout::FMP_SLOT;
        }
        Self::pointer_lower_bound(func, address, 0)
            .is_none_or(|address| address < EvmMemoryLayout::HEAP_START)
    }

    /// Returns a conservative lower bound for a compiler-owned pointer.
    ///
    /// This deliberately does not trust a coarse `Heap` region: inline MIR can
    /// subtract from a pointer, and a typed argument may still originate in
    /// opaque code. Only monotonic derivations from compiler-owned bases prove
    /// that a write cannot reach reserved low memory.
    fn pointer_lower_bound(func: &Function, value: ValueId, depth: usize) -> Option<u64> {
        if depth > 8 {
            return None;
        }
        if let Some(value) = func.value_u64(value) {
            return Some(value);
        }
        let Value::Inst(inst) = func.value(value) else { return None };
        match &func.instructions[*inst].kind {
            InstKind::Fmp | InstKind::Alloc { .. } | InstKind::AbiEncode { .. } => {
                Some(EvmMemoryLayout::HEAP_START)
            }
            InstKind::MLoad(address)
                if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
            {
                Some(EvmMemoryLayout::HEAP_START)
            }
            InstKind::InternalFrameAddr(offset) => EvmMemoryLayout::HEAP_START.checked_add(*offset),
            InstKind::Add(first, second) => Self::pointer_lower_bound(func, *first, depth + 1)
                .and_then(|base| base.checked_add(func.value_u64(*second)?))
                .or_else(|| {
                    Self::pointer_lower_bound(func, *second, depth + 1)
                        .and_then(|base| base.checked_add(func.value_u64(*first)?))
                }),
            InstKind::Sub(base, offset) => Self::pointer_lower_bound(func, *base, depth + 1)
                .and_then(|base| base.checked_sub(func.value_u64(*offset)?)),
            InstKind::SlicePtr(slice) => {
                let Value::Inst(slice) = func.value(*slice) else { return None };
                match &func.instructions[*slice].kind {
                    InstKind::MakeSlice { ptr, location: SliceLocation::Memory, .. } => {
                        Self::pointer_lower_bound(func, *ptr, depth + 1)
                    }
                    InstKind::AbiEncode { .. } => Some(EvmMemoryLayout::HEAP_START),
                    _ => None,
                }
            }
            InstKind::Phi(incoming) => incoming
                .iter()
                .map(|(_, value)| Self::pointer_lower_bound(func, *value, depth + 1))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .min(),
            InstKind::Select(_, first, second) => {
                Some(
                    Self::pointer_lower_bound(func, *first, depth + 1)?
                        .min(Self::pointer_lower_bound(func, *second, depth + 1)?),
                )
            }
            _ => None,
        }
    }

    fn resolved_location_size(
        &self,
        func: &Function,
        size: SizeOperand,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> LocationSize {
        match size {
            SizeOperand::Const(size) => LocationSize::Const(size),
            SizeOperand::Value(value) => {
                let value = crate::mir::utils::resolve_replacement(value, replacements);
                self.location_size(func, value)
            }
            SizeOperand::Unknown => LocationSize::Unknown,
        }
    }

    fn abi_type_reads_memory(ty: &AbiType) -> bool {
        match ty {
            // Calldata and returndata ABI values read their own buffers, not
            // memory (returndata does not occur as an ABI type in practice).
            AbiType::Word
            | AbiType::Bytes(SliceLocation::Calldata | SliceLocation::Returndata)
            | AbiType::DynamicArray {
                location: SliceLocation::Calldata | SliceLocation::Returndata,
                ..
            } => false,
            AbiType::Bytes(SliceLocation::Memory)
            | AbiType::DynamicArray { location: SliceLocation::Memory, .. }
            | AbiType::FixedArray { .. }
            | AbiType::Tuple(_) => true,
        }
    }
}

fn cyclic_blocks(func: &Function) -> Vec<bool> {
    let successors: Vec<Vec<_>> =
        func.blocks
            .iter()
            .map(|block| {
                block.terminator.as_ref().map_or_else(Vec::new, |terminator| {
                    terminator.successors().into_iter().collect()
                })
            })
            .collect();
    let mut predecessors = vec![Vec::new(); func.blocks.len()];
    for (block, block_successors) in successors.iter().enumerate() {
        for &successor in block_successors {
            predecessors[successor.index()].push(BlockId::from_usize(block));
        }
    }

    // Kosaraju's two linear scans classify all strongly connected components.
    // Doing one reachability search per block made constructing this analysis
    // quadratic on functions with many basic blocks.
    let mut visited = vec![false; func.blocks.len()];
    let mut finish_order = Vec::with_capacity(func.blocks.len());
    for start in func.blocks.indices() {
        if visited[start.index()] {
            continue;
        }
        visited[start.index()] = true;
        let mut stack = vec![(start, false)];
        while let Some((block, expanded)) = stack.pop() {
            if expanded {
                finish_order.push(block);
                continue;
            }
            stack.push((block, true));
            for &successor in &successors[block.index()] {
                if !std::mem::replace(&mut visited[successor.index()], true) {
                    stack.push((successor, false));
                }
            }
        }
    }

    let mut cyclic = vec![false; func.blocks.len()];
    visited.fill(false);
    for start in finish_order.into_iter().rev() {
        if visited[start.index()] {
            continue;
        }
        visited[start.index()] = true;
        let mut component = Vec::new();
        let mut stack = vec![start];
        while let Some(block) = stack.pop() {
            component.push(block);
            for &predecessor in &predecessors[block.index()] {
                if !std::mem::replace(&mut visited[predecessor.index()], true) {
                    stack.push(predecessor);
                }
            }
        }
        let is_cycle = component.len() > 1 || successors[start.index()].contains(&start);
        if is_cycle {
            for block in component {
                cyclic[block.index()] = true;
            }
        }
    }
    cyclic
}

#[derive(Clone, Copy)]
enum SizeOperand {
    Const(u64),
    Value(ValueId),
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{FunctionBuilder, MirType};
    use alloy_primitives::U256;
    use solar_interface::Ident;

    fn function() -> Function {
        Function::new(Ident::DUMMY)
    }

    #[test]
    fn canonicalizes_memory_bases_and_offsets() {
        let mut func = function();
        let (arg, frame, heap, absolute) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let arg = builder.add_param(MirType::MemPtr);
            let offset = builder.imm_u64(64);
            let arg = builder.add(arg, offset);
            let frame = builder.internal_frame_addr(32);
            let frame = builder.add(frame, offset);
            let heap = builder.fmp();
            let heap = builder.add(heap, offset);
            let absolute = builder.imm_u64(0x20);
            (arg, frame, heap, absolute)
        };
        let aa = AliasAnalysis::new(&func);

        assert_eq!(
            aa.memory_address(&func, arg),
            Some(MemoryAddress {
                region: MemoryRegion::Unknown,
                base: MemoryBase::Value(ValueId::from_usize(0)),
                offset: 64,
            })
        );
        assert_eq!(aa.memory_address(&func, frame), Some(MemoryAddress::internal_frame(96)));
        assert_eq!(aa.memory_address(&func, heap).unwrap().region, MemoryRegion::Heap);
        assert_eq!(aa.memory_address(&func, absolute), Some(MemoryAddress::absolute(0x20)));
    }

    #[test]
    fn distinguishes_and_classifies_memory_ranges() {
        let base = ValueId::from_usize(0);
        let location = |offset, size| {
            Location::Memory(MemoryLocation::new(
                MemoryAddress { region: MemoryRegion::Heap, base: MemoryBase::Value(base), offset },
                LocationSize::Const(size),
            ))
        };

        assert_eq!(
            AliasAnalysis::alias_locations(location(0, 32), location(0, 32)),
            AliasResult::MustAlias
        );
        assert_eq!(
            AliasAnalysis::alias_locations(location(0, 32), location(16, 32)),
            AliasResult::PartialAlias
        );
        assert_eq!(
            AliasAnalysis::alias_locations(location(0, 32), location(32, 32)),
            AliasResult::NoAlias
        );

        let scratch = Location::Memory(AliasAnalysis::fmp_location());
        assert_eq!(AliasAnalysis::alias_locations(scratch, location(0, 32)), AliasResult::NoAlias);
    }

    #[test]
    fn tracks_fresh_allocations_through_control_flow_values() {
        let mut func = function();
        let (first, second, same_path, joined_path) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let size = builder.imm_u64(32);
            let first = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            let second = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            let condition = builder.add_param(MirType::Bool);
            let same_path = builder.select(condition, first, first);
            let joined_path = builder.select(condition, first, second);
            (first, second, same_path, joined_path)
        };
        let aa = AliasAnalysis::new(&func);
        let location = |value| {
            MemoryLocation::new(aa.memory_address(&func, value).unwrap(), LocationSize::Const(32))
        };

        assert_eq!(aa.memory_alias(location(first), location(second)), AliasResult::NoAlias);
        assert_eq!(aa.memory_address(&func, same_path), aa.memory_address(&func, first));
        assert_eq!(aa.memory_address(&func, joined_path).unwrap().region, MemoryRegion::Heap);
        assert_eq!(aa.memory_alias(location(joined_path), location(first)), AliasResult::MayAlias);
    }

    #[test]
    fn forgets_freshness_after_an_fmp_reset() {
        let mut func = function();
        let (first, second) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let size = builder.imm_u64(32);
            let first = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            builder.set_fmp(first);
            let second = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            (first, second)
        };
        let aa = AliasAnalysis::new(&func);
        let location = |value| {
            MemoryLocation::new(aa.memory_address(&func, value).unwrap(), LocationSize::Const(32))
        };

        assert!(matches!(location(first).address.base, MemoryBase::Allocation(_)));
        assert!(matches!(location(second).address.base, MemoryBase::Value(_)));
        assert_eq!(aa.memory_alias(location(first), location(second)), AliasResult::MayAlias);
    }

    #[test]
    fn does_not_globalize_loop_allocation_identity() {
        let mut func = function();
        let allocation = {
            let mut builder = FunctionBuilder::new(&mut func);
            let condition = builder.add_param(MirType::Bool);
            let header = builder.create_block();
            let exit = builder.create_block();
            builder.jump(header);
            builder.switch_to_block(header);
            let size = builder.imm_u64(32);
            let allocation = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            builder.branch(condition, header, exit);
            builder.switch_to_block(exit);
            builder.stop();
            allocation
        };

        let aa = AliasAnalysis::new(&func);
        let address = aa.memory_address(&func, allocation).unwrap();
        assert!(matches!(address.base, MemoryBase::DynamicAllocation(_)));
        let location = MemoryLocation::new(address, LocationSize::Const(32));
        assert_eq!(aa.memory_alias(location, location), AliasResult::MayAlias);
    }

    #[test]
    fn distinct_loop_allocations_do_not_alias() {
        let mut func = function();
        let (first, second) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let condition = builder.add_param(MirType::Bool);
            let header = builder.create_block();
            let exit = builder.create_block();
            builder.jump(header);
            builder.switch_to_block(header);
            let size = builder.imm_u64(32);
            // Two distinct allocation sites inside the same loop: each bumps the
            // free-memory pointer, so their regions never overlap.
            let first = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            let second = builder.alloc(size, crate::mir::AllocationSemantics::INTERNAL);
            builder.branch(condition, header, exit);
            builder.switch_to_block(exit);
            builder.stop();
            (first, second)
        };

        let aa = AliasAnalysis::new(&func);
        let location = |value| {
            MemoryLocation::new(aa.memory_address(&func, value).unwrap(), LocationSize::Const(32))
        };
        assert!(matches!(location(first).address.base, MemoryBase::DynamicAllocation(_)));
        assert!(matches!(location(second).address.base, MemoryBase::DynamicAllocation(_)));
        assert_eq!(aa.memory_alias(location(first), location(second)), AliasResult::NoAlias);
        // The same loop allocation still may hit different instances.
        assert_eq!(aa.memory_alias(location(first), location(first)), AliasResult::MayAlias);
    }

    #[test]
    fn classifies_storage_aliases() {
        let first = Location::Storage(StorageAlias::Slot(U256::from(1)));
        let second = Location::Storage(StorageAlias::Slot(U256::from(2)));
        assert_eq!(AliasAnalysis::alias_locations(first, first), AliasResult::MustAlias);
        assert_eq!(AliasAnalysis::alias_locations(first, second), AliasResult::NoAlias);

        let symbolic = Location::Storage(StorageAlias::Symbolic(ValueId::from_usize(0)));
        assert_eq!(AliasAnalysis::alias_locations(first, symbolic), AliasResult::MayAlias);
        assert_eq!(
            AliasAnalysis::alias_locations(
                first,
                Location::Transient(StorageAlias::Slot(U256::from(1))),
            ),
            AliasResult::NoAlias
        );
    }

    #[test]
    fn reports_precise_copy_modref() {
        let mut func = function();
        let copy = {
            let mut builder = FunctionBuilder::new(&mut func);
            let dest = builder.imm_u64(0x80);
            let source = builder.imm_u64(0x20);
            let size = builder.imm_u64(32);
            builder.mcopy(dest, source, size);
            *builder.func().blocks[builder.current_block()].instructions.last().unwrap()
        };
        let aa = AliasAnalysis::new(&func);
        let effects = aa.instruction_mod_ref(&func, copy);

        assert_eq!(effects.reads().len(), 1);
        assert_eq!(effects.writes().len(), 1);
        assert!(effects.reads_space(AddressSpace::Memory));
        assert!(effects.writes_space(AddressSpace::Memory));
    }

    #[test]
    fn static_call_reads_state_but_cannot_write_it() {
        let mut func = function();
        let call = {
            let mut builder = FunctionBuilder::new(&mut func);
            let gas = builder.imm_u64(100_000);
            let address = builder.imm_u64(1);
            let offset = builder.imm_u64(0x80);
            let size = builder.imm_u64(32);
            builder.staticcall(gas, address, offset, size, offset, size);
            *builder.func().blocks[builder.current_block()].instructions.last().unwrap()
        };
        let effects = AliasAnalysis::new(&func).instruction_mod_ref(&func, call);

        assert!(effects.reads_space(AddressSpace::Storage));
        assert!(!effects.writes_space(AddressSpace::Storage));
        assert!(effects.reads_space(AddressSpace::Memory));
        assert!(effects.writes_space(AddressSpace::Memory));
    }
}

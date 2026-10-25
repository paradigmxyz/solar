//! Shared MIR alias and ModRef analysis.
//!
//! Pointer provenance follows SSA definitions through address arithmetic,
//! slices, selects, and control-flow joins. Fresh allocations retain unique
//! identities, while incompatible incoming paths conservatively join to a
//! symbolic pointer. Region labels do not prove disjointness: raw pointer operations
//! and FMP recycling can cross those regions. The analysis exposes the memory,
//! storage, and transient-storage effects of each instruction.

use super::{CfgInfo, MemoryCallSummaries};
use crate::mir::{
    AbiType, AddressCallKind, ArgIdx, BlockId, Builtin, Callee, FrameMode, FrameSlotKind, Function,
    ImmutableId, InstId, InstKind, MAX_SCRATCH_ERROR_WORDS, MemoryRegion, MirType, PtrLocation,
    RequireKind, StorageAlias, Terminator, Value, ValueId, memory::EvmMemoryLayout,
};
use smallvec::SmallVec;
use solar_data_structures::{bit_set::DenseBitSet, index::IndexVec, map::FxHashMap};
use std::{
    cell::{OnceCell, RefCell},
    collections::VecDeque,
    sync::Arc,
};

/// An address space tracked by ModRef analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AddressSpace {
    /// EVM linear memory.
    Memory,
    /// Persistent contract storage.
    Storage,
    /// Transaction-scoped transient storage.
    Transient,
    /// Constructor-assigned immutable values.
    Immutable,
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
    /// An unrecycled allocation site with multiple dynamic loop instances.
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

    /// Returns whether this address is the base of a fresh allocation.
    pub(crate) fn is_allocation_base(self) -> bool {
        self.offset == 0 && matches!(self.base, MemoryBase::Allocation(_))
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
    /// One module immutable.
    Immutable(ImmutableId),
}

impl Location {
    /// Returns this location's address space.
    #[must_use]
    pub(crate) const fn address_space(self) -> AddressSpace {
        match self {
            Self::Memory(_) => AddressSpace::Memory,
            Self::Storage(_) => AddressSpace::Storage,
            Self::Transient(_) => AddressSpace::Transient,
            Self::Immutable(_) => AddressSpace::Immutable,
        }
    }

    /// Returns the slot `alias` names in storage or transient storage `location`.
    #[must_use]
    pub(crate) const fn slot(location: PtrLocation, alias: StorageAlias) -> Option<Self> {
        match location {
            PtrLocation::Storage => Some(Self::Storage(alias)),
            PtrLocation::Transient => Some(Self::Transient(alias)),
            PtrLocation::Memory | PtrLocation::Calldata | PtrLocation::Returndata => None,
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

/// Memory and state accesses that one MIR operation may perform.
///
/// A write footprint bounds possible writes; it does not prove that the
/// operation overwrites every byte or executes the write on every path.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct ModRef {
    reads: SmallVec<[Access; 4]>,
    writes: SmallVec<[Access; 4]>,
    observes_gas: bool,
}

impl ModRef {
    /// Returns exact and address-space-wide possible reads.
    #[must_use]
    pub(crate) fn reads(&self) -> &[Access] {
        &self.reads
    }

    /// Returns exact and address-space-wide possible writes.
    #[must_use]
    pub(crate) fn writes(&self) -> &[Access] {
        &self.writes
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

    /// Returns whether this operation may read `location`.
    #[must_use]
    pub(crate) fn may_read(&self, aa: &AliasAnalysis, location: Location) -> bool {
        self.reads.iter().any(|&access| aa.access_may_alias(access, location))
    }

    /// Returns whether this operation may write `location`.
    #[must_use]
    pub(crate) fn may_write(&self, aa: &AliasAnalysis, location: Location) -> bool {
        self.writes.iter().any(|&access| aa.access_may_alias(access, location))
    }

    fn merge_call_summary(
        &mut self,
        summary: &super::memory_summary::FunctionMemorySummary,
        func: &Function,
        aa: &AliasAnalysis,
        args: &[ValueId],
    ) {
        for space in [
            AddressSpace::Memory,
            AddressSpace::Storage,
            AddressSpace::Transient,
            AddressSpace::Immutable,
        ] {
            for write in [false, true] {
                if !(if write { summary.writes(space) } else { summary.reads(space) }) {
                    continue;
                }
                let mut record = |access| {
                    if write {
                        self.write(access);
                    } else {
                        self.read(access);
                    }
                };
                if space == AddressSpace::Memory {
                    for access in summary.memory_accesses(func, aa, args, write) {
                        record(access);
                    }
                } else if let Some(slots) = summary.storage_slots(space, write) {
                    for &slot in slots {
                        let location = match space {
                            AddressSpace::Storage => Location::Storage(StorageAlias::Slot(slot)),
                            AddressSpace::Transient => {
                                Location::Transient(StorageAlias::Slot(slot))
                            }
                            _ => unreachable!(),
                        };
                        record(Access::Location(location));
                    }
                } else {
                    record(Access::Any(space));
                }
            }
        }
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

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct AllocationProvenance {
    dynamic: bool,
    unique: bool,
}

/// Memoized address resolution of one value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum AddressState {
    #[default]
    Unresolved,
    /// Resolution is in progress further up the recursion.
    Visiting,
    Resolved(Option<MemoryAddress>),
}

#[derive(Clone, Debug, Default)]
struct PointerProvenance {
    allocations: FxHashMap<InstId, AllocationProvenance>,
    /// Grows on demand, since transforms may add values after construction.
    addresses: RefCell<IndexVec<ValueId, AddressState>>,
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
        // The cycle classification, the FMP-reset dataflow, and the
        // per-instruction reset scan below exist only to classify allocation
        // sites. A function without allocations gets an empty map either way,
        // so skip all of it — most small functions (getters, setters, pure
        // helpers) take this path on every rebuild.
        let is_allocation = |inst_id| match func.inst(inst_id).kind {
            InstKind::Alloc { .. } | InstKind::AbiDecode { .. } => true,
            InstKind::AbiEncode { mode, .. } => mode != crate::mir::AbiEncodeMode::Scratch,
            _ => false,
        };
        let has_allocations = func.instructions().any(&is_allocation);
        if !has_allocations {
            return Self::default();
        }
        let cfg = CfgInfo::new(func);
        let cyclic = cfg.cyclic_blocks();
        // The index of each block's first instruction that may reset the FMP.
        let first_resets = func
            .blocks
            .iter()
            .map(|block| {
                block
                    .instructions
                    .iter()
                    .position(|&inst| may_reset_fmp(func, inst, call_summaries))
            })
            .collect::<IndexVec<BlockId, _>>();

        // `poisoned[block]` means at least one path into the block may have
        // recycled the FMP. The monotone OR lattice handles joins and loops.
        let mut reachable = DenseBitSet::new_empty(func.blocks.len());
        let mut poisoned = DenseBitSet::new_empty(func.blocks.len());
        let mut worklist = VecDeque::from([BlockId::ENTRY]);
        reachable.insert(BlockId::ENTRY);
        while let Some(block) = worklist.pop_front() {
            let out = poisoned.contains(block) || first_resets[block].is_some();
            let Some(terminator) = &func.blocks[block].terminator else { continue };
            for successor in terminator.successors() {
                let mut changed = reachable.insert(successor);
                if out {
                    changed |= poisoned.insert(successor);
                }
                if changed {
                    worklist.push_back(successor);
                }
            }
        }

        let mut allocations = FxHashMap::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            let entry_fresh = reachable.contains(block_id) && !poisoned.contains(block_id);
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                if is_allocation(inst_id) {
                    let fresh =
                        entry_fresh && first_resets[block_id].is_none_or(|first| index <= first);
                    allocations.insert(
                        inst_id,
                        AllocationProvenance {
                            dynamic: fresh && cyclic.contains(block_id),
                            unique: fresh && !cyclic.contains(block_id),
                        },
                    );
                }
            }
        }

        Self { allocations, addresses: RefCell::default() }
    }
}

/// Shared cached, control-flow-aware provenance, alias, and ModRef analysis.
///
/// One instance is an immutable snapshot of a function. Recompute it after a
/// transform mutates definitions or CFG edges.
#[derive(Clone, Debug)]
pub(crate) struct AliasAnalysis {
    /// Pointer-provenance facts, built on first use: many pass invocations
    /// construct the analysis but never issue a memory query (pure or
    /// memory-free functions), so the block scans and the FMP dataflow are
    /// deferred until a query actually needs them.
    provenance: OnceCell<PointerProvenance>,
    /// Values whose derived pointer graph reaches a capturing use.
    escaping_values: RefCell<Option<DenseBitSet<ValueId>>>,
    call_summaries: Option<Arc<MemoryCallSummaries>>,
}

impl AliasAnalysis {
    /// Computes a function-local snapshot with conservative internal calls.
    #[must_use]
    pub(crate) fn new(func: &Function) -> Self {
        Self::with_optional_summaries(func, None)
    }

    /// An unpopulated snapshot that resolves internal calls through
    /// `summaries`: provenance and memos build lazily on the first query, so
    /// this is equivalent to [`Self::with_call_summaries`] without needing the
    /// function up front.
    #[must_use]
    pub(crate) fn empty_with_summaries(summaries: Arc<MemoryCallSummaries>) -> Self {
        Self {
            provenance: OnceCell::new(),
            escaping_values: RefCell::new(None),
            call_summaries: Some(summaries),
        }
    }

    /// Computes a snapshot using module-level internal-call summaries.
    #[must_use]
    pub(crate) fn with_call_summaries(
        func: &Function,
        summaries: Arc<MemoryCallSummaries>,
    ) -> Self {
        Self::with_optional_summaries(func, Some(summaries))
    }

    /// Returns whether a fresh snapshot of one function holds the same memoized facts as this
    /// one. Values added since this snapshot, such as immediates a rewrite created and
    /// discarded, must be unresolved.
    #[cfg(debug_assertions)]
    pub(crate) fn same_memo(&self, fresh: &Self) -> bool {
        let same_provenance = match (self.provenance.get(), fresh.provenance.get()) {
            (None, None) => true,
            (Some(cached), Some(fresh)) => {
                let (cached_addresses, fresh_addresses) =
                    (cached.addresses.borrow(), fresh.addresses.borrow());
                let common = cached_addresses.len().min(fresh_addresses.len());
                cached.allocations == fresh.allocations
                    && cached_addresses.raw[..common] == fresh_addresses.raw[..common]
                    && cached_addresses.raw[common..]
                        .iter()
                        .chain(&fresh_addresses.raw[common..])
                        .all(|state| matches!(state, AddressState::Unresolved))
            }
            _ => false,
        };
        let same_escaping =
            match (&*self.escaping_values.borrow(), &*fresh.escaping_values.borrow()) {
                (None, None) => true,
                (Some(cached), Some(fresh)) => cached.iter().eq(fresh.iter()),
                _ => false,
            };
        same_provenance && same_escaping
    }

    /// Drops value-dependent memoization after instruction operands are rewritten.
    pub(crate) fn clear_cached_addresses(&self) {
        if let Some(provenance) = self.provenance.get() {
            provenance.addresses.borrow_mut().clear();
        }
        self.escaping_values.borrow_mut().take();
    }

    fn with_optional_summaries(
        _func: &Function,
        call_summaries: Option<Arc<MemoryCallSummaries>>,
    ) -> Self {
        Self { provenance: OnceCell::new(), escaping_values: RefCell::new(None), call_summaries }
    }

    /// The lazily built provenance snapshot for `func`. Every query passes the
    /// same function the analysis was created for.
    fn provenance(&self, func: &Function) -> &PointerProvenance {
        self.provenance.get_or_init(|| {
            PointerProvenance::new(
                func,
                self.call_summaries.as_deref(),
                Self::instruction_may_reset_fmp_with_summaries,
            )
        })
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
        let address = self.memory_address(func, address)?;
        Self::bounded_memory_location(func, Self::refine_region(func, inst_id, address), size)
    }

    /// Narrows an address's region to the one its instruction's metadata names.
    fn refine_region(
        func: &Function,
        inst_id: InstId,
        mut address: MemoryAddress,
    ) -> MemoryAddress {
        if let Some(region) = func.inst(inst_id).metadata.memory_region()
            && region != MemoryRegion::Unknown
        {
            address.region = region;
        }
        address
    }

    /// Returns the memory an access at `address` may touch. Unlike [`Self::memory_location`],
    /// which names the accessed word, this covers everything at or after `base + k` for an address
    /// `ptradd (ptradd base, index), k` with a runtime index: `ptradd` does not wrap, so the
    /// access stays inside the base's object.
    fn access_extent(
        &self,
        func: &Function,
        inst_id: InstId,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        let Some(base) = self.runtime_offset_base(func, address) else {
            return self.memory_location(func, inst_id, address, size);
        };
        let base = base?;
        Some(MemoryLocation::new(Self::refine_region(func, inst_id, base), LocationSize::Unknown))
    }

    /// Returns the memory an access at `address` may touch, without instruction metadata, as
    /// [`Self::access_extent`] does.
    #[must_use]
    pub(crate) fn bare_access_extent(
        &self,
        func: &Function,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        match self.runtime_offset_base(func, address) {
            Some(base) => Some(MemoryLocation::new(base?, LocationSize::Unknown)),
            None => self.bare_memory_location(func, address, size),
        }
    }

    /// Returns the address `base + k` of an address `ptradd (ptradd base, index), k` with a
    /// runtime index and constant `ptradd`s `k` around it, or `None` without a runtime index.
    fn runtime_offset_base(
        &self,
        func: &Function,
        address: ValueId,
    ) -> Option<Option<MemoryAddress>> {
        // The canonical address of the chain is the runtime-indexed `ptradd` plus `k`.
        let address = self.memory_address(func, address)?;
        let MemoryBase::Value(indexed) = address.base else { return None };
        let Value::Inst(inst) = func.value(indexed) else { return None };
        let InstKind::PtrAdd(base, index) = func.inst(*inst).kind else { return None };
        func.value_u64(index).is_none().then(|| {
            let base = self.memory_address(func, base)?.checked_add(address.offset)?;
            // NOTE: `ptradd` keeps the access inside its object, but a direct access to an
            // allocation of runtime size cannot prove it fits, so `bounded_memory_location` names
            // that object by its pointer value. Name the indexed access the same way, or the two
            // would look like different objects and every indexed store would clobber the
            // object's header words.
            match base.base {
                MemoryBase::Allocation(inst) | MemoryBase::DynamicAllocation(inst)
                    if Self::allocation_size(func, inst).is_none() =>
                {
                    Self::unbounded_allocation(func, base, inst)
                }
                _ => Some(base),
            }
        })
    }

    /// Creates a memory location without instruction metadata.
    #[must_use]
    pub(crate) fn bare_memory_location(
        &self,
        func: &Function,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        Self::bounded_memory_location(func, self.memory_address(func, address)?, size)
    }

    /// Retains allocation disjointness only when the complete access fits its allocation.
    fn bounded_memory_location(
        func: &Function,
        mut address: MemoryAddress,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        if let MemoryBase::Allocation(inst) | MemoryBase::DynamicAllocation(inst) = address.base {
            let bound = Self::allocation_size(func, inst);
            let within_bounds = size.as_const().is_some_and(|size| {
                size == 0
                    || address
                        .offset
                        .checked_add(size)
                        .zip(bound)
                        .is_some_and(|(end, bound)| end <= bound)
            });
            if !within_bounds {
                address = Self::unbounded_allocation(func, address, inst)?;
            }
        }
        Some(MemoryLocation::new(address, size))
    }

    /// The constant byte size of allocation `inst`, if any.
    fn allocation_size(func: &Function, inst: InstId) -> Option<u64> {
        match func.inst(inst).kind {
            InstKind::Alloc { size, .. } => func.value_u64(size),
            _ => None,
        }
    }

    /// Names an access to allocation `inst` that may leave its bounds by the allocation's
    /// pointer value instead, which keeps no disjointness from other allocations.
    fn unbounded_allocation(
        func: &Function,
        mut address: MemoryAddress,
        inst: InstId,
    ) -> Option<MemoryAddress> {
        // A fresh allocation's nonnegative offsets can reach other heap objects while
        // staying outside reserved memory. Loop allocations may follow an FMP reset.
        if matches!(address.base, MemoryBase::DynamicAllocation(_)) {
            address.region = MemoryRegion::Unknown;
        }
        address.base = MemoryBase::Value(func.inst_result_value(inst)?);
        Some(address)
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
        if self.escaping_values.borrow().is_none() {
            let escaping = self.compute_escaping_values(func);
            *self.escaping_values.borrow_mut() = Some(escaping);
        }
        self.escaping_values.borrow().as_ref().unwrap().contains(root)
    }

    fn compute_escaping_values(&self, func: &Function) -> DenseBitSet<ValueId> {
        let mut escaping = DenseBitSet::new_empty(func.num_values());

        for block in &func.blocks {
            for &inst_id in &block.instructions {
                let kind = &func.inst(inst_id).kind;
                kind.visit_operands(|operand| {
                    if self.instruction_operand_escapes(kind, operand) {
                        escaping.insert(operand);
                    }
                });
            }

            if let Some(terminator) = &block.terminator {
                terminator.visit_operands(|operand| {
                    if self.terminator_operand_escapes(terminator, operand) {
                        escaping.insert(operand);
                    }
                });
            }
        }

        let mut worklist = escaping.iter().collect::<VecDeque<_>>();
        while let Some(value) = worklist.pop_front() {
            let Value::Inst(inst_id) = func.value(value) else { continue };
            let kind = &func.inst(*inst_id).kind;
            let mut propagate = |predecessor| {
                if escaping.insert(predecessor) {
                    worklist.push_back(predecessor);
                }
            };
            match kind {
                InstKind::Add(first, second)
                | InstKind::Sub(first, second)
                | InstKind::PtrAdd(first, second)
                | InstKind::MakeSlice { ptr: first, len: second, .. } => {
                    propagate(*first);
                    propagate(*second);
                }
                InstKind::Phi(incoming) => {
                    for &(_, predecessor) in incoming {
                        propagate(predecessor);
                    }
                }
                InstKind::Select(_, first, second) => {
                    propagate(*first);
                    propagate(*second);
                }
                InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    aggregate: predecessor,
                    index: MirType::SLICE_PTR_FIELD,
                }
                | InstKind::IntToPtr(predecessor)
                | InstKind::PtrToInt(predecessor, 256) => {
                    propagate(*predecessor);
                }
                _ => {}
            }
        }
        escaping
    }

    fn terminator_operand_escapes(&self, terminator: &Terminator, operand: ValueId) -> bool {
        match terminator {
            Terminator::Revert { offset, .. } | Terminator::ReturnData { offset, .. }
                if operand == *offset =>
            {
                false
            }
            Terminator::TailCall { function, args } => {
                let summary =
                    self.call_summaries.as_deref().and_then(|summaries| summaries.get(*function));
                summary.is_none_or(|summary| {
                    args.iter().enumerate().any(|(index, &arg)| {
                        arg == operand && summary.captures_param(ArgIdx::new(index))
                    })
                })
            }
            _ => true,
        }
    }

    fn instruction_operand_escapes(&self, kind: &InstKind, operand: ValueId) -> bool {
        match kind {
            InstKind::Add(_, _)
            | InstKind::Sub(_, _)
            | InstKind::PtrAdd(_, _)
            | InstKind::Phi(_)
            | InstKind::Select(_, _, _)
            | InstKind::MakeSlice { .. }
            | InstKind::ExtractValue { ty: MirType::Slice(_), index: MirType::SLICE_PTR_FIELD, .. }
            | InstKind::IntToPtr(..)
            | InstKind::PtrToInt(_, 256) => false,
            InstKind::Load(address, PtrLocation::Memory) => operand != *address,
            InstKind::Store(address, _, PtrLocation::Memory)
            | InstKind::Store8(address, _)
            | InstKind::MemoryZero(address, _) => operand != *address,
            InstKind::Copy(dest, source, _, PtrLocation::Memory) => {
                operand != *dest && operand != *source
            }
            InstKind::Copy(dest, _, _, PtrLocation::Calldata | PtrLocation::Returndata)
            | InstKind::DataCopy(_, dest, _)
            | InstKind::CodeCopy(dest, _, _)
            | InstKind::Keccak256(dest, _)
            | InstKind::Log { offset: dest, .. } => operand != *dest,
            InstKind::ExtCodeCopy(_, dest, _, _) => operand != *dest,
            InstKind::AddressCall { input, .. } => operand != *input,
            InstKind::Call { args_offset, ret_offset, .. }
            | InstKind::CallCode { args_offset, ret_offset, .. }
            | InstKind::StaticCall { args_offset, ret_offset, .. }
            | InstKind::DelegateCall { args_offset, ret_offset, .. } => {
                operand != *args_offset && operand != *ret_offset
            }
            InstKind::Create(_, offset, _) | InstKind::Create2(_, offset, _, _) => {
                operand != *offset
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::Require(kind)), args } => {
                match (kind, args.as_ref()) {
                    (RequireKind::ErrorString, [_, value]) => *value != operand,
                    (RequireKind::CustomError(layout), [condition, selector, values @ ..]) => {
                        *condition == operand || *selector == operand
                            || values.iter().zip(layout.types.iter()).any(|(&value, ty)| value == operand && !Self::abi_type_reads_memory(ty))
                    }
                    _ => true,
                }
            }
            InstKind::AbiEncodePacked { parts, .. } => parts.iter().any(|part| {
                matches!(part, crate::mir::PackedPart::Scalar { value, .. } if *value == operand)
            }),
            InstKind::AbiEncode { args, layout, .. } => args
                .iter()
                .zip(layout.types.iter())
                .filter(|(arg, _)| **arg == operand)
                .any(|(_, ty)| !Self::abi_type_reads_memory(ty)),
            InstKind::AbiDecode { data, .. } => operand != *data,
            InstKind::ICall { function: Callee::Function(function), args, .. } => self
                .call_summaries
                .as_deref()
                .and_then(|summaries| summaries.get(*function))
                .is_none_or(|summary| {
                    args.iter().enumerate().any(|(index, &arg)| {
                        arg == operand && summary.captures_param(ArgIdx::new(index))
                    })
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
            (Location::Immutable(first), Location::Immutable(second)) => {
                if first == second {
                    AliasResult::MustAlias
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
        let kind = &func.inst(inst_id).kind;
        let resolve = |value| crate::mir::utils::resolve_replacement(value, replacements);
        let mut effects = ModRef::default();
        let scratch_words = match kind {
            InstKind::StorageBytesStore(..)
            | InstKind::StorageBytesStoreLiteral { .. }
            | InstKind::StorageClearWords(..)
            | InstKind::StorageArrayDataSlot(..)
            | InstKind::StorageArrayElementSlot { .. } => 1,
            InstKind::MappingSlot(..) => 2,
            _ => 0,
        };
        if scratch_words != 0 {
            effects.write(Access::Location(Location::Memory(MemoryLocation::new(
                MemoryAddress::absolute(0),
                LocationSize::Const(scratch_words * EvmMemoryLayout::WORD_SIZE),
            ))));
        }
        let read_memory = |effects: &mut ModRef, address, size| {
            if let Some(location) = self.access_extent(
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
        // A memory slice built in this function reads exactly `[ptr, ptr + len)`; other
        // locations read no memory.
        let read_memory_slice = |effects: &mut ModRef, slice| {
            if func.value_slice_location(slice) != Some(PtrLocation::Memory) {
                return;
            }
            if let Value::Inst(inst) = func.value(resolve(slice))
                && let InstKind::MakeSlice { ptr, len, .. } = func.inst(*inst).kind
            {
                read_memory(effects, ptr, SizeOperand::Value(len));
            } else {
                effects.read_any(AddressSpace::Memory);
            }
        };
        let write_memory = |effects: &mut ModRef, address, size| {
            if let Some(location) = self.access_extent(
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
            InstKind::Load(address, PtrLocation::Memory) => {
                read_memory(&mut effects, address, SizeOperand::Const(32));
            }
            InstKind::Store(address, _, PtrLocation::Memory) => {
                write_memory(&mut effects, address, SizeOperand::Const(32));
            }
            InstKind::Store8(address, _) => {
                write_memory(&mut effects, address, SizeOperand::Const(1));
            }
            InstKind::MemoryZero(address, size) => {
                write_memory(&mut effects, address, SizeOperand::Value(size));
            }
            InstKind::Fmp => {
                effects.read(Access::Location(Location::Memory(Self::fmp_location())));
            }
            InstKind::SetFmp(_) => {
                effects.write(Access::Location(Location::Memory(Self::fmp_location())));
            }
            InstKind::FrameLoad { offset, mode, kind } => {
                if let Some(location) = Self::frame_location(func, offset, mode, kind) {
                    effects.read(Access::Location(Location::Memory(location)));
                } else {
                    effects.read_any(AddressSpace::Memory);
                }
            }
            InstKind::FrameStore { offset, mode, kind, .. } => {
                if let Some(location) = Self::frame_location(func, offset, mode, kind) {
                    effects.write(Access::Location(Location::Memory(location)));
                } else {
                    effects.write_any(AddressSpace::Memory);
                }
            }
            InstKind::Alloc { size, semantics } => {
                let fmp = Access::Location(Location::Memory(Self::fmp_location()));
                effects.read(fmp);
                effects.write(fmp);
                if semantics.initialization == crate::mir::AllocationInitialization::Zeroed
                    && let Some(ptr) = func.inst_result_value(inst_id)
                {
                    // The initialization writes exactly the allocation's own extent. Keep that
                    // identity even for a dynamic size; arbitrary stores still require bounds.
                    if let Some(address) = self.memory_address(func, resolve(ptr)) {
                        let size = self.resolved_location_size(
                            func,
                            SizeOperand::Value(size),
                            replacements,
                        );
                        let location = MemoryLocation::new(address, size);
                        effects.write(Access::Location(Location::Memory(location)));
                    } else {
                        effects.write_any(AddressSpace::Memory);
                    }
                }
            }
            InstKind::StorageBytesLoad(..) | InstKind::StorageArrayLoad { .. } => {
                effects.read_any(AddressSpace::Storage);
                effects.read_any(AddressSpace::Memory);
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::Erc7201), .. }
            | InstKind::AbiEncodePacked { .. }
            | InstKind::ICall {
                function:
                    Callee::Builtin(
                        Builtin::Sha256
                        | Builtin::Ripemd160
                        | Builtin::EcRecover
                        | Builtin::ReturndataBytes,
                    ),
                ..
            } => {
                effects.read_any(AddressSpace::Memory);
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::ICall { function: Callee::Builtin(Builtin::Require(_)), .. } => {
                let InstKind::ICall { function: Callee::Builtin(Builtin::Require(kind)), args } =
                    kind
                else {
                    unreachable!()
                };
                match (kind, args.as_ref()) {
                    (RequireKind::ErrorString, [_, value]) => {
                        read_memory(&mut effects, *value, SizeOperand::Unknown);
                        // The revert path encodes the string at the free-memory pointer.
                        effects.read(Access::Location(Location::Memory(Self::fmp_location())));
                    }
                    (RequireKind::CustomError(layout), _)
                        if layout.types.iter().any(Self::abi_type_reads_memory) =>
                    {
                        // Aggregate payloads can follow references into distinct child objects.
                        effects.read_any(AddressSpace::Memory);
                    }
                    (RequireKind::CustomError(layout), [_, selector, values @ ..])
                        if func.value_u256(*selector).is_none()
                            || values.len() > MAX_SCRATCH_ERROR_WORDS
                            || !layout.types.iter().all(|ty| matches!(ty, AbiType::Word(_))) =>
                    {
                        // The revert path encodes the payload at the free-memory pointer.
                        effects.read(Access::Location(Location::Memory(Self::fmp_location())));
                    }
                    _ => {}
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
            InstKind::AbiDecode { data, .. } => {
                if func.value_slice_location(data).is_some() {
                    read_memory_slice(&mut effects, data);
                } else {
                    read_memory(&mut effects, data, SizeOperand::Unknown);
                }
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::StorageBytesStore(_, object) => {
                effects.read_any(AddressSpace::Storage);
                effects.write_any(AddressSpace::Storage);
                read_memory(&mut effects, object, SizeOperand::Unknown);
            }
            InstKind::StorageBytesStoreLiteral { .. } => {
                effects.read_any(AddressSpace::Storage);
                effects.write_any(AddressSpace::Storage);
            }
            InstKind::StorageClearWords(..) => effects.write_any(AddressSpace::Storage),
            InstKind::Copy(dest, source, size, PtrLocation::Memory) => {
                read_memory(&mut effects, source, SizeOperand::Value(size));
                write_memory(&mut effects, dest, SizeOperand::Value(size));
            }
            InstKind::Copy(dest, _, size, PtrLocation::Calldata | PtrLocation::Returndata)
            | InstKind::DataCopy(_, dest, size)
            | InstKind::CodeCopy(dest, _, size) => {
                write_memory(&mut effects, dest, SizeOperand::Value(size));
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                write_memory(&mut effects, dest, SizeOperand::Value(size));
            }
            InstKind::Keccak256(address, size) | InstKind::Log { offset: address, size, .. } => {
                read_memory(&mut effects, address, SizeOperand::Value(size));
            }
            InstKind::MappingSlotBytes(key, _) => {
                if func.value_ty(key) == Some(MirType::MEMPTR) {
                    read_memory(&mut effects, key, SizeOperand::Unknown);
                }
                effects.read(Access::Location(Location::Memory(Self::fmp_location())));
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::MSize => effects.read_any(AddressSpace::Memory),
            InstKind::Load(slot, PtrLocation::Storage) => {
                effects.read(Access::Location(Location::Storage(
                    self.storage_alias_after_replacements(func, inst_id, slot, replacements),
                )))
            }
            InstKind::Store(slot, _, PtrLocation::Storage) => {
                effects.write(Access::Location(Location::Storage(
                    self.storage_alias_after_replacements(func, inst_id, slot, replacements),
                )))
            }
            InstKind::Load(slot, PtrLocation::Transient) => {
                effects.read(Access::Location(Location::Transient(
                    self.storage_alias_after_replacements(func, inst_id, slot, replacements),
                )))
            }
            InstKind::Store(slot, _, PtrLocation::Transient) => {
                effects.write(Access::Location(Location::Transient(
                    self.storage_alias_after_replacements(func, inst_id, slot, replacements),
                )))
            }
            InstKind::LoadImmutable(id) => {
                effects.read(Access::Location(Location::Immutable(id)));
            }
            InstKind::StoreImmutable(id, _) => {
                effects.write(Access::Location(Location::Immutable(id)));
            }
            InstKind::AddressCall { kind, .. } => {
                effects.read_any(AddressSpace::Memory);
                effects.read_any(AddressSpace::Storage);
                effects.read_any(AddressSpace::Transient);
                if kind != AddressCallKind::Static {
                    effects.write_any(AddressSpace::Storage);
                    effects.write_any(AddressSpace::Transient);
                }
            }
            InstKind::ICall {
                function: Callee::Builtin(Builtin::Send | Builtin::Transfer),
                ..
            } => {
                effects.read_any(AddressSpace::Storage);
                effects.write_any(AddressSpace::Storage);
                effects.read_any(AddressSpace::Transient);
                effects.write_any(AddressSpace::Transient);
            }
            InstKind::Call { args_offset, args_size, ret_offset, ret_size, .. }
            | InstKind::CallCode { args_offset, args_size, ret_offset, ret_size, .. }
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
            InstKind::ICall { function: Callee::Function(function), .. } => {
                let has_multiple_returns = self
                    .call_summaries
                    .as_deref()
                    .and_then(|summaries| summaries.get(function))
                    .is_none_or(|summary| summary.has_multiple_returns);
                // MIR names only result 0; a multi-result call publishes its tail
                // results through a caller-side buffer written by backend lowering.
                // That write has no MIR representation, so even a memory-clean
                // callee must count as a memory writer or later loads of the
                // buffer could be reused across a second call.
                if has_multiple_returns {
                    effects.read_any(AddressSpace::Memory);
                    effects.write_any(AddressSpace::Memory);
                }
                if let Some(summary) =
                    self.call_summaries.as_deref().and_then(|summaries| summaries.get(function))
                {
                    effects.observes_gas = summary.may_observe_gas();
                    let args =
                        kind.operands().into_iter().map(resolve).collect::<SmallVec<[_; 8]>>();
                    effects.merge_call_summary(summary, func, self, &args);
                } else {
                    effects.observes_gas = true;
                    effects.read_any(AddressSpace::Memory);
                    effects.write_any(AddressSpace::Memory);
                    effects.read_any(AddressSpace::Storage);
                    effects.write_any(AddressSpace::Storage);
                    effects.read_any(AddressSpace::Transient);
                    effects.write_any(AddressSpace::Transient);
                    effects.read_any(AddressSpace::Immutable);
                    effects.write_any(AddressSpace::Immutable);
                }

                // Additional returns are copied into a fresh buffer and its
                // pointer is published through the reserved scratch word.
                // The backend performs that store after the MIR call, so
                // model it here even when the callee itself is memory-pure.
                if has_multiple_returns {
                    effects.write(Access::Location(Location::Memory(MemoryLocation::new(
                        MemoryAddress::absolute(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT),
                        LocationSize::Const(EvmMemoryLayout::WORD_SIZE),
                    ))));
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
        effects.observes_gas |= kind.observes_gas();
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
                    effects.observes_gas = summary.may_observe_gas();
                    effects.merge_call_summary(summary, func, self, &terminator.operands());
                } else {
                    effects.observes_gas = true;
                    effects.read_any(AddressSpace::Memory);
                    effects.write_any(AddressSpace::Memory);
                    effects.read_any(AddressSpace::Storage);
                    effects.write_any(AddressSpace::Storage);
                    effects.read_any(AddressSpace::Transient);
                    effects.write_any(AddressSpace::Transient);
                    effects.read_any(AddressSpace::Immutable);
                    effects.write_any(AddressSpace::Immutable);
                }
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Return { .. }
            | Terminator::RevertReturndata
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

    /// Returns the physical memory range occupied by a logical frame slot.
    #[must_use]
    pub(crate) fn frame_location(
        func: &Function,
        offset: u64,
        mode: FrameMode,
        kind: FrameSlotKind,
    ) -> Option<MemoryLocation> {
        let address = match mode {
            FrameMode::External => MemoryAddress {
                region: MemoryRegion::Scratch,
                base: MemoryBase::Absolute,
                offset: EvmMemoryLayout::HEAP_START.checked_add(offset)?,
            },
            FrameMode::Internal => {
                let args = (func.params.len() as u64).checked_mul(EvmMemoryLayout::WORD_SIZE)?;
                let returns = (func.return_components().len() as u64)
                    .checked_mul(EvmMemoryLayout::WORD_SIZE)?;
                let offset = EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE
                    .checked_add(args)?
                    .checked_add(returns)?
                    .checked_add(offset)?;
                MemoryAddress::internal_frame(offset)
            }
            FrameMode::MultiReturn => MemoryAddress {
                region: MemoryRegion::Scratch,
                base: MemoryBase::Absolute,
                offset: EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT,
            },
        };
        let size = match kind {
            FrameSlotKind::Word => EvmMemoryLayout::WORD_SIZE,
            FrameSlotKind::Slice(_) => EvmMemoryLayout::WORD_SIZE * 2,
        };
        Some(MemoryLocation::new(address, LocationSize::Const(size)))
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
        // Region labels describe layout, not ownership. Raw addresses and recycled
        // allocations can overlap scratch, heap, or frame memory.
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
                // One allocation site that runs more than once gives each
                // instance its own region, so two accesses to different
                // instances never overlap and two accesses to one instance
                // overlap exactly when their offsets do. Disjoint offsets are
                // therefore disjoint either way, which the comparison below
                // establishes; an overlap only means they *may* alias, because
                // the accesses may belong to different instances.
                if a_dynamic || b_dynamic {
                    return match Self::compare_offsets(first, second) {
                        AliasResult::NoAlias => AliasResult::NoAlias,
                        _ => AliasResult::MayAlias,
                    };
                }
                // Same unique static allocation: compare offsets below.
            }
            (Some((_, true)), _) | (_, Some((_, true))) => return AliasResult::MayAlias,
            _ => {}
        }
        if first.address.base != second.address.base {
            return AliasResult::MayAlias;
        }
        Self::compare_offsets(first, second)
    }

    /// Compares two ranges that share a base, by offset and width.
    fn compare_offsets(first: MemoryLocation, second: MemoryLocation) -> AliasResult {
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
            // A location of unknown extent still starts at its offset, so a
            // bounded access that ends at or before that start is disjoint.
            (LocationSize::Const(first_size), LocationSize::Dynamic(_) | LocationSize::Unknown) => {
                match first.address.offset.checked_add(first_size) {
                    Some(first_end) if first_end <= second.address.offset => AliasResult::NoAlias,
                    _ => AliasResult::MayAlias,
                }
            }
            (
                LocationSize::Dynamic(_) | LocationSize::Unknown,
                LocationSize::Const(second_size),
            ) => match second.address.offset.checked_add(second_size) {
                Some(second_end) if second_end <= first.address.offset => AliasResult::NoAlias,
                _ => AliasResult::MayAlias,
            },
            _ => AliasResult::MayAlias,
        }
    }

    fn memory_address_with_depth(
        &self,
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        let provenance = self.provenance(func);
        {
            let mut addresses = provenance.addresses.borrow_mut();
            if addresses.len() <= value.index() {
                addresses
                    .resize(func.num_values().max(value.index() + 1), AddressState::Unresolved);
            }
            match addresses[value] {
                AddressState::Resolved(address) => return address,
                AddressState::Unresolved if depth <= 8 => addresses[value] = AddressState::Visiting,
                AddressState::Unresolved | AddressState::Visiting => {
                    drop(addresses);
                    return Some(MemoryAddress::symbolic(
                        value,
                        self.pointer_region(func, value, 0),
                    ));
                }
            }
        }
        let address = (|| match func.value(value) {
            Value::Immediate(immediate) => {
                Some(MemoryAddress::absolute(immediate.as_u256()?.try_into().ok()?))
            }
            // An opaque pointer argument implies no allocation origin or region.
            Value::Arg(_) => Some(MemoryAddress::symbolic(value, MemoryRegion::Unknown)),
            Value::Undef(_) | Value::Error(_) => None,
            Value::Inst(inst_id) => match func.inst(*inst_id).kind {
                InstKind::InternalFrameAddr(offset) => Some(MemoryAddress::internal_frame(offset)),
                InstKind::Alloc { .. } => Some(if self.allocation_is_dynamic(func, *inst_id) {
                    MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::DynamicAllocation(*inst_id),
                        offset: 0,
                    }
                } else if self.allocation_has_unique_provenance(func, *inst_id) {
                    MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::Allocation(*inst_id),
                        offset: 0,
                    }
                } else {
                    MemoryAddress::symbolic(value, MemoryRegion::Heap)
                }),
                InstKind::Add(first, second)
                    if (super::integers::integer_bits(func, value) == 256
                        || super::integers::arithmetic_no_wrap(func, value)) =>
                {
                    self.address_add(func, first, second, depth)
                        .or_else(|| self.address_add(func, second, first, depth))
                        .or_else(|| {
                            Some(MemoryAddress::symbolic(
                                value,
                                self.pointer_region(func, value, 0),
                            ))
                        })
                }
                InstKind::PtrAdd(base, offset) => {
                    self.address_add(func, base, offset, depth).or_else(|| {
                        Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)))
                    })
                }
                InstKind::Sub(base, offset)
                    if (super::integers::integer_bits(func, value) == 256
                        || super::integers::arithmetic_no_wrap(func, value)) =>
                {
                    self.address_sub(func, base, offset, depth).or_else(|| {
                        Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)))
                    })
                }
                InstKind::IntToPtr(ptr) | InstKind::PtrToInt(ptr, 256) => {
                    self.memory_address_with_depth(func, ptr, depth + 1)
                }
                InstKind::ExtractValue {
                    ty: MirType::Slice(_),
                    aggregate: slice,
                    index: MirType::SLICE_PTR_FIELD,
                } => self.slice_pointer_address(func, slice, depth),
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
        provenance.addresses.borrow_mut()[value] = AddressState::Resolved(address);
        address
    }

    fn address_add(
        &self,
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        if let Value::Inst(inst) = func.value(offset)
            && let InstKind::Sub(value, subtrahend) = func.inst(*inst).kind
            && subtrahend == base
        {
            return self.memory_address_with_depth(func, value, depth + 1);
        }
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
        match &func.inst(*inst_id).kind {
            InstKind::MakeSlice { ptr, location, .. } => match location {
                PtrLocation::Memory => self.memory_address_with_depth(func, *ptr, depth + 1),
                // Pointers into other locations index their own address
                // spaces, not memory, so they carry no memory provenance.
                _ => None,
            },
            InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. } => {
                Some(if self.allocation_is_dynamic(func, *inst_id) {
                    MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::DynamicAllocation(*inst_id),
                        offset: 0,
                    }
                } else if self.allocation_has_unique_provenance(func, *inst_id) {
                    MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::Allocation(*inst_id),
                        offset: 0,
                    }
                } else {
                    MemoryAddress::symbolic(slice, MemoryRegion::Heap)
                })
            }
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
            return MemoryRegion::Unknown;
        };
        match func.inst(*inst_id).kind {
            InstKind::InternalFrameAddr(_) => MemoryRegion::InternalFrame,
            InstKind::Fmp | InstKind::Alloc { .. } => MemoryRegion::Heap,
            InstKind::Load(address, PtrLocation::Memory)
                if func.value_u64(address) == Some(EvmMemoryLayout::FMP_SLOT) =>
            {
                MemoryRegion::Heap
            }
            InstKind::Add(first, second)
                if (super::integers::integer_bits(func, value) == 256
                    || super::integers::arithmetic_no_wrap(func, value)) =>
            {
                let first = self.pointer_region(func, first, depth + 1);
                if first != MemoryRegion::Unknown {
                    first
                } else {
                    self.pointer_region(func, second, depth + 1)
                }
            }
            InstKind::Sub(base, offset) if Self::sub_keeps_pointer_region(func, offset) => {
                self.pointer_region(func, base, depth + 1)
            }
            InstKind::IntToPtr(base)
            | InstKind::PtrToInt(base, 256)
            | InstKind::PtrAdd(base, _) => self.pointer_region(func, base, depth + 1),
            InstKind::Phi(ref incoming) => {
                self.join_pointer_regions(func, incoming.iter().map(|(_, value)| *value), depth)
            }
            InstKind::Select(_, first, second) => {
                self.join_pointer_regions(func, [first, second], depth)
            }
            InstKind::ExtractValue {
                ty: MirType::Slice(_),
                aggregate: slice,
                index: MirType::SLICE_PTR_FIELD,
            } => {
                let Value::Inst(slice_inst) = func.value(slice) else {
                    return MemoryRegion::Unknown;
                };
                match &func.inst(*slice_inst).kind {
                    InstKind::MakeSlice { location: PtrLocation::Memory, .. }
                    | InstKind::AbiEncode { .. } => MemoryRegion::Heap,
                    _ => MemoryRegion::Unknown,
                }
            }
            _ => MemoryRegion::Unknown,
        }
    }

    /// Whether `sub(pointer, offset)` stays in the pointer's region: a constant
    /// step back of at most one word, such as from a dynamic object's data to
    /// its length word. Any other subtrahend can move the result anywhere, as
    /// in `sub(p, sub(p, x)) == x`.
    #[must_use]
    pub(crate) fn sub_keeps_pointer_region(func: &Function, offset: ValueId) -> bool {
        func.value_u64(offset).is_some_and(|offset| offset <= EvmMemoryLayout::WORD_SIZE)
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

    fn allocation_has_unique_provenance(&self, func: &Function, target: InstId) -> bool {
        self.provenance(func).allocations.get(&target).is_some_and(|facts| facts.unique)
    }

    fn allocation_is_dynamic(&self, func: &Function, target: InstId) -> bool {
        self.provenance(func).allocations.get(&target).is_some_and(|facts| facts.dynamic)
    }

    /// Returns whether an instruction may recycle or arbitrarily replace the FMP.
    #[must_use]
    pub(crate) fn instruction_may_reset_fmp(&self, func: &Function, inst: InstId) -> bool {
        Self::instruction_may_reset_fmp_with_summaries(func, inst, self.call_summaries.as_deref())
    }

    /// Returns whether an instruction may write the free-memory-pointer word.
    ///
    /// Unlike [`Self::instruction_may_reset_fmp`], this also counts allocations that bump the
    /// word, so a cached copy of it stays valid across an instruction only when this is false. A
    /// heap pointer plus a constant never reaches the word, so such writes do not count.
    #[must_use]
    pub(crate) fn instruction_may_write_fmp(&self, func: &Function, inst: InstId) -> bool {
        self.instruction_mod_ref(func, inst).writes().iter().any(|&access| match access {
            Access::Any(AddressSpace::Memory) => true,
            Access::Location(Location::Memory(location)) => {
                Self::location_may_overlap_fmp(func, location)
            }
            Access::Any(_) | Access::Location(_) => false,
        })
    }

    /// Returns whether an instruction may terminate the current EVM call successfully.
    #[must_use]
    pub(crate) fn instruction_may_terminate(&self, func: &Function, inst: InstId) -> bool {
        match &func.inst(inst).kind {
            InstKind::ICall { function: Callee::Function(function), .. } => self
                .call_summaries
                .as_deref()
                .and_then(|summaries| summaries.get(*function))
                .is_none_or(|summary| summary.may_terminate()),
            kind => kind.effects().control.may_terminate,
        }
    }

    fn instruction_may_reset_fmp_with_summaries(
        func: &Function,
        inst: InstId,
        call_summaries: Option<&MemoryCallSummaries>,
    ) -> bool {
        match func.inst(inst).kind {
            InstKind::SetFmp(_) | InstKind::MappingSlotBytes(..) => true,
            InstKind::ICall { function: Callee::Function(function), .. } => call_summaries
                .and_then(|summaries| summaries.get(function))
                .is_none_or(|summary| summary.may_reset_fmp()),
            InstKind::Store(address, _, PtrLocation::Memory) => {
                Self::range_may_overlap_fmp(func, address, Some(32))
            }
            InstKind::Store8(address, _) => Self::range_may_overlap_fmp(func, address, Some(1)),
            InstKind::Copy(dest, _, size, _)
            | InstKind::MemoryZero(dest, size)
            | InstKind::DataCopy(_, dest, size)
            | InstKind::CodeCopy(dest, _, size) => {
                Self::range_may_overlap_fmp(func, dest, func.value_u64(size))
            }
            InstKind::ExtCodeCopy(_, dest, _, size) => {
                Self::range_may_overlap_fmp(func, dest, func.value_u64(size))
            }
            InstKind::Call { ret_offset, ret_size, .. }
            | InstKind::CallCode { ret_offset, ret_size, .. }
            | InstKind::StaticCall { ret_offset, ret_size, .. }
            | InstKind::DelegateCall { ret_offset, ret_size, .. } => {
                Self::range_may_overlap_fmp(func, ret_offset, func.value_u64(ret_size))
            }
            _ => false,
        }
    }

    /// Returns whether a physical memory range may touch the reserved FMP word.
    pub(crate) fn range_may_overlap_fmp(
        func: &Function,
        address: ValueId,
        size: Option<u64>,
    ) -> bool {
        if size == Some(0) {
            return false;
        }
        if let Some(address) = func.value_u64(address) {
            return Self::const_range_may_overlap_fmp(address, size);
        }
        Self::pointer_lower_bound(func, address)
            .is_none_or(|address| address < EvmMemoryLayout::HEAP_START)
    }

    /// Returns whether a canonical memory location may touch the reserved FMP word.
    fn location_may_overlap_fmp(func: &Function, location: MemoryLocation) -> bool {
        let MemoryLocation { address, size } = location;
        if size == LocationSize::Const(0) {
            return false;
        }
        if let Some(address) = address.as_absolute() {
            return Self::const_range_may_overlap_fmp(address, size.as_const());
        }
        Self::address_lower_bound(func, address, size)
            .is_none_or(|bound| bound < EvmMemoryLayout::HEAP_START)
    }

    /// Returns a conservative lower bound for the first byte of an access of `size` at a
    /// canonical address, which holds even when a runtime index wraps around.
    ///
    /// An access of unknown extent through a non-absolute base is a runtime-indexed `ptradd`
    /// whose index the canonical address discards, so it has no bound.
    pub(crate) fn address_lower_bound(
        func: &Function,
        address: MemoryAddress,
        size: LocationSize,
    ) -> Option<u64> {
        if size == LocationSize::Unknown && address.as_absolute().is_none() {
            return None;
        }
        match address.base {
            MemoryBase::Absolute => Some(address.offset),
            // An allocation is a heap pointer.
            MemoryBase::Allocation(_) | MemoryBase::DynamicAllocation(_) => {
                EvmMemoryLayout::HEAP_START.checked_add(address.offset)
            }
            MemoryBase::Value(base) => {
                Self::read_lower_bound(func, base)?.checked_add(address.offset)
            }
            MemoryBase::InternalFrame => None,
        }
    }

    fn const_range_may_overlap_fmp(address: u64, size: Option<u64>) -> bool {
        let Some(size) = size else { return true };
        let Some(end) = address.checked_add(size) else { return true };
        let fmp_end = EvmMemoryLayout::FMP_SLOT + EvmMemoryLayout::WORD_SIZE;
        address < fmp_end && end > EvmMemoryLayout::FMP_SLOT
    }

    /// Returns a conservative lower bound for a compiler-owned pointer, trusting that `ptradd`
    /// never wraps.
    pub(crate) fn pointer_lower_bound(func: &Function, value: ValueId) -> Option<u64> {
        Self::lower_bound(func, value, 0, true)
    }

    /// Returns a lower bound for a pointer that also holds when assembly forges the free-memory
    /// pointer or a runtime `ptradd` offset wraps, for deciding which slots a read may observe.
    pub(crate) fn read_lower_bound(func: &Function, value: ValueId) -> Option<u64> {
        Self::lower_bound(func, value, 0, false)
    }

    /// Returns a conservative lower bound for a compiler-owned pointer.
    ///
    /// This deliberately does not trust a coarse `Heap` region: inline MIR can
    /// subtract from a pointer, and a typed argument may still originate in
    /// opaque code. Only monotonic derivations from compiler-owned bases prove
    /// that a write cannot reach reserved low memory.
    fn lower_bound(func: &Function, value: ValueId, depth: usize, trusted: bool) -> Option<u64> {
        if depth > 8 {
            return None;
        }
        if let Some(value) = func.value_u64(value) {
            return Some(value);
        }
        let Value::Inst(inst) = func.value(value) else { return None };
        match &func.inst(*inst).kind {
            InstKind::Fmp | InstKind::Alloc { .. } => Some(EvmMemoryLayout::HEAP_START),
            // A decoded scalar is calldata the caller controls, not a pointer.
            InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. } => func
                .value_ty(value)
                .is_some_and(MirType::is_memory_reference)
                .then_some(EvmMemoryLayout::HEAP_START),
            // Assembly can store any word in the slot, so only the allocator's own read of it is
            // a heap pointer when `trusted` is false.
            InstKind::Load(address, PtrLocation::Memory)
                if func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT)
                    && (trusted || func.inst(*inst).metadata.alloc_ptr()) =>
            {
                Some(EvmMemoryLayout::HEAP_START)
            }
            InstKind::PtrToInt(value, 256) | InstKind::IntToPtr(value) => {
                Self::lower_bound(func, *value, depth + 1, trusted)
            }
            InstKind::InternalFrameAddr(offset) => EvmMemoryLayout::HEAP_START.checked_add(*offset),
            InstKind::Add(first, second)
                if (super::integers::integer_bits(func, value) == 256
                    || super::integers::arithmetic_no_wrap(func, value)) =>
            {
                Self::lower_bound(func, *first, depth + 1, trusted)
                    .and_then(|base| base.checked_add(func.value_u64(*second)?))
                    .or_else(|| {
                        Self::lower_bound(func, *second, depth + 1, trusted)
                            .and_then(|base| base.checked_add(func.value_u64(*first)?))
                    })
                    .or_else(|| {
                        // The allocator's bump `p + size` is not below `p`: allocation proved or
                        // checked that it does not wrap. A sum the program computes, such as an
                        // index, may wrap, so it keeps no bound.
                        func.inst(*inst)
                            .metadata
                            .alloc_ptr()
                            .then(|| Self::lower_bound(func, *first, depth + 1, trusted))?
                    })
            }
            InstKind::Sub(base, offset)
                if (super::integers::integer_bits(func, value) == 256
                    || super::integers::arithmetic_no_wrap(func, value)) =>
            {
                Self::lower_bound(func, *base, depth + 1, trusted)
                    .and_then(|base| base.checked_sub(func.value_u64(*offset)?))
            }
            // `ptradd` does not wrap, so a runtime offset only moves the pointer up. A program
            // that forges the length an index is checked against can still wrap it around, so
            // `trusted` is false for reads, which must not miss a slot below the heap.
            InstKind::PtrAdd(base, offset) => {
                let offset = match func.value_u64(*offset) {
                    Some(offset) => offset,
                    None if trusted => 0,
                    None => return None,
                };
                Self::lower_bound(func, *base, depth + 1, trusted)?.checked_add(offset)
            }
            InstKind::ExtractValue {
                ty: MirType::Slice(_),
                aggregate: slice,
                index: MirType::SLICE_PTR_FIELD,
            } => {
                let Value::Inst(slice) = func.value(*slice) else { return None };
                match &func.inst(*slice).kind {
                    InstKind::MakeSlice { ptr, location: PtrLocation::Memory, .. } => {
                        Self::lower_bound(func, *ptr, depth + 1, trusted)
                    }
                    InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. } => {
                        Some(EvmMemoryLayout::HEAP_START)
                    }
                    _ => None,
                }
            }
            InstKind::Phi(incoming) => incoming
                .iter()
                .map(|(_, value)| Self::lower_bound(func, *value, depth + 1, trusted))
                .collect::<Option<Vec<_>>>()?
                .into_iter()
                .min(),
            InstKind::Select(_, first, second) => {
                Some(Self::lower_bound(func, *first, depth + 1, trusted)?.min(Self::lower_bound(
                    func,
                    *second,
                    depth + 1,
                    trusted,
                )?))
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
            AbiType::Word(_) | AbiType::Function => false,
            // Calldata and returndata ABI values read their own buffers, not
            // memory (returndata does not occur as an ABI type in practice).
            AbiType::Bytes(location) | AbiType::DynamicArray { location, .. } => {
                *location == PtrLocation::Memory
            }
            AbiType::FixedArray { .. } | AbiType::Tuple(_) => true,
        }
    }
}

#[derive(Clone, Copy)]
enum SizeOperand {
    Const(u64),
    Value(ValueId),
    Unknown,
}

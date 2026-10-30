//! Shared MIR alias and ModRef analysis.
//!
//! Pointer provenance follows SSA definitions through address arithmetic,
//! slices, selects, and control-flow joins. Fresh allocations retain unique
//! identities, while incompatible incoming paths conservatively join to a
//! symbolic pointer. Region labels do not prove disjointness: raw pointer operations
//! and FMP recycling can cross those regions. The analysis exposes the memory,
//! storage, and transient-storage effects of each instruction. Runtime pointer offsets retain
//! their symbolic value: a forged array length can admit indices that wrap into the object's
//! header, another allocation, or reserved memory.
//!
//! An access keeps its allocation's identity while it stays inside the object: within the
//! allocation's constant size or the first word of a checked allocation, which reserves a
//! header. Other accesses may leave the object and keep no disjointness from other objects,
//! but constant offsets still separate accesses to the same pointer. Runtime indices retain
//! their symbolic values, since forged lengths can admit offsets that wrap onto the header.
//! An access at the end of an object's payload, `p + 32 + len`, starts past its length word
//! when `len` is provably below `2^255` and an earlier length store bounds `p`. Such an access
//! may leave the object but cannot wrap around onto its length word.

use super::{CfgInfo, DominatorTree, MemoryCallSummaries};
use crate::mir::{
    AbiType, AddressCallKind, AllocationFailure, ArgIdx, BlockId, Builtin, Callee, FrameMode,
    FrameSlotKind, Function, FunctionId, ImmutableId, InstId, InstKind, MAX_SCRATCH_ERROR_WORDS,
    MemoryRegion, MirType, PtrLocation, RawMemoryAccess, RawMemorySize, RequireKind, StorageAlias,
    Terminator, Value, ValueId, memory::EvmMemoryLayout,
};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_data_structures::{
    bit_set::DenseBitSet,
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use std::{
    cell::{Cell, OnceCell, RefCell},
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

impl AddressSpace {
    /// Every address space.
    pub(crate) const ALL: [Self; 4] =
        [Self::Memory, Self::Storage, Self::Transient, Self::Immutable];

    /// The spaces of storage slots.
    pub(crate) const SLOTS: [Self; 2] = [Self::Storage, Self::Transient];

    /// Returns the position of a slot space in [`Self::SLOTS`], or `None` for another space.
    #[must_use]
    pub(crate) const fn slot_index(self) -> Option<usize> {
        match self {
            Self::Storage => Some(0),
            Self::Transient => Some(1),
            Self::Memory | Self::Immutable => None,
        }
    }

    /// Returns the slot `alias` names in this space, or `None` for a space without slots.
    #[must_use]
    pub(crate) const fn slot(self, alias: StorageAlias) -> Option<Location> {
        match self {
            Self::Storage => Some(Location::Storage(alias)),
            Self::Transient => Some(Location::Transient(alias)),
            Self::Memory | Self::Immutable => None,
        }
    }
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
    /// An access to one fresh heap allocation that may leave its bounds. It keeps no
    /// disjointness from other objects, but shares its pointer with the allocation's other
    /// accesses, so offsets still separate it from them.
    UnboundedAllocation(InstId),
    /// An unrecycled allocation site with multiple dynamic loop instances.
    DynamicAllocation(InstId),
    /// A symbolic MIR value.
    Value(ValueId),
    /// The result of one free-memory-pointer read that no reset of the pointer can precede.
    FreeMemory(InstId),
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
            | MemoryBase::UnboundedAllocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::Value(_)
            | MemoryBase::FreeMemory(_) => None,
        }
    }

    /// Returns the internal-frame byte offset, if known.
    #[must_use]
    pub(crate) const fn as_internal_frame_offset(self) -> Option<u64> {
        match self.base {
            MemoryBase::InternalFrame => Some(self.offset),
            MemoryBase::Absolute
            | MemoryBase::Allocation(_)
            | MemoryBase::UnboundedAllocation(_)
            | MemoryBase::DynamicAllocation(_)
            | MemoryBase::Value(_)
            | MemoryBase::FreeMemory(_) => None,
        }
    }

    /// Returns whether this address is the base of a fresh allocation of constant size.
    pub(crate) fn is_fixed_allocation_base(self, func: &Function) -> bool {
        self.offset == 0
            && matches!(self.base, MemoryBase::Allocation(inst)
                if AliasAnalysis::allocation_size(func, inst).is_some())
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

    /// Returns the slot a storage or transient-storage location names.
    #[must_use]
    pub(crate) const fn slot_alias(self) -> Option<StorageAlias> {
        match self {
            Self::Storage(alias) | Self::Transient(alias) => Some(alias),
            Self::Memory(_) | Self::Immutable(_) => None,
        }
    }

    /// Returns the slot `alias` names in storage or transient storage `location`.
    #[must_use]
    pub(crate) const fn slot(location: PtrLocation, alias: StorageAlias) -> Option<Self> {
        match location {
            PtrLocation::Storage => AddressSpace::Storage.slot(alias),
            PtrLocation::Transient => AddressSpace::Transient.slot(alias),
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
        for space in AddressSpace::ALL {
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
                        if let Some(location) = space.slot(StorageAlias::Slot(slot)) {
                            record(Access::Location(location));
                        }
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

    fn read_write_any(&mut self, spaces: impl IntoIterator<Item = AddressSpace>) {
        for space in spaces {
            self.read_any(space);
            self.write_any(space);
        }
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

/// The order of unique allocations relative to the free-memory-pointer reads and unique
/// allocations that neither a reset nor a wrapped write precedes.
///
/// No path from the entry to such a read or `alloc` `P` resets the pointer or may write through a
/// wrapped index, so every unique `alloc` that executes before `P` ends at or below the pointer `P`
/// returns, and an access at a nonnegative offset from `P` cannot reach it.
#[derive(Clone, Debug)]
struct FreshFmps {
    dominators: DominatorTree,
    /// The block and index of each unique `alloc` and each fresh free-memory-pointer read.
    positions: FxHashMap<InstId, (BlockId, usize)>,
    /// The unique `alloc`s, whose runtime-indexed accesses wrap and so keep no such order.
    allocs: FxHashSet<InstId>,
    /// Whether the function writes memory through a runtime index, which wraps and may rewind the
    /// free-memory pointer under the freshness of its allocations.
    wraps: bool,
}

impl FreshFmps {
    /// Returns whether one base is a unique allocation that precedes the instruction the other
    /// names.
    fn separates(&self, first: MemoryLocation, second: MemoryLocation) -> bool {
        let (alloc, fresh, sizes) = match (first.address.base, second.address.base) {
            (
                MemoryBase::Allocation(alloc),
                MemoryBase::FreeMemory(fresh) | MemoryBase::UnboundedAllocation(fresh),
            ) => (alloc, fresh, [first.size, second.size]),
            (
                MemoryBase::FreeMemory(fresh) | MemoryBase::UnboundedAllocation(fresh),
                MemoryBase::Allocation(alloc),
            ) => (alloc, fresh, [second.size, first.size]),
            _ => return false,
        };
        // A runtime index wraps, so an indexed access, to either allocation, may land below it.
        (!self.allocs.contains(&fresh) || !self.wraps && !sizes.contains(&LocationSize::Unknown))
            && self.precedes(alloc, fresh)
    }

    /// Returns whether `alloc` executes before `fresh` on every path.
    fn precedes(&self, alloc: InstId, fresh: InstId) -> bool {
        let (Some(&(alloc_block, alloc_index)), Some(&(fresh_block, fresh_index))) =
            (self.positions.get(&alloc), self.positions.get(&fresh))
        else {
            return false;
        };
        if alloc_block == fresh_block {
            alloc_index < fresh_index
        } else {
            self.dominators.dominates(alloc_block, fresh_block)
        }
    }
}

#[derive(Clone, Debug, Default)]
struct PointerProvenance {
    allocations: FxHashMap<InstId, AllocationProvenance>,
    /// Present when some unique `alloc` exists.
    fresh_fmps: Option<FreshFmps>,
    /// Grows on demand, since transforms may add values after construction.
    addresses: RefCell<IndexVec<ValueId, AddressState>>,
}

impl PointerProvenance {
    fn new(
        func: &Function,
        call_summaries: Option<&MemoryCallSummaries>,
        may_reset_fmp: impl Fn(&Function, InstId, Option<&MemoryCallSummaries>) -> bool,
        may_wrap_write: impl Fn(&Function, InstId, Option<&MemoryCallSummaries>) -> bool,
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
        let proofs = ReadBounds::default();
        // The index of each block's first instruction that may reset the FMP.
        let first_resets = func
            .blocks
            .iter()
            .map(|block| {
                block.instructions.iter().position(|&inst| {
                    may_reset_fmp(func, inst, call_summaries)
                        && !proofs.raw_writes_avoid_fmp(func, inst)
                })
            })
            .collect::<IndexVec<BlockId, _>>();

        // `poisoned[block]` means at least one path into the block may have
        // recycled the FMP. The monotone OR lattice handles joins and loops.
        let poison = |first_resets: &IndexVec<BlockId, Option<usize>>| {
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
            (reachable, poisoned)
        };
        let (reachable, poisoned) = poison(&first_resets);
        // A write through a wrapped runtime index may land on the FMP word, so it ends the
        // freshness of the reads after it.
        // NOTE: Allocations keep their uniqueness, which assumes that an access based on an
        // allocation stays inside it.
        let first_wraps = func
            .blocks
            .iter()
            .map(|block| {
                block
                    .instructions
                    .iter()
                    .position(|&inst| may_wrap_write(func, inst, call_summaries))
            })
            .collect::<IndexVec<BlockId, _>>();
        let first_rewinds = first_resets
            .iter()
            .zip(&first_wraps)
            .map(|(&reset, &wrap)| reset.into_iter().chain(wrap).min())
            .collect::<IndexVec<BlockId, _>>();
        let read_poisoned = if first_rewinds.iter().any(Option::is_some) {
            poison(&first_rewinds).1
        } else {
            poisoned.clone()
        };

        let mut allocations = FxHashMap::default();
        // The unique `alloc`s and the free-memory-pointer reads that neither a reset nor a wrapped
        // write precedes.
        let mut positions = FxHashMap::default();
        let mut allocs = FxHashSet::default();
        for (block_id, block) in func.blocks.iter_enumerated() {
            let entry_fresh = reachable.contains(block_id) && !poisoned.contains(block_id);
            for (index, &inst_id) in block.instructions.iter().enumerate() {
                if is_allocation(inst_id) {
                    let fresh =
                        entry_fresh && first_resets[block_id].is_none_or(|first| index <= first);
                    let unique = fresh && !cyclic.contains(block_id);
                    allocations.insert(
                        inst_id,
                        AllocationProvenance { dynamic: fresh && !unique, unique },
                    );
                    if unique && matches!(func.inst(inst_id).kind, InstKind::Alloc { .. }) {
                        positions.insert(inst_id, (block_id, index));
                        allocs.insert(inst_id);
                    }
                } else if matches!(func.inst(inst_id).kind, InstKind::Fmp)
                    && reachable.contains(block_id)
                    && !read_poisoned.contains(block_id)
                    && first_rewinds[block_id].is_none_or(|first| index < first)
                {
                    positions.insert(inst_id, (block_id, index));
                }
            }
        }
        let wraps = !allocs.is_empty() && first_wraps.iter().any(Option::is_some);
        let fresh_fmps = (!allocs.is_empty()).then(|| FreshFmps {
            dominators: cfg.dominators().clone(),
            positions,
            allocs,
            wraps,
        });

        Self { allocations, fresh_fmps, addresses: RefCell::default() }
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
    /// Whether some access may run through a runtime index, which wraps and so may land on any
    /// object. Computed on first use.
    wraps: Cell<Option<bool>>,
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
            wraps: Cell::new(None),
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
                    && cached.fresh_fmps.as_ref().map(|f| &f.positions)
                        == fresh.fresh_fmps.as_ref().map(|f| &f.positions)
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
        self.wraps.set(None);
    }

    fn with_optional_summaries(
        _func: &Function,
        call_summaries: Option<Arc<MemoryCallSummaries>>,
    ) -> Self {
        Self {
            provenance: OnceCell::new(),
            escaping_values: RefCell::new(None),
            wraps: Cell::new(None),
            call_summaries,
        }
    }

    /// The lazily built provenance snapshot for `func`. Every query passes the
    /// same function the analysis was created for.
    fn provenance(&self, func: &Function) -> &PointerProvenance {
        self.provenance.get_or_init(|| {
            PointerProvenance::new(
                func,
                self.call_summaries.as_deref(),
                Self::instruction_may_reset_fmp_with_summaries,
                Self::instruction_may_wrap_write_with_summaries,
            )
        })
    }

    /// Returns whether some access of `func` may run through a runtime index, cached for the
    /// life of the snapshot.
    fn function_wraps(&self, func: &Function) -> bool {
        if let Some(wraps) = self.wraps.get() {
            return wraps;
        }
        // The scan resolves addresses, which may query here again; the pessimistic value answers.
        self.wraps.set(Some(true));
        let wraps = self.function_may_wrap(func);
        self.wraps.set(Some(wraps));
        wraps
    }

    /// Returns whether an access of `func` may run through a runtime index or has an unknown
    /// extent, so that it wraps and may land on any object.
    fn function_may_wrap(&self, func: &Function) -> bool {
        let wraps = |effects: ModRef| {
            effects.reads().iter().chain(effects.writes()).any(|access| match access {
                Access::Any(AddressSpace::Memory) => true,
                Access::Location(Location::Memory(location)) => {
                    location.size == LocationSize::Unknown
                }
                _ => false,
            })
        };
        let indexed = |address| Self::address_has_runtime_index(func, address, 8);
        let exact = |inst| {
            self.instruction_may_wrap_write(func, inst)
                || wraps(self.instruction_mod_ref(func, inst))
        };
        func.instructions().any(|inst| match func.inst(inst).kind {
            // Only an access through a runtime index, or at a heap pointer plus a bounded offset,
            // can have an unknown extent, and checking its address is cheaper than building its
            // ModRef.
            InstKind::Load(address, PtrLocation::Memory)
            | InstKind::Store(address, _, PtrLocation::Memory)
            | InstKind::Store8(address, _) => indexed(address) && exact(inst),
            InstKind::Load(..) | InstKind::Store(..) => false,
            InstKind::MemoryZero(address, size) if func.value_u64(size).is_some() => {
                indexed(address) && exact(inst)
            }
            InstKind::Alloc { size, .. } if func.value_u64(size).is_some() => false,
            InstKind::Fmp | InstKind::SetFmp(..) => false,
            ref kind if Self::is_value_computation(kind) => false,
            _ => exact(inst),
        }) || func
            .blocks
            .iter()
            .filter_map(|block| block.terminator.as_ref())
            .any(|terminator| wraps(self.terminator_mod_ref(func, terminator)))
    }

    /// Returns whether an instruction only computes a value, so it reads and writes no memory.
    fn is_value_computation(kind: &InstKind) -> bool {
        matches!(
            kind,
            InstKind::Zext(..)
                | InstKind::Trunc(..)
                | InstKind::Sext(..)
                | InstKind::PtrToInt(..)
                | InstKind::IntToPtr(..)
                | InstKind::PtrAdd(..)
                | InstKind::Add(..)
                | InstKind::Sub(..)
                | InstKind::Mul(..)
                | InstKind::Div(..)
                | InstKind::SDiv(..)
                | InstKind::Mod(..)
                | InstKind::SMod(..)
                | InstKind::AddMod(..)
                | InstKind::MulMod(..)
                | InstKind::And(..)
                | InstKind::Or(..)
                | InstKind::Xor(..)
                | InstKind::Not(..)
                | InstKind::Shl(..)
                | InstKind::Shr(..)
                | InstKind::Sar(..)
                | InstKind::Lt(..)
                | InstKind::Gt(..)
                | InstKind::SLt(..)
                | InstKind::SGt(..)
                | InstKind::Eq(..)
                | InstKind::Ne(..)
                | InstKind::Phi(..)
                | InstKind::Select(..)
        )
    }

    /// Canonicalizes a MIR value used as a memory address.
    #[must_use]
    pub(crate) fn memory_address(&self, func: &Function, value: ValueId) -> Option<MemoryAddress> {
        // These roots need neither control-flow provenance nor recursive address resolution.
        // Recursive operands still go through the resolver and its depth limit.
        match func.value(value) {
            Value::Immediate(immediate) => {
                Some(MemoryAddress::absolute(immediate.as_u256()?.try_into().ok()?))
            }
            Value::Arg(_) => Some(MemoryAddress::symbolic(value, MemoryRegion::Unknown)),
            Value::Undef(_) | Value::Error(_) => None,
            Value::Inst(_) => self.memory_address_with_depth(func, value, 0),
        }
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

    /// Returns the memory an access at `address` may touch. A proved payload-end address
    /// covers everything from the first payload byte upward; other addresses retain their
    /// symbolic runtime indices and access sizes, as in [`Self::memory_location`].
    fn access_extent(
        &self,
        func: &Function,
        inst_id: InstId,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        let (base, unknown_extent) = self.access_base(func, address)?;
        let base = Self::refine_region(func, inst_id, base);
        if unknown_extent {
            Some(MemoryLocation::new(base, LocationSize::Unknown))
        } else {
            Self::bounded_memory_location(func, base, size)
        }
    }

    /// Returns a canonical access address and whether its extent is unknown. The end of an
    /// object's payload, `p + 32 + len`, and a heap pointer plus a bounded offset have a proved
    /// lower bound at their base even when the exact offset is unknown. Other addresses retain
    /// their runtime indices, which may wrap.
    fn access_base(&self, func: &Function, address: ValueId) -> Option<(MemoryAddress, bool)> {
        let base = if let Some(object) = Self::payload_end(func, address) {
            self.memory_address(func, object)?.checked_add(EvmMemoryLayout::WORD_SIZE)?
        } else if let Some(base) = Self::bounded_sum_base(func, address) {
            self.memory_address(func, base)?
        } else {
            return Some((self.memory_address(func, address)?, false));
        };
        let base = match base.base {
            MemoryBase::Allocation(inst) | MemoryBase::DynamicAllocation(inst) => {
                Self::unbounded_allocation(func, base, inst)?
            }
            _ => base,
        };
        Some((base, true))
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
            let bound = if matches!(address.base, MemoryBase::Allocation(_)) {
                Self::allocation_min_size(func, inst)
            } else {
                Self::allocation_size(func, inst)
            };
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

    /// The fewest bytes allocation `inst` reserves: its constant size, or one word for a checked
    /// allocation of runtime size, a Solidity object whose size includes its header.
    fn allocation_min_size(func: &Function, inst: InstId) -> Option<u64> {
        match func.inst(inst).kind {
            InstKind::Alloc { size, semantics } => {
                func.value_u64(size).or((semantics.failure == AllocationFailure::Checked)
                    .then_some(EvmMemoryLayout::WORD_SIZE))
            }
            _ => None,
        }
    }

    /// Names an access to allocation `inst` that may leave its bounds, which keeps no
    /// disjointness from other allocations: a unique allocation's access compares with the
    /// allocation's other accesses by offset, and a loop allocation's goes by its pointer value.
    fn unbounded_allocation(
        func: &Function,
        mut address: MemoryAddress,
        inst: InstId,
    ) -> Option<MemoryAddress> {
        // A fresh allocation's nonnegative offsets can reach other heap objects while
        // staying outside reserved memory. Loop allocations may follow an FMP reset.
        if matches!(address.base, MemoryBase::DynamicAllocation(_)) {
            address.region = MemoryRegion::Unknown;
            address.base = MemoryBase::Value(func.inst_result_value(inst)?);
        } else {
            address.base = MemoryBase::UnboundedAllocation(inst);
        }
        Some(address)
    }

    /// Converts a MIR size operand to a canonical location size.
    #[must_use]
    pub(crate) fn location_size(&self, func: &Function, value: ValueId) -> LocationSize {
        func.value_u64(value).map_or(LocationSize::Dynamic(value), LocationSize::Const)
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
                effects.read_write_any([AddressSpace::Memory]);
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
                effects.read_write_any([AddressSpace::Memory]);
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
                // Scratch encodings also read the free memory pointer for their destination,
                // even though they do not reserve the output.
                effects.read(Access::Location(Location::Memory(Self::fmp_location())));
                for (&arg, ty) in args.iter().zip(layout.types.iter()) {
                    if Self::abi_type_reads_memory(ty) {
                        read_memory(&mut effects, arg, SizeOperand::Unknown);
                    }
                }
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::AbiDecode { data, .. } => {
                // A decoding allocates its memory values at the free memory pointer.
                effects.read(Access::Location(Location::Memory(Self::fmp_location())));
                if func.value_slice_location(data).is_some() {
                    read_memory_slice(&mut effects, data);
                } else {
                    read_memory(&mut effects, data, SizeOperand::Unknown);
                }
                effects.write_any(AddressSpace::Memory);
            }
            InstKind::StorageBytesStore(_, object) => {
                effects.read_write_any([AddressSpace::Storage]);
                read_memory(&mut effects, object, SizeOperand::Unknown);
            }
            InstKind::StorageBytesStoreLiteral { .. } => {
                effects.read_write_any([AddressSpace::Storage]);
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
            InstKind::Load(slot, location @ (PtrLocation::Storage | PtrLocation::Transient)) => {
                let alias = func.storage_alias_after_replacements(inst_id, slot, replacements);
                effects.reads.extend(Location::slot(location, alias).map(Access::Location));
            }
            InstKind::Store(
                slot,
                _,
                location @ (PtrLocation::Storage | PtrLocation::Transient),
            ) => {
                let alias = func.storage_alias_after_replacements(inst_id, slot, replacements);
                effects.writes.extend(Location::slot(location, alias).map(Access::Location));
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
                effects.read_write_any(AddressSpace::SLOTS);
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
                    effects.read_write_any([AddressSpace::Memory]);
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
                    effects.read_write_any(AddressSpace::ALL);
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
                effects.read_write_any(AddressSpace::SLOTS);
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
                    effects.read_write_any(AddressSpace::ALL);
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
        func: &Function,
        first: MemoryLocation,
        second: MemoryLocation,
    ) -> AliasResult {
        // The order of fresh pointers comes only from the provenance snapshot, so without it the
        // static rules are complete.
        // NOTE: Like the disjointness of two allocations, this assumes that an access based on an
        // allocation stays inside it.
        if let Some(fresh) = self.provenance.get().and_then(|p| p.fresh_fmps.as_ref())
            && fresh.separates(first, second)
        {
            return AliasResult::NoAlias;
        }
        if (Self::absolute_below_unique_allocation(first, second)
            || Self::absolute_below_unique_allocation(second, first))
            && !self.function_wraps(func)
        {
            return AliasResult::NoAlias;
        }
        Self::memory_alias_locations(first, second)
    }

    /// Returns whether `other` is an absolute range that ends at or below the start of `alloc`.
    /// Unlike [`Self::absolute_below_allocation`], it also covers the reserved words below the
    /// heap, which holds for unique allocations only.
    ///
    /// No reset of the free-memory pointer precedes a unique allocation, so its pointer is at
    /// or above the heap start, and a location at offset `n` in it starts at or above
    /// `HEAP_START + n`. `Allocation` bases come only from the provenance snapshot, which
    /// assigns them to unique allocations. The caller applies the rule only to a function with
    /// no access through a runtime index: such an access wraps and may land on an earlier
    /// object, which the disjointness of allocations assumes away, and their aliasing with the
    /// free-memory pointer slot used to keep passes from trusting it.
    // NOTE: Like `pointer_lower_bound`, this trusts the pointer to enter the function at or above
    // the heap start, which memory-unsafe assembly in a caller could violate.
    fn absolute_below_unique_allocation(alloc: MemoryLocation, other: MemoryLocation) -> bool {
        if matches!(alloc.address.base, MemoryBase::Allocation(_))
            && other.address.base == MemoryBase::Absolute
            && let Some(size) = other.size.as_const()
            && let Some(end) = other.address.offset.checked_add(size)
            && let Some(start) = EvmMemoryLayout::HEAP_START.checked_add(alloc.address.offset)
        {
            end <= start
        } else {
            false
        }
    }

    /// Whether `absolute` is a heap-start-or-higher absolute range that ends at or below the
    /// lowest address `allocation` can start at: every allocation lies at or above the heap
    /// start, as [`Self::pointer_lower_bound`] trusts. Reserved low memory stays `MayAlias`,
    /// since the free-memory-pointer slot there reflects allocations.
    fn absolute_below_allocation(absolute: MemoryLocation, allocation: MemoryLocation) -> bool {
        let (MemoryBase::Absolute, Some(_), LocationSize::Const(size)) =
            (absolute.address.base, Self::allocation_base(allocation.address.base), absolute.size)
        else {
            return false;
        };
        let start = EvmMemoryLayout::HEAP_START.checked_add(allocation.address.offset);
        absolute.address.offset >= EvmMemoryLayout::HEAP_START
            && absolute
                .address
                .offset
                .checked_add(size)
                .zip(start)
                .is_some_and(|(end, start)| end <= start)
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
        // An access that may leave its allocation shares the allocation's pointer, but nothing
        // keeps it from other objects.
        match (first.address.base, second.address.base) {
            (
                MemoryBase::UnboundedAllocation(a),
                MemoryBase::Allocation(b) | MemoryBase::UnboundedAllocation(b),
            )
            | (MemoryBase::Allocation(b), MemoryBase::UnboundedAllocation(a)) => {
                return if a == b {
                    Self::compare_offsets(first, second)
                } else {
                    AliasResult::MayAlias
                };
            }
            _ => {}
        }
        let first_alloc = Self::allocation_base(first.address.base);
        let second_alloc = Self::allocation_base(second.address.base);
        if Self::absolute_below_allocation(first, second)
            || Self::absolute_below_allocation(second, first)
        {
            return AliasResult::NoAlias;
        }
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
        // An access beyond the extent of a unique allocation starts at its offset and goes up, so
        // it cannot reach the allocation's words below that offset. A runtime index wraps, so an
        // indexed access has no such start.
        if let (MemoryBase::Allocation(site), MemoryBase::FreeMemory(fresh))
        | (MemoryBase::FreeMemory(fresh), MemoryBase::Allocation(site)) =
            (first.address.base, second.address.base)
            && site == fresh
        {
            if first.size == LocationSize::Unknown || second.size == LocationSize::Unknown {
                return AliasResult::MayAlias;
            }
            return match Self::compare_offsets(first, second) {
                AliasResult::NoAlias => AliasResult::NoAlias,
                _ => AliasResult::MayAlias,
            };
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
                InstKind::Fmp
                    if provenance
                        .fresh_fmps
                        .as_ref()
                        .is_some_and(|fresh| fresh.positions.contains_key(inst_id)) =>
                {
                    Some(MemoryAddress {
                        region: MemoryRegion::Heap,
                        base: MemoryBase::FreeMemory(*inst_id),
                        offset: 0,
                    })
                }
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
            InstKind::Fmp | InstKind::Alloc { .. } | InstKind::HeapFloor => MemoryRegion::Heap,
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

    /// Returns whether an instruction may write memory through a runtime index, which wraps and
    /// so may land on the free-memory pointer.
    #[must_use]
    pub(crate) fn instruction_may_wrap_write(&self, func: &Function, inst: InstId) -> bool {
        match func.inst(inst).kind {
            InstKind::ICall { function: Callee::Function(_), .. } => {
                Self::instruction_may_wrap_write_with_summaries(
                    func,
                    inst,
                    self.call_summaries.as_deref(),
                )
            }
            _ => {
                !func.inst(inst).kind.effect_kind().is_read_only()
                    && Self::instruction_writes_through_runtime_index_with(
                        func,
                        inst,
                        &self.instruction_mod_ref(func, inst),
                    )
            }
        }
    }

    /// Returns whether an instruction itself writes memory at an address computed with a runtime
    /// index, directly or through a select or phi, or with a write of unknown extent. `effects`
    /// are the [`Self::instruction_mod_ref`] effects of `inst`.
    #[must_use]
    pub(crate) fn instruction_writes_through_runtime_index_with(
        func: &Function,
        inst: InstId,
        effects: &ModRef,
    ) -> bool {
        !func.inst(inst).kind.effect_kind().is_read_only()
            && (Self::writes_at_indexed_address(func, inst)
                || effects.writes().iter().any(|access| {
                    matches!(
                        access,
                        Access::Location(Location::Memory(location))
                            if location.size == LocationSize::Unknown
                    )
                }))
    }

    /// Returns whether an instruction writes memory at an address computed with a runtime index.
    fn writes_at_indexed_address(func: &Function, inst: InstId) -> bool {
        let indexed = |address| Self::address_has_runtime_index(func, address, 8);
        match func.inst(inst).kind {
            InstKind::Store(address, _, PtrLocation::Memory)
            | InstKind::Store8(address, _)
            | InstKind::MemoryZero(address, _)
            | InstKind::Copy(address, ..)
            | InstKind::DataCopy(_, address, _)
            | InstKind::CodeCopy(address, ..)
            | InstKind::ExtCodeCopy(_, address, ..) => indexed(address),
            InstKind::Call { ret_offset: address, .. }
            | InstKind::CallCode { ret_offset: address, .. }
            | InstKind::StaticCall { ret_offset: address, .. }
            | InstKind::DelegateCall { ret_offset: address, .. } => indexed(address),
            _ => false,
        }
    }

    /// Like [`Self::instruction_may_wrap_write`], without the writes of unknown extent, which
    /// need the provenance this feeds. Such a write starts at its address and cannot wrap
    /// around without a length no gas can pay for.
    fn instruction_may_wrap_write_with_summaries(
        func: &Function,
        inst: InstId,
        call_summaries: Option<&MemoryCallSummaries>,
    ) -> bool {
        match func.inst(inst).kind {
            InstKind::ICall { function: Callee::Function(function), .. } => call_summaries
                .and_then(|summaries| summaries.get(function))
                .is_none_or(|summary| summary.may_wrap_write()),
            _ => Self::writes_at_indexed_address(func, inst),
        }
    }

    /// Returns whether an address is computed with a runtime value, on any path of a select or
    /// phi: added to a pointer, or through any arithmetic this does not follow. Pointers that
    /// come from an allocation, a load, a call, or an argument, and constant offsets from them,
    /// are not. A chain too deep to follow counts as indexed.
    fn address_has_runtime_index(func: &Function, address: ValueId, depth: u32) -> bool {
        let Some(depth) = depth.checked_sub(1) else { return true };
        let Value::Inst(inst) = func.value(address) else { return false };
        let indexed = |value| Self::address_has_runtime_index(func, value, depth);
        match &func.inst(*inst).kind {
            InstKind::Alloc { .. }
            | InstKind::Fmp
            | InstKind::HeapFloor
            | InstKind::Load(..)
            | InstKind::ICall { .. }
            | InstKind::InternalFrameAddr(_)
            | InstKind::FrameLoad { .. } => false,
            InstKind::PtrAdd(base, offset) => func.value_u64(*offset).is_none() || indexed(*base),
            InstKind::Add(first, second) => match (func.value_u64(*first), func.value_u64(*second))
            {
                (_, Some(_)) => indexed(*first),
                (Some(_), None) => indexed(*second),
                (None, None) => Self::bounded_sum_base(func, address).is_none(),
            },
            InstKind::Sub(base, offset) => func.value_u64(*offset).is_none() || indexed(*base),
            InstKind::Select(_, first, second) => indexed(*first) || indexed(*second),
            InstKind::Phi(incoming) => incoming.iter().any(|&(_, value)| indexed(value)),
            InstKind::PtrToInt(value, 256) | InstKind::IntToPtr(value) => indexed(*value),
            InstKind::MakeSlice { ptr, .. } => indexed(*ptr),
            InstKind::ExtractValue { aggregate, .. } => indexed(*aggregate),
            _ => true,
        }
    }

    /// Returns whether an instruction may write the free-memory-pointer word.
    ///
    /// Unlike [`Self::instruction_may_reset_fmp`], this also counts allocations that bump the
    /// word, so a cached copy of it stays valid across an instruction only when this is false.
    /// Raw accesses keep their address expression, including bounds established before the write.
    #[must_use]
    pub(crate) fn instruction_may_write_fmp(
        &self,
        func: &Function,
        inst: InstId,
        proofs: &ReadBounds,
    ) -> bool {
        if func.inst(inst).kind.effect_kind().is_read_only()
            || proofs.raw_writes_avoid_fmp(func, inst)
        {
            return false;
        }
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
        if let MemoryBase::Value(base) = address.base
            && let LocationSize::Const(size) = size
            && address.offset != 0
            && address.offset.checked_add(size).is_some_and(|end| end <= EvmMemoryLayout::FMP_SLOT)
            && Self::untrusted_read_lower_bound(func, base)
                .is_some_and(|bound| bound >= EvmMemoryLayout::HEAP_START)
        {
            // A wrapped base + offset lies below offset, so the access ends before the FMP
            // slot. A non-wrapped sum remains at or above the heap start.
            return false;
        }
        Self::address_lower_bound(func, address, size)
            .is_none_or(|bound| bound < EvmMemoryLayout::HEAP_START)
    }

    /// Returns a conservative lower bound for the first byte of an access of `size` at a
    /// canonical address, which holds even when a runtime index wraps around.
    ///
    /// An access of unknown extent through a non-absolute base keeps no bound.
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
            MemoryBase::Allocation(_)
            | MemoryBase::UnboundedAllocation(_)
            | MemoryBase::DynamicAllocation(_) => {
                EvmMemoryLayout::HEAP_START.checked_add(address.offset)
            }
            MemoryBase::Value(base) => {
                if !Self::constant_offset_no_wrap(func, base, address.offset) {
                    return None;
                }
                Self::untrusted_read_lower_bound(func, base)?.checked_add(address.offset)
            }
            // The pointer read's result is the symbolic base, bounded only if it is the
            // allocator's own read.
            MemoryBase::FreeMemory(read) => {
                let value = func.inst_result_value(read)?;
                if !Self::constant_offset_no_wrap(func, value, address.offset) {
                    return None;
                }
                Self::untrusted_read_lower_bound(func, value)?.checked_add(address.offset)
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

    /// Returns a conservative lower bound for a compiler-owned pointer.
    pub(crate) fn pointer_lower_bound(func: &Function, value: ValueId) -> Option<u64> {
        Self::lower_bound(func, value, 0, true)
    }

    /// Returns a lower bound for a pointer that also holds when assembly forges the free-memory
    /// pointer or a runtime `ptradd` offset wraps, for deciding which slots a read may observe.
    fn untrusted_read_lower_bound(func: &Function, value: ValueId) -> Option<u64> {
        Self::lower_bound(func, value, 0, false)
    }

    /// Returns the least address `value` can hold when a compiler-owned base, an allocation, a
    /// frame or a read of the free memory pointer, reaches it through constant additions alone.
    /// Unlike [`Self::pointer_lower_bound`], which bounds where a write may land, it trusts no
    /// runtime offset: assembly that forges a length can index an object's element around to
    /// low memory, and a read there must see what was stored. A decoded value that is no memory
    /// object is no pointer either. A word loaded from the free memory pointer's slot counts only
    /// when `free_pointer` says nothing in the module can leave a word below the heap there, as
    /// a copy into the slot can. `call` bounds the pointer an internal call returns.
    #[must_use]
    pub(crate) fn read_lower_bound(
        func: &Function,
        value: ValueId,
        call: &dyn Fn(FunctionId) -> Option<u64>,
        free_pointer: bool,
    ) -> Option<u64> {
        Self::constant_offset_lower_bound(func, value, call, free_pointer, None, 0, None)
    }

    /// Returns the least address a word at `offset` of the current internal-call frame can
    /// have. A fixed frame lies at or above the heap start but omits the frame header that MIR
    /// offsets include, and an external entry's frame starts at the heap start.
    #[must_use]
    pub(crate) fn frame_lower_bound(offset: u64) -> u64 {
        EvmMemoryLayout::HEAP_START
            .saturating_add(offset.saturating_sub(EvmMemoryLayout::INTERNAL_FRAME_HEADER_SIZE))
    }

    fn constant_offset_lower_bound(
        func: &Function,
        value: ValueId,
        call: &dyn Fn(FunctionId) -> Option<u64>,
        free_pointer: bool,
        before: Option<InstId>,
        depth: usize,
        proofs: Option<&ReadBounds>,
    ) -> Option<u64> {
        if depth > 8 {
            return None;
        }
        if let Some(value) = func.value_u64(value) {
            return Some(value);
        }
        let Value::Inst(inst) = func.value(value) else { return None };
        let bound = |value| {
            Self::constant_offset_lower_bound(
                func,
                value,
                call,
                free_pointer,
                before,
                depth + 1,
                proofs,
            )
        };
        match &func.inst(*inst).kind {
            InstKind::Fmp
            | InstKind::HeapFloor
            | InstKind::Alloc { .. }
            | InstKind::AbiEncode { .. } => Some(EvmMemoryLayout::HEAP_START),
            InstKind::AbiDecode { .. } if func.value_ty(value) == Some(MirType::MEMPTR) => {
                Some(EvmMemoryLayout::HEAP_START)
            }
            InstKind::Load(address, PtrLocation::Memory)
                if (free_pointer || func.inst(*inst).metadata.alloc_ptr())
                    && func.value_u64(*address) == Some(EvmMemoryLayout::FMP_SLOT) =>
            {
                Some(EvmMemoryLayout::HEAP_START)
            }
            InstKind::InternalFrameAddr(offset) => Some(Self::frame_lower_bound(*offset)),
            InstKind::ICall { function: Callee::Function(function), .. } => call(*function),
            InstKind::PtrToInt(value, 256) | InstKind::IntToPtr(value) => bound(*value),
            InstKind::Add(first, second) => {
                let lower = func
                    .value_u64(*second)
                    .and_then(|offset| bound(*first)?.checked_add(offset))
                    .or_else(|| {
                        func.value_u64(*first)
                            .and_then(|offset| bound(*second)?.checked_add(offset))
                    })
                    .or_else(|| func.inst(*inst).metadata.alloc_ptr().then(|| bound(*first))?)?;
                // First establish a useful base. Proving that arbitrary scalar arithmetic
                // cannot wrap cannot give an address a heap lower bound.
                (func.inst(*inst).metadata.alloc_ptr()
                    || match proofs {
                        Some(proofs) => {
                            proofs.constant_add_after_access(func, value, before.unwrap_or(*inst))
                                || proofs.arithmetic_no_wrap(func, value)
                        }
                        None => {
                            super::integers::arithmetic_no_wrap(func, value)
                                || Self::constant_add_after_access_at(
                                    func,
                                    value,
                                    before.unwrap_or(*inst),
                                )
                        }
                    })
                .then_some(lower)
            }
            InstKind::PtrAdd(base, offset) => {
                let offset = func.value_u64(*offset)?;
                let lower = bound(*base)?.checked_add(offset)?;
                // Pointer offsets wrap just like word additions. A lower bound for the base
                // alone cannot exclude reaching reserved memory after wraparound.
                let no_wrap = match proofs {
                    Some(proofs) => {
                        proofs.constant_offset_no_wrap(func, *base, offset)
                            || proofs.constant_add_after_access(
                                func,
                                value,
                                before.unwrap_or(*inst),
                            )
                    }
                    None => {
                        Self::constant_offset_no_wrap(func, *base, offset)
                            || Self::constant_add_after_access_at(
                                func,
                                value,
                                before.unwrap_or(*inst),
                            )
                    }
                };
                no_wrap.then_some(lower)
            }
            InstKind::ExtractValue {
                ty: MirType::Slice(PtrLocation::Memory),
                aggregate: slice,
                index: MirType::SLICE_PTR_FIELD,
            } => {
                let Value::Inst(slice) = func.value(*slice) else { return None };
                match &func.inst(*slice).kind {
                    InstKind::MakeSlice { ptr, location: PtrLocation::Memory, .. } => bound(*ptr),
                    InstKind::AbiEncode { .. } | InstKind::AbiDecode { .. } => {
                        Some(EvmMemoryLayout::HEAP_START)
                    }
                    _ => None,
                }
            }
            InstKind::Phi(incoming) => incoming
                .iter()
                .map(|(_, value)| {
                    // An incoming definition may belong to a previous loop iteration. A
                    // successful access before the current user cannot bound that iteration's
                    // operands; use only accesses before the incoming definition itself.
                    Self::constant_offset_lower_bound(
                        func,
                        *value,
                        call,
                        free_pointer,
                        None,
                        depth + 1,
                        proofs,
                    )
                })
                .try_fold(None, |least, bound| {
                    let bound = bound?;
                    Some(Some(least.map_or(bound, |least: u64| least.min(bound))))
                })?,
            InstKind::Select(_, first, second) => Some(bound(*first)?.min(bound(*second)?)),
            _ => None,
        }
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
            InstKind::Fmp | InstKind::Alloc { .. } | InstKind::HeapFloor => {
                Some(EvmMemoryLayout::HEAP_START)
            }
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
            InstKind::Add(first, second) => {
                // Adding to a lower bound alone does not prove that the actual sum cannot wrap.
                func.value_u64(*second)
                    .and_then(|offset| {
                        Self::lower_bound(func, *first, depth + 1, trusted)?.checked_add(offset)
                    })
                    .or_else(|| {
                        func.value_u64(*first).and_then(|offset| {
                            Self::lower_bound(func, *second, depth + 1, trusted)?
                                .checked_add(offset)
                        })
                    })
                    .filter(|_| {
                        super::integers::arithmetic_no_wrap(func, value)
                            || Self::constant_add_after_access(func, value)
                            || Self::constant_add_to_checked_alloc(func, value)
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
                    .or_else(|| {
                        if !trusted {
                            return None;
                        }
                        let object = Self::payload_end(func, value)?;
                        Self::lower_bound(func, object, depth + 1, trusted)?
                            .checked_add(EvmMemoryLayout::WORD_SIZE)
                    })
            }
            InstKind::Sub(base, offset)
                if (super::integers::integer_bits(func, value) == 256
                    || super::integers::arithmetic_no_wrap(func, value)) =>
            {
                Self::lower_bound(func, *base, depth + 1, trusted)
                    .and_then(|base| base.checked_sub(func.value_u64(*offset)?))
            }
            // A forged length can admit a runtime offset that wraps below the object's start,
            // even for a compiler-owned allocation. Such writes may reset the FMP as well.
            InstKind::PtrAdd(base, offset) => {
                let offset = func.value_u64(*offset)?;
                Self::lower_bound(func, *base, depth + 1, trusted)?.checked_add(offset).filter(
                    |_| {
                        Self::constant_offset_no_wrap(func, *base, offset)
                            || Self::constant_add_after_access(func, value)
                    },
                )
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
                .try_fold(None, |least, bound| {
                    let bound = bound?;
                    Some(Some(least.map_or(bound, |least: u64| least.min(bound))))
                })?,
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

    /// Returns the memory object `p` of an address `p + 32 + len`, the end of `p`'s payload, in
    /// either association, when `len` is a length below `2^255` stored into `p` before the address
    /// is computed: by `p`'s allocation block, as `new bytes(len)` stores it, or earlier in the
    /// address's own block, as a trusted `mstore(p, len)` that sets a buffer's length does. Inline
    /// assembly spells such an address with plain additions, as a zero word stored after a fresh
    /// result to pad it does.
    ///
    /// NOTE: Only the bound keeps the sum from wrapping around the address space, onto the length
    /// word or below it: assembly may store any word as a length, and `new bytes(len)` bounds
    /// `len` only by a panic this analysis does not follow. A bound also comes from an earlier
    /// access of `len` bytes in the address's block (see [`Self::bounded_offset`]). The store into
    /// `p` places `p` below `2^64`, as no store reaches memory past that, so the sum stays below
    /// `2^256` and an access at it stays clear of the length word, though it may leave the object.
    fn payload_end(func: &Function, address: ValueId) -> Option<ValueId> {
        let address = Self::memory_address_value(func, address);
        let Value::Inst(user) = *func.value(address) else { return None };
        let InstKind::Add(a, b) = func.inst(user).kind else { return None };
        let add = |value| match *func.value(value) {
            Value::Inst(inst) => match func.inst(inst).kind {
                InstKind::Add(c, d) => Some((c, d)),
                _ => None,
            },
            _ => None,
        };
        let object = |value| match *func.value(value) {
            Value::Inst(inst) => match func.inst(inst).kind {
                InstKind::PtrToInt(object, 256) => Some(object),
                _ => None,
            },
            _ => None,
        };
        let word = |value| func.value_u64(value) == Some(EvmMemoryLayout::WORD_SIZE);
        for (inner, outer) in [(a, b), (b, a)] {
            let Some((c, d)) = add(inner) else { continue };
            for (pointer, other) in [(c, d), (d, c)] {
                let Some(object) = object(pointer) else { continue };
                // (p + 32) + len, or (p + len) + 32
                let len = match (word(other), word(outer)) {
                    (true, _) => outer,
                    (_, true) => other,
                    _ => continue,
                };
                if Self::bounded_offset(func, len, user)
                    && Self::stores_length_before(func, object, len, user)
                {
                    return Some(object);
                }
            }
        }
        None
    }

    /// Whether `offset`, an operand of `user`, is below `2^255`: by its type or opcode, or because
    /// an earlier access in `user`'s block spans that many bytes. A successful access of a
    /// nonempty range ends below `2^64`, so its size is below `2^64`, or zero. The search for the
    /// block gives up after 1024 instructions, so a query costs the same in a large function.
    fn bounded_offset(func: &Function, offset: ValueId, user: InstId) -> bool {
        const BUDGET: usize = 1024;
        let below_half = |bounds: (U256, U256)| bounds.1 <= U256::MAX >> 1;
        if below_half(super::integers::unsigned_bounds_with_opcodes(func, offset)) {
            return true;
        }
        let mut sizes = SmallVec::<[ValueId; 8]>::new();
        let mut budget = BUDGET;
        for block in &func.blocks {
            let searched = &block.instructions[..block.instructions.len().min(budget)];
            if let Some(at) = searched.iter().position(|&inst| inst == user) {
                for &inst in &searched[..at] {
                    func.inst(inst).kind.visit_raw_memory(|access, _, size| {
                        if let RawMemorySize::Value(size) = size
                            && access != RawMemoryAccess::PartialWrite
                        {
                            sizes.push(size);
                        }
                    });
                }
                let known =
                    |value| sizes.contains(&value).then_some((U256::ZERO, U256::from(u64::MAX)));
                return below_half(super::integers::unsigned_bounds_with_opcodes_and(
                    func, offset, known,
                ));
            }
            budget -= searched.len();
            if budget == 0 {
                break;
            }
        }
        false
    }

    /// Returns the heap pointer `base` of a sum `base + offset`, in either order, when `offset`
    /// is bounded and so cannot wrap the sum below `base`: a compiler-owned pointer, an allocation
    /// or the free-memory pointer, plus constants. Like the freshness of those pointers, this
    /// assumes that assembly has not forged the pointer to the top of the address space.
    ///
    /// For an object `p` this subsumes [`Self::payload_end`]; that one still serves objects that
    /// are no heap pointer, such as arguments, once their length is stored.
    fn bounded_sum_base(func: &Function, address: ValueId) -> Option<ValueId> {
        let address = Self::memory_address_value(func, address);
        let Value::Inst(user) = *func.value(address) else { return None };
        let InstKind::Add(first, second) = func.inst(user).kind else { return None };
        [(first, second), (second, first)].into_iter().find_map(|(base, offset)| {
            (func.value_u64(offset).is_none()
                && Self::is_heap_pointer(func, base)
                && Self::bounded_offset(func, offset, user))
            .then_some(base)
        })
    }

    /// Whether `value` is an allocation or the free-memory pointer, plus constant offsets.
    fn is_heap_pointer(func: &Function, mut value: ValueId) -> bool {
        loop {
            value = Self::memory_address_value(func, value);
            let Value::Inst(inst) = *func.value(value) else { return false };
            match func.inst(inst).kind {
                InstKind::Alloc { .. } | InstKind::Fmp => return true,
                InstKind::Add(base, offset) | InstKind::PtrAdd(base, offset)
                    if func.value_u64(offset).is_some() =>
                {
                    value = base;
                }
                _ => return false,
            }
        }
    }

    /// Whether `len` is stored into the length word of `object` before `user` runs: earlier in
    /// `user`'s block, or after `object`'s allocation in another block, which control leaves
    /// before any use of the allocation outside it.
    fn stores_length_before(func: &Function, object: ValueId, len: ValueId, user: InstId) -> bool {
        let stores = |inst| func.memory_length_store(inst) == Some((object, len));
        let alloc = match *func.value(object) {
            Value::Inst(inst) if matches!(func.inst(inst).kind, InstKind::Alloc { .. }) => {
                Some(inst)
            }
            _ => None,
        };
        func.blocks.iter().any(|block| {
            let insts = &block.instructions;
            match insts.iter().position(|&inst| inst == user) {
                Some(at) => insts[..at].iter().copied().any(stores),
                None => alloc.is_some_and(|alloc| {
                    insts
                        .iter()
                        .position(|&inst| inst == alloc)
                        .is_some_and(|from| insts[from..].iter().copied().any(stores))
                }),
            }
        })
    }

    /// Whether adding a canonical byte offset preserves the ordering of the actual base.
    /// Canonical addresses describe modular equality, so a lower bound for the base alone
    /// cannot rule out a sum wrapping into reserved memory.
    pub(crate) fn constant_offset_no_wrap(func: &Function, base: ValueId, offset: u64) -> bool {
        if offset == 0 {
            return true;
        }
        let base = Self::memory_address_value(func, base);
        if let Value::Inst(inst) = *func.value(base)
            && let InstKind::Alloc { size, semantics } = func.inst(inst).kind
        {
            // Checked allocations cap their end at MAX_ALLOCATION_END, so their base plus a
            // u64 offset cannot wrap. Other allocations prove only their own bump valid.
            if semantics.failure == AllocationFailure::Checked
                || func.value_u64(size).is_some_and(|size| offset <= size)
            {
                return true;
            }
        }
        super::integers::unsigned_bounds(func, base).1.checked_add(U256::from(offset)).is_some()
    }

    /// Whether a constant word addition follows an access that bounds its base below `2^64`.
    /// A successful nonempty memory access cannot reach a larger address, so adding a `u64`
    /// constant to that same value cannot wrap a word. Only earlier accesses in the addition's
    /// own block count; zero-length accesses and partial writes prove nothing. The search is
    /// bounded to avoid rescanning a large function for every address query.
    fn constant_add_after_access(func: &Function, value: ValueId) -> bool {
        let Value::Inst(user) = *func.value(value) else { return false };
        Self::constant_add_after_access_at(func, value, user)
    }

    /// Like [`Self::constant_add_after_access`], using accesses before `user` rather than before
    /// the addition. The address may have been computed before the access bounds its base.
    fn constant_add_after_access_at(func: &Function, value: ValueId, user: InstId) -> bool {
        let Some(base) = Self::constant_add_base(func, value) else { return false };
        let mut budget = 256usize;
        for block in &func.blocks {
            let searched = &block.instructions[..block.instructions.len().min(budget)];
            if let Some(at) = searched.iter().position(|&inst| inst == user) {
                return searched[..at].iter().any(|&inst| {
                    let mut accessed = false;
                    func.inst(inst).kind.visit_raw_memory(|access, address, size| {
                        accessed |= access != RawMemoryAccess::PartialWrite
                            && size.constant(func).is_some_and(|size| size != 0)
                            && Self::memory_address_value(func, address) == base;
                    });
                    accessed
                });
            }
            budget -= searched.len();
            if budget == 0 {
                return false;
            }
        }
        false
    }

    /// Follows only casts that preserve every bit of a memory address.
    pub(crate) fn memory_address_value(func: &Function, mut value: ValueId) -> ValueId {
        for _ in 0..8 {
            let Value::Inst(inst) = *func.value(value) else { break };
            match func.inst(inst).kind {
                InstKind::PtrToInt(pointer, 256) | InstKind::IntToPtr(pointer) => value = pointer,
                _ => break,
            }
        }
        value
    }

    /// Whether a word-sized addition of a nonnegative `u64` constant to a checked allocation
    /// cannot wrap, as for [`InstKind::PtrAdd`]: such an allocation caps its end at
    /// `MAX_ALLOCATION_END`.
    fn constant_add_to_checked_alloc(func: &Function, value: ValueId) -> bool {
        Self::constant_add_base(func, value).is_some_and(|base| {
            matches!(*func.value(base), Value::Inst(inst) if matches!(
                func.inst(inst).kind,
                InstKind::Alloc { semantics, .. } if semantics.failure == AllocationFailure::Checked
            ))
        })
    }

    /// The base of a word-sized addition or pointer offset by a nonnegative `u64` constant.
    fn constant_add_base(func: &Function, value: ValueId) -> Option<ValueId> {
        let Value::Inst(inst) = *func.value(value) else { return None };
        let base = match func.inst(inst).kind {
            InstKind::Add(first, second) if super::integers::integer_bits(func, value) == 256 => {
                if func.value_u64(second).is_some() {
                    first
                } else if func.value_u64(first).is_some() {
                    second
                } else {
                    return None;
                }
            }
            InstKind::PtrAdd(base, offset) if func.value_u64(offset).is_some() => base,
            _ => return None,
        };
        Some(Self::memory_address_value(func, base))
    }
}

#[derive(Clone, Copy)]
enum SizeOperand {
    Const(u64),
    Value(ValueId),
    Unknown,
}

/// Reusable address proofs for one immutable function snapshot.
///
/// Successful nonempty accesses and arithmetic bounds are independent of call-return facts,
/// so liveness fixpoint rounds can reuse them while those facts grow. Replace this snapshot
/// after any instruction or CFG change, including removal of an access used as a proof.
/// Lower bounds themselves are not cached because their call-return facts can change.
#[derive(Default)]
pub(crate) struct ReadBounds {
    accesses: OnceCell<MemoryAccessOrder>,
    arithmetic: RefCell<IndexVec<ValueId, Option<bool>>>,
    offsets: RefCell<FxHashMap<(ValueId, u64), bool>>,
}

impl ReadBounds {
    /// The counterpart of [`AliasAnalysis::read_lower_bound`] with reusable safety proofs.
    pub(crate) fn read_lower_bound(
        &self,
        func: &Function,
        value: ValueId,
        call: &dyn Fn(FunctionId) -> Option<u64>,
        free_pointer: bool,
    ) -> Option<u64> {
        AliasAnalysis::constant_offset_lower_bound(
            func,
            value,
            call,
            free_pointer,
            None,
            0,
            Some(self),
        )
    }

    /// Like [`Self::read_lower_bound`], using successful memory accesses before `user` to prove
    /// that constant additions cannot wrap. Only accesses in `user`'s own block count.
    pub(crate) fn read_lower_bound_at(
        &self,
        func: &Function,
        value: ValueId,
        call: &dyn Fn(FunctionId) -> Option<u64>,
        free_pointer: bool,
        user: InstId,
    ) -> Option<u64> {
        AliasAnalysis::constant_offset_lower_bound(
            func,
            value,
            call,
            free_pointer,
            Some(user),
            0,
            Some(self),
        )
    }

    /// The counterpart of [`AliasAnalysis::constant_offset_no_wrap`] with cached offset proofs.
    pub(crate) fn constant_offset_no_wrap(
        &self,
        func: &Function,
        value: ValueId,
        offset: u64,
    ) -> bool {
        if offset == 0 {
            return true;
        }
        *self
            .offsets
            .borrow_mut()
            .entry((value, offset))
            .or_insert_with(|| AliasAnalysis::constant_offset_no_wrap(func, value, offset))
    }

    fn arithmetic_no_wrap(&self, func: &Function, value: ValueId) -> bool {
        if let Some(&Some(proved)) = self.arithmetic.borrow().get(value) {
            return proved;
        }
        let proved = super::integers::arithmetic_no_wrap(func, value);
        let mut arithmetic = self.arithmetic.borrow_mut();
        arithmetic.resize(func.num_values(), None);
        arithmetic[value] = Some(proved);
        proved
    }

    fn constant_add_after_access(&self, func: &Function, value: ValueId, user: InstId) -> bool {
        let Some(base) = AliasAnalysis::constant_add_base(func, value) else { return false };
        let accesses = self.accesses.get_or_init(|| MemoryAccessOrder::new(func));
        let Some(&(block, index)) = accesses.positions[user].as_ref() else { return false };
        accesses.first[block].get(&base).is_some_and(|&access| access < index)
    }

    /// Whether an instruction has raw writes and all avoid the FMP word. Evaluate bounds at
    /// the write: its address may have been computed before an earlier access proved its base
    /// nonwrapping. Reads and instructions with no raw write provide no such guarantee.
    fn raw_writes_avoid_fmp(&self, func: &Function, inst: InstId) -> bool {
        let mut raw_write = false;
        let mut avoids_fmp = true;
        func.inst(inst).kind.visit_raw_memory(|access, address, size| {
            if access == RawMemoryAccess::Read {
                return;
            }
            raw_write = true;
            let size = size.constant(func);
            if size == Some(0) {
                return;
            }
            avoids_fmp &= match func.value_u64(address) {
                Some(address) => !AliasAnalysis::const_range_may_overlap_fmp(address, size),
                None => self
                    .read_lower_bound_at(func, address, &|_| None, false, inst)
                    .is_some_and(|bound| bound >= EvmMemoryLayout::HEAP_START),
            };
        });
        raw_write && avoids_fmp
    }

    /// Drops every cached proof, keeping the allocations.
    pub(crate) fn clear(&mut self) {
        self.accesses.take();
        self.arithmetic.get_mut().clear();
        self.offsets.get_mut().clear();
    }
}

/// The first successful access to each address in each block and placed instruction order.
/// No fact crosses a block boundary, so an access in another branch cannot justify a proof.
struct MemoryAccessOrder {
    positions: IndexVec<InstId, Option<(BlockId, usize)>>,
    first: IndexVec<BlockId, FxHashMap<ValueId, usize>>,
}

impl MemoryAccessOrder {
    fn new(func: &Function) -> Self {
        let mut positions = IndexVec::from_vec(vec![None; func.num_insts()]);
        let mut first = IndexVec::new();
        for (block, data) in func.blocks.iter_enumerated() {
            let mut accesses = FxHashMap::default();
            for (index, &inst) in data.instructions.iter().enumerate() {
                positions[inst] = Some((block, index));
                func.inst(inst).kind.visit_raw_memory(|access, address, size| {
                    if access != RawMemoryAccess::PartialWrite
                        && size.constant(func).is_some_and(|size| size != 0)
                    {
                        let address = AliasAnalysis::memory_address_value(func, address);
                        accesses.entry(address).or_insert(index);
                    }
                });
            }
            first.push(accesses);
        }
        Self { positions, first }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::FunctionBuilder;
    use solar_interface::Ident;

    #[test]
    fn access_proofs_are_not_limited_to_the_function_prefix() {
        for separate_block in [false, true] {
            let mut func = Function::new(Ident::DUMMY);
            let sum = {
                let mut builder = FunctionBuilder::new(&mut func);
                let base = builder.add_param(MirType::I256);
                let word = builder.imm(32);
                // repeat 300: sload base
                // mload base
                // sum = add base, 32
                for _ in 0..300 {
                    builder.load(base, PtrLocation::Storage);
                }
                if separate_block {
                    let next = builder.create_block();
                    builder.jump(next);
                    builder.switch_to_block(next);
                }
                builder.load(base, PtrLocation::Memory);
                builder.add(base, word)
            };
            let Value::Inst(user) = *func.value(sum) else { unreachable!() };
            let proofs = ReadBounds::default();
            assert!(proofs.constant_add_after_access(&func, sum, user));
            assert!(proofs.constant_add_after_access(&func, sum, user));
        }
    }

    #[test]
    fn access_proofs_require_an_earlier_access_in_the_same_block() {
        let mut func = Function::new(Ident::DUMMY);
        let (before, after, other) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let base = builder.add_param(MirType::I256);
            let word = builder.imm(32);
            // before = add base, 32
            // mload base
            // after = add base, 32
            let before = builder.add(base, word);
            builder.load(base, PtrLocation::Memory);
            let after = builder.add(base, word);
            let next = builder.create_block();
            builder.jump(next);
            builder.switch_to_block(next);
            // other = add base, 32
            let other = builder.add(base, word);
            (before, after, other)
        };
        let proofs = ReadBounds::default();
        for (sum, expected) in [(before, false), (after, true), (other, false)] {
            let Value::Inst(user) = *func.value(sum) else { unreachable!() };
            assert_eq!(proofs.constant_add_after_access(&func, sum, user), expected);
        }
    }

    #[test]
    fn access_proofs_ignore_empty_and_partial_writes() {
        let mut func = Function::new(Ident::DUMMY);
        let (empty, partial, full) = {
            let mut builder = FunctionBuilder::new(&mut func);
            let base = builder.add_param(MirType::I256);
            let word = builder.imm(32);
            let zero = builder.imm(0);
            // calldatacopy base, 0, 0
            // empty = add base, 32
            builder.copy(base, zero, zero, PtrLocation::Calldata);
            let empty = builder.add(base, word);
            // staticcall 0, 0, 0, 0, base, 32
            // partial = add base, 32
            builder.emit_inst(
                InstKind::StaticCall {
                    gas: zero,
                    addr: zero,
                    args_offset: zero,
                    args_size: zero,
                    ret_offset: base,
                    ret_size: word,
                },
                Some(MirType::I1),
            );
            let partial = builder.add(base, word);
            // calldatacopy base, 0, 32
            // full = add base, 32
            builder.copy(base, zero, word, PtrLocation::Calldata);
            let full = builder.add(base, word);
            (empty, partial, full)
        };
        let proofs = ReadBounds::default();
        for (sum, expected) in [(empty, false), (partial, false), (full, true)] {
            let Value::Inst(user) = *func.value(sum) else { unreachable!() };
            assert_eq!(proofs.constant_add_after_access(&func, sum, user), expected);
        }
    }
}

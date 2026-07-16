//! Shared MIR alias and ModRef analysis.
//!
//! This is deliberately a value-local basic alias analysis rather than a
//! flow-sensitive points-to analysis. It canonicalizes addresses as a base plus
//! constant offset, keeps compiler-owned memory regions disjoint, and exposes
//! the memory, storage, and transient-storage effects of each instruction.
//! Transformations can therefore share one conservative answer without
//! attaching pass-specific facts to MIR values.

use crate::mir::{
    Function, InstId, InstKind, MemoryRegion, SliceLocation, StorageAlias, Terminator, Value,
    ValueId,
};
use smallvec::SmallVec;
use solar_data_structures::map::FxHashMap;

/// An address space tracked by ModRef analysis.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AddressSpace {
    /// EVM linear memory.
    Memory,
    /// Persistent contract storage.
    Storage,
    /// Transaction-scoped transient storage.
    Transient,
}

/// The canonical base of a memory address.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryBase {
    /// An absolute EVM memory address.
    Absolute,
    /// The current function's internal-call frame.
    InternalFrame,
    /// A symbolic MIR value.
    Value(ValueId),
}

/// A canonical memory address represented as a base plus byte offset.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MemoryAddress {
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
    pub const fn absolute(offset: u64) -> Self {
        let region = if offset < 0x80 { MemoryRegion::Scratch } else { MemoryRegion::Unknown };
        Self { region, base: MemoryBase::Absolute, offset }
    }

    /// Creates an address in the current internal-call frame.
    #[must_use]
    pub const fn internal_frame(offset: u64) -> Self {
        Self { region: MemoryRegion::InternalFrame, base: MemoryBase::InternalFrame, offset }
    }

    /// Creates a symbolic address.
    #[must_use]
    pub const fn symbolic(value: ValueId, region: MemoryRegion) -> Self {
        Self { region, base: MemoryBase::Value(value), offset: 0 }
    }

    /// Returns the absolute address, if known.
    #[must_use]
    pub const fn as_absolute(self) -> Option<u64> {
        match self.base {
            MemoryBase::Absolute => Some(self.offset),
            MemoryBase::InternalFrame | MemoryBase::Value(_) => None,
        }
    }

    /// Returns the internal-frame byte offset, if known.
    #[must_use]
    pub const fn as_internal_frame_offset(self) -> Option<u64> {
        match self.base {
            MemoryBase::InternalFrame => Some(self.offset),
            MemoryBase::Absolute | MemoryBase::Value(_) => None,
        }
    }

    /// Returns this address advanced by `offset`, if it fits.
    #[must_use]
    pub fn checked_add(self, offset: u64) -> Option<Self> {
        Some(Self { offset: self.offset.checked_add(offset)?, ..self })
    }
}

/// The byte width of a memory location.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LocationSize {
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
    pub const fn as_const(self) -> Option<u64> {
        match self {
            Self::Const(size) => Some(size),
            Self::Dynamic(_) | Self::Unknown => None,
        }
    }
}

/// A canonical memory byte range.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct MemoryLocation {
    /// Start address.
    pub address: MemoryAddress,
    /// Range width.
    pub size: LocationSize,
}

impl MemoryLocation {
    /// Creates a memory location.
    #[must_use]
    pub const fn new(address: MemoryAddress, size: LocationSize) -> Self {
        Self { address, size }
    }

    /// Returns this location advanced by `offset`, preserving its size.
    #[must_use]
    pub fn checked_add(self, offset: u64) -> Option<Self> {
        Some(Self { address: self.address.checked_add(offset)?, ..self })
    }
}

/// A location in one of the stateful EVM address spaces.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Location {
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
    pub const fn address_space(self) -> AddressSpace {
        match self {
            Self::Memory(_) => AddressSpace::Memory,
            Self::Storage(_) => AddressSpace::Storage,
            Self::Transient(_) => AddressSpace::Transient,
        }
    }
}

/// Alias relationship between two locations.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum AliasResult {
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
    pub const fn may_alias(self) -> bool {
        !matches!(self, Self::NoAlias)
    }
}

/// One exact or address-space-wide instruction access.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Access {
    /// Access to one canonical location.
    Location(Location),
    /// Conservative access to an entire address space.
    Any(AddressSpace),
}

impl Access {
    /// Returns this access's address space.
    #[must_use]
    pub const fn address_space(self) -> AddressSpace {
        match self {
            Self::Location(location) => location.address_space(),
            Self::Any(space) => space,
        }
    }
}

/// Memory and state accesses performed by one MIR operation.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ModRef {
    reads: SmallVec<[Access; 4]>,
    writes: SmallVec<[Access; 4]>,
    observes_memory_size: bool,
    observes_gas: bool,
}

impl ModRef {
    /// Returns exact and address-space-wide reads.
    #[must_use]
    pub fn reads(&self) -> &[Access] {
        &self.reads
    }

    /// Returns exact and address-space-wide writes.
    #[must_use]
    pub fn writes(&self) -> &[Access] {
        &self.writes
    }

    /// Returns whether the operation observes the active memory size.
    #[must_use]
    pub const fn observes_memory_size(&self) -> bool {
        self.observes_memory_size
    }

    /// Returns whether the operation observes remaining gas.
    #[must_use]
    pub const fn observes_gas(&self) -> bool {
        self.observes_gas
    }

    /// Returns whether any access reads `space`.
    #[must_use]
    pub fn reads_space(&self, space: AddressSpace) -> bool {
        self.reads.iter().any(|access| access.address_space() == space)
    }

    /// Returns whether any access writes `space`.
    #[must_use]
    pub fn writes_space(&self, space: AddressSpace) -> bool {
        self.writes.iter().any(|access| access.address_space() == space)
    }

    /// Returns whether this operation may read `location`.
    #[must_use]
    pub fn may_read(&self, aa: &AliasAnalysis, location: Location) -> bool {
        self.reads.iter().any(|&access| aa.access_may_alias(access, location))
    }

    /// Returns whether this operation may write `location`.
    #[must_use]
    pub fn may_write(&self, aa: &AliasAnalysis, location: Location) -> bool {
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

/// Shared value-local basic alias and ModRef analysis.
#[derive(Clone, Copy, Debug, Default)]
pub struct AliasAnalysis;

impl AliasAnalysis {
    /// Canonicalizes a MIR value used as a memory address.
    #[must_use]
    pub fn memory_address(self, func: &Function, value: ValueId) -> Option<MemoryAddress> {
        self.memory_address_with_depth(func, value, 0)
    }

    /// Creates a memory location, using instruction metadata to refine its region.
    #[must_use]
    pub fn memory_location(
        self,
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

    /// Creates a memory location without instruction metadata.
    #[must_use]
    pub fn bare_memory_location(
        self,
        func: &Function,
        address: ValueId,
        size: LocationSize,
    ) -> Option<MemoryLocation> {
        Some(MemoryLocation::new(self.memory_address(func, address)?, size))
    }

    /// Converts a MIR size operand to a canonical location size.
    #[must_use]
    pub fn location_size(self, func: &Function, value: ValueId) -> LocationSize {
        func.value_u64(value).map_or(LocationSize::Dynamic(value), LocationSize::Const)
    }

    /// Returns the canonical storage alias for an instruction operand.
    #[must_use]
    pub fn storage_alias(self, func: &Function, inst_id: InstId, slot: ValueId) -> StorageAlias {
        func.storage_alias(inst_id, slot)
    }

    /// Returns the canonical storage alias after value replacements.
    #[must_use]
    pub fn storage_alias_after_replacements(
        self,
        func: &Function,
        inst_id: InstId,
        slot: ValueId,
        replacements: &FxHashMap<ValueId, ValueId>,
    ) -> StorageAlias {
        func.storage_alias_after_replacements(inst_id, slot, replacements)
    }

    /// Computes the alias relationship between two locations.
    #[must_use]
    pub fn alias(self, first: Location, second: Location) -> AliasResult {
        match (first, second) {
            (Location::Memory(first), Location::Memory(second)) => self.memory_alias(first, second),
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
    pub fn instruction_mod_ref(self, func: &Function, inst_id: InstId) -> ModRef {
        self.instruction_mod_ref_with_replacements(func, inst_id, &FxHashMap::default())
    }

    /// Computes instruction ModRef effects after applying value replacements.
    #[must_use]
    pub fn instruction_mod_ref_with_replacements(
        self,
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
            InstKind::Alloc(_) => {
                let fmp = Access::Location(Location::Memory(Self::fmp_location()));
                effects.read(fmp);
                effects.write(fmp);
            }
            InstKind::AbiEncode { .. } => {
                effects.read_any(AddressSpace::Memory);
                effects.write_any(AddressSpace::Memory);
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
            InstKind::MappingSlotMemory(_, _) => effects.read_any(AddressSpace::Memory),
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
            InstKind::InternalCall { .. } => {
                effects.read_any(AddressSpace::Memory);
                effects.write_any(AddressSpace::Memory);
                effects.read_any(AddressSpace::Storage);
                effects.write_any(AddressSpace::Storage);
                effects.read_any(AddressSpace::Transient);
                effects.write_any(AddressSpace::Transient);
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
    pub fn terminator_mod_ref(self, func: &Function, terminator: &Terminator) -> ModRef {
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
            Terminator::Return { .. } | Terminator::TailCall { .. } => {
                effects.read_any(AddressSpace::Memory);
            }
            Terminator::Jump(_)
            | Terminator::Branch { .. }
            | Terminator::Switch { .. }
            | Terminator::Stop
            | Terminator::Invalid
            | Terminator::SelfDestruct { .. } => {}
        }
        effects
    }

    /// Returns the canonical free-memory-pointer word location.
    #[must_use]
    pub const fn fmp_location() -> MemoryLocation {
        MemoryLocation::new(
            MemoryAddress {
                region: MemoryRegion::Scratch,
                base: MemoryBase::Absolute,
                offset: 0x40,
            },
            LocationSize::Const(32),
        )
    }

    fn access_may_alias(self, access: Access, location: Location) -> bool {
        match access {
            Access::Any(space) => space == location.address_space(),
            Access::Location(other) => self.alias(other, location).may_alias(),
        }
    }

    fn memory_alias(self, first: MemoryLocation, second: MemoryLocation) -> AliasResult {
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
        self,
        func: &Function,
        value: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        if depth > 8 {
            return Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0)));
        }
        match func.value(value) {
            Value::Immediate(immediate) => {
                Some(MemoryAddress::absolute(immediate.as_u256()?.try_into().ok()?))
            }
            Value::Arg { .. } => Some(MemoryAddress::symbolic(value, MemoryRegion::Unknown)),
            Value::Undef(_) | Value::Error(_) => None,
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::InternalFrameAddr(offset) => Some(MemoryAddress::internal_frame(offset)),
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
                _ => Some(MemoryAddress::symbolic(value, self.pointer_region(func, value, 0))),
            },
        }
    }

    fn address_add(
        self,
        func: &Function,
        base: ValueId,
        offset: ValueId,
        depth: usize,
    ) -> Option<MemoryAddress> {
        self.memory_address_with_depth(func, base, depth + 1)?.checked_add(func.value_u64(offset)?)
    }

    fn address_sub(
        self,
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
        self,
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
                SliceLocation::Calldata => None,
            },
            InstKind::AbiEncode { .. } => Some(MemoryAddress::symbolic(slice, MemoryRegion::Heap)),
            _ => Some(MemoryAddress::symbolic(slice, MemoryRegion::Unknown)),
        }
    }

    fn pointer_region(self, func: &Function, value: ValueId, depth: usize) -> MemoryRegion {
        if depth > 8 {
            return MemoryRegion::Unknown;
        }
        let Value::Inst(inst_id) = func.value(value) else {
            return MemoryRegion::Unknown;
        };
        match func.instructions[*inst_id].kind {
            InstKind::InternalFrameAddr(_) => MemoryRegion::InternalFrame,
            InstKind::Fmp | InstKind::Alloc(_) => MemoryRegion::Heap,
            InstKind::MLoad(address) if func.value_u64(address) == Some(0x40) => MemoryRegion::Heap,
            InstKind::Add(first, second) => {
                let first = self.pointer_region(func, first, depth + 1);
                if first != MemoryRegion::Unknown {
                    first
                } else {
                    self.pointer_region(func, second, depth + 1)
                }
            }
            InstKind::Sub(base, _) => self.pointer_region(func, base, depth + 1),
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

    fn resolved_location_size(
        self,
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
        }
    }
}

#[derive(Clone, Copy)]
enum SizeOperand {
    Const(u64),
    Value(ValueId),
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
        let aa = AliasAnalysis;

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
        let aa = AliasAnalysis;
        let base = ValueId::from_usize(0);
        let location = |offset, size| {
            Location::Memory(MemoryLocation::new(
                MemoryAddress { region: MemoryRegion::Heap, base: MemoryBase::Value(base), offset },
                LocationSize::Const(size),
            ))
        };

        assert_eq!(aa.alias(location(0, 32), location(0, 32)), AliasResult::MustAlias);
        assert_eq!(aa.alias(location(0, 32), location(16, 32)), AliasResult::PartialAlias);
        assert_eq!(aa.alias(location(0, 32), location(32, 32)), AliasResult::NoAlias);

        let scratch = Location::Memory(AliasAnalysis::fmp_location());
        assert_eq!(aa.alias(scratch, location(0, 32)), AliasResult::NoAlias);
    }

    #[test]
    fn classifies_storage_aliases() {
        let aa = AliasAnalysis;
        let first = Location::Storage(StorageAlias::Slot(U256::from(1)));
        let second = Location::Storage(StorageAlias::Slot(U256::from(2)));
        assert_eq!(aa.alias(first, first), AliasResult::MustAlias);
        assert_eq!(aa.alias(first, second), AliasResult::NoAlias);

        let symbolic = Location::Storage(StorageAlias::Symbolic(ValueId::from_usize(0)));
        assert_eq!(aa.alias(first, symbolic), AliasResult::MayAlias);
        assert_eq!(
            aa.alias(first, Location::Transient(StorageAlias::Slot(U256::from(1)))),
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
        let aa = AliasAnalysis;
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
        let effects = AliasAnalysis.instruction_mod_ref(&func, call);

        assert!(effects.reads_space(AddressSpace::Storage));
        assert!(!effects.writes_space(AddressSpace::Storage));
        assert!(effects.reads_space(AddressSpace::Memory));
        assert!(effects.writes_space(AddressSpace::Memory));
    }
}

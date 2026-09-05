//! Physical memory ranges used to protect compiler-owned activation words.
//!
//! Region names do not prove disjointness: arbitrary pointer arithmetic can move a heap or frame
//! pointer into another region, and source code can reset the free-memory pointer. Only canonical
//! absolute addresses, exact current-frame offsets and reserved allocation identities are resolved
//! here. Unknown symbolic bases and unknown ranges remain possible aliases, regardless of metadata.
//!
//! Final frame layout resolves static offsets and deferred allocations. Dynamic activations begin
//! above the fixed-memory floor, permitting separation from fixed ranges below that floor. Before
//! layout, stack-residence selection only accepts low-memory writes and writes entirely contained
//! within a declared allocation. No instructions or value identities are rewritten by these checks.

use crate::{
    analysis::{Access, AddressSpace, Location, MemoryAddress, MemoryBase, ModRef},
    backend::evm::storage::{FrameAddress, FunctionStorage},
    memory::EvmMemoryLayout,
    mir,
};

/// Recognizes writes disjoint from every possible compiler-owned activation word before layout.
pub(super) fn disjoint_frame_write(function: &mir::Function, effects: &ModRef) -> bool {
    effects.writes().iter().all(|access| {
        let Access::Location(Location::Memory(location)) = access else { return false };
        let Some(size) = location.size.as_const() else { return false };
        if size == 0 {
            return true;
        }
        let Some(end) = location.address.offset.checked_add(size) else { return false };
        match location.address.base {
            MemoryBase::Absolute => end <= EvmMemoryLayout::HEAP_START,
            MemoryBase::Allocation(id) | MemoryBase::DynamicAllocation(id) => {
                matches!(function.inst(id).kind, mir::InstKind::Alloc { size, .. }
                    if function.value_u64(size).is_some_and(|capacity| end <= capacity))
            }
            _ => false,
        }
    })
}

/// Tests writes against one live home or protocol word using final physical storage locations.
pub(crate) fn may_overlap(
    storage: &FunctionStorage,
    fixed_memory_end: u64,
    effects: &ModRef,
    home: FrameAddress,
) -> bool {
    accesses_overlap(storage, fixed_memory_end, effects.writes(), home)
}

pub(crate) fn accesses_overlap(
    storage: &FunctionStorage,
    fixed_memory_end: u64,
    accesses: &[Access],
    home: FrameAddress,
) -> bool {
    accesses.iter().any(|access| match access {
        Access::Any(AddressSpace::Memory) => true,
        Access::Location(Location::Memory(location)) => {
            if location.size.as_const() == Some(0) {
                return false;
            }
            physical_address(storage, location.address).is_none_or(|address| {
                ranges_overlap(address, location.size.as_const(), home, fixed_memory_end)
            })
        }
        _ => false,
    })
}

fn physical_address(storage: &FunctionStorage, address: MemoryAddress) -> Option<FrameAddress> {
    match address.base {
        MemoryBase::Absolute => Some(FrameAddress::Absolute(address.offset)),
        MemoryBase::InternalFrame => storage.address(address.offset).ok(),
        MemoryBase::Allocation(id) | MemoryBase::DynamicAllocation(id) => storage
            .allocation_address(id)
            .ok()?
            .checked_add(address.offset)
            .map(FrameAddress::Absolute),
        MemoryBase::Value(_) => None,
    }
}

fn ranges_overlap(
    address: FrameAddress,
    size: Option<u64>,
    home: FrameAddress,
    fixed_memory_end: u64,
) -> bool {
    if size == Some(0) {
        return false;
    }
    match (address, home) {
        (FrameAddress::Absolute(start), FrameAddress::Absolute(home))
        | (FrameAddress::Relative(start), FrameAddress::Relative(home)) => {
            let Some(size) = size else { return true };
            start.checked_add(size).is_none_or(|end| end > home)
                && home.checked_add(EvmMemoryLayout::WORD_SIZE).is_none_or(|end| end > start)
        }
        (FrameAddress::Absolute(start), FrameAddress::Relative(_)) => {
            size.and_then(|size| start.checked_add(size)).is_none_or(|end| end > fixed_memory_end)
        }
        (FrameAddress::Relative(_), FrameAddress::Absolute(home)) => {
            home.checked_add(EvmMemoryLayout::WORD_SIZE).is_none_or(|end| end > fixed_memory_end)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FrameAddress::*, ranges_overlap};

    #[test]
    fn exact_ranges_and_dynamic_floor() {
        for home in [0, 32, 128, 160, 1024, u64::MAX - 31] {
            for size in [0, 1, 31, 32, 33, u64::MAX] {
                for start in [0, 31, 32, 127, 128, 159, 160, 161, 1024, u64::MAX] {
                    let expected = size != 0
                        && u128::from(start) < u128::from(home) + 32
                        && u128::from(home) < u128::from(start) + u128::from(size);
                    assert_eq!(
                        ranges_overlap(Absolute(start), Some(size), Absolute(home), 2048),
                        expected
                    );
                    assert_eq!(
                        ranges_overlap(Relative(start), Some(size), Relative(home), 2048),
                        expected
                    );
                }
            }
        }
        assert!(!ranges_overlap(Absolute(2016), Some(32), Relative(0), 2048));
        assert!(ranges_overlap(Absolute(2016), Some(33), Relative(0), 2048));
        assert!(ranges_overlap(Absolute(0), None, Relative(0), 2048));
        assert!(!ranges_overlap(Relative(0), None, Absolute(2016), 2048));
        assert!(ranges_overlap(Relative(0), None, Absolute(2017), 2048));
        assert!(ranges_overlap(Absolute(0), None, Absolute(0), 2048));
        assert!(ranges_overlap(Relative(0), None, Relative(0), 2048));
        assert!(!ranges_overlap(Absolute(u64::MAX), Some(0), Relative(0), 2048));
    }
}

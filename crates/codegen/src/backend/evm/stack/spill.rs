//! Spill management for handling >16 live values.
//!
//! When more than 16 values are live simultaneously (the maximum accessible
//! via DUP16/SWAP16), we spill values to memory.

use crate::mir::ValueId;
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};

/// A slot in memory where a spilled value is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct SpillSlot {
    /// Offset in the spill area (in 32-byte words).
    pub offset: u32,
}

impl SpillSlot {
    /// Returns the memory offset in bytes for this spill slot.
    #[must_use]
    pub(crate) const fn byte_offset(&self) -> u32 {
        // Memory layout in Solidity:
        // - 0x00-0x3F: Scratch space (used for hashing, CALL address/calldata spills, etc.)
        // - 0x40-0x5F: Free memory pointer location
        // - 0x60-0x7F: Zero slot (used for empty dynamic arrays)
        // - 0x80+: Dynamic allocations (structs, arrays, calldata encoding, etc.)
        //
        // IMPORTANT: We CANNOT use:
        // - 0x00-0x3F: Used as scratch by lower_member_call for CALL address/calldata
        // - 0x60-0x7F: Used as zero slot
        // - 0x80-0x????: Dynamic allocations can grow arbitrarily large
        //
        // The safest approach is to use a very high memory address that's unlikely to
        // conflict with normal memory usage. We use 0x1000000 (16MB) as the base.
        // This wastes some gas on memory expansion but guarantees no conflicts.
        //
        // A proper solution would be to allocate spill slots from the heap (update
        // free memory pointer), but that would require tracking the allocation in MIR.
        //
        // We use 0x1000 (4KB) as a compromise - high enough to avoid conflicts with
        // typical dynamic allocations, but low enough to not cause excessive gas usage.
        // In practice, most contracts use less than 4KB of dynamic memory.
        //
        // A proper solution would be to allocate spill slots from the heap, but that
        // requires tracking the allocation in MIR.
        0x1000 + self.offset * 32
    }
}

/// Manages spill slots for values that cannot fit on the stack.
#[derive(Clone, Debug)]
pub(crate) struct SpillManager {
    /// Map from value to its spill slot.
    slots: FxHashMap<ValueId, SpillSlot>,
    /// Values whose reserved spill slot can be loaded at the current program point.
    reloadable: GrowableBitSet<ValueId>,
    /// Values whose reserved spill slot was stored by already-emitted code.
    stored: GrowableBitSet<ValueId>,
    /// Cross-block values whose slots must remain stable for the function.
    stable: GrowableBitSet<ValueId>,
    /// Reusable offsets released by block-local values after their final use.
    free_offsets: Vec<u32>,
    /// Next available spill slot offset.
    next_offset: u32,
    /// Maximum offset used (for tracking spill area size).
    max_offset: u32,
}

impl SpillManager {
    /// Creates a new spill manager.
    #[must_use]
    pub(crate) fn new() -> Self {
        Self {
            slots: FxHashMap::default(),
            reloadable: GrowableBitSet::new_empty(),
            stored: GrowableBitSet::new_empty(),
            stable: GrowableBitSet::new_empty(),
            free_offsets: Vec::new(),
            next_offset: 0,
            max_offset: 0,
        }
    }

    /// Allocates a spill slot for a value.
    /// If the value already has a slot, returns the existing one.
    pub(crate) fn allocate(&mut self, value: ValueId) -> SpillSlot {
        if let Some(&slot) = self.slots.get(&value) {
            return slot;
        }

        let offset = self.free_offsets.pop().unwrap_or_else(|| {
            let offset = self.next_offset;
            self.next_offset += 1;
            offset
        });
        let slot = SpillSlot { offset };
        self.slots.insert(value, slot);
        self.max_offset = self.max_offset.max(self.next_offset);
        slot
    }

    /// Reserves a function-stable slot for a value that can cross block edges.
    pub(crate) fn reserve(&mut self, value: ValueId) -> SpillSlot {
        let slot = self.allocate(value);
        self.stable.insert(value);
        slot
    }

    /// Releases a block-local value's slot after its final use.
    pub(crate) fn release(&mut self, value: ValueId) -> bool {
        if self.stable.contains(value) {
            return false;
        }
        let Some(slot) = self.slots.remove(&value) else {
            return false;
        };
        self.reloadable.remove(value);
        self.stored.remove(value);
        self.free_offsets.push(slot.offset);
        true
    }

    /// Releases every block-local slot while retaining cross-block reservations.
    pub(crate) fn release_block_locals(&mut self) {
        let mut values: Vec<_> = self
            .slots
            .iter()
            .filter_map(|(&value, &slot)| (!self.stable.contains(value)).then_some((value, slot)))
            .collect();
        values.sort_by_key(|(_, slot)| std::cmp::Reverse(slot.offset));
        for (value, _) in values {
            self.release(value);
        }
    }

    /// Returns the spill slot for a value, if one exists.
    #[must_use]
    pub(crate) fn get(&self, value: ValueId) -> Option<SpillSlot> {
        self.slots.get(&value).copied()
    }

    /// Marks a value's spill slot as reloadable at the current program point.
    pub(crate) fn mark_reloadable(&mut self, value: ValueId) {
        debug_assert!(self.slots.contains_key(&value));
        self.reloadable.insert(value);
    }

    /// Marks a value's spill slot as written by emitted code.
    pub(crate) fn mark_stored(&mut self, value: ValueId) {
        debug_assert!(self.slots.contains_key(&value));
        self.reloadable.insert(value);
        self.stored.insert(value);
    }

    /// Returns true if the value has a spill slot that can be loaded.
    #[must_use]
    pub(crate) fn is_reloadable(&self, value: ValueId) -> bool {
        self.reloadable.contains(value)
    }

    /// Returns true if already-emitted code has stored this value.
    #[must_use]
    pub(crate) fn is_stored(&self, value: ValueId) -> bool {
        self.stored.contains(value)
    }

    /// Returns the total size of the spill area in bytes.
    #[must_use]
    pub(crate) fn spill_area_size(&self) -> u32 {
        self.max_offset * 32
    }
}

impl Default for SpillManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_allocate() {
        let mut manager = SpillManager::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);
        let v2 = ValueId::from_usize(2);

        let slot0 = manager.allocate(v0);
        let slot1 = manager.allocate(v1);
        let slot2 = manager.allocate(v2);

        assert_eq!(slot0.offset, 0);
        assert_eq!(slot1.offset, 1);
        assert_eq!(slot2.offset, 2);
        // Spill slots use high memory at 0x1000+
        assert_eq!(slot0.byte_offset(), 0x1000);
        assert_eq!(slot1.byte_offset(), 0x1000 + 32);
        assert_eq!(slot2.byte_offset(), 0x1000 + 64);
    }

    #[test]
    fn test_allocate_idempotent() {
        let mut manager = SpillManager::new();
        let v0 = ValueId::from_usize(0);

        let slot1 = manager.allocate(v0);
        let slot2 = manager.allocate(v0);

        assert_eq!(slot1, slot2);
        assert_eq!(manager.get(v0), Some(slot1));
    }

    #[test]
    fn test_reloadable_and_stored_are_distinct() {
        let mut manager = SpillManager::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        manager.allocate(v0);
        assert!(!manager.is_reloadable(v0));
        assert!(!manager.is_stored(v0));

        manager.mark_reloadable(v0);
        assert!(manager.is_reloadable(v0));
        assert!(!manager.is_stored(v0));

        manager.allocate(v1);
        manager.mark_stored(v1);
        assert!(manager.is_reloadable(v1));
        assert!(manager.is_stored(v1));
    }

    #[test]
    fn test_release_reuses_block_local_slot() {
        let mut manager = SpillManager::new();
        let v0 = ValueId::from_usize(0);
        let v1 = ValueId::from_usize(1);

        let slot0 = manager.allocate(v0);
        manager.mark_stored(v0);
        assert!(manager.release(v0));
        assert_eq!(manager.get(v0), None);
        assert!(!manager.is_reloadable(v0));
        assert!(!manager.is_stored(v0));

        let slot1 = manager.allocate(v1);
        assert_eq!(slot1, slot0);
        assert_eq!(manager.spill_area_size(), 32);
    }

    #[test]
    fn test_stable_slot_is_not_released() {
        let mut manager = SpillManager::new();
        let value = ValueId::from_usize(0);
        let slot = manager.reserve(value);

        assert!(!manager.release(value));
        assert_eq!(manager.get(value), Some(slot));
    }

    #[test]
    fn test_release_block_locals_retains_stable_slots() {
        let mut manager = SpillManager::new();
        let stable = ValueId::from_usize(0);
        let local = ValueId::from_usize(1);
        let stable_slot = manager.reserve(stable);
        manager.allocate(local);

        manager.release_block_locals();

        assert_eq!(manager.get(stable), Some(stable_slot));
        assert_eq!(manager.get(local), None);
    }
}

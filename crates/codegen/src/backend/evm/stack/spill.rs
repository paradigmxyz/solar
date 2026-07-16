//! Spill management for handling >16 live values.
//!
//! When more than 16 values are live simultaneously (the maximum accessible
//! via DUP16/SWAP16), we spill values to memory.

use crate::{memory::EvmMemoryLayout, mir::ValueId};
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
        // Stack scheduling is already a physical backend phase, so spill slots
        // use the absolute area selected by the shared EVM memory policy.
        (EvmMemoryLayout::SPILL_BASE as u32) + self.offset * (EvmMemoryLayout::WORD_SIZE as u32)
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

        let slot = SpillSlot { offset: self.next_offset };
        self.slots.insert(value, slot);
        self.next_offset += 1;
        self.max_offset = self.max_offset.max(self.next_offset);
        slot
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
        // Spill slots use the backend spill area from the memory policy.
        assert_eq!(slot0.byte_offset(), EvmMemoryLayout::SPILL_BASE as u32);
        assert_eq!(slot1.byte_offset(), EvmMemoryLayout::SPILL_BASE as u32 + 32);
        assert_eq!(slot2.byte_offset(), EvmMemoryLayout::SPILL_BASE as u32 + 64);
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
}

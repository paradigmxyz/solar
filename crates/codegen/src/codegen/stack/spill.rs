//! Spill management for handling >16 live values.
//!
//! When more than 16 values are live simultaneously (the maximum accessible
//! via DUP16/SWAP16), we spill values to memory.

use crate::mir::ValueId;
use rustc_hash::FxHashMap;

/// A slot in memory where a spilled value is stored.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SpillSlot {
    /// Offset in the spill area (in 32-byte words).
    pub offset: u32,
}

impl SpillSlot {
    /// Returns the memory offset in bytes for this spill slot.
    #[must_use]
    pub const fn byte_offset(&self) -> u32 {
        // Spill area is at a high fixed address to avoid conflicts with:
        // - Scratch memory (0x00-0x7F) used by external calls
        // - Dynamic allocations starting at free memory pointer (0x80+)
        // Using 0x10000 (64KB) as base - far above typical allocation sizes
        0x10000 + self.offset * 32
    }
}

/// Manages spill slots for values that cannot fit on the stack.
#[derive(Clone, Debug)]
pub struct SpillManager {
    /// Map from value to its spill slot.
    slots: FxHashMap<ValueId, SpillSlot>,
    /// Next available spill slot offset.
    next_offset: u32,
    /// Maximum offset used (for tracking spill area size).
    max_offset: u32,
}

impl SpillManager {
    /// Creates a new spill manager.
    #[must_use]
    pub fn new() -> Self {
        Self { slots: FxHashMap::default(), next_offset: 0, max_offset: 0 }
    }

    /// Allocates a spill slot for a value.
    /// If the value already has a slot, returns the existing one.
    pub fn allocate(&mut self, value: ValueId) -> SpillSlot {
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
    pub fn get(&self, value: ValueId) -> Option<SpillSlot> {
        self.slots.get(&value).copied()
    }

    /// Returns true if the value is currently spilled.
    #[must_use]
    pub fn is_spilled(&self, value: ValueId) -> bool {
        self.slots.contains_key(&value)
    }

    /// Frees a spill slot (when the value is reloaded and no longer needed in memory).
    /// Note: Simple implementation doesn't reuse slots.
    pub fn free(&mut self, value: ValueId) {
        self.slots.remove(&value);
    }

    /// Returns the total size of the spill area in bytes.
    #[must_use]
    pub fn spill_area_size(&self) -> u32 {
        self.max_offset * 32
    }

    /// Clears all spill slots (used at function boundaries).
    pub fn clear(&mut self) {
        self.slots.clear();
        self.next_offset = 0;
    }

    /// Returns the number of currently spilled values.
    #[must_use]
    pub fn count(&self) -> usize {
        self.slots.len()
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

        let slot0 = manager.allocate(v0);
        let slot1 = manager.allocate(v1);

        assert_eq!(slot0.offset, 0);
        assert_eq!(slot1.offset, 1);
        assert_eq!(slot0.byte_offset(), 0x80);
        assert_eq!(slot1.byte_offset(), 0x80 + 32);
    }

    #[test]
    fn test_allocate_idempotent() {
        let mut manager = SpillManager::new();
        let v0 = ValueId::from_usize(0);

        let slot1 = manager.allocate(v0);
        let slot2 = manager.allocate(v0);

        assert_eq!(slot1, slot2);
        assert_eq!(manager.count(), 1);
    }

    #[test]
    fn test_free() {
        let mut manager = SpillManager::new();
        let v0 = ValueId::from_usize(0);

        manager.allocate(v0);
        assert!(manager.is_spilled(v0));

        manager.free(v0);
        assert!(!manager.is_spilled(v0));
    }
}

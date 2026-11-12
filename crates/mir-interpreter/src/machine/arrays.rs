//! Direct dynamic storage-array semantics, independent of builtin expansion.
//!
//! Allocate before hashing the data slot, read packed elements one storage word
//! at a time, and retain signed/fixed-byte encodings and enum panics. Bytes
//! elements load their own objects in order, sharing memory and scratch space.
//! More than 2048 elements is unsupported, never a shortened successful copy.

use super::{Outcome, Run, limit, panic_data};
use alloy_primitives::U256;
use solar_mir::{AllocationSemantics, MemoryObjectKind, ValueLayout};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn load_storage_array(
        &mut self,
        slot: U256,
        element: ValueLayout,
        variants: Option<u64>,
    ) -> ControlFlow<Outcome, U256> {
        let length = self.storage_read(slot)?;
        if length > U256::from(u64::MAX / 32) {
            return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x41))));
        }
        let pointer = self.semantic_allocate(
            (length + U256::ONE) * U256::from(32),
            AllocationSemantics::SOLIDITY_UNINITIALIZED,
        )?;
        limit(self.memory.store(pointer, length))?;
        let base = self.fixed_slot(&[slot])?;
        if length > U256::from(2048) {
            return ControlFlow::Break(Outcome::Unsupported(
                "MIR storage array work limit of 2048 elements",
            ));
        }
        let (bits, per_slot) = match element {
            ValueLayout::UInt(size) | ValueLayout::Int(size) | ValueLayout::FixedBytes(size) => {
                let bits = usize::from(size.bytes()) * 8;
                (bits, 256 / bits)
            }
            ValueLayout::MemoryObject(MemoryObjectKind::Bytes) => (256, 1),
            _ => return ControlFlow::Break(Outcome::Unsupported("storage array element layout")),
        };
        let mut packed = U256::ZERO;
        self.burn(length.to::<u64>())?;
        for index in 0..length.to::<usize>() {
            let in_slot = index % per_slot;
            let value = if element == ValueLayout::MemoryObject(MemoryObjectKind::Bytes) {
                self.load_storage_bytes(base.wrapping_add(U256::from(index)))?
            } else {
                if in_slot == 0 {
                    packed = self.storage_read(base.wrapping_add(U256::from(index / per_slot)))?;
                }
                let value = (packed >> (in_slot * bits)) & (U256::MAX >> (256 - bits));
                match element {
                    ValueLayout::Int(_) if value.bit(bits - 1) => {
                        value | !(U256::MAX >> (256 - bits))
                    }
                    ValueLayout::FixedBytes(_) => value << (256 - bits),
                    _ => value,
                }
            };
            if variants.is_some_and(|bound| value >= U256::from(bound)) {
                return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x21))));
            }
            limit(self.memory.store(pointer.wrapping_add(U256::from(32 + 32 * index)), value))?;
        }
        ControlFlow::Continue(pointer)
    }
}

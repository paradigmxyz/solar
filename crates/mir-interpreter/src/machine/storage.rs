//! Direct Solidity byte-string storage semantics. Header checks precede reads or
//! mutation; long tails are cleared by word index, even when slot addresses wrap.
//! Full-word memory traffic and scratch hashing remain visible. Work limits are
//! interpreter errors, not truncated storage operations or successful outcomes.

use super::{Outcome, Run, calldata_word, limit, panic_data};
use alloy_primitives::U256;
use solar_evmir::op;
use solar_mir::AllocationSemantics;
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn bytes_header(header: U256) -> ControlFlow<Outcome, (bool, U256)> {
        let long = header.bit(0);
        let length = if long { header >> 1 } else { (header >> 1) & U256::from(127) };
        if long == (length < U256::from(32)) {
            ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x22))))
        } else {
            ControlFlow::Continue((long, length))
        }
    }

    pub(super) fn storage_read(&mut self, slot: U256) -> ControlFlow<Outcome, U256> {
        let Some(word) = self.world_opcode(op::SLOAD, &[slot])? else {
            return ControlFlow::Break(Outcome::Unsupported("missing storage read"));
        };
        ControlFlow::Continue(word)
    }

    pub(super) fn clear_storage_words(
        &mut self,
        slot: U256,
        first: U256,
        last: U256,
    ) -> ControlFlow<Outcome> {
        let base = self.fixed_slot(&[slot])?;
        let count = last.saturating_sub(first);
        if count > U256::from(2048) {
            return ControlFlow::Break(Outcome::Unsupported(
                "MIR storage work limit of 2048 words",
            ));
        }
        self.burn(count.to::<u64>())?;
        for index in 0..count.to::<u64>() {
            self.world_opcode(
                op::SSTORE,
                &[base.wrapping_add(first).wrapping_add(U256::from(index)), U256::ZERO],
            )?;
        }
        ControlFlow::Continue(())
    }

    pub(super) fn load_storage_bytes(&mut self, slot: U256) -> ControlFlow<Outcome, U256> {
        let header = self.storage_read(slot)?;
        let (long, length) = Self::bytes_header(header)?;
        let Some(size) = length.checked_add(U256::from(63)).map(|n| n & !U256::from(31)) else {
            return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x41))));
        };
        let pointer = self.semantic_allocate(size, AllocationSemantics::SOLIDITY_UNINITIALIZED)?;
        limit(self.memory.store(pointer, length))?;
        if long {
            let base = self.fixed_slot(&[slot])?;
            let count = (length + U256::from(31)) / U256::from(32);
            if count > U256::from(2048) {
                return ControlFlow::Break(Outcome::Unsupported(
                    "MIR storage work limit of 2048 words",
                ));
            }
            self.burn(count.to::<u64>())?;
            for index in 0..count.to::<u64>() {
                let word = self.storage_read(base.wrapping_add(U256::from(index)))?;
                limit(self.memory.store(pointer.wrapping_add(U256::from(32 + 32 * index)), word))?;
            }
        } else {
            limit(
                self.memory.store(pointer.wrapping_add(U256::from(32)), header & !U256::from(255)),
            )?;
        }
        ControlFlow::Continue(pointer)
    }

    pub(super) fn store_storage_bytes(
        &mut self,
        slot: U256,
        object: U256,
        literal: Option<&[u8]>,
    ) -> ControlFlow<Outcome> {
        let header = self.storage_read(slot)?;
        let (old_long, old_length) = Self::bytes_header(header)?;
        let length = if let Some(bytes) = literal {
            U256::from(bytes.len())
        } else {
            limit(self.memory.load(object))?
        };
        let Some(rounded) = length.checked_add(U256::from(31)) else {
            return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x11))));
        };
        let short = length < U256::from(32);
        let count = if short { U256::ZERO } else { rounded / U256::from(32) };
        if old_long && old_length > length {
            self.clear_storage_words(slot, count, (old_length + U256::from(31)) / U256::from(32))?;
        }
        if short {
            let data = if let Some(bytes) = literal {
                calldata_word(bytes, U256::ZERO)
            } else {
                limit(self.memory.load(object.wrapping_add(U256::from(32))))?
            };
            let mask = if length.is_zero() {
                U256::ZERO
            } else {
                U256::MAX << (8 * (32 - length.to::<usize>()))
            };
            self.world_opcode(op::SSTORE, &[slot, (data & mask) | (length * U256::from(2))])?;
        } else {
            self.world_opcode(
                op::SSTORE,
                &[slot, length.wrapping_mul(U256::from(2)).wrapping_add(U256::from(1))],
            )?;
            let base = self.fixed_slot(&[slot])?;
            if count > U256::from(2048) {
                return ControlFlow::Break(Outcome::Unsupported(
                    "MIR storage work limit of 2048 words",
                ));
            }
            self.burn(count.to::<u64>())?;
            for index in 0..count.to::<u64>() {
                let offset = U256::from(32 * index);
                let data = if let Some(bytes) = literal {
                    calldata_word(bytes, offset)
                } else {
                    limit(
                        self.memory.load(object.wrapping_add(U256::from(32)).wrapping_add(offset)),
                    )?
                };
                let keep = (length - offset).min(U256::from(32)).to::<usize>();
                let mask = U256::MAX << (8 * (32 - keep));
                self.world_opcode(
                    op::SSTORE,
                    &[base.wrapping_add(U256::from(index)), data & mask],
                )?;
            }
        }
        ControlFlow::Continue(())
    }
}

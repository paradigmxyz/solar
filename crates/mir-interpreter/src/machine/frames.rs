//! Logical frame-slot traffic with explicit, convention-defined addresses.
//!
//! External locals use the shared region at 128; the multi-return word names
//! the buffer-pointer slot at 32. Slices retain pointer/length store ordering.
//! Internal frame placement is not guessed: it requires backend context and is
//! unsupported in direct semantic execution.

use super::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_mir::{FrameMode, FrameSlotKind, memory::EvmMemoryLayout};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn semantic_frame_address(
        offset: u64,
        mode: FrameMode,
        kind: FrameSlotKind,
    ) -> ControlFlow<Outcome, U256> {
        match mode {
            FrameMode::External => {
                ControlFlow::Continue(U256::from(EvmMemoryLayout::HEAP_START) + U256::from(offset))
            }
            FrameMode::MultiReturn if offset == 0 && kind == FrameSlotKind::Word => {
                ControlFlow::Continue(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT))
            }
            _ => ControlFlow::Break(Outcome::Unsupported("MIR frame needs backend placement")),
        }
    }

    pub(super) fn semantic_frame_load(
        &mut self,
        address: U256,
        kind: FrameSlotKind,
    ) -> ControlFlow<Outcome, RuntimeValue> {
        let first = limit(self.memory.load(address))?;
        let value = match kind {
            FrameSlotKind::Word => RuntimeValue::Word(first),
            FrameSlotKind::Slice(_) => {
                let length = limit(self.memory.load(address.wrapping_add(U256::from(32))))?;
                RuntimeValue::Aggregate(vec![RuntimeValue::Word(first), RuntimeValue::Word(length)])
            }
        };
        ControlFlow::Continue(value)
    }

    pub(super) fn semantic_frame_store(
        &mut self,
        address: U256,
        kind: FrameSlotKind,
        value: RuntimeValue,
    ) -> ControlFlow<Outcome> {
        let fields = match (kind, value) {
            (FrameSlotKind::Word, RuntimeValue::Word(word)) => vec![word],
            (FrameSlotKind::Slice(_), RuntimeValue::Aggregate(fields)) if fields.len() == 2 => {
                let [RuntimeValue::Word(pointer), RuntimeValue::Word(length)] = fields.as_slice()
                else {
                    return ControlFlow::Break(Outcome::Unsupported("undefined MIR frame slice"));
                };
                vec![*pointer, *length]
            }
            _ => return ControlFlow::Break(Outcome::Unsupported("MIR frame slot value shape")),
        };
        for (index, word) in fields.into_iter().enumerate() {
            limit(self.memory.store(address.wrapping_add(U256::from(32 * index)), word))?;
        }
        ControlFlow::Continue(())
    }
}

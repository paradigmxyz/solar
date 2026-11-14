//! Logical frame-slot traffic with explicit, convention-defined addresses.
//!
//! External locals use the shared region at 128; the multi-return word names
//! the buffer-pointer slot at 32. Slices retain pointer/length store ordering.
//! Internal frames use explicitly supplied bases for each live activation of
//! their function. No backend records or implicit frame setup supplies them.

use super::{Outcome, PlacementRead, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_mir::{FrameMode, FrameSlotKind, Function, memory::EvmMemoryLayout};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn semantic_frame_address(
        &mut self,
        body: &Function,
        offset: u64,
        mode: FrameMode,
        kind: FrameSlotKind,
    ) -> ControlFlow<Outcome, U256> {
        match mode {
            FrameMode::Internal => {
                let Some(offset) = EvmMemoryLayout::internal_frame_prefix(
                    body.params.len() + body.return_components().len(),
                )
                .and_then(|prefix| prefix.checked_add(offset)) else {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "internal frame prefix overflow",
                    ));
                };
                let activation =
                    self.frames.iter().filter(|frame| std::ptr::eq(frame.body, body)).count() - 1;
                let base =
                    self.placement(PlacementRead::FrameBase(&body.name.to_string(), activation))?;
                ControlFlow::Continue(base.wrapping_add(U256::from(offset)))
            }
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

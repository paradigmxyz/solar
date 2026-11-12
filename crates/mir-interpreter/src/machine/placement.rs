//! Explicit link and placement context for direct MIR windows.
//!
//! These operations read supplied frame bases, linked extents and relocation
//! values. Constructor immutables alias their staging memory; runtime values
//! cannot be written. No backend is invoked and no missing input defaults to zero.
//! GAS is admitted only as a single-opcode sampling window, not a priced MIR body.

use super::{Outcome, PlacementRead, Run, limit};
use alloy_primitives::U256;
use solar_mir::{Function, ImmutableId, InstId, Terminator, ValueLayout};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn placement(&mut self, query: PlacementRead<'_>) -> ControlFlow<Outcome, U256> {
        self.world.as_mut().and_then(|w| w.host.placement(query)).map_or_else(
            || {
                ControlFlow::Break(Outcome::Unsupported(
                    "missing explicit MIR placement/link context",
                ))
            },
            ControlFlow::Continue,
        )
    }

    pub(super) fn semantic_immutable(
        &mut self,
        id: ImmutableId,
        value: Option<U256>,
    ) -> ControlFlow<Outcome, U256> {
        if !matches!(self.machine.module.immutable_type(id), ValueLayout::UInt(size) if size.bits()==256)
        {
            return ControlFlow::Break(Outcome::Unsupported("non-word immutable representation"));
        }
        let slot = self
            .world
            .as_mut()
            .and_then(|w| w.host.placement(PlacementRead::ImmutableSlot(id.index())));
        if let Some(slot) = slot {
            if let Some(word) = value {
                limit(self.memory.store(slot, word))?;
                return ControlFlow::Continue(word);
            }
            return limit(self.memory.load(slot));
        }
        if value.is_some() {
            return ControlFlow::Break(Outcome::Unsupported(
                "immutable write requires constructor staging",
            ));
        }
        self.placement(PlacementRead::ImmutableValue(id.index()))
    }

    pub(super) fn gas_window(&self, body: &Function, inst: InstId) -> bool {
        if self.frames.len() != 1 || body.blocks.len() != 1 || body.instructions().count() != 1 {
            return false;
        }
        matches!(&body.blocks[solar_mir::BlockId::ENTRY].terminator,
            Some(Terminator::Return {values}) if values.as_slice()==body.inst(inst).result().as_slice())
    }
}

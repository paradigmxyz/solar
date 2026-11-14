//! Return-buffer publication independent of lowering or backend placement inference.
//!
//! A supplied semantic placement selects a physical convention for aggregate or
//! slice results. Their leaves are ordered by the declared types, with extras at
//! buffer + 32*k before publishing the pointer at 0x20. Unplaced single logical
//! products retain their SSA-only behavior. Expanded scalar returns require a
//! placement. Tail calls pass their original return owner to this helper.
//! The first physical word is not read here: its caller may discard it, including
//! when it is undefined. Undefined extra words cannot be stored or invented.

use super::{Outcome, PlacementRead, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_mir::{Function, memory::EvmMemoryLayout};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn publish_semantic_returns(
        &mut self,
        owner: &Function,
        values: &[RuntimeValue],
    ) -> ControlFlow<Outcome> {
        let buffer = self.world.as_mut().and_then(|world| {
            world.host.placement(PlacementRead::ReturnBuffer(&owner.name.to_string()))
        });
        if values.len() <= 1 && buffer.is_none() {
            return ControlFlow::Continue(());
        }
        if values.len() != owner.return_components().len() {
            return ControlFlow::Break(Outcome::Unsupported("MIR return owner arity mismatch"));
        }
        let mut leaves = Vec::new();
        for (value, &ty) in values.iter().zip(owner.return_components()) {
            // Independently bound callee types, not just the selected outer signature.
            if let Err(reason) = crate::values::widths(self.machine.module, ty) {
                return ControlFlow::Break(Outcome::Unsupported(reason));
            }
            if let Err(reason) = value.leaves(self.machine.module, ty, &mut leaves) {
                return ControlFlow::Break(Outcome::Unsupported(reason));
            }
        }
        if leaves.len() <= 1 {
            return ControlFlow::Continue(());
        }
        let Some(buffer) = buffer else {
            return ControlFlow::Break(Outcome::Unsupported("missing MIR return buffer placement"));
        };
        let mut extras = Vec::new();
        for value in &leaves[1..] {
            let RuntimeValue::Word(word) = value else {
                return ControlFlow::Break(Outcome::Unsupported(
                    "undefined MIR physical return component",
                ));
            };
            extras.push(*word);
        }
        self.publish_return_extras(buffer, &extras)
    }

    pub(super) fn publish_multi_return(
        &mut self,
        body: &Function,
        values: &[U256],
    ) -> ControlFlow<Outcome> {
        if self.transaction.is_none() {
            return ControlFlow::Break(Outcome::Unsupported("call returning several values"));
        }
        let Some(layout) = &self.machine.module.backend_memory else {
            return ControlFlow::Break(Outcome::Unsupported(
                "return buffer without backend placement",
            ));
        };
        let function = self.machine.function_id(body);
        let Some(&buffer) = layout.returns.get(&function) else {
            return ControlFlow::Break(Outcome::Unsupported(
                "missing backend return buffer placement",
            ));
        };
        self.publish_return_extras(buffer, &values[1..])
    }

    fn publish_return_extras(&mut self, buffer: U256, extras: &[U256]) -> ControlFlow<Outcome> {
        // mstore buffer + 32 * k, result k
        // mstore 0x20, buffer
        for (index, &value) in extras.iter().enumerate() {
            let offset =
                buffer.wrapping_add(U256::from((index as u64 + 1) * EvmMemoryLayout::WORD_SIZE));
            limit(self.memory.store(offset, value))?;
        }
        limit(self.memory.store(U256::from(EvmMemoryLayout::MULTI_RETURN_BUFFER_PTR_SLOT), buffer))
    }
}

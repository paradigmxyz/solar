//! Direct program-data copying at an explicit link boundary.
//!
//! Literal entries retain their bytes and library patches. Deferred entries and
//! reads past an entry require a supplied code image and data extent. Known data
//! must match that image after patching; deferred images are boundary inputs, not
//! compiled or inferred by the interpreter. Empty copies read no link inputs.

use super::{Outcome, PlacementRead, Run, calldata_byte, limit};
use alloy_primitives::U256;
use solar_mir::DataRef;
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn data_copy(
        &mut self,
        data: DataRef,
        dest: U256,
        len: U256,
    ) -> ControlFlow<Outcome> {
        self.burn_words(len)?;
        if len.is_zero() {
            return ControlFlow::Continue(());
        }
        let entry = self.machine.module.data.get(data.id).expect("verified data reference");
        let mut known = entry.bytes.known().map(|bytes| bytes.to_vec());
        for relocation in &entry.library_relocations {
            let address = self.placement(PlacementRead::Library(relocation.library.index()))?;
            if address > U256::MAX >> 96 {
                return ControlFlow::Break(Outcome::Unsupported("invalid library relocation"));
            }
            let Some(bytes) = known.as_mut().and_then(|bytes| {
                bytes.get_mut(relocation.offset..relocation.offset.saturating_add(20))
            }) else {
                return ControlFlow::Break(Outcome::Unsupported("invalid data relocation"));
            };
            bytes.copy_from_slice(&address.to_be_bytes::<32>()[12..]);
        }
        let base = self
            .world
            .as_mut()
            .and_then(|world| world.host.placement(PlacementRead::DataOffset(data.id.index())));
        if let Some(base) = base {
            let size = self.placement(PlacementRead::DataSize(data.id.index()))?;
            let Some(code) = self.world.as_mut().and_then(|world| world.host.code_image()) else {
                return ControlFlow::Break(Outcome::Unsupported("missing explicit MIR code image"));
            };
            let (Ok(start), Ok(size)) = (usize::try_from(base), usize::try_from(size)) else {
                return ControlFlow::Break(Outcome::Unsupported("invalid linked data extent"));
            };
            let Some(entry) = start.checked_add(size).and_then(|end| code.get(start..end)) else {
                return ControlFlow::Break(Outcome::Unsupported("linked data outside code image"));
            };
            if known.as_ref().is_some_and(|bytes| bytes != entry) {
                return ControlFlow::Break(Outcome::Unsupported(
                    "linked data disagrees with declaration",
                ));
            }
            let source = base + U256::from(data.offset);
            let bytes = (0..len.to::<u64>())
                .map(|index| calldata_byte(&code, source, index))
                .collect::<Vec<_>>();
            return limit(self.memory.write(dest, &bytes));
        }
        let Some(bytes) = known else {
            return ControlFlow::Break(Outcome::Unsupported("missing linked data extent"));
        };
        let start = data.offset as usize;
        let Some(bytes) = bytes.get(start..start.saturating_add(len.to::<usize>())) else {
            return ControlFlow::Break(Outcome::Unsupported("datacopy past its data"));
        };
        limit(self.memory.write(dest, bytes))
    }
}

//! Static packed ABI memory semantics. Narrow scalars can share one word with
//! adjacent literals up to a total width of 32 bytes. Other pieces retain their
//! individual word stores, including unaligned tails. Scratch placement depends
//! on the last byte written, not just the length hashed. This is independent of
//! compiler expansion and retains exact MSIZE and overwritten padding.

use crate::machine::{Outcome, Run, calldata_word, limit};
use alloy_primitives::{U256, keccak256};
use solar_mir::{AllocationSemantics, PackedPart, ValueLayout};
use std::ops::ControlFlow;

pub(super) struct Piece {
    pub(super) bytes: Vec<u8>,
    pub(super) scalar: bool,
}

fn emit(bytes: &[u8], offset: &mut usize, writes: &mut Vec<(usize, U256)>) {
    for (index, chunk) in bytes.chunks(32).enumerate() {
        writes.push((*offset + index * 32, calldata_word(chunk, U256::ZERO)));
    }
    *offset += bytes.len();
}

fn flush(pending: &mut Vec<&Piece>, offset: &mut usize, writes: &mut Vec<(usize, U256)>) {
    if pending.len() >= 2 && pending.iter().any(|piece| piece.scalar) {
        let bytes =
            pending.iter().flat_map(|piece| piece.bytes.iter().copied()).collect::<Vec<_>>();
        emit(&bytes, offset, writes);
    } else {
        for piece in pending.iter() {
            emit(&piece.bytes, offset, writes);
        }
    }
    pending.clear();
}

pub(super) fn word_stores(pieces: &[Piece]) -> Vec<(usize, U256)> {
    let mut offset = 0;
    let mut writes = Vec::new();
    let mut pending = Vec::new();
    let mut pending_length = 0;
    for piece in pieces {
        let size = piece.bytes.len();
        if size > 32 || piece.scalar && size == 32 {
            flush(&mut pending, &mut offset, &mut writes);
            pending_length = 0;
            emit(&piece.bytes, &mut offset, &mut writes);
        } else {
            if pending_length + size > 32 {
                flush(&mut pending, &mut offset, &mut writes);
                pending_length = 0;
            }
            pending.push(piece);
            pending_length += size;
        }
    }
    flush(&mut pending, &mut offset, &mut writes);
    writes
}

impl Run<'_, '_, '_> {
    pub(in crate::machine) fn semantic_packed(
        &mut self,
        parts: &[PackedPart],
        values: &[U256],
        hash: bool,
    ) -> ControlFlow<Outcome, U256> {
        if parts.len() > 2048 {
            return ControlFlow::Break(Outcome::Unsupported("packed part limit of 2048"));
        }
        let mut values = values.iter();
        let mut pieces = Vec::new();
        let mut length = 0;
        for part in parts {
            let (bytes, scalar) = match part {
                PackedPart::Literal(bytes) => (bytes.to_vec(), false),
                PackedPart::Scalar { ty, .. } => {
                    let Some(size) = ty.type_size() else {
                        return ControlFlow::Break(Outcome::Unsupported("packed scalar layout"));
                    };
                    let size = usize::from(size.bytes());
                    if size == 0 || size > 32 {
                        return ControlFlow::Break(Outcome::Unsupported("packed scalar width"));
                    }
                    let Some(value) = values.next() else {
                        return ControlFlow::Break(Outcome::Unsupported("packed scalar operand"));
                    };
                    let word = value.to_be_bytes::<32>();
                    (
                        (if matches!(ty, ValueLayout::FixedBytes(_)) {
                            &word[..size]
                        } else {
                            &word[32 - size..]
                        })
                        .to_vec(),
                        true,
                    )
                }
                _ => return ControlFlow::Break(Outcome::Unsupported("packed reference layout")),
            };
            length += bytes.len();
            if length > 65536 {
                return ControlFlow::Break(Outcome::Unsupported("packed bytes exceed 64 KiB"));
            }
            pieces.push(Piece { bytes, scalar });
        }
        let aligned = pieces.iter().all(|piece| piece.bytes.len().is_multiple_of(32));
        let writes = word_stores(&pieces);
        let write_end = writes.iter().map(|(offset, _)| offset + 32).max().unwrap_or_default();
        let length = U256::from(length);
        let base = if hash {
            if write_end <= 64 { U256::ZERO } else { limit(self.memory.load(U256::from(64)))? }
        } else {
            let object = self.semantic_allocate(
                (length + U256::from(63)) & !U256::from(31),
                AllocationSemantics::INTERNAL,
            )?;
            limit(self.memory.store(object, length))?;
            object
        };
        let data = base.wrapping_add(if hash { U256::ZERO } else { U256::from(32) });
        self.burn(writes.len() as u64)?;
        for (offset, word) in writes {
            limit(self.memory.store(data.wrapping_add(U256::from(offset)), word))?;
        }
        if !hash && !aligned {
            limit(self.memory.store(data.wrapping_add(length), U256::ZERO))?;
        }
        if hash {
            self.burn_words(length)?;
            let bytes = limit(self.memory.read(data, length))?;
            ControlFlow::Continue(U256::from_be_bytes(keccak256(bytes).0))
        } else {
            ControlFlow::Continue(base)
        }
    }
}

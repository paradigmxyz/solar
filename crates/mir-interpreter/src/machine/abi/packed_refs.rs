//! Packed byte references. Resolve lengths before reserving output, then read
//! each payload when its copy occurs, retaining aliases with prior output writes.
//! Memory copies have snapshot semantics; storage copies retain whole dirty
//! words and header checks. A restored header borrow has no final memory write.

use super::{
    packed::{Piece, word_stores},
    packed_arrays::PackedArray,
};
use crate::machine::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::{U256, keccak256};
use solar_mir::{AllocationSemantics, Function, PackedPart, PtrLocation, ValueLayout};
use std::ops::ControlFlow;

enum Prepared {
    Static(Piece),
    Bytes { location: PtrLocation, source: U256, length: U256, object: bool },
    Storage { slot: U256, header: U256, long: bool, length: U256 },
    Array(PackedArray),
}

impl Prepared {
    fn length(&self) -> U256 {
        match self {
            Self::Static(piece) => U256::from(piece.bytes.len()),
            Self::Bytes { length, .. } | Self::Storage { length, .. } => *length,
            Self::Array(array) => array.bytes,
        }
    }
}

impl Run<'_, '_, '_> {
    pub(in crate::machine) fn semantic_packed_references(
        &mut self,
        body: &Function,
        parts: &[PackedPart],
        hash: bool,
    ) -> ControlFlow<Outcome, U256> {
        if parts.len() > 2048 {
            return ControlFlow::Break(Outcome::Unsupported("packed part limit of 2048"));
        }
        let mut prepared = Vec::new();
        let mut total = U256::ZERO;
        for part in parts {
            let piece = match part {
                PackedPart::Literal(bytes) => {
                    Prepared::Static(Piece { bytes: bytes.to_vec(), scalar: false })
                }
                PackedPart::Scalar { value, ty } => {
                    let Some(size) = ty
                        .type_size()
                        .map(|size| usize::from(size.bytes()))
                        .filter(|&size| (1..=32).contains(&size))
                    else {
                        return ControlFlow::Break(Outcome::Unsupported("packed scalar layout"));
                    };
                    let word = self.frame().read(*value)?.to_be_bytes::<32>();
                    let bytes = if matches!(ty, ValueLayout::FixedBytes(_)) {
                        &word[..size]
                    } else {
                        &word[32 - size..]
                    };
                    Prepared::Static(Piece { bytes: bytes.to_vec(), scalar: true })
                }
                PackedPart::Bytes { value, location } => {
                    let location = body.value_slice_location(*value).unwrap_or(*location);
                    let input = self.frame().read_value(*value)?;
                    if location == PtrLocation::Storage {
                        let RuntimeValue::Word(slot) = input else {
                            return ControlFlow::Break(Outcome::Unsupported("packed storage slot"));
                        };
                        let header = self.storage_read(slot)?;
                        let (long, length) = Self::bytes_header(header)?;
                        Prepared::Storage { slot, header, long, length }
                    } else {
                        let (source, length, object) = match input {
                            RuntimeValue::Word(pointer) if location == PtrLocation::Memory => {
                                let length = limit(self.memory.load(pointer))?;
                                (pointer.wrapping_add(U256::from(32)), length, true)
                            }
                            RuntimeValue::Aggregate(fields) => {
                                let [RuntimeValue::Word(source), RuntimeValue::Word(length)] =
                                    fields.as_slice()
                                else {
                                    return ControlFlow::Break(Outcome::Unsupported(
                                        "packed byte slice",
                                    ));
                                };
                                (*source, *length, false)
                            }
                            _ => {
                                return ControlFlow::Break(Outcome::Unsupported(
                                    "packed byte representation",
                                ));
                            }
                        };
                        Prepared::Bytes { location, source, length, object }
                    }
                }
                PackedPart::Array { value, element, source, wordwise } => {
                    let input = self.frame().read_value(*value)?;
                    Prepared::Array(self.prepare_packed_array(input, element, *source, *wordwise)?)
                }
            };
            if piece.length() > U256::from(65536) || total + piece.length() > U256::from(65536) {
                return ControlFlow::Break(Outcome::Unsupported("packed bytes exceed 64 KiB"));
            }
            total += piece.length();
            prepared.push(piece);
        }
        if hash
            && let Some(Prepared::Bytes {
                location: PtrLocation::Memory,
                source,
                length,
                object: true,
            }) = prepared.last()
        {
            let mut prefix = Vec::new();
            let mut static_prefix = true;
            for piece in &prepared[..prepared.len() - 1] {
                if let Prepared::Static(piece) = piece {
                    prefix.extend(&piece.bytes);
                } else {
                    static_prefix = false;
                    break;
                }
            }
            if static_prefix && prefix.len() <= 32 {
                self.burn_words(*length)?;
                prefix.extend(limit(self.memory.read(*source, *length))?);
                return ControlFlow::Continue(U256::from_be_bytes(keccak256(prefix).0));
            }
        }
        let aligned = prepared.iter().all(|piece| match piece {
            Prepared::Static(piece) => piece.bytes.len().is_multiple_of(32),
            Prepared::Array(_) => true,
            _ => false,
        });
        let base = if hash {
            limit(self.memory.load(U256::from(64)))?
        } else {
            let base = self.semantic_allocate(
                (total + U256::from(63)) & !U256::from(31),
                AllocationSemantics::INTERNAL,
            )?;
            limit(self.memory.store(base, total))?;
            base
        };
        let start = base.wrapping_add(if hash { U256::ZERO } else { U256::from(32) });
        let mut cursor = start;
        let mut pending = Vec::new();
        let mut work = 2048;
        for piece in prepared {
            match piece {
                Prepared::Static(piece) => pending.push(piece),
                piece => {
                    let length = piece.length();
                    cursor = self.packed_static_segment(cursor, &pending)?;
                    pending.clear();
                    match piece {
                        Prepared::Bytes { location, source, length, .. } => {
                            self.abi_copy_source(location, cursor, source, length)?;
                        }
                        Prepared::Storage { slot, header, long, length } => {
                            if long {
                                let parent = self.fixed_slot(&[slot])?;
                                for index in 0..length.to::<usize>().div_ceil(32) {
                                    let word =
                                        self.storage_read(parent.wrapping_add(U256::from(index)))?;
                                    limit(
                                        self.memory.store(
                                            cursor.wrapping_add(U256::from(index * 32)),
                                            word,
                                        ),
                                    )?;
                                }
                            } else {
                                limit(self.memory.store(cursor, header & !U256::from(255)))?;
                            }
                        }
                        Prepared::Array(array) => {
                            self.write_packed_array(&array, cursor, 33, &mut work, true)?;
                        }
                        Prepared::Static(_) => unreachable!(),
                    }
                    cursor = cursor.wrapping_add(length);
                }
            }
        }
        let end = self.packed_static_segment(cursor, &pending)?;
        if !hash && !aligned {
            limit(self.memory.store(end, U256::ZERO))?;
        }
        if hash {
            self.burn_words(total)?;
            let bytes = limit(self.memory.read(start, total))?;
            ControlFlow::Continue(U256::from_be_bytes(keccak256(bytes).0))
        } else {
            ControlFlow::Continue(base)
        }
    }

    fn packed_static_segment(
        &mut self,
        base: U256,
        pieces: &[Piece],
    ) -> ControlFlow<Outcome, U256> {
        let writes = word_stores(pieces);
        self.burn(writes.len() as u64)?;
        for (offset, word) in writes {
            limit(self.memory.store(base.wrapping_add(U256::from(offset)), word))?;
        }
        ControlFlow::Continue(
            base.wrapping_add(U256::from(
                pieces.iter().map(|piece| piece.bytes.len()).sum::<usize>(),
            )),
        )
    }
}

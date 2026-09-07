//! Expand packed ABI layouts after semantic optimization and before ABI and memory lowering.
//!
//! Scalars write their packed width; array leaves occupy full, cleaned ABI words. The encoder
//! computes dynamic lengths only after argument evaluation, then emits copies or nested array
//! loops. Hash-only encodings use the existing scratch policy when the shape permits it. Literal
//! bytes and adjacent narrow scalars can share a word store without merging signed high bits.

use crate::{
    memory::EvmMemoryLayout,
    mir::{
        AbiType, AbiWordValidator, AllocationSemantics, FunctionBuilder, MemoryObjectKind,
        MemoryObjectLayout, MirType, PackedArraySource, PackedPart, PanicCode, SliceLocation,
        ValueId, packed_element_bytes,
    },
};
use alloy_primitives::{Bytes, U256};

enum PackedPiece {
    Bytes(Bytes),
    Static { value: ValueId, length: u64, fixed_bytes: bool, signed: bool },
    Dynamic { source: ValueId, length: ValueId },
    Array { value: ValueId, length: ValueId, element: AbiType, source: PackedArraySource },
}

pub(super) fn lower_packed(
    builder: &mut FunctionBuilder<'_>,
    parts: Box<[PackedPart]>,
    hash: bool,
) -> ValueId {
    let mut encoder = PackedEncoder { builder };
    let (pieces, total) = encoder.prepare(parts, !hash);
    if hash {
        encoder.hash(pieces, total).expect("validated scratch packed shape")
    } else {
        encoder.encode(pieces, total)
    }
}

struct PackedEncoder<'a, 'b> {
    builder: &'a mut FunctionBuilder<'b>,
}

impl PackedEncoder<'_, '_> {
    fn prepare(&mut self, parts: Box<[PackedPart]>, checked: bool) -> (Vec<PackedPiece>, ValueId) {
        // pieces = resolve_source_lengths(parts)
        // total = sum(piece lengths)
        let mut total = self.builder.imm(0);
        let mut pieces = Vec::with_capacity(parts.len());
        for part in parts {
            let (piece, length) = match part {
                PackedPart::Literal(bytes) => {
                    let length = self.builder.imm(bytes.len() as u64);
                    (PackedPiece::Bytes(bytes), length)
                }
                PackedPart::Scalar { value, ty } => {
                    let length = u64::from(ty.type_size().expect("packed scalar").bytes());
                    (
                        PackedPiece::Static {
                            value,
                            length,
                            fixed_bytes: matches!(ty, MirType::FixedBytes(_)),
                            signed: matches!(ty, MirType::Int(_)),
                        },
                        self.builder.imm(length),
                    )
                }
                PackedPart::Bytes(value) => {
                    let is_slice = self.builder.func().value_slice_location(value).is_some();
                    let length = if is_slice {
                        self.builder.slice_len(value)
                    } else {
                        self.builder.memory_object_len(value, MemoryObjectKind::Bytes)
                    };
                    let source = if is_slice {
                        value
                    } else {
                        let pointer =
                            self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                        self.builder.make_slice(pointer, length, SliceLocation::Memory)
                    };
                    (PackedPiece::Dynamic { source, length }, length)
                }
                PackedPart::Array { value, element, source } => {
                    let length = match source {
                        PackedArraySource::Memory {
                            layout: MemoryObjectLayout::DynamicArray { .. },
                        } => self.builder.memory_object_len(value, MemoryObjectKind::DynamicArray),
                        PackedArraySource::Memory {
                            layout: MemoryObjectLayout::FixedArray { len, .. },
                        } => self.builder.imm(len),
                        PackedArraySource::Memory { .. } => unreachable!("packed array layout"),
                        PackedArraySource::Slice(_) => self.builder.slice_len(value),
                    };
                    let width = self
                        .builder
                        .imm(packed_element_bytes(&element).expect("packed array element"));
                    let bytes = self.builder.checked_mul(length, width);
                    (PackedPiece::Array { value, length, element, source }, bytes)
                }
            };
            total = self.add_packed_total(total, length, checked);
            pieces.push(piece);
        }
        (pieces, total)
    }

    fn encode(&mut self, pieces: Vec<PackedPiece>, total: ValueId) -> ValueId {
        // output = bytes(total)
        let output = self.builder.alloc_bytes_object(total, AllocationSemantics::INTERNAL);

        let mut offset = self.builder.imm(0);
        let mut index = 0;
        // for piece { write_packed(output, piece) }
        while index < pieces.len() {
            if let Some((consumed, length)) =
                self.try_write_packed_word(output, offset, &pieces[index..])
            {
                let length = self.builder.imm(length);
                offset = self.builder.checked_add(offset, length);
                index += consumed;
                continue;
            }

            match &pieces[index] {
                PackedPiece::Bytes(bytes) => {
                    // for chunk { mstore(output + offset, chunk) }
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm(U256::from_be_bytes(padded));
                        self.builder.memory_object_store_word(output, offset, value);
                        let length = self.builder.imm(chunk.len() as u64);
                        offset = self.builder.checked_add(offset, length);
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    // copy(source, output + offset)
                    self.builder.memory_object_copy_from_slice_at(
                        output,
                        MemoryObjectKind::Bytes,
                        offset,
                        *source,
                    );
                    offset = self.builder.checked_add(offset, *length);
                }
                PackedPiece::Array { value, length, element, source } => {
                    // offset = copy_packed_array(output, offset, value)
                    offset =
                        self.copy_packed_array(output, offset, *value, *length, element, *source);
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    // mstore(output + offset, align(value, length))
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    self.builder.memory_object_store_word(output, offset, value);
                    let length = self.builder.imm(*length);
                    offset = self.builder.checked_add(offset, length);
                }
            }
            index += 1;
        }
        output
    }

    fn hash(&mut self, pieces: Vec<PackedPiece>, total: ValueId) -> Option<ValueId> {
        let has_dynamic = pieces.iter().any(|piece| matches!(piece, PackedPiece::Dynamic { .. }));
        // base = has_dynamic ? fmp : (scratch_needed ? fmp : 0)
        let base = if has_dynamic {
            Some(self.builder.fmp())
        } else {
            let _ = u64::try_from(self.builder.func().value_u256(total)?).ok()?;
            let mut max_write_end = 0u64;
            let mut offset = 0u64;
            for piece in &pieces {
                match piece {
                    PackedPiece::Bytes(bytes) => {
                        for chunk in bytes.chunks(32) {
                            max_write_end = max_write_end.max(offset.checked_add(32)?);
                            offset = offset.checked_add(u64::try_from(chunk.len()).ok()?)?;
                        }
                    }
                    PackedPiece::Static { length, .. } => {
                        max_write_end = max_write_end.max(offset.checked_add(32)?);
                        offset = offset.checked_add(*length)?;
                    }
                    PackedPiece::Dynamic { .. } | PackedPiece::Array { .. } => return None,
                }
            }
            (max_write_end > EvmMemoryLayout::FMP_SLOT).then(|| self.builder.fmp())
        };

        let zero = base.unwrap_or_else(|| self.builder.imm(0));
        let mut offset = 0u64;
        let mut cursor = has_dynamic.then(|| base.expect("dynamic packed input has a base"));
        let mut index = 0;
        // write_packed(base, pieces)
        while index < pieces.len() {
            if let Some((consumed, length, value)) = self.try_pack_packed_word(&pieces[index..]) {
                let dest = self.packed_scratch_offset(cursor.or(base), offset);
                self.builder.mstore(dest, value);
                offset = offset.checked_add(length)?;
                index += consumed;
                continue;
            }

            let piece = &pieces[index];
            match piece {
                PackedPiece::Bytes(bytes) => {
                    // for chunk { mstore(base + offset, chunk) }
                    for chunk in bytes.chunks(32) {
                        let mut padded = [0u8; 32];
                        padded[..chunk.len()].copy_from_slice(chunk);
                        let value = self.builder.imm(U256::from_be_bytes(padded));
                        let dest = self.packed_scratch_offset(cursor.or(base), offset);
                        self.builder.mstore(dest, value);
                        offset = offset.checked_add(u64::try_from(chunk.len()).ok()?)?;
                    }
                }
                PackedPiece::Dynamic { source, length } => {
                    // copy(source, cursor + offset)
                    let dest = self.packed_scratch_offset(cursor, offset);
                    let location = self.builder.func().value_slice_location(*source)?;
                    let source_length = self.builder.slice_len(*source);
                    let pointer = self.builder.slice_ptr(*source);
                    self.builder.copy_slice_data(location, dest, pointer, source_length);
                    cursor = Some(self.builder.add(dest, *length));
                    offset = 0;
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    // mstore(base + offset, align(value, length))
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm((32 - *length) * 8);
                        self.builder.shl(shift, *value)
                    };
                    let dest = self.packed_scratch_offset(cursor.or(base), offset);
                    self.builder.mstore(dest, value);
                    offset = offset.checked_add(*length)?;
                }
                PackedPiece::Array { .. } => return None,
            }
            index += 1;
        }

        // size = has_dynamic ? end(base) - base : total
        // hash = keccak256(base, size)
        let size = if has_dynamic {
            let cursor = cursor.expect("dynamic packed input has a cursor");
            let end = self.builder.add_u64_offset(cursor, offset);
            self.builder.sub(end, zero)
        } else {
            total
        };
        Some(self.builder.keccak256(zero, size))
    }

    fn add_packed_total(&mut self, lhs: ValueId, rhs: ValueId, checked: bool) -> ValueId {
        if let (Some(lhs), Some(rhs)) =
            (self.builder.func().value_u256(lhs), self.builder.func().value_u256(rhs))
            && let Some(result) = lhs.checked_add(rhs)
        {
            return self.builder.imm(result);
        }
        if checked { self.builder.checked_add(lhs, rhs) } else { self.builder.add(lhs, rhs) }
    }

    fn packed_scratch_offset(&mut self, base: Option<ValueId>, offset: u64) -> ValueId {
        match base {
            Some(base) => self.builder.add_u64_offset(base, offset),
            None => self.builder.imm(offset),
        }
    }

    fn copy_packed_array(
        &mut self,
        output: ValueId,
        offset: ValueId,
        value: ValueId,
        length: ValueId,
        element: &AbiType,
        source: PackedArraySource,
    ) -> ValueId {
        // byte_length = length * padded_width(element)
        // end_offset = offset + byte_length
        let element_bytes = packed_element_bytes(element).expect("packed array shape");
        let element_bytes_value = self.builder.imm(element_bytes);
        let byte_length = self.builder.checked_mul(length, element_bytes_value);
        let end_offset = self.builder.checked_add(offset, byte_length);
        let base = match source {
            PackedArraySource::Memory { .. } => None,
            PackedArraySource::Slice(_) => Some(self.builder.slice_ptr(value)),
        };
        let memory_source = match source {
            PackedArraySource::Slice(SliceLocation::Memory) => Some(self.builder.make_slice(
                base.expect("slice base"),
                byte_length,
                SliceLocation::Memory,
            )),
            _ => None,
        };

        // for i in 0..length {
        //     destination = output + offset + i * element_width
        // }
        let preheader = self.builder.current_block();
        let header = self.builder.create_block();
        let body = self.builder.create_block();
        let exit = self.builder.create_block();
        self.builder.jump(header);

        self.builder.switch_to_block(header);
        let zero = self.builder.imm(0);
        let index = self.builder.phi(vec![(preheader, zero)]);
        let more = self.builder.lt(index, length);
        self.builder.branch(more, body, exit);

        self.builder.switch_to_block(body);
        let element_offset = self.builder.checked_mul(index, element_bytes_value);
        let destination = self.builder.checked_add(offset, element_offset);
        match element {
            AbiType::Word(_) | AbiType::Function => {
                // element = normalize(load_element(value, i))
                // mstore(destination, element)
                let element_value = match source {
                    PackedArraySource::Memory { layout } => {
                        self.builder.memory_object_load_element(value, layout, index)
                    }
                    PackedArraySource::Slice(location) => match location {
                        SliceLocation::Memory => self.builder.memory_slice_load_word(
                            memory_source.expect("memory slice"),
                            element_offset,
                        ),
                        SliceLocation::Calldata => {
                            self.builder.calldata_slice_load_word(value, element_offset)
                        }
                        SliceLocation::Returndata => unreachable!("returndata packed array"),
                    },
                };
                let element_value = match element {
                    AbiType::Word(Some(validator)) => {
                        if let AbiWordValidator::EnumRange(variants) = validator {
                            // valid = element < variant_count
                            // panic_if_zero(valid, enum_conversion)
                            let variants = self.builder.imm(*variants);
                            let valid = self.builder.lt(element_value, variants);
                            self.builder.panic_if_zero(valid, PanicCode::EnumConversion);
                        }
                        validator.cleanup(self.builder, element_value)
                    }
                    _ => element_value,
                };
                let element_value = if matches!(element, AbiType::Function)
                    && matches!(source, PackedArraySource::Memory { .. })
                {
                    AbiWordValidator::from_mir_type(MirType::Function)
                        .expect("function words always require cleanup")
                        .cleanup(self.builder, element_value)
                } else {
                    element_value
                };
                self.builder.memory_object_store_word(output, destination, element_value);
            }
            AbiType::FixedArray { element: nested, len } => {
                // copy_packed_array(output, destination, value[i])
                let nested_length = self.builder.imm(*len);
                let (nested_value, nested_source) = match source {
                    PackedArraySource::Memory { layout } => {
                        let nested_value =
                            self.builder.memory_object_load_element(value, layout, index);
                        let nested_layout = MemoryObjectLayout::word_fixed_array(*len);
                        (nested_value, PackedArraySource::Memory { layout: nested_layout })
                    }
                    PackedArraySource::Slice(location) => {
                        let base = self.builder.slice_ptr(value);
                        let pointer = self.builder.add(base, element_offset);
                        let nested_value =
                            self.builder.make_slice(pointer, nested_length, location);
                        (nested_value, PackedArraySource::Slice(location))
                    }
                };
                self.copy_packed_array(
                    output,
                    destination,
                    nested_value,
                    nested_length,
                    nested,
                    nested_source,
                );
            }
            AbiType::DynamicArray { .. } | AbiType::Bytes(_) | AbiType::Tuple(_) => {
                unreachable!("packed array shape")
            }
        }
        let next = self.builder.add_u64_offset(index, 1);
        let backedge = self.builder.current_block();
        self.builder.jump(header);
        self.builder.add_phi_incoming(index, backedge, next);

        self.builder.switch_to_block(exit);
        end_offset
    }

    fn try_pack_packed_word(&mut self, pieces: &[PackedPiece]) -> Option<(usize, u64, ValueId)> {
        let mut constant = U256::ZERO;
        let mut terms = Vec::new();
        let mut length = 0u64;
        let mut consumed = 0;

        for piece in pieces {
            match piece {
                PackedPiece::Bytes(bytes) => {
                    let piece_length = u64::try_from(bytes.len()).ok()?;
                    if piece_length == 0 {
                        consumed += 1;
                        continue;
                    }
                    if length.checked_add(piece_length)? > 32 {
                        break;
                    }
                    let shift = (32 - length - piece_length) * 8;
                    constant |= U256::from_be_slice(bytes) << usize::try_from(shift).unwrap();
                    length += piece_length;
                    consumed += 1;
                }
                PackedPiece::Static { value, length: piece_length, fixed_bytes: false, signed }
                    if *piece_length < 32 =>
                {
                    if *piece_length == 0 {
                        consumed += 1;
                        continue;
                    }
                    if length.checked_add(*piece_length)? > 32 {
                        break;
                    }
                    let shift = (32 - length - *piece_length) * 8;
                    terms.push((*value, shift, *piece_length, *signed));
                    length += *piece_length;
                    consumed += 1;
                }
                _ => break,
            }
        }

        if consumed < 2 || length == 0 || terms.is_empty() {
            return None;
        }

        let mut value = self.builder.imm(constant);
        for (term, shift, size, signed) in terms {
            // word |= (term & width_mask_if_signed) << shift
            let term = if signed {
                let mask = self.builder.imm((U256::from(1) << (size * 8)) - U256::from(1));
                self.builder.and(term, mask)
            } else {
                term
            };
            let term = if shift == 0 {
                term
            } else {
                let shift = self.builder.imm(shift);
                self.builder.shl(shift, term)
            };
            value = self.builder.or(value, term);
        }
        Some((consumed, length, value))
    }

    fn try_write_packed_word(
        &mut self,
        output: ValueId,
        offset: ValueId,
        pieces: &[PackedPiece],
    ) -> Option<(usize, u64)> {
        let (consumed, length, value) = self.try_pack_packed_word(pieces)?;
        self.builder.memory_object_store_word(output, offset, value);
        Some((consumed, length))
    }
}

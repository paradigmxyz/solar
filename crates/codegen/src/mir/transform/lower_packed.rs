//! Expand packed ABI layouts after semantic optimization and before ABI and memory lowering.
//!
//! Scalars write their packed width; array leaves occupy full, cleaned ABI words. The encoder
//! computes dynamic lengths only after argument evaluation, then emits copies or nested array
//! loops. Hash-only encodings use the existing scratch policy when the shape permits it. Literal
//! bytes and adjacent narrow scalars, fixed bytes included, can share a word store without merging
//! signed high bits or the low bytes of a fixed-bytes word.
//! Encodings made entirely of whole words need no allocation-size rounding. As in solc, part
//! lengths and offsets add without overflow checks; the size rounding still checks. Array loops use
//! plain element-offset arithmetic: their index is below the length whose total extent was checked.
//! Bytes results retain their FMP bump at the encoding point even when their size becomes constant.
//! Known local byte lengths stay constant through expansion without duplicating their payloads.
//! Recovery requires a same-block allocation and header store, and rejects unknown memory writes.
//! When all parts have bounded lengths and byte sources retain heap allocations, their copies
//! into the fresh output are disjoint. Unknown lengths and frame-eligible sources keep alias
//! checks.

use crate::mir::{
    AbiType, AbiWordValidator, AllocationSemantics, BlockId, EffectKind, Function, FunctionBuilder,
    InstKind, MemoryObjectLayout, PackedArraySource, PackedPart, PanicCode, PtrLocation, Value,
    ValueId, ValueLayout, memory::EvmMemoryLayout, packed_element_bytes,
};
use alloy_primitives::{Bytes, U256};

enum PackedPiece {
    Bytes(Bytes),
    Static { value: ValueId, length: u64, fixed_bytes: bool, signed: bool },
    Dynamic { source: ValueId, length: ValueId, bounded_heap: bool },
    Array { value: ValueId, length: ValueId, element: AbiType, source: PackedArraySource },
}

pub(super) fn lower_packed(
    builder: &mut FunctionBuilder<'_>,
    parts: Box<[PackedPart]>,
    hash: bool,
) -> ValueId {
    let mut encoder = PackedEncoder { builder };
    let (pieces, total) = encoder.prepare(parts);
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
    fn prepare(&mut self, parts: Box<[PackedPart]>) -> (Vec<PackedPiece>, ValueId) {
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
                            fixed_bytes: matches!(ty, ValueLayout::FixedBytes(_)),
                            signed: matches!(ty, ValueLayout::Int(_)),
                        },
                        self.builder.imm(length),
                    )
                }
                PackedPart::Bytes(value) => {
                    let known_length =
                        known_byte_length(self.builder.func(), value, self.builder.current_block());
                    let bounded_heap = known_length.is_some()
                        && matches!(self.builder.func().value(value), Value::Inst(inst)
                            if self.builder.func().inst(*inst).metadata.preserves_fmp());
                    let source = if let Some(length) = known_length {
                        // source = make_memory_slice (memory_data value), length
                        let length = self.builder.imm(length);
                        let data = self.builder.memory_data(value);
                        self.builder.make_slice(data, length, PtrLocation::Memory)
                    } else {
                        self.builder.memory_view(value)
                    };
                    let length = self.builder.slice_len(source);
                    (PackedPiece::Dynamic { source, length, bounded_heap }, length)
                }
                PackedPart::Array { value, element, source } => {
                    let length = match source {
                        PackedArraySource::Memory {
                            layout: MemoryObjectLayout::DynamicArray { .. },
                        } => self.builder.memory_len(value),
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
            total = self.add_folded(total, length);
            pieces.push(piece);
        }
        (pieces, total)
    }

    fn encode(&mut self, pieces: Vec<PackedPiece>, total: ValueId) -> ValueId {
        let bounded = pieces.iter().all(|piece| match piece {
            PackedPiece::Bytes(_) | PackedPiece::Static { .. } => true,
            PackedPiece::Dynamic { bounded_heap, .. } => *bounded_heap,
            PackedPiece::Array { .. } => false,
        });
        let word_aligned = pieces.iter().all(|piece| match piece {
            PackedPiece::Bytes(bytes) => bytes.len().is_multiple_of(32),
            PackedPiece::Static { length, .. } => length.is_multiple_of(32),
            PackedPiece::Array { .. } => true,
            PackedPiece::Dynamic { .. } => false,
        });
        let output = if word_aligned {
            // size = checked_add(total, 32)
            // output = alloc size
            // mstore (ptrtoint output), total
            let header = self.builder.imm(32);
            let size = self.builder.checked_add(total, header);
            let output = self.builder.alloc(size, AllocationSemantics::INTERNAL);
            self.builder.set_memory_len(output, total);
            output
        } else {
            // output = bytes(total)
            self.builder.alloc_bytes_object(total, AllocationSemantics::INTERNAL)
        };
        // output = alloc bytes(...) !preserves_fmp
        if let Value::Inst(alloc) = *self.builder.func().value(output) {
            self.builder.func_mut().inst_mut(alloc).metadata.set_preserves_fmp(true);
        }

        let mut offset = self.builder.imm(0);
        let mut index = 0;
        // for piece { write_packed(output, piece) }
        while index < pieces.len() {
            if let Some((consumed, length)) =
                self.try_write_packed_word(output, offset, &pieces[index..])
            {
                let length = self.builder.imm(length);
                offset = self.add_folded(offset, length);
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
                        self.builder.memory_store_word(output, offset, value);
                        let length = self.builder.imm(chunk.len() as u64);
                        offset = self.add_folded(offset, length);
                    }
                }
                PackedPiece::Dynamic { source, length, .. } => {
                    if bounded {
                        // data = memory_data output
                        // dest = ptradd data, offset
                        // source_ptr = extract_value source, 0
                        // mcopy dest, source_ptr, length !disjoint
                        let data = self.builder.memory_data(output);
                        let dest = self.builder.ptradd(data, offset);
                        let source_ptr = self.builder.slice_ptr(*source);
                        let copy =
                            self.builder.copy_heap(dest, source_ptr, *length, PtrLocation::Memory);
                        self.builder.func_mut().inst_mut(copy).metadata.set_disjoint(true);
                    } else {
                        // copy(source, output + offset)
                        self.builder.memory_copy_from_slice_at(output, offset, *source);
                    }
                    offset = self.add_folded(offset, *length);
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
                        self.builder.shl(*value, shift)
                    };
                    self.builder.memory_store_word(output, offset, value);
                    let length = self.builder.imm(*length);
                    offset = self.add_folded(offset, length);
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
                self.builder.store(dest, value);
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
                        self.builder.store(dest, value);
                        offset = offset.checked_add(u64::try_from(chunk.len()).ok()?)?;
                    }
                }
                PackedPiece::Dynamic { source, length, .. } => {
                    // copy(source, cursor + offset)
                    let dest = self.packed_scratch_offset(cursor, offset);
                    let location = self.builder.func().value_slice_location(*source)?;
                    let source_length = self.builder.slice_len(*source);
                    let pointer = self.builder.slice_ptr(*source);
                    self.builder.copy_heap(dest, pointer, source_length, location);
                    cursor = Some(self.builder.add(dest, *length));
                    offset = 0;
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    // mstore(base + offset, align(value, length))
                    let value = if *fixed_bytes || *length == 32 {
                        *value
                    } else {
                        let shift = self.builder.imm((32 - *length) * 8);
                        self.builder.shl(*value, shift)
                    };
                    let dest = self.packed_scratch_offset(cursor.or(base), offset);
                    self.builder.store(dest, value);
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

    fn add_folded(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        if let (Some(lhs), Some(rhs)) =
            (self.builder.func().value_u256(lhs), self.builder.func().value_u256(rhs))
            && let Some(result) = lhs.checked_add(rhs)
        {
            return self.builder.imm(result);
        }
        self.builder.add(lhs, rhs)
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
            PackedArraySource::Slice(PtrLocation::Memory) => Some(self.builder.make_slice(
                base.expect("slice base"),
                byte_length,
                PtrLocation::Memory,
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
        // index < length and the checked end_offset bound both operations.
        // element_offset = index * element_width
        // destination = offset + element_offset
        let element_offset = self.builder.mul(index, element_bytes_value);
        let destination = self.builder.add(offset, element_offset);
        match element {
            AbiType::Word(_) | AbiType::Function => {
                // element = normalize(load_element(value, i))
                // mstore(destination, element)
                let element_value = match source {
                    PackedArraySource::Memory { layout } => {
                        self.builder.memory_load_element(value, layout, index)
                    }
                    PackedArraySource::Slice(location) => {
                        debug_assert_ne!(location, PtrLocation::Returndata);
                        self.builder.slice_load_word(memory_source.unwrap_or(value), element_offset)
                    }
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
                    AbiWordValidator::from_layout(ValueLayout::Function)
                        .expect("function words always require cleanup")
                        .cleanup(self.builder, element_value)
                } else {
                    element_value
                };
                self.builder.memory_store_word(output, destination, element_value);
            }
            AbiType::FixedArray { element: nested, len } => {
                // copy_packed_array(output, destination, value[i])
                let nested_length = self.builder.imm(*len);
                let (nested_value, nested_source) = match source {
                    PackedArraySource::Memory { layout } => {
                        let nested_value = self.builder.memory_load_element(value, layout, index);
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
                PackedPiece::Static { value, length: piece_length, fixed_bytes, signed }
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
                    terms.push((*value, shift, *piece_length, *signed, *fixed_bytes));
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
        for (term, shift, size, signed, fixed_bytes) in terms {
            if fixed_bytes {
                // word |= (term & high_width_mask) >> (256 - shift - width)
                let bits = u16::try_from(size * 8).unwrap();
                let term = AbiWordValidator::LeftAligned(bits).cleanup(self.builder, term);
                let right = 256 - shift - size * 8;
                let term = if right == 0 {
                    term
                } else {
                    let right = self.builder.imm(right);
                    self.builder.shr(term, right)
                };
                value = self.builder.or(value, term);
                continue;
            }
            // word |= (term & width_mask_if_signed) << shift
            let term = if signed {
                let bits = u16::try_from(size * 8).unwrap();
                AbiWordValidator::Unsigned(bits).cleanup(self.builder, term)
            } else {
                term
            };
            let term = if shift == 0 {
                term
            } else {
                let shift = self.builder.imm(shift);
                self.builder.shl(term, shift)
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
        self.builder.memory_store_word(output, offset, value);
        Some((consumed, length))
    }
}

fn known_byte_length(func: &Function, object: ValueId, block: BlockId) -> Option<u64> {
    let size = func.alloc_size(object)?;
    let Value::Inst(allocation) = func.value(object) else { return None };
    let instructions = &func.blocks[block].instructions;
    let start = instructions.iter().position(|inst| inst == allocation)?;
    let mut length = None;
    for &inst in &instructions[start + 1..] {
        let (address, bytes, value) = match func.inst(inst).kind {
            InstKind::Store(address, value, PtrLocation::Memory) => (address, 32, Some(value)),
            InstKind::DataCopy(_, address, size) => (address, func.value_u64(size)?, None),
            InstKind::Alloc { .. } => continue,
            _ => {
                if matches!(
                    func.inst(inst).kind.effect_kind(),
                    EffectKind::MemoryWrite
                        | EffectKind::ICall
                        | EffectKind::ExternalCall
                        | EffectKind::Create
                ) {
                    return None;
                }
                continue;
            }
        };
        let (base, offset) = func.object_address(address)?;
        if offset.checked_add(bytes)? > func.alloc_size(base)? {
            return None;
        }
        if base == object {
            if offset == 0 && bytes == 32 {
                length = value.and_then(|value| func.value_u64(value));
            } else if offset < 32 && bytes != 0 {
                return None;
            }
        } else {
            let Value::Inst(other) = func.value(base) else { return None };
            if !instructions[start + 1..].contains(other) {
                return None;
            }
        }
    }
    let length = length?;
    (length.checked_add(32)? <= size).then_some(length)
}

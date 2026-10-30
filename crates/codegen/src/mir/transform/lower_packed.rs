//! Packed ABI encoding for `lower-builtins`, which expands `abi_encode_packed` with it after
//! semantic optimization and before ABI and memory lowering. This module is not a pass.
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
//!
//! A hash of an encoding that ends with bytes in memory, after literals and scalars of at most
//! one word in all, reads the bytes where they are, as Solady's `toEthSignedMessageHash` does:
//! the prefix is written over the last bytes of the length word of the bytes, which it then
//! immediately precedes, and the length is written back once the hash has read the range. Only
//! the hash runs between the two writes, so nothing observes the borrowed length word, and the
//! two writes leave memory as it was, whatever the word held, even when the bytes are the zero
//! slot or lie inside another object.

use crate::mir::{
    AbiType, AbiWordValidator, AllocationSemantics, BlockId, EffectKind, Function, FunctionBuilder,
    InstKind, MemoryObjectLayout, MirType, PackedArraySource, PackedPart, PtrLocation, Value,
    ValueId, ValueLayout, memory::EvmMemoryLayout, packed_element_bytes,
};
use alloy_primitives::{Bytes, U256};

enum PackedPiece {
    Bytes(Bytes),
    Static { value: ValueId, length: u64, fixed_bytes: bool, signed: bool },
    Dynamic { source: ValueId, length: ValueId, bounded_heap: bool },
    Array { value: ValueId, length: ValueId, element: AbiType, source: PackedArraySource },
}

impl PackedPiece {
    /// The byte length of a literal or scalar piece.
    fn static_len(&self) -> Option<u64> {
        match self {
            Self::Bytes(bytes) => Some(bytes.len() as u64),
            Self::Static { length, .. } => Some(*length),
            Self::Dynamic { .. } | Self::Array { .. } => None,
        }
    }
}

pub(super) fn lower_packed(
    builder: &mut FunctionBuilder<'_>,
    parts: Box<[PackedPart]>,
    hash: bool,
) -> ValueId {
    let mut encoder = PackedEncoder { builder };
    if hash && let Some(hash) = encoder.hash_over_length(&parts) {
        return hash;
    }
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
                PackedPart::Literal(_) | PackedPart::Scalar { .. } => {
                    let piece = static_piece(&part).expect("packed literal or scalar");
                    let length =
                        self.builder.imm(piece.static_len().expect("packed literal or scalar"));
                    (piece, length)
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
            total = self.builder.add_folded(total, length);
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
            if let Some((consumed, length, value)) = self.try_pack_packed_word(&pieces[index..]) {
                self.builder.memory_store_word(output, offset, value);
                let length = self.builder.imm(length);
                offset = self.builder.add_folded(offset, length);
                index += consumed;
                continue;
            }

            match &pieces[index] {
                PackedPiece::Bytes(bytes) => {
                    // for chunk { mstore(output + offset, chunk) }
                    for chunk in bytes.chunks(32) {
                        let value = self.builder.imm(padded_word(chunk));
                        self.builder.memory_store_word(output, offset, value);
                        let length = self.builder.imm(chunk.len() as u64);
                        offset = self.builder.add_folded(offset, length);
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
                    offset = self.builder.add_folded(offset, *length);
                }
                PackedPiece::Array { value, length, element, source } => {
                    // offset = copy_packed_array(output, offset, value)
                    offset =
                        self.copy_packed_array(output, offset, *value, *length, element, *source);
                }
                PackedPiece::Static { value, length, fixed_bytes, .. } => {
                    // mstore(output + offset, align(value, length))
                    let value = self.align_static(*value, *length, *fixed_bytes);
                    self.builder.memory_store_word(output, offset, value);
                    let length = self.builder.imm(*length);
                    offset = self.builder.add_folded(offset, length);
                }
            }
            index += 1;
        }
        if !word_aligned {
            // Clear the word after the encoding, as solc's final copy does: the
            // bytes past its end in the last word read as zero padding, which
            // code that reads whole words of the result, such as assembly,
            // relies on. A bytes or calldata copy writes no further than its
            // length, so the copied parts would otherwise leave the
            // allocation's earlier contents there.
            // mstore(output + 32 + total, 0)
            let zero = self.builder.imm(0);
            self.builder.memory_store_word(output, offset, zero);
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
            (static_write_end(&pieces)? > EvmMemoryLayout::FMP_SLOT).then(|| self.builder.fmp())
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
                        let value = self.builder.imm(padded_word(chunk));
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
                    let value = self.align_static(*value, *length, *fixed_bytes);
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
            let end = self.builder.add_offset(cursor, offset);
            self.builder.sub(end, zero)
        } else {
            total
        };
        Some(self.builder.keccak256(zero, size))
    }

    /// Left-aligns a scalar of `length` bytes in its word; fixed bytes already are.
    fn align_static(&mut self, value: ValueId, length: u64, fixed_bytes: bool) -> ValueId {
        if fixed_bytes || length == 32 {
            value
        } else {
            let shift = self.builder.imm((32 - length) * 8);
            self.builder.shl_folded(value, shift)
        }
    }

    fn packed_scratch_offset(&mut self, base: Option<ValueId>, offset: u64) -> ValueId {
        match base {
            Some(base) => self.builder.add_offset(base, offset),
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
        let loop_ = self.builder.begin_counted_loop(length);
        let index = loop_.index();
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
                            // panic_if(valid == 0, enum_conversion)
                            self.builder.validate_enum_value(*variants, element_value);
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
            AbiType::FixedArray { element: nested, len, .. } => {
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
        self.builder.finish_counted_loop(loop_);
        end_offset
    }

    fn try_pack_packed_word(&mut self, pieces: &[PackedPiece]) -> Option<(usize, u64, ValueId)> {
        let (consumed, length) = packed_word_group(pieces)?;
        Some((consumed, length, self.pack_word(&pieces[..consumed], 0)))
    }

    /// One word holding the literals and narrow scalars `pieces`, packed from its byte `start`.
    fn pack_word(&mut self, pieces: &[PackedPiece], start: u64) -> ValueId {
        let mut constant = U256::ZERO;
        let mut terms = Vec::new();
        let mut offset = start;
        for piece in pieces {
            match piece {
                PackedPiece::Bytes(bytes) => {
                    let piece_length = bytes.len() as u64;
                    if piece_length == 0 {
                        continue;
                    }
                    let shift = (32 - offset - piece_length) * 8;
                    constant |= U256::from_be_slice(bytes) << usize::try_from(shift).unwrap();
                    offset += piece_length;
                }
                PackedPiece::Static { value, length: piece_length, fixed_bytes, signed } => {
                    if *piece_length == 0 {
                        continue;
                    }
                    let shift = (32 - offset - *piece_length) * 8;
                    terms.push((*value, shift, *piece_length, *signed, *fixed_bytes));
                    offset += *piece_length;
                }
                PackedPiece::Dynamic { .. } | PackedPiece::Array { .. } => {
                    unreachable!("packed word groups hold only literals and narrow scalars")
                }
            }
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
                    self.builder.shr_folded(term, right)
                };
                value = self.builder.or_folded(value, term);
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
                self.builder.shl_folded(term, shift)
            };
            value = self.builder.or_folded(value, term);
        }
        value
    }

    /// The hash of `parts` read in place, when they are literals and scalars of at most one word
    /// in all followed by bytes in memory: the prefix borrows the last bytes of the length word of
    /// the bytes, which is written back after the hash. Only a length that inline assembly forged
    /// past `2^255` can wrap the hashed size, and the encoder's copy of that many bytes runs out of
    /// gas: so does the hash, of every byte, for such a length.
    fn hash_over_length(&mut self, parts: &[PackedPart]) -> Option<ValueId> {
        let (&PackedPart::Bytes(object), prefix) = parts.split_last()? else { return None };
        if self.builder.func().value_ty(object) != Some(MirType::MEMPTR) {
            return None;
        }
        let pieces = static_pieces(prefix)?;
        let mut size = 0u64;
        for piece in &pieces {
            size += piece.static_len()?;
        }
        if size > 32 {
            return None;
        }
        if size == 0 {
            // hash = keccak256 (ptradd object, 32), (mload object)
            let length = self.builder.memory_len(object);
            let data = self.builder.memory_data(object);
            return Some(self.builder.keccak256(data, length));
        }
        // length = mload object
        // mstore object, prefix in the word's last `size` bytes
        // start = ptradd object, 32 - size
        // total = or (add size, length), (sar length, 255)
        // hash = keccak256 start, total
        // mstore object, length
        let length = self.builder.memory_len(object);
        let word = self.pack_word(&pieces, 32 - size);
        self.builder.store(object, word);
        let start = self.builder.address_value(object, 32 - size);
        let size = self.builder.imm(size);
        let total = self.builder.add(size, length);
        let top = self.builder.imm(255);
        let huge = self.builder.sar(length, top);
        let total = self.builder.or(total, huge);
        let hash = self.builder.keccak256(start, total);
        self.builder.store(object, length);
        Some(hash)
    }
}

/// The end of the scratch space a packed hash of `parts` writes, when the hash is built there:
/// every part is a literal or a scalar, and the word stores the writes make, merged words
/// included, end by the free memory pointer's slot.
pub(super) fn scratch_hash_end(parts: &[PackedPart]) -> Option<u64> {
    let pieces = static_pieces(parts)?;
    let end = static_write_end(&pieces)?;
    (end <= EvmMemoryLayout::FMP_SLOT).then_some(end)
}

/// How far the word stores of a packed encoding of literals and scalars reach from its start,
/// planned as the writes make them: one word per literal chunk and scalar, or one per group of
/// pieces that share a word.
fn static_write_end(pieces: &[PackedPiece]) -> Option<u64> {
    let mut max_write_end = 0u64;
    let mut offset = 0u64;
    let mut index = 0;
    while index < pieces.len() {
        if let Some((consumed, length)) = packed_word_group(&pieces[index..]) {
            max_write_end = max_write_end.max(offset.checked_add(32)?);
            offset = offset.checked_add(length)?;
            index += consumed;
            continue;
        }
        match &pieces[index] {
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
        index += 1;
    }
    Some(max_write_end)
}

/// The leading pieces that share one word store: literal bytes and scalars narrower than a word,
/// at least two of them and one scalar, together at most 32 bytes. Returns how many pieces the
/// word takes and how many bytes they fill.
fn packed_word_group(pieces: &[PackedPiece]) -> Option<(usize, u64)> {
    let mut length = 0u64;
    let mut consumed = 0;
    let mut scalars = 0;
    for piece in pieces {
        match piece {
            PackedPiece::Bytes(bytes) => {
                let piece_length = u64::try_from(bytes.len()).ok()?;
                if piece_length != 0 && length.checked_add(piece_length)? > 32 {
                    break;
                }
                length += piece_length;
                consumed += 1;
            }
            PackedPiece::Static { length: piece_length, .. } if *piece_length < 32 => {
                if *piece_length != 0 {
                    if length.checked_add(*piece_length)? > 32 {
                        break;
                    }
                    scalars += 1;
                }
                length += *piece_length;
                consumed += 1;
            }
            _ => break,
        }
    }
    (consumed >= 2 && length != 0 && scalars != 0).then_some((consumed, length))
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

/// The pieces of `parts` when every part is a literal or a scalar.
fn static_pieces(parts: &[PackedPart]) -> Option<Vec<PackedPiece>> {
    parts.iter().map(static_piece).collect()
}

/// The piece of `part` when it is a literal or a scalar.
fn static_piece(part: &PackedPart) -> Option<PackedPiece> {
    match part {
        PackedPart::Literal(bytes) => Some(PackedPiece::Bytes(bytes.clone())),
        PackedPart::Scalar { value, ty } => Some(PackedPiece::Static {
            value: *value,
            length: u64::from(ty.type_size()?.bytes()),
            fixed_bytes: matches!(ty, ValueLayout::FixedBytes(_)),
            signed: matches!(ty, ValueLayout::Int(_)),
        }),
        PackedPart::Bytes(_) | PackedPart::Array { .. } => None,
    }
}

/// A chunk of at most 32 bytes as a left-aligned word.
fn padded_word(chunk: &[u8]) -> U256 {
    let mut padded = [0u8; 32];
    padded[..chunk.len()].copy_from_slice(chunk);
    U256::from_be_bytes(padded)
}

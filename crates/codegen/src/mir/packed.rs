//! Owned input shapes for packed ABI encoding, independent of HIR types, and the in-place
//! layout of an encoding that ends with bytes in memory.

use super::{
    AbiType, FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, SliceLocation, ValueId,
    ValueLayout,
};
use alloy_primitives::{Bytes, U256};

/// One packed argument; array elements retain their padded ABI word shape.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PackedPart {
    Literal(Bytes),
    Scalar { value: ValueId, ty: ValueLayout },
    Bytes(ValueId),
    Array { value: ValueId, element: AbiType, source: PackedArraySource },
}

impl PackedPart {
    pub(crate) fn value(&self) -> Option<ValueId> {
        match *self {
            Self::Literal(_) => None,
            Self::Scalar { value, .. } | Self::Bytes(value) | Self::Array { value, .. } => {
                Some(value)
            }
        }
    }

    pub(crate) fn value_mut(&mut self) -> Option<&mut ValueId> {
        match self {
            Self::Literal(_) => None,
            Self::Scalar { value, .. } | Self::Bytes(value) | Self::Array { value, .. } => {
                Some(value)
            }
        }
    }
}

/// Array source representation, including the memory stride of direct elements.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PackedArraySource {
    Memory { layout: MemoryObjectLayout },
    Slice(SliceLocation),
}

/// Returns the padded encoded width of a supported array element.
pub(crate) fn packed_element_bytes(element: &AbiType) -> Option<u64> {
    match element {
        AbiType::Word(_) | AbiType::Function => Some(32),
        AbiType::FixedArray { element, len } => packed_element_bytes(element)?.checked_mul(*len),
        AbiType::DynamicArray { .. } | AbiType::Bytes(_) | AbiType::Tuple(_) => None,
    }
}

/// A packed encoding laid out in place: its static prefix written over the length word of the
/// memory bytes it ends with, which the prefix then immediately precedes.
pub(crate) struct PrefixedBytes {
    /// The address of the encoding's first byte.
    pub(crate) start: ValueId,
    /// The encoding's size: the prefix and the bytes.
    pub(crate) size: ValueId,
    /// The address of the bytes' length word.
    header: ValueId,
    /// The bytes' length, which the length word held.
    length: ValueId,
    /// Whether the length word was written over.
    written: bool,
}

/// The size of a packed prefix of literals and scalars, when it fits in one word.
pub(crate) fn static_prefix_size(prefix: &[PackedPart]) -> Option<u64> {
    let mut size = 0u64;
    for part in prefix {
        size += match part {
            PackedPart::Literal(bytes) => u64::try_from(bytes.len()).ok()?,
            PackedPart::Scalar { ty, .. } => u64::from(ty.type_size()?.bytes()),
            PackedPart::Bytes(_) | PackedPart::Array { .. } => return None,
        };
    }
    (size <= 32).then_some(size)
}

/// Lays out the packed `prefix`, which [`static_prefix_size`] accepts, followed by the memory
/// bytes `object` without copying them: the prefix is written over the last bytes of the
/// object's length word. The caller reads the range, runs nothing else that touches the bytes or
/// their length, and then calls [`restore_length`].
pub(crate) fn prefix_over_length(
    builder: &mut FunctionBuilder<'_>,
    prefix: &[PackedPart],
    object: ValueId,
) -> PrefixedBytes {
    let prefix_size = static_prefix_size(prefix).expect("static packed prefix");
    // length = object.len
    // header = ptrtoint object
    let length = builder.memory_object_len(object, MemoryObjectKind::Bytes);
    let header = builder.cast_word(object);
    if prefix_size > 0 {
        // word = prefix, in the last prefix_size bytes
        // mstore(header, word)
        let mut constant = U256::ZERO;
        let mut word = None;
        let mut offset = 0u64;
        for part in prefix {
            match part {
                PackedPart::Literal(bytes) => {
                    let length = bytes.len() as u64;
                    if length > 0 {
                        let shift = usize::try_from((prefix_size - offset - length) * 8).unwrap();
                        constant |= U256::from_be_slice(bytes) << shift;
                    }
                    offset += length;
                }
                PackedPart::Scalar { value, ty } => {
                    let length = u64::from(ty.type_size().expect("packed scalar").bytes());
                    let value = builder.cast_word(*value);
                    // term = the scalar's packed bytes as an integer
                    let term = match ty {
                        ValueLayout::FixedBytes(_) if length < 32 => {
                            let shift = builder.imm((32 - length) * 8);
                            builder.shr(shift, value)
                        }
                        ValueLayout::Int(_) if length < 32 => {
                            let mask = builder.imm(
                                (U256::from(1) << usize::try_from(length * 8).unwrap())
                                    - U256::from(1),
                            );
                            builder.and(value, mask)
                        }
                        _ => value,
                    };
                    let shift = (prefix_size - offset - length) * 8;
                    let term = if shift == 0 {
                        term
                    } else {
                        let shift = builder.imm(shift);
                        builder.shl(shift, term)
                    };
                    word = Some(match word {
                        Some(word) => builder.or(word, term),
                        None => term,
                    });
                    offset += length;
                }
                PackedPart::Bytes(_) | PackedPart::Array { .. } => unreachable!("static prefix"),
            }
        }
        let word = match word {
            Some(word) if constant.is_zero() => word,
            Some(word) => {
                let constant = builder.imm(constant);
                builder.or(word, constant)
            }
            None => builder.imm(constant),
        };
        builder.mstore(header, word);
    }
    // start = header + 32 - prefix_size
    // size = prefix_size + length
    let start = builder.add_u64_offset(header, 32 - prefix_size);
    let prefix_size_value = builder.imm(prefix_size);
    let size = builder.add(prefix_size_value, length);
    PrefixedBytes { start, size, header, length, written: prefix_size > 0 }
}

/// Writes back the length word [`prefix_over_length`] wrote the prefix over.
pub(crate) fn restore_length(builder: &mut FunctionBuilder<'_>, prefixed: &PrefixedBytes) {
    if prefixed.written {
        // mstore(header, length)
        builder.mstore(prefixed.header, prefixed.length);
    }
}

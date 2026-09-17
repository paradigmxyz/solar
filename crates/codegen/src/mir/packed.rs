//! Owned input shapes for packed ABI encoding, independent of HIR types.

use super::{AbiType, MemoryObjectLayout, MirType, SliceLocation, ValueId};
use alloy_primitives::Bytes;

/// One packed argument; array elements retain their padded ABI word shape.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum PackedPart {
    Literal(Bytes),
    Scalar { value: ValueId, ty: MirType },
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

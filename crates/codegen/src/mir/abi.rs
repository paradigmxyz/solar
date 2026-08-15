//! Semantic ABI layout descriptors used by MIR encoding operations.

use super::{FunctionBuilder, MirType, SliceLocation, ValueId};
use alloy_primitives::U256;
use std::{fmt, sync::Arc};

/// An interned ABI tuple layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AbiLayout {
    /// Types encoded as one ABI tuple.
    pub types: Box<[AbiType]>,
}

impl AbiLayout {
    /// Creates a tuple layout from its element types.
    #[must_use]
    pub(crate) fn new(types: impl Into<Box<[AbiType]>>) -> Self {
        Self { types: types.into() }
    }

    /// Returns the tuple head size in bytes.
    #[must_use]
    pub(crate) fn head_size(&self) -> u64 {
        self.types.iter().map(AbiType::head_size).sum()
    }

    /// Returns the number of scratch words required by the encoder.
    #[must_use]
    pub(crate) fn scratch_words(&self) -> u64 {
        self.types.iter().map(AbiType::loop_depth).max().unwrap_or(0) * 5
    }
}

/// Shared reference returned by the module ABI-layout interner.
pub(crate) type AbiLayoutRef = Arc<AbiLayout>;

/// ABI input shape retained until the ABI lowering phase.
///
/// Unlike [`AbiType`], scalar leaves keep their MIR type so the ABI phase can
/// validate narrow words before it stores them in an aggregate object.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct AbiParamLayout {
    /// Types encoded as one ABI tuple.
    pub types: Box<[AbiParamType]>,
}

impl AbiParamLayout {
    /// Creates an input tuple layout.
    #[must_use]
    pub(crate) fn new(types: impl Into<Box<[AbiParamType]>>) -> Self {
        Self { types: types.into() }
    }

    /// Returns the tuple head size in bytes.
    #[must_use]
    pub(crate) fn head_size(&self) -> u64 {
        self.types.iter().map(AbiParamType::head_size).sum()
    }

    /// Returns the tuple head size, or `None` when it exceeds the ABI layout range.
    #[must_use]
    pub(crate) fn checked_head_size(&self) -> Option<u64> {
        self.types.iter().try_fold(0u64, |size, ty| size.checked_add(ty.checked_head_size()?))
    }
}

/// Source data location of an ABI parameter.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiParamLocation {
    /// A memory-backed parameter is decoded before entering the body.
    Memory,
    /// A calldata-backed parameter remains lazy until the body uses it.
    Calldata,
}

impl fmt::Display for AbiParamLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (index, ty) in self.types.iter().enumerate() {
            if index != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{ty}")?;
        }
        write!(f, "]")
    }
}

/// ABI input shape with scalar type information for decoding.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiParamType {
    /// A scalar encoded as one word.
    Scalar(MirType),
    /// An enum encoded as a bounded unsigned word.
    Enum {
        /// MIR representation of the enum value.
        ty: MirType,
        /// Number of declared variants.
        variants: u64,
    },
    /// A dynamic byte string.
    Bytes,
    /// A dynamic array.
    DynamicArray(Box<Self>),
    /// A fixed-size array.
    FixedArray {
        /// Array element layout.
        element: Box<Self>,
        /// Number of elements.
        len: u64,
    },
    /// A struct or tuple.
    Tuple(Box<[Self]>),
}

impl fmt::Display for AbiParamType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Scalar(ty) => write!(f, "{ty}"),
            Self::Enum { ty, variants } => write!(f, "enum<{variants}, {ty}>"),
            Self::Bytes => f.write_str("bytes"),
            Self::DynamicArray(element) => write!(f, "array<_, {element}>"),
            Self::FixedArray { element, len } => write!(f, "array<{len}, {element}>"),
            Self::Tuple(fields) => {
                write!(f, "tuple<")?;
                for (index, ty) in fields.iter().enumerate() {
                    if index != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ">")
            }
        }
    }
}

impl AbiParamType {
    /// Returns whether ABI return encoding must canonicalize a value in this shape.
    #[must_use]
    pub(crate) fn needs_return_cleanup(&self) -> bool {
        match self {
            Self::Scalar(ty) | Self::Enum { ty, .. } => match ty {
                MirType::UInt(size) | MirType::Int(size) => size.bits() < 256,
                MirType::Address | MirType::Bool | MirType::Function => true,
                MirType::FixedBytes(size) => size.bytes() < 32,
                _ => false,
            },
            Self::Bytes => false,
            Self::FixedArray { element, .. } | Self::DynamicArray(element) => {
                element.needs_return_cleanup()
            }
            Self::Tuple(fields) => fields.iter().any(Self::needs_return_cleanup),
        }
    }

    /// Returns whether a nested ABI value needs return-word canonicalization.
    #[must_use]
    pub(crate) fn needs_nested_return_cleanup(&self) -> bool {
        match self {
            Self::Scalar(..) | Self::Bytes => false,
            // Return encoding needs the variant count to validate the raw word.
            Self::Enum { .. } => true,
            Self::FixedArray { .. } | Self::DynamicArray(..) | Self::Tuple(..) => {
                self.needs_return_cleanup()
            }
        }
    }

    /// Returns the memory representation used for an aggregate child.
    #[must_use]
    pub(crate) fn mir_type(&self) -> MirType {
        match self {
            Self::Scalar(ty) => *ty,
            Self::Enum { ty, .. } => *ty,
            Self::Bytes => MirType::MemoryObject(super::MemoryObjectKind::Bytes),
            Self::DynamicArray(_) => MirType::MemoryObject(super::MemoryObjectKind::DynamicArray),
            Self::FixedArray { .. } => MirType::MemoryObject(super::MemoryObjectKind::FixedArray),
            Self::Tuple(_) => MirType::MemoryObject(super::MemoryObjectKind::Struct),
        }
    }

    /// Returns whether the ABI value occupies an offset in its containing head.
    #[must_use]
    pub(crate) fn is_dynamic(&self) -> bool {
        match self {
            Self::Scalar(_) | Self::Enum { .. } => false,
            Self::Bytes | Self::DynamicArray(_) => true,
            Self::FixedArray { element, .. } => element.is_dynamic(),
            Self::Tuple(fields) => fields.iter().any(Self::is_dynamic),
        }
    }

    /// Returns the size occupied by this value in its containing tuple head.
    #[must_use]
    pub(crate) fn head_size(&self) -> u64 {
        if self.is_dynamic() {
            return 32;
        }
        match self {
            Self::FixedArray { element, len } => element.head_size() * len,
            Self::Tuple(fields) => fields.iter().map(Self::head_size).sum(),
            _ => 32,
        }
    }

    /// Returns the size of this value's in-place ABI head.
    #[must_use]
    pub(crate) fn data_head_size(&self) -> u64 {
        match self {
            Self::FixedArray { element, len } => element.head_size().saturating_mul(*len),
            Self::Tuple(fields) => {
                fields.iter().fold(0, |size, field| size.saturating_add(field.head_size()))
            }
            _ => 32,
        }
    }

    /// Returns the static head size, or `None` when the shape exceeds the
    /// representable ABI layout range.
    #[must_use]
    pub(crate) fn checked_head_size(&self) -> Option<u64> {
        if self.is_dynamic() {
            return Some(32);
        }
        match self {
            Self::FixedArray { element, len } => element.checked_head_size()?.checked_mul(*len),
            Self::Tuple(fields) => fields
                .iter()
                .try_fold(0u64, |size, field| size.checked_add(field.checked_head_size()?)),
            _ => Some(32),
        }
    }
}

/// The ABI-relevant shape and source representation of one value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiType {
    /// A scalar encoded as one word.
    Word,
    /// An external function pointer encoded as a left-aligned `bytes24` word.
    Function,
    /// A dynamic byte string represented in the given address space.
    Bytes(SliceLocation),
    /// A dynamic array represented in the given address space.
    DynamicArray {
        /// Array element layout.
        element: Box<Self>,
        /// Address space containing the array.
        location: SliceLocation,
    },
    /// A fixed-size array represented by a memory pointer.
    FixedArray {
        /// Array element layout.
        element: Box<Self>,
        /// Number of elements.
        len: u64,
    },
    /// A struct or tuple represented by a memory pointer.
    Tuple(Box<[Self]>),
}

impl AbiType {
    /// Returns whether the ABI value occupies an offset in its containing head.
    #[must_use]
    pub(crate) fn is_dynamic(&self) -> bool {
        match self {
            Self::Word | Self::Function => false,
            Self::Bytes(_) | Self::DynamicArray { .. } => true,
            Self::FixedArray { element, .. } => element.is_dynamic(),
            Self::Tuple(fields) => fields.iter().any(Self::is_dynamic),
        }
    }

    /// Returns the size occupied by this value in its containing tuple head.
    #[must_use]
    pub(crate) fn head_size(&self) -> u64 {
        if self.is_dynamic() {
            return 32;
        }
        match self {
            Self::FixedArray { element, len } => element.head_size() * len,
            Self::Tuple(fields) => fields.iter().map(Self::head_size).sum(),
            _ => 32,
        }
    }

    /// Returns the maximum nested dynamic-array loop depth.
    #[must_use]
    pub(crate) fn loop_depth(&self) -> u64 {
        match self {
            Self::DynamicArray { element, .. } if matches!(element.as_ref(), Self::Word) => 0,
            Self::DynamicArray { element, .. } => 1 + element.loop_depth(),
            Self::FixedArray { element, .. } => element.loop_depth(),
            Self::Tuple(fields) => fields.iter().map(Self::loop_depth).max().unwrap_or(0),
            Self::Word | Self::Function | Self::Bytes(_) => 0,
        }
    }
}

impl fmt::Display for AbiLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[")?;
        for (index, ty) in self.types.iter().enumerate() {
            if index != 0 {
                write!(f, ", ")?;
            }
            write!(f, "{ty}")?;
        }
        write!(f, "]")
    }
}

impl fmt::Display for AbiType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word => write!(f, "word"),
            Self::Function => write!(f, "function"),
            // ABI values live in calldata (inputs) or memory (outputs); the
            // location's own `Display` yields the `memory`/`calldata` prefix.
            Self::Bytes(location) => write!(f, "{location}_bytes"),
            Self::DynamicArray { element, location } => write!(f, "{location}_array<{element}>"),
            Self::FixedArray { element, len } => write!(f, "array<{len}, {element}>"),
            Self::Tuple(fields) => {
                write!(f, "tuple<")?;
                for (index, ty) in fields.iter().enumerate() {
                    if index != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{ty}")?;
                }
                write!(f, ">")
            }
        }
    }
}

/// Validation and canonicalization of a narrow ABI word.
///
/// Shared by the ABI lowering phase (wrappers, constructors, returns) and the
/// calldata decoding helpers in the function lowerer.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub(crate) enum AbiWordValidator {
    /// Word must equal itself masked to a low-bit (or high-bit) range.
    Mask(U256),
    /// Word must equal its sign-extension of `byte_index` bytes.
    SignExtend(u64),
    /// Word must be a canonical 0 or 1.
    Bool,
    /// Word must be less than the number of enum variants.
    EnumRange(u64),
}

impl AbiWordValidator {
    /// Returns the validator for a scalar MIR type, or `None` when a full word
    /// carries no canonicality requirement.
    #[must_use]
    pub(crate) fn from_mir_type(ty: MirType) -> Option<Self> {
        Some(match ty {
            MirType::UInt(size) => {
                let bits = size.bits();
                if bits >= 256 {
                    return None;
                }
                Self::Mask(U256::MAX >> (256 - usize::from(bits)))
            }
            MirType::Int(size) => {
                let bits = size.bits();
                if bits >= 256 {
                    return None;
                }
                Self::SignExtend(u64::from(bits / 8) - 1)
            }
            MirType::Address => Self::Mask(U256::MAX >> 96),
            MirType::FixedBytes(size) => {
                let bytes = size.bytes();
                if bytes >= 32 {
                    return None;
                }
                Self::Mask(U256::MAX << (256 - 8 * usize::from(bytes)))
            }
            MirType::Function => Self::Mask(U256::MAX << 64),
            MirType::Bool => Self::Bool,
            _ => return None,
        })
    }

    /// Returns the validator for a scalar return type.
    #[must_use]
    pub(crate) fn from_return_mir_type(ty: MirType) -> Option<Self> {
        if ty == MirType::Function {
            return Some(Self::Mask(U256::MAX >> 64));
        }
        Self::from_mir_type(ty)
    }

    /// Builds the condition that is true when `word` is canonical.
    pub(crate) fn condition(self, builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
        match self {
            Self::Mask(mask) => {
                let mask = builder.imm_u256(mask);
                let canonical = builder.and(word, mask);
                builder.eq(word, canonical)
            }
            Self::SignExtend(byte_index) => {
                let byte_index = builder.imm_u64(byte_index);
                let canonical = builder.signextend(byte_index, word);
                builder.eq(word, canonical)
            }
            Self::Bool => {
                let zero = builder.iszero(word);
                let canonical = builder.iszero(zero);
                builder.eq(word, canonical)
            }
            Self::EnumRange(variants) => {
                let variants = builder.imm_u64(variants);
                builder.lt(word, variants)
            }
        }
    }

    /// Builds the canonical form of `word`.
    pub(crate) fn cleanup(self, builder: &mut FunctionBuilder<'_>, word: ValueId) -> ValueId {
        match self {
            Self::Mask(mask) => {
                let mask = builder.imm_u256(mask);
                builder.and(word, mask)
            }
            Self::SignExtend(byte_index) => {
                let byte_index = builder.imm_u64(byte_index);
                builder.signextend(byte_index, word)
            }
            Self::Bool => {
                let zero = builder.iszero(word);
                builder.iszero(zero)
            }
            Self::EnumRange(variants) => {
                let mask = enum_cleanup_mask(variants);
                let mask = builder.imm_u256(mask);
                builder.and(word, mask)
            }
        }
    }
}

/// Returns the mask keeping the low `bits` bits needed to represent `variants`
/// distinct enum values, where `bits = ceil(log2(variants))` and at least 1.
#[must_use]
pub(crate) fn enum_cleanup_mask(variants: u64) -> U256 {
    let bits = (u64::BITS - (variants.max(1) - 1).leading_zeros()).max(1);
    U256::MAX >> (256 - bits as usize)
}

#[cfg(test)]
mod tests {
    use super::enum_cleanup_mask;
    use alloy_primitives::U256;

    #[test]
    fn enum_cleanup_mask_masks_low_bits() {
        assert_eq!(enum_cleanup_mask(1), U256::from(0b1));
        assert_eq!(enum_cleanup_mask(2), U256::from(0b1));
        assert_eq!(enum_cleanup_mask(3), U256::from(0b11));
        assert_eq!(enum_cleanup_mask(256), U256::from(0xff));
    }
}

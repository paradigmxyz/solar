//! Semantic ABI layout descriptors used by MIR encoding operations.

use super::{MirType, SliceLocation};
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
            Self::Scalar(..) | Self::Enum { .. } | Self::Bytes => false,
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

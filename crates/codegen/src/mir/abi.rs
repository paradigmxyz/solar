//! Semantic ABI layout descriptors used by MIR encoding operations.

use super::SliceLocation;
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

/// The ABI-relevant shape and source representation of one value.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum AbiType {
    /// A scalar encoded as one word.
    Word,
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
            Self::Word => false,
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
            Self::Word | Self::Bytes(_) => 0,
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

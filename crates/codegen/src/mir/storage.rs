//! Semantic layouts for statically shaped memory/storage aggregates.

use std::{fmt, sync::Arc};

/// An interned layout for a statically shaped aggregate copied between memory
/// and storage.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageLayout {
    /// A struct, with one memory word per field.
    Struct(Box<[StorageField]>),
    /// A fixed-size array, with one memory word per element.
    Array {
        /// Element shape.
        element: StorageField,
        /// Number of elements.
        len: u64,
    },
}

impl StorageLayout {
    /// Returns the number of words in this aggregate's direct memory allocation.
    #[must_use]
    pub fn memory_words(&self) -> u64 {
        match self {
            Self::Struct(fields) => fields.len().max(1) as u64,
            Self::Array { len, .. } => (*len).max(1),
        }
    }

    /// Returns the number of contiguous storage slots occupied by this aggregate.
    #[must_use]
    pub fn storage_slots(&self) -> u64 {
        match self {
            Self::Struct(fields) => {
                fields.iter().map(StorageField::storage_slots).sum::<u64>().max(1)
            }
            Self::Array { element, len } => element.storage_slots().saturating_mul(*len).max(1),
        }
    }

    /// Returns whether copying this aggregate requires following or creating a
    /// nested memory allocation.
    #[must_use]
    pub fn has_nested_layout(&self) -> bool {
        match self {
            Self::Struct(fields) => fields.iter().any(StorageField::is_aggregate),
            Self::Array { element, .. } => element.is_aggregate(),
        }
    }
}

/// The storage shape represented by one word in a parent memory allocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageField {
    /// One scalar storage slot stored directly in the parent memory word.
    Word,
    /// A nested aggregate represented by a pointer in the parent memory word.
    Aggregate(StorageLayoutRef),
}

impl StorageField {
    /// Returns the number of storage slots occupied by this field.
    #[must_use]
    pub fn storage_slots(&self) -> u64 {
        match self {
            Self::Word => 1,
            Self::Aggregate(layout) => layout.storage_slots(),
        }
    }

    /// Returns whether this field refers to a nested aggregate allocation.
    #[must_use]
    pub const fn is_aggregate(&self) -> bool {
        matches!(self, Self::Aggregate(_))
    }
}

/// Shared reference returned by the module storage-layout interner.
pub type StorageLayoutRef = Arc<StorageLayout>;

impl fmt::Display for StorageLayout {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Struct(fields) => {
                write!(f, "struct<")?;
                for (index, field) in fields.iter().enumerate() {
                    if index != 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{field}")?;
                }
                write!(f, ">")
            }
            Self::Array { element, len } => write!(f, "array<{len}, {element}>"),
        }
    }
}

impl fmt::Display for StorageField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word => write!(f, "word"),
            Self::Aggregate(layout) => write!(f, "{layout}"),
        }
    }
}

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
        let mut cursor = StorageCursor::default();
        match self {
            Self::Struct(fields) => {
                for field in fields {
                    cursor.allocate(field);
                }
                cursor.storage_slots()
            }
            Self::Array { element: StorageField::Packed(field), len } => {
                let elements_per_slot = u64::from(32 / field.size);
                len.div_ceil(elements_per_slot).max(1)
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

/// The storage representation of a scalar smaller than one word.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackedStorageField {
    /// Number of bytes occupied in storage.
    pub size: u8,
    /// Whether the MIR value keeps its bytes at the most-significant end of the word.
    pub left_aligned: bool,
    /// Whether loads need sign extension.
    pub signed: bool,
}

impl PackedStorageField {
    /// Creates a packed scalar description.
    #[must_use]
    pub const fn new(size: u8, left_aligned: bool, signed: bool) -> Self {
        Self { size, left_aligned, signed }
    }
}

/// The storage shape represented by one word in a parent memory allocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageField {
    /// One scalar storage slot stored directly in the parent memory word.
    Word,
    /// One scalar packed into a byte window of a storage slot.
    Packed(PackedStorageField),
    /// A nested aggregate represented by a pointer in the parent memory word.
    Aggregate(StorageLayoutRef),
}

impl StorageField {
    /// Returns the number of storage slots occupied by this field.
    #[must_use]
    pub fn storage_slots(&self) -> u64 {
        match self {
            Self::Word | Self::Packed(_) => 1,
            Self::Aggregate(layout) => layout.storage_slots(),
        }
    }

    /// Returns whether this field refers to a nested aggregate allocation.
    #[must_use]
    pub const fn is_aggregate(&self) -> bool {
        matches!(self, Self::Aggregate(_))
    }
}

/// A field's slot and byte offset relative to its aggregate base.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct StoragePosition {
    pub(crate) slot: u64,
    pub(crate) offset: u8,
}

/// Allocates fields according to Solidity's storage packing rules.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct StorageCursor {
    slot: u64,
    offset: u8,
}

impl StorageCursor {
    /// Allocates one field and returns its relative storage position.
    pub(crate) fn allocate(&mut self, field: &StorageField) -> StoragePosition {
        if let StorageField::Packed(field) = field {
            if self.offset + field.size > 32 {
                self.slot = self.slot.saturating_add(1);
                self.offset = 0;
            }
            let position = StoragePosition { slot: self.slot, offset: self.offset };
            self.offset += field.size;
            if self.offset == 32 {
                self.slot = self.slot.saturating_add(1);
                self.offset = 0;
            }
            return position;
        }

        self.align();
        let position = StoragePosition { slot: self.slot, offset: 0 };
        self.slot = self.slot.saturating_add(field.storage_slots());
        position
    }

    /// Returns the rounded-up number of occupied slots.
    pub(crate) fn storage_slots(self) -> u64 {
        self.slot.saturating_add(u64::from(self.offset != 0)).max(1)
    }

    fn align(&mut self) {
        if self.offset != 0 {
            self.slot = self.slot.saturating_add(1);
            self.offset = 0;
        }
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
            Self::Packed(field) => {
                write!(
                    f,
                    "word<{}, {}, {}>",
                    field.size,
                    u8::from(field.left_aligned),
                    u8::from(field.signed)
                )
            }
            Self::Aggregate(layout) => write!(f, "{layout}"),
        }
    }
}

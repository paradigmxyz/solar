//! Semantic layouts for statically shaped memory/storage aggregates.

use std::{fmt, sync::Arc};

/// An interned layout for a statically shaped aggregate copied between memory
/// and storage, using solc's packed storage layout.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageLayout {
    /// A struct, with one memory word per field and explicitly placed storage
    /// fields.
    Struct(Box<[StructField]>),
    /// A fixed-size array, with one memory word per element. Packed elements
    /// share slots by their stride.
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
            Self::Struct(fields) => fields
                .iter()
                .map(|field| field.slot + field.shape.storage_slots())
                .max()
                .unwrap_or(0)
                .max(1),
            Self::Array { element, len } => match element {
                StorageField::Packed(value) => {
                    let per_slot = u64::from(value.per_slot());
                    len.div_ceil(per_slot).max(1)
                }
                _ => element.storage_slots().saturating_mul(*len).max(1),
            },
        }
    }

    /// Returns whether copying this aggregate requires following or creating a
    /// nested memory allocation.
    #[must_use]
    pub fn has_nested_layout(&self) -> bool {
        match self {
            Self::Struct(fields) => fields.iter().any(|field| field.shape.is_aggregate()),
            Self::Array { element, .. } => element.is_aggregate(),
        }
    }

    /// Visits storage slots whose aggregate copy preserves bytes with a
    /// read-modify-write instead of replacing the complete word.
    pub(crate) fn for_each_preserved_storage_slot(&self, mut visit: impl FnMut(u64)) {
        self.for_each_preserved_storage_slot_at(0, &mut visit);
    }

    fn for_each_preserved_storage_slot_at(&self, base: u64, visit: &mut impl FnMut(u64)) {
        match self {
            Self::Struct(fields) => {
                for (index, field) in fields.iter().enumerate() {
                    match &field.shape {
                        StorageField::Aggregate(layout) => layout
                            .for_each_preserved_storage_slot_at(
                                base.saturating_add(field.slot),
                                visit,
                            ),
                        StorageField::Word => {}
                        StorageField::Packed(_) => {
                            if fields[..index].iter().any(|earlier| earlier.slot == field.slot) {
                                continue;
                            }
                            let mut covered = 0u64;
                            for packed in fields.iter().filter(|packed| packed.slot == field.slot) {
                                match packed.shape {
                                    StorageField::Word => {
                                        covered = u64::from(u32::MAX);
                                        break;
                                    }
                                    StorageField::Packed(value) => {
                                        let bytes = (1u64 << value.size) - 1;
                                        covered |= bytes << packed.offset;
                                    }
                                    StorageField::Aggregate(_) => {}
                                }
                            }
                            if covered != u64::from(u32::MAX) {
                                visit(base.saturating_add(field.slot));
                            }
                        }
                    }
                }
            }
            Self::Array { element, len } => match element {
                StorageField::Word => {}
                StorageField::Packed(value) => {
                    let per_slot = u64::from(value.per_slot());
                    for slot in 0..len.div_ceil(per_slot) {
                        let remaining = len.saturating_sub(slot.saturating_mul(per_slot));
                        let values = remaining.min(per_slot);
                        if values.saturating_mul(u64::from(value.size)) < 32 {
                            visit(base.saturating_add(slot));
                        }
                    }
                }
                StorageField::Aggregate(layout) => {
                    let stride = layout.storage_slots();
                    for index in 0..*len {
                        layout.for_each_preserved_storage_slot_at(
                            base.saturating_add(index.saturating_mul(stride)),
                            visit,
                        );
                    }
                }
            },
        }
    }
}

/// A struct field placed at an explicit storage position within its aggregate.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct StructField {
    /// Slot offset of this field relative to the aggregate's base slot.
    pub slot: u64,
    /// Byte offset of the value's low-order byte within its slot. Zero for
    /// full-word and aggregate fields.
    pub offset: u8,
    /// The field's storage shape.
    pub shape: StorageField,
}

/// The storage shape represented by one word in a parent memory allocation.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum StorageField {
    /// One scalar storage slot stored directly in the parent memory word.
    Word,
    /// A scalar packed into part of a shared storage slot.
    Packed(PackedValue),
    /// A nested aggregate represented by a pointer in the parent memory word.
    Aggregate(StorageLayoutRef),
}

impl StorageField {
    /// Returns the number of storage slots occupied by this field when placed
    /// at a slot boundary. Packed fields count the slot they share.
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

/// A scalar value packed into part of a storage slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct PackedValue {
    /// Value size in bytes, in `1..=31`.
    pub size: u8,
    /// How the packed bytes map to the value's canonical word form.
    pub kind: PackedKind,
}

impl PackedValue {
    /// Returns how many values of this size share one storage slot.
    #[must_use]
    pub const fn per_slot(self) -> u8 {
        32 / self.size
    }

    /// Returns the low-aligned bit mask covering the value's bytes.
    #[must_use]
    pub fn mask(self) -> alloy_primitives::U256 {
        (alloy_primitives::U256::from(1) << (usize::from(self.size) * 8))
            - alloy_primitives::U256::from(1)
    }
}

/// How a packed value's storage bytes map to its canonical word form.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PackedKind {
    /// Low-aligned, zero-extended: unsigned integers, addresses, booleans,
    /// enums, and function values.
    Unsigned,
    /// Low-aligned, sign-extended: signed integers.
    Signed,
    /// High-aligned, zero-padded on the right: `bytesN`.
    HighAligned,
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

impl fmt::Display for StructField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.slot)?;
        if self.offset != 0 {
            write!(f, "+{}", self.offset)?;
        }
        write!(f, ":{}", self.shape)
    }
}

impl fmt::Display for StorageField {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Word => write!(f, "word"),
            Self::Packed(value) => write!(f, "{value}"),
            Self::Aggregate(layout) => write!(f, "{layout}"),
        }
    }
}

impl fmt::Display for PackedValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.kind {
            PackedKind::Unsigned => write!(f, "u{}", u16::from(self.size) * 8),
            PackedKind::Signed => write!(f, "i{}", u16::from(self.size) * 8),
            PackedKind::HighAligned => write!(f, "b{}", self.size),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn u128_field(slot: u64, offset: u8) -> StructField {
        StructField {
            slot,
            offset,
            shape: StorageField::Packed(PackedValue { size: 16, kind: PackedKind::Unsigned }),
        }
    }

    #[test]
    fn packed_struct_slot_count() {
        let market = StorageLayout::Struct(
            (0..6u64).map(|i| u128_field(i / 2, if i % 2 == 0 { 0 } else { 16 })).collect(),
        );
        assert_eq!(market.storage_slots(), 3);
        assert_eq!(market.memory_words(), 6);
        assert_eq!(
            market.to_string(),
            "struct<0:u128, 0+16:u128, 1:u128, 1+16:u128, 2:u128, 2+16:u128>"
        );
    }

    #[test]
    fn packed_array_slot_count() {
        let layout = StorageLayout::Array {
            element: StorageField::Packed(PackedValue { size: 16, kind: PackedKind::Unsigned }),
            len: 5,
        };
        assert_eq!(layout.storage_slots(), 3);
        let addresses = StorageLayout::Array {
            element: StorageField::Packed(PackedValue { size: 20, kind: PackedKind::Unsigned }),
            len: 3,
        };
        assert_eq!(addresses.storage_slots(), 3);
    }
}

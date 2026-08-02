use super::Lowerer;
use crate::mir::{
    FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, StorageField, StorageLayout,
    StorageLayoutRef, TypeSize, ValueId,
};
use alloy_primitives::U256;
use solar_interface::Span;
use solar_sema::{
    hir,
    hir::ElementaryType,
    ty::{Ty, TyKind},
};
use std::sync::Arc;

/// Storage position for a state variable.
#[derive(Clone, Copy, Debug)]
pub(super) struct StorageLocation {
    pub(super) slot: U256,
    pub(super) offset: u8,
    pub(super) size: TypeSize,
    pub(super) encoding: StorageEncoding,
}

/// Semantic encoding of a packed storage value.
#[derive(Clone, Copy, Debug)]
pub(super) enum StorageEncoding {
    Unsigned,
    Signed,
    FixedBytes,
}

#[derive(Clone, Copy)]
enum PackedStorageOffset {
    Static(u8),
    Dynamic(ValueId),
}

#[derive(Clone, Copy)]
struct PackedStorageField {
    offset: PackedStorageOffset,
    size: TypeSize,
    encoding: StorageEncoding,
}

#[derive(Clone, Copy)]
enum MemoryObjectAccess {
    Field { object: ValueId, layout: MemoryObjectLayout, field: u64 },
    Element { object: ValueId, layout: MemoryObjectLayout, index: ValueId },
}

fn store_memory_object_word(
    builder: &mut FunctionBuilder<'_>,
    destination: MemoryObjectAccess,
    value: ValueId,
) {
    match destination {
        MemoryObjectAccess::Field { object, layout, field } => {
            builder.memory_object_store_field(object, layout, field, value)
        }
        MemoryObjectAccess::Element { object, layout, index } => {
            builder.memory_object_store_element(object, layout, index, value)
        }
    }
}

fn load_memory_object_word(
    builder: &mut FunctionBuilder<'_>,
    source: MemoryObjectAccess,
) -> ValueId {
    match source {
        MemoryObjectAccess::Field { object, layout, field } => {
            builder.memory_object_load_field(object, layout, field)
        }
        MemoryObjectAccess::Element { object, layout, index } => {
            builder.memory_object_load_element(object, layout, index)
        }
    }
}

impl StorageLocation {
    pub(super) const WORD_SIZE: TypeSize = TypeSize::new_int_bits(256);

    const fn full_word(slot: U256) -> Self {
        Self { slot, offset: 0, size: Self::WORD_SIZE, encoding: StorageEncoding::Unsigned }
    }

    const fn is_packed(self) -> bool {
        self.offset != 0 || self.size.bits() != Self::WORD_SIZE.bits()
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Stores one Solidity value at a runtime-computed storage slot.
    pub(super) fn store_storage_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slot: ValueId,
        value: ValueId,
    ) {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => {
                let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
                let fields = field_tys.len() as u64;
                for (index, field_ty) in field_tys.into_iter().enumerate() {
                    let field_value = builder.memory_object_load_field(
                        value,
                        MemoryObjectLayout::structure(fields),
                        index as u64,
                    );
                    let location = self.get_struct_field_storage_location(struct_id, index);
                    let field_slot = self.offset_storage_slot_u256(builder, slot, location.slot);
                    if location.is_packed() {
                        self.store_storage_location_at_slot(
                            builder,
                            location,
                            field_slot,
                            field_value,
                        );
                    } else {
                        self.store_storage_value_at(builder, field_ty, field_slot, field_value);
                    }
                }
            }
            TyKind::Array(element_ty, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                for index in 0..len {
                    let index_value = builder.imm_u64(index);
                    let element_value = builder.memory_object_load_element(
                        value,
                        MemoryObjectLayout::word_fixed_array(len),
                        index_value,
                    );
                    if let Some((size, encoding)) = self.packed_storage_encoding(element_ty)
                        && size < StorageLocation::WORD_SIZE
                    {
                        let location = Self::packed_array_location(size, encoding, index);
                        self.store_storage_location_at_slot(builder, location, slot, element_value);
                    } else {
                        let element_slots =
                            self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                        let storage_offset = index.saturating_mul(element_slots);
                        let element_slot = self.offset_storage_slot(builder, slot, storage_offset);
                        self.store_storage_value_at(
                            builder,
                            element_ty,
                            element_slot,
                            element_value,
                        );
                    }
                }
            }
            TyKind::DynArray(element_ty) => {
                self.copy_memory_dyn_array_to_storage(builder, slot, value, element_ty);
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.copy_memory_bytes_to_storage(builder, slot, value);
            }
            TyKind::Mapping(..) => {}
            _ => builder.sstore(slot, value),
        }
    }

    /// Clears one Solidity value at a runtime-computed storage slot.
    pub(super) fn clear_storage_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slot: ValueId,
    ) {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Struct(struct_id) => {
                let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
                for (index, field_ty) in field_tys.into_iter().enumerate() {
                    let location = self.get_struct_field_storage_location(struct_id, index);
                    let field_slot = self.offset_storage_slot_u256(builder, slot, location.slot);
                    if location.is_packed() {
                        let zero = builder.imm_u64(0);
                        self.store_storage_location_at_slot(builder, location, field_slot, zero);
                    } else {
                        self.clear_storage_value_at(builder, field_ty, field_slot);
                    }
                }
            }
            TyKind::Array(element_ty, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                for index in 0..len {
                    if let Some((size, encoding)) = self.packed_storage_encoding(element_ty)
                        && size < StorageLocation::WORD_SIZE
                    {
                        let location = Self::packed_array_location(size, encoding, index);
                        let zero = builder.imm_u64(0);
                        self.store_storage_location_at_slot(builder, location, slot, zero);
                    } else {
                        let element_slots =
                            self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                        let storage_offset = index.saturating_mul(element_slots);
                        let element_slot = self.offset_storage_slot(builder, slot, storage_offset);
                        self.clear_storage_value_at(builder, element_ty, element_slot);
                    }
                }
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                let empty = self.allocate_memory_object(builder, 32, MemoryObjectKind::Bytes);
                let zero = builder.imm_u64(0);
                builder.set_memory_object_len(empty, zero, MemoryObjectKind::Bytes);
                self.copy_memory_bytes_to_storage(builder, slot, empty);
            }
            TyKind::Mapping(..) => {}
            TyKind::Err(_) => {}
            _ if matches!(ty.kind, TyKind::DynArray(_)) || ty.is_value_type() => {
                let zero = builder.imm_u64(0);
                builder.sstore(slot, zero);
            }
            _ => {
                self.gcx.dcx().err("codegen cannot clear this storage value").emit();
            }
        }
    }

    fn offset_storage_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        offset: u64,
    ) -> ValueId {
        self.offset_storage_slot_u256(builder, slot, U256::from(offset))
    }

    fn offset_storage_slot_u256(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        offset: U256,
    ) -> ValueId {
        if offset.is_zero() {
            slot
        } else {
            let offset = builder.imm_u256(offset);
            builder.add(slot, offset)
        }
    }

    fn packed_array_location(
        size: TypeSize,
        encoding: StorageEncoding,
        index: u64,
    ) -> StorageLocation {
        let bytes = u64::from(size.bytes());
        let per_slot = u64::from(StorageLocation::WORD_SIZE.bytes()) / bytes;
        StorageLocation {
            slot: U256::from(index / per_slot),
            offset: ((index % per_slot) * bytes) as u8,
            size,
            encoding,
        }
    }

    /// Allocates the storage location for a state variable.
    pub(super) fn allocate_storage_location(
        &mut self,
        ty: Ty<'gcx>,
        span: Span,
    ) -> StorageLocation {
        if let Some((size, encoding)) = self.packed_storage_encoding(ty)
            && size < StorageLocation::WORD_SIZE
        {
            let bytes = size.bytes();
            if self.next_storage_offset + bytes > StorageLocation::WORD_SIZE.bytes() {
                self.next_storage_slot += U256::from(1);
                self.next_storage_offset = 0;
            }
            let location = StorageLocation {
                slot: self.next_storage_slot,
                offset: self.next_storage_offset,
                size,
                encoding,
            };
            self.next_storage_offset += bytes;
            if self.next_storage_offset == StorageLocation::WORD_SIZE.bytes() {
                self.next_storage_slot += U256::from(1);
                self.next_storage_offset = 0;
            }
            return location;
        }

        if self.next_storage_offset != 0 {
            self.next_storage_slot += U256::from(1);
            self.next_storage_offset = 0;
        }

        let slot = self.next_storage_slot;
        let num_slots = self.calculate_storage_slots_for_ty(ty, span);
        // Storage slots span the EVM's full 2^256 space; a layout that walks
        // past its end wraps back onto earlier variables, so reject it.
        match self.next_storage_slot.checked_add(U256::from(num_slots)) {
            Some(next) => self.next_storage_slot = next,
            None => {
                self.gcx
                    .dcx()
                    .err("contract storage layout exceeds the addressable storage space")
                    .span(span)
                    .emit();
            }
        }
        StorageLocation::full_word(slot)
    }

    /// Returns the size of scalar types that this lowering can safely pack.
    pub(super) fn packed_storage_size(&self, ty: Ty<'gcx>) -> Option<TypeSize> {
        self.packed_storage_encoding(ty).map(|(size, _)| size)
    }

    /// Returns the packed size and semantic encoding of a scalar type.
    pub(super) fn packed_storage_encoding(
        &self,
        ty: Ty<'gcx>,
    ) -> Option<(TypeSize, StorageEncoding)> {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bool) => {
                Some((TypeSize::new_int_bits(8), StorageEncoding::Unsigned))
            }
            TyKind::Elementary(ElementaryType::Address(_)) => {
                Some((TypeSize::new_int_bits(160), StorageEncoding::Unsigned))
            }
            TyKind::Elementary(ElementaryType::FixedBytes(size)) => {
                Some((size, StorageEncoding::FixedBytes))
            }
            TyKind::Elementary(ElementaryType::Int(size)) => Some((size, StorageEncoding::Signed)),
            TyKind::Elementary(ElementaryType::UInt(size)) => {
                Some((size, StorageEncoding::Unsigned))
            }
            TyKind::Contract(_) => Some((TypeSize::new_int_bits(160), StorageEncoding::Unsigned)),
            TyKind::Enum(enum_id) => {
                let variants = self.gcx.hir.enumm(enum_id).variants.len().max(1);
                let bits = (usize::BITS - (variants - 1).leading_zeros()).max(1);
                Some((
                    TypeSize::new_int_bits((bits.div_ceil(8) * 8) as u16),
                    StorageEncoding::Unsigned,
                ))
            }
            TyKind::Udvt(inner, _) => self.packed_storage_encoding(inner),
            _ => None,
        }
    }

    /// Calculates the number of storage slots needed for a type.
    pub(super) fn calculate_storage_slots_for_ty(&self, ty: Ty<'gcx>, span: Span) -> u64 {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => self
                .calculate_storage_slots_for_sequence(self.gcx.struct_field_types(struct_id), span),
            TyKind::Array(elem, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx
                        .dcx()
                        .err("fixed-size storage arrays this large are not supported")
                        .span(span)
                        .emit();
                    return 1;
                };
                if let Some(size) = self.packed_storage_size(elem)
                    && size < StorageLocation::WORD_SIZE
                {
                    let Some(bytes) = u64::from(size.bytes()).checked_mul(len) else {
                        self.gcx
                            .dcx()
                            .err("fixed-size storage arrays this large are not supported")
                            .span(span)
                            .emit();
                        return 1;
                    };
                    return bytes.div_ceil(u64::from(StorageLocation::WORD_SIZE.bytes())).max(1);
                }
                len.checked_mul(self.calculate_storage_slots_for_ty(elem, span)).unwrap_or_else(
                    || {
                        self.gcx
                            .dcx()
                            .err("fixed-size storage arrays this large are not supported")
                            .span(span)
                            .emit();
                        1
                    },
                )
            }
            _ => 1,
        }
    }

    fn calculate_storage_slots_for_sequence(&self, tys: &[Ty<'gcx>], span: Span) -> u64 {
        let mut slots = 0u64;
        let mut offset = 0u64;
        for &ty in tys {
            if let Some(size) = self.packed_storage_size(ty)
                && size < StorageLocation::WORD_SIZE
            {
                let bytes = u64::from(size.bytes());
                if offset + bytes > u64::from(StorageLocation::WORD_SIZE.bytes()) {
                    slots = slots.checked_add(1).unwrap_or(1);
                    offset = 0;
                }
                offset += bytes;
                if offset == u64::from(StorageLocation::WORD_SIZE.bytes()) {
                    slots = slots.checked_add(1).unwrap_or(1);
                    offset = 0;
                }
                continue;
            }

            if offset != 0 {
                slots = slots.checked_add(1).unwrap_or(1);
                offset = 0;
            }
            slots = slots
                .checked_add(self.calculate_storage_slots_for_ty(ty, span))
                .unwrap_or_else(|| {
                    self.gcx
                        .dcx()
                        .err("storage aggregates this large are not supported")
                        .span(span)
                        .emit();
                    1
                });
        }
        if offset != 0 {
            slots = slots.saturating_add(1);
        }
        slots.max(1)
    }

    pub(super) fn load_storage_location_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
    ) -> ValueId {
        if !location.is_packed() {
            return builder.sload(slot);
        }

        let field = PackedStorageField {
            offset: PackedStorageOffset::Static(location.offset),
            size: location.size,
            encoding: location.encoding,
        };
        self.load_packed_storage_field(builder, slot, field)
    }

    pub(super) fn store_storage_location(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        value: ValueId,
    ) {
        let slot = builder.imm_u256(location.slot);
        self.store_storage_location_at_slot(builder, location, slot, value);
    }

    pub(super) fn store_storage_location_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        value: ValueId,
    ) {
        if !location.is_packed() {
            builder.sstore(slot, value);
            return;
        }

        let field = PackedStorageField {
            offset: PackedStorageOffset::Static(location.offset),
            size: location.size,
            encoding: location.encoding,
        };
        self.store_packed_storage_field(builder, slot, field, value);
    }

    pub(super) fn load_storage_location_at_dynamic_offset(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        byte_offset: ValueId,
        size: TypeSize,
        encoding: StorageEncoding,
    ) -> ValueId {
        let field = PackedStorageField {
            offset: PackedStorageOffset::Dynamic(byte_offset),
            size,
            encoding,
        };
        self.load_packed_storage_field(builder, slot, field)
    }

    pub(super) fn store_storage_location_at_dynamic_offset(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        byte_offset: ValueId,
        size: TypeSize,
        encoding: StorageEncoding,
        value: ValueId,
    ) {
        let field = PackedStorageField {
            offset: PackedStorageOffset::Dynamic(byte_offset),
            size,
            encoding,
        };
        self.store_packed_storage_field(builder, slot, field, value);
    }

    fn load_packed_storage_field(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        field: PackedStorageField,
    ) -> ValueId {
        let word = builder.sload(slot);
        let shifted = match Self::packed_storage_shift(builder, field.offset) {
            Some(shift) => builder.shr(shift, word),
            None => word,
        };
        let mask = builder.imm_u256(Self::packed_storage_mask(field.size));
        let value = builder.and(shifted, mask);
        match field.encoding {
            StorageEncoding::Unsigned => value,
            StorageEncoding::Signed => {
                let byte_index = builder.imm_u64(u64::from(field.size.bytes()) - 1);
                builder.signextend(byte_index, value)
            }
            StorageEncoding::FixedBytes => {
                let shift = builder.imm_u64(
                    u64::from(StorageLocation::WORD_SIZE.bytes() - field.size.bytes()) * 8,
                );
                builder.shl(shift, value)
            }
        }
    }

    fn store_packed_storage_field(
        &self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        field: PackedStorageField,
        value: ValueId,
    ) {
        let field_mask = Self::packed_storage_mask(field.size);
        let (shift, keep_mask) = match field.offset {
            PackedStorageOffset::Static(offset) => {
                let shifted_mask = field_mask << (usize::from(offset) * 8);
                (Self::packed_storage_shift(builder, field.offset), builder.imm_u256(!shifted_mask))
            }
            PackedStorageOffset::Dynamic(byte_offset) => {
                let eight = builder.imm_u64(8);
                let shift = builder.mul(byte_offset, eight);
                let field_mask = builder.imm_u256(field_mask);
                let shifted_mask = builder.shl(shift, field_mask);
                (Some(shift), builder.not(shifted_mask))
            }
        };
        let word = builder.sload(slot);
        let cleared = builder.and(word, keep_mask);
        let value_mask = builder.imm_u256(field_mask);
        let masked = match field.encoding {
            StorageEncoding::FixedBytes => {
                let shift = builder.imm_u64(
                    u64::from(StorageLocation::WORD_SIZE.bytes() - field.size.bytes()) * 8,
                );
                let shifted = builder.shr(shift, value);
                builder.and(shifted, value_mask)
            }
            StorageEncoding::Unsigned | StorageEncoding::Signed => builder.and(value, value_mask),
        };
        let shifted = match shift {
            Some(shift) => builder.shl(shift, masked),
            None => masked,
        };
        let updated = builder.or(cleared, shifted);
        builder.sstore(slot, updated);
    }

    fn packed_storage_shift(
        builder: &mut FunctionBuilder<'_>,
        offset: PackedStorageOffset,
    ) -> Option<ValueId> {
        match offset {
            PackedStorageOffset::Static(0) => None,
            PackedStorageOffset::Static(offset) => Some(builder.imm_u64(u64::from(offset) * 8)),
            PackedStorageOffset::Dynamic(offset) => {
                let eight = builder.imm_u64(8);
                Some(builder.mul(offset, eight))
            }
        }
    }

    fn packed_storage_mask(size: TypeSize) -> U256 {
        if size >= StorageLocation::WORD_SIZE {
            U256::MAX
        } else {
            (U256::from(1) << size.bits()) - U256::from(1)
        }
    }

    /// Gets the storage slot offset for a struct field.
    pub(super) fn get_struct_field_slot_offset(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> u64 {
        self.get_struct_field_storage_location(struct_id, field_index).slot.as_limbs()[0]
    }

    /// Returns the packed storage position of a struct field relative to the
    /// struct's base slot.
    pub(super) fn get_struct_field_storage_location(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> StorageLocation {
        if let Some(&location) = self.struct_field_offsets.get(&(struct_id, field_index)) {
            return location;
        }

        let mut slot = 0u64;
        let mut offset = 0u64;
        let mut result = StorageLocation::full_word(U256::ZERO);
        for (index, &field_ty) in self.gcx.struct_field_types(struct_id).iter().enumerate() {
            let location = if let Some((size, encoding)) = self.packed_storage_encoding(field_ty)
                && size < StorageLocation::WORD_SIZE
            {
                let bytes = u64::from(size.bytes());
                if offset + bytes > u64::from(StorageLocation::WORD_SIZE.bytes()) {
                    slot = slot.saturating_add(1);
                    offset = 0;
                }
                let location = StorageLocation {
                    slot: U256::from(slot),
                    offset: u8::try_from(offset).unwrap_or(0),
                    size,
                    encoding,
                };
                offset += bytes;
                if offset == u64::from(StorageLocation::WORD_SIZE.bytes()) {
                    slot = slot.saturating_add(1);
                    offset = 0;
                }
                location
            } else {
                if offset != 0 {
                    slot = slot.saturating_add(1);
                    offset = 0;
                }
                let location = StorageLocation::full_word(U256::from(slot));
                slot =
                    slot.saturating_add(self.calculate_storage_slots_for_ty(field_ty, Span::DUMMY));
                location
            };
            if index == field_index {
                result = location;
                break;
            }
        }

        self.struct_field_offsets.insert((struct_id, field_index), result);
        result
    }

    /// Calculates the number of 32-byte memory words needed for a value.
    ///
    /// A memory struct has one word per field. Nested structs and other
    /// reference types occupy one pointer word in their parent allocation.
    pub(super) fn calculate_memory_words_for_ty(&self, ty: Ty<'gcx>) -> u64 {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => self.gcx.struct_field_types(struct_id).len().max(1) as u64,
            _ => 1,
        }
    }

    fn storage_field_for_ty(&mut self, ty: Ty<'gcx>) -> StorageField {
        self.storage_layout_for_ty(ty).map_or(StorageField::Word, StorageField::Aggregate)
    }

    fn storage_layout_for_ty(&mut self, ty: Ty<'gcx>) -> Option<StorageLayoutRef> {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => Some(self.storage_layout_for_struct(struct_id)),
            TyKind::Array(element, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx
                        .dcx()
                        .err("fixed-size storage arrays this large are not supported")
                        .emit();
                    return None;
                };
                let element = self.storage_field_for_ty(element);
                Some(Arc::new(StorageLayout::Array { element, len }))
            }
            _ => None,
        }
    }

    fn storage_layout_for_struct(&mut self, struct_id: hir::StructId) -> StorageLayoutRef {
        if let Some(layout) = self.struct_layouts.get(&struct_id) {
            return Arc::clone(layout);
        }

        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let fields = field_tys
            .into_iter()
            .map(|field_ty| self.storage_field_for_ty(field_ty))
            .collect::<Vec<_>>();
        let layout = Arc::new(StorageLayout::Struct(fields.into()));
        self.struct_layouts.insert(struct_id, Arc::clone(&layout));
        layout
    }

    /// Recursively copies a struct from storage to memory.
    /// Allocates nested structs separately and stores their pointers in the parent.
    /// Returns the next memory offset after all fields are copied.
    pub(super) fn copy_storage_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: U256,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let base_slot = builder.imm_u256(base_slot);
        self.copy_storage_to_memory_at(builder, struct_id, base_slot, mem_ptr, mem_offset)
    }

    /// Recursively copies a struct from a runtime-computed storage slot to memory.
    pub(super) fn copy_storage_to_memory_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let layout = self.storage_layout_for_struct(struct_id);
        let memory = if mem_offset == 0 {
            mem_ptr
        } else {
            let offset = builder.imm_u64(mem_offset);
            builder.add(mem_ptr, offset)
        };
        if self.struct_needs_packed_storage_copy(struct_id) {
            self.copy_packed_storage_struct_to_memory(builder, struct_id, base_slot, memory);
            return mem_offset + self.calculate_memory_words_for_ty_struct(struct_id) * 32;
        }
        builder.storage_to_memory(Arc::clone(&layout), base_slot, memory);
        mem_offset + layout.memory_words() * 32
    }

    fn struct_needs_packed_storage_copy(&self, struct_id: hir::StructId) -> bool {
        self.gcx
            .struct_field_types(struct_id)
            .iter()
            .any(|&ty| self.ty_needs_packed_storage_copy(ty))
    }

    fn ty_needs_packed_storage_copy(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Struct(id) => self.struct_needs_packed_storage_copy(id),
            TyKind::Array(elem, _) => {
                self.packed_storage_size(elem).is_some_and(|size| size < StorageLocation::WORD_SIZE)
                    || self.ty_needs_packed_storage_copy(elem)
            }
            _ => self.packed_storage_size(ty).is_some_and(|size| size < StorageLocation::WORD_SIZE),
        }
    }

    fn copy_packed_storage_struct_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
        memory: ValueId,
    ) {
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let layout = MemoryObjectLayout::structure(field_tys.len() as u64);
        for (index, field_ty) in field_tys.into_iter().enumerate() {
            let field_memory =
                MemoryObjectAccess::Field { object: memory, layout, field: index as u64 };
            let location = self.get_struct_field_storage_location(struct_id, index);
            let field_slot = self.offset_storage_slot_u256(builder, base_slot, location.slot);
            self.copy_storage_field_to_memory(
                builder,
                field_ty,
                location,
                field_slot,
                field_memory,
            );
        }
    }

    fn copy_storage_field_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        location: StorageLocation,
        slot: ValueId,
        destination: MemoryObjectAccess,
    ) {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => {
                let fields = self.gcx.struct_field_types(struct_id).len() as u64;
                let nested = self.allocate_memory_object(
                    builder,
                    fields.max(1) * 32,
                    MemoryObjectKind::Struct,
                );
                if self.struct_needs_packed_storage_copy(struct_id) {
                    self.copy_packed_storage_struct_to_memory(builder, struct_id, slot, nested);
                } else {
                    let layout = self.storage_layout_for_struct(struct_id);
                    builder.storage_to_memory(Arc::clone(&layout), slot, nested);
                }
                store_memory_object_word(builder, destination, nested);
            }
            TyKind::Array(element, len) => {
                let Ok(len) = u64::try_from(len) else { return };
                let array = self.allocate_memory_object(
                    builder,
                    len.max(1) * 32,
                    MemoryObjectKind::FixedArray,
                );
                let layout = MemoryObjectLayout::word_fixed_array(len);
                let packed = self
                    .packed_storage_encoding(element)
                    .filter(|(size, _)| *size < StorageLocation::WORD_SIZE);
                let element_slots = self.calculate_storage_slots_for_ty(element, Span::DUMMY);
                for index in 0..len {
                    let index_value = builder.imm_u64(index);
                    let element_memory =
                        MemoryObjectAccess::Element { object: array, layout, index: index_value };
                    let (element_slot, element_location) = if let Some((size, encoding)) = packed {
                        let element_location = Self::packed_array_location(size, encoding, index);
                        let element_slot =
                            self.offset_storage_slot_u256(builder, slot, element_location.slot);
                        (element_slot, element_location)
                    } else {
                        let offset = U256::from(index.saturating_mul(element_slots));
                        let element_slot = self.offset_storage_slot_u256(builder, slot, offset);
                        (element_slot, StorageLocation::full_word(U256::ZERO))
                    };
                    self.copy_storage_field_to_memory(
                        builder,
                        element,
                        element_location,
                        element_slot,
                        element_memory,
                    );
                }
                store_memory_object_word(builder, destination, array);
            }
            TyKind::DynArray(element) => {
                let value = self.copy_storage_dyn_array_to_memory(builder, slot, element);
                store_memory_object_word(builder, destination, value);
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                let value = self.materialize_storage_bytes(builder, slot);
                store_memory_object_word(builder, destination, value);
            }
            _ => {
                let value = if location.is_packed() {
                    self.load_storage_location_at_slot(builder, location, slot)
                } else {
                    builder.sload(slot)
                };
                store_memory_object_word(builder, destination, value);
            }
        }
    }

    pub(super) fn copy_storage_dyn_array_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        element: Ty<'gcx>,
    ) -> ValueId {
        let len = builder.sload(slot);
        let word = builder.imm_u64(32);
        let data_size = builder.mul(len, word);
        let total_size = builder.add(word, data_size);
        let array = self.allocate_memory_object_dynamic(
            builder,
            total_size,
            MemoryObjectKind::DynamicArray,
        );
        builder.set_memory_object_len(array, len, MemoryObjectKind::DynamicArray);

        let element_slots = self.calculate_storage_slots_for_ty(element, Span::DUMMY);
        let element = element.peel_refs();
        self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
            let storage_slot = builder.storage_array_element_slot(slot, index, element_slots);
            this.copy_storage_field_to_memory(
                builder,
                element,
                StorageLocation::full_word(U256::ZERO),
                storage_slot,
                MemoryObjectAccess::Element {
                    object: array,
                    layout: MemoryObjectLayout::WORD_ARRAY,
                    index,
                },
            );
        });
        array
    }

    /// Clears every storage slot occupied by a struct at a runtime-computed base slot.
    pub(super) fn clear_storage_struct_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
    ) {
        if self.struct_needs_packed_storage_copy(struct_id) {
            self.clear_packed_storage_struct(builder, struct_id, base_slot);
            return;
        }
        let layout = self.storage_layout_for_struct(struct_id);
        builder.clear_storage(layout, base_slot);
    }

    fn clear_packed_storage_struct(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
    ) {
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        for (index, field_ty) in field_tys.into_iter().enumerate() {
            let location = self.get_struct_field_storage_location(struct_id, index);
            let field_slot = self.offset_storage_slot_u256(builder, base_slot, location.slot);
            if location.is_packed() {
                let zero = builder.imm_u64(0);
                self.store_storage_location_at_slot(builder, location, field_slot, zero);
            } else {
                match field_ty.peel_refs().kind {
                    TyKind::Struct(inner) if self.struct_needs_packed_storage_copy(inner) => {
                        self.clear_packed_storage_struct(builder, inner, field_slot);
                    }
                    _ => self.clear_storage_value_at(builder, field_ty, field_slot),
                }
            }
        }
    }

    /// Recursively copies a struct from memory to storage.
    /// Follows nested-struct pointers stored in the parent memory allocation.
    /// Returns the next memory offset after all fields are read.
    pub(super) fn copy_memory_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: U256,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let base_slot = builder.imm_u256(base_slot);
        self.copy_memory_to_storage_at(builder, struct_id, base_slot, mem_ptr, mem_offset)
    }

    /// Recursively copies a struct from memory to a runtime-computed storage slot.
    pub(super) fn copy_memory_to_storage_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let memory = if mem_offset == 0 {
            mem_ptr
        } else {
            let offset = builder.imm_u64(mem_offset);
            builder.add(mem_ptr, offset)
        };
        // Packed fields and dynamic fields do not fit the flat slot-for-word
        // layout instruction; copy field by field so offsets, storage lengths,
        // and payloads are preserved.
        if self.struct_needs_deep_storage_copy(struct_id)
            || self.struct_needs_packed_storage_copy(struct_id)
        {
            self.deep_copy_memory_struct_to_storage(builder, struct_id, base_slot, memory);
            return mem_offset + self.calculate_memory_words_for_ty_struct(struct_id) * 32;
        }
        let layout = self.storage_layout_for_struct(struct_id);
        builder.memory_to_storage(Arc::clone(&layout), memory, base_slot);
        mem_offset + layout.memory_words() * 32
    }

    fn calculate_memory_words_for_ty_struct(&self, struct_id: hir::StructId) -> u64 {
        self.gcx.struct_field_types(struct_id).len().max(1) as u64
    }

    /// Whether a struct (recursively) has a `bytes`/`string`/dynamic-array
    /// field, which the flat layout copy cannot represent.
    fn struct_needs_deep_storage_copy(&self, struct_id: hir::StructId) -> bool {
        self.gcx.struct_field_types(struct_id).iter().any(|&f| self.ty_needs_deep_storage_copy(f))
    }

    fn ty_needs_deep_storage_copy(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_)
            | TyKind::Slice(_) => true,
            TyKind::Struct(id) => self.struct_needs_deep_storage_copy(id),
            TyKind::Array(elem, _) => self.ty_needs_deep_storage_copy(elem),
            TyKind::Udvt(inner, _) => self.ty_needs_deep_storage_copy(inner),
            _ => false,
        }
    }

    /// Copies each field of a memory struct to storage, deep-copying dynamic
    /// fields so their storage length and payload are written.
    fn deep_copy_memory_struct_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
        mem_base: ValueId,
    ) {
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let layout = MemoryObjectLayout::structure(field_tys.len() as u64);
        for (i, &field_ty) in field_tys.iter().enumerate() {
            let location = self.get_struct_field_storage_location(struct_id, i);
            let field_slot = self.offset_storage_slot_u256(builder, base_slot, location.slot);
            let mem_word = load_memory_object_word(
                builder,
                MemoryObjectAccess::Field { object: mem_base, layout, field: i as u64 },
            );
            if location.is_packed() {
                self.store_storage_location_at_slot(builder, location, field_slot, mem_word);
            } else {
                self.copy_memory_field_to_storage(
                    builder,
                    field_ty.peel_refs(),
                    field_slot,
                    mem_word,
                );
            }
        }
    }

    /// Copies one struct field / array element from its memory word to a
    /// storage slot. The memory word is a value for scalar fields or a pointer
    /// for reference fields.
    fn copy_memory_field_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slot: ValueId,
        mem_word: ValueId,
    ) {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.copy_memory_bytes_to_storage(builder, slot, mem_word);
            }
            TyKind::DynArray(elem) => {
                self.copy_memory_dyn_array_to_storage(builder, slot, mem_word, elem);
            }
            TyKind::Struct(id) => {
                // `mem_word` is a pointer to the nested struct's memory.
                self.copy_memory_to_storage_at(builder, id, slot, mem_word, 0);
            }
            TyKind::Array(elem, len) => {
                self.copy_memory_fixed_array_to_storage(builder, slot, mem_word, elem, len.to());
            }
            _ => {
                // A scalar field: its memory word is the value.
                builder.sstore(slot, mem_word);
            }
        }
    }

    /// Copies a memory dynamic array to a storage dynamic array at `slot`:
    /// writes the length, then each element at `keccak256(slot) + i *
    /// elem_slots`.
    pub(super) fn copy_memory_dyn_array_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        mem_ptr: ValueId,
        elem: Ty<'gcx>,
    ) {
        let len = builder.memory_object_len(mem_ptr, MemoryObjectKind::DynamicArray);
        builder.sstore(slot, len);
        let elem_slots = self.calculate_storage_slots_for_ty(elem, Span::DUMMY);
        let elem = elem.peel_refs();
        self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
            let mem_word = load_memory_object_word(
                builder,
                MemoryObjectAccess::Element {
                    object: mem_ptr,
                    layout: MemoryObjectLayout::WORD_ARRAY,
                    index,
                },
            );
            let elem_slot = builder.storage_array_element_slot(slot, index, elem_slots);
            this.copy_memory_field_to_storage(builder, elem, elem_slot, mem_word);
        });
    }

    /// Copies a memory fixed-size array to consecutive storage slots.
    pub(super) fn copy_memory_fixed_array_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        mem_ptr: ValueId,
        elem: Ty<'gcx>,
        len: u64,
    ) {
        let packed = self
            .packed_storage_encoding(elem)
            .filter(|(size, _)| *size < StorageLocation::WORD_SIZE);
        let elem_slots =
            packed.map_or_else(|| self.calculate_storage_slots_for_ty(elem, Span::DUMMY), |_| 0);
        let elem = elem.peel_refs();
        for i in 0..len {
            let index = builder.imm_u64(i);
            let mem_word = load_memory_object_word(
                builder,
                MemoryObjectAccess::Element {
                    object: mem_ptr,
                    layout: MemoryObjectLayout::word_fixed_array(len),
                    index,
                },
            );
            if let Some((size, encoding)) = packed {
                let location = Self::packed_array_location(size, encoding, i);
                self.store_storage_location_at_slot(builder, location, slot, mem_word);
            } else {
                let elem_slot = if i * elem_slots == 0 {
                    slot
                } else {
                    let off = builder.imm_u64(i * elem_slots);
                    builder.add(slot, off)
                };
                self.copy_memory_field_to_storage(builder, elem, elem_slot, mem_word);
            }
        }
    }
}

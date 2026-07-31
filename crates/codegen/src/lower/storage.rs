use super::Lowerer;
use crate::mir::{
    FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, PackedStorageField, StorageCursor,
    StorageField, StorageLayout, StorageLayoutRef, ValueId,
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
    pub(super) field: Option<PackedStorageField>,
}

impl StorageLocation {
    const fn full_word(slot: U256) -> Self {
        Self { slot, offset: 0, field: None }
    }
}

/// A storage location whose slot and byte offset are runtime MIR values.
#[derive(Clone, Copy)]
pub(super) struct StorageAccess {
    pub(super) slot: ValueId,
    pub(super) offset: ValueId,
    pub(super) field: Option<PackedStorageField>,
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
                let mut cursor = StorageCursor::default();
                for (index, field_ty) in field_tys.into_iter().enumerate() {
                    let memory = builder.memory_object_field_addr(
                        value,
                        MemoryObjectLayout::structure(fields),
                        index as u64,
                    );
                    let field_value = builder.mload(memory);
                    let field = self.storage_field_for_ty(field_ty);
                    let position = cursor.allocate(&field);
                    let field_slot = self.offset_storage_slot(builder, slot, position.slot);
                    if let StorageField::Packed(field) = field {
                        self.store_storage_location_at_slot(
                            builder,
                            StorageLocation {
                                slot: U256::ZERO,
                                offset: position.offset,
                                field: Some(field),
                            },
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
                let element = self.storage_field_for_ty(element_ty);
                if let StorageField::Packed(field) = element {
                    let per_slot = u64::from(32 / field.size);
                    for slot_offset in 0..len.div_ceil(per_slot) {
                        let element_slot = self.offset_storage_slot(builder, slot, slot_offset);
                        let zero = builder.imm_u64(0);
                        builder.sstore(element_slot, zero);
                    }
                }
                let mut cursor = StorageCursor::default();
                for index in 0..len {
                    let index_value = builder.imm_u64(index);
                    let memory = builder.memory_object_element_addr(
                        value,
                        MemoryObjectLayout::word_fixed_array(len),
                        index_value,
                    );
                    let element_value = builder.mload(memory);
                    let position = cursor.allocate(&element);
                    let element_slot = self.offset_storage_slot(builder, slot, position.slot);
                    if let StorageField::Packed(field) = element {
                        self.store_storage_location_at_slot(
                            builder,
                            StorageLocation {
                                slot: U256::ZERO,
                                offset: position.offset,
                                field: Some(field),
                            },
                            element_slot,
                            element_value,
                        );
                    } else {
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
            _ => {
                if let Some(field) = self.packed_storage_field(ty) {
                    self.store_storage_field_at_slot(builder, field, slot, value);
                } else {
                    builder.sstore(slot, value);
                }
            }
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
                let mut cursor = StorageCursor::default();
                let mut cleared_packed_slot = None;
                for field_ty in field_tys {
                    let field = self.storage_field_for_ty(field_ty);
                    let position = cursor.allocate(&field);
                    let field_slot = self.offset_storage_slot(builder, slot, position.slot);
                    if matches!(field, StorageField::Packed(_)) {
                        if cleared_packed_slot != Some(position.slot) {
                            let zero = builder.imm_u64(0);
                            builder.sstore(field_slot, zero);
                            cleared_packed_slot = Some(position.slot);
                        }
                    } else {
                        self.clear_storage_value_at(builder, field_ty, field_slot);
                    }
                }
            }
            TyKind::Array(element_ty, len) => {
                if matches!(element_ty.peel_refs().kind, TyKind::Mapping(..)) {
                    return;
                }
                let Ok(len) = u64::try_from(len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                let element = self.storage_field_for_ty(element_ty);
                if let StorageField::Packed(field) = element {
                    let per_slot = u64::from(32 / field.size);
                    for slot_offset in 0..len.div_ceil(per_slot) {
                        let element_slot = self.offset_storage_slot(builder, slot, slot_offset);
                        let zero = builder.imm_u64(0);
                        builder.sstore(element_slot, zero);
                    }
                    return;
                }
                let mut cursor = StorageCursor::default();
                for _ in 0..len {
                    let position = cursor.allocate(&element);
                    let element_slot = self.offset_storage_slot(builder, slot, position.slot);
                    if let StorageField::Packed(field) = element {
                        let zero = builder.imm_u64(0);
                        self.store_storage_location_at_slot(
                            builder,
                            StorageLocation {
                                slot: U256::ZERO,
                                offset: position.offset,
                                field: Some(field),
                            },
                            element_slot,
                            zero,
                        );
                    } else {
                        self.clear_storage_value_at(builder, element_ty, element_slot);
                    }
                }
            }
            TyKind::DynArray(element_ty) => {
                let old_len = builder.sload(slot);
                let zero = builder.imm_u64(0);
                builder.sstore(slot, zero);
                if matches!(element_ty.peel_refs().kind, TyKind::Mapping(..)) {
                    return;
                }
                builder.mstore(zero, slot);
                let word = builder.imm_u64(32);
                let data_slot = builder.keccak256(zero, word);
                let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                let element_ty = element_ty.peel_refs();
                if let Some(field) = self.packed_storage_field(element_ty) {
                    let old_slots = Self::packed_array_slot_count(builder, old_len, field);
                    self.emit_storage_elements_loop(
                        builder,
                        zero,
                        old_slots,
                        move |_, builder, index| {
                            let element_slot = builder.add(data_slot, index);
                            let zero = builder.imm_u64(0);
                            builder.sstore(element_slot, zero);
                        },
                    );
                    return;
                }
                self.emit_storage_elements_loop(
                    builder,
                    zero,
                    old_len,
                    move |this, builder, index| {
                        let access = this.storage_array_element_access_unchecked(
                            builder,
                            data_slot,
                            index,
                            element_ty,
                            element_slots,
                        );
                        if access.field.is_some() {
                            let zero = builder.imm_u64(0);
                            this.store_storage_access(builder, access, zero);
                        } else {
                            this.clear_storage_value_at(builder, element_ty, access.slot);
                        }
                    },
                );
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                let empty = self.allocate_memory_object(builder, 32, MemoryObjectKind::Bytes);
                let zero = builder.imm_u64(0);
                builder.set_memory_object_len(empty, zero, MemoryObjectKind::Bytes);
                self.copy_memory_bytes_to_storage(builder, slot, empty);
            }
            TyKind::Mapping(..) => {}
            TyKind::Err(_) => {}
            _ if ty.is_value_type() => {
                let zero = builder.imm_u64(0);
                if let Some(field) = self.packed_storage_field(ty) {
                    self.store_storage_field_at_slot(builder, field, slot, zero);
                } else {
                    builder.sstore(slot, zero);
                }
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
        if offset == 0 {
            slot
        } else {
            let offset = builder.imm_u64(offset);
            builder.add(slot, offset)
        }
    }

    /// Allocates the storage location for a state variable.
    pub(super) fn allocate_storage_location(
        &mut self,
        ty: Ty<'gcx>,
        span: Span,
    ) -> StorageLocation {
        if let Some(field) = self.packed_storage_field(ty) {
            if self.next_storage_offset + field.size > 32 {
                self.advance_next_storage_slot(span);
                self.next_storage_offset = 0;
            }
            let location = StorageLocation {
                slot: self.next_storage_slot,
                offset: self.next_storage_offset,
                field: Some(field),
            };
            self.next_storage_offset += field.size;
            if self.next_storage_offset == 32 {
                self.advance_next_storage_slot(span);
                self.next_storage_offset = 0;
            }
            return location;
        }

        if self.next_storage_offset != 0 {
            self.advance_next_storage_slot(span);
            self.next_storage_offset = 0;
        }

        let slot = self.next_storage_slot;
        let num_slots = self.calculate_storage_slots_for_ty_u256(ty, span);
        // Storage slots span the EVM's full 2^256 space; a layout that walks
        // past its end wraps back onto earlier variables, so reject it.
        match self.next_storage_slot.checked_add(num_slots) {
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

    fn advance_next_storage_slot(&mut self, span: Span) {
        if let Some(next) = self.next_storage_slot.checked_add(U256::from(1)) {
            self.next_storage_slot = next;
        } else {
            self.storage_layout_overflow(span);
        }
    }

    /// Returns the packed storage representation of a sub-word scalar.
    pub(super) fn packed_storage_field(&self, ty: Ty<'gcx>) -> Option<PackedStorageField> {
        match ty.peel_refs().kind {
            TyKind::Elementary(elem) => match elem {
                ElementaryType::Bool => Some(PackedStorageField::new(1, false, false)),
                ElementaryType::Address(_) => Some(PackedStorageField::new(20, false, false)),
                ElementaryType::Int(size) | ElementaryType::Fixed(size, _) => {
                    let size = size.bytes();
                    (size < 32).then(|| PackedStorageField::new(size, false, true))
                }
                ElementaryType::UInt(size) | ElementaryType::UFixed(size, _) => {
                    let size = size.bytes();
                    (size < 32).then(|| PackedStorageField::new(size, false, false))
                }
                ElementaryType::FixedBytes(size) => {
                    let size = size.bytes();
                    (size < 32).then(|| PackedStorageField::new(size, true, false))
                }
                ElementaryType::String | ElementaryType::Bytes => None,
            },
            TyKind::Contract(_) => Some(PackedStorageField::new(20, false, false)),
            TyKind::Enum(_) => Some(PackedStorageField::new(1, false, false)),
            TyKind::Udvt(inner, _) => self.packed_storage_field(inner),
            TyKind::Fn(function) => Some(PackedStorageField::new(
                if function.is_external() { 24 } else { 8 },
                false,
                false,
            )),
            _ => None,
        }
    }

    /// Calculates the number of storage slots needed for a type.
    pub(super) fn calculate_storage_slots_for_ty(&mut self, ty: Ty<'gcx>, span: Span) -> u64 {
        let slots = self.calculate_storage_slots_for_ty_u256(ty, span);
        match u64::try_from(slots) {
            Ok(slots) => slots,
            Err(_) => {
                self.gcx.dcx().err("storage layout is too large for codegen").span(span).emit();
                1
            }
        }
    }

    fn calculate_storage_slots_for_ty_u256(&mut self, ty: Ty<'gcx>, span: Span) -> U256 {
        let ty = ty.peel_refs();
        let slots = match ty.kind {
            TyKind::Struct(struct_id) => {
                let mut slots = U256::ZERO;
                let mut offset = 0u8;
                for &field_ty in self.gcx.struct_field_types(struct_id) {
                    if let Some(field) = self.packed_storage_field(field_ty) {
                        if offset + field.size > 32 {
                            let Some(next) = slots.checked_add(U256::from(1)) else {
                                return self.storage_layout_overflow(span);
                            };
                            slots = next;
                            offset = 0;
                        }
                        offset += field.size;
                        if offset == 32 {
                            let Some(next) = slots.checked_add(U256::from(1)) else {
                                return self.storage_layout_overflow(span);
                            };
                            slots = next;
                            offset = 0;
                        }
                    } else {
                        if offset != 0 {
                            let Some(next) = slots.checked_add(U256::from(1)) else {
                                return self.storage_layout_overflow(span);
                            };
                            slots = next;
                            offset = 0;
                        }
                        let field_slots = self.calculate_storage_slots_for_ty_u256(field_ty, span);
                        let Some(next) = slots.checked_add(field_slots) else {
                            return self.storage_layout_overflow(span);
                        };
                        slots = next;
                    }
                }
                if offset != 0 {
                    let Some(next) = slots.checked_add(U256::from(1)) else {
                        return self.storage_layout_overflow(span);
                    };
                    slots = next;
                }
                slots
            }
            TyKind::Array(element, len) => {
                if let Some(field) = self.packed_storage_field(element) {
                    let per_slot = U256::from(32 / field.size);
                    len / per_slot + U256::from(u8::from(len % per_slot != U256::ZERO))
                } else {
                    let element_slots = self.calculate_storage_slots_for_ty_u256(element, span);
                    let Some(slots) = len.checked_mul(element_slots) else {
                        return self.storage_layout_overflow(span);
                    };
                    slots
                }
            }
            _ => U256::from(1),
        };
        slots.max(U256::from(1))
    }

    fn storage_layout_overflow(&self, span: Span) -> U256 {
        self.gcx
            .dcx()
            .err("contract storage layout exceeds the addressable storage space")
            .span(span)
            .emit();
        U256::ZERO
    }

    pub(super) fn load_storage_location_at_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
    ) -> ValueId {
        let word = builder.sload(slot);
        let Some(field) = location.field else { return word };
        let value = if location.offset == 0 {
            word
        } else {
            let shift = builder.imm_u64(u64::from(location.offset) * 8);
            builder.shr(shift, word)
        };
        let mask = builder.imm_u256(Self::packed_storage_mask(field.size));
        let value = builder.and(value, mask);
        if field.left_aligned {
            let shift = builder.imm_u64(u64::from(32 - field.size) * 8);
            builder.shl(shift, value)
        } else if field.signed {
            let size = builder.imm_u64(u64::from(field.size - 1));
            builder.signextend(size, value)
        } else {
            value
        }
    }

    pub(super) fn store_storage_location(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        value: ValueId,
    ) {
        let slot = builder.imm_u256(location.slot);
        self.store_storage_location_at_slot(builder, location, slot, value);
    }

    pub(super) fn store_storage_location_at_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
        value: ValueId,
    ) {
        let Some(field) = location.field else {
            builder.sstore(slot, value);
            return;
        };
        let value = if field.left_aligned {
            let shift = builder.imm_u64(u64::from(32 - field.size) * 8);
            builder.shr(shift, value)
        } else {
            value
        };
        let shift_bits = usize::from(location.offset) * 8;
        let field_mask = Self::packed_storage_mask(field.size);
        let shifted_mask = field_mask << shift_bits;
        let keep_mask = builder.imm_u256(!shifted_mask);
        let value_mask = builder.imm_u256(field_mask);

        let word = builder.sload(slot);
        let cleared = builder.and(word, keep_mask);
        let masked = builder.and(value, value_mask);
        let shifted = if location.offset == 0 {
            masked
        } else {
            let shift = builder.imm_u64(shift_bits as u64);
            builder.shl(shift, masked)
        };
        let updated = builder.or(cleared, shifted);
        builder.sstore(slot, updated);
    }

    pub(super) fn load_storage_access(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        access: StorageAccess,
    ) -> ValueId {
        let word = builder.sload(access.slot);
        let Some(field) = access.field else { return word };
        let eight = builder.imm_u64(8);
        let shift = builder.mul(access.offset, eight);
        let value = builder.shr(shift, word);
        let mask = builder.imm_u256(Self::packed_storage_mask(field.size));
        let value = builder.and(value, mask);
        if field.left_aligned {
            let shift = builder.imm_u64(u64::from(32 - field.size) * 8);
            builder.shl(shift, value)
        } else if field.signed {
            let size = builder.imm_u64(u64::from(field.size - 1));
            builder.signextend(size, value)
        } else {
            value
        }
    }

    pub(super) fn store_storage_access(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        access: StorageAccess,
        value: ValueId,
    ) {
        let Some(field) = access.field else {
            builder.sstore(access.slot, value);
            return;
        };
        let value = if field.left_aligned {
            let shift = builder.imm_u64(u64::from(32 - field.size) * 8);
            builder.shr(shift, value)
        } else {
            value
        };
        let mask = builder.imm_u256(Self::packed_storage_mask(field.size));
        let value = builder.and(value, mask);
        let eight = builder.imm_u64(8);
        let shift = builder.mul(access.offset, eight);
        let shifted = builder.shl(shift, value);
        let field_mask = builder.shl(shift, mask);
        let keep_mask = builder.not(field_mask);
        let word = builder.sload(access.slot);
        let cleared = builder.and(word, keep_mask);
        let updated = builder.or(cleared, shifted);
        builder.sstore(access.slot, updated);
    }

    pub(super) fn store_storage_field_at_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        field: PackedStorageField,
        slot: ValueId,
        value: ValueId,
    ) {
        let value = if field.left_aligned {
            let shift = builder.imm_u64(u64::from(32 - field.size) * 8);
            builder.shr(shift, value)
        } else {
            value
        };
        let mask = builder.imm_u256(Self::packed_storage_mask(field.size));
        let value = builder.and(value, mask);
        builder.sstore(slot, value);
    }

    fn packed_storage_mask(size: u8) -> U256 {
        if size >= 32 {
            U256::MAX
        } else {
            (U256::from(1) << (usize::from(size) * 8)) - U256::from(1)
        }
    }

    /// Gets the relative storage location of a struct field.
    pub(super) fn get_struct_field_storage_location(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> StorageLocation {
        if let Some(&location) = self.struct_field_offsets.get(&(struct_id, field_index)) {
            return location;
        }

        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let mut cursor = StorageCursor::default();
        for (i, field_ty) in field_tys.into_iter().enumerate() {
            let field = self.storage_field_for_ty(field_ty);
            let position = cursor.allocate(&field);
            if i == field_index {
                let location = StorageLocation {
                    slot: U256::from(position.slot),
                    offset: position.offset,
                    field: match field {
                        StorageField::Packed(field) => Some(field),
                        _ => None,
                    },
                };
                self.struct_field_offsets.insert((struct_id, field_index), location);
                return location;
            }
        }

        StorageLocation::full_word(U256::ZERO)
    }

    /// Gets the relative storage slot of a struct field.
    pub(crate) fn get_struct_field_slot_offset(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> u64 {
        self.get_struct_field_storage_location(struct_id, field_index).slot.to::<u64>()
    }

    /// Calculates the number of 32-byte memory words needed for a value.
    ///
    /// A memory struct has one word per field. Nested structs and other
    /// reference types occupy one pointer word in their parent allocation.
    pub(crate) fn calculate_memory_words_for_ty(&self, ty: Ty<'gcx>) -> u64 {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => self.gcx.struct_field_types(struct_id).len().max(1) as u64,
            _ => 1,
        }
    }

    fn storage_field_for_ty(&mut self, ty: Ty<'gcx>) -> StorageField {
        if let Some(field) = self.packed_storage_field(ty) {
            StorageField::Packed(field)
        } else {
            self.storage_layout_for_ty(ty).map_or(StorageField::Word, StorageField::Aggregate)
        }
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
                Some(self.module.intern_storage_layout(StorageLayout::Array { element, len }))
            }
            _ => None,
        }
    }

    fn storage_layout_for_struct(&mut self, struct_id: hir::StructId) -> StorageLayoutRef {
        if let Some(layout) = self.struct_storage_layouts.get(&struct_id) {
            return Arc::clone(layout);
        }

        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let fields = field_tys
            .into_iter()
            .map(|field_ty| self.storage_field_for_ty(field_ty))
            .collect::<Vec<_>>();
        let layout = self.module.intern_storage_layout(StorageLayout::Struct(fields.into()));
        self.struct_storage_layouts.insert(struct_id, Arc::clone(&layout));
        layout
    }

    /// Recursively copies a struct from storage to memory.
    /// Allocates nested structs separately and stores their pointers in the parent.
    /// Returns the next memory offset after all fields are copied.
    pub(crate) fn copy_storage_to_memory(
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
    pub(crate) fn copy_storage_to_memory_at(
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
        builder.storage_to_memory(Arc::clone(&layout), base_slot, memory);
        mem_offset + layout.memory_words() * 32
    }

    /// Clears every storage slot occupied by a struct at a runtime-computed base slot.
    pub(crate) fn clear_storage_struct_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
    ) {
        let layout = self.storage_layout_for_struct(struct_id);
        builder.clear_storage(layout, base_slot);
    }

    /// Recursively copies a struct from memory to storage.
    /// Follows nested-struct pointers stored in the parent memory allocation.
    /// Returns the next memory offset after all fields are read.
    pub(crate) fn copy_memory_to_storage(
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
    pub(crate) fn copy_memory_to_storage_at(
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
        // Dynamic fields (bytes/string/dynamic arrays) do not fit the flat
        // slot-for-word layout instruction; copy field by field so their
        // storage length and data are written, not the memory pointer word.
        if self.struct_needs_deep_storage_copy(struct_id) {
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
    pub(super) fn struct_needs_deep_storage_copy(&self, struct_id: hir::StructId) -> bool {
        self.gcx.struct_field_types(struct_id).iter().any(|&f| self.ty_needs_deep_storage_copy(f))
    }

    pub(super) fn struct_needs_recursive_storage_clear(&self, struct_id: hir::StructId) -> bool {
        self.gcx
            .struct_field_types(struct_id)
            .iter()
            .any(|&field| self.ty_needs_recursive_storage_clear(field))
    }

    fn ty_needs_recursive_storage_clear(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_)
            | TyKind::Mapping(..)
            | TyKind::Slice(_) => true,
            TyKind::Struct(id) => self.struct_needs_recursive_storage_clear(id),
            TyKind::Array(element, _) => self.ty_needs_recursive_storage_clear(element),
            TyKind::Udvt(inner, _) => self.ty_needs_recursive_storage_clear(inner),
            _ => false,
        }
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
        for (i, &field_ty) in field_tys.iter().enumerate() {
            let location = self.get_struct_field_storage_location(struct_id, i);
            let field_slot = if location.slot == 0 {
                base_slot
            } else {
                let off = builder.imm_u256(location.slot);
                builder.add(base_slot, off)
            };
            let mem_word_addr = if i == 0 {
                mem_base
            } else {
                let off = builder.imm_u64((i as u64) * 32);
                builder.add(mem_base, off)
            };
            let mem_word = builder.mload(mem_word_addr);
            if location.field.is_some() {
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
        let old_len = builder.sload(slot);
        let len = builder.memory_object_len(mem_ptr, MemoryObjectKind::DynamicArray);
        builder.sstore(slot, len);
        let zero = builder.imm_u64(0);
        builder.mstore(zero, slot);
        let word = builder.imm_u64(32);
        let data_slot = builder.keccak256(zero, word);
        let data_ptr = builder.memory_object_data(mem_ptr, MemoryObjectKind::DynamicArray);
        let elem_slots = self.calculate_storage_slots_for_ty(elem, Span::DUMMY);
        let elem = elem.peel_refs();
        let packed = self.packed_storage_field(elem);
        if let Some(field) = packed {
            let old_slots = Self::packed_array_slot_count(builder, old_len, field);
            let new_slots = Self::packed_array_slot_count(builder, len, field);
            let old_is_larger = builder.gt(old_slots, new_slots);
            let slots = builder.select(old_is_larger, old_slots, new_slots);
            self.emit_storage_elements_loop(builder, zero, slots, move |_, builder, index| {
                let element_slot = builder.add(data_slot, index);
                let zero = builder.imm_u64(0);
                builder.sstore(element_slot, zero);
            });
        }
        self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
            let mem_off = builder.mul(index, word);
            let mem_word_addr = builder.add(data_ptr, mem_off);
            let mem_word = builder.mload(mem_word_addr);
            let access = this.storage_array_element_access_unchecked(
                builder, data_slot, index, elem, elem_slots,
            );
            if access.field.is_some() {
                this.store_storage_access(builder, access, mem_word);
            } else {
                this.copy_memory_field_to_storage(builder, elem, access.slot, mem_word);
            }
        });
        if packed.is_some() {
            return;
        }
        self.emit_storage_elements_loop(builder, len, old_len, move |this, builder, index| {
            let access = this.storage_array_element_access_unchecked(
                builder, data_slot, index, elem, elem_slots,
            );
            if access.field.is_some() {
                let zero = builder.imm_u64(0);
                this.store_storage_access(builder, access, zero);
            } else {
                this.clear_storage_value_at(builder, elem, access.slot);
            }
        });
    }

    fn emit_storage_elements_loop(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        start: ValueId,
        end: ValueId,
        body: impl FnOnce(&mut Self, &mut FunctionBuilder<'_>, ValueId) + Copy,
    ) {
        let preheader = builder.current_block();
        let header = builder.create_block();
        let body_block = builder.create_block();
        let exit = builder.create_block();
        builder.jump(header);

        builder.switch_to_block(header);
        let index = builder.phi(vec![(preheader, start)]);
        let has_more = builder.lt(index, end);
        builder.branch(has_more, body_block, exit);

        builder.switch_to_block(body_block);
        body(self, builder, index);
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let latch = builder.current_block();
        builder.jump(header);
        builder.add_phi_incoming(index, latch, next);

        builder.switch_to_block(exit);
    }

    fn packed_array_slot_count(
        builder: &mut FunctionBuilder<'_>,
        len: ValueId,
        field: PackedStorageField,
    ) -> ValueId {
        let per_slot = builder.imm_u64(u64::from(32 / field.size));
        let slots = builder.div(len, per_slot);
        let remainder = builder.mod_(len, per_slot);
        let is_exact = builder.iszero(remainder);
        let one = builder.imm_u64(1);
        let rounded = builder.add(slots, one);
        builder.select(is_exact, slots, rounded)
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
        let elem = elem.peel_refs();
        let field = self.storage_field_for_ty(elem);
        if let StorageField::Packed(field) = field {
            let per_slot = u64::from(32 / field.size);
            for slot_offset in 0..len.div_ceil(per_slot) {
                let element_slot = self.offset_storage_slot(builder, slot, slot_offset);
                let zero = builder.imm_u64(0);
                builder.sstore(element_slot, zero);
            }
        }
        let mut cursor = StorageCursor::default();
        for i in 0..len {
            let mem_word_addr = if i == 0 {
                mem_ptr
            } else {
                let off = builder.imm_u64(i * 32);
                builder.add(mem_ptr, off)
            };
            let mem_word = builder.mload(mem_word_addr);
            let position = cursor.allocate(&field);
            let elem_slot = if position.slot == 0 {
                slot
            } else {
                let off = builder.imm_u64(position.slot);
                builder.add(slot, off)
            };
            if let StorageField::Packed(field) = field {
                self.store_storage_location_at_slot(
                    builder,
                    StorageLocation {
                        slot: U256::ZERO,
                        offset: position.offset,
                        field: Some(field),
                    },
                    elem_slot,
                    mem_word,
                );
            } else {
                self.copy_memory_field_to_storage(builder, elem, elem_slot, mem_word);
            }
        }
    }
}

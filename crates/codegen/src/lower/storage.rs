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
    pub(super) slot: u64,
    pub(super) offset: u8,
    pub(super) field: Option<PackedStorageField>,
}

impl StorageLocation {
    const fn full_word(slot: u64) -> Self {
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
                                slot: 0,
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
                                slot: 0,
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
            TyKind::DynArray(_) => {
                self.gcx
                    .dcx()
                    .err("codegen does not support pushing a dynamic array value yet")
                    .emit();
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
                for field_ty in field_tys {
                    let field = self.storage_field_for_ty(field_ty);
                    let position = cursor.allocate(&field);
                    let field_slot = self.offset_storage_slot(builder, slot, position.slot);
                    if let StorageField::Packed(field) = field {
                        let zero = builder.imm_u64(0);
                        self.store_storage_location_at_slot(
                            builder,
                            StorageLocation {
                                slot: 0,
                                offset: position.offset,
                                field: Some(field),
                            },
                            field_slot,
                            zero,
                        );
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
                let element = self.storage_field_for_ty(element_ty);
                let mut cursor = StorageCursor::default();
                for _ in 0..len {
                    let position = cursor.allocate(&element);
                    let element_slot = self.offset_storage_slot(builder, slot, position.slot);
                    if let StorageField::Packed(field) = element {
                        let zero = builder.imm_u64(0);
                        self.store_storage_location_at_slot(
                            builder,
                            StorageLocation {
                                slot: 0,
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
                self.next_storage_slot += 1;
                self.next_storage_offset = 0;
            }
            let location = StorageLocation {
                slot: self.next_storage_slot,
                offset: self.next_storage_offset,
                field: Some(field),
            };
            self.next_storage_offset += field.size;
            if self.next_storage_offset == 32 {
                self.next_storage_slot += 1;
                self.next_storage_offset = 0;
            }
            return location;
        }

        if self.next_storage_offset != 0 {
            self.next_storage_slot += 1;
            self.next_storage_offset = 0;
        }

        let slot = self.next_storage_slot;
        let num_slots = self.calculate_storage_slots_for_ty(ty, span);
        self.next_storage_slot += num_slots;
        StorageLocation::full_word(slot)
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
                function.is_external(),
                false,
            )),
            _ => None,
        }
    }

    /// Calculates the number of storage slots needed for a type.
    pub(super) fn calculate_storage_slots_for_ty(&mut self, ty: Ty<'gcx>, _span: Span) -> u64 {
        self.storage_layout_for_ty(ty).map_or(1, |layout| layout.storage_slots())
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
        let slot = builder.imm_u64(location.slot);
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
                    slot: position.slot,
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

        StorageLocation::full_word(0)
    }

    /// Gets the relative storage slot of a struct field.
    pub(crate) fn get_struct_field_slot_offset(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> u64 {
        self.get_struct_field_storage_location(struct_id, field_index).slot
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
        base_slot: u64,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let base_slot = builder.imm_u64(base_slot);
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
        base_slot: u64,
        mem_ptr: ValueId,
        mem_offset: u64,
    ) -> u64 {
        let base_slot = builder.imm_u64(base_slot);
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
        let layout = self.storage_layout_for_struct(struct_id);
        let memory = if mem_offset == 0 {
            mem_ptr
        } else {
            let offset = builder.imm_u64(mem_offset);
            builder.add(mem_ptr, offset)
        };
        builder.memory_to_storage(Arc::clone(&layout), memory, base_slot);
        mem_offset + layout.memory_words() * 32
    }
}

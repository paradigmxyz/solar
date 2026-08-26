use super::Lowerer;
use crate::mir::{
    FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, PackedKind, PackedValue, StorageField,
    StorageLayout, StorageLayoutRef, StructField, ValueId,
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
    /// Packed shape when the variable shares its slot; `None` for values that
    /// occupy whole slots.
    pub(super) packed: Option<PackedValue>,
}

impl StorageLocation {
    const fn full_word(slot: U256) -> Self {
        Self { slot, offset: 0, packed: None }
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Stores one Solidity value at a runtime-computed storage slot.
    ///
    /// The slot is the value's base slot; packed struct fields and array
    /// elements within aggregates are placed by their layout.
    pub(super) fn store_storage_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slot: ValueId,
        value: ValueId,
    ) {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => {
                self.copy_memory_to_storage_at(builder, struct_id, slot, value, 0);
            }
            TyKind::Array(element_ty, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                if self.packed_value_of_ty(element_ty).is_some()
                    && let Some(layout) = self.storage_layout_for_ty(ty)
                {
                    builder.memory_to_storage(layout, value, slot);
                    return;
                }
                let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                for index in 0..len {
                    let index_value = builder.imm_u64(index);
                    let memory = builder.memory_object_element_addr(
                        value,
                        MemoryObjectLayout::word_fixed_array(len),
                        index_value,
                    );
                    let element_value = builder.mload(memory);
                    let storage_offset = index.saturating_mul(element_slots);
                    let element_slot = self.offset_storage_slot(builder, slot, storage_offset);
                    self.store_storage_value_at(builder, element_ty, element_slot, element_value);
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
                // A narrow scalar that owns its slot stores solc's low-aligned
                // masked form.
                let value = match self.packed_value_of_ty(ty) {
                    Some(packed) => builder.prepare_packed(value, packed),
                    None => value,
                };
                builder.sstore(slot, value);
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
                let layout = self.storage_layout_for_struct(struct_id);
                let StorageLayout::Struct(fields) = layout.as_ref() else { unreachable!() };
                // Packed neighbors share a slot: one zero write clears them all.
                let mut cleared_slot = None;
                for (field, field_ty) in fields.iter().zip(field_tys) {
                    match field.shape {
                        StorageField::Packed(_) => {
                            if cleared_slot == Some(field.slot) {
                                continue;
                            }
                            cleared_slot = Some(field.slot);
                            let field_slot = self.offset_storage_slot(builder, slot, field.slot);
                            let zero = builder.imm_u64(0);
                            builder.sstore(field_slot, zero);
                        }
                        StorageField::Word | StorageField::Aggregate(_) => {
                            let field_slot = self.offset_storage_slot(builder, slot, field.slot);
                            self.clear_storage_value_at(builder, field_ty, field_slot);
                        }
                    }
                }
            }
            TyKind::Array(element_ty, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                if self.packed_value_of_ty(element_ty).is_some()
                    && let Some(layout) = self.storage_layout_for_ty(ty)
                {
                    builder.clear_storage(layout, slot);
                    return;
                }
                let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                for index in 0..len {
                    let storage_offset = index.saturating_mul(element_slots);
                    let element_slot = self.offset_storage_slot(builder, slot, storage_offset);
                    self.clear_storage_value_at(builder, element_ty, element_slot);
                }
            }
            TyKind::DynArray(element_ty) => {
                self.clear_dynamic_storage_array(builder, slot, element_ty);
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
                builder.sstore(slot, zero);
            }
            _ => {
                self.gcx.dcx().err("codegen cannot clear this storage value").emit();
            }
        }
    }

    /// Clears every element owned by a dynamic storage array, then its length.
    fn clear_dynamic_storage_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        element_ty: Ty<'gcx>,
    ) {
        let len = builder.sload(slot);
        let zero = builder.imm_u64(0);
        let word = builder.imm_u64(32);
        builder.mstore(zero, slot);
        let data = builder.keccak256(zero, word);
        let packed = self.packed_value_of_ty(element_ty);

        let entry = builder.current_block();
        let head = builder.create_block();
        let body = builder.create_block();
        let exit = builder.create_block();
        builder.jump(head);

        builder.switch_to_block(head);
        let index = builder.phi(vec![(entry, zero)]);
        let limit = if let Some(packed) = packed {
            let per_slot = u64::from(packed.per_slot());
            let divisor = builder.imm_u64(per_slot);
            let full_slots = builder.div(len, divisor);
            let remainder = builder.mod_(len, divisor);
            let no_tail = builder.iszero(remainder);
            let has_tail = builder.iszero(no_tail);
            builder.add(full_slots, has_tail)
        } else {
            len
        };
        let more = builder.lt(index, limit);
        builder.branch(more, body, exit);

        builder.switch_to_block(body);
        let element_slot = if packed.is_some() {
            builder.add(data, index)
        } else {
            let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
            let offset = if element_slots == 1 {
                index
            } else {
                let stride = builder.imm_u64(element_slots);
                builder.mul(index, stride)
            };
            builder.add(data, offset)
        };
        if packed.is_some() {
            builder.sstore(element_slot, zero);
        } else {
            self.clear_storage_value_at(builder, element_ty, element_slot);
        }
        let one = builder.imm_u64(1);
        let next = builder.add(index, one);
        let latch = builder.current_block();
        builder.add_phi_incoming(index, latch, next);
        builder.jump(head);

        builder.switch_to_block(exit);
        builder.sstore(slot, zero);
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

    /// Allocates the storage location for a state variable, packing value
    /// types by solc's rules.
    pub(super) fn allocate_storage_location(
        &mut self,
        ty: Ty<'gcx>,
        span: Span,
    ) -> StorageLocation {
        if let Some(value) = self.packed_value_of_ty(ty) {
            let bytes = value.size;
            if self.next_storage_offset + bytes > 32 {
                self.next_storage_slot += U256::from(1);
                self.next_storage_offset = 0;
            }
            let location = StorageLocation {
                slot: self.next_storage_slot,
                offset: self.next_storage_offset,
                packed: Some(value),
            };
            self.next_storage_offset += bytes;
            if self.next_storage_offset == 32 {
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

    /// Returns the packed shape of a value type that shares storage slots, or
    /// `None` for types that occupy whole slots. Matches solc's packing rules
    /// and the emitted `storageLayout` artifact.
    pub(crate) fn packed_value_of_ty(&self, ty: Ty<'gcx>) -> Option<PackedValue> {
        let value = |size: u8, kind| Some(PackedValue { size, kind });
        match ty.peel_refs().kind {
            TyKind::Elementary(ty) => match ty {
                ElementaryType::Bool => value(1, PackedKind::Unsigned),
                ElementaryType::Address(_) => value(20, PackedKind::Unsigned),
                ElementaryType::UInt(size) | ElementaryType::UFixed(size, _) => (size.bytes() < 32)
                    .then(|| PackedValue { size: size.bytes(), kind: PackedKind::Unsigned }),
                ElementaryType::Int(size) | ElementaryType::Fixed(size, _) => (size.bytes() < 32)
                    .then(|| PackedValue { size: size.bytes(), kind: PackedKind::Signed }),
                ElementaryType::FixedBytes(size) => (size.bytes() < 32)
                    .then(|| PackedValue { size: size.bytes(), kind: PackedKind::HighAligned }),
                ElementaryType::Bytes | ElementaryType::String => None,
            },
            TyKind::Contract(_) => value(20, PackedKind::Unsigned),
            TyKind::Enum(_) => value(1, PackedKind::Unsigned),
            TyKind::Udvt(inner, _) => self.packed_value_of_ty(inner),
            TyKind::Fn(f) => {
                if f.is_external() {
                    value(24, PackedKind::Unsigned)
                } else {
                    value(8, PackedKind::Unsigned)
                }
            }
            _ => None,
        }
    }

    /// Calculates the number of storage slots needed for a type.
    pub(super) fn calculate_storage_slots_for_ty(&mut self, ty: Ty<'gcx>, span: Span) -> u64 {
        match ty.peel_refs().kind {
            TyKind::Struct(_) | TyKind::Array(..) => match self.storage_layout_for_ty(ty) {
                Some(layout) => layout.storage_slots(),
                None => {
                    self.gcx
                        .dcx()
                        .err("storage aggregates this large are not supported")
                        .span(span)
                        .emit();
                    1
                }
            },
            _ => 1,
        }
    }

    pub(super) fn load_storage_location_at_slot(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        slot: ValueId,
    ) -> ValueId {
        let word = builder.sload(slot);
        match location.packed {
            None => word,
            Some(value) => builder.extract_packed(word, value, location.offset),
        }
    }

    pub(super) fn store_storage_location(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        value_id: ValueId,
    ) {
        let slot = builder.imm_u256(location.slot);
        match location.packed {
            None => builder.sstore(slot, value_id),
            Some(value) => builder.store_packed(slot, location.offset, value, value_id),
        }
    }

    /// Gets the placement of a struct field within its aggregate: slot offset,
    /// byte offset, and shape.
    pub(crate) fn struct_field_placement(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> StructField {
        let layout = self.storage_layout_for_struct(struct_id);
        let StorageLayout::Struct(fields) = layout.as_ref() else {
            unreachable!("struct layout is always StorageLayout::Struct")
        };
        fields[field_index].clone()
    }

    /// Loads a struct field value from its resolved slot, unpacking packed
    /// fields into canonical form.
    pub(crate) fn load_struct_field_at(
        &self,
        builder: &mut FunctionBuilder<'_>,
        placement: &StructField,
        slot: ValueId,
    ) -> ValueId {
        match placement.shape {
            StorageField::Packed(value) => builder.load_packed(slot, placement.offset, value),
            _ => builder.sload(slot),
        }
    }

    /// Stores a struct field value at its resolved slot, read-modify-writing
    /// packed fields.
    pub(crate) fn store_struct_field_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        field_ty: Option<Ty<'gcx>>,
        placement: &StructField,
        slot: ValueId,
        value_id: ValueId,
    ) {
        match placement.shape {
            StorageField::Packed(value) => {
                builder.store_packed(slot, placement.offset, value, value_id);
            }
            _ => {
                if let Some(field_ty) = field_ty {
                    self.copy_memory_field_to_storage(builder, field_ty, slot, value_id);
                } else {
                    builder.sstore(slot, value_id);
                }
            }
        }
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
        if let Some(value) = self.packed_value_of_ty(ty) {
            return StorageField::Packed(value);
        }
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
        let mut fields = Vec::with_capacity(field_tys.len());
        let mut slot = 0u64;
        let mut offset = 0u8;
        for field_ty in field_tys {
            if let Some(value) = self.packed_value_of_ty(field_ty) {
                if offset + value.size > 32 {
                    slot += 1;
                    offset = 0;
                }
                fields.push(StructField { slot, offset, shape: StorageField::Packed(value) });
                offset += value.size;
                if offset == 32 {
                    slot += 1;
                    offset = 0;
                }
                continue;
            }
            if offset != 0 {
                slot += 1;
                offset = 0;
            }
            let shape = self.storage_field_for_ty(field_ty);
            let field_slots = shape.storage_slots();
            fields.push(StructField { slot, offset: 0, shape });
            slot = match slot.checked_add(field_slots) {
                Some(slot) => slot,
                None => {
                    self.gcx.dcx().err("storage structs this large are not supported").emit();
                    slot
                }
            };
        }
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
        let memory = if mem_offset == 0 {
            mem_ptr
        } else {
            let offset = builder.imm_u64(mem_offset);
            builder.add(mem_ptr, offset)
        };
        // Dynamic fields (bytes/string/dynamic arrays) do not fit the flat
        // slot-for-word layout instruction; materialize them field by field so
        // their memory objects are rebuilt, not their raw length slots.
        if self.struct_needs_deep_storage_copy(struct_id) {
            self.deep_copy_storage_struct_to_memory(builder, struct_id, base_slot, memory);
            return mem_offset + self.calculate_memory_words_for_ty_struct(struct_id) * 32;
        }
        let layout = self.storage_layout_for_struct(struct_id);
        builder.storage_to_memory(Arc::clone(&layout), base_slot, memory);
        mem_offset + layout.memory_words() * 32
    }

    /// Materializes each field of a storage struct into a memory struct,
    /// rebuilding dynamic fields as fresh memory objects.
    fn deep_copy_storage_struct_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
        mem_base: ValueId,
    ) {
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        for (i, &field_ty) in field_tys.iter().enumerate() {
            let placement = self.struct_field_placement(struct_id, i);
            let field_slot = self.offset_storage_slot(builder, base_slot, placement.slot);
            let value = match field_ty.peel_refs().kind {
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                    self.materialize_storage_bytes(builder, field_slot)
                }
                TyKind::DynArray(_) | TyKind::Array(..) => self
                    .materialize_storage_array_value(builder, field_ty, field_slot, Span::DUMMY)
                    .unwrap_or_else(|| builder.sload(field_slot)),
                TyKind::Struct(inner) => {
                    let words = self.calculate_memory_words_for_ty_struct(inner);
                    let ptr =
                        self.allocate_memory_object(builder, words * 32, MemoryObjectKind::Struct);
                    self.copy_storage_to_memory_at(builder, inner, field_slot, ptr, 0);
                    ptr
                }
                _ => match placement.shape {
                    StorageField::Packed(packed) => {
                        builder.load_packed(field_slot, placement.offset, packed)
                    }
                    _ => builder.sload(field_slot),
                },
            };
            let dest = if i == 0 {
                mem_base
            } else {
                let off = builder.imm_u64((i as u64) * 32);
                builder.add(mem_base, off)
            };
            builder.mstore(dest, value);
        }
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
        for (i, &field_ty) in field_tys.iter().enumerate() {
            let placement = self.struct_field_placement(struct_id, i);
            let field_slot = self.offset_storage_slot(builder, base_slot, placement.slot);
            let mem_word_addr = if i == 0 {
                mem_base
            } else {
                let off = builder.imm_u64((i as u64) * 32);
                builder.add(mem_base, off)
            };
            let mem_word = builder.mload(mem_word_addr);
            if let StorageField::Packed(value) = placement.shape {
                builder.store_packed(field_slot, placement.offset, value, mem_word);
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
    /// for reference fields. The value must occupy its slot alone.
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
    /// writes the length, then each element at its packed position in the data
    /// area at `keccak256(slot)`.
    pub(super) fn copy_memory_dyn_array_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        mem_ptr: ValueId,
        elem: Ty<'gcx>,
    ) {
        let len = builder.memory_object_len(mem_ptr, MemoryObjectKind::DynamicArray);
        builder.sstore(slot, len);
        let zero = builder.imm_u64(0);
        builder.mstore(zero, slot);
        let word = builder.imm_u64(32);
        let data_slot = builder.keccak256(zero, word);
        let data_ptr = builder.memory_object_data(mem_ptr, MemoryObjectKind::DynamicArray);
        let packed_elem = self.packed_value_of_ty(elem);
        let elem_slots = self.calculate_storage_slots_for_ty(elem, Span::DUMMY);
        let elem = elem.peel_refs();
        self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
            let mem_off = builder.mul(index, word);
            let mem_word_addr = builder.add(data_ptr, mem_off);
            let mem_word = builder.mload(mem_word_addr);
            if let Some(value) = packed_elem {
                this.store_packed_array_element(builder, data_slot, index, value, mem_word);
                return;
            }
            let elem_slot = if elem_slots == 1 {
                builder.add(data_slot, index)
            } else {
                let stride = builder.imm_u64(elem_slots);
                let off = builder.mul(index, stride);
                builder.add(data_slot, off)
            };
            this.copy_memory_field_to_storage(builder, elem, elem_slot, mem_word);
        });
    }

    /// Computes the slot and intra-slot bit shift of a packed array element at
    /// a runtime index.
    pub(crate) fn packed_array_element_position(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        data_slot: ValueId,
        index: ValueId,
        value: PackedValue,
    ) -> (ValueId, ValueId) {
        let per_slot = builder.imm_u64(u64::from(value.per_slot()));
        let slot_offset = builder.div(index, per_slot);
        let slot = builder.add(data_slot, slot_offset);
        let within = builder.mod_(index, per_slot);
        let bits = builder.imm_u64(u64::from(value.size) * 8);
        let shift = builder.mul(within, bits);
        (slot, shift)
    }

    /// Loads and canonicalizes a packed array element at a runtime index.
    pub(crate) fn load_packed_array_element(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        data_slot: ValueId,
        index: ValueId,
        value: PackedValue,
    ) -> ValueId {
        let (slot, shift) = self.packed_array_element_position(builder, data_slot, index, value);
        let word = builder.sload(slot);
        let shifted = builder.shr(shift, word);
        builder.canonicalize_packed(shifted, value)
    }

    /// Read-modify-writes a packed array element at a runtime index.
    pub(crate) fn store_packed_array_element(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        data_slot: ValueId,
        index: ValueId,
        value: PackedValue,
        element: ValueId,
    ) {
        let (slot, shift) = self.packed_array_element_position(builder, data_slot, index, value);
        let word = builder.sload(slot);
        let mask = builder.imm_u256(value.mask());
        let shifted_mask = builder.shl(shift, mask);
        let keep_mask = builder.not(shifted_mask);
        let cleared = builder.and(word, keep_mask);
        let prepared = builder.prepare_packed(element, value);
        let shifted_value = builder.shl(shift, prepared);
        let combined = builder.or(cleared, shifted_value);
        builder.sstore(slot, combined);
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
        if self.packed_value_of_ty(elem).is_some() {
            let element = self.storage_field_for_ty(elem);
            let layout = self.module.intern_storage_layout(StorageLayout::Array { element, len });
            builder.memory_to_storage(layout, mem_ptr, slot);
            return;
        }
        let elem_slots = self.calculate_storage_slots_for_ty(elem, Span::DUMMY);
        let elem = elem.peel_refs();
        for i in 0..len {
            let mem_word_addr = if i == 0 {
                mem_ptr
            } else {
                let off = builder.imm_u64(i * 32);
                builder.add(mem_ptr, off)
            };
            let mem_word = builder.mload(mem_word_addr);
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

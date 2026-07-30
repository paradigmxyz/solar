use super::{Lowerer, checked_arith::PanicCode};
use crate::mir::{
    FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, StorageField, StorageLayout,
    StorageLayoutRef, ValueId,
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
    pub(super) size: u8,
}

impl StorageLocation {
    const WORD_SIZE: u8 = 32;

    const fn full_word(slot: U256) -> Self {
        Self { slot, offset: 0, size: Self::WORD_SIZE }
    }

    const fn is_packed(self) -> bool {
        self.offset != 0 || self.size != Self::WORD_SIZE
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Materializes a storage-reference expression into its memory value.
    pub(super) fn materialize_storage_value_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<(ValueId, Ty<'gcx>)> {
        let ty = self.get_expr_type(expr)?;
        let TyKind::Ref(_, solar_ast::DataLocation::Storage) = ty.kind else {
            return None;
        };
        let slot = self.lower_storage_reference_expr(
            builder,
            expr,
            "unsupported storage value expression",
        );
        let value_ty = ty.peel_refs();
        let value = self.load_storage_value_at(builder, value_ty, slot);
        Some((value, value_ty))
    }

    /// Loads one Solidity value from a runtime-computed storage slot,
    /// recursively materializing reference values into memory.
    pub(super) fn load_storage_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slot: ValueId,
    ) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => {
                let fields = self.gcx.struct_field_types(struct_id).len().max(1) as u64;
                let ptr =
                    self.allocate_memory_object(builder, fields * 32, MemoryObjectKind::Struct);
                self.copy_storage_to_memory_at(builder, struct_id, slot, ptr, 0);
                ptr
            }
            TyKind::Array(element_ty, len) => {
                let (len, size) = if let Ok(len) = u64::try_from(len)
                    && let Some(size) = len.checked_mul(32)
                {
                    (len, size)
                } else {
                    return self.err_value(
                        builder,
                        Span::DUMMY,
                        "fixed-size storage array is too large to materialize",
                    );
                };
                let ptr = self.allocate_memory(builder, size);
                let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                for index in 0..len {
                    let element_slot =
                        self.offset_storage_slot(builder, slot, index * element_slots);
                    let value = self.load_storage_value_at(builder, element_ty, element_slot);
                    let address = self.offset_ptr(builder, ptr, index * 32);
                    builder.mstore(address, value);
                }
                ptr
            }
            TyKind::DynArray(element_ty) => self.load_storage_dyn_array(builder, slot, element_ty),
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.materialize_storage_bytes(builder, slot)
            }
            TyKind::Mapping(..) => slot,
            _ => builder.sload(slot),
        }
    }

    fn load_storage_dyn_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        element_ty: Ty<'gcx>,
    ) -> ValueId {
        let len = builder.sload(slot);
        let shift = builder.imm_u64(251);
        let too_big = builder.shr(shift, len);
        self.emit_panic_if(builder, too_big, PanicCode::MemoryAllocationOverflow);
        let word = builder.imm_u64(32);
        let byte_len = builder.mul(len, word);
        let total = builder.add(byte_len, word);
        let overflow = builder.lt(total, byte_len);
        self.emit_panic_if(builder, overflow, PanicCode::MemoryAllocationOverflow);

        let ptr =
            self.allocate_memory_object_dynamic(builder, total, MemoryObjectKind::DynamicArray);
        builder.set_memory_object_len(ptr, len, MemoryObjectKind::DynamicArray);
        let data = builder.memory_object_data(ptr, MemoryObjectKind::DynamicArray);

        let scratch = builder.imm_u64(0);
        builder.mstore(scratch, slot);
        let data_slot = builder.keccak256(scratch, word);
        let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
        self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
            let element_slot = if element_slots == 1 {
                builder.add(data_slot, index)
            } else {
                let stride = builder.imm_u64(element_slots);
                let offset = builder.mul(index, stride);
                builder.add(data_slot, offset)
            };
            let value = this.load_storage_value_at(builder, element_ty, element_slot);
            let memory_offset = builder.mul(index, word);
            let address = builder.add(data, memory_offset);
            builder.mstore(address, value);
        });
        ptr
    }

    /// Stores one Solidity value at a runtime-computed storage slot.
    pub(super) fn store_storage_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slot: ValueId,
        value: ValueId,
    ) {
        self.store_storage_value_from_memory_at(builder, ty, ty, slot, value);
    }

    /// Stores a memory value at a runtime-computed storage slot, preserving
    /// source array lengths when Solidity permits a shorter fixed array to be
    /// copied into a larger fixed or dynamic storage array.
    pub(super) fn store_storage_value_from_memory_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        destination_ty: Ty<'gcx>,
        source_ty: Ty<'gcx>,
        slot: ValueId,
        value: ValueId,
    ) {
        let destination_ty = destination_ty.peel_refs();
        let source_ty = source_ty.peel_refs();
        match destination_ty.kind {
            TyKind::Struct(struct_id) => {
                let destination_fields = self.gcx.struct_field_types(struct_id).to_vec();
                let source_fields = match source_ty.kind {
                    TyKind::Struct(source_id) => self.gcx.struct_field_types(source_id).to_vec(),
                    _ => destination_fields.clone(),
                };
                let fields = destination_fields.len() as u64;
                let mut storage_offset = 0;
                for (index, destination_field) in destination_fields.into_iter().enumerate() {
                    let memory = builder.memory_object_field_addr(
                        value,
                        MemoryObjectLayout::structure(fields),
                        index as u64,
                    );
                    let field_value = builder.mload(memory);
                    let field_slot = self.offset_storage_slot(builder, slot, storage_offset);
                    let source_field =
                        source_fields.get(index).copied().unwrap_or(destination_field);
                    self.store_storage_value_from_memory_at(
                        builder,
                        destination_field,
                        source_field,
                        field_slot,
                        field_value,
                    );
                    storage_offset +=
                        self.calculate_storage_slots_for_ty(destination_field, Span::DUMMY);
                }
            }
            TyKind::Array(destination_element, destination_len) => {
                let Ok(destination_len) = u64::try_from(destination_len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                let (source_element, source_len) = match source_ty.kind {
                    TyKind::Array(element, len) => {
                        let Ok(len) = u64::try_from(len) else {
                            self.gcx
                                .dcx()
                                .err("fixed-size memory array is too large for codegen")
                                .emit();
                            return;
                        };
                        (element, len)
                    }
                    _ => (destination_element, destination_len),
                };
                self.copy_memory_fixed_array_to_storage(
                    builder,
                    slot,
                    value,
                    source_element,
                    source_len,
                    destination_element,
                    destination_len,
                );
            }
            TyKind::DynArray(destination_element) => match source_ty.kind {
                TyKind::Array(source_element, source_len) => {
                    let Ok(source_len) = u64::try_from(source_len) else {
                        self.gcx
                            .dcx()
                            .err("fixed-size memory array is too large for codegen")
                            .emit();
                        return;
                    };
                    self.copy_memory_array_to_dynamic_storage(
                        builder,
                        slot,
                        value,
                        source_element,
                        Some(source_len),
                        destination_element,
                    );
                }
                TyKind::DynArray(source_element) => self.copy_memory_array_to_dynamic_storage(
                    builder,
                    slot,
                    value,
                    source_element,
                    None,
                    destination_element,
                ),
                _ => {
                    self.copy_memory_dyn_array_to_storage(builder, slot, value, destination_element)
                }
            },
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
                let mut storage_offset = 0;
                for field_ty in field_tys {
                    let field_slot = self.offset_storage_slot(builder, slot, storage_offset);
                    self.clear_storage_value_at(builder, field_ty, field_slot);
                    storage_offset += self.calculate_storage_slots_for_ty(field_ty, Span::DUMMY);
                }
            }
            TyKind::Array(element_ty, len) => {
                let Ok(len) = u64::try_from(len) else {
                    self.gcx.dcx().err("fixed-size storage array is too large for codegen").emit();
                    return;
                };
                let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                for index in 0..len {
                    let storage_offset = index.saturating_mul(element_slots);
                    let element_slot = self.offset_storage_slot(builder, slot, storage_offset);
                    self.clear_storage_value_at(builder, element_ty, element_slot);
                }
            }
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                let empty = self.allocate_memory_object(builder, 32, MemoryObjectKind::Bytes);
                let zero = builder.imm_u64(0);
                builder.set_memory_object_len(empty, zero, MemoryObjectKind::Bytes);
                self.copy_memory_bytes_to_storage(builder, slot, empty);
            }
            TyKind::DynArray(element_ty) => {
                let len = builder.sload(slot);
                let scratch = builder.imm_u64(0);
                builder.mstore(scratch, slot);
                let word = builder.imm_u64(32);
                let data_slot = builder.keccak256(scratch, word);
                let element_slots = self.calculate_storage_slots_for_ty(element_ty, Span::DUMMY);
                self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
                    let element_slot = if element_slots == 1 {
                        builder.add(data_slot, index)
                    } else {
                        let stride = builder.imm_u64(element_slots);
                        let offset = builder.mul(index, stride);
                        builder.add(data_slot, offset)
                    };
                    this.clear_storage_value_at(builder, element_ty, element_slot);
                });
                let zero = builder.imm_u64(0);
                builder.sstore(slot, zero);
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
        if self.storage_exhausted {
            self.emit_storage_layout_overflow(span);
            return StorageLocation::full_word(U256::MAX);
        }

        if let Some(size) = self.packed_storage_size(ty)
            && size < StorageLocation::WORD_SIZE
        {
            if self.next_storage_offset + size > StorageLocation::WORD_SIZE {
                self.advance_storage_cursor(1, span);
                self.next_storage_offset = 0;
                if self.storage_exhausted {
                    self.emit_storage_layout_overflow(span);
                    return StorageLocation::full_word(U256::MAX);
                }
            }
            let location = StorageLocation {
                slot: self.next_storage_slot,
                offset: self.next_storage_offset,
                size,
            };
            self.next_storage_offset += size;
            if self.next_storage_offset == StorageLocation::WORD_SIZE {
                self.advance_storage_cursor(1, span);
                self.next_storage_offset = 0;
            }
            return location;
        }

        if self.next_storage_offset != 0 {
            self.advance_storage_cursor(1, span);
            self.next_storage_offset = 0;
            if self.storage_exhausted {
                self.emit_storage_layout_overflow(span);
                return StorageLocation::full_word(U256::MAX);
            }
        }

        let slot = self.next_storage_slot;
        let num_slots = self.calculate_storage_slots_for_ty(ty, span);
        self.advance_storage_cursor(num_slots, span);
        StorageLocation::full_word(slot)
    }

    fn advance_storage_cursor(&mut self, slots: u64, span: Span) {
        let slots = U256::from(slots);
        if let Some(next) = self.next_storage_slot.checked_add(slots) {
            self.next_storage_slot = next;
            return;
        }

        // A range ending at slot `2^256 - 1` fits exactly, but its exclusive
        // end cannot be represented by `U256`.
        let last_offset = slots - U256::from(1);
        if U256::MAX - self.next_storage_slot == last_offset {
            self.storage_exhausted = true;
        } else {
            self.storage_exhausted = true;
            self.emit_storage_layout_overflow(span);
        }
    }

    fn emit_storage_layout_overflow(&self, span: Span) {
        self.gcx
            .dcx()
            .err("contract storage layout exceeds the addressable storage space")
            .span(span)
            .emit();
    }

    /// Returns the byte width for scalar types that this lowering can safely pack.
    fn packed_storage_size(&self, ty: Ty<'gcx>) -> Option<u8> {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bool) => Some(1),
            TyKind::Udvt(inner, _) => self.packed_storage_size(inner),
            _ => None,
        }
    }

    /// Calculates the number of storage slots needed for a type.
    pub(super) fn calculate_storage_slots_for_ty(&self, ty: Ty<'gcx>, span: Span) -> u64 {
        match ty.peel_refs().kind {
            TyKind::Struct(struct_id) => {
                let mut total = 0u64;
                for &field_ty in self.gcx.struct_field_types(struct_id) {
                    total = match total
                        .checked_add(self.calculate_storage_slots_for_ty(field_ty, span))
                    {
                        Some(total) => total,
                        None => {
                            self.gcx
                                .dcx()
                                .err("storage structs this large are not supported")
                                .span(span)
                                .emit();
                            return 1;
                        }
                    };
                }
                total.max(1)
            }
            // Fixed-size arrays occupy one slot per element (no packing),
            // starting at the base slot. Dynamic arrays keep one length slot.
            TyKind::Array(elem, len) => {
                let elem_slots = self.calculate_storage_slots_for_ty(elem, span);
                match u64::try_from(len).ok().and_then(|len| len.checked_mul(elem_slots)) {
                    Some(slots) => slots.max(1),
                    None => {
                        self.gcx
                            .dcx()
                            .err("fixed-size storage arrays this large are not supported")
                            .span(span)
                            .emit();
                        1
                    }
                }
            }
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
        if !location.is_packed() {
            return word;
        }

        let shifted = if location.offset == 0 {
            word
        } else {
            let shift = builder.imm_u64(u64::from(location.offset) * 8);
            builder.shr(shift, word)
        };
        let mask = Self::packed_storage_mask(location.size);
        let mask = builder.imm_u256(mask);
        builder.and(shifted, mask)
    }

    pub(super) fn store_storage_location(
        &self,
        builder: &mut FunctionBuilder<'_>,
        location: StorageLocation,
        value: ValueId,
    ) {
        let slot = builder.imm_u256(location.slot);
        if !location.is_packed() {
            builder.sstore(slot, value);
            return;
        }

        let shift_bits = usize::from(location.offset) * 8;
        let field_mask = Self::packed_storage_mask(location.size);
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

    fn packed_storage_mask(size: u8) -> U256 {
        if size >= StorageLocation::WORD_SIZE {
            U256::MAX
        } else {
            (U256::from(1) << (usize::from(size) * 8)) - U256::from(1)
        }
    }

    /// Gets the storage slot offset for a struct field.
    pub(crate) fn get_struct_field_slot_offset(
        &mut self,
        struct_id: hir::StructId,
        field_index: usize,
    ) -> u64 {
        if let Some(&offset) = self.struct_field_offsets.get(&(struct_id, field_index)) {
            return offset;
        }

        let mut offset = 0u64;
        for (i, &field_ty) in self.gcx.struct_field_types(struct_id).iter().enumerate() {
            if i == field_index {
                break;
            }
            offset += self.calculate_storage_slots_for_ty(field_ty, Span::DUMMY);
        }

        self.struct_field_offsets.insert((struct_id, field_index), offset);
        offset
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
        let fields = field_tys
            .into_iter()
            .map(|field_ty| self.storage_field_for_ty(field_ty))
            .collect::<Vec<_>>();
        let layout = self.module.intern_storage_layout(StorageLayout::Struct(fields.into()));
        self.struct_storage_layouts.insert(struct_id, Arc::clone(&layout));
        layout
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
        if !self.struct_needs_deep_storage_copy(struct_id) {
            let layout = self.storage_layout_for_struct(struct_id);
            builder.storage_to_memory(Arc::clone(&layout), base_slot, memory);
            return mem_offset + layout.memory_words() * 32;
        }
        let field_tys = self.gcx.struct_field_types(struct_id).to_vec();
        let mut storage_offset = 0;
        for (index, field_ty) in field_tys.iter().copied().enumerate() {
            let field_slot = self.offset_storage_slot(builder, base_slot, storage_offset);
            let value = self.load_storage_value_at(builder, field_ty, field_slot);
            let field_address = self.offset_ptr(builder, memory, index as u64 * 32);
            builder.mstore(field_address, value);
            storage_offset += self.calculate_storage_slots_for_ty(field_ty, Span::DUMMY);
        }
        mem_offset + field_tys.len().max(1) as u64 * 32
    }

    /// Clears every storage slot occupied by a struct at a runtime-computed base slot.
    pub(crate) fn clear_storage_struct_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        struct_id: hir::StructId,
        base_slot: ValueId,
    ) {
        if !self.struct_needs_deep_storage_copy(struct_id) {
            let layout = self.storage_layout_for_struct(struct_id);
            builder.clear_storage(layout, base_slot);
            return;
        }
        let fields = self.gcx.struct_field_types(struct_id).to_vec();
        let mut storage_offset = 0;
        for field_ty in fields {
            let field_slot = self.offset_storage_slot(builder, base_slot, storage_offset);
            self.clear_storage_value_at(builder, field_ty, field_slot);
            storage_offset += self.calculate_storage_slots_for_ty(field_ty, Span::DUMMY);
        }
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
        self.copy_memory_array_to_dynamic_storage(builder, slot, mem_ptr, elem, None, elem);
    }

    fn copy_memory_array_to_dynamic_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        mem_ptr: ValueId,
        source_elem: Ty<'gcx>,
        source_len: Option<u64>,
        destination_elem: Ty<'gcx>,
    ) {
        let old_len = builder.sload(slot);
        let len = source_len
            .map(|len| builder.imm_u64(len))
            .unwrap_or_else(|| builder.memory_object_len(mem_ptr, MemoryObjectKind::DynamicArray));
        builder.sstore(slot, len);
        let zero = builder.imm_u64(0);
        builder.mstore(zero, slot);
        let word = builder.imm_u64(32);
        let data_slot = builder.keccak256(zero, word);
        let data_ptr = if source_len.is_some() {
            mem_ptr
        } else {
            builder.memory_object_data(mem_ptr, MemoryObjectKind::DynamicArray)
        };
        let elem_slots = self.calculate_storage_slots_for_ty(destination_elem, Span::DUMMY);
        let source_elem = source_elem.peel_refs();
        let destination_elem = destination_elem.peel_refs();
        self.emit_decode_elements_loop(builder, len, move |this, builder, index| {
            let mem_off = builder.mul(index, word);
            let mem_word_addr = builder.add(data_ptr, mem_off);
            let mem_word = builder.mload(mem_word_addr);
            let elem_slot = if elem_slots == 1 {
                builder.add(data_slot, index)
            } else {
                let stride = builder.imm_u64(elem_slots);
                let off = builder.mul(index, stride);
                builder.add(data_slot, off)
            };
            this.store_storage_value_from_memory_at(
                builder,
                destination_elem,
                source_elem,
                elem_slot,
                mem_word,
            );
        });

        let shrunk = builder.lt(len, old_len);
        let stale_len = builder.sub(old_len, len);
        let stale_len = builder.select(shrunk, stale_len, zero);
        self.emit_decode_elements_loop(builder, stale_len, move |this, builder, index| {
            let stale_index = builder.add(len, index);
            let elem_slot = if elem_slots == 1 {
                builder.add(data_slot, stale_index)
            } else {
                let stride = builder.imm_u64(elem_slots);
                let off = builder.mul(stale_index, stride);
                builder.add(data_slot, off)
            };
            this.clear_storage_value_at(builder, destination_elem, elem_slot);
        });
    }

    /// Copies a memory fixed-size array to a fixed storage array, then clears
    /// any destination elements beyond the source length.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn copy_memory_fixed_array_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        mem_ptr: ValueId,
        source_elem: Ty<'gcx>,
        source_len: u64,
        destination_elem: Ty<'gcx>,
        destination_len: u64,
    ) {
        let elem_slots = self.calculate_storage_slots_for_ty(destination_elem, Span::DUMMY);
        let source_elem = source_elem.peel_refs();
        let destination_elem = destination_elem.peel_refs();
        let copied_len = source_len.min(destination_len);
        for i in 0..copied_len {
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
            self.store_storage_value_from_memory_at(
                builder,
                destination_elem,
                source_elem,
                elem_slot,
                mem_word,
            );
        }
        for i in copied_len..destination_len {
            let elem_slot = if i * elem_slots == 0 {
                slot
            } else {
                let off = builder.imm_u64(i * elem_slots);
                builder.add(slot, off)
            };
            self.clear_storage_value_at(builder, destination_elem, elem_slot);
        }
    }
}

use super::Lowerer;
use crate::mir::{FunctionBuilder, StorageField, StorageLayout, StorageLayoutRef, ValueId};
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
    pub(super) size: u8,
}

impl StorageLocation {
    const WORD_SIZE: u8 = 32;

    const fn full_word(slot: u64) -> Self {
        Self { slot, offset: 0, size: Self::WORD_SIZE }
    }

    const fn is_packed(self) -> bool {
        self.offset != 0 || self.size != Self::WORD_SIZE
    }
}

impl<'gcx> Lowerer<'gcx> {
    /// Allocates the storage location for a state variable.
    pub(super) fn allocate_storage_location(
        &mut self,
        ty: Ty<'gcx>,
        span: Span,
    ) -> StorageLocation {
        if let Some(size) = self.packed_storage_size(ty)
            && size < StorageLocation::WORD_SIZE
        {
            if self.next_storage_offset + size > StorageLocation::WORD_SIZE {
                self.next_storage_slot += 1;
                self.next_storage_offset = 0;
            }
            let location = StorageLocation {
                slot: self.next_storage_slot,
                offset: self.next_storage_offset,
                size,
            };
            self.next_storage_offset += size;
            if self.next_storage_offset == StorageLocation::WORD_SIZE {
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
                    total += self.calculate_storage_slots_for_ty(field_ty, span);
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
        let slot = builder.imm_u64(location.slot);
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

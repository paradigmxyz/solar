//! Index expression lowering.

use super::Lowerer;
use crate::mir::{FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, TypeSize, ValueId};
use alloy_primitives::U256;
use solar_sema::{
    hir::{self, ElementaryType},
    ty::TyKind,
};

impl<'gcx> Lowerer<'gcx> {
    pub(super) fn lower_index_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
        base: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> ValueId {
        if let Some(array) = self.storage_array_slot_of_base(builder, base) {
            let index_val = self.lower_index_value(builder, base.span, index);
            if let Some(packed) = array.packed {
                let data_slot =
                    self.storage_array_data_slot(builder, array.slot, array.fixed_len, index_val);
                return self.load_packed_array_element(builder, data_slot, index_val, packed);
            }
            let element_slot = self.lower_storage_array_element_slot(
                builder,
                array.slot,
                array.fixed_len,
                index_val,
                array.elem_slots,
            );
            if let Some(ty) = self.get_expr_type(expr)
                && let TyKind::Struct(struct_id) = ty.peel_refs().kind
            {
                let struct_size = self.calculate_memory_words_for_ty(ty) * 32;
                let struct_ptr =
                    self.allocate_memory_object(builder, struct_size, MemoryObjectKind::Struct);
                self.copy_storage_to_memory_at(builder, struct_id, element_slot, struct_ptr, 0);
                return struct_ptr;
            }
            if self.expr_has_bytes_or_string_type(expr) {
                return self.materialize_storage_bytes(builder, element_slot);
            }
            return builder.sload(element_slot);
        }

        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            if mapping.value_is_mapping {
                return mapping.slot;
            }
            if let Some(ty) = self.get_expr_type(expr)
                && let TyKind::Struct(struct_id) = ty.peel_refs().kind
            {
                let struct_size = self.calculate_memory_words_for_ty(ty) * 32;
                let struct_ptr = self.allocate_memory_object(
                    builder,
                    struct_size,
                    crate::mir::MemoryObjectKind::Struct,
                );
                self.copy_storage_to_memory_at(builder, struct_id, mapping.slot, struct_ptr, 0);
                return struct_ptr;
            }
            if self.expr_has_bytes_or_string_type(expr) {
                return self.materialize_storage_bytes(builder, mapping.slot);
            }
            // A narrow mapping value owns its slot but is stored in solc's
            // low-aligned masked form; canonicalize on load.
            if let Some(ty) = self.get_expr_type(expr)
                && let Some(packed) = self.packed_value_of_ty(ty)
            {
                return builder.load_packed(mapping.slot, 0, packed);
            }
            return builder.sload(mapping.slot);
        }

        if let Some((slice, is_bytes)) = self.calldata_bytes_source(builder, base) {
            let index_val = self.lower_index_value(builder, base.span, index);
            let len = builder.slice_len(slice);
            self.emit_index_bounds_check(builder, index_val, len);
            let offset_32 = builder.imm_u64(32);
            let data_pos = builder.slice_ptr(slice);
            if is_bytes {
                let byte_pos = builder.add(data_pos, index_val);
                let word = builder.calldataload(byte_pos);
                let mask = builder.imm_u256(U256::from(0xffu64) << 248);
                return builder.and(word, mask);
            }
            // Only a word element sits inline in one slot. Anything wider — a
            // struct, a fixed array, a dynamic element — is laid out by the ABI
            // rules for the element type, so stride by its head size and rebuild
            // it, rather than loading a single word at `data + i * 32`.
            let elem_ty = self.get_expr_type(base).and_then(|ty| match ty.peel_refs().kind {
                TyKind::DynArray(elem) | TyKind::Slice(elem) | TyKind::Array(elem, _) => Some(elem),
                _ => None,
            });
            if let Some(elem_ty) = elem_ty
                && !self.abi_is_word_element(elem_ty)
            {
                let element_pos = if self.abi_is_dynamic(elem_ty) {
                    // A dynamic element's slot holds its offset from the array's
                    // data start.
                    let slot_offset = builder.mul(index_val, offset_32);
                    let slot_pos = builder.add(data_pos, slot_offset);
                    let tail_offset = builder.calldataload(slot_pos);
                    builder.add(data_pos, tail_offset)
                } else {
                    let stride = match self.abi_head_size(elem_ty) {
                        Ok(size) => builder.imm_u64(size),
                        Err(guar) => return builder.error_value(guar),
                    };
                    let byte_offset = builder.mul(index_val, stride);
                    builder.add(data_pos, byte_offset)
                };
                return self.materialize_calldata_value_at(
                    builder,
                    super::bytes::AbiSource::Calldata,
                    elem_ty,
                    element_pos,
                );
            }

            let byte_offset = builder.mul(index_val, offset_32);
            let element_pos = builder.add(data_pos, byte_offset);
            return builder.calldataload(element_pos);
        }

        // Storage `bytes`/`string` (state variable or a field reached through a
        // storage reference): its value lowers to a `[length][data...]` memory
        // copy; index into that with a bounds check.
        if self.expr_is_storage_bytes_lvalue(base) {
            let base_val = self.lower_value_expr(builder, base);
            let index_val = self.lower_index_value(builder, base.span, index);
            let len = builder.memory_object_len(base_val, MemoryObjectKind::Bytes);
            self.emit_index_bounds_check(builder, index_val, len);
            let data_base = builder.memory_object_data(base_val, MemoryObjectKind::Bytes);
            let byte_addr = builder.add(data_base, index_val);
            let word = builder.mload(byte_addr);
            let mask = builder.imm_u256(U256::from(0xffu64) << 248);
            return builder.and(word, mask);
        }

        if self.is_memory_bytes_expr(base) {
            let base_val = self.lower_value_expr(builder, base);
            let index_val = self.lower_index_value(builder, base.span, index);
            let len = builder.memory_object_len(base_val, MemoryObjectKind::Bytes);
            self.emit_index_bounds_check(builder, index_val, len);
            let data_base = builder.memory_object_data(base_val, MemoryObjectKind::Bytes);
            let byte_addr = builder.add(data_base, index_val);
            let word = builder.mload(byte_addr);
            let mask = builder.imm_u256(U256::from(0xffu64) << 248);
            return builder.and(word, mask);
        }

        if let Some(ty) = self.get_expr_type(base)
            && let TyKind::Elementary(ElementaryType::FixedBytes(n)) = ty.peel_refs().kind
        {
            let base_val = self.lower_value_expr(builder, base);
            let index_val = self.lower_index_value(builder, base.span, index);
            let n_val = builder.imm_u64(u64::from(n.bytes()));
            self.emit_index_bounds_check(builder, index_val, n_val);
            let eight = builder.imm_u64(8);
            let shift = builder.mul(index_val, eight);
            let shifted = builder.shl(shift, base_val);
            return self.clean_fixed_bytes(builder, shifted, TypeSize::new_fb_bytes(1));
        }

        let base_val = self.lower_value_expr(builder, base);
        let index_val = self.lower_index_value(builder, base.span, index);
        let layout = if self.is_dynamic_array_expr(base)
            || Self::value_is_dynamic_array_object(builder, base_val)
        {
            let len = builder.memory_object_len(base_val, MemoryObjectKind::DynamicArray);
            self.emit_index_bounds_check(builder, index_val, len);
            MemoryObjectLayout::WORD_ARRAY
        } else {
            let Some(len) = self.fixed_array_len_of_expr(base) else {
                return self.err_value(
                    builder,
                    base.span,
                    "codegen expected a memory array for index access",
                );
            };
            let len_val = builder.imm_u64(len);
            self.emit_index_bounds_check(builder, index_val, len_val);
            MemoryObjectLayout::word_fixed_array(len)
        };
        let addr = builder.memory_object_element_addr(base_val, layout, index_val);
        builder.mload(addr)
    }

    pub(super) fn lower_index_assign(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lhs: &hir::Expr<'_>,
        base: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
        rhs: ValueId,
    ) {
        if let Some(array) = self.storage_array_slot_of_base(builder, base) {
            let index_val = self.lower_index_value(builder, base.span, index);
            if let Some(packed) = array.packed {
                let data_slot =
                    self.storage_array_data_slot(builder, array.slot, array.fixed_len, index_val);
                self.store_packed_array_element(builder, data_slot, index_val, packed, rhs);
                return;
            }
            let element_slot = self.lower_storage_array_element_slot(
                builder,
                array.slot,
                array.fixed_len,
                index_val,
                array.elem_slots,
            );
            builder.sstore(element_slot, rhs);
            return;
        }

        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            if let Some(ty) = self.get_expr_type(lhs)
                && let TyKind::Struct(struct_id) = ty.peel_refs().kind
            {
                self.copy_memory_to_storage_at(builder, struct_id, mapping.slot, rhs, 0);
            } else if self.expr_has_bytes_or_string_type(lhs) {
                self.copy_memory_bytes_to_storage(builder, mapping.slot, rhs);
            } else if let Some(packed) =
                self.get_expr_type(lhs).and_then(|ty| self.packed_value_of_ty(ty))
            {
                // A narrow mapping value owns its slot: store solc's
                // low-aligned masked form without a read-modify-write.
                let prepared = builder.prepare_packed(rhs, packed);
                builder.sstore(mapping.slot, prepared);
            } else {
                builder.sstore(mapping.slot, rhs);
            }
            return;
        }

        if self.expr_is_storage_bytes_lvalue(base)
            && let Some(slot) = self.lower_lvalue_slot(builder, base)
        {
            let index_val = self.lower_index_value(builder, base.span, index);
            self.store_storage_bytes_element(builder, slot, index_val, rhs);
            return;
        }

        if self.is_memory_bytes_expr(base) {
            let base_val = self.lower_value_expr(builder, base);
            let index_val = self.lower_index_value(builder, base.span, index);
            let len = builder.memory_object_len(base_val, MemoryObjectKind::Bytes);
            self.emit_index_bounds_check(builder, index_val, len);
            let data_base = builder.memory_object_data(base_val, MemoryObjectKind::Bytes);
            let byte_addr = builder.add(data_base, index_val);
            let byte_val = self.bytes1_store_byte(builder, rhs);
            builder.mstore8(byte_addr, byte_val);
            return;
        }

        let base_val = self.lower_value_expr(builder, base);
        let index_val = self.lower_index_value(builder, base.span, index);
        let layout = if self.is_dynamic_array_expr(base)
            || Self::value_is_dynamic_array_object(builder, base_val)
        {
            let len = builder.memory_object_len(base_val, MemoryObjectKind::DynamicArray);
            self.emit_index_bounds_check(builder, index_val, len);
            MemoryObjectLayout::WORD_ARRAY
        } else {
            let Some(len) = self.fixed_array_len_of_expr(base) else {
                self.err_value(
                    builder,
                    base.span,
                    "codegen expected a memory array for indexed assignment",
                );
                return;
            };
            let len_val = builder.imm_u64(len);
            self.emit_index_bounds_check(builder, index_val, len_val);
            MemoryObjectLayout::word_fixed_array(len)
        };
        let addr = builder.memory_object_element_addr(base_val, layout, index_val);
        builder.mstore(addr, rhs);
    }

    pub(super) fn lower_index_lvalue_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        if let Some(array) = self.storage_array_slot_of_base(builder, base) {
            let index_val = self.lower_index_value(builder, base.span, index);
            return Some(self.lower_storage_array_element_slot(
                builder,
                array.slot,
                array.fixed_len,
                index_val,
                array.elem_slots,
            ));
        }
        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            return Some(mapping.slot);
        }
        None
    }

    pub(super) fn lower_index_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        span: solar_interface::Span,
        index: Option<&hir::Expr<'_>>,
    ) -> ValueId {
        match index {
            Some(index) => self.lower_value_expr(builder, index),
            None => self.err_value(builder, span, "codegen expected an index expression"),
        }
    }
}

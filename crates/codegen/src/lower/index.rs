//! Index expression lowering.

use super::Lowerer;
use crate::mir::{FunctionBuilder, MemoryObjectKind, MemoryObjectLayout, TypeSize, ValueId};
use alloy_primitives::U256;
use solar_sema::{
    hir::{self, ElementaryType},
    ty::{Ty, TyKind},
};

impl<'gcx> Lowerer<'gcx> {
    pub(super) fn lower_index_expr(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
        base: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> ValueId {
        if let Some((slot_val, fixed_len, elem_slots)) =
            self.storage_array_slot_of_base(builder, base)
        {
            let index_val = self.lower_index_value(builder, base.span, index);
            let element_slot = self.lower_storage_array_element_slot(
                builder, slot_val, fixed_len, index_val, elem_slots,
            );
            if let Some(ty) = self.get_expr_type(expr) {
                return self.load_storage_value_at(builder, ty, element_slot);
            }
            return builder.sload(element_slot);
        }

        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            if mapping.value_is_mapping {
                return mapping.slot;
            }
            if let Some(ty) = self.get_expr_type(expr) {
                return self.load_storage_value_at(builder, ty, mapping.slot);
            }
            return builder.sload(mapping.slot);
        }

        let calldata_source = self.calldata_bytes_source(builder, base);
        if let super::CalldataValue::Slice(slice, is_bytes) = calldata_source {
            let index_val = self.lower_index_value(builder, base.span, index);
            let len = builder.slice_len(slice);
            self.emit_index_bounds_check(builder, index_val, len);
            let offset_32 = builder.imm_u64(32);
            let data_pos = builder.slice_ptr(slice);
            let is_memory = Self::value_is_memory_slice(builder, slice);
            if is_bytes {
                let byte_pos = builder.add(data_pos, index_val);
                let word = if is_memory {
                    builder.mload(byte_pos)
                } else {
                    builder.calldataload(byte_pos)
                };
                let mask = builder.imm_u256(U256::from(0xffu64) << 248);
                return builder.and(word, mask);
            }
            if is_memory {
                let byte_offset = builder.mul(index_val, offset_32);
                let element_pos = builder.add(data_pos, byte_offset);
                return builder.mload(element_pos);
            }
            // Only a word element sits inline in one slot. Anything wider — a
            // struct, a fixed array, a dynamic element — is laid out by the ABI
            // rules for the element type, so stride by its head size and rebuild
            // it, rather than loading a single word at `data + i * 32`.
            let elem_ty = self.get_expr_type(base).and_then(|ty| ty.base_type(self.gcx));
            if let Some(elem_ty) = elem_ty
                && !self.abi_is_word_element(elem_ty)
            {
                let element_pos = if self.abi_is_dynamic(elem_ty) {
                    // A dynamic element's slot holds its offset from the array's
                    // data start.
                    let slot_offset = builder.mul(index_val, offset_32);
                    let slot_pos = builder.add(data_pos, slot_offset);
                    let end = builder.calldatasize();
                    self.require_abi_range(builder, slot_pos, offset_32, end);
                    let max_len = builder.imm_u256(U256::MAX / U256::from(32));
                    let head_overflow = builder.gt(len, max_len);
                    self.emit_abi_decode_revert_if(builder, head_overflow);
                    let head_size = builder.mul(len, offset_32);
                    self.resolve_abi_value_pos(
                        builder,
                        super::bytes::AbiSource::Calldata,
                        elem_ty,
                        slot_pos,
                        data_pos,
                        head_size,
                        end,
                    )
                } else {
                    let stride = builder.imm_u64(self.abi_head_size(elem_ty));
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
            let value = builder.calldataload(element_pos);
            if let Some(elem_ty) = elem_ty {
                self.emit_abi_field_clean_check(builder, elem_ty, value);
                return self.normalize_abi_decoded_word(builder, elem_ty, value);
            }
            return value;
        }
        let prelowered_base = match calldata_source {
            super::CalldataValue::Lowered(value) => Some(value),
            super::CalldataValue::Slice(..) | super::CalldataValue::NotApplicable => None,
        };

        // Storage `bytes`/`string` (state variable or a field reached through a
        // storage reference): materialize its packed storage representation,
        // then index the resulting `[length][data...]` memory copy.
        if self.expr_is_storage_bytes_lvalue(base) {
            let slot = self.lower_lvalue_slot(builder, base).unwrap_or_else(|| {
                self.err_value(builder, base.span, "unsupported storage bytes expression")
            });
            let base_val = self.materialize_storage_bytes(builder, slot);
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
            let base_val = prelowered_base.unwrap_or_else(|| self.lower_value_expr(builder, base));
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

        let base_val = prelowered_base.unwrap_or_else(|| self.lower_value_expr(builder, base));
        let index_val = self.lower_index_value(builder, base.span, index);
        let layout = if self.is_dynamic_array_expr(base)
            || Self::value_is_dynamic_array_object(builder, base_val)
        {
            let len = self
                .new_dynamic_memory_array_const_len(base)
                .map(|len| builder.imm_u64(len))
                .unwrap_or_else(|| {
                    builder.memory_object_len(base_val, MemoryObjectKind::DynamicArray)
                });
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
        source_ty: Option<Ty<'gcx>>,
    ) {
        if let Some((slot_val, fixed_len, elem_slots)) =
            self.storage_array_slot_of_base(builder, base)
        {
            let index_val = self.lower_index_value(builder, base.span, index);
            let element_slot = self.lower_storage_array_element_slot(
                builder, slot_val, fixed_len, index_val, elem_slots,
            );
            if let Some(ty) = self.get_expr_type(lhs) {
                self.store_storage_value_from_memory_at(
                    builder,
                    ty,
                    source_ty.unwrap_or(ty),
                    element_slot,
                    rhs,
                );
            } else {
                builder.sstore(element_slot, rhs);
            }
            return;
        }

        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            if let Some(ty) = self.get_expr_type(lhs) {
                self.store_storage_value_from_memory_at(
                    builder,
                    ty,
                    source_ty.unwrap_or(ty),
                    mapping.slot,
                    rhs,
                );
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
        if let Some((slot_val, fixed_len, elem_slots)) =
            self.storage_array_slot_of_base(builder, base)
        {
            let index_val = self.lower_index_value(builder, base.span, index);
            return Some(self.lower_storage_array_element_slot(
                builder, slot_val, fixed_len, index_val, elem_slots,
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

//! Index expression lowering.

use super::Lowerer;
use crate::mir::{FunctionBuilder, TypeSize, ValueId};
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
        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            if mapping.value_is_mapping {
                return mapping.slot;
            }
            if let Some(ty) = self.get_expr_type(expr)
                && let TyKind::Struct(struct_id) = ty.peel_refs().kind
            {
                let struct_size = self.calculate_memory_words_for_ty(ty) * 32;
                let struct_ptr = self.allocate_memory(builder, struct_size);
                self.copy_storage_to_memory_at(builder, struct_id, mapping.slot, struct_ptr, 0);
                return struct_ptr;
            }
            if self.expr_has_bytes_or_string_type(expr) {
                return self.materialize_storage_bytes(builder, mapping.slot);
            }
            return builder.sload(mapping.slot);
        }

        if let Some((slot_val, fixed_len, elem_slots)) =
            self.storage_array_slot_of_base(builder, base)
        {
            let index_val = self.lower_index_or_zero(builder, index);
            let element_slot = self.lower_storage_array_element_slot(
                builder, slot_val, fixed_len, index_val, elem_slots,
            );
            return builder.sload(element_slot);
        }

        if let Some((head, is_bytes)) = self.calldata_dyn_head(base) {
            let index_val = self.lower_index_or_zero(builder, index);
            let four = builder.imm_u64(4);
            let len_pos = builder.add(four, head);
            let len = builder.calldataload(len_pos);
            self.emit_index_bounds_check(builder, index_val, len);
            let offset_32 = builder.imm_u64(32);
            let data_pos = builder.add(len_pos, offset_32);
            if is_bytes {
                let byte_pos = builder.add(data_pos, index_val);
                let word = builder.calldataload(byte_pos);
                let mask = builder.imm_u256(U256::from(0xffu64) << 248);
                return builder.and(word, mask);
            }
            let byte_offset = builder.mul(index_val, offset_32);
            let element_pos = builder.add(data_pos, byte_offset);
            return builder.calldataload(element_pos);
        }

        // Storage `bytes`/`string` (state variable or a field reached through a
        // storage reference): its value lowers to a `[length][data...]` memory
        // copy; index into that with a bounds check.
        if self.expr_is_storage_bytes_lvalue(base) {
            let base_val = self.lower_expr(builder, base);
            let index_val = self.lower_index_or_zero(builder, index);
            let len = builder.mload(base_val);
            self.emit_index_bounds_check(builder, index_val, len);
            let offset_32 = builder.imm_u64(32);
            let data_base = builder.add(base_val, offset_32);
            let byte_addr = builder.add(data_base, index_val);
            let word = builder.mload(byte_addr);
            let mask = builder.imm_u256(U256::from(0xffu64) << 248);
            return builder.and(word, mask);
        }

        if self.is_memory_bytes_expr(base) {
            let base_val = self.lower_expr(builder, base);
            let index_val = self.lower_index_or_zero(builder, index);
            let len = builder.mload(base_val);
            self.emit_index_bounds_check(builder, index_val, len);
            let offset_32 = builder.imm_u64(32);
            let data_base = builder.add(base_val, offset_32);
            let byte_addr = builder.add(data_base, index_val);
            let word = builder.mload(byte_addr);
            let mask = builder.imm_u256(U256::from(0xffu64) << 248);
            return builder.and(word, mask);
        }

        if let Some(ty) = self.get_expr_type(base)
            && let TyKind::Elementary(ElementaryType::FixedBytes(n)) = ty.peel_refs().kind
        {
            let base_val = self.lower_expr(builder, base);
            let index_val = self.lower_index_or_zero(builder, index);
            let n_val = builder.imm_u64(u64::from(n.bytes()));
            self.emit_index_bounds_check(builder, index_val, n_val);
            let eight = builder.imm_u64(8);
            let shift = builder.mul(index_val, eight);
            let shifted = builder.shl(shift, base_val);
            return self.clean_fixed_bytes(builder, shifted, TypeSize::new_fb_bytes(1));
        }

        let base_val = self.lower_expr(builder, base);
        let index_val = self.lower_index_or_zero(builder, index);
        let offset_32 = builder.imm_u64(32);
        let byte_offset = builder.mul(index_val, offset_32);
        let data_base = if self.is_dynamic_memory_array_expr(base) {
            let len = self
                .new_dynamic_memory_array_const_len(base)
                .map(|len| builder.imm_u64(len))
                .unwrap_or_else(|| builder.mload(base_val));
            self.emit_index_bounds_check(builder, index_val, len);
            builder.add(base_val, offset_32)
        } else {
            if let Some(len) = self.fixed_array_len_of_expr(base) {
                let len_val = builder.imm_u64(len);
                self.emit_index_bounds_check(builder, index_val, len_val);
            }
            base_val
        };
        let addr = builder.add(data_base, byte_offset);
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
        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            if let Some(ty) = self.get_expr_type(lhs)
                && let TyKind::Struct(struct_id) = ty.peel_refs().kind
            {
                self.copy_memory_to_storage_at(builder, struct_id, mapping.slot, rhs, 0);
            } else if self.expr_has_bytes_or_string_type(lhs) {
                self.copy_memory_bytes_to_storage(builder, mapping.slot, rhs);
            } else {
                builder.sstore(mapping.slot, rhs);
            }
            return;
        }

        if self.expr_is_storage_bytes_lvalue(base)
            && let Some(slot) = self.lower_lvalue_slot(builder, base)
        {
            let index_val = self.lower_index_or_zero(builder, index);
            self.store_storage_bytes_element(builder, slot, index_val, rhs);
            return;
        }

        if let Some((slot_val, fixed_len, elem_slots)) =
            self.storage_array_slot_of_base(builder, base)
        {
            let index_val = self.lower_index_or_zero(builder, index);
            let element_slot = self.lower_storage_array_element_slot(
                builder, slot_val, fixed_len, index_val, elem_slots,
            );
            builder.sstore(element_slot, rhs);
            return;
        }

        if self.is_memory_bytes_expr(base) {
            let base_val = self.lower_expr(builder, base);
            let index_val = self.lower_index_or_zero(builder, index);
            let len = builder.mload(base_val);
            self.emit_index_bounds_check(builder, index_val, len);
            let offset_32 = builder.imm_u64(32);
            let data_base = builder.add(base_val, offset_32);
            let byte_addr = builder.add(data_base, index_val);
            let byte_val = self.bytes1_store_byte(builder, rhs);
            builder.mstore8(byte_addr, byte_val);
            return;
        }

        let base_val = self.lower_expr(builder, base);
        let index_val = self.lower_index_or_zero(builder, index);
        let offset_32 = builder.imm_u64(32);
        let byte_offset = builder.mul(index_val, offset_32);
        let data_base = if self.is_dynamic_memory_array_expr(base) {
            let len = builder.mload(base_val);
            self.emit_index_bounds_check(builder, index_val, len);
            builder.add(base_val, offset_32)
        } else {
            if let Some(len) = self.fixed_array_len_of_expr(base) {
                let len_val = builder.imm_u64(len);
                self.emit_index_bounds_check(builder, index_val, len_val);
            }
            base_val
        };
        let addr = builder.add(data_base, byte_offset);
        builder.mstore(addr, rhs);
    }

    pub(super) fn lower_index_lvalue_slot(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        base: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        if let Some(mapping) = self.lower_mapping_element_slot(builder, base, index) {
            return Some(mapping.slot);
        }
        if let Some((slot_val, fixed_len, elem_slots)) =
            self.storage_array_slot_of_base(builder, base)
        {
            let index_val = self.lower_index_or_zero(builder, index);
            return Some(self.lower_storage_array_element_slot(
                builder, slot_val, fixed_len, index_val, elem_slots,
            ));
        }
        None
    }

    pub(super) fn lower_index_or_zero(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> ValueId {
        match index {
            Some(index) => self.lower_expr(builder, index),
            None => builder.imm_u64(0),
        }
    }
}

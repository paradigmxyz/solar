//! Bytes and string lowering helpers.

use super::{Lowerer, checked_arith::PanicCode};
use crate::mir::{FunctionBuilder, ValueId};
use alloy_primitives::{U256, keccak256};
use solar_ast::LitKind;
use solar_interface::{Symbol, kw, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::{Ty, TyKind},
};

impl<'gcx> Lowerer<'gcx> {
    /// Lowers a string/bytes literal to Solidity's memory layout
    /// `[length][data...]` and returns the memory pointer. General literal
    /// lowering still returns a word; ABI return encoding needs a real pointer.
    pub(super) fn lower_string_literal_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        lit: &hir::Lit<'_>,
    ) -> Option<ValueId> {
        let LitKind::Str(_, bytes, _) = &lit.kind else { return None };
        Some(self.lower_string_bytes_to_memory(builder, bytes.as_byte_str()))
    }

    /// Materializes constant bytes as a `[length][data...]` memory string.
    pub(super) fn lower_string_bytes_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        bytes: &[u8],
    ) -> ValueId {
        let len = bytes.len();
        let aligned = len.div_ceil(32) * 32;
        let ptr = self.allocate_memory(builder, (32 + aligned) as u64);
        let len_val = builder.imm_u64(len as u64);
        builder.mstore(ptr, len_val);

        let word = builder.imm_u64(32);
        let data_start = builder.add(ptr, word);
        for (i, chunk) in bytes.chunks(32).enumerate() {
            let mut padded = [0u8; 32];
            padded[..chunk.len()].copy_from_slice(chunk);
            let val = builder.imm_u256(U256::from_be_bytes(padded));
            let off = builder.imm_u64((i * 32) as u64);
            let dest = builder.add(data_start, off);
            builder.mstore(dest, val);
        }

        ptr
    }

    pub(super) fn lower_expr_as_memory_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let ExprKind::Lit(lit) = &expr.kind
            && let Some(ptr) = self.lower_string_literal_to_memory(builder, lit)
        {
            return ptr;
        }
        if self.expr_is_calldata_dynamic_bytes(expr) {
            let slice = self.lower_expr(builder, expr);
            return self.materialize_calldata_bytes(builder, slice);
        }
        self.lower_expr(builder, expr)
    }

    /// Copies a calldata `bytes`/`string` parameter into Solidity's memory
    /// bytes layout (`[length][data...]`).
    pub(super) fn materialize_calldata_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slice: ValueId,
    ) -> ValueId {
        let len = builder.slice_len(slice);

        let word_size = builder.imm_u64(32);
        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(len, thirty_one);
        let rounded_overflow = builder.lt(rounded, len);
        self.emit_panic_if(builder, rounded_overflow, PanicCode::MemoryAllocationOverflow);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let is_empty = builder.iszero(padded);
        let data_size = builder.select(is_empty, word_size, padded);
        let total_size = builder.add(word_size, data_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);

        let ptr = self.allocate_memory_dynamic(builder, total_size);
        builder.mstore(ptr, len);

        let data_ptr = builder.add(ptr, word_size);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word_size);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        let data_pos = builder.slice_ptr(slice);
        builder.calldatacopy(data_ptr, data_pos, len);
        ptr
    }

    /// Copies calldata bytes whose absolute length-word position is `len_pos`
    /// into Solidity's memory bytes layout.
    pub(super) fn materialize_calldata_bytes_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        len_pos: ValueId,
    ) -> ValueId {
        let len = builder.calldataload(len_pos);

        let word_size = builder.imm_u64(32);
        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(len, thirty_one);
        let rounded_overflow = builder.lt(rounded, len);
        self.emit_panic_if(builder, rounded_overflow, PanicCode::MemoryAllocationOverflow);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let is_empty = builder.iszero(padded);
        let data_size = builder.select(is_empty, word_size, padded);
        let total_size = builder.add(word_size, data_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);

        let ptr = self.allocate_memory_dynamic(builder, total_size);
        builder.mstore(ptr, len);

        let data_ptr = builder.add(ptr, word_size);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word_size);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        let data_pos = builder.add(len_pos, word_size);
        builder.calldatacopy(data_ptr, data_pos, len);
        ptr
    }

    /// Copies a calldata bytes/string SLICE `base[start:end]` into Solidity's
    /// memory bytes layout (`[length][data...]`). `slice` describes `base`;
    /// `start`/`end` default to `0` and `base.length`.
    pub(super) fn materialize_calldata_slice(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slice: ValueId,
        start: Option<ValueId>,
        end: Option<ValueId>,
    ) -> ValueId {
        let base_len = builder.slice_len(slice);
        let base_data_pos = builder.slice_ptr(slice);
        let word_size = builder.imm_u64(32);

        let start = match start {
            Some(s) => s,
            None => builder.imm_u64(0),
        };
        let end = match end {
            Some(e) => e,
            None => base_len,
        };
        let len = builder.sub(end, start);
        let slice_pos = builder.add(base_data_pos, start);

        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(len, thirty_one);
        let rounded_overflow = builder.lt(rounded, len);
        self.emit_panic_if(builder, rounded_overflow, PanicCode::MemoryAllocationOverflow);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let is_empty = builder.iszero(padded);
        let data_size = builder.select(is_empty, word_size, padded);
        let total_size = builder.add(word_size, data_size);
        let total_overflow = builder.lt(total_size, data_size);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);

        let ptr = self.allocate_memory_dynamic(builder, total_size);
        builder.mstore(ptr, len);

        let data_ptr = builder.add(ptr, word_size);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word_size);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        builder.calldatacopy(data_ptr, slice_pos, len);
        ptr
    }

    pub(super) fn var_expects_memory_bytes_value(&self, var: &hir::Variable<'_>) -> bool {
        matches!(
            var.ty.kind,
            hir::TypeKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String)
        ) && !matches!(
            var.data_location,
            Some(solar_ast::DataLocation::Calldata | solar_ast::DataLocation::Storage)
        )
    }

    /// Whether a declared variable wants a MEMORY dynamic-array value: a
    /// calldata-array initializer must materialize as a memory copy.
    pub(super) fn var_expects_memory_dyn_array_value(&self, var: &hir::Variable<'_>) -> bool {
        matches!(&var.ty.kind, hir::TypeKind::Array(arr) if arr.size.is_none())
            && !matches!(
                var.data_location,
                Some(solar_ast::DataLocation::Calldata | solar_ast::DataLocation::Storage)
            )
    }

    /// Whether an assignment target wants a MEMORY dynamic-array value.
    pub(super) fn lhs_expects_memory_dyn_array_value(&self, lhs: &hir::Expr<'_>) -> bool {
        let ExprKind::Ident(res_slice) = &lhs.kind else { return false };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return false;
        };
        self.var_expects_memory_dyn_array_value(self.gcx.hir.variable(*var_id))
    }

    /// Lowers an expression whose consumer needs a MEMORY dynamic array: a
    /// calldata dynamic array materializes as a `[length][elems...]` copy;
    /// anything else lowers normally (it is already a memory pointer).
    pub(super) fn lower_expr_as_memory_dyn_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let Some((slice, false)) = self.calldata_dyn_slice(expr) {
            if let Some(ty) = self.get_expr_type(expr)
                && let TyKind::DynArray(elem) | TyKind::Slice(elem) = ty.peel_refs().kind
                && !self.abi_is_word_element(elem)
            {
                // Reference/aggregate elements rebuild one at a time; an
                // ABI-root slice's data is preceded by its length word.
                let word = builder.imm_u64(32);
                let data_pos = builder.slice_ptr(slice);
                let len_pos = builder.sub(data_pos, word);
                return self.materialize_calldata_dynamic_array_at(builder, elem, len_pos);
            }
            return self.materialize_calldata_dyn_array(builder, slice);
        }
        self.lower_expr(builder, expr)
    }

    /// Copies a single-word calldata array whose absolute length-word position
    /// is `len_pos` into memory.
    pub(super) fn materialize_calldata_word_array_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        len_pos: ValueId,
    ) -> ValueId {
        let len = builder.calldataload(len_pos);
        let word_size = builder.imm_u64(32);
        let data_pos = builder.add(len_pos, word_size);
        self.copy_calldata_word_array(builder, data_pos, len)
    }

    /// Copies a single-word calldata array SLICE into memory.
    pub(super) fn materialize_calldata_dyn_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slice: ValueId,
    ) -> ValueId {
        let len = builder.slice_len(slice);
        let data_pos = builder.slice_ptr(slice);
        self.copy_calldata_word_array(builder, data_pos, len)
    }

    /// Copies `len` calldata words starting at `data_pos` into a fresh memory
    /// `[length][elems...]` array.
    fn copy_calldata_word_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        data_pos: ValueId,
        len: ValueId,
    ) -> ValueId {
        let word_size = builder.imm_u64(32);
        // Guard `len * 32` overflow before sizing the allocation.
        let shift = builder.imm_u64(251);
        let too_big = builder.shr(shift, len);
        self.emit_panic_if(builder, too_big, PanicCode::MemoryAllocationOverflow);
        let byte_len = builder.mul(len, word_size);
        let total_size = builder.add(word_size, byte_len);

        let ptr = self.allocate_memory_dynamic(builder, total_size);
        builder.mstore(ptr, len);
        let data_ptr = builder.add(ptr, word_size);
        builder.calldatacopy(data_ptr, data_pos, byte_len);
        ptr
    }

    /// Materializes a calldata value whose ABI body starts at the absolute
    /// calldata position `pos`.
    fn materialize_calldata_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        pos: ValueId,
    ) -> ValueId {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.materialize_calldata_bytes_at(builder, pos)
            }
            TyKind::DynArray(elem) | TyKind::Slice(elem) => {
                self.materialize_calldata_dynamic_array_at(builder, elem, pos)
            }
            TyKind::Array(elem, len) => {
                self.materialize_calldata_fixed_array_at(builder, elem, len.to::<u64>(), pos)
            }
            TyKind::Struct(id) => {
                let fields = self.gcx.struct_field_types(id).to_vec();
                self.materialize_calldata_fields_at(builder, &fields, pos)
            }
            TyKind::Tuple(fields) => self.materialize_calldata_fields_at(builder, fields, pos),
            TyKind::Udvt(inner, _) => self.materialize_calldata_value_at(builder, inner, pos),
            _ => builder.calldataload(pos),
        }
    }

    /// Materializes a dynamic calldata array. Arrays of ABI-word values can
    /// be copied directly; reference and aggregate elements are rebuilt one at
    /// a time so their memory slots contain memory pointers.
    fn materialize_calldata_dynamic_array_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elem: Ty<'gcx>,
        len_pos: ValueId,
    ) -> ValueId {
        if self.abi_is_word_element(elem) {
            return self.materialize_calldata_word_array_at(builder, len_pos);
        }

        let len = builder.calldataload(len_pos);
        let word = builder.imm_u64(32);
        let shift = builder.imm_u64(251);
        let too_big = builder.shr(shift, len);
        self.emit_panic_if(builder, too_big, PanicCode::MemoryAllocationOverflow);
        let byte_len = builder.mul(len, word);
        let total_size = builder.add(word, byte_len);
        let total_overflow = builder.lt(total_size, byte_len);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);

        let ptr = self.allocate_memory_dynamic(builder, total_size);
        builder.mstore(ptr, len);

        // Recursive materialization allocates memory and can introduce CFG, so
        // keep loop state in dedicated memory rather than MIR values.
        let scratch = self.allocate_memory(builder, 3 * 32);
        let remaining_slot = scratch;
        let source_slot = self.offset_ptr(builder, scratch, 32);
        let dest_slot = self.offset_ptr(builder, scratch, 64);
        let tuple_base = builder.add(len_pos, word);
        let dest = builder.add(ptr, word);
        builder.mstore(remaining_slot, len);
        builder.mstore(source_slot, tuple_base);
        builder.mstore(dest_slot, dest);

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let done_block = builder.create_block();
        builder.jump(cond_block);

        builder.switch_to_block(cond_block);
        let remaining = builder.mload(remaining_slot);
        let zero = builder.imm_u64(0);
        let has_next = builder.gt(remaining, zero);
        builder.branch(has_next, body_block, done_block);

        builder.switch_to_block(body_block);
        let source = builder.mload(source_slot);
        let elem_pos = self.calldata_abi_value_pos(builder, elem, source, tuple_base);
        let value = self.materialize_calldata_value_at(builder, elem, elem_pos);
        let dest = builder.mload(dest_slot);
        builder.mstore(dest, value);

        let one = builder.imm_u64(1);
        let remaining = builder.mload(remaining_slot);
        let next_remaining = builder.sub(remaining, one);
        builder.mstore(remaining_slot, next_remaining);
        let source = builder.mload(source_slot);
        let elem_head_size = builder.imm_u64(self.abi_head_size(elem));
        let next_source = builder.add(source, elem_head_size);
        builder.mstore(source_slot, next_source);
        let dest = builder.mload(dest_slot);
        let next_dest = builder.add(dest, word);
        builder.mstore(dest_slot, next_dest);
        builder.jump(cond_block);

        builder.switch_to_block(done_block);
        ptr
    }

    /// Materializes a fixed-size calldata array into memory slots.
    fn materialize_calldata_fixed_array_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        elem: Ty<'gcx>,
        len: u64,
        pos: ValueId,
    ) -> ValueId {
        let size = len.checked_mul(32).expect("fixed array memory size overflow");
        let ptr = self.allocate_memory(builder, size);
        let mut head_offset = 0;
        for i in 0..len {
            let head_pos = self.offset_ptr(builder, pos, head_offset);
            let elem_pos = self.calldata_abi_value_pos(builder, elem, head_pos, pos);
            let value = self.materialize_calldata_value_at(builder, elem, elem_pos);
            let dest = self.offset_ptr(builder, ptr, i * 32);
            builder.mstore(dest, value);
            head_offset += self.abi_head_size(elem);
        }
        ptr
    }

    /// Materializes ABI tuple fields into Solidity's one-slot-per-field memory
    /// representation used for structs and tuples.
    fn materialize_calldata_fields_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        fields: &[Ty<'gcx>],
        pos: ValueId,
    ) -> ValueId {
        let size = (fields.len() as u64).checked_mul(32).expect("aggregate memory size overflow");
        let ptr = self.allocate_memory(builder, size);
        let mut head_offset = 0;
        for (i, &field) in fields.iter().enumerate() {
            let head_pos = self.offset_ptr(builder, pos, head_offset);
            let field_pos = self.calldata_abi_value_pos(builder, field, head_pos, pos);
            let value = self.materialize_calldata_value_at(builder, field, field_pos);
            let dest = self.offset_ptr(builder, ptr, (i as u64) * 32);
            builder.mstore(dest, value);
            head_offset += self.abi_head_size(field);
        }
        ptr
    }

    /// Resolves an ABI head position to the corresponding value body. Dynamic
    /// offsets are relative to the containing tuple's head area.
    fn calldata_abi_value_pos(
        &self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        head_pos: ValueId,
        tuple_base: ValueId,
    ) -> ValueId {
        if self.abi_is_dynamic(ty) {
            let offset = builder.calldataload(head_pos);
            builder.add(tuple_base, offset)
        } else {
            head_pos
        }
    }

    pub(super) fn lhs_expects_memory_bytes_value(&self, lhs: &hir::Expr<'_>) -> bool {
        if let ExprKind::Ident(res_slice) = &lhs.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
            && self.gcx.hir.variable(*var_id).data_location
                == Some(solar_ast::DataLocation::Calldata)
        {
            return false;
        }
        if self.expr_has_bytes_or_string_type(lhs) {
            return true;
        }

        let ExprKind::Ident(res_slice) = &lhs.kind else { return false };
        let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first() else {
            return false;
        };
        let var = self.gcx.hir.variable(*var_id);
        self.var_expects_memory_bytes_value(var)
    }

    /// Normalizes a `bytes1`-typed value to its single byte (in the word's low
    /// 8 bits) for `mstore8`. Runtime `bytes1` values are left-aligned (the
    /// convention used by every bytes-element read path), so they shift down;
    /// constants are disambiguated by value: a left-aligned constant has only
    /// the top byte set, while a number-literal constant is already the low
    /// byte.
    pub(super) fn bytes1_store_byte(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
    ) -> ValueId {
        if let crate::mir::Value::Immediate(imm) = builder.func().value(value)
            && let Some(v) = imm.as_u256()
        {
            let byte = if v <= U256::from(0xffu64) { v } else { v >> 248 };
            return builder.imm_u256(byte);
        }
        let shift = builder.imm_u64(248);
        builder.shr(shift, value)
    }

    pub(super) fn store_storage_bytes_element(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        index: ValueId,
        value: ValueId,
    ) {
        let word = builder.sload(slot);
        let one = builder.imm_u64(1);
        let long_bit = builder.and(word, one);
        let is_long = builder.eq(long_bit, one);
        let low_byte_mask = builder.imm_u64(0xff);
        let shift_one = builder.imm_u64(1);
        let len_low = builder.and(word, low_byte_mask);
        let short_len = builder.shr(shift_one, len_low);
        let long_len = builder.shr(shift_one, word);
        let len = builder.select(is_long, long_len, short_len);
        self.emit_index_bounds_check(builder, index, len);
        let byte = self.bytes1_store_byte(builder, value);

        let short_block = builder.create_block();
        let long_block = builder.create_block();
        let done_block = builder.create_block();
        builder.branch(is_long, long_block, short_block);

        builder.switch_to_block(short_block);
        let shift = self.storage_byte_shift(builder, index);
        let updated = self.replace_byte_in_word(builder, word, shift, byte);
        builder.sstore(slot, updated);
        builder.jump(done_block);

        builder.switch_to_block(long_block);
        let word_size = builder.imm_u64(32);
        let scratch = builder.imm_u64(0);
        builder.mstore(scratch, slot);
        let data_slot = builder.keccak256(scratch, word_size);
        let word_index = builder.div(index, word_size);
        let elem_slot = builder.add(data_slot, word_index);
        let byte_index = builder.mod_(index, word_size);
        let data_word = builder.sload(elem_slot);
        let shift = self.storage_byte_shift(builder, byte_index);
        let updated = self.replace_byte_in_word(builder, data_word, shift, byte);
        builder.sstore(elem_slot, updated);
        builder.jump(done_block);

        builder.switch_to_block(done_block);
    }

    fn storage_byte_shift(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        index_in_word: ValueId,
    ) -> ValueId {
        let thirty_one = builder.imm_u64(31);
        let bytes_from_right = builder.sub(thirty_one, index_in_word);
        let eight = builder.imm_u64(8);
        builder.mul(bytes_from_right, eight)
    }

    fn replace_byte_in_word(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        word: ValueId,
        shift: ValueId,
        byte: ValueId,
    ) -> ValueId {
        let byte_mask = builder.imm_u64(0xff);
        let shifted_mask = builder.shl(shift, byte_mask);
        let keep_mask = builder.not(shifted_mask);
        let cleared = builder.and(word, keep_mask);
        let shifted_byte = builder.shl(shift, byte);
        builder.or(cleared, shifted_byte)
    }

    pub(super) fn lower_storage_bytes_method_call(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        method: Symbol,
        args: &CallArgs<'_>,
    ) -> ValueId {
        let current = self.materialize_storage_bytes(builder, slot);
        let len = builder.mload(current);
        match method {
            sym::push => {
                let one = builder.imm_u64(1);
                let new_len = builder.add(len, one);
                let overflow = builder.lt(new_len, len);
                self.emit_panic_if(builder, overflow, PanicCode::MemoryAllocationOverflow);

                let resized = self.resize_memory_bytes(builder, current, len, new_len);
                let byte = args
                    .exprs()
                    .next()
                    .map(|arg| {
                        let value = self.lower_expr(builder, arg);
                        self.bytes1_store_byte(builder, value)
                    })
                    .unwrap_or_else(|| builder.imm_u64(0));
                let word = builder.imm_u64(32);
                let data = builder.add(resized, word);
                let dst = builder.add(data, len);
                builder.mstore8(dst, byte);
                self.copy_memory_bytes_to_storage(builder, slot, resized);
            }
            kw::Pop => {
                self.emit_panic_if_zero(builder, len, PanicCode::PopEmptyArray);
                let one = builder.imm_u64(1);
                let new_len = builder.sub(len, one);
                let resized = self.resize_memory_bytes(builder, current, new_len, new_len);
                self.copy_memory_bytes_to_storage(builder, slot, resized);
            }
            _ => {}
        }
        builder.imm_u64(0)
    }

    pub(super) fn resize_memory_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        src: ValueId,
        copy_len: ValueId,
        new_len: ValueId,
    ) -> ValueId {
        let word = builder.imm_u64(32);
        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(new_len, thirty_one);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let zero = builder.imm_u64(0);
        let is_empty = builder.iszero(padded);
        let data_size = builder.select(is_empty, word, padded);
        let total = builder.add(word, data_size);
        let ptr = self.allocate_memory_dynamic(builder, total);
        builder.mstore(ptr, new_len);

        let data = builder.add(ptr, word);
        let last_word_off = builder.sub(data_size, word);
        let last_word = builder.add(data, last_word_off);
        builder.mstore(last_word, zero);

        let src_data = builder.add(src, word);
        self.mcopy(builder, data, src_data, copy_len, None);
        ptr
    }

    /// Whether an expression is a memory `bytes`/`string` value with the packed
    /// `[length][data...]` layout. Storage bytes identifiers materialize to a
    /// packed memory copy too, but have dedicated index paths and are excluded,
    /// as are calldata bytes (which lower to their ABI head).
    pub(super) fn is_memory_bytes_expr(&self, expr: &hir::Expr<'_>) -> bool {
        if !self.is_dynamic_bytes_expr(expr) {
            return false;
        }
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            // Storage bytes and calldata bytes have dedicated paths.
            return !self.storage_slots.contains_key(var_id)
                && self.gcx.hir.variable(*var_id).data_location
                    != Some(solar_ast::DataLocation::Calldata);
        }
        true
    }

    /// Whether an expression is a storage `bytes`/`string` state variable, whose value
    /// lowers to a packed `[length][data...]` memory copy.
    pub(super) fn is_storage_bytes_expr(&self, expr: &hir::Expr<'_>) -> bool {
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            return self.storage_slots.contains_key(var_id)
                && matches!(
                    var.ty.kind,
                    hir::TypeKind::Elementary(
                        hir::ElementaryType::Bytes | hir::ElementaryType::String
                    )
                );
        }
        false
    }

    /// Whether an expression is an lvalue of storage-located `bytes`/`string`
    /// type: a state variable, a storage-reference local, or a `bytes` field
    /// reached through one (e.g. `state.part` with `S storage state`). Unlike
    /// [`Self::is_storage_bytes_expr`], this covers member/index receivers and
    /// is meant to be paired with `lower_lvalue_slot`, which resolves the slot
    /// for exactly these shapes.
    pub(super) fn expr_is_storage_bytes_lvalue(&self, expr: &hir::Expr<'_>) -> bool {
        if self.is_storage_bytes_expr(expr) {
            return true;
        }
        let Some(ty) = self.get_expr_type(expr) else { return false };
        if let TyKind::Ref(inner, solar_ast::DataLocation::Storage) = ty.kind {
            return matches!(
                inner.kind,
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            );
        }
        false
    }

    /// Copies the returndata of the call that was just lowered into a fresh
    /// `bytes memory` allocation (`[length][data...]`) and returns the pointer.
    ///
    /// Must be emitted directly after the call instruction: the EVM return
    /// buffer is only invalidated by another external call, so reading it here
    /// is safe.
    pub(super) fn materialize_returndata_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
    ) -> ValueId {
        let size = builder.returndatasize();
        // total = 32 (length word) + ceil32(size), keeping the free memory
        // pointer word-aligned. With empty returndata this degenerates to a
        // 32-byte allocation holding a zero length.
        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(size, thirty_one);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let word = builder.imm_u64(32);
        let total = builder.add(padded, word);
        let ptr = self.allocate_memory_dynamic(builder, total);
        builder.mstore(ptr, size);
        let data_ptr = builder.add(ptr, word);
        let zero = builder.imm_u64(0);
        builder.returndatacopy(data_ptr, zero, size);
        ptr
    }

    /// Lowers a bytes argument to memory and returns (offset, size).
    /// Used for low-level calls: addr.call(data), addr.staticcall(data), addr.delegatecall(data).
    pub(super) fn lower_bytes_arg_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> (ValueId, ValueId) {
        // Handle literal strings/bytes: "" or hex"..."
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            let bytes = bytes.as_byte_str();
            let len = bytes.len();

            if len == 0 {
                // Empty bytes - no calldata
                return (builder.imm_u64(0), builder.imm_u64(0));
            }

            // Write the (left-aligned) bytes into a fresh allocation.
            let alloc_size = (len as u64).div_ceil(32) * 32;
            let ptr = self.allocate_memory(builder, alloc_size);
            for (i, chunk) in bytes.chunks(32).enumerate() {
                let mut padded = [0u8; 32];
                padded[..chunk.len()].copy_from_slice(chunk);
                let val = builder.imm_u256(U256::from_be_bytes(padded));
                let addr = if i == 0 {
                    ptr
                } else {
                    let offset_val = builder.imm_u64((i as u64) * 32);
                    builder.add(ptr, offset_val)
                };
                builder.mstore(addr, val);
            }

            return (ptr, builder.imm_u64(len as u64));
        }

        // Handle the abi.encode* family.
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Member(base, member) = &callee.kind
            && let ExprKind::Ident(res_slice) = &base.kind
            && let Some(hir::Res::Builtin(Builtin::Abi)) = res_slice.first()
        {
            match member.name {
                sym::encodePacked => {
                    // Returns a `bytes memory` pointer: `[length][data...]`.
                    let ptr = self.lower_abi_encode_packed(builder, args);
                    let word = builder.imm_u64(32);
                    let data = builder.add(ptr, word);
                    let len = builder.mload(ptr);
                    return (data, len);
                }
                sym::encode => {
                    let arg_exprs: Vec<_> = args.exprs().collect();
                    if let Some(payload) = self.abi_encode_call_payload(builder, None, &arg_exprs) {
                        return payload;
                    }
                }
                sym::encodeWithSelector => {
                    let mut exprs = args.exprs();
                    if let Some(selector_expr) = exprs.next() {
                        // `bytes4` values are left-aligned words.
                        let selector = self.lower_expr(builder, selector_expr);
                        let arg_exprs: Vec<_> = exprs.collect();
                        if let Some(payload) =
                            self.abi_encode_call_payload(builder, Some(selector), &arg_exprs)
                        {
                            return payload;
                        }
                    }
                }
                sym::encodeWithSignature => {
                    let mut exprs = args.exprs();
                    if let Some(sig_expr) = exprs.next()
                        && let ExprKind::Lit(lit) = &sig_expr.kind
                        && let LitKind::Str(_, sig, _) = &lit.kind
                    {
                        let hash = keccak256(sig.as_byte_str());
                        let selector =
                            U256::from(u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]]))
                                << 224;
                        let selector = builder.imm_u256(selector);
                        let arg_exprs: Vec<_> = exprs.collect();
                        if let Some(payload) =
                            self.abi_encode_call_payload(builder, Some(selector), &arg_exprs)
                        {
                            return payload;
                        }
                    }
                }
                _ => {}
            }

            let guar = self
                .gcx
                .dcx()
                .err(format!(
                    "codegen does not support `abi.{}` with these arguments as low-level call data yet",
                    member.name
                ))
                .span(expr.span)
                .emit();
            let err = builder.error_value(guar);
            return (err, err);
        }

        // A `bytes memory` value: `[length][data...]` pointer.
        if self.expr_yields_memory_bytes(expr) {
            let ptr = self.lower_expr(builder, expr);
            let word = builder.imm_u64(32);
            let data = builder.add(ptr, word);
            let len = builder.mload(ptr);
            return (data, len);
        }

        // A `bytes`/`string` calldata value: copy it into memory (a low-level
        // call reads its input from memory), then use that region. This arises in
        // proxy fallbacks such as `impl.delegatecall(data)` with `bytes calldata`.
        if self.expr_is_calldata_dynamic_bytes(expr) {
            let slice = self.lower_expr(builder, expr);
            let ptr = self.materialize_calldata_bytes(builder, slice);
            let word = builder.imm_u64(32);
            let len = builder.mload(ptr);
            let data = builder.add(ptr, word);
            return (data, len);
        }

        let guar = self
            .gcx
            .dcx()
            .err("codegen does not support this `bytes` expression as low-level call data yet")
            .span(expr.span)
            .emit();
        let err = builder.error_value(guar);
        (err, err)
    }

    /// Looks through a `bytes(x)` / `string(x)` conversion to the underlying
    /// value; returns `expr` unchanged otherwise.
    pub(super) fn peel_bytes_conversion<'b>(&self, expr: &'b hir::Expr<'b>) -> &'b hir::Expr<'b> {
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Type(ty) = &callee.kind
            && matches!(
                ty.kind,
                hir::TypeKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String)
            )
            && let Some(inner) = args.exprs().next()
        {
            return inner;
        }
        expr
    }

    /// Computes `keccak256` over the byte contents of a dynamic `bytes`/`string`
    /// expression, materializing calldata (and storage) values to memory first.
    /// This is what indexed event topics and `keccak256(bytes(s))` need: the
    /// hash of the raw data, never the pointer word. Returns `None` when `expr`
    /// is not a dynamic bytes/string value.
    pub(super) fn keccak_dynamic_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let inner = self.peel_bytes_conversion(expr);

        // String/bytes literal: hash the bytes at compile time.
        if let ExprKind::Lit(lit) = &inner.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            let hash = keccak256(bytes.as_byte_str());
            return Some(builder.imm_u256(U256::from_be_bytes(hash.0)));
        }

        if !self.expr_has_bytes_or_string_type(inner) {
            return None;
        }

        // Calldata `bytes`/`string`: copy the data into memory, then hash it
        // (`keccak256` only reads memory).
        if self.expr_is_calldata_dynamic_bytes(inner) {
            let slice = self.lower_expr(builder, inner);
            let ptr = self.materialize_calldata_bytes(builder, slice);
            let word = builder.imm_u64(32);
            let len = builder.mload(ptr);
            let data = builder.add(ptr, word);
            return Some(builder.keccak256(data, len));
        }

        // Memory and storage values lower to a memory `[length][data...]`
        // pointer; hash the data that follows the length word.
        let ptr = self.lower_expr(builder, inner);
        let word = builder.imm_u64(32);
        let len = builder.mload(ptr);
        let data = builder.add(ptr, word);
        Some(builder.keccak256(data, len))
    }

    /// Whether lowering `expr` yields a memory `bytes`/`string` pointer
    /// (`[length][data...]`).
    pub(super) fn expr_yields_memory_bytes(&self, expr: &hir::Expr<'_>) -> bool {
        // Calldata- and storage-located values lower to their ABI head or
        // storage slot, not to a memory pointer.
        if let ExprKind::Ident(res_slice) = &expr.kind
            && let Some(hir::Res::Item(hir::ItemId::Variable(var_id))) = res_slice.first()
        {
            let var = self.gcx.hir.variable(*var_id);
            if var.data_location != Some(solar_ast::DataLocation::Memory) {
                return false;
            }
        }
        let Some(ty) = self.get_expr_type(expr) else { return false };
        let TyKind::Ref(inner, solar_ast::DataLocation::Memory) = ty.kind else {
            return false;
        };
        matches!(inner.kind, TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String))
    }
}

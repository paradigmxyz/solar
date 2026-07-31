//! Bytes and string lowering helpers.

use super::{Lowerer, call::StorageArrayMethod, checked_arith::PanicCode};
use crate::mir::{FunctionBuilder, MemoryObjectKind, SliceLocation, ValueId};
use alloy_primitives::{U256, keccak256};
use solar_ast::LitKind;
use solar_interface::{diagnostics::ErrorGuaranteed, sym};
use solar_sema::{
    builtins::Builtin,
    hir::{self, CallArgs, ElementaryType, ExprKind},
    ty::{Ty, TyKind},
};

/// The ABI-encoded region an argument decode reads from. External calls read
/// calldata after the selector; constructors read the argument blob CODECOPY'd
/// into memory at the heap start.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum AbiSource {
    Calldata,
    Memory,
}

impl AbiSource {
    fn load(self, builder: &mut FunctionBuilder<'_>, pos: ValueId) -> ValueId {
        match self {
            Self::Calldata => builder.calldataload(pos),
            Self::Memory => builder.mload(pos),
        }
    }

    fn copy(self, builder: &mut FunctionBuilder<'_>, dst: ValueId, src: ValueId, len: ValueId) {
        match self {
            Self::Calldata => builder.calldatacopy(dst, src, len),
            Self::Memory => builder.mcopy(dst, src, len),
        }
    }
}

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
        let ptr =
            self.allocate_memory_object(builder, (32 + aligned) as u64, MemoryObjectKind::Bytes);
        let len_val = builder.imm_u64(len as u64);
        builder.set_memory_object_len(ptr, len_val, MemoryObjectKind::Bytes);

        let data_start = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
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
            let value = self.lower_value_expr(builder, expr);
            // A decoded calldata-struct member is already a memory bytes
            // pointer despite its calldata-located type.
            if Self::value_is_calldata_slice(builder, value) {
                return self.materialize_calldata_bytes(builder, value);
            }
            return value;
        }
        let value = self.lower_value_expr(builder, expr);
        self.coerce_memory_slice_value(builder, value)
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

        let ptr = self.allocate_memory_object_dynamic(builder, total_size, MemoryObjectKind::Bytes);
        builder.set_memory_object_len(ptr, len, MemoryObjectKind::Bytes);

        let data_ptr = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word_size);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        let data_pos = builder.slice_ptr(slice);
        builder.calldatacopy(data_ptr, data_pos, len);
        ptr
    }

    /// Whether a lowered value is a logical memory slice (an ABI-encode
    /// payload) rather than a `[length][data...]` bytes pointer.
    pub(super) fn value_is_memory_slice(builder: &FunctionBuilder<'_>, value: ValueId) -> bool {
        use crate::mir::{MirType, SliceLocation};
        matches!(builder.func().value_ty(value), Some(MirType::Slice(SliceLocation::Memory)))
    }

    /// Whether a lowered value is a logical calldata slice. A calldata-typed
    /// expression does not guarantee one: a member of a calldata struct that
    /// was decoded to memory in the prologue lowers to a memory bytes
    /// pointer.
    pub(super) fn value_is_calldata_slice(builder: &FunctionBuilder<'_>, value: ValueId) -> bool {
        use crate::mir::{MirType, SliceLocation};
        matches!(builder.func().value_ty(value), Some(MirType::Slice(SliceLocation::Calldata)))
    }

    /// Whether a lowered value is a `[length][data...]` dynamic-array memory
    /// object. A calldata-located dynamic array can lower to one — an element of
    /// a calldata array of arrays is rebuilt in memory — so the declared type
    /// does not settle whether the length header is there to skip.
    pub(super) fn value_is_dynamic_array_object(
        builder: &FunctionBuilder<'_>,
        value: ValueId,
    ) -> bool {
        use crate::mir::{MemoryObjectKind, MirType};
        matches!(
            builder.func().value_ty(value),
            Some(MirType::MemoryObject(MemoryObjectKind::DynamicArray))
        )
    }

    /// Adapts a logical memory slice to Solidity's `[length][data...]` memory
    /// bytes layout. ABI-encode payloads are memory slices; a `bytes memory`
    /// consumer needs a real length-prefixed object.
    pub(super) fn materialize_memory_slice_bytes(
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
        let total_size = builder.add(word_size, padded);
        let total_overflow = builder.lt(total_size, padded);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);

        let ptr = self.allocate_memory_object_dynamic(builder, total_size, MemoryObjectKind::Bytes);
        builder.mstore(ptr, len);
        let data_ptr = builder.add(ptr, word_size);
        let data_pos = builder.slice_ptr(slice);
        builder.mcopy(data_ptr, data_pos, len);
        ptr
    }

    /// Coerces a lowered value into a `bytes memory` consumer's shape: a
    /// logical memory slice materializes as a length-prefixed object, anything
    /// else already is one word.
    pub(super) fn coerce_memory_slice_value(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: ValueId,
    ) -> ValueId {
        if Self::value_is_memory_slice(builder, value) {
            return self.materialize_memory_slice_bytes(builder, value);
        }
        value
    }

    /// Coerces an argument to what the callee's parameter expects.
    ///
    /// A parameter declared `calldata` takes the slice as it is. Anything else
    /// wants a `[length][data...]` memory object, so a calldata slice — a
    /// `bytes calldata` value reaching a `bytes memory` parameter, which
    /// Solidity converts implicitly — is copied into memory here. Leaving it a
    /// slice makes it an aggregate use that slice lowering cannot fold, and the
    /// backend cannot emit.
    pub(super) fn coerce_arg_for_param(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        param_id: hir::VariableId,
        arg: &hir::Expr<'_>,
        value: ValueId,
    ) -> ValueId {
        let param = self.gcx.hir.variable(param_id);
        if Self::calldata_dynamic_var_kind(param).is_some() {
            // The callee's signature says slice. A `calldata` struct member
            // lowered to the rebuilt copy instead, which cannot serve one, so
            // read the member at its own calldata position.
            if !Self::value_is_calldata_slice(builder, value)
                && let Some(slice) = self.calldata_member_slice(builder, arg)
            {
                return slice;
            }
            return value;
        }
        if Self::value_is_calldata_slice(builder, value) {
            let ty = self.gcx.type_of_item(param_id.into());
            if matches!(ty.peel_refs().kind, TyKind::DynArray(_) | TyKind::Slice(_)) {
                return self.materialize_calldata_dyn_array_for_ty(builder, ty, value);
            }
            return self.materialize_calldata_bytes(builder, value);
        }
        self.coerce_memory_slice_value(builder, value)
    }

    /// Copies calldata bytes whose absolute length-word position is `len_pos`
    /// into Solidity's memory bytes layout.
    pub(super) fn materialize_calldata_bytes_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        len_pos: ValueId,
    ) -> ValueId {
        let len = source.load(builder, len_pos);

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

        let ptr = self.allocate_memory_object_dynamic(builder, total_size, MemoryObjectKind::Bytes);
        builder.mstore(ptr, len);

        let data_ptr = builder.add(ptr, word_size);
        let zero = builder.imm_u64(0);
        let last_word_offset = builder.sub(data_size, word_size);
        let last_word = builder.add(data_ptr, last_word_offset);
        builder.mstore(last_word, zero);

        let data_pos = builder.add(len_pos, word_size);
        source.copy(builder, data_ptr, data_pos, len);
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
        if let Some(var_id) = self.gcx.resolved_variable(lhs) {
            let var = self.gcx.hir.variable(var_id);
            if !var.is_struct_member() {
                return self.var_expects_memory_dyn_array_value(var);
            }
        }
        // A member or element target names no variable of its own; its type
        // says where it lives. A memory struct's array field assigned from a
        // `calldata` one needs the copy just as a local would.
        self.get_expr_type(lhs).is_some_and(|ty| {
            matches!(ty.kind, TyKind::Ref(inner, solar_ast::DataLocation::Memory)
                if matches!(inner.kind, TyKind::DynArray(_)))
        })
    }

    /// Lowers an expression whose consumer needs a MEMORY dynamic array: a
    /// calldata dynamic array materializes as a `[length][elems...]` copy;
    /// anything else lowers normally (it is already a memory pointer).
    pub(super) fn lower_expr_as_memory_dyn_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let Some((slice, false)) = self.calldata_bytes_source(builder, expr) {
            if let Some(ty) = self.get_expr_type(expr) {
                return self.materialize_calldata_dyn_array_for_ty(builder, ty, slice);
            }
            return self.materialize_calldata_dyn_array(builder, slice);
        }
        self.lower_value_expr(builder, expr)
    }

    /// Copies a single-word calldata array whose absolute length-word position
    /// is `len_pos` into memory.
    pub(super) fn materialize_calldata_word_array_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        len_pos: ValueId,
    ) -> ValueId {
        let len = source.load(builder, len_pos);
        let word_size = builder.imm_u64(32);
        let data_pos = builder.add(len_pos, word_size);
        self.copy_calldata_word_array(builder, source, data_pos, len)
    }

    /// Materializes a calldata dynamic-array slice into memory, rebuilding
    /// reference/aggregate elements so their memory slots hold memory
    /// pointers. Word elements copy verbatim.
    pub(super) fn materialize_calldata_dyn_array_for_ty(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ty: Ty<'gcx>,
        slice: ValueId,
    ) -> ValueId {
        if let TyKind::DynArray(elem) | TyKind::Slice(elem) = ty.peel_refs().kind
            && !self.abi_is_word_element(elem)
        {
            let data_pos = builder.slice_ptr(slice);
            let len = builder.slice_len(slice);
            return self.materialize_calldata_nested_array(
                builder,
                AbiSource::Calldata,
                elem,
                data_pos,
                len,
            );
        }
        self.materialize_calldata_dyn_array(builder, slice)
    }

    /// Copies a single-word calldata array SLICE into memory.
    pub(super) fn materialize_calldata_dyn_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slice: ValueId,
    ) -> ValueId {
        let len = builder.slice_len(slice);
        let data_pos = builder.slice_ptr(slice);
        self.copy_calldata_word_array(builder, AbiSource::Calldata, data_pos, len)
    }

    /// Copies `len` calldata words starting at `data_pos` into a fresh memory
    /// `[length][elems...]` array.
    fn copy_calldata_word_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
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

        let ptr = self.allocate_memory_object_dynamic(
            builder,
            total_size,
            MemoryObjectKind::DynamicArray,
        );
        builder.set_memory_object_len(ptr, len, MemoryObjectKind::DynamicArray);
        let data_ptr = builder.memory_object_data(ptr, MemoryObjectKind::DynamicArray);
        source.copy(builder, data_ptr, data_pos, byte_len);
        ptr
    }

    /// Materializes a calldata value whose ABI body starts at the absolute
    /// calldata position `pos`.
    pub(super) fn materialize_calldata_value_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        ty: Ty<'gcx>,
        pos: ValueId,
    ) -> ValueId {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                self.materialize_calldata_bytes_at(builder, source, pos)
            }
            TyKind::DynArray(elem) | TyKind::Slice(elem) => {
                self.materialize_calldata_dynamic_array_at(builder, source, elem, pos)
            }
            TyKind::Array(elem, len) => {
                let Ok(len) = u64::try_from(len) else {
                    return builder.error_value(self.abi_head_size_overflow());
                };
                self.materialize_calldata_fixed_array_at(builder, source, elem, len, pos)
            }
            TyKind::Struct(id) => {
                let fields = self.gcx.struct_field_types(id).to_vec();
                self.materialize_calldata_fields_at(builder, source, &fields, pos)
            }
            TyKind::Tuple(fields) => {
                self.materialize_calldata_fields_at(builder, source, fields, pos)
            }
            TyKind::Udvt(inner, _) => {
                self.materialize_calldata_value_at(builder, source, inner, pos)
            }
            _ => {
                // A value-typed leaf: solc reverts on a dirty narrow value
                // decoded from calldata, so validate before storing it.
                let word = source.load(builder, pos);
                self.emit_abi_field_clean_check(builder, ty, word);
                word
            }
        }
    }

    /// Materializes a dynamic calldata array. Arrays of ABI-word values can
    /// be copied directly; reference and aggregate elements are rebuilt one at
    /// a time so their memory slots contain memory pointers.
    fn materialize_calldata_dynamic_array_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        elem: Ty<'gcx>,
        len_pos: ValueId,
    ) -> ValueId {
        if self.abi_is_word_element(elem) {
            return self.materialize_calldata_word_array_at(builder, source, len_pos);
        }

        let len = source.load(builder, len_pos);
        let word = builder.imm_u64(32);
        let data_pos = builder.add(len_pos, word);
        self.materialize_calldata_nested_array(builder, source, elem, data_pos, len)
    }

    /// Materializes a calldata dynamic array of reference/aggregate elements
    /// from its data position and length: per the ABI, element offset words
    /// start at `data_pos` and are relative to it. Elements rebuild one at a
    /// time so their memory slots contain memory pointers.
    pub(super) fn materialize_calldata_nested_array(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        elem: Ty<'gcx>,
        data_pos: ValueId,
        len: ValueId,
    ) -> ValueId {
        let word = builder.imm_u64(32);
        let shift = builder.imm_u64(251);
        let too_big = builder.shr(shift, len);
        self.emit_panic_if(builder, too_big, PanicCode::MemoryAllocationOverflow);
        let byte_len = builder.mul(len, word);
        let total_size = builder.add(word, byte_len);
        let total_overflow = builder.lt(total_size, byte_len);
        self.emit_panic_if(builder, total_overflow, PanicCode::MemoryAllocationOverflow);

        let ptr = self.allocate_memory_object_dynamic(
            builder,
            total_size,
            MemoryObjectKind::DynamicArray,
        );
        builder.mstore(ptr, len);

        // Recursive materialization allocates memory and can introduce CFG, so
        // keep loop state in dedicated memory rather than MIR values.
        let scratch = self.allocate_memory(builder, 3 * 32);
        let remaining_slot = scratch;
        let source_slot = self.offset_ptr(builder, scratch, 32);
        let dest_slot = self.offset_ptr(builder, scratch, 64);
        let tuple_base = data_pos;
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
        let head_cursor = builder.mload(source_slot);
        let elem_pos = self.calldata_abi_value_pos(builder, source, elem, head_cursor, tuple_base);
        let value = self.materialize_calldata_value_at(builder, source, elem, elem_pos);
        let dest = builder.mload(dest_slot);
        builder.mstore(dest, value);

        let one = builder.imm_u64(1);
        let remaining = builder.mload(remaining_slot);
        let next_remaining = builder.sub(remaining, one);
        builder.mstore(remaining_slot, next_remaining);
        let head_cursor = builder.mload(source_slot);
        let elem_head_size = match self.abi_head_size(elem) {
            Ok(size) => builder.imm_u64(size),
            Err(guar) => return builder.error_value(guar),
        };
        let next_source = builder.add(head_cursor, elem_head_size);
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
        source: AbiSource,
        elem: Ty<'gcx>,
        len: u64,
        pos: ValueId,
    ) -> ValueId {
        // A field/element sema type carries the canonical `Ref(_, Storage)`
        // location; peel it so ABI head sizing does not mistake an inline
        // aggregate for a one-word storage slot.
        let elem = elem.peel_refs();
        let elem_head_size = match self.abi_head_size(elem) {
            Ok(size) if len.checked_mul(size).is_some() => size,
            Ok(_) => return builder.error_value(self.abi_head_size_overflow()),
            Err(guar) => return builder.error_value(guar),
        };
        let Some(size) = len.checked_mul(32) else {
            return builder.error_value(self.abi_head_size_overflow());
        };
        let ptr = self.allocate_memory(builder, size);
        let mut head_offset = 0;
        for i in 0..len {
            let head_pos = self.offset_ptr(builder, pos, head_offset);
            let elem_pos = self.calldata_abi_value_pos(builder, source, elem, head_pos, pos);
            let value = self.materialize_calldata_value_at(builder, source, elem, elem_pos);
            let dest = self.offset_ptr(builder, ptr, i * 32);
            builder.mstore(dest, value);
            head_offset += elem_head_size;
        }
        ptr
    }

    /// Materializes ABI tuple fields into Solidity's one-slot-per-field memory
    /// representation used for structs and tuples.
    fn materialize_calldata_fields_at(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        fields: &[Ty<'gcx>],
        pos: ValueId,
    ) -> ValueId {
        let word_count = fields.len() as u64;
        // A copy built from calldata keeps its source position in a trailing
        // word. Reads go through the copy, which is why it is rebuilt at all;
        // the position is only needed where the copy cannot stand in — a
        // `calldata`-located member reaching a `calldata` parameter, which
        // expects a slice and not an object. Keeping it in the copy means it
        // survives assignment, indexing and internal calls without any of them
        // knowing, where tracking it beside the value only ever covered the
        // expression forms someone remembered to enumerate.
        // See `calldata_base_of_copy`.
        let carries_base = source == AbiSource::Calldata
            && fields.iter().any(|&f| self.abi_is_dynamic(f.peel_refs()));
        if let Err(guar) = self.abi_head_size_sum(fields.iter().map(|&field| field.peel_refs())) {
            return builder.error_value(guar);
        }
        let Some(size) =
            word_count.checked_add(u64::from(carries_base)).and_then(|words| words.checked_mul(32))
        else {
            return builder.error_value(self.abi_head_size_overflow());
        };
        let ptr = self.allocate_memory(builder, size);
        let mut head_offset = 0;
        for (i, &field) in fields.iter().enumerate() {
            // A field sema type carries the canonical `Ref(_, Storage)`
            // location; peel it so ABI head sizing does not mistake an inline
            // aggregate field for a one-word storage slot.
            let field = field.peel_refs();
            let head_pos = self.offset_ptr(builder, pos, head_offset);
            let field_pos = self.calldata_abi_value_pos(builder, source, field, head_pos, pos);
            let value = self.materialize_calldata_value_at(builder, source, field, field_pos);
            let dest = self.offset_ptr(builder, ptr, (i as u64) * 32);
            builder.mstore(dest, value);
            let field_size = match self.abi_head_size(field) {
                Ok(size) => size,
                Err(guar) => return builder.error_value(guar),
            };
            head_offset += field_size;
        }
        if carries_base {
            let base_slot = self.offset_ptr(builder, ptr, word_count * 32);
            builder.mstore(base_slot, pos);
        }
        ptr
    }

    /// Loads the calldata position a struct copy was built from.
    ///
    /// Valid only for a copy of a `calldata` aggregate with a dynamic member,
    /// which [`Self::materialize_calldata_fields_at`] gives a trailing word
    /// holding the position. A `calldata`-located type is the proof: no other
    /// way of producing one exists in the language.
    pub(super) fn calldata_base_of_copy(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        ptr: ValueId,
        field_count: u64,
    ) -> ValueId {
        let base_slot = self.offset_ptr(builder, ptr, field_count * 32);
        builder.mload(base_slot)
    }

    /// Resolves an ABI head position to the corresponding value body. Dynamic
    /// offsets are relative to the containing tuple's head area.
    fn calldata_abi_value_pos(
        &self,
        builder: &mut FunctionBuilder<'_>,
        source: AbiSource,
        ty: Ty<'gcx>,
        head_pos: ValueId,
        tuple_base: ValueId,
    ) -> ValueId {
        if self.abi_is_dynamic(ty) {
            let offset = source.load(builder, head_pos);
            builder.add(tuple_base, offset)
        } else {
            head_pos
        }
    }

    pub(super) fn lhs_expects_memory_bytes_value(&self, lhs: &hir::Expr<'_>) -> bool {
        if let Some(var_id) = self.gcx.resolved_variable(lhs)
            && self.gcx.hir.variable(var_id).data_location
                == Some(solar_ast::DataLocation::Calldata)
        {
            return false;
        }
        if self.expr_has_bytes_or_string_type(lhs) {
            return true;
        }

        let Some(var_id) = self.gcx.resolved_variable(lhs) else {
            return false;
        };
        let var = self.gcx.hir.variable(var_id);
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
        builtin: Builtin,
        args: &CallArgs<'_>,
    ) -> Option<ValueId> {
        let method = match self.storage_array_method(builtin, args) {
            Ok(method) => method,
            Err(guar) => {
                return (builtin == Builtin::ArrayPush0).then(|| builder.error_value(guar));
            }
        };
        let current = self.materialize_storage_bytes(builder, slot);
        let len = builder.memory_object_len(current, MemoryObjectKind::Bytes);
        match method {
            StorageArrayMethod::PushDefault | StorageArrayMethod::Push(_) => {
                let one = builder.imm_u64(1);
                let new_len = builder.add(len, one);
                let overflow = builder.lt(new_len, len);
                self.emit_panic_if(builder, overflow, PanicCode::MemoryAllocationOverflow);

                let resized = self.resize_memory_bytes(builder, current, len, new_len);
                let byte = method
                    .argument()
                    .map(|arg| {
                        let value = self.lower_value_expr(builder, arg);
                        self.bytes1_store_byte(builder, value)
                    })
                    .unwrap_or_else(|| builder.imm_u64(0));
                let data = builder.memory_object_data(resized, MemoryObjectKind::Bytes);
                let dst = builder.add(data, len);
                builder.mstore8(dst, byte);
                self.copy_memory_bytes_to_storage(builder, slot, resized);
                // The storage reference returned by `push()` reads as the
                // newly zero-initialized byte when the call is used as an
                // rvalue. Reuse the value written above.
                return method.is_push_default().then_some(byte);
            }
            StorageArrayMethod::Pop => {
                self.emit_panic_if_zero(builder, len, PanicCode::PopEmptyArray);
                let one = builder.imm_u64(1);
                let new_len = builder.sub(len, one);
                let resized = self.resize_memory_bytes(builder, current, new_len, new_len);
                self.copy_memory_bytes_to_storage(builder, slot, resized);
            }
        }
        None
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
        let ptr = self.allocate_memory_object_dynamic(builder, total, MemoryObjectKind::Bytes);
        builder.set_memory_object_len(ptr, new_len, MemoryObjectKind::Bytes);

        let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        let last_word_off = builder.sub(data_size, word);
        let last_word = builder.add(data, last_word_off);
        builder.mstore(last_word, zero);

        let src_data = builder.memory_object_data(src, MemoryObjectKind::Bytes);
        builder.mcopy(data, src_data, copy_len);
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
        if let Some(var_id) = self.gcx.resolved_variable(expr) {
            // Storage bytes and calldata bytes have dedicated paths.
            return !self.storage_slots.contains_key(&var_id)
                && self.gcx.hir.variable(var_id).data_location
                    != Some(solar_ast::DataLocation::Calldata);
        }
        true
    }

    /// Whether an expression is a storage `bytes`/`string` state variable, whose value
    /// lowers to a packed `[length][data...]` memory copy.
    pub(super) fn is_storage_bytes_expr(&self, expr: &hir::Expr<'_>) -> bool {
        if let Some(var_id) = self.gcx.resolved_variable(expr) {
            let var = self.gcx.hir.variable(var_id);
            return self.storage_slots.contains_key(&var_id)
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
    /// The most recent external call's return data as a logical returndata
    /// slice covering the whole buffer, `(0, returndatasize)`.
    ///
    /// The buffer is volatile: any subsequent call, create, or low-level
    /// `.call` overwrites it. A returndata slice must therefore be consumed —
    /// materialized into memory via [`Self::materialize_returndata_slice`] —
    /// before any such instruction, and must not be retained across one.
    pub(super) fn returndata_slice(&mut self, builder: &mut FunctionBuilder<'_>) -> ValueId {
        let zero = builder.imm_u64(0);
        let size = builder.returndatasize();
        builder.make_slice(zero, size, SliceLocation::Returndata)
    }

    pub(super) fn materialize_returndata_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
    ) -> ValueId {
        let slice = self.returndata_slice(builder);
        self.materialize_returndata_slice(builder, slice)
    }

    /// Copies a returndata slice into a fresh `[length][data]` memory bytes
    /// object. `lower-slices` folds the slice's `(offset, len)` projections back
    /// to the underlying `returndatasize`/offset, so the emitted code is a
    /// single `returndatacopy` behind an aligned allocation.
    pub(super) fn materialize_returndata_slice(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slice: ValueId,
    ) -> ValueId {
        let offset = builder.slice_ptr(slice);
        let size = builder.slice_len(slice);
        // total = 32 (length word) + ceil32(size), keeping the free memory
        // pointer word-aligned. With empty returndata this degenerates to a
        // 32-byte allocation holding a zero length.
        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(size, thirty_one);
        let mask = builder.not(thirty_one);
        let padded = builder.and(rounded, mask);
        let word = builder.imm_u64(32);
        let total = builder.add(padded, word);
        let ptr = self.allocate_memory_object_dynamic(builder, total, MemoryObjectKind::Bytes);
        builder.set_memory_object_len(ptr, size, MemoryObjectKind::Bytes);
        let data_ptr = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        builder.returndatacopy(data_ptr, offset, size);
        ptr
    }

    /// Lowers a bytes argument to memory and returns (offset, size).
    /// Used for low-level calls: addr.call(data), addr.staticcall(data), addr.delegatecall(data).
    pub(super) fn lower_bytes_arg_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &hir::Expr<'_>,
    ) -> Result<(ValueId, ValueId), ErrorGuaranteed> {
        // Handle literal strings/bytes: "" or hex"..."
        if let ExprKind::Lit(lit) = &expr.kind
            && let LitKind::Str(_, bytes, _) = &lit.kind
        {
            let bytes = bytes.as_byte_str();
            let len = bytes.len();

            if len == 0 {
                // Empty bytes - no calldata
                return Ok((builder.imm_u64(0), builder.imm_u64(0)));
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

            return Ok((ptr, builder.imm_u64(len as u64)));
        }

        // Handle the abi.encode* family.
        if let ExprKind::Call(callee, args, _) = &expr.kind
            && let ExprKind::Member(base, member) = &callee.kind
            && self.gcx.resolved_builtin(base) == Some(Builtin::Abi)
        {
            match member.name {
                sym::encodePacked => {
                    // Returns a `bytes memory` pointer: `[length][data...]`.
                    let exprs = self.variadic_builtin_args(Builtin::AbiEncodePacked, args)?;
                    let ptr = self.lower_abi_encode_packed(builder, exprs)?;
                    let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
                    let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
                    return Ok((data, len));
                }
                sym::encode => {
                    let arg_exprs = self.variadic_builtin_args(Builtin::AbiEncode, args)?;
                    return self.abi_encode_call_payload(builder, None, arg_exprs.iter());
                }
                sym::encodeWithSelector => {
                    let ([selector], exprs) =
                        self.builtin_args_with_rest(Builtin::AbiEncodeWithSelector, args)?;
                    let selector = self.lower_selector_word(builder, selector);
                    return self.abi_encode_call_payload(builder, Some(selector), exprs.iter());
                }
                sym::encodeWithSignature => {
                    let ([signature], exprs) =
                        self.builtin_args_with_rest(Builtin::AbiEncodeWithSignature, args)?;
                    let selector = self.lower_signature_selector(builder, signature);
                    return self.abi_encode_call_payload(builder, Some(selector), exprs.iter());
                }
                sym::encodeCall => {
                    return self.abi_encode_call_from_args(builder, args);
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
            return Err(guar);
        }

        // A `bytes memory` value: `[length][data...]` pointer.
        if self.expr_yields_memory_bytes(expr) {
            let ptr = self.lower_value_expr(builder, expr);
            let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
            let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
            return Ok((data, len));
        }

        // A `bytes`/`string` calldata value: copy it into memory (a low-level
        // call reads its input from memory), then use that region. This arises in
        // proxy fallbacks such as `impl.delegatecall(data)` with `bytes calldata`.
        if self.expr_is_calldata_dynamic_bytes(expr) {
            let value = self.lower_value_expr(builder, expr);
            // A decoded calldata-struct member is already a memory bytes
            // pointer despite its calldata-located type.
            let ptr = if Self::value_is_calldata_slice(builder, value) {
                self.materialize_calldata_bytes(builder, value)
            } else {
                value
            };
            let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
            let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
            return Ok((data, len));
        }

        // A storage `bytes`/`string`: decode its short/long form into memory,
        // which a low-level call reads its input from. This arises in reentrancy
        // harnesses that stash the payload in storage and replay it.
        if self.is_storage_bytes_expr(expr)
            && let Some(slot) = self.lower_lvalue_slot(builder, expr)
        {
            let ptr = self.materialize_storage_bytes(builder, slot);
            let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
            let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
            return Ok((data, len));
        }

        let guar = self
            .gcx
            .dcx()
            .err("codegen does not support this `bytes` expression as low-level call data yet")
            .span(expr.span)
            .emit();
        Err(guar)
    }

    /// The left-aligned selector word for an `abi.encodeWithSignature`
    /// signature.
    ///
    /// A string literal hashes at compile time. A conditional between
    /// signatures resolves each side and selects between the two constants,
    /// which keeps the common `cond ? "f(uint256)" : "g(uint256)"` free of a
    /// runtime hash. Any other string is hashed at runtime and truncated to its
    /// leading four bytes.
    pub(super) fn lower_signature_selector(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        sig_expr: &hir::Expr<'_>,
    ) -> ValueId {
        if let ExprKind::Lit(lit) = &sig_expr.kind
            && let LitKind::Str(_, sig, _) = &lit.kind
        {
            let hash = keccak256(sig.as_byte_str());
            let selector =
                U256::from(u32::from_be_bytes([hash[0], hash[1], hash[2], hash[3]])) << 224;
            return builder.imm_u256(selector);
        }

        if let ExprKind::Ternary(cond, then_expr, else_expr) = &sig_expr.kind {
            let cond = self.lower_value_expr(builder, cond);
            let then_selector = self.lower_signature_selector(builder, then_expr);
            let else_selector = self.lower_signature_selector(builder, else_expr);
            return builder.select(cond, then_selector, else_selector);
        }

        // A signature only known at runtime: hash the string's bytes and keep
        // the leading four, which occupy the word's high bytes.
        let hash = match self.keccak_dynamic_bytes(builder, sig_expr) {
            Some(hash) => hash,
            None => {
                let ptr = self.lower_expr_as_memory_bytes(builder, sig_expr);
                builder.keccak256_bytes(ptr)
            }
        };
        let shift = builder.imm_u64(224);
        let truncated = builder.shr(shift, hash);
        builder.shl(shift, truncated)
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
            && let hir::CallArgsKind::Unnamed([inner]) = args.kind
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

        // Checked before the `bytes`/`string` type guard below, which does not
        // recognize a slice type (`b[a:c]`).
        //
        // Calldata `bytes`/`string`: copy the data into memory, then hash it
        // (`keccak256` only reads memory).
        if self.expr_is_calldata_dynamic_bytes(inner) {
            let slice = self.lower_value_expr(builder, inner);
            // Slicing a calldata struct's field yields a memory slice, because
            // the struct is decoded to memory in the prologue. Hash it where it
            // already is: copying it as calldata would read the wrong region.
            if Self::value_is_memory_slice(builder, slice) {
                let ptr = builder.slice_ptr(slice);
                let len = builder.slice_len(slice);
                return Some(builder.keccak256(ptr, len));
            }
            // The member itself, unsliced, is that memory copy: a bytes object,
            // not a slice at all. Hash it through the object reference.
            if !Self::value_is_calldata_slice(builder, slice) {
                return Some(builder.keccak256_bytes(slice));
            }
            let ptr = self.materialize_calldata_bytes(builder, slice);
            return Some(builder.keccak256_bytes(ptr));
        }

        if !self.expr_has_bytes_or_string_type(inner) {
            return None;
        }

        // Memory and storage values lower to a memory `[length][data...]`
        // object; hash its contents through the object reference, so the
        // optimizer sees one whole-object read instead of separate length and
        // data projections.
        let ptr = self.lower_value_expr(builder, inner);
        Some(builder.keccak256_bytes(ptr))
    }

    /// Whether lowering `expr` yields a memory `bytes`/`string` pointer
    /// (`[length][data...]`).
    pub(super) fn expr_yields_memory_bytes(&self, expr: &hir::Expr<'_>) -> bool {
        // Calldata- and storage-located declarations lower to their ABI head
        // or storage slot, not to a memory pointer. Struct fields inherit their
        // location from the surrounding expression type.
        if let Some(var_id) = self.gcx.resolved_variable(expr) {
            let var = self.gcx.hir.variable(var_id);
            if var.is_state_variable()
                || matches!(
                    var.data_location,
                    Some(solar_ast::DataLocation::Calldata | solar_ast::DataLocation::Storage)
                )
            {
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

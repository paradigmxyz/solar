//! ABI encoding helpers for source-level encodes, calls, errors, and events.

use super::{Lowerer, helpers::HelperKey};
use crate::mir::{AbiLayout, FunctionBuilder, MemoryObjectKind, MirType, ValueId};
use alloy_primitives::U256;
use solar_ast::ElementaryType;
use solar_data_structures::map::FxHashSet;
use solar_interface::diagnostics::ErrorGuaranteed;
use solar_sema::{
    builtins::Builtin,
    hir,
    ty::{Ty, TyKind},
};

struct LoweredAbiItems<'gcx> {
    items: Vec<(ValueId, Ty<'gcx>)>,
    calldata_slices: FxHashSet<ValueId>,
}

impl<'gcx> Lowerer<'gcx> {
    /// Returns `base + off`, avoiding a redundant add for the first item.
    pub(super) fn offset_ptr(
        &self,
        builder: &mut FunctionBuilder<'_>,
        base: ValueId,
        off: u64,
    ) -> ValueId {
        if off == 0 {
            base
        } else if builder.func().value_u256(base).is_some_and(|base| base.is_zero()) {
            builder.imm_u64(off)
        } else {
            let off = builder.imm_u64(off);
            builder.add(base, off)
        }
    }

    /// Emits ABI-encoded custom error data and terminates with `REVERT`.
    pub(super) fn emit_abi_error_revert(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        selector: [u8; 4],
        items: &[(ValueId, Ty<'gcx>)],
    ) {
        let Some(types) =
            items.iter().map(|&(_, ty)| self.abi_type(ty, false)).collect::<Option<Vec<_>>>()
        else {
            let guar = self.abi_type_error();
            let err = builder.error_value(guar);
            builder.revert(err, err);
            return;
        };
        let layout = self.module.intern_abi_layout(AbiLayout::new(types));
        let selector = U256::from(u32::from_be_bytes(selector)) << 224;
        let selector = builder.imm_u256(selector);
        let args: Vec<_> = items.iter().map(|&(value, _)| value).collect();
        let payload = builder.abi_encode(layout, Some(selector), args);
        let ptr = builder.slice_ptr(payload);
        let len = builder.slice_len(payload);
        builder.revert(ptr, len);
    }

    pub(super) fn abi_is_word_element(&self, ty: Ty<'gcx>) -> bool {
        match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => false,
            TyKind::Elementary(_) | TyKind::Enum(_) | TyKind::Contract(_) => true,
            TyKind::Udvt(inner, _) => self.abi_is_word_element(inner),
            _ => false,
        }
    }

    /// Allocates a shaped Solidity memory object with a dynamic byte size.
    pub(super) fn allocate_memory_object_dynamic(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        size: ValueId,
        kind: crate::mir::MemoryObjectKind,
    ) -> ValueId {
        let layout = match kind {
            crate::mir::MemoryObjectKind::Bytes => crate::mir::MemoryObjectLayout::Bytes,
            crate::mir::MemoryObjectKind::DynamicArray => {
                crate::mir::MemoryObjectLayout::DynamicArray { element_words: 1 }
            }
            crate::mir::MemoryObjectKind::FixedArray | crate::mir::MemoryObjectKind::Struct => {
                unreachable!("statically shaped objects require a constant allocation size")
            }
        };
        builder.alloc_object(size, layout, crate::mir::AllocationSemantics::INTERNAL)
    }

    /// Resolves each argument's ABI type and lowers it to a `(value, type)`
    /// item for the tuple encoder. Calldata bytes and word arrays stay as
    /// slices so the encoder can copy them directly into the destination.
    /// Arguments are evaluated before any output buffer is reserved: lowering
    /// an argument can allocate memory of its own.
    fn lower_abi_encode_items<'hir>(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_exprs: impl ExactSizeIterator<Item = &'hir hir::Expr<'hir>> + Clone,
    ) -> Result<LoweredAbiItems<'gcx>, ErrorGuaranteed> {
        let mut tys = Vec::with_capacity(arg_exprs.len());
        for arg in arg_exprs.clone() {
            let Some(ty) = self.get_expr_type(arg) else {
                return Err(self
                    .gcx
                    .dcx()
                    .err("codegen cannot determine this ABI argument's type")
                    .span(arg.span)
                    .emit());
            };
            // String literals encode as `string memory` values.
            let ty = match ty.peel_refs().kind {
                TyKind::StringLiteral(..) => self.gcx.types.string_ref.memory,
                _ => ty,
            };
            tys.push(ty);
        }
        let mut items = Vec::with_capacity(arg_exprs.len());
        let mut calldata_slices = FxHashSet::default();
        for (arg, ty) in arg_exprs.zip(tys) {
            let value = if let Some((slice, is_bytes)) = self.calldata_dyn_slice(builder, arg)
                && (is_bytes
                    || matches!(ty.peel_refs().kind, TyKind::DynArray(elem) if self.abi_is_word_element(elem)))
            {
                calldata_slices.insert(slice);
                slice
            } else if self.expr_is_calldata_dynamic_bytes(arg) {
                let value = self.lower_value_expr(builder, arg);
                // A decoded calldata-struct member is already a memory bytes
                // pointer despite its calldata-located type; only genuine
                // slices stay lazy in the payload.
                if Self::value_is_calldata_slice(builder, value) {
                    calldata_slices.insert(value);
                }
                value
            } else {
                self.coerce_value_for_type(builder, arg, ty)
            };
            items.push((value, ty));
        }
        Ok(LoweredAbiItems { items, calldata_slices })
    }

    /// Lowers `abi.encode(...)` to a fresh `bytes memory` object.
    pub(super) fn lower_abi_encode_to_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_exprs: &[hir::Expr<'_>],
    ) -> Result<ValueId, ErrorGuaranteed> {
        let LoweredAbiItems { items, calldata_slices } =
            self.lower_abi_encode_items(builder, arg_exprs.iter())?;
        let types = items
            .iter()
            .map(|&(value, ty)| self.abi_type(ty, calldata_slices.contains(&value)))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| self.abi_type_error())?;
        let layout = self.module.intern_abi_layout(AbiLayout::new(types));
        let args = items.into_iter().map(|(value, _)| value).collect::<Vec<_>>();
        let payload = builder.abi_encode(layout, None, args);
        Ok(self.materialize_memory_slice_bytes(builder, payload))
    }

    /// Lowers `keccak256(abi.encode(...))` through the typed ABI operation.
    pub(super) fn lower_keccak_abi_encode(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_exprs: &[hir::Expr<'_>],
    ) -> Result<ValueId, ErrorGuaranteed> {
        let LoweredAbiItems { items, calldata_slices } =
            self.lower_abi_encode_items(builder, arg_exprs.iter())?;
        let types = items
            .iter()
            .map(|&(value, ty)| self.abi_type(ty, calldata_slices.contains(&value)))
            .collect::<Option<Vec<_>>>()
            .ok_or_else(|| self.abi_type_error())?;
        let layout = self.module.intern_abi_layout(AbiLayout::new(types));
        let args = items.into_iter().map(|(value, _)| value).collect::<Vec<_>>();
        let payload = builder.abi_encode(layout, None, args);
        let data = builder.slice_ptr(payload);
        let size = builder.slice_len(payload);
        Ok(builder.keccak256(data, size))
    }

    /// ABI-encodes event data through the typed ABI operation.
    pub(super) fn abi_encode_event_data(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
    ) -> (ValueId, ValueId) {
        if items.is_empty() {
            let zero = builder.imm_u64(0);
            return (zero, zero);
        }

        // Events use the same tuple ABI as source `abi.encode`. Keep the
        // payload as a typed MIR slice so allocation and layout stay in the
        // ABI phase.
        if let Some(types) = items
            .iter()
            .map(|&(value, ty)| self.abi_type(ty, Self::value_is_calldata_slice(builder, value)))
            .collect::<Option<Vec<_>>>()
        {
            let layout = self.module.intern_abi_layout(AbiLayout::new(types));
            let args = items.iter().map(|&(value, _)| value).collect::<Vec<_>>();
            let payload = builder.abi_encode(layout, None, args);
            return (builder.slice_ptr(payload), builder.slice_len(payload));
        }

        // Recursive or otherwise unsupported ABI shapes must not be encoded
        // through a HIR-level scratch buffer. Bail at the phase boundary and
        // let the caller continue with an error sentinel.
        let data = builder.error_value(self.abi_type_error());
        let size = builder.imm_u64(0);
        (data, size)
    }

    /// Lowers `abi.encodeCall(F, (args...))` to a `(data, len)` payload: the
    /// function reference `F` supplies the 4-byte selector, and the second
    /// argument's tuple elements are ABI-encoded after it.
    pub(super) fn abi_encode_call_from_args(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        args: &hir::CallArgs<'_>,
    ) -> Result<(ValueId, ValueId), ErrorGuaranteed> {
        let [func_ref, args_tuple] = self.builtin_args(Builtin::AbiEncodeCall, args)?;
        let selector = self.lower_resolved_function_selector(func_ref).ok_or_else(|| {
            self.gcx
                .dcx()
                .err("codegen cannot resolve the `abi.encodeCall` function reference")
                .span(func_ref.span)
                .emit()
        })?;
        let selector_word = builder.imm_u256(U256::from(selector) << 224);
        let arg_exprs: Vec<_> = match &args_tuple.kind {
            hir::ExprKind::Tuple(elems) => elems.iter().filter_map(|e| *e).collect(),
            _ => vec![args_tuple],
        };
        self.abi_encode_call_payload(builder, Some(selector_word), arg_exprs.iter().copied())
    }

    /// ABI-encodes call arguments (optionally prefixed by a left-aligned
    /// 4-byte selector word) into a fresh allocation from the free memory
    /// pointer. Returns `(offset, size)` of the encoded payload.
    pub(super) fn abi_encode_call_payload<'hir>(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        selector: Option<ValueId>,
        arg_exprs: impl ExactSizeIterator<Item = &'hir hir::Expr<'hir>> + Clone,
    ) -> Result<(ValueId, ValueId), ErrorGuaranteed> {
        let LoweredAbiItems { items, calldata_slices } =
            self.lower_abi_encode_items(builder, arg_exprs.clone())?;
        let types = items
            .iter()
            .zip(arg_exprs)
            .map(|(&(value, ty), arg)| {
                self.abi_type(ty, calldata_slices.contains(&value)).ok_or_else(|| {
                    self.gcx
                        .dcx()
                        .err("codegen cannot encode this ABI argument's type")
                        .span(arg.span)
                        .emit()
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        let layout = self.module.intern_abi_layout(AbiLayout::new(types));
        let args: Vec<_> = items.into_iter().map(|(value, _)| value).collect();
        let payload = builder.abi_encode(layout, selector, args);
        let ptr = builder.slice_ptr(payload);
        let len = builder.slice_len(payload);
        Ok((ptr, len))
    }

    /// Decodes a storage `bytes`/`string` slot into the memory layout the ABI
    /// encoder expects (`[length][data...]`), through the module's shared
    /// `__load_storage_bytes` helper: the short/long-form decode and copy loop
    /// is far larger than a call, and real contracts read storage strings from
    /// several sites.
    pub(super) fn materialize_storage_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
    ) -> ValueId {
        if self.outlined_helpers.is_synthesizing(HelperKey::LoadStorageBytes) {
            return self.materialize_storage_bytes_inline(builder, slot);
        }
        let helper = self.ensure_load_storage_bytes_helper();
        builder.internal_call(
            helper,
            vec![slot],
            MirType::MemoryObject(crate::mir::MemoryObjectKind::Bytes),
            1,
        )
    }

    /// The out-of-line body of [`Self::materialize_storage_bytes`].
    pub(super) fn materialize_storage_bytes_inline(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
    ) -> ValueId {
        let word = builder.sload(slot);
        let one = builder.imm_u64(1);
        let long_bit = builder.and(word, one);
        let is_long = builder.eq(long_bit, one);

        let low_byte_mask = builder.imm_u64(0xff);
        let len_low = builder.and(word, low_byte_mask);
        let shift = builder.imm_u64(1);
        let short_len = builder.shr(shift, len_low);
        let long_len = builder.shr(shift, word);
        let len = builder.select(is_long, long_len, short_len);

        let word_size = builder.imm_u64(32);
        let thirty_one = builder.imm_u64(31);
        let padded_mask = builder.not(thirty_one);
        let len_plus_rounding = builder.add(len, thirty_one);
        let padded = builder.and(len_plus_rounding, padded_mask);
        let is_empty = builder.iszero(padded);
        let data_size = builder.select(is_empty, word_size, padded);
        let total_size = builder.add(word_size, data_size);

        let ptr = self.allocate_memory_object_dynamic(
            builder,
            total_size,
            crate::mir::MemoryObjectKind::Bytes,
        );
        builder.set_memory_object_len(ptr, len, MemoryObjectKind::Bytes);
        let data_ptr = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);

        let short_block = builder.create_block();
        let long_block = builder.create_block();
        let done_block = builder.create_block();
        builder.branch(is_long, long_block, short_block);

        builder.switch_to_block(short_block);
        let data_mask = builder.imm_u256(U256::MAX - U256::from(0xffu64));
        let data = builder.and(word, data_mask);
        builder.mstore(data_ptr, data);
        builder.jump(done_block);

        builder.switch_to_block(long_block);
        let remaining = builder.div(padded, word_size);
        let zero = builder.imm_u64(0);

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let preheader = builder.current_block();
        builder.jump(cond_block);

        builder.switch_to_block(cond_block);
        let remaining_phi = builder.phi(vec![(preheader, remaining)]);
        let index_phi = builder.phi(vec![(preheader, zero)]);
        let dst_phi = builder.phi(vec![(preheader, data_ptr)]);
        let has_remaining = builder.gt(remaining_phi, zero);
        builder.branch(has_remaining, body_block, done_block);

        builder.switch_to_block(body_block);
        let storage_slot = builder.storage_array_element_slot(slot, index_phi, 1);
        let data_word = builder.sload(storage_slot);
        builder.mstore(dst_phi, data_word);
        let word_size = builder.imm_u64(32);
        let next_dst = builder.add(dst_phi, word_size);
        let next_remaining = builder.sub(remaining_phi, one);
        let next_index = builder.add(index_phi, one);
        let latch = builder.current_block();
        builder.jump(cond_block);
        builder.add_phi_incoming(remaining_phi, latch, next_remaining);
        builder.add_phi_incoming(index_phi, latch, next_index);
        builder.add_phi_incoming(dst_phi, latch, next_dst);

        builder.switch_to_block(done_block);
        ptr
    }

    /// Encodes a memory `bytes`/`string` value (`[length][data...]` at `ptr`)
    /// into a storage `bytes`/`string` at `slot` using Solidity's short/long
    /// storage forms, then clears any leftover data slots from a previous
    /// longer value.
    pub(super) fn copy_memory_bytes_to_storage(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        slot: ValueId,
        ptr: ValueId,
    ) {
        let len = builder.memory_object_len(ptr, MemoryObjectKind::Bytes);
        let word_size = builder.imm_u64(32);
        let data = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);

        // Decode the previous value's data-word count so stale slots are cleared.
        let old_word = builder.sload(slot);
        let one = builder.imm_u64(1);
        let old_long_bit = builder.and(old_word, one);
        let old_is_long = builder.eq(old_long_bit, one);
        let low_byte_mask = builder.imm_u64(0xff);
        let old_len_low = builder.and(old_word, low_byte_mask);
        let shift_one = builder.imm_u64(1);
        let old_short_len = builder.shr(shift_one, old_len_low);
        let old_long_len = builder.shr(shift_one, old_word);
        let old_len = builder.select(old_is_long, old_long_len, old_short_len);
        let thirty_one = builder.imm_u64(31);
        let not_31 = builder.not(thirty_one);
        let old_len_round = builder.add(old_len, thirty_one);
        let old_padded = builder.and(old_len_round, not_31);
        let old_words_long = builder.div(old_padded, word_size);
        let zero = builder.imm_u64(0);
        let old_words = builder.select(old_is_long, old_words_long, zero);

        let new_len_round = builder.add(len, thirty_one);
        let new_padded = builder.and(new_len_round, not_31);
        let new_words_long = builder.div(new_padded, word_size);
        let is_long = builder.gt(len, thirty_one);
        let new_words = builder.select(is_long, new_words_long, zero);

        // Loop counters remain in a typed frame slot while storage words are
        // copied and cleared.
        let scratch = self.alloc_temp_frame_word();

        let short_block = builder.create_block();
        let long_block = builder.create_block();
        let copy_cond = builder.create_block();
        let copy_body = builder.create_block();
        let clear_init = builder.create_block();
        let clear_cond = builder.create_block();
        let clear_body = builder.create_block();
        let done_block = builder.create_block();

        builder.branch(is_long, long_block, short_block);

        // Short form: `data bytes | (len * 2)` packed into the main slot.
        // Mask the loaded word to exactly `len` bytes: memory past the value
        // is not guaranteed to be zero.
        builder.switch_to_block(short_block);
        let word = builder.mload(data);
        let eight = builder.imm_u64(8);
        let len_bits = builder.mul(len, eight);
        let all_ones = builder.imm_u256(U256::MAX);
        let low_mask = builder.shr(len_bits, all_ones);
        let keep_mask = builder.not(low_mask);
        let masked = builder.and(word, keep_mask);
        let len_twice_short = builder.shl(shift_one, len);
        let stored = builder.or(masked, len_twice_short);
        builder.sstore(slot, stored);
        builder.jump(clear_init);

        // Long form: `len * 2 + 1` in the main slot, data words at
        // `keccak256(slot) + i`.
        builder.switch_to_block(long_block);
        let len_twice_long = builder.shl(shift_one, len);
        let main_word = builder.or(len_twice_long, one);
        builder.sstore(slot, main_word);
        self.store_temp_frame_word(builder, scratch, zero);
        builder.jump(copy_cond);

        builder.switch_to_block(copy_cond);
        let i = self.load_temp_frame_word(builder, scratch);
        let more = builder.lt(i, new_words);
        builder.branch(more, copy_body, clear_init);

        builder.switch_to_block(copy_body);
        let i = self.load_temp_frame_word(builder, scratch);
        let dst = builder.storage_array_element_slot(slot, i, 1);
        let src_off = builder.mul(i, word_size);
        let src = builder.add(data, src_off);
        let data_word = builder.mload(src);
        builder.sstore(dst, data_word);
        let next_i = builder.add(i, one);
        self.store_temp_frame_word(builder, scratch, next_i);
        builder.jump(copy_cond);

        // Clear data slots `[new_words, old_words)` left over from a longer
        // previous value.
        builder.switch_to_block(clear_init);
        self.store_temp_frame_word(builder, scratch, new_words);
        builder.jump(clear_cond);

        builder.switch_to_block(clear_cond);
        let j = self.load_temp_frame_word(builder, scratch);
        let more_clear = builder.lt(j, old_words);
        builder.branch(more_clear, clear_body, done_block);

        builder.switch_to_block(clear_body);
        let j = self.load_temp_frame_word(builder, scratch);
        let clear_dst = builder.storage_array_element_slot(slot, j, 1);
        builder.sstore(clear_dst, zero);
        let next_j = builder.add(j, one);
        self.store_temp_frame_word(builder, scratch, next_j);
        builder.jump(clear_cond);

        builder.switch_to_block(done_block);
    }
}

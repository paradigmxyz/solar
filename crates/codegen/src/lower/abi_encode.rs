//! ABI encoding of external function return values.
//!
//! Driven by the cached sema [`Ty`] of each return,
//! this lays out the Solidity ABI tuple encoding (head slots + dynamic tail) for
//! a function's return values into a memory buffer and terminates the function
//! with [`crate::mir::Terminator::ReturnData`]. Internal-frame functions do NOT
//! use this — they return raw words/pointers through the internal frame.

use super::Lowerer;
use crate::{
    mir::{AbiLayout, AbiType, FunctionBuilder, MemoryObjectKind, MirType, SliceLocation, ValueId},
    transform::lower_abi_encode,
};
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
    pub(super) fn abi_type(&self, ty: Ty<'gcx>, calldata: bool) -> Option<AbiType> {
        let mut visiting = FxHashSet::default();
        self.abi_type_inner(ty, calldata, &mut visiting)
    }

    pub(super) fn abi_type_error(&self) -> ErrorGuaranteed {
        self.recovery_error(None, "codegen cannot materialize this ABI type")
    }

    fn abi_type_inner(
        &self,
        ty: Ty<'gcx>,
        calldata: bool,
        visiting: &mut FxHashSet<solar_sema::hir::StructId>,
    ) -> Option<AbiType> {
        if matches!(ty.kind, TyKind::Mapping(..) | TyKind::Ref(_, solar_ast::DataLocation::Storage))
        {
            return Some(AbiType::Word);
        }
        let location = if calldata { SliceLocation::Calldata } else { SliceLocation::Memory };
        Some(match ty.peel_refs().kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                AbiType::Bytes(location)
            }
            TyKind::DynArray(element) => AbiType::DynamicArray {
                element: Box::new(self.abi_type_inner(element, false, visiting)?),
                location,
            },
            TyKind::Slice(inner) => self.abi_type_inner(inner, calldata, visiting)?,
            TyKind::Array(element, len) => {
                let len = match u64::try_from(len) {
                    Ok(len) => len,
                    Err(_) => {
                        self.abi_head_size_overflow();
                        return None;
                    }
                };
                let element = Box::new(self.abi_type_inner(element, false, visiting)?);
                if len.checked_mul(element.head_size()).is_none() {
                    self.abi_head_size_overflow();
                    return None;
                }
                AbiType::FixedArray { element, len }
            }
            TyKind::Struct(id) => {
                if !visiting.insert(id) {
                    return None;
                }
                // A field's sema type may carry a storage location ref, but a
                // field of a memory/calldata aggregate is a value, never a
                // storage pointer: peel it so the top-level library
                // storage-parameter guard cannot collapse it to one word.
                let fields = self
                    .gcx
                    .struct_field_types(id)
                    .iter()
                    .map(|&field| self.abi_type_inner(field.peel_refs(), false, visiting))
                    .collect::<Option<Vec<_>>>()?;
                visiting.remove(&id);
                AbiType::Tuple(fields.into())
            }
            TyKind::Tuple(fields) => AbiType::Tuple(
                fields
                    .iter()
                    .map(|&field| self.abi_type_inner(field.peel_refs(), false, visiting))
                    .collect::<Option<Vec<_>>>()?
                    .into(),
            ),
            TyKind::Udvt(inner, _) => self.abi_type_inner(inner, calldata, visiting)?,
            _ => AbiType::Word,
        })
    }

    /// Whether `ty` is encoded dynamically (offset slot in the head + data in the
    /// tail): `bytes`/`string`, dynamic arrays, and any aggregate containing one.
    pub(super) fn abi_is_dynamic(&self, ty: Ty<'gcx>) -> bool {
        let ty = ty.peel_refs();
        match ty.kind {
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
            | TyKind::DynArray(_)
            | TyKind::Slice(_) => true,
            TyKind::Struct(id) => {
                ty.is_recursive(self.gcx)
                    || self.gcx.struct_field_types(id).iter().any(|&f| self.abi_is_dynamic(f))
            }
            TyKind::Array(elem, _) => self.abi_is_dynamic(elem),
            TyKind::Tuple(fields) => fields.iter().any(|&f| self.abi_is_dynamic(f)),
            TyKind::Udvt(inner, _) => self.abi_is_dynamic(inner),
            _ => false,
        }
    }

    /// Static ABI head size, in bytes, of one top-level item: 32 for any dynamic
    /// type (an offset slot), the recursive sum for a static struct, `N *
    /// head(T)` for a static `T[N]`, and 32 for every value type.
    ///
    /// Returns the existing diagnostic guarantee when the size exceeds `u64`.
    pub(super) fn abi_head_size(&self, ty: Ty<'gcx>) -> Result<u64, ErrorGuaranteed> {
        // A storage reference (a mapping, or a struct/array in storage — legal
        // for library function parameters) travels as its slot: one word.
        if matches!(ty.kind, TyKind::Mapping(..) | TyKind::Ref(_, solar_ast::DataLocation::Storage))
        {
            return Ok(32);
        }
        let ty = ty.peel_refs();
        if self.abi_is_dynamic(ty) {
            return Ok(32);
        }
        match ty.kind {
            TyKind::Array(elem, n) => {
                let len = u64::try_from(n).map_err(|_| self.abi_head_size_overflow())?;
                len.checked_mul(self.abi_head_size(elem)?)
                    .ok_or_else(|| self.abi_head_size_overflow())
            }
            TyKind::Struct(id) => {
                self.abi_head_size_sum(self.gcx.struct_field_types(id).iter().copied())
            }
            _ => Ok(32),
        }
    }

    pub(super) fn abi_head_size_sum(
        &self,
        tys: impl IntoIterator<Item = Ty<'gcx>>,
    ) -> Result<u64, ErrorGuaranteed> {
        tys.into_iter().try_fold(0u64, |size, ty| {
            size.checked_add(self.abi_head_size(ty)?).ok_or_else(|| self.abi_head_size_overflow())
        })
    }

    pub(super) fn abi_head_size_overflow(&self) -> ErrorGuaranteed {
        self.recovery_error(None, "ABI head size exceeds codegen limits")
    }

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

    /// Encodes a tuple of `(value, type)` items at `dest` using ABI head/tail
    /// layout. Static items are written inline in the head; dynamic items write a
    /// relative tail offset in the head and append their body to the shared tail.
    fn abi_encode_tuple(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
        dest: ValueId,
        calldata_slices: &FxHashSet<ValueId>,
        scratch: lower_abi_encode::AbiScratch,
    ) -> ValueId {
        let values: Vec<_> = items.iter().map(|&(value, _)| value).collect();
        let types = items
            .iter()
            .map(|&(value, ty)| self.abi_type(ty, calldata_slices.contains(&value)))
            .collect::<Option<Vec<_>>>();
        let Some(types) = types else {
            return builder.error_value(self.abi_type_error());
        };
        lower_abi_encode::encode_tuple(builder, &values, &types, dest, scratch)
    }

    /// Emits ABI-encoded custom error data and terminates with `REVERT`.
    pub(super) fn emit_abi_error_revert(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        selector: [u8; 4],
        items: &[(ValueId, Ty<'gcx>)],
    ) {
        let head_size = match self.abi_head_size_sum(items.iter().map(|&(_, ty)| ty)) {
            Ok(size) => size,
            Err(guar) => {
                let err = builder.error_value(guar);
                builder.revert(err, err);
                return;
            }
        };
        let scratch_words = self.abi_scratch_words(items);
        let scratch_base =
            (scratch_words > 0).then(|| self.allocate_memory(builder, scratch_words * 32));

        let buf = self.allocate_memory(builder, 4 + head_size);
        let selector = U256::from(u32::from_be_bytes(selector)) << 224;
        let selector = builder.imm_u256(selector);
        builder.mstore(buf, selector);

        let size = if items.is_empty() {
            builder.imm_u64(4)
        } else {
            let args_base = self.offset_ptr(builder, buf, 4);
            let calldata_slices = FxHashSet::default();
            let args_size = self.abi_encode_tuple(
                builder,
                items,
                args_base,
                &calldata_slices,
                lower_abi_encode::AbiScratch { base: scratch_base, depth: 0 },
            );
            let selector_size = builder.imm_u64(4);
            builder.add(args_size, selector_size)
        };
        builder.revert(buf, size);
    }

    fn abi_scratch_words(&self, items: &[(ValueId, Ty<'gcx>)]) -> u64 {
        items
            .iter()
            .map(|&(_, ty)| self.abi_type(ty, false).map_or(0, |ty| ty.loop_depth()))
            .max()
            .unwrap_or(0)
            * 5
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
                self.lower_return_value_for_ty(builder, arg, ty)
            };
            items.push((value, ty));
        }
        Ok(LoweredAbiItems { items, calldata_slices })
    }

    /// Lowers `abi.encode(...)` to a fresh `bytes memory` allocation
    /// (`[length][ABI tuple encoding]`) from the free memory pointer and
    /// returns the pointer.
    pub(super) fn lower_abi_encode_to_bytes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_exprs: &[hir::Expr<'_>],
    ) -> Result<ValueId, ErrorGuaranteed> {
        let LoweredAbiItems { items, calldata_slices } =
            self.lower_abi_encode_items(builder, arg_exprs.iter())?;
        let scratch_words = self.abi_scratch_words(&items);
        let scratch_base =
            (scratch_words > 0).then(|| self.allocate_memory(builder, scratch_words * 32));

        let ptr = builder.fmp_object(crate::mir::MemoryObjectLayout::Bytes);
        let word = builder.imm_u64(32);
        let dest = builder.memory_object_data(ptr, MemoryObjectKind::Bytes);
        let size = if items.is_empty() {
            builder.imm_u64(0)
        } else {
            self.abi_encode_tuple(
                builder,
                &items,
                dest,
                &calldata_slices,
                lower_abi_encode::AbiScratch { base: scratch_base, depth: 0 },
            )
        };
        builder.set_memory_object_len(ptr, size, MemoryObjectKind::Bytes);

        // Finalize the allocation: length word + encoded data. The tuple
        // encoder itself never allocates (its scratch is reserved above), so
        // nothing else writes into the buffer region before this bump. The
        // encoded size is always a multiple of 32, keeping the free memory
        // pointer word-aligned.
        let total = builder.add(size, word);
        let new_free_ptr = builder.add(ptr, total);
        builder.set_fmp(new_free_ptr);
        Ok(ptr)
    }

    /// Lowers `keccak256(abi.encode(...))` without materializing a `bytes`
    /// object: the tuple encoding is staged at the unbumped free memory
    /// pointer and hashed in place, like solc.
    pub(super) fn lower_keccak_abi_encode(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        arg_exprs: &[hir::Expr<'_>],
    ) -> Result<ValueId, ErrorGuaranteed> {
        let LoweredAbiItems { items, calldata_slices } =
            self.lower_abi_encode_items(builder, arg_exprs.iter())?;
        // Loop scratch must be a real allocation so it sits below the staging
        // area read by the hash.
        let scratch_words = self.abi_scratch_words(&items);
        let scratch_base =
            (scratch_words > 0).then(|| self.allocate_memory(builder, scratch_words * 32));

        let data = builder.fmp();
        let size = if items.is_empty() {
            builder.imm_u64(0)
        } else {
            self.abi_encode_tuple(
                builder,
                &items,
                data,
                &calldata_slices,
                lower_abi_encode::AbiScratch { base: scratch_base, depth: 0 },
            )
        };
        Ok(builder.keccak256(data, size))
    }

    /// ABI-encodes already-lowered tuple items into a fresh allocation from
    /// the free memory pointer and returns `(offset, size)`.
    pub(super) fn abi_encode_items_to_memory(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
    ) -> (ValueId, ValueId) {
        if items.is_empty() {
            let zero = builder.imm_u64(0);
            return (zero, zero);
        }

        let scratch_words = self.abi_scratch_words(items);
        let scratch_base =
            (scratch_words > 0).then(|| self.allocate_memory(builder, scratch_words * 32));

        let data = builder.fmp();
        let calldata_slices = FxHashSet::default();
        let size = self.abi_encode_tuple(
            builder,
            items,
            data,
            &calldata_slices,
            lower_abi_encode::AbiScratch { base: scratch_base, depth: 0 },
        );

        let thirty_one = builder.imm_u64(31);
        let rounded = builder.add(size, thirty_one);
        let mask = builder.not(thirty_one);
        let aligned = builder.and(rounded, mask);
        let new_free_ptr = builder.add(data, aligned);
        builder.set_fmp(new_free_ptr);

        (data, size)
    }

    /// ABI-encodes static event data in scratch memory.
    pub(super) fn abi_encode_event_data(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: &[(ValueId, Ty<'gcx>)],
    ) -> (ValueId, ValueId) {
        if items.is_empty() {
            let zero = builder.imm_u64(0);
            return (zero, zero);
        }
        if items.iter().any(|&(_, ty)| self.abi_is_dynamic(ty)) {
            return self.abi_encode_items_to_memory(builder, items);
        }

        let data = builder.imm_u64(0);
        let calldata_slices = FxHashSet::default();
        let size = self.abi_encode_tuple(
            builder,
            items,
            data,
            &calldata_slices,
            lower_abi_encode::AbiScratch { base: None, depth: 0 },
        );
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

    pub(super) fn lower_return_value_for_ty(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        expr: &solar_sema::hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> ValueId {
        if self.expr_is_calldata_dynamic_bytes(expr) {
            let value = self.lower_value_expr(builder, expr);
            if Self::value_is_calldata_slice(builder, value) {
                return self.materialize_calldata_bytes(builder, value);
            }
            // A decoded calldata-struct member is already a memory bytes
            // pointer despite its calldata-located type.
            return value;
        }
        if let Some((slice, is_bytes)) = self.calldata_bytes_source(builder, expr) {
            return if is_bytes {
                self.materialize_calldata_bytes(builder, slice)
            } else {
                self.materialize_calldata_dyn_array_for_ty(builder, ty, slice)
            };
        }
        if matches!(ty.kind, TyKind::Ref(_, solar_ast::DataLocation::Calldata)) {
            let value = self.lower_value_expr(builder, expr);
            if !Self::value_is_calldata_slice(builder, value) {
                return value;
            }
            return match ty.peel_refs().kind {
                TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                    self.materialize_calldata_bytes(builder, value)
                }
                TyKind::DynArray(_) | TyKind::Slice(_) => {
                    self.materialize_calldata_dyn_array_for_ty(builder, ty, value)
                }
                _ => value,
            };
        }
        if matches!(
            ty.peel_refs().kind,
            TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
        ) && let solar_sema::hir::ExprKind::Lit(lit) = &expr.kind
            && let Some(ptr) = self.lower_string_literal_to_memory(builder, lit)
        {
            return ptr;
        }
        let value = self.lower_value_expr(builder, expr);
        self.coerce_memory_slice_value(builder, value)
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
        if self.synthesizing_helper {
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

        let scratch_base = self.allocate_memory(builder, 32);
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
        builder.mstore(scratch_base, slot);
        let data_slot = builder.keccak256(scratch_base, word_size);
        let remaining = builder.div(padded, word_size);

        let cond_block = builder.create_block();
        let body_block = builder.create_block();
        let preheader = builder.current_block();
        builder.jump(cond_block);

        builder.switch_to_block(cond_block);
        let remaining_phi = builder.phi(vec![(preheader, remaining)]);
        let storage_slot_phi = builder.phi(vec![(preheader, data_slot)]);
        let dst_phi = builder.phi(vec![(preheader, data_ptr)]);
        let zero = builder.imm_u64(0);
        let has_remaining = builder.gt(remaining_phi, zero);
        builder.branch(has_remaining, body_block, done_block);

        builder.switch_to_block(body_block);
        let data_word = builder.sload(storage_slot_phi);
        builder.mstore(dst_phi, data_word);
        let next_storage_slot = builder.add(storage_slot_phi, one);
        let word_size = builder.imm_u64(32);
        let next_dst = builder.add(dst_phi, word_size);
        let next_remaining = builder.sub(remaining_phi, one);
        let latch = builder.current_block();
        builder.jump(cond_block);
        builder.add_phi_incoming(remaining_phi, latch, next_remaining);
        builder.add_phi_incoming(storage_slot_phi, latch, next_storage_slot);
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

        // Loop counter scratch; its first word also stages `slot` for the
        // data-slot keccak.
        let scratch = self.allocate_memory(builder, 32);
        builder.mstore(scratch, slot);
        let data_slot = builder.keccak256(scratch, word_size);

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
        builder.mstore(scratch, zero);
        builder.jump(copy_cond);

        builder.switch_to_block(copy_cond);
        let i = builder.mload(scratch);
        let more = builder.lt(i, new_words);
        builder.branch(more, copy_body, clear_init);

        builder.switch_to_block(copy_body);
        let i = builder.mload(scratch);
        let dst = builder.add(data_slot, i);
        let src_off = builder.mul(i, word_size);
        let src = builder.add(data, src_off);
        let data_word = builder.mload(src);
        builder.sstore(dst, data_word);
        let next_i = builder.add(i, one);
        builder.mstore(scratch, next_i);
        builder.jump(copy_cond);

        // Clear data slots `[new_words, old_words)` left over from a longer
        // previous value.
        builder.switch_to_block(clear_init);
        builder.mstore(scratch, new_words);
        builder.jump(clear_cond);

        builder.switch_to_block(clear_cond);
        let j = builder.mload(scratch);
        let more_clear = builder.lt(j, old_words);
        builder.branch(more_clear, clear_body, done_block);

        builder.switch_to_block(clear_body);
        let j = builder.mload(scratch);
        let clear_dst = builder.add(data_slot, j);
        builder.sstore(clear_dst, zero);
        let next_j = builder.add(j, one);
        builder.mstore(scratch, next_j);
        builder.jump(clear_cond);

        builder.switch_to_block(done_block);
    }

    /// Terminates the current function with the raw values gathered for its
    /// declared returns. `lower-abi` turns returns from external entries into
    /// ABI-encoded returndata; internal functions keep this convention.
    pub(super) fn finish_return(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        items: Vec<(ValueId, Ty<'gcx>)>,
    ) {
        let vals: Vec<ValueId> = items.into_iter().map(|(v, _)| v).collect();
        builder.ret(vals);
    }

    /// Gathers `(value, type)` for each declared return of an explicit `return`
    /// expression (tuple / ternary-tuple / single / multi-value call), pairing
    /// values with the function's declared return types.
    pub(super) fn gather_return_items(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        value: Option<&solar_sema::hir::Expr<'_>>,
    ) -> Vec<(ValueId, Ty<'gcx>)> {
        use solar_sema::hir::ExprKind;
        let tys = self.current_return_tys.clone();
        let Some(expr) = value else {
            return Vec::new();
        };
        // A return expression with no declared return types is malformed input
        // that upstream analysis already reported; do not index an empty list.
        if tys.is_empty() {
            return Vec::new();
        }
        if let ExprKind::Tuple(elements) = &expr.kind {
            return elements
                .iter()
                .flatten()
                .zip(tys)
                .map(|(e, ty)| (self.lower_return_value_for_ty(builder, e, ty), ty))
                .collect();
        }
        if let Some(arity) = self.get_ternary_tuple_arity(expr) {
            let first = self.lower_value_expr(builder, expr);
            let mut items = Vec::with_capacity(arity);
            items.push((first, tys[0]));
            if arity > 1 {
                let base = self.multi_return_buffer_base(builder);
                for (i, &ty) in tys.iter().enumerate().take(arity).skip(1) {
                    items.push((self.load_multi_return_value(builder, base, i), ty));
                }
            }
            return items;
        }
        let first = self.lower_return_value_for_ty(builder, expr, tys[0]);
        let mut items = vec![(first, tys[0])];
        if tys.len() > 1 {
            let tail_base = self.multi_return_buffer_base(builder);
            for (i, &ty) in tys.iter().enumerate().skip(1) {
                items.push((self.load_multi_return_value(builder, tail_base, i), ty));
            }
        }
        items
    }
}

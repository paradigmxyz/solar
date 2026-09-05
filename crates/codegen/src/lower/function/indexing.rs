//! Calldata and memory array indexing and range slices.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn memory_object_length(
        &mut self,
        object: ValueId,
        layout: MemoryObjectLayout,
    ) -> ValueId {
        match layout {
            MemoryObjectLayout::DynamicArray { .. } => {
                self.builder.memory_object_len(object, layout.kind())
            }
            MemoryObjectLayout::FixedArray { len, .. } => self.builder.imm(len),
            _ => unreachable!("array layout expected"),
        }
    }

    pub(super) fn array_element_and_length(
        &mut self,
        receiver_ty: Ty<'gcx>,
        object: ValueId,
        layout: MemoryObjectLayout,
    ) -> Option<(Ty<'gcx>, ValueId)> {
        match (receiver_ty.peel_refs().kind, layout) {
            (TyKind::DynArray(element), MemoryObjectLayout::DynamicArray { .. })
            | (TyKind::Array(element, _), MemoryObjectLayout::FixedArray { .. }) => {
                Some((element, self.memory_object_length(object, layout)))
            }
            _ => None,
        }
    }

    pub(super) fn array_element_type(&self, ty: Ty<'gcx>) -> Option<Ty<'gcx>> {
        match ty.peel_refs().kind {
            TyKind::DynArray(element) | TyKind::Array(element, _) => Some(element),
            TyKind::Slice(_) => ty.base_type(self.cx.gcx),
            _ => None,
        }
    }

    pub(super) fn lower_index(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        index: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        if let Some(place) = self.resolve_storage_byte_place(expr) {
            // value = load_storage_byte(place)
            return self.load_lvalue_place(&place);
        }
        if let Some(access) = self.storage_access(expr) {
            // value = load_storage(access)
            return self.load_storage_access(expr, access);
        }
        let Some(index) = index else {
            return self.cx.report_unsupported(expr.span, "index");
        };
        let index = self.lower_typed_expr(index, self.cx.gcx.types.uint(256))?;
        let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?;
        let object = self.lower_expr(receiver)?;
        if let TyKind::Elementary(solar_sema::hir::ElementaryType::FixedBytes(size)) =
            receiver_ty.peel_refs().kind
        {
            // bounds_check(index, width)
            // value = byte(index, receiver)
            let length = self.builder.imm(u64::from(size.bytes()));
            self.builder.bounds_check(index, length);
            let byte = self.builder.byte(index, object);
            return Some(self.normalize_byte_value(expr, byte));
        }
        if let Some(MirType::Slice(location)) = self.builder.func().value_ty(object) {
            // bounds_check(index, slice.length)
            let length = self.builder.slice_len(object);
            self.builder.bounds_check(index, length);
            let base = self.builder.slice_ptr(object);
            return if self.is_dynamic_bytes_type(receiver_ty) {
                // value = byte(0, load_word(slice, index))
                let word = match location {
                    SliceLocation::Calldata => self.builder.calldata_slice_load_word(object, index),
                    SliceLocation::Memory => self.builder.memory_slice_load_word(object, index),
                    SliceLocation::Returndata => {
                        return self.cx.report_unsupported(expr.span, "returndata index");
                    }
                };
                let zero = self.builder.imm(0);
                let byte = self.builder.byte(zero, word);
                Some(self.normalize_byte_value(expr, byte))
            } else {
                match receiver_ty.peel_refs().kind {
                    TyKind::DynArray(_) | TyKind::Array(_, _) | TyKind::Slice(_) => {
                        // head = slice.data + index * element_head_size
                        // value = decode_calldata_element(head, slice.data)
                        if location != SliceLocation::Calldata {
                            return self
                                .cx
                                .report_unsupported(expr.span, "memory array slice index");
                        }
                        let element = self
                            .cx
                            .gcx
                            .type_of_expr(expr.id)?
                            .with_loc_if_ref(self.cx.gcx, DataLocation::Calldata);
                        let head_size = self.builder.imm(self.types.abi_type(element)?.head_size());
                        let offset = self.builder.checked_mul(index, head_size);
                        let head = self.builder.add(base, offset);
                        // Dynamic-array slices retain their element base for nested offsets;
                        // arbitrary slices still cannot validate offsets relative to the tuple.
                        let validate_bounds =
                            !matches!(receiver.peel_parens().kind, ExprKind::Slice(..))
                                && self.types.abi_type(element)?.is_dynamic();
                        self.materialize_calldata_index_value_at(
                            element,
                            head,
                            base,
                            expr.span,
                            validate_bounds,
                        )
                    }
                    _ => self.cx.report_unsupported(expr.span, "slice index"),
                }
            };
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        match layout {
            MemoryObjectLayout::DynamicArray { .. } | MemoryObjectLayout::FixedArray { .. } => {
                // bounds_check(index, object.length)
                // value = load_element(object, index)
                let Some((element, length)) =
                    self.array_element_and_length(receiver_ty, object, layout)
                else {
                    return self.cx.report_unsupported(expr.span, "array index");
                };
                self.builder.bounds_check(index, length);
                let value = self.builder.memory_object_load_element(object, layout, index);
                if self.types.memory_layout(element).is_some() {
                    return self.materialize_array_element(object, layout, index, element, value);
                }
                Some(self.normalize_memory_scalar(element, value))
            }
            MemoryObjectLayout::Bytes => {
                // bounds_check(index, object.length)
                // value = load_byte(object, index)
                let length = self.builder.memory_object_len(object, layout.kind());
                self.builder.bounds_check(index, length);
                let value = self.builder.memory_object_load_byte(object, index);
                Some(self.normalize_byte_value(expr, value))
            }
            MemoryObjectLayout::Struct { .. } => {
                self.cx.report_unsupported(expr.span, "struct index")
            }
        }
    }

    pub(super) fn lower_slice(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        start: Option<&hir::Expr<'_>>,
        end: Option<&hir::Expr<'_>>,
    ) -> Option<ValueId> {
        let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?;
        let value = self.lower_expr(receiver)?;
        let (source, location) = match self.builder.func().value_ty(value) {
            Some(MirType::Slice(location)) => (value, location),
            _ => {
                let layout = self.types.memory_layout(receiver_ty)?;
                if layout != MemoryObjectLayout::Bytes {
                    return self.cx.report_unsupported(expr.span, "slice");
                }
                let length = self.builder.memory_object_len(value, MemoryObjectKind::Bytes);
                let pointer = self.builder.memory_object_data(value, MemoryObjectKind::Bytes);
                (
                    self.builder.make_slice(pointer, length, SliceLocation::Memory),
                    SliceLocation::Memory,
                )
            }
        };
        let is_bytes = self.is_dynamic_bytes_type(receiver_ty);
        let element_stride = if is_bytes {
            1
        } else if !matches!(receiver_ty.peel_refs().kind, TyKind::DynArray(_) | TyKind::Slice(_))
            || location != SliceLocation::Calldata
        {
            return self.cx.report_unsupported(expr.span, "slice");
        } else {
            let element = self.array_element_type(receiver_ty)?;
            // The semantic checker rejects range access on arrays with
            // dynamically encoded base types, matching solc, so only
            // statically encoded elements reach this point.
            self.types.abi_type(element)?.head_size()
        };
        let base_ptr = self.builder.slice_ptr(source);
        let base_len = self.builder.slice_len(source);
        // start = provided_start | 0
        // end = provided_end | base.length
        let start = if let Some(start) = start {
            self.lower_typed_expr(start, self.cx.gcx.types.uint(256))?
        } else {
            self.builder.imm(0)
        };
        let end = if let Some(end) = end {
            self.lower_typed_expr(end, self.cx.gcx.types.uint(256))?
        } else {
            base_len
        };
        let past_end = self.builder.gt(end, base_len);
        let backwards = self.builder.lt(end, start);
        if self.builder.encodes_revert_reasons() {
            // if end < start { revert(Error("Slice starts after end")) }
            // if end > base.length { revert(Error("Slice is greater than length")) }
            self.builder.revert_if(backwards, RevertReason::SliceStartsAfterEnd);
            self.builder.revert_if(past_end, RevertReason::SliceGreaterThanLength);
        } else {
            // if end > base.length || end < start { revert(0, 0) }
            let invalid = self.builder.or(past_end, backwards);
            self.builder.revert_if(invalid, RevertReason::Empty);
        }
        let length = self.builder.sub(end, start);
        let start_offset = if element_stride == 1 {
            start
        } else {
            let stride = self.builder.imm(element_stride);
            self.builder.checked_mul(start, stride)
        };
        // result = slice(base.data + start * stride, end - start)
        let pointer = self.builder.add(base_ptr, start_offset);
        Some(self.builder.make_slice(pointer, length, location))
    }
}

//! Literal, member, environment, and shared expression lowering.

use super::*;
use crate::mir::Immediate;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    pub(super) fn lower_string_literal_word(&mut self, bytes: &[u8]) -> ValueId {
        let len = bytes.len().min(32);
        let mut padded = [0_u8; 32];
        padded[..len].copy_from_slice(&bytes[..len]);
        self.builder.imm(U256::from_be_bytes(padded))
    }

    pub(super) fn lower_fixed_bytes_literal(
        &mut self,
        ty: Ty<'gcx>,
        expr: &hir::Expr<'_>,
    ) -> Option<ValueId> {
        let TyKind::Elementary(ElementaryType::FixedBytes(size)) = ty.peel_refs().kind else {
            return None;
        };
        let ExprKind::Lit(lit) = self.peel_bytes_conversion(expr).peel_parens().kind else {
            return None;
        };
        match &lit.kind {
            LitKind::Str(_, bytes, _) => Some(self.lower_string_literal_word(bytes.as_byte_str())),
            // value << (32 - N) * 8
            LitKind::Number(value) => {
                Some(self.builder.imm(*value << (usize::from(32 - size.bytes()) * 8)))
            }
            _ => None,
        }
    }

    pub(super) fn lower_literal(&mut self, kind: LitKind<'_>, span: Span) -> Option<ValueId> {
        match kind {
            LitKind::Str(_, value, _) => self.lower_shared_bytes_literal(value),
            LitKind::Number(value) => Some(self.builder.imm(value)),
            LitKind::Bool(value) => Some(self.builder.imm_bool(value)),
            LitKind::Address(value) => Some(self.builder.alloc_value(Value::Immediate(
                Immediate::for_type(Some(MirType::I160), U256::from_be_slice(value.as_slice())),
            ))),
            _ => self.cx.report_unsupported(span, "literal"),
        }
    }

    pub(super) fn lower_member(
        &mut self,
        expr: &hir::Expr<'_>,
        receiver: &hir::Expr<'_>,
        name: Ident,
    ) -> Option<ValueId> {
        if let Some(builtin) = self.cx.gcx.resolved_builtin(expr) {
            // value = lower_builtin(member)
            return self.lower_builtin_value(expr, builtin);
        }
        if let Some(value) = self.lower_internal_function_value(expr) {
            // value = internal_function_pointer(member)
            return Some(value);
        }
        if let Some(TyKind::Fn(function)) = self.cx.gcx.type_of_expr(expr.id).map(|ty| ty.kind)
            && function.is_external()
            && let Some(function_id) = self.cx.gcx.resolved_function(expr)
        {
            // value = address(receiver) << 32 | selector(function)
            let address = self.lower_expr(receiver)?;
            let address_shift = self.builder.imm(32);
            let address = self.builder.shl(address_shift, address);
            let selector = self.cx.gcx.function_selector(function_id).0;
            let selector = self.builder.imm(U256::from_be_slice(&selector));
            return Some(self.builder.or(address, selector));
        }
        if let Some(access) = self.storage_access(expr) {
            // value = load_storage(member_slot)
            return self.load_storage_access(expr, access);
        }
        // A declaration named `length` wins over the builtin member, the way
        // name resolution bound it: `enum E { length }` has `E.length`.
        if name.name == sym::length
            && self.cx.gcx.resolved_expr(expr).and_then(|resolved| resolved.as_variable()).is_none()
        {
            let receiver_ty = self.cx.gcx.type_of_expr(receiver.id)?;
            if matches!(
                receiver_ty.peel_refs().kind,
                TyKind::StringLiteral(..)
                    | TyKind::Elementary(
                        solar_sema::hir::ElementaryType::Bytes
                            | solar_sema::hir::ElementaryType::String,
                    )
            ) && let ExprKind::Lit(lit) = self.peel_bytes_conversion(receiver).peel_parens().kind
                && let LitKind::Str(_, bytes, _) = &lit.kind
            {
                return Some(self.builder.imm(bytes.as_byte_str().len() as u64));
            }
            return self.lower_array_length(receiver, receiver_ty, expr.span, "length member");
        }

        let resolved = self.cx.gcx.resolved_expr(expr)?;
        let id = resolved.as_variable()?;
        let variable = self.cx.gcx.hir.variable(id);
        if variable.is_constant() {
            return self.lower_constant_variable(id, expr.span);
        }
        if let Some(index) = resolved.enum_variant_index(&self.cx.gcx.hir) {
            return Some(self.builder.imm(index));
        }
        if variable.is_state_variable() {
            return self.load_variable(id, expr.span);
        }
        let Some(field) = resolved.struct_field_index(&self.cx.gcx.hir) else {
            return self.cx.report_unsupported(expr.span, "struct field");
        };
        let receiver_ty = self.type_of_expr_or_variable(receiver)?;
        let object = self.lower_expr(receiver)?;
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && self.builder.func().value_slice_location(object) == Some(SliceLocation::Calldata)
        {
            // head = receiver.ptr + field_offset
            // value = decode_calldata(field, head, receiver.ptr)
            let AbiType::Tuple(fields) = self.types.abi_type(receiver_ty)? else {
                return self.cx.report_unsupported(expr.span, "calldata struct field");
            };
            let offset =
                self.builder.imm(fields[..field].iter().map(AbiType::head_size).sum::<u64>());
            let base = self.builder.slice_ptr(object);
            let head = self.builder.add(base, offset);
            let field_ty = self
                .cx
                .gcx
                .type_of_item(id.into())
                .with_loc_if_ref(self.cx.gcx, DataLocation::Calldata);
            let validate_bounds = fields[field].is_dynamic();
            return self.materialize_calldata_value_at_inner(
                field_ty,
                head,
                base,
                expr.span,
                validate_bounds,
            );
        }

        // value = normalize(mload_field(object, field))
        let layout = self.types.memory_layout(receiver_ty)?;
        let field_ty =
            self.cx.gcx.type_of_item(id.into()).with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
        let value = self.builder.memory_object_load_field_as(
            object,
            layout,
            field as u64,
            types::TypeLowerer::mir_type(field_ty),
        );
        if receiver_ty.is_ref_at(DataLocation::Calldata)
            && let TyKind::Fn(function) = field_ty.peel_refs().kind
            && function.is_external()
        {
            // validate_abi field_value
            self.builder.validate_abi(value);
        }
        Some(self.normalize_memory_scalar(field_ty, value))
    }

    pub(super) fn lower_array_length(
        &mut self,
        receiver: &hir::Expr<'_>,
        receiver_ty: Ty<'gcx>,
        span: Span,
        what: &'static str,
    ) -> Option<ValueId> {
        if let TyKind::Array(_, len) = receiver_ty.peel_refs().kind {
            // length = static_len
            self.lower_discarded_expr(receiver)?;
            return Some(self.builder.imm(len));
        }
        if receiver_ty.is_ref_at(DataLocation::Storage) {
            if let Some(access) = self.storage_access(receiver) {
                return match receiver_ty.peel_refs().kind {
                    TyKind::DynArray(_) => {
                        // length = sload(slot)
                        Some(self.builder.sload(access.slot))
                    }
                    TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String) => {
                        // length = storage_bytes_length(slot)
                        Some(self.storage_bytes_length(access.slot))
                    }
                    _ => self.cx.report_unsupported(span, what),
                };
            }
            let object = self.lower_expr(receiver)?;
            return match self.builder.func().value_ty(object) {
                Some(MirType::MemPtr) => {
                    // length = object.len
                    Some(self.builder.memory_object_len(object, MemoryObjectKind::Bytes))
                }
                _ => self.cx.report_unsupported(span, what),
            };
        }
        let object = self.lower_expr(receiver)?;
        if matches!(self.builder.func().value_ty(object), Some(MirType::Slice(_))) {
            // length = slice.len
            return Some(self.builder.slice_len(object));
        }
        let layout = self.types.memory_layout(receiver_ty)?;
        match layout.kind() {
            MemoryObjectKind::Bytes | MemoryObjectKind::DynamicArray => {
                // length = object.len
                Some(self.builder.memory_object_len(object, layout.kind()))
            }
            _ => self.cx.report_unsupported(span, what),
        }
    }

    pub(super) fn type_of_expr_or_variable(&self, expr: &hir::Expr<'_>) -> Option<Ty<'gcx>> {
        self.cx.gcx.type_of_expr(expr.id).or_else(|| {
            self.cx.gcx.resolved_variable(expr).map(|id| self.cx.gcx.type_of_item(id.into()))
        })
    }

    pub(super) fn normalize_byte_value(&mut self, expr: &hir::Expr<'_>, value: ValueId) -> ValueId {
        let Some(ty) = self.cx.gcx.type_of_expr(expr.id) else { return value };
        self.normalize_byte_type(ty, value)
    }

    pub(super) fn normalize_byte_type(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        let TyKind::Elementary(ElementaryType::FixedBytes(size)) = ty.peel_refs().kind else {
            return value;
        };
        let shift = self.builder.imm(u64::from(32 - size.bytes()) * 8);
        self.builder.shl(shift, value)
    }

    pub(super) fn peel_bytes_conversion<'b>(&self, expr: &'b hir::Expr<'b>) -> &'b hir::Expr<'b> {
        let mut expr = expr;
        loop {
            expr = expr.peel_parens();
            if let Some((callee, args, _)) = expr.as_call()
                && let ExprKind::Type(ty) = &callee.kind
                && matches!(
                    ty.kind,
                    hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                )
                && let hir::CallArgsKind::Unnamed([inner]) = args.kind
            {
                expr = inner;
            } else {
                return expr;
            }
        }
    }

    /// Lowers a use of a constant to its value, which constant evaluation computes once.
    ///
    /// A constant without a value is an error at its first use, and an error value at every use.
    /// A value too large to fold, such as `new bytes(65537)`, is built from the initializer at
    /// every use instead, as solc does.
    pub(super) fn lower_constant_variable(
        &mut self,
        id: VariableId,
        span: Span,
    ) -> Option<ValueId> {
        if let Some(initializer) = self.unfolded_constant_initializer(id) {
            // value = <initializer>
            return self.lower_expr(initializer);
        }
        match self.cx.gcx.const_eval().require_var(id) {
            Ok(value) => self
                .lower_const_value(value, span)
                .or_else(|| self.cx.report_unsupported(span, "constant")),
            // value = error
            Err(guar) => Some(self.builder.error_value(guar)),
        }
    }

    /// Returns the initializer of a constant whose value is too large to fold, which uses build
    /// at runtime instead.
    pub(super) fn unfolded_constant_initializer(
        &self,
        id: VariableId,
    ) -> Option<&'gcx hir::Expr<'gcx>> {
        let gcx = self.cx.gcx;
        if let Err(err) = gcx.const_eval().eval_var(id)
            && matches!(err.kind, EvalErrorKind::ValueTooLarge)
        {
            gcx.hir.variable(id).initializer
        } else {
            None
        }
    }

    /// Lowers a constant value to an immediate, to shared memory for byte arrays, or to the
    /// bytecode of a contract.
    ///
    /// Integer literals wider than an EVM word and aggregates have no immediate.
    pub(super) fn lower_const_value(&mut self, value: &ConstValue, span: Span) -> Option<ValueId> {
        match value {
            // value = imm bool
            ConstValue::Bool(value) => Some(self.builder.imm_bool(*value)),
            // value = shared_bytes(data)
            ConstValue::Bytes(data, _) => self.lower_shared_bytes_literal(*data),
            // value = imm i160 address
            ConstValue::Address(_) | ConstValue::Contract(..) => Some(self.builder.alloc_value(
                Value::Immediate(Immediate::for_type(Some(MirType::I160), value.as_word()?)),
            )),
            ConstValue::Udvt(_, value) => self.lower_const_value(value, span),
            &ConstValue::Code(contract, kind) => {
                self.lower_contract_code(span, contract, kind.into())
            }
            // value = imm word
            value => Some(self.builder.imm(value.as_word()?)),
        }
    }
}

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

    /// Returns the bytes of a `string` or `bytes` value that the constant evaluator folds, such as
    /// a literal, a conversion of one, or a constant.
    pub(super) fn constant_bytes(&self, expr: &hir::Expr<'_>) -> Option<ByteSymbol> {
        match self.cx.gcx.const_eval().eval_expr(expr) {
            Ok(ConstValue::Bytes(bytes, _)) => Some(*bytes),
            _ => None,
        }
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
            let address = self.builder.shl_folded(address, address_shift);
            let selector = self.cx.gcx.function_selector(function_id).0;
            let selector = self.builder.imm(U256::from_be_slice(&selector));
            return Some(self.builder.or_folded(address, selector));
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
            && self.builder.func().value_slice_location(object) == Some(PtrLocation::Calldata)
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

        // Only a memory object has addressable fields.
        self.types.memory_layout(receiver_ty)?;
        // value = normalize(mload (ptradd object, field * 32))
        let field_ty =
            self.cx.gcx.type_of_item(id.into()).with_loc_if_ref(self.cx.gcx, DataLocation::Memory);
        let value = self.builder.memory_load_field_as(
            object,
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
                Some(MirType::MEMPTR) => {
                    // length = object.len
                    Some(self.builder.memory_len(object))
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
                Some(self.builder.memory_len(object))
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
        self.builder.shl_folded(value, shift)
    }

    pub(super) fn peel_bytes_conversion<'b>(&self, expr: &'b hir::Expr<'b>) -> &'b hir::Expr<'b> {
        self.peel_type_conversions(expr, |_, callee| {
            matches!(
                &callee.kind,
                ExprKind::Type(ty) if matches!(
                    ty.kind,
                    hir::TypeKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
                )
            )
        })
    }

    /// Peels parentheses and the single-argument type conversions that `peel` accepts. `peel`
    /// receives the conversion call and its callee.
    pub(super) fn peel_type_conversions<'b>(
        &self,
        mut expr: &'b hir::Expr<'b>,
        peel: impl Fn(&'b hir::Expr<'b>, &'b hir::Expr<'b>) -> bool,
    ) -> &'b hir::Expr<'b> {
        loop {
            expr = expr.peel_parens();
            if let Some((callee, args, _)) = expr.as_call()
                && let hir::CallArgsKind::Unnamed([inner]) = args.kind
                && peel(expr, callee)
                && is_type_conversion(self.cx.gcx, callee)
            {
                expr = inner;
            } else {
                return expr;
            }
        }
    }

    /// Lowers a use of a constant to its value, which constant evaluation computes once.
    ///
    /// A constant without a value at compile time, such as `keccak256(type(C).creationCode)` or
    /// `uint8(255) + 1`, which reverts, calls a helper that runs its initializer, like solc's IR
    /// codegen does.
    pub(super) fn lower_constant_variable(
        &mut self,
        id: VariableId,
        span: Span,
    ) -> Option<ValueId> {
        if let Ok(value) = self.cx.gcx.const_eval().eval_var(id)
            && let Some(value) = self.lower_const_value(value, span)
        {
            return Some(value);
        }
        self.call_constant_helper(id)
    }

    /// Calls the helper that computes the value of the constant `id` from its initializer.
    pub(super) fn call_constant_helper(&mut self, id: VariableId) -> Option<ValueId> {
        let gcx = self.cx.gcx;
        // The parser reports a constant without an initializer.
        let initializer =
            gcx.hir.variable(id).initializer.expect("constant without an initializer");
        let ty = gcx.type_of_item(id.into());
        let mir_ty = types::TypeLowerer::mir_type(ty);
        // The initializer runs in the arithmetic mode of the use, as in solc.
        let unchecked = self.unchecked;
        let suffix = if unchecked { "_unchecked" } else { "" };
        let name = helper_name(kw::Constant, format_args!("{}{suffix}", id.index()));
        // fn @constant_N[_unchecked]() -> T { value = <initializer>; ret value }
        let helper = self.lazy_helper(name, |this, function| {
            let mut lowerer = FunctionLowerer::new(this.cx.reborrow(), function);
            lowerer.unchecked = unchecked;
            lowerer.builder.replace_source_span(initializer.span);
            let value = lowerer.lower_typed_expr(initializer, ty)?;
            lowerer.builder.set_return_type(mir_ty);
            lowerer.builder.ret([value]);
            Some(())
        })?;
        // value = icall @constant_N[_unchecked]
        Some(self.builder.icall(helper, Vec::new(), mir_ty))
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
                let kind = match kind {
                    solar_sema::eval::CodeKind::Creation => crate::link::CodeKind::Creation,
                    solar_sema::eval::CodeKind::Runtime => crate::link::CodeKind::Runtime,
                };
                self.lower_contract_code(span, contract, kind)
            }
            // value = imm word
            value => Some(self.builder.imm(value.as_word()?)),
        }
    }
}

/// Returns whether calling `callee` converts its argument to another type.
pub(super) fn is_type_conversion(gcx: Gcx<'_>, callee: &hir::Expr<'_>) -> bool {
    matches!(callee.kind, ExprKind::TypeCall(_) | ExprKind::Type(_))
        || gcx.resolved_expr(callee).is_some_and(|res| {
            matches!(
                res,
                hir::Res::Item(
                    hir::ItemId::Contract(_) | hir::ItemId::Enum(_) | hir::ItemId::Udvt(_)
                )
            )
        })
        || gcx.type_of_expr(callee.id).is_some_and(|ty| matches!(ty.kind, TyKind::Type(_)))
}

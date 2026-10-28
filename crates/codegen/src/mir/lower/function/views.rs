//! `@custom:solar-view` declarations: `bytes` and `string` decoded from calldata, read in place.
//!
//! ```solidity
//! /// @custom:solar-view
//! (uint256 id, bytes memory payload) = abi.decode(data, (uint256, bytes));
//! ```
//!
//! Other compilers decode copies. This compiler binds each `bytes` and `string` variable of the
//! declaration, a view, to the calldata slice where the decode finds its bytes, which never
//! change. Type checking allows only reads of a view in place, so no use can tell it from the
//! copy (see `solar_sema`'s `typeck/solar_view.rs`). A read lowers from the slice as a read of
//! calldata `bytes` does: the length, an index, `keccak256` from calldata copied past the free
//! memory pointer, a decode of the slice, and the encodings, which copy their arguments from
//! calldata.
//!
//! The decode's result fields are calldata slices for the views and the usual words for value
//! types; ABI lowering validates each view as the copying decode validates the copy, and replays
//! the allocation checks the copies would make, so every input fails where it would. No memory is
//! copied. Functions that decode a view reserve the copies' space so nested allocation checks
//! account for earlier decodes. Other view decodes leave the free memory pointer unchanged.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Lowers the `@custom:solar-view` declaration of `ids`, initialized by `initializer`, an
    /// `abi.decode` of calldata `bytes` or of a view.
    pub(super) fn lower_view_declaration(
        &mut self,
        ids: &[Option<VariableId>],
        initializer: &hir::Expr<'_>,
    ) -> Option<()> {
        let Some((_, args, _)) = initializer.peel_parens().as_call() else {
            return self
                .cx
                .report_unsupported(initializer.span, "`@custom:solar-view` initializer");
        };
        let args = self.builtin_args::<2>(Builtin::AbiDecode, args)?;
        let types = match args[1].kind {
            ExprKind::Tuple(types) => types.iter().flatten().copied().collect::<Vec<_>>(),
            _ => vec![&args[1]],
        };
        let mut decoded_types = Vec::with_capacity(types.len());
        for ty_expr in &types {
            let Some(TyKind::Type(ty)) = self.cx.gcx.type_of_expr(ty_expr.id).map(|ty| ty.kind)
            else {
                return self.cx.report_unsupported(ty_expr.span, "abi.decode target type");
            };
            decoded_types.push(ty.with_loc_if_ref(self.cx.gcx, DataLocation::Memory));
        }
        // data = calldata slice of the input, or the slice a view holds
        let data = match self.view_slice(&args[0]) {
            Some(slice) => slice,
            None => self.lower_expr(&args[0])?,
        };
        let (data, layout) = self.lower_abi_decode_layout(data, &decoded_types, args[1].span)?;
        let layout = self.cx.module.intern_abi_param_layout(layout);
        // A `bytes` or `string` field is the calldata slice of its bytes.
        let fields = decoded_types
            .iter()
            .map(|&ty| {
                if is_bytes_or_string(ty) {
                    MirType::Slice(PtrLocation::Calldata)
                } else {
                    types::TypeLowerer::mir_type(ty)
                }
            })
            .collect();
        let Some(result_ty) = self.cx.module.intern_return_type(fields) else {
            return self.cx.report_unsupported(initializer.span, "`@custom:solar-view` decode");
        };
        // result = abi_decode(layout, data)
        // value_i = extract_value result, i
        let result = self.builder.abi_decode(layout, data, result_ty);
        let values = self.unpack_return_value(result);
        if values.len() != ids.len() {
            return self
                .cx
                .report_unsupported(initializer.span, "`@custom:solar-view` declaration");
        }
        for (&id, value) in ids.iter().zip(values) {
            let Some(id) = id else { continue };
            if self.builder.func().value_ty(value) == Some(MirType::Slice(PtrLocation::Calldata)) {
                self.views.insert(id);
                self.values.insert(id, value);
            } else {
                let span = self.cx.gcx.hir.variable(id).span;
                let value = self.materialize_scalar_carrier(id, value, span)?;
                self.values.insert(id, value);
            }
        }
        Some(())
    }

    /// Returns the calldata slice `expr` reads when it names a view, through parentheses and
    /// conversions between `bytes` and `string`.
    pub(super) fn view_slice(&self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let expr = self.peel_bytes_conversion(expr).peel_parens();
        let id = self.cx.gcx.resolved_variable(expr)?;
        if !self.views.contains(&id) {
            return None;
        }
        self.values.get(&id).copied()
    }
}

/// Whether `ty` is `bytes` or `string`.
fn is_bytes_or_string(ty: Ty<'_>) -> bool {
    matches!(
        ty.peel_refs().kind,
        TyKind::Elementary(ElementaryType::Bytes | ElementaryType::String)
    )
}

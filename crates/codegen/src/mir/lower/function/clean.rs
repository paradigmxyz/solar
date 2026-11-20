//! How Solidity values that inline assembly may have left with bits above their type are cleaned.
//!
//! A Solidity value is clean outside inline assembly: every integer, `bool`, address, enum and
//! fixed-bytes value is exactly the canonical value of its type. Only four places can see bits
//! above a type, and each cleans them once:
//!
//! - A Solidity read of a variable that inline assembly references. Such a variable is a *raw
//!   carrier*, found by `raw_scalars`, whose value is an `i256` word that assembly reads and writes
//!   as it is. [`FunctionLowerer::load_variable`] cleans it.
//! - A typed memory read, [`FunctionLowerer::normalize_memory_scalar`].
//! - The result of an internal call to a function whose return variable assembly mentions. That
//!   function returns the raw word, and [`FunctionLowerer::clean_call_result`] cleans it in the
//!   caller.
//! - An argument or a result of a call through an internal function pointer that assembly retyped.
//!   The dispatcher cleans it to the type that receives it.
//!
//! Everything else, such as copies, internal call arguments, tuple components, ternary arms and
//! operators, then carries clean values, so no value needs a flag that says whether it is clean.
//! Where `solc` via-IR keeps dirty bits across such a copy and cleans them at each consumer, the
//! compiler has already cleaned them, so assembly that reads a Solidity copy of a dirty variable
//! sees clean bits.

use super::*;

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Casts a canonical integer to the native MIR type of the expression's Solidity type.
    pub(super) fn cast_native(&mut self, value: ValueId, expr: &'gcx hir::Expr<'gcx>) -> ValueId {
        let Some(ty) = self.cx.gcx.type_of_expr(expr.id) else { return value };
        if self.in_inline_assembly || !ty.is_value_type() {
            return value;
        }
        match types::TypeLowerer::mir_type(ty) {
            native @ MirType::Int(_) => self.builder.cast(value, native),
            _ => value,
        }
    }

    /// Cleans a word to the canonical value of `ty`: an enum is range checked, a `bool` is tested
    /// against zero, an integer or address is cast to its native type, and a fixed-bytes value is
    /// masked.
    ///
    /// This is the one cleaning path. Variable and memory reads use it, and nothing else.
    fn normalize_abi_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Enum(..) => {
                self.validate_enum(ty, value);
                self.builder.cast(value, types::TypeLowerer::mir_type(ty))
            }
            TyKind::Elementary(ElementaryType::Bool) => self.builder.ne_zero(value),
            _ if matches!(
                types::TypeLowerer::value_layout(ty),
                crate::mir::ValueLayout::Int(_)
                    | crate::mir::ValueLayout::UInt(_)
                    | crate::mir::ValueLayout::Address
                    | crate::mir::ValueLayout::Function
            ) =>
            {
                self.builder.cast(value, types::TypeLowerer::mir_type(ty))
            }
            _ => AbiWordValidator::from_layout(types::TypeLowerer::value_layout(ty))
                .map_or(value, |validator| validator.cleanup(&mut self.builder, value)),
        }
    }

    /// Cleans a word read from memory for `ty`: inline assembly may have stored a dirty word.
    ///
    /// External functions are low-aligned as scalar MIR values, but Solidity memory stores their
    /// 24-byte representation left-aligned, so this also converts that. Use it at every typed
    /// memory read.
    pub(super) fn normalize_memory_scalar(&mut self, ty: Ty<'gcx>, value: ValueId) -> ValueId {
        if matches!(ty.peel_refs().kind, TyKind::Fn(function) if function.is_external()) {
            let shift = self.builder.imm(64);
            return self.builder.shr(value, shift);
        }
        self.normalize_abi_scalar(value, ty)
    }

    /// Binds the canonical value of a local `id`, which is the carrier word of a raw carrier.
    pub(super) fn bind_local(
        &mut self,
        id: VariableId,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        let ty = self.cx.gcx.type_of_item(id.into());
        let value = self.materialize_call_argument(ty, value, span)?;
        Some(self.materialize_raw_scalar(id, value))
    }

    /// Returns the value the return variable `id` hands to the caller. A raw carrier returns its
    /// word, which the caller cleans where it receives it.
    pub(super) fn returned_value(
        &mut self,
        id: VariableId,
        value: ValueId,
        span: Span,
    ) -> Option<ValueId> {
        if self.cx.state.is_raw_scalar(self.cx.gcx, id) {
            Some(self.materialize_raw_scalar(id, value))
        } else {
            let ty = self.cx.gcx.type_of_item(id.into());
            self.materialize_call_argument(ty, value, span)
        }
    }

    /// Cleans the result of an internal call to a function whose return variables are `returns`:
    /// a raw carrier returns a word, which becomes the canonical value of its type here.
    pub(super) fn clean_call_result(&mut self, returns: &[VariableId], result: ValueId) -> ValueId {
        let gcx = self.cx.gcx;
        let is_raw = |this: &Self, id| this.cx.state.is_raw_scalar(gcx, id);
        if self.in_inline_assembly || !returns.iter().any(|&id| is_raw(self, id)) {
            return result;
        }
        if let &[id] = returns {
            return self.normalize_abi_scalar(result, gcx.type_of_item(id.into()));
        }
        // fields = extract_value result; result = make_struct(clean(fields))
        let words = self.unpack_return_value(result);
        let values = returns
            .iter()
            .zip(words)
            .map(|(&id, word)| {
                if is_raw(self, id) {
                    self.normalize_abi_scalar(word, gcx.type_of_item(id.into()))
                } else {
                    word
                }
            })
            .collect::<Vec<_>>();
        let types =
            returns.iter().map(|&id| types::TypeLowerer::mir_type(gcx.type_of_item(id.into())));
        match self.cx.module.intern_return_type(types.collect()) {
            Some(MirType::Struct(target)) => self.builder.make_struct(target, values),
            _ => result,
        }
    }

    /// Reads a binding as a canonical value.
    ///
    /// Inline assembly reads the raw word, and so does a read that is not a Solidity one.
    pub(super) fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        let value = self.load_variable_word(id, span)?;
        if self.in_inline_assembly || !self.cx.state.is_raw_scalar(self.cx.gcx, id) {
            return Some(value);
        }
        let ty = self.cx.gcx.type_of_item(id.into());
        Some(self.normalize_abi_scalar(value, ty))
    }
}

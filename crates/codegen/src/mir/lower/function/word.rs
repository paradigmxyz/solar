//! How Solidity values that may carry bits above their type move through lowering.
//!
//! Inline assembly can leave bits above a variable's type, and `solc` via-IR keeps them across
//! copies, assignments, internal call, modifier and operator arguments, returns, tuple
//! components, ternary joins and equal-type conversions, then cleans them at every consumer. A
//! variable that assembly can reach, or that a raw value can be copied into, is a *raw carrier*:
//! the exposure analysis in `raw_scalars` finds them, and their values are `i256` words.
//!
//! [`FunctionLowerer::lower_expr`] is the only function that lowers an expression. It returns a
//! [`Word`]: the value at its carrier type, and whether it may hold such bits. Builder operations
//! take a `ValueId`, so a word reaches a consumer only through [`Word::clean`], which the
//! consumer asks for through [`FunctionLowerer::operand`], or through a sink that copies it into
//! a binding or a place and keeps the raw bits only when the destination is a raw carrier.

use super::*;

/// The value of an expression at its carrier type, and whether it may hold bits above its type.
#[must_use]
#[derive(Clone, Copy)]
pub(super) struct Word {
    value: ValueId,
    raw: bool,
}

impl Word {
    /// A canonical value at its native type.
    pub(super) fn from_clean(value: ValueId) -> Self {
        Self { value, raw: false }
    }

    /// A word that may hold bits above its type.
    ///
    /// Only three kinds of expression produce one: the read of a raw-carrier binding, a call or
    /// internal function pointer call whose return is a raw carrier, and a join or bitwise
    /// operation with a raw input.
    pub(super) fn raw(value: ValueId) -> Self {
        Self { value, raw: true }
    }

    /// Returns whether the word may hold bits above its type.
    pub(super) fn is_raw(self) -> bool {
        self.raw
    }

    /// Returns the canonical value of type `ty`, cleaning a raw word at the current span.
    ///
    /// Inline assembly reads raw words, so inside an assembly block this is the word itself.
    pub(super) fn clean<'gcx>(
        self,
        lowerer: &mut FunctionLowerer<'gcx, '_>,
        ty: Ty<'gcx>,
    ) -> ValueId {
        if self.raw && !lowerer.in_inline_assembly {
            lowerer.normalize_abi_scalar(self.value, ty)
        } else {
            self.value
        }
    }

    /// Returns the word as an operand of an instruction that ignores the bits above the type.
    ///
    /// Use it only for EVM address operands, which the EVM truncates, and for the index of a
    /// storage array or `bytes`, which via-IR checks as a raw word.
    pub(super) fn raw_operand(self) -> ValueId {
        self.value
    }

    /// Returns the word as the carrier word of a raw carrier, widening a canonical value of type
    /// `ty` with `zext` or `sext` as its signedness requires.
    fn carrier<'gcx>(self, lowerer: &mut FunctionLowerer<'gcx, '_>, ty: Ty<'gcx>) -> ValueId {
        if self.raw {
            return self.value;
        }
        let layout = types::TypeLowerer::value_layout(ty);
        raw_scalars::cast_carrier(&mut lowerer.builder, self.value, layout, MirType::I256)
    }

    /// Casts a canonical integer to the native MIR type of the expression's Solidity type.
    pub(super) fn into_native<'gcx>(
        self,
        lowerer: &mut FunctionLowerer<'gcx, '_>,
        expr: &hir::Expr<'_>,
    ) -> Self {
        let Some(ty) = lowerer.cx.gcx.type_of_expr(expr.id) else { return self };
        if lowerer.in_inline_assembly || !ty.is_value_type() || self.raw {
            return self;
        }
        let native = types::TypeLowerer::mir_type(ty);
        let value = match native {
            MirType::Int(_) => lowerer.builder.cast(self.value, native),
            _ => self.value,
        };
        // A raw word is the carrier of a narrower type; a canonical one never is.
        debug_assert!(
            !(lowerer.builder.func().value_ty(value) == Some(MirType::I256)
                && native.integer_bits().is_some_and(|bits| bits < 256)),
            "expression of a type narrower than a word lowered to a raw word: {:?}",
            expr.span
        );
        Self::from_clean(value)
    }
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Lowers `expr` as an operand: its canonical value at the native type of its Solidity type.
    ///
    /// The cleanup of a raw word follows the read under the span of the consumer that asked for
    /// it, as in solc: legacy `ExpressionCompiler::acceptAndConvert` (ExpressionCompiler.cpp:3087)
    /// converts after the operand's `LocationSetter` has popped, and via-IR
    /// `endVisit(FunctionCall)` (IRGeneratorForStatements.cpp:961) sets the call's location
    /// before it converts.
    pub(super) fn operand(&mut self, expr: &hir::Expr<'_>) -> Option<ValueId> {
        let word = self.lower_expr(expr)?;
        // Only an expression left untyped by a reported error has no type.
        Some(match self.cx.gcx.type_of_expr(expr.id) {
            Some(ty) => word.clean(self, ty),
            None => word.value,
        })
    }

    /// Lowers `lhs op rhs` for `&`, `|` and `^` on raw words, as via-IR does: it emits the
    /// operation on the operands without cleaning them. A canonical operand is widened first.
    pub(super) fn raw_bitwise(
        &mut self,
        kind: BinOpKind,
        lhs: Word,
        rhs: Word,
        ty: Ty<'gcx>,
    ) -> Word {
        let lhs = lhs.carrier(self, ty);
        let rhs = rhs.carrier(self, ty);
        // result = op lhs, rhs
        Word::raw(self.binary(kind, lhs, rhs, None))
    }

    /// Returns whether `lhs op rhs` keeps raw bits: both operands have the result type, or are
    /// literals that convert to it, and at least one has it.
    pub(super) fn forwards_bitwise(
        &self,
        lhs: &hir::Expr<'_>,
        rhs: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> bool {
        let is_literal = |expr: &hir::Expr<'_>| {
            self.cx.gcx.type_of_expr(expr.id).is_some_and(|ty| {
                matches!(ty.peel_refs().kind, TyKind::IntLiteral(..) | TyKind::StringLiteral(..))
            })
        };
        let has_ty = |expr: &hir::Expr<'_>| self.cx.gcx.type_of_expr(expr.id) == Some(ty);
        let (lhs_has_ty, rhs_has_ty) = (has_ty(lhs), has_ty(rhs));
        (arithmetic_kind(ty).is_some() || operators::fixed_bytes_width(ty).is_some())
            && (lhs_has_ty || is_literal(lhs))
            && (rhs_has_ty || is_literal(rhs))
            && (lhs_has_ty || rhs_has_ty)
    }

    /// Joins words that meet at a merge: a raw word if any input is raw, with the canonical ones
    /// widened in their own blocks.
    pub(super) fn join_words(&mut self, incoming: &[(BlockId, Word)], ty: Ty<'gcx>) -> Word {
        if incoming.windows(2).all(|pair| pair[0].1.value == pair[1].1.value) {
            return incoming[0].1;
        }
        if !incoming.iter().any(|(_, word)| word.raw) {
            let incoming = incoming.iter().map(|&(block, word)| (block, word.value)).collect();
            return Word::from_clean(self.merge_value_phi(incoming));
        }
        // word_i = widen value_i, in block_i
        // phi(words)
        let current = self.builder.current_block();
        let mut words = Vec::with_capacity(incoming.len());
        for &(block, word) in incoming {
            self.builder.switch_to_block(block);
            words.push((block, word.carrier(self, ty)));
        }
        self.builder.switch_to_block(current);
        Word::raw(self.merge_value_phi(words))
    }

    /// Cleans a word to the canonical value of `ty`: an enum is range checked, a `bool` is tested
    /// against zero, an integer or address is cast to its native type, and a fixed-bytes value is
    /// masked.
    ///
    /// This is the one cleaning path. [`Word::clean`] and the memory decoding entry points use
    /// it, and nothing else.
    fn normalize_abi_scalar(&mut self, value: ValueId, ty: Ty<'gcx>) -> ValueId {
        match ty.peel_refs().kind {
            TyKind::Enum(..) => {
                self.validate_enum(ty, value);
                self.builder.cast(value, types::TypeLowerer::mir_type(ty))
            }
            TyKind::Fn(_) => value,
            TyKind::Elementary(ElementaryType::Bool) => self.builder.ne_zero(value),
            _ if matches!(
                types::TypeLowerer::value_layout(ty),
                crate::mir::ValueLayout::Int(_)
                    | crate::mir::ValueLayout::UInt(_)
                    | crate::mir::ValueLayout::Address
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

    /// Returns whether the variable `id` is a raw carrier holding a scalar word.
    pub(super) fn is_raw_binding(&self, id: VariableId) -> bool {
        self.cx.state.raw_scalars.contains(&id)
            && types::TypeLowerer::mir_type(self.cx.gcx.type_of_item(id.into()))
                .integer_bits()
                .is_some()
    }

    /// Returns whether a result of type `ty` from an internal function pointer call is a raw word.
    pub(super) fn is_raw_pointer_return(&self, ty: Ty<'gcx>) -> bool {
        let carrier = self.cx.state.pointer_carrier(types::TypeLowerer::mir_type(ty));
        ty.is_value_type() && carrier == MirType::I256
    }

    /// Wraps `value`, which the binding `id` holds or an internal call returned in it: raw for a
    /// raw carrier.
    pub(super) fn binding_word(&self, value: ValueId, id: VariableId) -> Word {
        Word { value, raw: self.is_raw_binding(id) }
    }

    /// Wraps the result of type `ty` of an internal function pointer call.
    pub(super) fn pointer_word(&self, value: ValueId, ty: Ty<'gcx>) -> Word {
        Word { value, raw: self.is_raw_pointer_return(ty) }
    }

    /// Reads a binding as a word: raw for a raw carrier.
    pub(super) fn load_binding_word(&mut self, id: VariableId, span: Span) -> Option<Word> {
        let value = self.load_variable_word(id, span)?;
        Some(self.binding_word(value, id))
    }

    /// Reads the place as a word: only a raw-carrier variable is raw.
    pub(super) fn load_place_word(&mut self, place: &LValuePlace<'gcx>) -> Option<Word> {
        match *place {
            LValuePlace::Variable { id, span } => self.load_binding_word(id, span),
            _ => Some(Word::from_clean(self.load_lvalue_place(place)?)),
        }
    }

    /// Reads a binding as a canonical value.
    ///
    /// Inline assembly reads the raw word.
    pub(super) fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        let word = self.load_binding_word(id, span)?;
        let ty = self.cx.gcx.type_of_item(id.into());
        Some(word.clean(self, ty))
    }

    /// Binds `word` to the variable `id`, converted to the binding's carrier type.
    ///
    /// A raw-carrier binding keeps the raw bits; any other binding gets the canonical value.
    pub(super) fn bind_word(&mut self, id: VariableId, word: Word, span: Span) -> Option<ValueId> {
        let ty = self.cx.gcx.type_of_item(id.into());
        let carrier = self.cx.state.scalar_carrier(self.cx.gcx, id);
        if carrier.integer_bits().is_some() {
            Some(self.word_into_carrier(word, ty, carrier, self.is_raw_binding(id)))
        } else {
            let value = word.clean(self, ty);
            self.materialize_call_argument(ty, value, span)
        }
    }

    /// Copies `word` into a slot of the carrier type `carrier`, such as an internal function
    /// pointer dispatcher's parameter. `keep_raw` says whether the slot holds raw words.
    pub(super) fn word_into_carrier(
        &mut self,
        word: Word,
        ty: Ty<'gcx>,
        carrier: MirType,
        keep_raw: bool,
    ) -> ValueId {
        let value = if keep_raw { word.value } else { word.clean(self, ty) };
        let layout = types::TypeLowerer::value_layout(ty);
        raw_scalars::cast_carrier(&mut self.builder, value, layout, carrier)
    }

    /// Stores `word` into a place: a raw-carrier variable keeps the raw bits and any other place
    /// gets the canonical value of type `ty`.
    pub(super) fn store_word(
        &mut self,
        place: &LValuePlace<'gcx>,
        word: Word,
        source_ty: Option<Ty<'gcx>>,
        ty: Ty<'gcx>,
    ) -> Option<()> {
        match *place {
            LValuePlace::Variable { id, span } => {
                let value = if self.is_raw_binding(id) { word.value } else { word.clean(self, ty) };
                self.store_variable(id, value, span)
            }
            _ => {
                let value = word.clean(self, ty);
                self.store_lvalue_place_with_source(place, value, source_ty)
            }
        }
    }

    /// Converts `word` from `source` to `target`; a raw word of the same type passes through.
    pub(super) fn convert_word(&mut self, word: Word, source: Ty<'gcx>, target: Ty<'gcx>) -> Word {
        if word.raw && source == target {
            return word;
        }
        let value = word.clean(self, source);
        Word::from_clean(self.coerce_value(value, source, target))
    }
}

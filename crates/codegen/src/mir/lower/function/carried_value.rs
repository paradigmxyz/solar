//! How Solidity values that may carry bits above their type move through lowering.
//!
//! Inline assembly can leave bits above a variable's type, and `solc` via-IR keeps them across
//! copies, assignments, internal call, modifier and operator arguments, returns, tuple
//! components, ternary joins and equal-type conversions, then cleans them at every consumer. A
//! variable that assembly can reach, or that a raw value can be copied into, is a *raw carrier*:
//! the exposure analysis in `raw_scalars` finds them, and their values are `i256` words.
//!
//! A [`CarriedValue`] is a value in transit along such a copy edge: it holds either the raw word
//! of a raw carrier or a canonical value, and the type says it has not been cleaned yet. A
//! [`RawWord`] is a raw word handed to a consumer that ignores the bits above its type.
//!
//! The invariant: bits above a type exist only in raw-carrier bindings and in the [`CarriedValue`]
//! and [`RawWord`] values that carry them between bindings. Everything else is canonical at the
//! native MIR type:
//!
//! - [`FunctionLowerer::lower_expr`] always returns a canonical value. Reading a raw-carrier
//!   binding, or the result of a call whose return variable is a raw carrier, goes through the one
//!   normalizer.
//! - [`FunctionLowerer::lower_carried_expr`] returns a [`CarriedValue`] for a copy edge. Only the
//!   sinks in this module take it, and a sink keeps the raw bits only when its destination is a raw
//!   carrier.
//! - [`FunctionLowerer::lower_raw_expr`] returns a [`RawWord`] for the few consumers that ignore
//!   the bits above the type, or match via-IR on purpose.
//!
//! Neither type converts to a [`ValueId`]; they have no `Deref`, `From` or `Into`, and their
//! fields are private to this module.

use super::*;

/// A lowered value that may carry bits above its type.
///
/// It is the word of a raw-carrier binding or of a call result whose return variable is a raw
/// carrier, or a canonical value. Clean it with [`CarriedValue::clean`], or hand it to a sink that
/// copies it into a binding.
#[must_use]
#[derive(Clone, Copy)]
pub(super) struct CarriedValue {
    value: ValueId,
    raw: bool,
}

impl CarriedValue {
    /// A canonical value at its native type.
    pub(super) fn from_clean(value: ValueId) -> Self {
        Self { value, raw: false }
    }

    /// A word of a binding or call result that is raw when `raw` and canonical otherwise.
    pub(super) fn from_word(value: ValueId, raw: bool) -> Self {
        Self { value, raw }
    }

    /// Returns the value when it is canonical, and itself when it is a raw word.
    pub(super) fn into_clean(self) -> Result<ValueId, Self> {
        if self.raw { Err(self) } else { Ok(self.value) }
    }

    /// Returns the canonical value of type `ty`.
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
}

/// A word that a consumer reads without caring about the bits above its type.
///
/// Only the EVM operands that ignore those bits, and the operands that match via-IR on
/// purpose, take one: address operands and the index of a storage array.
#[must_use]
#[derive(Clone, Copy)]
pub(super) struct RawWord(ValueId);

impl RawWord {
    /// Returns the operand for an EVM instruction that ignores the bits above the type.
    pub(super) fn operand(self) -> ValueId {
        self.0
    }
}

impl<'gcx, 'ctx> FunctionLowerer<'gcx, 'ctx> {
    /// Cleans a word to the canonical value of `ty`: an enum is range checked, a `bool` is tested
    /// against zero, an integer or address is cast to its native type, and a fixed-bytes value is
    /// masked.
    ///
    /// This is the one cleaning path. [`CarriedValue::clean`] and the memory decoding entry points
    /// use it, and nothing else.
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

    /// Lowers `expr` along a copy edge, as `ty`, keeping the raw bits of a raw-carrier source.
    ///
    /// Use it only where the value is copied into a binding: a variable, an internal call, modifier
    /// or user-defined operator argument, a return, a tuple component or a state initializer.
    pub(super) fn lower_carried_expr(
        &mut self,
        expr: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<CarriedValue> {
        self.with_expr_context(expr, |this| this.lower_carried_expr_inner(expr, ty))
    }

    fn lower_carried_expr_inner(
        &mut self,
        expr: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<CarriedValue> {
        let source_ty = self.cx.gcx.type_of_expr(expr.id);
        match &expr.kind {
            ExprKind::Tuple([Some(inner)]) => return self.lower_carried_expr(inner, ty),
            ExprKind::Call(callee, args)
                if let hir::CallArgsKind::Unnamed([argument]) = args.kind
                    && expressions::is_type_conversion(self.cx.gcx, callee)
                    && source_ty.is_some()
                    && self.cx.gcx.type_of_expr(argument.id) == source_ty =>
            {
                return self.lower_carried_expr(argument, ty);
            }
            ExprKind::Ternary(condition, then_expr, else_expr)
                if source_ty.is_some_and(|ty| ty.is_value_type()) =>
            {
                return self.lower_carried_ternary(condition, then_expr, else_expr, ty);
            }
            _ => {}
        }
        let Some(source_ty) = source_ty.filter(|ty| ty.is_value_type()) else {
            return Some(CarriedValue::from_clean(self.lower_typed_expr(expr, ty)?));
        };
        let carried = match self.lower_carried_leaf(expr)? {
            Some(carried) => carried,
            None => match self.lower_carried_shape(expr, source_ty)? {
                Some(carried) => carried,
                None => return Some(CarriedValue::from_clean(self.lower_typed_expr(expr, ty)?)),
            },
        };
        if source_ty == ty {
            return Some(carried);
        }
        let value = carried.clean(self, source_ty);
        Some(CarriedValue::from_clean(self.coerce_value(value, source_ty, ty)))
    }

    /// Lowers `expr` as a word for a consumer that ignores the bits above its type.
    ///
    /// Use it for address operands (balance, code, codehash, low-level call targets, transfer
    /// and send, external call targets, `selfdestruct`) and for the index of a storage array.
    pub(super) fn lower_raw_expr(&mut self, expr: &hir::Expr<'_>) -> Option<RawWord> {
        let Some(ty) = self.cx.gcx.type_of_expr(expr.id) else {
            return Some(RawWord(self.lower_expr(expr)?));
        };
        let carried = self.lower_carried_expr(expr, ty)?;
        Some(RawWord(carried.value))
    }

    /// Lowers a leaf that can produce a raw word: a raw-carrier variable, an internal call, or a
    /// user-defined operator. Returns `Some(None)` for any other expression.
    pub(super) fn lower_carried_leaf(
        &mut self,
        expr: &hir::Expr<'_>,
    ) -> Option<Option<CarriedValue>> {
        match &expr.kind {
            ExprKind::Ident(_)
                if self.cx.gcx.resolved_builtin(expr).is_none()
                    && let Some(id) = self.cx.gcx.resolved_variable(expr)
                    && self.is_raw_binding(id) =>
            {
                Some(self.load_variable_carried(id, expr.span).map(Some)?)
            }
            ExprKind::Call(callee, args) if self.is_internal_call(callee) => {
                let (callee, call_opts) = callee.split_call_options();
                Some(match self.lower_call(expr, callee, *args, call_opts, true)? {
                    CallResult::Void => None,
                    CallResult::Value(value) => Some(CarriedValue::from_clean(value)),
                    CallResult::CarriedValue(carried) => Some(carried),
                })
            }
            ExprKind::Assign(lhs, None, rhs) => {
                Some(Some(self.lower_assignment(expr, lhs, None, rhs)?))
            }
            ExprKind::Binary(..) | ExprKind::Unary(..)
                if let Some(function_id) = self.cx.gcx.user_operator(expr.id) =>
            {
                Some(Some(self.lower_user_operator_expr(expr, function_id)?))
            }
            _ => Some(None),
        }
    }

    /// Lowers an expression that forwards the raw bits of its operands, as `solc` via-IR does:
    /// `payable(x)` of an `address payable`, a postfix `x++` or `x--`, `&&` and `||` on `bool`,
    /// and `&`, `|` and `^` of operands of the result type. Returns `Some(None)` for any other
    /// expression.
    fn lower_carried_shape(
        &mut self,
        expr: &hir::Expr<'_>,
        source_ty: Ty<'gcx>,
    ) -> Option<Option<CarriedValue>> {
        match &expr.kind {
            ExprKind::Payable(inner) if self.cx.gcx.type_of_expr(inner.id) == Some(source_ty) => {
                self.lower_carried_expr(inner, source_ty).map(Some)
            }
            ExprKind::Unary(op, operand)
                if matches!(op.kind, UnOpKind::PostInc | UnOpKind::PostDec) =>
            {
                self.lower_inc_dec(op.kind, operand).map(Some)
            }
            ExprKind::Binary(lhs, op, rhs)
                if op.kind.is_logical()
                    && matches!(source_ty.kind, TyKind::Elementary(ElementaryType::Bool)) =>
            {
                self.lower_carried_logical(lhs, op.kind, rhs, source_ty).map(Some)
            }
            ExprKind::Binary(lhs, op, rhs)
                if matches!(op.kind, BinOpKind::BitAnd | BinOpKind::BitOr | BinOpKind::BitXor)
                    && self.cx.gcx.user_operator(expr.id).is_none()
                    && !self.cx.gcx.unsupported_udvt_operator(expr.id)
                    && self.forwards_bitwise(lhs, rhs, source_ty) =>
            {
                self.lower_carried_bitwise(lhs, op.kind, rhs, source_ty).map(Some)
            }
            _ => Some(None),
        }
    }

    /// Returns whether `lhs op rhs` keeps raw bits: both operands have the result type, or are
    /// literals that convert to it, and at least one has it.
    fn forwards_bitwise(&self, lhs: &hir::Expr<'_>, rhs: &hir::Expr<'_>, ty: Ty<'gcx>) -> bool {
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

    /// Lowers `lhs op rhs` for `&`, `|` and `^`: the operation runs on the raw words when an
    /// operand is raw, because via-IR emits it without cleaning its operands.
    fn lower_carried_bitwise(
        &mut self,
        lhs: &hir::Expr<'_>,
        kind: BinOpKind,
        rhs: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<CarriedValue> {
        let operand = |this: &mut Self, expr: &hir::Expr<'_>| {
            if this.cx.gcx.type_of_expr(expr.id) == Some(ty) {
                this.lower_carried_expr(expr, ty)
            } else {
                Some(CarriedValue::from_clean(this.lower_typed_expr(expr, ty)?))
            }
        };
        let lhs = operand(self, lhs)?;
        let rhs = operand(self, rhs)?;
        if !lhs.raw && !rhs.raw {
            let value = self.binary(kind, lhs.value, rhs.value, Some(ty));
            return Some(CarriedValue::from_clean(match operators::fixed_bytes_width(ty) {
                Some(bytes) => self.clean_fixed_bytes(value, bytes),
                None => value,
            }));
        }
        let lhs = self.raw_word(lhs, ty);
        let rhs = self.raw_word(rhs, ty);
        // result = op lhs, rhs
        Some(CarriedValue::from_word(self.binary(kind, lhs, rhs, None), true))
    }

    /// Lowers `&&` and `||` along a copy edge: the result is the operand that decides it, raw
    /// bits included, as in via-IR.
    fn lower_carried_logical(
        &mut self,
        lhs: &hir::Expr<'_>,
        kind: BinOpKind,
        rhs: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<CarriedValue> {
        // if && { branch(lhs, rhs, false) }
        // if || { branch(lhs, lhs, rhs) }
        let lhs = self.lower_carried_expr(lhs, ty)?;
        let condition = lhs.clean(self, ty);
        let is_and = kind == BinOpKind::And;
        let short = |this: &mut Self| {
            if lhs.raw && !is_and {
                Some(lhs)
            } else {
                Some(CarriedValue::from_clean(this.builder.imm_bool(!is_and)))
            }
        };
        let (then_branch, else_branch) = self.lower_branches(
            condition,
            is_and,
            |this| if is_and { this.lower_carried_expr(rhs, ty) } else { short(this) },
            |this| if is_and { short(this) } else { this.lower_carried_expr(rhs, ty) },
        )?;
        let (rhs, short) =
            if is_and { (then_branch, else_branch) } else { (else_branch, then_branch) };
        if rhs.terminated || rhs.value.value == short.value.value {
            Some(short.value)
        } else {
            Some(self.merge_carried(&[(rhs.block, rhs.value), (short.block, short.value)], ty))
        }
    }

    /// Loads the place `place` without cleaning a raw-carrier variable.
    pub(super) fn load_lvalue_place_carried(
        &mut self,
        place: &LValuePlace<'gcx>,
    ) -> Option<CarriedValue> {
        match *place {
            LValuePlace::Variable { id, span } => self.load_variable_carried(id, span),
            _ => Some(CarriedValue::from_clean(self.load_lvalue_place(place)?)),
        }
    }

    /// Returns whether calling `callee` runs an internal function or an internal function pointer.
    fn is_internal_call(&self, callee: &hir::Expr<'_>) -> bool {
        let callee = callee.split_call_options().0;
        if self.cx.gcx.resolved_builtin(callee).is_some()
            || expressions::is_type_conversion(self.cx.gcx, callee)
            || matches!(callee.kind, ExprKind::New(_))
        {
            return false;
        }
        match self.cx.gcx.type_of_expr(callee.id).map(|ty| ty.kind) {
            Some(TyKind::Fn(function)) => function.is_internal() && !function.is_delegate_call(),
            _ => false,
        }
    }

    /// Lowers a ternary along a copy edge: raw branches keep their bits, so the join is a raw
    /// word when either branch is.
    fn lower_carried_ternary(
        &mut self,
        condition: &hir::Expr<'_>,
        then_expr: &hir::Expr<'_>,
        else_expr: &hir::Expr<'_>,
        ty: Ty<'gcx>,
    ) -> Option<CarriedValue> {
        let condition = self.lower_expr(condition)?;
        // branch(condition, then, else)
        // value = then | else | phi(then, else)
        let (then_branch, else_branch) = self.lower_branches(
            condition,
            true,
            |this| this.lower_carried_expr(then_expr, ty),
            |this| this.lower_carried_expr(else_expr, ty),
        )?;
        match (then_branch.terminated, else_branch.terminated) {
            (true, false) => Some(else_branch.value),
            (false, true) => Some(then_branch.value),
            _ if then_branch.value.value == else_branch.value.value => Some(then_branch.value),
            _ => Some(self.merge_carried(
                &[(then_branch.block, then_branch.value), (else_branch.block, else_branch.value)],
                ty,
            )),
        }
    }

    /// Returns `carried` as the word of a raw carrier, widening a canonical value of type `ty`.
    fn raw_word(&mut self, carried: CarriedValue, ty: Ty<'gcx>) -> ValueId {
        if carried.raw {
            return carried.value;
        }
        // word = zext or sext value
        let layout = types::TypeLowerer::value_layout(ty);
        raw_scalars::cast_carrier(&mut self.builder, carried.value, layout, MirType::I256)
    }

    /// Joins carried values: a raw word if any is raw, widening the others in their blocks.
    pub(super) fn merge_carried(
        &mut self,
        incoming: &[(BlockId, CarriedValue)],
        ty: Ty<'gcx>,
    ) -> CarriedValue {
        if incoming.windows(2).all(|pair| pair[0].1.value == pair[1].1.value) {
            return incoming[0].1;
        }
        if !incoming.iter().any(|(_, carried)| carried.raw) {
            let phi =
                self.merge_value_phi(incoming.iter().map(|&(block, c)| (block, c.value)).collect());
            return CarriedValue::from_clean(phi);
        }
        // word_i = widen value_i, in block_i
        // phi(words)
        let current = self.builder.current_block();
        let mut words = Vec::with_capacity(incoming.len());
        for &(block, carried) in incoming {
            self.builder.switch_to_block(block);
            words.push((block, self.raw_word(carried, ty)));
        }
        self.builder.switch_to_block(current);
        CarriedValue::from_word(self.merge_value_phi(words), true)
    }

    /// Returns whether a result of type `ty` from an internal function pointer call is a raw word.
    pub(super) fn is_raw_pointer_return(&self, ty: Ty<'gcx>) -> bool {
        let carrier = self.cx.state.pointer_carrier(types::TypeLowerer::mir_type(ty));
        ty.is_value_type() && carrier == MirType::I256
    }

    /// Reads a binding without cleaning it: the sink side of a copy edge.
    pub(super) fn load_variable_carried(
        &mut self,
        id: VariableId,
        span: Span,
    ) -> Option<CarriedValue> {
        let value = self.load_variable_word(id, span)?;
        Some(CarriedValue::from_word(value, self.is_raw_binding(id)))
    }

    /// Reads a binding as a canonical value.
    ///
    /// Inline assembly reads the raw word.
    pub(super) fn load_variable(&mut self, id: VariableId, span: Span) -> Option<ValueId> {
        let carried = self.load_variable_carried(id, span)?;
        let ty = self.cx.gcx.type_of_item(id.into());
        Some(carried.clean(self, ty))
    }

    /// Returns what the binding `id` holds for `carried`: the raw word in a raw carrier and the
    /// canonical value in any other binding.
    pub(super) fn carried_into_binding(
        &mut self,
        id: VariableId,
        carried: CarriedValue,
    ) -> ValueId {
        if self.is_raw_binding(id) {
            carried.value
        } else {
            let ty = self.cx.gcx.type_of_item(id.into());
            carried.clean(self, ty)
        }
    }

    /// Copies `carried` into the binding `id`, converting it to the binding's carrier type.
    ///
    /// A raw-carrier binding keeps the raw bits; any other binding gets the canonical value.
    pub(super) fn materialize_scalar_carrier(
        &mut self,
        id: VariableId,
        carried: CarriedValue,
        span: Span,
    ) -> Option<ValueId> {
        let ty = self.cx.gcx.type_of_item(id.into());
        let carrier = self.cx.state.scalar_carrier(self.cx.gcx, id);
        if carrier.integer_bits().is_some() {
            Some(self.carried_into_carrier(carried, ty, carrier, self.is_raw_binding(id)))
        } else {
            let value = carried.clean(self, ty);
            self.materialize_call_argument(ty, value, span)
        }
    }

    /// Copies `carried` into a slot of the carrier type `carrier`, such as an internal function
    /// pointer dispatcher's parameter. `keep_raw` says whether the slot holds raw words.
    pub(super) fn carried_into_carrier(
        &mut self,
        carried: CarriedValue,
        ty: Ty<'gcx>,
        carrier: MirType,
        keep_raw: bool,
    ) -> ValueId {
        let value = if keep_raw { carried.value } else { carried.clean(self, ty) };
        raw_scalars::cast_carrier(
            &mut self.builder,
            value,
            types::TypeLowerer::value_layout(ty),
            carrier,
        )
    }

    /// Stores `carried` into a place: a raw-carrier variable keeps the raw bits and any other
    /// place gets the canonical value.
    pub(super) fn store_carried_place(
        &mut self,
        place: &LValuePlace<'gcx>,
        carried: CarriedValue,
        source_ty: Option<Ty<'gcx>>,
        ty: Ty<'gcx>,
    ) -> Option<()> {
        match *place {
            LValuePlace::Variable { id, span } => {
                let value = self.carried_into_binding(id, carried);
                self.store_variable(id, value, span)
            }
            _ => {
                let value = carried.clean(self, ty);
                self.store_lvalue_place_with_source(place, value, source_ty)
            }
        }
    }
}

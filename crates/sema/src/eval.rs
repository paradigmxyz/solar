use crate::{builtins::Builtin, hir, ty::Gcx};
use alloy_primitives::{B256, U256, keccak256};
use num_bigint::{BigInt, BigUint, Sign};
use num_rational::Ratio;
use num_traits::{One, Signed, Zero};
use solar_ast::{ElementaryType, LitKind, StrKind, TypeSize};
use solar_data_structures::map::FxHashMap;
use solar_interface::{ByteSymbol, Span, diagnostics::ErrorGuaranteed, error_code};
use std::fmt;

const RECURSION_LIMIT: usize = 64;
// Typed arithmetic can temporarily need one bit beyond the EVM word before its
// result is checked against the type.
const MAX_INTERMEDIATE_BITS: u64 = solar_ast::TypeSize::MAX as u64 + 1;
// The precision of literal arithmetic, like solc's rational numbers.
const LITERAL_PRECISION_BITS: u64 = 4096;

// TODO: `convertType` for truncating and extending correctly: https://github.com/argotorg/solidity/blob/de1a017ccb935d149ed6bcbdb730d89883f8ce02/libsolidity/analysis/ConstantEvaluator.cpp#L234

/// Computes the ERC-7201 storage namespace base slot.
///
/// Reference: <https://docs.soliditylang.org/en/latest/units-and-global-variables.html#mathematical-and-cryptographic-functions>
pub fn erc7201_slot(namespace_id: &[u8]) -> B256 {
    let inner = keccak256(namespace_id);
    let inner = U256::from_be_bytes(inner.0).wrapping_sub(U256::from(1));
    let mut outer = keccak256(inner.to_be_bytes::<32>());
    outer.0[31] = 0;
    outer
}

/// Evaluates the given array size expression, emitting an error diagnostic if it fails.
pub fn eval_array_len(gcx: Gcx<'_>, size: &hir::Expr<'_>) -> Result<U256, ErrorGuaranteed> {
    if let Ok(ConstValue::Rational(_)) = gcx.try_eval_const_value(size) {
        let msg = "array length cannot be fractional";
        return Err(gcx.dcx().err(msg).code(error_code!(3208)).span(size.span).emit());
    }
    let int = gcx.eval_const(size)?;
    let Some(int) = int.as_u256() else {
        if int.is_negative() {
            return Err(gcx.dcx().emit_err(size.span, "array length cannot be negative"));
        }
        let err = gcx.dcx().err("array length is too large").code(error_code!(1847));
        return Err(err.span(size.span).note("the maximum is `2**256 - 1`").emit());
    };
    if int.is_zero() {
        let msg = "array length must be greater than zero";
        Err(gcx.dcx().emit_err(size.span, msg))
    } else {
        Ok(int)
    }
}

impl<'gcx> Gcx<'gcx> {
    /// Evaluates the given expression as an integer constant, emitting an error diagnostic if it
    /// fails.
    pub fn eval_const(self, expr: &hir::Expr<'_>) -> Result<&'gcx IntScalar, ErrorGuaranteed> {
        self.try_eval_const(expr).map_err(|err| self.emit_const_eval_error(expr, err))
    }

    /// Evaluates the given expression as an integer constant without emitting diagnostics.
    pub fn try_eval_const(self, expr: &hir::Expr<'_>) -> Result<&'gcx IntScalar, EvalError> {
        match self.try_eval_const_value(expr)? {
            ConstValue::Integer(value) => Ok(value),
            ConstValue::Rational(_) | ConstValue::Bool(_) => Err(EE::UnsupportedExpr.into()),
            ConstValue::String(_) => Err(EE::UnsupportedLiteral.into()),
        }
    }

    /// Evaluates the given expression to a typed constant value, emitting an error diagnostic if
    /// it fails.
    pub fn eval_const_value(
        self,
        expr: &hir::Expr<'_>,
    ) -> Result<&'gcx ConstValue, ErrorGuaranteed> {
        self.try_eval_const_value(expr).map_err(|err| self.emit_const_eval_error(expr, err))
    }

    /// Evaluates the given expression to a typed constant value without emitting diagnostics.
    pub fn try_eval_const_value(self, expr: &hir::Expr<'_>) -> Result<&'gcx ConstValue, EvalError> {
        match self.eval_const_value_result(expr) {
            Ok(value) => Ok(value),
            Err(err) => Err(err.clone()),
        }
    }

    pub(crate) fn eval_const_value_result(self, expr: &hir::Expr<'_>) -> &'gcx EvalResult {
        // Constant values can own big-integer buffers. Keep them in the cache itself so they
        // are dropped with the compilation instead of leaking from the dropless HIR arena.
        self.eval_cache.insert(expr.id, |_| Box::new(eval_const(self, expr)))
    }

    /// Emits a diagnostic for the given constant evaluation error.
    ///
    /// Like its value, the error of an expression is reported once, however many checks
    /// evaluate it. A failed literal operation is reported once, however many expressions contain
    /// it, as type checking reports it too.
    pub fn emit_const_eval_error(self, expr: &hir::Expr<'_>, err: EvalError) -> ErrorGuaranteed {
        match err.kind {
            EE::AlreadyEmitted(guar) => guar,
            _ => self.emit_once(err.literal.unwrap_or(expr.id), || {
                let msg = format!("failed to evaluate constant: {}", err.kind.msg());
                let label = "evaluation of constant value failed here";
                self.dcx().emit_err_label(expr.span, msg, err.span, label)
            }),
        }
    }

    /// Emits the error of the given expression once, however many checks find it.
    pub(crate) fn emit_once(
        self,
        expr: hir::ExprId,
        emit: impl FnOnce() -> ErrorGuaranteed,
    ) -> ErrorGuaranteed {
        self.expr_errors.insert_cloned(expr, |_| emit())
    }
}

pub(crate) fn eval_const(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> EvalResult {
    ConstantEvaluator::new(gcx).try_eval_value(expr)
}

/// Evaluates Solidity constant expressions.
///
/// This supports the source-level constants needed by semantic analysis and
/// codegen's HIR lowering pre-folds. It does not evaluate runtime-dependent
/// expressions such as function calls or memory allocation.
struct ConstantEvaluator<'gcx> {
    gcx: Gcx<'gcx>,
    depth: usize,
    /// The values of the constants evaluated so far, as constants can use each other many times.
    constants: FxHashMap<hir::VariableId, EvalResult>,
}

pub(crate) type EvalResult = Result<ConstValue, EvalError>;

impl<'gcx> ConstantEvaluator<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, depth: 0, constants: FxHashMap::default() }
    }

    fn try_eval_value(&mut self, expr: &hir::Expr<'_>) -> EvalResult {
        let mut res = self.eval_expr(expr);
        if let Err(e) = &mut res
            && e.span.is_dummy()
        {
            e.span = expr.span;
            e.literal = expr.is_numeric_literal().then(|| expr.peel_parens().id);
        }
        res
    }

    fn eval_expr(&mut self, expr: &hir::Expr<'_>) -> EvalResult {
        let expr = expr.peel_parens();
        match expr.kind {
            // hir::ExprKind::Array(_) => unimplemented!(),
            // hir::ExprKind::Assign(_, _, _) => unimplemented!(),
            hir::ExprKind::Binary(l, bin_op, r) => {
                let l = self.eval_operand(l)?;
                let r = self.eval_operand(r)?;
                l.binop(r, bin_op.kind).map_err(Into::into)
            }
            hir::ExprKind::Call(callee, ref args) => self.eval_call(callee, args),
            // hir::ExprKind::Delete(_) => unimplemented!(),
            hir::ExprKind::Ident(res) => {
                // Ignore invalid overloads since they will get correctly detected later.
                let Some(id) = res.iter().find_map(|res| res.as_variable()) else {
                    return Err(EE::NonConstantVar.into());
                };

                let v = self.gcx.hir.variable(id);
                if v.mutability != Some(hir::VarMut::Constant) {
                    return Err(EE::NonConstantVar.into());
                }
                let value = match self.constants.get(&id) {
                    Some(value) => value.clone(),
                    None => {
                        // Like solc, only constants count towards the limit, which stops cyclic
                        // definitions.
                        self.depth += 1;
                        if self.depth > RECURSION_LIMIT {
                            return Err(EE::RecursionLimitReached.spanned(expr.span));
                        }
                        let initializer =
                            v.initializer.expect("constant variable has no initializer");
                        let value = self.try_eval_value(initializer);
                        self.depth -= 1;
                        self.constants.insert(id, value.clone());
                        value
                    }
                };
                // Each use of a failing constant is reported separately.
                let value = value.map_err(|err| EvalError { literal: None, ..err })?;
                // The constant's declared type carries over into the surrounding expression, so
                // arithmetic on it is checked against that type instead of widening to the
                // mathematical result.
                Ok(match value {
                    ConstValue::Integer(value) => {
                        ConstValue::Integer(value.typed(IntTy::from_hir_ty(&v.ty)))
                    }
                    value => value,
                })
            }
            // hir::ExprKind::Index(_, _) => unimplemented!(),
            // hir::ExprKind::Slice(_, _, _) => unimplemented!(),
            hir::ExprKind::Lit(lit) => self.eval_lit(lit),
            // hir::ExprKind::Member(_, _) => unimplemented!(),
            // hir::ExprKind::New(_) => unimplemented!(),
            // hir::ExprKind::Payable(_) => unimplemented!(),
            hir::ExprKind::Ternary(cond, t, f) => {
                let ConstValue::Bool(cond) = self.try_eval_value(cond)? else {
                    return Err(EE::UnsupportedExpr.into());
                };
                let (taken, other) = if cond { (t, f) } else { (f, t) };
                let value = match self.eval_operand(taken)? {
                    ConstValue::Integer(value) => value,
                    ConstValue::Rational(_) => return Err(EE::UnsupportedExpr.into()),
                    value => return Ok(value),
                };
                // Like at runtime, the result has the common type of both branches, so both must
                // be constants.
                let other = self.eval_operand(other)?.into_integer()?;
                let ty = value.int_ty()?.common(other.int_ty()?).ok_or(EE::UnsupportedExpr)?;
                Ok(ConstValue::Integer(value.typed(Some(ty))))
            }
            // hir::ExprKind::Tuple(_) => unimplemented!(),
            // hir::ExprKind::TypeCall(_) => unimplemented!(),
            // hir::ExprKind::Type(_) => unimplemented!(),
            hir::ExprKind::Unary(un_op, v) => {
                let v = self.eval_operand(v)?;
                v.unop(un_op.kind).map_err(Into::into)
            }
            hir::ExprKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
            _ => Err(EE::UnsupportedExpr.into()),
        }
    }

    /// Evaluates an operand, reusing its cached value.
    ///
    /// Type checking evaluates literal arithmetic at every operation, from the operands up, so
    /// evaluating the operands again would take quadratic time in long literal chains.
    fn eval_operand(&mut self, expr: &hir::Expr<'_>) -> EvalResult {
        match self.gcx.eval_cache.get(&expr.peel_parens().id) {
            Some(result) => result.clone(),
            None => self.try_eval_value(expr),
        }
    }

    fn eval_call(&mut self, callee: &hir::Expr<'_>, args: &hir::CallArgs<'_>) -> EvalResult {
        if let hir::ExprKind::Ident(res) = callee.peel_parens().kind
            && matches!(res.first(), Some(hir::Res::Builtin(Builtin::Erc7201)))
            && let hir::CallArgsKind::Unnamed([arg]) = args.kind
            && let ConstValue::String(namespace_id) = self.try_eval_value(arg)?
        {
            let slot = IntScalar::new(erc7201_slot(namespace_id.as_byte_str()).into());
            return Ok(ConstValue::Integer(slot.typed(Some(IntTy::full_width(false)))));
        }
        Err(EE::UnsupportedExpr.into())
    }

    fn eval_lit(&mut self, lit: &hir::Lit<'_>) -> EvalResult {
        match lit.kind {
            LitKind::Str(StrKind::Str | StrKind::Unicode, s, _) => Ok(ConstValue::String(s)),
            LitKind::Str(StrKind::Hex, _, _) => Err(EE::UnsupportedLiteral.into()),
            LitKind::Number(n) => Ok(ConstValue::Integer(IntScalar::new(n))),
            LitKind::Rational(ratio) => Ok(ConstValue::Rational(Ratio::new_raw(
                IntScalar::bigint_from_u256(*ratio.numer()),
                IntScalar::bigint_from_u256(*ratio.denom()),
            ))),
            LitKind::Address(address) => {
                Ok(ConstValue::Integer(IntScalar::from_be_bytes(address.as_slice())))
            }
            LitKind::Bool(bool) => Ok(ConstValue::Bool(bool)),
            LitKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
        }
    }
}

/// A typed Solidity constant value.
#[derive(Clone, Debug)]
pub enum ConstValue {
    /// Integer-like constant value.
    Integer(IntScalar),
    /// Fractional value of literal arithmetic, such as `0.5` or `1 / 3`.
    ///
    /// Integral values of literal arithmetic are [`Self::Integer`]s.
    Rational(Ratio<BigInt>),
    /// Boolean constant value.
    Bool(bool),
    /// String constant value.
    String(ByteSymbol),
}

impl ConstValue {
    /// Creates the value of literal arithmetic.
    fn literal(value: Ratio<BigInt>) -> Self {
        if value.is_integer() {
            Self::Integer(IntScalar { data: value.into_raw().0, ty: None })
        } else {
            Self::Rational(value)
        }
    }

    /// Returns the non-negative integer value as unsigned data.
    pub fn as_u256(&self) -> Option<U256> {
        match self {
            Self::Integer(value) => value.as_u256(),
            Self::Rational(_) | Self::Bool(_) | Self::String(_) => None,
        }
    }

    /// Returns the boolean value, if this is a boolean constant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Integer(_) | Self::Rational(_) | Self::String(_) => None,
        }
    }

    /// Returns whether this is an integer constant with value zero.
    pub fn is_zero(&self) -> bool {
        matches!(self, Self::Integer(value) if value.is_zero())
    }

    /// Converts this value into an integer constant.
    pub fn into_integer(self) -> Result<IntScalar, EvalError> {
        match self {
            Self::Integer(value) => Ok(value),
            Self::Rational(_) | Self::Bool(_) => Err(EE::UnsupportedExpr.into()),
            Self::String(_) => Err(EE::UnsupportedLiteral.into()),
        }
    }

    /// Applies the given unary operation to this value.
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        Ok(match (self, op) {
            (Self::Integer(value), op) if value.ty.is_some() => Self::Integer(value.unop(op)?),
            (Self::Bool(value), hir::UnOpKind::Not) => Self::Bool(!value),
            (value, op) => {
                let Some(value) = value.into_literal() else {
                    return Err(EE::UnsupportedUnaryOp);
                };
                return literal_unop(value, op);
            }
        })
    }

    /// Applies the given binary operation to this value.
    pub fn binop(self, rhs: Self, op: hir::BinOpKind) -> Result<Self, EE> {
        use hir::BinOpKind::*;
        Ok(match (self, rhs) {
            (Self::Integer(lhs), Self::Integer(rhs)) if lhs.ty.is_some() || rhs.ty.is_some() => {
                match op {
                    Lt => Self::Bool(lhs.data < rhs.data),
                    Le => Self::Bool(lhs.data <= rhs.data),
                    Gt => Self::Bool(lhs.data > rhs.data),
                    Ge => Self::Bool(lhs.data >= rhs.data),
                    Eq => Self::Bool(lhs.data == rhs.data),
                    Ne => Self::Bool(lhs.data != rhs.data),
                    Add | Sub | Mul | Div | Rem | Pow | BitOr | BitAnd | BitXor | Shr | Shl
                    | Sar => Self::Integer(lhs.binop(rhs, op)?),
                    Or | And => return Err(EE::UnsupportedBinaryOp),
                }
            }
            (Self::Bool(lhs), Self::Bool(rhs)) => match op {
                And => Self::Bool(lhs && rhs),
                Or => Self::Bool(lhs || rhs),
                Eq => Self::Bool(lhs == rhs),
                Ne => Self::Bool(lhs != rhs),
                BitAnd => Self::Bool(lhs & rhs),
                BitOr => Self::Bool(lhs | rhs),
                BitXor => Self::Bool(lhs ^ rhs),
                _ => return Err(EE::UnsupportedBinaryOp),
            },
            (lhs, rhs) => {
                let (Some(lhs), Some(rhs)) = (lhs.into_literal(), rhs.into_literal()) else {
                    return Err(EE::UnsupportedBinaryOp);
                };
                return literal_binop(lhs, rhs, op);
            }
        })
    }

    /// Returns `true` if this is a fraction too large for any fixed-point type, which solc's
    /// `RationalNumberType::fixedPointType` uses as its mobile type.
    pub fn is_fraction_too_large(&self) -> bool {
        let Self::Rational(value) = self else { return false };
        let max =
            if value.is_negative() { BigInt::one() << 255 } else { (BigInt::one() << 256) - 1 };
        value.abs() > Ratio::from_integer(max)
    }

    /// Returns the exact value of a literal number, which has no declared type.
    fn into_literal(self) -> Option<Ratio<BigInt>> {
        match self {
            Self::Integer(IntScalar { data, ty: None }) => Some(data.into()),
            Self::Rational(value) => Some(value),
            Self::Integer(_) | Self::Bool(_) | Self::String(_) => None,
        }
    }
}

/// Applies the given unary operation to a literal number, like solc's
/// `ConstantEvaluator::evaluateUnaryOperator`.
fn literal_unop(value: Ratio<BigInt>, op: hir::UnOpKind) -> Result<ConstValue, EE> {
    match op {
        hir::UnOpKind::Neg => Ok(ConstValue::literal(-value)),
        hir::UnOpKind::BitNot if value.is_integer() => {
            Ok(ConstValue::literal((!value.into_raw().0).into()))
        }
        _ => Err(EE::UnsupportedUnaryOp),
    }
}

/// Applies the given binary operation to two literal numbers, like solc's
/// `ConstantEvaluator::evaluateBinaryOperator`.
///
/// Arithmetic is exact. Bitwise operators and shifts apply to integers only, and exponents must
/// be integers. Like solc, the most significant bit of the result's numerator and denominator is
/// at most bit 4096.
fn literal_binop(
    lhs: Ratio<BigInt>,
    rhs: Ratio<BigInt>,
    op: hir::BinOpKind,
) -> Result<ConstValue, EE> {
    use hir::BinOpKind::*;
    let integers = lhs.is_integer() && rhs.is_integer();
    let value = match op {
        Lt => return Ok(ConstValue::Bool(lhs < rhs)),
        Le => return Ok(ConstValue::Bool(lhs <= rhs)),
        Gt => return Ok(ConstValue::Bool(lhs > rhs)),
        Ge => return Ok(ConstValue::Bool(lhs >= rhs)),
        Eq => return Ok(ConstValue::Bool(lhs == rhs)),
        Ne => return Ok(ConstValue::Bool(lhs != rhs)),
        Add => lhs + rhs,
        Sub => lhs - rhs,
        Mul => lhs * rhs,
        Div | Rem if rhs.is_zero() => return Err(EE::DivisionByZero),
        Div => lhs / rhs,
        // Truncates the quotient, like integer division.
        Rem => lhs % rhs,
        Pow => literal_pow(lhs, rhs)?,
        BitOr if integers => (lhs.into_raw().0 | rhs.into_raw().0).into(),
        BitAnd if integers => (lhs.into_raw().0 & rhs.into_raw().0).into(),
        BitXor if integers => (lhs.into_raw().0 ^ rhs.into_raw().0).into(),
        Shl | Shr if integers => literal_shift(lhs.into_raw().0, rhs.into_raw().0, op)?.into(),
        BitOr | BitAnd | BitXor | Shl | Shr | Sar | Or | And => {
            return Err(EE::UnsupportedBinaryOp);
        }
    };
    if value.numer().bits().max(value.denom().bits()) > LITERAL_PRECISION_BITS + 1 {
        return Err(EE::ArithmeticOverflow);
    }
    Ok(ConstValue::literal(value))
}

/// Shifts a literal integer, like solc.
///
/// A negative amount is an operator error rather than an evaluation error, as the type checker
/// rejects it for any operands.
fn literal_shift(value: BigInt, amount: BigInt, op: hir::BinOpKind) -> Result<BigInt, EE> {
    if amount.is_negative() {
        return Err(EE::UnsupportedBinaryOp);
    }
    let amount = u32::try_from(amount).map_err(|_| EE::ArithmeticOverflow)?;
    if op == hir::BinOpKind::Shr {
        // Rounds towards negative infinity, like the EVM's `sar`.
        Ok(value >> amount)
    } else {
        int_shl(value, amount)
    }
}

/// Raises a literal number to an integer power, like solc.
///
/// The numerator and denominator are raised separately. Like solc, `0` raised to a negative
/// power is `0`.
fn literal_pow(base: Ratio<BigInt>, exp: Ratio<BigInt>) -> Result<Ratio<BigInt>, EE> {
    if !exp.is_integer() {
        return Err(EE::UnsupportedBinaryOp);
    }
    let exp = exp.into_raw().0;
    let (numer, denom) = base.into_raw();
    let value =
        Ratio::new_raw(int_pow(&numer, exp.magnitude())?, int_pow(&denom, exp.magnitude())?);
    Ok(if exp.is_negative() && !value.is_zero() { value.recip() } else { value })
}

/// Shifts an integer to the left, within the precision of literal arithmetic.
///
/// Typed arithmetic checks the result against its type afterwards.
fn int_shl(value: BigInt, amount: u32) -> Result<BigInt, EE> {
    if !value.is_zero() && value.bits() + u64::from(amount) > LITERAL_PRECISION_BITS {
        return Err(EE::ArithmeticOverflow);
    }
    Ok(value << amount)
}

/// Raises an integer to a power, within the precision of literal arithmetic.
///
/// Powers of `0`, `1` and `-1` are exact for any exponent. Like solc's `fitsPrecisionExp`, other
/// bases are bounded by their bit length times the exponent, which never rejects a power that
/// fits in a word. Typed arithmetic checks the result against its type afterwards.
fn int_pow(base: &BigInt, exp: &BigUint) -> Result<BigInt, EE> {
    if exp.is_zero() {
        return Ok(BigInt::one());
    }
    if base.is_zero() || base.is_one() {
        return Ok(base.clone());
    }
    if *base == -BigInt::one() {
        return Ok(if exp.bit(0) { base.clone() } else { BigInt::one() });
    }
    let exp = u32::try_from(exp).map_err(|_| EE::ArithmeticOverflow)?;
    if base.bits() * u64::from(exp) > LITERAL_PRECISION_BITS {
        return Err(EE::ArithmeticOverflow);
    }
    Ok(base.pow(exp))
}

/// The declared integer type of a constant value.
#[derive(Clone, Copy, Eq, Debug)]
struct IntTy {
    signed: bool,
    size: TypeSize,
}

/// `int` and `int256` denote the same type with a different size, so compare the bit widths
/// instead of the sizes as written.
impl PartialEq for IntTy {
    fn eq(&self, other: &Self) -> bool {
        self.signed == other.signed && self.bits() == other.bits()
    }
}

impl IntTy {
    /// Returns the integer type denoted by the given type, if it is an integer type.
    ///
    /// A `bytesN` value is computed like a `uintN`, so a left shift that drops bits fails instead
    /// of keeping them, and lowering computes it at runtime.
    fn from_hir_ty(ty: &hir::Type<'_>) -> Option<Self> {
        match ty.kind {
            hir::TypeKind::Elementary(ElementaryType::Int(size)) => {
                Some(Self { signed: true, size })
            }
            hir::TypeKind::Elementary(
                ElementaryType::UInt(size) | ElementaryType::FixedBytes(size),
            ) => Some(Self { signed: false, size }),
            _ => None,
        }
    }

    /// Returns the number of bits of the type.
    fn bits(self) -> u16 {
        self.size.bits()
    }

    /// Returns whether the given value is representable in this type.
    fn contains(self, value: &BigInt) -> bool {
        let value_bits = if self.signed { self.bits() - 1 } else { self.bits() };
        if value.is_negative() {
            self.signed && -value <= BigInt::one() << value_bits
        } else {
            value.bits() <= value_bits as u64
        }
    }

    /// Returns the widest integer type of the given signedness.
    fn full_width(signed: bool) -> Self {
        Self { signed, size: TypeSize::new_int_bits(256) }
    }

    /// Returns the narrowest integer type of the given signedness holding `bits` value bits.
    ///
    /// Integer types come in whole bytes, so the width is rounded up to the next multiple of
    /// eight; a value needing more than a word has no such type.
    fn narrowest(bits: u64, signed: bool) -> Option<Self> {
        let bits = bits.max(1).div_ceil(8) * 8;
        u16::try_from(bits)
            .ok()
            .and_then(TypeSize::try_new_int_bits)
            .map(|size| Self { signed, size })
    }

    /// Returns whether values of this type implicitly convert to `other`.
    ///
    /// Integer types only convert implicitly to wider types of the same signedness.
    fn converts_to(self, other: Self) -> bool {
        self.signed == other.signed && other.bits() >= self.bits()
    }

    /// Returns the type both `self` and `other` implicitly convert to, if any.
    fn common(self, other: Self) -> Option<Self> {
        if other.converts_to(self) {
            Some(self)
        } else if self.converts_to(other) {
            Some(other)
        } else {
            None
        }
    }
}

/// Represents an integer value for constant evaluation.
#[derive(Clone, Debug)]
pub struct IntScalar {
    data: BigInt,
    /// The declared type the value was computed in, if it came from a typed constant.
    ///
    /// Values built only from literals carry no type, and [`ConstValue`] computes them exactly;
    /// as soon as a typed constant takes part in the expression, arithmetic is checked against
    /// the declared type instead of yielding the mathematical result.
    ty: Option<IntTy>,
}

impl IntScalar {
    /// Creates a new non-negative integer value.
    pub fn new(data: U256) -> Self {
        Self { data: Self::bigint_from_u256(data), ty: None }
    }

    /// Creates a new integer value from a boolean.
    pub fn from_bool(value: bool) -> Self {
        Self::new(U256::from(value as u8))
    }

    /// Creates a new integer value from big-endian bytes.
    ///
    /// # Panics
    ///
    /// Panics if `bytes` is empty or has a length greater than 32.
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        Self::new(U256::from_be_slice(bytes))
    }

    /// Returns the bit length of the integer value.
    ///
    /// This is the number of bits needed for the literal type.
    pub fn bit_len(&self) -> u64 {
        Self::bits(&self.data)
    }

    /// Returns whether the value is negative.
    pub fn is_negative(&self) -> bool {
        self.data.is_negative()
    }

    /// Returns whether the value requires a signed integer type.
    pub fn is_signed(&self) -> bool {
        self.is_negative()
    }

    /// Returns whether the integer value is zero.
    pub fn is_zero(&self) -> bool {
        self.data.is_zero()
    }

    /// Returns the non-negative integer value as unsigned data.
    pub fn as_u256(&self) -> Option<U256> {
        let data = self.data.to_biguint()?;
        U256::try_from_le_slice(&data.to_bytes_le())
    }

    /// Returns the 256-bit two's-complement EVM word for this integer value.
    pub fn as_evm_word(&self) -> U256 {
        if let Some(value) = self.as_u256() {
            return value;
        }
        let magnitude = U256::try_from_le_slice(&self.data.magnitude().to_bytes_le())
            .expect("constant evaluator keeps integers within 256 bits");
        U256::ZERO.wrapping_sub(magnitude)
    }

    /// Converts the integer value to a boolean.
    pub fn to_bool(&self) -> bool {
        !self.data.is_zero()
    }

    fn bigint_from_u256(data: U256) -> BigInt {
        BigInt::from_bytes_be(Sign::Plus, &data.to_be_bytes::<32>())
    }

    fn checked(data: BigInt) -> Result<Self, EE> {
        if Self::bits(&data) > MAX_INTERMEDIATE_BITS {
            return Err(EE::ArithmeticOverflow);
        }
        Ok(Self { data, ty: None })
    }

    /// Attaches the declared type of the constant this value was read from.
    ///
    /// An initializer that does not fit its declared type is already a type error at the
    /// declaration, so the value stays untyped instead of reporting a second error here.
    fn typed(mut self, ty: Option<IntTy>) -> Self {
        self.ty = ty.filter(|ty| ty.contains(&self.data));
        self
    }

    /// Sets the type the value was computed in, rejecting values outside of its range.
    fn retype(mut self, ty: Option<IntTy>) -> Result<Self, EE> {
        if let Some(ty) = ty
            && !ty.contains(&self.data)
        {
            return Err(EE::ArithmeticOverflow);
        }
        self.ty = ty;
        Ok(self)
    }

    fn bits(data: &BigInt) -> u64 {
        if data.is_zero() {
            return 1;
        }
        if data.is_positive() {
            return data.bits();
        }
        let abs = data.magnitude();
        // Signed N-bit two's-complement values cover [-2^(N - 1), 2^(N - 1) - 1].
        // Negative powers of two therefore fit in one fewer value bit than other negatives.
        if Self::is_power_of_two(abs) { abs.bits() } else { abs.bits() + 1 }
    }

    fn is_power_of_two(value: &BigUint) -> bool {
        !value.is_zero() && (value & (value - BigUint::one())).is_zero()
    }

    fn negate(self) -> Result<Self, EE> {
        Self::checked(-self.data)
    }

    fn shift_amount(r: Self) -> Option<usize> {
        r.as_u256()?.try_into().ok()
    }

    fn bitop(self, r: Self, f: impl FnOnce(BigInt, BigInt) -> BigInt) -> Result<Self, EE> {
        Self::checked(f(self.data, r.data))
    }

    /// Applies the given unary operation to this value.
    ///
    /// The operation is performed in the operand's type, so a result outside of that type's range
    /// is an error rather than the mathematical value.
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        let ty = self.ty;
        let value = match op {
            hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec => return Err(EE::UnsupportedUnaryOp),
            hir::UnOpKind::Not | hir::UnOpKind::BitNot => Self::checked(!self.data)?,
            // Negating an unsigned value is not arithmetic that overflows but an operator the
            // operand's type does not have, which is what solc reports for it.
            hir::UnOpKind::Neg if ty.is_some_and(|ty| !ty.signed) => {
                return Err(EE::NegateUnsigned);
            }
            hir::UnOpKind::Neg => self.negate()?,
        };
        value.retype(ty)
    }

    /// Returns the type of the value: its declared type, or the mobile type of a literal.
    fn int_ty(&self) -> Result<IntTy, EE> {
        self.ty.map_or_else(|| self.mobile_ty(), Ok)
    }

    /// Returns the mobile type of a literal operand.
    ///
    /// A typed operand makes the other, untyped one leave the rationals: the type checker gives
    /// the literal its mobile type, the narrowest integer type of its sign holding it. A literal
    /// too large for a word has no mobile type at all, and the operator does not apply to it,
    /// which is what solc reports as "Literal too large".
    fn mobile_ty(&self) -> Result<IntTy, EE> {
        IntTy::narrowest(Self::bits(&self.data), self.is_negative()).ok_or(EE::LiteralTooLarge)
    }

    /// Returns the type a literal operand and a typed operand are computed in, if any.
    ///
    /// The literal takes its mobile type, and the operation is performed in the common type of
    /// that and the typed operand: the mobile type when the typed operand converts to it, and
    /// the typed operand's own type when the literal fits in it instead. Neither holds only when
    /// the two have different signedness, and the result stays untyped because the type checker
    /// already rejects such operands.
    fn literal_common_ty(literal: &Self, typed: IntTy) -> Result<Option<IntTy>, EE> {
        let mobile = literal.mobile_ty()?;
        Ok(if typed.converts_to(mobile) {
            Some(mobile)
        } else {
            typed.contains(&literal.data).then_some(typed)
        })
    }

    /// Returns the type the given binary operation is performed in, if any.
    ///
    /// Shifts and exponentiation are performed in the left operand's type, every other operation
    /// in the common type of both operands. Operands without a common type stay untyped because
    /// the type checker already rejects them.
    ///
    /// A literal paired with a typed operand must first have a mobile type, and the operation is
    /// rejected when it does not. This check comes before the operation because folding retypes
    /// only the result: `(1 << 256) >> ONE` with a typed `ONE` would otherwise shift the literal
    /// back into range and be accepted, where solc rejects the operands and the runtime
    /// expression yields `0`.
    ///
    /// A shift or exponentiation whose left operand is a literal is the further exception: it is
    /// always performed in `uint256`, or `int256` for a negative literal, rather than at full
    /// precision. Keeping it unbounded would fold `(1 << SHIFT) >> SHIFT` to `1` where the EVM
    /// shifts the bit out and yields `0`, and would let an exponentiation that reverts with
    /// `Panic(0x11)` at runtime evaluate to a value wider than a word.
    fn binop_ty(l: &Self, r: &Self, op: hir::BinOpKind) -> Result<Option<IntTy>, EE> {
        use hir::BinOpKind::*;
        Ok(match op {
            Shl | Shr | Sar | Pow => match (l.ty, r.ty) {
                (Some(ty), Some(_)) => Some(ty),
                (Some(ty), None) => {
                    r.mobile_ty()?;
                    Some(ty)
                }
                (None, Some(_)) => {
                    l.mobile_ty()?;
                    Some(IntTy::full_width(l.is_negative()))
                }
                (None, None) => None,
            },
            _ => match (l.ty, r.ty) {
                (None, None) => None,
                (Some(ty), None) => Self::literal_common_ty(r, ty)?,
                (None, Some(ty)) => Self::literal_common_ty(l, ty)?,
                (Some(l), Some(r)) => l.common(r),
            },
        })
    }

    /// Applies the given binary operation to this value.
    ///
    /// Typed arithmetic stays checked: a result outside of the operation's type is an error, like
    /// it is at runtime. [`ConstValue`] computes literal arithmetic exactly instead.
    pub fn binop(self, r: Self, op: hir::BinOpKind) -> Result<Self, EE> {
        let ty = Self::binop_ty(&self, &r, op)?;
        self.binop_value(r, op)?.retype(ty)
    }

    fn binop_value(self, r: Self, op: hir::BinOpKind) -> Result<Self, EE> {
        use hir::BinOpKind::*;
        Ok(match op {
            Add => Self::checked(self.data + r.data)?,
            Sub => Self::checked(self.data - r.data)?,
            Mul => Self::checked(self.data * r.data)?,
            Div => {
                if r.data.is_zero() {
                    return Err(EE::DivisionByZero);
                }
                Self::checked(self.data / r.data)?
            }
            Rem => {
                if r.data.is_zero() {
                    return Err(EE::DivisionByZero);
                }
                Self::checked(self.data % r.data)?
            }
            Pow => {
                let exp = r.data.to_biguint().ok_or(EE::ArithmeticOverflow)?;
                Self::checked(int_pow(&self.data, &exp)?)?
            }
            BitOr => self.bitop(r, |a, b| a | b)?,
            BitAnd => self.bitop(r, |a, b| a & b)?,
            BitXor => self.bitop(r, |a, b| a ^ b)?,
            Shr => {
                let r = Self::shift_amount(r).ok_or(EE::ArithmeticOverflow)?;
                Self::checked(self.data >> r)?
            }
            Shl => {
                let amount = u32::try_from(r.data).map_err(|_| EE::ArithmeticOverflow)?;
                Self::checked(int_shl(self.data, amount)?)?
            }
            Sar => return Err(EE::UnsupportedBinaryOp),
            Lt | Le | Gt | Ge | Eq | Ne | Or | And => return Err(EE::UnsupportedBinaryOp),
        })
    }
}

#[derive(Clone, Debug)]
pub enum EvalErrorKind {
    RecursionLimitReached,
    ArithmeticOverflow,
    LiteralTooLarge,
    NegateUnsigned,
    DivisionByZero,
    UnsupportedLiteral,
    UnsupportedUnaryOp,
    UnsupportedBinaryOp,
    UnsupportedExpr,
    NonConstantVar,
    AlreadyEmitted(ErrorGuaranteed),
}
use EvalErrorKind as EE;

impl EvalErrorKind {
    pub fn spanned(self, span: Span) -> EvalError {
        EvalError { kind: self, span, literal: None }
    }

    pub(crate) fn msg(&self) -> &'static str {
        match self {
            Self::RecursionLimitReached => "recursion limit reached",
            Self::ArithmeticOverflow => "arithmetic overflow",
            Self::LiteralTooLarge => "literal is too large for the type of the other operand",
            Self::NegateUnsigned => "cannot apply unary operator `-` to an unsigned type",
            Self::DivisionByZero => "attempted to divide by zero",
            Self::UnsupportedLiteral => "unsupported literal",
            Self::UnsupportedUnaryOp => "unsupported unary operation",
            Self::UnsupportedBinaryOp => "unsupported binary operation",
            Self::UnsupportedExpr => "unsupported expression",
            Self::NonConstantVar => "only constant variables are allowed",
            Self::AlreadyEmitted(_) => unreachable!(),
        }
    }
}

#[derive(Clone, Debug)]
pub struct EvalError {
    pub span: Span,
    pub kind: EvalErrorKind,
    /// The failed literal operation, if the error comes from one in the evaluated expression.
    pub literal: Option<hir::ExprId>,
}

impl From<EE> for EvalError {
    fn from(value: EE) -> Self {
        Self { kind: value, span: Span::DUMMY, literal: None }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.msg().fmt(f)
    }
}

impl std::error::Error for EvalError {}

#[cfg(test)]
mod tests {
    use super::{ConstValue, IntScalar, IntTy, erc7201_slot};
    use crate::hir;
    use alloy_primitives::{U256, b256};
    use num_bigint::BigInt;
    use solar_ast::TypeSize;

    fn int(signed: bool, bits: u16) -> IntTy {
        IntTy { signed, size: TypeSize::new_int_bits(bits) }
    }

    #[test]
    fn const_value_integer_accessors() {
        let zero = ConstValue::Integer(IntScalar::new(U256::ZERO));
        assert_eq!(zero.as_u256(), Some(U256::ZERO));
        assert_eq!(zero.as_bool(), None);
        assert!(zero.is_zero());

        let one = ConstValue::Integer(IntScalar::new(U256::from(1)));
        assert_eq!(one.as_u256(), Some(U256::from(1)));
        assert!(!one.is_zero());

        let negative =
            ConstValue::Integer(IntScalar::new(U256::from(1)).unop(hir::UnOpKind::Neg).unwrap());
        assert_eq!(negative.as_u256(), None);
        assert!(!negative.is_zero());
    }

    #[test]
    fn const_value_bool_accessors_preserve_type() {
        let value = ConstValue::Bool(false);
        assert_eq!(value.as_bool(), Some(false));
        assert_eq!(value.as_u256(), None);
        assert!(!value.is_zero());
    }

    #[test]
    fn int_ty_contains_range_boundaries() {
        assert!(int(true, 8).contains(&BigInt::from(127)));
        assert!(!int(true, 8).contains(&BigInt::from(128)));
        assert!(int(true, 8).contains(&BigInt::from(-128)));
        assert!(!int(true, 8).contains(&BigInt::from(-129)));
        assert!(int(false, 8).contains(&BigInt::from(255)));
        assert!(!int(false, 8).contains(&BigInt::from(256)));
        assert!(!int(false, 8).contains(&BigInt::from(-1)));
    }

    #[test]
    fn int_ty_common_widens_only_within_signedness() {
        assert_eq!(int(true, 8).common(int(true, 16)), Some(int(true, 16)));
        assert_eq!(int(true, 16).common(int(true, 8)), Some(int(true, 16)));
        assert_eq!(int(false, 8).common(int(false, 8)), Some(int(false, 8)));
        assert_eq!(int(true, 8).common(int(false, 16)), None);
        assert_eq!(int(false, 8).common(int(true, 16)), None);
    }

    #[test]
    fn int_ty_eq_ignores_how_the_size_is_written() {
        let plain = IntTy { signed: true, size: TypeSize::ZERO };
        assert_eq!(plain.bits(), int(true, 256).bits());
        assert_eq!(plain, int(true, 256));
        assert_eq!(plain.common(int(true, 256)), Some(int(true, 256)));
        assert_ne!(plain, int(false, 256));
        assert_ne!(plain, int(true, 128));
    }

    #[test]
    fn erc7201_slot_matches_eip_example() {
        assert_eq!(
            erc7201_slot(b"example.main"),
            b256!("183a6125c38840424c4a85fa12bab2ab606c4b6d0e7cc73c0c06ba5300eab500")
        );
    }

    #[test]
    fn erc7201_slot_subtracts_from_full_inner_hash() {
        assert_eq!(
            erc7201_slot(b"85"),
            b256!("06d0d983459328e82eacb1bf2d6fadfa38a6896e9d4cbfe0e1aa41c6281bab00")
        );
    }
}

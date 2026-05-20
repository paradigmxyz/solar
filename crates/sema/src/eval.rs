use crate::{hir, ty::Gcx};
use alloy_primitives::U256;
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Signed, Zero};
use solar_ast::LitKind;
use solar_interface::{Span, diagnostics::ErrorGuaranteed};
use std::fmt;

const RECURSION_LIMIT: usize = 64;
const MAX_BITS: u64 = solar_ast::TypeSize::MAX as u64;

// TODO: `convertType` for truncating and extending correctly: https://github.com/argotorg/solidity/blob/de1a017ccb935d149ed6bcbdb730d89883f8ce02/libsolidity/analysis/ConstantEvaluator.cpp#L234

/// Evaluates the given array size expression, emitting an error diagnostic if it fails.
pub fn eval_array_len(gcx: Gcx<'_>, size: &hir::Expr<'_>) -> Result<U256, ErrorGuaranteed> {
    match ConstantEvaluator::new(gcx).eval(size) {
        Ok(int) => {
            let Some(int) = int.as_u256() else {
                let msg = "array length cannot be negative";
                return Err(gcx.dcx().err(msg).span(size.span).emit());
            };
            if int.is_zero() {
                let msg = "array length must be greater than zero";
                Err(gcx.dcx().err(msg).span(size.span).emit())
            } else {
                Ok(int)
            }
        }
        Err(guar) => Err(guar),
    }
}

/// Evaluates simple constants.
///
/// This only supports basic arithmetic and logical operations, and does not support more complex
/// operations like function calls or memory allocation.
///
/// This is only supposed to be used for array sizes and other simple constants.
pub struct ConstantEvaluator<'gcx> {
    pub gcx: Gcx<'gcx>,
    depth: usize,
}

type EvalResult<'gcx> = Result<IntScalar, EvalError>;

impl<'gcx> ConstantEvaluator<'gcx> {
    /// Creates a new constant evaluator.
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, depth: 0 }
    }

    /// Evaluates the given expression, emitting an error diagnostic if it fails.
    pub fn eval(&mut self, expr: &hir::Expr<'_>) -> Result<IntScalar, ErrorGuaranteed> {
        self.try_eval(expr).map_err(|err| self.emit_eval_error(expr, err))
    }

    /// Evaluates the given expression, returning an error if it fails.
    pub fn try_eval(&mut self, expr: &hir::Expr<'_>) -> EvalResult<'gcx> {
        self.depth += 1;
        if self.depth > RECURSION_LIMIT {
            return Err(EE::RecursionLimitReached.spanned(expr.span));
        }
        let mut res = self.eval_expr(expr);
        if let Err(e) = &mut res
            && e.span.is_dummy()
        {
            e.span = expr.span;
        }
        self.depth = self.depth.checked_sub(1).unwrap();
        res
    }

    /// Emits a diagnostic for the given evaluation error.
    pub fn emit_eval_error(&self, expr: &hir::Expr<'_>, err: EvalError) -> ErrorGuaranteed {
        match err.kind {
            EE::AlreadyEmitted(guar) => guar,
            _ => {
                let msg = format!("failed to evaluate constant: {}", err.kind.msg());
                let label = "evaluation of constant value failed here";
                self.gcx.dcx().err(msg).span(expr.span).span_label(err.span, label).emit()
            }
        }
    }

    fn eval_expr(&mut self, expr: &hir::Expr<'_>) -> EvalResult<'gcx> {
        let expr = expr.peel_parens();
        match expr.kind {
            // hir::ExprKind::Array(_) => unimplemented!(),
            // hir::ExprKind::Assign(_, _, _) => unimplemented!(),
            hir::ExprKind::Binary(l, bin_op, r) => {
                let l = self.try_eval(l)?;
                let r = self.try_eval(r)?;
                l.binop(r, bin_op.kind).map_err(Into::into)
            }
            // hir::ExprKind::Call(_, _) => unimplemented!(),
            // hir::ExprKind::Delete(_) => unimplemented!(),
            hir::ExprKind::Ident(res) => {
                // Ignore invalid overloads since they will get correctly detected later.
                let Some(v) = res.iter().find_map(|res| res.as_variable()) else {
                    return Err(EE::NonConstantVar.into());
                };

                let v = self.gcx.hir.variable(v);
                if v.mutability != Some(hir::VarMut::Constant) {
                    return Err(EE::NonConstantVar.into());
                }
                self.try_eval(v.initializer.expect("constant variable has no initializer"))
            }
            // hir::ExprKind::Index(_, _) => unimplemented!(),
            // hir::ExprKind::Slice(_, _, _) => unimplemented!(),
            hir::ExprKind::Lit(lit) => self.eval_lit(lit),
            // hir::ExprKind::Member(_, _) => unimplemented!(),
            // hir::ExprKind::New(_) => unimplemented!(),
            // hir::ExprKind::Payable(_) => unimplemented!(),
            // hir::ExprKind::Ternary(cond, t, f) => {
            //     let cond = self.try_eval(cond)?;
            //     Ok(if cond.to_bool() { self.try_eval(t)? } else { self.try_eval(f)? })
            // }
            // hir::ExprKind::Tuple(_) => unimplemented!(),
            // hir::ExprKind::TypeCall(_) => unimplemented!(),
            // hir::ExprKind::Type(_) => unimplemented!(),
            hir::ExprKind::Unary(un_op, v) => {
                let v = self.try_eval(v)?;
                v.unop(un_op.kind).map_err(Into::into)
            }
            hir::ExprKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
            _ => Err(EE::UnsupportedExpr.into()),
        }
    }

    fn eval_lit(&mut self, lit: &hir::Lit<'_>) -> EvalResult<'gcx> {
        match lit.kind {
            // LitKind::Str(str_kind, arc) => todo!(),
            LitKind::Number(n) => Ok(IntScalar::new(n)),
            // LitKind::Rational(ratio) => todo!(),
            LitKind::Address(address) => Ok(IntScalar::from_be_bytes(address.as_slice())),
            LitKind::Bool(bool) => Ok(IntScalar::from_be_bytes(&[bool as u8])),
            LitKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
            _ => Err(EE::UnsupportedLiteral.into()),
        }
    }
}

/// Represents an integer value for constant evaluation.
pub struct IntScalar {
    data: BigInt,
}

impl IntScalar {
    /// Creates a new non-negative integer value.
    pub fn new(data: U256) -> Self {
        Self { data: Self::bigint_from_u256(data) }
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

    /// Converts the integer value to a boolean.
    pub fn to_bool(&self) -> bool {
        !self.data.is_zero()
    }

    fn bigint_from_u256(data: U256) -> BigInt {
        BigInt::from_bytes_be(Sign::Plus, &data.to_be_bytes::<32>())
    }

    fn checked(data: BigInt) -> Result<Self, EE> {
        if Self::bits(&data) > MAX_BITS {
            return Err(EE::ArithmeticOverflow);
        }
        Ok(Self { data })
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
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        Ok(match op {
            hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec => return Err(EE::UnsupportedUnaryOp),
            hir::UnOpKind::Not | hir::UnOpKind::BitNot => Self::checked(!self.data)?,
            hir::UnOpKind::Neg => self.negate()?,
        })
    }

    /// Applies the given binary operation to this value.
    ///
    /// For literal arithmetic, this preserves the exact mathematical value.
    pub fn binop(self, r: Self, op: hir::BinOpKind) -> Result<Self, EE> {
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
                if r.is_negative() {
                    return Err(EE::ArithmeticOverflow);
                }
                self.checked_pow(r)?
            }
            BitOr => self.bitop(r, |a, b| a | b)?,
            BitAnd => self.bitop(r, |a, b| a & b)?,
            BitXor => self.bitop(r, |a, b| a ^ b)?,
            Shr => {
                let r = Self::shift_amount(r).ok_or(EE::ArithmeticOverflow)?;
                Self::checked(self.data >> r)?
            }
            Shl => self.checked_shl(r)?,
            Sar => return Err(EE::UnsupportedBinaryOp),
            Lt | Le | Gt | Ge | Eq | Ne | Or | And => return Err(EE::UnsupportedBinaryOp),
        })
    }

    fn checked_shl(self, r: Self) -> Result<Self, EE> {
        if self.data.is_zero() {
            return Ok(self);
        }
        let shift: u64 = r
            .as_u256()
            .ok_or(EE::ArithmeticOverflow)?
            .try_into()
            .map_err(|_| EE::ArithmeticOverflow)?;
        let bits = Self::bits(&self.data);
        if shift > MAX_BITS.saturating_sub(bits) {
            return Err(EE::ArithmeticOverflow);
        }
        Self::checked(self.data << usize::try_from(shift).map_err(|_| EE::ArithmeticOverflow)?)
    }

    fn checked_pow(self, r: Self) -> Result<Self, EE> {
        if self.data.is_zero() {
            return Ok(self);
        }
        if self.data.is_one() {
            return Ok(self);
        }
        if self.data == BigInt::from(-1) {
            let exp = r.as_u256().ok_or(EE::ArithmeticOverflow)?;
            let is_odd = exp.bit(0);
            return Self::checked(if is_odd { self.data } else { BigInt::one() });
        }
        let exp = r.as_u256().ok_or(EE::ArithmeticOverflow)?;
        if exp > U256::from(MAX_BITS) {
            return Err(EE::ArithmeticOverflow);
        }
        let exp = exp.try_into().map_err(|_| EE::ArithmeticOverflow)?;
        Self::checked(self.data.pow(exp))
    }
}

#[derive(Debug)]
pub enum EvalErrorKind {
    RecursionLimitReached,
    ArithmeticOverflow,
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
        EvalError { kind: self, span }
    }

    fn msg(&self) -> &'static str {
        match self {
            Self::RecursionLimitReached => "recursion limit reached",
            Self::ArithmeticOverflow => "arithmetic overflow",
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

#[derive(Debug)]
pub struct EvalError {
    pub span: Span,
    pub kind: EvalErrorKind,
}

impl From<EE> for EvalError {
    fn from(value: EE) -> Self {
        Self { kind: value, span: Span::DUMMY }
    }
}

impl fmt::Display for EvalError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.kind.msg().fmt(f)
    }
}

impl std::error::Error for EvalError {}

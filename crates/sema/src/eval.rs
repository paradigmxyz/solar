use crate::{hir, ty::Gcx};
use alloy_primitives::U256;
use solar_ast::LitKind;
use solar_interface::{Span, diagnostics::ErrorGuaranteed};
use std::fmt;

const RECURSION_LIMIT: usize = 64;

// TODO: `convertType` for truncating and extending correctly: https://github.com/argotorg/solidity/blob/de1a017ccb935d149ed6bcbdb730d89883f8ce02/libsolidity/analysis/ConstantEvaluator.cpp#L234

/// Evaluates the given array size expression, emitting an error diagnostic if it fails.
pub fn eval_array_len(gcx: Gcx<'_>, size: &hir::Expr<'_>) -> Result<U256, ErrorGuaranteed> {
    match ConstantEvaluator::new(gcx).eval(size) {
        Ok(int) => {
            if int.is_negative() {
                let msg = "array length cannot be negative";
                Err(gcx.dcx().err(msg).span(size.span).emit())
            } else if int.data.is_zero() {
                let msg = "array length must be greater than zero";
                Err(gcx.dcx().err(msg).span(size.span).emit())
            } else {
                Ok(int.data)
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
            // hir::ExprKind::CallOptions(_, _) => unimplemented!(),
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

/// Represents an integer value with an explicit sign for literal type tracking.
///
/// The `data` field always stores the absolute value of the number.
/// The `negative` field indicates whether the value is negative.
pub struct IntScalar {
    /// The absolute value of the integer.
    pub data: U256,
    /// Whether the value is negative.
    pub negative: bool,
}

impl IntScalar {
    /// Creates a new non-negative integer value.
    pub fn new(data: U256) -> Self {
        Self { data, negative: false }
    }

    /// Creates a new integer value with the given sign.
    pub fn new_signed(data: U256, negative: bool) -> Self {
        // Zero is never negative.
        Self { data, negative: negative && !data.is_zero() }
    }

    /// Returns the bit length of the integer value.
    ///
    /// This is the number of bits needed to represent the value,
    /// not including the sign bit.
    pub fn bit_len(&self) -> u64 {
        self.data.bit_len() as u64
    }

    /// Returns whether the value is negative.
    pub fn is_negative(&self) -> bool {
        self.negative
    }

    /// Returns the negation of this value.
    pub fn negate(self) -> Self {
        if self.data.is_zero() { self } else { Self::new_signed(self.data, !self.negative) }
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

    /// Converts the integer value to a boolean.
    pub fn to_bool(&self) -> bool {
        !self.data.is_zero()
    }

    /// Applies the given unary operation to this value.
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        Ok(match op {
            hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec => return Err(EE::UnsupportedUnaryOp),
            hir::UnOpKind::Not | hir::UnOpKind::BitNot => Self::new(!self.data),
            hir::UnOpKind::Neg => self.negate(),
        })
    }

    /// Applies the given binary operation to this value.
    ///
    /// For signed arithmetic, this handles the sign tracking properly.
    pub fn binop(self, r: Self, op: hir::BinOpKind) -> Result<Self, EE> {
        use hir::BinOpKind::*;
        Ok(match op {
            Add => self.checked_add(r).ok_or(EE::ArithmeticOverflow)?,
            Sub => self.checked_sub(r).ok_or(EE::ArithmeticOverflow)?,
            Mul => self.checked_mul(r).ok_or(EE::ArithmeticOverflow)?,
            Div => self.checked_div(r).ok_or(EE::DivisionByZero)?,
            Rem => self.checked_rem(r).ok_or(EE::DivisionByZero)?,
            Pow => self.checked_pow(r).ok_or(EE::ArithmeticOverflow)?,
            BitOr => Self::new(self.data | r.data),
            BitAnd => Self::new(self.data & r.data),
            BitXor => Self::new(self.data ^ r.data),
            Shr => Self::new(self.data.wrapping_shr(r.data.try_into().unwrap_or(usize::MAX))),
            Shl => Self::new(self.data.wrapping_shl(r.data.try_into().unwrap_or(usize::MAX))),
            Sar => Self::new(self.data.arithmetic_shr(r.data.try_into().unwrap_or(usize::MAX))),
            Lt | Le | Gt | Ge | Eq | Ne | Or | And => return Err(EE::UnsupportedBinaryOp),
        })
    }

    /// Checked addition with sign handling.
    fn checked_add(self, r: Self) -> Option<Self> {
        match (self.negative, r.negative) {
            // Both non-negative: simple add
            (false, false) => Some(Self::new(self.data.checked_add(r.data)?)),
            // Both negative: negate(|a| + |b|)
            (true, true) => Some(Self::new_signed(self.data.checked_add(r.data)?, true)),
            // Different signs: subtract the smaller absolute value from the larger
            (false, true) => {
                // a + (-b) = a - b
                if self.data >= r.data {
                    Some(Self::new(self.data.checked_sub(r.data)?))
                } else {
                    Some(Self::new_signed(r.data.checked_sub(self.data)?, true))
                }
            }
            (true, false) => {
                // (-a) + b = b - a
                if r.data >= self.data {
                    Some(Self::new(r.data.checked_sub(self.data)?))
                } else {
                    Some(Self::new_signed(self.data.checked_sub(r.data)?, true))
                }
            }
        }
    }

    /// Checked subtraction with sign handling.
    fn checked_sub(self, r: Self) -> Option<Self> {
        // a - b = a + (-b)
        self.checked_add(r.negate())
    }

    /// Checked multiplication with sign handling.
    fn checked_mul(self, r: Self) -> Option<Self> {
        let result = self.data.checked_mul(r.data)?;
        // Result is negative if exactly one operand is negative
        Some(Self::new_signed(result, self.negative != r.negative))
    }

    /// Checked division with sign handling.
    fn checked_div(self, r: Self) -> Option<Self> {
        if r.data.is_zero() {
            return None;
        }
        let result = self.data.checked_div(r.data)?;
        // Result is negative if exactly one operand is negative
        Some(Self::new_signed(result, self.negative != r.negative))
    }

    /// Checked remainder with sign handling.
    fn checked_rem(self, r: Self) -> Option<Self> {
        if r.data.is_zero() {
            return None;
        }
        let result = self.data.checked_rem(r.data)?;
        // Result has the sign of the dividend
        Some(Self::new_signed(result, self.negative))
    }

    /// Checked exponentiation.
    fn checked_pow(self, r: Self) -> Option<Self> {
        // Exponent must be non-negative
        if r.negative {
            return None;
        }
        let result = self.data.checked_pow(r.data)?;
        // Result is negative if base is negative and exponent is odd
        let result_negative = self.negative && r.data.bit(0);
        Some(Self::new_signed(result, result_negative))
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

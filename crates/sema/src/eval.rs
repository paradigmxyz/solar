use crate::{hir, ty::Gcx};
use alloy_primitives::U256;
use solar_ast::LitKind;
use solar_interface::{Span, diagnostics::ErrorGuaranteed};
use std::fmt;

const RECURSION_LIMIT: usize = 64;

// TODO: `convertType` for truncating and extending correctly: https://github.com/argotorg/solidity/blob/de1a017ccb935d149ed6bcbdb730d89883f8ce02/libsolidity/analysis/ConstantEvaluator.cpp#L234

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
                l.binop(&r, bin_op.kind).map_err(Into::into)
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

pub struct IntScalar {
    pub data: U256,
}

impl IntScalar {
    pub fn new(data: U256) -> Self {
        Self { data }
    }

    /// Creates a new integer value from a boolean.
    pub fn from_bool(value: bool) -> Self {
        Self { data: U256::from(value as u8) }
    }

    /// Creates a new integer value from big-endian bytes.
    ///
    /// # Panics
    ///
    /// Panics if `bytes` is empty or has a length greater than 32.
    pub fn from_be_bytes(bytes: &[u8]) -> Self {
        Self { data: U256::from_be_slice(bytes) }
    }

    /// Converts the integer value to a boolean.
    pub fn to_bool(&self) -> bool {
        !self.data.is_zero()
    }

    /// Applies the given unary operation to this value.
    pub fn unop(&self, op: hir::UnOpKind) -> Result<Self, EE> {
        Ok(match op {
            hir::UnOpKind::PreInc
            | hir::UnOpKind::PreDec
            | hir::UnOpKind::PostInc
            | hir::UnOpKind::PostDec => return Err(EE::UnsupportedUnaryOp),
            hir::UnOpKind::Not | hir::UnOpKind::BitNot => Self::new(!self.data),
            hir::UnOpKind::Neg => Self::new(self.data.wrapping_neg()),
        })
    }

    /// Applies the given binary operation to this value.
    pub fn binop(&self, r: &Self, op: hir::BinOpKind) -> Result<Self, EE> {
        let l = self;
        Ok(match op {
            // hir::BinOpKind::Lt => Self::from_bool(l.data < r.data),
            // hir::BinOpKind::Le => Self::from_bool(l.data <= r.data),
            // hir::BinOpKind::Gt => Self::from_bool(l.data > r.data),
            // hir::BinOpKind::Ge => Self::from_bool(l.data >= r.data),
            // hir::BinOpKind::Eq => Self::from_bool(l.data == r.data),
            // hir::BinOpKind::Ne => Self::from_bool(l.data != r.data),
            // hir::BinOpKind::Or => Self::from_bool(l.data != 0 || r.data != 0),
            // hir::BinOpKind::And => Self::from_bool(l.data != 0 && r.data != 0),
            hir::BinOpKind::BitOr => Self::new(l.data | r.data),
            hir::BinOpKind::BitAnd => Self::new(l.data & r.data),
            hir::BinOpKind::BitXor => Self::new(l.data ^ r.data),
            hir::BinOpKind::Shr => {
                Self::new(l.data.wrapping_shr(r.data.try_into().unwrap_or(usize::MAX)))
            }
            hir::BinOpKind::Shl => {
                Self::new(l.data.wrapping_shl(r.data.try_into().unwrap_or(usize::MAX)))
            }
            hir::BinOpKind::Sar => {
                Self::new(l.data.arithmetic_shr(r.data.try_into().unwrap_or(usize::MAX)))
            }
            hir::BinOpKind::Add => {
                Self::new(l.data.checked_add(r.data).ok_or(EE::ArithmeticOverflow)?)
            }
            hir::BinOpKind::Sub => {
                Self::new(l.data.checked_sub(r.data).ok_or(EE::ArithmeticOverflow)?)
            }
            hir::BinOpKind::Pow => {
                Self::new(l.data.checked_pow(r.data).ok_or(EE::ArithmeticOverflow)?)
            }
            hir::BinOpKind::Mul => {
                Self::new(l.data.checked_mul(r.data).ok_or(EE::ArithmeticOverflow)?)
            }
            hir::BinOpKind::Div => Self::new(l.data.checked_div(r.data).ok_or(EE::DivisionByZero)?),
            hir::BinOpKind::Rem => Self::new(l.data.checked_rem(r.data).ok_or(EE::DivisionByZero)?),
            hir::BinOpKind::Lt
            | hir::BinOpKind::Le
            | hir::BinOpKind::Gt
            | hir::BinOpKind::Ge
            | hir::BinOpKind::Eq
            | hir::BinOpKind::Ne
            | hir::BinOpKind::Or
            | hir::BinOpKind::And => return Err(EE::UnsupportedBinaryOp),
        })
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

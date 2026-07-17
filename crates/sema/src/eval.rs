use crate::{builtins::Builtin, hir, ty::Gcx};
use alloy_primitives::{B256, U256, keccak256};
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Signed, Zero};
use solar_ast::{LitKind, StrKind};
use solar_interface::{ByteSymbol, Span, diagnostics::ErrorGuaranteed, sym};
use std::fmt;

const RECURSION_LIMIT: usize = 64;
const MAX_BITS: u64 = solar_ast::TypeSize::MAX as u64;

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
    let int = gcx.eval_const(size)?;
    let Some(int) = int.as_u256() else {
        let msg = "array length cannot be negative";
        return Err(gcx.dcx().emit_err(size.span, msg));
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
            ConstValue::Bool(_) => Err(EE::UnsupportedExpr.into()),
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

    /// Evaluates a non-negative integer constant with wrapping 256-bit arithmetic.
    ///
    /// This is useful for source expressions evaluated in an `unchecked` context. Addition,
    /// subtraction, and multiplication wrap as EVM words; all other expressions use ordinary
    /// constant evaluation.
    pub fn try_eval_const_u256_wrapping(self, expr: &hir::Expr<'_>) -> Option<U256> {
        let expr = expr.peel_parens();
        if let hir::ExprKind::Binary(lhs, op, rhs) = &expr.kind
            && matches!(op.kind, hir::BinOpKind::Add | hir::BinOpKind::Sub | hir::BinOpKind::Mul)
        {
            let lhs = self.try_eval_const_u256_wrapping(lhs)?;
            let rhs = self.try_eval_const_u256_wrapping(rhs)?;
            return Some(match op.kind {
                hir::BinOpKind::Add => lhs.wrapping_add(rhs),
                hir::BinOpKind::Sub => lhs.wrapping_sub(rhs),
                hir::BinOpKind::Mul => lhs.wrapping_mul(rhs),
                _ => unreachable!(),
            });
        }
        self.try_eval_const_value(expr).ok()?.as_u256()
    }

    /// Emits a diagnostic for the given constant evaluation error.
    pub fn emit_const_eval_error(self, expr: &hir::Expr<'_>, err: EvalError) -> ErrorGuaranteed {
        match err.kind {
            EE::AlreadyEmitted(guar) => guar,
            _ => {
                let msg = format!("failed to evaluate constant: {}", err.kind.msg());
                let label = "evaluation of constant value failed here";
                self.dcx().emit_err_label(expr.span, msg, err.span, label)
            }
        }
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
}

pub(crate) type EvalResult = Result<ConstValue, EvalError>;

impl<'gcx> ConstantEvaluator<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, depth: 0 }
    }

    fn try_eval_value(&mut self, expr: &hir::Expr<'_>) -> EvalResult {
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

    fn eval_expr(&mut self, expr: &hir::Expr<'_>) -> EvalResult {
        let expr = expr.peel_parens();
        match expr.kind {
            // hir::ExprKind::Array(_) => unimplemented!(),
            // hir::ExprKind::Assign(_, _, _) => unimplemented!(),
            hir::ExprKind::Binary(l, bin_op, r) => {
                let l = self.try_eval_value(l)?;
                let r = self.try_eval_value(r)?;
                l.binop(r, bin_op.kind).map_err(Into::into)
            }
            hir::ExprKind::Call(callee, ref args, opts) => self.eval_call(callee, args, opts),
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
                self.try_eval_value(v.initializer.expect("constant variable has no initializer"))
            }
            // hir::ExprKind::Index(_, _) => unimplemented!(),
            // hir::ExprKind::Slice(_, _, _) => unimplemented!(),
            hir::ExprKind::Lit(lit) => self.eval_lit(lit),
            hir::ExprKind::Member(base, member)
                if matches!(member.name, sym::min | sym::max)
                    && let hir::ExprKind::TypeCall(ty) = &base.peel_parens().kind =>
            {
                self.eval_type_bound(ty, member.name == sym::max)
            }
            // hir::ExprKind::New(_) => unimplemented!(),
            // hir::ExprKind::Payable(_) => unimplemented!(),
            hir::ExprKind::Ternary(cond, t, f) => {
                let ConstValue::Bool(cond) = self.try_eval_value(cond)? else {
                    return Err(EE::UnsupportedExpr.into());
                };
                if cond { self.try_eval_value(t) } else { self.try_eval_value(f) }
            }
            // hir::ExprKind::Tuple(_) => unimplemented!(),
            // hir::ExprKind::TypeCall(_) => unimplemented!(),
            // hir::ExprKind::Type(_) => unimplemented!(),
            hir::ExprKind::Unary(un_op, v) => {
                let v = self.try_eval_value(v)?;
                v.unop(un_op.kind).map_err(Into::into)
            }
            hir::ExprKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
            _ => Err(EE::UnsupportedExpr.into()),
        }
    }

    fn eval_call(
        &mut self,
        callee: &hir::Expr<'_>,
        args: &hir::CallArgs<'_>,
        opts: Option<&hir::CallOptions<'_>>,
    ) -> EvalResult {
        if opts.is_none()
            && let hir::ExprKind::Ident(res) = callee.peel_parens().kind
            && matches!(res.first(), Some(hir::Res::Builtin(Builtin::Erc7201)))
            && let hir::CallArgsKind::Unnamed([arg]) = args.kind
            && let ConstValue::String(namespace_id) = self.try_eval_value(arg)?
        {
            return Ok(ConstValue::Integer(IntScalar::new(
                erc7201_slot(namespace_id.as_byte_str()).into(),
            )));
        }
        Err(EE::UnsupportedExpr.into())
    }

    fn eval_lit(&mut self, lit: &hir::Lit<'_>) -> EvalResult {
        match lit.kind {
            LitKind::Str(StrKind::Str | StrKind::Unicode, s, _) => Ok(ConstValue::String(s)),
            LitKind::Str(StrKind::Hex, _, _) => Err(EE::UnsupportedLiteral.into()),
            LitKind::Number(n) => Ok(ConstValue::Integer(IntScalar::new(n))),
            // LitKind::Rational(ratio) => todo!(),
            LitKind::Address(address) => {
                Ok(ConstValue::Integer(IntScalar::from_be_bytes(address.as_slice())))
            }
            LitKind::Bool(bool) => Ok(ConstValue::Bool(bool)),
            LitKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
            _ => Err(EE::UnsupportedLiteral.into()),
        }
    }

    fn eval_type_bound(&self, ty: &hir::Type<'_>, max: bool) -> EvalResult {
        let value = match ty.kind {
            hir::TypeKind::Elementary(hir::ElementaryType::UInt(size)) => {
                if max {
                    (BigInt::one() << size.bits()) - 1
                } else {
                    BigInt::zero()
                }
            }
            hir::TypeKind::Elementary(hir::ElementaryType::Int(size)) => {
                let bound = BigInt::one() << (size.bits() - 1);
                if max { bound - 1 } else { -bound }
            }
            hir::TypeKind::Custom(hir::ItemId::Enum(id)) => {
                if max {
                    BigInt::from(self.gcx.hir.enumm(id).variants.len().saturating_sub(1))
                } else {
                    BigInt::zero()
                }
            }
            _ => return Err(EE::UnsupportedExpr.into()),
        };
        Ok(ConstValue::Integer(IntScalar::checked(value)?))
    }
}

/// A typed Solidity constant value.
#[derive(Debug)]
pub enum ConstValue {
    /// Integer-like constant value.
    Integer(IntScalar),
    /// Boolean constant value.
    Bool(bool),
    /// String constant value.
    String(ByteSymbol),
}

impl ConstValue {
    /// Returns the non-negative integer value as unsigned data.
    pub fn as_u256(&self) -> Option<U256> {
        match self {
            Self::Integer(value) => value.as_u256(),
            Self::Bool(_) | Self::String(_) => None,
        }
    }

    /// Returns the boolean value, if this is a boolean constant.
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            Self::Bool(value) => Some(*value),
            Self::Integer(_) | Self::String(_) => None,
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
            Self::Bool(_) => Err(EE::UnsupportedExpr.into()),
            Self::String(_) => Err(EE::UnsupportedLiteral.into()),
        }
    }

    /// Applies the given unary operation to this value.
    pub fn unop(self, op: hir::UnOpKind) -> Result<Self, EE> {
        Ok(match (self, op) {
            (Self::Integer(value), op) => Self::Integer(value.unop(op)?),
            (Self::Bool(value), hir::UnOpKind::Not) => Self::Bool(!value),
            (Self::Bool(_) | Self::String(_), _) => return Err(EE::UnsupportedUnaryOp),
        })
    }

    /// Applies the given binary operation to this value.
    pub fn binop(self, rhs: Self, op: hir::BinOpKind) -> Result<Self, EE> {
        use hir::BinOpKind::*;
        Ok(match (self, rhs) {
            (Self::Integer(lhs), Self::Integer(rhs)) => match op {
                Lt => Self::Bool(lhs.data < rhs.data),
                Le => Self::Bool(lhs.data <= rhs.data),
                Gt => Self::Bool(lhs.data > rhs.data),
                Ge => Self::Bool(lhs.data >= rhs.data),
                Eq => Self::Bool(lhs.data == rhs.data),
                Ne => Self::Bool(lhs.data != rhs.data),
                Add | Sub | Mul | Div | Rem | Pow | BitOr | BitAnd | BitXor | Shr | Shl | Sar => {
                    Self::Integer(lhs.binop(rhs, op)?)
                }
                Or | And => return Err(EE::UnsupportedBinaryOp),
            },
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
            _ => return Err(EE::UnsupportedBinaryOp),
        })
    }
}

/// Represents an integer value for constant evaluation.
#[derive(Debug)]
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

#[derive(Clone, Debug)]
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

    pub(crate) fn msg(&self) -> &'static str {
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

#[derive(Clone, Debug)]
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

#[cfg(test)]
mod tests {
    use super::{ConstValue, IntScalar, erc7201_slot};
    use crate::{Compiler, hir};
    use alloy_primitives::{U256, b256};
    use solar_interface::{Session, config::CompileOpts};
    use std::{ops::ControlFlow, path::PathBuf};

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

    #[test]
    fn evaluates_type_bounds_in_constant_expressions() {
        const SOURCE: &str = r#"
contract C {
    uint8 constant U8_MAX = type(uint8).max;
    int8 constant I8_MIN = type(int8).min;
    uint256 constant COMPOSED = type(uint8).max * 2 + 1;

    function wrapped() external pure returns (uint256) {
        unchecked {
            return type(uint256).max + 1;
        }
    }
}
"#;

        let sess = Session::builder().opts(CompileOpts::default()).with_test_emitter().build();
        let mut compiler = Compiler::new(sess);
        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            let file =
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), SOURCE).unwrap();
            pcx.add_file(file);
            pcx.parse();
            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        compiler.enter(|c| {
            let gcx = c.gcx();
            let value = |name: &str| {
                let variable = gcx
                    .hir
                    .variable_ids()
                    .map(|id| gcx.hir.variable(id))
                    .find(|variable| variable.name.is_some_and(|ident| ident.as_str() == name))
                    .unwrap();
                gcx.try_eval_const(variable.initializer.unwrap()).unwrap()
            };

            assert_eq!(value("U8_MAX").as_u256(), Some(U256::from(255)));
            assert!(value("I8_MIN").is_negative());
            assert_eq!(value("COMPOSED").as_u256(), Some(U256::from(511)));

            let wrapped = gcx
                .hir
                .function_ids()
                .find(|&id| gcx.item_canonical_name(id).to_string() == "C.wrapped")
                .map(|id| gcx.hir.function(id))
                .unwrap();
            let [stmt] = wrapped.body.unwrap().stmts else { panic!("expected unchecked block") };
            let hir::StmtKind::UncheckedBlock(block) = stmt.kind else {
                panic!("expected unchecked block")
            };
            let [stmt] = block.stmts else { panic!("expected return statement") };
            let hir::StmtKind::Return(Some(expr)) = stmt.kind else {
                panic!("expected return statement")
            };
            assert_eq!(gcx.try_eval_const_u256_wrapping(expr), Some(U256::ZERO));
        });
    }
}

use crate::{builtins::Builtin, hir, ty::Gcx};
use alloy_primitives::{B256, U256, keccak256};
use num_bigint::{BigInt, BigUint, Sign};
use num_traits::{One, Signed, Zero};
use solar_ast::{LitKind, StrKind};
use solar_interface::{ByteSymbol, Span, diagnostics::ErrorGuaranteed};
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
    match ConstantEvaluator::new(gcx).eval(size) {
        Ok(int) => {
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
        Err(guar) => Err(guar),
    }
}

impl<'gcx> Gcx<'gcx> {
    /// Attempts to evaluate a literal expression without emitting diagnostics.
    ///
    /// Returns `None` for non-literal expressions and unsupported literals.
    pub fn eval_const(self, expr: &hir::Expr<'_>) -> Option<ConstValue> {
        let hir::ExprKind::Lit(lit) = expr.peel_parens().kind else { return None };
        try_eval_lit(lit).ok()
    }
}

/// Evaluates Solidity constant expressions.
///
/// This supports the source-level constants needed by semantic analysis and
/// codegen's HIR lowering pre-folds. It does not evaluate runtime-dependent
/// expressions such as function calls or memory allocation.
pub struct ConstantEvaluator<'gcx> {
    pub gcx: Gcx<'gcx>,
    depth: usize,
}

type EvalResult = Result<ConstValue, EvalError>;

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
    pub fn try_eval(&mut self, expr: &hir::Expr<'_>) -> Result<IntScalar, EvalError> {
        self.try_eval_value(expr).and_then(ConstValue::into_integer)
    }

    /// Evaluates the given expression to a typed constant value, emitting an
    /// error diagnostic if it fails.
    pub fn eval_value(&mut self, expr: &hir::Expr<'_>) -> Result<ConstValue, ErrorGuaranteed> {
        self.try_eval_value(expr).map_err(|err| self.emit_eval_error(expr, err))
    }

    /// Evaluates the given expression to a typed constant value, returning an
    /// error if it fails.
    pub fn try_eval_value(&mut self, expr: &hir::Expr<'_>) -> EvalResult {
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
                self.gcx.dcx().emit_err_label(expr.span, msg, err.span, label)
            }
        }
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
            hir::ExprKind::Lit(lit) => try_eval_lit(lit),
            // hir::ExprKind::Member(_, _) => unimplemented!(),
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
}

fn try_eval_lit(lit: &hir::Lit<'_>) -> EvalResult {
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

#[cfg(test)]
mod tests {
    use super::{ConstValue, IntScalar, erc7201_slot};
    use crate::{Compiler, hir, ty::Gcx};
    use alloy_primitives::{U256, address, b256};
    use solar_interface::{ColorChoice, Session};
    use std::path::PathBuf;

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
    fn gcx_eval_const_evaluates_literals_only() {
        let compiler = lower_source(
            r#"
contract Test {
    uint256 number = 42;
    bool boolean = true;
    string text = "hello";
    string unicodeText = unicode"hello";
    address account = 0x52908400098527886E0F7030069857D2E4169EE7;
    uint256 parenthesized = (7);

    uint256 binary = 1 + 2;
    int256 unary = -1;
    uint256 constant named = 9;
    uint256 identifier = named;
    bytes hexString = hex"1234";
}
"#,
        );

        compiler.enter_sequential(|compiler| {
            let gcx = compiler.gcx();

            assert_integer(gcx.eval_const(initializer(gcx, "number")), U256::from(42));
            assert!(matches!(
                gcx.eval_const(initializer(gcx, "boolean")),
                Some(ConstValue::Bool(true))
            ));
            assert_string(gcx.eval_const(initializer(gcx, "text")), b"hello");
            assert_string(gcx.eval_const(initializer(gcx, "unicodeText")), b"hello");
            assert_integer(
                gcx.eval_const(initializer(gcx, "account")),
                U256::from_be_slice(
                    address!("52908400098527886E0F7030069857D2E4169EE7").as_slice(),
                ),
            );
            assert_integer(gcx.eval_const(initializer(gcx, "parenthesized")), U256::from(7));

            for name in ["binary", "unary", "identifier", "hexString"] {
                assert!(gcx.eval_const(initializer(gcx, name)).is_none(), "{name}");
            }
        });
        assert!(compiler.sess().dcx.has_errors().is_ok());
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

    fn assert_integer(value: Option<ConstValue>, expected: U256) {
        let Some(ConstValue::Integer(value)) = value else {
            panic!("expected integer literal, got {value:?}")
        };
        assert_eq!(value.as_u256(), Some(expected));
    }

    fn assert_string(value: Option<ConstValue>, expected: &[u8]) {
        let Some(ConstValue::String(value)) = value else {
            panic!("expected string literal, got {value:?}")
        };
        assert_eq!(value.as_byte_str(), expected);
    }

    fn initializer<'gcx>(gcx: Gcx<'gcx>, name: &str) -> &'gcx hir::Expr<'gcx> {
        gcx.hir
            .variables()
            .find(|variable| variable.name.is_some_and(|variable| variable.as_str() == name))
            .and_then(|variable| variable.initializer)
            .unwrap_or_else(|| panic!("initializer for `{name}` not found"))
    }

    fn lower_source(src: &str) -> Compiler {
        let sess =
            Session::builder().with_buffer_emitter(ColorChoice::Never).single_threaded().build();
        let mut compiler = Compiler::new(sess);

        let _ = compiler.enter_mut(|compiler| -> solar_interface::Result<_> {
            let mut parsing_context = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("test.sol"), src.to_string())
                .unwrap();
            parsing_context.add_file(file);
            parsing_context.parse();
            let _ = compiler.lower_asts()?;
            Ok(())
        });

        compiler
    }
}

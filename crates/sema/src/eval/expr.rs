//! Evaluation of HIR expressions.

use super::{
    ConstEval, EvalError, EvalErrorKind as EE, EvalResult, Mode, builtins,
    value::{BytesKind, CodeKind, ConstValue, IntScalar, IntTy, ValueTy},
};
use crate::{
    builtins::Builtin,
    hir,
    ty::{CallableParamSource, Gcx},
};
use alloy_primitives::U256;
use num_bigint::{BigInt, Sign};
use num_rational::Ratio;
use solar_ast::{ElementaryType, LitKind, LitUint};
use solar_interface::{ByteSymbol, Symbol, sym};
use std::ops::ControlFlow;

/// Evaluates an expression. Subexpressions go through the memoized query.
pub(super) fn eval(cx: ConstEval<'_>, expr: &hir::Expr<'_>) -> EvalResult {
    eval_inner(cx, expr).map_err(|mut err| {
        if err.span.is_dummy() {
            err.span = expr.span;
        }
        err
    })
}

/// Evaluates the initializer of a constant and converts it to the constant's declared type.
pub(super) fn eval_constant(cx: ConstEval<'_>, id: hir::VariableId) -> EvalResult {
    let gcx = cx.gcx;
    let var = gcx.hir.variable(id);
    // The parser reports a constant without an initializer, so an error was already emitted.
    let Some(init) = var.initializer else {
        return Err(already_emitted(gcx).spanned(var.span));
    };
    let value = cx.expr_result(init).clone()?;
    let Some(ty) = ValueTy::from_hir(&var.ty) else {
        return Err(EE::UnsupportedExpr.spanned(init.span));
    };
    // solc's evaluator keeps the values of integer, string and `bytes` constants.
    if cx.mode == Mode::Early && !matches!(ty, ValueTy::Int(_) | ValueTy::Bytes | ValueTy::String) {
        return Err(EE::UnsupportedExpr.spanned(var.ty.span));
    }
    let value = match (value, ty) {
        // An initializer that does not fit its declared type is already a type error at the
        // declaration, so keep the literal instead of reporting a second error at each use.
        (ConstValue::Int(value), ValueTy::Int(_)) if value.is_literal() => {
            ConstValue::Int(value.clone()).implicit_to(gcx, ty).unwrap_or(ConstValue::Int(value))
        }
        (value @ ConstValue::Rational(_), ValueTy::Int(_)) => value,
        (value, ty) => value.implicit_to(gcx, ty).map_err(|kind| kind.spanned(init.span))?,
    };
    Ok(value)
}

fn eval_inner(cx: ConstEval<'_>, expr: &hir::Expr<'_>) -> EvalResult {
    let gcx = cx.gcx;
    let early = cx.mode == Mode::Early;
    let ev = |expr| cx.expr_result(expr).clone();
    match expr.kind {
        hir::ExprKind::Lit(lit) => eval_lit(cx, lit),
        hir::ExprKind::Tuple([Some(inner)]) => ev(inner),
        hir::ExprKind::Ident(_) | hir::ExprKind::Member(..)
            if let Some(res @ (hir::Res::Item(hir::ItemId::Variable(_)) | hir::Res::Err(_))) =
                cx.res(expr) =>
        {
            eval_res(cx, res)
        }
        hir::ExprKind::Unary(op, inner) => {
            let value = ev(inner)?;
            // solc folds `-` and `~` of numbers, and converts `~` of an unsigned value back to its
            // type, which fails.
            if early {
                match (op.kind, &value) {
                    (hir::UnOpKind::BitNot, ConstValue::Int(value)) if value.is_unsigned() => {
                        return Err(EE::UnaryOverflow.into());
                    }
                    (
                        hir::UnOpKind::Neg | hir::UnOpKind::BitNot,
                        ConstValue::Int(_) | ConstValue::Rational(_),
                    ) => {}
                    _ => return Err(EE::UnsupportedUnaryOp.into()),
                }
            }
            Ok(value.unop(op.kind)?)
        }
        hir::ExprKind::Binary(lhs, op, rhs) => {
            // Evaluate both operands, even of `&&` and `||`; see the module docs.
            let (l, r) = both(ev(lhs), ev(rhs))?;
            let mut solc = Ok(());
            if early {
                if op.kind.is_cmp() || op.kind.is_logical() {
                    return Err(EE::UnsupportedBinaryOp.into());
                }
                if let (ConstValue::Int(l), ConstValue::Int(r)) = (&l, &r) {
                    solc = l.solc_binop(r, op.kind);
                }
            } else if let (hir::BinOpKind::And, Some(false)) | (hir::BinOpKind::Or, Some(true)) =
                (op.kind, l.as_bool())
            {
                return Ok(l);
            }
            // Only an untyped literal next to a typed operand can be too large.
            let literal =
                if matches!(&l, ConstValue::Int(l) if l.is_literal()) { lhs } else { rhs };
            // A literal too large for its operation is a type error, which comes first, then solc's
            // error, if any.
            match (l.binop(r, op.kind), solc) {
                (Err(EE::LiteralTooLarge), _) => Err(EE::LiteralTooLarge.spanned(literal.span)),
                (_, Err(kind)) | (Err(kind), _) => Err(kind.into()),
                (Ok(value), Ok(())) => Ok(value),
            }
        }
        hir::ExprKind::Call(callee, args) if early => match (cx.res(callee), args.kind) {
            (Some(hir::Res::Builtin(Builtin::Erc7201)), hir::CallArgsKind::Unnamed([arg])) => {
                Ok(builtins::call(gcx, Builtin::Erc7201, &[ev(arg)?])?)
            }
            _ => unsupported_early(cx, expr),
        },
        hir::ExprKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
        _ if early => unsupported_early(cx, expr),
        hir::ExprKind::Member(base, member) => eval_member(cx, base, member.name),
        hir::ExprKind::Ternary(cond, then, els) => {
            let (cond, (then, els)) = both(ev(cond), both(ev(then), ev(els)))?;
            let Some(cond) = cond.as_bool() else {
                return Err(EE::UnsupportedExpr.into());
            };
            // Both branches decide the result type.
            let (then, els) = then.unify(gcx, els)?;
            Ok(if cond { then } else { els })
        }
        hir::ExprKind::Call(callee, args) if let hir::ExprKind::New(ty) = &callee.kind => {
            eval_new(cx, ty, &args)
        }
        hir::ExprKind::Call(callee, args) => eval_call(cx, callee, &args),
        hir::ExprKind::Payable(inner) => Ok(ev(inner)?.explicit_to(gcx, ValueTy::Address)?),
        hir::ExprKind::Index(base, Some(index)) => {
            let (base, index) = both(ev(base), ev(index))?;
            let index = index.as_u256().ok_or(EE::OutOfRange)?;
            let bytes = match &base {
                ConstValue::FixedBytes(value) => value.as_slice(),
                ConstValue::Bytes(data, BytesKind::Bytes) => data.as_byte_str(),
                _ => return Err(EE::UnsupportedExpr.into()),
            };
            let byte = usize::try_from(index).ok().and_then(|index| bytes.get(index));
            Ok(builtins::fixed_bytes(&[*byte.ok_or(EE::OutOfRange)?]))
        }
        _ => Err(EE::UnsupportedExpr.into()),
    }
}

/// Returns the error of an expression that solc's evaluator does not compute.
///
/// solc's evaluator still visits its subexpressions, so it reports their errors, such as the `1 /
/// 0` in `true ? 1 : 1 / 0`.
fn unsupported_early(cx: ConstEval<'_>, expr: &hir::Expr<'_>) -> EvalResult {
    let mut result = Err(EE::UnsupportedExpr.into());
    let _ = expr.visit(&mut |sub| {
        if sub.id != expr.id
            && let Err(err) = cx.expr_result(sub)
            && !err.kind.is_unsupported()
        {
            result = Err(*err);
            return ControlFlow::Break(());
        }
        ControlFlow::Continue(())
    });
    result
}

fn eval_lit(cx: ConstEval<'_>, lit: &hir::Lit<'_>) -> EvalResult {
    let value = match lit.kind {
        LitKind::Number(n) => ConstValue::Int(IntScalar::new(n)),
        LitKind::BigNumber(n) => ConstValue::literal(lit_uint(n).into()),
        LitKind::Rational(numer, denom) => {
            ConstValue::Rational(Box::new(Ratio::new_raw(lit_uint(numer), lit_uint(denom))))
        }
        LitKind::Str(_, s, _) => ConstValue::Bytes(s, BytesKind::Literal),
        LitKind::Err(guar) => return Err(EE::AlreadyEmitted(guar).into()),
        // solc's evaluator only computes number and string literals.
        _ if cx.mode == Mode::Early => return Err(EE::UnsupportedExpr.into()),
        LitKind::Address(address) => ConstValue::Address(address),
        LitKind::Bool(value) => ConstValue::Bool(value),
    };
    Ok(value)
}

fn lit_uint(n: LitUint) -> BigInt {
    BigInt::from_bytes_be(Sign::Plus, n.to_be_bytes())
}

/// Evaluates a name of a variable.
fn eval_res(cx: ConstEval<'_>, res: hir::Res) -> EvalResult {
    let gcx = cx.gcx;
    match res {
        hir::Res::Item(hir::ItemId::Variable(id)) if gcx.hir.variable(id).is_constant() => {
            cx.var_result(id).clone()
        }
        hir::Res::Item(hir::ItemId::Variable(id))
            if let Some(hir::ItemId::Enum(enum_id)) = gcx.hir.variable(id).parent
                && let Some(index) = res.enum_variant_index(&gcx.hir) =>
        {
            Ok(ConstValue::Enum(enum_id, index as u32))
        }
        hir::Res::Item(hir::ItemId::Variable(_)) => Err(EE::NonConstantVar.into()),
        hir::Res::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
        _ => unreachable!("not a variable: {res:?}"),
    }
}

/// Evaluates a member access that does not name a variable.
fn eval_member(cx: ConstEval<'_>, base: &hir::Expr<'_>, member: Symbol) -> EvalResult {
    let gcx = cx.gcx;
    let res = cx.res(base);
    let value = match (&base.kind, member) {
        (hir::ExprKind::TypeCall(ty), sym::min | sym::max) => {
            let max = member == sym::max;
            match ValueTy::from_hir(ty) {
                Some(ValueTy::Int(ty)) => {
                    ConstValue::Int(IntScalar::typed(if max { ty.max() } else { ty.min() }, ty))
                }
                Some(ValueTy::Enum(id)) => {
                    let len = gcx.hir.enumm(id).variants.len() as u32;
                    // An enum without variants is already an error.
                    let Some(last) = len.checked_sub(1) else {
                        return Err(already_emitted(gcx).into());
                    };
                    ConstValue::Enum(id, if max { last } else { 0 })
                }
                _ => return Err(EE::UnsupportedExpr.into()),
            }
        }
        (hir::ExprKind::TypeCall(ty), sym::creationCode | sym::runtimeCode) => {
            let hir::TypeKind::Custom(hir::ItemId::Contract(id)) = ty.kind else {
                return Err(EE::UnsupportedExpr.into());
            };
            let kind =
                if member == sym::creationCode { CodeKind::Creation } else { CodeKind::Runtime };
            ConstValue::Code(id, kind)
        }
        (hir::ExprKind::TypeCall(ty), sym::name | sym::interfaceId) => {
            let hir::TypeKind::Custom(hir::ItemId::Contract(id)) = ty.kind else {
                return Err(EE::UnsupportedExpr.into());
            };
            let contract = gcx.hir.contract(id);
            if member == sym::name {
                let name = contract.name.as_str();
                ConstValue::Bytes(ByteSymbol::intern(name.as_bytes()), BytesKind::String)
            } else if contract.kind.is_interface() {
                builtins::fixed_bytes(&gcx.interface_id(id).0)
            } else {
                return Err(EE::UnsupportedExpr.into());
            }
        }
        // Enum variants come first, as one can be named `selector`.
        _ if let Some(hir::Res::Item(hir::ItemId::Enum(id))) = res => {
            let variants = gcx.hir.enumm(id).variants;
            let index = variants.iter().position(|&variant| {
                gcx.hir.variable(variant).name.is_some_and(|name| name.name == member)
            });
            ConstValue::Enum(id, index.ok_or(EE::UnsupportedExpr)? as u32)
        }
        (_, sym::selector) => match res {
            Some(hir::Res::Item(id)) if has_selector(gcx, id) => match id {
                hir::ItemId::Event(id) => builtins::fixed_bytes(&gcx.event_selector(id).0),
                _ => builtins::fixed_bytes(&gcx.function_selector(id).0),
            },
            _ => return Err(EE::UnsupportedExpr.into()),
        },
        (_, sym::length) => match cx.expr_result(base).clone()? {
            ConstValue::FixedBytes(value) => {
                ConstValue::Int(IntScalar::typed(value.size().into(), IntTy::UINT8))
            }
            ConstValue::Bytes(data, BytesKind::Bytes) => {
                ConstValue::Int(IntScalar::uint256(U256::from(data.as_byte_str().len())))
            }
            _ => return Err(EE::UnsupportedExpr.into()),
        },
        _ => return Err(EE::UnsupportedExpr.into()),
    };
    Ok(value)
}

/// Evaluates a call: a type conversion, a pure builtin, `abi.encodeCall`, `wrap` or `unwrap` of a
/// user-defined value type, or a struct constructor, the only call with named arguments.
fn eval_call(
    cx: ConstEval<'_>,
    callee: &hir::Expr<'_>,
    call_args: &hir::CallArgs<'_>,
) -> EvalResult {
    enum Callee {
        Convert(ValueTy),
        Builtin(Builtin),
        Wrap(hir::UdvtId),
        Unwrap(hir::UdvtId),
    }

    let gcx = cx.gcx;
    let elementary = |expr: &hir::Expr<'_>| match expr.kind {
        hir::ExprKind::Type(hir::Type { kind: hir::TypeKind::Elementary(ty), .. }) => Some(ty),
        _ => None,
    };
    // Only struct constructors take named arguments.
    let args = match call_args.kind {
        hir::CallArgsKind::Unnamed(args) => Ok(args),
        hir::CallArgsKind::Named(_) => Err(EE::UnsupportedExpr),
    };
    let callee = match &callee.kind {
        hir::ExprKind::Type(ty) => {
            Callee::Convert(ValueTy::from_hir(ty).ok_or(EE::UnsupportedExpr)?)
        }
        hir::ExprKind::Member(base, member) if member.name == sym::concat => match elementary(base)
        {
            Some(ElementaryType::Bytes) => Callee::Builtin(Builtin::BytesConcat),
            Some(ElementaryType::String) => Callee::Builtin(Builtin::StringConcat),
            _ => return Err(EE::UnsupportedExpr.into()),
        },
        hir::ExprKind::Member(base, member)
            if let Some(hir::Res::Item(hir::ItemId::Udvt(id))) = cx.res(base) =>
        {
            match member.name {
                sym::wrap => Callee::Wrap(id),
                sym::unwrap => Callee::Unwrap(id),
                _ => return Err(EE::UnsupportedExpr.into()),
            }
        }
        _ => match cx.res(callee) {
            Some(hir::Res::Item(hir::ItemId::Contract(id))) => {
                Callee::Convert(ValueTy::Contract(id))
            }
            Some(hir::Res::Item(hir::ItemId::Enum(id))) => Callee::Convert(ValueTy::Enum(id)),
            Some(hir::Res::Item(hir::ItemId::Struct(id))) => {
                return eval_struct(cx, id, call_args);
            }
            Some(hir::Res::Builtin(Builtin::AbiEncodeCall)) => {
                return eval_encode_call(cx, args?);
            }
            Some(hir::Res::Builtin(builtin)) => Callee::Builtin(builtin),
            _ => return Err(EE::UnsupportedExpr.into()),
        },
    };

    let mut args = operands(args?.iter().map(|arg| cx.expr_result(arg).clone()))?;
    let single = |args: &mut Vec<ConstValue>| match args[..] {
        [_] => Ok(args.pop().unwrap()),
        _ => Err(EE::UnsupportedExpr),
    };
    let value = match callee {
        Callee::Convert(ty) => single(&mut args)?.explicit_to(gcx, ty)?,
        Callee::Wrap(id) => {
            let underlying = ValueTy::from_hir(&gcx.hir.udvt(id).ty).ok_or(EE::UnsupportedExpr)?;
            let value = single(&mut args)?.implicit_to(gcx, underlying)?;
            ConstValue::Udvt(id, Box::new(value))
        }
        Callee::Unwrap(id) => match single(&mut args)? {
            ConstValue::Udvt(udvt, value) if udvt == id => *value,
            _ => return Err(EE::UnsupportedExpr.into()),
        },
        Callee::Builtin(builtin) => builtins::call(gcx, builtin, &args)?,
    };
    Ok(value)
}

/// Evaluates `new T(len)` of a byte array or a dynamic array, which holds zero values.
fn eval_new(cx: ConstEval<'_>, ty: &hir::Type<'_>, args: &hir::CallArgs<'_>) -> EvalResult {
    let gcx = cx.gcx;
    let hir::CallArgsKind::Unnamed([len]) = args.kind else {
        return Err(EE::UnsupportedExpr.into());
    };
    let len = cx.eval_int(len)?.as_u256().ok_or(EE::OutOfRange)?;
    let len = usize::try_from(len).map_err(|_| EE::ValueTooLarge)?;
    let value = match ty.kind {
        hir::TypeKind::Elementary(
            elementary @ (ElementaryType::Bytes | ElementaryType::String),
        ) => {
            if len > builtins::MAX_BYTES {
                return Err(EE::ValueTooLarge.into());
            }
            let kind = if elementary == ElementaryType::Bytes {
                BytesKind::Bytes
            } else {
                BytesKind::String
            };
            builtins::bytes_value(vec![0; len], kind)?
        }
        hir::TypeKind::Array(hir::TypeArray { element, size: None }) => {
            let mut budget = builtins::MAX_VALUES;
            ConstValue::Array {
                elements: zero_values(gcx, element, len, &mut budget)?,
                dynamic: true,
            }
        }
        _ => return Err(EE::UnsupportedExpr.into()),
    };
    Ok(value)
}

/// Returns the zero value of a type in memory, counting the values it builds against `budget`.
fn zero_value(gcx: Gcx<'_>, ty: &hir::Type<'_>, budget: &mut usize) -> Result<ConstValue, EE> {
    *budget = budget.checked_sub(1).ok_or(EE::ValueTooLarge)?;
    Ok(match ty.kind {
        hir::TypeKind::Custom(hir::ItemId::Struct(id)) => {
            let fields = gcx.hir.strukt(id).fields;
            let fields =
                fields.iter().map(|&field| zero_value(gcx, &gcx.hir.variable(field).ty, budget));
            ConstValue::Struct(id, fields.collect::<Result<_, _>>()?)
        }
        hir::TypeKind::Array(hir::TypeArray { size: None, .. }) => {
            ConstValue::Array { elements: Box::new([]), dynamic: true }
        }
        hir::TypeKind::Array(hir::TypeArray { element, size: Some(size) }) => {
            // Type checking reported the errors of the length.
            let len = gcx.early_const_eval().eval_array_len(size).map_err(EE::AlreadyEmitted)?;
            let len = usize::try_from(len).map_err(|_| EE::ValueTooLarge)?;
            ConstValue::Array { elements: zero_values(gcx, element, len, budget)?, dynamic: false }
        }
        _ => ValueTy::from_hir(ty).ok_or(EE::UnsupportedExpr)?.zero(gcx)?,
    })
}

/// Returns `len` zero values of a type, counting them against `budget`.
fn zero_values(
    gcx: Gcx<'_>,
    ty: &hir::Type<'_>,
    len: usize,
    budget: &mut usize,
) -> Result<Box<[ConstValue]>, EE> {
    if len == 0 {
        return Ok(Box::new([]));
    }
    let before = *budget;
    let zero = zero_value(gcx, ty, budget)?;
    let per_value = before - *budget;
    let rest = per_value.checked_mul(len - 1).ok_or(EE::ValueTooLarge)?;
    *budget = budget.checked_sub(rest).ok_or(EE::ValueTooLarge)?;
    Ok(vec![zero; len].into())
}

/// Evaluates a struct constructor, converting the arguments to the field types.
fn eval_struct(cx: ConstEval<'_>, id: hir::StructId, args: &hir::CallArgs<'_>) -> EvalResult {
    let gcx = cx.gcx;
    let fields = gcx.hir.strukt(id).fields;
    if args.len() != fields.len() {
        return Err(EE::UnsupportedExpr.into());
    }
    let names = gcx.callable_param_names(CallableParamSource::Struct(id));
    let args = operands::<_, Vec<_>>((0..fields.len()).map(|index| {
        let arg = args.argument_for_parameter(index, Some(&names)).ok_or(EE::UnsupportedExpr)?;
        cx.expr_result(arg).clone()
    }))?;
    let values = std::iter::zip(fields, args)
        .map(|(&field, arg)| implicit_to_declared(gcx, arg, &gcx.hir.variable(field).ty))
        .collect::<Result<_, _>>()?;
    Ok(ConstValue::Struct(id, values))
}

/// Converts a value implicitly to a declared type, like an assignment. Structs and arrays keep
/// their values, as type checking checks their types.
fn implicit_to_declared(
    gcx: Gcx<'_>,
    value: ConstValue,
    ty: &hir::Type<'_>,
) -> Result<ConstValue, EE> {
    match ValueTy::from_hir(ty) {
        Some(ty) => value.implicit_to(gcx, ty),
        None if matches!(value, ConstValue::Struct(..) | ConstValue::Array { .. }) => Ok(value),
        None => Err(EE::UnsupportedExpr),
    }
}

/// Evaluates `abi.encodeCall(f, (args))`: the selector of `f` followed by the ABI encoding of the
/// arguments in its parameter types.
fn eval_encode_call(cx: ConstEval<'_>, args: &[hir::Expr<'_>]) -> EvalResult {
    let gcx = cx.gcx;
    let [function, call_args] = args else { return Err(EE::UnsupportedExpr.into()) };
    let Some(hir::Res::Item(id @ hir::ItemId::Function(f))) = cx.res(function) else {
        return Err(EE::UnsupportedExpr.into());
    };
    if !has_selector(gcx, id) {
        return Err(EE::UnsupportedExpr.into());
    }
    let call_args = match &call_args.kind {
        hir::ExprKind::Tuple(args) => {
            args.iter().map(|arg| arg.ok_or(EE::UnsupportedExpr)).collect()
        }
        _ => Ok(vec![call_args]),
    }?;
    let params = gcx.hir.function(f).parameters;
    if params.len() != call_args.len() {
        return Err(EE::UnsupportedExpr.into());
    }
    let args = operands::<_, Vec<_>>(call_args.iter().map(|arg| cx.expr_result(arg).clone()))?;
    let mut values = vec![builtins::fixed_bytes(&gcx.function_selector(id).0)];
    for (&param, arg) in std::iter::zip(params, args) {
        values.push(implicit_to_declared(gcx, arg, &gcx.hir.variable(param).ty)?);
    }
    Ok(builtins::call(gcx, Builtin::AbiEncodeWithSelector, &values)?)
}

/// Returns whether the given item has a selector: a public or external function, an error or a
/// non-anonymous event.
///
/// Type checking reports `.selector` of other items, and signatures with recursive types.
fn has_selector(gcx: Gcx<'_>, id: hir::ItemId) -> bool {
    let kind_has_selector = match id {
        hir::ItemId::Function(id) => gcx.hir.function(id).is_part_of_external_interface(),
        hir::ItemId::Error(_) => true,
        hir::ItemId::Event(id) => !gcx.hir.event(id).anonymous,
        _ => false,
    };
    kind_has_selector && gcx.has_abi_signature(id)
}

/// Returns the values of two operands that both must evaluate, see [`operands`].
fn both<T, U>(l: Result<T, EvalError>, r: Result<U, EvalError>) -> Result<(T, U), EvalError> {
    match (l, r) {
        (Ok(l), Ok(r)) => Ok((l, r)),
        (Err(l), Err(r)) if l.kind.is_unsupported() && !r.kind.is_unsupported() => Err(r),
        (Err(err), _) | (_, Err(err)) => Err(err),
    }
}

/// Returns the values of operands that all must evaluate. Of several errors, it returns the first
/// that is not [`EE::is_unsupported`], so the reported error does not depend on the order of the
/// operands.
fn operands<T, C: FromIterator<T>>(
    results: impl IntoIterator<Item = Result<T, EvalError>>,
) -> Result<C, EvalError> {
    let mut error = None::<EvalError>;
    let values = results
        .into_iter()
        .filter_map(|result| {
            result
                .inspect_err(|&err| {
                    if error
                        .is_none_or(|prev| prev.kind.is_unsupported() && !err.kind.is_unsupported())
                    {
                        error = Some(err);
                    }
                })
                .ok()
        })
        .collect();
    error.map_or(Ok(values), Err)
}

/// Returns the error for an expression that an earlier error already rejects.
fn already_emitted(gcx: Gcx<'_>) -> EE {
    match gcx.dcx().has_errors() {
        Err(guar) => EE::AlreadyEmitted(guar),
        Ok(()) => EE::UnsupportedExpr,
    }
}

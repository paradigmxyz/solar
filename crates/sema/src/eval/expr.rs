//! Evaluation of HIR expressions.

use super::{
    EvalError, EvalErrorKind as EE, EvalResult, Folded, builtins,
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
use solar_data_structures::map::FxHashSet;
use solar_interface::{ByteSymbol, Symbol, sym};

/// Evaluates an expression. Subexpressions go through the memoized query.
pub(super) fn eval(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> EvalResult {
    eval_inner(gcx, expr).map_err(|mut err| {
        if err.span.is_dummy() {
            err.span = expr.span;
        }
        err
    })
}

/// Evaluates the initializer of a constant and converts it to the constant's declared type.
pub(super) fn eval_constant(gcx: Gcx<'_>, id: hir::VariableId) -> EvalResult {
    let var = gcx.hir.variable(id);
    // The parser reports a constant without an initializer, so an error was already emitted.
    let Some(init) = var.initializer else {
        return Err(already_emitted(gcx).spanned(var.span));
    };
    let Folded { value, solc } = gcx.const_eval().expr_result(init).clone()?;
    let Some(ty) = ValueTy::from_hir(&var.ty) else {
        return Err(EE::UnsupportedExpr.spanned(init.span));
    };
    // solc's evaluator keeps the values of integer and string constants.
    let solc = solc
        .filter(|_| matches!(ty, ValueTy::Int(_) | ValueTy::Bytes | ValueTy::String))
        .map(|depth| depth + 1);
    let value = match (value, ty) {
        // An initializer that does not fit its declared type is already a type error at the
        // declaration, so keep the literal instead of reporting a second error at each use.
        (ConstValue::Int(value), ValueTy::Int(_)) if value.is_literal() => {
            ConstValue::Int(value.clone()).implicit_to(gcx, ty).unwrap_or(ConstValue::Int(value))
        }
        (value @ ConstValue::Rational(_), ValueTy::Int(_)) => value,
        (value, ty) => value.implicit_to(gcx, ty).map_err(|kind| kind.spanned(init.span))?,
    };
    Ok(Folded::solc(value, solc))
}

fn eval_inner(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> EvalResult {
    let ev = |expr| gcx.const_eval().expr_result(expr).clone();
    match expr.kind {
        hir::ExprKind::Lit(lit) => eval_lit(lit),
        hir::ExprKind::Tuple([Some(inner)]) => ev(inner),
        hir::ExprKind::Ident(_) if let Some(res) = gcx.resolve_path_expr(expr) => {
            eval_res(gcx, res)
        }
        // solc's evaluator only follows identifiers.
        hir::ExprKind::Member(..) if let Some(res) = gcx.resolve_path_expr(expr) => {
            eval_res(gcx, res)
                .map(|folded| Folded::new(folded.value))
                .map_err(|err| err.solc(false))
        }
        hir::ExprKind::Member(base, member) => eval_member(gcx, base, member.name),
        hir::ExprKind::Unary(op, inner) => {
            let Folded { value, solc } = ev(inner)?;
            // solc folds `-` and `~` of numbers, except `~` of unsigned values, which it rejects.
            let solc = solc.filter(|_| match (op.kind, &value) {
                (hir::UnOpKind::BitNot, ConstValue::Int(value)) => !value.is_unsigned(),
                (hir::UnOpKind::Neg | hir::UnOpKind::BitNot, value) => {
                    matches!(value, ConstValue::Int(_) | ConstValue::Rational(_))
                }
                _ => false,
            });
            let value =
                value.unop(op.kind).map_err(|kind| EvalError::from(kind).solc(solc.is_some()))?;
            Ok(Folded::solc(value, solc))
        }
        hir::ExprKind::Binary(lhs, op, rhs) => {
            // Evaluate both operands, even of `&&` and `||`; see the module docs.
            let (l, r) = both(ev(lhs), ev(rhs))?;
            if let (hir::BinOpKind::And, Some(false)) | (hir::BinOpKind::Or, Some(true)) =
                (op.kind, l.value.as_bool())
            {
                return Ok(Folded::new(l.value));
            }
            let folds = !op.kind.is_cmp()
                && !matches!(op.kind, hir::BinOpKind::And | hir::BinOpKind::Or)
                && match (&l.value, &r.value) {
                    (ConstValue::Int(l), ConstValue::Int(r)) => l.solc_folds_binop(r, op.kind),
                    (ConstValue::Rational(_), _) | (_, ConstValue::Rational(_)) => true,
                    _ => false,
                };
            let solc = l.solc.zip(r.solc).filter(|_| folds).map(|(l, r)| l.max(r));
            // Only an untyped literal next to a typed operand can be too large.
            let literal =
                if matches!(&l.value, ConstValue::Int(l) if l.is_literal()) { lhs } else { rhs };
            let value = l.value.binop(r.value, op.kind).map_err(|kind| match kind {
                EE::LiteralTooLarge => kind.spanned(literal.span),
                kind => EvalError::from(kind).solc(solc.is_some()),
            })?;
            Ok(Folded::solc(value, solc))
        }
        hir::ExprKind::Ternary(cond, then, els) => {
            let (cond, (then, els)) = both(ev(cond), both(ev(then), ev(els)))?;
            let Some(cond) = cond.value.as_bool() else {
                return Err(EE::UnsupportedExpr.into());
            };
            // Both branches decide the result type.
            let (then, els) = then.value.unify(gcx, els.value)?;
            Ok(Folded::new(if cond { then } else { els }))
        }
        hir::ExprKind::Call(callee, args) if let hir::ExprKind::New(ty) = &callee.kind => {
            eval_new(gcx, ty, &args)
        }
        hir::ExprKind::Call(callee, args) => eval_call(gcx, callee, &args),
        hir::ExprKind::Payable(inner) => {
            Ok(Folded::new(ev(inner)?.value.explicit_to(gcx, ValueTy::Address)?))
        }
        hir::ExprKind::Index(base, Some(index)) => {
            let (base, index) = both(ev(base), ev(index))?;
            let (base, index) = (base.value, index.value.as_u256().ok_or(EE::OutOfRange)?);
            let bytes = match &base {
                ConstValue::FixedBytes(value) => value.as_slice(),
                ConstValue::Bytes(data, BytesKind::Bytes) => data.as_byte_str(),
                _ => return Err(EE::UnsupportedExpr.into()),
            };
            let byte = usize::try_from(index).ok().and_then(|index| bytes.get(index));
            Ok(Folded::new(builtins::fixed_bytes(&[*byte.ok_or(EE::OutOfRange)?])))
        }
        hir::ExprKind::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
        _ => Err(EE::UnsupportedExpr.into()),
    }
}

fn eval_lit(lit: &hir::Lit<'_>) -> EvalResult {
    Ok(match lit.kind {
        LitKind::Number(n) => Folded::solc(ConstValue::Int(IntScalar::new(n)), Some(0)),
        LitKind::BigNumber(n) => Folded::solc(ConstValue::literal(lit_uint(n).into()), Some(0)),
        LitKind::Rational(numer, denom) => {
            let value = Ratio::new_raw(lit_uint(numer), lit_uint(denom));
            Folded::solc(ConstValue::Rational(Box::new(value)), Some(0))
        }
        LitKind::Str(_, s, _) => Folded::solc(ConstValue::Bytes(s, BytesKind::Literal), Some(0)),
        LitKind::Address(address) => Folded::new(ConstValue::Address(address)),
        LitKind::Bool(value) => Folded::new(ConstValue::Bool(value)),
        LitKind::Err(guar) => return Err(EE::AlreadyEmitted(guar).into()),
    })
}

fn lit_uint(n: LitUint) -> BigInt {
    BigInt::from_bytes_be(Sign::Plus, n.to_be_bytes())
}

/// Evaluates a name.
fn eval_res(gcx: Gcx<'_>, res: hir::Res) -> EvalResult {
    match res {
        hir::Res::Item(hir::ItemId::Variable(id)) if gcx.hir.variable(id).is_constant() => {
            gcx.const_eval().var_result(id).clone()
        }
        hir::Res::Item(hir::ItemId::Variable(_)) => Err(EE::NonConstantVar.into()),
        hir::Res::Err(guar) => Err(EE::AlreadyEmitted(guar).into()),
        _ => Err(EE::UnsupportedExpr.into()),
    }
}

/// Evaluates a member access that does not name a declaration.
fn eval_member(gcx: Gcx<'_>, base: &hir::Expr<'_>, member: Symbol) -> EvalResult {
    let res = gcx.resolve_path_expr(base);
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
            } else if contract.kind.is_interface()
                && !contract.linearized_bases.iter().any(|&base| {
                    gcx.hir
                        .contract(base)
                        .functions()
                        .any(|f| signature_needs_constants(gcx, hir::ItemId::Function(f)))
                })
            {
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
        (_, sym::length) => match gcx.const_eval().expr_result(base).clone()?.value {
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
    Ok(Folded::new(value))
}

/// Evaluates a call: a type conversion, a pure builtin, `abi.encodeCall`, `wrap` or `unwrap` of a
/// user-defined value type, or a struct constructor, the only call with named arguments.
fn eval_call(gcx: Gcx<'_>, callee: &hir::Expr<'_>, call_args: &hir::CallArgs<'_>) -> EvalResult {
    enum Callee {
        Convert(ValueTy),
        Builtin(Builtin),
        Wrap(hir::UdvtId),
        Unwrap(hir::UdvtId),
    }

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
            if let Some(hir::Res::Item(hir::ItemId::Udvt(id))) = gcx.resolve_path_expr(base) =>
        {
            match member.name {
                sym::wrap => Callee::Wrap(id),
                sym::unwrap => Callee::Unwrap(id),
                _ => return Err(EE::UnsupportedExpr.into()),
            }
        }
        _ => match gcx.resolve_path_expr(callee) {
            Some(hir::Res::Item(hir::ItemId::Contract(id))) => {
                Callee::Convert(ValueTy::Contract(id))
            }
            Some(hir::Res::Item(hir::ItemId::Enum(id))) => Callee::Convert(ValueTy::Enum(id)),
            Some(hir::Res::Item(hir::ItemId::Struct(id))) => {
                return eval_struct(gcx, id, call_args);
            }
            Some(hir::Res::Builtin(Builtin::AbiEncodeCall)) => {
                return eval_encode_call(gcx, args?);
            }
            Some(hir::Res::Builtin(builtin)) => Callee::Builtin(builtin),
            _ => return Err(EE::UnsupportedExpr.into()),
        },
    };

    let mut args = operands(args?.iter().map(|arg| gcx.const_eval().expr_result(arg).clone()))?;
    let single = |args: &mut Vec<Folded>| match args[..] {
        [_] => Ok(args.pop().unwrap().value),
        _ => Err(EE::UnsupportedExpr),
    };
    Ok(match callee {
        Callee::Convert(ty) => Folded::new(single(&mut args)?.explicit_to(gcx, ty)?),
        Callee::Wrap(id) => {
            let underlying = ValueTy::from_hir(&gcx.hir.udvt(id).ty).ok_or(EE::UnsupportedExpr)?;
            let value = single(&mut args)?.implicit_to(gcx, underlying)?;
            Folded::new(ConstValue::Udvt(id, Box::new(value)))
        }
        Callee::Unwrap(id) => match single(&mut args)? {
            ConstValue::Udvt(udvt, value) if udvt == id => Folded::new(*value),
            _ => return Err(EE::UnsupportedExpr.into()),
        },
        Callee::Builtin(builtin) => {
            // solc folds `erc7201` of a string it folds.
            let solc = match &args[..] {
                [arg] if builtin == Builtin::Erc7201 => arg.solc,
                _ => None,
            };
            let args = args.into_iter().map(|arg| arg.value).collect::<Vec<_>>();
            Folded::solc(builtins::call(gcx, builtin, &args)?, solc)
        }
    })
}

/// Evaluates `new T(len)` of a byte array or a dynamic array, which holds zero values.
fn eval_new(gcx: Gcx<'_>, ty: &hir::Type<'_>, args: &hir::CallArgs<'_>) -> EvalResult {
    let hir::CallArgsKind::Unnamed([len]) = args.kind else {
        return Err(EE::UnsupportedExpr.into());
    };
    let len = gcx.const_eval().eval_int(len)?.as_u256().ok_or(EE::OutOfRange)?;
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
    Ok(Folded::new(value))
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
            // Propagate the length's error, such as a cycle.
            let len = gcx.const_eval().eval_int(size).map_err(|err| err.kind)?.as_u256();
            let len = len.and_then(|len| usize::try_from(len).ok()).ok_or(EE::UnsupportedExpr)?;
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
fn eval_struct(gcx: Gcx<'_>, id: hir::StructId, args: &hir::CallArgs<'_>) -> EvalResult {
    let fields = gcx.hir.strukt(id).fields;
    if args.len() != fields.len() {
        return Err(EE::UnsupportedExpr.into());
    }
    let names = gcx.callable_param_names(CallableParamSource::Struct(id));
    let args = operands::<_, Vec<_>>((0..fields.len()).map(|index| {
        let arg = args.argument_for_parameter(index, Some(&names)).ok_or(EE::UnsupportedExpr)?;
        gcx.const_eval().expr_result(arg).clone()
    }))?;
    let values = std::iter::zip(fields, args)
        .map(|(&field, arg)| implicit_to_declared(gcx, arg.value, &gcx.hir.variable(field).ty))
        .collect::<Result<_, _>>()?;
    Ok(Folded::new(ConstValue::Struct(id, values)))
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
fn eval_encode_call(gcx: Gcx<'_>, args: &[hir::Expr<'_>]) -> EvalResult {
    let [function, call_args] = args else { return Err(EE::UnsupportedExpr.into()) };
    let Some(hir::Res::Item(id @ hir::ItemId::Function(f))) = gcx.resolve_path_expr(function)
    else {
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
    let args = operands::<_, Vec<_>>(
        call_args.iter().map(|arg| gcx.const_eval().expr_result(arg).clone()),
    )?;
    let mut values = vec![builtins::fixed_bytes(&gcx.function_selector(id).0)];
    for (&param, arg) in std::iter::zip(params, args) {
        values.push(implicit_to_declared(gcx, arg.value, &gcx.hir.variable(param).ty)?);
    }
    Ok(Folded::new(builtins::call(gcx, Builtin::AbiEncodeWithSelector, &values)?))
}

/// Returns whether the given item has a selector that folds: a public or external function, an
/// error or a non-anonymous event, whose signature needs no constants.
///
/// Type checking reports `.selector` of other items, and signatures with recursive types.
fn has_selector(gcx: Gcx<'_>, id: hir::ItemId) -> bool {
    let kind_has_selector = match id {
        hir::ItemId::Function(id) => gcx.hir.function(id).is_part_of_external_interface(),
        hir::ItemId::Error(_) => true,
        hir::ItemId::Event(id) => !gcx.hir.event(id).anonymous,
        _ => false,
    };
    kind_has_selector && !signature_needs_constants(gcx, id) && gcx.has_abi_signature(id)
}

/// Returns whether the types of the given function, error or event depend on the value of a
/// constant: an array length other than a number literal in its parameter or return types.
///
/// They can depend on the constant being evaluated, and computing the signature, which computes
/// all of the item's types, would then evaluate that constant again, so these selectors do not
/// fold.
fn signature_needs_constants(gcx: Gcx<'_>, id: hir::ItemId) -> bool {
    fn needs_constants(
        gcx: Gcx<'_>,
        ty: &hir::Type<'_>,
        seen: &mut FxHashSet<hir::StructId>,
    ) -> bool {
        match ty.kind {
            hir::TypeKind::Array(array) => {
                array
                    .size
                    .is_some_and(|size| !matches!(size.peel_parens().kind, hir::ExprKind::Lit(_)))
                    || needs_constants(gcx, &array.element, seen)
            }
            hir::TypeKind::Custom(hir::ItemId::Struct(id)) => {
                seen.insert(id)
                    && gcx
                        .hir
                        .strukt(id)
                        .fields
                        .iter()
                        .any(|&field| needs_constants(gcx, &gcx.hir.variable(field).ty, seen))
            }
            hir::TypeKind::Function(function) => function
                .parameters
                .iter()
                .chain(function.returns)
                .any(|&var| needs_constants(gcx, &gcx.hir.variable(var).ty, seen)),
            hir::TypeKind::Mapping(mapping) => {
                needs_constants(gcx, &mapping.key, seen)
                    || needs_constants(gcx, &mapping.value, seen)
            }
            _ => false,
        }
    }

    let (parameters, returns) = match id {
        hir::ItemId::Function(id) => {
            let function = gcx.hir.function(id);
            (function.parameters, function.returns)
        }
        hir::ItemId::Error(id) => (gcx.hir.error(id).parameters, &[][..]),
        hir::ItemId::Event(id) => (gcx.hir.event(id).parameters, &[][..]),
        _ => return false,
    };
    let mut seen = FxHashSet::default();
    parameters
        .iter()
        .chain(returns)
        .any(|&var| needs_constants(gcx, &gcx.hir.variable(var).ty, &mut seen))
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
/// that is not [`EE::is_unsupported`], so whether a constant reverts does not depend on the order
/// of its operands.
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

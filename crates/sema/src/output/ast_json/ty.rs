//! Type names and type descriptions.

use super::{Cx, Empty, IdentPath, Params, Sym, W, name, serialize_opt};
use crate::{
    builtins::Builtin,
    eval::ConstValue,
    hir,
    ty::{Component, Gcx, Ty, TyFn, TyFnKind, TyKind, TySolcPrinter},
};
use serde::ser::{Error as _, Serialize, SerializeMap, Serializer};
use serde_json::value::RawValue;
use solar_ast::{self as ast, DataLocation, ElementaryType};
use solar_data_structures::map::FxHashMap;
use solar_interface::{Ident, Span, kw, sym};
use std::cell::RefCell;

/// The written `typeDescriptions` of the types: for the type, whether solc marks it as a pointer,
/// and whether it is the type of an expression.
pub(super) type TypeCache<'gcx> = RefCell<FxHashMap<(Ty<'gcx>, bool, bool), Box<RawValue>>>;

/// The `typeDescriptions` of a declaration or a type name.
pub(super) struct TypeDescriptions<'a, 'gcx> {
    pub(super) gcx: Gcx<'gcx>,
    pub(super) cache: Option<&'a TypeCache<'gcx>>,
    pub(super) ty: Ty<'gcx>,
    /// Whether solc marks the reference types as pointers.
    pub(super) pointer: bool,
    /// Whether it is the type of an expression, which solc prints with data locations.
    pub(super) expr: bool,
    /// The value of the literal of the type.
    pub(super) literal: Option<&'gcx ConstValue>,
    /// What solc knows of the components of a tuple expression.
    pub(super) components: Vec<Component<'gcx>>,
    /// Whether the call options `gas`, `value` and `salt` are set on the function.
    pub(super) call_options: [bool; 3],
    /// The kind of the builtin function of the type, such as `keccak256`.
    pub(super) function_kind: Option<&'static str>,
}

impl<'a, 'gcx> TypeDescriptions<'a, 'gcx> {
    /// The descriptions of a declaration or a type name.
    pub(super) fn new(cx: Cx<'a, 'gcx>, ty: Ty<'gcx>, pointer: bool) -> Self {
        Self {
            gcx: cx.gcx,
            cache: cx.json.types.as_ref(),
            ty,
            pointer,
            expr: false,
            literal: None,
            components: Vec::new(),
            call_options: [false; 3],
            function_kind: None,
        }
    }
}

impl TypeDescriptions<'_, '_> {
    /// Writes the descriptions, or the written ones if only the type decides them.
    fn serialize_cached<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let only_type = self.literal.is_none()
            && self.components.is_empty()
            && self.call_options == [false; 3]
            && self.function_kind.is_none();
        let Some(cache) = self.cache.filter(|_| only_type) else { return self.write(s) };
        let key = (self.ty, self.pointer, self.expr);
        if let Some(written) = cache.borrow().get(&key) {
            return written.serialize(s);
        }
        let written = serde_json::value::to_raw_value(&Uncached(self)).map_err(S::Error::custom)?;
        let result = written.serialize(s);
        cache.borrow_mut().insert(key, written);
        result
    }

    fn write<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self {
            gcx,
            ty,
            pointer,
            expr,
            literal,
            ref components,
            call_options,
            function_kind,
            ..
        } = *self;
        let printer = |buf| {
            TySolcPrinter::new(gcx, buf)
                .literal(literal)
                .components(components.clone())
                .call_options(call_options)
                .function_kind(function_kind)
        };
        let (mut identifier, mut string) = (String::new(), String::new());
        printer(&mut identifier).print_identifier(ty, pointer).unwrap();
        if expr {
            printer(&mut string).locations(pointer).print(ty)
        } else {
            printer(&mut string).print(ty)
        }
        .unwrap();
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("typeIdentifier", &identifier)?;
        m.serialize_entry("typeString", &string)?;
        m.end()
    }
}

impl Serialize for TypeDescriptions<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.serialize_cached(s)
    }
}

/// Descriptions that are written without the cache.
struct Uncached<'a, 'b, 'gcx>(&'a TypeDescriptions<'b, 'gcx>);

impl Serialize for Uncached<'_, '_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.0.write(s)
    }
}

/// The `typeDescriptions` of an expression, which are empty if the expression has no type.
pub(super) struct ExprType<'a, 'b, 'gcx> {
    cx: Cx<'a, 'gcx>,
    expr: &'b hir::Expr<'gcx>,
    /// The arguments of the call that the expression is the callee of, which give `require` its
    /// type.
    args: Option<hir::CallArgs<'gcx>>,
}

impl<'a, 'b, 'gcx> ExprType<'a, 'b, 'gcx> {
    pub(super) fn new(cx: Cx<'a, 'gcx>, expr: &'b hir::Expr<'gcx>) -> Self {
        Self { cx, expr, args: None }
    }

    pub(super) fn args(mut self, args: Option<hir::CallArgs<'gcx>>) -> Self {
        self.args = args;
        self
    }
}

impl Serialize for ExprType<'_, '_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self { cx, expr, args } = *self;
        let gcx = cx.gcx;
        let Some(mut ty) = expr_ty(gcx, expr) else { return Empty.serialize(s) };
        if let hir::ExprKind::Call(callee, _) = expr.peel_parens().kind
            && let Some(callee_ty) = gcx.type_of_expr(callee.id)
        {
            // The call of an error has the magic type `error`.
            if matches!(callee_ty.kind, TyKind::Error(..)) {
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("typeIdentifier", "t_error")?;
                m.serialize_entry("typeString", "error")?;
                return m.end();
            }
            // Before Byzantium, the dynamically encoded value of an external call has no size to
            // read.
            if !gcx.sess.opts.evm_version.supports_returndata()
                && let TyKind::Fn(f) = callee_ty.kind
                && matches!(f.kind, TyFnKind::External | TyFnKind::DelegateCall)
                && let [ret] = f.returns
                && !ret.encodes_as_slot()
                && ret.is_dynamically_encoded(gcx)
            {
                let mut m = s.serialize_map(Some(2))?;
                m.serialize_entry("typeIdentifier", "t_inaccessible")?;
                m.serialize_entry("typeString", "inaccessible dynamic type")?;
                return m.end();
            }
        }
        // Only a function has a builtin that decides its type.
        let builtin = matches!(ty.kind, TyKind::Fn(_))
            .then(|| gcx.resolved_expr(expr.peel_parens()).and_then(|res| res.as_builtin()))
            .flatten();
        if let TyKind::Fn(f) = ty.kind {
            // solc gives an attached function the receiver as its first parameter, and `require`
            // the type of its message.
            let callee = expr.peel_parens();
            let receiver = match callee.kind {
                hir::ExprKind::Member(base, _)
                    if gcx.resolved_callee(callee.id).is_some_and(|callee| callee.attached) =>
                {
                    // The receiver has the type that the function declares.
                    let declared = f.function_id.and_then(|id| {
                        let declared = gcx.type_of_item(id.into());
                        // A public library function takes its calldata parameters in memory.
                        let declared = match f.kind {
                            TyFnKind::DelegateCall => {
                                declared.as_externally_callable_function(true, gcx)
                            }
                            _ => declared,
                        };
                        match declared.kind {
                            TyKind::Fn(declared) => declared.parameters.first().copied(),
                            _ => None,
                        }
                    });
                    declared.or_else(|| expr_ty(gcx, base))
                }
                _ => None,
            };
            let message = match (builtin, args) {
                (Some(Builtin::Require), Some(args)) => {
                    args.exprs().nth(1).and_then(|arg| message_ty(gcx, arg))
                }
                _ => None,
            };
            let parameters = match (receiver, message) {
                (Some(receiver), _) => Some([&[receiver][..], f.parameters].concat()),
                (None, Some(message)) => Some(vec![gcx.types.bool, message]),
                (None, None) => None,
            };
            if let Some(parameters) = parameters {
                ty = gcx.mk_ty_fn(TyFn {
                    parameters: gcx.mk_tys(&parameters),
                    attached: receiver.is_some(),
                    ..*f
                });
            }
        }
        let literal_ty =
            |ty: Ty<'_>| ty.is_number_literal() || matches!(ty.kind, TyKind::StringLiteral(..));
        let literal = literal_ty(ty).then(|| gcx.const_eval().eval_expr(expr).ok()).flatten();
        let components = tuple_components(gcx, expr);
        let call_options = match expr.peel_parens().kind {
            hir::ExprKind::CallOptions(_, options) => [kw::Gas, sym::value, sym::salt]
                .map(|name| options.args.iter().any(|arg| arg.name.name == name)),
            _ => [false; 3],
        };
        let function_kind = match expr.peel_parens().kind {
            // The only builtin function without a builtin is `new` on an array.
            hir::ExprKind::New(_) => Some("objectcreation"),
            _ => builtin.map(Builtin::solc_kind),
        };
        let descriptions = TypeDescriptions {
            gcx,
            cache: cx.json.types.as_ref(),
            ty,
            // The pointer only marks the references, which the types of a function do not have.
            pointer: !(ty.is_value_type() && !matches!(ty.kind, TyKind::Fn(_)))
                && is_pointer(gcx, expr),
            expr: true,
            literal,
            components,
            call_options,
            function_kind,
        };
        descriptions.serialize(s)
    }
}

/// Returns what solc knows of the components of a tuple expression.
fn tuple_components<'gcx>(gcx: Gcx<'gcx>, expr: &hir::Expr<'gcx>) -> Vec<Component<'gcx>> {
    let literal_ty =
        |ty: Ty<'_>| ty.is_number_literal() || matches!(ty.kind, TyKind::StringLiteral(..));
    let expr = expr.peel_parens();
    let hir::ExprKind::Tuple(components) = expr.kind else { return Vec::new() };
    if !expr_ty(gcx, expr).is_some_and(|ty| matches!(ty.kind, TyKind::Tuple(_))) {
        return Vec::new();
    }
    components
        .iter()
        .map(|component| {
            let Some(component) = component else {
                return Component { literal: None, pointer: true, empty: true, components: vec![] };
            };
            Component {
                literal: expr_ty(gcx, component)
                    .filter(|ty| literal_ty(*ty))
                    .and_then(|_| gcx.const_eval().eval_expr(component).ok()),
                pointer: is_pointer(gcx, component),
                empty: false,
                components: tuple_components(gcx, component),
            }
        })
        .collect()
}

/// Returns the type of the expression, which type checking leaves out for a type in an
/// expression, such as `uint256` in `abi.decode(data, (uint256))`.
pub(super) fn expr_ty<'gcx>(gcx: Gcx<'gcx>, expr: &hir::Expr<'gcx>) -> Option<Ty<'gcx>> {
    // solc types an inline array by the common mobile type of its elements, not by the type that
    // type checking seeds it with from its destination.
    if let hir::ExprKind::Array(_) = expr.kind
        && let Some(ty) = gcx.solc_ty(expr.id)
    {
        return Some(ty);
    }
    gcx.type_of_expr(expr.id).or_else(|| match &expr.kind {
        hir::ExprKind::Type(ty) => Some(gcx.mk_ty(TyKind::Type(gcx.type_of_hir_ty(ty)))),
        hir::ExprKind::Lit(lit) => Some(gcx.type_of_lit(lit)),
        hir::ExprKind::Tuple([Some(inner)]) => expr_ty(gcx, inner),
        hir::ExprKind::Tuple(components) => {
            let tys = components
                .iter()
                .map(|component| expr_ty(gcx, (*component)?))
                .collect::<Option<Vec<_>>>()?;
            Some(gcx.mk_ty_tuple(gcx.mk_tys(&tys)))
        }
        _ => None,
    })
}

/// Returns the type of the message of `require`: `error` for a call of an error, else the type
/// that a string converts to.
fn message_ty<'gcx>(gcx: Gcx<'gcx>, message: &hir::Expr<'gcx>) -> Option<Ty<'gcx>> {
    if let hir::ExprKind::Call(callee, _) = message.peel_parens().kind
        && let Some(ty) = gcx.type_of_expr(callee.id)
        && matches!(ty.kind, TyKind::Error(..))
    {
        return Some(ty);
    }
    // The parameter of `require` and `revert` is `string memory`.
    Some(expr_ty(gcx, message)?.mobile(gcx)?.with_loc_if_ref(gcx, DataLocation::Memory))
}

/// Returns whether solc types the expression as a pointer, which it does for the reference types
/// of local variables, parameters and calls of functions, and as a reference for all other storage
/// expressions.
fn is_pointer(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> bool {
    match expr.kind {
        hir::ExprKind::Ident(_) => gcx
            .resolved_expr(expr)
            .and_then(|res| res.as_variable())
            .is_some_and(|id| !gcx.hir.variable(id).is_state_variable()),
        hir::ExprKind::Call(callee, _) => gcx.resolved_builtin(callee).is_none(),
        hir::ExprKind::Assign(lhs, ..) => is_pointer(gcx, lhs),
        hir::ExprKind::Ternary(..) => true,
        hir::ExprKind::Tuple([Some(inner)]) => is_pointer(gcx, inner),
        _ => false,
    }
}

impl<'gcx> Serialize for W<'_, 'gcx, (&ast::Type<'_>, &hir::Type<'gcx>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (ty, hir_ty)) = self;
        let gcx = cx.gcx;
        // The type of a type name is a pointer when it is a reference type.
        let descriptions = TypeDescriptions::new(*cx, gcx.type_of_hir_ty(hir_ty), true);
        match (&ty.kind, &hir_ty.kind) {
            (ast::TypeKind::Elementary(elementary), _) => {
                let (mut m, _) = cx.node(s, "ElementaryTypeName", ty.span)?;
                m.serialize_entry("name", &elementary_name(*elementary))?;
                m.serialize_entry("typeDescriptions", &descriptions)?;
                if let ElementaryType::Address(payable) = elementary
                    && (*payable || !cx.in_mapping_key)
                {
                    m.serialize_entry(
                        "stateMutability",
                        if *payable { "payable" } else { "nonpayable" },
                    )?;
                }
                m.end()
            }
            (ast::TypeKind::Custom(path), hir::TypeKind::Custom(item)) => {
                let (mut m, _) = cx.node(s, "UserDefinedTypeName", ty.span)?;
                let declaration = Some(cx.item_id(*item));
                m.serialize_entry("pathNode", &cx.w(IdentPath { path, declaration }))?;
                m.serialize_entry("referencedDeclaration", &declaration)?;
                m.serialize_entry("typeDescriptions", &descriptions)?;
                m.end()
            }
            (ast::TypeKind::Array(array), hir::TypeKind::Array(hir_array)) => {
                let (mut m, _) = cx.node(s, "ArrayTypeName", ty.span)?;
                let element = Cx { in_mapping_key: cx.in_mapping_key || cx.in_local, ..*cx };
                m.serialize_entry("baseType", &element.w((&array.element, &hir_array.element)))?;
                let length = array.size.as_deref().zip(hir_array.size);
                serialize_opt(&mut m, "length", length.map(|exprs| cx.w(exprs)))?;
                m.serialize_entry("typeDescriptions", &descriptions)?;
                m.end()
            }
            (ast::TypeKind::Mapping(mapping), hir::TypeKind::Mapping(hir_mapping)) => {
                let (mut m, _) = cx.node(s, "Mapping", ty.span)?;
                m.serialize_entry(
                    "keyType",
                    &Cx { in_mapping_key: true, ..*cx }.w((&mapping.key, &hir_mapping.key)),
                )?;
                m.serialize_entry("keyName", &name(mapping.key_name))?;
                m.serialize_entry("keyNameLocation", &cx.src(span_of(mapping.key_name)))?;
                m.serialize_entry("valueType", &cx.w((&mapping.value, &hir_mapping.value)))?;
                m.serialize_entry("valueName", &name(mapping.value_name))?;
                m.serialize_entry("valueNameLocation", &cx.src(span_of(mapping.value_name)))?;
                m.serialize_entry("typeDescriptions", &descriptions)?;
                m.end()
            }
            (ast::TypeKind::Function(function), hir::TypeKind::Function(hir_function)) => {
                // solc ends a function type with the next token, which also holds its empty list of
                // returns.
                let next = cx.token_after(ty.span);
                let (mut m, id) = cx.node(s, "FunctionTypeName", ty.span.with_hi(next.hi()))?;
                m.serialize_entry("visibility", hir_function.visibility.to_str())?;
                m.serialize_entry("stateMutability", hir_function.state_mutability.to_str())?;
                let parameters = Params {
                    id: None,
                    span: function.parameters.span,
                    ast: function.parameters.vars,
                    hir: hir_function.parameters,
                    scope: id,
                };
                m.serialize_entry("parameterTypes", &cx.w(parameters))?;
                let span = function.returns.as_ref().map_or(next.shrink_to_lo(), |r| r.span);
                let returns = Params {
                    id: None,
                    span,
                    ast: function.returns(),
                    hir: hir_function.returns,
                    scope: id,
                };
                m.serialize_entry("returnParameterTypes", &cx.w(returns))?;
                m.serialize_entry("typeDescriptions", &descriptions)?;
                m.end()
            }
            _ => unreachable!("lowering does not match the AST: {ty:?}"),
        }
    }
}

fn span_of(name: Option<Ident>) -> Span {
    name.map_or(Span::DUMMY, |name| name.span)
}

/// Returns the name of an elementary type as written, such as `uint` and `address`.
pub(super) fn elementary_name(ty: ElementaryType) -> Sym {
    Sym(match ty {
        ElementaryType::Address(_) => kw::Address,
        ElementaryType::Bool => kw::Bool,
        ElementaryType::String => kw::String,
        ElementaryType::Bytes => kw::Bytes,
        ElementaryType::Int(size) => size.int_keyword(),
        ElementaryType::UInt(size) => size.uint_keyword(),
        ElementaryType::FixedBytes(size) => size.bytes_keyword(),
    })
}

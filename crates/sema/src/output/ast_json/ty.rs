//! Type names and type descriptions.

use super::{Cx, IdentPath, Params, Sym, W, name};
use crate::{
    hir,
    ty::{Gcx, Ty, TySolcPrinter},
};
use serde::ser::{Serialize, SerializeMap, Serializer};
use solar_ast::{self as ast, ElementaryType};
use solar_interface::{Ident, Span, kw};

/// The `typeDescriptions` of a declaration or a type name.
pub(super) struct TypeDescriptions<'gcx> {
    pub(super) gcx: Gcx<'gcx>,
    pub(super) ty: Ty<'gcx>,
    /// Whether solc marks the reference types as pointers.
    pub(super) pointer: bool,
}

impl Serialize for TypeDescriptions<'_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self { gcx, ty, pointer } = *self;
        let (mut identifier, mut string) = (String::new(), String::new());
        TySolcPrinter::new(gcx, &mut identifier).print_identifier(ty, pointer).unwrap();
        TySolcPrinter::new(gcx, &mut string).print(ty).unwrap();
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("typeIdentifier", &identifier)?;
        m.serialize_entry("typeString", &string)?;
        m.end()
    }
}

impl Serialize for W<'_, '_, (&ast::Type<'_>, &hir::Type<'_>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (ty, hir_ty)) = self;
        let gcx = cx.gcx;
        // The type of a type name is a pointer when it is a reference type.
        let descriptions = TypeDescriptions { gcx, ty: gcx.type_of_hir_ty(hir_ty), pointer: true };
        match (&ty.kind, &hir_ty.kind) {
            (ast::TypeKind::Elementary(elementary), _) => {
                let (mut m, _) = cx.node(s, "ElementaryTypeName", ty.span)?;
                m.serialize_entry("name", &elementary_name(*elementary))?;
                m.serialize_entry("typeDescriptions", &descriptions)?;
                if let ElementaryType::Address(payable) = elementary
                    && !cx.in_mapping_key
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
                m.serialize_entry("baseType", &cx.w((&array.element, &hir_array.element)))?;
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
                    span: function.parameters.span,
                    ast: function.parameters.vars,
                    hir: hir_function.parameters,
                    scope: id,
                };
                m.serialize_entry("parameterTypes", &cx.w(parameters))?;
                let span = function.returns.as_ref().map_or(next.shrink_to_lo(), |r| r.span);
                let returns =
                    Params { span, ast: function.returns(), hir: hir_function.returns, scope: id };
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
fn elementary_name(ty: ElementaryType) -> Sym {
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

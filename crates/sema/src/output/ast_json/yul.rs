//! Inline assembly of the AST JSON.
//!
//! The `AST` of an inline assembly block is written from the Yul AST, as solc's `AsmJsonConverter`
//! does, since lowering hoists the Yul functions out of the block and rewrites `for`, `switch` and
//! variable declarations. Yul nodes have no IDs, so writing them leaves the ID counter alone. They
//! have a `src` and a `nativeSrc`, which are equal for inline assembly.
//!
//! The `externalReferences` come from type checking instead, which resolved the names and records
//! the identifiers that name a variable: those of the block, and of the Yul functions that lowering
//! hoisted out of it, are the ones within its span.

use super::{Cx, Seq, Sym, W};
use crate::ty::YulReference;
use alloy_primitives::hex;
use serde::ser::{Serialize, SerializeMap, Serializer};
use solar_ast::{self as ast, LitKind, yul};
use solar_interface::{Ident, Span, sym};

/// The `externalReferences` of an inline assembly block: the identifiers of the block, and of the
/// Yul functions that lowering hoisted out of it, that name a declaration outside the block, as
/// type checking recorded them.
pub(super) struct ExternalReferences {
    /// The span of the `assembly` statement.
    pub(super) span: Span,
}

impl<'gcx> Serialize for W<'_, 'gcx, ExternalReferences> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, ExternalReferences { span }) = self;
        let hir = &cx.gcx.hir;
        // The declarations of the block, which are the Yul variables, lie within its span.
        let mut refs = cx
            .gcx
            .yul_references(*span)
            .iter()
            .filter(|r| !span.contains(hir.variable(r.declaration).span))
            .collect::<Vec<_>>();
        // solc sorts the entries by name, then by the text of their location.
        refs.sort_by_cached_key(|r| (cx.text(r.span), cx.src(r.span).to_string()));
        s.collect_seq(refs.into_iter().map(|reference| ExternalReference { cx: *cx, reference }))
    }
}

/// An entry of `externalReferences`.
struct ExternalReference<'a, 'gcx> {
    cx: Cx<'a, 'gcx>,
    reference: &'a YulReference,
}

impl Serialize for ExternalReference<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self { cx, reference: YulReference { span, suffix, declaration } } = self;
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("src", &cx.src(*span))?;
        m.serialize_entry("declaration", &cx.item_id(*declaration))?;
        m.serialize_entry("isSlot", &(*suffix == Some(sym::slot)))?;
        m.serialize_entry("isOffset", &(*suffix == Some(sym::offset)))?;
        if let Some(suffix) = suffix {
            m.serialize_entry("suffix", &Sym(*suffix))?;
        }
        // The only identifiers that take more than a stack slot have a suffix that takes one.
        m.serialize_entry("valueSize", &1)?;
        m.end()
    }
}

impl<'gcx> Serialize for W<'_, 'gcx, &yul::Block<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, block) = self;
        let mut m = cx.yul_node(s, "YulBlock", block.span)?;
        m.serialize_entry("statements", &Seq(|| block.stmts.iter().map(|stmt| cx.w(stmt))))?;
        m.end()
    }
}

impl<'gcx> Serialize for W<'_, 'gcx, &yul::Stmt<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, yul::Stmt { span, kind, .. }) = self;
        let span = *span;
        match kind {
            yul::StmtKind::Block(block) => cx.w(block).serialize(s),
            yul::StmtKind::AssignSingle(path, value) => {
                let mut m = cx.yul_node(s, "YulAssignment", span)?;
                m.serialize_entry("variableNames", &[cx.w(&**path)])?;
                m.serialize_entry("value", &cx.w(value))?;
                m.end()
            }
            yul::StmtKind::AssignMulti(paths, value) => {
                let mut m = cx.yul_node(s, "YulAssignment", span)?;
                m.serialize_entry(
                    "variableNames",
                    &Seq(|| paths.iter().map(|path| cx.w(&**path))),
                )?;
                m.serialize_entry("value", &cx.w(value))?;
                m.end()
            }
            yul::StmtKind::Expr(expr) => {
                let mut m = cx.yul_node(s, "YulExpressionStatement", span)?;
                m.serialize_entry("expression", &cx.w(expr))?;
                m.end()
            }
            yul::StmtKind::If(condition, body) => {
                let mut m = cx.yul_node(s, "YulIf", span)?;
                m.serialize_entry("condition", &cx.w(condition))?;
                m.serialize_entry("body", &cx.w(body))?;
                m.end()
            }
            yul::StmtKind::For(for_) => {
                let mut m = cx.yul_node(s, "YulForLoop", span)?;
                m.serialize_entry("pre", &cx.w(&for_.init))?;
                m.serialize_entry("condition", &cx.w(&for_.cond))?;
                m.serialize_entry("post", &cx.w(&for_.step))?;
                m.serialize_entry("body", &cx.w(&for_.body))?;
                m.end()
            }
            yul::StmtKind::Switch(switch) => {
                let mut m = cx.yul_node(s, "YulSwitch", span)?;
                m.serialize_entry("expression", &cx.w(&switch.selector))?;
                m.serialize_entry("cases", &Seq(|| switch.cases.iter().map(|case| cx.w(case))))?;
                m.end()
            }
            yul::StmtKind::Leave => cx.yul_node(s, "YulLeave", span)?.end(),
            yul::StmtKind::Break => cx.yul_node(s, "YulBreak", span)?.end(),
            yul::StmtKind::Continue => cx.yul_node(s, "YulContinue", span)?.end(),
            yul::StmtKind::FunctionDef(function) => {
                let mut m = cx.yul_node(s, "YulFunctionDefinition", span)?;
                m.serialize_entry("name", &Sym(function.name.name))?;
                // solc leaves out an empty list.
                if !function.parameters.is_empty() {
                    m.serialize_entry("parameters", &cx.w(TypedNames(function.parameters)))?;
                }
                if !function.returns.is_empty() {
                    m.serialize_entry("returnVariables", &cx.w(TypedNames(function.returns)))?;
                }
                m.serialize_entry("body", &cx.w(&function.body))?;
                m.end()
            }
            yul::StmtKind::VarDecl(names, value) => {
                let mut m = cx.yul_node(s, "YulVariableDeclaration", span)?;
                m.serialize_entry("variables", &cx.w(TypedNames(names)))?;
                if let Some(value) = value {
                    m.serialize_entry("value", &cx.w(value))?;
                }
                m.end()
            }
        }
    }
}

/// The names that a declaration or a function declares.
struct TypedNames<'a>(&'a [Ident]);

impl Serialize for W<'_, '_, TypedNames<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, TypedNames(names)) = self;
        s.collect_seq(names.iter().map(|name| cx.w(TypedName(name))))
    }
}

struct TypedName<'a>(&'a Ident);

impl Serialize for W<'_, '_, TypedName<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, TypedName(name)) = self;
        let mut m = cx.yul_node(s, "YulTypedName", name.span)?;
        m.serialize_entry("name", &Sym(name.name))?;
        // solc dropped the types of Yul, and keeps the field.
        m.serialize_entry("type", "")?;
        m.end()
    }
}

impl Serialize for W<'_, '_, &yul::StmtSwitchCase<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, case) = self;
        let mut m = cx.yul_node(s, "YulCase", case.span)?;
        match &case.constant {
            Some(constant) => m.serialize_entry("value", &cx.w(Literal(constant)))?,
            None => m.serialize_entry("value", "default")?,
        }
        m.serialize_entry("body", &cx.w(&case.body))?;
        m.end()
    }
}

impl Serialize for W<'_, '_, &yul::Expr<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, yul::Expr { span, kind }) = self;
        match kind {
            yul::ExprKind::Path(path) => cx.w(&**path).serialize(s),
            yul::ExprKind::Call(call) => {
                let mut m = cx.yul_node(s, "YulFunctionCall", *span)?;
                m.serialize_entry("functionName", &cx.w(ast::PathSlice::from_ref(&call.name)))?;
                m.serialize_entry(
                    "arguments",
                    &Seq(|| call.arguments.iter().map(|arg| cx.w(arg))),
                )?;
                m.end()
            }
            yul::ExprKind::Lit(lit) => cx.w(Literal(lit)).serialize(s),
        }
    }
}

/// A `YulIdentifier`: a variable with its suffix, or a function or builtin.
impl Serialize for W<'_, '_, &ast::PathSlice> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, path) = self;
        let mut m = cx.yul_node(s, "YulIdentifier", path.span())?;
        m.serialize_entry("name", &format_args!("{path}"))?;
        m.end()
    }
}

/// A `YulLiteral`.
struct Literal<'a>(&'a ast::Lit<'a>);

impl Serialize for W<'_, '_, Literal<'_>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Literal(lit)) = self;
        let mut m = cx.yul_node(s, "YulLiteral", lit.span)?;
        match lit.kind {
            LitKind::Str(_, bytes, _) => {
                let bytes = bytes.as_byte_str();
                m.serialize_entry("kind", "string")?;
                m.serialize_entry("hexValue", &hex::encode(bytes))?;
                // solc leaves out the value of a string that is not UTF-8.
                if let Ok(value) = std::str::from_utf8(bytes) {
                    m.serialize_entry("value", value)?;
                }
            }
            LitKind::Bool(_) => {
                m.serialize_entry("kind", "bool")?;
                m.serialize_entry("value", &Sym(lit.symbol))?;
            }
            _ => {
                m.serialize_entry("kind", "number")?;
                m.serialize_entry("value", &Sym(lit.symbol))?;
            }
        }
        m.serialize_entry("type", "")?;
        m.end()
    }
}

impl<'a, 'gcx> Cx<'a, 'gcx> {
    /// Starts a Yul node, which has no ID and the same `src` and `nativeSrc`.
    fn yul_node<S: Serializer>(
        self,
        s: S,
        node_type: &str,
        span: Span,
    ) -> Result<S::SerializeMap, S::Error> {
        let mut m = s.serialize_map(None)?;
        m.serialize_entry("nodeType", node_type)?;
        m.serialize_entry("src", &self.src(span))?;
        m.serialize_entry("nativeSrc", &self.src(span))?;
        Ok(m)
    }
}

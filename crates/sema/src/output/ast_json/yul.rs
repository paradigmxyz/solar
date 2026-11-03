//! Inline assembly of the AST JSON.
//!
//! The `AST` of an inline assembly block is written from the Yul AST, as solc's `AsmJsonConverter`
//! does, since lowering hoists the Yul functions out of the block and rewrites `for`, `switch` and
//! variable declarations. Yul nodes have no IDs, so writing them leaves the ID counter alone. They
//! have a `src` and a `nativeSrc`, which are equal for inline assembly.
//!
//! The `externalReferences` come from the HIR instead, which resolved the names: the identifiers
//! of the lowered block and of the Yul functions that lowering hoisted out of it that name a
//! declaration outside the block. Those functions follow the Solidity function that holds the
//! block in the HIR, in the order of the source, so a block finds its own by their spans.

use super::{Cx, Seq, Sym, W};
use crate::{
    hir::{self, ExprKind, Res, Visit},
    ty::Gcx,
};
use alloy_primitives::hex;
use serde::ser::{Serialize, SerializeMap, Serializer};
use solar_ast::{self as ast, LitKind, yul};
use solar_data_structures::Never;
use solar_interface::{Ident, Span, Symbol, kw, sym};
use std::ops::ControlFlow;

/// The `externalReferences` of an inline assembly block.
pub(super) struct ExternalReferences<'a, 'gcx> {
    /// The span of the `assembly` statement.
    pub(super) span: Span,
    /// The lowered block.
    pub(super) block: &'a hir::Block<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, ExternalReferences<'_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, ExternalReferences { span, block }) = self;
        let hir = &cx.gcx.hir;
        let mut refs = Collector { gcx: cx.gcx, assembly: *span, refs: Vec::new() };
        refs.visit_block(block);
        if let Some(function) = cx.function {
            let hoisted = hir.functions.raw[function.index() + 1..]
                .iter()
                .take_while(|function| function.is_yul);
            for function in hoisted.filter(|function| span.contains(function.span)) {
                if let Some(body) = &function.body {
                    refs.visit_block(body);
                }
            }
        }

        // solc sorts the entries by name, then by the text of their location.
        refs.refs.sort_by_cached_key(|r| (cx.text(r.span), cx.src(r.span).to_string()));
        s.collect_seq(refs.refs.iter().map(|reference| ExternalReference { cx: *cx, reference }))
    }
}

/// The Yul identifier of an external reference.
struct Reference {
    /// The span of the identifier, with its suffix.
    span: Span,
    /// The suffix of the identifier: `slot`, `offset`, `length`, `address` or `selector`.
    suffix: Option<Symbol>,
    /// The declaration the identifier names.
    declaration: hir::ItemId,
}

/// An entry of `externalReferences`.
struct ExternalReference<'a, 'gcx> {
    cx: Cx<'a, 'gcx>,
    reference: &'a Reference,
}

impl Serialize for ExternalReference<'_, '_> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self { cx, reference: Reference { span, suffix, declaration } } = self;
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

/// Finds the identifiers that name a declaration outside the assembly block.
///
/// The declarations of the block, which are the Yul variables and functions, lie within its span.
struct Collector<'gcx> {
    gcx: Gcx<'gcx>,
    assembly: Span,
    refs: Vec<Reference>,
}

impl<'gcx> Collector<'gcx> {
    fn visit_block(&mut self, block: &hir::Block<'gcx>) {
        for stmt in block.stmts {
            let _ = self.visit_stmt(stmt);
        }
    }
}

impl<'gcx> Visit<'gcx> for Collector<'gcx> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Never> {
        // `x.slot` has the identifier `x` as its receiver, and the path of any other member is no
        // Solidity name for solc.
        let (ident, suffix) = match expr.kind {
            ExprKind::YulMember(ident, suffix) => {
                let suffixes = [sym::slot, sym::offset, sym::length, kw::Address, sym::selector];
                if !suffixes.contains(&suffix.name) {
                    return ControlFlow::Continue(());
                }
                (ident, Some(suffix))
            }
            _ => (expr, None),
        };
        if let ExprKind::Ident(&[Res::Item(declaration)]) = ident.kind {
            if !self.assembly.contains(self.gcx.hir.item(declaration).span()) {
                let span = suffix.map_or(ident.span, |suffix| ident.span.to(suffix.span));
                self.refs.push(Reference { span, suffix: suffix.map(|s| s.name), declaration });
            }
            return ControlFlow::Continue(());
        }
        self.walk_expr(expr)
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

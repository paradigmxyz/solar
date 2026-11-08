//! Statements and expressions of the AST JSON.
//!
//! Every statement and expression is written from the AST node and, with sema, the HIR node of
//! the same span, which gives the node its type and its resolution. Lowering rewrites a few of
//! them, which the writers undo:
//!
//! - `while`, `do while` and `for` become a `loop` with `if`, `break` and `continue` statements,
//!   and `for` also an extra block for its initialization;
//! - `emit` and `revert` become a call whose callee is the name path as one expression, so the
//!   names before the last one have no HIR node;
//! - the parentheses around the callee of a call are gone;
//! - `payable(x)` and `type(T)` have no callee expression, and a type in an expression, such as
//!   `uint[]` in `abi.decode(data, (uint[]))`, is one expression.
//!
//! The nodes without a HIR node get a fresh ID and a type taken from name resolution or from the
//! node they belong to. The attributes that solc computes in its type checker come from the type
//! checker results: the type, the resolution of a name, purity, and the user defined operator.

use super::{
    Cx, Empty, Node, Params, Seq, Sym, Var, W, last_docs, name,
    ty::{ExprType, TypeDescriptions, elementary_name},
    yul,
};
use crate::{
    builtins::Builtin,
    hir::{self, ItemId, Res, StmtKind},
    ty::{Gcx, Ty, TyFnKind, TyKind},
};
use alloy_primitives::hex;
use serde::ser::{Serialize, SerializeMap, Serializer};
use solar_ast::{self as ast, DocComments, ElementaryType, LitKind, StrKind};
use solar_interface::{Ident, Span, SpannedOption, kw};

/// What an expression learns from its parent. [`Cx`] holds it for the next expression.
#[derive(Clone, Copy, Default)]
pub(super) struct Parent<'gcx> {
    /// The arguments of the call that the expression is the callee of.
    args: Option<hir::CallArgs<'gcx>>,
    /// Whether the parent writes to the expression.
    lvalue: bool,
    /// Whether the expression is the external call of a `try` statement.
    try_call: bool,
}

/// The arguments of a call, a modifier invocation or an inheritance specifier.
pub(super) struct Args<'a, 'ast, 'gcx>(
    pub(super) &'a ast::CallArgs<'ast>,
    pub(super) Option<&'a hir::CallArgs<'gcx>>,
);

impl<'gcx> Serialize for W<'_, 'gcx, Args<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Args(ast, hir)) = self;
        let mut hir = hir.map(|hir| hir.exprs());
        s.collect_seq(ast.exprs().map(|ast| cx.expr(ast, hir.as_mut().and_then(|hir| hir.next()))))
    }
}

/// The `StorageLayoutSpecifier` of a contract.
pub(super) struct StorageLayout<'a, 'ast, 'gcx>(
    pub(super) &'a ast::StorageLayoutSpecifier<'ast>,
    pub(super) Option<&'a hir::Expr<'gcx>>,
);

impl<'gcx> Serialize for W<'_, 'gcx, StorageLayout<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, StorageLayout(layout, slot)) = self;
        cx.write(s, "StorageLayoutSpecifier", layout.span, |m| {
            m.entry("baseSlotExpression", &cx.expr(layout.slot, *slot));
        })
    }
}

/// A `Block` or an `UncheckedBlock`.
pub(super) struct Block<'a, 'ast, 'gcx> {
    pub(super) docs: Option<&'a DocComments<'ast>>,
    /// The span of the node, which is the span of the statement for an unchecked block.
    pub(super) span: Span,
    pub(super) ast: &'a ast::Block<'ast>,
    pub(super) hir: Option<&'a hir::Block<'gcx>>,
    pub(super) unchecked: bool,
}

impl<'gcx> Serialize for W<'_, 'gcx, Block<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Block { docs, span, ast, hir, unchecked }) = self;
        let node_type = if *unchecked { "UncheckedBlock" } else { "Block" };
        let (mut m, id) = cx.stmt_node(s, node_type, *span, *docs)?;
        let cx = Cx { block: id, ..*cx };
        m.entry("statements", &cx.w(Stmts(ast.stmts, hir.map(|hir| hir.stmts))));
        m.end()
    }
}

/// A list of statements.
struct Stmts<'a, 'ast, 'gcx>(&'a [ast::Stmt<'ast>], Option<&'a [hir::Stmt<'gcx>]>);

impl<'gcx> Serialize for W<'_, 'gcx, Stmts<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Stmts(ast, hir)) = self;
        s.collect_seq(ast.iter().enumerate().map(|(i, ast)| cx.stmt(ast, hir.map(|hir| &hir[i]))))
    }
}

impl<'a, 'gcx> Cx<'a, 'gcx> {
    /// Starts the node of a statement, which has the text of its doc comments.
    fn stmt_node<S: Serializer>(
        self,
        s: S,
        node_type: &str,
        span: Span,
        docs: Option<&DocComments<'_>>,
    ) -> Result<(Node<S::SerializeMap>, usize), S::Error> {
        let (mut m, id) = self.node(s, node_type, span)?;
        if let Some((_, text)) = docs.and_then(|docs| last_docs(self, docs)) {
            m.entry("documentation", &text);
        }
        Ok((m, id))
    }

    /// Writes the node of a statement: the fields that `fields` writes.
    fn write_stmt<S: Serializer>(
        self,
        s: S,
        node_type: &str,
        span: Span,
        docs: Option<&DocComments<'_>>,
        fields: impl FnOnce(&mut Node<S::SerializeMap>),
    ) -> Result<S::Ok, S::Error> {
        let (mut m, _) = self.stmt_node(s, node_type, span, docs)?;
        fields(&mut m);
        m.end()
    }

    /// Returns the span of a statement. solc leaves the `;` out of a simple statement but not out
    /// of `return;` and `do while`, and ends a compound statement where its last body ends.
    fn stmt_span(self, stmt: &ast::Stmt<'_>) -> Span {
        match &stmt.kind {
            ast::StmtKind::Expr(expr) | ast::StmtKind::Return(Some(expr)) => {
                stmt.span.with_hi(expr.span.hi())
            }
            ast::StmtKind::DeclSingle(_)
            | ast::StmtKind::DeclMulti(..)
            | ast::StmtKind::Emit(..)
            | ast::StmtKind::Revert(..)
            | ast::StmtKind::Placeholder
            | ast::StmtKind::Break
            | ast::StmtKind::Continue => self.without_semicolon(stmt.span),
            ast::StmtKind::If(_, then, other) => {
                stmt.span.with_hi(self.stmt_span(other.as_deref().unwrap_or(then)).hi())
            }
            ast::StmtKind::While(_, body) | ast::StmtKind::For { body, .. } => {
                stmt.span.with_hi(self.stmt_span(body).hi())
            }
            _ => stmt.span,
        }
    }

    /// Returns the writer of an expression, from its AST node and its HIR node, with sema.
    pub(super) fn expr<'n, 'ast>(
        self,
        ast: &'n ast::Expr<'ast>,
        hir: impl Into<Option<&'n hir::Expr<'gcx>>>,
    ) -> W<'a, 'gcx, (&'n ast::Expr<'ast>, Option<&'n hir::Expr<'gcx>>)> {
        self.w((ast, hir.into()))
    }

    /// Returns the writer of a statement, from its AST node and its HIR node, with sema.
    pub(super) fn stmt<'n, 'ast>(
        self,
        ast: &'n ast::Stmt<'ast>,
        hir: impl Into<Option<&'n hir::Stmt<'gcx>>>,
    ) -> W<'a, 'gcx, (&'n ast::Stmt<'ast>, Option<&'n hir::Stmt<'gcx>>)> {
        self.w((ast, hir.into()))
    }

    /// Writes the attributes that every expression but an identifier has. Without sema, an
    /// expression only has empty descriptions of its type.
    fn expr_attrs<M: SerializeMap>(
        self,
        m: &mut Node<M>,
        expr: Option<&hir::Expr<'gcx>>,
        parent: Parent<'gcx>,
        is_lvalue: bool,
    ) {
        m.entry("typeDescriptions", &ExprType::new(self, expr).args(parent.args));
        let Some(expr) = expr else { return };
        self.argument_types(m, parent);
        m.entry("isLValue", &is_lvalue);
        m.entry("isPure", &self.gcx.is_pure(expr));
        m.entry("isConstant", &false);
        m.entry("lValueRequested", &parent.lvalue);
    }

    /// Writes the types of the arguments of the call that the expression is the callee of.
    fn argument_types<M: SerializeMap>(self, m: &mut Node<M>, parent: Parent<'gcx>) {
        let Some(args) = parent.args else { return };
        m.entry("argumentTypes", &Seq(|| args.exprs().map(|arg| ExprType::new(self, arg))));
    }

    /// Returns the `typeDescriptions` of an expression that has no HIR node.
    fn synthetic_type(
        self,
        ty: Ty<'gcx>,
        function_kind: Option<&'static str>,
    ) -> TypeDescriptions<'a, 'gcx> {
        TypeDescriptions { expr: true, function_kind, ..TypeDescriptions::new(self, ty, true) }
    }
}

impl<'gcx> Serialize for W<'_, 'gcx, (&ast::Stmt<'_>, Option<&hir::Stmt<'gcx>>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (stmt, hir)) = self;
        let gcx = cx.gcx;
        // solc ignores the doc comments in the head of a `for`.
        let docs = (!cx.for_init).then_some(&stmt.docs);
        let span = cx.stmt_span(stmt);
        let kind = hir.map(|hir| &hir.kind);
        match &stmt.kind {
            ast::StmtKind::Block(ast) => {
                let hir = paired!(kind, StmtKind::Block(hir) => hir);
                cx.w(Block { docs, span, ast, hir, unchecked: false }).serialize(s)
            }
            ast::StmtKind::UncheckedBlock(ast) => {
                let hir = paired!(kind, StmtKind::UncheckedBlock(hir) => hir);
                cx.w(Block { docs, span, ast, hir, unchecked: true }).serialize(s)
            }
            ast::StmtKind::Assembly(assembly) => {
                paired!(kind, StmtKind::AssemblyBlock(..) => ());
                cx.write_stmt(s, "InlineAssembly", span, docs, |m| {
                    m.entry("AST", &cx.w(&assembly.block));
                    let references = yul::ExternalReferences { span };
                    m.entry("externalReferences", &cx.w(references));
                    m.entry("evmVersion", &format_args!("{}", gcx.sess.opts.evm_version));
                    if !assembly.flags.is_empty() {
                        let flags = Seq(|| assembly.flags.iter().map(|flag| Sym(flag.value)));
                        m.entry("flags", &flags);
                    }
                })
            }
            ast::StmtKind::DeclSingle(ast) => {
                let id = paired!(kind, StmtKind::DeclSingle(id) => *id);
                cx.write_stmt(s, "VariableDeclarationStatement", span, docs, |m| {
                    // Without sema, the variable takes its ID here.
                    let node = id.map_or_else(|| cx.ids.fresh(), |id| cx.item_id(id));
                    m.entry("assignments", &[node]);
                    // solc parses the type of a single declaration as an expression first.
                    let single = Cx { in_local: true, ..*cx };
                    m.entry("declarations", &[local(single, ast, id, node)]);
                    let value = id.map(|id| gcx.hir.variable(id).initializer);
                    let initial_value =
                        ast.initializer.as_deref().map(|init| cx.expr(init, value.flatten()));
                    m.opt("initialValue", initial_value);
                })
            }
            ast::StmtKind::DeclMulti(vars, ast) => {
                let hirs = paired!(kind, StmtKind::DeclMulti(ids, hir) => (*ids, *hir));
                cx.write_stmt(s, "VariableDeclarationStatement", span, docs, |m| {
                    // Without sema, the variables take their IDs here.
                    let nodes = vars
                        .iter()
                        .enumerate()
                        .map(|(i, var)| match (var, hirs) {
                            (SpannedOption::None(_), _) => None,
                            (_, Some((ids, _))) => ids[i].map(|id| cx.item_id(id)),
                            (_, None) => Some(cx.ids.fresh()),
                        })
                        .collect::<Vec<_>>();
                    m.entry("assignments", &nodes);
                    // solc parses the type of the first variable of a tuple declaration as an
                    // expression too, but not those of the others.
                    let first = vars.iter().position(|var| matches!(var, SpannedOption::Some(_)));
                    let declarations = Seq(|| {
                        vars.iter().enumerate().map(|(i, var)| match (var, nodes[i]) {
                            (SpannedOption::Some(var), Some(node)) => {
                                let ids = hirs.map(|(ids, _)| ids);
                                let id = ids.and_then(|ids| ids[i]);
                                Some(local(Cx { in_local: first == Some(i), ..*cx }, var, id, node))
                            }
                            _ => None,
                        })
                    });
                    m.entry("declarations", &declarations);
                    m.entry("initialValue", &cx.expr(ast, hirs.map(|(_, hir)| hir)));
                })
            }
            ast::StmtKind::Expr(ast) => {
                let hir = paired!(kind, StmtKind::Expr(hir) => *hir);
                cx.write_stmt(s, "ExpressionStatement", span, docs, |m| {
                    m.entry("expression", &cx.expr(ast, hir));
                })
            }
            ast::StmtKind::If(cond, then, other) => {
                let hirs = paired!(kind, StmtKind::If(cond, then, other) => (*cond, *then, *other));
                cx.write_stmt(s, "IfStatement", span, docs, |m| {
                    m.entry("condition", &cx.expr(cond, hirs.map(|hirs| hirs.0)));
                    m.entry("trueBody", &cx.stmt(then, hirs.map(|hirs| hirs.1)));
                    let falsy = other.as_deref();
                    m.opt(
                        "falseBody",
                        falsy.map(|other| cx.stmt(other, hirs.and_then(|hirs| hirs.2))),
                    );
                })
            }
            ast::StmtKind::While(cond, body) => {
                let hirs = paired!(kind, StmtKind::Loop(block, _) => match block.stmts[0].kind {
                    StmtKind::If(cond, body, _) => (cond, body),
                    _ => unreachable!("lowering does not match the AST: {stmt:?}"),
                });
                cx.write_stmt(s, "WhileStatement", span, docs, |m| {
                    m.entry("condition", &cx.expr(cond, hirs.map(|hirs| hirs.0)));
                    m.entry("body", &cx.stmt(body, hirs.map(|hirs| hirs.1)));
                })
            }
            ast::StmtKind::DoWhile(body, cond) => {
                let hirs = paired!(kind, StmtKind::Loop(block, _) => match block.stmts[1].kind {
                    StmtKind::If(cond, ..) => (cond, &block.stmts[0]),
                    _ => unreachable!("lowering does not match the AST: {stmt:?}"),
                });
                cx.write_stmt(s, "DoWhileStatement", span, docs, |m| {
                    m.entry("condition", &cx.expr(cond, hirs.map(|hirs| hirs.0)));
                    m.entry("body", &cx.stmt(body, hirs.map(|hirs| hirs.1)));
                })
            }
            ast::StmtKind::For { init, cond, next, body } => {
                let (mut m, id) = cx.stmt_node(s, "ForStatement", span, docs)?;
                let parts = hir.map(|hir| ForParts::new(init.is_some(), cond.is_some(), hir));
                let cx = Cx { block: id, ..*cx };
                let init_cx = Cx { for_init: true, ..cx };
                let init = init.as_deref();
                m.opt(
                    "initializationExpression",
                    init.map(|init| init_cx.stmt(init, parts.and_then(|parts| parts.init))),
                );
                let cond = cond.as_deref();
                m.opt("condition", cond.map(|cond| cx.expr(cond, parts.and_then(|p| p.cond))));
                let next = next.as_deref();
                m.opt(
                    "loopExpression",
                    next.map(|next| cx.w(Next(next, parts.and_then(|parts| parts.next)))),
                );
                m.entry("body", &cx.stmt(body, parts.map(|parts| parts.body)));
                if let Some(parts) = parts {
                    m.entry(
                        "isSimpleCounterLoop",
                        &parts.cond.is_some_and(|cond| gcx.is_simple_counter_loop(cond.id)),
                    );
                }
                m.end()
            }
            ast::StmtKind::Continue => cx.write_stmt(s, "Continue", span, docs, |_| {}),
            ast::StmtKind::Break => cx.write_stmt(s, "Break", span, docs, |_| {}),
            ast::StmtKind::Placeholder => {
                cx.write_stmt(s, "PlaceholderStatement", span, docs, |_| {})
            }
            ast::StmtKind::Return(ast) => {
                let hir = paired!(kind, StmtKind::Return(hir) => *hir);
                cx.write_stmt(s, "Return", span, docs, |m| {
                    let value = ast.as_deref();
                    m.opt("expression", value.map(|value| cx.expr(value, hir.flatten())));
                    m.opt("functionReturnParameters", cx.returns.filter(|_| cx.sema()));
                })
            }
            ast::StmtKind::Emit(path, args) => {
                let hir = paired!(kind, StmtKind::Emit(hir) => *hir);
                cx.write_stmt(s, "EmitStatement", span, docs, |m| {
                    m.entry("eventCall", &cx.w(PathCall { path, args, hir }));
                })
            }
            ast::StmtKind::Revert(path, args) => {
                let hir = paired!(kind, StmtKind::Revert(hir) => *hir);
                cx.write_stmt(s, "RevertStatement", span, docs, |m| {
                    m.entry("errorCall", &cx.w(PathCall { path, args, hir }));
                })
            }
            ast::StmtKind::Try(ast) => {
                let hir = paired!(kind, StmtKind::Try(hir) => hir);
                cx.write_stmt(s, "TryStatement", span, docs, |m| {
                    // The type checker marks the call, which solc leaves unmarked without it.
                    let try_call = cx.sema();
                    let call = Cx { parent: Parent { try_call, ..Parent::default() }, ..*cx };
                    m.entry("externalCall", &call.expr(ast.expr, hir.map(|hir| &hir.expr)));
                    let clauses = Seq(|| {
                        ast.clauses
                            .iter()
                            .enumerate()
                            .map(|(i, clause)| cx.w((clause, hir.map(|hir| &hir.clauses[i]))))
                    });
                    m.entry("clauses", &clauses);
                })
            }
        }
    }
}

/// Returns the writer of the variable that a declaration statement declares. Its node has the
/// given ID, which the statement lists in its `assignments`.
fn local<'a, 'ast, 'gcx>(
    cx: Cx<'a, 'gcx>,
    ast: &'a ast::VariableDefinition<'ast>,
    id: Option<hir::VariableId>,
    node: usize,
) -> W<'a, 'gcx, Var<'a, 'ast>> {
    let scope = cx.sema().then_some(cx.block);
    cx.w(Var { ast, id, node: Some(node), value: false, docs: None, scope })
}

/// The loop expression of a `for` statement: an `ExpressionStatement`.
struct Next<'a, 'ast, 'gcx>(&'a ast::Expr<'ast>, Option<&'a hir::Expr<'gcx>>);

impl<'gcx> Serialize for W<'_, 'gcx, Next<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Next(ast, hir)) = self;
        cx.write(s, "ExpressionStatement", ast.span, |m| {
            m.entry("expression", &cx.expr(ast, *hir));
        })
    }
}

/// A `try` statement clause: a `TryCatchClause`.
impl<'gcx> Serialize
    for W<'_, 'gcx, (&ast::TryCatchClause<'_>, Option<&hir::TryCatchClause<'gcx>>)>
{
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (ast, hir)) = self;
        let (mut m, id) = cx.node(s, "TryCatchClause", ast.span)?;
        let cx = Cx { block: id, ..*cx };
        m.entry("errorName", &name(ast.name));
        if !ast.args.span.is_dummy() {
            let params = Params {
                id: None,
                span: ast.args.span,
                ast: ast.args.vars,
                hir: hir.map(|hir| hir.args),
                scope: cx.sema().then_some(id),
            };
            m.entry("parameters", &cx.w(params));
        }
        m.entry(
            "block",
            &cx.w(Block {
                docs: None,
                span: ast.block.span,
                ast: &ast.block,
                hir: hir.map(|hir| &hir.block),
                unchecked: false,
            }),
        );
        m.end()
    }
}

/// The parts of a `for` loop in the HIR, which lowering splits into a block and a loop.
#[derive(Clone, Copy)]
struct ForParts<'hir> {
    init: Option<&'hir hir::Stmt<'hir>>,
    cond: Option<&'hir hir::Expr<'hir>>,
    body: &'hir hir::Stmt<'hir>,
    next: Option<&'hir hir::Expr<'hir>>,
}

impl<'hir> ForParts<'hir> {
    fn new(has_init: bool, has_cond: bool, stmt: &hir::Stmt<'hir>) -> Self {
        let unexpected = || -> ! { unreachable!("lowering does not match the AST: {stmt:?}") };
        let (init, stmt) = if has_init {
            let StmtKind::Block(block) = &stmt.kind else { unexpected() };
            let stmts = block.stmts;
            (Some(&stmts[0]), &stmts[1])
        } else {
            (None, stmt)
        };
        let StmtKind::Loop(block, hir::LoopSource::For { update }) = &stmt.kind else {
            unexpected()
        };
        let next = update.map(|update| match update.kind {
            StmtKind::Expr(next) => next,
            _ => unexpected(),
        });
        let stmts = block.stmts;
        let (cond, body) = match (&stmts[0].kind, has_cond) {
            (StmtKind::If(cond, body, _), true) => (Some(*cond), *body),
            _ => (None, &stmts[0]),
        };
        Self { init, cond, body, next }
    }
}

/// The node of an expression that has the attributes that every expression but an identifier has.
struct ExprNode<'a, 'gcx> {
    cx: Cx<'a, 'gcx>,
    span: Span,
    hir: Option<&'a hir::Expr<'gcx>>,
    parent: Parent<'gcx>,
}

impl<'gcx> ExprNode<'_, 'gcx> {
    /// Writes the node: the fields that `fields` writes, then the attributes.
    fn write<S: Serializer>(
        &self,
        s: S,
        node_type: &str,
        is_lvalue: bool,
        fields: impl FnOnce(&mut Node<S::SerializeMap>),
    ) -> Result<S::Ok, S::Error> {
        let (mut m, _) = self.cx.node(s, node_type, self.span)?;
        fields(&mut m);
        self.cx.expr_attrs(&mut m, self.hir, self.parent, is_lvalue);
        m.end()
    }
}

impl<'gcx> Serialize for W<'_, 'gcx, (&ast::Expr<'_>, Option<&hir::Expr<'gcx>>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (ast, hir)) = self;
        let (ast, hir) = (*ast, *hir);
        let parent = cx.parent;
        let cx = Cx { parent: Parent::default(), ..*cx };
        let gcx = cx.gcx;
        let span = ast.span;
        let e = ExprNode { cx, span, hir, parent };
        let kind = hir.map(|hir| &hir.kind);
        let is_lvalue = hir.is_some_and(|hir| gcx.is_lvalue(hir.id));
        // The parentheses around a callee are no HIR node, and take the type of the callee.
        if let ast::ExprKind::Tuple(components) = &ast.kind
            && let [SpannedOption::Some(inner)] = components.as_slice()
            && kind.is_some_and(|kind| !matches!(kind, hir::ExprKind::Tuple(_)))
        {
            return e.write(s, "TupleExpression", parent.lvalue, |m| {
                m.entry("isInlineArray", &false);
                m.entry("components", &[Some(cx.expr(inner, hir))]);
            });
        }
        match &ast.kind {
            ast::ExprKind::Ident(ident) => {
                let resolutions = paired!(kind, hir::ExprKind::Ident(resolutions) => *resolutions);
                cx.write(s, "Identifier", span, |m| {
                    m.entry("name", ident.as_str());
                    if let Some(hir) = hir {
                        let res = gcx.resolved_expr(hir);
                        m.opt(
                            "referencedDeclaration",
                            res.and_then(|res| cx.reference(res, *ident, None)),
                        );
                    }
                    let overloads = resolutions.map(|resolutions| cx.overloads(resolutions));
                    m.entry("overloadedDeclarations", &overloads.unwrap_or_default());
                    m.entry("typeDescriptions", &ExprType::new(cx, hir).args(parent.args));
                    cx.argument_types(m, parent);
                })
            }
            ast::ExprKind::Lit(lit, sub) => {
                paired!(kind, hir::ExprKind::Lit(_) => ());
                e.write(s, "Literal", false, |m| {
                    let (kind, value) = match &lit.kind {
                        LitKind::Str(kind, bytes, _) => {
                            let kind = match kind {
                                StrKind::Str => "string",
                                StrKind::Unicode => "unicodeString",
                                StrKind::Hex => "hexString",
                            };
                            (kind, bytes.as_byte_str())
                        }
                        LitKind::Bool(_) => ("bool", lit.symbol.as_str().as_bytes()),
                        _ => ("number", lit.symbol.as_str().as_bytes()),
                    };
                    m.entry("kind", kind);
                    m.opt("value", std::str::from_utf8(value).ok());
                    m.entry("hexValue", &hex::encode(value));
                    m.opt("subdenomination", sub.map(|sub| sub.to_str()));
                })
            }
            ast::ExprKind::Member(base, member) => {
                let hir_base = paired!(kind, hir::ExprKind::Member(base, _) => *base);
                e.write(s, "MemberAccess", is_lvalue, |m| {
                    m.entry("memberName", member.as_str());
                    m.entry("memberLocation", &cx.src(member.span));
                    m.entry("expression", &cx.expr(base, hir_base));
                    if let (Some(hir), Some(hir_base)) = (hir, hir_base) {
                        let res = gcx.resolved_expr(hir);
                        let base_res = gcx.resolved_expr(hir_base);
                        let declaration = res.and_then(|res| cx.reference(res, *member, base_res));
                        m.opt("referencedDeclaration", declaration);
                    }
                })
            }
            ast::ExprKind::Index(base, ast::IndexKind::Index(index)) => {
                let hirs = paired!(kind, hir::ExprKind::Index(base, index) => (*base, *index));
                e.write(s, "IndexAccess", is_lvalue, |m| {
                    m.entry("baseExpression", &cx.expr(base, hirs.map(|hirs| hirs.0)));
                    let index = index.as_deref();
                    m.opt(
                        "indexExpression",
                        index.map(|index| cx.expr(index, hirs.and_then(|hirs| hirs.1))),
                    );
                })
            }
            ast::ExprKind::Index(base, ast::IndexKind::Range(start, end)) => {
                let hirs = paired!(kind, hir::ExprKind::Slice(base, lo, hi) => (*base, *lo, *hi));
                e.write(s, "IndexRangeAccess", false, |m| {
                    m.entry("baseExpression", &cx.expr(base, hirs.map(|hirs| hirs.0)));
                    let start = start.as_deref();
                    let hir_start = hirs.and_then(|hirs| hirs.1);
                    m.opt("startExpression", start.map(|start| cx.expr(start, hir_start)));
                    let end = end.as_deref();
                    let hir_end = hirs.and_then(|hirs| hirs.2);
                    m.opt("endExpression", end.map(|end| cx.expr(end, hir_end)));
                })
            }
            ast::ExprKind::Assign(lhs, op, rhs) => {
                let hirs = paired!(kind, hir::ExprKind::Assign(lhs, _, rhs) => (*lhs, *rhs));
                e.write(s, "Assignment", false, |m| {
                    let operator = format_args!("{}=", op.map_or("", |op| op.kind.to_str()));
                    m.entry("operator", &operator);
                    m.entry("leftHandSide", &cx.written().expr(lhs, hirs.map(|hirs| hirs.0)));
                    m.entry("rightHandSide", &cx.expr(rhs, hirs.map(|hirs| hirs.1)));
                })
            }
            ast::ExprKind::Binary(lhs, op, rhs) => {
                let hirs = paired!(kind, hir::ExprKind::Binary(lhs, _, rhs) => (*lhs, *rhs));
                e.write(s, "BinaryOperation", false, |m| {
                    m.entry("operator", op.kind.to_str());
                    m.entry("leftExpression", &cx.expr(lhs, hirs.map(|hirs| hirs.0)));
                    m.entry("rightExpression", &cx.expr(rhs, hirs.map(|hirs| hirs.1)));
                    match hir {
                        Some(hir) if op.kind.is_cmp() || op.kind.is_logical() => {
                            let common = gcx.solc_ty(hir.id);
                            m.opt("commonType", common.map(|ty| cx.synthetic_type(ty, None)));
                        }
                        _ => {
                            m.entry("commonType", &ExprType::new(cx, hir));
                        }
                    }
                    if let Some(function) = hir.and_then(|hir| gcx.user_operator(hir.id)) {
                        m.entry("function", &cx.item_id(function));
                    }
                })
            }
            ast::ExprKind::Unary(_, inner) | ast::ExprKind::Delete(inner) => {
                let hir_inner = paired!(
                    kind,
                    hir::ExprKind::Unary(_, inner) | hir::ExprKind::Delete(inner) => *inner
                );
                let (prefix, operator, writes) = match &ast.kind {
                    ast::ExprKind::Unary(op, _) => {
                        (op.kind.is_prefix(), op.kind.to_str(), op.kind.has_side_effects())
                    }
                    _ => (true, "delete", true),
                };
                e.write(s, "UnaryOperation", false, |m| {
                    m.entry("prefix", &prefix);
                    m.entry("operator", operator);
                    let inner_cx = if writes { cx.written() } else { cx };
                    m.entry("subExpression", &inner_cx.expr(inner, hir_inner));
                    if let Some(function) = hir.and_then(|hir| gcx.user_operator(hir.id)) {
                        m.entry("function", &cx.item_id(function));
                    }
                })
            }
            ast::ExprKind::Ternary(cond, then, other) => {
                let hirs = paired!(kind, hir::ExprKind::Ternary(cond, then, other) => (*cond, *then, *other));
                e.write(s, "Conditional", false, |m| {
                    m.entry("condition", &cx.expr(cond, hirs.map(|hirs| hirs.0)));
                    m.entry("trueExpression", &cx.expr(then, hirs.map(|hirs| hirs.1)));
                    m.entry("falseExpression", &cx.expr(other, hirs.map(|hirs| hirs.2)));
                })
            }
            ast::ExprKind::Tuple(components) => {
                let hirs = paired!(kind, hir::ExprKind::Tuple(components) => *components);
                e.write(s, "TupleExpression", is_lvalue, |m| {
                    m.entry("isInlineArray", &false);
                    let inner =
                        Cx { parent: Parent { lvalue: parent.lvalue, ..Parent::default() }, ..cx };
                    let components = Seq(|| {
                        components.iter().enumerate().map(|(i, ast)| match ast {
                            SpannedOption::Some(ast) => {
                                Some(inner.expr(ast, hirs.and_then(|hirs| hirs[i])))
                            }
                            SpannedOption::None(_) => None,
                        })
                    });
                    m.entry("components", &components);
                })
            }
            ast::ExprKind::Array(components) => {
                let hirs = paired!(kind, hir::ExprKind::Array(components) => *components);
                e.write(s, "TupleExpression", false, |m| {
                    m.entry("isInlineArray", &true);
                    let components = Seq(|| {
                        components
                            .iter()
                            .enumerate()
                            .map(|(i, ast)| cx.expr(ast, hirs.map(|hirs| &hirs[i])))
                    });
                    m.entry("components", &components);
                })
            }
            ast::ExprKind::CallOptions(callee, options) => {
                let hirs =
                    paired!(kind, hir::ExprKind::CallOptions(callee, opts) => (*callee, opts.args));
                e.write(s, "FunctionCallOptions", false, |m| {
                    let callee_cx =
                        Cx { parent: Parent { args: parent.args, ..Parent::default() }, ..cx };
                    m.entry("expression", &callee_cx.expr(callee, hirs.map(|hirs| hirs.0)));
                    m.entry("names", &Seq(|| options.iter().map(|option| Sym(option.name.name))));
                    let values = Seq(|| {
                        options.iter().enumerate().map(|(i, option)| {
                            cx.expr(option.value, hirs.map(|hirs| &hirs.1[i].value))
                        })
                    });
                    m.entry("options", &values);
                })
            }
            ast::ExprKind::Call(callee, args) => {
                let hirs = paired!(kind, hir::ExprKind::Call(callee, args) => (*callee, *args));
                e.write(s, "FunctionCall", is_lvalue, |m| {
                    let callee_cx = Cx {
                        parent: Parent { args: hirs.map(|hirs| hirs.1), ..Parent::default() },
                        ..cx
                    };
                    m.entry("expression", &callee_cx.expr(callee, hirs.map(|hirs| hirs.0)));
                    let kind = hirs.map(|hirs| call_kind(gcx, hirs.0));
                    let arguments = cx.w(Args(args, hirs.as_ref().map(|hirs| &hirs.1)));
                    cx.call_attrs(m, named_args(args), &arguments, kind, parent.try_call);
                })
            }
            ast::ExprKind::New(ty) => {
                let hir_ty = paired!(kind, hir::ExprKind::New(ty) => ty);
                e.write(s, "NewExpression", false, |m| {
                    m.entry("typeName", &cx.w((ty, hir_ty)));
                })
            }
            ast::ExprKind::Type(ty) => {
                let hir_ty = paired!(kind, hir::ExprKind::Type(ty) => ty);
                cx.w(TypeExpr { ast: ty, hir: hir_ty, expr: hir, parent }).serialize(s)
            }
            ast::ExprKind::Payable(args) => {
                let inner = paired!(kind, hir::ExprKind::Payable(inner) => *inner);
                e.write(s, "FunctionCall", false, |m| {
                    let hir_args = inner.map(|inner| hir::CallArgs {
                        span: args.span,
                        kind: hir::CallArgsKind::Unnamed(std::slice::from_ref(inner)),
                    });
                    // solc ends the callee with the `(` after it.
                    let keyword = span.with_hi(span.lo() + kw::Payable.as_str().len() as u32);
                    let callee =
                        PayableCallee { span: keyword.to(cx.token_after(keyword)), args: hir_args };
                    m.entry("expression", &cx.w(callee));
                    let arguments = cx.w(Args(args, hir_args.as_ref()));
                    let kind = cx.sema().then_some("typeConversion");
                    cx.call_attrs(m, named_args(args), &arguments, kind, parent.try_call);
                })
            }
            ast::ExprKind::TypeCall(ty) => {
                let hir_ty = paired!(kind, hir::ExprKind::TypeCall(ty) => ty);
                e.write(s, "FunctionCall", false, |m| {
                    let arg_ty =
                        hir_ty.map(|hir_ty| gcx.mk_ty(TyKind::Type(gcx.type_of_hir_ty(hir_ty))));
                    let callee = TypeCallee {
                        span: span.with_hi(span.lo() + kw::Type.as_str().len() as u32),
                        arg_ty,
                    };
                    m.entry("expression", &cx.w(callee));
                    let arg =
                        TypeExpr { ast: ty, hir: hir_ty, expr: None, parent: Parent::default() };
                    let kind = cx.sema().then_some("functionCall");
                    cx.call_attrs(m, &[], &[cx.w(arg)], kind, parent.try_call);
                })
            }
            ast::ExprKind::Err(_) => unreachable!("the AST has errors"),
        }
    }
}

impl<'a, 'gcx> Cx<'a, 'gcx> {
    /// Returns the writer of an expression that the parent writes to.
    fn written(self) -> Self {
        Cx { parent: Parent { lvalue: true, ..Parent::default() }, ..self }
    }

    /// Returns the ID of the declaration that an identifier refers to: solc gives negative IDs to
    /// the builtins.
    ///
    /// The identifier is a member of `base` if that is a module, and else a name in this source.
    fn reference(self, res: Res, ident: Ident, base: Option<Res>) -> Option<i64> {
        match res {
            Res::Builtin(builtin) => builtin.solc_id(),
            Res::Namespace(source) => {
                let scope = match base {
                    Some(Res::Namespace(module)) => module,
                    _ => self.source,
                };
                Some(self.module_alias_id(scope, ident, source)? as i64)
            }
            Res::Item(_) | Res::Err(_) => self.item_reference(res).map(|id| id as i64),
        }
    }

    /// Returns the IDs of the declarations that a name can refer to, which solc lists when there
    /// is more than one: it has three `require` and two `revert` overloads.
    fn overloads(self, resolutions: &[Res]) -> Vec<i64> {
        let count = |res: &Res| if *res == Res::Builtin(Builtin::Require) { 3 } else { 1 };
        if resolutions.iter().map(count).sum::<usize>() < 2 {
            return Vec::new();
        }
        // solc leaves out the functions that another candidate overrides.
        let resolutions = resolutions.iter().filter(|res| {
            let Res::Item(ItemId::Function(id)) = res else { return true };
            !resolutions.iter().any(|other| match other {
                Res::Item(other @ ItemId::Function(_)) => {
                    self.gcx.base_override_items(*other).contains(&ItemId::Function(*id))
                }
                _ => false,
            })
        });
        let ids = resolutions.map(|res| {
            let id = match res {
                Res::Builtin(builtin) => builtin.solc_id(),
                res => self.item_reference(*res).map(|id| id as i64),
            };
            (id, count(res))
        });
        ids.flat_map(|(id, count)| std::iter::repeat_n(id, count)).flatten().collect()
    }

    /// Returns the ID of the item that a resolution refers to.
    fn item_reference(self, res: Res) -> Option<usize> {
        match res {
            // The getter of a public state variable is the variable.
            Res::Item(ItemId::Function(id)) if let Some(var) = self.gcx.hir.function(id).gettee => {
                Some(self.item_id(var))
            }
            Res::Item(id) => Some(self.item_id(id)),
            _ => None,
        }
    }

    /// Writes the attributes of a call that follow its callee.
    fn call_attrs<M: SerializeMap>(
        self,
        m: &mut Node<M>,
        named: &[ast::NamedArg<'_>],
        arguments: &impl Serialize,
        kind: Option<&str>,
        try_call: bool,
    ) {
        m.entry("names", &Seq(|| named.iter().map(|arg| Sym(arg.name.name))));
        m.entry("nameLocations", &Seq(|| named.iter().map(|arg| self.src(arg.name.span))));
        m.entry("arguments", arguments);
        m.entry("tryCall", &try_call);
        m.opt("kind", kind);
    }
}

/// Returns the named arguments of a call.
fn named_args<'a, 'ast>(args: &'a ast::CallArgs<'ast>) -> &'a [ast::NamedArg<'ast>] {
    match &args.kind {
        ast::CallArgsKind::Named(named) => named,
        ast::CallArgsKind::Unnamed(_) => &[],
    }
}

/// Returns the kind of a call from its callee: a type is a conversion, unless it is a struct.
fn call_kind<'gcx>(gcx: Gcx<'gcx>, callee: &hir::Expr<'gcx>) -> &'static str {
    match gcx.type_of_expr(callee.id).map(|ty| ty.kind) {
        Some(TyKind::Type(ty)) if matches!(ty.kind, TyKind::Struct(_)) => "structConstructorCall",
        Some(TyKind::Type(_)) => "typeConversion",
        _ => "functionCall",
    }
}

/// The callee of `payable(x)`: an `ElementaryTypeNameExpression` for `address payable`.
struct PayableCallee<'gcx> {
    span: Span,
    /// The arguments of the call, with sema.
    args: Option<hir::CallArgs<'gcx>>,
}

impl<'gcx> Serialize for W<'_, 'gcx, PayableCallee<'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, PayableCallee { span, args }) = self;
        let gcx = cx.gcx;
        cx.write(s, "ElementaryTypeNameExpression", *span, |m| {
            m.entry("typeName", &cx.w(ElementaryName(*span, ElementaryType::Address(true))));
            let Some(args) = args else {
                m.entry("typeDescriptions", &Empty);
                return;
            };
            let ty = gcx.mk_ty(TyKind::Type(gcx.types.address_payable));
            m.entry("typeDescriptions", &cx.synthetic_type(ty, None));
            cx.argument_types(m, Parent { args: Some(*args), ..Parent::default() });
            m.entry("isLValue", &false);
            m.entry("isPure", &true);
            m.entry("isConstant", &false);
            m.entry("lValueRequested", &false);
        })
    }
}

/// An elementary type name in an expression, which solc gives no type.
struct ElementaryName(Span, ElementaryType);

impl Serialize for W<'_, '_, ElementaryName> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, ElementaryName(span, ty)) = self;
        cx.write(s, "ElementaryTypeName", *span, |m| {
            m.entry("name", &elementary_name(*ty));
            m.entry("typeDescriptions", &Empty);
            if *ty == ElementaryType::Address(true) {
                m.entry("stateMutability", "payable");
            }
        })
    }
}

/// The callee of `type(T)`: the builtin `type`.
struct TypeCallee<'gcx> {
    span: Span,
    /// The type of the argument, `type(T)`, with sema.
    arg_ty: Option<Ty<'gcx>>,
}

impl<'gcx> Serialize for W<'_, 'gcx, TypeCallee<'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, TypeCallee { span, arg_ty }) = self;
        let gcx = cx.gcx;
        cx.write(s, "Identifier", *span, |m| {
            m.entry("name", "type");
            if arg_ty.is_some() {
                m.entry("referencedDeclaration", &-27);
            }
            m.entry("overloadedDeclarations", &[(); 0]);
            let Some(arg_ty) = arg_ty else {
                m.entry("typeDescriptions", &Empty);
                return;
            };
            let ty =
                gcx.mk_ty_fn_with_kind(TyFnKind::Builtin, &[], hir::StateMutability::Pure, &[]);
            m.entry("typeDescriptions", &cx.synthetic_type(ty, Some("metatype")));
            m.entry("argumentTypes", &[cx.synthetic_type(*arg_ty, None)]);
        })
    }
}

/// A type in an expression: `uint256`, `uint256[]`, or a user-defined type in `type(T)`.
struct TypeExpr<'a, 'ast, 'gcx> {
    ast: &'a ast::Type<'ast>,
    hir: Option<&'a hir::Type<'gcx>>,
    /// The expression of the type, which is none for the types of `type(T)` and array elements.
    expr: Option<&'a hir::Expr<'gcx>>,
    parent: Parent<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, TypeExpr<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, TypeExpr { ast, hir, expr, parent }) = self;
        let gcx = cx.gcx;
        let ty = hir.map(|hir| {
            expr.and_then(|expr| gcx.type_of_expr(expr.id))
                .unwrap_or_else(|| gcx.mk_ty(TyKind::Type(gcx.type_of_hir_ty(hir))))
        });
        let attrs = |m: &mut Node<S::SerializeMap>| {
            let Some(ty) = ty else {
                m.entry("typeDescriptions", &Empty);
                return;
            };
            m.entry("typeDescriptions", &cx.synthetic_type(ty, None));
            cx.argument_types(m, *parent);
            m.entry("isLValue", &false);
            m.entry("isPure", &true);
            m.entry("isConstant", &false);
            m.entry("lValueRequested", &false);
        };
        let kind = hir.map(|hir| &hir.kind);
        match &ast.kind {
            ast::TypeKind::Elementary(elementary) => {
                cx.write(s, "ElementaryTypeNameExpression", ast.span, |m| {
                    m.entry("typeName", &cx.w(ElementaryName(ast.span, *elementary)));
                    attrs(m);
                })
            }
            ast::TypeKind::Array(array) => {
                let hir_array = paired!(kind, hir::TypeKind::Array(array) => array);
                cx.write(s, "IndexAccess", ast.span, |m| {
                    let element = TypeExpr {
                        ast: &array.element,
                        hir: hir_array.map(|hir| &hir.element),
                        expr: None,
                        parent: Parent::default(),
                    };
                    m.entry("baseExpression", &cx.w(element));
                    let size = array.size.as_deref();
                    m.opt(
                        "indexExpression",
                        size.map(|size| cx.expr(size, hir_array.and_then(|hir| hir.size))),
                    );
                    attrs(m);
                })
            }
            ast::TypeKind::Custom(path) => {
                let item = paired!(kind, hir::TypeKind::Custom(item) => *item);
                // Lowering resolved the last segment in the scope of its contract.
                let mut res = resolve_path(*cx, path.segments());
                if let Some(item) = item {
                    *res.last_mut().unwrap() = Some(Res::Item(item));
                }
                let path =
                    PathExpr { segments: path.segments(), res: &res, last: None, parent: *parent };
                cx.w(path).serialize(s)
            }
            _ => unreachable!("lowering does not match the AST: {ast:?}"),
        }
    }
}

/// The call of an `emit` or `revert` statement.
struct PathCall<'a, 'ast, 'gcx> {
    path: &'a ast::AstPath<'ast>,
    args: &'a ast::CallArgs<'ast>,
    hir: Option<&'a hir::Expr<'gcx>>,
}

impl<'gcx> Serialize for W<'_, 'gcx, PathCall<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, PathCall { path, args, hir }) = self;
        let hirs = paired!(hir.map(|hir| &hir.kind), hir::ExprKind::Call(callee, args) => (*callee, *args));
        cx.write(s, "FunctionCall", path.span().to(args.span), |m| {
            let parent = Parent { args: hirs.map(|hirs| hirs.1), ..Parent::default() };
            let res = resolve_path(*cx, path.segments());
            let last = hirs.map(|hirs| hirs.0);
            let callee = PathExpr { segments: path.segments(), res: &res, last, parent };
            m.entry("expression", &cx.w(callee));
            let arguments = cx.w(Args(args, hirs.as_ref().map(|hirs| &hirs.1)));
            let kind = cx.sema().then_some("functionCall");
            cx.call_attrs(m, named_args(args), &arguments, kind, false);
            cx.expr_attrs(m, *hir, Parent::default(), false);
        })
    }
}

/// Returns what each segment of a name path resolves to, from name resolution alone, with sema.
fn resolve_path(cx: Cx<'_, '_>, segments: &[Ident]) -> Vec<Option<Res>> {
    // The only segment of a name is the last one, which lowering or type checking resolved.
    if !cx.sema() || segments.len() == 1 {
        return vec![None; segments.len()];
    }
    let resolutions = cx.gcx.source_path_resolutions(segments, cx.source, None);
    let resolutions = resolutions.unwrap_or_else(|| vec![Vec::new(); segments.len()]);
    resolutions
        .iter()
        .map(|resolutions| match resolutions[..] {
            [res] => Some(res),
            _ => None,
        })
        .collect()
}

/// A name path as an expression: an `Identifier`, or `MemberAccess` nodes over one.
///
/// The names before the last one have no HIR node, and take their resolution and type from name
/// resolution.
struct PathExpr<'a, 'gcx> {
    segments: &'a [Ident],
    /// What each segment resolves to.
    res: &'a [Option<Res>],
    /// The HIR node of the last segment, which gives it the resolution and type of the type
    /// checker.
    last: Option<&'a hir::Expr<'gcx>>,
    parent: Parent<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, PathExpr<'_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, PathExpr { segments, res, last, parent }) = self;
        let gcx = cx.gcx;
        let (&ident, rest) = segments.split_last().unwrap();
        let (res_last, ty) = match last {
            Some(last) => (gcx.resolved_expr(last), gcx.type_of_expr(last.id)),
            None => {
                let res = res[segments.len() - 1];
                (res, res.map(|res| gcx.type_of_res(res)))
            }
        };
        let span = segments[0].span.to(ident.span);
        let descriptions = |m: &mut Node<S::SerializeMap>| {
            match (last, ty) {
                (Some(last), _) => {
                    m.entry("typeDescriptions", &ExprType::new(*cx, *last).args(parent.args))
                }
                (None, Some(ty)) => m.entry("typeDescriptions", &cx.synthetic_type(ty, None)),
                (None, None) => m.entry("typeDescriptions", &Empty),
            };
        };
        if rest.is_empty() {
            let (mut m, _) = cx.node(s, "Identifier", span)?;
            m.entry("name", ident.as_str());
            let declaration = res_last.and_then(|res| cx.reference(res, ident, None));
            m.opt("referencedDeclaration", declaration);
            let overloads = match last.map(|last| &last.kind) {
                Some(hir::ExprKind::Ident(resolutions)) => cx.overloads(resolutions),
                _ => Vec::new(),
            };
            m.entry("overloadedDeclarations", &overloads);
            descriptions(&mut m);
            cx.argument_types(&mut m, *parent);
            return m.end();
        }
        cx.write(s, "MemberAccess", span, |m| {
            m.entry("memberName", ident.as_str());
            m.entry("memberLocation", &cx.src(ident.span));
            let base = PathExpr {
                segments: rest,
                res: &res[..rest.len()],
                last: None,
                parent: Parent::default(),
            };
            m.entry("expression", &cx.w(base));
            let base_res = res[rest.len() - 1];
            let declaration = res_last.and_then(|res| cx.reference(res, ident, base_res));
            m.opt("referencedDeclaration", declaration);
            descriptions(m);
            if !cx.sema() {
                return;
            }
            cx.argument_types(m, *parent);
            m.entry("isLValue", &false);
            // A member of a module is pure, but a member of a contract is not.
            let of_module = res.len().checked_sub(2).and_then(|i| res[i]);
            m.entry("isPure", &matches!(of_module, Some(Res::Namespace(_))));
            m.entry("isConstant", &false);
            m.entry("lValueRequested", &false);
        })
    }
}

//! Statements and expressions of the AST JSON.
//!
//! Every statement and expression is written from the AST node and the HIR node of the same span,
//! which gives the node its type and its resolution. Lowering rewrites a few of them, which the
//! writers undo:
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
    Cx, Empty, Params, Seq, Sym, Var, W, last_docs, name, serialize_opt,
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
    pub(super) &'a hir::CallArgs<'gcx>,
);

impl<'gcx> Serialize for W<'_, 'gcx, Args<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Args(ast, hir)) = self;
        s.collect_seq(ast.exprs().zip(hir.exprs()).map(|exprs| cx.w(exprs)))
    }
}

/// The `StorageLayoutSpecifier` of a contract.
pub(super) struct StorageLayout<'a, 'ast, 'gcx>(
    pub(super) &'a ast::StorageLayoutSpecifier<'ast>,
    pub(super) &'a hir::Expr<'gcx>,
);

impl<'gcx> Serialize for W<'_, 'gcx, StorageLayout<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, StorageLayout(layout, slot)) = self;
        let (mut m, _) = cx.node(s, "StorageLayoutSpecifier", layout.span)?;
        m.serialize_entry("baseSlotExpression", &cx.w((&*layout.slot, *slot)))?;
        m.end()
    }
}

/// A `Block` or an `UncheckedBlock`.
pub(super) struct Block<'a, 'ast, 'gcx> {
    pub(super) docs: Option<&'a DocComments<'ast>>,
    /// The span of the node, which is the span of the statement for an unchecked block.
    pub(super) span: Span,
    pub(super) ast: &'a ast::Block<'ast>,
    pub(super) hir: &'a hir::Block<'gcx>,
    pub(super) unchecked: bool,
}

impl<'gcx> Serialize for W<'_, 'gcx, Block<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Block { docs, span, ast, hir, unchecked }) = self;
        let node_type = if *unchecked { "UncheckedBlock" } else { "Block" };
        let (mut m, id) = cx.stmt_node(s, node_type, *span, *docs)?;
        let cx = Cx { block: id, ..*cx };
        m.serialize_entry("statements", &cx.w(Stmts(ast.stmts, hir.stmts)))?;
        m.end()
    }
}

/// A list of statements.
struct Stmts<'a, 'ast, 'gcx>(&'a [ast::Stmt<'ast>], &'a [hir::Stmt<'gcx>]);

impl<'gcx> Serialize for W<'_, 'gcx, Stmts<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Stmts(ast, hir)) = self;
        s.collect_seq(ast.iter().zip(*hir).map(|stmts| cx.w(stmts)))
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
    ) -> Result<(S::SerializeMap, usize), S::Error> {
        let (mut m, id) = self.node(s, node_type, span)?;
        if let Some((_, text)) = docs.and_then(|docs| last_docs(self, docs)) {
            m.serialize_entry("documentation", &text)?;
        }
        Ok((m, id))
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

    /// Writes the attributes that every expression but an identifier has.
    fn expr_attrs<M: SerializeMap>(
        self,
        m: &mut M,
        expr: &hir::Expr<'gcx>,
        parent: Parent<'gcx>,
        is_lvalue: bool,
    ) -> Result<(), M::Error> {
        m.serialize_entry("typeDescriptions", &ExprType::new(self, expr).args(parent.args))?;
        self.argument_types(m, parent)?;
        m.serialize_entry("isLValue", &is_lvalue)?;
        m.serialize_entry("isPure", &self.gcx.is_pure(expr))?;
        m.serialize_entry("isConstant", &false)?;
        m.serialize_entry("lValueRequested", &parent.lvalue)
    }

    /// Writes the types of the arguments of the call that the expression is the callee of.
    fn argument_types<M: SerializeMap>(
        self,
        m: &mut M,
        parent: Parent<'gcx>,
    ) -> Result<(), M::Error> {
        let Some(args) = parent.args else { return Ok(()) };
        m.serialize_entry(
            "argumentTypes",
            &Seq(|| args.exprs().map(|arg| ExprType::new(self, arg))),
        )
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

impl<'gcx> Serialize for W<'_, 'gcx, (&ast::Stmt<'_>, &hir::Stmt<'gcx>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (stmt, hir)) = self;
        let gcx = cx.gcx;
        // solc ignores the doc comments in the head of a `for`.
        let docs = (!cx.for_init).then_some(&stmt.docs);
        let span = cx.stmt_span(stmt);
        match (&stmt.kind, &hir.kind) {
            (ast::StmtKind::Block(ast), StmtKind::Block(hir)) => {
                cx.w(Block { docs, span, ast, hir, unchecked: false }).serialize(s)
            }
            (ast::StmtKind::UncheckedBlock(ast), StmtKind::UncheckedBlock(hir)) => {
                cx.w(Block { docs, span, ast, hir, unchecked: true }).serialize(s)
            }
            (ast::StmtKind::Assembly(assembly), StmtKind::AssemblyBlock(..)) => {
                let (mut m, _) = cx.stmt_node(s, "InlineAssembly", span, docs)?;
                m.serialize_entry("AST", &cx.w(&assembly.block))?;
                let references = yul::ExternalReferences { span };
                m.serialize_entry("externalReferences", &cx.w(references))?;
                m.serialize_entry("evmVersion", &format_args!("{}", gcx.sess.opts.evm_version))?;
                if !assembly.flags.is_empty() {
                    let flags = Seq(|| assembly.flags.iter().map(|flag| Sym(flag.value)));
                    m.serialize_entry("flags", &flags)?;
                }
                m.end()
            }
            (ast::StmtKind::DeclSingle(ast), StmtKind::DeclSingle(id)) => {
                let (mut m, _) = cx.stmt_node(s, "VariableDeclarationStatement", span, docs)?;
                m.serialize_entry("assignments", &[cx.item_id(*id)])?;
                // solc parses the type of a single declaration as an expression first.
                let single = Cx { in_local: true, ..*cx };
                m.serialize_entry("declarations", &[local(single, ast, *id)])?;
                let initial_value =
                    ast.initializer.as_deref().zip(gcx.hir.variable(*id).initializer);
                serialize_opt(&mut m, "initialValue", initial_value.map(|exprs| cx.w(exprs)))?;
                m.end()
            }
            (ast::StmtKind::DeclMulti(vars, ast), StmtKind::DeclMulti(ids, hir)) => {
                let (mut m, _) = cx.stmt_node(s, "VariableDeclarationStatement", span, docs)?;
                let assignments = Seq(|| ids.iter().map(|id| id.map(|id| cx.item_id(id))));
                m.serialize_entry("assignments", &assignments)?;
                // solc parses the type of the first variable of a tuple declaration as an
                // expression too, but not those of the others.
                let first = vars.iter().position(|var| matches!(var, SpannedOption::Some(_)));
                let declarations = Seq(|| {
                    vars.iter().zip(*ids).enumerate().map(|(i, (var, id))| match (var, id) {
                        (SpannedOption::Some(var), Some(id)) => {
                            Some(local(Cx { in_local: first == Some(i), ..*cx }, var, *id))
                        }
                        _ => None,
                    })
                });
                m.serialize_entry("declarations", &declarations)?;
                m.serialize_entry("initialValue", &cx.w((&**ast, *hir)))?;
                m.end()
            }
            (ast::StmtKind::Expr(ast), StmtKind::Expr(hir)) => {
                let (mut m, _) = cx.stmt_node(s, "ExpressionStatement", span, docs)?;
                m.serialize_entry("expression", &cx.w((&**ast, *hir)))?;
                m.end()
            }
            (ast::StmtKind::If(cond, then, other), StmtKind::If(h_cond, h_then, h_other)) => {
                let (mut m, _) = cx.stmt_node(s, "IfStatement", span, docs)?;
                m.serialize_entry("condition", &cx.w((&**cond, *h_cond)))?;
                m.serialize_entry("trueBody", &cx.w((&**then, *h_then)))?;
                let falsy = other.as_deref().zip(*h_other);
                serialize_opt(&mut m, "falseBody", falsy.map(|stmts| cx.w(stmts)))?;
                m.end()
            }
            (ast::StmtKind::While(cond, body), StmtKind::Loop(block, _)) => {
                let (mut m, _) = cx.stmt_node(s, "WhileStatement", span, docs)?;
                let StmtKind::If(h_cond, h_body, _) = block.stmts[0].kind else {
                    unreachable!("lowering does not match the AST: {stmt:?}")
                };
                m.serialize_entry("condition", &cx.w((&**cond, h_cond)))?;
                m.serialize_entry("body", &cx.w((&**body, h_body)))?;
                m.end()
            }
            (ast::StmtKind::DoWhile(body, cond), StmtKind::Loop(block, _)) => {
                let (mut m, _) = cx.stmt_node(s, "DoWhileStatement", span, docs)?;
                let StmtKind::If(h_cond, ..) = block.stmts[1].kind else {
                    unreachable!("lowering does not match the AST: {stmt:?}")
                };
                m.serialize_entry("condition", &cx.w((&**cond, h_cond)))?;
                m.serialize_entry("body", &cx.w((&**body, &block.stmts[0])))?;
                m.end()
            }
            (ast::StmtKind::For { init, cond, next, body }, _) => {
                let (mut m, id) = cx.stmt_node(s, "ForStatement", span, docs)?;
                let parts = ForParts::new(init.is_some(), cond.is_some(), hir);
                let cx = Cx { block: id, ..*cx };
                let init = init.as_deref().zip(parts.init);
                let init_cx = Cx { for_init: true, ..cx };
                serialize_opt(
                    &mut m,
                    "initializationExpression",
                    init.map(|stmts| init_cx.w(stmts)),
                )?;
                let cond = cond.as_deref().zip(parts.cond);
                serialize_opt(&mut m, "condition", cond.map(|exprs| cx.w(exprs)))?;
                let next = next.as_deref().zip(parts.next);
                serialize_opt(&mut m, "loopExpression", next.map(|exprs| cx.w(Next(exprs))))?;
                m.serialize_entry("body", &cx.w((&**body, parts.body)))?;
                m.serialize_entry(
                    "isSimpleCounterLoop",
                    &parts.cond.is_some_and(|cond| gcx.is_simple_counter_loop(cond.id)),
                )?;
                m.end()
            }
            (ast::StmtKind::Continue, _) => cx.stmt_node(s, "Continue", span, docs)?.0.end(),
            (ast::StmtKind::Break, _) => cx.stmt_node(s, "Break", span, docs)?.0.end(),
            (ast::StmtKind::Placeholder, _) => {
                cx.stmt_node(s, "PlaceholderStatement", span, docs)?.0.end()
            }
            (ast::StmtKind::Return(ast), StmtKind::Return(hir)) => {
                let (mut m, _) = cx.stmt_node(s, "Return", span, docs)?;
                let value = ast.as_deref().zip(*hir);
                serialize_opt(&mut m, "expression", value.map(|exprs| cx.w(exprs)))?;
                serialize_opt(&mut m, "functionReturnParameters", cx.returns)?;
                m.end()
            }
            (ast::StmtKind::Emit(path, args), StmtKind::Emit(hir)) => {
                let (mut m, _) = cx.stmt_node(s, "EmitStatement", span, docs)?;
                m.serialize_entry("eventCall", &cx.w(PathCall { path, args, hir }))?;
                m.end()
            }
            (ast::StmtKind::Revert(path, args), StmtKind::Revert(hir)) => {
                let (mut m, _) = cx.stmt_node(s, "RevertStatement", span, docs)?;
                m.serialize_entry("errorCall", &cx.w(PathCall { path, args, hir }))?;
                m.end()
            }
            (ast::StmtKind::Try(ast), StmtKind::Try(hir)) => {
                let (mut m, _) = cx.stmt_node(s, "TryStatement", span, docs)?;
                let call = Cx { parent: Parent { try_call: true, ..Parent::default() }, ..*cx };
                m.serialize_entry("externalCall", &call.w((&*ast.expr, &hir.expr)))?;
                let clauses = Seq(|| ast.clauses.iter().zip(hir.clauses).map(|c| cx.w(c)));
                m.serialize_entry("clauses", &clauses)?;
                m.end()
            }
            _ => unreachable!("lowering does not match the AST: {stmt:?}"),
        }
    }
}

/// Returns the writer of the variable that a declaration statement declares.
fn local<'a, 'ast, 'gcx>(
    cx: Cx<'a, 'gcx>,
    ast: &'a ast::VariableDefinition<'ast>,
    id: hir::VariableId,
) -> W<'a, 'gcx, Var<'a, 'ast>> {
    cx.w(Var { ast, id, value: false, docs: None, scope: cx.block })
}

/// The loop expression of a `for` statement: an `ExpressionStatement`.
struct Next<'a, 'ast, 'gcx>((&'a ast::Expr<'ast>, &'a hir::Expr<'gcx>));

impl<'gcx> Serialize for W<'_, 'gcx, Next<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, Next((ast, hir))) = self;
        let (mut m, _) = cx.node(s, "ExpressionStatement", ast.span)?;
        m.serialize_entry("expression", &cx.w((*ast, *hir)))?;
        m.end()
    }
}

/// A `try` statement clause: a `TryCatchClause`.
impl<'gcx> Serialize for W<'_, 'gcx, (&ast::TryCatchClause<'_>, &hir::TryCatchClause<'gcx>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (ast, hir)) = self;
        let (mut m, id) = cx.node(s, "TryCatchClause", ast.span)?;
        let cx = Cx { block: id, ..*cx };
        m.serialize_entry("errorName", &name(ast.name))?;
        if !ast.args.span.is_dummy() {
            let params = Params {
                id: None,
                span: ast.args.span,
                ast: ast.args.vars,
                hir: hir.args,
                scope: id,
            };
            m.serialize_entry("parameters", &cx.w(params))?;
        }
        m.serialize_entry(
            "block",
            &cx.w(Block {
                docs: None,
                span: ast.block.span,
                ast: &ast.block,
                hir: &hir.block,
                unchecked: false,
            }),
        )?;
        m.end()
    }
}

/// The parts of a `for` loop in the HIR, which lowering splits into a block and a loop.
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

impl<'gcx> Serialize for W<'_, 'gcx, (&ast::Expr<'_>, &hir::Expr<'gcx>)> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, (ast, hir)) = self;
        let (ast, hir) = (*ast, *hir);
        let parent = cx.parent;
        let cx = Cx { parent: Parent::default(), ..*cx };
        let gcx = cx.gcx;
        let span = ast.span;
        // The parentheses around a callee are no HIR node, and take the type of the callee.
        if let ast::ExprKind::Tuple(components) = &ast.kind
            && let [SpannedOption::Some(inner)] = components.as_slice()
            && !matches!(hir.kind, hir::ExprKind::Tuple(_))
        {
            let (mut m, _) = cx.node(s, "TupleExpression", span)?;
            m.serialize_entry("isInlineArray", &false)?;
            m.serialize_entry("components", &[Some(cx.w((&**inner, hir)))])?;
            cx.expr_attrs(&mut m, hir, parent, parent.lvalue)?;
            return m.end();
        }
        match (&ast.kind, &hir.kind) {
            (ast::ExprKind::Ident(ident), hir::ExprKind::Ident(resolutions)) => {
                let (mut m, _) = cx.node(s, "Identifier", span)?;
                m.serialize_entry("name", ident.as_str())?;
                let declaration =
                    gcx.resolved_expr(hir).and_then(|res| cx.reference(res, *ident, None));
                serialize_opt(&mut m, "referencedDeclaration", declaration)?;
                m.serialize_entry("overloadedDeclarations", &cx.overloads(resolutions))?;
                m.serialize_entry("typeDescriptions", &ExprType::new(cx, hir).args(parent.args))?;
                cx.argument_types(&mut m, parent)?;
                m.end()
            }
            (ast::ExprKind::Lit(lit, sub), hir::ExprKind::Lit(_)) => {
                let (mut m, _) = cx.node(s, "Literal", span)?;
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
                m.serialize_entry("kind", kind)?;
                serialize_opt(&mut m, "value", std::str::from_utf8(value).ok())?;
                m.serialize_entry("hexValue", &hex::encode(value))?;
                serialize_opt(&mut m, "subdenomination", sub.map(|sub| sub.to_str()))?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (ast::ExprKind::Member(base, member), hir::ExprKind::Member(hir_base, _)) => {
                let (mut m, _) = cx.node(s, "MemberAccess", span)?;
                m.serialize_entry("memberName", member.as_str())?;
                m.serialize_entry("memberLocation", &cx.src(member.span))?;
                m.serialize_entry("expression", &cx.w((&**base, *hir_base)))?;
                let res = gcx.resolved_expr(hir);
                let base_res = gcx.resolved_expr(hir_base);
                let declaration = res.and_then(|res| cx.reference(res, *member, base_res));
                serialize_opt(&mut m, "referencedDeclaration", declaration)?;
                cx.expr_attrs(&mut m, hir, parent, gcx.is_lvalue(hir.id))?;
                m.end()
            }
            (
                ast::ExprKind::Index(base, index),
                hir::ExprKind::Index(..) | hir::ExprKind::Slice(..),
            ) => match (index, &hir.kind) {
                (ast::IndexKind::Index(index), hir::ExprKind::Index(hir_base, hir_index)) => {
                    let (mut m, _) = cx.node(s, "IndexAccess", span)?;
                    m.serialize_entry("baseExpression", &cx.w((&**base, *hir_base)))?;
                    let index = index.as_deref().zip(*hir_index);
                    serialize_opt(&mut m, "indexExpression", index.map(|exprs| cx.w(exprs)))?;
                    cx.expr_attrs(&mut m, hir, parent, gcx.is_lvalue(hir.id))?;
                    m.end()
                }
                (
                    ast::IndexKind::Range(start, end),
                    hir::ExprKind::Slice(hir_base, h_start, h_end),
                ) => {
                    let (mut m, _) = cx.node(s, "IndexRangeAccess", span)?;
                    m.serialize_entry("baseExpression", &cx.w((&**base, *hir_base)))?;
                    let start = start.as_deref().zip(*h_start);
                    serialize_opt(&mut m, "startExpression", start.map(|exprs| cx.w(exprs)))?;
                    let end = end.as_deref().zip(*h_end);
                    serialize_opt(&mut m, "endExpression", end.map(|exprs| cx.w(exprs)))?;
                    cx.expr_attrs(&mut m, hir, parent, false)?;
                    m.end()
                }
                _ => unreachable!("lowering does not match the AST: {ast:?}"),
            },
            (ast::ExprKind::Assign(lhs, op, rhs), hir::ExprKind::Assign(hir_lhs, _, hir_rhs)) => {
                let (mut m, _) = cx.node(s, "Assignment", span)?;
                let operator = format_args!("{}=", op.map_or("", |op| op.kind.to_str()));
                m.serialize_entry("operator", &operator)?;
                m.serialize_entry("leftHandSide", &cx.written().w((&**lhs, *hir_lhs)))?;
                m.serialize_entry("rightHandSide", &cx.w((&**rhs, *hir_rhs)))?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (ast::ExprKind::Binary(lhs, op, rhs), hir::ExprKind::Binary(hir_lhs, _, hir_rhs)) => {
                let (mut m, _) = cx.node(s, "BinaryOperation", span)?;
                m.serialize_entry("operator", op.kind.to_str())?;
                m.serialize_entry("leftExpression", &cx.w((&**lhs, *hir_lhs)))?;
                m.serialize_entry("rightExpression", &cx.w((&**rhs, *hir_rhs)))?;
                if op.kind.is_cmp() || op.kind.is_logical() {
                    let common = gcx.solc_ty(hir.id);
                    serialize_opt(
                        &mut m,
                        "commonType",
                        common.map(|ty| cx.synthetic_type(ty, None)),
                    )?;
                } else {
                    m.serialize_entry("commonType", &ExprType::new(cx, hir))?;
                }
                if let Some(function) = gcx.user_operator(hir.id) {
                    m.serialize_entry("function", &cx.item_id(function))?;
                }
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (
                ast::ExprKind::Unary(_, inner) | ast::ExprKind::Delete(inner),
                hir::ExprKind::Unary(_, hir_inner) | hir::ExprKind::Delete(hir_inner),
            ) => {
                let (prefix, operator, writes) = match &ast.kind {
                    ast::ExprKind::Unary(op, _) => {
                        (op.kind.is_prefix(), op.kind.to_str(), op.kind.has_side_effects())
                    }
                    _ => (true, "delete", true),
                };
                let (mut m, _) = cx.node(s, "UnaryOperation", span)?;
                m.serialize_entry("prefix", &prefix)?;
                m.serialize_entry("operator", operator)?;
                let inner_cx = if writes { cx.written() } else { cx };
                m.serialize_entry("subExpression", &inner_cx.w((&**inner, *hir_inner)))?;
                if let Some(function) = gcx.user_operator(hir.id) {
                    m.serialize_entry("function", &cx.item_id(function))?;
                }
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (
                ast::ExprKind::Ternary(cond, then, other),
                hir::ExprKind::Ternary(h_cond, h_then, h_other),
            ) => {
                let (mut m, _) = cx.node(s, "Conditional", span)?;
                m.serialize_entry("condition", &cx.w((&**cond, *h_cond)))?;
                m.serialize_entry("trueExpression", &cx.w((&**then, *h_then)))?;
                m.serialize_entry("falseExpression", &cx.w((&**other, *h_other)))?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (ast::ExprKind::Tuple(components), hir::ExprKind::Tuple(hir_components)) => {
                let (mut m, _) = cx.node(s, "TupleExpression", span)?;
                m.serialize_entry("isInlineArray", &false)?;
                let inner =
                    Cx { parent: Parent { lvalue: parent.lvalue, ..Parent::default() }, ..cx };
                let components = Seq(|| {
                    components.iter().zip(*hir_components).map(|(ast, hir)| match (ast, hir) {
                        (SpannedOption::Some(ast), Some(hir)) => Some(inner.w((&**ast, *hir))),
                        _ => None,
                    })
                });
                m.serialize_entry("components", &components)?;
                cx.expr_attrs(&mut m, hir, parent, gcx.is_lvalue(hir.id))?;
                m.end()
            }
            (ast::ExprKind::Array(components), hir::ExprKind::Array(hir_components)) => {
                let (mut m, _) = cx.node(s, "TupleExpression", span)?;
                m.serialize_entry("isInlineArray", &true)?;
                let components = Seq(|| {
                    components.iter().zip(*hir_components).map(|(ast, hir)| cx.w((&**ast, hir)))
                });
                m.serialize_entry("components", &components)?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (
                ast::ExprKind::CallOptions(callee, options),
                hir::ExprKind::CallOptions(hir_callee, hir_options),
            ) => {
                let (mut m, _) = cx.node(s, "FunctionCallOptions", span)?;
                let callee_cx =
                    Cx { parent: Parent { args: parent.args, ..Parent::default() }, ..cx };
                m.serialize_entry("expression", &callee_cx.w((&**callee, *hir_callee)))?;
                m.serialize_entry(
                    "names",
                    &Seq(|| options.iter().map(|option| Sym(option.name.name))),
                )?;
                let values = Seq(|| {
                    options
                        .iter()
                        .zip(hir_options.args)
                        .map(|(option, hir)| cx.w((&*option.value, &hir.value)))
                });
                m.serialize_entry("options", &values)?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (ast::ExprKind::Call(callee, args), hir::ExprKind::Call(hir_callee, hir_args)) => {
                let (mut m, _) = cx.node(s, "FunctionCall", span)?;
                let callee_cx =
                    Cx { parent: Parent { args: Some(*hir_args), ..Parent::default() }, ..cx };
                m.serialize_entry("expression", &callee_cx.w((&**callee, *hir_callee)))?;
                let kind = call_kind(gcx, hir_callee);
                cx.call_attrs(
                    &mut m,
                    named_args(args),
                    &cx.w(Args(args, hir_args)),
                    kind,
                    parent.try_call,
                )?;
                cx.expr_attrs(&mut m, hir, parent, gcx.is_lvalue(hir.id))?;
                m.end()
            }
            (ast::ExprKind::New(ty), hir::ExprKind::New(hir_ty)) => {
                let (mut m, _) = cx.node(s, "NewExpression", span)?;
                m.serialize_entry("typeName", &cx.w((ty, hir_ty)))?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (ast::ExprKind::Type(ty), hir::ExprKind::Type(hir_ty)) => {
                cx.w(TypeExpr { ast: ty, hir: hir_ty, expr: Some(hir), parent }).serialize(s)
            }
            (ast::ExprKind::Payable(args), hir::ExprKind::Payable(inner)) => {
                let (mut m, _) = cx.node(s, "FunctionCall", span)?;
                let hir_args = hir::CallArgs {
                    span: args.span,
                    kind: hir::CallArgsKind::Unnamed(std::slice::from_ref(*inner)),
                };
                // solc ends the callee with the `(` after it.
                let keyword = span.with_hi(span.lo() + kw::Payable.as_str().len() as u32);
                let callee =
                    PayableCallee { span: keyword.to(cx.token_after(keyword)), args: hir_args };
                m.serialize_entry("expression", &cx.w(callee))?;
                let arguments = cx.w(Args(args, &hir_args));
                cx.call_attrs(
                    &mut m,
                    named_args(args),
                    &arguments,
                    "typeConversion",
                    parent.try_call,
                )?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            (ast::ExprKind::TypeCall(ty), hir::ExprKind::TypeCall(hir_ty)) => {
                let (mut m, _) = cx.node(s, "FunctionCall", span)?;
                let arg_ty = gcx.mk_ty(TyKind::Type(gcx.type_of_hir_ty(hir_ty)));
                let callee = TypeCallee {
                    span: span.with_hi(span.lo() + kw::Type.as_str().len() as u32),
                    arg_ty,
                };
                m.serialize_entry("expression", &cx.w(callee))?;
                let arg = TypeExpr { ast: ty, hir: hir_ty, expr: None, parent: Parent::default() };
                cx.call_attrs(&mut m, &[], &[cx.w(arg)], "functionCall", parent.try_call)?;
                cx.expr_attrs(&mut m, hir, parent, false)?;
                m.end()
            }
            _ => unreachable!("lowering does not match the AST: {ast:?}"),
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
        m: &mut M,
        named: &[ast::NamedArg<'_>],
        arguments: &impl Serialize,
        kind: &str,
        try_call: bool,
    ) -> Result<(), M::Error> {
        m.serialize_entry("names", &Seq(|| named.iter().map(|arg| Sym(arg.name.name))))?;
        m.serialize_entry(
            "nameLocations",
            &Seq(|| named.iter().map(|arg| self.src(arg.name.span))),
        )?;
        m.serialize_entry("arguments", arguments)?;
        m.serialize_entry("tryCall", &try_call)?;
        m.serialize_entry("kind", kind)
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
    /// The arguments of the call.
    args: hir::CallArgs<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, PayableCallee<'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, PayableCallee { span, args }) = self;
        let gcx = cx.gcx;
        let (mut m, _) = cx.node(s, "ElementaryTypeNameExpression", *span)?;
        m.serialize_entry("typeName", &cx.w(ElementaryName(*span, ElementaryType::Address(true))))?;
        let ty = gcx.mk_ty(TyKind::Type(gcx.types.address_payable));
        m.serialize_entry("typeDescriptions", &cx.synthetic_type(ty, None))?;
        cx.argument_types(&mut m, Parent { args: Some(*args), ..Parent::default() })?;
        m.serialize_entry("isLValue", &false)?;
        m.serialize_entry("isPure", &true)?;
        m.serialize_entry("isConstant", &false)?;
        m.serialize_entry("lValueRequested", &false)?;
        m.end()
    }
}

/// An elementary type name in an expression, which solc gives no type.
struct ElementaryName(Span, ElementaryType);

impl Serialize for W<'_, '_, ElementaryName> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, ElementaryName(span, ty)) = self;
        let (mut m, _) = cx.node(s, "ElementaryTypeName", *span)?;
        m.serialize_entry("name", &elementary_name(*ty))?;
        m.serialize_entry("typeDescriptions", &Empty)?;
        if *ty == ElementaryType::Address(true) {
            m.serialize_entry("stateMutability", "payable")?;
        }
        m.end()
    }
}

/// The callee of `type(T)`: the builtin `type`.
struct TypeCallee<'gcx> {
    span: Span,
    /// The type of the argument, `type(T)`.
    arg_ty: Ty<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, TypeCallee<'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, TypeCallee { span, arg_ty }) = self;
        let gcx = cx.gcx;
        let (mut m, _) = cx.node(s, "Identifier", *span)?;
        m.serialize_entry("name", "type")?;
        m.serialize_entry("referencedDeclaration", &-27)?;
        m.serialize_entry("overloadedDeclarations", &[(); 0])?;
        let ty = gcx.mk_ty_fn_with_kind(TyFnKind::Builtin, &[], hir::StateMutability::Pure, &[]);
        m.serialize_entry("typeDescriptions", &cx.synthetic_type(ty, Some("metatype")))?;
        m.serialize_entry("argumentTypes", &[cx.synthetic_type(*arg_ty, None)])?;
        m.end()
    }
}

/// A type in an expression: `uint256`, `uint256[]`, or a user-defined type in `type(T)`.
struct TypeExpr<'a, 'ast, 'gcx> {
    ast: &'a ast::Type<'ast>,
    hir: &'a hir::Type<'gcx>,
    /// The expression of the type, which is none for the types of `type(T)` and array elements.
    expr: Option<&'a hir::Expr<'gcx>>,
    parent: Parent<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, TypeExpr<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, TypeExpr { ast, hir, expr, parent }) = self;
        let gcx = cx.gcx;
        let ty = expr
            .and_then(|expr| gcx.type_of_expr(expr.id))
            .unwrap_or_else(|| gcx.mk_ty(TyKind::Type(gcx.type_of_hir_ty(hir))));
        let attrs = |m: &mut S::SerializeMap| -> Result<(), S::Error> {
            m.serialize_entry("typeDescriptions", &cx.synthetic_type(ty, None))?;
            cx.argument_types(m, *parent)?;
            m.serialize_entry("isLValue", &false)?;
            m.serialize_entry("isPure", &true)?;
            m.serialize_entry("isConstant", &false)?;
            m.serialize_entry("lValueRequested", &false)
        };
        match (&ast.kind, &hir.kind) {
            (ast::TypeKind::Elementary(elementary), _) => {
                let (mut m, _) = cx.node(s, "ElementaryTypeNameExpression", ast.span)?;
                m.serialize_entry("typeName", &cx.w(ElementaryName(ast.span, *elementary)))?;
                attrs(&mut m)?;
                m.end()
            }
            (ast::TypeKind::Array(array), hir::TypeKind::Array(hir_array)) => {
                let (mut m, _) = cx.node(s, "IndexAccess", ast.span)?;
                let element = TypeExpr {
                    ast: &array.element,
                    hir: &hir_array.element,
                    expr: None,
                    parent: Parent::default(),
                };
                m.serialize_entry("baseExpression", &cx.w(element))?;
                let size = array.size.as_deref().zip(hir_array.size);
                serialize_opt(&mut m, "indexExpression", size.map(|exprs| cx.w(exprs)))?;
                attrs(&mut m)?;
                m.end()
            }
            (ast::TypeKind::Custom(path), hir::TypeKind::Custom(item)) => {
                // Lowering resolved the last segment in the scope of its contract.
                let mut res = resolve_path(*cx, path.segments());
                *res.last_mut().unwrap() = Some(Res::Item(*item));
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
    hir: &'a hir::Expr<'gcx>,
}

impl<'gcx> Serialize for W<'_, 'gcx, PathCall<'_, '_, 'gcx>> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        let Self(cx, PathCall { path, args, hir }) = self;
        let hir::ExprKind::Call(callee, hir_args) = &hir.kind else {
            unreachable!("lowering does not match the AST: {path:?}")
        };
        let (mut m, _) = cx.node(s, "FunctionCall", path.span().to(args.span))?;
        let parent = Parent { args: Some(*hir_args), ..Parent::default() };
        let res = resolve_path(*cx, path.segments());
        let callee = PathExpr { segments: path.segments(), res: &res, last: Some(callee), parent };
        m.serialize_entry("expression", &cx.w(callee))?;
        let arguments = cx.w(Args(args, hir_args));
        cx.call_attrs(&mut m, named_args(args), &arguments, "functionCall", false)?;
        cx.expr_attrs(&mut m, hir, Parent::default(), false)?;
        m.end()
    }
}

/// Returns what each segment of a name path resolves to, from name resolution alone.
fn resolve_path(cx: Cx<'_, '_>, segments: &[Ident]) -> Vec<Option<Res>> {
    // The only segment of a name is the last one, which lowering or type checking resolved.
    if let [_] = segments {
        return vec![None];
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
        let descriptions = |m: &mut S::SerializeMap| match (last, ty) {
            (Some(last), _) => {
                m.serialize_entry("typeDescriptions", &ExprType::new(*cx, last).args(parent.args))
            }
            (None, Some(ty)) => m.serialize_entry("typeDescriptions", &cx.synthetic_type(ty, None)),
            (None, None) => m.serialize_entry("typeDescriptions", &Empty),
        };
        if rest.is_empty() {
            let (mut m, _) = cx.node(s, "Identifier", span)?;
            m.serialize_entry("name", ident.as_str())?;
            let declaration = res_last.and_then(|res| cx.reference(res, ident, None));
            serialize_opt(&mut m, "referencedDeclaration", declaration)?;
            let overloads = match last.map(|last| &last.kind) {
                Some(hir::ExprKind::Ident(resolutions)) => cx.overloads(resolutions),
                _ => Vec::new(),
            };
            m.serialize_entry("overloadedDeclarations", &overloads)?;
            descriptions(&mut m)?;
            cx.argument_types(&mut m, *parent)?;
            return m.end();
        }
        let (mut m, _) = cx.node(s, "MemberAccess", span)?;
        m.serialize_entry("memberName", ident.as_str())?;
        m.serialize_entry("memberLocation", &cx.src(ident.span))?;
        let base = PathExpr {
            segments: rest,
            res: &res[..rest.len()],
            last: None,
            parent: Parent::default(),
        };
        m.serialize_entry("expression", &cx.w(base))?;
        let base_res = res[rest.len() - 1];
        let declaration = res_last.and_then(|res| cx.reference(res, ident, base_res));
        serialize_opt(&mut m, "referencedDeclaration", declaration)?;
        descriptions(&mut m)?;
        cx.argument_types(&mut m, *parent)?;
        m.serialize_entry("isLValue", &false)?;
        // A member of a module is pure, but a member of a contract is not.
        let of_module = res.len().checked_sub(2).and_then(|i| res[i]);
        m.serialize_entry("isPure", &matches!(of_module, Some(Res::Namespace(_))))?;
        m.serialize_entry("isConstant", &false)?;
        m.serialize_entry("lValueRequested", &false)?;
        m.end()
    }
}

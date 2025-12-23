//! Yul HIR.

use crate::hir;
use solar_ast as ast;
use solar_interface::{Ident, Span};

/// A block of Yul statements: `{ ... }`.
#[derive(Debug)]
pub struct Block<'hir> {
    /// The span of the block, including the `{` and `}`.
    pub span: Span,
    /// The statements in the block.
    pub stmts: &'hir [Stmt<'hir>],
}

impl<'hir> std::ops::Deref for Block<'hir> {
    type Target = [Stmt<'hir>];

    fn deref(&self) -> &Self::Target {
        self.stmts
    }
}

/// A Yul statement.
#[derive(Debug)]
pub struct Stmt<'hir> {
    /// The span of the statement.
    pub span: Span,
    /// The kind of statement.
    pub kind: StmtKind<'hir>,
}

/// A kind of Yul statement.
#[derive(Debug)]
pub enum StmtKind<'hir> {
    /// A blocked scope: `{ ... }`.
    Block(Block<'hir>),

    /// A single-variable assignment statement: `x := 1`.
    AssignSingle(Path<'hir>, &'hir Expr<'hir>),

    /// A multiple-variable assignment statement: `x, y, z := foo(1, 2)`.
    AssignMulti(&'hir [Path<'hir>], &'hir Expr<'hir>),

    /// An expression statement. This can only be a function call.
    Expr(&'hir Expr<'hir>),

    /// An if statement: `if lt(a, b) { ... }`.
    If(&'hir Expr<'hir>, Block<'hir>),

    /// A for statement: `for {let i := 0} lt(i,10) {i := add(i,1)} { ... }`.
    For(&'hir StmtFor<'hir>),

    /// A switch statement: `switch expr case 0 { ... } default { ... }`.
    Switch(StmtSwitch<'hir>),

    /// A leave statement: `leave`.
    Leave,

    /// A break statement: `break`.
    Break,

    /// A continue statement: `continue`.
    Continue,

    /// A function definition statement: `function f() { ... }`.
    FunctionDef(Function<'hir>),

    /// A variable declaration statement: `let x := 0`.
    VarDecl(&'hir [Ident], Option<&'hir Expr<'hir>>),
}

/// A Yul for statement: `for {let i := 0} lt(i,10) {i := add(i,1)} { ... }`.
#[derive(Debug)]
pub struct StmtFor<'hir> {
    pub init: Block<'hir>,
    pub cond: Expr<'hir>,
    pub step: Block<'hir>,
    pub body: Block<'hir>,
}

/// A Yul switch statement: `switch expr case 0 { ... } default { ... }`.
#[derive(Debug)]
pub struct StmtSwitch<'hir> {
    pub selector: Expr<'hir>,
    /// The cases of the switch statement. Includes the default case in the last position, if any.
    pub cases: &'hir [StmtSwitchCase<'hir>],
}

impl<'hir> StmtSwitch<'hir> {
    /// Returns the default case of the switch statement, if any.
    pub fn default_case(&self) -> Option<&StmtSwitchCase<'hir>> {
        self.cases.last().filter(|case| case.constant.is_none())
    }
}

/// Represents a non-default case of a Yul switch statement.
#[derive(Debug)]
pub struct StmtSwitchCase<'hir> {
    pub span: Span,
    /// The constant of the case, if any. `None` for the default case.
    pub constant: Option<&'hir hir::Lit<'hir>>,
    pub body: Block<'hir>,
}

/// Yul function definition: `function f() -> a, b { ... }`.
#[derive(Debug)]
pub struct Function<'hir> {
    pub name: Ident,
    pub parameters: &'hir [Ident],
    pub returns: &'hir [Ident],
    pub body: Block<'hir>,
}

/// A Yul expression.
#[derive(Debug)]
pub struct Expr<'hir> {
    /// The span of the expression.
    pub span: Span,
    /// The kind of expression.
    pub kind: ExprKind<'hir>,
}

/// A kind of Yul expression.
#[derive(Debug)]
pub enum ExprKind<'hir> {
    /// A single path.
    Path(Path<'hir>),
    /// A function call: `foo(a, b)`.
    Call(ExprCall<'hir>),
    /// A literal.
    Lit(&'hir hir::Lit<'hir>),
}

/// A Yul path, which is just a dot-separated list of identifiers.
#[derive(Clone, Copy, Debug)]
pub struct Path<'hir> {
    pub segments: &'hir [Ident],
}

/// A Yul function call expression: `foo(a, b)`.
#[derive(Debug)]
pub struct ExprCall<'hir> {
    pub name: Ident,
    pub arguments: &'hir [Expr<'hir>],
}


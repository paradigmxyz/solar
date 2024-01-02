//! Yul AST.

use super::{Lit, Path};
use sulk_interface::{Ident, Span};

/// A block of Yul statements: `{ ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulBlock>
pub type Block = Vec<Stmt>;

/// A Yul statement.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulStatement>
#[derive(Clone, Debug)]
pub struct Stmt {
    /// The span of the statement.
    pub span: Span,
    /// The kind of statement.
    pub kind: StmtKind,
}

/// A kind of Yul statement.
#[derive(Clone, Debug)]
pub enum StmtKind {
    /// A blocked scope: `{ ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulBlock>
    Block(Block),

    /// A single-variable assignment statement: `x := 1`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulAssignment>
    AssignSingle(Path, Expr),

    /// A multiple-variable assignment statement: `x, y, z := foo(1, 2)`.
    ///
    /// Multi-assignments require a function call on the right-hand side.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulAssignment>
    AssignMulti(Vec<Path>, ExprCall),

    /// An expression statement. This can only be a function call.
    Expr(ExprCall),

    /// An if statement: `if lt(a, b) { ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulIfStatement>
    If(Expr, Block),

    /// A for statement: `for {let i := 0} lt(i,10) {i := add(i,1)} { ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulForStatement>
    ///
    /// Breakdown of parts: <https://docs.soliditylang.org/en/latest/yul.html#loops>
    For { init: Block, cond: Expr, step: Block, body: Block },

    /// A switch statement: `switch expr case 0 { ... } default { ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulSwitchStatement>
    Switch(StmtSwitch),

    /// A leave statement: `leave`.
    Leave,

    /// A break statement: `break`.
    Break,

    /// A continue statement: `continue`.
    Continue,

    /// A function definition statement: `function f() { ... }`.
    FunctionDef(Function),

    /// A variable declaration statement: `let x := 0`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulVariableDeclaration>
    VarDecl(Vec<Ident>, Option<Expr>),
}

/// A Yul switch statement can consist of only a default-case or one
/// or more non-default cases optionally followed by a default-case.
///
/// Example switch statement in Yul:
///
/// ```solidity
/// switch exponent
/// case 0 { result := 1 }
/// case 1 { result := base }
/// default { revert(0, 0) }
/// ```
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulSwitchStatement>
#[derive(Clone, Debug)]
pub struct StmtSwitch {
    pub selector: Expr,
    pub branches: Vec<StmtSwitchCase>,
    pub default_case: Option<Block>,
}

/// Represents a non-default case of a Yul switch statement.
///
/// See [`StmtSwitch`] for more information.
#[derive(Clone, Debug)]
pub struct StmtSwitchCase {
    pub constant: Lit,
    pub body: Block,
}

/// Yul function definition: `function f() -> a, b { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulFunctionDefinition>
#[derive(Clone, Debug)]
pub struct Function {
    pub name: Ident,
    pub parameters: Vec<Ident>,
    pub returns: Vec<Ident>,
    pub body: Block,
}

/// A Yul expression.
#[derive(Clone, Debug)]
pub struct Expr {
    /// The span of the expression.
    pub span: Span,
    /// The kind of expression.
    pub kind: ExprKind,
}

/// A kind of Yul expression.
#[derive(Clone, Debug)]
pub enum ExprKind {
    /// A single identifier.
    Ident(Ident),
    /// A function call: `foo(a, b)`.
    Call(ExprCall),
    /// A literal.
    Lit(Lit),
}

/// A Yul function call expression: `foo(a, b)`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulFunctionCall>
#[derive(Clone, Debug)]
pub struct ExprCall {
    pub name: Ident,
    pub arguments: Vec<Expr>,
}

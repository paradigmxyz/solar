//! Yul AST.

use super::{DocComments, Lit, Path, StrLit};
use bumpalo::boxed::Box;
use sulk_interface::{Ident, Span};

/// A block of Yul statements: `{ ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulBlock>
pub type Block<'ast> = Box<'ast, [Stmt<'ast>]>;

/// A Yul object.
///
/// Reference: <https://docs.soliditylang.org/en/latest/yul.html#specification-of-yul-object>
#[derive(Debug)]
pub struct Object<'ast> {
    /// The doc-comments of the object.
    pub docs: DocComments<'ast>,
    /// The span of the object, including the `object` keyword, but excluding the doc-comments.
    pub span: Span,
    /// The name of the object.
    pub name: StrLit,
    /// The `code` block.
    pub code: CodeBlock<'ast>,
    /// Sub-objects, if any.
    pub children: Box<'ast, [Object<'ast>]>,
    /// `data` segments, if any.
    pub data: Box<'ast, [Data]>,
}

/// A Yul `code` block. See [`Object`].
#[derive(Debug)]
pub struct CodeBlock<'ast> {
    /// The span of the code block, including the `code` keyword.
    ///
    /// The `code` keyword may not be present in the source code if the object is parsed as a
    /// plain [`Block`].
    pub span: Span,
    /// The `code` block.
    pub code: Block<'ast>,
}

/// A Yul `data` segment. See [`Object`].
#[derive(Clone, Debug)]
pub struct Data {
    /// The span of the code block, including the `data` keyword.
    pub span: Span,
    /// The name of the data segment.
    pub name: StrLit,
    /// The data. Can only be a `Str` or `HexStr` literal.
    pub data: Lit,
}

/// A Yul statement.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulStatement>
#[derive(Debug)]
pub struct Stmt<'ast> {
    /// The doc-comments of the statement.
    pub docs: DocComments<'ast>,
    /// The span of the statement.
    pub span: Span,
    /// The kind of statement.
    pub kind: StmtKind<'ast>,
}

/// A kind of Yul statement.
#[derive(Debug)]
pub enum StmtKind<'ast> {
    /// A blocked scope: `{ ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulBlock>
    Block(Block<'ast>),

    /// A single-variable assignment statement: `x := 1`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulAssignment>
    AssignSingle(Path, Expr<'ast>),

    /// A multiple-variable assignment statement: `x, y, z := foo(1, 2)`.
    ///
    /// Multi-assignments require a function call on the right-hand side.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulAssignment>
    AssignMulti(Box<'ast, [Path]>, ExprCall<'ast>),

    /// An expression statement. This can only be a function call.
    Expr(ExprCall<'ast>),

    /// An if statement: `if lt(a, b) { ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulIfStatement>
    If(Expr<'ast>, Block<'ast>),

    /// A for statement: `for {let i := 0} lt(i,10) {i := add(i,1)} { ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulForStatement>
    ///
    /// Breakdown of parts: <https://docs.soliditylang.org/en/latest/yul.html#loops>
    For { init: Block<'ast>, cond: Expr<'ast>, step: Block<'ast>, body: Block<'ast> },

    /// A switch statement: `switch expr case 0 { ... } default { ... }`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulSwitchStatement>
    Switch(StmtSwitch<'ast>),

    /// A leave statement: `leave`.
    Leave,

    /// A break statement: `break`.
    Break,

    /// A continue statement: `continue`.
    Continue,

    /// A function definition statement: `function f() { ... }`.
    FunctionDef(Function<'ast>),

    /// A variable declaration statement: `let x := 0`.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulVariableDeclaration>
    VarDecl(Box<'ast, [Ident]>, Option<Expr<'ast>>),
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
#[derive(Debug)]
pub struct StmtSwitch<'ast> {
    pub selector: Expr<'ast>,
    pub branches: Box<'ast, [StmtSwitchCase<'ast>]>,
    pub default_case: Option<Block<'ast>>,
}

/// Represents a non-default case of a Yul switch statement.
///
/// See [`StmtSwitch`] for more information.
#[derive(Debug)]
pub struct StmtSwitchCase<'ast> {
    pub constant: Lit,
    pub body: Block<'ast>,
}

/// Yul function definition: `function f() -> a, b { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulFunctionDefinition>
#[derive(Debug)]
pub struct Function<'ast> {
    pub name: Ident,
    pub parameters: Box<'ast, [Ident]>,
    pub returns: Box<'ast, [Ident]>,
    pub body: Block<'ast>,
}

/// A Yul expression.
#[derive(Debug)]
pub struct Expr<'ast> {
    /// The span of the expression.
    pub span: Span,
    /// The kind of expression.
    pub kind: ExprKind<'ast>,
}

/// A kind of Yul expression.
#[derive(Debug)]
pub enum ExprKind<'ast> {
    /// A single path.
    Path(Path),
    /// A function call: `foo(a, b)`.
    Call(ExprCall<'ast>),
    /// A literal.
    Lit(Lit),
}

/// A Yul function call expression: `foo(a, b)`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulFunctionCall>
#[derive(Debug)]
pub struct ExprCall<'ast> {
    pub name: Ident,
    pub arguments: Box<'ast, [Expr<'ast>]>,
}

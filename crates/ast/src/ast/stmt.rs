use super::{yul, CallArgs, DocComments, Expr, ParameterList, Path, StrLit, VariableDefinition};
use bumpalo::boxed::Box;
use sulk_interface::{Ident, Span};

/// A block of statements.
pub type Block<'ast> = Box<'ast, [Stmt<'ast>]>;

/// A statement, usually ending in a semicolon.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.statement>
#[derive(Debug)]
pub struct Stmt<'ast> {
    pub docs: DocComments<'ast>,
    pub span: Span,
    pub kind: StmtKind<'ast>,
}

/// A kind of statement.
#[derive(Debug)]
pub enum StmtKind<'ast> {
    /// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
    Assembly(StmtAssembly<'ast>),

    /// A single-variable declaration statement: `uint256 foo = 42;`.
    DeclSingle(VariableDefinition<'ast>),

    /// A multi-variable declaration statement: `(bool success, bytes memory value) = ...;`.
    ///
    /// Multi-assignments require an expression on the right-hand side.
    DeclMulti(Box<'ast, [Option<VariableDefinition<'ast>>]>, Box<'ast, Expr<'ast>>),

    /// A blocked scope: `{ ... }`.
    Block(Block<'ast>),

    /// A break statement: `break;`.
    Break,

    /// A continue statement: `continue;`.
    Continue,

    /// A do-while statement: `do { ... } while (condition);`.
    DoWhile(Block<'ast>, Box<'ast, Expr<'ast>>),

    /// An emit statement: `emit Foo.bar(42);`.
    Emit(Path, CallArgs<'ast>),

    /// An expression with a trailing semicolon.
    Expr(Box<'ast, Expr<'ast>>),

    /// A for statement: `for (uint256 i; i < 42; ++i) { ... }`.
    For {
        init: Option<Box<'ast, Stmt<'ast>>>,
        cond: Option<Box<'ast, Expr<'ast>>>,
        next: Option<Box<'ast, Expr<'ast>>>,
        body: Box<'ast, Stmt<'ast>>,
    },

    /// An `if` statement with an optional `else` block: `if (expr) { ... } else { ... }`.
    If(Box<'ast, Expr<'ast>>, Box<'ast, Stmt<'ast>>, Option<Box<'ast, Stmt<'ast>>>),

    /// A return statement: `return 42;`.
    Return(Option<Box<'ast, Expr<'ast>>>),

    /// A revert statement: `revert Foo.bar(42);`.
    Revert(Path, CallArgs<'ast>),

    /// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
    Try(StmtTry<'ast>),

    /// An unchecked block: `unchecked { ... }`.
    UncheckedBlock(Block<'ast>),

    /// A while statement: `while (i < 42) { ... }`.
    While(Box<'ast, Expr<'ast>>, Box<'ast, Stmt<'ast>>),
}

/// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
#[derive(Debug)]
pub struct StmtAssembly<'ast> {
    /// The assembly block dialect.
    pub dialect: Option<StrLit>,
    /// Additional flags.
    pub flags: Box<'ast, [StrLit]>,
    /// The assembly block.
    pub block: yul::Block<'ast>,
}

/// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.tryStatement>
#[derive(Debug)]
pub struct StmtTry<'ast> {
    pub expr: Box<'ast, Expr<'ast>>,
    pub returns: ParameterList<'ast>,
    /// The try block.
    pub block: Block<'ast>,
    /// The list of catch clauses. Cannot be parsed empty.
    pub catch: Box<'ast, [CatchClause<'ast>]>,
}

/// A catch clause: `catch (...) { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.catchClause>
#[derive(Debug)]
pub struct CatchClause<'ast> {
    pub name: Option<Ident>,
    pub args: ParameterList<'ast>,
    pub block: Block<'ast>,
}

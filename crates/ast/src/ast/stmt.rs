use super::{yul, CallArgs, DocComment, Expr, ParameterList, Path, StrLit, VariableDeclaration};
use sulk_interface::{Ident, Span};

/// A block of statements.
pub type Block = Vec<Stmt>;

/// A statement, usually ending in a semicolon.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.statement>
#[derive(Clone, Debug)]
pub struct Stmt {
    pub docs: Vec<DocComment>,
    pub span: Span,
    pub kind: StmtKind,
}

/// A kind of statement.
#[derive(Clone, Debug)]
pub enum StmtKind {
    /// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
    Assembly(StmtAssembly),

    /// A single-variable declaration statement: `uint256 foo = 42;`.
    DeclSingle(VariableDeclaration, Option<Box<Expr>>),

    /// A multi-variable declaration statement: `(bool success, bytes memory value) = ...;`.
    ///
    /// Multi-assignments require an expression on the right-hand side.
    DeclMulti(Vec<Option<VariableDeclaration>>, Box<Expr>),

    /// A blocked scope: `{ ... }`.
    Block(Block),

    /// A break statement: `break;`.
    Break,

    /// A continue statement: `continue;`.
    Continue,

    /// A do-while statement: `do { ... } while (condition);`.
    DoWhile(Block, Box<Expr>),

    /// An emit statement: `emit Foo.bar(42);`.
    Emit(Path, CallArgs),

    /// An expression with a trailing semicolon.
    Expr(Box<Expr>),

    /// A for statement: `for (uint256 i; i < 42; ++i) { ... }`.
    For {
        init: Option<Box<Stmt>>,
        cond: Option<Box<Expr>>,
        next: Option<Box<Expr>>,
        body: Box<Stmt>,
    },

    /// An `if` statement with an optional `else` block: `if (expr) { ... } else
    /// { ... }`.
    If(Box<Expr>, Box<Stmt>, Option<Box<Stmt>>),

    /// A return statement: `return 42;`.
    Return(Option<Box<Expr>>),

    /// A revert statement: `revert Foo.bar(42);`.
    Revert(Path, CallArgs),

    /// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
    Try(StmtTry),

    /// An unchecked block: `unchecked { ... }`.
    UncheckedBlock(Block),

    /// A while statement: `while (i < 42) { ... }`.
    While(Box<Expr>, Box<Stmt>),
}

/// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
#[derive(Clone, Debug)]
pub struct StmtAssembly {
    /// The assembly block dialect.
    pub dialect: Option<StrLit>,
    /// Additional flags.
    pub flags: Vec<StrLit>,
    /// The assembly block.
    pub block: yul::Block,
}

/// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.tryStatement>
#[derive(Clone, Debug)]
pub struct StmtTry {
    pub expr: Box<Expr>,
    pub returns: ParameterList,
    /// The try block.
    pub block: Block,
    /// The list of catch clauses. Cannot be parsed empty.
    pub catch: Vec<CatchClause>,
}

/// A catch clause: `catch (...) { ... }`.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.tryStatement>
#[derive(Clone, Debug)]
pub struct CatchClause {
    pub name: Option<Ident>,
    pub args: ParameterList,
    pub block: Block,
}

/// A kind of variable declaration statement.
#[derive(Clone, Debug)]
pub enum VarDeclKind {
    /// A single variable declaration: `uint x ...`.
    Single(VariableDeclaration),
    /// A tuple of variable declarations: `(uint x, uint y) ...`.
    Tuple(Vec<Option<VariableDeclaration>>),
}

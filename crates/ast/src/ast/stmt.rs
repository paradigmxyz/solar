use super::{Expr, ParameterList, Path, VariableDeclaration};
use sulk_interface::{Ident, Span};

/// A block of statements.
pub type Block = Vec<Stmt>;

/// A statement, usually ending in a semicolon.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.statement>
pub struct Stmt {
    pub span: Span,
    pub kind: StmtKind,
}

/// A kind of statement.
#[derive(Clone)]
pub enum StmtKind {
    /// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
    Assembly(StmtAssembly),

    /// A blocked scope: `{ ... }`.
    Block(Block),

    /// A break statement: `break;`.
    Break,

    /// A continue statement: `continue;`.
    Continue,

    /// A do-while statement: `do { ... } while (condition);`.
    DoWhile(Block, Box<Expr>),

    /// An emit statement: `emit FooBar(42);`.
    Emit(Path, ParameterList),

    /// An expression with a trailing semicolon.
    Expr(Box<Expr>),

    /// A for statement: `for (uint256 i; i < 42; ++i) { ... }`.
    For { init: Box<Stmt>, cond: Option<Box<Expr>>, next: Option<Box<Stmt>>, block: Block },

    /// An `if` statement with an optional `else` block: `if (expr) { ... } else
    /// { ... }`.
    If(Box<Expr>, Block, Option<Box<Stmt>>),

    /// A return statement: `return 42;`.
    Return(Box<Expr>),

    /// A revert statement: `revert Custom.Error(...);`.
    Revert(Path, ParameterList),

    /// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
    Try(StmtTry),

    /// An unchecked block: `unchecked { ... }`.
    UncheckedBlock(Block),

    /// A variable declaration statement: `uint256 foo = 42;`.
    VarDecl(VarDeclKind, Option<Box<Expr>>),

    /// A while statement: `while (i < 42) { ... }`.
    While(Box<Expr>, Block),
}

/// An assembly block, with optional flags: `assembly "evmasm" (...) { ... }`.
pub struct StmtAssembly {
    /// The assembly block dialect.
    pub dialect: LitStr,
    /// Additional flags.
    pub flags: Vec<LitStr>,
    /// The assembly block.
    pub block: Block,
}

/// A try statement: `try fooBar(42) returns (...) { ... } catch (...) { ... }`.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.tryStatement>
#[derive(Clone)]
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
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.tryStatement>
pub struct CatchClause {
    pub name: Option<Ident>,
    pub list: ParameterList,
    pub block: Block,
}

/// A kind of variable declaration statement.
pub enum VarDeclKind {
    /// A single variable declaration: `uint x ...`.
    Single(VariableDeclaration),
    /// A tuple of variable declarations: `(uint x, uint y) ...`.
    Tuple(Vec<Option<VariableDeclaration>>),
}

impl VarDeclKind {
    pub fn list(&self) -> &[Option<VariableDeclaration>] {
        match self {
            VarDeclKind::Single(decl) => std::slice::from_ref(Some(decl)),
            VarDeclKind::Tuple(list) => list,
        }
    }
}

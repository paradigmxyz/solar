//! Yul AST.

/// A block of Yul statements.
pub type YulBlock = Vec<YulStmt>;

/// A Yul statement.
///
/// Solidity Reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulStatement>
#[derive(Clone, Debug)]
pub enum YulStmt {
    /// A Yul blocked scope: `{ ... }`.
    Block(YulBlock),
    /*
    /// A variable declaration statement: `let x := 0`.
    Decl(YulVarDecl),

    /// A variable assignment statement: `x := 1`.
    Assign(YulVarAssign),

    /// A function call statement: `foo(a, b)`.
    Call(YulFnCall),

    /// A if statement: `if lt(a, b) { ... }`.
    If(YulIf),

    /// A for statement: `for {let i := 0} lt(i,10) {i := add(i,1)} { ... }`.
    For(YulFor),

    /// A switch statement: `switch expr case 0 { ... } default { ... }`.
    Switch(YulSwitch),

    /// A leave statement: `leave`.
    Leave,

    /// A break statement: `break`.
    Break,

    /// A continue statement: `continue`.
    Continue,

    /// A function definition statement: `function f() { ... }`.
    FunctionDef(YulFunction),

    */
}

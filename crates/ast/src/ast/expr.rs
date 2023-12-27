use super::{LitKind, Ty};
use std::fmt;
use sulk_interface::{Ident, Span};

/// A list of named arguments: `{a: "1", b: 2}`.
pub type NamedArgList = Vec<NamedArg>;

/// An expression.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.expression>
#[derive(Clone, Debug)]
pub struct Expr {
    pub span: Span,
    pub kind: ExprKind,
}

/// A kind of expression.
#[derive(Clone, Debug)]
pub enum ExprKind {
    /// An array literal expression: `[a, b, c, d]`.
    Array(Vec<Expr>),

    /// An assignment: `a = b`, `a += b`.
    Assign(Box<Expr>, Option<BinOp>, Box<Expr>),

    /// A binary operation: `a + b`, `a >> b`.
    Binary(Box<Expr>, BinOp, Box<Expr>),

    /// A function call expression: `foo(42)` or `foo({ bar: 42 })`.
    Call(Box<Expr>, CallArgs),

    /// Function call options: `foo.bar{ value: 1, gas: 2 }`.
    CallOptions(Box<Expr>, NamedArgList),

    /// A unary `delete` expression: `delete vector`.
    Delete(Box<Expr>),

    /// An identifier: `foo`.
    Ident(Ident),

    /// A square bracketed indexing expression: `vector[2]`, `slice[42:69]`.
    Index(Box<Expr>, Option<Box<Expr>>, Option<Box<Expr>>),

    /// A literal: `hex"1234"`, `5.6 ether`.
    Lit(LitKind),

    /// Access of a named member: `obj.k`.
    Member(Box<Expr>, Ident),

    /// A `new` expression: `new Contract`.
    New(Ty),

    /// A `payable` expression: `payable(address(0x...))`.
    Payable(Box<Expr>),

    /// A ternary (AKA conditional) expression: `foo ? bar : baz`.
    Ternary(Box<Expr>, Box<Expr>, Box<Expr>),

    /// A tuple expression: `(a, b, c, d)`.
    Tuple(Vec<Expr>),

    /// A `type()` expression: `type(uint256)`
    TypeCall(Ty),

    /// A unary operation: `!x`, `-x`, `x++`.
    Unary(UnOp, Box<Expr>),
}

/// A binary operation: `a + b`, `a += b`.
#[derive(Clone, Debug)]
pub struct BinOp {
    pub span: Span,
    pub kind: BinOpKind,
}

/// A kind of binary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BinOpKind {
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `>`
    Gt,
    /// `>=`
    Ge,
    /// `==`
    Eq,
    /// `!=`
    Ne,
    /// `||`
    Or,
    /// `&&`
    And,

    /// `>>>`
    Sar,
    /// `>>`
    Shr,
    /// `<<`
    Shl,
    /// `&`
    BitAnd,
    /// `|`
    BitOr,
    /// `^`
    BitXor,

    /// `+`
    Add,
    /// `-`
    Sub,
    /// `**`
    Pow,
    /// `*`
    Mul,
    /// `/`
    Div,
    /// `%`
    Rem,
}

impl fmt::Display for BinOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind.to_str())
    }
}

impl BinOpKind {
    /// Returns the string representation of the operator.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Lt => "<",
            Self::Le => "<=",
            Self::Gt => ">",
            Self::Ge => ">=",
            Self::Eq => "==",
            Self::Ne => "!=",
            Self::Or => "||",
            Self::And => "&&",
            Self::Sar => ">>>",
            Self::Shr => ">>",
            Self::Shl => "<<",
            Self::BitAnd => "&",
            Self::BitOr => "|",
            Self::BitXor => "^",
            Self::Add => "+",
            Self::Sub => "-",
            Self::Pow => "**",
            Self::Mul => "*",
            Self::Div => "/",
            Self::Rem => "%",
        }
    }

    /// Returns `true` if the operator is able to be used in an assignment.
    pub const fn assignable(self) -> bool {
        // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.expression
        match self {
            Self::BitOr
            | Self::BitXor
            | Self::BitAnd
            | Self::Shl
            | Self::Shr
            | Self::Sar
            | Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::Rem => true,

            Self::Lt
            | Self::Le
            | Self::Gt
            | Self::Ge
            | Self::Eq
            | Self::Ne
            | Self::Or
            | Self::And
            | Self::Pow => false,
        }
    }
}

/// A unary operation: `!x`, `-x`, `x++`.
#[derive(Clone, Debug)]
pub struct UnOp {
    pub span: Span,
    pub kind: UnOpKind,
}

/// A kind of unary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum UnOpKind {
    /// `++x`
    PrefixInc,
    /// `--x`
    PrefixDec,
    /// `!`
    Not,
    /// `-`
    Neg,
    /// `~`
    BitNot,

    /// `x++`
    PostfixInc,
    /// `x--`
    PostfixDec,
}

impl fmt::Display for UnOp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.kind.to_str())
    }
}

impl UnOpKind {
    /// Returns the string representation of the operator.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::PrefixInc => "++",
            Self::PrefixDec => "--",
            Self::Not => "!",
            Self::Neg => "-",
            Self::BitNot => "~",
            Self::PostfixInc => "++",
            Self::PostfixDec => "--",
        }
    }

    /// Returns `true` if the operator is a prefix operator.
    pub const fn is_prefix(self) -> bool {
        match self {
            Self::PrefixInc | Self::PrefixDec | Self::Not | Self::Neg | Self::BitNot => true,
            Self::PostfixInc | Self::PostfixDec => false,
        }
    }

    /// Returns `true` if the operator is a postfix operator.
    pub const fn is_postfix(self) -> bool {
        !self.is_prefix()
    }
}

/// A list of function call arguments.
#[derive(Clone, Debug)]
pub enum CallArgs {
    /// A list of unnamed arguments: `(1, 2, 3)`.
    Unnamed(Vec<Expr>),

    /// A list of named arguments: `({x: 1, y: 2, z: 3})`.
    Named(NamedArgList),
}

/// A named argument: `name: value`.
#[derive(Clone, Debug)]
pub struct NamedArg {
    pub name: Ident,
    pub value: Expr,
}

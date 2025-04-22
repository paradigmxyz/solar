use super::{Box, Lit, SubDenomination, Type};
use either::Either;
use solar_interface::{Ident, Span};
use std::fmt;

/// A list of named arguments: `{a: "1", b: 2}`.
///
/// Present in [`CallArgsKind::Named`] and [`ExprKind::CallOptions`].
pub type NamedArgList<'ast> = Box<'ast, [NamedArg<'ast>]>;

/// An expression.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.expression>
#[derive(Debug)]
pub struct Expr<'ast> {
    pub span: Span,
    pub kind: ExprKind<'ast>,
}

impl AsRef<Self> for Expr<'_> {
    fn as_ref(&self) -> &Self {
        self
    }
}

impl<'ast> Expr<'ast> {
    /// Creates a new expression from an identifier.
    pub fn from_ident(ident: Ident) -> Self {
        Self { span: ident.span, kind: ExprKind::Ident(ident) }
    }

    /// Creates a new expression from a type.
    pub fn from_ty(ty: Type<'ast>) -> Self {
        Self { span: ty.span, kind: ExprKind::Type(ty) }
    }
}

/// A kind of expression.
#[derive(Debug)]
pub enum ExprKind<'ast> {
    /// An array literal expression: `[a, b, c, d]`.
    Array(Box<'ast, [Box<'ast, Expr<'ast>>]>),

    /// An assignment: `a = b`, `a += b`.
    Assign(Box<'ast, Expr<'ast>>, Option<BinOp>, Box<'ast, Expr<'ast>>),

    /// A binary operation: `a + b`, `a >> b`.
    Binary(Box<'ast, Expr<'ast>>, BinOp, Box<'ast, Expr<'ast>>),

    /// A function call expression: `foo(42)` or `foo({ bar: 42 })`.
    Call(Box<'ast, Expr<'ast>>, CallArgs<'ast>),

    /// Function call options: `foo.bar{ value: 1, gas: 2 }`.
    CallOptions(Box<'ast, Expr<'ast>>, NamedArgList<'ast>),

    /// A unary `delete` expression: `delete vector`.
    Delete(Box<'ast, Expr<'ast>>),

    /// An identifier: `foo`.
    Ident(Ident),

    /// A square bracketed indexing expression: `vector[index]`, `slice[l:r]`.
    Index(Box<'ast, Expr<'ast>>, IndexKind<'ast>),

    /// A literal: `hex"1234"`, `5.6 ether`.
    ///
    /// Note that the `SubDenomination` is only present for numeric literals, and it's already
    /// applied to `Lit`'s value. It is only present for error reporting/formatting purposes.
    Lit(&'ast mut Lit, Option<SubDenomination>),

    /// Access of a named member: `obj.k`.
    Member(Box<'ast, Expr<'ast>>, Ident),

    /// A `new` expression: `new Contract`.
    New(Type<'ast>),

    /// A `payable` expression: `payable(address(0x...))`.
    Payable(CallArgs<'ast>),

    /// A ternary (AKA conditional) expression: `foo ? bar : baz`.
    Ternary(Box<'ast, Expr<'ast>>, Box<'ast, Expr<'ast>>, Box<'ast, Expr<'ast>>),

    /// A tuple expression: `(a,,, b, c, d)`.
    Tuple(Box<'ast, [Option<Box<'ast, Expr<'ast>>>]>),

    /// A `type()` expression: `type(uint256)`.
    TypeCall(Type<'ast>),

    /// An elementary type name: `uint256`.
    Type(Type<'ast>),

    /// A unary operation: `!x`, `-x`, `x++`.
    Unary(UnOp, Box<'ast, Expr<'ast>>),
}

/// A binary operation: `a + b`, `a += b`.
#[derive(Clone, Copy, Debug)]
pub struct BinOp {
    pub span: Span,
    pub kind: BinOpKind,
}

/// A kind of binary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

    /// `>>`
    Shr,
    /// `<<`
    Shl,
    /// `>>>`
    Sar,
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
#[derive(Clone, Copy, Debug)]
pub struct UnOp {
    pub span: Span,
    pub kind: UnOpKind,
}

/// A kind of unary operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum UnOpKind {
    /// `++x`
    PreInc,
    /// `--x`
    PreDec,
    /// `!`
    Not,
    /// `-`
    Neg,
    /// `~`
    BitNot,

    /// `x++`
    PostInc,
    /// `x--`
    PostDec,
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
            Self::PreInc => "++",
            Self::PreDec => "--",
            Self::Not => "!",
            Self::Neg => "-",
            Self::BitNot => "~",
            Self::PostInc => "++",
            Self::PostDec => "--",
        }
    }

    /// Returns `true` if the operator is a prefix operator.
    pub const fn is_prefix(self) -> bool {
        match self {
            Self::PreInc | Self::PreDec | Self::Not | Self::Neg | Self::BitNot => true,
            Self::PostInc | Self::PostDec => false,
        }
    }

    /// Returns `true` if the operator is a postfix operator.
    pub const fn is_postfix(self) -> bool {
        !self.is_prefix()
    }
}

/// A list of function call arguments.
#[derive(Debug)]
pub struct CallArgs<'ast> {
    /// The span of the arguments. This points to the parenthesized list of arguments.
    ///
    /// If the list is empty, this points to the empty `()` or to where the `(` would be.
    pub span: Span,
    pub kind: CallArgsKind<'ast>,
}

impl<'ast> CallArgs<'ast> {
    /// Creates a new empty list of arguments.
    ///
    /// `span` should be an empty span.
    pub fn empty(span: Span) -> Self {
        Self { span, kind: CallArgsKind::empty() }
    }

    /// Returns `true` if the argument list is not present in the source code.
    ///
    /// For example, a modifier `m` can be invoked in a function declaration as `m` or `m()`. In the
    /// first case, this returns `true`, and the span will point to after `m`. In the second case,
    /// this returns `false`.
    pub fn is_dummy(&self) -> bool {
        self.span.lo() == self.span.hi()
    }

    /// Returns the length of the arguments.
    pub fn len(&self) -> usize {
        self.kind.len()
    }

    /// Returns `true` if the list of arguments is empty.
    pub fn is_empty(&self) -> bool {
        self.kind.is_empty()
    }

    /// Returns an iterator over the expressions.
    pub fn exprs(
        &self,
    ) -> impl ExactSizeIterator<Item = &Expr<'ast>> + DoubleEndedIterator + Clone {
        self.kind.exprs()
    }

    /// Returns an iterator over the expressions.
    pub fn exprs_mut(
        &mut self,
    ) -> impl ExactSizeIterator<Item = &mut Box<'ast, Expr<'ast>>> + DoubleEndedIterator {
        self.kind.exprs_mut()
    }
}

/// A list of function call argument expressions.
#[derive(Debug)]
pub enum CallArgsKind<'ast> {
    /// A list of unnamed arguments: `(1, 2, 3)`.
    Unnamed(Box<'ast, [Box<'ast, Expr<'ast>>]>),

    /// A list of named arguments: `({x: 1, y: 2, z: 3})`.
    Named(NamedArgList<'ast>),
}

impl Default for CallArgsKind<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

impl<'ast> CallArgsKind<'ast> {
    /// Creates a new empty list of unnamed arguments.
    pub fn empty() -> Self {
        Self::Unnamed(Box::default())
    }

    /// Returns the length of the arguments.
    pub fn len(&self) -> usize {
        match self {
            Self::Unnamed(exprs) => exprs.len(),
            Self::Named(args) => args.len(),
        }
    }

    /// Returns `true` if the list of arguments is empty.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns an iterator over the expressions.
    pub fn exprs(
        &self,
    ) -> impl ExactSizeIterator<Item = &Expr<'ast>> + DoubleEndedIterator + Clone {
        match self {
            Self::Unnamed(exprs) => Either::Left(exprs.iter().map(|expr| &**expr)),
            Self::Named(args) => Either::Right(args.iter().map(|arg| &*arg.value)),
        }
    }

    /// Returns an iterator over the expressions.
    pub fn exprs_mut(
        &mut self,
    ) -> impl ExactSizeIterator<Item = &mut Box<'ast, Expr<'ast>>> + DoubleEndedIterator {
        match self {
            Self::Unnamed(exprs) => Either::Left(exprs.iter_mut()),
            Self::Named(args) => Either::Right(args.iter_mut().map(|arg| &mut arg.value)),
        }
    }

    /// Returns the span of the argument expressions. Does not include the parentheses.
    pub fn span(&self) -> Option<Span> {
        if self.is_empty() {
            return None;
        }
        Some(Span::join_first_last(self.exprs().map(|e| e.span)))
    }
}

/// A named argument: `name: value`.
#[derive(Debug)]
pub struct NamedArg<'ast> {
    pub name: Ident,
    pub value: Box<'ast, Expr<'ast>>,
}

/// A kind of square bracketed indexing expression: `vector[index]`, `slice[l:r]`.
#[derive(Debug)]
pub enum IndexKind<'ast> {
    /// A single index: `vector[index]`.
    Index(Option<Box<'ast, Expr<'ast>>>),

    /// A slice: `slice[l:r]`.
    Range(Option<Box<'ast, Expr<'ast>>>, Option<Box<'ast, Expr<'ast>>>),
}

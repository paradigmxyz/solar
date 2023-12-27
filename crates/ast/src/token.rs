//! Solidity source code token.

use std::{borrow::Cow, fmt};
use sulk_interface::{Ident, Span, Symbol};

/// The type of a comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommentKind {
    /// `// ...`
    Line,
    /// `/* ... */`
    Block,
}

/// A **bin**ary **op**eration token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOpToken {
    /// `+`
    Plus,
    /// `-`
    Minus,
    /// `*`
    Star,
    /// `/`
    Slash,
    /// `%`
    Percent,
    /// `^`
    Caret,
    /// `&`
    And,
    /// `|`
    Or,
    /// `<<`
    Shl,
    /// `>>`
    Shr,
}

impl fmt::Display for BinOpToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl BinOpToken {
    /// Returns the string representation of the binary operator token.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Plus => "+",
            Self::Minus => "-",
            Self::Star => "*",
            Self::Slash => "/",
            Self::Percent => "%",
            Self::Caret => "^",
            Self::And => "&",
            Self::Or => "|",
            Self::Shl => "<<",
            Self::Shr => ">>",
        }
    }

    /// Returns the string representation of the binary operator token followed by an equals sign
    /// (`=`).
    pub const fn to_str_with_eq(self) -> &'static str {
        match self {
            Self::Plus => "+=",
            Self::Minus => "-=",
            Self::Star => "*=",
            Self::Slash => "/=",
            Self::Percent => "%=",
            Self::Caret => "^=",
            Self::And => "&=",
            Self::Or => "|=",
            Self::Shl => "<<=",
            Self::Shr => ">>=",
        }
    }
}

/// Describes how a sequence of token trees is delimited.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Delimiter {
    /// `( ... )`
    Parenthesis,
    /// `{ ... }`
    Brace,
    /// `[ ... ]`
    Bracket,
}

/// A literal token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lit {
    /// The literal kind.
    pub kind: LitKind,
    /// The symbol of the literal token, excluding any quotes.
    pub symbol: Symbol,
}

impl Lit {
    pub fn new(kind: LitKind, symbol: Symbol) -> Self {
        Self { kind, symbol }
    }

    // /// Returns `true` if this is semantically a float literal. This includes
    // /// ones like `1f32` that have an `Integer` kind but a float suffix.
    // pub fn is_semantic_float(&self) -> bool {
    //     match self.kind {
    //         LitKind::Rational => true,
    //         LitKind::Integer => match self.suffix {
    //             Some(sym) => sym == sym::f32 || sym == sym::f64,
    //             None => false,
    //         },
    //         _ => false,
    //     }
    // }

    // /// Keep this in sync with `Token::can_begin_literal_or_bool` excluding unary negation.
    // pub fn from_token(token: &Token) -> Option<Lit> {
    //     match token.uninterpolate().kind {
    //         Ident(name, false) if name.is_bool_lit() => {
    //             Some(Lit::new(Bool, name, None))
    //         }
    //         Literal(token_lit) => Some(token_lit),
    //         Interpolated(ref nt)
    //             if let NtExpr(expr) | NtLiteral(expr) = &**nt
    //             && let ast::ExprKind::Lit(token_lit) = expr.kind =>
    //         {
    //             Some(token_lit)
    //         }
    //         _ => None,
    //     }
    // }
}

impl fmt::Display for Lit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self { kind, symbol } = self;
        match kind {
            LitKind::Str => write!(f, "\"{symbol}\""),
            LitKind::UnicodeStr => write!(f, "unicode\"{symbol}\""),
            LitKind::HexStr => write!(f, "hex\"{symbol}\""),
            LitKind::Integer | LitKind::Rational | LitKind::Err => {
                write!(f, "{symbol}")
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LitKind {
    Integer,
    Rational,
    Str,
    UnicodeStr,
    HexStr,
    Err,
}

impl LitKind {
    /// An English article for the literal token kind.
    pub fn article(self) -> &'static str {
        match self {
            Self::Integer | Self::Err => "an",
            _ => "a",
        }
    }

    pub fn descr(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Rational => "rational",
            Self::Str => "string",
            Self::UnicodeStr => "unicode string",
            Self::HexStr => "hex string",
            Self::Err => "error",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    // Expression-operator symbols.
    /// `=`
    Eq,
    /// `<`
    Lt,
    /// `<=`
    Le,
    /// `==`
    EqEq,
    /// `!=`
    Ne,
    /// `>=`
    Ge,
    /// `>`
    Gt,
    /// `&&`
    AndAnd,
    /// `||`
    OrOr,
    /// `!`
    Not,
    /// `~`
    Tilde,
    BinOp(BinOpToken),
    BinOpEq(BinOpToken),

    // Structural symbols.
    /// `@`
    At,
    /// `.`
    Dot,
    /// `,`
    Comma,
    /// `;`
    Semi,
    /// `:`
    Colon,
    /// `->`
    Arrow,
    /// `=>`
    FatArrow,
    /// `?`
    Question,
    /// An opening delimiter (e.g., `{`).
    OpenDelim(Delimiter),
    /// A closing delimiter (e.g., `}`).
    CloseDelim(Delimiter),

    // Literals.
    /// A literal token.
    Literal(Lit),

    /// Identifier token.
    Ident(Symbol),

    /// A doc comment token.
    /// `Symbol` is the doc comment's data excluding its "quotes" (`///`, `/**`, etc)
    /// similarly to symbols in string literal tokens.
    DocComment(CommentKind, Symbol),

    /// End of file marker.
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Literal(lit) => lit.fmt(f),
            Self::Ident(ident) => ident.fmt(f),
            _ => f.write_str(&self.as_str()),
        }
    }
}

impl TokenKind {
    /// Returns the string representation of the token kind.
    pub fn as_str(&self) -> Cow<'static, str> {
        match self {
            Self::Eq => "=",
            Self::Lt => "<",
            Self::Le => "<=",
            Self::EqEq => "==",
            Self::Ne => "!=",
            Self::Ge => ">=",
            Self::Gt => ">",
            Self::AndAnd => "&&",
            Self::OrOr => "||",
            Self::Not => "!",
            Self::Tilde => "~",
            Self::BinOp(op) => op.to_str(),
            Self::BinOpEq(op) => op.to_str_with_eq(),

            Self::At => "@",
            Self::Dot => ".",
            Self::Comma => ",",
            Self::Semi => ";",
            Self::Colon => ":",
            Self::Arrow => "->",
            Self::FatArrow => "=>",
            Self::Question => "?",
            Self::OpenDelim(Delimiter::Parenthesis) => "(",
            Self::CloseDelim(Delimiter::Parenthesis) => ")",
            Self::OpenDelim(Delimiter::Brace) => "{",
            Self::CloseDelim(Delimiter::Brace) => "}",
            Self::OpenDelim(Delimiter::Bracket) => "[",
            Self::CloseDelim(Delimiter::Bracket) => "]",

            Self::Literal(lit) => return lit.to_string().into(),
            Self::Ident(ident) => return ident.as_str().to_string().into(),
            Self::DocComment(CommentKind::Block, _symbol) => "<block doc-comment>",
            Self::DocComment(CommentKind::Line, _symbol) => "<line doc-comment>",
            Self::Eof => "<eof>",
        }
        .into()
    }

    /// Creates a new literal token kind.
    pub fn lit(kind: LitKind, symbol: Symbol) -> Self {
        Self::Literal(Lit::new(kind, symbol))
    }

    /// Returns tokens that are likely to be typed accidentally instead of the current token.
    /// Enables better error recovery when the wrong token is found.
    pub fn similar_tokens(&self) -> Option<Vec<Self>> {
        match *self {
            Self::Comma => Some(vec![Self::Dot, Self::Lt, Self::Semi]),
            Self::Semi => Some(vec![Self::Colon, Self::Comma]),
            Self::FatArrow => Some(vec![Self::Eq, Self::Arrow]),
            _ => None,
        }
    }
}

/// A single token.
#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    /// The kind of the token.
    pub kind: TokenKind,
    /// The full span of the token.
    pub span: Span,
}

impl Token {
    /// The [EOF](TokenKind::Eof) token.
    pub const EOF: Self = Self::new(TokenKind::Eof, Span::DUMMY);

    /// A dummy token that will be thrown away later.
    pub const DUMMY: Self = Self::new(TokenKind::Question, Span::DUMMY);

    /// Creates a new token.
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Recovers a `Token` from an `Ident`.
    pub fn from_ast_ident(ident: Ident) -> Self {
        Self::new(TokenKind::Ident(ident.name), ident.span)
    }

    /// Creates a new identifier if the kind is [`TokenKind::Ident`].
    #[inline]
    pub const fn ident(&self) -> Option<Ident> {
        match self.kind {
            TokenKind::Ident(ident) => Some(Ident::new(ident, self.span)),
            _ => None,
        }
    }

    pub fn is_op(&self) -> bool {
        match self.kind {
            TokenKind::Eq
            | TokenKind::Lt
            | TokenKind::Le
            | TokenKind::EqEq
            | TokenKind::Ne
            | TokenKind::Ge
            | TokenKind::Gt
            | TokenKind::AndAnd
            | TokenKind::OrOr
            | TokenKind::Not
            | TokenKind::Tilde
            | TokenKind::BinOp(_)
            | TokenKind::BinOpEq(_)
            | TokenKind::At
            | TokenKind::Dot
            | TokenKind::Comma
            | TokenKind::Semi
            | TokenKind::Colon
            | TokenKind::Arrow
            | TokenKind::FatArrow
            | TokenKind::Question => true,

            TokenKind::OpenDelim(..)
            | TokenKind::CloseDelim(..)
            | TokenKind::Literal(..)
            | TokenKind::DocComment(..)
            | TokenKind::Ident(..)
            | TokenKind::Eof => false,
        }
    }

    pub fn is_like_plus(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::BinOp(BinOpToken::Plus) | TokenKind::BinOpEq(BinOpToken::Plus)
        )
    }
}

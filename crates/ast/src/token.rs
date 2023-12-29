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

/// A binary operation token.
///
/// Note that this enum contains only binary operators that can also be used in assignments.
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
    /// `>>>`
    Sar,
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
            Self::Sar => ">>>",
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
            Self::Sar => ">>>=",
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
    /// Creates a new literal token.
    #[inline]
    pub const fn new(kind: LitKind, symbol: Symbol) -> Self {
        Self { kind, symbol }
    }
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

/// A kind of literal token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LitKind {
    /// An integer literal token.
    Integer,
    /// A rational literal token.
    Rational,
    /// A string literal token.
    Str,
    /// A unicode string literal token.
    UnicodeStr,
    /// A hex string literal token.
    HexStr,
    /// An error occurred while lexing the literal token.
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

/// A kind of token.
#[derive(Clone, Debug, PartialEq, Eq)]
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
    /// `++`
    PlusPlus,
    /// `--`
    MinusMinus,
    /// `**`
    StarStar,
    /// A binary operator token.
    BinOp(BinOpToken),
    /// A binary operator token, followed by an equals sign (`=`).
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
    /// `Symbol` is the doc comment's data excluding its "quotes" (`///`, `/**`)
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
    /// Creates a new literal token kind.
    pub fn lit(kind: LitKind, symbol: Symbol) -> Self {
        Self::Literal(Lit::new(kind, symbol))
    }

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
            Self::PlusPlus => "++",
            Self::MinusMinus => "--",
            Self::StarStar => "**",
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
            Self::Ident(ident) => return ident.to_string().into(),
            Self::DocComment(CommentKind::Block, _symbol) => "<block doc-comment>",
            Self::DocComment(CommentKind::Line, _symbol) => "<line doc-comment>",
            Self::Eof => "<eof>",
        }
        .into()
    }

    /// Returns `true` if the token kind is an operator.
    pub const fn is_op(&self) -> bool {
        use TokenKind::*;
        match self {
            Eq | Lt | Le | EqEq | Ne | Ge | Gt | AndAnd | OrOr | Not | Tilde | PlusPlus
            | MinusMinus | StarStar | BinOp(_) | BinOpEq(_) | At | Dot | Comma | Semi | Colon
            | Arrow | FatArrow | Question => true,

            OpenDelim(..) | CloseDelim(..) | Literal(..) | DocComment(..) | Ident(..) | Eof => {
                false
            }
        }
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

    /// Glues two token kinds together.
    pub const fn glue(&self, other: &Self) -> Option<Self> {
        use BinOpToken::*;
        use TokenKind::*;
        Some(match *self {
            Eq => match other {
                Eq => EqEq,
                Gt => FatArrow,
                _ => return None,
            },
            Lt => match other {
                Eq => Le,
                Lt => BinOp(Shl),
                Le => BinOpEq(Shl),
                _ => return None,
            },
            Gt => match other {
                Eq => Ge,
                Gt => BinOp(Shr),
                Ge => BinOpEq(Shr),
                BinOp(Shr) => BinOp(Sar),
                BinOpEq(Shr) => BinOpEq(Sar),
                _ => return None,
            },
            Not => match other {
                Eq => Ne,
                _ => return None,
            },
            BinOp(op) => match (op, other) {
                (op, Eq) => BinOpEq(op),
                (And, BinOp(And)) => AndAnd,
                (Or, BinOp(Or)) => OrOr,
                (Minus, Gt) => Arrow,
                (Shr, Gt) => BinOp(Sar),
                (Shr, Ge) => BinOpEq(Sar),
                (Plus, BinOp(Plus)) => PlusPlus,
                (Minus, BinOp(Minus)) => MinusMinus,
                (Star, BinOp(Star)) => StarStar,
                _ => return None,
            },

            Le | EqEq | Ne | Ge | AndAnd | OrOr | Tilde | PlusPlus | MinusMinus | StarStar
            | BinOpEq(_) | At | Dot | Comma | Semi | Colon | Arrow | FatArrow | Question
            | OpenDelim(_) | CloseDelim(_) | Literal(_) | Ident(_) | DocComment(..) | Eof => {
                return None
            }
        })
    }
}

/// A single token.
#[derive(Clone, Debug, PartialEq, Eq)]
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

    /// Returns `true` if the token is an operator.
    pub const fn is_op(&self) -> bool {
        self.kind.is_op()
    }

    /// Returns `true` if the token is an identifier.
    #[inline]
    pub const fn is_ident(&self) -> bool {
        matches!(self.kind, TokenKind::Ident(_))
    }

    /// Returns `true` if the token is a given keyword, `kw`.
    pub fn is_keyword(&self, kw: Symbol) -> bool {
        self.is_ident_where(|id| id.name == kw)
    }

    /// Returns `true` if the token is a keyword used in the language.
    pub fn is_used_keyword(&self) -> bool {
        self.is_ident_where(Ident::is_used_keyword)
    }

    /// Returns `true` if the token is a keyword reserved for possible future use.
    pub fn is_unused_keyword(&self) -> bool {
        self.is_ident_where(Ident::is_unused_keyword)
    }

    /// Returns `true` if the token is either a special identifier or a keyword.
    pub fn is_reserved_ident(&self, in_yul: bool) -> bool {
        self.is_ident_where(|i| i.is_reserved(in_yul))
    }

    /// Returns `true` if the token is the identifier `true` or `false`.
    pub fn is_bool_lit(&self) -> bool {
        self.is_ident_where(|id| id.name.is_bool_lit())
    }

    /// Returns `true` if the token is a numeric literal.
    pub fn is_numeric_lit(&self) -> bool {
        matches!(
            self.kind,
            TokenKind::Literal(Lit { kind: LitKind::Integer, .. })
                | TokenKind::Literal(Lit { kind: LitKind::Rational, .. })
        )
    }

    /// Returns `true` if the token is the integer literal.
    pub fn is_integer_lit(&self) -> bool {
        matches!(self.kind, TokenKind::Literal(Lit { kind: LitKind::Integer, .. }))
    }

    /// Returns `true` if the token is an identifier for which `pred` holds.
    pub fn is_ident_where(&self, pred: impl FnOnce(Ident) -> bool) -> bool {
        self.ident().map(pred).unwrap_or(false)
    }

    /// Returns this token's full description: `{self.description()} '{self.kind}'`.
    pub fn full_description(&self) -> String {
        // https://github.com/rust-lang/rust/blob/44bf2a32a52467c45582c3355a893400e620d010/compiler/rustc_parse/src/parser/mod.rs#L378
        if let Some(description) = self.description() {
            format!("{description} `{}`", self.kind)
        } else {
            format!("`{}`", self.kind)
        }
    }

    /// Returns this token's description, if any.
    pub fn description(&self) -> Option<TokenDescription> {
        TokenDescription::from_token(self)
    }

    /// Glues two tokens together.
    pub fn glue(&self, other: &Self) -> Option<Self> {
        self.kind.glue(&other.kind).map(|kind| Self::new(kind, self.span.to(other.span)))
    }
}

/// A description of a token.
///
/// Precedes the token string in error messages like `keyword 'for'` in `expected identifier, got
/// keyword 'for'`. See [`full_description`](Token::full_description).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenDescription {
    // /// A reserved identifier.
    // ReservedIdentifier,
    /// A keyword.
    Keyword,
    /// A reserved keyword.
    ReservedKeyword,
    /// A doc comment.
    DocComment,
}

impl fmt::Display for TokenDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl TokenDescription {
    /// Returns the description of the given token.
    pub fn from_token(token: &Token) -> Option<Self> {
        match token.kind {
            // _ if token.is_special_ident() => Some(TokenDescription::ReservedIdentifier),
            _ if token.is_used_keyword() => Some(Self::Keyword),
            _ if token.is_unused_keyword() => Some(Self::ReservedKeyword),
            TokenKind::DocComment(..) => Some(Self::DocComment),
            _ => None,
        }
    }

    /// Returns the string representation of the token description.
    pub const fn to_str(self) -> &'static str {
        match self {
            // Self::ReservedIdentifier => "reserved identifier",
            Self::Keyword => "keyword",
            Self::ReservedKeyword => "reserved keyword",
            Self::DocComment => "doc comment",
        }
    }
}

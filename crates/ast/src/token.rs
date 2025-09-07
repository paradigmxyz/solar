//! Solidity source code token.

use crate::{
    DocComment, StrKind,
    ast::{BinOp, BinOpKind, UnOp, UnOpKind},
};
use solar_interface::{Ident, Span, Symbol, diagnostics::ErrorGuaranteed};
use std::{borrow::Cow, fmt};

/// The type of a comment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommentKind {
    /// `// ...`, `/// ...`
    Line,
    /// `/* ... */`, `/** ... */`
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

    /// Returns the binary operator kind.
    #[inline]
    pub const fn as_binop(self) -> BinOpKind {
        match self {
            Self::Plus => BinOpKind::Add,
            Self::Minus => BinOpKind::Sub,
            Self::Star => BinOpKind::Mul,
            Self::Slash => BinOpKind::Div,
            Self::Percent => BinOpKind::Rem,
            Self::Caret => BinOpKind::BitXor,
            Self::And => BinOpKind::BitAnd,
            Self::Or => BinOpKind::BitOr,
            Self::Shl => BinOpKind::Shl,
            Self::Shr => BinOpKind::Shr,
            Self::Sar => BinOpKind::Sar,
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

impl Delimiter {
    /// Returns the string representation of the opening delimiter.
    pub const fn to_open_str(self) -> &'static str {
        match self {
            Self::Parenthesis => "(",
            Self::Brace => "{",
            Self::Bracket => "[",
        }
    }

    /// Returns the string representation of the closing delimiter.
    pub const fn to_close_str(self) -> &'static str {
        match self {
            Self::Parenthesis => ")",
            Self::Brace => "}",
            Self::Bracket => "]",
        }
    }
}

/// A literal token. Different from an AST literal as this is unparsed and only contains the raw
/// contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TokenLit {
    /// The symbol of the literal token, excluding any quotes.
    pub symbol: Symbol,
    /// The literal kind.
    pub kind: TokenLitKind,
}

impl fmt::Display for TokenLit {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let &Self { kind, symbol } = self;
        match kind {
            TokenLitKind::Str => write!(f, "\"{symbol}\""),
            TokenLitKind::UnicodeStr => write!(f, "unicode\"{symbol}\""),
            TokenLitKind::HexStr => write!(f, "hex\"{symbol}\""),
            TokenLitKind::Integer | TokenLitKind::Rational | TokenLitKind::Err(_) => {
                write!(f, "{symbol}")
            }
        }
    }
}

impl TokenLit {
    /// Creates a new literal token.
    #[inline]
    pub const fn new(kind: TokenLitKind, symbol: Symbol) -> Self {
        Self { kind, symbol }
    }

    /// Returns a description of the literal.
    pub const fn description(self) -> &'static str {
        self.kind.description()
    }
}

/// A kind of literal token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum TokenLitKind {
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
    Err(ErrorGuaranteed),
}

impl From<StrKind> for TokenLitKind {
    fn from(str_kind: StrKind) -> Self {
        match str_kind {
            StrKind::Str => Self::Str,
            StrKind::Unicode => Self::UnicodeStr,
            StrKind::Hex => Self::HexStr,
        }
    }
}

impl TokenLitKind {
    /// Returns the description of the literal kind.
    pub const fn description(self) -> &'static str {
        match self {
            Self::Integer => "integer",
            Self::Rational => "rational",
            Self::Str => "string",
            Self::UnicodeStr => "unicode string",
            Self::HexStr => "hex string",
            Self::Err(_) => "error",
        }
    }
}

/// A kind of token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
    /// `:=`
    Walrus,
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
    ///
    /// Note that this does not include boolean literals.
    ///
    /// `Symbol` is the literal's parsed data. In string literals, this is the unescaped value, and
    /// excludes its quotes (`"`, `'`) and prefix (`hex`, `unicode`).
    Literal(TokenLitKind, Symbol),

    /// Identifier token.
    Ident(Symbol),

    /// A comment or doc-comment token.
    ///
    /// `Symbol` is the comment's data excluding its "quotes" (`//`, `/**`)
    /// similarly to symbols in string literal tokens.
    Comment(bool /* is_doc */, CommentKind, Symbol),

    /// End of file marker.
    Eof,
}

impl fmt::Display for TokenKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.description())
    }
}

impl TokenKind {
    /// Creates a new literal token kind.
    pub fn lit(kind: TokenLitKind, symbol: Symbol) -> Self {
        Self::Literal(kind, symbol)
    }

    /// Returns the string representation of the token kind.
    pub fn as_str(&self) -> &str {
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
            Self::Walrus => ":=",
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
            Self::OpenDelim(d) => d.to_open_str(),
            Self::CloseDelim(d) => d.to_close_str(),

            Self::Literal(.., symbol) | Self::Ident(.., symbol) | Self::Comment(.., symbol) => {
                symbol.as_str()
            }

            Self::Eof => "<eof>",
        }
    }

    /// Returns the description of the token kind.
    pub fn description(&self) -> Cow<'_, str> {
        match self {
            Self::Literal(kind, _) => return format!("<{}>", kind.description()).into(),
            Self::Ident(symbol) => return symbol.to_string().into(),
            Self::Comment(false, CommentKind::Block, _) => "<block comment>",
            Self::Comment(true, CommentKind::Block, _) => "<block doc-comment>",
            Self::Comment(false, CommentKind::Line, _) => "<line comment>",
            Self::Comment(true, CommentKind::Line, _) => "<line doc-comment>",
            _ => self.as_str(),
        }
        .into()
    }

    /// Returns `true` if the token kind is an operator.
    pub const fn is_op(&self) -> bool {
        use TokenKind::*;
        match self {
            Eq | Lt | Le | EqEq | Ne | Ge | Gt | AndAnd | OrOr | Not | Tilde | Walrus
            | PlusPlus | MinusMinus | StarStar | BinOp(_) | BinOpEq(_) | At | Dot | Comma
            | Colon | Arrow | FatArrow | Question => true,

            OpenDelim(..) | CloseDelim(..) | Literal(..) | Comment(..) | Ident(..) | Semi | Eof => {
                false
            }
        }
    }

    /// Returns the token kind as a unary operator, if any.
    pub fn as_unop(&self, is_postfix: bool) -> Option<UnOpKind> {
        let kind = if is_postfix {
            match self {
                Self::PlusPlus => UnOpKind::PostInc,
                Self::MinusMinus => UnOpKind::PostDec,
                _ => return None,
            }
        } else {
            match self {
                Self::PlusPlus => UnOpKind::PreInc,
                Self::MinusMinus => UnOpKind::PreDec,
                Self::Not => UnOpKind::Not,
                Self::Tilde => UnOpKind::BitNot,
                Self::BinOp(BinOpToken::Minus) => UnOpKind::Neg,
                _ => return None,
            }
        };
        debug_assert_eq!(kind.is_postfix(), is_postfix);
        Some(kind)
    }

    /// Returns the token kind as a binary operator, if any.
    #[inline]
    pub fn as_binop(&self) -> Option<BinOpKind> {
        match self {
            Self::Eq => Some(BinOpKind::Eq),
            Self::Lt => Some(BinOpKind::Lt),
            Self::Le => Some(BinOpKind::Le),
            Self::EqEq => Some(BinOpKind::Eq),
            Self::Ne => Some(BinOpKind::Ne),
            Self::Ge => Some(BinOpKind::Ge),
            Self::Gt => Some(BinOpKind::Gt),
            Self::AndAnd => Some(BinOpKind::And),
            Self::OrOr => Some(BinOpKind::Or),
            Self::StarStar => Some(BinOpKind::Pow),
            Self::BinOp(op) => Some(op.as_binop()),
            _ => None,
        }
    }

    /// Returns the token kind as a binary operator, if any.
    #[inline]
    pub fn as_binop_eq(&self) -> Option<BinOpKind> {
        match self {
            Self::BinOpEq(op) => Some(op.as_binop()),
            _ => None,
        }
    }

    /// Returns `true` if the token kind is a normal comment (not a doc-comment).
    #[inline]
    pub const fn is_comment(&self) -> bool {
        matches!(self, Self::Comment(false, ..))
    }

    /// Returns `true` if the token kind is a comment or doc-comment.
    #[inline]
    pub const fn is_comment_or_doc(&self) -> bool {
        matches!(self, Self::Comment(..))
    }
}

/// A single token.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    /// The kind of the token.
    pub kind: TokenKind,
    /// The full span of the token.
    pub span: Span,
}

impl From<Ident> for Token {
    #[inline]
    fn from(ident: Ident) -> Self {
        Self::from_ast_ident(ident)
    }
}

impl Token {
    /// The [EOF](TokenKind::Eof) token.
    pub const EOF: Self = Self::new(TokenKind::Eof, Span::DUMMY);

    /// A dummy token that will be thrown away later.
    pub const DUMMY: Self = Self::new(TokenKind::Question, Span::DUMMY);

    /// Creates a new token.
    #[inline]
    pub const fn new(kind: TokenKind, span: Span) -> Self {
        Self { kind, span }
    }

    /// Recovers a `Token` from an `Ident`.
    #[inline]
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

    /// Returns the literal if the kind is [`TokenKind::Literal`].
    #[inline]
    pub const fn lit(&self) -> Option<TokenLit> {
        match self.kind {
            TokenKind::Literal(kind, symbol) => Some(TokenLit::new(kind, symbol)),
            _ => None,
        }
    }

    /// Returns this token's literal kind, if any.
    #[inline]
    pub const fn lit_kind(&self) -> Option<TokenLitKind> {
        match self.kind {
            TokenKind::Literal(kind, _) => Some(kind),
            _ => None,
        }
    }

    /// Returns the comment if the kind is [`TokenKind::Comment`], and whether it's a doc-comment.
    #[inline]
    pub const fn comment(&self) -> Option<(bool, DocComment)> {
        match self.kind {
            TokenKind::Comment(is_doc, kind, symbol) => {
                Some((is_doc, DocComment { span: self.span, kind, symbol }))
            }
            _ => None,
        }
    }

    /// Returns the comment if the kind is [`TokenKind::Comment`].
    ///
    /// Does not check that `is_doc` is `true`.
    #[inline]
    pub const fn doc(&self) -> Option<DocComment> {
        match self.kind {
            TokenKind::Comment(_, kind, symbol) => {
                Some(DocComment { span: self.span, kind, symbol })
            }
            _ => None,
        }
    }

    /// Returns `true` if the token is an operator.
    #[inline]
    pub const fn is_op(&self) -> bool {
        self.kind.is_op()
    }

    /// Returns the token as a unary operator, if any.
    #[inline]
    pub fn as_unop(&self, is_postfix: bool) -> Option<UnOp> {
        self.kind.as_unop(is_postfix).map(|kind| UnOp { span: self.span, kind })
    }

    /// Returns the token as a binary operator, if any.
    #[inline]
    pub fn as_binop(&self) -> Option<BinOp> {
        self.kind.as_binop().map(|kind| BinOp { span: self.span, kind })
    }

    /// Returns the token as a binary operator, if any.
    #[inline]
    pub fn as_binop_eq(&self) -> Option<BinOp> {
        self.kind.as_binop_eq().map(|kind| BinOp { span: self.span, kind })
    }

    /// Returns `true` if the token is an identifier.
    #[inline]
    pub const fn is_ident(&self) -> bool {
        matches!(self.kind, TokenKind::Ident(_))
    }

    /// Returns `true` if the token is a literal. Includes `bool` literals.
    #[inline]
    pub fn is_lit(&self) -> bool {
        matches!(self.kind, TokenKind::Literal(..)) || self.is_bool_lit()
    }

    /// Returns `true` if the token is a given keyword, `kw`.
    #[inline]
    pub fn is_keyword(&self, kw: Symbol) -> bool {
        self.is_ident_where(|id| id.name == kw)
    }

    /// Returns `true` if the token is any of the given keywords.
    #[inline]
    pub fn is_keyword_any(&self, kws: &[Symbol]) -> bool {
        self.is_ident_where(|id| kws.contains(&id.name))
    }

    /// Returns `true` if the token is a keyword used in the language.
    #[inline]
    pub fn is_used_keyword(&self) -> bool {
        self.is_ident_where(Ident::is_used_keyword)
    }

    /// Returns `true` if the token is a keyword reserved for possible future use.
    #[inline]
    pub fn is_unused_keyword(&self) -> bool {
        self.is_ident_where(Ident::is_unused_keyword)
    }

    /// Returns `true` if the token is a keyword.
    #[inline]
    pub fn is_reserved_ident(&self, yul: bool) -> bool {
        self.is_ident_where(|i| i.is_reserved(yul))
    }

    /// Returns `true` if the token is an identifier, but not a keyword.
    #[inline]
    pub fn is_non_reserved_ident(&self, yul: bool) -> bool {
        self.is_ident_where(|i| i.is_non_reserved(yul))
    }

    /// Returns `true` if the token is an elementary type name.
    ///
    /// Note that this does not include `[u]fixedMxN` types.
    #[inline]
    pub fn is_elementary_type(&self) -> bool {
        self.is_ident_where(Ident::is_elementary_type)
    }

    /// Returns `true` if the token is the identifier `true` or `false`.
    #[inline]
    pub fn is_bool_lit(&self) -> bool {
        self.is_ident_where(|id| id.name.is_bool_lit())
    }

    /// Returns `true` if the token is a numeric literal.
    #[inline]
    pub fn is_numeric_lit(&self) -> bool {
        matches!(self.kind, TokenKind::Literal(TokenLitKind::Integer | TokenLitKind::Rational, _))
    }

    /// Returns `true` if the token is the integer literal.
    #[inline]
    pub fn is_integer_lit(&self) -> bool {
        matches!(self.kind, TokenKind::Literal(TokenLitKind::Integer, _))
    }

    /// Returns `true` if the token is the rational literal.
    #[inline]
    pub fn is_rational_lit(&self) -> bool {
        matches!(self.kind, TokenKind::Literal(TokenLitKind::Rational, _))
    }

    /// Returns `true` if the token is a string literal.
    #[inline]
    pub fn is_str_lit(&self) -> bool {
        matches!(self.kind, TokenKind::Literal(TokenLitKind::Str, _))
    }

    /// Returns `true` if the token is an identifier for which `pred` holds.
    #[inline]
    pub fn is_ident_where(&self, pred: impl FnOnce(Ident) -> bool) -> bool {
        self.ident().map(pred).unwrap_or(false)
    }

    /// Returns `true` if the token is an end-of-file marker.
    #[inline]
    pub const fn is_eof(&self) -> bool {
        matches!(self.kind, TokenKind::Eof)
    }

    /// Returns `true` if the token is the given open delimiter.
    #[inline]
    pub fn is_open_delim(&self, d: Delimiter) -> bool {
        self.kind == TokenKind::OpenDelim(d)
    }

    /// Returns `true` if the token is the given close delimiter.
    #[inline]
    pub fn is_close_delim(&self, d: Delimiter) -> bool {
        self.kind == TokenKind::CloseDelim(d)
    }

    /// Returns `true` if the token is a normal comment (not a doc-comment).
    #[inline]
    pub const fn is_comment(&self) -> bool {
        self.kind.is_comment()
    }

    /// Returns `true` if the token is a comment or doc-comment.
    #[inline]
    pub const fn is_comment_or_doc(&self) -> bool {
        self.kind.is_comment_or_doc()
    }

    /// Returns `true` if the token is a location specifier.
    #[inline]
    pub fn is_location_specifier(&self) -> bool {
        self.is_ident_where(Ident::is_location_specifier)
    }

    /// Returns `true` if the token is a mutability specifier.
    #[inline]
    pub fn is_mutability_specifier(&self) -> bool {
        self.is_ident_where(Ident::is_mutability_specifier)
    }

    /// Returns `true` if the token is a visibility specifier.
    #[inline]
    pub fn is_visibility_specifier(&self) -> bool {
        self.is_ident_where(Ident::is_visibility_specifier)
    }

    /// Returns this token's full description: `{self.description()} '{self.kind}'`.
    pub fn full_description(&self) -> impl fmt::Display + '_ {
        // https://github.com/rust-lang/rust/blob/44bf2a32a52467c45582c3355a893400e620d010/compiler/rustc_parse/src/parser/mod.rs#L378
        if let Some(description) = self.description() {
            format!("{description} `{}`", self.kind)
        } else {
            format!("`{}`", self.kind)
        }
    }

    /// Returns the string representation of the token.
    pub fn as_str(&self) -> &str {
        self.kind.as_str()
    }

    /// Returns this token's description, if any.
    #[inline]
    pub fn description(self) -> Option<TokenDescription> {
        TokenDescription::from_token(self)
    }
}

/// A description of a token.
///
/// Precedes the token string in error messages like `keyword 'for'` in `expected identifier, found
/// keyword 'for'`. See [`full_description`](Token::full_description).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TokenDescription {
    /// A keyword.
    Keyword,
    /// A reserved keyword.
    ReservedKeyword,
    /// A Yul keyword.
    YulKeyword,
    /// A Yul EVM builtin.
    YulEvmBuiltin,
}

impl fmt::Display for TokenDescription {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.to_str())
    }
}

impl TokenDescription {
    /// Returns the description of the given token.
    pub fn from_token(token: Token) -> Option<Self> {
        match token.kind {
            _ if token.is_used_keyword() => Some(Self::Keyword),
            _ if token.is_unused_keyword() => Some(Self::ReservedKeyword),
            _ if token.is_ident_where(|id| id.is_yul_keyword()) => Some(Self::YulKeyword),
            _ if token.is_ident_where(|id| id.is_yul_evm_builtin()) => Some(Self::YulEvmBuiltin),
            _ => None,
        }
    }

    /// Returns the string representation of the token description.
    pub const fn to_str(self) -> &'static str {
        match self {
            Self::Keyword => "keyword",
            Self::ReservedKeyword => "reserved keyword",
            Self::YulKeyword => "Yul keyword",
            Self::YulEvmBuiltin => "Yul EVM builtin keyword",
        }
    }
}

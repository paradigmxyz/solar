use rsolc_span::{Ident, Span, Symbol};
use std::fmt;

pub use BinOpToken::*;
pub use LitKind::*;
pub use TokenKind::*;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CommentKind {
    Line,
    Block,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BinOpToken {
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Caret,
    And,
    Or,
    Shl,
    Shr,
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
    // /// `Ø ... Ø`
    // /// An invisible delimiter, that may, for example, appear around tokens coming from a
    // /// "macro variable" `$var`. It is important to preserve operator priorities in cases like
    // /// `$var * 3` where `$var` is `1 + 2`.
    // /// Invisible delimiters might not survive roundtrip of a token stream through a string.
    // Invisible,
}

// Note that the suffix is *not* considered when deciding the `LitKind` in this
// type. This means that float literals like `1f32` are classified by this type
// as `Int`. Only upon conversion to `ast::LitKind` will such a literal be
// given the `Float` kind.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LitKind {
    Bool, // AST only, must never appear in a `Token`
    Integer,
    Rational,
    // TODO: Enum with 2 variants?
    Str(/* unicode */ bool),
    HexStr,
    Err,
}

/// A literal token.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Lit {
    pub kind: LitKind,
    pub symbol: Symbol,
}

impl Lit {
    pub fn new(kind: LitKind, symbol: Symbol) -> Lit {
        Lit { kind, symbol }
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
        let Lit { kind, symbol } = self;
        match kind {
            Str(false) => write!(f, "\"{symbol}\""),
            Str(true) => write!(f, "unicode\"{symbol}\""),
            HexStr => write!(f, "hex\"{symbol}\""),
            Integer | Rational | Bool | Err => write!(f, "{symbol}"),
        }
    }
}

impl LitKind {
    /// An English article for the literal token kind.
    pub fn article(self) -> &'static str {
        match self {
            Integer | Err => "an",
            _ => "a",
        }
    }

    pub fn descr(self) -> &'static str {
        match self {
            Bool => panic!("literal token contains `Lit::Bool`"),
            Integer => "integer",
            Rational => "rational",
            Str(_) => "string",
            HexStr => "hex string",
            Err => "error",
        }
    }

    // pub(crate) fn may_have_suffix(self) -> bool {
    //     matches!(self, Integer | Float | Err)
    // }
}

/*
pub fn ident_can_begin_expr(name: Symbol, span: Span, is_raw: bool) -> bool {
    let ident_token = Token::new(Ident(name, is_raw), span);

    !ident_token.is_reserved_ident()
        || ident_token.is_path_segment_keyword()
        || [
            kw::Async,
            kw::Do,
            kw::Box,
            kw::Break,
            kw::Const,
            kw::Continue,
            kw::False,
            kw::For,
            kw::If,
            kw::Let,
            kw::Loop,
            kw::Match,
            kw::Move,
            kw::Return,
            kw::True,
            kw::Try,
            kw::Unsafe,
            kw::While,
            kw::Yield,
            kw::Static,
        ]
        .contains(&name)
}

fn ident_can_begin_type(name: Symbol, span: Span, is_raw: bool) -> bool {
    let ident_token = Token::new(Ident(name, is_raw), span);

    !ident_token.is_reserved_ident()
        || ident_token.is_path_segment_keyword()
        || [kw::Underscore, kw::For, kw::Impl, kw::Fn, kw::Unsafe, kw::Extern, kw::Typeof, kw::Dyn]
            .contains(&name)
}
*/

#[derive(Clone, Debug, PartialEq)]
pub enum TokenKind {
    // Expression-operator symbols.
    Eq,
    Lt,
    Le,
    EqEq,
    Ne,
    Ge,
    Gt,
    AndAnd,
    OrOr,
    Not,
    Tilde,
    BinOp(BinOpToken),
    BinOpEq(BinOpToken),

    // Structural symbols
    At,
    Dot,
    // DotDot,
    // DotDotDot,
    // DotDotEq,
    Comma,
    Semi,
    Colon,
    // ModSep,
    // RArrow,
    // LArrow,
    // Arrow,
    FatArrow,
    // Pound,
    // Dollar,
    Question,
    /// An opening delimiter (e.g., `{`).
    OpenDelim(Delimiter),
    /// A closing delimiter (e.g., `}`).
    CloseDelim(Delimiter),

    // Literals
    Literal(Lit),

    /// Identifier token.
    /// Do not forget about `NtIdent` when you want to match on identifiers.
    /// It's recommended to use `Token::(ident,uninterpolate,uninterpolated_span)` to
    /// treat regular and interpolated identifiers in the same way.
    Ident(Symbol),
    /// A doc comment token.
    /// `Symbol` is the doc comment's data excluding its "quotes" (`///`, `//*`, etc)
    /// similarly to symbols in string literal tokens.
    DocComment(CommentKind, Symbol),

    Eof,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Token {
    pub kind: TokenKind,
    pub span: Span,
}

impl TokenKind {
    pub fn lit(kind: LitKind, symbol: Symbol) -> TokenKind {
        Literal(Lit::new(kind, symbol))
    }

    /// An approximation to proc-macro-style single-character operators used by rustc parser.
    /// If the operator token can be broken into two tokens, the first of which is single-character,
    /// then this function performs that operation, otherwise it returns `None`.
    pub fn break_two_token_op(&self) -> Option<(TokenKind, TokenKind)> {
        Some(match *self {
            Le => (Lt, Eq),
            EqEq => (Eq, Eq),
            Ne => (Not, Eq),
            Ge => (Gt, Eq),
            AndAnd => (BinOp(And), BinOp(And)),
            OrOr => (BinOp(Or), BinOp(Or)),
            BinOp(Shl) => (Lt, Lt),
            BinOp(Shr) => (Gt, Gt),
            BinOpEq(Plus) => (BinOp(Plus), Eq),
            BinOpEq(Minus) => (BinOp(Minus), Eq),
            BinOpEq(Star) => (BinOp(Star), Eq),
            BinOpEq(Slash) => (BinOp(Slash), Eq),
            BinOpEq(Percent) => (BinOp(Percent), Eq),
            BinOpEq(Caret) => (BinOp(Caret), Eq),
            BinOpEq(And) => (BinOp(And), Eq),
            BinOpEq(Or) => (BinOp(Or), Eq),
            BinOpEq(Shl) => (Lt, Le),
            BinOpEq(Shr) => (Gt, Ge),
            // DotDot => (Dot, Dot),
            // DotDotDot => (Dot, DotDot),
            // ModSep => (Colon, Colon),
            // RArrow => (BinOp(Minus), Gt),
            // LArrow => (Lt, BinOp(Minus)),
            FatArrow => (Eq, Gt),
            _ => return None,
        })
    }

    /// Returns tokens that are likely to be typed accidentally instead of the current token.
    /// Enables better error recovery when the wrong token is found.
    pub fn similar_tokens(&self) -> Option<Vec<TokenKind>> {
        match *self {
            Comma => Some(vec![Dot, Lt, Semi]),
            Semi => Some(vec![Colon, Comma]),
            // FatArrow => Some(vec![Eq, RArrow]),
            _ => None,
        }
    }
}

impl Token {
    pub fn new(kind: TokenKind, span: Span) -> Self {
        Token { kind, span }
    }

    /// Some token that will be thrown away later.
    pub fn dummy() -> Self {
        Token::new(TokenKind::Question, Span::DUMMY)
    }

    /// Recovers a `Token` from an `Ident`. This creates a raw identifier if necessary.
    pub fn from_ast_ident(ident: Ident) -> Self {
        Token::new(Ident(ident.name), ident.span)
    }

    // /// For interpolated tokens, returns a span of the fragment to which the interpolated
    // /// token refers. For all other tokens this is just a regular span.
    // /// It is particularly important to use this for identifiers and lifetimes
    // /// for which spans affect name resolution and edition checks.
    // /// Note that keywords are also identifiers, so they should use this
    // /// if they keep spans or perform edition checks.
    // pub fn uninterpolated_span(&self) -> Span {
    //     match &self.kind {
    //         Interpolated(nt) => nt.span(),
    //         _ => self.span,
    //     }
    // }

    pub fn is_op(&self) -> bool {
        match self.kind {
            Eq | Lt | Le | EqEq | Ne | Ge | Gt | AndAnd | OrOr | Not | Tilde | BinOp(_)
            | BinOpEq(_) | At | Dot | Comma | Semi | Colon | FatArrow | Question => true,

            OpenDelim(..) | CloseDelim(..) | Literal(..) | DocComment(..) | Ident(..) | Eof => {
                false
            }
        }
    }

    pub fn is_like_plus(&self) -> bool {
        matches!(self.kind, BinOp(Plus) | BinOpEq(Plus))
    }
}

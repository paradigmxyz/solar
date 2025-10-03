//! Raw, low-level tokens. Created using [`Cursor`](crate::Cursor).

use solar_ast::{
    Base, StrKind,
    token::{BinOpToken, Delimiter},
};

/// A raw token.
///
/// It doesn't contain information about data that has been parsed, only the type of the token and
/// its size.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RawToken {
    /// The kind of token.
    pub kind: RawTokenKind,
    /// The length of the token in bytes.
    pub len: u32,
}

impl RawToken {
    /// The [`EOF`](RawTokenKind::Eof) token with length 0.
    pub const EOF: Self = Self::new(RawTokenKind::Eof, 0);

    /// Creates a new token.
    #[inline]
    pub const fn new(kind: RawTokenKind, len: u32) -> Self {
        Self { kind, len }
    }
}

/// Common lexeme types.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawTokenKind {
    // Multi-char tokens:
    /// `// comment`
    ///
    /// `/// doc comment`
    LineComment { is_doc: bool },

    /// `/* block comment */`
    ///
    /// `/** block doc comment */`
    BlockComment { is_doc: bool, terminated: bool },

    /// Any whitespace character sequence.
    Whitespace,

    /// `ident` or `continue`
    ///
    /// At this step, keywords are also considered identifiers.
    Ident,

    /// Examples: `123`, `0x123`, `hex"123"`. Note that `_` is an invalid
    /// suffix, but may be present here on string and float literals. Users of
    /// this type will need to check for and reject that case.
    ///
    /// See [`RawLiteralKind`] for more details.
    Literal { kind: RawLiteralKind },

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

    /// Unknown token, not expected by the lexer, e.g. `№`
    Unknown,

    /// End of input.
    Eof,
}

impl RawTokenKind {
    /// Returns `true` if this token is EOF.
    #[inline]
    pub const fn is_eof(&self) -> bool {
        matches!(self, Self::Eof)
    }

    /// Returns `true` if this token is a line comment or a block comment.
    #[inline]
    pub const fn is_comment(&self) -> bool {
        matches!(self, Self::LineComment { .. } | Self::BlockComment { .. })
    }

    /// Returns `true` if this token is a whitespace, line comment, or block comment.
    #[inline]
    pub const fn is_trivial(&self) -> bool {
        matches!(self, Self::Whitespace | Self::LineComment { .. } | Self::BlockComment { .. })
    }
}

/// The literal types supported by the lexer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RawLiteralKind {
    /// `123`, `0x123`; empty_int: `0x`
    Int { base: Base, empty_int: bool },
    /// `123.321`, `1.2e3`, `.2e3`; empty_exponent: `2e`, `2.3e`, `.3e`
    Rational { base: Base, empty_exponent: bool },
    /// `"abc"`, `"abc`; `unicode"abc"`, `unicode"abc`; `hex"abc"`, `hex"abc`
    Str { kind: StrKind, terminated: bool },
}

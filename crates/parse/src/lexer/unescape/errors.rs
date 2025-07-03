use solar_interface::{BytePos, Span, diagnostics::DiagCtxt};
use std::ops::Range;

/// Errors and warnings that can occur during string unescaping.
#[derive(Debug, PartialEq, Eq)]
pub enum EscapeError {
    /// Escaped '\' character without continuation.
    LoneSlash,
    /// Invalid escape character (e.g. '\z').
    InvalidEscape,
    /// Raw '\r' encountered.
    BareCarriageReturn,
    /// Can only skip one line of whitespace.
    ///
    /// ```text
    /// "this is \
    ///  ok" == "this is ok";
    ///
    /// "this is \
    ///  \
    ///  also ok" == "this is also ok";
    ///
    /// "this is \
    ///  
    ///  not ok"; // error: cannot skip multiple lines
    /// ```
    CannotSkipMultipleLines,

    /// Numeric character escape is too short (e.g. '\x1').
    HexEscapeTooShort,
    /// Invalid character in numeric escape (e.g. '\xz1').
    InvalidHexEscape,

    /// Unicode character escape is too short (e.g. '\u1').
    UnicodeEscapeTooShort,
    /// Invalid character in unicode character escape (e.g. '\uz111').
    InvalidUnicodeEscape,

    /// Newline in string literal. These must be escaped.
    StrNewline,
    /// Non-ASCII character in non-unicode literal.
    StrNonAsciiChar,

    /// Non hex-digit character in hex literal.
    HexNotHexDigit,
    /// Underscore in hex literal.
    HexBadUnderscore,
    /// Odd number of hex digits in hex literal.
    HexOddDigits,
    /// Hex literal with the `0x` prefix.
    HexPrefix,
}

impl EscapeError {
    fn msg(&self) -> &'static str {
        match self {
            Self::LoneSlash => "invalid trailing slash in literal",
            Self::InvalidEscape => "unknown character escape",
            Self::BareCarriageReturn => "bare CR not allowed in string, use `\\r` instead",
            Self::CannotSkipMultipleLines => "cannot skip multiple lines with `\\`",
            Self::HexEscapeTooShort => "hex escape must be followed by 2 hex digits",
            Self::InvalidHexEscape => "invalid character in hex escape",
            Self::UnicodeEscapeTooShort => "unicode escape must be followed by 4 hex digits",
            Self::InvalidUnicodeEscape => "invalid character in unicode escape",
            Self::StrNewline => "unescaped newline",
            Self::StrNonAsciiChar => {
                "unicode characters are not allowed in string literals; use a `unicode\"...\"` literal instead"
            }
            Self::HexNotHexDigit => "invalid hex digit",
            Self::HexBadUnderscore => "invalid underscore in hex literal",
            Self::HexOddDigits => "odd number of hex nibbles",
            Self::HexPrefix => "hex prefix is not allowed",
        }
    }
}

pub(crate) fn emit_unescape_error(
    dcx: &DiagCtxt,
    // interior part of the literal, between quotes
    lit: &str,
    // span of the error part of the literal
    err_span: Span,
    // range of the error inside `lit`
    range: Range<usize>,
    error: EscapeError,
) {
    let last_char = || {
        let c = lit[range.clone()].chars().next_back().unwrap();
        let span = err_span.with_lo(err_span.hi() - BytePos(c.len_utf8() as u32));
        (c, span)
    };
    let mut diag = dcx.err(error.msg()).span(err_span);
    if matches!(
        error,
        EscapeError::InvalidEscape
            | EscapeError::InvalidHexEscape
            | EscapeError::InvalidUnicodeEscape
    ) {
        diag = diag.span(last_char().1);
    }
    diag.emit();
}

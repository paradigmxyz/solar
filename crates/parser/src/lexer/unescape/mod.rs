//! Utilities for validating string and char literals and turning them into
//! values they represent.

use std::{ops::Range, str::Chars};

#[cfg(test)]
mod tests;

/// Errors and warnings that can occur during string unescaping.
#[derive(Debug, PartialEq, Eq)]
pub enum EscapeError {
    // /// Expected 1 char, but 0 were found.
    // ZeroChars,
    // /// Expected 1 char, but more than 1 were found.
    // MoreThanOneChar,
    /// Escaped '\' character without continuation.
    LoneSlash,
    /// Invalid escape character (e.g. '\z').
    InvalidEscape,
    /// Raw '\r' encountered.
    BareCarriageReturn,
    /// Unescaped character that was expected to be escaped (e.g. raw '\t').
    EscapeOnlyChar,

    /// Numeric character escape is too short (e.g. '\x1').
    TooShortHexEscape,
    /// Invalid character in numeric escape (e.g. '\xz')
    InvalidCharInHexEscape,

    /// Unicode character escape is too short (e.g. '\u1').
    TooShortUnicodeEscape,
    /// Non-hexadecimal value in '\uXXXX'.
    InvalidCharInUnicodeEscape,
    /// Invalid in-bound unicode character code, e.g. '\u{DFFF}'.
    LoneSurrogateUnicodeEscape,

    /// Non-ascii character in non-unicode literal.
    NonAsciiCharInNonUnicode,

    /// New-line character in any string literal.
    NewLine,
    /// Escape character in hex literal.
    EscapeInHex,
    /// Non hex-digit character in hex literal.
    NonHexDigitInHex,
    /// Odd number of hex digits in hex literal.
    OddHexDigits,
    /// Hex literal with the `0x` prefix.
    HexPrefix,
    /*
    /// After a line ending with '\', the next line contains whitespace
    /// characters that are not skipped.
    UnskippedWhitespaceWarning,

    /// After a line ending with '\', multiple lines are skipped.
    MultipleSkippedLinesWarning,
    */
}

impl EscapeError {
    /// Returns true for actual errors, as opposed to warnings.
    pub fn is_fatal(&self) -> bool {
        false
        // TODO
        /*
        !matches!(
            self,
            EscapeError::UnskippedWhitespaceWarning | EscapeError::MultipleSkippedLinesWarning
        )
        */
    }
}

/// Takes a contents of a literal (without quotes) and produces a
/// sequence of escaped characters or errors.
///
/// Values are returned through invoking of the provided callback.
pub fn unescape_literal<F>(src: &str, quote: char, mode: Mode, callback: &mut F)
where
    F: FnMut(Range<usize>, Result<char, EscapeError>),
{
    match mode {
        Mode::Str => unescape_str(src, quote, false, callback),
        Mode::UnicodeStr => unescape_str(src, quote, true, callback),
        Mode::HexStr => unescape_hex_str(src, callback),
    }
}

/// What kind of literal do we parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    Str,
    UnicodeStr,
    HexStr,
}

fn scan_escape(chars: &mut Chars<'_>) -> Result<char, EscapeError> {
    // Previous character was '\\', unescape what follows.
    // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityLexer.EscapeSequence
    // Hex and unicode escapes are not validated.
    let res = match chars.next().ok_or(EscapeError::LoneSlash)? {
        // Both quotes are valid escapes for both string literals in Solidity,
        // e.g escaping single in a double, or double in a single is ok.
        '\'' => '\'',
        '"' => '"',
        '\\' => '\\',
        'n' => '\n',
        'r' => '\r',
        't' => '\t',

        'x' => {
            // Parse hexadecimal character code.
            let hi = chars.next().ok_or(EscapeError::TooShortHexEscape)?;
            let hi = hi.to_digit(16).ok_or(EscapeError::InvalidCharInHexEscape)?;

            let lo = chars.next().ok_or(EscapeError::TooShortHexEscape)?;
            let lo = lo.to_digit(16).ok_or(EscapeError::InvalidCharInHexEscape)?;

            let value = hi * 16 + lo;
            value as u8 as char
        }

        'u' => {
            // Parse hexadecimal unicode character code.
            let mut value = 0;
            for _ in 0..4 {
                let d = chars.next().ok_or(EscapeError::TooShortUnicodeEscape)?;
                let d = d.to_digit(16).ok_or(EscapeError::InvalidCharInUnicodeEscape)?;
                value = value * 16 + d;
            }
            // FIXME: `'\u{D800}'..='\u{DFFF}'` are valid in Solidity but not in Rust.
            char::from_u32(value).ok_or(EscapeError::LoneSurrogateUnicodeEscape)?
        }

        _ => return Err(EscapeError::InvalidEscape),
    };
    Ok(res)
}

/// Takes a contents of a string literal (without quotes) and produces a
/// sequence of escaped characters or errors.
fn unescape_str<F>(src: &str, quote: char, is_unicode: bool, callback: &mut F)
where
    F: FnMut(Range<usize>, Result<char, EscapeError>),
{
    debug_assert!(quote == '"' || quote == '\'');
    let mut chars = src.chars();
    // The `start` and `end` computation here is complicated because
    // `skip_ascii_whitespace` makes us to skip over chars without counting
    // them in the range computation.
    while let Some(c) = chars.next() {
        let start = src.len() - chars.as_str().len() - c.len_utf8();
        let res = match c {
            '\\' => {
                // FIXME: Doesn't look like Solidity supports this.
                /*
                match chars.clone().next() {
                    Some('\n') => {
                        // Rust language specification requires us to skip whitespaces
                        // if unescaped '\' character is followed by '\n'.
                        // For details see [Rust language reference]
                        // (https://doc.rust-lang.org/reference/tokens.html#string-literals).
                        skip_ascii_whitespace(&mut chars, start, callback);
                        continue;
                    }
                    _ => scan_escape(&mut chars),
                }
                */
                scan_escape(&mut chars)
            }
            '\n' => Err(EscapeError::NewLine),
            '\t' => Ok('\t'),
            '\r' => Err(EscapeError::BareCarriageReturn),
            c @ ('\'' | '"') if c == quote => Err(EscapeError::EscapeOnlyChar),
            c if !is_unicode && !c.is_ascii() => Err(EscapeError::NonAsciiCharInNonUnicode),
            c => Ok(c),
        };
        let end = src.len() - chars.as_str().len();
        callback(start..end, res);
    }

    /*
    fn skip_ascii_whitespace<F>(chars: &mut Chars<'_>, start: usize, callback: &mut F)
    where
        F: FnMut(Range<usize>, Result<char, EscapeError>),
    {
        let tail = chars.as_str();
        let first_non_space = tail
            .bytes()
            .position(|b| b != b' ' && b != b'\t' && b != b'\n' && b != b'\r')
            .unwrap_or(tail.len());
        if tail[1..first_non_space].contains('\n') {
            // The +1 accounts for the escaping slash.
            let end = start + first_non_space + 1;
            callback(start..end, Err(EscapeError::MultipleSkippedLinesWarning));
        }
        let tail = &tail[first_non_space..];
        if let Some(c) = tail.chars().nth(0) {
            if c.is_whitespace() {
                // For error reporting, we would like the span to contain the character that was not
                // skipped. The +1 is necessary to account for the leading \ that started the
                // escape.
                let end = start + first_non_space + c.len_utf8() + 1;
                callback(start..end, Err(EscapeError::UnskippedWhitespaceWarning));
            }
        }
        *chars = tail.chars();
    }
    */
}

/// Takes a contents of a hex literal (without quotes) and produces a
/// sequence of escaped characters or errors.
fn unescape_hex_str<F>(src: &str, callback: &mut F)
where
    F: FnMut(Range<usize>, Result<char, EscapeError>),
{
    let mut chars = src.char_indices();
    if src.starts_with("0x") || src.starts_with("0X") {
        chars.next();
        chars.next();
        callback(0..2, Err(EscapeError::HexPrefix));
    }

    let mut count = 0;
    for (start, c) in chars {
        let res = match c {
            '\\' => Err(EscapeError::EscapeInHex),
            // FIXME: This should be more strict.
            // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.hexStringLiteral
            '_' => continue,
            c if !c.is_ascii_hexdigit() => Err(EscapeError::NonHexDigitInHex),
            c => {
                count += 1;
                Ok(c)
            }
        };
        let end = start + c.len_utf8();
        callback(start..end, res);
    }
    if count % 2 != 0 {
        callback(0..src.len(), Err(EscapeError::OddHexDigits));
    }
}

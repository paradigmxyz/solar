//! Utilities for validating string and char literals and turning them into values they represent.

use std::{ops::Range, str::Chars};

/// Takes a contents of a literal (without quotes) and produces a
/// sequence of escaped characters or errors.
///
/// Values are returned through invoking of the provided callback.
pub fn unescape_literal<F>(src: &str, mode: Mode, callback: &mut F)
where
    F: FnMut(Range<usize>, Result<char, EscapeError>),
{
    match mode {
        Mode::Str => unescape_str(src, false, callback),
        Mode::UnicodeStr => unescape_str(src, true, callback),
        Mode::HexStr => unescape_hex_str(src, callback),
    }
}

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
    TooShortHexEscape,
    /// Invalid character in numeric escape (e.g. '\xz')
    InvalidCharInHexEscape,

    /// Unicode character escape is too short (e.g. '\u1').
    TooShortUnicodeEscape,
    /// Non-hexadecimal value in '\uXXXX'.
    InvalidCharInUnicodeEscape,
    /// Invalid in-bound unicode character code, e.g. '\u{DFFF}'.
    LoneSurrogateUnicodeEscape,

    /// Newline in string literal. These must be escaped.
    NewlineInStr,
    /// Non-ASCII character in non-unicode literal.
    NonAsciiCharInNonUnicode,

    /// Non hex-digit character in hex literal.
    NonHexDigitInHex,
    /// Underscore in hex literal.
    BadUnderscoreInHex,
    /// Odd number of hex digits in hex literal.
    OddHexDigits,
    /// Hex literal with the `0x` prefix.
    HexPrefix,
}

/// What kind of literal do we parse.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// Normal string literal (e.g. `"a"`).
    Str,
    /// Unicode string literal (e.g. `unicode"😀"`).
    UnicodeStr,
    /// Hex string literal (e.g. `hex"1234"`).
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

/// Takes a contents of a string literal (without quotes) and produces a sequence of escaped
/// characters or errors.
fn unescape_str<F>(src: &str, is_unicode: bool, callback: &mut F)
where
    F: FnMut(Range<usize>, Result<char, EscapeError>),
{
    let mut chars = src.chars();
    // The `start` and `end` computation here is complicated because
    // `skip_ascii_whitespace` makes us to skip over chars without counting
    // them in the range computation.
    while let Some(c) = chars.next() {
        let start = src.len() - chars.as_str().len() - c.len_utf8();
        let res = match c {
            '\\' => match chars.clone().next() {
                Some('\n') => {
                    skip_ascii_whitespace(&mut chars, start, callback);
                    continue;
                }
                _ => scan_escape(&mut chars),
            },
            '\n' => Err(EscapeError::NewlineInStr),
            '\r' => Err(EscapeError::BareCarriageReturn),
            c if !is_unicode && !c.is_ascii() => Err(EscapeError::NonAsciiCharInNonUnicode),
            c => Ok(c),
        };
        let end = src.len() - chars.as_str().len();
        callback(start..end, res);
    }
}

fn skip_ascii_whitespace<F>(chars: &mut Chars<'_>, start: usize, callback: &mut F)
where
    F: FnMut(Range<usize>, Result<char, EscapeError>),
{
    // Skip the first newline.
    let tail = &chars.as_str()[1..];
    let first_non_space =
        tail.bytes().position(|b| !matches!(b, b' ' | b'\t')).unwrap_or(tail.len());
    let mut tail = &tail[first_non_space..];
    if let Some(tail2) = tail.strip_prefix('\n').or_else(|| tail.strip_prefix("\r\n")) {
        tail = tail2;
        // +1 for the first newline.
        let start = start + 1 + first_non_space;
        let end = start + 1;
        callback(start..end, Err(EscapeError::CannotSkipMultipleLines));
    }
    *chars = tail.chars();
}

/// Takes a contents of a hex literal (without quotes) and produces a sequence of escaped characters
/// or errors.
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

    let count = chars.clone().filter(|(_, c)| c.is_ascii_hexdigit()).count();
    if count % 2 != 0 {
        callback(0..src.len(), Err(EscapeError::OddHexDigits));
        return;
    }

    let mut emit_underscore_errors = true;
    let mut allow_underscore = false;
    let mut even = true;
    for (start, c) in chars {
        let res = match c {
            '_' => {
                if emit_underscore_errors && (!allow_underscore || !even) {
                    // Don't spam errors for multiple underscores.
                    emit_underscore_errors = false;
                    Err(EscapeError::BadUnderscoreInHex)
                } else {
                    allow_underscore = false;
                    continue;
                }
            }
            c if !c.is_ascii_hexdigit() => Err(EscapeError::NonHexDigitInHex),
            c => Ok(c),
        };

        if res.is_ok() {
            even = !even;
            allow_underscore = true;
        }

        let end = start + c.len_utf8();
        callback(start..end, res);
    }

    if emit_underscore_errors && src.len() > 1 && src.ends_with('_') {
        callback(src.len() - 1..src.len(), Err(EscapeError::BadUnderscoreInHex));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use EscapeError::*;

    type ExErr = (Range<usize>, EscapeError);

    #[track_caller]
    fn check(mode: Mode, src: &str, expected_str: &str, expected_errs: &[ExErr]) {
        let mut ok = String::with_capacity(src.len());
        let mut errs = Vec::new();
        unescape_literal(src, mode, &mut |range, c| match c {
            Ok(c) => ok.push(c),
            Err(e) => errs.push((range, e)),
        });
        assert_eq!(errs, expected_errs, "{mode:?}: {src:?}");
        assert_eq!(ok, expected_str, "{mode:?}: {src:?}");
    }

    #[test]
    fn unescape_str() {
        let cases: &[(&str, &str, &[ExErr])] = &[
            ("", "", &[]),
            (" ", " ", &[]),
            ("\t", "\t", &[]),
            (" \t ", " \t ", &[]),
            ("foo", "foo", &[]),
            ("hello world", "hello world", &[]),
            (r"\", "", &[(0..1, LoneSlash)]),
            (r"\\", "\\", &[]),
            (r"\\\", "\\", &[(2..3, LoneSlash)]),
            (r"\\\\", "\\\\", &[]),
            (r"\\ ", "\\ ", &[]),
            (r"\\ \", "\\ ", &[(3..4, LoneSlash)]),
            (r"\\ \\", "\\ \\", &[]),
            (r"\x", "", &[(0..2, TooShortHexEscape)]),
            (r"\x1", "", &[(0..3, TooShortHexEscape)]),
            (r"\xz", "", &[(0..3, InvalidCharInHexEscape)]),
            (r"\xzf", "f", &[(0..3, InvalidCharInHexEscape)]),
            (r"\xzz", "z", &[(0..3, InvalidCharInHexEscape)]),
            (r"\x69", "\x69", &[]),
            (r"\xE8", "è", &[]),
            (r"\u", "", &[(0..2, TooShortUnicodeEscape)]),
            (r"\u1", "", &[(0..3, TooShortUnicodeEscape)]),
            (r"\uz", "", &[(0..3, InvalidCharInUnicodeEscape)]),
            (r"\uzf", "f", &[(0..3, InvalidCharInUnicodeEscape)]),
            (r"\u12", "", &[(0..4, TooShortUnicodeEscape)]),
            (r"\u123", "", &[(0..5, TooShortUnicodeEscape)]),
            (r"\u1234", "\u{1234}", &[]),
            (r"\u00e8", "è", &[]),
            (r"\r", "\r", &[]),
            (r"\t", "\t", &[]),
            (r"\n", "\n", &[]),
            (r"\n\n", "\n\n", &[]),
            (r"\ ", "", &[(0..2, InvalidEscape)]),
            (r"\?", "", &[(0..2, InvalidEscape)]),
            ("\r\n", "", &[(0..1, BareCarriageReturn), (1..2, NewlineInStr)]), // TODO: ?
            ("\n", "", &[(0..1, NewlineInStr)]),
            ("\\\n", "", &[]),
            ("\\\na", "a", &[]),
            ("\\\n  a", "a", &[]),
            ("\\\n \t a", "a", &[]),
            (" \\\n \t a", " a", &[]),
            ("\\\n \t a\n", "a", &[(6..7, NewlineInStr)]),
        ];
        for &(src, expected_str, expected_errs) in cases {
            check(Mode::Str, src, expected_str, expected_errs);
            check(Mode::UnicodeStr, src, expected_str, expected_errs);
        }
    }

    #[test]
    fn unescape_unicode_str() {
        let cases: &[(&str, &str, &[ExErr], &[ExErr])] = &[
            ("è", "è", &[], &[(0..2, NonAsciiCharInNonUnicode)]),
            ("😀", "😀", &[], &[(0..4, NonAsciiCharInNonUnicode)]),
        ];
        for &(src, expected_str, e1, e2) in cases {
            check(Mode::UnicodeStr, src, expected_str, e1);
            check(Mode::Str, src, "", e2);
        }
    }

    #[test]
    fn unescape_hex_str() {
        let cases: &[(&str, &str, &[ExErr])] = &[
            ("", "", &[]),
            ("z", "", &[(0..1, NonHexDigitInHex)]),
            ("\n", "", &[(0..1, NonHexDigitInHex)]),
            ("  11", "11", &[(0..1, NonHexDigitInHex), (1..2, NonHexDigitInHex)]),
            ("0x", "", &[(0..2, HexPrefix)]),
            ("0X", "", &[(0..2, HexPrefix)]),
            ("0x11", "11", &[(0..2, HexPrefix)]),
            ("0X11", "11", &[(0..2, HexPrefix)]),
            ("1", "", &[(0..1, OddHexDigits)]),
            ("12", "12", &[]),
            ("123", "", &[(0..3, OddHexDigits)]),
            ("1234", "1234", &[]),
            ("_", "", &[(0..1, BadUnderscoreInHex)]),
            ("_11", "11", &[(0..1, BadUnderscoreInHex)]),
            ("_11_", "11", &[(0..1, BadUnderscoreInHex)]),
            ("11_", "11", &[(2..3, BadUnderscoreInHex)]),
            ("11_22", "1122", &[]),
            ("11__", "11", &[(3..4, BadUnderscoreInHex)]),
            ("11__22", "1122", &[(3..4, BadUnderscoreInHex)]),
            ("1_2", "12", &[(1..2, BadUnderscoreInHex)]),
        ];
        for &(src, expected_str, expected_errs) in cases {
            check(Mode::HexStr, src, expected_str, expected_errs);
        }
    }
}

//! Low-level Solidity lexer.
//!
//! Modified from Rust's [`rustc_lexer`](https://github.com/rust-lang/rust/blob/45749b21b7fd836f6c4f11dd40376f7c83e2791b/compiler/rustc_lexer/src/lib.rs).

use solar_ast::{Base, StrKind};
use solar_data_structures::hint::unlikely;
use std::str::Chars;

pub mod token;
use token::{RawLiteralKind, RawToken, RawTokenKind};

#[cfg(test)]
mod tests;

/// Returns `true` if the given character is considered a whitespace.
#[inline]
pub const fn is_whitespace(c: char) -> bool {
    is_whitespace_byte(ch2u8(c))
}
/// Returns `true` if the given character is considered a whitespace.
#[inline]
pub const fn is_whitespace_byte(c: u8) -> bool {
    matches!(c, b' ' | b'\t' | b'\n' | b'\r')
}

/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start(c: char) -> bool {
    is_id_start_byte(ch2u8(c))
}
/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start_byte(c: u8) -> bool {
    matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'_' | b'$')
}

/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue(c: char) -> bool {
    is_id_continue_byte(ch2u8(c))
}
/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue_byte(c: u8) -> bool {
    matches!(c, b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'_' | b'$')
}

/// Returns `true` if the given string is a valid Solidity identifier.
///
/// An identifier in Solidity has to start with a letter, a dollar-sign or an underscore and may
/// additionally contain numbers after the first symbol.
///
/// Reference: <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityLexer.Identifier>
#[inline]
pub const fn is_ident(s: &str) -> bool {
    is_ident_bytes(s.as_bytes())
}

/// Returns `true` if the given byte slice is a valid Solidity identifier.
///
/// See [`is_ident`] for more details.
pub const fn is_ident_bytes(s: &[u8]) -> bool {
    // Note: valid idents can only contain ASCII characters, so we can use the byte representation
    // here.
    let [first, ref rest @ ..] = *s else {
        return false;
    };

    if !is_id_start_byte(first) {
        return false;
    }

    let mut i = 0;
    while i < rest.len() {
        if !is_id_continue_byte(rest[i]) {
            return false;
        }
        i += 1;
    }

    true
}

/// Converts a `char` to a `u8`.
#[inline(always)]
const fn ch2u8(c: char) -> u8 {
    c as u32 as u8
}

const EOF: u8 = b'\0';

/// Peekable iterator over a char sequence.
///
/// Next characters can be peeked via `first` method,
/// and position can be shifted forward via `bump` method.
#[derive(Clone, Debug)]
pub struct Cursor<'a> {
    len_remaining: usize,
    chars: Chars<'a>,
    #[cfg(debug_assertions)]
    prev: u8,
}

impl<'a> Cursor<'a> {
    /// Creates a new cursor over the given input string slice.
    pub fn new(input: &'a str) -> Self {
        Cursor {
            len_remaining: input.len(),
            chars: input.chars(),
            #[cfg(debug_assertions)]
            prev: EOF,
        }
    }

    /// Parses a token from the input string.
    pub fn advance_token(&mut self) -> RawToken {
        let first_char = match self.bump_ret() {
            Some(c) => c,
            None => return RawToken::EOF,
        };
        let token_kind = if first_char.is_ascii() {
            self.advance_token_kind(first_char)
        } else {
            RawTokenKind::Unknown
        };
        let len = self.pos_within_token();
        self.reset_pos_within_token();
        RawToken::new(token_kind, len)
    }

    #[inline]
    fn advance_token_kind(&mut self, first_char: u8) -> RawTokenKind {
        match first_char {
            // Slash, comment or block comment.
            b'/' => match self.first() {
                b'/' => self.line_comment(),
                b'*' => self.block_comment(),
                _ => RawTokenKind::Slash,
            },

            // Whitespace sequence.
            c if is_whitespace_byte(c) => self.whitespace(),

            // Identifier (this should be checked after other variant that can start as identifier).
            c if is_id_start_byte(c) => self.ident_or_prefixed_literal(c),

            // Numeric literal.
            b'0'..=b'9' => {
                let kind = self.number(first_char);
                RawTokenKind::Literal { kind }
            }
            b'.' if self.first().is_ascii_digit() => {
                let kind = self.rational_number_after_dot(Base::Decimal);
                RawTokenKind::Literal { kind }
            }

            // One-symbol tokens.
            b';' => RawTokenKind::Semi,
            b',' => RawTokenKind::Comma,
            b'.' => RawTokenKind::Dot,
            b'(' => RawTokenKind::OpenParen,
            b')' => RawTokenKind::CloseParen,
            b'{' => RawTokenKind::OpenBrace,
            b'}' => RawTokenKind::CloseBrace,
            b'[' => RawTokenKind::OpenBracket,
            b']' => RawTokenKind::CloseBracket,
            b'~' => RawTokenKind::Tilde,
            b'?' => RawTokenKind::Question,
            b':' => RawTokenKind::Colon,
            b'=' => RawTokenKind::Eq,
            b'!' => RawTokenKind::Bang,
            b'<' => RawTokenKind::Lt,
            b'>' => RawTokenKind::Gt,
            b'-' => RawTokenKind::Minus,
            b'&' => RawTokenKind::And,
            b'|' => RawTokenKind::Or,
            b'+' => RawTokenKind::Plus,
            b'*' => RawTokenKind::Star,
            b'^' => RawTokenKind::Caret,
            b'%' => RawTokenKind::Percent,

            // String literal.
            b'\'' | b'"' => {
                let terminated = self.eat_string(first_char);
                let kind = RawLiteralKind::Str { kind: StrKind::Str, terminated };
                RawTokenKind::Literal { kind }
            }

            _ => RawTokenKind::Unknown,
        }
    }

    #[inline(never)]
    fn line_comment(&mut self) -> RawTokenKind {
        debug_assert!(self.prev() == b'/' && self.first() == b'/');
        self.bump();

        // `////` (more than 3 slashes) is not considered a doc comment.
        let is_doc = matches!(self.first(), b'/' if self.second() != b'/');

        self.eat_until(b'\n');
        RawTokenKind::LineComment { is_doc }
    }

    fn block_comment(&mut self) -> RawTokenKind {
        debug_assert!(self.prev() == b'/' && self.first() == b'*');
        self.bump();

        // `/***` (more than 2 stars) is not considered a doc comment.
        // `/**/` is not considered a doc comment.
        let is_doc = matches!(self.first(), b'*' if !matches!(self.second(), b'*' | b'/'));

        let mut terminated = false;
        while let Some(c) = self.bump_ret() {
            if c == b'*' && self.first() == b'/' {
                terminated = true;
                self.bump();
                break;
            }
        }

        RawTokenKind::BlockComment { is_doc, terminated }
    }

    fn whitespace(&mut self) -> RawTokenKind {
        debug_assert!(is_whitespace_byte(self.prev()));
        self.eat_while(is_whitespace_byte);
        RawTokenKind::Whitespace
    }

    fn ident_or_prefixed_literal(&mut self, first: u8) -> RawTokenKind {
        debug_assert!(is_id_start_byte(self.prev()));

        // Start is already eaten, eat the rest of identifier.
        let start = self.as_str().as_ptr();
        self.eat_while(is_id_continue_byte);

        // Check if the identifier is a string literal prefix.
        if unlikely(matches!(first, b'h' | b'u')) {
            // SAFETY: within bounds and lifetime of `self.chars`.
            let id = unsafe {
                let start = start.sub(1);
                std::slice::from_raw_parts(
                    start,
                    self.as_str().as_ptr().offset_from_unsigned(start),
                )
            };
            let is_hex = id == b"hex";
            if is_hex || id == b"unicode" {
                if let quote @ (b'\'' | b'"') = self.first() {
                    self.bump();
                    let terminated = self.eat_string(quote);
                    let kind = if is_hex { StrKind::Hex } else { StrKind::Unicode };
                    return RawTokenKind::Literal {
                        kind: RawLiteralKind::Str { kind, terminated },
                    };
                }
            }
        }

        RawTokenKind::Ident
    }

    fn number(&mut self, first_digit: u8) -> RawLiteralKind {
        debug_assert!(self.prev().is_ascii_digit());
        let mut base = Base::Decimal;
        if first_digit == b'0' {
            // Attempt to parse encoding base.
            let has_digits = match self.first() {
                b'b' => {
                    base = Base::Binary;
                    self.bump();
                    self.eat_decimal_digits()
                }
                b'o' => {
                    base = Base::Octal;
                    self.bump();
                    self.eat_decimal_digits()
                }
                b'x' => {
                    base = Base::Hexadecimal;
                    self.bump();
                    self.eat_hexadecimal_digits()
                }
                // Not a base prefix.
                b'0'..=b'9' | b'_' | b'.' | b'e' | b'E' => {
                    self.eat_decimal_digits();
                    true
                }
                // Just a 0.
                _ => return RawLiteralKind::Int { base, empty_int: false },
            };
            // Base prefix was provided, but there were no digits after it, e.g. "0x".
            if !has_digits {
                return RawLiteralKind::Int { base, empty_int: true };
            }
        } else {
            // No base prefix, parse number in the usual way.
            self.eat_decimal_digits();
        };

        match self.first() {
            // Don't be greedy if this is actually an integer literal followed by field/method
            // access (`12.foo()`).
            // `_` is special cased, we assume it's always an invalid rational: https://github.com/ethereum/solidity/blob/c012b725bb8ce755b93ce0dd05e83c34c499acd6/liblangutil/Scanner.cpp#L979
            b'.' if !is_id_start_byte(self.second()) || self.second() == b'_' => {
                self.bump();
                self.rational_number_after_dot(base)
            }
            b'e' | b'E' => {
                self.bump();
                let empty_exponent = !self.eat_exponent();
                RawLiteralKind::Rational { base, empty_exponent }
            }
            _ => RawLiteralKind::Int { base, empty_int: false },
        }
    }

    #[cold]
    fn rational_number_after_dot(&mut self, base: Base) -> RawLiteralKind {
        self.eat_decimal_digits();
        let empty_exponent = match self.first() {
            b'e' | b'E' => {
                self.bump();
                !self.eat_exponent()
            }
            _ => false,
        };
        RawLiteralKind::Rational { base, empty_exponent }
    }

    /// Eats a string until the given quote character. Returns `true` if the string was terminated.
    fn eat_string(&mut self, quote: u8) -> bool {
        debug_assert_eq!(self.prev(), quote);
        while let Some(c) = self.bump_ret() {
            if c == quote {
                return true;
            }
            if c == b'\\' {
                let first = self.first();
                if first == b'\\' || first == quote {
                    // Bump again to skip escaped character.
                    self.bump();
                }
            }
        }
        // End of file reached.
        false
    }

    /// Eats characters for a decimal number. Returns `true` if any digits were encountered.
    fn eat_decimal_digits(&mut self) -> bool {
        self.eat_digits(|x| x.is_ascii_digit())
    }

    /// Eats characters for a hexadecimal number. Returns `true` if any digits were encountered.
    fn eat_hexadecimal_digits(&mut self) -> bool {
        self.eat_digits(|x| x.is_ascii_hexdigit())
    }

    fn eat_digits(&mut self, mut is_digit: impl FnMut(u8) -> bool) -> bool {
        let mut has_digits = false;
        loop {
            match self.first() {
                b'_' => {
                    self.bump();
                }
                c if is_digit(c) => {
                    has_digits = true;
                    self.bump();
                }
                _ => break,
            }
        }
        has_digits
    }

    /// Eats the exponent. Returns `true` if any digits were encountered.
    fn eat_exponent(&mut self) -> bool {
        debug_assert!(self.prev() == b'e' || self.prev() == b'E');
        // b'+' is not a valid prefix for an exponent.
        if self.first() == b'-' {
            self.bump();
        }
        self.eat_decimal_digits()
    }

    /// Returns the remaining input as a string slice.
    #[inline]
    pub fn as_str(&self) -> &'a str {
        self.chars.as_str()
    }

    /// Returns the last eaten symbol. Only available with `debug_assertions` enabled.
    #[inline]
    fn prev(&self) -> u8 {
        #[cfg(debug_assertions)]
        return self.prev;
        #[cfg(not(debug_assertions))]
        return EOF;
    }

    /// Peeks the next symbol from the input stream without consuming it.
    /// If requested position doesn't exist, `EOF` is returned.
    /// However, getting `EOF` doesn't always mean actual end of file,
    /// it should be checked with `is_eof` method.
    #[inline]
    fn first(&self) -> u8 {
        self.peek_byte(0)
    }

    /// Peeks the second symbol from the input stream without consuming it.
    #[inline]
    fn second(&self) -> u8 {
        // This function is only called after `first` was called and checked, so in practice it
        // doesn't matter if it's part of the first UTF-8 character.
        self.peek_byte(1)
    }

    // Do not use directly.
    #[doc(hidden)]
    #[inline]
    fn peek_byte(&self, index: usize) -> u8 {
        self.as_str().as_bytes().get(index).copied().unwrap_or(EOF)
    }

    /// Checks if there is nothing more to consume.
    #[inline]
    fn is_eof(&self) -> bool {
        self.as_str().is_empty()
    }

    /// Returns amount of already consumed symbols.
    #[inline]
    fn pos_within_token(&self) -> u32 {
        (self.len_remaining - self.as_str().len()) as u32
    }

    /// Resets the number of bytes consumed to 0.
    #[inline]
    fn reset_pos_within_token(&mut self) {
        self.len_remaining = self.as_str().len();
    }

    /// Moves to the next character.
    fn bump(&mut self) {
        self.bump_inlined();
    }

    /// Moves to the next character, returning the current one.
    fn bump_ret(&mut self) -> Option<u8> {
        let c = self.as_str().as_bytes().first().copied();
        self.bump_inlined();
        c
    }

    #[inline]
    fn bump_inlined(&mut self) {
        // NOTE: This intentionally does not assign `_c` in the next line, as rustc currently emit a
        // lot more LLVM IR (for an `assume`), which messes with the optimizations and inling costs.
        #[cfg(not(debug_assertions))]
        self.chars.next();
        #[cfg(debug_assertions)]
        if let Some(c) = self.chars.next() {
            self.prev = c as u8;
        }
    }

    /// Advances `n` bytes, without setting `prev`.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    fn ignore_bytes(&mut self, n: usize) {
        debug_assert!(n <= self.as_str().len());
        self.chars = unsafe { self.as_str().get_unchecked(n..) }.chars();
    }

    /// Eats symbols until `ch` is found or until the end of file is reached.
    #[inline]
    fn eat_until(&mut self, ch: u8) {
        let b = self.as_str().as_bytes();
        self.ignore_bytes(memchr::memchr(ch, b).unwrap_or(b.len()));
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    #[inline]
    fn eat_while(&mut self, mut predicate: impl FnMut(u8) -> bool) {
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }
}

impl Iterator for Cursor<'_> {
    type Item = RawToken;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let token = self.advance_token();
        if token.kind == RawTokenKind::Eof {
            None
        } else {
            Some(token)
        }
    }
}

impl std::iter::FusedIterator for Cursor<'_> {}

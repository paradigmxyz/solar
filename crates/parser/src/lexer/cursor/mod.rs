//! Low-level Solidity lexer.
//!
//! Modified from Rust's [`rustc_lexer`](https://github.com/rust-lang/rust/blob/45749b21b7fd836f6c4f11dd40376f7c83e2791b/compiler/rustc_lexer/src/lib.rs).

use std::str::Chars;
use sulk_ast::ast::Base;

mod token;
pub(crate) use token::{LiteralKind, Token, TokenKind};

#[cfg(test)]
mod tests;

/// Returns `true` if `c` is considered a whitespace.
#[inline]
pub const fn is_whitespace(c: char) -> bool {
    matches!(c, '\t' | '\n' | ' ')
}

/// Returns `true` if the given character is valid at the start of a Solidity identifier.
#[inline]
pub const fn is_id_start(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '_' | '$')
}

/// Returns `true` if the given character is valid in a Solidity identifier.
#[inline]
pub const fn is_id_continue(c: char) -> bool {
    matches!(c, 'a'..='z' | 'A'..='Z' | '0'..='9' | '_' | '$')
}

/// Returns `true` if the given string is a valid Solidity identifier.
///
/// An identifier in Solidity has to start with a letter, a dollar-sign or an underscore and may
/// additionally contain numbers after the first symbol.
///
/// Solidity reference:
/// <https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityLexer.Identifier>
pub const fn is_ident(s: &str) -> bool {
    // Note: valid idents can only contain ASCII characters, so we can
    // use the byte representation here.
    let [first, rest @ ..] = s.as_bytes() else {
        return false;
    };

    if !is_id_start(*first as char) {
        return false;
    }

    let mut i = 0;
    while i < rest.len() {
        if !is_id_continue(rest[i] as char) {
            return false;
        }
        i += 1;
    }

    true
}

const EOF_CHAR: char = '\0';

/// Peekable iterator over a char sequence.
///
/// Next characters can be peeked via `first` method,
/// and position can be shifted forward via `bump` method.
#[derive(Clone, Debug)]
pub(crate) struct Cursor<'a> {
    len_remaining: usize,
    /// Iterator over chars. Slightly faster than a &str.
    chars: Chars<'a>,
    #[cfg(debug_assertions)]
    prev: char,
}

impl<'a> Cursor<'a> {
    /// Creates a new cursor over the given input string slice.
    pub(crate) fn new(input: &'a str) -> Cursor<'a> {
        Cursor {
            len_remaining: input.len(),
            chars: input.chars(),
            #[cfg(debug_assertions)]
            prev: EOF_CHAR,
        }
    }

    /// Parses a token from the input string.
    pub(crate) fn advance_token(&mut self) -> Token {
        let first_char = match self.bump() {
            Some(c) => c,
            None => return Token::EOF,
        };

        let token_kind = match first_char {
            // Slash, comment or block comment.
            '/' => match self.first() {
                '/' => self.line_comment(),
                '*' => self.block_comment(),
                _ => TokenKind::Slash,
            },

            // Whitespace sequence.
            c if is_whitespace(c) => self.whitespace(),

            // Identifier (this should be checked after other variant that can start as identifier).
            c if is_id_start(c) => self.ident_or_prefixed_literal(c),

            // Numeric literal.
            c @ '0'..='9' => {
                let kind = self.number(c);
                TokenKind::Literal { kind }
            }

            // One-symbol tokens.
            ';' => TokenKind::Semi,
            ',' => TokenKind::Comma,
            '.' => TokenKind::Dot,
            '(' => TokenKind::OpenParen,
            ')' => TokenKind::CloseParen,
            '{' => TokenKind::OpenBrace,
            '}' => TokenKind::CloseBrace,
            '[' => TokenKind::OpenBracket,
            ']' => TokenKind::CloseBracket,
            '~' => TokenKind::Tilde,
            '?' => TokenKind::Question,
            ':' => TokenKind::Colon,
            '=' => TokenKind::Eq,
            '!' => TokenKind::Bang,
            '<' => TokenKind::Lt,
            '>' => TokenKind::Gt,
            '-' => TokenKind::Minus,
            '&' => TokenKind::And,
            '|' => TokenKind::Or,
            '+' => TokenKind::Plus,
            '*' => TokenKind::Star,
            '^' => TokenKind::Caret,
            '%' => TokenKind::Percent,

            // String literal.
            c @ ('\'' | '"') => {
                let terminated = self.eat_string(c);
                let kind = LiteralKind::Str { terminated, unicode: false };
                TokenKind::Literal { kind }
            }

            // Identifier starting with an emoji. Only lexed for graceful error recovery.
            // c if !c.is_ascii() && unic_emoji_char::is_emoji(c) => {
            //     self.fake_ident_or_unknown_prefix()
            // }
            _ => TokenKind::Unknown,
        };
        let res = Token::new(token_kind, self.pos_within_token());
        self.reset_pos_within_token();
        res
    }

    fn line_comment(&mut self) -> TokenKind {
        debug_assert!(self.prev() == '/' && self.first() == '/');
        self.bump();

        // `////` (more than 3 slashes) is not considered a doc comment.
        let is_doc = matches!(self.first(), '/' if self.second() != '/');

        self.eat_while(|c| c != '\n');
        TokenKind::LineComment { is_doc }
    }

    fn block_comment(&mut self) -> TokenKind {
        debug_assert!(self.prev() == '/' && self.first() == '*');
        self.bump();

        // `/***` (more than 2 stars) is not considered a doc comment.
        // `/**/` is not considered a doc comment.
        let is_doc = matches!(self.first(), '*' if !matches!(self.second(), '*' | '/'));

        let mut depth = 1usize;
        while let Some(c) = self.bump() {
            match c {
                '/' if self.first() == '*' => {
                    self.bump();
                    depth += 1;
                }
                '*' if self.first() == '/' => {
                    self.bump();
                    depth -= 1;
                    if depth == 0 {
                        // This block comment is closed, so for a construction like "/* */ */"
                        // there will be a successfully parsed block comment "/* */"
                        // and " */" will be processed separately.
                        break;
                    }
                }
                _ => (),
            }
        }

        TokenKind::BlockComment { is_doc, terminated: depth == 0 }
    }

    fn whitespace(&mut self) -> TokenKind {
        debug_assert!(is_whitespace(self.prev()));
        self.eat_while(is_whitespace);
        TokenKind::Whitespace
    }

    fn ident_or_prefixed_literal(&mut self, first_char: char) -> TokenKind {
        debug_assert!(is_id_start(self.prev()));

        // Check for potential prefixed literals.
        match first_char {
            // `hex"01234"`
            'h' => {
                if let Some(terminated) = self.maybe_string_prefix("hex") {
                    let kind = LiteralKind::HexStr { terminated };
                    return TokenKind::Literal { kind };
                }
            }
            // `unicode"abc"`
            'u' => {
                if let Some(terminated) = self.maybe_string_prefix("unicode") {
                    let kind = LiteralKind::Str { terminated, unicode: true };
                    return TokenKind::Literal { kind };
                }
            }
            _ => {}
        }

        // Start is already eaten, eat the rest of identifier.
        self.eat_while(is_id_continue);
        // Known prefixes must have been handled earlier.
        // So if we see a prefix here, it is definitely an unknown prefix.
        match self.first() {
            '"' | '\'' => TokenKind::UnknownPrefix,
            _ => TokenKind::Ident,
        }
    }

    fn number(&mut self, first_digit: char) -> LiteralKind {
        debug_assert!('0' <= self.prev() && self.prev() <= '9');
        let mut base = Base::Decimal;
        if first_digit == '0' {
            // Attempt to parse encoding base.
            let has_digits = match self.first() {
                'b' => {
                    base = Base::Binary;
                    self.bump();
                    self.eat_decimal_digits()
                }
                'o' => {
                    base = Base::Octal;
                    self.bump();
                    self.eat_decimal_digits()
                }
                'x' => {
                    base = Base::Hexadecimal;
                    self.bump();
                    self.eat_hexadecimal_digits()
                }
                // Not a base prefix.
                '0'..='9' | '_' | '.' | 'e' | 'E' => {
                    self.eat_decimal_digits();
                    true
                }
                // Just a 0.
                _ => return LiteralKind::Int { base, empty_int: false },
            };
            // Base prefix was provided, but there were no digits after it, e.g. "0x".
            if !has_digits {
                return LiteralKind::Int { base, empty_int: true };
            }
        } else {
            // No base prefix, parse number in the usual way.
            self.eat_decimal_digits();
        };

        match self.first() {
            // Don't be greedy if this is actually an integer literal followed
            // by field/method access (`12.foo()`)
            '.' if !is_id_start(self.second()) => {
                // might have stuff after the ., and if it does,
                // it needs to start with a number
                self.bump();
                let mut empty_exponent = false;
                if self.first().is_ascii_digit() {
                    self.eat_decimal_digits();
                    match self.first() {
                        'e' | 'E' => {
                            self.bump();
                            empty_exponent = !self.eat_exponent();
                        }
                        _ => (),
                    }
                }
                LiteralKind::Rational { base, empty_exponent }
            }
            'e' | 'E' => {
                self.bump();
                let empty_exponent = !self.eat_exponent();
                LiteralKind::Rational { base, empty_exponent }
            }
            _ => LiteralKind::Int { base, empty_int: false },
        }
    }

    fn maybe_string_prefix(&mut self, prefix: &str) -> Option<bool> {
        debug_assert_eq!(self.prev(), prefix.chars().next().unwrap());
        let prefix = &prefix[1..];
        let s = self.as_str();
        if s.starts_with(prefix) {
            let skip = prefix.len();
            let Some(quote @ ('"' | '\'')) = s.chars().nth(skip) else { return None };
            self.ignore(skip);
            self.bump();
            let terminated = self.eat_string(quote);
            Some(terminated)
        } else {
            None
        }
    }

    /// Eats a string until the given quote character. Returns `true` if the string was terminated.
    fn eat_string(&mut self, quote: char) -> bool {
        debug_assert_eq!(self.prev(), quote);
        while let Some(c) = self.bump() {
            if c == quote {
                return true;
            }
            if c == '\\' && (self.first() == '\\' || self.first() == quote) {
                // Bump again to skip escaped character.
                self.bump();
            }
        }
        // End of file reached.
        false
    }

    /// Eats characters for a decimal number. Returns `true` if any digits were encountered.
    fn eat_decimal_digits(&mut self) -> bool {
        let mut has_digits = false;
        loop {
            match self.first() {
                '_' => {
                    self.bump();
                }
                '0'..='9' => {
                    has_digits = true;
                    self.bump();
                }
                _ => break,
            }
        }
        has_digits
    }

    /// Eats characters for a hexadecimal number. Returns `true` if any digits were encountered.
    fn eat_hexadecimal_digits(&mut self) -> bool {
        let mut has_digits = false;
        loop {
            match self.first() {
                '_' => {
                    self.bump();
                }
                '0'..='9' | 'a'..='f' | 'A'..='F' => {
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
        debug_assert!(self.prev() == 'e' || self.prev() == 'E');
        // '+' is not a valid prefix for an exponent.
        if self.first() == '-' {
            self.bump();
        }
        self.eat_decimal_digits()
    }

    /// Returns the remaining input as a string slice.
    fn as_str(&self) -> &'a str {
        self.chars.as_str()
    }

    /// Returns the last eaten symbol.
    #[cfg(debug_assertions)]
    fn prev(&self) -> char {
        self.prev
    }

    /// Peeks the next symbol from the input stream without consuming it.
    /// If requested position doesn't exist, `EOF_CHAR` is returned.
    /// However, getting `EOF_CHAR` doesn't always mean actual end of file,
    /// it should be checked with `is_eof` method.
    fn first(&self) -> char {
        // `.next()` optimizes better than `.nth(0)`
        self.chars.clone().next().unwrap_or(EOF_CHAR)
    }

    /// Peeks the second symbol from the input stream without consuming it.
    fn second(&self) -> char {
        // `.next()` optimizes better than `.nth(1)`
        let mut iter = self.chars.clone();
        iter.next();
        iter.next().unwrap_or(EOF_CHAR)
    }

    /// Checks if there is nothing more to consume.
    fn is_eof(&self) -> bool {
        self.chars.as_str().is_empty()
    }

    /// Returns amount of already consumed symbols.
    fn pos_within_token(&self) -> u32 {
        (self.len_remaining - self.chars.as_str().len()) as u32
    }

    /// Resets the number of bytes consumed to 0.
    fn reset_pos_within_token(&mut self) {
        self.len_remaining = self.chars.as_str().len();
    }

    /// Moves to the next character.
    fn bump(&mut self) -> Option<char> {
        #[cfg(not(debug_assertions))]
        {
            self.chars.next()
        }

        #[cfg(debug_assertions)]
        {
            let c = self.chars.next();
            if let Some(c) = c {
                self.prev = c;
            }
            c
        }
    }

    /// Advances `n` characters, without setting `prev`.
    fn ignore(&mut self, n: usize) {
        for _ in 0..n {
            self.chars.next();
        }
    }

    /// Eats symbols while predicate returns true or until the end of file is reached.
    fn eat_while(&mut self, mut predicate: impl FnMut(char) -> bool) {
        // It was tried making optimized version of this for eg. line comments, but
        // LLVM can inline all of this and compile it down to fast iteration over bytes.
        while predicate(self.first()) && !self.is_eof() {
            self.bump();
        }
    }
}

impl Iterator for Cursor<'_> {
    type Item = Token;

    fn next(&mut self) -> Option<Self::Item> {
        let token = self.advance_token();
        if token.kind == TokenKind::Eof {
            None
        } else {
            Some(token)
        }
    }
}

impl std::iter::FusedIterator for Cursor<'_> {}

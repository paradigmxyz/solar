//! Low-level Solidity lexer.
//!
//! Modified from Rust's [rustc_lexer].
//!
//! [rustc_lexer]: https://github.com/rust-lang/rust/blob/45749b21b7fd836f6c4f11dd40376f7c83e2791b/compiler/rustc_lexer/src/lib.rs

mod cursor;

pub use cursor::Cursor;

mod token;
pub use token::{Base, LiteralKind, Token, TokenKind};

#[cfg(test)]
mod tests;

use LiteralKind::*;
use TokenKind::*;

/// Creates an iterator that produces tokens from the input string.
///
/// Alias for [`Cursor::new`].
#[inline]
pub fn tokenize(input: &str) -> Cursor {
    Cursor::new(input)
}

/// True if `c` is considered a whitespace according to Rust language definition.
///
/// See [Rust language reference][ref] for definitions of these classes.
///
/// [ref]: https://doc.rust-lang.org/reference/whitespace.html
pub fn is_whitespace(c: char) -> bool {
    // This is Pattern_White_Space.
    //
    // Note that this set is stable (ie, it doesn't change with different
    // Unicode versions), so it's ok to just hard-code the values.

    matches!(
        c,
        // Usual ASCII suspects
        '\u{0009}'   // \t
        | '\u{000A}' // \n
        | '\u{000B}' // vertical tab
        | '\u{000C}' // form feed
        | '\u{000D}' // \r
        | '\u{0020}' // space

        // NEXT LINE from latin1
        | '\u{0085}'

        // Bidi markers
        | '\u{200E}' // LEFT-TO-RIGHT MARK
        | '\u{200F}' // RIGHT-TO-LEFT MARK

        // Dedicated whitespace characters from Unicode
        | '\u{2028}' // LINE SEPARATOR
        | '\u{2029}' // PARAGRAPH SEPARATOR
    )
}

/// True if `c` is valid as a first character of an identifier.
pub fn is_id_start(c: char) -> bool {
    // This is XID_Start OR '_' (which formally is not a XID_Start).
    c == '_' || unicode_ident::is_xid_start(c)
}

/// True if `c` is valid as a non-first character of an identifier.
pub fn is_id_continue(c: char) -> bool {
    unicode_ident::is_xid_continue(c)
}

/// The passed string is lexically an identifier.
pub fn is_ident(string: &str) -> bool {
    let mut chars = string.chars();
    if let Some(start) = chars.next() {
        is_id_start(start) && chars.all(is_id_continue)
    } else {
        false
    }
}

impl Cursor<'_> {
    /// Parses a token from the input string.
    pub fn advance_token(&mut self) -> Token {
        let first_char = match self.bump() {
            Some(c) => c,
            None => return Token::eof(),
        };

        let token_kind = match first_char {
            // Slash, comment or block comment.
            '/' => match self.first() {
                '/' => self.line_comment(),
                '*' => self.block_comment(),
                _ => Slash,
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
            ';' => Semi,
            ',' => Comma,
            '.' => Dot,
            '(' => OpenParen,
            ')' => CloseParen,
            '{' => OpenBrace,
            '}' => CloseBrace,
            '[' => OpenBracket,
            ']' => CloseBracket,
            '~' => Tilde,
            '?' => Question,
            ':' => Colon,
            '$' => Dollar,
            '=' => Eq,
            '<' => Lt,
            '>' => Gt,
            '-' => Minus,
            '&' => And,
            '|' => Or,
            '+' => Plus,
            '*' => Star,
            '^' => Caret,
            '%' => Percent,

            // String literal.
            c @ ('\'' | '"') => {
                let terminated = self.eat_string(c);
                let kind = Str { terminated, unicode: false };
                Literal { kind }
            }

            // Identifier starting with an emoji. Only lexed for graceful error recovery.
            // c if !c.is_ascii() && unic_emoji_char::is_emoji(c) => {
            //     self.fake_ident_or_unknown_prefix()
            // }
            _ => Unknown,
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
        LineComment { is_doc }
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

        BlockComment { is_doc, terminated: depth == 0 }
    }

    fn whitespace(&mut self) -> TokenKind {
        debug_assert!(is_whitespace(self.prev()));
        self.eat_while(is_whitespace);
        Whitespace
    }

    fn ident_or_prefixed_literal(&mut self, first_char: char) -> TokenKind {
        debug_assert!(is_id_start(self.prev()));

        // Check for potential prefixed literals.
        match first_char {
            // `hex"01234"`
            'h' => {
                if let Some(lit) = self.maybe_hex_literal() {
                    return lit;
                }
            }
            // `unicode"abc"`
            'u' => {
                if let Some(lit) = self.maybe_unicode_literal() {
                    return lit;
                }
            }
            _ => {}
        }

        // Start is already eaten, eat the rest of identifier.
        self.eat_while(is_id_continue);
        // Known prefixes must have been handled earlier. So if
        // we see a prefix here, it is definitely an unknown prefix.
        self.eat_identifier();
        match self.first() {
            '"' | '\'' => UnknownPrefix,
            _ => Ident,
        }
    }

    fn number(&mut self, first_digit: char) -> LiteralKind {
        debug_assert!('0' <= self.prev() && self.prev() <= '9');
        let mut base = Base::Decimal;
        if first_digit == '0' {
            // Attempt to parse encoding base.
            let has_digits = match self.first() {
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
                _ => return Int { base, empty_int: false },
            };
            // Base prefix was provided, but there were no digits
            // after it, e.g. "0x".
            if !has_digits {
                return Int { base, empty_int: true };
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
                if self.first().is_digit(10) {
                    self.eat_decimal_digits();
                    match self.first() {
                        'e' | 'E' => {
                            self.bump();
                            empty_exponent = !self.eat_exponent();
                        }
                        _ => (),
                    }
                }
                Rational { base, empty_exponent }
            }
            'e' | 'E' => {
                self.bump();
                let empty_exponent = !self.eat_exponent();
                Rational { base, empty_exponent }
            }
            _ => Int { base, empty_int: false },
        }
    }

    fn maybe_hex_literal(&mut self) -> Option<TokenKind> {
        debug_assert_eq!(self.prev(), 'h');
        let mut chars = self.chars.clone();
        let Some('e') = chars.next() else { return None };
        let Some('x') = chars.next() else { return None };
        let Some(quote @ ('"' | '\'')) = chars.next() else { return None };

        self.ignore::<2>();
        self.bump();
        let terminated = self.eat_string(quote);
        let kind = HexStr { terminated };
        Some(Literal { kind })
    }

    fn maybe_unicode_literal(&mut self) -> Option<TokenKind> {
        debug_assert_eq!(self.prev(), 'u');
        let mut chars = self.chars.clone();
        let Some('n') = chars.next() else { return None };
        let Some('i') = chars.next() else { return None };
        let Some('c') = chars.next() else { return None };
        let Some('o') = chars.next() else { return None };
        let Some('d') = chars.next() else { return None };
        let Some('e') = chars.next() else { return None };
        let Some(quote @ ('"' | '\'')) = chars.next() else { return None };

        self.ignore::<6>();
        self.bump();
        let terminated = self.eat_string(quote);
        let kind = Str { terminated, unicode: true };
        Some(Literal { kind })
    }

    fn eat_string(&mut self, quote: char) -> bool {
        debug_assert_eq!(self.prev(), quote);
        while let Some(c) = self.bump() {
            if c == quote {
                return true;
            }
            let next;
            if c == '\\'
                && ({
                    next = self.first();
                    next
                } == '\\'
                    || next == quote)
            {
                // Bump again to skip escaped character.
                self.bump();
            }
        }
        // End of file reached.
        false
    }

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

    /// Eats the exponent.
    ///
    /// Returns true if at least one digit was met, and returns false otherwise.
    fn eat_exponent(&mut self) -> bool {
        debug_assert!(self.prev() == 'e' || self.prev() == 'E');
        // '+' is not a valid prefix for an exponent
        if self.first() == '-' {
            self.bump();
        }
        self.eat_decimal_digits()
    }

    /// Eats the identifier.
    ///
    /// Note: succeeds on `_`, which isn't a valid identifier.
    fn eat_identifier(&mut self) {
        if !is_id_start(self.first()) {
            return;
        }
        self.bump();

        self.eat_while(is_id_continue);
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

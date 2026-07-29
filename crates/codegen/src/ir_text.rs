//! Common primitives for the MIR and EVM IR text formats.
//!
//! Symbols use Solidity-identifier syntax so the ordinary lexer can tokenize
//! them. Bytes that are not valid at their position are written as `$xx`;
//! literal `$` bytes use the same escape, and `$e` represents an empty symbol.
//! Duplicate MIR function declarations append `$$N` to their mangled symbol.
//! The parser strips the suffix after using it to resolve textual references.
//! MIR text, MIR DOT output, standalone MIR function displays, and EVM IR text
//! all use this spelling.

use alloy_primitives::U256;
use solar_ast::{
    Arena,
    token::{Token, TokenKind, TokenLitKind},
};
use solar_data_structures::fmt;
use solar_interface::{Session, Span, Symbol, source_map::SourceFile};
use solar_parse::PErr;

/// A decoded symbol and its optional textual declaration disambiguator.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TextSymbol {
    pub(crate) symbol: Symbol,
    pub(crate) disambiguator: Option<usize>,
}

impl fmt::Display for TextSymbol {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", display_disambiguated_symbol(self.symbol, self.disambiguator))
    }
}

/// Displays a symbol in the IR text format.
pub(crate) fn display_symbol(symbol: Symbol) -> impl fmt::Display {
    display_disambiguated_symbol(symbol, None)
}

/// Displays a symbol with an optional declaration disambiguator.
pub(crate) fn display_disambiguated_symbol(
    symbol: Symbol,
    disambiguator: Option<usize>,
) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        let bytes = symbol.as_str().as_bytes();
        if bytes.is_empty() {
            write!(f, "$e")?;
        } else {
            for (index, &byte) in bytes.iter().enumerate() {
                let valid = byte.is_ascii_alphabetic()
                    || byte == b'_'
                    || index != 0 && byte.is_ascii_digit();
                if valid {
                    write!(f, "{}", char::from(byte))?;
                } else {
                    write!(f, "${byte:02x}")?;
                }
            }
        }
        if let Some(disambiguator) = disambiguator {
            write!(f, "$${disambiguator}")?;
        }
        Ok(())
    })
}

/// Shared parser primitives for the textual IR parsers.
pub(crate) struct Parser<'sess, 'ast> {
    parser: solar_parse::Parser<'sess, 'ast, 'ast>,
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    pub(crate) fn new(sess: &'sess Session, arena: &'ast Arena, source: &SourceFile) -> Self {
        Self { parser: solar_parse::Parser::from_source_file(sess, arena, source) }
    }

    pub(crate) fn token(&self) -> Token {
        self.parser.token
    }

    pub(crate) fn look_ahead(&self, distance: usize) -> Token {
        self.parser.look_ahead(distance)
    }

    pub(crate) fn bump(&mut self) {
        self.parser.bump();
    }

    pub(crate) fn is_eof(&self) -> bool {
        self.token().kind == TokenKind::Eof
    }

    pub(crate) fn check(&self, kind: TokenKind) -> bool {
        self.token().kind == kind
    }

    pub(crate) fn eat(&mut self, kind: TokenKind) -> bool {
        self.parser.eat(kind)
    }

    pub(crate) fn expect(&mut self, kind: TokenKind) -> Result<(), PErr<'sess>> {
        self.parser.expect(kind).map(drop)
    }

    pub(crate) fn check_keyword(&self, keyword: Symbol) -> bool {
        self.token().is_keyword(keyword)
    }

    pub(crate) fn eat_keyword(&mut self, keyword: Symbol) -> bool {
        if self.check_keyword(keyword) {
            self.bump();
            true
        } else {
            false
        }
    }

    pub(crate) fn expect_keyword(&mut self, keyword: Symbol) -> Result<(), PErr<'sess>> {
        if self.eat_keyword(keyword) {
            Ok(())
        } else {
            Err(self.error(format!("expected `{keyword}`")))
        }
    }

    pub(crate) fn parse_ident(&mut self) -> Result<Symbol, PErr<'sess>> {
        self.parse_ident_opt().ok_or_else(|| self.error("expected identifier"))
    }

    pub(crate) fn parse_ident_opt(&mut self) -> Option<Symbol> {
        let TokenKind::Ident(symbol) = self.token().kind else { return None };
        self.bump();
        Some(symbol)
    }

    pub(crate) fn parse_symbol(&mut self) -> Result<Symbol, PErr<'sess>> {
        let span = self.token().span;
        let symbol = self.parse_text_symbol()?;
        if symbol.disambiguator.is_some() {
            return Err(
                self.error_at(span, "symbol disambiguators are only valid for MIR functions")
            );
        }
        Ok(symbol.symbol)
    }

    pub(crate) fn parse_text_symbol(&mut self) -> Result<TextSymbol, PErr<'sess>> {
        let span = self.token().span;
        let encoded = self.parse_ident()?;
        decode_text_symbol(encoded.as_str())
            .map(|(symbol, disambiguator)| TextSymbol {
                symbol: Symbol::intern(&symbol),
                disambiguator,
            })
            .map_err(|message| self.error_at(span, message))
    }

    pub(crate) fn parse_uint(&mut self) -> Result<U256, PErr<'sess>> {
        let TokenKind::Literal(TokenLitKind::Integer, symbol) = self.token().kind else {
            return Err(self.error("expected integer literal"));
        };
        let text = symbol.as_str();
        let value = if let Some(text) = text.strip_prefix("0x").or_else(|| text.strip_prefix("0X"))
        {
            U256::from_str_radix(text, 16)
        } else {
            text.parse()
        };
        let value = value.map_err(|err| self.error(format!("invalid integer: {err}")))?;
        self.bump();
        Ok(value)
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> PErr<'sess> {
        self.error_at(self.token().span, message)
    }

    pub(crate) fn error_at(&self, span: Span, message: impl Into<String>) -> PErr<'sess> {
        self.parser.dcx().err(message.into()).span(span)
    }
}

fn decode_text_symbol(encoded: &str) -> Result<(String, Option<usize>), &'static str> {
    let (encoded, disambiguator) = if let Some((encoded, suffix)) = encoded.split_once("$$") {
        if suffix.is_empty() || !suffix.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err("invalid symbol disambiguator");
        }
        let disambiguator = suffix.parse().map_err(|_| "symbol disambiguator is out of range")?;
        (encoded, Some(disambiguator))
    } else {
        (encoded, None)
    };

    if encoded == "$e" {
        return Ok((String::new(), disambiguator));
    }
    if encoded.is_empty() {
        return Err("empty symbols must be encoded as `$e`");
    }

    let encoded = encoded.as_bytes();
    let mut decoded = Vec::with_capacity(encoded.len());
    let mut index = 0;
    while index < encoded.len() {
        if encoded[index] != b'$' {
            decoded.push(encoded[index]);
            index += 1;
            continue;
        }
        if index + 2 >= encoded.len() {
            return Err("incomplete symbol escape");
        }
        let high = decode_hex(encoded[index + 1]).ok_or("invalid symbol escape")?;
        let low = decode_hex(encoded[index + 2]).ok_or("invalid symbol escape")?;
        decoded.push(high << 4 | low);
        index += 3;
    }

    String::from_utf8(decoded)
        .map(|decoded| (decoded, disambiguator))
        .map_err(|_| "symbol escape is not valid UTF-8")
}

fn decode_hex(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::ColorChoice;

    #[test]
    fn symbols_round_trip() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            for (raw, encoded) in [
                ("", "$e"),
                ("name", "name"),
                ("123", "$3123"),
                ("a.b$c", "a$2eb$24c"),
                ("é", "$c3$a9"),
            ] {
                let symbol = Symbol::intern(raw);
                assert_eq!(display_symbol(symbol).to_string(), encoded);
                assert_eq!(decode_text_symbol(encoded), Ok((raw.to_string(), None)));
            }
        });
    }

    #[test]
    fn disambiguated_symbols_round_trip() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(|| {
            let symbol = Symbol::intern("over.load");
            let encoded = display_disambiguated_symbol(symbol, Some(42)).to_string();
            assert_eq!(encoded, "over$2eload$$42");
            assert_eq!(decode_text_symbol(&encoded), Ok(("over.load".to_string(), Some(42))));
        });
    }

    #[test]
    fn malformed_symbols_are_rejected() {
        for encoded in ["$", "$x0", "name$$", "name$$x", "$e$$1$$2"] {
            assert!(decode_text_symbol(encoded).is_err(), "{encoded}");
        }
    }
}

use alloy_primitives::U256;
use solar_ast::{
    Arena,
    token::{Token, TokenKind, TokenLitKind},
};
use solar_interface::{Session, Span, Symbol, source_map::SourceFile};
use solar_parse::PErr;

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

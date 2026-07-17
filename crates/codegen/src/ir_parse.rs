use alloy_primitives::U256;
use solar_ast::{
    Arena,
    token::{Token, TokenKind, TokenLitKind},
};
use solar_interface::{BytePos, Session, Span, Symbol, source_map::SourceFile};
use solar_parse::PErr;

/// Shared parser primitives for the textual IR parsers.
pub(crate) struct Parser<'sess, 'ast, 'src> {
    source: &'src SourceFile,
    parser: solar_parse::Parser<'sess, 'ast, 'ast>,
}

impl<'sess, 'ast, 'src> Parser<'sess, 'ast, 'src> {
    pub(crate) fn new(sess: &'sess Session, arena: &'ast Arena, source: &'src SourceFile) -> Self {
        Self { source, parser: solar_parse::Parser::from_source_file(sess, arena, source) }
    }

    pub(crate) fn token(&self) -> Token {
        self.parser.token
    }

    pub(crate) fn prev_token(&self) -> Token {
        self.parser.prev_token
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

    pub(crate) fn span_text(&self, span: Span) -> &'src str {
        let start = (span.lo() - self.source.start_pos).to_usize();
        let end = (span.hi() - self.source.start_pos).to_usize();
        &self.source.src[start..end]
    }

    pub(crate) fn span_from(&self, lo: BytePos) -> Span {
        let hi = self.prev_token().span.hi().max(lo);
        Span::new(lo, hi)
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> PErr<'sess> {
        self.error_at(self.token().span, message)
    }

    pub(crate) fn error_at(&self, span: Span, message: impl Into<String>) -> PErr<'sess> {
        self.parser.dcx().err(message.into()).span(span)
    }

    pub(crate) fn skip_to_eol(&mut self) {
        if self.at_newline() {
            return;
        }
        self.skip_current_line();
    }

    pub(crate) fn skip_current_line(&mut self) {
        let start = self.relative_pos(self.token().span.lo());
        let end = self.source.src[start..]
            .find('\n')
            .map_or(self.source.src.len(), |offset| start + offset);
        while !self.is_eof() && self.relative_pos(self.token().span.lo()) < end {
            self.bump();
        }
    }

    pub(crate) fn at_newline(&self) -> bool {
        if self.prev_token().span.is_dummy() || self.is_eof() {
            return self.is_eof();
        }
        let start = self.relative_pos(self.prev_token().span.hi());
        let end = self.relative_pos(self.token().span.lo());
        self.source.src[start..end].contains(['\n', '\r'])
    }

    fn relative_pos(&self, pos: BytePos) -> usize {
        (pos - self.source.start_pos).to_usize()
    }
}

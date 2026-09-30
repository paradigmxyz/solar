use crate::link::{
    CodeKind, ContractCode, Data, DataId, LibraryId, LibraryRelocation, LibraryTable, QualifiedName,
};
use alloy_primitives::{Bytes, U256};
use solar_ast::{
    Arena,
    token::{BinOpToken, Delimiter, Token, TokenKind, TokenLitKind},
};
use solar_data_structures::index::IndexVec;
use solar_interface::{Session, Span, Symbol, source_map::SourceFile, sym};
use solar_parse::PErr;

/// Shared parser primitives for the textual IR parsers.
pub(crate) struct Parser<'sess, 'ast> {
    pub(crate) libraries: LibraryTable,
    parser: solar_parse::Parser<'sess, 'ast, 'ast>,
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    pub(crate) fn new(sess: &'sess Session, arena: &'ast Arena, source: &SourceFile) -> Self {
        Self {
            parser: solar_parse::Parser::from_source_file(sess, arena, source),
            libraries: LibraryTable::default(),
        }
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

    pub(crate) fn parse_data_id(&mut self) -> Result<(U256, Option<Symbol>), PErr<'sess>> {
        if matches!(self.token().kind, TokenKind::Literal(TokenLitKind::Integer, _)) {
            return self.parse_uint().map(|id| (id, None));
        }

        let span = self.token().span;
        let name = self.parse_ident()?;
        let Some((base, index)) = name.as_str().rsplit_once('_') else {
            return Err(self.error_at(span, format!("invalid data identifier `{name}`")));
        };
        let id = index.parse().map_err(|err| {
            self.error_at(span, format!("invalid data identifier `{name}`: {err}"))
        })?;
        Ok((id, Some(Symbol::intern(base))))
    }

    pub(crate) fn parse_data_ref(&mut self) -> Result<(U256, u32, Span), PErr<'sess>> {
        let id_span = self.token().span;
        let (id, _) = self.parse_data_id()?;
        let mut offset_span = id_span;
        let offset = if self.eat(TokenKind::BinOp(BinOpToken::Plus)) {
            offset_span = self.token().span;
            let value = self.parse_uint()?;
            u32::try_from(value)
                .map_err(|_| self.error_at(offset_span, "data offset exceeds `u32`"))?
        } else {
            0
        };
        Ok((id, offset, offset_span))
    }

    pub(crate) fn parse_data_bytes(&mut self) -> Result<Bytes, PErr<'sess>> {
        let TokenKind::Literal(TokenLitKind::HexStr, bytes) = self.token().kind else {
            return Err(self.error("expected hex string literal"));
        };
        let bytes = alloy_primitives::hex::decode(bytes.as_str())
            .map_err(|err| self.error(format!("invalid data: {err}")))?;
        self.bump();
        Ok(bytes.into())
    }

    /// Parses the entries of an `@libraries` section: `L_0: "a.sol:L"`.
    pub(crate) fn parse_library_declarations(&mut self) -> Result<(), PErr<'sess>> {
        while self.check_declaration() {
            let span = self.token().span;
            let (id, _) = self.parse_indexed_name("library")?;
            if id != self.libraries.len() {
                return Err(self.error_at(
                    span,
                    format!("expected library ID {}, found {id}", self.libraries.len()),
                ));
            }
            self.expect(TokenKind::Colon)?;
            let library = self.parse_qualified_name()?;
            if self.libraries.intern(library).index() != id {
                let message = format!("library `{}` is already declared", library.as_str());
                return Err(self.error_at(span, message));
            }
        }
        Ok(())
    }

    /// Parses the entries of a `@data` section:
    ///
    /// ```text
    /// Child_creation_code_0: creation_code "b.sol:Child"
    /// literal_1: hex"..." library_relocations [2: L_0]
    /// ```
    pub(crate) fn parse_data_declarations(
        &mut self,
    ) -> Result<IndexVec<DataId, Data>, PErr<'sess>> {
        let mut data = IndexVec::new();
        while self.check_declaration() {
            let span = self.token().span;
            let (id, name) = self.parse_data_id()?;
            if id != U256::from(data.len()) {
                let message = format!("expected data ID {}, found {id}", data.len());
                return Err(self.error_at(span, message));
            }
            self.expect(TokenKind::Colon)?;
            let kind = if self.eat_keyword(sym::creation_code) {
                Some(CodeKind::Creation)
            } else if self.eat_keyword(sym::runtime_code) {
                Some(CodeKind::Runtime)
            } else {
                None
            };
            if let Some(kind) = kind {
                let contract = self.parse_qualified_name()?;
                data.push(Data::contract_code(ContractCode { contract, kind }, name));
                continue;
            }
            if !matches!(self.token().kind, TokenKind::Literal(TokenLitKind::HexStr, _)) {
                return Err(self.error("expected `hex\"...\"`, `creation_code`, or `runtime_code`"));
            }
            let bytes = self.parse_data_bytes()?;
            let library_relocations = self.parse_data_library_relocations(&bytes)?;
            data.push(Data { library_relocations, ..Data::new(bytes, name) });
        }
        Ok(data)
    }

    /// Returns whether the next tokens start a declaration: `name_index:` or `index:`.
    ///
    /// EVM IR block labels contain no `_`, so they end a section.
    fn check_declaration(&self) -> bool {
        let is_name = match self.token().kind {
            TokenKind::Ident(name) => name.as_str().contains('_'),
            TokenKind::Literal(TokenLitKind::Integer, _) => true,
            _ => false,
        };
        is_name && self.look_ahead(1).kind == TokenKind::Colon
    }

    /// Parses a reference to a declared library: `Name_index`.
    pub(crate) fn parse_library_ref(&mut self) -> Result<LibraryId, PErr<'sess>> {
        let span = self.token().span;
        let (id, name) = self.parse_indexed_name("library")?;
        if id >= self.libraries.len() {
            return Err(self.error_at(span, format!("unknown library `{name}_{id}`")));
        }
        Ok(LibraryId::new(id))
    }

    /// Parses an `name_index` identifier.
    fn parse_indexed_name(&mut self, what: &str) -> Result<(usize, Symbol), PErr<'sess>> {
        let span = self.token().span;
        let name = self.parse_ident()?;
        let Some((base, index)) = name.as_str().rsplit_once('_') else {
            return Err(self.error_at(span, format!("invalid {what} identifier `{name}`")));
        };
        let id = index.parse().map_err(|err| {
            self.error_at(span, format!("invalid {what} identifier `{name}`: {err}"))
        })?;
        Ok((id, Symbol::intern(base)))
    }

    /// Parses the operands that follow a data reference in a data size: `[, addend[, aligned]]`.
    pub(crate) fn parse_data_size_operands(&mut self) -> Result<(u64, bool), PErr<'sess>> {
        if !self.eat(TokenKind::Comma) {
            return Ok((0, false));
        }
        let value = self.parse_uint()?;
        let addend = u64::try_from(value)
            .map_err(|_| self.error(format!("integer `{value}` does not fit in u64")))?;
        let aligned = self.eat(TokenKind::Comma);
        if aligned {
            self.expect_keyword(sym::aligned)?;
        }
        Ok((addend, aligned))
    }

    /// Parses a fully qualified contract name: `"source:Name"`.
    fn parse_qualified_name(&mut self) -> Result<QualifiedName, PErr<'sess>> {
        if !matches!(self.token().kind, TokenKind::Literal(TokenLitKind::Str, _)) {
            return Err(self.error("expected fully qualified name string"));
        }
        let span = self.token().span;
        let (literal, _) = self.parser.parse_lit(false)?;
        let solar_ast::LitKind::Str(_, value, _) = literal.kind else { unreachable!() };
        let text = std::str::from_utf8(value.as_byte_str())
            .map_err(|_| self.error_at(span, "fully qualified name must be UTF-8"))?;
        QualifiedName::parse(text)
            .ok_or_else(|| self.error_at(span, "expected fully qualified name `source:Name`"))
    }

    /// Parses optional library relocations following a constant-data declaration.
    pub(crate) fn parse_data_library_relocations(
        &mut self,
        bytes: &[u8],
    ) -> Result<Vec<LibraryRelocation>, PErr<'sess>> {
        let mut relocations = Vec::<LibraryRelocation>::new();
        if self.eat_keyword(sym::library_relocations) {
            self.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
            while !self.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                let value = self.parse_uint()?;
                let offset = usize::try_from(value)
                    .map_err(|_| self.error("library offset exceeds `usize`"))?;
                if offset.checked_add(20).is_none_or(|end| end > bytes.len()) {
                    return Err(self.error("library relocation exceeds data size"));
                }
                if relocations.last().is_some_and(|previous| previous.offset + 20 > offset) {
                    return Err(
                        self.error("library relocations must be ordered and non-overlapping")
                    );
                }
                self.expect(TokenKind::Colon)?;
                let library = self.parse_library_ref()?;
                relocations.push(LibraryRelocation { offset, library });
                if !self.eat(TokenKind::Comma) {
                    self.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
                    break;
                }
            }
        }
        Ok(relocations)
    }

    /// Parses the canonical `lo..hi` source-span bounds syntax.
    pub(crate) fn parse_span_bounds(&mut self) -> Result<(u32, u32), PErr<'sess>> {
        if let TokenKind::Literal(TokenLitKind::Rational, symbol) = self.token().kind
            && let Some(lo) = symbol.as_str().strip_suffix('.')
        {
            let lo = lo.parse().map_err(|err| self.error(format!("invalid integer: {err}")))?;
            let lo = self.u256_to_u32(lo)?;
            self.bump();
            let TokenKind::Literal(TokenLitKind::Rational, symbol) = self.token().kind else {
                return Err(self.error("expected span end"));
            };
            let Some(hi) = symbol.as_str().strip_prefix('.') else {
                return Err(self.error("expected span end"));
            };
            let hi = hi.parse().map_err(|err| self.error(format!("invalid integer: {err}")))?;
            let hi = self.u256_to_u32(hi)?;
            self.bump();
            return Ok((lo, hi));
        }

        let lo = self.parse_uint()?;
        let lo = self.u256_to_u32(lo)?;
        self.expect(TokenKind::Dot)?;
        if let TokenKind::Literal(TokenLitKind::Rational, symbol) = self.token().kind
            && let Some(hi) = symbol.as_str().strip_prefix('.')
        {
            let hi = hi.parse().map_err(|err| self.error(format!("invalid integer: {err}")))?;
            let hi = self.u256_to_u32(hi)?;
            self.bump();
            return Ok((lo, hi));
        }
        self.expect(TokenKind::Dot)?;
        let hi = self.parse_uint()?;
        Ok((lo, self.u256_to_u32(hi)?))
    }

    fn u256_to_u32(&self, value: U256) -> Result<u32, PErr<'sess>> {
        value.try_into().map_err(|_| self.error(format!("integer `{value}` does not fit in u32")))
    }

    pub(crate) fn error(&self, message: impl Into<String>) -> PErr<'sess> {
        self.error_at(self.token().span, message)
    }

    pub(crate) fn error_at(&self, span: Span, message: impl Into<String>) -> PErr<'sess> {
        self.parser.dcx().err(message.into()).span(span)
    }
}

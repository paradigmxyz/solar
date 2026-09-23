//! Interactive recovery skips a failed statement or declaration as a whole.
//!
//! The normal parser only records a token cursor. After an error, a separate cursor scans the
//! original tokens, including the consumed prefix, so semicolons in a `for` header cannot become
//! statement boundaries. This scanner recognizes control-statement structure but neither parses
//! expressions nor builds an AST. Ambiguous or mismatched structure falls back to discarding the
//! rest of the enclosing block; losing annotations is preferable to assigning the wrong scope.

use super::{PARSER_RECURSION_LIMIT, Parser};
use smallvec::SmallVec;
use solar_ast::token::{Delimiter, Token, TokenKind};
use solar_interface::{kw, sym};

/// The current token can be supplied by `bump_with`, so retain it separately from the token index.
#[derive(Clone, Copy)]
pub(super) struct RecoveryPoint {
    token: Token,
    next_token_index: usize,
}

impl RecoveryPoint {
    pub(super) fn position(self) -> (usize, bool) {
        // The synthetic EOF has the same index as the final token, but follows that token.
        (self.next_token_index, self.token.is_eof())
    }
}

impl Parser<'_, '_, '_> {
    /// Reset the no-progress guard for a new production while remembering an already diagnosed
    /// boundary. Missing semicolons or braces at that boundary need no cascading diagnostic.
    pub(super) fn finish_recovery(&mut self) {
        self.last_recovered_token_span = Some(self.token.span);
        self.last_unexpected_token_span = None;
        self.expected_tokens.clear();
    }

    #[inline]
    pub(super) fn recovery_point(&self) -> RecoveryPoint {
        RecoveryPoint { token: self.token, next_token_index: self.next_token_index }
    }

    pub(super) fn is_recovery_item_start(&self) -> bool {
        is_item_start(self.token, |distance| self.look_ahead(distance))
    }

    #[cold]
    pub(super) fn recover_statement(&mut self, start: RecoveryPoint) {
        self.recover(start, false);
    }

    #[cold]
    pub(super) fn recover_item(&mut self, start: RecoveryPoint) {
        self.recover(start, true);
    }

    fn recover(&mut self, start: RecoveryPoint, item: bool) {
        debug_assert!(self.recover_incomplete_input);
        let current = self.recovery_point();
        let mut scanner = Scanner { tokens: &self.tokens, point: start };
        let complete = if item { scanner.item() } else { scanner.statement(0) };
        if !complete || scanner.point.position() < current.position() {
            // A nested parser may already have recovered past the boundary found by the scanner.
            // Never rewind or resume in the middle of a construct in that case.
            scanner.point = start;
            scanner.enclosing_boundary(current);
        }
        let end = scanner.point.position();

        self.finish_recovery();
        while self.recovery_point().position() < end && !self.token.is_eof() {
            self.bump();
        }
    }
}

struct Scanner<'a> {
    tokens: &'a [Token],
    point: RecoveryPoint,
}

impl Scanner<'_> {
    fn bump(&mut self) {
        loop {
            let Some(&token) = self.tokens.get(self.point.next_token_index) else {
                self.point.token = Token::new(TokenKind::Eof, self.point.token.span);
                return;
            };
            self.point.next_token_index += 1;
            self.point.token = token;
            if !token.is_comment_or_doc() {
                return;
            }
        }
    }

    fn look_ahead(&self, distance: usize) -> Token {
        self.tokens[self.point.next_token_index..]
            .iter()
            .copied()
            .filter(|token| !token.is_comment_or_doc())
            .nth(distance - 1)
            .unwrap_or(Token::EOF)
    }

    fn is_item_start(&self) -> bool {
        is_item_start(self.point.token, |distance| self.look_ahead(distance))
    }

    fn eat(&mut self, kind: TokenKind) -> bool {
        if self.point.token.kind != kind {
            return false;
        }
        self.bump();
        true
    }

    /// Scan balanced delimiters iteratively, rejecting mismatches instead of guessing a scope.
    fn delimited(&mut self, delimiter: Delimiter) -> bool {
        if self.point.token.kind != TokenKind::OpenDelim(delimiter) {
            return false;
        }
        let mut delimiters = SmallVec::<[Delimiter; 8]>::new();
        loop {
            match self.point.token.kind {
                TokenKind::OpenDelim(delimiter) => {
                    if delimiters.len() == PARSER_RECURSION_LIMIT {
                        return false;
                    }
                    delimiters.push(delimiter);
                }
                TokenKind::CloseDelim(delimiter) => {
                    if delimiters.pop() != Some(delimiter) {
                        return false;
                    }
                    self.bump();
                    if delimiters.is_empty() {
                        return true;
                    }
                    continue;
                }
                TokenKind::Eof => return false,
                _ => {}
            }
            self.bump();
        }
    }

    fn statement(&mut self, depth: usize) -> bool {
        if depth == PARSER_RECURSION_LIMIT || self.is_item_start() {
            return false;
        }
        match self.point.token.kind {
            TokenKind::OpenDelim(Delimiter::Brace) => self.delimited(Delimiter::Brace),
            TokenKind::Ident(kw::If) => {
                self.bump();
                self.delimited(Delimiter::Parenthesis)
                    && self.statement(depth + 1)
                    && (!self.eat(TokenKind::Ident(kw::Else)) || self.statement(depth + 1))
            }
            TokenKind::Ident(kw::For | kw::While) => {
                self.bump();
                self.delimited(Delimiter::Parenthesis) && self.statement(depth + 1)
            }
            TokenKind::Ident(kw::Do) => {
                self.bump();
                if !self.statement(depth + 1)
                    || !self.eat(TokenKind::Ident(kw::While))
                    || !self.delimited(Delimiter::Parenthesis)
                {
                    return false;
                }
                // A missing trailing semicolon cannot extend a completed do-while statement.
                self.eat(TokenKind::Semi);
                true
            }
            TokenKind::Ident(kw::Try) => self.try_statement(),
            TokenKind::Ident(kw::Unchecked) => {
                self.bump();
                self.delimited(Delimiter::Brace)
            }
            TokenKind::Ident(kw::Assembly) => self.sequence(true),
            TokenKind::Ident(kw::Else | kw::Catch) | TokenKind::CloseDelim(_) | TokenKind::Eof => {
                false
            }
            _ => self.sequence(false),
        }
    }

    fn item(&mut self) -> bool {
        if self.point.token.is_keyword(kw::Abstract) && self.look_ahead(1).is_keyword(kw::Contract)
        {
            self.bump();
        }
        let body = match self.point.token.kind {
            TokenKind::Ident(kw::Function) => {
                !self.look_ahead(1).is_open_delim(Delimiter::Parenthesis)
            }
            TokenKind::Ident(
                kw::Modifier
                | kw::Constructor
                | kw::Fallback
                | kw::Receive
                | kw::Contract
                | kw::Interface
                | kw::Library
                | kw::Struct
                | kw::Enum,
            ) => true,
            _ => false,
        };
        self.sequence(body)
    }

    /// Only declarations and assembly end at a body brace. Expression call options, named
    /// arguments, imports, and using lists continue through their braces to a semicolon.
    fn sequence(&mut self, body: bool) -> bool {
        let mut first = true;
        loop {
            if !first
                && (self.is_item_start()
                    || matches!(self.point.token.kind, TokenKind::Ident(kw::Else | kw::Catch)))
            {
                return true;
            }
            first = false;
            match self.point.token.kind {
                TokenKind::Semi => {
                    self.bump();
                    return true;
                }
                TokenKind::CloseDelim(Delimiter::Brace) | TokenKind::Eof => return true,
                TokenKind::OpenDelim(delimiter) => {
                    if !self.delimited(delimiter) {
                        return false;
                    }
                    if body && delimiter == Delimiter::Brace {
                        return true;
                    }
                }
                TokenKind::CloseDelim(_) => return false,
                _ => self.bump(),
            }
        }
    }

    fn try_statement(&mut self) -> bool {
        self.bump(); // `try`
        loop {
            match self.point.token.kind {
                TokenKind::OpenDelim(Delimiter::Brace) => {
                    // Match the expression parser's distinction between call options and a body.
                    let call_options = self.look_ahead(1).is_ident()
                        && self.look_ahead(2).kind == TokenKind::Colon;
                    if !self.delimited(Delimiter::Brace) {
                        return false;
                    }
                    if !call_options {
                        break;
                    }
                }
                TokenKind::OpenDelim(delimiter) => {
                    if !self.delimited(delimiter) {
                        return false;
                    }
                }
                TokenKind::Semi | TokenKind::CloseDelim(_) | TokenKind::Eof => return false,
                _ if self.is_item_start() => return false,
                _ => self.bump(),
            }
        }
        while self.eat(TokenKind::Ident(kw::Catch)) {
            if self.point.token.is_ident() {
                self.bump();
            }
            if self.point.token.is_open_delim(Delimiter::Parenthesis)
                && !self.delimited(Delimiter::Parenthesis)
            {
                return false;
            }
            if !self.delimited(Delimiter::Brace) {
                return false;
            }
        }
        true
    }

    /// An uncertain construct owns the remaining statements of its block. Ignore unmatched
    /// parentheses and brackets here so they cannot consume the enclosing `}` or the next item.
    fn enclosing_boundary(&mut self, current: RecoveryPoint) {
        let start = self.point.position();
        let mut braces = 0usize;
        while !self.point.token.is_eof() {
            if braces == 0
                && self.point.position() >= current.position()
                && (self.point.token.is_close_delim(Delimiter::Brace)
                    || (self.point.position() != start && self.is_item_start()))
            {
                break;
            }
            match self.point.token.kind {
                TokenKind::OpenDelim(Delimiter::Brace) => braces += 1,
                TokenKind::CloseDelim(Delimiter::Brace) => braces = braces.saturating_sub(1),
                _ => {}
            }
            self.bump();
        }
    }
}

fn is_item_start(token: Token, look_ahead: impl Fn(usize) -> Token) -> bool {
    match token.kind {
        TokenKind::Ident(kw::Function | kw::Type) => look_ahead(1).is_ident(),
        TokenKind::Ident(kw::Abstract) => look_ahead(1).is_keyword(kw::Contract),
        // `error` is contextual: ordinary variables and calls with that name remain statements.
        TokenKind::Ident(sym::error) => {
            look_ahead(1).is_ident() && look_ahead(2).is_open_delim(Delimiter::Parenthesis)
        }
        TokenKind::Ident(
            kw::Modifier
            | kw::Constructor
            | kw::Fallback
            | kw::Receive
            | kw::Contract
            | kw::Interface
            | kw::Library
            | kw::Struct
            | kw::Enum
            | kw::Event
            | kw::Import
            | kw::Pragma
            | kw::Using,
        ) => true,
        _ => false,
    }
}

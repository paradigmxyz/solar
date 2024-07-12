use crate::{Lexer, PErr, PResult};
use bumpalo::{boxed::Box, Bump};
use std::fmt::{self, Write};
use sulk_ast::{
    ast::{DocComment, DocComments, Path},
    token::{Delimiter, Token, TokenKind},
};
use sulk_interface::{
    diagnostics::DiagCtxt,
    source_map::{FileName, SourceFile},
    Ident, Result, Session, Span, Symbol,
};

mod bump_ext;
use bump_ext::BumpExt;

mod expr;
mod item;
mod lit;
mod stmt;
mod ty;
mod yul;

/// Solidity parser.
pub struct Parser<'sess, 'ast> {
    /// The parser session.
    pub sess: &'sess Session,
    /// The arena where the AST nodes are allocated.
    pub arena: &'ast Bump,

    /// The current token.
    pub token: Token,
    /// The previous token.
    pub prev_token: Token,
    /// List of expected tokens. Cleared after each `bump` call.
    expected_tokens: Vec<ExpectedToken>,
    /// The span of the last unexpected token.
    last_unexpected_token_span: Option<Span>,

    /// Whether the parser is in Yul mode.
    ///
    /// Currently, this can only happen when parsing a Yul "assembly" block.
    in_yul: bool,
    /// Whether the parser is currently parsing a contract block.
    in_contract: bool,

    /// The token stream.
    tokens: std::vec::IntoIter<Token>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExpectedToken {
    Token(TokenKind),
    Keyword(Symbol),
    Lit,
    StrLit,
    VersionNumber,
    Ident,
    Path,
    ElementaryType,
}

impl fmt::Display for ExpectedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Token(t) => return write!(f, "`{t}`"),
            Self::Keyword(kw) => return write!(f, "`{kw}`"),
            Self::StrLit => "string literal",
            Self::VersionNumber => "`*`, `X`, `x`, decimal integer literal",
            Self::Lit => "literal",
            Self::Ident => "identifier",
            Self::Path => "path",
            Self::ElementaryType => "elementary type name",
        })
    }
}

impl ExpectedToken {
    fn to_string_many(tokens: &[Self]) -> String {
        or_list(tokens)
    }

    fn eq_kind(&self, other: &TokenKind) -> bool {
        match self {
            Self::Token(kind) => kind == other,
            _ => false,
        }
    }
}

/// A sequence separator.
struct SeqSep {
    /// The separator token.
    sep: Option<TokenKind>,
    /// `true` if a trailing separator is allowed.
    trailing_sep_allowed: bool,
    /// `true` if a trailing separator is required.
    trailing_sep_required: bool,
}

impl SeqSep {
    fn trailing_enforced(t: TokenKind) -> Self {
        Self { sep: Some(t), trailing_sep_required: true, trailing_sep_allowed: true }
    }

    #[allow(dead_code)]
    fn trailing_allowed(t: TokenKind) -> Self {
        Self { sep: Some(t), trailing_sep_required: false, trailing_sep_allowed: true }
    }

    #[allow(dead_code)]
    fn trailing_disallowed(t: TokenKind) -> Self {
        Self { sep: Some(t), trailing_sep_required: false, trailing_sep_allowed: false }
    }

    fn none() -> Self {
        Self { sep: None, trailing_sep_required: false, trailing_sep_allowed: false }
    }
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Creates a new parser.
    ///
    /// # Panics
    ///
    /// Panics if any of the tokens are comments.
    pub fn new(sess: &'sess Session, arena: &'ast Bump, tokens: Vec<Token>) -> Self {
        debug_assert!(
            tokens.iter().all(|t| !t.is_comment()),
            "comments should be stripped before parsing"
        );
        let mut parser = Self {
            sess,
            arena,
            token: Token::DUMMY,
            prev_token: Token::DUMMY,
            expected_tokens: Vec::new(),
            last_unexpected_token_span: None,
            in_yul: false,
            in_contract: false,
            tokens: tokens.into_iter(),
        };
        parser.bump();
        parser
    }

    /// Creates a new parser from a source code string.
    pub fn from_source_code(
        sess: &'sess Session,
        arena: &'ast Bump,
        filename: FileName,
        src: String,
    ) -> Result<Self> {
        Self::from_lazy_source_code(sess, arena, filename, || Ok(src))
    }

    /// Creates a new parser from a source code closure.
    pub fn from_lazy_source_code(
        sess: &'sess Session,
        arena: &'ast Bump,
        filename: FileName,
        get_src: impl FnOnce() -> std::io::Result<String>,
    ) -> Result<Self> {
        let file = sess
            .source_map()
            .new_source_file(filename, get_src)
            .map_err(|e| sess.dcx.err(e.to_string()).emit())?;
        Ok(Self::from_source_file(sess, arena, &file))
    }

    /// Creates a new parser from a source file.
    pub fn from_source_file(sess: &'sess Session, arena: &'ast Bump, file: &SourceFile) -> Self {
        Self::from_lexer(arena, Lexer::from_source_file(sess, file))
    }

    /// Creates a new parser from a lexer.
    pub fn from_lexer(arena: &'ast Bump, lexer: Lexer<'sess, '_>) -> Self {
        Self::new(lexer.sess, arena, lexer.into_tokens())
    }

    /// Returns the diagnostic context.
    #[inline]
    pub fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    /// Allocates an object on the AST arena.
    pub(crate) fn alloc<T>(&self, value: T) -> Box<'ast, T> {
        Box::new_in(value, self.arena)
    }

    /// Allocates a list of objects on the AST arena.
    pub(crate) fn alloc_vec<T>(&self, values: Vec<T>) -> Box<'ast, [T]> {
        unsafe { Box::from_raw(self.arena.alloc_vec(values)) }
    }

    /// Returns an "unexpected token" error in a [`PResult`] for the current token.
    #[inline]
    #[track_caller]
    pub fn unexpected<T>(&mut self) -> PResult<'sess, T> {
        Err(self.unexpected_error())
    }

    /// Returns an "unexpected token" error for the current token.
    #[inline]
    #[track_caller]
    pub fn unexpected_error(&mut self) -> PErr<'sess> {
        #[cold]
        #[inline(never)]
        #[track_caller]
        fn unexpected_ok(b: bool) -> ! {
            unreachable!("`unexpected()` returned Ok({b})")
        }
        match self.expect_one_of(&[], &[]) {
            Ok(b) => unexpected_ok(b),
            Err(e) => e,
        }
    }

    /// Expects and consumes the token `t`. Signals an error if the next token is not `t`.
    #[track_caller]
    pub fn expect(&mut self, tok: &TokenKind) -> PResult<'sess, bool /* recovered */> {
        if self.expected_tokens.is_empty() {
            if self.check_noexpect(tok) {
                self.bump();
                Ok(false)
            } else {
                Err(self.unexpected_error_with(tok))
            }
        } else {
            self.expect_one_of(std::slice::from_ref(tok), &[])
        }
    }

    /// Creates a [`PErr`] for an unexpected token `t`.
    #[track_caller]
    fn unexpected_error_with(&mut self, t: &TokenKind) -> PErr<'sess> {
        let prev_span = if self.prev_token.span.is_dummy() {
            // We don't want to point at the following span after a dummy span.
            // This happens when the parser finds an empty token stream.
            self.token.span
        } else if self.token.is_eof() {
            // EOF, don't want to point at the following char, but rather the last token.
            self.prev_token.span
        } else {
            self.prev_token.span.shrink_to_hi()
        };
        let span = self.token.span;

        let this_token_str = self.token.full_description();
        let label_exp = format!("expected `{t}`");
        let msg = format!("{label_exp}, found {this_token_str}");
        let mut err = self.dcx().err(msg).span(span);
        if !self.sess.source_map().is_multiline(prev_span.until(span)) {
            // When the spans are in the same line, it means that the only content
            // between them is whitespace, point only at the found token.
            err = err.span_label(span, label_exp);
        } else {
            err = err.span_label(prev_span, label_exp);
            err = err.span_label(span, "unexpected token");
        }
        err
    }

    /// Expect next token to be edible or inedible token. If edible,
    /// then consume it; if inedible, then return without consuming
    /// anything. Signal a fatal error if next token is unexpected.
    #[track_caller]
    pub fn expect_one_of(
        &mut self,
        edible: &[TokenKind],
        inedible: &[TokenKind],
    ) -> PResult<'sess, bool /* recovered */> {
        if edible.contains(&self.token.kind) {
            self.bump();
            Ok(false)
        } else if inedible.contains(&self.token.kind) {
            // leave it in the input
            Ok(false)
        } else if self.token.kind != TokenKind::Eof
            && self.last_unexpected_token_span == Some(self.token.span)
        {
            panic!("called unexpected twice on the same token");
        } else {
            self.expected_one_of_not_found(edible, inedible)
        }
    }

    #[track_caller]
    fn expected_one_of_not_found(
        &mut self,
        edible: &[TokenKind],
        inedible: &[TokenKind],
    ) -> PResult<'sess, bool> {
        let mut expected = edible
            .iter()
            .chain(inedible)
            .cloned()
            .map(ExpectedToken::Token)
            .chain(self.expected_tokens.iter().cloned())
            .filter(|token| {
                // Filter out suggestions that suggest the same token
                // which was found and deemed incorrect.
                fn is_ident_eq_keyword(found: &TokenKind, expected: &ExpectedToken) -> bool {
                    if let TokenKind::Ident(current_sym) = found {
                        if let ExpectedToken::Keyword(suggested_sym) = expected {
                            return current_sym == suggested_sym;
                        }
                    }
                    false
                }

                if !token.eq_kind(&self.token.kind) {
                    let eq = is_ident_eq_keyword(&self.token.kind, token);
                    // If the suggestion is a keyword and the found token is an ident,
                    // the content of which are equal to the suggestion's content,
                    // we can remove that suggestion (see the `return false` below).

                    // If this isn't the case however, and the suggestion is a token the
                    // content of which is the same as the found token's, we remove it as well.
                    if !eq {
                        if let ExpectedToken::Token(kind) = &token {
                            if kind == &self.token.kind {
                                return false;
                            }
                        }
                        return true;
                    }
                }
                false
            })
            .collect::<Vec<_>>();
        expected.sort_by_cached_key(ToString::to_string);
        expected.dedup();

        let expect = ExpectedToken::to_string_many(&expected);
        let actual = self.token.full_description();
        let (msg_exp, (mut label_span, label_exp)) = match expected.len() {
            0 => (
                format!("unexpected token: {actual}"),
                (self.prev_token.span, "unexpected token after this".to_string()),
            ),
            1 => (
                format!("expected {expect}, found {actual}"),
                (self.prev_token.span.shrink_to_hi(), format!("expected {expect}")),
            ),
            len => {
                let fmt = format!("expected one of {expect}, found {actual}");
                let short_expect = if len > 6 { format!("{len} possible tokens") } else { expect };
                let s = self.prev_token.span.shrink_to_hi();
                (fmt, (s, format!("expected one of {short_expect}")))
            }
        };
        if self.token.is_eof() {
            // This is EOF; don't want to point at the following char, but rather the last token.
            label_span = self.prev_token.span;
        };

        self.last_unexpected_token_span = Some(self.token.span);
        let mut err = self.dcx().err(msg_exp).span(self.token.span);

        if self.prev_token.span.is_dummy()
            || !self
                .sess
                .source_map()
                .is_multiline(self.token.span.shrink_to_hi().until(label_span.shrink_to_lo()))
        {
            // When the spans are in the same line, it means that the only content between
            // them is whitespace, point at the found token in that case.
            err = err.span_label(self.token.span, label_exp);
        } else {
            err = err.span_label(label_span, label_exp);
            err = err.span_label(self.token.span, "unexpected token");
        }

        Err(err)
    }

    /// Expects and consumes a semicolon.
    #[track_caller]
    fn expect_semi(&mut self) -> PResult<'sess, ()> {
        self.expect(&TokenKind::Semi).map(drop)
    }

    /// Checks if the next token is `tok`, and returns `true` if so.
    ///
    /// This method will automatically add `tok` to `expected_tokens` if `tok` is not
    /// encountered.
    fn check(&mut self, tok: &TokenKind) -> bool {
        let is_present = self.check_noexpect(tok);
        if !is_present {
            self.expected_tokens.push(ExpectedToken::Token(tok.clone()));
        }
        is_present
    }

    fn check_noexpect(&self, tok: &TokenKind) -> bool {
        self.token.kind == *tok
    }

    /// Consumes a token 'tok' if it exists. Returns whether the given token was present.
    ///
    /// the main purpose of this function is to reduce the cluttering of the suggestions list
    /// which using the normal eat method could introduce in some cases.
    pub fn eat_noexpect(&mut self, tok: &TokenKind) -> bool {
        let is_present = self.check_noexpect(tok);
        if is_present {
            self.bump()
        }
        is_present
    }

    /// Consumes a token 'tok' if it exists. Returns whether the given token was present.
    pub fn eat(&mut self, tok: &TokenKind) -> bool {
        let is_present = self.check(tok);
        if is_present {
            self.bump()
        }
        is_present
    }

    /// If the next token is the given keyword, returns `true` without eating it.
    /// An expectation is also added for diagnostics purposes.
    fn check_keyword(&mut self, kw: Symbol) -> bool {
        self.expected_tokens.push(ExpectedToken::Keyword(kw));
        self.token.is_keyword(kw)
    }

    /// If the next token is the given keyword, eats it and returns `true`.
    /// Otherwise, returns `false`. An expectation is also added for diagnostics purposes.
    pub fn eat_keyword(&mut self, kw: Symbol) -> bool {
        if self.check_keyword(kw) {
            self.bump();
            true
        } else {
            false
        }
    }

    /// If the given word is not a keyword, signals an error.
    /// If the next token is not the given word, signals an error.
    /// Otherwise, eats it.
    fn expect_keyword(&mut self, kw: Symbol) -> PResult<'sess, ()> {
        if !self.eat_keyword(kw) {
            self.unexpected()
        } else {
            Ok(())
        }
    }

    fn check_ident(&mut self) -> bool {
        self.check_or_expected(self.token.is_ident(), ExpectedToken::Ident)
    }

    fn check_nr_ident(&mut self) -> bool {
        self.check_or_expected(self.token.is_non_reserved_ident(self.in_yul), ExpectedToken::Ident)
    }

    fn check_path(&mut self) -> bool {
        self.check_or_expected(self.token.is_ident(), ExpectedToken::Path)
    }

    fn check_lit(&mut self) -> bool {
        self.check_or_expected(self.token.is_lit(), ExpectedToken::Lit)
    }

    fn check_str_lit(&mut self) -> bool {
        self.check_or_expected(self.token.is_str_lit(), ExpectedToken::StrLit)
    }

    fn check_elementary_type(&mut self) -> bool {
        self.check_or_expected(self.token.is_elementary_type(), ExpectedToken::ElementaryType)
    }

    fn check_or_expected(&mut self, ok: bool, t: ExpectedToken) -> bool {
        if !ok {
            self.expected_tokens.push(t);
        }
        ok
    }

    /// Parses a comma-separated sequence delimited by parentheses (e.g. `(x, y)`).
    /// The function `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_paren_comma_seq<T>(
        &mut self,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */)> {
        self.parse_delim_comma_seq(Delimiter::Parenthesis, allow_empty, f)
    }

    /// Parses a comma-separated sequence, including both delimiters.
    /// The function `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_delim_comma_seq<T>(
        &mut self,
        delim: Delimiter,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */)> {
        self.parse_delim_seq(delim, SeqSep::trailing_disallowed(TokenKind::Comma), allow_empty, f)
    }

    /// Parses a comma-separated sequence.
    /// The function `f` must consume tokens until reaching the next separator.
    #[track_caller]
    #[inline]
    fn parse_nodelim_comma_seq<T>(
        &mut self,
        stop: &TokenKind,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */)> {
        self.parse_seq_to_before_end(
            stop,
            SeqSep::trailing_disallowed(TokenKind::Comma),
            allow_empty,
            f,
        )
        .map(|(v, trailing, _recovered)| (v, trailing))
    }

    /// Parses a `sep`-separated sequence, including both delimiters.
    /// The function `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_delim_seq<T>(
        &mut self,
        delim: Delimiter,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */)> {
        self.parse_unspanned_seq(
            &TokenKind::OpenDelim(delim),
            &TokenKind::CloseDelim(delim),
            sep,
            allow_empty,
            f,
        )
    }

    /// Parses a sequence, including both delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_unspanned_seq<T>(
        &mut self,
        bra: &TokenKind,
        ket: &TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */)> {
        self.expect(bra)?;
        self.parse_seq_to_end(ket, sep, allow_empty, f)
    }

    /// Parses a sequence, including only the closing delimiter. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_seq_to_end<T>(
        &mut self,
        ket: &TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */)> {
        let (val, trailing, recovered) = self.parse_seq_to_before_end(ket, sep, allow_empty, f)?;
        if !recovered {
            self.expect(ket)?;
        }
        Ok((val, trailing))
    }

    /// Parses a sequence, not including the delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_seq_to_before_end<T>(
        &mut self,
        ket: &TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */, bool /* recovered */)> {
        self.parse_seq_to_before_tokens(&[ket], sep, allow_empty, f)
    }

    /// Checks if the next token is contained within `kets`, and returns `true` if so.
    fn expect_any(&mut self, kets: &[&TokenKind]) -> bool {
        kets.iter().any(|k| self.check(k))
    }

    /// Parses a sequence until the specified delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    fn parse_seq_to_before_tokens<T>(
        &mut self,
        kets: &[&TokenKind],
        sep: SeqSep,
        allow_empty: bool,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Box<'ast, [T]>, bool /* trailing */, bool /* recovered */)> {
        let mut first = true;
        let mut recovered = false;
        let mut trailing = false;
        let mut v = Vec::new();

        if !allow_empty {
            v.push(f(self)?);
            first = false;
        }

        while !self.expect_any(kets) {
            if let TokenKind::CloseDelim(..) | TokenKind::Eof = self.token.kind {
                break;
            }

            if let Some(tk) = &sep.sep {
                if first {
                    // no separator for the first element
                    first = false;
                } else {
                    // check for separator
                    match self.expect(tk) {
                        Ok(recovered_) => {
                            if recovered_ {
                                recovered = true;
                                break;
                            }
                        }
                        Err(e) => return Err(e),
                    }
                }
            }

            if sep.trailing_sep_allowed && self.expect_any(kets) {
                trailing = true;
                break;
            }

            let t = f(self)?;
            v.push(t);
        }

        if sep.trailing_sep_required && !trailing {
            if let Some(tk) = &sep.sep {
                self.expect(tk)?;
            }
        }

        Ok((self.alloc_vec(v), trailing, recovered))
    }

    /// Advance the parser by one token.
    pub fn bump(&mut self) {
        let mut next = self.tokens.next().unwrap_or(Token::EOF);
        if next.span.is_dummy() {
            // Tweak the location for better diagnostics.
            next.span = self.token.span;
        }
        self.inlined_bump_with(next);
    }

    /// Advance the parser by one token using provided token as the next one.
    pub fn bump_with(&mut self, next: Token) {
        self.inlined_bump_with(next);
    }

    /// This always-inlined version should only be used on hot code paths.
    #[inline(always)]
    fn inlined_bump_with(&mut self, next_token: Token) {
        self.prev_token = std::mem::replace(&mut self.token, next_token);
        self.expected_tokens.clear();
    }

    /// Returns the token `dist` tokens ahead of the current one.
    ///
    /// [`Eof`](Token::EOF) will be returned if the look-ahead is any distance past the end of the
    /// tokens.
    #[inline]
    pub fn look_ahead(&self, dist: usize) -> &Token {
        if dist == 0 {
            &self.token
        } else {
            self.tokens.as_slice().get(dist - 1).unwrap_or(&Token::EOF)
        }
    }

    /// Calls `f` with the token `dist` tokens ahead of the current one.
    ///
    /// See [`look_ahead`](Self::look_ahead) for more information.
    #[inline]
    pub fn look_ahead_with<R>(&self, dist: usize, f: impl FnOnce(&Token) -> R) -> R {
        f(self.look_ahead(dist))
    }

    /// Runs `f` with the parser in a contract context.
    fn in_contract<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = std::mem::replace(&mut self.in_contract, true);
        let res = f(self);
        self.in_contract = old;
        res
    }

    /// Runs `f` with the parser in a Yul context.
    fn in_yul<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = std::mem::replace(&mut self.in_yul, true);
        let res = f(self);
        self.in_yul = old;
        res
    }
}

/// Common parsing methods.
impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Provides a spanned parser.
    #[track_caller]
    pub fn parse_spanned<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (Span, T)> {
        let lo = self.token.span;
        let res = f(self);
        let span = lo.to(self.prev_token.span);
        match res {
            Ok(t) => Ok((span, t)),
            Err(e) if e.span.is_dummy() => Err(e.span(span)),
            Err(e) => Err(e),
        }
    }

    /// Parses contiguous doc comments. Can be empty.
    pub fn parse_doc_comments(&mut self) -> PResult<'sess, DocComments<'ast>> {
        let mut doc_comments = Vec::new();
        while let Token { span, kind: TokenKind::Comment(is_doc, kind, symbol) } = self.token {
            if !is_doc {
                self.dcx().bug("comments should not be in the token stream").span(span).emit();
            }
            doc_comments.push(DocComment { kind, span, symbol });
            self.bump();
        }
        Ok(self.alloc_vec(doc_comments))
    }

    /// Parses a qualified identifier: `foo.bar.baz`.
    #[track_caller]
    pub fn parse_path(&mut self) -> PResult<'sess, Path> {
        let first = self.parse_ident()?;
        self.parse_path_with(first)
    }

    /// Parses a qualified identifier starting with the given identifier.
    #[track_caller]
    pub fn parse_path_with(&mut self, first: Ident) -> PResult<'sess, Path> {
        if self.in_yul {
            self.parse_path_with_f(first, Self::parse_yul_path_ident)
        } else {
            self.parse_path_with_f(first, Self::parse_ident)
        }
    }

    /// Parses either an identifier or a Yul EVM builtin.
    fn parse_yul_path_ident(&mut self) -> PResult<'sess, Ident> {
        let ident = self.ident_or_err(true)?;
        if !ident.is_yul_evm_builtin() && ident.is_reserved(true) {
            self.expected_ident_found_err().emit();
        }
        self.bump();
        Ok(ident)
    }

    /// Parses a qualified identifier: `foo.bar.baz`.
    #[track_caller]
    pub fn parse_path_any(&mut self) -> PResult<'sess, Path> {
        let first = self.parse_ident_any()?;
        self.parse_path_with_f(first, Self::parse_ident_any)
    }

    /// Parses a qualified identifier starting with the given identifier.
    #[track_caller]
    fn parse_path_with_f(
        &mut self,
        first: Ident,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, Ident>,
    ) -> PResult<'sess, Path> {
        if !self.check_noexpect(&TokenKind::Dot) {
            return Ok(Path::new_single(first));
        }

        let mut path = Vec::with_capacity(2);
        path.push(first);
        while self.eat(&TokenKind::Dot) {
            path.push(f(self)?);
        }
        Ok(Path::new(path))
    }

    /// Parses an identifier.
    #[track_caller]
    pub fn parse_ident(&mut self) -> PResult<'sess, Ident> {
        self.parse_ident_common(true)
    }

    /// Parses an identifier. Does not check if the identifier is a reserved keyword.
    #[track_caller]
    pub fn parse_ident_any(&mut self) -> PResult<'sess, Ident> {
        let ident = self.ident_or_err(true)?;
        self.bump();
        Ok(ident)
    }

    /// Parses an optional identifier.
    #[track_caller]
    pub fn parse_ident_opt(&mut self) -> PResult<'sess, Option<Ident>> {
        if self.check_ident() {
            self.parse_ident().map(Some)
        } else {
            Ok(None)
        }
    }

    #[track_caller]
    fn parse_ident_common(&mut self, recover: bool) -> PResult<'sess, Ident> {
        let ident = self.ident_or_err(recover)?;
        if ident.is_reserved(self.in_yul) {
            let err = self.expected_ident_found_err();
            if recover {
                err.emit();
            } else {
                return Err(err);
            }
        }
        self.bump();
        Ok(ident)
    }

    /// Returns Ok if the current token is an identifier. Does not advance the parser.
    #[track_caller]
    fn ident_or_err(&mut self, recover: bool) -> PResult<'sess, Ident> {
        match self.token.ident() {
            Some(ident) => Ok(ident),
            None => self.expected_ident_found(recover),
        }
    }

    #[track_caller]
    fn expected_ident_found(&mut self, recover: bool) -> PResult<'sess, Ident> {
        self.expected_ident_found_other(self.token.clone(), recover)
    }

    #[track_caller]
    fn expected_ident_found_other(&mut self, token: Token, recover: bool) -> PResult<'sess, Ident> {
        let msg = format!("expected identifier, found {}", token.full_description());
        let span = token.span;
        let mut err = self.dcx().err(msg).span(span);

        let mut recovered_ident = None;

        let suggest_remove_comma = token.kind == TokenKind::Comma && self.look_ahead(1).is_ident();
        if suggest_remove_comma {
            if recover {
                self.bump();
                recovered_ident = self.ident_or_err(false).ok();
            }
            err = err.span_help(span, "remove this comma");
        }

        if recover {
            if let Some(ident) = recovered_ident {
                err.emit();
                return Ok(ident);
            }
        }
        Err(err)
    }

    #[track_caller]
    fn expected_ident_found_err(&mut self) -> PErr<'sess> {
        self.expected_ident_found(false).unwrap_err()
    }
}

fn or_list<T: fmt::Display>(list: &[T]) -> String {
    let len = list.len();
    let mut s = String::with_capacity(16 * len);
    for (i, t) in list.iter().enumerate() {
        if i > 0 {
            let is_last = i == len - 1;
            s.push_str(if len > 2 && is_last {
                ", or "
            } else if len == 2 && is_last {
                " or "
            } else {
                ", "
            });
        }
        let _ = write!(s, "{t}");
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_or_list() {
        let tests: &[(&[&str], &str)] = &[
            (&[], ""),
            (&["`<eof>`"], "`<eof>`"),
            (&["integer", "identifier"], "integer or identifier"),
            (&["path", "string literal", "`&&`"], "path, string literal, or `&&`"),
            (&["`&&`", "`||`", "`&&`", "`||`"], "`&&`, `||`, `&&`, or `||`"),
        ];
        for &(tokens, expected) in tests {
            assert_eq!(or_list(tokens), expected, "{tokens:?}");
        }
    }
}

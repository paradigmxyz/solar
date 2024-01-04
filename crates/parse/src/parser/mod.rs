use crate::{PErr, PResult, ParseSess};
use std::fmt::{self, Write};
use sulk_ast::{
    ast::Path,
    token::{Delimiter, Token, TokenKind},
};
use sulk_interface::{
    diagnostics::{DiagCtxt, FatalError},
    source_map::SpanSnippetError,
    Ident, Span, Symbol,
};

mod expr;
mod item;
mod lit;
mod stmt;
mod ty;
mod yul;

/// Solidity parser.
pub struct Parser<'a> {
    /// The parser session.
    pub sess: &'a ParseSess,

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
    IntLit,
    Ident,
    Path,
    ElementaryType,
}

impl fmt::Display for ExpectedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Token(t) => return write!(f, "`{t}`"),
            Self::Keyword(kw) => return write!(f, "`{kw}`"),
            Self::StrLit => "a string literal",
            Self::IntLit => "a decimal integer literal",
            Self::Lit => "a literal",
            Self::Ident => "an identifier",
            Self::Path => "a path",
            Self::ElementaryType => "an elementary type name",
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

/// Used by [`Parser::expect_any_with_type`].
#[derive(Clone, Copy, Debug)]
enum TokenExpectType {
    /// Unencountered tokens are inserted into [`Parser::expected_tokens`].
    /// See [`Parser::check`].
    Expect,

    /// Unencountered tokens are not inserted into [`Parser::expected_tokens`].
    /// See [`Parser::check_noexpect`].
    #[allow(dead_code)] // TODO
    NoExpect,
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

impl<'a> Parser<'a> {
    /// Creates a new parser.
    pub fn new(sess: &'a ParseSess, tokens: Vec<Token>) -> Self {
        let mut parser = Self {
            sess,
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

    /// Returns the diagnostic context.
    #[inline]
    pub fn dcx(&self) -> &'a DiagCtxt {
        &self.sess.dcx
    }

    #[allow(dead_code)] // TODO
    fn span_to_snippet(&self, span: Span) -> Result<String, SpanSnippetError> {
        self.sess.source_map().span_to_snippet(span)
    }

    /// Returns an "unexpected token" error for the current token.
    #[inline]
    #[track_caller]
    pub fn unexpected<T>(&mut self) -> PResult<'a, T> {
        #[cold]
        #[inline(never)]
        fn unexpected_ok(b: bool) -> ! {
            unreachable!("`unexpected()` return Ok({b})")
        }
        self.expect_one_of(&[], &[]).map(|x| unexpected_ok(x))
    }

    /// Expects and consumes the token `t`. Signals an error if the next token is not `t`.
    #[track_caller]
    pub fn expect(&mut self, tok: &TokenKind) -> PResult<'a, bool /* recovered */> {
        if self.expected_tokens.is_empty() {
            if self.check_noexpect(tok) {
                self.bump();
                Ok(false)
            } else {
                Err(self.unexpected_error(tok))
            }
        } else {
            self.expect_one_of(std::slice::from_ref(tok), &[])
        }
    }

    /// Creates a [`PErr`] for an unexpected token `t`.
    #[track_caller]
    fn unexpected_error(&mut self, t: &TokenKind) -> PErr<'a> {
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
    ) -> PResult<'a, bool /* recovered */> {
        if edible.contains(&self.token.kind) {
            self.bump();
            Ok(false)
        } else if inedible.contains(&self.token.kind) {
            // leave it in the input
            Ok(false)
        } else if self.token.kind != TokenKind::Eof
            && self.last_unexpected_token_span == Some(self.token.span)
        {
            FatalError.raise();
        } else {
            self.expected_one_of_not_found(edible, inedible)
        }
    }

    #[track_caller]
    fn expected_one_of_not_found(
        &mut self,
        edible: &[TokenKind],
        inedible: &[TokenKind],
    ) -> PResult<'a, bool> {
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
            len @ 2.. => {
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
    fn expect_semi(&mut self) -> PResult<'a, ()> {
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
    fn expect_keyword(&mut self, kw: Symbol) -> PResult<'a, ()> {
        if !self.eat_keyword(kw) {
            self.unexpected()
        } else {
            Ok(())
        }
    }

    fn check_ident(&mut self) -> bool {
        self.check_or_expected(self.token.is_ident(), ExpectedToken::Ident)
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
    fn parse_paren_comma_seq<T>(
        &mut self,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */)> {
        self.parse_delim_comma_seq(Delimiter::Parenthesis, f)
    }

    /// Parses a comma-separated sequence, including both delimiters.
    /// The function `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    fn parse_delim_comma_seq<T>(
        &mut self,
        delim: Delimiter,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */)> {
        self.parse_delim_seq(delim, SeqSep::trailing_disallowed(TokenKind::Comma), f)
    }

    /// Parses a comma-separated sequence.
    /// The function `f` must consume tokens until reaching the next separator.
    fn parse_nodelim_comma_seq<T>(
        &mut self,
        stop: &TokenKind,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */)> {
        self.parse_seq_to_before_end(stop, SeqSep::trailing_disallowed(TokenKind::Comma), f).map(
            |(v, trailing, recovered)| {
                if !recovered {
                    self.eat(stop);
                }
                (v, trailing)
            },
        )
    }

    /// Parses a `sep`-separated sequence, including both delimiters.
    /// The function `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    fn parse_delim_seq<T>(
        &mut self,
        delim: Delimiter,
        sep: SeqSep,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */)> {
        self.parse_unspanned_seq(
            &TokenKind::OpenDelim(delim),
            &TokenKind::CloseDelim(delim),
            sep,
            f,
        )
    }

    /// Parses a sequence, including both delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    fn parse_unspanned_seq<T>(
        &mut self,
        bra: &TokenKind,
        ket: &TokenKind,
        sep: SeqSep,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */)> {
        self.expect(bra)?;
        self.parse_seq_to_end(ket, sep, f)
    }

    /// Parses a sequence, including only the closing delimiter. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    fn parse_seq_to_end<T>(
        &mut self,
        ket: &TokenKind,
        sep: SeqSep,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */)> {
        let (val, trailing, recovered) = self.parse_seq_to_before_end(ket, sep, f)?;
        if !recovered {
            self.eat(ket);
        }
        Ok((val, trailing))
    }

    /// Parses a sequence, not including the delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    fn parse_seq_to_before_end<T>(
        &mut self,
        ket: &TokenKind,
        sep: SeqSep,
        f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */, bool /* recovered */)> {
        self.parse_seq_to_before_tokens(&[ket], sep, TokenExpectType::Expect, f)
    }

    /// Checks if the next token is contained within `kets`, and returns `true` if so.
    fn expect_any_with_type(&mut self, kets: &[&TokenKind], expect: TokenExpectType) -> bool {
        kets.iter().any(|k| match expect {
            TokenExpectType::Expect => self.check(k),
            TokenExpectType::NoExpect => self.check_noexpect(k),
        })
    }

    /// Parses a sequence until the specified delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    fn parse_seq_to_before_tokens<T>(
        &mut self,
        kets: &[&TokenKind],
        sep: SeqSep,
        expect: TokenExpectType,
        mut f: impl FnMut(&mut Parser<'a>) -> PResult<'a, T>,
    ) -> PResult<'a, (Vec<T>, bool /* trailing */, bool /* recovered */)> {
        let mut first = true;
        let mut recovered = false;
        let mut trailing = false;
        let mut v = Vec::new();

        while !self.expect_any_with_type(kets, expect) {
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
                        Err(mut expect_err) => {
                            let sp = self.prev_token.span.shrink_to_hi();

                            // Attempt to keep parsing if it was an omitted separator.
                            match f(self) {
                                Ok(t) => {
                                    // Parsed successfully, therefore most probably the code only
                                    // misses a separator.
                                    let token_str = tk.to_string();
                                    /* expect_err
                                        .span_suggestion_short(
                                            sp,
                                            format!("missing `{token_str}`"),
                                            token_str,
                                            Applicability::MaybeIncorrect,
                                        )
                                        .emit();
                                    */
                                    expect_err
                                        .span_help(sp, format!("missing `{token_str}`"))
                                        .emit();

                                    v.push(t);
                                    continue;
                                }
                                Err(e) => {
                                    // Parsing failed, therefore it must be something more serious
                                    // than just a missing separator.
                                    for xx in &e.children {
                                        // propagate the help message from sub error 'e' to main
                                        // error 'expect_err;
                                        expect_err.children.push(xx.clone());
                                    }
                                    e.cancel();
                                    if self.token.kind == TokenKind::Colon {
                                        // we will try to recover in
                                        // `maybe_recover_struct_lit_bad_delims`
                                        return Err(expect_err);
                                    } else if let [TokenKind::CloseDelim(Delimiter::Parenthesis)] =
                                        kets
                                    {
                                        return Err(expect_err);
                                    } else {
                                        expect_err.emit();
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
            }

            if sep.trailing_sep_required {
                for ket in kets {
                    self.expect(ket)?;
                }
                trailing = true;
                break;
            }

            if sep.trailing_sep_allowed && self.expect_any_with_type(kets, expect) {
                trailing = true;
                break;
            }

            let t = f(self)?;
            v.push(t);
        }

        Ok((v, trailing, recovered))
    }

    /// Advance the parser by one token.
    pub fn bump(&mut self) {
        let next = self.tokens.next().unwrap_or(Token::EOF);
        // TODO
        // if next.span.is_dummy() {
        //     // Tweak the location for better diagnostics, but keep syntactic context intact.
        //     let fallback_span = self.token.span;
        //     next.span = fallback_span.with_ctxt(next.span.ctxt());
        // }
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
    pub fn look_ahead(&self, dist: usize) -> &Token {
        self.tokens.as_slice().get(dist).unwrap_or(&Token::EOF)
    }

    /// Calls `f` with the token `dist` tokens ahead of the current one.
    ///
    /// See [`look_ahead`](Self::look_ahead) for more information.
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
impl<'a> Parser<'a> {
    /// Provides a spanned parser.
    pub fn parse_spanned<T>(
        &mut self,
        f: impl FnOnce(&mut Self) -> PResult<'a, T>,
    ) -> PResult<'a, (Span, T)> {
        let lo = self.token.span;
        let res = f(self);
        let span = lo.to(self.prev_token.span);
        match res {
            Ok(t) => Ok((span, t)),
            Err(e) if e.span.is_dummy() => Err(e.span(span)),
            Err(e) => Err(e),
        }
    }

    /// Parses a qualified identifier: `foo.bar.baz`.
    pub fn parse_path(&mut self) -> PResult<'a, Path> {
        let first = self.parse_ident()?;
        self.parse_path_with(first)
    }

    /// Parses a qualified identifier starting with the given identifier.
    pub fn parse_path_with(&mut self, first: Ident) -> PResult<'a, Path> {
        if !self.check_noexpect(&TokenKind::Dot) {
            return Ok(Path::new_single(first));
        }

        let mut path = Vec::with_capacity(4);
        path.push(first);
        loop {
            path.push(self.parse_ident()?);
            if !self.eat(&TokenKind::Dot) {
                break;
            }
        }
        Ok(Path::new(path))
    }

    /// Parses an identifier.
    pub fn parse_ident(&mut self) -> PResult<'a, Ident> {
        self.parse_ident_maybe_recover(true)
    }

    /// Parses an optional identifier.
    pub fn parse_ident_opt(&mut self) -> PResult<'a, Option<Ident>> {
        if self.token.is_ident() {
            self.parse_ident().map(Some)
        } else {
            Ok(None)
        }
    }

    fn parse_ident_maybe_recover(&mut self, recover: bool) -> PResult<'a, Ident> {
        let ident = self.ident_or_err(recover)?;
        if ident.is_reserved(self.in_yul) {
            let err = self.expected_ident_found_err();
            if recover {
                err.emit();
            } else {
                return Err(err);
            }
        }
        Ok(ident)
    }

    fn ident_or_err(&mut self, recover: bool) -> PResult<'a, Ident> {
        match self.token.ident() {
            Some(ident) => Ok(ident),
            None => self.expected_ident_found(recover),
        }
    }

    fn expected_ident_found(&mut self, recover: bool) -> PResult<'a, Ident> {
        let msg = format!("expected identifier, found {}", self.token.full_description());
        let mut err = self.dcx().err(msg);

        let mut recovered_ident = None;
        // We take this here so that the correct original token is retained in the diagnostic,
        // regardless of eager recovery.
        let bad_token = self.token.clone();

        let suggest_remove_comma =
            self.token.kind == TokenKind::Comma && self.look_ahead(1).is_ident();
        if suggest_remove_comma {
            if recover {
                self.bump();
                recovered_ident = self.ident_or_err(false).ok();
            }
            err = err.span_help(bad_token.span, "remove this comma");
        }

        if recover {
            if let Some(ident) = recovered_ident {
                err.emit();
                return Ok(ident);
            }
        }
        Err(err)
    }

    fn expected_ident_found_err(&mut self) -> PErr<'a> {
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
    fn expected_token_to_string_many() {
        use ExpectedToken::*;
        use TokenKind as TK;

        let tests: &[(&[ExpectedToken], &str)] = &[
            (&[], ""),
            (&[Token(TK::Eof)], "`<eof>`"),
            (&[IntLit, Ident], "a decimal integer literal or an identifier"),
            (&[Path, StrLit, Token(TK::AndAnd)], "a path, a string literal, or `&&`"),
            (
                &[Token(TK::AndAnd), Token(TK::OrOr), Token(TK::AndAnd), Token(TK::OrOr)],
                "`&&`, `||`, `&&`, or `||`",
            ),
        ];
        for &(tokens, expected) in tests {
            assert_eq!(ExpectedToken::to_string_many(tokens), expected, "{tokens:?}");
        }
    }
}

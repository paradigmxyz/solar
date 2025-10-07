use crate::{Lexer, PErr, PResult};
use smallvec::SmallVec;
use solar_ast::{
    self as ast, AstPath, Box, BoxSlice, DocComment, DocComments,
    token::{Delimiter, Token, TokenKind},
};
use solar_data_structures::{BumpExt, fmt::or_list};
use solar_interface::{
    Ident, Result, Session, Span, Symbol,
    diagnostics::DiagCtxt,
    source_map::{FileName, SourceFile},
};
use std::{fmt, path::Path};

mod expr;
mod item;
mod lit;
mod stmt;
mod ty;
mod yul;

/// Maximum allowed recursive descent depth for selected parser entry points.
const PARSER_RECURSION_LIMIT: usize = 128;

/// Solidity and Yul parser.
///
/// # Examples
///
/// ```
/// # mod solar { pub use {solar_ast as ast, solar_interface as interface, solar_parse as parse}; }
/// # fn main() {}
#[doc = include_str!("../../doc-examples/parser.rs")]
/// ```
pub struct Parser<'sess, 'ast> {
    /// The parser session.
    pub sess: &'sess Session,
    /// The arena where the AST nodes are allocated.
    pub arena: &'ast ast::Arena,

    /// The current token.
    pub token: Token,
    /// The previous token.
    pub prev_token: Token,
    /// List of expected tokens. Cleared after each `bump` call.
    expected_tokens: Vec<ExpectedToken>,
    /// The span of the last unexpected token.
    last_unexpected_token_span: Option<Span>,
    /// The current doc-comments.
    docs: Vec<DocComment>,

    /// The token stream.
    tokens: std::vec::IntoIter<Token>,

    /// Whether the parser is in Yul mode.
    ///
    /// Currently, this can only happen when parsing a Yul "assembly" block.
    in_yul: bool,
    /// Whether the parser is currently parsing a contract block.
    in_contract: bool,

    /// Current recursion depth for recursive parsing operations.
    recursion_depth: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum ExpectedToken {
    Token(TokenKind),
    Keyword(Symbol),
    Lit,
    StrLit,
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
            Self::Lit => "literal",
            Self::Ident => "identifier",
            Self::Path => "path",
            Self::ElementaryType => "elementary type name",
        })
    }
}

impl ExpectedToken {
    fn to_string_many(tokens: &[Self]) -> String {
        or_list(tokens).to_string()
    }

    fn eq_kind(&self, other: TokenKind) -> bool {
        match *self {
            Self::Token(kind) => kind == other,
            _ => false,
        }
    }
}

/// A sequence separator.
#[derive(Debug)]
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

    fn trailing_disallowed(t: TokenKind) -> Self {
        Self { sep: Some(t), trailing_sep_required: false, trailing_sep_allowed: false }
    }

    fn none() -> Self {
        Self { sep: None, trailing_sep_required: false, trailing_sep_allowed: false }
    }
}

/// Indicates whether the parser took a recovery path and continued.
#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum Recovered {
    No,
    Yes,
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Creates a new parser.
    pub fn new(sess: &'sess Session, arena: &'ast ast::Arena, tokens: Vec<Token>) -> Self {
        assert!(sess.is_entered(), "session should be entered before parsing");
        let mut parser = Self {
            sess,
            arena,
            token: Token::DUMMY,
            prev_token: Token::DUMMY,
            expected_tokens: Vec::with_capacity(8),
            last_unexpected_token_span: None,
            docs: Vec::with_capacity(4),
            tokens: tokens.into_iter(),
            in_yul: false,
            in_contract: false,
            recursion_depth: 0,
        };
        parser.bump();
        parser
    }

    /// Creates a new parser from a source code string.
    pub fn from_source_code(
        sess: &'sess Session,
        arena: &'ast ast::Arena,
        filename: FileName,
        src: impl Into<String>,
    ) -> Result<Self> {
        Self::from_lazy_source_code(sess, arena, filename, || Ok(src.into()))
    }

    /// Creates a new parser from a file.
    ///
    /// The file will not be read if it has already been added into the source map.
    pub fn from_file(sess: &'sess Session, arena: &'ast ast::Arena, path: &Path) -> Result<Self> {
        Self::from_lazy_source_code(sess, arena, FileName::Real(path.to_path_buf()), || {
            sess.source_map().file_loader().load_file(path)
        })
    }

    /// Creates a new parser from a source code closure.
    ///
    /// The closure will not be called if the file name has already been added into the source map.
    pub fn from_lazy_source_code(
        sess: &'sess Session,
        arena: &'ast ast::Arena,
        filename: FileName,
        get_src: impl FnOnce() -> std::io::Result<String>,
    ) -> Result<Self> {
        let file = sess
            .source_map()
            .new_source_file_with(filename, get_src)
            .map_err(|e| sess.dcx.err(e.to_string()).emit())?;
        Ok(Self::from_source_file(sess, arena, &file))
    }

    /// Creates a new parser from a source file.
    ///
    /// Note that the source file must be added to the source map before calling this function.
    /// Prefer using [`from_source_code`](Self::from_source_code) or [`from_file`](Self::from_file)
    /// instead.
    pub fn from_source_file(
        sess: &'sess Session,
        arena: &'ast ast::Arena,
        file: &SourceFile,
    ) -> Self {
        Self::from_lexer(arena, Lexer::from_source_file(sess, file))
    }

    /// Creates a new parser from a lexer.
    pub fn from_lexer(arena: &'ast ast::Arena, lexer: Lexer<'sess, '_>) -> Self {
        Self::new(lexer.sess, arena, lexer.into_tokens())
    }

    /// Returns the diagnostic context.
    #[inline]
    pub fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    /// Allocates an object on the AST arena.
    pub fn alloc<T>(&self, value: T) -> Box<'ast, T> {
        self.arena.alloc(value)
    }

    /// Allocates a list of objects on the AST arena.
    ///
    /// # Panics
    ///
    /// Panics if the list is empty.
    pub fn alloc_path(&self, segments: &[Ident]) -> AstPath<'ast> {
        // SAFETY: `Ident` is `Copy`.
        AstPath::new_in(self.arena.bump(), segments)
    }

    /// Allocates a list of objects on the AST arena.
    pub fn alloc_vec<T>(&self, values: Vec<T>) -> BoxSlice<'ast, T> {
        self.arena.alloc_vec_thin((), values)
    }

    /// Allocates a list of objects on the AST arena.
    pub fn alloc_smallvec<A: smallvec::Array>(
        &self,
        values: SmallVec<A>,
    ) -> BoxSlice<'ast, A::Item> {
        self.arena.alloc_smallvec_thin((), values)
    }

    /// Returns an "unexpected token" error in a [`PResult`] for the current token.
    #[inline]
    #[track_caller]
    pub fn unexpected<T>(&mut self) -> PResult<'sess, T> {
        Err(self.unexpected_error())
    }

    /// Returns an "unexpected token" error for the current token.
    #[cold]
    #[track_caller]
    pub fn unexpected_error(&mut self) -> PErr<'sess> {
        match self.expected_one_of_not_found(&[], &[]) {
            Ok(b) => unreachable!("`unexpected()` returned Ok({b:?})"),
            Err(e) => e,
        }
    }

    /// Expects and consumes the token `t`. Signals an error if the next token is not `t`.
    #[inline]
    #[track_caller]
    pub fn expect(&mut self, tok: TokenKind) -> PResult<'sess, Recovered> {
        if self.check_noexpect(tok) {
            self.bump();
            Ok(Recovered::No)
        } else {
            self.expected_one_of_not_found(std::slice::from_ref(&tok), &[])
        }
    }

    /// Expect next token to be edible or inedible token. If edible,
    /// then consume it; if inedible, then return without consuming
    /// anything. Signal a fatal error if next token is unexpected.
    #[track_caller]
    pub fn expect_one_of(
        &mut self,
        edible: &[TokenKind],
        inedible: &[TokenKind],
    ) -> PResult<'sess, Recovered> {
        if edible.contains(&self.token.kind) {
            self.bump();
            Ok(Recovered::No)
        } else if inedible.contains(&self.token.kind) {
            // leave it in the input
            Ok(Recovered::No)
        } else {
            self.expected_one_of_not_found(edible, inedible)
        }
    }

    #[cold]
    #[track_caller]
    fn expected_one_of_not_found(
        &mut self,
        edible: &[TokenKind],
        inedible: &[TokenKind],
    ) -> PResult<'sess, Recovered> {
        if self.token.kind != TokenKind::Eof
            && self.last_unexpected_token_span == Some(self.token.span)
        {
            panic!("called unexpected twice on the same token");
        }

        let mut expected = edible
            .iter()
            .chain(inedible)
            .cloned()
            .map(ExpectedToken::Token)
            .chain(self.expected_tokens.iter().cloned())
            .filter(|token| {
                // Filter out suggestions that suggest the same token
                // which was found and deemed incorrect.
                fn is_ident_eq_keyword(found: TokenKind, expected: &ExpectedToken) -> bool {
                    if let TokenKind::Ident(current_sym) = found
                        && let ExpectedToken::Keyword(suggested_sym) = expected
                    {
                        return current_sym == *suggested_sym;
                    }
                    false
                }

                if !token.eq_kind(self.token.kind) {
                    let eq = is_ident_eq_keyword(self.token.kind, token);
                    // If the suggestion is a keyword and the found token is an ident,
                    // the content of which are equal to the suggestion's content,
                    // we can remove that suggestion (see the `return false` below).

                    // If this isn't the case however, and the suggestion is a token the
                    // content of which is the same as the found token's, we remove it as well.
                    if !eq {
                        if let ExpectedToken::Token(kind) = token
                            && *kind == self.token.kind
                        {
                            return false;
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
    #[inline]
    #[track_caller]
    fn expect_semi(&mut self) -> PResult<'sess, ()> {
        self.expect(TokenKind::Semi).map(drop)
    }

    /// Checks if the next token is `tok`, and returns `true` if so.
    ///
    /// This method will automatically add `tok` to `expected_tokens` if `tok` is not
    /// encountered.
    #[inline]
    #[must_use]
    fn check(&mut self, tok: TokenKind) -> bool {
        let is_present = self.check_noexpect(tok);
        if !is_present {
            self.push_expected(ExpectedToken::Token(tok));
        }
        is_present
    }

    #[inline]
    #[must_use]
    fn check_noexpect(&self, tok: TokenKind) -> bool {
        self.token.kind == tok
    }

    /// Consumes a token 'tok' if it exists. Returns whether the given token was present.
    ///
    /// the main purpose of this function is to reduce the cluttering of the suggestions list
    /// which using the normal eat method could introduce in some cases.
    #[inline]
    #[must_use]
    pub fn eat_noexpect(&mut self, tok: TokenKind) -> bool {
        let is_present = self.check_noexpect(tok);
        if is_present {
            self.bump()
        }
        is_present
    }

    /// Consumes a token 'tok' if it exists. Returns whether the given token was present.
    #[inline]
    #[must_use]
    pub fn eat(&mut self, tok: TokenKind) -> bool {
        let is_present = self.check(tok);
        if is_present {
            self.bump()
        }
        is_present
    }

    /// If the next token is the given keyword, returns `true` without eating it.
    /// An expectation is also added for diagnostics purposes.
    #[inline]
    #[must_use]
    fn check_keyword(&mut self, kw: Symbol) -> bool {
        let is_keyword = self.token.is_keyword(kw);
        if !is_keyword {
            self.push_expected(ExpectedToken::Keyword(kw));
        }
        is_keyword
    }

    /// If the next token is the given keyword, eats it and returns `true`.
    /// Otherwise, returns `false`. An expectation is also added for diagnostics purposes.
    #[inline]
    #[must_use]
    pub fn eat_keyword(&mut self, kw: Symbol) -> bool {
        let is_keyword = self.check_keyword(kw);
        if is_keyword {
            self.bump();
        }
        is_keyword
    }

    /// If the given word is not a keyword, signals an error.
    /// If the next token is not the given word, signals an error.
    /// Otherwise, eats it.
    #[track_caller]
    fn expect_keyword(&mut self, kw: Symbol) -> PResult<'sess, ()> {
        if !self.eat_keyword(kw) { self.unexpected() } else { Ok(()) }
    }

    #[must_use]
    fn check_ident(&mut self) -> bool {
        self.check_or_expected(self.token.is_ident(), ExpectedToken::Ident)
    }

    #[must_use]
    fn check_nr_ident(&mut self) -> bool {
        self.check_or_expected(self.token.is_non_reserved_ident(self.in_yul), ExpectedToken::Ident)
    }

    #[must_use]
    fn check_path(&mut self) -> bool {
        self.check_or_expected(self.token.is_ident(), ExpectedToken::Path)
    }

    #[must_use]
    fn check_lit(&mut self) -> bool {
        self.check_or_expected(self.token.is_lit(), ExpectedToken::Lit)
    }

    #[must_use]
    fn check_str_lit(&mut self) -> bool {
        self.check_or_expected(self.token.is_str_lit(), ExpectedToken::StrLit)
    }

    #[must_use]
    fn check_elementary_type(&mut self) -> bool {
        self.check_or_expected(self.token.is_elementary_type(), ExpectedToken::ElementaryType)
    }

    #[must_use]
    fn check_or_expected(&mut self, ok: bool, t: ExpectedToken) -> bool {
        if !ok {
            self.push_expected(t);
        }
        ok
    }

    // #[inline(never)]
    fn push_expected(&mut self, expected: ExpectedToken) {
        self.expected_tokens.push(expected);
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
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
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
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        self.parse_delim_seq(delim, SeqSep::trailing_disallowed(TokenKind::Comma), allow_empty, f)
    }

    /// Parses a comma-separated sequence.
    /// The function `f` must consume tokens until reaching the next separator.
    #[track_caller]
    #[inline]
    fn parse_nodelim_comma_seq<T>(
        &mut self,
        stop: TokenKind,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        self.parse_seq_to_before_end(
            stop,
            SeqSep::trailing_disallowed(TokenKind::Comma),
            allow_empty,
            f,
        )
        .map(|(v, _recovered)| v)
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
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        self.parse_unspanned_seq(
            TokenKind::OpenDelim(delim),
            TokenKind::CloseDelim(delim),
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
        bra: TokenKind,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
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
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        let (val, recovered) = self.parse_seq_to_before_end(ket, sep, allow_empty, f)?;
        if recovered == Recovered::No {
            self.expect(ket)?;
        }
        Ok(val)
    }

    /// Parses a sequence, not including the delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline]
    fn parse_seq_to_before_end<T>(
        &mut self,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (BoxSlice<'ast, T>, Recovered)> {
        self.parse_seq_to_before_tokens(ket, sep, allow_empty, f)
    }

    /// Parses a sequence until the specified delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    fn parse_seq_to_before_tokens<T>(
        &mut self,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (BoxSlice<'ast, T>, Recovered)> {
        let mut first = true;
        let mut recovered = Recovered::No;
        let mut trailing = false;
        let mut v = SmallVec::<[T; 8]>::new();

        if !allow_empty {
            v.push(f(self)?);
            first = false;
        }

        while !self.check_noexpect(ket) {
            if TokenKind::Eof == self.token.kind {
                break;
            }

            if let Some(sep_kind) = sep.sep {
                if first {
                    // no separator for the first element
                    first = false;
                } else {
                    // check for separator
                    match self.expect(sep_kind) {
                        Ok(recovered_) => {
                            if recovered_ == Recovered::Yes {
                                recovered = Recovered::Yes;
                                break;
                            }
                        }
                        Err(e) => return Err(e),
                    }

                    if self.check_noexpect(ket) {
                        trailing = true;
                        break;
                    }
                }
            }

            v.push(f(self)?);
        }

        if let Some(sep_kind) = sep.sep {
            let open_close_delim = first && allow_empty;
            if !open_close_delim
                && sep.trailing_sep_required
                && !trailing
                && let Err(e) = self.expect(sep_kind)
            {
                e.emit();
            }
            if !sep.trailing_sep_allowed && trailing {
                let msg = format!("trailing `{sep_kind}` separator is not allowed");
                self.dcx().err(msg).span(self.prev_token.span).emit();
            }
        }

        Ok((self.alloc_smallvec(v), recovered))
    }

    /// Advance the parser by one token.
    pub fn bump(&mut self) {
        let next = self.next_token();
        if next.is_comment_or_doc() {
            return self.bump_trivia(next);
        }
        self.inlined_bump_with(next);
    }

    /// Advance the parser by one token using provided token as the next one.
    ///
    /// # Panics
    ///
    /// Panics if the provided token is a comment.
    pub fn bump_with(&mut self, next: Token) {
        self.inlined_bump_with(next);
    }

    /// This always-inlined version should only be used on hot code paths.
    #[inline(always)]
    fn inlined_bump_with(&mut self, next: Token) {
        #[cfg(debug_assertions)]
        if next.is_comment_or_doc() {
            self.dcx().bug("`bump_with` should not be used with comments").span(next.span).emit();
        }
        self.prev_token = std::mem::replace(&mut self.token, next);
        self.expected_tokens.clear();
        self.docs.clear();
    }

    /// Bumps comments and docs.
    ///
    /// Pushes docs to `self.docs`. Retrieve them with `parse_doc_comments`.
    #[cold]
    fn bump_trivia(&mut self, next: Token) {
        self.docs.clear();

        debug_assert!(next.is_comment_or_doc());
        self.prev_token = std::mem::replace(&mut self.token, next);
        while let Some((is_doc, doc)) = self.token.comment() {
            if is_doc {
                self.docs.push(doc);
            }
            // Don't set `prev_token` on purpose.
            self.token = self.next_token();
        }

        self.expected_tokens.clear();
    }

    /// Advances the internal `tokens` iterator, without updating the parser state.
    ///
    /// Use [`bump`](Self::bump) and [`token`](Self::token) instead.
    #[inline(always)]
    fn next_token(&mut self) -> Token {
        self.tokens.next().unwrap_or(Token::new(TokenKind::Eof, self.token.span))
    }

    /// Returns the token `dist` tokens ahead of the current one.
    ///
    /// [`Eof`](Token::EOF) will be returned if the look-ahead is any distance past the end of the
    /// tokens.
    #[inline]
    pub fn look_ahead(&self, dist: usize) -> Token {
        // Specialize for the common `dist` cases.
        match dist {
            0 => self.token,
            1 => self.look_ahead_full(1),
            2 => self.look_ahead_full(2),
            dist => self.look_ahead_full(dist),
        }
    }

    fn look_ahead_full(&self, dist: usize) -> Token {
        self.tokens
            .as_slice()
            .iter()
            .copied()
            .filter(|t| !t.is_comment_or_doc())
            .nth(dist - 1)
            .unwrap_or(Token::EOF)
    }

    /// Calls `f` with the token `dist` tokens ahead of the current one.
    ///
    /// See [`look_ahead`](Self::look_ahead) for more information.
    #[inline]
    pub fn look_ahead_with<R>(&self, dist: usize, f: impl FnOnce(Token) -> R) -> R {
        f(self.look_ahead(dist))
    }

    /// Runs `f` with the parser in a contract context.
    #[inline]
    fn in_contract<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = std::mem::replace(&mut self.in_contract, true);
        let res = f(self);
        self.in_contract = old;
        res
    }

    /// Runs `f` with the parser in a Yul context.
    #[inline]
    fn in_yul<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = std::mem::replace(&mut self.in_yul, true);
        let res = f(self);
        self.in_yul = old;
        res
    }

    /// Runs `f` with recursion depth tracking and limit enforcement.
    #[inline]
    pub fn with_recursion_limit<T>(
        &mut self,
        context: &str,
        f: impl FnOnce(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, T> {
        self.recursion_depth += 1;
        let res = if self.recursion_depth > PARSER_RECURSION_LIMIT {
            Err(self.recursion_limit_reached(context))
        } else {
            f(self)
        };
        self.recursion_depth -= 1;
        res
    }

    #[cold]
    fn recursion_limit_reached(&mut self, context: &str) -> PErr<'sess> {
        let mut err = self.dcx().err("recursion limit reached").span(self.token.span);
        if !self.prev_token.span.is_dummy() {
            err = err.span_label(self.prev_token.span, format!("while parsing {context}"));
        }
        err
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
    #[inline]
    pub fn parse_doc_comments(&mut self) -> DocComments<'ast> {
        if !self.docs.is_empty() { self.parse_doc_comments_inner() } else { Default::default() }
    }

    #[cold]
    fn parse_doc_comments_inner(&mut self) -> DocComments<'ast> {
        let docs = self.arena.alloc_thin_slice_copy((), &self.docs);
        self.docs.clear();
        docs.into()
    }

    /// Parses a qualified identifier: `foo.bar.baz`.
    #[track_caller]
    pub fn parse_path(&mut self) -> PResult<'sess, AstPath<'ast>> {
        let first = self.parse_ident()?;
        self.parse_path_with(first)
    }

    /// Parses a qualified identifier starting with the given identifier.
    #[track_caller]
    pub fn parse_path_with(&mut self, first: Ident) -> PResult<'sess, AstPath<'ast>> {
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
    pub fn parse_path_any(&mut self) -> PResult<'sess, AstPath<'ast>> {
        let first = self.parse_ident_any()?;
        self.parse_path_with_f(first, Self::parse_ident_any)
    }

    /// Parses a qualified identifier starting with the given identifier.
    #[track_caller]
    fn parse_path_with_f(
        &mut self,
        first: Ident,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, Ident>,
    ) -> PResult<'sess, AstPath<'ast>> {
        if !self.check_noexpect(TokenKind::Dot) {
            return Ok(self.alloc_path(&[first]));
        }

        let mut path = SmallVec::<[_; 4]>::new();
        path.push(first);
        while self.eat(TokenKind::Dot) {
            path.push(f(self)?);
        }
        Ok(self.alloc_path(&path))
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
        if self.check_ident() { self.parse_ident().map(Some) } else { Ok(None) }
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

    #[cold]
    #[track_caller]
    fn expected_ident_found(&mut self, recover: bool) -> PResult<'sess, Ident> {
        self.expected_ident_found_other(self.token, recover)
    }

    #[cold]
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

        if recover && let Some(ident) = recovered_ident {
            err.emit();
            return Ok(ident);
        }
        Err(err)
    }

    #[cold]
    #[track_caller]
    fn expected_ident_found_err(&mut self) -> PErr<'sess> {
        self.expected_ident_found(false).unwrap_err()
    }
}

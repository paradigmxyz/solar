use crate::{Lexer, PErr, PResult};
use smallvec::SmallVec;
use solar_ast::{
    self as ast, AstPath, Box, BoxSlice, DocComment, DocComments,
    token::{Delimiter, Token, TokenKind},
};
use solar_data_structures::{BumpExt, fmt::or_list};
use solar_interface::{
    BytePos, Ident, Result, Session, Span, Symbol,
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
    docs: Vec<DocComment<'ast>>,

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

        while !self.check(ket) {
            if self.token.kind == TokenKind::Eof {
                recovered = Recovered::Yes;
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

                    if self.check(ket) {
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
        while let Some((is_doc, kind, symbol)) = self.token.comment() {
            if is_doc {
                let natspec = if let Some(items) =
                    parse_natspec(self.token.span, symbol, self.in_yul, self.dcx())
                {
                    self.alloc_smallvec(items)
                } else {
                    BoxSlice::default()
                };
                self.docs.push(DocComment { kind, span: self.token.span, symbol, natspec });
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
        // SAFETY: Doesn't have `Drop` and we clear right after to pass ownership to the caller.
        // We use this to avoid deallocating the vector's memory.
        assert!(!std::mem::needs_drop::<DocComments<'_>>());
        let docs = unsafe { self.arena.alloc_thin_slice_unchecked((), &self.docs) };
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

// Default @notice behavior:
// - Per Solidity spec, doc comments without any `@` tags default to `@notice`.
//
// We use a simplified approach compared to Solc's NatSpec parsing.
//
// Solc implementation notes:
// - Reference: https://github.com/ethereum/solidity/blob/develop/libsolidity/analysis/DocStringTagParser.cpp
// - Parses, validates, and stores tag content directly when parsing.
// - Lexer merges consecutive line comments (`///`) into a single doc comment token.
//
// Our implementation:
// - Each `///` line is a separate doc comment (not merged by lexer). Because of that, each line
//   without tags becomes an individual `@notice` item.
// - Defers validation to lowering phase.
// - Follows Solc's Yul behavior: silently ignores unknown tags in Yul context https://github.com/argotorg/solidity/blob/2ca5fb3b6adcb1a8fb2c0904fb37526121cf2c72/libyul/AsmParser.cpp#L151

/// Parses NatSpec items from a single doc comment.
fn parse_natspec(
    comment_span: Span,
    comment_symbol: Symbol,
    in_yul: bool,
    dcx: &DiagCtxt,
) -> Option<SmallVec<[ast::NatSpecItem; 6]>> {
    let content = comment_symbol.as_str();
    let bytes = content.as_bytes();

    // Early-exit if no tag is found.
    if memchr::memchr(b'@', bytes).is_none() {
        if content.trim().is_empty() {
            return None;
        }

        // Create a synthetic @notice tag for the entire comment
        let mut items = SmallVec::<[ast::NatSpecItem; 6]>::new();
        items.push(ast::NatSpecItem {
            kind: ast::NatSpecKind::Notice,
            span: comment_span,
            content_start: 0,
            content_end: content.len() as u32,
        });
        return Some(items);
    }

    // Line comments: '///', Block comments: '/**'.
    const PREFIX_BYTES: u32 = 3;
    let (mut line_start, mut content_start, mut span, mut kind) = (0, 0usize, None, None);
    let mut items = SmallVec::<[ast::NatSpecItem; 6]>::new();

    fn flush_item(
        items: &mut SmallVec<[ast::NatSpecItem; 6]>,
        kind: &mut Option<ast::NatSpecKind>,
        span: &mut Option<Span>,
        content_start: usize,
        content_end: usize,
    ) {
        if let Some(k) = kind.take() {
            items.push(ast::NatSpecItem {
                span: span.take().unwrap(),
                kind: k,
                content_start: content_start as u32,
                content_end: content_end as u32,
            });
        }
    }

    // Iterate over each line and look for tags.
    let mut prev_line_end = 0;
    for line_end in memchr::memchr_iter(b'\n', bytes).chain(std::iter::once(bytes.len())) {
        if let Some(tag_offset) = memchr::memchr(b'@', &bytes[line_start..line_end]) {
            let tag_start = line_start + tag_offset + 1;
            flush_item(&mut items, &mut kind, &mut span, content_start, prev_line_end);

            // Skip leading whitespace after '@'
            let tag_slice = &bytes[tag_start..line_end];
            let trimmed = tag_slice.len() - tag_slice.trim_ascii_start().len();
            let (tag, rest_start) = split_once_ws(content, bytes, tag_start + trimmed, line_end);

            // Calculate span: from first non-whitespace char after '@' to end of tag name.
            let tag_lo =
                comment_span.lo().0 + PREFIX_BYTES + 1 + (line_start + tag_offset + trimmed) as u32; // +1 for '@'
            let tag_hi = tag_lo + tag.len() as u32;
            span = Some(Span::new(BytePos(tag_lo), BytePos(tag_hi)));
            content_start = rest_start;

            kind = Some(match tag {
                "title" => ast::NatSpecKind::Title,
                "author" => ast::NatSpecKind::Author,
                "notice" => ast::NatSpecKind::Notice,
                "dev" => ast::NatSpecKind::Dev,
                "param" | "return" | "inheritdoc" => {
                    let (name, content_start_pos) =
                        split_once_ws(content, bytes, rest_start, line_end);
                    content_start = content_start_pos;
                    let ident = Ident::new(Symbol::intern(name), comment_span);
                    match tag {
                        "param" => ast::NatSpecKind::Param { name: ident },
                        "return" => ast::NatSpecKind::Return { name: ident },
                        "inheritdoc" => ast::NatSpecKind::Inheritdoc { contract: ident },
                        _ => unreachable!(),
                    }
                }
                _ => {
                    if let Some(custom_tag) = tag.strip_prefix("custom:") {
                        let ident = Ident::new(Symbol::intern(custom_tag), comment_span);
                        ast::NatSpecKind::Custom { name: ident }
                    } else if ast::NATSPEC_INTERNAL_TAGS[..].contains(&tag) {
                        let ident = Ident::new(Symbol::intern(tag), comment_span);
                        ast::NatSpecKind::Internal { tag: ident }
                    } else {
                        // Emit error for invalid solidity tags, but ignore in Yul.
                        if !in_yul {
                            dcx
                                .err(format!("invalid natspec tag '@{tag}', custom tags must use format '@custom:name'"))
                                .span(comment_span)
                                .emit();
                        }
                        line_start = line_end + 1;
                        prev_line_end = line_end;
                        continue;
                    }
                }
            });
        }

        prev_line_end = line_end;
        line_start = line_end + 1;
    }
    flush_item(&mut items, &mut kind, &mut span, content_start, bytes.len());
    Some(items)
}

/// Splits a string slice at the first whitespace character using the `memchr` crate.
/// Returns the content up to the whitespace and the position of the first following non-blank char.
#[inline]
fn split_once_ws<'a>(
    content: &'a str,
    bytes: &'a [u8],
    start: usize,
    end: usize,
) -> (&'a str, usize) {
    if let Some(ws_pos) =
        memchr::memchr3(b' ', b'\t', b'\r', &bytes[start..end]).map(|offset| start + offset)
    {
        let rest = &bytes[ws_pos..end];
        (&content[start..ws_pos], ws_pos + (rest.len() - rest.trim_ascii_start().len()))
    } else {
        (&content[start..end], end)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{Session, SourceMap};

    fn check_natspec_item(
        sm: &SourceMap,
        symbol: Symbol,
        item: &ast::NatSpecItem,
        snip: &str,
        kind: &str,
        name: Option<&str>,
        content: Option<&str>,
    ) {
        assert_eq!(sm.span_to_snippet(item.span).unwrap(), snip);

        let actual_name = match &item.kind {
            ast::NatSpecKind::Title if kind == "title" => None,
            ast::NatSpecKind::Author if kind == "author" => None,
            ast::NatSpecKind::Notice if kind == "notice" => None,
            ast::NatSpecKind::Dev if kind == "dev" => None,
            ast::NatSpecKind::Param { name } if kind == "param" => Some(name.name.as_str()),
            ast::NatSpecKind::Return { name } if kind == "return" => Some(name.name.as_str()),
            ast::NatSpecKind::Inheritdoc { contract } if kind == "inheritdoc" => {
                Some(contract.name.as_str())
            }
            ast::NatSpecKind::Custom { name } if kind == "custom" => Some(name.name.as_str()),
            ast::NatSpecKind::Internal { tag } if kind == "internal" => Some(tag.name.as_str()),
            _ => panic!("kind mismatch: expected {kind}, got {:?}", item.kind),
        };
        assert_eq!(actual_name, name);

        if let Some(expected) = content {
            let actual = &symbol.as_str()[item.content_start as usize..item.content_end as usize];
            assert_eq!(actual.trim(), expected.trim());
        }
    }

    #[test]
    fn parse_natspec_line_cmnts() {
        let src = r#"
/// @title MyContract
/// @author Alice
/// @notice This is a notice
/// that spans multiple lines
/// and continues here
/// @dev This is dev documentation
/// @param x The input parameter
/// @return result The return value
/// @inheritdoc BaseContract
/// @custom:security High priority
/// @solidity memory-safe
/// @ notice with space
"#;

        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let arena = ast::Arena::new();
            let mut parser = Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), src)
                .expect("failed to create parser");

            let sm = sess.source_map();
            let docs = parser.parse_doc_comments();

            let natspec_items: Vec<_> = docs.iter().flat_map(|d| d.natspec.iter().map(move |i| (d.symbol, i))).collect();
            assert_eq!(natspec_items.len(), 12);

            let check = |i: usize, snip, kind, name, content| {
                check_natspec_item(sm, natspec_items[i].0, natspec_items[i].1, snip, kind, name, content)
            };

            check(0, "title", "title", None, Some("MyContract"));
            check(1, "author", "author", None, Some("Alice"));
            check(2, "notice", "notice", None, Some("This is a notice"));
            let span3 = sm.span_to_snippet(natspec_items[3].1.span).unwrap();
            check(3, &span3, "notice", None, Some("that spans multiple lines"));
            let span4 = sm.span_to_snippet(natspec_items[4].1.span).unwrap();
            check(4, &span4, "notice", None, Some("and continues here"));
            check(5, "dev", "dev", None, Some("This is dev documentation"));
            check(6, "param", "param", Some("x"), Some("The input parameter"));
            check(7, "return", "return", Some("result"), Some("The return value"));
            check(8, "inheritdoc", "inheritdoc", Some("BaseContract"), Some(""));
            check(9, "custom:security", "custom", Some("security"), Some("High priority"));
            check(10, "solidity", "internal", Some("solidity"), Some("memory-safe"));
            check(11, "notice", "notice", None, Some("with space"));

            assert_eq!(sm.span_to_snippet(docs.span()).unwrap(), "/// @title MyContract\n/// @author Alice\n/// @notice This is a notice\n/// that spans multiple lines\n/// and continues here\n/// @dev This is dev documentation\n/// @param x The input parameter\n/// @return result The return value\n/// @inheritdoc BaseContract\n/// @custom:security High priority\n/// @solidity memory-safe\n/// @ notice with space");
        });
    }

    #[test]
    fn parse_natspec_block_cmnts() {
        let src = r#"
/**
 * @title MyContract
 * @author Alice
 * @notice This is a notice
 * that spans multiple lines
 * and continues here
 * @dev This is dev documentation
 * @param x The input parameter
 * @return result The return value
 * @inheritdoc BaseContract
 * @custom:security High priority
 * @src 0:123:456
 */
"#;

        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let arena = ast::Arena::new();
            let mut parser =
                Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), src)
                    .expect("failed to create parser");

            let sm = sess.source_map();
            let docs = parser.parse_doc_comments();
            assert_eq!(docs.len(), 1);

            let (sym, items) = (docs[0].symbol, &docs[0].natspec);
            assert_eq!(items.len(), 9);

            let check = |i: usize, span, kind, name, content| {
                check_natspec_item(sm, sym, &items[i], span, kind, name, content)
            };

            check(0, "title", "title", None, Some("MyContract"));
            check(1, "author", "author", None, Some("Alice"));
            check(2, "notice", "notice", None, Some("This is a notice\n * that spans multiple lines\n * and continues here"));
            check(3, "dev", "dev", None, Some("This is dev documentation"));
            check(4, "param", "param", Some("x"), Some("The input parameter"));
            check(5, "return", "return", Some("result"), Some("The return value"));
            check(6, "inheritdoc", "inheritdoc", Some("BaseContract"), Some(""));
            check(7, "custom:security", "custom", Some("security"), Some("High priority"));
            check(8, "src", "internal", Some("src"), Some("0:123:456"));

            assert_eq!(sm.span_to_snippet(docs.span()).unwrap(), "/**\n * @title MyContract\n * @author Alice\n * @notice This is a notice\n * that spans multiple lines\n * and continues here\n * @dev This is dev documentation\n * @param x The input parameter\n * @return result The return value\n * @inheritdoc BaseContract\n * @custom:security High priority\n * @src 0:123:456\n */");
        });
    }

    #[test]
    fn parse_natspec_line_cmnts_no_tags() {
        let src = r#"
/// This is a simple comment
/// It has no tags at all
/// Just plain documentation
contract Test {}
"#;

        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let arena = ast::Arena::new();
            let mut parser =
                Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), src)
                    .expect("failed to create parser");

            let sm = sess.source_map();
            let docs = parser.parse_doc_comments();
            assert_eq!(docs.len(), 3);

            for (doc, expected) in docs.iter().zip([
                "This is a simple comment",
                "It has no tags at all",
                "Just plain documentation",
            ]) {
                assert_eq!(doc.natspec.len(), 1);
                let item = &doc.natspec[0];
                let span = sm.span_to_snippet(item.span).unwrap();
                check_natspec_item(sm, doc.symbol, item, &span, "notice", None, Some(expected));
            }
        });
    }

    #[test]
    fn parse_natspec_block_cmnt_no_tags() {
        let src = r#"
/**
 * This is a block comment
 * with multiple lines
 * but no tags at all
 */
contract Test {}
"#;

        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let arena = ast::Arena::new();
            let mut parser =
                Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), src)
                    .expect("failed to create parser");

            let sm = sess.source_map();
            let docs = parser.parse_doc_comments();
            assert_eq!(docs.len(), 1);
            assert_eq!(docs[0].natspec.len(), 1);

            let item = &docs[0].natspec[0];
            let snip = sm.span_to_snippet(item.span).unwrap();
            check_natspec_item(
                sm,
                docs[0].symbol,
                item,
                &snip,
                "notice",
                None,
                Some("* This is a block comment\n * with multiple lines\n * but no tags at all"),
            );
        });
    }
}

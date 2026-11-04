use crate::{Lexer, PErr, PResult};
use doc_text::DocText;
use smallvec::SmallVec;
use solar_ast::{
    self as ast, AstPath, Box, BoxSlice, DocComment, DocComments,
    token::{Delimiter, Token, TokenKind},
};
use solar_data_structures::{BumpExt, fmt::or_list};
use solar_interface::{
    BytePos, Ident, Result, Session, Span, Symbol,
    diagnostics::DiagCtxt,
    error_code,
    source_map::{FileName, SourceFile},
};
use std::{fmt, path::Path};

mod doc_text;
mod expr;
mod item;
mod lit;
mod stmt;
mod ty;
mod yul;

/// Maximum allowed recursive descent depth for selected parser entry points.
const PARSER_RECURSION_LIMIT: usize = 256;

/// Solidity and Yul parser.
///
/// # Examples
///
/// ```
/// # mod solar { pub use {solar_ast as ast, solar_interface as interface, solar_parse as parse}; }
/// # fn main() {}
#[doc = include_str!("../../doc-examples/parser.rs")]
/// ```
pub struct Parser<'sess, 'ast, 'cb> {
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
    /// The statements of the blocks being parsed, innermost last.
    ///
    /// Blocks collect their statements here instead of in a buffer on the stack, which would take
    /// space in every level of nesting.
    stmts: Vec<ast::Stmt<'ast>>,
    /// Like `stmts`, for Yul blocks.
    yul_stmts: Vec<ast::yul::Stmt<'ast>>,

    /// The token stream.
    tokens: std::vec::IntoIter<Token>,

    /// Whether the parser is in a Yul block.
    in_yul: bool,
    /// Whether the parser is parsing a standalone Yul file.
    pure_yul: bool,
    /// Whether the parser is currently parsing a contract block.
    in_contract: bool,
    /// Whether the parser is currently parsing a modifier body.
    in_modifier: bool,
    /// Whether incomplete input should be recovered into a partial AST.
    recover_incomplete_input: bool,

    /// Current recursion depth for recursive parsing operations.
    recursion_depth: usize,
    /// Callback invoked after a top-level import directive is parsed.
    #[allow(clippy::type_complexity)]
    import_callback:
        Option<std::boxed::Box<dyn FnMut(ast::ItemId, Span, &ast::ImportDirective<'ast>) + 'cb>>,
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

impl<'sess, 'ast, 'cb> Parser<'sess, 'ast, 'cb> {
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
            stmts: Vec::new(),
            yul_stmts: Vec::new(),
            tokens: tokens.into_iter(),
            in_yul: false,
            pure_yul: false,
            in_contract: false,
            in_modifier: false,
            recover_incomplete_input: sess.opts.unstable.recover_incomplete_input,
            recursion_depth: 0,
            import_callback: None,
        };
        parser.bump();
        parser
    }

    /// Sets the import callback.
    pub fn set_import_callback(
        &mut self,
        import_callback: impl FnMut(ast::ItemId, Span, &ast::ImportDirective<'ast>) + 'cb,
    ) {
        self.import_callback = Some(std::boxed::Box::new(import_callback));
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

    /// Moves the elements of `values` into a list on the AST arena.
    ///
    /// Unlike [`alloc_smallvec`](Self::alloc_smallvec), this does not move `values` itself, which
    /// would copy its inline buffer when its address has escaped.
    fn alloc_drain<A: smallvec::Array>(&self, values: &mut SmallVec<A>) -> BoxSlice<'ast, A::Item> {
        self.arena.alloc_smallvec_drain_thin((), values)
    }

    /// Allocates a list of objects on the AST arena.
    pub fn alloc_from_iter<T>(&self, values: impl Iterator<Item = T>) -> BoxSlice<'ast, T> {
        self.arena.alloc_from_iter_thin((), values)
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
        let mut err = self.dcx().err_at(self.token.span, msg_exp);

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
        let recover = self.recover_incomplete_input
            && matches!(self.token.kind, TokenKind::CloseDelim(Delimiter::Brace) | TokenKind::Eof);
        if recover && self.last_unexpected_token_span == Some(self.token.span) {
            return Ok(());
        }
        match self.expect(TokenKind::Semi) {
            Ok(_) => Ok(()),
            Err(err) if recover => {
                err.emit();
                Ok(())
            }
            Err(err) => Err(err),
        }
    }

    /// Expects and consumes the closing delimiter `ket` of a sequence.
    ///
    /// When recovering incomplete input, a delimiter missing at EOF is reported once, for the
    /// innermost sequence, and the partial sequence is kept.
    #[track_caller]
    fn expect_seq_close(&mut self, ket: TokenKind) -> PResult<'sess, ()> {
        if self.recover_incomplete_input && self.token.is_eof() {
            if self.last_unexpected_token_span != Some(self.token.span)
                && let Err(err) = self.expect(ket)
            {
                err.emit();
            }
            return Ok(());
        }
        self.expect(ket).map(drop)
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
        self.parse_unspanned_seq::<_, 8>(
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
    #[inline(always)]
    fn parse_unspanned_seq<T, const N: usize>(
        &mut self,
        bra: TokenKind,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        self.expect(bra)?;
        self.parse_seq_to_end::<_, N>(ket, sep, allow_empty, f)
    }

    /// Parses a sequence, including only the closing delimiter. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    #[inline(always)]
    fn parse_seq_to_end<T, const N: usize>(
        &mut self,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        let (val, recovered) = self.parse_seq_to_before_tokens::<_, N>(ket, sep, allow_empty, f)?;
        if recovered == Recovered::No {
            self.expect_seq_close(ket)?;
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
        self.parse_seq_to_before_tokens::<_, 8>(ket, sep, allow_empty, f)
    }

    /// Parses a sequence until the specified delimiters. The function
    /// `f` must consume tokens until reaching the next separator or
    /// closing bracket.
    #[track_caller]
    fn parse_seq_to_before_tokens<T, const N: usize>(
        &mut self,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, (BoxSlice<'ast, T>, Recovered)> {
        let mut v = SmallVec::<[T; N]>::new();
        let recovered = self.parse_seq_into(ket, sep, allow_empty, f, |_, value| v.push(value))?;
        Ok((self.alloc_drain(&mut v), recovered))
    }

    /// Parses a brace-delimited block, collecting the items in `buf` instead of on the stack.
    #[track_caller]
    fn parse_block_seq<T>(
        &mut self,
        buf: impl Fn(&mut Self) -> &mut Vec<T> + Copy,
        f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, BoxSlice<'ast, T>> {
        self.expect(TokenKind::OpenDelim(Delimiter::Brace))?;
        let start = buf(self).len();
        let ket = TokenKind::CloseDelim(Delimiter::Brace);
        let res =
            self.parse_seq_into(ket, SeqSep::none(), true, f, |this, value| buf(this).push(value));
        let arena = self.arena;
        let items = buf(self);
        // SAFETY: The moved elements are removed from `items` without being dropped.
        let slice = unsafe {
            let slice = arena.alloc_thin_slice_unchecked((), &items[start..]);
            items.set_len(start);
            slice
        };
        if res? == Recovered::No {
            self.expect_seq_close(ket)?;
        }
        Ok(slice)
    }

    /// Parses a sequence until the specified delimiters, passing each item to `push`.
    #[track_caller]
    fn parse_seq_into<T>(
        &mut self,
        ket: TokenKind,
        sep: SeqSep,
        allow_empty: bool,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, T>,
        mut push: impl FnMut(&mut Self, T),
    ) -> PResult<'sess, Recovered> {
        let mut first = true;
        let mut recovered = Recovered::No;
        let mut trailing = false;

        loop {
            let required_first = first && !allow_empty;
            if !required_first && self.check(ket) {
                break;
            }
            if !required_first && self.token.kind == TokenKind::Eof {
                // The caller reports the missing `ket`; list the separator as an alternative.
                if !first && let Some(sep_kind) = sep.sep {
                    self.push_expected(ExpectedToken::Token(sep_kind));
                }
                break;
            }

            if first {
                // No separator for the first element.
                first = false;
            } else if let Some(sep_kind) = sep.sep {
                // Check for a separator.
                let recovered_ = self.expect(sep_kind)?;
                if recovered_ == Recovered::Yes {
                    recovered = Recovered::Yes;
                    break;
                }

                if self.check(ket) {
                    trailing = true;
                    break;
                }
            }

            match f(self) {
                Ok(value) => push(self, value),
                Err(err) if self.can_recover_sequence(ket) => {
                    err.emit();
                    if ket == TokenKind::CloseDelim(Delimiter::Brace)
                        && sep.sep.is_none()
                        && self.token.kind == TokenKind::Semi
                    {
                        self.bump();
                        continue;
                    }
                    if self.token.kind == ket {
                        self.bump();
                    }
                    recovered = Recovered::Yes;
                    break;
                }
                Err(err) => return Err(err),
            }
        }

        if let Some(sep_kind) = sep.sep {
            let open_close_delim = first && allow_empty;
            if !open_close_delim
                && sep.trailing_sep_required
                && !trailing
                && let Err(e) = self.expect(sep_kind)
            {
                e.emit();
                // At EOF, this error also lists `ket`, so the caller must not expect it again.
                if self.token.is_eof() {
                    recovered = Recovered::Yes;
                }
            }
            if !sep.trailing_sep_allowed && trailing {
                let msg = format!("trailing `{sep_kind}` separator is not allowed");
                self.dcx().emit_err(self.prev_token.span, msg);
            }
        }

        Ok(recovered)
    }

    fn can_recover_sequence(&self, ket: TokenKind) -> bool {
        self.recover_incomplete_input
            && (self.token.kind == ket
                || matches!(
                    self.token.kind,
                    TokenKind::Semi | TokenKind::Eof | TokenKind::CloseDelim(_)
                ))
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
        // The doc comments before the current token. A run of adjacent `///` comments is read as
        // one, like solc does, and `run_start` is the first comment of the current run.
        let mut run_start = 0;
        // The span of the previous comment, if it is a `///` doc comment.
        let mut prev_line_doc = None;
        while let Some((is_doc, kind, symbol)) = self.token.comment() {
            if is_doc {
                let span = self.token.span;
                let continues_run = prev_line_doc.is_some_and(|prev: Span| {
                    kind == ast::CommentKind::Line
                        && self
                            .sess
                            .source_map()
                            .span_to_snippet(Span::new(prev.hi(), span.lo()))
                            .is_ok_and(|between| doc_text::continues_line_comment(&between))
                });
                if !continues_run {
                    self.parse_natspec_run(run_start);
                    run_start = self.docs.len();
                }
                self.docs.push(DocComment { kind, span, symbol, natspec: BoxSlice::default() });
                prev_line_doc = (kind == ast::CommentKind::Line).then_some(span);
            } else {
                prev_line_doc = None;
            }
            // Don't set `prev_token` on purpose.
            self.token = self.next_token();
        }
        self.parse_natspec_run(run_start);

        self.expected_tokens.clear();
    }

    /// Parses the NatSpec items of the doc comments from `start` on, which are one comment for
    /// solc, and gives them to the first.
    fn parse_natspec_run(&mut self, start: usize) {
        let Some(docs) = self.docs.get(start..).filter(|docs| !docs.is_empty()) else { return };
        let span = docs[0].span.to(docs[docs.len() - 1].span);
        let kind = docs[0].kind;
        let text = DocText::read(docs);
        let mut items = SmallVec::new();
        parse_natspec(span, &text, kind, self.in_yul, self.dcx(), &mut items);
        if !items.is_empty() {
            self.docs[start].natspec = self.alloc_drain(&mut items);
        }
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

    /// Runs `f` while parsing a standalone Yul file.
    #[inline]
    fn in_pure_yul<R>(&mut self, f: impl FnOnce(&mut Self) -> R) -> R {
        let old = std::mem::replace(&mut self.pure_yul, true);
        let res = f(self);
        self.pure_yul = old;
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
        let mut err = self.dcx().err_at(self.token.span, "recursion limit reached");
        if !self.prev_token.span.is_dummy() {
            err = err.span_label(self.prev_token.span, format!("while parsing {context}"));
        }
        err
    }
}

/// Common parsing methods.
impl<'sess, 'ast, 'cb> Parser<'sess, 'ast, 'cb> {
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
        if !ident.is_yul_builtin() && self.is_reserved_yul_ident(ident) {
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
        if self.in_yul && ident.is_future_yul_reserved_identifier(self.sess.opts.evm_version) {
            self.dcx().emit_warn_code(
                ident.span,
                error_code!(5470),
                format!(
                    "`{ident}` will be promoted to Yul reserved identifier in the future and will not be allowed anymore as an identifier"
                ),
            );
        } else if if self.in_yul {
            self.is_reserved_yul_ident(ident)
        } else {
            // `[u]fixedMxN` types are keywords outside of Yul.
            ident.is_reserved(false) || ast::ElementaryType::is_fixed_mxn(ident.name)
        } {
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

    #[inline]
    fn is_reserved_yul_ident(&self, ident: Ident) -> bool {
        ident.is_yul_keyword()
            || (ident.is_yul_builtin()
                && !ident.is_future_yul_reserved_identifier(self.sess.opts.evm_version)
                && (self.pure_yul || ident.is_reserved_yul_builtin_in(self.sess.opts.evm_version)))
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
        let mut err = self.dcx().err_at(span, msg);

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
// Solc implementation notes:
// - Reference: https://github.com/ethereum/solidity/blob/develop/libsolidity/analysis/DocStringTagParser.cpp
// - The scanner merges adjacent `///` lines into one doc comment and strips the decoration of `/**
//   */` comments. The tag parser then joins the lines of each tag.
//
// Our implementation:
// - Reads the same text as solc's scanner (`DocText`), so adjacent `///` comments are one comment
//   with their items attached to the first, and each item holds its joined lines.
// - Defers validation to lowering phase.
// - Follows Solc's Yul behavior: silently ignores unknown tags in Yul context https://github.com/argotorg/solidity/blob/2ca5fb3b6adcb1a8fb2c0904fb37526121cf2c72/libyul/AsmParser.cpp#L151

/// The item whose content is being read.
struct PendingItem {
    kind: ast::NatSpecKind,
    span: Span,
    /// Offset into the text where the content starts.
    start: usize,
    /// Whether the item is the `@notice` of text before any tag, which is dropped when blank.
    implicit: bool,
}

/// Builds items from the text of a doc comment.
struct ItemBuilder<'a> {
    text: &'a DocText,
    /// The interned text, for items whose content is a slice of it.
    symbol: Option<Symbol>,
}

impl ItemBuilder<'_> {
    /// Pushes the item with the content between its start and `end`.
    fn push(&mut self, items: &mut SmallVec<[ast::NatSpecItem; 6]>, item: PendingItem, end: usize) {
        let content = self.text.as_str();
        let raw = content.get(item.start..end).unwrap_or_default();
        if item.implicit && raw.trim_ascii().is_empty() {
            return;
        }

        let lo = raw.len() - raw.trim_ascii_start().len();
        let content_lo =
            if lo == raw.len() { item.span.hi() } else { self.text.source_pos(item.start + lo) };

        // Like solc, the lines are joined with a space, unless the next one starts with a blank.
        let (symbol, content_start, content_end) = if raw.contains('\n') {
            let mut joined = String::with_capacity(raw.len());
            for (index, line) in raw.split('\n').enumerate() {
                // An `@` that names no tag is dropped with the blank after it, and the text
                // before it, as solc does.
                let line = if index > 0
                    && let Some(rest) = line.trim_start_matches([' ', '\t']).strip_prefix('@')
                    && (rest.is_empty() || rest.starts_with([' ', '\t']))
                {
                    rest.get(1..).unwrap_or_default()
                } else {
                    line
                };
                if index > 0 && !line.starts_with([' ', '\t']) {
                    joined.push(' ');
                }
                joined.push_str(line);
            }
            (Symbol::intern(&joined), 0, joined.len())
        } else {
            (*self.symbol.get_or_insert_with(|| Symbol::intern(content)), item.start, end)
        };
        items.push(ast::NatSpecItem {
            kind: item.kind,
            span: item.span,
            symbol,
            content_lo,
            content_start: content_start as u32,
            content_end: content_end as u32,
        });
    }
}

/// Parses NatSpec items from the text of a doc comment and appends them to `items`.
fn parse_natspec(
    comment_span: Span,
    text: &DocText,
    comment_kind: ast::CommentKind,
    in_yul: bool,
    dcx: &DiagCtxt,
    items: &mut SmallVec<[ast::NatSpecItem; 6]>,
) {
    let content = text.as_str();
    let bytes = content.as_bytes();
    let mut builder = ItemBuilder { text, symbol: None };

    #[cold]
    #[inline(never)]
    fn invalid_tag(dcx: &DiagCtxt, tag: &str, span: Span) {
        dcx.emit_warn_code(
            span,
            error_code!(6546),
            format!("invalid natspec tag '@{tag}', custom tags must use format '@custom:name'"),
        );
    }

    // Returns the offset of the first non-blank byte at or after `from`, up to `to`.
    let skip_blanks = |from: usize, to: usize| {
        from + bytes[from..to].iter().take_while(|b| matches!(b, b' ' | b'\t')).count()
    };

    // Check if '@' is located at the logical start of the line.
    let is_line_start = |line_start: usize, at_pos: usize| {
        match comment_kind {
            // can only be followed by whitespace
            ast::CommentKind::Line => bytes[line_start..at_pos].trim_ascii().is_empty(),

            // can only be followed by whitespaces and '*' decorations, which
            // may repeat (`* * @return` still starts a tag, as in solc, where
            // the scanner strips one decoration and the parser finds the tag
            // anywhere in the line).
            ast::CommentKind::Block => {
                let mut rest = bytes[line_start..at_pos].trim_ascii_start();
                while let Some(stripped) = rest.strip_prefix(b"*") {
                    rest = stripped.trim_ascii_start();
                }
                rest.is_empty()
            }
        }
    };

    // Text before the first tag is a `@notice`.
    let mut pending = Some(PendingItem {
        kind: ast::NatSpecKind::Notice,
        span: comment_span,
        start: skip_blanks(0, bytes.iter().position(|&b| b == b'\n').unwrap_or(bytes.len())),
        implicit: true,
    });
    // Whether the tag has no content on its line, so its content starts on the next one.
    let mut content_on_next_line = false;

    // Iterate over each line and look for tags.
    let (mut line_start, mut prev_line_end) = (0, 0);
    for line_end in memchr::memchr_iter(b'\n', bytes).chain(std::iter::once(bytes.len())) {
        let tag_offset = memchr::memchr(b'@', &bytes[line_start..line_end])
            .filter(|&offset| is_line_start(line_start, line_start + offset));
        let starts_tag = tag_offset.is_some_and(|offset| {
            bytes.get(line_start + offset + 1).is_some_and(|b| !matches!(b, b' ' | b'\t' | b'\n'))
        });
        if content_on_next_line && !starts_tag {
            // Like solc, a line after a tag without content is its content. Unlike solc, a tag is
            // never content, so `@custom:solar-cold` can be followed by another tag.
            if let Some(item) = &mut pending {
                item.start = skip_blanks(line_start, line_end);
            }
            content_on_next_line = false;
        } else if let Some(tag_offset) = tag_offset {
            content_on_next_line = false;
            let tag_start = line_start + tag_offset + 1;

            // The tag name must immediately follow the '@': `@ param` is
            // plain text continuing the previous tag, matching solc.
            let name_end = tag_start
                + bytes[tag_start..line_end]
                    .iter()
                    .position(|b| matches!(b, b' ' | b'\t'))
                    .unwrap_or(line_end - tag_start);
            let tag = &content[tag_start..name_end];
            let rest_start = skip_blanks(name_end, line_end);
            if !tag.is_empty() {
                // Calculate span: from the char after '@' to end of tag name.
                let tag_lo = text.source_pos(line_start + tag_offset).0 + 1; // +1 for '@'
                let tag_hi = tag_lo + tag.len() as u32;
                let span = Span::new(BytePos(tag_lo), BytePos(tag_hi));
                let mut start = rest_start;

                let kind = match tag {
                    "title" => Some(ast::NatSpecKind::Title),
                    "author" => Some(ast::NatSpecKind::Author),
                    "notice" => Some(ast::NatSpecKind::Notice),
                    "dev" => Some(ast::NatSpecKind::Dev),
                    "param" | "inheritdoc" => {
                        // Names are identifier prefixes, mirroring solc: `@param -`
                        // documents an unnamed parameter with an empty name.
                        let (name, content_start) =
                            crate::natspec::split_once_ident(content, rest_start, line_end);
                        start = content_start;
                        let name_lo = text.source_pos(rest_start);
                        let name_span = Span::new(name_lo, name_lo + name.len() as u32);
                        let ident = Ident::new(Symbol::intern(name), name_span);
                        Some(match tag {
                            "param" => ast::NatSpecKind::Param { name: ident },
                            "inheritdoc" => ast::NatSpecKind::Inheritdoc { contract: ident },
                            _ => unreachable!(),
                        })
                    }
                    "return" => Some(ast::NatSpecKind::Return { name: None }),
                    _ => {
                        if let Some(custom_tag) = tag.strip_prefix("custom:") {
                            let ident = Ident::new(Symbol::intern(custom_tag), comment_span);
                            Some(ast::NatSpecKind::Custom { name: ident })
                        } else if ast::NATSPEC_INTERNAL_TAGS[..].contains(&tag) {
                            let ident = Ident::new(Symbol::intern(tag), comment_span);
                            Some(ast::NatSpecKind::Internal { tag: ident })
                        } else {
                            // Emit error for invalid solidity tags, but ignore in Yul.
                            if !in_yul {
                                invalid_tag(dcx, tag, comment_span);
                            }
                            None
                        }
                    }
                };

                if let Some(kind) = kind {
                    if let Some(item) = pending.take() {
                        builder.push(items, item, prev_line_end);
                    }
                    content_on_next_line =
                        name_end == line_end && !matches!(tag, "param" | "inheritdoc");
                    pending = Some(PendingItem { kind, span, start, implicit: false });
                }
            }
        }

        prev_line_end = line_end;
        line_start = line_end + 1;
    }
    if let Some(item) = pending {
        builder.push(items, item, bytes.len());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{Session, SourceMap};

    fn check_natspec_item(
        sm: &SourceMap,
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
            ast::NatSpecKind::Return { .. } if kind == "return" => None,
            ast::NatSpecKind::Inheritdoc { contract } if kind == "inheritdoc" => {
                Some(contract.name.as_str())
            }
            ast::NatSpecKind::Custom { name } if kind == "custom" => Some(name.name.as_str()),
            ast::NatSpecKind::Internal { tag } if kind == "internal" => Some(tag.name.as_str()),
            _ => panic!("kind mismatch: expected {kind}, got {:?}", item.kind),
        };
        assert_eq!(actual_name, name);

        if let Some(expected) = content {
            assert_eq!(item.content(), expected);
        }
    }

    #[test]
    fn parse_file_import_callback() {
        let src = r#"
import "a.sol";
contract C {}
import * as B from "b.sol";
"#;

        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let arena = ast::Arena::new();
            let mut imports = Vec::new();
            let mut parser =
                Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), src)
                    .expect("failed to create parser");

            parser.set_import_callback(|id, span, import| {
                imports.push((id, span, import.path.value.as_str().to_string()));
            });
            let ast = parser.parse_file().expect("failed to parse file");
            drop(parser);

            assert_eq!(ast.items.len(), 3);
            assert_eq!(imports.len(), 2);
            assert_eq!(imports[0].0, ast::ItemId::new(0));
            assert_eq!(imports[0].2, "a.sol");
            assert_eq!(imports[1].0, ast::ItemId::new(2));
            assert_eq!(imports[1].2, "b.sol");
            assert_eq!(
                sess.source_map().span_to_snippet(imports[0].1).unwrap(),
                r#"import "a.sol";"#
            );
        });
    }

    #[test]
    fn nonempty_sequence_requires_a_first_element() {
        for (allow_empty, succeeds) in [(true, true), (false, false)] {
            let sess = Session::builder()
                .with_buffer_emitter(Default::default())
                .single_threaded()
                .build();
            sess.enter_sequential(|| {
                let arena = ast::Arena::new();
                let mut parser =
                    Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), "{}")
                        .unwrap();
                let result = parser
                    .parse_delim_comma_seq(Delimiter::Brace, allow_empty, Parser::parse_ident)
                    .map_err(|err| err.emit());

                assert_eq!(result.is_ok(), succeeds);
            });
        }
    }

    #[test]
    fn parse_natspec_line_cmnts() {
        let src = r#"
/// @title MyContract
/// @author Alice
/// @notice This is a notice
/// that spans multiple lines
///   and continues here
/// @dev This is dev documentation
/// @param x The input parameter
/// @return result The return value
/// @inheritdoc BaseContract
/// @custom:security High priority
/// @solidity memory-safe
"#;

        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let arena = ast::Arena::new();
            let mut parser = Parser::from_source_code(&sess, &arena, "test.sol".to_string().into(), src)
                .expect("failed to create parser");

            let sm = sess.source_map();
            let docs = parser.parse_doc_comments();

            // The comments are one for solc, so the first has all the items.
            assert_eq!(docs.len(), 11);
            assert!(docs[1..].iter().all(|doc| doc.natspec.is_empty()));
            let items = &docs[0].natspec;
            assert_eq!(items.len(), 9);

            let check = |i: usize, snip, kind, name, content| {
                check_natspec_item(sm, &items[i], snip, kind, name, content)
            };

            check(0, "title", "title", None, Some("MyContract"));
            check(1, "author", "author", None, Some("Alice"));
            check(
                2,
                "notice",
                "notice",
                None,
                Some("This is a notice that spans multiple lines   and continues here"),
            );
            check(3, "dev", "dev", None, Some("This is dev documentation"));
            check(4, "param", "param", Some("x"), Some("The input parameter"));
            check(5, "return", "return", None, Some("result The return value"));
            check(6, "inheritdoc", "inheritdoc", Some("BaseContract"), Some(""));
            check(7, "custom:security", "custom", Some("security"), Some("High priority"));
            check(8, "solidity", "internal", Some("solidity"), Some("memory-safe"));

            assert_eq!(sm.span_to_snippet(docs.span()).unwrap(), "/// @title MyContract\n/// @author Alice\n/// @notice This is a notice\n/// that spans multiple lines\n///   and continues here\n/// @dev This is dev documentation\n/// @param x The input parameter\n/// @return result The return value\n/// @inheritdoc BaseContract\n/// @custom:security High priority\n/// @solidity memory-safe");
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
 *   and continues here
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

            let items = &docs[0].natspec;
            assert_eq!(items.len(), 9);

            let check = |i: usize, span, kind, name, content| {
                check_natspec_item(sm, &items[i], span, kind, name, content)
            };

            check(0, "title", "title", None, Some("MyContract"));
            check(1, "author", "author", None, Some("Alice"));
            check(
                2,
                "notice",
                "notice",
                None,
                Some("This is a notice that spans multiple lines   and continues here"),
            );
            check(3, "dev", "dev", None, Some("This is dev documentation"));
            check(4, "param", "param", Some("x"), Some("The input parameter"));
            check(5, "return", "return", None, Some("result The return value"));
            check(6, "inheritdoc", "inheritdoc", Some("BaseContract"), Some(""));
            check(7, "custom:security", "custom", Some("security"), Some("High priority"));
            check(8, "src", "internal", Some("src"), Some("0:123:456"));

            assert_eq!(sm.span_to_snippet(docs.span()).unwrap(), "/**\n * @title MyContract\n * @author Alice\n * @notice This is a notice\n * that spans multiple lines\n *   and continues here\n * @dev This is dev documentation\n * @param x The input parameter\n * @return result The return value\n * @inheritdoc BaseContract\n * @custom:security High priority\n * @src 0:123:456\n */");
        });
    }

    #[test]
    fn parse_natspec_line_cmnts_no_tags() {
        let src = r#"
/// This is a simple comment
/// It has no tags at all
///
///   Just plain documentation
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
            assert_eq!(docs.len(), 4);
            assert!(docs[1..].iter().all(|doc| doc.natspec.is_empty()));

            assert_eq!(docs[0].natspec.len(), 1);
            let item = &docs[0].natspec[0];
            let span = sm.span_to_snippet(item.span).unwrap();
            check_natspec_item(
                sm,
                item,
                &span,
                "notice",
                None,
                Some("This is a simple comment It has no tags at all   Just plain documentation"),
            );
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
                item,
                &snip,
                "notice",
                None,
                Some("This is a block comment with multiple lines but no tags at all"),
            );
        });
    }
}

//! Solidity and Yul lexer.

use solar_ast::{
    Base, StrKind,
    token::{CommentKind, Token, TokenKind, TokenLitKind},
};
use solar_data_structures::{hint::cold_path, map::FxHashMap};
use solar_interface::{
    BytePos, Session, Span, Symbol, diagnostics::DiagCtxt, kw, source_map::SourceFile, sym,
};
use std::collections::hash_map::Entry;

mod cursor;
use cursor::token::{RawLiteralKind, RawToken, RawTokenKind};
pub use cursor::*;

pub mod unescape;

mod unicode_chars;

mod utf8;

/// Solidity and Yul lexer.
///
/// Converts a [`Cursor`]'s output from simple [`RawTokenKind`]s into rich [`TokenKind`]s, by
/// converting strings into interned symbols, and running additional validation.
pub struct Lexer<'sess, 'src> {
    /// Cursor for getting lexer tokens.
    cursor: Cursor<'src>,
    /// The absolute offset within the source_map of the current character.
    pos: BytePos,

    /// The parsing context.
    pub(crate) sess: &'sess Session,
    /// Initial position, read-only.
    start_pos: BytePos,
    /// Source text to tokenize.
    src: &'src str,
    /// Local cache for identifiers, which tend to repeat heavily within one source.
    symbol_cache: FxHashMap<&'src str, Symbol>,
    /// Tiny cache in front of `symbol_cache` for repeated identifiers.
    symbol_cache_l1: [Option<(&'src str, Symbol)>; 32],
    /// Whether doc-comments should be emitted for the parser.
    retain_doc_comments: bool,
    /// When a "unknown start of token: \u{a0}" has already been emitted earlier
    /// in this file, it's safe to treat further occurrences of the non-breaking
    /// space character as whitespace.
    nbsp_is_whitespace: bool,
}

impl<'sess, 'src> Lexer<'sess, 'src> {
    /// Creates a new `Lexer` for the given source string.
    pub fn new(sess: &'sess Session, src: &'src str) -> Self {
        Self::with_start_pos(sess, src, BytePos(0))
    }

    /// Creates a new `Lexer` for the given source file.
    ///
    /// Note that the source file must be added to the source map before calling this function.
    pub fn from_source_file(sess: &'sess Session, file: &'src SourceFile) -> Self {
        Self::with_start_pos(sess, &file.src, file.start_pos)
    }

    /// Creates a new `Lexer` for the given source string and starting position.
    pub fn with_start_pos(sess: &'sess Session, src: &'src str, start_pos: BytePos) -> Self {
        assert!(sess.is_entered(), "session should be entered before lexing");
        let symbol_cache = if src.len() >= 64 * 1024 {
            FxHashMap::with_capacity_and_hasher(src.len() / 128, Default::default())
        } else {
            FxHashMap::default()
        };
        Self {
            sess,
            start_pos,
            pos: start_pos,
            src,
            cursor: Cursor::new(src),
            symbol_cache,
            symbol_cache_l1: [None; 32],
            retain_doc_comments: true,
            nbsp_is_whitespace: false,
        }
    }

    /// Sets whether doc-comment tokens should be emitted for the parser.
    ///
    /// When disabled, doc-comments without invalid NatSpec tags are skipped before interning their
    /// body. Comments that may emit a warning are still tokenized so the parser can report the
    /// diagnostic with the same NatSpec rules used when docs are retained.
    pub fn set_retain_doc_comments(&mut self, retain: bool) {
        self.retain_doc_comments = retain;
    }

    /// Returns a reference to the diagnostic context.
    #[inline]
    pub fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    /// Consumes the lexer and collects the remaining tokens into a vector.
    ///
    /// Note that this skips comments, as [required by the parser](crate::Parser::new).
    ///
    /// Prefer using this method instead of manually collecting tokens using [`Iterator`].
    pub fn into_tokens(mut self) -> Vec<Token> {
        // This is an estimate of the number of tokens in the source.
        let mut tokens = Vec::with_capacity(self.src.len() / 8);
        loop {
            let token = self.slop_for_parser();
            if token.is_eof() {
                break;
            }
            tokens.push(token);
        }
        trace!(
            src.len = self.src.len(),
            tokens.len = tokens.len(),
            tokens.capacity = tokens.capacity(),
            ratio = %format_args!("{:.2}", self.src.len() as f64 / tokens.len() as f64),
            "lexed"
        );
        tokens
    }

    /// Slops up a token from the input string.
    ///
    /// Advances the lexer by the length of the token.
    /// Prefer using `self` as an iterator instead.
    pub fn slop(&mut self) -> Token {
        self.slop_with_comment_handling::<false>()
    }

    /// Parser-specialized tokenization path.
    ///
    /// This intentionally duplicates the generic `slop` cooking logic so the parser can skip
    /// whitespace and non-doc comments before building a token. Keeping the branch split avoids a
    /// hot-path comment/whitespace policy flag in normal parsing.
    pub(crate) fn slop_for_parser(&mut self) -> Token {
        let mut swallow_next_invalid = 0;
        loop {
            let mut skipped = 0;
            let RawToken { kind: raw_kind, len } = self.cursor.slop_skip_whitespace(&mut skipped);
            self.pos += skipped;
            let start = self.pos;
            self.pos += len;

            // Now "cook" the token, converting the simple `RawTokenKind` into a rich `TokenKind`.
            // This turns strings into interned symbols and runs additional validation.
            let kind = match raw_kind {
                RawTokenKind::LineComment { is_doc } => {
                    if !is_doc {
                        continue;
                    }

                    // Opening delimiter is not included into the symbol.
                    let content_start = start + BytePos(3);
                    let content = self.str_from(content_start);
                    if !self.retain_doc_comments
                        && !doc_comment_has_invalid_natspec_tag(content, CommentKind::Line)
                    {
                        continue;
                    }
                    self.cook_doc_comment(content_start, content, is_doc, CommentKind::Line)
                }
                RawTokenKind::BlockComment { is_doc, terminated } => {
                    if !terminated {
                        cold_path();
                        let msg = if is_doc {
                            "unterminated block doc-comment"
                        } else {
                            "unterminated block comment"
                        };
                        self.dcx().err(msg).span(self.new_span(start, self.pos)).emit();
                    }

                    if !is_doc {
                        continue;
                    }

                    // Opening delimiter and closing delimiter are not included into the symbol.
                    let content_start = start + BytePos(3);
                    let content_end = self.pos - (terminated as u32) * 2;
                    let content = self.str_from_to(content_start, content_end);
                    if !self.retain_doc_comments
                        && !doc_comment_has_invalid_natspec_tag(content, CommentKind::Block)
                    {
                        continue;
                    }
                    self.cook_doc_comment(content_start, content, is_doc, CommentKind::Block)
                }
                RawTokenKind::Whitespace => unreachable!("parser lexer skips whitespace"),
                RawTokenKind::Ident => {
                    let sym = self.ident_symbol_from(start);
                    TokenKind::Ident(sym)
                }
                RawTokenKind::Literal { kind } => {
                    let (kind, symbol) = self.cook_literal(start, self.pos, kind);
                    TokenKind::Literal(kind, symbol)
                }

                // Expression-operator symbols.
                RawTokenKind::Eq => TokenKind::Eq,
                RawTokenKind::Lt => TokenKind::Lt,
                RawTokenKind::Le => TokenKind::Le,
                RawTokenKind::EqEq => TokenKind::EqEq,
                RawTokenKind::Ne => TokenKind::Ne,
                RawTokenKind::Ge => TokenKind::Ge,
                RawTokenKind::Gt => TokenKind::Gt,
                RawTokenKind::AndAnd => TokenKind::AndAnd,
                RawTokenKind::OrOr => TokenKind::OrOr,
                RawTokenKind::Not => TokenKind::Not,
                RawTokenKind::Tilde => TokenKind::Tilde,
                RawTokenKind::Walrus => TokenKind::Walrus,
                RawTokenKind::PlusPlus => TokenKind::PlusPlus,
                RawTokenKind::MinusMinus => TokenKind::MinusMinus,
                RawTokenKind::StarStar => TokenKind::StarStar,
                RawTokenKind::BinOp(binop) => TokenKind::BinOp(binop),
                RawTokenKind::BinOpEq(binop) => TokenKind::BinOpEq(binop),

                // Structural symbols.
                RawTokenKind::At => TokenKind::At,
                RawTokenKind::Dot => TokenKind::Dot,
                RawTokenKind::Comma => TokenKind::Comma,
                RawTokenKind::Semi => TokenKind::Semi,
                RawTokenKind::Colon => TokenKind::Colon,
                RawTokenKind::Arrow => TokenKind::Arrow,
                RawTokenKind::FatArrow => TokenKind::FatArrow,
                RawTokenKind::Question => TokenKind::Question,
                RawTokenKind::OpenDelim(delim) => TokenKind::OpenDelim(delim),
                RawTokenKind::CloseDelim(delim) => TokenKind::CloseDelim(delim),

                RawTokenKind::Unknown => {
                    if let Some(token) = self.handle_unknown_token(start, &mut swallow_next_invalid)
                    {
                        token
                    } else {
                        continue;
                    }
                }

                RawTokenKind::Eof => TokenKind::Eof,
            };
            let span = self.new_span(start, self.pos);
            return Token::new(kind, span);
        }
    }

    fn slop_with_comment_handling<const SKIP_NORMAL_COMMENTS: bool>(&mut self) -> Token {
        let mut swallow_next_invalid = 0;
        loop {
            let RawToken { kind: raw_kind, len } = self.cursor.slop();
            let start = self.pos;
            self.pos += len;

            // Now "cook" the token, converting the simple `RawTokenKind` into a rich `TokenKind`.
            // This turns strings into interned symbols and runs additional validation.
            let kind = match raw_kind {
                RawTokenKind::LineComment { is_doc } => {
                    if SKIP_NORMAL_COMMENTS && !is_doc {
                        continue;
                    }

                    // Opening delimiter is not included into the symbol.
                    let content_start = start + BytePos(if is_doc { 3 } else { 2 });
                    let content = self.str_from(content_start);
                    self.cook_doc_comment(content_start, content, is_doc, CommentKind::Line)
                }
                RawTokenKind::BlockComment { is_doc, terminated } => {
                    if !terminated {
                        cold_path();
                        let msg = if is_doc {
                            "unterminated block doc-comment"
                        } else {
                            "unterminated block comment"
                        };
                        self.dcx().err(msg).span(self.new_span(start, self.pos)).emit();
                    }

                    if SKIP_NORMAL_COMMENTS && !is_doc {
                        continue;
                    }

                    // Opening delimiter and closing delimiter are not included into the symbol.
                    let content_start = start + BytePos(if is_doc { 3 } else { 2 });
                    let content_end = self.pos - (terminated as u32) * 2;
                    let content = self.str_from_to(content_start, content_end);
                    self.cook_doc_comment(content_start, content, is_doc, CommentKind::Block)
                }
                RawTokenKind::Whitespace => {
                    continue;
                }
                RawTokenKind::Ident => {
                    let sym = self.ident_symbol_from(start);
                    TokenKind::Ident(sym)
                }
                RawTokenKind::Literal { kind } => {
                    let (kind, symbol) = self.cook_literal(start, self.pos, kind);
                    TokenKind::Literal(kind, symbol)
                }

                // Expression-operator symbols.
                RawTokenKind::Eq => TokenKind::Eq,
                RawTokenKind::Lt => TokenKind::Lt,
                RawTokenKind::Le => TokenKind::Le,
                RawTokenKind::EqEq => TokenKind::EqEq,
                RawTokenKind::Ne => TokenKind::Ne,
                RawTokenKind::Ge => TokenKind::Ge,
                RawTokenKind::Gt => TokenKind::Gt,
                RawTokenKind::AndAnd => TokenKind::AndAnd,
                RawTokenKind::OrOr => TokenKind::OrOr,
                RawTokenKind::Not => TokenKind::Not,
                RawTokenKind::Tilde => TokenKind::Tilde,
                RawTokenKind::Walrus => TokenKind::Walrus,
                RawTokenKind::PlusPlus => TokenKind::PlusPlus,
                RawTokenKind::MinusMinus => TokenKind::MinusMinus,
                RawTokenKind::StarStar => TokenKind::StarStar,
                RawTokenKind::BinOp(binop) => TokenKind::BinOp(binop),
                RawTokenKind::BinOpEq(binop) => TokenKind::BinOpEq(binop),

                // Structural symbols.
                RawTokenKind::At => TokenKind::At,
                RawTokenKind::Dot => TokenKind::Dot,
                RawTokenKind::Comma => TokenKind::Comma,
                RawTokenKind::Semi => TokenKind::Semi,
                RawTokenKind::Colon => TokenKind::Colon,
                RawTokenKind::Arrow => TokenKind::Arrow,
                RawTokenKind::FatArrow => TokenKind::FatArrow,
                RawTokenKind::Question => TokenKind::Question,
                RawTokenKind::OpenDelim(delim) => TokenKind::OpenDelim(delim),
                RawTokenKind::CloseDelim(delim) => TokenKind::CloseDelim(delim),

                RawTokenKind::Unknown => {
                    if let Some(token) = self.handle_unknown_token(start, &mut swallow_next_invalid)
                    {
                        token
                    } else {
                        continue;
                    }
                }

                RawTokenKind::Eof => TokenKind::Eof,
            };
            let span = self.new_span(start, self.pos);
            return Token::new(kind, span);
        }
    }

    #[cold]
    fn handle_unknown_token(
        &mut self,
        start: BytePos,
        swallow_next_invalid: &mut usize,
    ) -> Option<TokenKind> {
        // Don't emit diagnostics for sequences of the same invalid token
        if *swallow_next_invalid > 0 {
            *swallow_next_invalid -= 1;
            return None;
        }
        let mut it = self.str_from_to_end(start).chars();
        let c = it.next().unwrap();
        if c == '\u{00a0}' {
            // If an error has already been reported on non-breaking
            // space characters earlier in the file, treat all
            // subsequent occurrences as whitespace.
            if self.nbsp_is_whitespace {
                return None;
            }
            self.nbsp_is_whitespace = true;
        }

        let repeats = it.take_while(|c1| *c1 == c).count();
        *swallow_next_invalid = repeats;

        let (token, sugg) = unicode_chars::check_for_substitution(self, start, c, repeats + 1);

        let span = self.new_span(start, self.pos + BytePos::from_usize(repeats * c.len_utf8()));
        let msg = format!("unknown start of token: {}", escaped_char(c));
        let mut err = self.dcx().err(msg).span(span);
        if let Some(sugg) = sugg {
            match sugg {
                unicode_chars::TokenSubstitution::DirectedQuotes {
                    span,
                    suggestion: _,
                    ascii_str,
                    ascii_name,
                } => {
                    let msg = format!(
                        "Unicode characters '“' (Left Double Quotation Mark) and '”' (Right Double Quotation Mark) look like '{ascii_str}' ({ascii_name}), but are not"
                    );
                    err = err.span_help(span, msg);
                }
                unicode_chars::TokenSubstitution::Other {
                    span,
                    suggestion: _,
                    ch,
                    u_name,
                    ascii_str,
                    ascii_name,
                } => {
                    let msg = format!(
                        "Unicode character '{ch}' ({u_name}) looks like '{ascii_str}' ({ascii_name}), but it is not"
                    );
                    err = err.span_help(span, msg);
                }
            }
        }
        if c == '\0' {
            let help = "source files must contain UTF-8 encoded text, unexpected null bytes might occur when a different encoding is used";
            err = err.help(help);
        }
        if repeats > 0 {
            let note = match repeats {
                1 => "once more".to_string(),
                _ => format!("{repeats} more times"),
            };
            err = err.note(format!("character repeats {note}"));
        }
        err.emit();

        token
    }

    fn cook_doc_comment(
        &self,
        _content_start: BytePos,
        content: &str,
        is_doc: bool,
        comment_kind: CommentKind,
    ) -> TokenKind {
        TokenKind::Comment(is_doc, comment_kind, self.intern(content))
    }

    fn cook_literal(
        &self,
        start: BytePos,
        end: BytePos,
        kind: RawLiteralKind,
    ) -> (TokenLitKind, Symbol) {
        match kind {
            RawLiteralKind::Str { kind, terminated } => {
                if !terminated {
                    cold_path();
                    let span = self.new_span(start, end);
                    let guar = self.dcx().err("unterminated string").span(span).emit();
                    (TokenLitKind::Err(guar), self.symbol_from_to(start, end))
                } else {
                    (kind.into(), self.cook_quoted(kind, start, end))
                }
            }
            RawLiteralKind::Int { base, empty_int } => {
                if empty_int {
                    cold_path();
                    let span = self.new_span(start, end);
                    self.dcx().err("no valid digits found for number").span(span).emit();
                    (TokenLitKind::Integer, self.symbol_from_to(start, end))
                } else {
                    if matches!(base, Base::Binary | Base::Octal) {
                        cold_path();
                        let start = start + 2;
                        // To uncomment if binary and octal literals are ever supported.
                        /*
                        let base = base as u32;
                        let s = self.str_from_to(start, end);
                        for (i, c) in s.char_indices() {
                            if c != '_' && c.to_digit(base).is_none() {
                                cold_path();
                                let msg = format!("invalid digit for a base {base} literal");
                                let lo = start + BytePos::from_usize(i);
                                let hi = lo + BytePos::from_usize(c.len_utf8());
                                let span = self.new_span(lo, hi);
                                self.dcx().err(msg).span(span).emit();
                            }
                        }
                        */
                        let msg = format!("integers in base {base} are not supported");
                        self.dcx().err(msg).span(self.new_span(start, end)).emit();
                    }
                    (TokenLitKind::Integer, self.symbol_from_integer(start, end, base))
                }
            }
            RawLiteralKind::Rational { base, empty_exponent } => {
                if empty_exponent {
                    cold_path();
                    let span = self.new_span(start, self.pos);
                    self.dcx().err("expected at least one digit in exponent").span(span).emit();
                }

                let unsupported_base =
                    matches!(base, Base::Binary | Base::Octal | Base::Hexadecimal);
                if unsupported_base {
                    cold_path();
                    let msg = format!("{base} rational numbers are not supported");
                    self.dcx().err(msg).span(self.new_span(start, end)).emit();
                }

                (TokenLitKind::Rational, self.symbol_from_to(start, end))
            }
        }
    }

    fn cook_quoted(&self, kind: StrKind, start: BytePos, end: BytePos) -> Symbol {
        // Account for quote (`"` or `'`) and prefix.
        let content_start = start + 1 + BytePos(kind.prefix().len() as u32);
        let content_end = end - 1;
        let lit_content = self.str_from_to(content_start, content_end);
        self.intern(lit_content)
    }

    #[inline]
    fn new_span(&self, lo: BytePos, hi: BytePos) -> Span {
        Span::new_unchecked(lo, hi)
    }

    #[inline]
    fn src_index(&self, pos: BytePos) -> usize {
        (pos - self.start_pos).to_usize()
    }

    fn ident_symbol_from(&mut self, start: BytePos) -> Symbol {
        let s = self.str_from_to(start, self.pos);
        if let Some(symbol) = hot_symbol(s) {
            return symbol;
        }

        let idx = l1_symbol_cache_index(s);
        if let Some((cached, symbol)) = self.symbol_cache_l1[idx]
            && cached == s
        {
            return symbol;
        }

        let sess = self.sess;
        let symbol = match self.symbol_cache.entry(s) {
            Entry::Occupied(entry) => *entry.get(),
            Entry::Vacant(entry) => {
                let symbol = sess.intern(s);
                entry.insert(symbol);
                symbol
            }
        };
        self.symbol_cache_l1[idx] = Some((s, symbol));
        symbol
    }

    /// Slice of the source text from `start` up to but excluding `self.pos`,
    /// meaning the slice does not include the character `self.ch`.
    #[cfg_attr(debug_assertions, track_caller)]
    fn str_from(&self, start: BytePos) -> &'src str {
        self.str_from_to(start, self.pos)
    }

    /// Same as `symbol_from`, with an explicit endpoint.
    #[cfg_attr(debug_assertions, track_caller)]
    fn symbol_from_to(&self, start: BytePos, end: BytePos) -> Symbol {
        self.intern(self.str_from_to(start, end))
    }

    fn symbol_from_integer(&self, start: BytePos, end: BytePos, base: Base) -> Symbol {
        if base == Base::Decimal {
            let start = self.src_index(start);
            let end = self.src_index(end);
            if end == start + 1 {
                let b = self.src.as_bytes()[start];
                if b.is_ascii_digit() {
                    return sym::integer(b - b'0');
                }
            }
        } else if base == Base::Hexadecimal {
            match self.str_from_to(start, end).as_bytes() {
                b"0x00" => return sym::zero_x00,
                b"0x01" => return sym::zero_x01,
                b"0x04" => return sym::zero_x04,
                b"0x0c" => return sym::zero_x0c,
                b"0x14" => return sym::zero_x14,
                b"0x1c" => return sym::zero_x1c,
                b"0x1f" => return sym::zero_x1f,
                b"0x20" => return sym::zero_x20,
                b"0x40" => return sym::zero_x40,
                b"0x60" => return sym::zero_x60,
                b"0x80" => return sym::zero_x80,
                b"0xff" => return sym::zero_xff,
                _ => {}
            }
        }
        self.symbol_from_to(start, end)
    }

    /// Slice of the source text spanning from `start` until the end.
    #[cfg_attr(debug_assertions, track_caller)]
    fn str_from_to_end(&self, start: BytePos) -> &'src str {
        self.str_from_to(start, self.start_pos + BytePos::from_usize(self.src.len()))
    }

    /// Slice of the source text spanning from `start` up to but excluding `end`.
    #[cfg_attr(debug_assertions, track_caller)]
    fn str_from_to(&self, start: BytePos, end: BytePos) -> &'src str {
        let range = self.src_index(start)..self.src_index(end);
        if cfg!(debug_assertions) {
            &self.src[range]
        } else {
            // SAFETY: `start` and `end` are produced from lexer byte positions over `self.src`;
            // debug builds use checked slicing to catch any invariant breakage.
            unsafe { self.src.get_unchecked(range) }
        }
    }

    fn intern(&self, s: &str) -> Symbol {
        self.sess.intern(s)
    }
}

fn hot_symbol(s: &str) -> Option<Symbol> {
    // Keep the highest-frequency pre-interned Solidity/Yul words out of the interner hot path.
    let bytes = s.as_bytes();
    if let [byte] = *bytes {
        return Some(match byte {
            b'a' => sym::a,
            b'b' => sym::b,
            b'c' => sym::c,
            b'd' => sym::d,
            b'i' => sym::i,
            b'm' => sym::m,
            b'n' => sym::n_,
            b'o' => sym::o,
            b'p' => sym::p,
            b'r' => sym::r,
            b's' => sym::s,
            b'x' => sym::x,
            b'y' => sym::y,
            b'z' => sym::z,
            _ => return None,
        });
    }

    Some(match bytes {
        b"add" => kw::Add,
        b"address" => kw::Address,
        b"amount" => sym::amount,
        b"and" => kw::And,
        b"args" => sym::args,
        b"assembly" => kw::Assembly,
        b"bool" => kw::Bool,
        b"break" => kw::Break,
        b"buffer" => sym::buffer,
        b"byte" => kw::Byte,
        b"bytes" => kw::Bytes,
        b"bytes32" => kw::Bytes32,
        b"calldata" => kw::Calldata,
        b"calldataload" => kw::Calldataload,
        b"constant" => kw::Constant,
        b"data" => sym::data,
        b"div" => kw::Div,
        b"end" => sym::end,
        b"error" => sym::error,
        b"eq" => kw::Eq,
        b"for" => kw::For,
        b"from" => sym::from,
        b"function" => kw::Function,
        b"gas" => kw::Gas,
        b"gt" => kw::Gt,
        b"hash" => sym::hash,
        b"id" => sym::id,
        b"if" => kw::If,
        b"int256" => kw::Int256,
        b"internal" => kw::Internal,
        b"iszero" => kw::Iszero,
        b"key" => sym::key,
        b"keccak256" => kw::Keccak256,
        b"let" => kw::Let,
        b"length" => sym::length,
        b"lt" => kw::Lt,
        b"map" => sym::map,
        b"memory" => kw::Memory,
        b"mload" => kw::Mload,
        b"mstore" => kw::Mstore,
        b"mstore8" => kw::Mstore8,
        b"mul" => kw::Mul,
        b"not" => kw::Not,
        b"offset" => sym::offset,
        b"or" => kw::Or,
        b"private" => kw::Private,
        b"pure" => kw::Pure,
        b"public" => kw::Public,
        b"result" => sym::result,
        b"return" => kw::Return,
        b"returns" => kw::Returns,
        b"returndatasize" => kw::Returndatasize,
        b"revert" => kw::Revert,
        b"shl" => kw::Shl,
        b"shr" => kw::Shr,
        b"sload" => kw::Sload,
        b"sstore" => kw::Sstore,
        b"staticcall" => kw::Staticcall,
        b"start" => sym::start,
        b"storage" => kw::Storage,
        b"string" => kw::String,
        b"sub" => kw::Sub,
        b"to" => sym::to,
        b"uint256" => kw::UInt256,
        b"value" => sym::value,
        b"view" => kw::View,
        b"virtual" => kw::Virtual,
        b"xor" => kw::Xor,
        _ => return None,
    })
}

#[inline]
fn l1_symbol_cache_index(s: &str) -> usize {
    debug_assert!(!s.is_empty());
    let bytes = s.as_bytes();
    (bytes[0] as usize ^ bytes.len()) & 31
}

fn doc_comment_has_invalid_natspec_tag(content: &str, comment_kind: CommentKind) -> bool {
    let bytes = content.as_bytes();
    if memchr::memchr(b'@', bytes).is_none() {
        return false;
    }

    let mut line_start = 0;
    for line_end in memchr::memchr_iter(b'\n', bytes).chain(std::iter::once(bytes.len())) {
        if let Some(tag_offset) = memchr::memchr(b'@', &bytes[line_start..line_end]) {
            let at_pos = line_start + tag_offset;
            if is_natspec_line_start(bytes, comment_kind, line_start, at_pos) {
                let tag_start = at_pos + 1;
                let tag_slice = &bytes[tag_start..line_end];
                let tag_start = tag_start + (tag_slice.len() - tag_slice.trim_ascii_start().len());
                let tag_end = memchr::memchr3(b' ', b'\t', b'\r', &bytes[tag_start..line_end])
                    .map_or(line_end, |offset| tag_start + offset);
                if !is_valid_natspec_tag(&content[tag_start..tag_end]) {
                    return true;
                }
            }
        }
        line_start = line_end + 1;
    }

    false
}

fn is_natspec_line_start(
    bytes: &[u8],
    comment_kind: CommentKind,
    line_start: usize,
    at_pos: usize,
) -> bool {
    match comment_kind {
        CommentKind::Line => bytes[line_start..at_pos].trim_ascii().is_empty(),
        CommentKind::Block => {
            let trimmed = bytes[line_start..at_pos].trim_ascii_start();
            trimmed.is_empty()
                || (trimmed.starts_with(b"*") && trimmed[1..].trim_ascii_end().is_empty())
        }
    }
}

fn is_valid_natspec_tag(tag: &str) -> bool {
    matches!(tag, "title" | "author" | "notice" | "dev" | "param" | "return" | "inheritdoc")
        || tag.starts_with("custom:")
        || solar_ast::NATSPEC_INTERNAL_TAGS.contains(&tag)
}

impl Iterator for Lexer<'_, '_> {
    type Item = Token;

    #[inline]
    fn next(&mut self) -> Option<Token> {
        let token = self.slop();
        if token.is_eof() { None } else { Some(token) }
    }
}

impl std::iter::FusedIterator for Lexer<'_, '_> {}

/// Pushes a character to a message string for error reporting
fn escaped_char(c: char) -> String {
    match c {
        '\u{20}'..='\u{7e}' => {
            // Don't escape \, ' or " for user-facing messages
            c.to_string()
        }
        _ => c.escape_default().to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use TokenKind::*;
    use solar_ast::token::BinOpToken::*;
    use std::ops::Range;

    type Expected<'a> = &'a [(Range<usize>, TokenKind)];

    // Run through the lexer to get the same input that the parser gets.
    #[track_caller]
    fn lex(src: &str, should_fail: bool, f: fn(Expected<'_>)) {
        let sess =
            Session::builder().with_buffer_emitter(Default::default()).single_threaded().build();
        sess.enter_sequential(|| {
            let file = sess.source_map().new_source_file("test".to_string(), src).unwrap();
            let tokens: Vec<_> = Lexer::from_source_file(&sess, &file)
                .filter(|t| !t.is_comment())
                .map(|t| (t.span.lo().to_usize()..t.span.hi().to_usize(), t.kind))
                .collect();
            let diags = sess.dcx.emitted_diagnostics().unwrap();
            assert_eq!(
                sess.dcx.has_errors().is_err(),
                should_fail,
                "{src:?} -> {tokens:?};\n{diags}",
            );
            f(&tokens)
        });
    }

    macro_rules! checks {
        ($( ($src:expr, $expected:expr $(,)?) ),* $(,)?) => {
            checks_full! { $( ($src, false, $expected) ),* }
        };
    }

    macro_rules! checks_full {
        ($( ($src:expr, $should_fail:expr, $expected:expr $(,)?) ),* $(,)?) => {{
            $(
                lex($src, $should_fail, |tokens| assert_eq!(tokens, $expected, "{:?}", $src));
            )*
        }};
    }

    fn lit(kind: TokenLitKind, symbol: &str) -> TokenKind {
        Literal(kind, sym(symbol))
    }

    fn id(symbol: &str) -> TokenKind {
        Ident(sym(symbol))
    }

    fn sym(s: &str) -> Symbol {
        Symbol::intern(s)
    }

    #[test]
    fn empty() {
        checks![
            ("", &[]),
            (" ", &[]),
            (" \n", &[]),
            ("\n", &[]),
            ("\n\t", &[]),
            ("\n \t", &[]),
            ("\n \t ", &[]),
            (" \n \t \t", &[]),
        ];
    }

    #[test]
    fn literals() {
        use TokenLitKind::*;
        checks![
            ("\"\"", &[(0..2, lit(Str, ""))]),
            ("\"\"\"\"", &[(0..2, lit(Str, "")), (2..4, lit(Str, ""))]),
            ("\"\" \"\"", &[(0..2, lit(Str, "")), (3..5, lit(Str, ""))]),
            ("\"\\\"\"", &[(0..4, lit(Str, "\\\""))]),
            ("unicode\"\"", &[(0..9, lit(UnicodeStr, ""))]),
            ("unicode \"\"", &[(0..7, id("unicode")), (8..10, lit(Str, ""))]),
            ("hex\"\"", &[(0..5, lit(HexStr, ""))]),
            ("hex \"\"", &[(0..3, id("hex")), (4..6, lit(Str, ""))]),
            //
            ("0", &[(0..1, lit(Integer, "0"))]),
            ("0a", &[(0..1, lit(Integer, "0")), (1..2, id("a"))]),
            ("0.e1", &[(0..1, lit(Integer, "0")), (1..2, Dot), (2..4, id("e1"))]),
            (
                "0.e-1",
                &[
                    (0..1, lit(Integer, "0")),
                    (1..2, Dot),
                    (2..3, id("e")),
                    (3..4, BinOp(Minus)),
                    (4..5, lit(Integer, "1")),
                ],
            ),
            ("0.0", &[(0..3, lit(Rational, "0.0"))]),
            ("0.", &[(0..2, lit(Rational, "0."))]),
            (".0", &[(0..2, lit(Rational, ".0"))]),
            ("0.0e1", &[(0..5, lit(Rational, "0.0e1"))]),
            ("0.0e-1", &[(0..6, lit(Rational, "0.0e-1"))]),
            ("0e1", &[(0..3, lit(Rational, "0e1"))]),
            ("0e1.", &[(0..3, lit(Rational, "0e1")), (3..4, Dot)]),
        ];

        checks_full![
            ("0b0", true, &[(0..3, lit(Integer, "0b0"))]),
            ("0B0", false, &[(0..1, lit(Integer, "0")), (1..3, id("B0"))]),
            ("0o0", true, &[(0..3, lit(Integer, "0o0"))]),
            ("0O0", false, &[(0..1, lit(Integer, "0")), (1..3, id("O0"))]),
            ("0xa", false, &[(0..3, lit(Integer, "0xa"))]),
            ("0Xa", false, &[(0..1, lit(Integer, "0")), (1..3, id("Xa"))]),
        ];
    }

    #[test]
    fn idents() {
        checks![
            ("$", &[(0..1, id("$"))]),
            ("a$", &[(0..2, id("a$"))]),
            ("a_$123_", &[(0..7, id("a_$123_"))]),
            ("   b", &[(3..4, id("b"))]),
            (" c\t ", &[(1..2, id("c"))]),
            (" \td ", &[(2..3, id("d"))]),
            (" \t\nef ", &[(3..5, id("ef"))]),
            (" \t\n\tghi ", &[(4..7, id("ghi"))]),
        ];
    }

    #[test]
    fn doc_comments() {
        use CommentKind::*;

        fn doc(kind: CommentKind, symbol: &str) -> TokenKind {
            Comment(true, kind, sym(symbol))
        }

        checks![
            ("// line comment", &[]),
            ("// / line comment", &[]),
            ("// ! line comment", &[]),
            ("// /* line comment", &[]), // */ <-- aaron-bond.better-comments doesn't like this
            ("/// line doc-comment", &[(0..20, doc(Line, " line doc-comment"))]),
            ("//// invalid doc-comment", &[]),
            ("///// invalid doc-comment", &[]),
            //
            ("/**/", &[]),
            ("/***/", &[]),
            ("/****/", &[]),
            ("/*/*/", &[]),
            ("/* /*/", &[]),
            ("/*/**/", &[]),
            ("/* /**/", &[]),
            ("/* normal block comment */", &[]),
            ("/* /* normal block comment */", &[]),
            ("/** block doc-comment */", &[(0..24, doc(Block, " block doc-comment "))]),
            ("/** /* block doc-comment */", &[(0..27, doc(Block, " /* block doc-comment "))]),
            ("/** block doc-comment /*/", &[(0..25, doc(Block, " block doc-comment /"))]),
        ];
    }

    #[test]
    fn operators() {
        use solar_ast::token::Delimiter::*;
        // From Solc `TOKEN_LIST`: https://github.com/argotorg/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/liblangutil/Token.h#L67
        checks![
            (")", &[(0..1, CloseDelim(Parenthesis))]),
            ("(", &[(0..1, OpenDelim(Parenthesis))]),
            ("[", &[(0..1, OpenDelim(Bracket))]),
            ("]", &[(0..1, CloseDelim(Bracket))]),
            ("{", &[(0..1, OpenDelim(Brace))]),
            ("}", &[(0..1, CloseDelim(Brace))]),
            (":", &[(0..1, Colon)]),
            (";", &[(0..1, Semi)]),
            (".", &[(0..1, Dot)]),
            ("?", &[(0..1, Question)]),
            ("=>", &[(0..2, FatArrow)]),
            ("->", &[(0..2, Arrow)]),
            ("=", &[(0..1, Eq)]),
            ("|=", &[(0..2, BinOpEq(Or))]),
            ("^=", &[(0..2, BinOpEq(Caret))]),
            ("&=", &[(0..2, BinOpEq(And))]),
            ("<<=", &[(0..3, BinOpEq(Shl))]),
            (">>=", &[(0..3, BinOpEq(Shr))]),
            (">>>=", &[(0..4, BinOpEq(Sar))]),
            ("+=", &[(0..2, BinOpEq(Plus))]),
            ("-=", &[(0..2, BinOpEq(Minus))]),
            ("*=", &[(0..2, BinOpEq(Star))]),
            ("/=", &[(0..2, BinOpEq(Slash))]),
            ("%=", &[(0..2, BinOpEq(Percent))]),
            (",", &[(0..1, Comma)]),
            ("||", &[(0..2, OrOr)]),
            ("&&", &[(0..2, AndAnd)]),
            ("|", &[(0..1, BinOp(Or))]),
            ("^", &[(0..1, BinOp(Caret))]),
            ("&", &[(0..1, BinOp(And))]),
            ("<<", &[(0..2, BinOp(Shl))]),
            (">>", &[(0..2, BinOp(Shr))]),
            (">>>", &[(0..3, BinOp(Sar))]),
            ("+", &[(0..1, BinOp(Plus))]),
            ("-", &[(0..1, BinOp(Minus))]),
            ("*", &[(0..1, BinOp(Star))]),
            ("/", &[(0..1, BinOp(Slash))]),
            ("%", &[(0..1, BinOp(Percent))]),
            ("**", &[(0..2, StarStar)]),
            ("==", &[(0..2, EqEq)]),
            ("!=", &[(0..2, Ne)]),
            ("<", &[(0..1, Lt)]),
            (">", &[(0..1, Gt)]),
            ("<=", &[(0..2, Le)]),
            (">=", &[(0..2, Ge)]),
            ("!", &[(0..1, Not)]),
            ("~", &[(0..1, Tilde)]),
            ("++", &[(0..2, PlusPlus)]),
            ("--", &[(0..2, MinusMinus)]),
            (":=", &[(0..2, Walrus)]),
        ];
    }

    #[test]
    fn glueing() {
        checks![
            ("=", &[(0..1, Eq)]),
            ("==", &[(0..2, EqEq)]),
            ("= =", &[(0..1, Eq), (2..3, Eq)]),
            ("===", &[(0..2, EqEq), (2..3, Eq)]),
            ("== =", &[(0..2, EqEq), (3..4, Eq)]),
            ("= ==", &[(0..1, Eq), (2..4, EqEq)]),
            ("====", &[(0..2, EqEq), (2..4, EqEq)]),
            ("== ==", &[(0..2, EqEq), (3..5, EqEq)]),
            ("= ===", &[(0..1, Eq), (2..4, EqEq), (4..5, Eq)]),
            ("=====", &[(0..2, EqEq), (2..4, EqEq), (4..5, Eq)]),
            //
            (" <", &[(1..2, Lt)]),
            (" <=", &[(1..3, Le)]),
            (" < =", &[(1..2, Lt), (3..4, Eq)]),
            (" <<", &[(1..3, BinOp(Shl))]),
            (" <<=", &[(1..4, BinOpEq(Shl))]),
            //
            (" >", &[(1..2, Gt)]),
            (" >=", &[(1..3, Ge)]),
            (" > =", &[(1..2, Gt), (3..4, Eq)]),
            (" >>", &[(1..3, BinOp(Shr))]),
            (" >>>", &[(1..4, BinOp(Sar))]),
            (" >>>=", &[(1..5, BinOpEq(Sar))]),
            //
            ("+", &[(0..1, BinOp(Plus))]),
            ("++", &[(0..2, PlusPlus)]),
            ("+++", &[(0..2, PlusPlus), (2..3, BinOp(Plus))]),
            ("+ =", &[(0..1, BinOp(Plus)), (2..3, Eq)]),
            ("+ +=", &[(0..1, BinOp(Plus)), (2..4, BinOpEq(Plus))]),
            ("+++=", &[(0..2, PlusPlus), (2..4, BinOpEq(Plus))]),
            ("+ +", &[(0..1, BinOp(Plus)), (2..3, BinOp(Plus))]),
            //
            ("-", &[(0..1, BinOp(Minus))]),
            ("--", &[(0..2, MinusMinus)]),
            ("---", &[(0..2, MinusMinus), (2..3, BinOp(Minus))]),
            ("- =", &[(0..1, BinOp(Minus)), (2..3, Eq)]),
            ("- -=", &[(0..1, BinOp(Minus)), (2..4, BinOpEq(Minus))]),
            ("---=", &[(0..2, MinusMinus), (2..4, BinOpEq(Minus))]),
            ("- -", &[(0..1, BinOp(Minus)), (2..3, BinOp(Minus))]),
        ];
    }
}

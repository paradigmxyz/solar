use crate::ParseSess;
use sulk_ast::token::{BinOpToken, CommentKind, Delimiter, Lit, LitKind, Token, TokenKind};
use sulk_interface::{diagnostics::DiagCtxt, sym, BytePos, Pos, Span, Symbol};

pub mod cursor;
use cursor::Base;
pub use cursor::{is_id_continue, is_id_start, is_ident, is_whitespace, Cursor};

pub mod unescape;

mod unicode_chars;
use unicode_chars::UNICODE_ARRAY;

mod utf8;

/// Solidity lexer.
pub struct Lexer<'a> {
    /// The current parser session.
    sess: &'a ParseSess,

    /// Initial position, read-only.
    start_pos: BytePos,

    /// The absolute offset within the source_map of the current character.
    pos: BytePos,

    /// Source text to tokenize.
    src: &'a str,

    /// Cursor for getting lexer tokens.
    cursor: Cursor<'a>,

    override_span: Option<Span>,

    /// When a "unknown start of token: \u{a0}" has already been emitted earlier
    /// in this file, it's safe to treat further occurrences of the non-breaking
    /// space character as whitespace.
    nbsp_is_whitespace: bool,
}

impl<'a> Lexer<'a> {
    /// Creates a new `Lexer` for the given source string.
    pub fn new(
        sess: &'a ParseSess,
        src: &'a str,
        start_pos: BytePos,
        override_span: Option<Span>,
    ) -> Self {
        Self {
            sess,
            start_pos,
            pos: start_pos,
            src,
            cursor: Cursor::new(src),
            override_span,
            nbsp_is_whitespace: false,
        }
    }

    /// Returns a reference to the diagnostic context.
    #[inline]
    pub fn dcx(&self) -> &'a DiagCtxt {
        &self.sess.dcx
    }

    /// Returns the next token, paired with a bool indicating if the token was
    /// preceded by whitespace.
    pub fn next_token(&mut self) -> (Token, bool) {
        let mut preceded_by_whitespace = false;
        let mut swallow_next_invalid = 0;
        loop {
            let token = self.cursor.advance_token();
            let start = self.pos;
            self.pos += token.len;

            // debug!("next_token: {:?}({:?})", token.kind, self.str_from(start));

            // Now "cook" the token, converting the simple `cursor::TokenKind` enum into a
            // rich `ast::TokenKind`. This turns strings into interned symbols and runs
            // additional validation.
            let kind = match token.kind {
                cursor::TokenKind::LineComment { is_doc } => {
                    // Skip non-doc comments.
                    if !is_doc {
                        preceded_by_whitespace = true;
                        continue;
                    }

                    // Opening delimiter of the length 3 is not included into the symbol.
                    let content_start = start + BytePos(3);
                    let content = self.str_from(content_start);
                    self.cook_doc_comment(content_start, content, CommentKind::Line)
                }
                cursor::TokenKind::BlockComment { is_doc, terminated } => {
                    if !terminated {
                        self.report_unterminated_block_comment(start, is_doc);
                    }

                    // Skip non-doc comments.
                    if is_doc {
                        preceded_by_whitespace = true;
                        continue;
                    }

                    // Opening delimiter of the length 3 and closing delimiter of the length 2
                    // are not included into the symbol.
                    let content_start = start + BytePos(3);
                    let content_end = self.pos - (terminated as u32) * 2;
                    let content = self.str_from_to(content_start, content_end);
                    self.cook_doc_comment(content_start, content, CommentKind::Block)
                }
                cursor::TokenKind::Whitespace => {
                    preceded_by_whitespace = true;
                    continue;
                }
                cursor::TokenKind::Ident => {
                    let sym = self.symbol_from(start);
                    TokenKind::Ident(sym)
                }
                cursor::TokenKind::UnknownPrefix => {
                    self.report_unknown_prefix(start);
                    let sym = self.symbol_from(start);
                    TokenKind::Ident(sym)
                }
                // Do not recover an identifier with emoji if the codepoint is a confusable
                // with a recoverable substitution token, like `➖`.
                cursor::TokenKind::InvalidIdent
                    if !UNICODE_ARRAY.iter().any(|&(c, _, _)| {
                        let sym = self.str_from(start);
                        sym.chars().count() == 1 && c == sym.chars().next().unwrap()
                    }) =>
                {
                    let sym = self.symbol_from(start);
                    TokenKind::Ident(sym)
                }
                cursor::TokenKind::Literal { kind } => {
                    let (kind, symbol) = self.cook_lexer_literal(start, self.pos, kind);
                    TokenKind::Literal(Lit { kind, symbol })
                }

                cursor::TokenKind::Semi => TokenKind::Semi,
                cursor::TokenKind::Comma => TokenKind::Comma,
                cursor::TokenKind::Dot => TokenKind::Dot,
                cursor::TokenKind::OpenParen => TokenKind::OpenDelim(Delimiter::Parenthesis),
                cursor::TokenKind::CloseParen => TokenKind::CloseDelim(Delimiter::Parenthesis),
                cursor::TokenKind::OpenBrace => TokenKind::OpenDelim(Delimiter::Brace),
                cursor::TokenKind::CloseBrace => TokenKind::CloseDelim(Delimiter::Brace),
                cursor::TokenKind::OpenBracket => TokenKind::OpenDelim(Delimiter::Bracket),
                cursor::TokenKind::CloseBracket => TokenKind::CloseDelim(Delimiter::Bracket),
                cursor::TokenKind::Tilde => TokenKind::Tilde,
                cursor::TokenKind::Question => TokenKind::Question,
                cursor::TokenKind::Colon => TokenKind::Colon,
                cursor::TokenKind::Eq => TokenKind::Eq,
                cursor::TokenKind::Bang => TokenKind::Not,
                cursor::TokenKind::Lt => TokenKind::Lt,
                cursor::TokenKind::Gt => TokenKind::Gt,
                cursor::TokenKind::Minus => TokenKind::BinOp(BinOpToken::Minus),
                cursor::TokenKind::And => TokenKind::BinOp(BinOpToken::And),
                cursor::TokenKind::Or => TokenKind::BinOp(BinOpToken::Or),
                cursor::TokenKind::Plus => TokenKind::BinOp(BinOpToken::Plus),
                cursor::TokenKind::Star => TokenKind::BinOp(BinOpToken::Star),
                cursor::TokenKind::Slash => TokenKind::BinOp(BinOpToken::Slash),
                cursor::TokenKind::Caret => TokenKind::BinOp(BinOpToken::Caret),
                cursor::TokenKind::Percent => TokenKind::BinOp(BinOpToken::Percent),

                cursor::TokenKind::Unknown | cursor::TokenKind::InvalidIdent => {
                    // Don't emit diagnostics for sequences of the same invalid token
                    if swallow_next_invalid > 0 {
                        swallow_next_invalid -= 1;
                        continue;
                    }
                    let mut it = self.str_from_to_end(start).chars();
                    let c = it.next().unwrap();
                    if c == '\u{00a0}' {
                        // If an error has already been reported on non-breaking
                        // space characters earlier in the file, treat all
                        // subsequent occurrences as whitespace.
                        if self.nbsp_is_whitespace {
                            preceded_by_whitespace = true;
                            continue;
                        }
                        self.nbsp_is_whitespace = true;
                    }

                    let repeats = it.take_while(|c1| *c1 == c).count();
                    swallow_next_invalid = repeats;

                    let span = self
                        .new_span(start, self.pos + BytePos::from_usize(repeats * c.len_utf8()));
                    let escaped = escaped_char(c);
                    let message = format!("unknown start of token: {escaped}");
                    let mut diag = self.dcx().err(message).span(span);
                    if c == '\0' {
                        let help = "source files must contain UTF-8 encoded text, unexpected null bytes might occur when a different encoding is used";
                        diag = diag.help(help);
                    }
                    if repeats > 0 {
                        let note = match repeats {
                            1 => "once more".to_string(),
                            _ => format!("{repeats} more times"),
                        };
                        diag = diag.note(format!("character repeats {note}"));
                    }
                    diag.emit();

                    // preceded_by_whitespace = true;
                    continue;
                    // TODO
                    /*
                    let (token, _sugg) =
                        unicode_chars::check_for_substitution(self, start, c, repeats + 1);

                    self.sess.emit_err(errors::UnknownTokenStart {
                        span,
                        escaped: escaped_char(c),
                        sugg,
                        null: if c == '\x00' { Some(errors::UnknownTokenNull) } else { None },
                        repeat: if repeats > 0 {
                            Some(errors::UnknownTokenRepeat { repeats })
                        } else {
                            None
                        },
                    });
                    if let Some(token) = token {
                        token
                    } else {
                        preceded_by_whitespace = true;
                        continue;
                    }
                    */
                }

                cursor::TokenKind::Eof => TokenKind::Eof,
            };
            let span = self.new_span(start, self.pos);
            return (Token::new(kind, span), preceded_by_whitespace);
        }
    }

    fn cook_doc_comment(
        &self,
        content_start: BytePos,
        content: &str,
        comment_kind: CommentKind,
    ) -> TokenKind {
        if content.contains('\r') {
            for (idx, _) in content.char_indices().filter(|&(_, c)| c == '\r') {
                let span = self.new_span(
                    content_start + BytePos(idx as u32),
                    content_start + BytePos(idx as u32 + 1),
                );
                let block = if matches!(comment_kind, CommentKind::Block) { "block " } else { "" };
                let msg = format!("bare CR not allowed in {block}doc-comment");
                self.dcx().err(msg).span(span).emit();
            }
        }

        TokenKind::DocComment(comment_kind, Symbol::intern(content))
    }

    fn cook_lexer_literal(
        &self,
        start: BytePos,
        end: BytePos,
        kind: cursor::LiteralKind,
    ) -> (LitKind, Symbol) {
        match kind {
            cursor::LiteralKind::Str { terminated, unicode } => {
                if !terminated {
                    let span = self.new_span(start, end);
                    self.dcx().fatal("unterminated string").span(span).emit();
                }
                let kind = if unicode { LitKind::UnicodeStr } else { LitKind::Str };
                let prefix_len = if unicode { 7 } else { 0 }; // `unicode`
                self.cook_quoted(kind, start, end, prefix_len)
            }
            cursor::LiteralKind::HexStr { terminated } => {
                if !terminated {
                    let span = self.new_span(start, end);
                    self.dcx().fatal("unterminated hex string").span(span).emit();
                }
                let prefix_len = 3; // `hex`
                self.cook_quoted(LitKind::HexStr, start, end, prefix_len)
            }
            cursor::LiteralKind::Int { base, empty_int } => {
                if empty_int {
                    let span = self.new_span(start, end);
                    self.dcx().err("no valid digits found for number").span(span).emit();
                    (LitKind::Integer, sym::integer(0))
                } else {
                    if matches!(base, Base::Binary | Base::Octal) {
                        let start = start + 2;
                        // TODO: enable if binary and octal literals are ever supported.
                        /*
                        let base = base as u32;
                        let s = self.str_from_to(start, end);
                        for (i, c) in s.char_indices() {
                            if c != '_' && c.to_digit(base).is_none() {
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
                    (LitKind::Integer, self.symbol_from_to(start, end))
                }
            }
            cursor::LiteralKind::Rational { base, empty_exponent } => {
                if empty_exponent {
                    let span = self.new_span(start, self.pos);
                    self.dcx().err("expected at least one digit in exponent").span(span).emit();
                }

                let unsupported_base =
                    matches!(base, Base::Binary | Base::Octal | Base::Hexadecimal);
                if unsupported_base {
                    let msg = format!("{base} rational numbers are not supported");
                    self.dcx().err(msg).span(self.new_span(start, end)).emit();
                }

                (LitKind::Rational, self.symbol_from_to(start, end))
            }
        }
    }

    fn cook_quoted(
        &self,
        kind: LitKind,
        start: BytePos,
        end: BytePos,
        prefix_len: u32,
    ) -> (LitKind, Symbol) {
        let mode = match kind {
            LitKind::Str => unescape::Mode::Str,
            LitKind::UnicodeStr => unescape::Mode::UnicodeStr,
            LitKind::HexStr => unescape::Mode::HexStr,
            _ => unreachable!(),
        };

        // Account for quote (`"` or `'`) and prefix.
        let content_start = start + 1 + BytePos(prefix_len);
        let content_end = end - 1;
        let lit_content = self.str_from_to(content_start, content_end);

        let mut has_fatal_err = false;
        unescape::unescape_literal(lit_content, mode, |range, result| {
            // Here we only check for errors. The actual unescaping is done later.
            if let Err(err) = result {
                has_fatal_err = true;
                let (start, end) = (range.start as u32, range.end as u32);
                let lo = content_start + BytePos(start);
                let hi = lo + BytePos(end - start);
                let span = self.new_span(lo, hi);
                unescape::emit_unescape_error(self.dcx(), lit_content, span, range, err);
            }
        });

        // We normally exclude the quotes for the symbol, but for errors we
        // include it because it results in clearer error messages.
        if has_fatal_err {
            (LitKind::Err, self.symbol_from_to(start, end))
        } else {
            (kind, Symbol::intern(lit_content))
        }
    }

    fn new_span(&self, lo: BytePos, hi: BytePos) -> Span {
        self.override_span.unwrap_or_else(|| Span::new(lo, hi))
    }

    #[inline]
    fn src_index(&self, pos: BytePos) -> usize {
        (pos - self.start_pos).to_usize()
    }

    /// Slice of the source text from `start` up to but excluding `self.pos`,
    /// meaning the slice does not include the character `self.ch`.
    fn symbol_from(&self, start: BytePos) -> Symbol {
        self.symbol_from_to(start, self.pos)
    }

    /// Slice of the source text from `start` up to but excluding `self.pos`,
    /// meaning the slice does not include the character `self.ch`.
    fn str_from(&self, start: BytePos) -> &'a str {
        self.str_from_to(start, self.pos)
    }

    /// Same as `symbol_from`, with an explicit endpoint.
    fn symbol_from_to(&self, start: BytePos, end: BytePos) -> Symbol {
        // debug!("taking an ident from {:?} to {:?}", start, end);
        Symbol::intern(self.str_from_to(start, end))
    }

    /// Slice of the source text spanning from `start` up to but excluding `end`.
    fn str_from_to(&self, start: BytePos, end: BytePos) -> &'a str {
        &self.src[self.src_index(start)..self.src_index(end)]
    }

    /// Slice of the source text spanning from `start` until the end.
    fn str_from_to_end(&self, start: BytePos) -> &'a str {
        &self.src[self.src_index(start)..]
    }

    fn report_unterminated_block_comment(&self, start: BytePos, is_doc: bool) {
        let msg =
            if is_doc { "unterminated block doc-comment" } else { "unterminated block comment" };
        self.dcx().fatal(msg).span(self.new_span(start, self.pos)).emit();
    }

    fn report_unknown_prefix(&self, start: BytePos) {
        let prefix = self.str_from_to(start, self.pos);
        let msg = format!("prefix {prefix} is unknown");
        self.dcx().err(msg).span(self.new_span(start, self.pos)).emit();
    }
}

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

use rsolc_ast::token::{BinOpToken, CommentKind, Delimiter, Lit, LitKind, Token, TokenKind};
use rsolc_interface::{sym, BytePos, Pos, Span, Symbol};

pub mod cursor;
pub use cursor::{is_id_continue, is_id_start, is_ident, is_whitespace, Cursor};

pub mod unescape;

mod unicode_chars;
use unicode_chars::UNICODE_ARRAY;

mod utf8;

pub struct StringReader<'a> {
    // sess: &'a ParseSess,
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

impl<'a> StringReader<'a> {
    pub fn new(src: &'a str, start_pos: BytePos, override_span: Option<Span>) -> Self {
        Self {
            start_pos,
            pos: start_pos,
            src,
            cursor: Cursor::new(src),
            override_span,
            nbsp_is_whitespace: false,
        }
    }

    /// Returns the next token, paired with a bool indicating if the token was
    /// preceded by whitespace.
    pub fn next_token(&mut self) -> (Token, bool) {
        let mut preceded_by_whitespace = false;
        let mut swallow_next_invalid = 0;
        // Skip trivial (whitespace & comments) tokens
        loop {
            let token = self.cursor.advance_token();
            let start = self.pos;
            self.pos += token.len;

            // debug!("next_token: {:?}({:?})", token.kind, self.str_from(start));

            // Now "cook" the token, converting the simple `cursor::TokenKind` enum into a
            // rich `rustc_ast::TokenKind`. This turns strings into interned symbols and runs
            // additional validation.
            let kind = match token.kind {
                cursor::TokenKind::LineComment { is_doc } => {
                    // Skip non-doc comments
                    if !is_doc {
                        self.lint_unicode_text_flow(start);
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

                    // Skip non-doc comments
                    if is_doc {
                        self.lint_unicode_text_flow(start);
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
                    if repeats > 0 {
                        swallow_next_invalid = repeats;
                    }

                    let (token, _sugg) =
                        unicode_chars::check_for_substitution(self, start, c, repeats + 1);

                    // TODO
                    /*
                    self.sess.emit_err(errors::UnknownTokenStart {
                        span: self.mk_sp(start, self.pos + Pos::from_usize(repeats * c.len_utf8())),
                        escaped: escaped_char(c),
                        sugg,
                        null: if c == '\x00' { Some(errors::UnknownTokenNull) } else { None },
                        repeat: if repeats > 0 {
                            Some(errors::UnknownTokenRepeat { repeats })
                        } else {
                            None
                        },
                    });
                    */

                    if let Some(token) = token {
                        token
                    } else {
                        preceded_by_whitespace = true;
                        continue;
                    }
                }

                cursor::TokenKind::Eof => TokenKind::Eof,
            };
            let span = self.mk_sp(start, self.pos);
            return (Token::new(kind, span), preceded_by_whitespace);
        }
    }

    fn cook_doc_comment(
        &self,
        _content_start: BytePos,
        content: &str,
        comment_kind: CommentKind,
    ) -> TokenKind {
        // TODO
        /*
        if content.contains('\r') {
            for (idx, _) in content.char_indices().filter(|&(_, c)| c == '\r') {
                let span = self.mk_sp(
                    content_start + BytePos(idx as u32),
                    content_start + BytePos(idx as u32 + 1),
                );
                let block = matches!(comment_kind, CommentKind::Block);
                self.sess.emit_err(errors::CrDocComment { span, block });
            }
        }
        */

        TokenKind::DocComment(comment_kind, Symbol::intern(content))
    }

    fn cook_lexer_literal(
        &self,
        start: BytePos,
        end: BytePos,
        kind: cursor::LiteralKind,
    ) -> (LitKind, Symbol) {
        match kind {
            cursor::LiteralKind::Str { terminated: _, unicode } => {
                // TODO
                // if !terminated {
                //     self.sess.span_diagnostic.span_fatal_with_code(
                //         self.mk_sp(start, end),
                //         "unterminated double quote string",
                //         error_code!(E0765),
                //     )
                // }
                let prefix_len = if unicode { 8 } else { 1 };
                let kind = if unicode { LitKind::UnicodeStr } else { LitKind::Str };
                self.cook_quoted(kind, start, end, prefix_len)
            }
            cursor::LiteralKind::HexStr { terminated: _ } => {
                // TODO
                // if !terminated {
                //     self.sess.span_diagnostic.span_fatal_with_code(
                //         self.mk_sp(start + BytePos(1), end),
                //         "unterminated double quote hex string",
                //         error_code!(E0766),
                //     )
                // }
                self.cook_quoted(LitKind::HexStr, start, end, 4)
            }
            cursor::LiteralKind::Int { base: _, empty_int } => {
                if empty_int {
                    // TODO
                    // let span = self.mk_sp(start, end);
                    // self.sess.emit_err(errors::NoDigitsLiteral { span });
                    (LitKind::Integer, sym::integer(0))
                } else {
                    (LitKind::Integer, self.symbol_from_to(start, end))
                }
            }
            cursor::LiteralKind::Rational { base: _, empty_exponent: _ } => {
                // TODO
                // if empty_exponent {
                //     let span = self.mk_sp(start, self.pos);
                //     self.sess.emit_err(errors::EmptyExponentFloat { span });
                // }
                // let base = match base {
                //     Base::Hexadecimal => Some("hexadecimal"),
                //     _ => None,
                // };
                // if let Some(base) = base {
                //     let span = self.mk_sp(start, end);
                //     self.sess.emit_err(errors::FloatLiteralUnsupportedBase { span, base });
                // }
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
        let content_start = start + BytePos(prefix_len);
        let content_end = end - BytePos(1); // `"` or `'`
        let mut lit_content = self.str_from_to(content_start - 1, content_end).chars();
        let _quote = lit_content.next().unwrap();
        let lit_content = lit_content.as_str();

        let mut has_fatal_err = false;
        unescape::unescape_literal(lit_content, mode, &mut |_range, result| {
            // Here we only check for errors. The actual unescaping is done later.
            // TODO
            if let Err(_err) = result {
                // let span_with_quotes = self.mk_sp(start, end);
                // let (start, end) = (range.start as u32, range.end as u32);
                // let lo = content_start + BytePos(start);
                // let hi = lo + BytePos(end - start);
                // let span = self.mk_sp(lo, hi);
                has_fatal_err = true;
                // emit_unescape_error(
                //     &self.sess.span_diagnostic,
                //     lit_content,
                //     span_with_quotes,
                //     span,
                //     mode,
                //     range,
                //     err,
                // );
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

    fn mk_sp(&self, lo: BytePos, hi: BytePos) -> Span {
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

    #[allow(unused)]
    fn struct_fatal_span_char(
        &self,
        from_pos: BytePos,
        to_pos: BytePos,
        m: &str,
        c: char,
        // ) -> DiagnosticBuilder<'a, !> {
    ) {
        // self.sess
        //     .span_diagnostic
        //     .struct_span_fatal(self.mk_sp(from_pos, to_pos), &format!("{}: {}", m,
        // escaped_char(c)))
    }

    /// Detect usages of Unicode codepoints changing the direction of the text on screen and loudly
    /// complain about it.
    #[allow(unused)]
    fn lint_unicode_text_flow(&self, start: BytePos) {
        // // Opening delimiter of the length 2 is not included into the comment text.
        // let content_start = start + BytePos(2);
        // let content = self.str_from(content_start);
        // if contains_text_flow_control_chars(content) {
        //     let span = self.mk_sp(start, self.pos);
        //     self.sess.buffer_lint_with_diagnostic(
        //         &TEXT_DIRECTION_CODEPOINT_IN_COMMENT,
        //         span,
        //         ast::CRATE_NODE_ID,
        //         "unicode codepoint changing visible direction of text present in comment",
        //         BuiltinLintDiagnostics::UnicodeTextFlow(span, content.to_string()),
        //     );
        // }
    }

    #[allow(unused)]
    fn report_unterminated_raw_string(
        &self,
        start: BytePos,
        n_hashes: u32,
        possible_offset: Option<u32>,
        found_terminators: u32,
    ) -> ! {
        // TODO
        todo!()
        /*
        let mut err = self.sess.span_diagnostic.struct_span_fatal_with_code(
            self.mk_sp(start, start),
            "unterminated raw string",
            error_code!(E0748),
        );

        err.span_label(self.mk_sp(start, start), "unterminated raw string");

        if n_hashes > 0 {
            err.note(&format!(
                "this raw string should be terminated with `\"{}`",
                "#".repeat(n_hashes as usize)
            ));
        }

        if let Some(possible_offset) = possible_offset {
            let lo = start + BytePos(possible_offset);
            let hi = lo + BytePos(found_terminators);
            let span = self.mk_sp(lo, hi);
            err.span_suggestion(
                span,
                "consider terminating the string here",
                "#".repeat(n_hashes as usize),
                Applicability::MaybeIncorrect,
            );
        }

        err.emit()
        */
    }

    #[allow(unused)]
    fn report_unterminated_block_comment(&self, start: BytePos, is_doc: bool) {
        // let msg =
        //     if is_doc { "unterminated block doc-comment" } else { "unterminated block comment" };
        // let last_bpos = self.pos;
        // let mut err = self.sess.span_diagnostic.struct_span_fatal_with_code(
        //     self.mk_sp(start, last_bpos),
        //     msg,
        //     error_code!(E0758),
        // );
        // let mut nested_block_comment_open_idxs = vec![];
        // let mut last_nested_block_comment_idxs = None;
        // let mut content_chars = self.str_from(start).char_indices().peekable();

        // while let Some((idx, current_char)) = content_chars.next() {
        //     match content_chars.peek() {
        //         Some((_, '*')) if current_char == '/' => {
        //             nested_block_comment_open_idxs.push(idx);
        //         }
        //         Some((_, '/')) if current_char == '*' => {
        //             last_nested_block_comment_idxs =
        //                 nested_block_comment_open_idxs.pop().map(|open_idx| (open_idx, idx));
        //         }
        //         _ => {}
        //     };
        // }

        // if let Some((nested_open_idx, nested_close_idx)) = last_nested_block_comment_idxs {
        //     err.span_label(self.mk_sp(start, start + BytePos(2)), msg)
        //         .span_label(
        //             self.mk_sp(
        //                 start + BytePos(nested_open_idx as u32),
        //                 start + BytePos(nested_open_idx as u32 + 2),
        //             ),
        //             "...as last nested comment starts here, maybe you want to close this
        // instead?",         )
        //         .span_label(
        //             self.mk_sp(
        //                 start + BytePos(nested_close_idx as u32),
        //                 start + BytePos(nested_close_idx as u32 + 2),
        //             ),
        //             "...and last nested comment terminates here.",
        //         );
        // }

        // err.emit();
    }

    #[allow(unused)]
    fn report_unknown_prefix(&self, start: BytePos) {
        // let prefix_span = self.mk_sp(start, self.pos);
        // let prefix = self.str_from_to(start, self.pos);

        // let expn_data = prefix_span.ctxt().outer_expn_data();

        // if expn_data.edition >= Edition::Edition2021 {
        //     // In Rust 2021, this is a hard error.
        //     let sugg = if prefix == "rb" {
        //         Some(errors::UnknownPrefixSugg::UseBr(prefix_span))
        //     } else if expn_data.is_root() {
        //         Some(errors::UnknownPrefixSugg::Whitespace(prefix_span.shrink_to_hi()))
        //     } else {
        //         None
        //     };
        //     self.sess.emit_err(errors::UnknownPrefix { span: prefix_span, prefix, sugg });
        // } else {
        //     // Before Rust 2021, only emit a lint for migration.
        //     self.sess.buffer_lint_with_diagnostic(
        //         &RUST_2021_PREFIXES_INCOMPATIBLE_SYNTAX,
        //         prefix_span,
        //         ast::CRATE_NODE_ID,
        //         &format!("prefix `{prefix}` is unknown"),
        //         BuiltinLintDiagnostics::ReservedPrefix(prefix_span),
        //     );
        // }
    }
}

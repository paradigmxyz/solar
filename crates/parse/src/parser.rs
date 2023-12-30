use crate::{PErr, PResult, ParseSess};
use std::fmt::{self, Write};
use sulk_ast::token::{Token, TokenKind};
use sulk_interface::{
    diagnostics::{DiagCtxt, FatalError},
    Ident, Span, Symbol,
};

/// Solidity parser.
pub struct Parser<'a> {
    /// The parser session.
    sess: &'a ParseSess,

    /// The current token.
    token: Token,
    /// The previous token.
    prev_token: Token,
    /// List of expected tokens. Cleared after each `bump` call.
    expected_tokens: Vec<ExpectedToken>,
    /// The span of the last unexpected token.
    last_unexpected_token_span: Option<Span>,

    /// Whether the parser is in Yul mode.
    ///
    /// Currently, this can only happen when parsing a Yul "assembly" block.
    in_yul: bool,

    /// The token stream.
    stream: std::vec::IntoIter<Token>,
}

#[allow(dead_code)] // TODO
#[derive(Clone, Debug, PartialEq, Eq)]
enum ExpectedToken {
    Token(TokenKind),
    Keyword(Symbol),
    Operator,
    Ident,
    Path,
    Type,
    Const,
}

impl fmt::Display for ExpectedToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Token(t) => return write!(f, "`{t}`"),
            Self::Keyword(kw) => return write!(f, "`{kw}`"),
            Self::Operator => "an operator",
            Self::Ident => "an identifier",
            Self::Path => "a path",
            Self::Type => "a type",
            Self::Const => "a const expression",
        })
    }
}

impl ExpectedToken {
    fn to_string_many(tokens: &[Self]) -> String {
        let len = tokens.len();
        let mut s = String::with_capacity(16 * len);
        for (i, token) in tokens.iter().enumerate() {
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
            let _ = write!(s, "{token}");
        }
        s
    }

    fn eq_kind(&self, other: &TokenKind) -> bool {
        match self {
            Self::Token(kind) => kind == other,
            _ => false,
        }
    }
}

impl<'a> Parser<'a> {
    /// Creates a new parser.
    pub fn new(sess: &'a ParseSess, stream: Vec<Token>) -> Self {
        let mut parser = Self {
            sess,
            token: Token::DUMMY,
            prev_token: Token::DUMMY,
            expected_tokens: Vec::new(),
            last_unexpected_token_span: None,
            in_yul: false,
            stream: stream.into_iter(),
        };
        parser.bump();
        parser
    }

    /// Returns the diagnostic context.
    #[inline]
    pub fn dcx(&self) -> &'a DiagCtxt {
        &self.sess.dcx
    }

    /// Returns an "unexpected token" error for the current token.
    pub fn unexpected<T>(&mut self) -> PResult<'a, T> {
        self.expect_one_of(&[], &[]).map(|_| unreachable!())
    }

    /// Expects and consumes the token `t`. Signals an error if the next token is not `t`.
    pub fn expect(&mut self, t: &TokenKind) -> PResult<'a, bool /* recovered */> {
        if self.expected_tokens.is_empty() {
            if self.token.kind == *t {
                self.bump();
                Ok(false)
            } else {
                Err(self.unexpected_error(t))
            }
        } else {
            self.expect_one_of(std::slice::from_ref(t), &[])
        }
    }

    /// Creates a [`PErr`] for an unexpected token `t`.
    fn unexpected_error(&mut self, t: &TokenKind) -> PErr<'a> {
        let prev_span = if self.prev_token.span.is_dummy() {
            // We don't want to point at the following span after a dummy span.
            // This happens when the parser finds an empty token stream.
            self.token.span
        } else if self.token.kind == TokenKind::Eof {
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
        if self.token.kind == TokenKind::Eof {
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

    /// Parses an identifier.
    pub fn parse_ident(&mut self) -> PResult<'a, Ident> {
        self.parse_ident_maybe_recover(true)
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

    /// Advance the parser by one token.
    pub fn bump(&mut self) {
        let next = self.stream.next().unwrap_or(Token::EOF);
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
        self.stream.as_slice().get(dist).unwrap_or(&Token::EOF)
    }

    /// Calls `f` with the token `dist` tokens ahead of the current one.
    ///
    /// See [`look_ahead`](Self::look_ahead) for more information.
    pub fn look_ahead_with<R>(&self, dist: usize, f: impl FnOnce(&Token) -> R) -> R {
        f(self.look_ahead(dist))
    }
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
            (&[Operator, Ident], "an operator or an identifier"),
            (&[Path, Const, Token(TK::AndAnd)], "a path, a const expression, or `&&`"),
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

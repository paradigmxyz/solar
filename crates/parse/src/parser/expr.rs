use crate::{PResult, Parser};
use sulk_ast::{ast::*, token::*};

impl<'a> Parser<'a> {
    /// Parses an expression.
    pub fn parse_expr(&mut self) -> PResult<'a, Expr> {
        self.parse_spanned(Self::parse_expr_kind).map(|(span, kind)| Expr { span, kind })
    }

    /// Parses an expression kind.
    fn parse_expr_kind(&mut self) -> PResult<'a, ExprKind> {
        todo!()
    }

    /// Parses a list of function call arguments.
    pub(super) fn parse_call_args(&mut self) -> PResult<'a, CallArgs> {
        if self.look_ahead(1).kind == TokenKind::OpenDelim(Delimiter::Brace) {
            self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;
            let r = self.parse_named_args().map(CallArgs::Named)?;
            self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            Ok(r)
        } else {
            self.parse_unnamed_args().map(CallArgs::Unnamed)
        }
    }

    /// Parses a list of named arguments: `{a: b, c: d, ...}`
    fn parse_named_args(&mut self) -> PResult<'a, NamedArgList> {
        self.parse_delim_comma_seq(Delimiter::Brace, Self::parse_named_arg).map(|(x, _)| x)
    }

    /// Parses a single named argument: `a: b`.
    fn parse_named_arg(&mut self) -> PResult<'a, NamedArg> {
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Colon)?;
        let value = self.parse_expr()?;
        Ok(NamedArg { name, value })
    }

    /// Parses a list of expressions: `(a, b, c, ...)`.
    fn parse_unnamed_args(&mut self) -> PResult<'a, Vec<Expr>> {
        self.parse_paren_comma_seq(Self::parse_expr).map(|(x, _)| x)
    }
}

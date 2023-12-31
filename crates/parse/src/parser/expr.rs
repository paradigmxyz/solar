use crate::{PResult, Parser};
use sulk_ast::{ast::*, token::*};

impl<'a> Parser<'a> {
    /// Parses an expression.
    pub fn parse_expr(&mut self) -> PResult<'a, Expr> {
        todo!()
    }

    /// Parses a list of function call arguments.
    pub(super) fn parse_call_args(&mut self) -> PResult<'a, CallArgs> {
        if self.look_ahead(1).kind == TokenKind::OpenDelim(Delimiter::Brace) {
            self.parse_named_args().map(CallArgs::Named)
        } else {
            self.parse_unnamed_args().map(CallArgs::Unnamed)
        }
    }

    /// Parses a list of named arguments: `({a: b, c: d, ...})`
    pub(super) fn parse_named_args(&mut self) -> PResult<'a, NamedArgList> {
        self.parse_paren_comma_seq(|this| this.parse_named_arg()).map(|(x, _)| x)
    }

    /// Parses a single named argument: `a: b`.
    fn parse_named_arg(&mut self) -> PResult<'a, NamedArg> {
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Colon)?;
        let value = self.parse_expr()?;
        Ok(NamedArg { name, value })
    }

    /// Parses a list of expressions: `(a, b, c, ...)`.
    pub(super) fn parse_unnamed_args(&mut self) -> PResult<'a, Vec<Expr>> {
        self.parse_paren_comma_seq(|this| this.parse_expr()).map(|(x, _)| x)
    }
}

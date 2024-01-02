use crate::{PResult, Parser};
use sulk_ast::{ast::*};
use sulk_interface::Symbol;

impl<'a> Parser<'a> {
    /// Parses a literal.
    pub fn parse_lit(&mut self) -> PResult<'a, Lit> {
        let lo = self.token.span;
        let (symbol, kind) = self.parse_lit_inner()?;
        let span = lo.to(self.prev_token.span);
        Ok(Lit { span, symbol, kind })
    }

    fn parse_lit_inner(&mut self) -> PResult<'a, (Symbol, LitKind)> {
        todo!()
    }
}

use crate::{parser::SeqSep, PResult, Parser};
use sulk_ast::{ast::*, token::*};

impl<'a> Parser<'a> {
    /// Parses a statement.
    pub fn parse_stmt(&mut self) -> PResult<'a, Stmt> {
        todo!()
    }

    /// Parses a block of statements.
    pub fn parse_block(&mut self) -> PResult<'a, Block> {
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), |this| this.parse_stmt())
            .map(|(x, _)| x)
    }
}

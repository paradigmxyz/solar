use crate::{PResult, Parser};
use sulk_ast::ast::*;

impl<'a> Parser<'a> {
    /// Parses a Yul block.
    ///
    /// Yul entry point.
    pub fn parse_yul_block(&mut self) -> PResult<'a, yul::YulBlock> {
        todo!()
    }
}

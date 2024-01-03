use super::item::VarDeclMode;
use crate::{parser::SeqSep, PResult, Parser};
use sulk_ast::{ast::*, token::*};
use sulk_interface::kw;

impl<'a> Parser<'a> {
    /// Parses a statement.
    pub fn parse_stmt(&mut self) -> PResult<'a, Stmt> {
        self.parse_spanned(|this| this.parse_stmt_kind()).map(|(span, kind)| Stmt { kind, span })
    }

    /// Parses a statement kind.
    fn parse_stmt_kind(&mut self) -> PResult<'a, StmtKind> {
        let mut semi = true;
        let kind = if self.eat_keyword(kw::If) {
            semi = false;
            self.parse_stmt_if()
        } else if self.eat_keyword(kw::While) {
            semi = false;
            self.parse_stmt_while()
        } else if self.eat_keyword(kw::Do) {
            semi = false;
            self.parse_stmt_do_while()
        } else if self.eat_keyword(kw::For) {
            semi = false;
            self.parse_stmt_for()
        } else if self.eat_keyword(kw::Unchecked) {
            semi = false;
            self.parse_block().map(StmtKind::UncheckedBlock)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            semi = false;
            self.parse_block().map(StmtKind::Block)
        } else if self.eat_keyword(kw::Continue) {
            Ok(StmtKind::Continue)
        } else if self.eat_keyword(kw::Break) {
            Ok(StmtKind::Break)
        } else if self.eat_keyword(kw::Return) {
            self.parse_expr().map(StmtKind::Return)
        } else if self.eat_keyword(kw::Throw) {
            let msg = "`throw` statements have been removed; use `revert`, `require`, or `assert` instead";
            Err(self.dcx().err(msg).span(self.prev_token.span))
        } else if self.eat_keyword(kw::Try) {
            semi = false;
            self.parse_stmt_try().map(StmtKind::Try)
        } else if self.eat_keyword(kw::Assembly) {
            semi = false;
            self.parse_stmt_assembly().map(StmtKind::Assembly)
        } else if self.eat_keyword(kw::Emit) {
            self.parse_path_call().map(|(path, params)| StmtKind::Emit(path, params))
        } else if self.check_keyword(kw::Revert) && self.look_ahead(1).is_ident() {
            self.parse_path_call().map(|(path, params)| StmtKind::Revert(path, params))
        } else {
            semi = false;
            self.parse_simple_stmt_kind()
        };
        if semi && kind.is_ok() {
            self.expect_semi()?;
        }
        kind
    }

    /// Parses a block of statements.
    pub(super) fn parse_block(&mut self) -> PResult<'a, Block> {
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), |this| this.parse_stmt())
            .map(|(x, _)| x)
    }

    /// Parses an if statement.
    fn parse_stmt_if(&mut self) -> PResult<'a, StmtKind> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        let true_stmt = self.parse_stmt()?;
        let else_stmt =
            if self.eat_keyword(kw::Else) { Some(Box::new(self.parse_stmt()?)) } else { None };
        Ok(StmtKind::If(expr, Box::new(true_stmt), else_stmt))
    }

    /// Parses a while statement.
    fn parse_stmt_while(&mut self) -> PResult<'a, StmtKind> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        let stmt = self.parse_stmt()?;
        Ok(StmtKind::While(expr, Box::new(stmt)))
    }

    /// Parses a do-while statement.
    fn parse_stmt_do_while(&mut self) -> PResult<'a, StmtKind> {
        let block = self.parse_block()?;
        self.expect_keyword(kw::While)?;
        self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let expr = self.parse_expr()?;
        self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        Ok(StmtKind::DoWhile(block, expr))
    }

    /// Parses a for statement.
    fn parse_stmt_for(&mut self) -> PResult<'a, StmtKind> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;

        let init = if self.eat(&TokenKind::Semi) { None } else { Some(self.parse_simple_stmt()?) };
        // Semi parsed by either `eat` or `parse_simple_stmt`.

        let cond = if self.check(&TokenKind::Semi) { None } else { Some(self.parse_expr()?) };
        self.expect_semi()?;

        let next = if self.check_noexpect(&TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        let body = Box::new(self.parse_stmt()?);
        Ok(StmtKind::For { init: init.map(Box::new), cond, next, body })
    }

    /// Parses a try statement.
    fn parse_stmt_try(&mut self) -> PResult<'a, StmtTry> {
        let expr = self.parse_expr()?;
        let returns = if self.eat_keyword(kw::Returns) {
            self.parse_parameter_list(VarDeclMode::AllowStorage)?
        } else {
            Vec::new()
        };
        let block = self.parse_block()?;
        let mut catch = Vec::new();
        while self.eat_keyword(kw::Catch) {
            let name = self.parse_ident_opt()?;
            let args = if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                self.parse_call_args()?
            } else {
                CallArgs::empty()
            };
            let block = self.parse_block()?;
            catch.push(CatchClause { name, args, block })
        }
        Ok(StmtTry { expr, returns, block, catch })
    }

    /// Parses an assembly block.
    fn parse_stmt_assembly(&mut self) -> PResult<'a, StmtAssembly> {
        let dialect = self.parse_str_lit_opt();
        let flags = if self.eat(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            self.parse_paren_comma_seq(|this| this.parse_str_lit())?.0
        } else {
            Vec::new()
        };
        let block = self.in_yul(|this| this.parse_yul_block())?;
        Ok(StmtAssembly { dialect, flags, block })
    }

    /// Parses a simple statement. These are just variable declarations and expressions.
    fn parse_simple_stmt(&mut self) -> PResult<'a, Stmt> {
        self.parse_spanned(|this| this.parse_simple_stmt_kind())
            .map(|(span, kind)| Stmt { kind, span })
    }

    /// Parses a simple statement kind. These are just variable declarations and expressions.
    ///
    /// Also used in the for loop initializer.
    fn parse_simple_stmt_kind(&mut self) -> PResult<'a, StmtKind> {
        if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            todo!()
        } else {
            todo!()
        }
    }

    /// Parses a path and a list of call arguments.
    fn parse_path_call(&mut self) -> PResult<'a, (Path, CallArgs)> {
        let path = self.parse_path()?;
        let params = self.parse_call_args()?;
        Ok((path, params))
    }
}

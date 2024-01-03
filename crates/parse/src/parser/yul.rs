use super::SeqSep;
use crate::{PResult, Parser};
use sulk_ast::{
    ast::{yul::*, Path},
    token::*,
};
use sulk_interface::{kw, Ident};

impl<'a> Parser<'a> {
    /// Parses a Yul statement.
    pub fn parse_yul_stmt(&mut self) -> PResult<'a, Stmt> {
        self.parse_spanned(Self::parse_yul_stmt_kind).map(|(span, kind)| Stmt { span, kind })
    }

    /// Parses a Yul block.
    pub fn parse_yul_block(&mut self) -> PResult<'a, Block> {
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), Self::parse_yul_stmt).map(|(x, _)| x)
    }

    /// Parses a Yul statement kind.
    fn parse_yul_stmt_kind(&mut self) -> PResult<'a, StmtKind> {
        if self.eat_keyword(kw::Let) {
            self.parse_yul_stmt_var_decl()
        } else if self.eat_keyword(kw::Function) {
            self.parse_yul_function()
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            self.parse_yul_block().map(StmtKind::Block)
        } else if self.eat_keyword(kw::If) {
            self.parse_yul_stmt_if()
        } else if self.eat_keyword(kw::Switch) {
            self.parse_yul_stmt_switch().map(StmtKind::Switch)
        } else if self.eat_keyword(kw::For) {
            self.parse_yul_stmt_for()
        } else if self.eat_keyword(kw::Break) {
            Ok(StmtKind::Break)
        } else if self.eat_keyword(kw::Continue) {
            Ok(StmtKind::Continue)
        } else if self.eat_keyword(kw::Leave) {
            Ok(StmtKind::Leave)
        } else if self.check_ident() {
            let path = self.parse_path()?;
            if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                let name = self.expect_single_ident_path(path)?;
                let expr = self.parse_yul_expr_call_with(name)?;
                Ok(StmtKind::Expr(expr))
            } else if self.eat(&TokenKind::Walrus) {
                let expr = self.parse_yul_expr()?;
                Ok(StmtKind::AssignSingle(path, expr))
            } else if self.check(&TokenKind::Comma) {
                let mut paths = Vec::with_capacity(2);
                paths.push(path);
                while self.eat(&TokenKind::Comma) {
                    paths.push(self.parse_path()?);
                }
                self.expect(&TokenKind::Walrus)?;
                let expr = self.parse_yul_expr()?;
                let ExprKind::Call(expr) = expr.kind else {
                    let msg = "only function calls are allowed in multi-assignment";
                    return Err(self.dcx().err(msg).span(expr.span));
                };
                Ok(StmtKind::AssignMulti(paths, expr))
            } else {
                self.unexpected()
            }
        } else {
            self.unexpected()
        }
    }

    /// Parses a Yul variable declaration.
    fn parse_yul_stmt_var_decl(&mut self) -> PResult<'a, StmtKind> {
        let mut idents = Vec::new();
        loop {
            idents.push(self.parse_ident()?);
            if !self.eat(&TokenKind::Comma) {
                break;
            }
        }
        let expr = if self.eat(&TokenKind::Walrus) { Some(self.parse_yul_expr()?) } else { None };
        Ok(StmtKind::VarDecl(idents, expr))
    }

    /// Parses a Yul function definition.
    fn parse_yul_function(&mut self) -> PResult<'a, StmtKind> {
        let name = self.parse_ident()?;
        let (parameters, _) = self.parse_paren_comma_seq(Self::parse_ident)?;
        let returns = if self.eat(&TokenKind::Arrow) {
            self.check_ident();
            let (returns, _) = self.parse_nodelim_comma_seq(
                &TokenKind::CloseDelim(Delimiter::Brace),
                Self::parse_ident,
            )?;
            if returns.is_empty() {
                return self.unexpected();
            }
            returns
        } else {
            Vec::new()
        };
        let body = self.parse_yul_block()?;
        Ok(StmtKind::FunctionDef(Function { name, parameters, returns, body }))
    }

    /// Parses a Yul if statement.
    fn parse_yul_stmt_if(&mut self) -> PResult<'a, StmtKind> {
        let cond = self.parse_yul_expr()?;
        let body = self.parse_yul_block()?;
        Ok(StmtKind::If(cond, body))
    }

    /// Parses a Yul switch statement.
    fn parse_yul_stmt_switch(&mut self) -> PResult<'a, StmtSwitch> {
        let selector = self.parse_yul_expr()?;
        let mut branches = Vec::new();
        while self.eat_keyword(kw::Case) {
            let constant = self.parse_lit()?;
            let body = self.parse_yul_block()?;
            branches.push(StmtSwitchCase { constant, body });
        }
        let default_case =
            if self.eat_keyword(kw::Default) { Some(self.parse_yul_block()?) } else { None };
        Ok(StmtSwitch { selector, branches, default_case })
    }

    /// Parses a Yul for statement.
    fn parse_yul_stmt_for(&mut self) -> PResult<'a, StmtKind> {
        let init = self.parse_yul_block()?;
        let cond = self.parse_yul_expr()?;
        let step = self.parse_yul_block()?;
        let body = self.parse_yul_block()?;
        Ok(StmtKind::For { init, cond, step, body })
    }

    /// Parses a Yul expression.
    fn parse_yul_expr(&mut self) -> PResult<'a, Expr> {
        self.parse_spanned(Self::parse_yul_expr_kind).map(|(span, kind)| Expr { span, kind })
    }

    /// Parses a Yul expression kind.
    fn parse_yul_expr_kind(&mut self) -> PResult<'a, ExprKind> {
        if self.check_ident() {
            let ident = self.parse_ident()?;
            if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                self.parse_yul_expr_call_with(ident).map(ExprKind::Call)
            } else {
                Ok(ExprKind::Ident(ident))
            }
        } else if self.check_lit() {
            self.parse_lit().map(ExprKind::Lit)
        } else {
            self.unexpected()
        }
    }

    /// Parses a Yul function call expression with the given name.
    fn parse_yul_expr_call_with(&mut self, name: Ident) -> PResult<'a, ExprCall> {
        let (parameters, _) = self.parse_paren_comma_seq(Self::parse_yul_expr)?;
        Ok(ExprCall { name, arguments: parameters })
    }

    /// Expects a single identifier path and returns the identifier.
    fn expect_single_ident_path(&mut self, path: Path) -> PResult<'a, Ident> {
        match path.get_ident() {
            Some(ident) => Ok(*ident),
            None => Err(self.dcx().err("dotted paths aren't allowed here").span(path.span())),
        }
    }
}

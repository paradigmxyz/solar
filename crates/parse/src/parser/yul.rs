use super::SeqSep;
use crate::{PResult, Parser};
use sulk_ast::{
    ast::{yul::*, Path},
    token::*,
};
use sulk_interface::{error_code, kw, Ident};

impl<'a> Parser<'a> {
    /// Parses a Yul statement.
    pub fn parse_yul_stmt(&mut self) -> PResult<'a, Stmt> {
        self.in_yul(Self::parse_yul_stmt)
    }

    /// Parses a Yul statement, without setting `in_yul`.
    pub fn parse_yul_stmt_unchecked(&mut self) -> PResult<'a, Stmt> {
        let docs = self.parse_doc_comments()?;
        self.parse_spanned(Self::parse_yul_stmt_kind).map(|(span, kind)| Stmt { docs, span, kind })
    }

    /// Parses a Yul block.
    pub fn parse_yul_block(&mut self) -> PResult<'a, Block> {
        self.in_yul(Self::parse_yul_block_unchecked)
    }

    /// Parses a Yul block, without setting `in_yul`.
    pub(super) fn parse_yul_block_unchecked(&mut self) -> PResult<'a, Block> {
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), true, Self::parse_yul_stmt_unchecked)
            .map(|(x, _)| x)
    }

    /// Parses a Yul statement kind.
    fn parse_yul_stmt_kind(&mut self) -> PResult<'a, StmtKind> {
        if self.eat_keyword(kw::Let) {
            self.parse_yul_stmt_var_decl()
        } else if self.eat_keyword(kw::Function) {
            self.parse_yul_function()
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
            self.parse_yul_block_unchecked().map(StmtKind::Block)
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
            let path = self.parse_path_any()?;
            if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                let name = self.expect_single_ident_path(path);
                self.parse_yul_expr_call_with(name).map(StmtKind::Expr)
            } else if self.eat(&TokenKind::Walrus) {
                self.check_valid_path(&path);
                let expr = self.parse_yul_expr()?;
                Ok(StmtKind::AssignSingle(path, expr))
            } else if self.check(&TokenKind::Comma) {
                self.check_valid_path(&path);
                let mut paths = Vec::with_capacity(4);
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
        let (parameters, _) = self.parse_paren_comma_seq(true, Self::parse_ident)?;
        let returns = if self.eat(&TokenKind::Arrow) {
            self.check_ident();
            let (returns, _) = self.parse_nodelim_comma_seq(
                &TokenKind::OpenDelim(Delimiter::Brace),
                false,
                Self::parse_ident,
            )?;
            returns
        } else {
            Vec::new()
        };
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::FunctionDef(Function { name, parameters, returns, body }))
    }

    /// Parses a Yul if statement.
    fn parse_yul_stmt_if(&mut self) -> PResult<'a, StmtKind> {
        let cond = self.parse_yul_expr()?;
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::If(cond, body))
    }

    /// Parses a Yul switch statement.
    fn parse_yul_stmt_switch(&mut self) -> PResult<'a, StmtSwitch> {
        let lo = self.prev_token.span;
        let selector = self.parse_yul_expr()?;
        let mut branches = Vec::new();
        while self.eat_keyword(kw::Case) {
            let constant = self.parse_lit()?;
            self.expect_no_subdenomination();
            let body = self.parse_yul_block_unchecked()?;
            branches.push(StmtSwitchCase { constant, body });
        }
        let default_case = if self.eat_keyword(kw::Default) {
            Some(self.parse_yul_block_unchecked()?)
        } else {
            None
        };
        if branches.is_empty() {
            let span = lo.to(self.prev_token.span);
            if default_case.is_none() {
                self.dcx().err("`switch` statement has no cases").span(span).emit();
            } else {
                self.dcx()
                    .warn("`switch` statement has only a default case")
                    .span(span)
                    .code(error_code!(E9592))
                    .emit();
            }
        }
        Ok(StmtSwitch { selector, branches, default_case })
    }

    /// Parses a Yul for statement.
    fn parse_yul_stmt_for(&mut self) -> PResult<'a, StmtKind> {
        let init = self.parse_yul_block_unchecked()?;
        let cond = self.parse_yul_expr()?;
        let step = self.parse_yul_block_unchecked()?;
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::For { init, cond, step, body })
    }

    /// Parses a Yul expression.
    fn parse_yul_expr(&mut self) -> PResult<'a, Expr> {
        self.parse_spanned(Self::parse_yul_expr_kind).map(|(span, kind)| Expr { span, kind })
    }

    /// Parses a Yul expression kind.
    fn parse_yul_expr_kind(&mut self) -> PResult<'a, ExprKind> {
        if self.check_lit() {
            // NOTE: We can't `expect_no_subdenomination` because they're valid variable names.
            self.parse_lit().map(ExprKind::Lit)
        } else if self.check_path() {
            let path = self.parse_path_any()?;
            if self.token.is_open_delim(Delimiter::Parenthesis) {
                // Paths are not allowed in call expressions, but Solc parses them anyway.
                let ident = self.expect_single_ident_path(path);
                self.parse_yul_expr_call_with(ident).map(ExprKind::Call)
            } else {
                for &ident in path.segments() {
                    if ident.is_yul_keyword() || ident.is_yul_evm_builtin() {
                        self.expected_ident_found_other(ident.into(), false).unwrap_err().emit();
                    }
                }
                Ok(ExprKind::Path(path))
            }
        } else {
            self.unexpected()
        }
    }

    /// Parses a Yul function call expression with the given name.
    fn parse_yul_expr_call_with(&mut self, name: Ident) -> PResult<'a, ExprCall> {
        if !name.is_yul_evm_builtin() && name.is_reserved(true) {
            self.expected_ident_found_other(name.into(), false).unwrap_err().emit();
        }
        let (parameters, _) = self.parse_paren_comma_seq(true, Self::parse_yul_expr)?;
        Ok(ExprCall { name, arguments: parameters })
    }

    /// Expects a single identifier path and returns the identifier.
    #[track_caller]
    fn expect_single_ident_path(&mut self, path: Path) -> Ident {
        if path.segments().len() > 1 {
            self.dcx().err("fully-qualified paths aren't allowed here").span(path.span()).emit();
        }
        *path.last()
    }

    // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulPath
    #[track_caller]
    fn check_valid_path(&mut self, path: &Path) {
        let first = path.first();
        if first.is_reserved(true) {
            self.expected_ident_found_other((*first).into(), false).unwrap_err().emit();
        }
        for ident in &path.segments()[1..] {
            if !ident.is_yul_evm_builtin() && ident.is_reserved(true) {
                self.expected_ident_found_other((*ident).into(), false).unwrap_err().emit();
            }
        }
    }
}

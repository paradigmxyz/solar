use super::SeqSep;
use crate::{PResult, Parser};
use smallvec::SmallVec;
use solar_ast::{token::*, yul::*, AstPath, Box, DocComments, LitKind, PathSlice, StrKind, StrLit};
use solar_interface::{error_code, kw, sym, Ident};

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a Yul object or plain block.
    ///
    /// The plain block gets returned as a Yul object named "object", with a single `code` block.
    /// See: <https://github.com/ethereum/solidity/blob/eff410eb746f202fe756a2473fd0c8a718348457/libyul/ObjectParser.cpp#L50>
    #[instrument(level = "debug", skip_all)]
    pub fn parse_yul_file_object(&mut self) -> PResult<'sess, Object<'ast>> {
        let docs = self.parse_doc_comments();
        let object = if self.check_keyword(sym::object) {
            self.parse_yul_object(docs)
        } else {
            let lo = self.token.span;
            self.parse_yul_block().map(|code| {
                let span = lo.to(self.prev_token.span);
                let name = StrLit { span, value: sym::object };
                let code = CodeBlock { span, code };
                Object { docs, span, name, code, children: Box::default(), data: Box::default() }
            })
        }?;
        self.expect(TokenKind::Eof)?;
        Ok(object)
    }

    /// Parses a Yul object.
    ///
    /// Reference: <https://docs.soliditylang.org/en/latest/yul.html#specification-of-yul-object>
    pub fn parse_yul_object(&mut self, docs: DocComments<'ast>) -> PResult<'sess, Object<'ast>> {
        let lo = self.token.span;
        self.expect_keyword(sym::object)?;
        let name = self.parse_str_lit()?;

        self.expect(TokenKind::OpenDelim(Delimiter::Brace))?;
        let code = self.parse_yul_code()?;
        let mut children = Vec::new();
        let mut data = Vec::new();
        loop {
            let docs = self.parse_doc_comments();
            if self.check_keyword(sym::object) {
                children.push(self.parse_yul_object(docs)?);
            } else if self.check_keyword(sym::data) {
                data.push(self.parse_yul_data()?);
            } else {
                break;
            }
        }
        self.expect(TokenKind::CloseDelim(Delimiter::Brace))?;

        let span = lo.to(self.prev_token.span);
        let children = self.alloc_vec(children);
        let data = self.alloc_vec(data);
        Ok(Object { docs, span, name, code, children, data })
    }

    /// Parses a Yul code block.
    fn parse_yul_code(&mut self) -> PResult<'sess, CodeBlock<'ast>> {
        let lo = self.token.span;
        self.expect_keyword(sym::code)?;
        let code = self.parse_yul_block()?;
        let span = lo.to(self.prev_token.span);
        Ok(CodeBlock { span, code })
    }

    /// Parses a Yul data segment.
    fn parse_yul_data(&mut self) -> PResult<'sess, Data<'ast>> {
        let lo = self.token.span;
        self.expect_keyword(sym::data)?;
        let name = self.parse_str_lit()?;
        let data = self.parse_lit()?;
        if !matches!(data.kind, LitKind::Str(StrKind::Str | StrKind::Hex, ..)) {
            let msg = "only string and hex string literals are allowed in `data` segments";
            return Err(self.dcx().err(msg).span(data.span));
        }
        let span = lo.to(self.prev_token.span);
        Ok(Data { span, name, data })
    }

    /// Parses a Yul statement.
    pub fn parse_yul_stmt(&mut self) -> PResult<'sess, Stmt<'ast>> {
        self.in_yul(Self::parse_yul_stmt)
    }

    /// Parses a Yul statement, without setting `in_yul`.
    pub fn parse_yul_stmt_unchecked(&mut self) -> PResult<'sess, Stmt<'ast>> {
        let docs = self.parse_doc_comments();
        self.parse_spanned(Self::parse_yul_stmt_kind).map(|(span, kind)| Stmt { docs, span, kind })
    }

    /// Parses a Yul block.
    pub fn parse_yul_block(&mut self) -> PResult<'sess, Block<'ast>> {
        self.in_yul(Self::parse_yul_block_unchecked)
    }

    /// Parses a Yul block, without setting `in_yul`.
    pub fn parse_yul_block_unchecked(&mut self) -> PResult<'sess, Block<'ast>> {
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), true, Self::parse_yul_stmt_unchecked)
    }

    /// Parses a Yul statement kind.
    fn parse_yul_stmt_kind(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        if self.eat_keyword(kw::Let) {
            self.parse_yul_stmt_var_decl()
        } else if self.eat_keyword(kw::Function) {
            self.parse_yul_function()
        } else if self.check(TokenKind::OpenDelim(Delimiter::Brace)) {
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
            if self.check(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                let name = self.expect_single_ident_path(path);
                self.parse_yul_expr_call_with(name).map(StmtKind::Expr)
            } else if self.eat(TokenKind::Walrus) {
                self.check_valid_path(path);
                let expr = self.parse_yul_expr()?;
                Ok(StmtKind::AssignSingle(path, expr))
            } else if self.check(TokenKind::Comma) {
                self.check_valid_path(path);
                let mut paths = SmallVec::<[_; 4]>::new();
                paths.push(path);
                while self.eat(TokenKind::Comma) {
                    paths.push(self.parse_path()?);
                }
                let paths = self.alloc_smallvec(paths);
                self.expect(TokenKind::Walrus)?;
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
    fn parse_yul_stmt_var_decl(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let mut idents = SmallVec::<[_; 8]>::new();
        loop {
            idents.push(self.parse_ident()?);
            if !self.eat(TokenKind::Comma) {
                break;
            }
        }
        let idents = self.alloc_smallvec(idents);
        let expr = if self.eat(TokenKind::Walrus) { Some(self.parse_yul_expr()?) } else { None };
        Ok(StmtKind::VarDecl(idents, expr))
    }

    /// Parses a Yul function definition.
    fn parse_yul_function(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let name = self.parse_ident()?;
        let parameters = self.parse_paren_comma_seq(true, Self::parse_ident)?;
        let returns = if self.eat(TokenKind::Arrow) {
            self.parse_nodelim_comma_seq(
                TokenKind::OpenDelim(Delimiter::Brace),
                false,
                Self::parse_ident,
            )?
        } else {
            Default::default()
        };
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::FunctionDef(Function { name, parameters, returns, body }))
    }

    /// Parses a Yul if statement.
    fn parse_yul_stmt_if(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let cond = self.parse_yul_expr()?;
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::If(cond, body))
    }

    /// Parses a Yul switch statement.
    fn parse_yul_stmt_switch(&mut self) -> PResult<'sess, StmtSwitch<'ast>> {
        let lo = self.prev_token.span;
        let selector = self.parse_yul_expr()?;
        let mut branches = Vec::new();
        while self.eat_keyword(kw::Case) {
            let constant = self.parse_lit()?;
            self.expect_no_subdenomination();
            let body = self.parse_yul_block_unchecked()?;
            branches.push(StmtSwitchCase { constant, body });
        }
        let branches = self.alloc_vec(branches);
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
                    .code(error_code!(9592))
                    .emit();
            }
        }
        Ok(StmtSwitch { selector, branches, default_case })
    }

    /// Parses a Yul for statement.
    fn parse_yul_stmt_for(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let init = self.parse_yul_block_unchecked()?;
        let cond = self.parse_yul_expr()?;
        let step = self.parse_yul_block_unchecked()?;
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::For { init, cond, step, body })
    }

    /// Parses a Yul expression.
    fn parse_yul_expr(&mut self) -> PResult<'sess, Expr<'ast>> {
        self.parse_spanned(Self::parse_yul_expr_kind).map(|(span, kind)| Expr { span, kind })
    }

    /// Parses a Yul expression kind.
    fn parse_yul_expr_kind(&mut self) -> PResult<'sess, ExprKind<'ast>> {
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
                self.check_valid_path(path);
                Ok(ExprKind::Path(path))
            }
        } else {
            self.unexpected()
        }
    }

    /// Parses a Yul function call expression with the given name.
    fn parse_yul_expr_call_with(&mut self, name: Ident) -> PResult<'sess, ExprCall<'ast>> {
        if !name.is_yul_evm_builtin() && name.is_reserved(true) {
            self.expected_ident_found_other(name.into(), false).unwrap_err().emit();
        }
        let arguments = self.parse_paren_comma_seq(true, Self::parse_yul_expr)?;
        Ok(ExprCall { name, arguments })
    }

    /// Expects a single identifier path and returns the identifier.
    #[track_caller]
    fn expect_single_ident_path(&mut self, path: AstPath<'_>) -> Ident {
        if path.segments().len() > 1 {
            self.dcx().err("fully-qualified paths aren't allowed here").span(path.span()).emit();
        }
        *path.last()
    }

    // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulPath
    #[track_caller]
    fn check_valid_path(&mut self, path: &PathSlice) {
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

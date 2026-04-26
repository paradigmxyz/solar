use super::{ExpectedToken, SeqSep};
use crate::{PResult, Parser};
use smallvec::SmallVec;
use solar_ast::{
    AstPath, Base, Box, DocComments, Lit, LitKind, PathSlice, StrKind, StrLit, Symbol, token::*,
    yul::*,
};
use solar_interface::{Ident, error_code, kw, sym};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum YulExprShape {
    Call,
    Other,
}

struct YulPathParts {
    idents: SmallVec<[Ident; 4]>,
}

impl YulPathParts {
    fn single(first: Ident) -> Self {
        let mut idents = SmallVec::new();
        idents.push(first);
        Self { idents }
    }

    fn push(&mut self, ident: Ident) {
        self.idents.push(ident);
    }

    fn len(&self) -> usize {
        self.idents.len()
    }

    fn first(&self) -> Ident {
        self.idents[0]
    }

    fn last(&self) -> Ident {
        self.idents[self.idents.len() - 1]
    }

    fn span(&self) -> solar_interface::Span {
        self.first().span.to(self.last().span)
    }

    fn tail(&self) -> &[Ident] {
        &self.idents[1..]
    }
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a Yul object or plain block.
    ///
    /// The plain block gets returned as a Yul object named "object", with a single `code` block.
    /// See: <https://github.com/argotorg/solidity/blob/eff410eb746f202fe756a2473fd0c8a718348457/libyul/ObjectParser.cpp#L50>
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
        let data = self.parse_yul_lit()?;
        if !matches!(data.kind, LitKind::Str(StrKind::Str | StrKind::Hex, ..)) {
            let msg = "only string and hex string literals are allowed in `data` segments";
            return Err(self.dcx().err(msg).span(data.span));
        }
        let span = lo.to(self.prev_token.span);
        Ok(Data { span, name, data })
    }

    fn parse_yul_lit(&mut self) -> PResult<'sess, Lit<'ast>> {
        let (lit, subdenomination) = self.parse_lit(false)?;
        assert!(subdenomination.is_none());
        Ok(lit)
    }

    fn parse_yul_lit_discard(&mut self) -> PResult<'sess, ()> {
        if let Some(lit) = self.token.lit()
            && lit.kind == TokenLitKind::Integer
            && can_skip_integer_lit(lit.symbol)
        {
            // Small and definitely-in-range integers do not need full literal construction in the
            // discard parser. Larger or ambiguous literals fall back to `parse_yul_lit` so normal
            // overflow and leading-zero diagnostics are preserved.
            self.bump();
            return Ok(());
        }
        self.parse_yul_lit().map(drop)
    }

    /// Parses a Yul statement.
    pub fn parse_yul_stmt(&mut self) -> PResult<'sess, Stmt<'ast>> {
        self.in_yul(Self::parse_yul_stmt_unchecked)
    }

    /// Parses a Yul statement, without setting `in_yul`.
    pub fn parse_yul_stmt_unchecked(&mut self) -> PResult<'sess, Stmt<'ast>> {
        self.with_recursion_limit("Yul statement", |this| {
            let docs = this.parse_doc_comments();
            this.parse_spanned(Self::parse_yul_stmt_kind).map(|(span, kind)| Stmt {
                docs,
                span,
                kind,
            })
        })
    }

    /// Parses a Yul block.
    pub fn parse_yul_block(&mut self) -> PResult<'sess, Block<'ast>> {
        self.in_yul(Self::parse_yul_block_unchecked)
    }

    /// Parses a Yul block, without setting `in_yul`.
    pub fn parse_yul_block_unchecked(&mut self) -> PResult<'sess, Block<'ast>> {
        if !self.retain_yul_ast {
            // Inline assembly is not lowered into HIR yet. In normal compilation we still parse it
            // fully for syntax and diagnostics, but avoid allocating the nested Yul AST.
            return self.parse_yul_block_discard();
        }
        let lo = self.token.span;
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), true, Self::parse_yul_stmt_unchecked)
            .map(|stmts| {
                let span = lo.to(self.prev_token.span);
                Block { span, stmts }
            })
    }

    fn parse_yul_block_discard(&mut self) -> PResult<'sess, Block<'ast>> {
        let lo = self.token.span;
        self.expect(TokenKind::OpenDelim(Delimiter::Brace))?;
        while !self.check_noexpect(TokenKind::CloseDelim(Delimiter::Brace)) {
            if self.token.is_eof() {
                self.expect(TokenKind::CloseDelim(Delimiter::Brace))?;
                break;
            }
            self.parse_yul_stmt_discard()?;
        }
        self.expect(TokenKind::CloseDelim(Delimiter::Brace))?;
        let span = lo.to(self.prev_token.span);
        Ok(Block { span, stmts: Default::default() })
    }

    fn parse_yul_stmt_discard(&mut self) -> PResult<'sess, ()> {
        let res = if self.enter_recursion_limit() {
            let _ = self.parse_doc_comments();
            self.parse_yul_stmt_kind_discard()
        } else {
            Err(self.recursion_limit_reached("Yul statement"))
        };
        self.exit_recursion_limit();
        res
    }

    fn parse_yul_stmt_kind_discard(&mut self) -> PResult<'sess, ()> {
        match self.token.kind {
            TokenKind::Ident(kw::Let) => {
                self.bump();
                self.parse_yul_var_decl_discard()
            }
            TokenKind::Ident(kw::Function) => {
                self.bump();
                self.parse_yul_function_discard()
            }
            TokenKind::OpenDelim(Delimiter::Brace) => self.parse_yul_block_discard().map(drop),
            TokenKind::Ident(kw::If) => {
                self.bump();
                self.parse_yul_expr_discard()?;
                self.parse_yul_block_discard().map(drop)
            }
            TokenKind::Ident(kw::Switch) => {
                self.bump();
                self.parse_yul_switch_discard()
            }
            TokenKind::Ident(kw::For) => {
                self.bump();
                self.parse_yul_block_discard()?;
                self.parse_yul_expr_discard()?;
                self.parse_yul_block_discard()?;
                self.parse_yul_block_discard().map(drop)
            }
            TokenKind::Ident(kw::Break | kw::Continue | kw::Leave) => {
                self.bump();
                Ok(())
            }
            TokenKind::Ident(_) => self.parse_yul_ident_stmt_discard(),
            _ => {
                self.push_expected_yul_stmt_start();
                self.unexpected()
            }
        }
    }

    /// Parses a Yul statement kind.
    fn parse_yul_stmt_kind(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        if self.token.is_keyword(kw::Let) {
            self.bump();
            self.parse_yul_stmt_var_decl()
        } else if self.token.is_keyword(kw::Function) {
            self.bump();
            self.parse_yul_function()
        } else if self.token.is_open_delim(Delimiter::Brace) {
            self.parse_yul_block_unchecked().map(StmtKind::Block)
        } else if self.token.is_keyword(kw::If) {
            self.bump();
            self.parse_yul_stmt_if()
        } else if self.token.is_keyword(kw::Switch) {
            self.bump();
            self.parse_yul_stmt_switch().map(StmtKind::Switch)
        } else if self.token.is_keyword(kw::For) {
            self.bump();
            self.parse_yul_stmt_for()
        } else if self.token.is_keyword(kw::Break) {
            self.bump();
            Ok(StmtKind::Break)
        } else if self.token.is_keyword(kw::Continue) {
            self.bump();
            Ok(StmtKind::Continue)
        } else if self.token.is_keyword(kw::Leave) {
            self.bump();
            Ok(StmtKind::Leave)
        } else if self.token.is_ident() {
            let lo = self.token.span;
            let first = self.parse_ident_any()?;
            if self.check_noexpect(TokenKind::Dot) {
                let path = self.parse_path_with_f(first, Self::parse_ident_any)?;
                if self.check(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                    let name = self.expect_single_ident_path(path);
                    let call = self.parse_yul_expr_call_with(name)?;
                    let span = lo.to(self.prev_token.span);
                    Ok(StmtKind::Expr(Expr { span, kind: ExprKind::Call(call) }))
                } else if self.eat(TokenKind::Walrus) {
                    self.check_valid_path(&path);
                    let expr = self.parse_yul_expr()?;
                    Ok(StmtKind::AssignSingle(path, expr))
                } else if self.check(TokenKind::Comma) {
                    self.check_valid_path(&path);
                    let mut paths = SmallVec::<[_; 4]>::new();
                    paths.push(path);
                    while self.eat(TokenKind::Comma) {
                        paths.push(self.parse_yul_path()?);
                    }
                    let paths = self.alloc_smallvec(paths);
                    self.expect(TokenKind::Walrus)?;
                    let expr = self.parse_yul_expr()?;
                    let ExprKind::Call(_expr) = &expr.kind else {
                        let msg = "only function calls are allowed in multi-assignment";
                        return Err(self.dcx().err(msg).span(expr.span));
                    };
                    Ok(StmtKind::AssignMulti(paths, expr))
                } else {
                    self.unexpected()
                }
            } else if self.check(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                let call = self.parse_yul_expr_call_with(first)?;
                let span = lo.to(self.prev_token.span);
                Ok(StmtKind::Expr(Expr { span, kind: ExprKind::Call(call) }))
            } else if self.eat(TokenKind::Walrus) {
                let path = self.alloc_path(&[first]);
                self.check_valid_single_path_ident(first);
                let expr = self.parse_yul_expr()?;
                Ok(StmtKind::AssignSingle(path, expr))
            } else if self.check(TokenKind::Comma) {
                let path = self.alloc_path(&[first]);
                self.check_valid_single_path_ident(first);
                let mut paths = SmallVec::<[_; 4]>::new();
                paths.push(path);
                while self.eat(TokenKind::Comma) {
                    paths.push(self.parse_yul_path()?);
                }
                let paths = self.alloc_smallvec(paths);
                self.expect(TokenKind::Walrus)?;
                let expr = self.parse_yul_expr()?;
                let ExprKind::Call(_expr) = &expr.kind else {
                    let msg = "only function calls are allowed in multi-assignment";
                    return Err(self.dcx().err(msg).span(expr.span));
                };
                Ok(StmtKind::AssignMulti(paths, expr))
            } else {
                self.unexpected()
            }
        } else {
            self.push_expected_yul_stmt_start();
            self.unexpected()
        }
    }

    fn push_expected_yul_stmt_start(&mut self) {
        self.push_expected(ExpectedToken::Keyword(kw::Let));
        self.push_expected(ExpectedToken::Keyword(kw::Function));
        self.push_expected(ExpectedToken::Token(TokenKind::OpenDelim(Delimiter::Brace)));
        self.push_expected(ExpectedToken::Keyword(kw::If));
        self.push_expected(ExpectedToken::Keyword(kw::Switch));
        self.push_expected(ExpectedToken::Keyword(kw::For));
        self.push_expected(ExpectedToken::Keyword(kw::Break));
        self.push_expected(ExpectedToken::Keyword(kw::Continue));
        self.push_expected(ExpectedToken::Keyword(kw::Leave));
        self.push_expected(ExpectedToken::Ident);
    }

    fn push_expected_yul_expr_start(&mut self) {
        self.push_expected(ExpectedToken::Lit);
        self.push_expected(ExpectedToken::Path);
    }

    fn parse_yul_var_decl_discard(&mut self) -> PResult<'sess, ()> {
        loop {
            self.parse_ident()?;
            if !self.eat_noexpect(TokenKind::Comma) {
                break;
            }
        }
        if self.eat_noexpect(TokenKind::Walrus) {
            self.parse_yul_expr_discard()?;
        }
        Ok(())
    }

    fn parse_yul_function_discard(&mut self) -> PResult<'sess, ()> {
        self.parse_ident()?;
        self.parse_yul_ident_paren_list_discard(true)?;
        if self.eat_noexpect(TokenKind::Arrow) {
            self.parse_yul_ident_list_until_brace_discard()?;
        }
        self.parse_yul_block_discard().map(drop)
    }

    fn parse_yul_switch_discard(&mut self) -> PResult<'sess, ()> {
        let lo = self.prev_token.span;
        self.parse_yul_expr_discard()?;
        let mut cases = 0usize;
        while self.check_keyword_noexpect(kw::Case) {
            self.bump();
            self.parse_yul_lit_discard()?;
            self.expect_no_subdenomination();
            self.parse_yul_block_discard()?;
            cases += 1;
        }
        let has_default = if self.check_keyword_noexpect(kw::Default) {
            self.bump();
            self.parse_yul_block_discard()?;
            true
        } else {
            false
        };
        if cases == 0 {
            let span = lo.to(self.prev_token.span);
            if !has_default {
                self.dcx().err("`switch` statement has no cases").span(span).emit();
            } else {
                self.dcx()
                    .warn("`switch` statement has only a default case")
                    .span(span)
                    .code(error_code!(9592))
                    .emit();
            }
        }
        Ok(())
    }

    fn parse_yul_ident_stmt_discard(&mut self) -> PResult<'sess, ()> {
        let first = self.parse_ident_any()?;
        if self.check_noexpect(TokenKind::Dot) {
            let path = self.parse_yul_path_after_first_discard(first)?;
            if self.check_noexpect(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                if path.len() > 1 {
                    self.dcx()
                        .err("fully-qualified paths aren't allowed here")
                        .span(path.span())
                        .emit();
                }
                self.parse_yul_call_args_discard(path.last())?;
            } else if self.eat_noexpect(TokenKind::Walrus) {
                self.check_valid_yul_path_parts(&path);
                self.parse_yul_expr_discard()?;
            } else if self.check_noexpect(TokenKind::Comma) {
                self.check_valid_yul_path_parts(&path);
                while self.eat_noexpect(TokenKind::Comma) {
                    let path = self.parse_yul_path_discard()?;
                    self.check_valid_yul_path_parts(&path);
                }
                self.expect(TokenKind::Walrus)?;
                if self.parse_yul_expr_discard()? != YulExprShape::Call {
                    let msg = "only function calls are allowed in multi-assignment";
                    return Err(self.dcx().err(msg).span(self.prev_token.span));
                }
            } else {
                return self.unexpected();
            }
        } else if self.check_noexpect(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            self.parse_yul_call_args_discard(first)?;
        } else if self.eat_noexpect(TokenKind::Walrus) {
            self.check_valid_single_yul_path_ident(first);
            self.parse_yul_expr_discard()?;
        } else if self.check_noexpect(TokenKind::Comma) {
            self.check_valid_single_yul_path_ident(first);
            while self.eat_noexpect(TokenKind::Comma) {
                let path = self.parse_yul_path_discard()?;
                self.check_valid_yul_path_parts(&path);
            }
            self.expect(TokenKind::Walrus)?;
            if self.parse_yul_expr_discard()? != YulExprShape::Call {
                let msg = "only function calls are allowed in multi-assignment";
                return Err(self.dcx().err(msg).span(self.prev_token.span));
            }
        } else {
            return self.unexpected();
        }
        Ok(())
    }

    fn parse_yul_expr_discard(&mut self) -> PResult<'sess, YulExprShape> {
        match self.token.kind {
            TokenKind::Literal(..) | TokenKind::Ident(kw::True | kw::False) => {
                self.parse_yul_lit_discard()?;
                Ok(YulExprShape::Other)
            }
            TokenKind::Ident(_) => {
                let first = self.parse_ident_any()?;
                if self.check_noexpect(TokenKind::Dot) {
                    let path = self.parse_yul_path_after_first_discard(first)?;
                    if self.token.kind == TokenKind::OpenDelim(Delimiter::Parenthesis) {
                        if path.len() > 1 {
                            self.dcx()
                                .err("fully-qualified paths aren't allowed here")
                                .span(path.span())
                                .emit();
                        }
                        self.parse_yul_call_args_discard(path.last())?;
                        Ok(YulExprShape::Call)
                    } else {
                        self.check_valid_yul_path_parts(&path);
                        Ok(YulExprShape::Other)
                    }
                } else if self.token.kind == TokenKind::OpenDelim(Delimiter::Parenthesis) {
                    self.parse_yul_call_args_discard(first)?;
                    Ok(YulExprShape::Call)
                } else {
                    self.check_valid_single_yul_path_ident(first);
                    Ok(YulExprShape::Other)
                }
            }
            _ => {
                self.push_expected_yul_expr_start();
                self.unexpected()
            }
        }
    }

    fn parse_yul_call_args_discard(&mut self, name: Ident) -> PResult<'sess, ()> {
        if name.is_yul_keyword() {
            self.expected_ident_found_other(name.into(), false).unwrap_err().emit();
        }
        debug_assert!(self.token.is_open_delim(Delimiter::Parenthesis));
        self.bump();
        if self.eat_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            return Ok(());
        }
        loop {
            self.parse_yul_expr_discard()?;
            if self.check_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                break;
            }
            if !self.eat_noexpect(TokenKind::Comma) {
                self.expect(TokenKind::Comma)?;
            }
            if self.check_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                self.dcx()
                    .err("trailing `,` separator is not allowed")
                    .span(self.prev_token.span)
                    .emit();
                break;
            }
        }
        self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis)).map(drop)
    }

    fn parse_yul_ident_paren_list_discard(&mut self, allow_empty: bool) -> PResult<'sess, ()> {
        self.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        if allow_empty && self.eat_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            return Ok(());
        }
        loop {
            self.parse_ident()?;
            if self.check_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                break;
            }
            if !self.eat_noexpect(TokenKind::Comma) {
                self.expect(TokenKind::Comma)?;
            }
            if self.check_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                self.dcx()
                    .err("trailing `,` separator is not allowed")
                    .span(self.prev_token.span)
                    .emit();
                break;
            }
        }
        self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis)).map(drop)
    }

    fn parse_yul_ident_list_until_brace_discard(&mut self) -> PResult<'sess, ()> {
        if self.check_noexpect(TokenKind::OpenDelim(Delimiter::Brace)) {
            self.push_expected(ExpectedToken::Ident);
            return self.unexpected();
        }
        loop {
            self.parse_ident()?;
            if self.check_noexpect(TokenKind::OpenDelim(Delimiter::Brace)) {
                break;
            }
            if !self.eat_noexpect(TokenKind::Comma) {
                self.expect(TokenKind::Comma)?;
            }
            if self.check_noexpect(TokenKind::OpenDelim(Delimiter::Brace)) {
                self.dcx()
                    .err("trailing `,` separator is not allowed")
                    .span(self.prev_token.span)
                    .emit();
                break;
            }
        }
        Ok(())
    }

    fn parse_yul_path_discard(&mut self) -> PResult<'sess, YulPathParts> {
        let first = self.parse_ident_any()?;
        self.parse_yul_path_after_first_discard(first)
    }

    fn parse_yul_path_after_first_discard(&mut self, first: Ident) -> PResult<'sess, YulPathParts> {
        let mut path = YulPathParts::single(first);
        while self.eat_noexpect(TokenKind::Dot) {
            path.push(self.parse_ident_any()?);
        }
        Ok(path)
    }

    fn check_valid_yul_path_parts(&mut self, path: &YulPathParts) {
        let first = path.first();
        if first.is_yul_keyword() || (path.len() == 1 && first.is_yul_evm_builtin()) {
            self.expected_ident_found_other(first.into(), false).unwrap_err().emit();
        }
        for &ident in path.tail() {
            if ident.is_yul_keyword() {
                self.expected_ident_found_other(ident.into(), false).unwrap_err().emit();
            }
        }
    }

    fn check_valid_single_yul_path_ident(&mut self, ident: Ident) {
        if ident.is_yul_keyword() || ident.is_yul_evm_builtin() {
            self.expected_ident_found_other(ident.into(), false).unwrap_err().emit();
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
        let mut cases = Vec::new();
        while self.check_keyword(kw::Case) {
            cases.push(self.parse_yul_stmt_switch_case(kw::Case)?);
        }
        let default_case = if self.check_keyword(kw::Default) {
            Some(self.parse_yul_stmt_switch_case(kw::Default)?)
        } else {
            None
        };
        if cases.is_empty() {
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
        if let Some(default_case) = default_case {
            cases.push(default_case);
        }
        let cases = self.alloc_vec(cases);
        Ok(StmtSwitch { selector, cases })
    }

    fn parse_yul_stmt_switch_case(&mut self, kw: Symbol) -> PResult<'sess, StmtSwitchCase<'ast>> {
        self.parse_spanned(|this| {
            debug_assert!(this.token.is_keyword(kw));
            this.bump();
            let constant = if kw == kw::Case {
                let lit = this.parse_yul_lit()?;
                this.expect_no_subdenomination();
                Some(lit)
            } else {
                None
            };
            let body = this.parse_yul_block_unchecked()?;
            Ok((constant, body))
        })
        .map(|(span, (constant, body))| StmtSwitchCase { span, constant, body })
    }

    /// Parses a Yul for statement.
    fn parse_yul_stmt_for(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let init = self.parse_yul_block_unchecked()?;
        let cond = self.parse_yul_expr()?;
        let step = self.parse_yul_block_unchecked()?;
        let body = self.parse_yul_block_unchecked()?;
        Ok(StmtKind::For(self.alloc(StmtFor { init, cond, step, body })))
    }

    /// Parses a Yul expression.
    fn parse_yul_expr(&mut self) -> PResult<'sess, Expr<'ast>> {
        self.parse_spanned(Self::parse_yul_expr_kind).map(|(span, kind)| Expr { span, kind })
    }

    /// Parses a Yul expression kind.
    fn parse_yul_expr_kind(&mut self) -> PResult<'sess, ExprKind<'ast>> {
        if self.token.is_lit() {
            // NOTE: We can't `expect_no_subdenomination` because they're valid variable names.
            self.parse_yul_lit().map(|lit| ExprKind::Lit(self.alloc(lit)))
        } else if self.token.is_ident() {
            let first = self.parse_ident_any()?;
            if self.check_noexpect(TokenKind::Dot) {
                let path = self.parse_path_with_f(first, Self::parse_ident_any)?;
                if self.token.is_open_delim(Delimiter::Parenthesis) {
                    // Paths are not allowed in call expressions, but Solc parses them anyway.
                    let ident = self.expect_single_ident_path(path);
                    self.parse_yul_expr_call_with(ident).map(ExprKind::Call)
                } else {
                    self.check_valid_path(&path);
                    Ok(ExprKind::Path(path))
                }
            } else if self.token.is_open_delim(Delimiter::Parenthesis) {
                // Paths are not allowed in call expressions, but Solc parses them anyway.
                self.parse_yul_expr_call_with(first).map(ExprKind::Call)
            } else {
                let path = self.alloc_path(&[first]);
                self.check_valid_single_path_ident(first);
                Ok(ExprKind::Path(path))
            }
        } else {
            self.push_expected_yul_expr_start();
            self.unexpected()
        }
    }

    /// Parses a Yul function call expression with the given name.
    fn parse_yul_expr_call_with(&mut self, name: Ident) -> PResult<'sess, ExprCall<'ast>> {
        if name.is_yul_keyword() {
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

    fn parse_yul_path(&mut self) -> PResult<'sess, AstPath<'ast>> {
        let path = self.parse_path_any()?;
        self.check_valid_path(&path);
        Ok(path)
    }

    // https://docs.soliditylang.org/en/latest/grammar.html#a4.SolidityParser.yulPath
    #[track_caller]
    fn check_valid_path(&mut self, path: &PathSlice) {
        // We allow EVM builtins in any position if multiple segments are present:
        // https://github.com/argotorg/solidity/issues/16054
        let segments = path.segments();
        let first = segments[0];
        if first.is_yul_keyword() || (segments.len() == 1 && first.is_yul_evm_builtin()) {
            self.expected_ident_found_other(first.into(), false).unwrap_err().emit();
        }
        for &ident in &segments[1..] {
            if ident.is_yul_keyword() {
                self.expected_ident_found_other(ident.into(), false).unwrap_err().emit();
            }
        }
    }

    #[track_caller]
    fn check_valid_single_path_ident(&mut self, ident: Ident) {
        if ident.is_yul_keyword() || ident.is_yul_evm_builtin() {
            self.expected_ident_found_other(ident.into(), false).unwrap_err().emit();
        }
    }
}

fn can_skip_integer_lit(symbol: Symbol) -> bool {
    if can_skip_common_integer_lit(symbol) {
        return true;
    }

    let s = symbol.as_str();
    let bytes = s.as_bytes();
    let (base, digits) = match bytes {
        [b'0', b'x', rest @ ..] | [b'0', b'X', rest @ ..] => (Base::Hexadecimal, rest),
        [b'0', b'b' | b'o', ..] | [b'0', b'B' | b'O', ..] => return false,
        _ => (Base::Decimal, bytes),
    };

    let mut digit_count = 0usize;
    let mut first_digit = None;
    for &b in digits {
        if b == b'_' {
            continue;
        }
        first_digit.get_or_insert(b);
        digit_count += 1;
    }
    if digit_count == 0 {
        return false;
    }

    match base {
        Base::Decimal => {
            if digit_count > 1 && first_digit == Some(b'0') {
                return false;
            }
            digit_count < 78
        }
        Base::Hexadecimal => digit_count <= 64,
        Base::Binary | Base::Octal => false,
    }
}

#[inline]
fn can_skip_common_integer_lit(symbol: Symbol) -> bool {
    (symbol >= sym::integer(0) && symbol <= sym::integer(9))
        || symbol == sym::zero_x00
        || symbol == sym::zero_x01
        || symbol == sym::zero_x04
        || symbol == sym::zero_x0c
        || symbol == sym::zero_x14
        || symbol == sym::zero_x1c
        || symbol == sym::zero_x1f
        || symbol == sym::zero_x20
        || symbol == sym::zero_x40
        || symbol == sym::zero_x60
        || symbol == sym::zero_x80
        || symbol == sym::zero_xff
}

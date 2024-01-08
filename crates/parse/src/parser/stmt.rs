use super::item::VarDeclMode;
use crate::{parser::SeqSep, PResult, Parser};
use sulk_ast::{ast::*, token::*};
use sulk_interface::kw;

impl<'a> Parser<'a> {
    /// Parses a statement.
    pub fn parse_stmt(&mut self) -> PResult<'a, Stmt> {
        let docs = self.parse_doc_comments()?;
        self.parse_spanned(Self::parse_stmt_kind).map(|(span, kind)| Stmt { docs, kind, span })
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
            let expr = if self.check(&TokenKind::Semi) { None } else { Some(self.parse_expr()?) };
            Ok(StmtKind::Return(expr))
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
            self.bump(); // `revert`
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
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), Self::parse_stmt).map(|(x, _)| x)
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
                self.parse_parameter_list(VarDeclMode::AllowStorage)?
            } else {
                Vec::new()
            };
            let block = self.parse_block()?;
            catch.push(CatchClause { name, args, block })
        }
        Ok(StmtTry { expr, returns, block, catch })
    }

    /// Parses an assembly block.
    fn parse_stmt_assembly(&mut self) -> PResult<'a, StmtAssembly> {
        let dialect = self.parse_str_lit_opt();
        let flags = if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            self.parse_paren_comma_seq(Self::parse_str_lit)?.0
        } else {
            Vec::new()
        };
        let block = self.parse_yul_block()?;
        Ok(StmtAssembly { dialect, flags, block })
    }

    /// Parses a simple statement. These are just variable declarations and expressions.
    fn parse_simple_stmt(&mut self) -> PResult<'a, Stmt> {
        let docs = self.parse_doc_comments()?;
        self.parse_spanned(Self::parse_simple_stmt_kind).map(|(span, kind)| Stmt {
            docs,
            kind,
            span,
        })
    }

    /// Parses a simple statement kind. These are just variable declarations and expressions.
    ///
    /// Also used in the for loop initializer.
    fn parse_simple_stmt_kind(&mut self) -> PResult<'a, StmtKind> {
        // TODO: This is probably wrong.
        if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            let (span, tuple) = self.parse_spanned(|this| {
                this.parse_seq_optional_items(Delimiter::Parenthesis, |this| {
                    this.parse_expr_or_var(true)
                })
            })?;
            if self.eat(&TokenKind::Semi) {
                // (,,a.b[c],);
                if tuple.iter().any(Option::is_none) {
                    let msg = "elements in tuple expressions cannot be empty";
                    self.dcx().err(msg).span(span).emit();
                }
                let exprs = self.map_option_list(tuple, ExprOrVar::into_expr)?;
                Ok(StmtKind::Expr(Box::new(Expr { span, kind: ExprKind::Tuple(exprs) })))
            } else if self.eat(&TokenKind::Eq) {
                // (,,a.b[c],) = call(...);
                // Can't mix exprs and vars.
                let rhs = self.parse_expr()?;
                self.expect_semi()?;
                let is_expr = tuple
                    .iter()
                    .flatten()
                    .map(|x| matches!(x, ExprOrVar::Expr(_)))
                    .next()
                    .unwrap_or(false);
                if is_expr {
                    let exprs = self.map_option_list(tuple, ExprOrVar::into_expr)?;
                    let lhs = Expr { span, kind: ExprKind::Tuple(exprs) };
                    let kind = ExprKind::Assign(Box::new(lhs), None, rhs);
                    Ok(StmtKind::Expr(Box::new(Expr { span, kind })))
                } else {
                    let lhs = self.map_option_list(tuple, ExprOrVar::into_var)?;
                    Ok(StmtKind::DeclMulti(lhs, rhs))
                }
            } else {
                self.unexpected()
            }
        } else {
            let e = self.parse_expr_or_var(false)?;
            if self.eat(&TokenKind::Semi) {
                match e {
                    ExprOrVar::Expr(expr) => Ok(StmtKind::Expr(expr)),
                    ExprOrVar::Var(var) => Ok(StmtKind::DeclSingle(var, None)),
                }
            } else if self.eat(&TokenKind::Eq) {
                let rhs = self.parse_expr()?;
                self.expect_semi()?;
                match e {
                    ExprOrVar::Expr(expr) => Ok(StmtKind::Expr(Box::new(Expr {
                        span: expr.span,
                        kind: ExprKind::Assign(expr, None, rhs),
                    }))),
                    ExprOrVar::Var(var) => Ok(StmtKind::DeclSingle(var, Some(rhs))),
                }
            } else {
                self.unexpected()
            }
        }
    }

    /// Parses a `delim`-delimited, comma-separated list of maybe-optional items.
    /// E.g. `(a, b) => [Some, Some]`, `(, a,, b,) => [None, Some, None, Some, None]`.
    pub(crate) fn parse_seq_optional_items<T>(
        &mut self,
        delim: Delimiter,
        mut f: impl FnMut(&mut Self) -> PResult<'a, T>,
    ) -> PResult<'a, Vec<Option<T>>> {
        self.expect(&TokenKind::OpenDelim(delim))?;
        let mut out = Vec::new();
        let close = TokenKind::CloseDelim(delim);
        while self.eat(&TokenKind::Comma) {
            out.push(None);
        }
        if !self.check(&close) {
            out.push(Some(f(self)?));
        }
        while !self.eat(&close) {
            self.expect(&TokenKind::Comma)?;
            if self.check(&TokenKind::Comma) || self.check(&close) {
                out.push(None);
            } else {
                out.push(Some(f(self)?));
            }
        }
        Ok(out)
    }

    fn parse_expr_or_var(&mut self, in_list: bool) -> PResult<'a, ExprOrVar> {
        let next_is_ok = |this: &mut Self| {
            this.look_ahead_with(1, |t| {
                t.is_ident()
                    || (in_list
                        && matches!(
                            t.kind,
                            TokenKind::Comma | TokenKind::CloseDelim(Delimiter::Parenthesis)
                        ))
            })
        };
        let is_var = |this: &mut Self| {
            this.token.is_keyword(kw::Mapping)
                || (this.token.is_keyword(kw::Function)
                    && this.look_ahead(1).is_open_delim(Delimiter::Parenthesis))
                || ((this.token.is_elementary_type()
                    || this.token.is_non_reserved_ident(this.in_yul))
                    && next_is_ok(this))
        };
        if self.token.is_ident() && is_var(self) {
            self.parse_variable_declaration(VarDeclMode::AllowStorage).map(ExprOrVar::Var)
        } else {
            self.parse_expr().map(ExprOrVar::Expr)
        }
    }

    fn map_option_list<T>(
        &mut self,
        exprs: Vec<Option<ExprOrVar>>,
        mut f: impl FnMut(ExprOrVar, &mut Self) -> PResult<'a, T>,
    ) -> PResult<'a, Vec<Option<T>>> {
        exprs
            .into_iter()
            .map(|x| match x {
                Some(x) => f(x, self).map(Some),
                None => Ok(None),
            })
            .collect()
    }

    /// Parses a path and a list of call arguments.
    fn parse_path_call(&mut self) -> PResult<'a, (Path, CallArgs)> {
        let path = self.parse_path()?;
        let params = self.parse_call_args()?;
        Ok((path, params))
    }
}

enum ExprOrVar {
    Expr(Box<Expr>),
    Var(VariableDeclaration),
}

impl ExprOrVar {
    fn into_expr<'a>(self, parser: &mut Parser<'a>) -> PResult<'a, Box<Expr>> {
        match self {
            Self::Expr(expr) => Ok(expr),
            Self::Var(var) => match var.name {
                Some(name) => Err(parser
                    .dcx()
                    .err("expected expression, found variable declaration")
                    .span(name.span)),
                // TODO: may need to convert `Ty::Array` to `Expr::Index`
                None => Ok(Box::new(Expr { span: var.ty.span, kind: ExprKind::Type(var.ty) })),
            },
        }
    }

    fn into_var<'a>(self, parser: &mut Parser<'a>) -> PResult<'a, VariableDeclaration> {
        match self {
            Self::Expr(expr) => Err(parser
                .dcx()
                .err("expected variable declaration, found expression")
                .span(expr.span)),
            Self::Var(var) => Ok(var),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ParseSess;
    use sulk_interface::source_map::FileName;

    #[test]
    fn optional_items_list() {
        fn check(tests: &[(&str, &[Option<&str>])]) {
            sulk_interface::enter(|| {
                let sess = ParseSess::with_test_emitter(false);
                for (i, &(s, results)) in tests.iter().enumerate() {
                    let name = i.to_string();
                    let mut parser =
                        Parser::from_source_code(&sess, FileName::Custom(name), s.into());

                    let list = parser
                        .parse_seq_optional_items(Delimiter::Parenthesis, Parser::parse_ident)
                        .map_err(|e| e.emit())
                        .unwrap();
                    let formatted: Vec<_> =
                        list.iter().map(|o| o.as_ref().map(|i| i.as_str())).collect();
                    assert_eq!(formatted.as_slice(), results, "{s:?}");
                }
            })
            .unwrap();
        }

        check(&[
            ("()", &[]),
            ("(a)", &[Some("a")]),
            // ("(,)", &[None, None]),
            ("(a,)", &[Some("a"), None]),
            ("(,b)", &[None, Some("b")]),
            ("(a,b)", &[Some("a"), Some("b")]),
            ("(a,b,)", &[Some("a"), Some("b"), None]),
            // ("(,,)", &[None, None, None]),
            ("(a,,)", &[Some("a"), None, None]),
            ("(a,b,)", &[Some("a"), Some("b"), None]),
            ("(a,b,c)", &[Some("a"), Some("b"), Some("c")]),
            ("(,b,c)", &[None, Some("b"), Some("c")]),
            ("(,,c)", &[None, None, Some("c")]),
            ("(a,,c)", &[Some("a"), None, Some("c")]),
        ]);
    }
}

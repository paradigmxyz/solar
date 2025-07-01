use super::item::VarFlags;
use crate::{parser::SeqSep, PResult, Parser};
use smallvec::SmallVec;
use solar_ast::{token::*, *};
use solar_data_structures::BumpExt;
use solar_interface::{kw, sym, Ident, Span, Spanned};

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses a statement.
    #[instrument(level = "debug", skip_all)]
    pub fn parse_stmt(&mut self) -> PResult<'sess, Stmt<'ast>> {
        let docs = self.parse_doc_comments();
        self.parse_spanned(Self::parse_stmt_kind).map(|(span, kind)| Stmt { docs, kind, span })
    }

    /// Parses a statement into a new allocation.
    pub fn parse_stmt_boxed(&mut self) -> PResult<'sess, Box<'ast, Stmt<'ast>>> {
        self.parse_stmt().map(|stmt| self.alloc(stmt))
    }

    /// Parses a statement kind.
    fn parse_stmt_kind(&mut self) -> PResult<'sess, StmtKind<'ast>> {
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
        } else if self.check(TokenKind::OpenDelim(Delimiter::Brace)) {
            semi = false;
            self.parse_block().map(StmtKind::Block)
        } else if self.eat_keyword(kw::Continue) {
            Ok(StmtKind::Continue)
        } else if self.eat_keyword(kw::Break) {
            Ok(StmtKind::Break)
        } else if self.eat_keyword(kw::Return) {
            let expr = if self.check(TokenKind::Semi) { None } else { Some(self.parse_expr()?) };
            Ok(StmtKind::Return(expr))
        } else if self.eat_keyword(kw::Throw) {
            let msg = "`throw` statements have been removed; use `revert`, `require`, or `assert` instead";
            Err(self.dcx().err(msg).span(self.prev_token.span))
        } else if self.eat_keyword(kw::Try) {
            semi = false;
            self.parse_stmt_try().map(|stmt| StmtKind::Try(self.alloc(stmt)))
        } else if self.eat_keyword(kw::Assembly) {
            semi = false;
            self.parse_stmt_assembly().map(StmtKind::Assembly)
        } else if self.eat_keyword(kw::Emit) {
            self.parse_path_call().map(|(path, params)| StmtKind::Emit(path, params))
        } else if self.check_keyword(kw::Revert) && self.look_ahead(1).is_ident() {
            self.bump(); // `revert`
            self.parse_path_call().map(|(path, params)| StmtKind::Revert(path, params))
        } else if self.check_keyword(sym::underscore) && self.look_ahead(1).kind == TokenKind::Semi
        {
            self.bump(); // `_`
            Ok(StmtKind::Placeholder)
        } else {
            self.parse_simple_stmt_kind()
        };
        if semi && kind.is_ok() {
            self.expect_semi()?;
        }
        kind
    }

    /// Parses a block of statements.
    pub(super) fn parse_block(&mut self) -> PResult<'sess, Block<'ast>> {
        let lo = self.token.span;
        self.parse_delim_seq(Delimiter::Brace, SeqSep::none(), true, Self::parse_stmt)
            .map(|stmts| Block { span: lo.to(self.prev_token.span), stmts })
    }

    /// Parses an if statement.
    fn parse_stmt_if(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        self.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let expr_lo = self.prev_token.span;
        let expr = {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            Spanned { span: expr_lo.to(self.prev_token.span), data: expr }
        };
        let cond_lo = self.prev_token.span;
        let cond_stmt = {
            let true_stmt = self.parse_stmt()?;
            Spanned { span: cond_lo.to(self.prev_token.span), data: self.alloc(true_stmt) }
        };
        let else_stmt = if self.eat_keyword(kw::Else) {
            let else_lo = self.prev_token.span;
            Some(Spanned { span: else_lo.to(self.prev_token.span), data: self.parse_stmt_boxed()? })
        } else {
            None
        };
        Ok(StmtKind::If(expr, cond_stmt, else_stmt))
    }

    /// Parses a while statement.
    fn parse_stmt_while(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        self.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let expr_lo = self.prev_token.span;
        let expr = {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            Spanned { span: expr_lo.to(self.prev_token.span), data: expr }
        };
        let stmt_lo = self.prev_token.span;
        let stmt = {
            let stmt = self.parse_stmt()?;
            Spanned { span: stmt_lo.to(self.prev_token.span), data: self.alloc(stmt) }
        };
        Ok(StmtKind::While(expr, stmt))
    }

    /// Parses a do-while statement.
    fn parse_stmt_do_while(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let stmt_lo = self.prev_token.span;
        let stmt = {
            let stmt = self.parse_stmt()?;
            Spanned { span: stmt_lo.to(self.prev_token.span), data: self.alloc(stmt) }
        };
        self.expect_keyword(kw::While)?;
        self.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let expr_lo = self.prev_token.span;
        let expr = {
            let expr = self.parse_expr()?;
            self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            Spanned { span: expr_lo.to(self.prev_token.span), data: expr }
        };
        Ok(StmtKind::DoWhile(stmt, expr))
    }

    /// Parses a for statement.
    fn parse_stmt_for(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        self.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;

        let init = if self.check(TokenKind::Semi) { None } else { Some(self.parse_simple_stmt()?) };
        self.expect(TokenKind::Semi)?;

        let cond = if self.check(TokenKind::Semi) { None } else { Some(self.parse_expr()?) };
        self.expect_semi()?;

        let next = if self.check_noexpect(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            None
        } else {
            Some(self.parse_expr()?)
        };
        self.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        let body = self.parse_stmt_boxed()?;
        Ok(StmtKind::For { init: init.map(|init| self.alloc(init)), cond, next, body })
    }

    /// Parses a try statement.
    fn parse_stmt_try(&mut self) -> PResult<'sess, StmtTry<'ast>> {
        let expr = self.parse_expr()?;
        let mut clauses = SmallVec::<[_; 4]>::new();

        let mut lo = self.token.span;
        let returns = if self.eat_keyword(kw::Returns) {
            self.parse_parameter_list(false, VarFlags::FUNCTION)?
        } else {
            Default::default()
        };
        let block = self.parse_block()?;
        let span = lo.to(self.prev_token.span);
        clauses.push(TryCatchClause { name: None, args: returns, block, span });

        lo = self.token.span;
        self.expect_keyword(kw::Catch)?;
        loop {
            let name = self.parse_ident_opt()?;
            let args = if self.check(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                self.parse_parameter_list(false, VarFlags::FUNCTION)?
            } else {
                Default::default()
            };
            let block = self.parse_block()?;
            let span = lo.to(self.prev_token.span);
            clauses.push(TryCatchClause { name, args, block, span });
            lo = self.token.span;
            if !self.eat_keyword(kw::Catch) {
                break;
            }
        }

        let clauses = self.alloc_smallvec(clauses);
        Ok(StmtTry { expr, clauses })
    }

    /// Parses an assembly block.
    fn parse_stmt_assembly(&mut self) -> PResult<'sess, StmtAssembly<'ast>> {
        let dialect = self.parse_str_lit_opt();
        let flags = if self.check(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            self.parse_paren_comma_seq(false, Self::parse_str_lit)?
        } else {
            Default::default()
        };
        let block = self.parse_yul_block()?;
        Ok(StmtAssembly { dialect, flags, block })
    }

    /// Parses a simple statement. These are just variable declarations and expressions.
    fn parse_simple_stmt(&mut self) -> PResult<'sess, Stmt<'ast>> {
        let docs = self.parse_doc_comments();
        self.parse_spanned(Self::parse_simple_stmt_kind).map(|(span, kind)| Stmt {
            docs,
            kind,
            span,
        })
    }

    /// Parses a simple statement kind. These are just variable declarations and expressions.
    ///
    /// Also used in the for loop initializer. Does not parse the trailing semicolon.
    fn parse_simple_stmt_kind(&mut self) -> PResult<'sess, StmtKind<'ast>> {
        let lo = self.token.span;
        if self.eat(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            let mut empty_components = 0usize;
            while self.eat(TokenKind::Comma) {
                empty_components += 1;
            }

            let (statement_type, iap) = self.try_parse_iap()?;
            match statement_type {
                LookAheadInfo::VariableDeclaration => {
                    let mut variables = smallvec_repeat_none(empty_components);
                    let ty = iap.into_ty(self);
                    variables
                        .push(Some(self.parse_variable_definition_with(VarFlags::FUNCTION, ty)?));
                    self.parse_optional_items_seq_required(
                        Delimiter::Parenthesis,
                        &mut variables,
                        |this| this.parse_variable_definition(VarFlags::FUNCTION),
                    )?;
                    self.expect(TokenKind::Eq)?;
                    let expr = self.parse_expr()?;
                    Ok(StmtKind::DeclMulti(self.alloc_smallvec(variables), expr))
                }
                LookAheadInfo::Expression => {
                    let mut components = smallvec_repeat_none(empty_components);
                    let expr = iap.into_expr(self);
                    components.push(Some(self.parse_expr_with(expr)?));
                    self.parse_optional_items_seq_required(
                        Delimiter::Parenthesis,
                        &mut components,
                        Self::parse_expr,
                    )?;
                    let partially_parsed = Expr {
                        span: lo.to(self.prev_token.span),
                        kind: ExprKind::Tuple(self.alloc_smallvec(components)),
                    };
                    self.parse_expr_with(Some(self.alloc(partially_parsed))).map(StmtKind::Expr)
                }
                LookAheadInfo::IndexAccessStructure => unreachable!(),
            }
        } else {
            let (statement_type, iap) = self.try_parse_iap()?;
            match statement_type {
                LookAheadInfo::VariableDeclaration => {
                    let ty = iap.into_ty(self);
                    self.parse_variable_definition_with(VarFlags::VAR, ty)
                        .map(|var| StmtKind::DeclSingle(self.alloc(var)))
                }
                LookAheadInfo::Expression => {
                    let expr = iap.into_expr(self);
                    self.parse_expr_with(expr).map(StmtKind::Expr)
                }
                LookAheadInfo::IndexAccessStructure => unreachable!(),
            }
        }
    }

    /// Parses a `delim`-delimited, comma-separated list of maybe-optional items.
    /// E.g. `(a, b) => [Some, Some]`, `(, a,, b,) => [None, Some, None, Some, None]`.
    pub(super) fn parse_optional_items_seq<T>(
        &mut self,
        delim: Delimiter,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, Box<'ast, [Option<T>]>> {
        self.expect(TokenKind::OpenDelim(delim))?;
        let mut out = SmallVec::<[_; 8]>::new();
        while self.eat(TokenKind::Comma) {
            out.push(None);
        }
        if !self.check(TokenKind::CloseDelim(delim)) {
            out.push(Some(f(self)?));
        }
        self.parse_optional_items_seq_required(delim, &mut out, f)
            .map(|()| self.alloc_smallvec(out))
    }

    fn parse_optional_items_seq_required<T>(
        &mut self,
        delim: Delimiter,
        out: &mut SmallVec<[Option<T>; 8]>,
        mut f: impl FnMut(&mut Self) -> PResult<'sess, T>,
    ) -> PResult<'sess, ()> {
        let close = TokenKind::CloseDelim(delim);
        while !self.eat(close) {
            self.expect(TokenKind::Comma)?;
            if self.check(TokenKind::Comma) || self.check(close) {
                out.push(None);
            } else {
                out.push(Some(f(self)?));
            }
        }
        Ok(())
    }

    /// Parses a path and a list of call arguments.
    fn parse_path_call(&mut self) -> PResult<'sess, (AstPath<'ast>, CallArgs<'ast>)> {
        let path = self.parse_path()?;
        let params = self.parse_call_args()?;
        Ok((path, params))
    }

    /// Never returns `LookAheadInfo::IndexAccessStructure`.
    fn try_parse_iap(&mut self) -> PResult<'sess, (LookAheadInfo, IndexAccessedPath<'ast>)> {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L1961
        if let ty @ (LookAheadInfo::VariableDeclaration | LookAheadInfo::Expression) =
            self.peek_statement_type()
        {
            return Ok((ty, IndexAccessedPath::default()));
        }

        let iap = self.parse_iap()?;
        let ty = if self.token.is_non_reserved_ident(self.in_yul)
            || self.token.is_location_specifier()
        {
            // `a.b memory`, `a[b] c`
            LookAheadInfo::VariableDeclaration
        } else {
            LookAheadInfo::Expression
        };
        Ok((ty, iap))
    }

    fn peek_statement_type(&mut self) -> LookAheadInfo {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L2528
        if self.token.is_keyword_any(&[kw::Mapping, kw::Function]) {
            return LookAheadInfo::VariableDeclaration;
        }

        if self.check_nr_ident() || self.check_elementary_type() {
            let next = self.look_ahead(1);
            if self.token.is_elementary_type() && next.is_ident_where(|id| id.name == kw::Payable) {
                return LookAheadInfo::VariableDeclaration;
            }
            if next.is_non_reserved_ident(self.in_yul)
                || next.is_location_specifier()
                // These aren't valid but we include them for a better error message.
                || next.is_mutability_specifier()
                || next.is_visibility_specifier()
            {
                return LookAheadInfo::VariableDeclaration;
            }
            if matches!(next.kind, TokenKind::OpenDelim(Delimiter::Bracket) | TokenKind::Dot) {
                return LookAheadInfo::IndexAccessStructure;
            }
        }
        LookAheadInfo::Expression
    }

    fn parse_iap(&mut self) -> PResult<'sess, IndexAccessedPath<'ast>> {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L2559
        let mut path = SmallVec::<[_; 4]>::new();
        if self.check_nr_ident() {
            path.push(IapKind::Member(self.parse_ident()?));
            while self.eat(TokenKind::Dot) {
                let id = self.ident_or_err(true)?;
                if id.name != kw::Address && id.is_reserved(self.in_yul) {
                    self.expected_ident_found_err().emit();
                }
                self.bump(); // `id`
                path.push(IapKind::Member(id));
            }
        } else if self.check_elementary_type() {
            let (span, kind) = self.parse_spanned(Self::parse_elementary_type)?;
            path.push(IapKind::MemberTy(span, kind));
        } else {
            return self.unexpected();
        }
        let n_idents = path.len();

        while self.check(TokenKind::OpenDelim(Delimiter::Bracket)) {
            let (span, kind) = self.parse_spanned(Self::parse_expr_index_kind)?;
            path.push(IapKind::Index(span, kind));
        }

        Ok(IndexAccessedPath { path, n_idents })
    }
}

#[derive(Debug)]
enum LookAheadInfo {
    /// `a.b`, `a[b]`
    IndexAccessStructure,
    VariableDeclaration,
    Expression,
}

#[derive(Debug)]
enum IapKind<'ast> {
    /// `[...]`
    Index(Span, IndexKind<'ast>),
    /// `<ident>` or `.<ident>`
    Member(Ident),
    /// `<ty>`
    MemberTy(Span, ElementaryType),
}

#[derive(Debug, Default)]
struct IndexAccessedPath<'ast> {
    path: SmallVec<[IapKind<'ast>; 4]>,
    /// The number of elements in `path` that are `IapKind::Member[Ty]` at the start.
    n_idents: usize,
}

impl<'ast> IndexAccessedPath<'ast> {
    fn into_ty(self, parser: &mut Parser<'_, 'ast>) -> Option<Type<'ast>> {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L2617
        let mut path = self.path.into_iter();
        let first = path.next()?;

        let mut ty = if let IapKind::MemberTy(span, kind) = first {
            debug_assert_eq!(self.n_idents, 1);
            Type { span, kind: TypeKind::Elementary(kind) }
        } else {
            debug_assert!(self.n_idents >= 1);
            let first = std::iter::once(&first);
            let path = first
                .chain(path.as_slice())
                .map(|x| match x {
                    IapKind::Member(id) => *id,
                    kind => unreachable!("{kind:?}"),
                })
                .take(self.n_idents);
            let path = PathSlice::from_mut_slice(parser.arena.alloc_from_iter(path));
            Type { span: path.span(), kind: TypeKind::Custom(path) }
        };

        for index in path.skip(self.n_idents - 1) {
            let IapKind::Index(span, kind) = index else { panic!("parsed too much") };
            let size = match kind {
                IndexKind::Index(expr) => expr,
                IndexKind::Range(l, r) => {
                    let msg = "expected array length, got range expression";
                    parser.dcx().err(msg).span(span).emit();
                    l.or(r)
                }
            };
            let span = ty.span.to(span);
            ty =
                Type { span, kind: TypeKind::Array(parser.alloc(TypeArray { element: ty, size })) };
        }

        Some(ty)
    }

    fn into_expr(self, parser: &mut Parser<'_, 'ast>) -> Option<Box<'ast, Expr<'ast>>> {
        // https://github.com/ethereum/solidity/blob/194b114664c7daebc2ff68af3c573272f5d28913/libsolidity/parsing/Parser.cpp#L2658
        let mut path = self.path.into_iter();

        let mut expr = parser.alloc(match path.next()? {
            IapKind::Member(ident) => Expr::from_ident(ident),
            IapKind::MemberTy(span, kind) => {
                Expr { span, kind: ExprKind::Type(Type { span, kind: TypeKind::Elementary(kind) }) }
            }
            IapKind::Index(..) => panic!("should not happen"),
        });
        for index in path {
            expr = parser.alloc(match index {
                IapKind::Member(ident) => {
                    Expr { span: expr.span.to(ident.span), kind: ExprKind::Member(expr, ident) }
                }
                IapKind::MemberTy(..) => panic!("should not happen"),
                IapKind::Index(span, kind) => {
                    Expr { span: expr.span.to(span), kind: ExprKind::Index(expr, kind) }
                }
            });
        }
        Some(expr)
    }
}

/// `T: !Clone`
fn smallvec_repeat_none<T>(n: usize) -> SmallVec<[Option<T>; 8]> {
    let mut v = SmallVec::with_capacity(n);
    v.extend(std::iter::repeat_with(|| None).take(n));
    v
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{source_map::FileName, Result, Session};

    #[test]
    fn optional_items_seq() {
        fn check(tests: &[(&str, &[Option<&str>])]) {
            solar_interface::enter(|| -> Result {
                let sess = Session::builder().with_test_emitter().build();
                for (i, &(s, results)) in tests.iter().enumerate() {
                    let name = i.to_string();
                    let arena = Arena::new();
                    let mut parser =
                        Parser::from_source_code(&sess, &arena, FileName::Custom(name), s)?;

                    let list = parser
                        .parse_optional_items_seq(Delimiter::Parenthesis, Parser::parse_ident)
                        .map_err(|e| e.emit())
                        .unwrap_or_else(|_| panic!("src: {s:?}"));
                    sess.dcx.has_errors().unwrap();
                    let formatted: Vec<_> =
                        list.iter().map(|o| o.as_ref().map(|i| i.as_str())).collect();
                    assert_eq!(formatted.as_slice(), results, "{s:?}");
                }
                Ok(())
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

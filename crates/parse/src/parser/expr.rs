use crate::{PResult, Parser};
use solar_ast::{ast::*, token::*};
use solar_interface::kw;

impl<'sess, 'ast> Parser<'sess, 'ast> {
    /// Parses an expression.
    #[inline]
    pub fn parse_expr(&mut self) -> PResult<'sess, Box<'ast, Expr<'ast>>> {
        self.parse_expr_with(None)
    }

    #[instrument(name = "parse_expr", level = "debug", skip_all)]
    pub(super) fn parse_expr_with(
        &mut self,
        with: Option<Box<'ast, Expr<'ast>>>,
    ) -> PResult<'sess, Box<'ast, Expr<'ast>>> {
        self.ignore_doc_comments();
        let expr = self.parse_binary_expr(4, with)?;
        if self.eat(&TokenKind::Question) {
            let then = self.parse_expr()?;
            self.expect(&TokenKind::Colon)?;
            let else_ = self.parse_expr()?;
            let span = expr.span.to(self.prev_token.span);
            Ok(self.alloc(Expr { span, kind: ExprKind::Ternary(expr, then, else_) }))
        } else {
            let kind = if let Some(binop_eq) = self.token.as_binop_eq() {
                Some(binop_eq)
            } else if self.token.kind == TokenKind::Eq {
                None
            } else {
                return Ok(expr);
            };
            self.bump(); // binop token
            let rhs = self.parse_expr()?;
            let span = expr.span.to(self.prev_token.span);
            Ok(self.alloc(Expr { span, kind: ExprKind::Assign(expr, kind, rhs) }))
        }
    }

    /// Parses a binary expression.
    fn parse_binary_expr(
        &mut self,
        min_precedence: usize,
        with: Option<Box<'ast, Expr<'ast>>>,
    ) -> PResult<'sess, Box<'ast, Expr<'ast>>> {
        let mut expr = self.parse_unary_expr(with)?;
        let mut precedence = token_precedence(&self.token);
        while precedence >= min_precedence {
            while token_precedence(&self.token) == precedence {
                // Parse a**b**c as a**(b**c)
                let next_precedence = if self.token.kind == TokenKind::BinOp(BinOpToken::Star) {
                    precedence + 1
                } else {
                    precedence
                };

                let token = self.token.clone();
                self.bump(); // binop token

                let rhs = self.parse_binary_expr(next_precedence, None)?;

                let span = expr.span.to(self.prev_token.span);

                let kind = if let Some(binop) = token.as_binop() {
                    ExprKind::Binary(expr, binop, rhs)
                } else if let Some(binop_eq) = token.as_binop_eq() {
                    ExprKind::Assign(expr, Some(binop_eq), rhs)
                } else if token.kind == TokenKind::Eq {
                    ExprKind::Assign(expr, None, rhs)
                } else {
                    let msg = format!("unknown binop token: {token:?}");
                    self.dcx().bug(msg).span(span).emit();
                };
                expr = self.alloc(Expr { span, kind });
            }
            precedence -= 1;
        }
        Ok(expr)
    }

    /// Parses a unary expression.
    fn parse_unary_expr(
        &mut self,
        with: Option<Box<'ast, Expr<'ast>>>,
    ) -> PResult<'sess, Box<'ast, Expr<'ast>>> {
        if with.is_none() && self.eat(&TokenKind::BinOp(BinOpToken::Plus)) {
            self.dcx().err("unary plus is not supported").span(self.prev_token.span).emit();
        }

        let lo = with.as_ref().map(|e| e.span).unwrap_or(self.token.span);
        let parse_lhs = |this: &mut Self, with| {
            this.parse_lhs_expr(with, lo).map(|expr| {
                if let Some(unop) = this.token.as_unop(true) {
                    this.bump(); // unop
                    let span = lo.to(this.prev_token.span);
                    this.alloc(Expr { span, kind: ExprKind::Unary(unop, expr) })
                } else {
                    expr
                }
            })
        };
        if let Some(with) = with {
            parse_lhs(self, Some(with))
        } else if self.eat_keyword(kw::Delete) {
            self.parse_unary_expr(None).map(|expr| {
                let span = lo.to(self.prev_token.span);
                self.alloc(Expr { span, kind: ExprKind::Delete(expr) })
            })
        } else if let Some(unop) = self.token.as_unop(false) {
            self.bump(); // unop
            self.parse_unary_expr(None).map(|expr| {
                let span = lo.to(self.prev_token.span);
                self.alloc(Expr { span, kind: ExprKind::Unary(unop, expr) })
            })
        } else {
            parse_lhs(self, None)
        }
    }

    /// Parses a primary left-hand-side expression.
    fn parse_lhs_expr(
        &mut self,
        with: Option<Box<'ast, Expr<'ast>>>,
        lo: Span,
    ) -> PResult<'sess, Box<'ast, Expr<'ast>>> {
        let mut expr = if let Some(with) = with {
            Ok(with)
        } else if self.eat_keyword(kw::New) {
            self.parse_type().map(|ty| {
                let span = lo.to(self.prev_token.span);
                self.alloc(Expr { span, kind: ExprKind::New(ty) })
            })
        } else if self.eat_keyword(kw::Payable) {
            self.parse_call_args().map(|args| {
                let span = lo.to(self.prev_token.span);
                self.alloc(Expr { span, kind: ExprKind::Payable(args) })
            })
        } else {
            self.parse_primary_expr()
        }?;
        loop {
            let kind = if self.eat(&TokenKind::Dot) {
                // expr.member
                let member = self.parse_ident_any()?;
                ExprKind::Member(expr, member)
            } else if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                // expr(args)
                let args = self.parse_call_args()?;
                ExprKind::Call(expr, args)
            } else if self.check(&TokenKind::OpenDelim(Delimiter::Bracket)) {
                let kind = self.parse_expr_index_kind()?;
                ExprKind::Index(expr, kind)
            } else if self.check(&TokenKind::OpenDelim(Delimiter::Brace)) {
                // This may be `try` statement block.
                if !self.look_ahead(1).is_ident() || self.look_ahead(2).kind != TokenKind::Colon {
                    break;
                }

                // expr{args}
                let args = self.parse_named_args()?;
                ExprKind::CallOptions(expr, args)
            } else {
                break;
            };
            let span = lo.to(self.prev_token.span);
            expr = self.alloc(Expr { span, kind });
        }
        Ok(expr)
    }

    /// Parses a primary expression.
    fn parse_primary_expr(&mut self) -> PResult<'sess, Box<'ast, Expr<'ast>>> {
        let lo = self.token.span;
        let kind = if self.check_lit() {
            let (lit, sub) = self.parse_lit_with_subdenomination()?;
            ExprKind::Lit(lit, sub)
        } else if self.eat_keyword(kw::Type) {
            self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;
            let ty = self.parse_type()?;
            self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            ExprKind::TypeCall(ty)
        } else if self.check_elementary_type() {
            let mut ty = self.parse_type()?;
            if let TypeKind::Elementary(ElementaryType::Address(payable)) = &mut ty.kind {
                if *payable {
                    let msg = "`address payable` cannot be used in an expression";
                    self.dcx().err(msg).span(ty.span).emit();
                    *payable = false;
                }
            }
            ExprKind::Type(ty)
        } else if self.check_nr_ident() {
            let ident = self.parse_ident()?;
            ExprKind::Ident(ident)
        } else if self.check(&TokenKind::OpenDelim(Delimiter::Parenthesis))
            || self.check(&TokenKind::OpenDelim(Delimiter::Bracket))
        {
            // Array or tuple expression.
            let TokenKind::OpenDelim(close_delim) = self.token.kind else { unreachable!() };
            let is_array = close_delim == Delimiter::Bracket;
            let list = self.parse_optional_items_seq(close_delim, |this| this.parse_expr())?;
            if is_array {
                if !list.iter().all(Option::is_some) {
                    let msg = "array expression components cannot be empty";
                    let span = lo.to(self.prev_token.span);
                    return Err(self.dcx().err(msg).span(span));
                }
                // SAFETY: All elements are checked to be `Some` above.
                ExprKind::Array(unsafe { option_boxes_unwrap_unchecked(list) })
            } else {
                ExprKind::Tuple(list)
            }
        } else {
            return self.unexpected();
        };
        let span = lo.to(self.prev_token.span);
        Ok(self.alloc(Expr { span, kind }))
    }

    /// Parses a list of function call arguments.
    #[track_caller]
    pub(super) fn parse_call_args(&mut self) -> PResult<'sess, CallArgs<'ast>> {
        if self.look_ahead(1).kind == TokenKind::OpenDelim(Delimiter::Brace) {
            self.expect(&TokenKind::OpenDelim(Delimiter::Parenthesis))?;
            let args = self.parse_named_args().map(CallArgs::Named)?;
            self.expect(&TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            Ok(args)
        } else {
            self.parse_unnamed_args().map(CallArgs::Unnamed)
        }
    }

    /// Parses a `[]` indexing expression.
    pub(super) fn parse_expr_index_kind(&mut self) -> PResult<'sess, IndexKind<'ast>> {
        self.expect(&TokenKind::OpenDelim(Delimiter::Bracket))?;
        let kind = if self.check(&TokenKind::CloseDelim(Delimiter::Bracket)) {
            // expr[]
            IndexKind::Index(None)
        } else {
            let start = if self.check(&TokenKind::Colon) { None } else { Some(self.parse_expr()?) };
            if self.eat_noexpect(&TokenKind::Colon) {
                // expr[start?:end?]
                let end = if self.check(&TokenKind::CloseDelim(Delimiter::Bracket)) {
                    None
                } else {
                    Some(self.parse_expr()?)
                };
                IndexKind::Range(start, end)
            } else {
                // expr[start?]
                IndexKind::Index(start)
            }
        };
        self.expect(&TokenKind::CloseDelim(Delimiter::Bracket))?;
        Ok(kind)
    }

    /// Parses a list of named arguments: `{a: b, c: d, ...}`
    #[track_caller]
    fn parse_named_args(&mut self) -> PResult<'sess, NamedArgList<'ast>> {
        self.parse_delim_comma_seq(Delimiter::Brace, false, Self::parse_named_arg)
    }

    /// Parses a single named argument: `a: b`.
    #[track_caller]
    fn parse_named_arg(&mut self) -> PResult<'sess, NamedArg<'ast>> {
        let name = self.parse_ident()?;
        self.expect(&TokenKind::Colon)?;
        let value = self.parse_expr()?;
        Ok(NamedArg { name, value })
    }

    /// Parses a list of expressions: `(a, b, c, ...)`.
    #[allow(clippy::vec_box)]
    #[track_caller]
    fn parse_unnamed_args(&mut self) -> PResult<'sess, Box<'ast, [Box<'ast, Expr<'ast>>]>> {
        self.parse_paren_comma_seq(true, Self::parse_expr)
    }
}

fn token_precedence(t: &Token) -> usize {
    use BinOpToken::*;
    use TokenKind::*;
    match t.kind {
        Question => 3,
        BinOpEq(_) => 2,
        Comma => 1,
        OrOr => 4,
        AndAnd => 5,
        BinOp(Or) => 8,
        BinOp(Caret) => 9,
        BinOp(And) => 10,
        BinOp(Shl) => 11,
        BinOp(Sar) => 11,
        BinOp(Shr) => 11,
        BinOp(Plus) => 12,
        BinOp(Minus) => 12,
        BinOp(Star) => 13,
        BinOp(Slash) => 13,
        BinOp(Percent) => 13,
        StarStar => 4,
        EqEq => 6,
        Ne => 6,
        Lt => 7,
        Gt => 7,
        Le => 7,
        Ge => 7,
        Walrus => 2,
        _ => 0,
    }
}

/// Converts a list of `Option<Box<'ast, T>>` into a list of `Box<'ast, T>`.
///
/// This only works because `Option<Box<'ast, T>>` is guaranteed to be a valid `Box<'ast, T>` when
/// `Some` when `T: Sized`.
///
/// # Safety
///
/// All elements of the list must be `Some`.
#[inline]
unsafe fn option_boxes_unwrap_unchecked<'a, 'b, T>(
    list: Box<'a, [Option<Box<'b, T>>]>,
) -> Box<'a, [Box<'b, T>]> {
    debug_assert!(list.iter().all(Option::is_some));
    // SAFETY: Caller must ensure that all elements are `Some`.
    unsafe { std::mem::transmute(list) }
}

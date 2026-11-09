//! Namespaced expression expansions, validated before entering ordinary name resolution.
//! Arguments are consumed in source order exactly once; this initial adapter rejects recipes
//! that would duplicate, discard or reorder argument evaluation.
//! Requests borrow token slices from one lazy cache per parser. Invocation ranges come from
//! successful parsing, so failed invocations need no delimiter pre-scan or token-text copies.

use crate::{PResult, Parser};
use alloy_primitives::U256;
use solar_ast::{
    self as ast,
    BinOpKind::*,
    token::{Delimiter, Token, TokenKind},
};
use solar_interface::{Ident, Session, Span, Symbol};
use solar_pass_plugin::{
    Expansion, ExpansionExpr, ExpansionRequest, ExpansionStmt, ExtensionKind, FrontendToken,
};
use std::sync::OnceLock;

pub(super) struct SourceTokens {
    tokens: Vec<Token>,
    frontend: OnceLock<Vec<FrontendToken>>,
}

impl SourceTokens {
    fn new(token: Token, remaining: &[Token]) -> Self {
        Self {
            tokens: std::iter::once(token)
                .chain(remaining.iter().copied())
                .take_while(|token| token.kind != TokenKind::Eof)
                .collect(),
            frontend: OnceLock::new(),
        }
    }

    fn view(&self, sess: &Session, span: Span) -> &[FrontendToken] {
        let tokens = self.frontend.get_or_init(|| {
            let map = sess.source_map();
            let file = map.lookup_source_file(span.lo());
            self.tokens
                .iter()
                .map(|token| FrontendToken {
                    text: map.span_to_snippet(token.span).unwrap_or_default(),
                    start: (token.span.lo() - file.start_pos).0,
                    end: (token.span.hi() - file.start_pos).0,
                })
                .collect()
        });
        let start = self.tokens.partition_point(|token| token.span.lo() < span.lo());
        let end = self.tokens.partition_point(|token| token.span.hi() <= span.hi());
        &tokens[start..end]
    }
}

impl<'sess, 'ast, 'cb> Parser<'sess, 'ast, 'cb> {
    /// Installs explicitly loaded frontend extensions for this parser.
    pub fn set_plugin_expansion(
        &mut self,
        callback: impl Fn(&str, &ExpansionRequest<'_>) -> Result<Expansion, String> + 'cb,
    ) {
        self.plugin_tokens = None;
        self.plugin_expansion = Some(std::boxed::Box::new(callback));
    }

    pub(super) fn parse_plugin_expr(&mut self) -> PResult<'sess, ast::Box<'ast, ast::Expr<'ast>>> {
        self.prepare_plugin_tokens();
        let start = self.token.span;
        self.expect(TokenKind::At)?;
        let namespace = self.parse_ident()?;
        self.expect(TokenKind::Dot)?;
        let name = self.parse_ident()?;
        let args = self.parse_paren_comma_seq(true, Self::parse_expr)?;
        let span = start.to(self.prev_token.span);
        self.finish_plugin_expr(namespace, name, args, span)
    }

    fn finish_plugin_expr(
        &self,
        namespace: Ident,
        name: Ident,
        args: ast::BoxSlice<'ast, ast::Box<'ast, ast::Expr<'ast>>>,
        span: Span,
    ) -> PResult<'sess, ast::Box<'ast, ast::Expr<'ast>>> {
        let callback = self
            .plugin_expansion
            .as_ref()
            .ok_or_else(|| self.dcx().err_at(span, "no frontend plugins are loaded"))?;
        let request = ExpansionRequest {
            name: name.name.as_str().to_owned(),
            kind: ExtensionKind::Expression,
            tokens: self.plugin_source_tokens(span),
        };
        let expansion = callback(namespace.name.as_str(), &request)
            .map_err(|error| self.dcx().err_at(span, error))?;
        if let Expansion::Builtin(signature) = &expansion {
            if signature.effect != solar_pass_plugin::BuiltinEffect::Pure {
                return Err(self
                    .dcx()
                    .err_at(span, "plugin builtins require an explicit pure effect"));
            }
            if signature.parameters.len() != args.len() {
                return Err(self.dcx().err_at(span, "wrong number of plugin builtin arguments"));
            }
            return Ok(self.alloc(ast::Expr {
                span,
                kind: ast::ExprKind::PluginBuiltin(namespace, name, args),
            }));
        }
        let Expansion::Expression(expr) = expansion else {
            return Err(self.dcx().err_at(span, "plugin returned a statement in an expression"));
        };
        let mut args = args
            .iter_mut()
            .map(|arg| {
                let placeholder = self.alloc(ast::Expr {
                    span,
                    kind: ast::ExprKind::Tuple(self.alloc_vec(Vec::new())),
                });
                std::mem::replace(arg, placeholder)
            })
            .enumerate();
        let expr = self.expand_plugin_expr(expr, &mut args, span, 0)?;
        if args.next().is_some() {
            return Err(self.dcx().err_at(span, "plugin expansion must use every argument once"));
        }
        Ok(expr)
    }

    pub(super) fn parse_plugin_stmt(&mut self) -> PResult<'sess, ast::StmtKind<'ast>> {
        self.prepare_plugin_tokens();
        let start = self.token.span;
        self.expect(TokenKind::At)?;
        let namespace = self.parse_ident()?;
        self.expect(TokenKind::Dot)?;
        let name = self.parse_ident()?;
        let args = self.parse_paren_comma_seq(true, Self::parse_expr)?;
        if !self.check(TokenKind::OpenDelim(Delimiter::Brace)) {
            let span = start.to(self.prev_token.span);
            let expr = self.finish_plugin_expr(namespace, name, args, span)?;
            self.expect_semi()?;
            return Ok(ast::StmtKind::Expr(expr));
        }
        let body = self.parse_block()?;
        let span = start.to(self.prev_token.span);
        let callback = self
            .plugin_expansion
            .as_ref()
            .ok_or_else(|| self.dcx().err_at(span, "no frontend plugins are loaded"))?;
        let request = ExpansionRequest {
            name: name.name.as_str().to_owned(),
            kind: ExtensionKind::Statement,
            tokens: self.plugin_source_tokens(span),
        };
        let expansion = callback(namespace.name.as_str(), &request)
            .map_err(|error| self.dcx().err_at(span, error))?;
        let Expansion::Statement(expansion) = expansion else {
            return Err(self.dcx().err_at(span, "plugin returned an expression in a statement"));
        };
        let mut prelude = Vec::new();
        let mut bound_args = Vec::new();
        for arg in args.iter_mut() {
            let ident = self.plugin_ident(span);
            let initializer = std::mem::replace(arg, self.plugin_uint(U256::ZERO, span));
            prelude.push(self.plugin_decl(ident, initializer, span));
            bound_args.push(self.alloc(ast::Expr::from_ident(ident)));
        }
        let mut args = bound_args.into_iter().enumerate();
        let mut body = Some(body);
        let result = self.expand_plugin_stmt(expansion, &mut args, &mut body, span, 0)?;
        if args.next().is_some() || body.is_some() {
            return Err(self
                .dcx()
                .err_at(span, "plugin expansion must use every argument and its body once"));
        }
        prelude.push(ast::Stmt { span, docs: Default::default(), kind: result });
        Ok(ast::StmtKind::Block(ast::Block { span, stmts: self.alloc_vec(prelude) }))
    }

    fn plugin_ident(&mut self, span: Span) -> Ident {
        let ident = Ident::new(
            Symbol::intern(&format!("#plugin.{}.{}", span.lo().0, self.plugin_temporary)),
            span,
        );
        self.plugin_temporary += 1;
        ident
    }

    fn plugin_decl(
        &self,
        ident: Ident,
        initializer: ast::Box<'ast, ast::Expr<'ast>>,
        span: Span,
    ) -> ast::Stmt<'ast> {
        let variable = self.alloc(ast::VariableDefinition {
            span,
            ty: ast::Type {
                span,
                kind: ast::TypeKind::Elementary(ast::ElementaryType::UInt(
                    ast::TypeSize::new_int_bits(256),
                )),
            },
            visibility: None,
            mutability: None,
            data_location: None,
            override_: None,
            indexed: false,
            name: Some(ident),
            initializer: Some(initializer),
        });
        ast::Stmt { span, docs: Default::default(), kind: ast::StmtKind::DeclSingle(variable) }
    }

    fn plugin_source_tokens(&self, span: Span) -> &[FrontendToken] {
        self.plugin_tokens
            .as_ref()
            .expect("plugin callback has a source cache")
            .view(self.sess, span)
    }

    fn plugin_uint(&self, value: U256, span: Span) -> ast::Box<'ast, ast::Expr<'ast>> {
        let lit = self.alloc(ast::Lit {
            span,
            symbol: Symbol::intern(&value.to_string()),
            kind: ast::LitKind::Number(value),
        });
        self.alloc(ast::Expr { span, kind: ast::ExprKind::Lit(lit, None) })
    }

    fn expand_plugin_stmt(
        &mut self,
        expansion: ExpansionStmt,
        args: &mut impl Iterator<Item = (usize, ast::Box<'ast, ast::Expr<'ast>>)>,
        body: &mut Option<ast::Block<'ast>>,
        span: Span,
        depth: usize,
    ) -> PResult<'sess, ast::StmtKind<'ast>> {
        if depth >= 128 {
            return Err(self.dcx().err_at(span, "plugin expansion exceeds nesting limit"));
        }
        match expansion {
            ExpansionStmt::Body => body
                .take()
                .map(ast::StmtKind::Block)
                .ok_or_else(|| self.dcx().err_at(span, "plugin expansion duplicates its body")),
            ExpansionStmt::Block(statements) => {
                let mut expanded = Vec::new();
                for statement in statements {
                    let kind = self.expand_plugin_stmt(statement, args, body, span, depth + 1)?;
                    expanded.push(ast::Stmt { span, docs: Default::default(), kind });
                }
                Ok(ast::StmtKind::Block(ast::Block { span, stmts: self.alloc_vec(expanded) }))
            }
            ExpansionStmt::Repeat { count, body: inner } => {
                let count = self.expand_plugin_expr(count, args, span, 0)?;
                let temporary = self.plugin_ident(span);
                let init = self.alloc(self.plugin_decl(temporary, count, span));
                let condition = self.alloc(ast::Expr {
                    span,
                    kind: ast::ExprKind::Binary(
                        self.alloc(ast::Expr::from_ident(temporary)),
                        ast::BinOp { span, kind: ast::BinOpKind::Gt },
                        self.plugin_uint(U256::ZERO, span),
                    ),
                });
                let decrement = self.alloc(ast::Expr {
                    span,
                    kind: ast::ExprKind::Unary(
                        ast::UnOp { span, kind: ast::UnOpKind::PreDec },
                        self.alloc(ast::Expr::from_ident(temporary)),
                    ),
                });
                let decrement = ast::Stmt {
                    span,
                    docs: Default::default(),
                    kind: ast::StmtKind::Expr(decrement),
                };
                let inner = self.expand_plugin_stmt(*inner, args, body, span, depth + 1)?;
                let inner = ast::Stmt { span, docs: Default::default(), kind: inner };
                let block = ast::Block { span, stmts: self.alloc_vec(vec![decrement, inner]) };
                let body = self.alloc(ast::Stmt {
                    span,
                    docs: Default::default(),
                    kind: ast::StmtKind::Block(block),
                });
                // for (uint256 remaining = count; remaining > 0;) { --remaining; body; }
                Ok(ast::StmtKind::For { init: Some(init), cond: Some(condition), next: None, body })
            }
        }
    }

    fn expand_plugin_expr(
        &self,
        expr: ExpansionExpr,
        args: &mut impl Iterator<Item = (usize, ast::Box<'ast, ast::Expr<'ast>>)>,
        span: Span,
        depth: usize,
    ) -> PResult<'sess, ast::Box<'ast, ast::Expr<'ast>>> {
        if depth >= 128 {
            return Err(self.dcx().err_at(span, "plugin expansion exceeds nesting limit"));
        }
        let kind = match expr {
            ExpansionExpr::Argument(index) => {
                let Some((expected, arg)) = args.next() else {
                    return Err(self.dcx().err_at(span, "plugin expansion duplicates an argument"));
                };
                if expected != index as usize {
                    return Err(self.dcx().err_at(span, "plugin expansion reorders arguments"));
                }
                return Ok(arg);
            }
            ExpansionExpr::Binary { op, left, right } => {
                let kind = match op.as_str() {
                    "+" => Add,
                    "-" => Sub,
                    "*" => Mul,
                    "/" => Div,
                    "%" => Rem,
                    "<<" => Shl,
                    ">>" => Shr,
                    "&" => BitAnd,
                    "|" => BitOr,
                    "^" => BitXor,
                    _ => return Err(self.dcx().err_at(span, "unsupported plugin binary operator")),
                };
                let left = self.expand_plugin_expr(*left, args, span, depth + 1)?;
                let right = self.expand_plugin_expr(*right, args, span, depth + 1)?;
                ast::ExprKind::Binary(left, ast::BinOp { span, kind }, right)
            }
            ExpansionExpr::Uint(value) => {
                let value = value
                    .parse::<U256>()
                    .map_err(|_| self.dcx().err_at(span, "invalid plugin uint256 literal"))?;
                return Ok(self.plugin_uint(value, span));
            }
        };
        Ok(self.alloc(ast::Expr { span, kind }))
    }

    fn prepare_plugin_tokens(&mut self) {
        if self.plugin_expansion.is_some() && self.plugin_tokens.is_none() {
            self.plugin_tokens = Some(SourceTokens::new(self.token, self.tokens.as_slice()));
        }
    }
}

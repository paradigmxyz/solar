//! Namespaced expression expansions, validated before entering ordinary name resolution.
//! Arguments are consumed in source order exactly once; this initial adapter rejects recipes
//! that would duplicate, discard or reorder argument evaluation.

use crate::{PResult, Parser};
use solar_ast::{self as ast, token::TokenKind};
use solar_interface::Span;
use solar_pass_plugin::{Expansion, ExpansionExpr, ExpansionRequest, ExtensionKind};

impl<'sess, 'ast, 'cb> Parser<'sess, 'ast, 'cb> {
    /// Installs explicitly loaded frontend extensions for this parser.
    pub fn set_plugin_expansion(
        &mut self,
        callback: impl Fn(&str, &ExpansionRequest) -> Result<Expansion, String> + 'cb,
    ) {
        self.plugin_expansion = Some(std::boxed::Box::new(callback));
    }

    pub(super) fn parse_plugin_expr(&mut self) -> PResult<'sess, ast::Box<'ast, ast::Expr<'ast>>> {
        let start = self.token.span;
        self.expect(TokenKind::At)?;
        let namespace = self.parse_ident()?;
        self.expect(TokenKind::Dot)?;
        let name = self.parse_ident()?;
        let args = self.parse_paren_comma_seq(false, Self::parse_expr)?;
        let span = start.to(self.prev_token.span);
        let request = ExpansionRequest {
            name: name.name.as_str().to_owned(),
            kind: ExtensionKind::Expression,
            tokens: Vec::new(),
        };
        let callback = self
            .plugin_expansion
            .as_ref()
            .ok_or_else(|| self.dcx().err_at(span, "no frontend plugins are loaded"))?;
        let expansion = callback(namespace.name.as_str(), &request)
            .map_err(|error| self.dcx().err_at(span, error))?;
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
                use ast::BinOpKind::*;
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
            ExpansionExpr::Uint(_) => {
                return Err(self.dcx().err_at(span, "plugin literals are not supported yet"));
            }
        };
        Ok(self.alloc(ast::Expr { span, kind }))
    }
}

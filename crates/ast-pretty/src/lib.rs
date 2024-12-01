#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

use core::fmt::{self, Write};
use solar_ast as ast;

/// AST pretty-printer.
#[derive(Debug)]
pub struct Printer<W> {
    writer: W,
    ident: usize,
}

impl<W> Printer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer, ident: 0 }
    }
}

impl<W: Write> Printer<W> {
    fn write_indent(&mut self) -> fmt::Result {
        write!(self.writer, "{}", "    ".repeat(self.ident))
    }

    fn write_block(&mut self, f: impl FnOnce(&mut Self) -> fmt::Result) -> fmt::Result {
        write!(self.writer, " {{\n")?;
        self.ident += 1;
        f(self)?;
        self.ident -= 1;
        write!(self.writer, "}}")
    }

    fn write_comma_separated<I>(
        &mut self,
        iter: &[I],
        f: impl Fn(&mut Self, &I) -> fmt::Result,
    ) -> fmt::Result {
        let mut iter = iter.iter();
        if let Some(first) = iter.next() {
            f(self, first)?;
            for item in iter {
                self.writer.write_str(", ")?;
                f(self, item)?;
            }
        }

        Ok(())
    }

    pub fn write_source_unit(&mut self, source_unit: &ast::SourceUnit<'_>) -> fmt::Result {
        for item in source_unit.items.iter() {
            self.write_item(item)?;
        }

        Ok(())
    }

    fn write_item(&mut self, item: &ast::Item<'_>) -> fmt::Result {
        match &item.kind {
            ast::ItemKind::Pragma(item) => self.write_pragma_directive(item)?,
            ast::ItemKind::Contract(item) => self.write_item_contract(item)?,
            ast::ItemKind::Struct(item) => self.write_item_struct(item)?,
            ast::ItemKind::Variable(item) => {
                self.write_variable_definition(item)?;
                self.writer.write_str(";\n")?;
            }
            _ => todo!(),
        };
        Ok(())
    }

    fn write_pragma_directive(&mut self, pragma: &ast::PragmaDirective<'_>) -> fmt::Result {
        let ast::PragmaDirective { tokens } = pragma;
        self.writer.write_str("pragma ")?;
        match tokens {
            ast::PragmaTokens::Custom(name, value) => {
                write!(self.writer, "{}", name.as_str())?;
                if let Some(value) = value {
                    write!(self.writer, " {}", value.as_str())?;
                }
            }
            ast::PragmaTokens::Verbatim(tokens) => {
                for token in tokens.iter() {
                    write!(self.writer, "{}", token.kind.as_str())?;
                }
            }
            ast::PragmaTokens::Version(ident, req) => {
                write!(self.writer, "{ident}")?;
                write!(self.writer, " {req}")?;
            }
        }
        self.writer.write_str(";\n")?;
        Ok(())
    }

    fn write_item_contract(&mut self, contract: &ast::ItemContract<'_>) -> fmt::Result {
        let ast::ItemContract { kind, name, bases, body } = contract;

        self.write_indent()?;
        write!(self.writer, "{kind} {name}")?;

        if !bases.is_empty() {
            self.writer.write_str(" is ")?;
            self.write_comma_separated(bases, |this, base| this.write_modifier(base))?;
        }

        self.write_block(|this| {
            for item in body.iter() {
                this.write_indent()?;
                this.write_item(item)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    fn write_item_struct(&mut self, struct_: &ast::ItemStruct<'_>) -> fmt::Result {
        let ast::ItemStruct { name, fields } = struct_;
        write!(self.writer, "struct {name}")?;

        self.write_block(|this| {
            for field in fields.iter() {
                this.write_indent()?;
                this.write_variable_definition(field)?;
                this.writer.write_str(";\n")?;
            }
            Ok(())
        })
    }

    fn write_variable_definition(&mut self, var_def: &ast::VariableDefinition<'_>) -> fmt::Result {
        let ast::VariableDefinition {
            name,
            ty,
            initializer,
            span: _,
            visibility,
            mutability,
            data_location,
            override_,
            indexed,
        } = var_def;

        self.write_ty(ty)?;

        if let Some(visibility) = visibility {
            write!(self.writer, " {visibility}")?;
        }
        if let Some(mutability) = mutability {
            write!(self.writer, " {mutability}")?;
        }
        if let Some(data_location) = data_location {
            write!(self.writer, " {data_location}")?;
        }
        if let Some(override_) = override_ {
            self.write_override(override_)?;
        }
        if *indexed {
            self.writer.write_str(" indexed")?;
        }

        if let Some(name) = name {
            write!(self.writer, " {name}")?;
        }

        if let Some(init) = initializer {
            self.writer.write_str(" = ")?;
            self.write_expr(init)?;
        }

        Ok(())
    }

    fn write_ty(&mut self, ty: &ast::Type<'_>) -> fmt::Result {
        let ast::Type { span: _, kind } = ty;
        match &kind {
            ast::TypeKind::Elementary(ty) => ty.write_abi_str(&mut self.writer)?,
            ast::TypeKind::Array(ty) => {
                let ast::TypeArray { size, element } = ty;
                self.write_ty(&element)?;
                self.writer.write_str("[")?;
                if let Some(size) = size {
                    self.write_expr(size)?;
                }
                self.writer.write_str("]")?;
            }
            ast::TypeKind::Function(ty) => {
                let ast::TypeFunction { parameters, returns, visibility, state_mutability } = ty;
                self.writer.write_str("function(")?;
                self.write_comma_separated(parameters, |this, param| {
                    this.write_variable_definition(param)
                })?;
                self.writer.write_str(")")?;
                if let Some(visibility) = visibility {
                    write!(self.writer, " {visibility}")?;
                }
                write!(self.writer, " {state_mutability}")?;
                if !returns.is_empty() {
                    self.writer.write_str(" returns(")?;
                    self.write_comma_separated(returns, |this, ret| {
                        this.write_variable_definition(ret)
                    })?;
                    self.writer.write_str(")")?;
                }
            }
            ast::TypeKind::Mapping(ty) => {
                let ast::TypeMapping { key, value, key_name, value_name } = ty;
                self.writer.write_str("mapping(")?;
                self.write_ty(key)?;
                if let Some(key_name) = key_name {
                    write!(self.writer, " {key_name}")?;
                }
                self.writer.write_str(" => ")?;
                self.write_ty(value)?;
                if let Some(value_name) = value_name {
                    write!(self.writer, " {value_name}")?;
                }
                self.writer.write_str(")")?;
            }
            ast::TypeKind::Custom(ty) => {
                write!(self.writer, "{ty}")?;
            }
        };

        Ok(())
    }

    fn write_override(&mut self, override_: &ast::Override<'_>) -> fmt::Result {
        let ast::Override { paths, span: _ } = override_;
        self.writer.write_str("override(")?;
        self.write_comma_separated(paths, |this, path| write!(this.writer, "{path}"))?;
        self.writer.write_str(")")
    }

    fn write_modifier(&mut self, modifier: &ast::Modifier<'_>) -> fmt::Result {
        let ast::Modifier { name, arguments } = modifier;
        write!(self.writer, "{name}")?;
        self.write_call_args(arguments)
    }

    fn write_call_args(&mut self, args: &ast::CallArgs<'_>) -> fmt::Result {
        self.writer.write_char('(')?;
        match args {
            ast::CallArgs::Unnamed(args) => {
                self.write_comma_separated(args, |this, expr| this.write_expr(expr))?;
            }
            ast::CallArgs::Named(args) => {
                self.writer.write_str("{{")?;
                self.write_comma_separated(args, |this, arg| {
                    let ast::NamedArg { name, value } = arg;
                    write!(this.writer, "{name}: ")?;
                    this.write_expr(value)
                })?;
                self.writer.write_str("}}")?;
            }
        }
        self.writer.write_char(')')
    }

    fn write_expr(&mut self, expr: &ast::Expr<'_>) -> fmt::Result {
        match &expr.kind {
            ast::ExprKind::Array(exprs) => {
                self.write_comma_separated(exprs, |this, expr| this.write_expr(expr))?;
            }
            ast::ExprKind::Assign(lhs, op, rhs) => {
                self.write_expr(lhs)?;
                self.writer.write_char(' ')?;
                if let Some(op) = op {
                    write!(self.writer, "{op}")?;
                }
                self.writer.write_str("= ")?;
                self.write_expr(rhs)?;
            }
            ast::ExprKind::Binary(lhs, op, rhs) => {
                self.write_expr(lhs)?;
                write!(self.writer, " {op} ")?;
                self.write_expr(rhs)?;
            }
            ast::ExprKind::Call(expr, args) => {
                self.write_expr(expr)?;
                self.write_call_args(args)?;
            }
            _ => todo!(),
        }

        Ok(())
    }
}

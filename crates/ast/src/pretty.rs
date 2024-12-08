//! AST pretty-printing.

use crate::ast;
use core::fmt::{self, Write};

/// AST pretty-printer.
#[derive(Debug)]
pub struct Printer<W> {
    writer: W,
    indent: usize,
}

impl<W> Printer<W> {
    pub fn new(writer: W) -> Self {
        Self { writer, indent: 0 }
    }
}

impl<W: Write> Printer<W> {
    fn print_indent(&mut self) -> fmt::Result {
        write!(self.writer, "{}", "    ".repeat(self.indent))
    }

    fn print_block(&mut self, f: impl FnOnce(&mut Self) -> fmt::Result) -> fmt::Result {
        self.writer.write_str("{\n")?;
        self.indent += 1;
        f(self)?;
        self.indent -= 1;
        self.writer.write_str("}")
    }

    fn print_comma_separated<I>(
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

    pub fn print_soure_unit(&mut self, source_unit: &ast::SourceUnit<'_>) -> fmt::Result {
        for item in source_unit.items.iter() {
            self.print_item(item)?;
        }

        Ok(())
    }

    fn print_item(&mut self, item: &ast::Item<'_>) -> fmt::Result {
        match &item.kind {
            ast::ItemKind::Pragma(item) => self.print_pragma_directive(item)?,
            ast::ItemKind::Contract(item) => self.print_item_contract(item)?,
            ast::ItemKind::Struct(item) => self.print_item_struct(item)?,
            ast::ItemKind::Variable(item) => {
                self.print_variable_definition(item)?;
                self.writer.write_str(";\n")?;
            }
            ast::ItemKind::Function(item) => self.print_item_function(item)?,
            _ => todo!(),
        };
        Ok(())
    }

    fn print_pragma_directive(&mut self, pragma: &ast::PragmaDirective<'_>) -> fmt::Result {
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

    fn print_item_contract(&mut self, contract: &ast::ItemContract<'_>) -> fmt::Result {
        let ast::ItemContract { kind, name, bases, body } = contract;

        self.print_indent()?;
        write!(self.writer, "{kind} {name}")?;

        if !bases.is_empty() {
            self.writer.write_str(" is ")?;
            self.print_comma_separated(bases, |this, base| this.print_modifier(base))?;
        }

        self.print_block(|this| {
            for item in body.iter() {
                this.print_indent()?;
                this.print_item(item)?;
            }
            Ok(())
        })?;

        Ok(())
    }

    fn print_item_struct(&mut self, struct_: &ast::ItemStruct<'_>) -> fmt::Result {
        let ast::ItemStruct { name, fields } = struct_;
        write!(self.writer, "struct {name}")?;

        self.print_block(|this| {
            for field in fields.iter() {
                this.print_indent()?;
                this.print_variable_definition(field)?;
                this.writer.write_str(";\n")?;
            }
            Ok(())
        })
    }

    fn print_item_function(&mut self, function: &ast::ItemFunction<'_>) -> fmt::Result {
        let ast::ItemFunction { kind, header, body } = function;
        let ast::FunctionHeader {
            name,
            parameters,
            visibility,
            state_mutability,
            modifiers,
            virtual_,
            override_,
            returns,
        } = header;

        write!(self.writer, "{kind}")?;
        if let Some(name) = name {
            write!(self.writer, " {name}")?;
        }

        self.writer.write_str("(")?;
        self.print_comma_separated(parameters, |this, param| {
            this.print_variable_definition(param)
        })?;
        self.writer.write_str(")")?;

        if let Some(visibility) = visibility {
            write!(self.writer, " {visibility}")?;
        }

        write!(self.writer, " {state_mutability}")?;

        for modifier in modifiers.iter() {
            self.writer.write_char(' ')?;
            self.print_modifier(modifier)?;
        }

        if *virtual_ {
            write!(self.writer, " virtual")?;
        }

        if let Some(override_) = override_ {
            self.print_override(override_)?;
        }

        if !returns.is_empty() {
            write!(self.writer, " returns (")?;
            self.print_comma_separated(returns, |this, val| this.print_variable_definition(val))?;
            self.writer.write_str(")")?;
        }

        if let Some(body) = body {
            self.print_block(|this| {
                for stmt in body.iter() {
                    this.print_stmt(stmt)?;
                }

                Ok(())
            })?;
        } else {
            self.writer.write_char(';')?;
        }

        Ok(())
    }

    fn print_variable_definition(&mut self, var_def: &ast::VariableDefinition<'_>) -> fmt::Result {
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

        self.print_ty(ty)?;

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
            self.print_override(override_)?;
        }
        if *indexed {
            self.writer.write_str(" indexed")?;
        }

        if let Some(name) = name {
            write!(self.writer, " {name}")?;
        }

        if let Some(init) = initializer {
            self.writer.write_str(" = ")?;
            self.print_expr(init)?;
        }

        Ok(())
    }

    fn print_ty(&mut self, ty: &ast::Type<'_>) -> fmt::Result {
        let ast::Type { span: _, kind } = ty;
        match &kind {
            ast::TypeKind::Elementary(ty) => ty.write_abi_str(&mut self.writer)?,
            ast::TypeKind::Array(ty) => {
                let ast::TypeArray { size, element } = ty;
                self.print_ty(element)?;
                self.writer.write_str("[")?;
                if let Some(size) = size {
                    self.print_expr(size)?;
                }
                self.writer.write_str("]")?;
            }
            ast::TypeKind::Function(ty) => {
                let ast::TypeFunction { parameters, returns, visibility, state_mutability } = ty;
                self.writer.write_str("function(")?;
                self.print_comma_separated(parameters, |this, param| {
                    this.print_variable_definition(param)
                })?;
                self.writer.write_str(")")?;
                if let Some(visibility) = visibility {
                    write!(self.writer, " {visibility}")?;
                }
                write!(self.writer, " {state_mutability}")?;
                if !returns.is_empty() {
                    self.writer.write_str(" returns(")?;
                    self.print_comma_separated(returns, |this, ret| {
                        this.print_variable_definition(ret)
                    })?;
                    self.writer.write_str(")")?;
                }
            }
            ast::TypeKind::Mapping(ty) => {
                let ast::TypeMapping { key, value, key_name, value_name } = ty;
                self.writer.write_str("mapping(")?;
                self.print_ty(key)?;
                if let Some(key_name) = key_name {
                    write!(self.writer, " {key_name}")?;
                }
                self.writer.write_str(" => ")?;
                self.print_ty(value)?;
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

    fn print_override(&mut self, override_: &ast::Override<'_>) -> fmt::Result {
        let ast::Override { paths, span: _ } = override_;
        self.writer.write_str("override(")?;
        self.print_comma_separated(paths, |this, path| write!(this.writer, "{path}"))?;
        self.writer.write_str(")")
    }

    fn print_modifier(&mut self, modifier: &ast::Modifier<'_>) -> fmt::Result {
        let ast::Modifier { name, arguments } = modifier;
        write!(self.writer, "{name}")?;
        self.print_call_args(arguments)
    }

    fn print_call_args(&mut self, args: &ast::CallArgs<'_>) -> fmt::Result {
        self.writer.write_char('(')?;
        match args {
            ast::CallArgs::Unnamed(args) => {
                self.print_comma_separated(args, |this, expr| this.print_expr(expr))?;
            }
            ast::CallArgs::Named(args) => {
                self.writer.write_str("{{")?;
                self.print_comma_separated(args, |this, arg| {
                    let ast::NamedArg { name, value } = arg;
                    write!(this.writer, "{name}: ")?;
                    this.print_expr(value)
                })?;
                self.writer.write_str("}}")?;
            }
        }
        self.writer.write_char(')')
    }

    fn print_expr(&mut self, expr: &ast::Expr<'_>) -> fmt::Result {
        match &expr.kind {
            ast::ExprKind::Array(exprs) => {
                self.print_comma_separated(exprs, |this, expr| this.print_expr(expr))?;
            }
            ast::ExprKind::Assign(lhs, op, rhs) => {
                self.print_expr(lhs)?;
                self.writer.write_char(' ')?;
                if let Some(op) = op {
                    write!(self.writer, "{op}")?;
                }
                self.writer.write_str("= ")?;
                self.print_expr(rhs)?;
            }
            ast::ExprKind::Binary(lhs, op, rhs) => {
                self.print_expr(lhs)?;
                write!(self.writer, " {op} ")?;
                self.print_expr(rhs)?;
            }
            ast::ExprKind::Call(expr, args) => {
                self.print_expr(expr)?;
                self.print_call_args(args)?;
            }
            _ => todo!(),
        }

        Ok(())
    }

    fn print_stmt(&mut self, stmt: &ast::Stmt<'_>) -> fmt::Result {
        let ast::Stmt { docs, span, kind } = stmt;
        match kind {
            ast::StmtKind::Block(block) => {
                self.writer.write_str("{\n")?;
                for stmt in block.iter() {
                    self.print_indent()?;
                    self.print_stmt(stmt)?;
                    self.writer.write_str(";\n")?;
                }
                self.writer.write_str("}\n")?;
            }
            ast::StmtKind::Break => {
                self.writer.write_str("break;\n")?;
            }
            ast::StmtKind::Continue => {
                self.writer.write_str("continue;\n")?;
            }
            _ => todo!()
        }
        Ok(())
    }
}

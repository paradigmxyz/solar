//! AST pretty-printing.

use crate::ast::{self, yul};
use core::fmt::{self, Write};

/// AST pretty-printer.
#[derive(Debug)]
pub struct Printer<W> {
    writer: W,
    indent: usize,
}

impl<W> Printer<W> {
    /// Creates a new printer with the given writer.
    pub fn new(writer: W) -> Self {
        Self { writer, indent: 0 }
    }
}

impl<W: Write> Printer<W> {
    /// Prints a single [Solidity source file](`ast::SourceUnit`).
    pub fn print_soure_unit(&mut self, source_unit: &ast::SourceUnit<'_>) -> fmt::Result {
        let ast::SourceUnit { items } = source_unit;

        for item in items.iter() {
            self.print_item(item)?;
            self.writer.write_char('\n')?;
        }

        Ok(())
    }

    /// Prints a single [item](`ast::Item`).
    pub fn print_item(&mut self, item: &ast::Item<'_>) -> fmt::Result {
        let ast::Item { docs, span: _, kind } = item;

        self.print_docs(docs)?;
        match kind {
            ast::ItemKind::Pragma(item) => self.print_pragma_directive(item)?,
            ast::ItemKind::Import(item) => self.print_import_directive(item)?,
            ast::ItemKind::Using(item) => self.print_using_directive(item)?,
            ast::ItemKind::Contract(item) => self.print_item_contract(item)?,
            ast::ItemKind::Function(item) => self.print_item_function(item)?,
            ast::ItemKind::Variable(item) => {
                self.print_variable_definition(item)?;
                self.writer.write_char(';')?;
            }
            ast::ItemKind::Struct(item) => self.print_item_struct(item)?,
            ast::ItemKind::Enum(item) => self.print_item_enum(item)?,
            ast::ItemKind::Udvt(item) => self.print_item_udvt(item)?,
            ast::ItemKind::Error(item) => self.print_item_error(item)?,
            ast::ItemKind::Event(item) => self.print_item_event(item)?,
        };

        Ok(())
    }

    /// Prints a single [statement](`ast::Stmt`).
    pub fn print_stmt(&mut self, stmt: &ast::Stmt<'_>) -> fmt::Result {
        let ast::Stmt { docs, span: _, kind } = stmt;

        self.print_docs(docs)?;
        match kind {
            ast::StmtKind::Assembly(ast::StmtAssembly { dialect, flags, block }) => {
                self.writer.write_str("assembly")?;
                if let Some(dialect) = dialect {
                    write!(self.writer, " \"{}\"", dialect.value.as_str())?;
                }

                if !flags.is_empty() {
                    self.writer.write_str(" (")?;
                    self.print_comma_separated(flags, |this, flag| {
                        write!(this.writer, "\"{}\"", flag.value.as_str())
                    })?;
                    self.writer.write_char(')')?;
                }

                self.writer.write_char(' ')?;
                self.print_block_lines(block, |this, stmt| this.print_yul_stmt(stmt))?
            }
            ast::StmtKind::DeclSingle(decl) => {
                self.print_variable_definition(decl)?;
                self.writer.write_char(';')?;
            }
            ast::StmtKind::DeclMulti(vars, def) => {
                self.writer.write_char('(')?;
                self.print_comma_separated(vars, |this, var| {
                    if let Some(var) = var {
                        this.print_variable_definition(var)?;
                    }

                    Ok(())
                })?;
                self.writer.write_str(") = ")?;
                self.print_expr(def)?;
                self.writer.write_char(';')?;
            }
            ast::StmtKind::Block(block) => {
                self.print_block_lines(block, |this, stmt| this.print_stmt(stmt))?
            }
            ast::StmtKind::Break => {
                self.writer.write_str("break;")?;
            }
            ast::StmtKind::Continue => {
                self.writer.write_str("continue;")?;
            }
            ast::StmtKind::DoWhile(body, cond) => {
                self.writer.write_str("do ")?;
                self.print_stmt(body)?;
                self.writer.write_str(" while (")?;
                self.print_expr(cond)?;
                self.writer.write_str(");")?;
            }
            ast::StmtKind::Emit(event, args) => {
                write!(self.writer, "emit {event}")?;
                self.print_call_args(args)?;
                self.writer.write_char(';')?;
            }
            ast::StmtKind::Expr(expr) => {
                self.print_expr(expr)?;
                self.writer.write_char(';')?;
            }
            ast::StmtKind::For { init, cond, next, body } => {
                self.writer.write_str("for (")?;

                if let Some(init) = init {
                    self.print_stmt(init)?;
                } else {
                    self.writer.write_char(';')?;
                }

                if let Some(cond) = cond {
                    self.writer.write_char(' ')?;
                    self.print_expr(cond)?;
                }

                self.writer.write_char(';')?;

                if let Some(next) = next {
                    self.writer.write_char(' ')?;
                    self.print_expr(next)?;
                }

                self.writer.write_str(") ")?;
                self.print_stmt(body)?;
            }
            ast::StmtKind::If(cond, body, else_) => {
                self.writer.write_str("if (")?;
                self.print_expr(cond)?;
                self.writer.write_str(") ")?;
                self.print_stmt(body)?;
                if let Some(else_) = else_ {
                    self.writer.write_str(" else ")?;
                    self.print_stmt(else_)?;
                }
            }
            ast::StmtKind::Return(expr) => {
                self.writer.write_str("return")?;
                if let Some(expr) = expr {
                    self.writer.write_char(' ')?;
                    self.print_expr(expr)?;
                }
                self.writer.write_char(';')?;
            }
            ast::StmtKind::Revert(error, args) => {
                write!(self.writer, "revert {error}")?;
                self.print_call_args(args)?;
                self.writer.write_char(';')?;
            }
            ast::StmtKind::Try(ast::StmtTry { expr, returns, block, catch }) => {
                self.writer.write_str("try ")?;
                self.print_expr(expr)?;
                if !returns.is_empty() {
                    self.writer.write_str(" returns (")?;
                    self.print_comma_separated(returns, |this, ret| {
                        this.print_variable_definition(ret)
                    })?;
                    self.writer.write_char(')')?;
                }

                self.print_block_lines(block, |this, stmt| this.print_stmt(stmt))?;

                for catch in catch.iter() {
                    let ast::CatchClause { name, args, block } = catch;

                    self.writer.write_str(" catch ")?;
                    if let Some(name) = name {
                        write!(self.writer, "{name}")?;
                    }
                    self.writer.write_char('(')?;
                    self.print_comma_separated(args, |this, arg| {
                        this.print_variable_definition(arg)
                    })?;
                    self.writer.write_str(") ")?;
                    self.print_block_lines(block, |this, stmt| this.print_stmt(stmt))?;
                }
            }
            ast::StmtKind::UncheckedBlock(block) => {
                self.writer.write_str("unchecked ")?;
                self.print_block_lines(block, |this, stmt| this.print_stmt(stmt))?;
            }
            ast::StmtKind::While(cond, body) => {
                self.writer.write_str("while (")?;
                self.print_expr(cond)?;
                self.writer.write_str(") ")?;
                self.print_stmt(body)?;
            }
            ast::StmtKind::Placeholder => {
                self.writer.write_str("_;")?;
            }
        }
        Ok(())
    }

    /// Prints a single [Yul statement](`yul::Stmt`).
    pub fn print_yul_stmt(&mut self, stmt: &yul::Stmt<'_>) -> fmt::Result {
        let yul::Stmt { docs, span: _, kind } = stmt;

        self.print_docs(docs)?;
        match kind {
            yul::StmtKind::Block(block) => {
                self.print_block_lines(block, |this, stmt| this.print_yul_stmt(stmt))?
            }
            yul::StmtKind::AssignSingle(path, expr) => {
                write!(self.writer, "{path} := ")?;
                self.print_yul_expr(expr)?;
            }
            yul::StmtKind::AssignMulti(paths, call) => {
                self.print_comma_separated(paths, |this, path| write!(this.writer, "{path}"))?;
                self.writer.write_str(" := ")?;
                self.print_yul_expr_call(call)?;
            }
            yul::StmtKind::Expr(call) => self.print_yul_expr_call(call)?,
            yul::StmtKind::If(cond, body) => {
                self.writer.write_str("if ")?;
                self.print_yul_expr(cond)?;
                self.writer.write_char(' ')?;
                self.print_block_lines(body, |this, stmt| this.print_yul_stmt(stmt))?;
            }
            yul::StmtKind::For { init, cond, step, body } => {
                self.writer.write_str("for ")?;
                self.print_block_lines(init, |this, stmt| this.print_yul_stmt(stmt))?;
                self.writer.write_char(' ')?;
                self.print_yul_expr(cond)?;
                self.writer.write_char(' ')?;
                self.print_block_lines(step, |this, stmt| this.print_yul_stmt(stmt))?;
                self.writer.write_char(' ')?;
                self.print_block_lines(body, |this, stmt| this.print_yul_stmt(stmt))?;
            }
            yul::StmtKind::Switch(yul::StmtSwitch { selector, branches, default_case }) => {
                self.writer.write_str("switch ")?;
                self.print_yul_expr(selector)?;
                for yul::StmtSwitchCase { constant, body } in branches.iter() {
                    self.writer.write_char('\n')?;
                    self.print_indent()?;
                    write!(self.writer, "case {constant} ")?;
                    self.print_block_lines(body, |this, stmt| this.print_yul_stmt(stmt))?;
                }
                if let Some(default_case) = default_case {
                    self.writer.write_char('\n')?;
                    self.print_indent()?;
                    self.writer.write_str("default ")?;
                    self.print_block_lines(default_case, |this, stmt| this.print_yul_stmt(stmt))?;
                }
            }
            yul::StmtKind::Leave => self.writer.write_str("leave")?,
            yul::StmtKind::Break => self.writer.write_str("break")?,
            yul::StmtKind::Continue => self.writer.write_str("continue")?,
            yul::StmtKind::FunctionDef(yul::Function { name, parameters, returns, body }) => {
                write!(self.writer, "function {name}(")?;
                self.print_comma_separated(parameters, |this, param| {
                    write!(this.writer, "{param}")
                })?;
                self.writer.write_str(") -> (")?;
                self.print_comma_separated(returns, |this, ret| write!(this.writer, "{ret}"))?;
                self.writer.write_str(") ")?;
                self.print_block_lines(body, |this, stmt| this.print_yul_stmt(stmt))?
            }
            yul::StmtKind::VarDecl(vars, init) => {
                self.writer.write_str("let ")?;
                self.print_comma_separated(vars, |this, var| write!(this.writer, "{var}"))?;
                if let Some(init) = init {
                    self.writer.write_str(" := ")?;
                    self.print_yul_expr(init)?;
                }
            }
        };

        Ok(())
    }

    /// Prints a single [Solidity expression](`ast::Expr`).
    pub fn print_expr(&mut self, expr: &ast::Expr<'_>) -> fmt::Result {
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
            ast::ExprKind::CallOptions(item, opts) => {
                self.print_expr(item)?;
                self.print_named_args(opts)?;
            }
            ast::ExprKind::Delete(expr) => {
                self.writer.write_str("delete ")?;
                self.print_expr(expr)?;
            }
            ast::ExprKind::Ident(ident) => write!(self.writer, "{ident}")?,
            ast::ExprKind::Index(item, index) => {
                self.print_expr(item)?;
                self.writer.write_char('[')?;
                match index {
                    ast::IndexKind::Index(expr) => {
                        if let Some(expr) = expr {
                            self.print_expr(expr)?;
                        }
                    }
                    ast::IndexKind::Range(start, end) => {
                        if let Some(start) = start {
                            self.print_expr(start)?;
                        }
                        self.writer.write_char(':')?;
                        if let Some(end) = end {
                            self.print_expr(end)?;
                        }
                    }
                }
                self.writer.write_char(']')?;
            }
            ast::ExprKind::Lit(lit, denom) => {
                write!(self.writer, "{lit}")?;
                if let Some(denom) = denom {
                    write!(self.writer, " {denom}")?;
                }
            }
            ast::ExprKind::Member(item, member) => {
                self.print_expr(item)?;
                write!(self.writer, ".{member}")?;
            }
            ast::ExprKind::New(ty) => {
                self.writer.write_str("new ")?;
                self.print_ty(ty)?;
            }
            ast::ExprKind::Payable(opts) => {
                self.writer.write_str("payable")?;
                self.print_call_args(opts)?;
            }
            ast::ExprKind::Ternary(cond, first, second) => {
                self.print_expr(cond)?;
                self.writer.write_str(" ? ")?;
                self.print_expr(first)?;
                self.writer.write_str(" : ")?;
                self.print_expr(second)?;
            }
            ast::ExprKind::Tuple(exprs) => {
                self.writer.write_char('(')?;
                self.print_comma_separated(exprs, |this, expr| {
                    if let Some(expr) = expr {
                        this.print_expr(expr)?;
                    }

                    Ok(())
                })?;
                self.writer.write_char(')')?;
            }
            ast::ExprKind::TypeCall(ty) => {
                self.writer.write_str("type(")?;
                self.print_ty(ty)?;
                self.writer.write_char(')')?;
            }
            ast::ExprKind::Type(ty) => self.print_ty(ty)?,
            ast::ExprKind::Unary(op, expr) => {
                if op.kind.is_prefix() {
                    write!(self.writer, "{op}")?;
                    self.print_expr(expr)?;
                } else {
                    self.print_expr(expr)?;
                    write!(self.writer, "{op}")?;
                }
            }
        }

        Ok(())
    }

    /// Prints a single [Yul expression](`yul::Expr`).
    pub fn print_yul_expr(&mut self, expr: &yul::Expr<'_>) -> fmt::Result {
        let yul::Expr { span: _, kind } = expr;

        match kind {
            yul::ExprKind::Path(path) => write!(self.writer, "{path}")?,
            yul::ExprKind::Call(call) => self.print_yul_expr_call(call)?,
            yul::ExprKind::Lit(lit) => write!(self.writer, "{lit}")?,
        };

        Ok(())
    }

    fn print_indent(&mut self) -> fmt::Result {
        write!(self.writer, "{}", "    ".repeat(self.indent))
    }

    fn print_block_lines<I>(
        &mut self,
        items: &[I],
        f: impl Fn(&mut Self, &I) -> fmt::Result,
    ) -> fmt::Result {
        if items.is_empty() {
            return self.writer.write_str("{}");
        }

        self.writer.write_char('{')?;
        self.indent += 1;
        for item in items {
            self.writer.write_char('\n')?;
            self.print_indent()?;
            f(self, item)?;
        }
        self.indent -= 1;
        self.writer.write_char('\n')?;
        self.print_indent()?;
        self.writer.write_char('}')
    }

    fn print_comma_separated<I>(
        &mut self,
        items: &[I],
        f: impl Fn(&mut Self, &I) -> fmt::Result,
    ) -> fmt::Result {
        let mut iter = items.iter();
        if let Some(first) = iter.next() {
            f(self, first)?;
            for item in iter {
                self.writer.write_str(", ")?;
                f(self, item)?;
            }
        }

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
        self.writer.write_char(';')?;
        Ok(())
    }

    fn print_import_directive(&mut self, import: &ast::ImportDirective<'_>) -> fmt::Result {
        let ast::ImportDirective { path, items } = import;

        self.writer.write_str("import ")?;
        match items {
            ast::ImportItems::Plain(alias) => {
                write!(self.writer, "\"{}\"", path.value)?;
                if let Some(alias) = alias {
                    write!(self.writer, " as {alias}")?;
                }
                self.writer.write_char(';')?;
            }
            ast::ImportItems::Aliases(items) => {
                self.writer.write_char('{')?;
                self.print_comma_separated(items, |this, (item, alias)| {
                    write!(this.writer, "{item}")?;
                    if let Some(alias) = alias {
                        write!(this.writer, " as {alias}")?;
                    }

                    Ok(())
                })?;
                write!(self.writer, "}} from \"{}\";", path.value)?;
            }
            ast::ImportItems::Glob(alias) => {
                self.writer.write_char('*')?;
                if let Some(alias) = alias {
                    write!(self.writer, " as {alias}")?;
                }
                write!(self.writer, " from \"{}\";", path.value)?;
            }
        };

        Ok(())
    }

    fn print_using_directive(&mut self, using: &ast::UsingDirective<'_>) -> fmt::Result {
        let ast::UsingDirective { list, ty, global } = using;

        self.writer.write_str("using ")?;

        match list {
            ast::UsingList::Single(path) => write!(self.writer, "{path}")?,
            ast::UsingList::Multiple(paths) => {
                self.writer.write_char('{')?;
                self.print_comma_separated(paths, |this, (path, op)| {
                    write!(this.writer, "{path}")?;
                    if let Some(op) = op {
                        write!(this.writer, " as {}", op.to_str())?;
                    }

                    Ok(())
                })?;
                self.writer.write_char('}')?;
            }
        };

        self.writer.write_str(" for ")?;

        if let Some(ty) = ty {
            self.print_ty(ty)?;
        } else {
            self.writer.write_char('*')?;
        }

        if *global {
            self.writer.write_str(" global")?;
        }

        self.writer.write_char(';')?;

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

        self.writer.write_char(' ')?;
        self.print_block_lines(body, |this, item| this.print_item(item))
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

        self.writer.write_char('(')?;
        self.print_comma_separated(parameters, |this, param| {
            this.print_variable_definition(param)
        })?;
        self.writer.write_char(')')?;

        if let Some(visibility) = visibility {
            write!(self.writer, " {visibility}")?;
        }

        // Skip writing default state mutability.
        if !state_mutability.is_non_payable() {
            write!(self.writer, " {state_mutability}")?;
        }

        for modifier in modifiers.iter() {
            self.writer.write_char(' ')?;
            self.print_modifier(modifier)?;
        }

        if *virtual_ {
            write!(self.writer, " virtual")?;
        }

        if let Some(override_) = override_ {
            self.writer.write_char(' ')?;
            self.print_override(override_)?;
        }

        if !returns.is_empty() {
            write!(self.writer, " returns (")?;
            self.print_comma_separated(returns, |this, val| this.print_variable_definition(val))?;
            self.writer.write_char(')')?;
        }

        if let Some(body) = body {
            self.writer.write_char(' ')?;
            self.print_block_lines(body, |this, stmt| this.print_stmt(stmt))?;
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
            self.writer.write_char(' ')?;
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

    fn print_item_struct(&mut self, struct_: &ast::ItemStruct<'_>) -> fmt::Result {
        let ast::ItemStruct { name, fields } = struct_;

        write!(self.writer, "struct {name} ")?;
        self.print_block_lines(fields, |this, field| {
            this.print_variable_definition(field)?;
            this.writer.write_char(';')
        })
    }

    fn print_item_enum(&mut self, enum_: &ast::ItemEnum<'_>) -> fmt::Result {
        let ast::ItemEnum { name, variants } = enum_;

        write!(self.writer, "enum {name} ")?;
        self.print_block_lines(variants, |this, variant| write!(this.writer, "{variant},"))
    }

    fn print_item_udvt(&mut self, udvt: &ast::ItemUdvt<'_>) -> fmt::Result {
        let ast::ItemUdvt { name, ty } = udvt;

        write!(self.writer, "type {name} is ")?;
        self.print_ty(ty)?;
        self.writer.write_char(';')
    }

    fn print_item_error(&mut self, error: &ast::ItemError<'_>) -> fmt::Result {
        let ast::ItemError { name, parameters } = error;

        write!(self.writer, "error {name}(")?;
        self.print_comma_separated(parameters, |this, param| {
            this.print_variable_definition(param)
        })?;

        self.writer.write_str(");")
    }

    fn print_item_event(&mut self, error: &ast::ItemEvent<'_>) -> fmt::Result {
        let ast::ItemEvent { name, parameters, anonymous } = error;

        write!(self.writer, "event {name}(")?;
        self.print_comma_separated(parameters, |this, param| {
            this.print_variable_definition(param)
        })?;

        if *anonymous {
            self.writer.write_str(" anonymous")?;
        }

        self.writer.write_str(");")
    }

    fn print_ty(&mut self, ty: &ast::Type<'_>) -> fmt::Result {
        let ast::Type { span: _, kind } = ty;

        match &kind {
            ast::TypeKind::Elementary(ty) => ty.write_abi_str(&mut self.writer)?,
            ast::TypeKind::Array(ty) => {
                let ast::TypeArray { size, element } = ty;
                self.print_ty(element)?;
                self.writer.write_char('[')?;
                if let Some(size) = size {
                    self.print_expr(size)?;
                }
                self.writer.write_char(']')?;
            }
            ast::TypeKind::Function(ty) => {
                let ast::TypeFunction { parameters, returns, visibility, state_mutability } = ty;
                self.writer.write_str("function(")?;
                self.print_comma_separated(parameters, |this, param| {
                    this.print_variable_definition(param)
                })?;
                self.writer.write_char(')')?;
                if let Some(visibility) = visibility {
                    write!(self.writer, " {visibility}")?;
                }
                if !state_mutability.is_non_payable() {
                    write!(self.writer, " {state_mutability}")?;
                }
                if !returns.is_empty() {
                    self.writer.write_str(" returns(")?;
                    self.print_comma_separated(returns, |this, ret| {
                        this.print_variable_definition(ret)
                    })?;
                    self.writer.write_char(')')?;
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
                self.writer.write_char(')')?;
            }
            ast::TypeKind::Custom(ty) => {
                write!(self.writer, "{ty}")?;
            }
        };

        Ok(())
    }

    fn print_override(&mut self, override_: &ast::Override<'_>) -> fmt::Result {
        let ast::Override { paths, span: _ } = override_;

        self.writer.write_str("override")?;
        if !paths.is_empty() {
            self.writer.write_char('(')?;
            self.print_comma_separated(paths, |this, path| write!(this.writer, "{path}"))?;
            self.writer.write_char(')')?;
        }

        Ok(())
    }

    fn print_modifier(&mut self, modifier: &ast::Modifier<'_>) -> fmt::Result {
        let ast::Modifier { name, arguments } = modifier;

        write!(self.writer, "{name}")?;
        if !arguments.is_empty() {
            self.print_call_args(arguments)?;
        }

        Ok(())
    }

    fn print_call_args(&mut self, args: &ast::CallArgs<'_>) -> fmt::Result {
        self.writer.write_char('(')?;
        match args {
            ast::CallArgs::Unnamed(args) => {
                self.print_comma_separated(args, |this, expr| this.print_expr(expr))?;
            }
            ast::CallArgs::Named(args) => self.print_named_args(args)?,
        }
        self.writer.write_char(')')
    }

    fn print_named_args(&mut self, args: &ast::NamedArgList<'_>) -> fmt::Result {
        self.writer.write_char('{')?;
        self.print_comma_separated(args, |this, arg| {
            let ast::NamedArg { name, value } = arg;
            write!(this.writer, "{name}: ")?;
            this.print_expr(value)
        })?;
        self.writer.write_char('}')
    }

    fn print_docs(&mut self, items: &ast::DocComments<'_>) -> fmt::Result {
        for item in items.iter() {
            let ast::DocComment { span: _, kind, symbol } = item;
            match kind {
                ast::CommentKind::Line => {
                    self.writer.write_str("/// ")?;
                    self.writer.write_str(symbol.as_str())?;
                }
                ast::CommentKind::Block => {
                    self.writer.write_str("/* ")?;
                    self.writer.write_str(symbol.as_str())?;
                    self.writer.write_str(" */")?;
                }
            }

            self.writer.write_char('\n')?;
            self.print_indent()?;
        }
        Ok(())
    }

    fn print_yul_expr_call(&mut self, call: &yul::ExprCall<'_>) -> fmt::Result {
        let yul::ExprCall { name, arguments } = call;

        write!(self.writer, "{name}(")?;
        self.print_comma_separated(arguments, |this, expr| this.print_yul_expr(expr))?;
        self.writer.write_char(')')
    }
}

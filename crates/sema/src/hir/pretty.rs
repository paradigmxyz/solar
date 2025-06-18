use super::*;
use std::fmt::{self, Write};

impl fmt::Display for Expr<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            ExprKind::Lit(lit) => write!(f, "{lit}"),
            ExprKind::Binary(lhs, op, rhs) => write!(f, "({lhs} {op} {rhs})"),
            ExprKind::Call(callee, args, _) => {
                write!(f, "{callee}")?;
                write!(f, "(")?;
                for (i, arg) in args.exprs().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{arg}")?;
                }
                write!(f, ")")
            }
            ExprKind::Ident(res) => match &res[0] {
                Res::Item(id) => match id {
                    ItemId::Variable(id) => write!(f, "<var:{id:?}>"),
                    ItemId::Function(id) => write!(f, "<fn:{id:?}>"),
                    _ => write!(f, "<item:{id:?}>"),
                },
                _ => write!(f, "<res:{:?}>", res[0]),
            },
            _ => write!(f, "<expr:{:?}>", self.kind),
        }
    }
}

impl fmt::Display for Type<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.kind {
            TypeKind::Elementary(ty) => write!(f, "{ty}"),
            TypeKind::Array(arr) => {
                write!(f, "{}", arr.element)?;
                if let Some(size) = arr.size {
                    write!(f, "[{size}]")?;
                } else {
                    write!(f, "[]")?;
                }
                Ok(())
            }
            TypeKind::Function(func) => {
                write!(f, "function (")?;
                for (i, &_param) in func.parameters.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    // TODO: Print parameter type
                }
                write!(f, ") {} {}", func.visibility, func.state_mutability)?;
                if !func.returns.is_empty() {
                    write!(f, " returns (")?;
                    for (i, &_ret) in func.returns.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        // TODO: Print return type
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            TypeKind::Mapping(map) => {
                write!(f, "mapping({} => {})", map.key, map.value)
            }
            TypeKind::Custom(id) => {
                write!(f, "{id:?}") // TODO: Print actual type name
            }
            TypeKind::Err(_) => write!(f, "<error>"),
        }
    }
}

/// A pretty-printer for HIR nodes
pub struct HirPrettyPrinter<'hir> {
    hir: &'hir Hir<'hir>,
    indent: usize,
    buffer: String,
}

impl<'hir> HirPrettyPrinter<'hir> {
    /// Creates a new HIR pretty-printer
    #[must_use = "Creates a new HIR pretty-printer that needs to be used"]
    pub fn new(hir: &'hir Hir<'hir>) -> Self {
        Self { hir, indent: 0, buffer: String::new() }
    }

    /// Returns the pretty-printed string
    pub fn finish(self) -> String {
        self.buffer
    }

    fn indent(&mut self) {
        self.indent += 4;
    }

    fn dedent(&mut self) {
        self.indent -= 4;
    }

    fn write_indent(&mut self) -> fmt::Result {
        for _ in 0..self.indent {
            self.buffer.push(' ');
        }
        Ok(())
    }

    #[allow(dead_code)]
    fn write_newline(&mut self) -> fmt::Result {
        self.buffer.push('\n');
        Ok(())
    }

    /// Pretty-prints a contract
    pub fn print_contract(&mut self, contract: &Contract<'hir>) -> fmt::Result {
        self.write_indent()?;
        write!(self.buffer, "contract {}", contract.name)?;

        if !contract.bases.is_empty() {
            write!(self.buffer, " is ")?;
            for (i, &base) in contract.bases.iter().enumerate() {
                if i > 0 {
                    write!(self.buffer, ", ")?;
                }
                write!(self.buffer, "{}", self.hir.contract(base).name)?;
            }
        }

        writeln!(self.buffer, " {{")?;
        self.indent();

        for &item in contract.items {
            self.print_item(item)?;
        }

        self.dedent();
        self.write_indent()?;
        writeln!(self.buffer, "}}")
    }

    /// Pretty-prints a function
    pub fn print_function(&mut self, function: &Function<'hir>) -> fmt::Result {
        self.write_indent()?;

        // Print function modifiers
        if function.marked_virtual {
            write!(self.buffer, "virtual ")?;
        }
        if function.override_ {
            write!(self.buffer, "override ")?;
        }

        // Print visibility and state mutability
        write!(self.buffer, "{} {} ", function.visibility, function.state_mutability)?;

        // Print function name or kind
        match function.name {
            Some(name) => write!(self.buffer, "{name}")?,
            None => match function.kind {
                FunctionKind::Constructor => write!(self.buffer, "constructor")?,
                FunctionKind::Fallback => write!(self.buffer, "fallback")?,
                FunctionKind::Receive => write!(self.buffer, "receive")?,
                _ => {}
            },
        }

        // Print parameters
        write!(self.buffer, "(")?;
        for (i, &param) in function.parameters.iter().enumerate() {
            if i > 0 {
                write!(self.buffer, ", ")?;
            }
            self.print_variable(param)?;
        }
        write!(self.buffer, ")")?;

        // Print return values
        if !function.returns.is_empty() {
            write!(self.buffer, " returns (")?;
            for (i, &ret) in function.returns.iter().enumerate() {
                if i > 0 {
                    write!(self.buffer, ", ")?;
                }
                self.print_variable(ret)?;
            }
            write!(self.buffer, ")")?;
        }

        // Print function body
        if let Some(body) = function.body {
            writeln!(self.buffer, " {{")?;
            self.indent();
            for stmt in body {
                self.print_stmt(stmt)?;
            }
            self.dedent();
            self.write_indent()?;
            writeln!(self.buffer, "}}")?;
        } else {
            writeln!(self.buffer, ";")?;
        }

        Ok(())
    }

    /// Pretty-prints a variable
    pub fn print_variable(&mut self, var_id: VariableId) -> fmt::Result {
        let var = &self.hir.variable(var_id);
        if let Some(name) = var.name {
            write!(self.buffer, "{} {}", var.ty, name)?;
        } else {
            write!(self.buffer, "{}", var.ty)?;
        }
        Ok(())
    }

    /// Pretty-prints a statement
    pub fn print_stmt(&mut self, stmt: &Stmt<'hir>) -> fmt::Result {
        self.write_indent()?;
        match &stmt.kind {
            StmtKind::Block(block) => {
                writeln!(self.buffer, "{{")?;
                self.indent();
                for stmt in *block {
                    self.print_stmt(stmt)?;
                }
                self.dedent();
                self.write_indent()?;
                writeln!(self.buffer, "}}")?;
            }
            StmtKind::If(cond, then, else_) => {
                write!(self.buffer, "if (")?;
                self.print_expr(cond)?;
                writeln!(self.buffer, ") {{")?;
                self.indent();
                self.print_stmt(then)?;
                self.dedent();
                if let Some(else_) = else_ {
                    self.write_indent()?;
                    writeln!(self.buffer, "}} else {{")?;
                    self.indent();
                    self.print_stmt(else_)?;
                    self.dedent();
                }
                self.write_indent()?;
                writeln!(self.buffer, "}}")?;
            }
            StmtKind::Return(expr) => {
                write!(self.buffer, "return")?;
                if let Some(expr) = expr {
                    write!(self.buffer, " ")?;
                    self.print_expr(expr)?;
                }
                writeln!(self.buffer, ";")?;
            }
            StmtKind::Expr(expr) => {
                self.print_expr(expr)?;
                writeln!(self.buffer, ";")?;
            }
            _ => {
                // TODO: Implement other statement kinds
                writeln!(
                    self.buffer,
                    "// TODO: Implement pretty-printing for this statement kind"
                )?;
            }
        }
        Ok(())
    }

    /// Pretty-prints an expression
    pub fn print_expr(&mut self, expr: &Expr<'hir>) -> fmt::Result {
        match &expr.kind {
            ExprKind::Lit(lit) => write!(self.buffer, "{lit}")?,
            ExprKind::Ident(res) => match &res[0] {
                Res::Item(id) => match id {
                    ItemId::Variable(id) => {
                        write!(self.buffer, "{}", self.hir.variable(*id).name.unwrap())?
                    }
                    ItemId::Function(id) => {
                        write!(self.buffer, "{}", self.hir.function(*id).name.unwrap())?
                    }
                    _ => write!(
                        self.buffer,
                        "// TODO: Implement pretty-printing for this item kind"
                    )?,
                },
                _ => write!(
                    self.buffer,
                    "// TODO: Implement pretty-printing for this resolution kind"
                )?,
            },
            ExprKind::Binary(lhs, op, rhs) => {
                write!(self.buffer, "(")?;
                self.print_expr(lhs)?;
                write!(self.buffer, " {op} ")?;
                self.print_expr(rhs)?;
                write!(self.buffer, ")")?;
            }
            ExprKind::Call(callee, args, _) => {
                self.print_expr(callee)?;
                write!(self.buffer, "(")?;
                for (i, arg) in args.exprs().enumerate() {
                    if i > 0 {
                        write!(self.buffer, ", ")?;
                    }
                    self.print_expr(arg)?;
                }
                write!(self.buffer, ")")?;
            }
            _ => {
                // TODO: Implement other expression kinds
                write!(self.buffer, "// TODO: Implement pretty-printing for this expression kind")?;
            }
        }
        Ok(())
    }

    /// Pretty-prints an item
    pub fn print_item(&mut self, item_id: ItemId) -> fmt::Result {
        match item_id {
            ItemId::Contract(id) => self.print_contract(self.hir.contract(id))?,
            ItemId::Function(id) => self.print_function(self.hir.function(id))?,
            _ => {
                // TODO: Implement other item kinds
                writeln!(self.buffer, "// TODO: Implement pretty-printing for this item kind")?;
            }
        }
        Ok(())
    }
}

impl Hir<'_> {
    /// Pretty-prints the entire HIR
    pub fn pretty_print(&self) -> String {
        let mut printer = HirPrettyPrinter::new(self);
        for source_id in self.source_ids() {
            let source = &self.source(source_id);
            for &item_id in source.items {
                let _ = printer.print_item(item_id);
            }
        }
        printer.finish()
    }
}

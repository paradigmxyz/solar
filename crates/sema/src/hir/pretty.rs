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
            TypeKind::Array(array) => {
                write!(f, "{}", array.element)?;
                if let Some(size) = array.size {
                    write!(f, "[{size}]")?;
                } else {
                    write!(f, "[]")?;
                }
                Ok(())
            }
            TypeKind::Function(func) => {
                write!(f, "function (")?;
                for (i, &param) in func.parameters.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "param_{}", param.index())?;
                }
                write!(f, ") {} {}", func.visibility, func.state_mutability)?;
                if !func.returns.is_empty() {
                    write!(f, " returns (")?;
                    for (i, &ret) in func.returns.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "ret_{}", ret.index())?;
                    }
                    write!(f, ")")?;
                }
                Ok(())
            }
            TypeKind::Mapping(map) => {
                write!(f, "mapping({} => {})", map.key, map.value)
            }
            TypeKind::Custom(id) => {
                // Only print a placeholder here; actual name printing is done in the pretty printer
                write!(f, "<custom_type:{id:?}>")
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
            for stmt in body.stmts {
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

    /// Prints a type, using HIR context for user-defined types
    fn print_type(&mut self, ty: &Type<'hir>) -> fmt::Result {
        match &ty.kind {
            TypeKind::Custom(id) => match id {
                ItemId::Contract(contract_id) => write!(self.buffer, "{}", self.hir.contract(*contract_id).name),
                ItemId::Struct(struct_id) => write!(self.buffer, "{}", self.hir.strukt(*struct_id).name),
                ItemId::Enum(enum_id) => write!(self.buffer, "{}", self.hir.enumm(*enum_id).name),
                ItemId::Udvt(udvt_id) => write!(self.buffer, "{}", self.hir.udvt(*udvt_id).name),
                _ => write!(self.buffer, "{:?}", id),
            },
            TypeKind::Mapping(map) => {
                write!(self.buffer, "mapping(")?;
                self.print_type(&map.key)?;
                if let Some(key_name) = map.key_name {
                    write!(self.buffer, " {}", key_name)?;
                }
                write!(self.buffer, " => ")?;
                self.print_type(&map.value)?;
                if let Some(value_name) = map.value_name {
                    write!(self.buffer, " {}", value_name)?;
                }
                write!(self.buffer, ")")
            }
            _ => write!(self.buffer, "{}", ty),
        }
    }

    /// Pretty-prints a variable
    pub fn print_variable(&mut self, var_id: VariableId) -> fmt::Result {
        let var = &self.hir.variable(var_id);
        if let Some(name) = var.name {
            self.print_type(&var.ty)?;
            write!(self.buffer, " {}", name)?;
        } else {
            self.print_type(&var.ty)?;
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
                for stmt in block.stmts {
                    self.print_stmt(stmt)?;
                }
                self.dedent();
                self.write_indent()?;
                writeln!(self.buffer, "}}")?;
            }
            StmtKind::UncheckedBlock(block) => {
                writeln!(self.buffer, "unchecked {{")?;
                self.indent();
                for stmt in block.stmts {
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
            StmtKind::DeclSingle(var_id) => {
                let var = &self.hir.variable(*var_id);
                let name = var.name.map(|n| n.to_string()).unwrap_or_else(|| "unnamed".to_string());
                write!(self.buffer, "{} {}", var.ty, name)?;
                if let Some(init) = var.initializer {
                    write!(self.buffer, " = ")?;
                    self.print_expr(init)?;
                }
                writeln!(self.buffer, ";")?;
            }
            StmtKind::DeclMulti(vars, expr) => {
                write!(self.buffer, "(")?;
                for (i, var_opt) in vars.iter().enumerate() {
                    if i > 0 {
                        write!(self.buffer, ", ")?;
                    }
                    if let Some(var_id) = var_opt {
                        let var = &self.hir.variable(*var_id);
                        let name = var.name.map(|n| n.to_string()).unwrap_or_else(|| "unnamed".to_string());
                        write!(self.buffer, "{} {}", var.ty, name)?;
                    } else {
                        write!(self.buffer, "_")?;
                    }
                }
                write!(self.buffer, ") = ")?;
                self.print_expr(expr)?;
                writeln!(self.buffer, ";")?;
            }
            StmtKind::Emit(expr) => {
                write!(self.buffer, "emit ")?;
                self.print_expr(expr)?;
                writeln!(self.buffer, ";")?;
            }
            StmtKind::Revert(expr) => {
                write!(self.buffer, "revert ")?;
                self.print_expr(expr)?;
                writeln!(self.buffer, ";")?;
            }
            StmtKind::Break => {
                writeln!(self.buffer, "break;")?;
            }
            StmtKind::Continue => {
                writeln!(self.buffer, "continue;")?;
            }
            StmtKind::Loop(block, source) => {
                match source {
                    LoopSource::For => {
                        writeln!(self.buffer, "for (...) {{")?;
                    }
                    LoopSource::While => {
                        writeln!(self.buffer, "while (...) {{")?;
                    }
                    LoopSource::DoWhile => {
                        writeln!(self.buffer, "do {{")?;
                    }
                }
                self.indent();
                for stmt in block.stmts {
                    self.print_stmt(stmt)?;
                }
                self.dedent();
                self.write_indent()?;
                if matches!(source, LoopSource::DoWhile) {
                    writeln!(self.buffer, "}} while (...);")?;
                } else {
                    writeln!(self.buffer, "}}")?;
                }
            }
            StmtKind::Try(try_stmt) => {
                write!(self.buffer, "try ")?;
                self.print_expr(&try_stmt.expr)?;
                writeln!(self.buffer, " {{")?;
                self.indent();
                // Print the first clause (returns)
                if let Some(clause) = try_stmt.clauses.first() {
                    for stmt in clause.block.stmts {
                        self.print_stmt(stmt)?;
                    }
                }
                self.dedent();
                self.write_indent()?;
                writeln!(self.buffer, "}} catch (...) {{")?;
                self.indent();
                // Print the catch clauses
                for clause in try_stmt.clauses.iter().skip(1) {
                    for stmt in clause.block.stmts {
                        self.print_stmt(stmt)?;
                    }
                }
                self.dedent();
                self.write_indent()?;
                writeln!(self.buffer, "}}")?;
            }
            StmtKind::Placeholder => {
                writeln!(self.buffer, "_;")?;
            }
            StmtKind::Err(_) => {
                writeln!(self.buffer, "// <error>")?;
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
                    ItemId::Contract(id) => {
                        write!(self.buffer, "{}", self.hir.contract(*id).name)?
                    }
                    ItemId::Struct(id) => {
                        write!(self.buffer, "{}", self.hir.strukt(*id).name)?
                    }
                    ItemId::Enum(id) => {
                        write!(self.buffer, "{}", self.hir.enumm(*id).name)?
                    }
                    ItemId::Udvt(id) => {
                        write!(self.buffer, "{}", self.hir.udvt(*id).name)?
                    }
                    ItemId::Event(id) => {
                        write!(self.buffer, "{}", self.hir.event(*id).name)?
                    }
                    ItemId::Error(id) => {
                        write!(self.buffer, "{}", self.hir.error(*id).name)?
                    }
                },
                Res::Namespace(id) => {
                    write!(self.buffer, "namespace_{}", id.index())?
                }
                Res::Builtin(builtin) => {
                    write!(self.buffer, "{:?}", builtin)?
                }
                Res::Err(_) => {
                    write!(self.buffer, "<error>")?
                }
            },
            ExprKind::Binary(lhs, op, rhs) => {
                write!(self.buffer, "(")?;
                self.print_expr(lhs)?;
                write!(self.buffer, " {op} ")?;
                self.print_expr(rhs)?;
                write!(self.buffer, ")")?;
            }
            ExprKind::Unary(op, expr) => {
                write!(self.buffer, "({op}")?;
                self.print_expr(expr)?;
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
            ExprKind::Member(obj, member) => {
                self.print_expr(obj)?;
                write!(self.buffer, ".{member}")?;
            }
            ExprKind::Index(array, index) => {
                self.print_expr(array)?;
                write!(self.buffer, "[")?;
                if let Some(index) = index {
                    self.print_expr(index)?;
                }
                write!(self.buffer, "]")?;
            }
            ExprKind::Ternary(cond, then, else_) => {
                write!(self.buffer, "(")?;
                self.print_expr(cond)?;
                write!(self.buffer, " ? ")?;
                self.print_expr(then)?;
                write!(self.buffer, " : ")?;
                self.print_expr(else_)?;
                write!(self.buffer, ")")?;
            }
            ExprKind::Assign(lhs, op, rhs) => {
                write!(self.buffer, "(")?;
                self.print_expr(lhs)?;
                if let Some(op) = op {
                    write!(self.buffer, " {op} ")?;
                } else {
                    write!(self.buffer, " = ")?;
                }
                self.print_expr(rhs)?;
                write!(self.buffer, ")")?;
            }
            ExprKind::New(ty) => {
                write!(self.buffer, "new {}", ty)?;
            }
            ExprKind::Delete(expr) => {
                write!(self.buffer, "delete ")?;
                self.print_expr(expr)?;
            }
            ExprKind::Array(elements) => {
                write!(self.buffer, "[")?;
                for (i, element) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(self.buffer, ", ")?;
                    }
                    self.print_expr(element)?;
                }
                write!(self.buffer, "]")?;
            }
            ExprKind::Slice(array, start, end) => {
                self.print_expr(array)?;
                write!(self.buffer, "[")?;
                if let Some(start) = start {
                    self.print_expr(start)?;
                }
                write!(self.buffer, ":")?;
                if let Some(end) = end {
                    self.print_expr(end)?;
                }
                write!(self.buffer, "]")?;
            }
            ExprKind::Payable(expr) => {
                write!(self.buffer, "payable(")?;
                self.print_expr(expr)?;
                write!(self.buffer, ")")?;
            }
            ExprKind::Tuple(elements) => {
                write!(self.buffer, "(")?;
                for (i, element_opt) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(self.buffer, ", ")?;
                    }
                    if let Some(element) = element_opt {
                        self.print_expr(element)?;
                    } else {
                        write!(self.buffer, "_")?;
                    }
                }
                write!(self.buffer, ")")?;
            }
            ExprKind::TypeCall(ty) => {
                write!(self.buffer, "type({})", ty)?;
            }
            ExprKind::Type(ty) => {
                write!(self.buffer, "{}", ty)?;
            }
            ExprKind::Err(_) => {
                write!(self.buffer, "// <error>")?;
            }
        }
        Ok(())
    }

    /// Pretty-prints an item
    pub fn print_item(&mut self, item_id: ItemId) -> fmt::Result {
        match item_id {
            ItemId::Contract(id) => self.print_contract(self.hir.contract(id))?,
            ItemId::Function(id) => self.print_function(self.hir.function(id))?,
            ItemId::Variable(id) => {
                let var = self.hir.variable(id);
                self.write_indent()?;
                if let Some(name) = var.name {
                    self.print_type(&var.ty)?;
                    writeln!(self.buffer, " {};", name)?;
                } else {
                    self.print_type(&var.ty)?;
                    writeln!(self.buffer, ";")?;
                }
            }
            ItemId::Struct(id) => {
                let strukt = self.hir.strukt(id);
                self.write_indent()?;
                writeln!(self.buffer, "struct {} {{", strukt.name)?;
                self.indent();
                for &field_id in strukt.fields {
                    let field = self.hir.variable(field_id);
                    self.write_indent()?;
                    self.print_type(&field.ty)?;
                    if let Some(name) = field.name {
                        writeln!(self.buffer, " {};", name)?;
                    } else {
                        writeln!(self.buffer, ";")?;
                    }
                }
                self.dedent();
                self.write_indent()?;
                writeln!(self.buffer, "}}")?;
            }
            ItemId::Enum(id) => {
                let enumm = self.hir.enumm(id);
                self.write_indent()?;
                writeln!(self.buffer, "enum {} {{", enumm.name)?;
                self.indent();
                for (i, variant) in enumm.variants.iter().enumerate() {
                    self.write_indent()?;
                    if i < enumm.variants.len() - 1 {
                        writeln!(self.buffer, "{},", variant)?;
                    } else {
                        writeln!(self.buffer, "{}", variant)?;
                    }
                }
                self.dedent();
                self.write_indent()?;
                writeln!(self.buffer, "}}")?;
            }
            ItemId::Udvt(id) => {
                let udvt = self.hir.udvt(id);
                self.write_indent()?;
                write!(self.buffer, "type {} = ", udvt.name)?;
                self.print_type(&udvt.ty)?;
                writeln!(self.buffer, ";")?;
            }
            ItemId::Event(id) => {
                let event = self.hir.event(id);
                self.write_indent()?;
                write!(self.buffer, "event {}(", event.name)?;
                for (i, &param_id) in event.parameters.iter().enumerate() {
                    if i > 0 {
                        write!(self.buffer, ", ")?;
                    }
                    let param = self.hir.variable(param_id);
                    if param.indexed {
                        write!(self.buffer, "indexed ")?;
                    }
                    self.print_type(&param.ty)?;
                    if let Some(name) = param.name {
                        write!(self.buffer, " {}", name)?;
                    }
                }
                write!(self.buffer, ")")?;
                if event.anonymous {
                    write!(self.buffer, " anonymous")?;
                }
                writeln!(self.buffer, ";")?;
            }
            ItemId::Error(id) => {
                let error = self.hir.error(id);
                self.write_indent()?;
                write!(self.buffer, "error {}(", error.name)?;
                for (i, &param_id) in error.parameters.iter().enumerate() {
                    if i > 0 {
                        write!(self.buffer, ", ")?;
                    }
                    let param = self.hir.variable(param_id);
                    self.print_type(&param.ty)?;
                    if let Some(name) = param.name {
                        write!(self.buffer, " {}", name)?;
                    }
                }
                writeln!(self.buffer, ");")?;
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

use crate::{
    builtins::Builtin,
    hir::{self, CallArgsKind, ExprKind, ItemId, Res, StmtKind, TypeKind, UsingEntryKind},
    ty::Gcx,
};
use std::fmt;

/// Pretty-prints HIR in a Solidity-like form.
pub struct HirPrinter<'gcx, W = String> {
    gcx: Gcx<'gcx>,
    out: W,
    indent: usize,
    mode: PrintMode,
    ends_with_open: bool,
}

impl<'gcx> HirPrinter<'gcx, String> {
    /// Creates a new HIR printer.
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        Self::with_mode(gcx, String::new(), PrintMode::Full)
    }

    /// Displays an item's declaration header in Solidity syntax.
    pub fn display(gcx: Gcx<'gcx>, item_id: ItemId) -> impl fmt::Display + use<'gcx> {
        fmt::from_fn(move |f| HirPrinter::with_mode(gcx, f, PrintMode::Header).print_item(item_id))
    }

    /// Prints all HIR sources and returns the accumulated output.
    pub fn print_all(mut self) -> String {
        for (id, source) in self.gcx.hir.sources_enumerated() {
            self.print_source(id, source);
        }
        self.finish()
    }

    /// Prints one HIR source.
    pub fn print_source(&mut self, id: hir::SourceId, source: &'gcx hir::Source<'gcx>) {
        self.print_source_inner(id, source).unwrap();
    }

    /// Returns the accumulated output.
    pub fn finish(self) -> String {
        self.out
    }
}

impl<'gcx, W: fmt::Write> HirPrinter<'gcx, W> {
    fn with_mode(gcx: Gcx<'gcx>, out: W, mode: PrintMode) -> Self {
        Self { gcx, out, indent: 0, mode, ends_with_open: false }
    }

    fn print_source_inner(
        &mut self,
        id: hir::SourceId,
        source: &'gcx hir::Source<'gcx>,
    ) -> fmt::Result {
        writeln!(self.out, "source {} \"{}\" {{", id.index(), source.file.name.display())?;
        self.ends_with_open = true;
        self.indent += 1;
        self.print_usings(source.usings)?;
        self.print_items(source.items)?;
        self.indent -= 1;
        writeln!(self.out, "}}")
    }

    fn print_items(&mut self, items: &[ItemId]) -> fmt::Result {
        for (i, &item) in items.iter().enumerate() {
            if i != 0 || !self.ends_with_open {
                self.out.write_char('\n')?;
            }
            self.print_item(item)?;
        }
        Ok(())
    }

    fn print_item(&mut self, item: ItemId) -> fmt::Result {
        match item {
            ItemId::Contract(id) => self.print_contract(id),
            ItemId::Function(id) => self.print_function(id),
            ItemId::Variable(id) => {
                self.write_indent()?;
                self.print_variable(id, VarMode::Item)?;
                if self.mode == PrintMode::Full {
                    self.out.write_str(";\n")?;
                }
                Ok(())
            }
            ItemId::Struct(id) => self.print_struct(id),
            ItemId::Enum(id) => self.print_enum(id),
            ItemId::Udvt(id) => self.print_udvt(id),
            ItemId::Error(id) => self.print_error(id),
            ItemId::Event(id) => self.print_event(id),
        }
    }

    fn print_contract(&mut self, id: hir::ContractId) -> fmt::Result {
        let contract = self.gcx.hir.contract(id);
        self.write_indent()?;
        write!(self.out, "{} {}", contract.kind, contract.name)?;
        if self.mode == PrintMode::Full
            && let Some(layout) = contract.layout
        {
            self.out.write_str(" layout at ")?;
            self.print_expr(layout)?;
        }
        if self.mode == PrintMode::Full && !contract.bases_args.is_empty() {
            self.out.write_str(" is ")?;
            for (i, base) in contract.bases_args.iter().enumerate() {
                if i != 0 {
                    self.out.write_str(", ")?;
                }
                self.print_modifier(base)?;
            }
        } else if !contract.bases.is_empty() {
            self.out.write_str(" is ")?;
            for (i, &base) in contract.bases.iter().enumerate() {
                if i != 0 {
                    self.out.write_str(", ")?;
                }
                self.out.write_str(self.gcx.hir.contract(base).name.as_str())?;
            }
        }
        if self.mode == PrintMode::Header {
            return Ok(());
        }
        self.out.write_str(" {\n")?;
        self.ends_with_open = true;
        self.indent += 1;
        self.print_usings(contract.usings)?;
        self.print_items(contract.items)?;
        self.indent -= 1;
        self.write_indent()?;
        self.out.write_str("}\n")?;
        self.ends_with_open = false;
        Ok(())
    }

    fn print_struct(&mut self, id: hir::StructId) -> fmt::Result {
        let strukt = self.gcx.hir.strukt(id);
        self.write_indent()?;
        write!(self.out, "struct {}", strukt.name)?;
        if self.mode == PrintMode::Header {
            return Ok(());
        }
        self.out.write_str(" {\n")?;
        self.indent += 1;
        for &field in strukt.fields {
            self.write_indent()?;
            self.print_variable(field, VarMode::Parameter)?;
            self.out.write_str(";\n")?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.out.write_str("}\n")
    }

    fn print_enum(&mut self, id: hir::EnumId) -> fmt::Result {
        let enumm = self.gcx.hir.enumm(id);
        self.write_indent()?;
        write!(self.out, "enum {}", enumm.name)?;
        if self.mode == PrintMode::Header {
            return Ok(());
        }
        self.out.write_str(" {")?;
        for (i, &variant) in enumm.variants.iter().enumerate() {
            if i != 0 {
                self.out.write_str(", ")?;
            }
            self.out.write_str(self.gcx.hir.variable(variant).name.unwrap().as_str())?;
        }
        self.out.write_str("}\n")
    }

    fn print_udvt(&mut self, id: hir::UdvtId) -> fmt::Result {
        let udvt = self.gcx.hir.udvt(id);
        self.write_indent()?;
        write!(self.out, "type {} is ", udvt.name)?;
        self.print_ty(&udvt.ty)?;
        if self.mode == PrintMode::Full {
            self.out.write_str(";\n")?;
        }
        Ok(())
    }

    fn print_error(&mut self, id: hir::ErrorId) -> fmt::Result {
        let error = self.gcx.hir.error(id);
        self.write_indent()?;
        write!(self.out, "error {}(", error.name)?;
        self.print_var_list(error.parameters, VarMode::Parameter)?;
        self.out.write_char(')')?;
        if self.mode == PrintMode::Full {
            self.out.write_str(";\n")?;
        }
        Ok(())
    }

    fn print_event(&mut self, id: hir::EventId) -> fmt::Result {
        let event = self.gcx.hir.event(id);
        self.write_indent()?;
        write!(self.out, "event {}(", event.name)?;
        self.print_var_list(event.parameters, VarMode::Parameter)?;
        self.out.write_char(')')?;
        if event.anonymous {
            self.out.write_str(" anonymous")?;
        }
        if self.mode == PrintMode::Full {
            self.out.write_str(";\n")?;
        }
        Ok(())
    }

    fn print_function(&mut self, id: hir::FunctionId) -> fmt::Result {
        let func = self.gcx.hir.function(id);
        if self.mode == PrintMode::Full
            && let Some(gettee) = func.gettee
        {
            self.write_indent()?;
            writeln!(self.out, "// getter for {}", self.var_name(gettee))?;
        }
        self.write_indent()?;
        if func.is_yul {
            self.out.write_str("yul ")?;
        }
        self.out.write_str(func.kind.to_str())?;
        if let Some(name) = func.name {
            write!(self.out, " {name}")?;
        }
        self.out.write_char('(')?;
        self.print_var_list(func.parameters, VarMode::Parameter)?;
        self.out.write_char(')')?;

        if self.mode == PrintMode::Full {
            write!(self.out, " {}", func.visibility)?;
            self.print_state_mutability(func.state_mutability)?;
            for modifier in func.modifiers {
                self.out.write_char(' ')?;
                self.print_modifier(modifier)?;
            }
        } else {
            match func.kind {
                hir::FunctionKind::Function if !func.is_yul => {
                    write!(self.out, " {}", func.visibility)?;
                    self.print_state_mutability(func.state_mutability)?;
                }
                hir::FunctionKind::Function => {}
                hir::FunctionKind::Constructor => {
                    if func.state_mutability == hir::StateMutability::Payable {
                        self.out.write_str(" payable")?;
                    }
                }
                hir::FunctionKind::Fallback | hir::FunctionKind::Receive => {
                    write!(self.out, " {}", func.visibility)?;
                    self.print_state_mutability(func.state_mutability)?;
                }
                hir::FunctionKind::Modifier => {}
            }
        }

        if func.marked_virtual {
            self.out.write_str(" virtual")?;
        }
        if func.override_ {
            self.out.write_str(" override")?;
            self.print_override_list(func.overrides)?;
        }
        if self.mode == PrintMode::Header {
            for modifier in func.modifiers {
                self.out.write_char(' ')?;
                self.print_modifier(modifier)?;
            }
        }
        if !func.returns.is_empty() {
            self.out.write_str(" returns (")?;
            self.print_var_list(func.returns, VarMode::Return)?;
            self.out.write_char(')')?;
        }
        if self.mode == PrintMode::Header {
            return Ok(());
        }
        if let Some(body) = &func.body {
            self.out.write_char(' ')?;
            self.print_block(body)?;
        } else {
            self.out.write_str(";\n")?;
        }
        Ok(())
    }

    fn print_usings(&mut self, usings: &[hir::UsingDirective<'gcx>]) -> fmt::Result {
        for using in usings {
            self.ends_with_open = false;
            self.write_indent()?;
            self.out.write_str("using ")?;
            match using.entries {
                [entry] if matches!(entry.kind, UsingEntryKind::Library(_)) => {
                    self.print_using_entry(entry)?;
                }
                entries => {
                    self.out.write_char('{')?;
                    for (i, entry) in entries.iter().enumerate() {
                        if i != 0 {
                            self.out.write_str(", ")?;
                        }
                        self.print_using_entry(entry)?;
                    }
                    self.out.write_char('}')?;
                }
            }
            self.out.write_str(" for ")?;
            if let Some(ty) = &using.ty {
                self.print_ty(ty)?;
            } else {
                self.out.write_char('*')?;
            }
            if using.global {
                self.out.write_str(" global")?;
            }
            self.out.write_str(";\n")?;
        }
        Ok(())
    }

    fn print_using_entry(&mut self, entry: &hir::UsingEntry<'gcx>) -> fmt::Result {
        match entry.kind {
            UsingEntryKind::Library(id) => {
                self.out.write_str(self.gcx.hir.contract(id).name.as_str())?;
            }
            UsingEntryKind::Functions(ids) => {
                for (i, &id) in ids.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(" | ")?;
                    }
                    self.out.write_str(self.gcx.hir.function(id).name.unwrap().as_str())?;
                }
            }
            UsingEntryKind::Err(_) => self.out.write_str("<error>")?,
        }
        if let Some(op) = entry.operator {
            write!(self.out, " as {}", op.to_str())?;
        }
        Ok(())
    }

    fn print_modifier(&mut self, modifier: &hir::Modifier<'gcx>) -> fmt::Result {
        self.out.write_str(&self.item_name(modifier.id))?;
        if self.mode == PrintMode::Header && !modifier.args.is_dummy() {
            if let Ok(args) = self.gcx.sess.source_map().span_to_snippet(modifier.args.span) {
                self.out.write_str(args.trim())?;
            } else {
                self.out.write_str("(...)")?;
            }
        } else if !modifier.args.is_dummy() || !modifier.args.is_empty() {
            self.print_call_args(&modifier.args)?;
        }
        Ok(())
    }

    fn print_variable(&mut self, id: hir::VariableId, mode: VarMode) -> fmt::Result {
        let var = self.gcx.hir.variable(id);
        self.print_ty(&var.ty)?;
        if self.mode == PrintMode::Header && mode == VarMode::Item {
            if let Some(visibility) = var.visibility {
                write!(self.out, " {visibility}")?;
            }
            if let Some(mutability) = var.mutability {
                write!(self.out, " {mutability}")?;
            }
            if var.override_ {
                self.out.write_str(" override")?;
                self.print_override_list(var.overrides)?;
            }
        }
        if let Some(data_location) = var.data_location {
            write!(self.out, " {data_location}")?;
        }
        if self.mode == PrintMode::Full && mode == VarMode::Item {
            if let Some(visibility) = var.visibility {
                write!(self.out, " {visibility}")?;
            }
            if let Some(mutability) = var.mutability {
                write!(self.out, " {mutability}")?;
            }
            if var.override_ {
                self.out.write_str(" override")?;
                self.print_override_list(var.overrides)?;
            }
        }
        if var.indexed {
            self.out.write_str(" indexed")?;
        }
        if self.mode == PrintMode::Full {
            write!(self.out, " {}", self.var_name(id))?;
        } else if let Some(name) = var.name {
            write!(self.out, " {name}")?;
        }
        if self.mode == PrintMode::Full
            && let Some(initializer) = var.initializer
        {
            self.out.write_str(" = ")?;
            self.print_expr(initializer)?;
        }
        Ok(())
    }

    fn print_var_list(&mut self, vars: &[hir::VariableId], mode: VarMode) -> fmt::Result {
        for (i, &var) in vars.iter().enumerate() {
            if i != 0 {
                self.out.write_str(", ")?;
            }
            self.print_variable(var, mode)?;
        }
        Ok(())
    }

    fn print_block(&mut self, block: &hir::Block<'gcx>) -> fmt::Result {
        self.out.write_str("{\n")?;
        self.indent += 1;
        for stmt in block.stmts {
            self.print_stmt(stmt)?;
        }
        self.indent -= 1;
        self.write_indent()?;
        self.out.write_str("}\n")
    }

    fn print_stmt(&mut self, stmt: &hir::Stmt<'gcx>) -> fmt::Result {
        self.write_indent()?;
        match &stmt.kind {
            StmtKind::DeclSingle(var) => {
                self.print_variable(*var, VarMode::Local)?;
                self.out.write_str(";\n")?;
            }
            StmtKind::DeclMulti(vars, expr) => {
                self.out.write_char('(')?;
                for (i, var) in vars.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(", ")?;
                    }
                    if let Some(var) = var {
                        self.print_variable(*var, VarMode::Local)?;
                    }
                }
                self.out.write_str(") = ")?;
                self.print_expr(expr)?;
                self.out.write_str(";\n")?;
            }
            StmtKind::Block(block) => self.print_block(block)?,
            StmtKind::UncheckedBlock(block) => {
                self.out.write_str("unchecked ")?;
                self.print_block(block)?;
            }
            StmtKind::AssemblyBlock(block) => {
                self.out.write_str("assembly ")?;
                self.print_block(block)?;
            }
            StmtKind::Emit(expr) => {
                self.out.write_str("emit ")?;
                self.print_expr(expr)?;
                self.out.write_str(";\n")?;
            }
            StmtKind::Revert(expr) => {
                self.out.write_str("revert ")?;
                self.print_expr(expr)?;
                self.out.write_str(";\n")?;
            }
            StmtKind::Return(expr) => {
                self.out.write_str("return")?;
                if let Some(expr) = expr {
                    self.out.write_char(' ')?;
                    self.print_expr(expr)?;
                }
                self.out.write_str(";\n")?;
            }
            StmtKind::Break => self.out.write_str("break;\n")?,
            StmtKind::Continue => self.out.write_str("continue;\n")?,
            StmtKind::Loop(block, source) => {
                write!(self.out, "hir.loop({}) ", source.name())?;
                self.print_block(block)?;
            }
            StmtKind::If(cond, then, else_) => {
                self.out.write_str("if (")?;
                self.print_condition(cond)?;
                self.out.write_str(") ")?;
                self.print_stmt_as_block(then)?;
                if let Some(else_) = else_ {
                    self.write_indent()?;
                    self.out.write_str("else ")?;
                    self.print_stmt_as_block(else_)?;
                }
            }
            StmtKind::Switch(switch) => {
                self.out.write_str("switch ")?;
                self.print_expr(switch.selector)?;
                self.out.write_str(" {\n")?;
                self.indent += 1;
                for case in switch.cases {
                    self.write_indent()?;
                    if let Some(lit) = case.constant {
                        write!(self.out, "case {lit} ")?;
                    } else {
                        self.out.write_str("default ")?;
                    }
                    self.print_block(&case.body)?;
                }
                self.indent -= 1;
                self.write_indent()?;
                self.out.write_str("}\n")?;
            }
            StmtKind::Try(try_) => self.print_try(try_)?,
            StmtKind::Expr(expr) => {
                self.print_expr(expr)?;
                self.out.write_str(";\n")?;
            }
            StmtKind::Placeholder => self.out.write_str("_;\n")?,
            StmtKind::Err(_) => self.out.write_str("<error>;\n")?,
        }
        Ok(())
    }

    fn print_stmt_as_block(&mut self, stmt: &hir::Stmt<'gcx>) -> fmt::Result {
        if let StmtKind::Block(block) = &stmt.kind {
            return self.print_block(block);
        }
        self.out.write_str("{\n")?;
        self.indent += 1;
        self.print_stmt(stmt)?;
        self.indent -= 1;
        self.write_indent()?;
        self.out.write_str("}\n")
    }

    fn print_try(&mut self, try_: &hir::StmtTry<'gcx>) -> fmt::Result {
        self.out.write_str("try ")?;
        self.print_expr(&try_.expr)?;
        self.out.write_char(' ')?;
        for (i, clause) in try_.clauses.iter().enumerate() {
            if i != 0 {
                self.write_indent()?;
            }
            match clause.name {
                Some(name) => write!(self.out, "catch {name}(")?,
                None if i == 0 => self.out.write_str("returns (")?,
                None => self.out.write_str("catch (")?,
            }
            self.print_var_list(clause.args, VarMode::Parameter)?;
            self.out.write_str(") ")?;
            self.print_block(&clause.block)?;
        }
        Ok(())
    }

    fn print_expr(&mut self, expr: &hir::Expr<'gcx>) -> fmt::Result {
        match &expr.kind {
            ExprKind::Array(exprs) => {
                self.out.write_char('[')?;
                for (i, expr) in exprs.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(", ")?;
                    }
                    self.print_expr(expr)?;
                }
                self.out.write_char(']')?;
            }
            ExprKind::Assign(lhs, op, rhs) => {
                self.print_expr(lhs)?;
                self.out.write_char(' ')?;
                if let Some(op) = op {
                    self.out.write_str(op.kind.to_str())?;
                }
                self.out.write_str("= ")?;
                self.print_expr(rhs)?;
            }
            ExprKind::Binary(lhs, op, rhs) => {
                self.out.write_char('(')?;
                self.print_expr(lhs)?;
                write!(self.out, " {} ", op.kind.to_str())?;
                self.print_expr(rhs)?;
                self.out.write_char(')')?;
            }
            ExprKind::Call(callee, args, opts) => {
                self.print_expr(callee)?;
                if let Some(opts) = opts {
                    self.out.write_str(" { ")?;
                    for (i, arg) in opts.args.iter().enumerate() {
                        if i != 0 {
                            self.out.write_str(", ")?;
                        }
                        write!(self.out, "{}: ", arg.name)?;
                        self.print_expr(&arg.value)?;
                    }
                    self.out.write_str(" }")?;
                }
                self.print_call_args(args)?;
            }
            ExprKind::Delete(expr) => {
                self.out.write_str("delete ")?;
                self.print_expr(expr)?;
            }
            ExprKind::Ident(res) => self.print_res_list(res)?,
            ExprKind::Index(expr, index) => {
                self.print_expr(expr)?;
                self.out.write_char('[')?;
                if let Some(index) = index {
                    self.print_expr(index)?;
                }
                self.out.write_char(']')?;
            }
            ExprKind::Slice(expr, start, end) => {
                self.print_expr(expr)?;
                self.out.write_char('[')?;
                if let Some(start) = start {
                    self.print_expr(start)?;
                }
                self.out.write_char(':')?;
                if let Some(end) = end {
                    self.print_expr(end)?;
                }
                self.out.write_char(']')?;
            }
            ExprKind::Lit(lit) => write!(self.out, "{lit}")?,
            ExprKind::Member(expr, ident) | ExprKind::YulMember(expr, ident) => {
                self.print_expr(expr)?;
                write!(self.out, ".{ident}")?;
            }
            ExprKind::New(ty) => {
                self.out.write_str("new ")?;
                self.print_ty(ty)?;
            }
            ExprKind::Payable(expr) => {
                self.out.write_str("payable(")?;
                self.print_expr(expr)?;
                self.out.write_char(')')?;
            }
            ExprKind::Ternary(cond, then, else_) => {
                self.out.write_char('(')?;
                self.print_expr(cond)?;
                self.out.write_str(" ? ")?;
                self.print_expr(then)?;
                self.out.write_str(" : ")?;
                self.print_expr(else_)?;
                self.out.write_char(')')?;
            }
            ExprKind::Tuple(exprs) => {
                self.out.write_char('(')?;
                for (i, expr) in exprs.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(", ")?;
                    }
                    if let Some(expr) = expr {
                        self.print_expr(expr)?;
                    }
                }
                self.out.write_char(')')?;
            }
            ExprKind::TypeCall(ty) => {
                self.out.write_str("type(")?;
                self.print_ty(ty)?;
                self.out.write_char(')')?;
            }
            ExprKind::Type(ty) => self.print_ty(ty)?,
            ExprKind::Unary(op, expr) => {
                if op.kind.is_prefix() {
                    self.out.write_str(op.kind.to_str())?;
                    self.print_expr(expr)?;
                } else {
                    self.print_expr(expr)?;
                    self.out.write_str(op.kind.to_str())?;
                }
            }
            ExprKind::Err(_) => self.out.write_str("<error>")?,
        }
        Ok(())
    }

    fn print_condition(&mut self, expr: &hir::Expr<'gcx>) -> fmt::Result {
        if let ExprKind::Binary(lhs, op, rhs) = &expr.kind {
            self.print_expr(lhs)?;
            write!(self.out, " {} ", op.kind.to_str())?;
            self.print_expr(rhs)
        } else {
            self.print_expr(expr)
        }
    }

    fn print_call_args(&mut self, args: &hir::CallArgs<'gcx>) -> fmt::Result {
        match args.kind {
            CallArgsKind::Unnamed(exprs) => {
                self.out.write_char('(')?;
                for (i, expr) in exprs.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(", ")?;
                    }
                    self.print_expr(expr)?;
                }
                self.out.write_char(')')?;
            }
            CallArgsKind::Named(args) => {
                self.out.write_str("({")?;
                for (i, arg) in args.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(", ")?;
                    }
                    write!(self.out, "{}: ", arg.name)?;
                    self.print_expr(&arg.value)?;
                }
                self.out.write_str("})")?;
            }
        }
        Ok(())
    }

    fn print_res_list(&mut self, res: &[Res]) -> fmt::Result {
        match res {
            [] => self.out.write_str("<unresolved>")?,
            [res] => {
                let label = self.res_name(*res);
                self.out.write_str(&label)?;
            }
            many => {
                self.out.write_str("overload(")?;
                for (i, &res) in many.iter().enumerate() {
                    if i != 0 {
                        self.out.write_str(" | ")?;
                    }
                    let label = self.res_name(res);
                    self.out.write_str(&label)?;
                }
                self.out.write_char(')')?;
            }
        }
        Ok(())
    }

    fn print_ty(&mut self, ty: &hir::Type<'gcx>) -> fmt::Result {
        match &ty.kind {
            TypeKind::Elementary(ty) => write!(self.out, "{ty}")?,
            TypeKind::Array(arr) => {
                self.print_ty(&arr.element)?;
                self.out.write_char('[')?;
                if let Some(size) = arr.size {
                    if self.mode == PrintMode::Header {
                        if let Ok(size) = self.gcx.sess.source_map().span_to_snippet(size.span) {
                            self.out.write_str(size.trim())?;
                        } else {
                            self.out.write_str("<error>")?;
                        }
                    } else {
                        self.print_expr(size)?;
                    }
                }
                self.out.write_char(']')?;
            }
            TypeKind::Function(func) => {
                self.out.write_str("function(")?;
                self.print_var_list(func.parameters, VarMode::Parameter)?;
                self.out.write_char(')')?;
                write!(self.out, " {}", func.visibility)?;
                self.print_state_mutability(func.state_mutability)?;
                if !func.returns.is_empty() {
                    self.out.write_str(" returns (")?;
                    self.print_var_list(func.returns, VarMode::Return)?;
                    self.out.write_char(')')?;
                }
            }
            TypeKind::Mapping(map) => {
                self.out.write_str("mapping(")?;
                self.print_ty(&map.key)?;
                if let Some(name) = map.key_name {
                    write!(self.out, " {name}")?;
                }
                self.out.write_str(" => ")?;
                self.print_ty(&map.value)?;
                if let Some(name) = map.value_name {
                    write!(self.out, " {name}")?;
                }
                self.out.write_char(')')?;
            }
            TypeKind::Custom(item) => {
                let label = self.item_name(*item);
                self.out.write_str(&label)?;
            }
            TypeKind::Err(_) => self.out.write_str("<error>")?,
        }
        Ok(())
    }

    fn print_state_mutability(&mut self, state_mutability: hir::StateMutability) -> fmt::Result {
        if state_mutability != hir::StateMutability::NonPayable {
            write!(self.out, " {state_mutability}")?;
        }
        Ok(())
    }

    fn print_override_list(&mut self, overrides: &[hir::ContractId]) -> fmt::Result {
        if overrides.is_empty() {
            return Ok(());
        }
        self.out.write_char('(')?;
        for (i, &contract) in overrides.iter().enumerate() {
            if i != 0 {
                self.out.write_str(", ")?;
            }
            self.out.write_str(self.gcx.hir.contract(contract).name.as_str())?;
        }
        self.out.write_char(')')
    }

    fn res_name(&self, res: Res) -> String {
        match res {
            Res::Item(item) => self.item_name(item),
            Res::Namespace(source) => {
                format!("namespace({})", self.gcx.hir.source(source).file.name.display())
            }
            Res::Builtin(builtin) => builtin_name(builtin).to_string(),
            Res::Err(_) => "<error>".to_string(),
        }
    }

    fn item_name(&self, item: ItemId) -> String {
        self.gcx.item_name_opt(item).map(|name| name.to_string()).unwrap_or_else(|| {
            match self.mode {
                PrintMode::Full => self.synthetic_item_name(item),
                PrintMode::Header => "<error>".to_string(),
            }
        })
    }

    fn synthetic_item_name(&self, item: ItemId) -> String {
        match item {
            ItemId::Contract(id) => format!("_contract{}", id.index()),
            ItemId::Function(id) => format!("_function{}", id.index()),
            ItemId::Variable(id) => self.synthetic_var_name(id),
            ItemId::Struct(id) => format!("_struct{}", id.index()),
            ItemId::Enum(id) => format!("_enum{}", id.index()),
            ItemId::Udvt(id) => format!("_udvt{}", id.index()),
            ItemId::Error(id) => format!("_error{}", id.index()),
            ItemId::Event(id) => format!("_event{}", id.index()),
        }
    }

    fn var_name(&self, id: hir::VariableId) -> String {
        self.gcx
            .hir
            .variable(id)
            .name
            .map(|name| name.to_string())
            .unwrap_or_else(|| self.synthetic_var_name(id))
    }

    fn synthetic_var_name(&self, id: hir::VariableId) -> String {
        format!("_var{}", id.index())
    }

    fn write_indent(&mut self) -> fmt::Result {
        for _ in 0..self.indent {
            self.out.write_str("    ")?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum PrintMode {
    Full,
    Header,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum VarMode {
    Item,
    Parameter,
    Return,
    Local,
}

fn builtin_name(builtin: Builtin) -> impl fmt::Display {
    solar_data_structures::fmt::from_fn(move |f| f.write_str(builtin.name().as_str()))
}

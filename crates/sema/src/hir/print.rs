use crate::{
    builtins::Builtin,
    hir::{self, CallArgsKind, ExprKind, ItemId, Res, StmtKind, TypeKind, UsingEntryKind},
    ty::Gcx,
};
use std::fmt::{self, Write};

/// Pretty-prints HIR in a Solidity-like form.
pub struct HirPrinter<'gcx> {
    gcx: Gcx<'gcx>,
    out: String,
    indent: usize,
}

impl<'gcx> HirPrinter<'gcx> {
    /// Creates a new HIR printer.
    pub fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx, out: String::new(), indent: 0 }
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
        writeln!(self.out, "source {} \"{}\" {{", id.index(), source.file.name.display()).unwrap();
        self.indent += 1;
        self.print_usings(source.usings);
        self.print_items(source.items);
        self.indent -= 1;
        writeln!(self.out, "}}").unwrap();
    }

    /// Returns the accumulated output.
    pub fn finish(self) -> String {
        self.out
    }

    fn print_items(&mut self, items: &[ItemId]) {
        for (i, &item) in items.iter().enumerate() {
            if i != 0 || !self.ends_with_open_source() {
                self.out.push('\n');
            }
            self.print_item(item);
        }
    }

    fn print_item(&mut self, item: ItemId) {
        match item {
            ItemId::Contract(id) => self.print_contract(id),
            ItemId::Function(id) => self.print_function(id),
            ItemId::Variable(id) => {
                self.write_indent();
                self.print_variable(id, VarMode::Item);
                self.out.push_str(";\n");
            }
            ItemId::Struct(id) => self.print_struct(id),
            ItemId::Enum(id) => self.print_enum(id),
            ItemId::Udvt(id) => self.print_udvt(id),
            ItemId::Error(id) => self.print_error(id),
            ItemId::Event(id) => self.print_event(id),
        }
    }

    fn print_contract(&mut self, id: hir::ContractId) {
        let contract = self.gcx.hir.contract(id);
        self.write_indent();
        write!(self.out, "{} {}", contract.kind, contract.name).unwrap();
        if let Some(layout) = contract.layout {
            self.out.push_str(" layout at ");
            self.print_expr(layout);
        }
        if !contract.bases_args.is_empty() {
            self.out.push_str(" is ");
            for (i, base) in contract.bases_args.iter().enumerate() {
                if i != 0 {
                    self.out.push_str(", ");
                }
                self.print_modifier(base);
            }
        } else if !contract.bases.is_empty() {
            self.out.push_str(" is ");
            for (i, &base) in contract.bases.iter().enumerate() {
                if i != 0 {
                    self.out.push_str(", ");
                }
                self.out.push_str(self.gcx.hir.contract(base).name.as_str());
            }
        }
        self.out.push_str(" {\n");
        self.indent += 1;
        self.print_usings(contract.usings);
        self.print_items(contract.items);
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("}\n");
    }

    fn print_struct(&mut self, id: hir::StructId) {
        let strukt = self.gcx.hir.strukt(id);
        self.write_indent();
        writeln!(self.out, "struct {} {{", strukt.name).unwrap();
        self.indent += 1;
        for &field in strukt.fields {
            self.write_indent();
            self.print_variable(field, VarMode::Parameter);
            self.out.push_str(";\n");
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("}\n");
    }

    fn print_enum(&mut self, id: hir::EnumId) {
        let enumm = self.gcx.hir.enumm(id);
        self.write_indent();
        write!(self.out, "enum {} {{", enumm.name).unwrap();
        for (i, &variant) in enumm.variants.iter().enumerate() {
            if i != 0 {
                self.out.push_str(", ");
            }
            self.out.push_str(self.gcx.hir.variable(variant).name.unwrap().as_str());
        }
        self.out.push_str("}\n");
    }

    fn print_udvt(&mut self, id: hir::UdvtId) {
        let udvt = self.gcx.hir.udvt(id);
        self.write_indent();
        write!(self.out, "type {} is ", udvt.name).unwrap();
        self.print_ty(&udvt.ty);
        self.out.push_str(";\n");
    }

    fn print_error(&mut self, id: hir::ErrorId) {
        let error = self.gcx.hir.error(id);
        self.write_indent();
        write!(self.out, "error {}(", error.name).unwrap();
        self.print_var_list(error.parameters, VarMode::Parameter);
        self.out.push_str(");\n");
    }

    fn print_event(&mut self, id: hir::EventId) {
        let event = self.gcx.hir.event(id);
        self.write_indent();
        write!(self.out, "event {}(", event.name).unwrap();
        self.print_var_list(event.parameters, VarMode::Parameter);
        self.out.push(')');
        if event.anonymous {
            self.out.push_str(" anonymous");
        }
        self.out.push_str(";\n");
    }

    fn print_function(&mut self, id: hir::FunctionId) {
        let func = self.gcx.hir.function(id);
        if let Some(gettee) = func.gettee {
            self.write_indent();
            writeln!(self.out, "// getter for {}", self.var_name(gettee)).unwrap();
        }
        self.write_indent();
        if func.is_yul {
            self.out.push_str("yul ");
        }
        self.out.push_str(func.kind.to_str());
        if let Some(name) = func.name {
            write!(self.out, " {name}").unwrap();
        }
        self.out.push('(');
        self.print_var_list(func.parameters, VarMode::Parameter);
        self.out.push(')');
        write!(self.out, " {}", func.visibility).unwrap();
        if func.state_mutability != hir::StateMutability::NonPayable {
            write!(self.out, " {}", func.state_mutability).unwrap();
        }
        for modifier in func.modifiers {
            self.out.push(' ');
            self.print_modifier(modifier);
        }
        if func.marked_virtual {
            self.out.push_str(" virtual");
        }
        if func.override_ {
            self.out.push_str(" override");
            if !func.overrides.is_empty() {
                self.out.push('(');
                for (i, &contract) in func.overrides.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    self.out.push_str(self.gcx.hir.contract(contract).name.as_str());
                }
                self.out.push(')');
            }
        }
        if !func.returns.is_empty() {
            self.out.push_str(" returns (");
            self.print_var_list(func.returns, VarMode::Return);
            self.out.push(')');
        }
        if let Some(body) = &func.body {
            self.out.push(' ');
            self.print_block(body);
        } else {
            self.out.push_str(";\n");
        }
    }

    fn print_usings(&mut self, usings: &[hir::UsingDirective<'gcx>]) {
        for using in usings {
            self.write_indent();
            self.out.push_str("using ");
            match using.entries {
                [entry] if matches!(entry.kind, UsingEntryKind::Library(_)) => {
                    self.print_using_entry(entry);
                }
                entries => {
                    self.out.push('{');
                    for (i, entry) in entries.iter().enumerate() {
                        if i != 0 {
                            self.out.push_str(", ");
                        }
                        self.print_using_entry(entry);
                    }
                    self.out.push('}');
                }
            }
            self.out.push_str(" for ");
            if let Some(ty) = &using.ty {
                self.print_ty(ty);
            } else {
                self.out.push('*');
            }
            if using.global {
                self.out.push_str(" global");
            }
            self.out.push_str(";\n");
        }
    }

    fn print_using_entry(&mut self, entry: &hir::UsingEntry<'gcx>) {
        match entry.kind {
            UsingEntryKind::Library(id) => {
                self.out.push_str(self.gcx.hir.contract(id).name.as_str());
            }
            UsingEntryKind::Functions(ids) => {
                for (i, &id) in ids.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(" | ");
                    }
                    self.out.push_str(self.gcx.hir.function(id).name.unwrap().as_str());
                }
            }
            UsingEntryKind::Err(_) => self.out.push_str("<error>"),
        }
        if let Some(op) = entry.operator {
            write!(self.out, " as {}", op.to_str()).unwrap();
        }
    }

    fn print_modifier(&mut self, modifier: &hir::Modifier<'gcx>) {
        self.out.push_str(&self.item_name(modifier.id));
        if !modifier.args.is_dummy() || !modifier.args.is_empty() {
            self.print_call_args(&modifier.args);
        }
    }

    fn print_variable(&mut self, id: hir::VariableId, mode: VarMode) {
        let var = self.gcx.hir.variable(id);
        self.print_ty(&var.ty);
        if let Some(data_location) = var.data_location {
            write!(self.out, " {data_location}").unwrap();
        }
        if mode == VarMode::Item {
            if let Some(visibility) = var.visibility {
                write!(self.out, " {visibility}").unwrap();
            }
            if let Some(mutability) = var.mutability {
                write!(self.out, " {mutability}").unwrap();
            }
            if var.override_ {
                self.out.push_str(" override");
                if !var.overrides.is_empty() {
                    self.out.push('(');
                    for (i, &contract) in var.overrides.iter().enumerate() {
                        if i != 0 {
                            self.out.push_str(", ");
                        }
                        self.out.push_str(self.gcx.hir.contract(contract).name.as_str());
                    }
                    self.out.push(')');
                }
            }
        }
        if var.indexed {
            self.out.push_str(" indexed");
        }
        write!(self.out, " {}", self.var_name(id)).unwrap();
        if let Some(initializer) = var.initializer {
            self.out.push_str(" = ");
            self.print_expr(initializer);
        }
    }

    fn print_var_list(&mut self, vars: &[hir::VariableId], mode: VarMode) {
        for (i, &var) in vars.iter().enumerate() {
            if i != 0 {
                self.out.push_str(", ");
            }
            self.print_variable(var, mode);
        }
    }

    fn print_block(&mut self, block: &hir::Block<'gcx>) {
        self.out.push_str("{\n");
        self.indent += 1;
        for stmt in block.stmts {
            self.print_stmt(stmt);
        }
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("}\n");
    }

    fn print_stmt(&mut self, stmt: &hir::Stmt<'gcx>) {
        self.write_indent();
        match &stmt.kind {
            StmtKind::DeclSingle(var) => {
                self.print_variable(*var, VarMode::Local);
                self.out.push_str(";\n");
            }
            StmtKind::DeclMulti(vars, expr) => {
                self.out.push('(');
                for (i, var) in vars.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    if let Some(var) = var {
                        self.print_variable(*var, VarMode::Local);
                    }
                }
                self.out.push_str(") = ");
                self.print_expr(expr);
                self.out.push_str(";\n");
            }
            StmtKind::Block(block) => self.print_block(block),
            StmtKind::UncheckedBlock(block) => {
                self.out.push_str("unchecked ");
                self.print_block(block);
            }
            StmtKind::AssemblyBlock(block) => {
                self.out.push_str("assembly ");
                self.print_block(block);
            }
            StmtKind::Emit(expr) => {
                self.out.push_str("emit ");
                self.print_expr(expr);
                self.out.push_str(";\n");
            }
            StmtKind::Revert(expr) => {
                self.out.push_str("revert ");
                self.print_expr(expr);
                self.out.push_str(";\n");
            }
            StmtKind::Return(expr) => {
                self.out.push_str("return");
                if let Some(expr) = expr {
                    self.out.push(' ');
                    self.print_expr(expr);
                }
                self.out.push_str(";\n");
            }
            StmtKind::Break => self.out.push_str("break;\n"),
            StmtKind::Continue => self.out.push_str("continue;\n"),
            StmtKind::Loop(block, source) => {
                write!(self.out, "hir.loop({}) ", source.name()).unwrap();
                self.print_block(block);
            }
            StmtKind::If(cond, then, else_) => {
                self.out.push_str("if (");
                self.print_condition(cond);
                self.out.push_str(") ");
                self.print_stmt_as_block(then);
                if let Some(else_) = else_ {
                    self.write_indent();
                    self.out.push_str("else ");
                    self.print_stmt_as_block(else_);
                }
            }
            StmtKind::Switch(switch) => {
                self.out.push_str("switch ");
                self.print_expr(switch.selector);
                self.out.push_str(" {\n");
                self.indent += 1;
                for case in switch.cases {
                    self.write_indent();
                    if let Some(lit) = case.constant {
                        write!(self.out, "case {lit} ").unwrap();
                    } else {
                        self.out.push_str("default ");
                    }
                    self.print_block(&case.body);
                }
                self.indent -= 1;
                self.write_indent();
                self.out.push_str("}\n");
            }
            StmtKind::Try(try_) => self.print_try(try_),
            StmtKind::Expr(expr) => {
                self.print_expr(expr);
                self.out.push_str(";\n");
            }
            StmtKind::Placeholder => self.out.push_str("_;\n"),
            StmtKind::Err(_) => self.out.push_str("<error>;\n"),
        }
    }

    fn print_stmt_as_block(&mut self, stmt: &hir::Stmt<'gcx>) {
        if let StmtKind::Block(block) = &stmt.kind {
            self.print_block(block);
            return;
        }
        self.out.push_str("{\n");
        self.indent += 1;
        self.print_stmt(stmt);
        self.indent -= 1;
        self.write_indent();
        self.out.push_str("}\n");
    }

    fn print_try(&mut self, try_: &hir::StmtTry<'gcx>) {
        self.out.push_str("try ");
        self.print_expr(&try_.expr);
        self.out.push(' ');
        for (i, clause) in try_.clauses.iter().enumerate() {
            if i != 0 {
                self.write_indent();
            }
            match clause.name {
                Some(name) => write!(self.out, "catch {name}(").unwrap(),
                None if i == 0 => self.out.push_str("returns ("),
                None => self.out.push_str("catch ("),
            }
            self.print_var_list(clause.args, VarMode::Parameter);
            self.out.push_str(") ");
            self.print_block(&clause.block);
        }
    }

    fn print_expr(&mut self, expr: &hir::Expr<'gcx>) {
        match &expr.kind {
            ExprKind::Array(exprs) => {
                self.out.push('[');
                for (i, expr) in exprs.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    self.print_expr(expr);
                }
                self.out.push(']');
            }
            ExprKind::Assign(lhs, op, rhs) => {
                self.print_expr(lhs);
                self.out.push(' ');
                if let Some(op) = op {
                    self.out.push_str(op.kind.to_str());
                }
                self.out.push_str("= ");
                self.print_expr(rhs);
            }
            ExprKind::Binary(lhs, op, rhs) => {
                self.out.push('(');
                self.print_expr(lhs);
                write!(self.out, " {} ", op.kind.to_str()).unwrap();
                self.print_expr(rhs);
                self.out.push(')');
            }
            ExprKind::Call(callee, args, opts) => {
                self.print_expr(callee);
                if let Some(opts) = opts {
                    self.out.push_str(" { ");
                    for (i, arg) in opts.args.iter().enumerate() {
                        if i != 0 {
                            self.out.push_str(", ");
                        }
                        write!(self.out, "{}: ", arg.name).unwrap();
                        self.print_expr(&arg.value);
                    }
                    self.out.push_str(" }");
                }
                self.print_call_args(args);
            }
            ExprKind::Delete(expr) => {
                self.out.push_str("delete ");
                self.print_expr(expr);
            }
            ExprKind::Ident(res) => self.print_res_list(res),
            ExprKind::Index(expr, index) => {
                self.print_expr(expr);
                self.out.push('[');
                if let Some(index) = index {
                    self.print_expr(index);
                }
                self.out.push(']');
            }
            ExprKind::Slice(expr, start, end) => {
                self.print_expr(expr);
                self.out.push('[');
                if let Some(start) = start {
                    self.print_expr(start);
                }
                self.out.push(':');
                if let Some(end) = end {
                    self.print_expr(end);
                }
                self.out.push(']');
            }
            ExprKind::Lit(lit) => write!(self.out, "{lit}").unwrap(),
            ExprKind::Member(expr, ident) | ExprKind::YulMember(expr, ident) => {
                self.print_expr(expr);
                write!(self.out, ".{ident}").unwrap();
            }
            ExprKind::New(ty) => {
                self.out.push_str("new ");
                self.print_ty(ty);
            }
            ExprKind::Payable(expr) => {
                self.out.push_str("payable(");
                self.print_expr(expr);
                self.out.push(')');
            }
            ExprKind::Ternary(cond, then, else_) => {
                self.out.push('(');
                self.print_expr(cond);
                self.out.push_str(" ? ");
                self.print_expr(then);
                self.out.push_str(" : ");
                self.print_expr(else_);
                self.out.push(')');
            }
            ExprKind::Tuple(exprs) => {
                self.out.push('(');
                for (i, expr) in exprs.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    if let Some(expr) = expr {
                        self.print_expr(expr);
                    }
                }
                self.out.push(')');
            }
            ExprKind::TypeCall(ty) => {
                self.out.push_str("type(");
                self.print_ty(ty);
                self.out.push(')');
            }
            ExprKind::Type(ty) => self.print_ty(ty),
            ExprKind::Unary(op, expr) => {
                if op.kind.is_prefix() {
                    self.out.push_str(op.kind.to_str());
                    self.print_expr(expr);
                } else {
                    self.print_expr(expr);
                    self.out.push_str(op.kind.to_str());
                }
            }
            ExprKind::Err(_) => self.out.push_str("<error>"),
        }
    }

    fn print_condition(&mut self, expr: &hir::Expr<'gcx>) {
        if let ExprKind::Binary(lhs, op, rhs) = &expr.kind {
            self.print_expr(lhs);
            write!(self.out, " {} ", op.kind.to_str()).unwrap();
            self.print_expr(rhs);
        } else {
            self.print_expr(expr);
        }
    }

    fn print_call_args(&mut self, args: &hir::CallArgs<'gcx>) {
        match args.kind {
            CallArgsKind::Unnamed(exprs) => {
                self.out.push('(');
                for (i, expr) in exprs.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    self.print_expr(expr);
                }
                self.out.push(')');
            }
            CallArgsKind::Named(args) => {
                self.out.push_str("({");
                for (i, arg) in args.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(", ");
                    }
                    write!(self.out, "{}: ", arg.name).unwrap();
                    self.print_expr(&arg.value);
                }
                self.out.push_str("})");
            }
        }
    }

    fn print_res_list(&mut self, res: &[Res]) {
        match res {
            [] => self.out.push_str("<unresolved>"),
            [res] => {
                let label = self.res_name(*res);
                self.out.push_str(&label);
            }
            many => {
                self.out.push_str("overload(");
                for (i, &res) in many.iter().enumerate() {
                    if i != 0 {
                        self.out.push_str(" | ");
                    }
                    let label = self.res_name(res);
                    self.out.push_str(&label);
                }
                self.out.push(')');
            }
        }
    }

    fn print_ty(&mut self, ty: &hir::Type<'gcx>) {
        match &ty.kind {
            TypeKind::Elementary(ty) => write!(self.out, "{ty}").unwrap(),
            TypeKind::Array(arr) => {
                self.print_ty(&arr.element);
                self.out.push('[');
                if let Some(size) = arr.size {
                    self.print_expr(size);
                }
                self.out.push(']');
            }
            TypeKind::Function(func) => {
                self.out.push_str("function(");
                self.print_var_list(func.parameters, VarMode::Parameter);
                self.out.push(')');
                write!(self.out, " {}", func.visibility).unwrap();
                if func.state_mutability != hir::StateMutability::NonPayable {
                    write!(self.out, " {}", func.state_mutability).unwrap();
                }
                if !func.returns.is_empty() {
                    self.out.push_str(" returns (");
                    self.print_var_list(func.returns, VarMode::Return);
                    self.out.push(')');
                }
            }
            TypeKind::Mapping(map) => {
                self.out.push_str("mapping(");
                self.print_ty(&map.key);
                if let Some(name) = map.key_name {
                    write!(self.out, " {name}").unwrap();
                }
                self.out.push_str(" => ");
                self.print_ty(&map.value);
                if let Some(name) = map.value_name {
                    write!(self.out, " {name}").unwrap();
                }
                self.out.push(')');
            }
            TypeKind::Custom(item) => {
                let label = self.item_name(*item);
                self.out.push_str(&label);
            }
            TypeKind::Err(_) => self.out.push_str("<error>"),
        }
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
        self.gcx
            .item_name_opt(item)
            .map(|name| name.to_string())
            .unwrap_or_else(|| self.synthetic_item_name(item))
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

    fn write_indent(&mut self) {
        for _ in 0..self.indent {
            self.out.push_str("    ");
        }
    }

    fn ends_with_open_source(&self) -> bool {
        self.out.ends_with("{\n")
    }
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

impl ItemId {
    /// Displays this item's declaration header in Solidity syntax.
    pub fn display<'gcx>(self, gcx: Gcx<'gcx>) -> impl fmt::Display + use<'gcx> {
        fmt::from_fn(move |f| HirPrinter::new(gcx).fmt_item(self, f))
    }
}

impl HirPrinter<'_> {
    fn fmt_item<W: fmt::Write>(&self, item_id: ItemId, buf: &mut W) -> fmt::Result {
        match item_id {
            ItemId::Contract(id) => self.fmt_contract(id, buf),
            ItemId::Function(id) => self.fmt_function(id, buf),
            ItemId::Variable(id) => self.fmt_variable(id, buf),
            ItemId::Struct(id) => self.fmt_struct(id, buf),
            ItemId::Enum(id) => self.fmt_enum(id, buf),
            ItemId::Udvt(id) => self.fmt_udvt(id, buf),
            ItemId::Error(id) => self.fmt_error(id, buf),
            ItemId::Event(id) => self.fmt_event(id, buf),
        }
    }

    fn fmt_contract<W: fmt::Write>(&self, id: hir::ContractId, buf: &mut W) -> fmt::Result {
        let contract = self.gcx.hir.contract(id);
        write!(buf, "{} {}", contract.kind, contract.name)?;
        if let Some((&first, rest)) = contract.bases.split_first() {
            write!(buf, " is {}", self.gcx.hir.contract(first).name)?;
            for &base in rest {
                write!(buf, ", {}", self.gcx.hir.contract(base).name)?;
            }
        }
        Ok(())
    }

    fn fmt_function<W: fmt::Write>(&self, id: hir::FunctionId, buf: &mut W) -> fmt::Result {
        let function = self.gcx.hir.function(id);
        if function.is_yul {
            buf.write_str("yul ")?;
        }

        write!(buf, "{}", function.kind)?;
        if let Some(name) = function.name {
            write!(buf, " {name}")?;
        }
        buf.write_char('(')?;
        self.fmt_variables(function.parameters, buf)?;
        buf.write_char(')')?;

        match function.kind {
            hir::FunctionKind::Function if !function.is_yul => {
                write!(buf, " {}", function.visibility)?;
                self.fmt_state_mutability(function.state_mutability, buf)?;
            }
            hir::FunctionKind::Function => {}
            hir::FunctionKind::Constructor => {
                if function.state_mutability == hir::StateMutability::Payable {
                    buf.write_str(" payable")?;
                }
            }
            hir::FunctionKind::Fallback | hir::FunctionKind::Receive => {
                write!(buf, " {}", function.visibility)?;
                self.fmt_state_mutability(function.state_mutability, buf)?;
            }
            hir::FunctionKind::Modifier => {}
        }

        if function.marked_virtual {
            buf.write_str(" virtual")?;
        }
        if function.override_ {
            buf.write_str(" override")?;
            self.fmt_override_list(function.overrides, buf)?;
        }
        for modifier in function.modifiers {
            buf.write_char(' ')?;
            self.fmt_item_name(modifier.id, buf)?;
            if !modifier.args.is_dummy() {
                if let Ok(args) = self.gcx.sess.source_map().span_to_snippet(modifier.args.span) {
                    buf.write_str(args.trim())?;
                } else {
                    buf.write_str("(...)")?;
                }
            }
        }
        if !function.returns.is_empty() {
            buf.write_str(" returns (")?;
            self.fmt_variables(function.returns, buf)?;
            buf.write_char(')')?;
        }
        Ok(())
    }

    fn fmt_variable<W: fmt::Write>(&self, id: hir::VariableId, buf: &mut W) -> fmt::Result {
        let variable = self.gcx.hir.variable(id);
        self.fmt_ty(&variable.ty, buf)?;
        if let Some(visibility) = variable.visibility {
            write!(buf, " {visibility}")?;
        }
        if let Some(mutability) = variable.mutability {
            write!(buf, " {mutability}")?;
        }
        if variable.override_ {
            buf.write_str(" override")?;
            self.fmt_override_list(variable.overrides, buf)?;
        }
        if let Some(data_location) = variable.data_location {
            write!(buf, " {data_location}")?;
        }
        if variable.indexed {
            buf.write_str(" indexed")?;
        }
        if let Some(name) = variable.name {
            write!(buf, " {name}")?;
        }
        Ok(())
    }

    fn fmt_struct<W: fmt::Write>(&self, id: hir::StructId, buf: &mut W) -> fmt::Result {
        write!(buf, "struct {}", self.gcx.hir.strukt(id).name)
    }

    fn fmt_enum<W: fmt::Write>(&self, id: hir::EnumId, buf: &mut W) -> fmt::Result {
        write!(buf, "enum {}", self.gcx.hir.enumm(id).name)
    }

    fn fmt_udvt<W: fmt::Write>(&self, id: hir::UdvtId, buf: &mut W) -> fmt::Result {
        let udvt = self.gcx.hir.udvt(id);
        write!(buf, "type {} is ", udvt.name)?;
        self.fmt_ty(&udvt.ty, buf)
    }

    fn fmt_event<W: fmt::Write>(&self, id: hir::EventId, buf: &mut W) -> fmt::Result {
        let event = self.gcx.hir.event(id);
        write!(buf, "event {}(", event.name)?;
        self.fmt_variables(event.parameters, buf)?;
        buf.write_char(')')?;
        if event.anonymous {
            buf.write_str(" anonymous")?;
        }
        Ok(())
    }

    fn fmt_error<W: fmt::Write>(&self, id: hir::ErrorId, buf: &mut W) -> fmt::Result {
        let error = self.gcx.hir.error(id);
        write!(buf, "error {}(", error.name)?;
        self.fmt_variables(error.parameters, buf)?;
        buf.write_char(')')
    }

    fn fmt_variables<W: fmt::Write>(
        &self,
        variables: &[hir::VariableId],
        buf: &mut W,
    ) -> fmt::Result {
        for (index, &id) in variables.iter().enumerate() {
            if index != 0 {
                buf.write_str(", ")?;
            }
            let variable = self.gcx.hir.variable(id);
            self.fmt_ty(&variable.ty, buf)?;
            if let Some(data_location) = variable.data_location {
                write!(buf, " {data_location}")?;
            }
            if variable.indexed {
                buf.write_str(" indexed")?;
            }
            if let Some(name) = variable.name {
                write!(buf, " {name}")?;
            }
        }
        Ok(())
    }

    fn fmt_ty<W: fmt::Write>(&self, ty: &hir::Type<'_>, buf: &mut W) -> fmt::Result {
        match &ty.kind {
            TypeKind::Elementary(elementary) => write!(buf, "{elementary}"),
            TypeKind::Array(array) => {
                self.fmt_ty(&array.element, buf)?;
                buf.write_char('[')?;
                if let Some(size) = array.size {
                    if let Ok(size) = self.gcx.sess.source_map().span_to_snippet(size.span) {
                        buf.write_str(size.trim())?;
                    } else {
                        buf.write_str("<error>")?;
                    }
                }
                buf.write_char(']')
            }
            TypeKind::Function(function) => {
                buf.write_str("function(")?;
                self.fmt_variables(function.parameters, buf)?;
                write!(buf, ") {}", function.visibility)?;
                self.fmt_state_mutability(function.state_mutability, buf)?;
                if !function.returns.is_empty() {
                    buf.write_str(" returns (")?;
                    self.fmt_variables(function.returns, buf)?;
                    buf.write_char(')')?;
                }
                Ok(())
            }
            TypeKind::Mapping(mapping) => {
                buf.write_str("mapping(")?;
                self.fmt_ty(&mapping.key, buf)?;
                if let Some(name) = mapping.key_name {
                    write!(buf, " {name}")?;
                }
                buf.write_str(" => ")?;
                self.fmt_ty(&mapping.value, buf)?;
                if let Some(name) = mapping.value_name {
                    write!(buf, " {name}")?;
                }
                buf.write_char(')')
            }
            TypeKind::Custom(item_id) => self.fmt_item_name(*item_id, buf),
            TypeKind::Err(_) => buf.write_str("<error>"),
        }
    }

    fn fmt_state_mutability<W: fmt::Write>(
        &self,
        state_mutability: hir::StateMutability,
        buf: &mut W,
    ) -> fmt::Result {
        if state_mutability != hir::StateMutability::NonPayable {
            write!(buf, " {state_mutability}")?;
        }
        Ok(())
    }

    fn fmt_override_list<W: fmt::Write>(
        &self,
        overrides: &[hir::ContractId],
        buf: &mut W,
    ) -> fmt::Result {
        if overrides.is_empty() {
            return Ok(());
        }
        buf.write_char('(')?;
        for (index, &contract) in overrides.iter().enumerate() {
            if index != 0 {
                buf.write_str(", ")?;
            }
            self.fmt_item_name(contract.into(), buf)?;
        }
        buf.write_char(')')
    }

    fn fmt_item_name<W: fmt::Write>(&self, item_id: ItemId, buf: &mut W) -> fmt::Result {
        if let Some(name) = self.gcx.item_name_opt(item_id) {
            write!(buf, "{name}")
        } else {
            buf.write_str("<error>")
        }
    }
}

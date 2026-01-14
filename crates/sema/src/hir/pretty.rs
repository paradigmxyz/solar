//! HIR pretty printing.
//!
//! This module provides a pretty-printer for the HIR that outputs Solidity-like source code.
//! Useful for debugging, diagnostics, and code suggestions.

use super::*;
use std::fmt::{self, Write};

/// A pretty-printer for HIR nodes.
pub struct HirPrettyPrinter<'a, 'hir> {
    hir: &'a Hir<'hir>,
    indent: usize,
}

impl<'a, 'hir> HirPrettyPrinter<'a, 'hir> {
    /// Creates a new HIR pretty-printer.
    pub fn new(hir: &'a Hir<'hir>) -> Self {
        Self { hir, indent: 0 }
    }

    fn write_indent(&self, f: &mut impl Write) -> fmt::Result {
        for _ in 0..self.indent {
            write!(f, "    ")?;
        }
        Ok(())
    }

    fn indented<F, R>(&mut self, f: F) -> R
    where
        F: FnOnce(&mut Self) -> R,
    {
        self.indent += 1;
        let result = f(self);
        self.indent -= 1;
        result
    }

    /// Formats a contract.
    pub fn fmt_contract(&mut self, contract: &Contract<'hir>, f: &mut impl Write) -> fmt::Result {
        write!(f, "{} {}", contract.kind, contract.name)?;

        // Write base classes
        if !contract.bases.is_empty() {
            write!(f, " is ")?;
            for (i, &base_id) in contract.bases.iter().enumerate() {
                if i > 0 {
                    write!(f, ", ")?;
                }
                let base = self.hir.contract(base_id);
                write!(f, "{}", base.name)?;
            }
        }

        writeln!(f, " {{")?;

        self.indented(|this| {
            for &item_id in contract.items {
                this.write_indent(f)?;
                this.fmt_item_id(item_id, f)?;
                writeln!(f)?;
            }
            Ok::<_, fmt::Error>(())
        })?;

        writeln!(f, "}}")
    }

    /// Formats an item by ID.
    pub fn fmt_item_id(&mut self, id: ItemId, f: &mut impl Write) -> fmt::Result {
        match id {
            ItemId::Contract(id) => self.fmt_contract(self.hir.contract(id), f),
            ItemId::Function(id) => self.fmt_function(self.hir.function(id), f),
            ItemId::Variable(id) => self.fmt_variable_decl(self.hir.variable(id), f),
            ItemId::Struct(id) => self.fmt_struct(self.hir.strukt(id), f),
            ItemId::Enum(id) => self.fmt_enum(self.hir.enumm(id), f),
            ItemId::Udvt(id) => self.fmt_udvt(self.hir.udvt(id), f),
            ItemId::Error(id) => self.fmt_error(self.hir.error(id), f),
            ItemId::Event(id) => self.fmt_event(self.hir.event(id), f),
        }
    }

    /// Formats a function.
    pub fn fmt_function(&mut self, func: &Function<'hir>, f: &mut impl Write) -> fmt::Result {
        write!(f, "{}", func.kind)?;

        if let Some(name) = func.name {
            write!(f, " {name}")?;
        }

        write!(f, "(")?;
        self.fmt_params(func.parameters, f)?;
        write!(f, ")")?;

        // Visibility
        if func.visibility != Visibility::Internal || func.kind.is_ordinary() {
            write!(f, " {}", func.visibility)?;
        }

        // State mutability
        if func.state_mutability != StateMutability::NonPayable {
            write!(f, " {}", func.state_mutability)?;
        }

        // Virtual/override
        if func.marked_virtual {
            write!(f, " virtual")?;
        }
        if func.override_ {
            write!(f, " override")?;
            if !func.overrides.is_empty() {
                write!(f, "(")?;
                for (i, &contract_id) in func.overrides.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}", self.hir.contract(contract_id).name)?;
                }
                write!(f, ")")?;
            }
        }

        // Modifiers
        for modifier in func.modifiers {
            write!(f, " ")?;
            self.fmt_modifier(modifier, f)?;
        }

        // Returns
        if !func.returns.is_empty() {
            write!(f, " returns (")?;
            self.fmt_params(func.returns, f)?;
            write!(f, ")")?;
        }

        // Body
        if let Some(body) = func.body {
            write!(f, " ")?;
            self.fmt_block(&body, f)?;
        } else {
            write!(f, ";")?;
        }

        Ok(())
    }

    /// Formats a modifier call.
    pub fn fmt_modifier(&mut self, modifier: &Modifier<'hir>, f: &mut impl Write) -> fmt::Result {
        match modifier.id {
            ItemId::Function(id) => {
                let func = self.hir.function(id);
                if let Some(name) = func.name {
                    write!(f, "{name}")?;
                }
            }
            ItemId::Contract(id) => {
                let contract = self.hir.contract(id);
                write!(f, "{}", contract.name)?;
            }
            _ => write!(f, "<modifier>")?,
        }

        if !modifier.args.is_empty() {
            write!(f, "(")?;
            self.fmt_call_args(&modifier.args, f)?;
            write!(f, ")")?;
        }

        Ok(())
    }

    /// Formats function parameters.
    pub fn fmt_params(&mut self, params: &[VariableId], f: &mut impl Write) -> fmt::Result {
        for (i, &param_id) in params.iter().enumerate() {
            if i > 0 {
                write!(f, ", ")?;
            }
            self.fmt_param(self.hir.variable(param_id), f)?;
        }
        Ok(())
    }

    /// Formats a single parameter.
    pub fn fmt_param(&mut self, var: &Variable<'hir>, f: &mut impl Write) -> fmt::Result {
        self.fmt_type(&var.ty, f)?;

        if let Some(loc) = var.data_location {
            write!(f, " {loc}")?;
        }

        if var.indexed {
            write!(f, " indexed")?;
        }

        if let Some(name) = var.name {
            write!(f, " {name}")?;
        }

        Ok(())
    }

    /// Formats a variable declaration.
    pub fn fmt_variable_decl(&mut self, var: &Variable<'hir>, f: &mut impl Write) -> fmt::Result {
        self.fmt_type(&var.ty, f)?;

        if let Some(vis) = var.visibility {
            write!(f, " {vis}")?;
        }

        if let Some(mutability) = var.mutability {
            write!(f, " {mutability}")?;
        }

        if let Some(loc) = var.data_location {
            write!(f, " {loc}")?;
        }

        if var.override_ {
            write!(f, " override")?;
        }

        if let Some(name) = var.name {
            write!(f, " {name}")?;
        }

        if let Some(init) = var.initializer {
            write!(f, " = ")?;
            self.fmt_expr(init, f)?;
        }

        write!(f, ";")
    }

    /// Formats a struct.
    pub fn fmt_struct(&mut self, s: &Struct<'hir>, f: &mut impl Write) -> fmt::Result {
        writeln!(f, "struct {} {{", s.name)?;

        self.indented(|this| {
            for &field_id in s.fields {
                this.write_indent(f)?;
                let field = this.hir.variable(field_id);
                this.fmt_type(&field.ty, f)?;
                if let Some(name) = field.name {
                    write!(f, " {name}")?;
                }
                writeln!(f, ";")?;
            }
            Ok::<_, fmt::Error>(())
        })?;

        write!(f, "}}")
    }

    /// Formats an enum.
    pub fn fmt_enum(&mut self, e: &Enum<'hir>, f: &mut impl Write) -> fmt::Result {
        writeln!(f, "enum {} {{", e.name)?;

        self.indented(|this| {
            for (i, variant) in e.variants.iter().enumerate() {
                this.write_indent(f)?;
                write!(f, "{variant}")?;
                if i < e.variants.len() - 1 {
                    writeln!(f, ",")?;
                } else {
                    writeln!(f)?;
                }
            }
            Ok::<_, fmt::Error>(())
        })?;

        write!(f, "}}")
    }

    /// Formats a UDVT.
    pub fn fmt_udvt(&mut self, u: &Udvt<'hir>, f: &mut impl Write) -> fmt::Result {
        write!(f, "type {} is ", u.name)?;
        self.fmt_type(&u.ty, f)?;
        write!(f, ";")
    }

    /// Formats an event.
    pub fn fmt_event(&mut self, e: &Event<'hir>, f: &mut impl Write) -> fmt::Result {
        write!(f, "event {}(", e.name)?;
        self.fmt_params(e.parameters, f)?;
        write!(f, ")")?;
        if e.anonymous {
            write!(f, " anonymous")?;
        }
        write!(f, ";")
    }

    /// Formats an error.
    pub fn fmt_error(&mut self, e: &Error<'hir>, f: &mut impl Write) -> fmt::Result {
        write!(f, "error {}(", e.name)?;
        self.fmt_params(e.parameters, f)?;
        write!(f, ");")
    }

    /// Formats a type.
    pub fn fmt_type(&mut self, ty: &Type<'hir>, f: &mut impl Write) -> fmt::Result {
        match &ty.kind {
            TypeKind::Elementary(elem) => write!(f, "{elem}"),
            TypeKind::Array(arr) => {
                self.fmt_type(&arr.element, f)?;
                write!(f, "[")?;
                if let Some(size) = arr.size {
                    self.fmt_expr(size, f)?;
                }
                write!(f, "]")
            }
            TypeKind::Function(func_ty) => {
                write!(f, "function(")?;
                self.fmt_params(func_ty.parameters, f)?;
                write!(f, ")")?;
                if func_ty.visibility != Visibility::Internal {
                    write!(f, " {}", func_ty.visibility)?;
                }
                if func_ty.state_mutability != StateMutability::NonPayable {
                    write!(f, " {}", func_ty.state_mutability)?;
                }
                if !func_ty.returns.is_empty() {
                    write!(f, " returns (")?;
                    self.fmt_params(func_ty.returns, f)?;
                    write!(f, ")")?;
                }
                Ok(())
            }
            TypeKind::Mapping(map) => {
                write!(f, "mapping(")?;
                self.fmt_type(&map.key, f)?;
                if let Some(key_name) = map.key_name {
                    write!(f, " {key_name}")?;
                }
                write!(f, " => ")?;
                self.fmt_type(&map.value, f)?;
                if let Some(value_name) = map.value_name {
                    write!(f, " {value_name}")?;
                }
                write!(f, ")")
            }
            TypeKind::Custom(id) => {
                let item = self.hir.item(*id);
                if let Some(name) = item.name() {
                    write!(f, "{name}")
                } else {
                    write!(f, "<custom>")
                }
            }
            TypeKind::Err(_) => write!(f, "<error>"),
        }
    }

    /// Formats a block.
    pub fn fmt_block(&mut self, block: &Block<'hir>, f: &mut impl Write) -> fmt::Result {
        writeln!(f, "{{")?;

        self.indented(|this| {
            for stmt in block.stmts {
                this.write_indent(f)?;
                this.fmt_stmt(stmt, f)?;
                writeln!(f)?;
            }
            Ok::<_, fmt::Error>(())
        })?;

        self.write_indent(f)?;
        write!(f, "}}")
    }

    /// Formats a statement.
    pub fn fmt_stmt(&mut self, stmt: &Stmt<'hir>, f: &mut impl Write) -> fmt::Result {
        match &stmt.kind {
            StmtKind::DeclSingle(var_id) => {
                let var = self.hir.variable(*var_id);
                self.fmt_type(&var.ty, f)?;
                if let Some(loc) = var.data_location {
                    write!(f, " {loc}")?;
                }
                if let Some(name) = var.name {
                    write!(f, " {name}")?;
                }
                if let Some(init) = var.initializer {
                    write!(f, " = ")?;
                    self.fmt_expr(init, f)?;
                }
                write!(f, ";")
            }
            StmtKind::DeclMulti(vars, expr) => {
                write!(f, "(")?;
                for (i, var_id) in vars.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if let Some(id) = var_id {
                        let var = self.hir.variable(*id);
                        self.fmt_type(&var.ty, f)?;
                        if let Some(loc) = var.data_location {
                            write!(f, " {loc}")?;
                        }
                        if let Some(name) = var.name {
                            write!(f, " {name}")?;
                        }
                    }
                }
                write!(f, ") = ")?;
                self.fmt_expr(expr, f)?;
                write!(f, ";")
            }
            StmtKind::Block(block) => self.fmt_block(block, f),
            StmtKind::UncheckedBlock(block) => {
                write!(f, "unchecked ")?;
                self.fmt_block(block, f)
            }
            StmtKind::Emit(expr) => {
                write!(f, "emit ")?;
                self.fmt_expr(expr, f)?;
                write!(f, ";")
            }
            StmtKind::Revert(expr) => {
                write!(f, "revert ")?;
                self.fmt_expr(expr, f)?;
                write!(f, ";")
            }
            StmtKind::Return(expr) => {
                write!(f, "return")?;
                if let Some(e) = expr {
                    write!(f, " ")?;
                    self.fmt_expr(e, f)?;
                }
                write!(f, ";")
            }
            StmtKind::Break => write!(f, "break;"),
            StmtKind::Continue => write!(f, "continue;"),
            StmtKind::Loop(block, source) => {
                write!(f, "{} ", source.name())?;
                self.fmt_block(block, f)
            }
            StmtKind::If(cond, then_stmt, else_stmt) => {
                write!(f, "if (")?;
                self.fmt_expr(cond, f)?;
                write!(f, ") ")?;
                self.fmt_stmt(then_stmt, f)?;
                if let Some(else_s) = else_stmt {
                    write!(f, " else ")?;
                    self.fmt_stmt(else_s, f)?;
                }
                Ok(())
            }
            StmtKind::Try(try_stmt) => {
                write!(f, "try ")?;
                self.fmt_expr(&try_stmt.expr, f)?;
                for (i, clause) in try_stmt.clauses.iter().enumerate() {
                    if i == 0 {
                        write!(f, " returns")?;
                    } else {
                        write!(f, " catch")?;
                        if let Some(name) = clause.name {
                            write!(f, " {name}")?;
                        }
                    }
                    write!(f, "(")?;
                    self.fmt_params(clause.args, f)?;
                    write!(f, ") ")?;
                    self.fmt_block(&clause.block, f)?;
                }
                Ok(())
            }
            StmtKind::Expr(expr) => {
                self.fmt_expr(expr, f)?;
                write!(f, ";")
            }
            StmtKind::Placeholder => write!(f, "_;"),
            StmtKind::Err(_) => write!(f, "<error>;"),
        }
    }

    /// Formats an expression.
    pub fn fmt_expr(&mut self, expr: &Expr<'hir>, f: &mut impl Write) -> fmt::Result {
        match &expr.kind {
            ExprKind::Array(elements) => {
                write!(f, "[")?;
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    self.fmt_expr(elem, f)?;
                }
                write!(f, "]")
            }
            ExprKind::Assign(lhs, op, rhs) => {
                self.fmt_expr(lhs, f)?;
                if let Some(bin_op) = op {
                    write!(f, " {}= ", bin_op.kind.to_str())?;
                } else {
                    write!(f, " = ")?;
                }
                self.fmt_expr(rhs, f)
            }
            ExprKind::Binary(lhs, op, rhs) => {
                self.fmt_expr(lhs, f)?;
                write!(f, " {} ", op.kind.to_str())?;
                self.fmt_expr(rhs, f)
            }
            ExprKind::Call(callee, args, options) => {
                self.fmt_expr(callee, f)?;
                if let Some(opts) = options {
                    write!(f, "{{")?;
                    for (i, opt) in opts.iter().enumerate() {
                        if i > 0 {
                            write!(f, ", ")?;
                        }
                        write!(f, "{}: ", opt.name)?;
                        self.fmt_expr(&opt.value, f)?;
                    }
                    write!(f, "}}")?;
                }
                write!(f, "(")?;
                self.fmt_call_args(args, f)?;
                write!(f, ")")
            }
            ExprKind::Delete(expr) => {
                write!(f, "delete ")?;
                self.fmt_expr(expr, f)
            }
            ExprKind::Ident(ress) => {
                if let Some(res) = ress.first() {
                    self.fmt_res(*res, f)?;
                } else {
                    write!(f, "<unresolved>")?;
                }
                Ok(())
            }
            ExprKind::Index(base, index) => {
                self.fmt_expr(base, f)?;
                write!(f, "[")?;
                if let Some(idx) = index {
                    self.fmt_expr(idx, f)?;
                }
                write!(f, "]")
            }
            ExprKind::Slice(base, start, end) => {
                self.fmt_expr(base, f)?;
                write!(f, "[")?;
                if let Some(s) = start {
                    self.fmt_expr(s, f)?;
                }
                write!(f, ":")?;
                if let Some(e) = end {
                    self.fmt_expr(e, f)?;
                }
                write!(f, "]")
            }
            ExprKind::Lit(lit) => write!(f, "{lit}"),
            ExprKind::Member(base, member) => {
                self.fmt_expr(base, f)?;
                write!(f, ".{member}")
            }
            ExprKind::New(ty) => {
                write!(f, "new ")?;
                self.fmt_type(ty, f)
            }
            ExprKind::Payable(expr) => {
                write!(f, "payable(")?;
                self.fmt_expr(expr, f)?;
                write!(f, ")")
            }
            ExprKind::Ternary(cond, then_expr, else_expr) => {
                self.fmt_expr(cond, f)?;
                write!(f, " ? ")?;
                self.fmt_expr(then_expr, f)?;
                write!(f, " : ")?;
                self.fmt_expr(else_expr, f)
            }
            ExprKind::Tuple(elements) => {
                write!(f, "(")?;
                for (i, elem) in elements.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    if let Some(e) = elem {
                        self.fmt_expr(e, f)?;
                    }
                }
                write!(f, ")")
            }
            ExprKind::TypeCall(ty) => {
                write!(f, "type(")?;
                self.fmt_type(ty, f)?;
                write!(f, ")")
            }
            ExprKind::Type(ty) => self.fmt_type(ty, f),
            ExprKind::Unary(op, expr) => {
                if op.kind.is_prefix() {
                    write!(f, "{}", op.kind.to_str())?;
                    self.fmt_expr(expr, f)
                } else {
                    self.fmt_expr(expr, f)?;
                    write!(f, "{}", op.kind.to_str())
                }
            }
            ExprKind::Err(_) => write!(f, "<error>"),
        }
    }

    /// Formats call arguments.
    pub fn fmt_call_args(&mut self, args: &CallArgs<'hir>, f: &mut impl Write) -> fmt::Result {
        match &args.kind {
            CallArgsKind::Unnamed(exprs) => {
                for (i, expr) in exprs.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    self.fmt_expr(expr, f)?;
                }
            }
            CallArgsKind::Named(named_args) => {
                write!(f, "{{")?;
                for (i, arg) in named_args.iter().enumerate() {
                    if i > 0 {
                        write!(f, ", ")?;
                    }
                    write!(f, "{}: ", arg.name)?;
                    self.fmt_expr(&arg.value, f)?;
                }
                write!(f, "}}")?;
            }
        }
        Ok(())
    }

    /// Formats a resolution.
    pub fn fmt_res(&mut self, res: Res, f: &mut impl Write) -> fmt::Result {
        match res {
            Res::Item(id) => {
                let item = self.hir.item(id);
                if let Some(name) = item.name() {
                    write!(f, "{name}")
                } else {
                    write!(f, "<unnamed>")
                }
            }
            Res::Namespace(id) => {
                let source = self.hir.source(id);
                write!(f, "{:?}", source.file.name)
            }
            Res::Builtin(builtin) => write!(f, "{builtin:?}"),
            Res::Err(_) => write!(f, "<error>"),
        }
    }
}

/// Extension trait for pretty-printing HIR nodes.
pub trait HirPrettyPrint<'hir> {
    /// Pretty-prints the HIR node to a string.
    fn pretty_print(&self, hir: &Hir<'hir>) -> String;
}

impl<'hir> HirPrettyPrint<'hir> for Contract<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_contract(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Function<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_function(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Struct<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_struct(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Enum<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_enum(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Event<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_event(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Error<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_error(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Variable<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_variable_decl(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Expr<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_expr(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Stmt<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_stmt(self, &mut s).unwrap();
        s
    }
}

impl<'hir> HirPrettyPrint<'hir> for Type<'hir> {
    fn pretty_print(&self, hir: &Hir<'hir>) -> String {
        let mut s = String::new();
        HirPrettyPrinter::new(hir).fmt_type(self, &mut s).unwrap();
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use solar_ast::TypeSize;

    #[test]
    fn test_elementary_type_formatting() {
        // Test that the pretty printer compiles and basic types work
        let hir = Hir::new();
        let ty = Type {
            span: Span::DUMMY,
            kind: TypeKind::Elementary(ElementaryType::UInt(TypeSize::new_int_bits(256))),
        };
        assert_eq!(ty.pretty_print(&hir), "uint256");
    }
}

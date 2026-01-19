//! Yul AST to HIR lowering and variable resolution.

use super::resolve::{ResolveContext, Res};
use crate::hir::{self, yul as hir_yul};
use solar_ast as ast;
use solar_data_structures::map::FxIndexMap;
use solar_interface::{Ident, Symbol, sym};

/// Context for lowering Yul blocks.
pub(super) struct YulLoweringContext<'a, 'gcx> {
    rcx: &'a mut ResolveContext<'gcx>,
    /// Stack of Yul scopes (for variable and function declarations).
    scopes: Vec<YulScope>,
}

/// A scope in Yul (variables and functions declared in a block).
#[derive(Default)]
struct YulScope {
    /// Yul variables declared in this scope.
    variables: FxIndexMap<Symbol, Ident>,
    /// Yul functions declared in this scope.
    functions: FxIndexMap<Symbol, Ident>,
}

impl<'a, 'gcx> YulLoweringContext<'a, 'gcx> {
    pub(super) fn new(rcx: &'a mut ResolveContext<'gcx>) -> Self {
        Self { rcx, scopes: Vec::new() }
    }

    /// Lower an assembly statement to HIR.
    pub(super) fn lower_assembly(
        &mut self,
        asm: &ast::StmtAssembly<'_>,
    ) -> &'gcx hir_yul::StmtAssembly<'gcx> {
        let dialect = asm.dialect.map(|d| d.value);
        let flags = self
            .rcx
            .arena
            .alloc_slice_fill_iter(asm.flags.iter().map(|f| f.value));
        let block = self.lower_block(&asm.block);
        self.rcx.arena.alloc(hir_yul::StmtAssembly { dialect, flags, block })
    }

    fn lower_block(&mut self, block: &ast::yul::Block<'_>) -> hir_yul::Block<'gcx> {
        self.enter_scope();

        // First pass: collect function definitions (they are visible in the entire block).
        for stmt in block.stmts.iter() {
            if let ast::yul::StmtKind::FunctionDef(func) = &stmt.kind {
                self.declare_function(func.name);
            }
        }

        // Second pass: lower all statements.
        let stmts = self
            .rcx
            .arena
            .alloc_slice_fill_iter(block.stmts.iter().map(|stmt| self.lower_stmt(stmt)));

        self.exit_scope();
        hir_yul::Block { span: block.span, stmts }
    }

    fn lower_stmt(&mut self, stmt: &ast::yul::Stmt<'_>) -> hir_yul::Stmt<'gcx> {
        let kind = match &stmt.kind {
            ast::yul::StmtKind::Block(block) => hir_yul::StmtKind::Block(self.lower_block(block)),

            ast::yul::StmtKind::VarDecl(names, init) => {
                // Declare variables in the current scope.
                for name in names.iter() {
                    self.declare_variable(*name);
                }
                let names = self.rcx.arena.alloc_slice_copy(&**names);
                let init = init.as_ref().map(|e| self.lower_expr(e));
                hir_yul::StmtKind::VarDecl(names, init)
            }

            ast::yul::StmtKind::AssignSingle(path, expr) => {
                let path = self.lower_path(path);
                let expr = self.lower_expr(expr);
                hir_yul::StmtKind::AssignSingle(path, expr)
            }

            ast::yul::StmtKind::AssignMulti(paths, expr) => {
                let paths = self
                    .rcx
                    .arena
                    .alloc_slice_fill_iter(paths.iter().map(|p| self.lower_path(p)));
                let expr = self.lower_expr(expr);
                hir_yul::StmtKind::AssignMulti(paths, expr)
            }

            ast::yul::StmtKind::Expr(expr) => {
                hir_yul::StmtKind::Expr(self.lower_expr(expr))
            }

            ast::yul::StmtKind::If(cond, body) => {
                let cond = self.lower_expr(cond);
                let body = self.lower_block(body);
                hir_yul::StmtKind::If(cond, body)
            }

            ast::yul::StmtKind::For(for_stmt) => {
                // For loop has special scoping: init block creates scope for entire loop.
                self.enter_scope();
                let init = self.lower_block_stmts(&for_stmt.init);
                let cond = self.lower_expr_full(&for_stmt.cond);
                let step = self.lower_block_stmts(&for_stmt.step);
                let body = self.lower_block(&for_stmt.body);
                self.exit_scope();

                hir_yul::StmtKind::For(self.rcx.arena.alloc(hir_yul::StmtFor {
                    init,
                    cond,
                    step,
                    body,
                }))
            }

            ast::yul::StmtKind::Switch(switch) => {
                let selector = self.lower_expr_full(&switch.selector);
                let cases = self.rcx.arena.alloc_slice_fill_iter(
                    switch.cases.iter().map(|case| self.lower_switch_case(case)),
                );
                hir_yul::StmtKind::Switch(hir_yul::StmtSwitch { selector, cases })
            }

            ast::yul::StmtKind::FunctionDef(func) => {
                // Function was already declared in first pass.
                hir_yul::StmtKind::FunctionDef(self.lower_function(func))
            }

            ast::yul::StmtKind::Leave => hir_yul::StmtKind::Leave,
            ast::yul::StmtKind::Break => hir_yul::StmtKind::Break,
            ast::yul::StmtKind::Continue => hir_yul::StmtKind::Continue,
        };

        hir_yul::Stmt { span: stmt.span, kind }
    }

    fn lower_switch_case(
        &mut self,
        case: &ast::yul::StmtSwitchCase<'_>,
    ) -> hir_yul::StmtSwitchCase<'gcx> {
        let constant = case.constant.as_ref().map(|lit| self.lower_lit(lit));
        let body = self.lower_block(&case.body);
        hir_yul::StmtSwitchCase { span: case.span, constant, body }
    }

    fn lower_function(&mut self, func: &ast::yul::Function<'_>) -> hir_yul::Function<'gcx> {
        // Function body has its own scope with parameters and returns.
        self.enter_scope();

        // Declare parameters.
        for param in func.parameters.iter() {
            self.declare_variable(*param);
        }

        // Declare return variables.
        for ret in func.returns.iter() {
            self.declare_variable(*ret);
        }

        let body = self.lower_block(&func.body);
        self.exit_scope();

        hir_yul::Function {
            name: func.name,
            parameters: self.rcx.arena.alloc_slice_copy(&*func.parameters),
            returns: self.rcx.arena.alloc_slice_copy(&*func.returns),
            body,
        }
    }

    fn lower_expr(&mut self, expr: &ast::yul::Expr<'_>) -> &'gcx hir_yul::Expr<'gcx> {
        self.rcx.arena.alloc(self.lower_expr_full(expr))
    }

    fn lower_expr_full(&mut self, expr: &ast::yul::Expr<'_>) -> hir_yul::Expr<'gcx> {
        let kind = match &expr.kind {
            ast::yul::ExprKind::Path(path) => {
                hir_yul::ExprKind::Path(self.lower_path(path))
            }

            ast::yul::ExprKind::Call(call) => {
                // Validate the call target exists (builtin or user-defined function).
                self.validate_call_target(call.name);
                let arguments = self
                    .rcx
                    .arena
                    .alloc_slice_fill_iter(call.arguments.iter().map(|e| self.lower_expr_full(e)));
                hir_yul::ExprKind::Call(hir_yul::ExprCall { name: call.name, arguments })
            }

            ast::yul::ExprKind::Lit(lit) => hir_yul::ExprKind::Lit(self.lower_lit(lit)),
        };

        hir_yul::Expr { span: expr.span, kind }
    }

    fn lower_path(&mut self, path: &ast::PathSlice) -> hir_yul::Path<'gcx> {
        let segments = self.rcx.arena.alloc_slice_copy(path.segments());
        let res = self.resolve_path(path);
        hir_yul::Path { span: path.span(), segments, res }
    }

    fn resolve_path(&mut self, path: &ast::PathSlice) -> hir_yul::PathRes {
        let segments = path.segments();

        match segments {
            [] => unreachable!("empty path"),
            [ident] => {
                // Single identifier: check Yul scopes first, then Solidity scopes.
                if self.lookup_yul_variable(ident.name).is_some() {
                    return hir_yul::PathRes::YulVariable;
                }

                // Try Solidity resolution.
                self.resolve_solidity_var(*ident)
            }
            [base, suffix] => {
                // Dotted path like x.slot or x.offset.
                // First check if base is a Yul variable (not allowed with .slot/.offset).
                if self.lookup_yul_variable(base.name).is_some() {
                    self.rcx.dcx().err(format!(
                        "Yul variable `{}` cannot have `.{}` suffix",
                        base.name, suffix.name
                    ))
                    .span(path.span())
                    .emit();
                    return hir_yul::PathRes::Err;
                }

                // Must be a Solidity variable.
                match self.resolve_solidity_var_id(*base) {
                    Some(var_id) => {
                        let var = self.rcx.hir.variable(var_id);
                        // Validate it's a storage variable.
                        if !var.is_state_variable() {
                            self.rcx.dcx().err(format!(
                                "`.{}` is only allowed on storage variables",
                                suffix.name
                            ))
                            .span(suffix.span)
                            .emit();
                            return hir_yul::PathRes::Err;
                        }

                        // Check suffix.
                        if suffix.name == sym::slot {
                            hir_yul::PathRes::StorageSlot(var_id)
                        } else if suffix.name == sym::offset {
                            hir_yul::PathRes::StorageOffset(var_id)
                        } else {
                            self.rcx.dcx().err(format!(
                                "unknown suffix `.{}`; expected `.slot` or `.offset`",
                                suffix.name
                            ))
                            .span(suffix.span)
                            .emit();
                            hir_yul::PathRes::Err
                        }
                    }
                    None => hir_yul::PathRes::Err,
                }
            }
            _ => {
                // Paths with more than 2 segments are not valid in Yul.
                self.rcx
                    .dcx()
                    .err("invalid path in assembly")
                    .span(path.span())
                    .emit();
                hir_yul::PathRes::Err
            }
        }
    }

    fn resolve_solidity_var(&mut self, ident: Ident) -> hir_yul::PathRes {
        match self.resolve_solidity_var_id(ident) {
            Some(var_id) => hir_yul::PathRes::SolidityVariable(var_id),
            None => hir_yul::PathRes::Err,
        }
    }

    fn resolve_solidity_var_id(&mut self, ident: Ident) -> Option<hir::VariableId> {
        // Try to resolve in Solidity scopes.
        let res = self.rcx.resolver.resolve_single(&ident, &self.rcx.scopes);
        match res {
            Ok(decl) => match decl.res {
                Res::Item(hir::ItemId::Variable(var_id)) => Some(var_id),
                Res::Item(item) => {
                    self.rcx
                        .dcx()
                        .err(format!(
                            "cannot access {} `{}` from assembly",
                            item.description(),
                            ident.name
                        ))
                        .span(ident.span)
                        .emit();
                    None
                }
                Res::Builtin(_) | Res::Namespace(_) => {
                    self.rcx
                        .dcx()
                        .err(format!("cannot access `{}` from assembly", ident.name))
                        .span(ident.span)
                        .emit();
                    None
                }
                Res::Err(_) => None,
            },
            Err(_) => {
                self.rcx
                    .dcx()
                    .err(format!("undefined variable `{}`", ident.name))
                    .span(ident.span)
                    .emit();
                None
            }
        }
    }

    fn validate_call_target(&mut self, name: Ident) {
        // Check if it's a Yul builtin.
        if name.is_yul_evm_builtin() {
            return;
        }

        // Check if it's a user-defined Yul function.
        if self.lookup_yul_function(name.name).is_some() {
            return;
        }

        // Not found.
        self.rcx
            .dcx()
            .err(format!("undefined function `{}`", name.name))
            .span(name.span)
            .emit();
    }

    fn lower_lit(&mut self, lit: &ast::Lit<'_>) -> &'gcx ast::Lit<'gcx> {
        self.rcx.arena.alloc(lit.copy_without_data())
    }

    /// Lower a block without creating a new scope (for init/step of for loop).
    fn lower_block_stmts(&mut self, block: &ast::yul::Block<'_>) -> hir_yul::Block<'gcx> {
        let stmts = self
            .rcx
            .arena
            .alloc_slice_fill_iter(block.stmts.iter().map(|stmt| self.lower_stmt(stmt)));
        hir_yul::Block { span: block.span, stmts }
    }

    // Scope management.

    fn enter_scope(&mut self) {
        self.scopes.push(YulScope::default());
    }

    fn exit_scope(&mut self) {
        self.scopes.pop();
    }

    fn declare_variable(&mut self, name: Ident) {
        let scope = self.scopes.last_mut().expect("no Yul scope");
        if let Some(prev) = scope.variables.insert(name.name, name) {
            self.rcx
                .dcx()
                .err(format!("variable `{}` already declared", name.name))
                .span(name.span)
                .span_note(prev.span, "previous declaration")
                .emit();
        }
    }

    fn declare_function(&mut self, name: Ident) {
        let scope = self.scopes.last_mut().expect("no Yul scope");
        if let Some(prev) = scope.functions.insert(name.name, name) {
            self.rcx
                .dcx()
                .err(format!("function `{}` already declared", name.name))
                .span(name.span)
                .span_note(prev.span, "previous declaration")
                .emit();
        }
    }

    fn lookup_yul_variable(&self, name: Symbol) -> Option<Ident> {
        for scope in self.scopes.iter().rev() {
            if let Some(&ident) = scope.variables.get(&name) {
                return Some(ident);
            }
        }
        None
    }

    fn lookup_yul_function(&self, name: Symbol) -> Option<Ident> {
        for scope in self.scopes.iter().rev() {
            if let Some(&ident) = scope.functions.get(&name) {
                return Some(ident);
            }
        }
        None
    }
}

use crate::{hir, ParsedSources};
use std::{fmt, sync::atomic::AtomicUsize};
use sulk_ast::ast;
use sulk_data_structures::{
    index::{Idx, IndexVec},
    map::{FxIndexMap, IndexEntry},
    smallvec::{smallvec, SmallVec},
    BumpExt,
};
use sulk_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    Ident, Session, Span, Symbol,
};

impl super::LoweringContext<'_, '_, '_> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn collect_exports(&mut self) {
        assert!(self.resolver.source_scopes.is_empty(), "exports already collected");
        self.resolver.source_scopes = self
            .hir
            .sources()
            .map(|source| {
                let mut scope = Declarations::with_capacity(source.items.len());
                for &item_id in source.items {
                    let item = self.hir.item(item_id);
                    if let Some(name) = item.name() {
                        let decl =
                            Declaration { kind: DeclarationKind::Item(item_id), span: name.span };
                        let _ = self.declare_in(&mut scope, name.name, decl);
                    }
                }
                scope
            })
            .collect();
    }

    #[instrument(level = "debug", skip_all)]
    pub(super) fn perform_imports(&mut self, sources: &ParsedSources<'_>) {
        for (source_id, source) in self.hir.sources_enumerated() {
            for &(item_id, import_id) in source.imports {
                let import_item = &sources[source_id].ast.as_ref().unwrap().items[item_id];
                let ast::ItemKind::Import(import) = &import_item.kind else { unreachable!() };
                let (source_scope, import_scope) = if source_id != import_id {
                    let (a, b) = super::get_two_mut_idx(
                        &mut self.resolver.source_scopes,
                        source_id,
                        import_id,
                    );
                    (a, Some(&*b))
                } else {
                    (&mut self.resolver.source_scopes[source_id], None)
                };
                match import.items {
                    ast::ImportItems::Plain(alias) | ast::ImportItems::Glob(alias) => {
                        if let Some(alias) = alias {
                            let _ = source_scope.declare_kind(
                                self.sess,
                                &self.hir,
                                alias,
                                DeclarationKind::Namespace(import_id),
                            );
                        } else if let Some(import_scope) = import_scope {
                            // Import all declarations.
                            for (&name, decls) in &import_scope.declarations {
                                for decl in decls {
                                    // Re-span to the import statement.
                                    let mut decl = *decl;
                                    decl.span = import_item.span;
                                    let _ = source_scope.declare(self.sess, &self.hir, name, decl);
                                }
                            }
                        }
                    }
                    ast::ImportItems::Aliases(ref aliases) => {
                        for &(import, alias) in aliases.iter() {
                            let name = alias.unwrap_or(import);
                            if let Some(import_scope) = import_scope {
                                Self::perform_alias_import(
                                    self.sess,
                                    &self.hir,
                                    source,
                                    source_scope,
                                    name,
                                    import,
                                    import_scope.resolve(import),
                                )
                            } else {
                                Self::perform_alias_import(
                                    self.sess,
                                    &self.hir,
                                    source,
                                    source_scope,
                                    name,
                                    import,
                                    source_scope.resolve_cloned(import),
                                )
                            }
                        }
                    }
                }
            }
        }
    }

    /// Separate function to avoid cloning `resolved` when the import is not a self-import.
    fn perform_alias_import(
        sess: &Session,
        hir: &hir::Hir<'_>,
        source: &hir::Source<'_>,
        source_scope: &mut Declarations,
        name: Ident,
        import: Ident,
        resolved: Option<impl AsRef<[Declaration]>>,
    ) {
        if let Some(resolved) = resolved {
            let resolved = resolved.as_ref();
            debug_assert!(!resolved.is_empty());
            for decl in resolved {
                // Re-span to the import name.
                let mut decl = *decl;
                decl.span = name.span;
                let _ = source_scope.declare(sess, hir, name.name, decl);
            }
        } else {
            let msg = format!(
                "declaration `{import}` not found in {}",
                sess.source_map().filename_for_diagnostics(&source.file.name)
            );
            let guar = sess.dcx.err(msg).span(import.span).emit();
            let _ = source_scope.declare_kind(sess, hir, name, DeclarationKind::Err(guar));
        }
    }

    #[instrument(level = "debug", skip_all)]
    pub(super) fn collect_contract_declarations(&mut self) {
        assert!(
            self.resolver.contract_scopes.is_empty(),
            "contract declarations already collected"
        );
        self.resolver.contract_scopes = self
            .hir
            .contracts()
            .map(|contract| {
                let mut scope = Declarations::with_capacity(contract.items.len());
                for &item_id in contract.items {
                    if let Some(name) = self.hir.item(item_id).name() {
                        let _ =
                            self.declare_kind_in(&mut scope, name, DeclarationKind::Item(item_id));
                    }
                }
                scope
            })
            .collect();
    }

    #[instrument(level = "debug", skip_all)]
    pub(super) fn resolve_base_contracts(&mut self) {
        let mut scopes = SymbolResolverScopes::new();
        for contract_id in self.hir.contract_ids() {
            let item = self.hir_to_ast[&hir::ItemId::Contract(contract_id)];
            let ast::ItemKind::Contract(ast_contract) = &item.kind else { unreachable!() };
            if ast_contract.bases.is_empty() {
                continue;
            }

            scopes.clear();
            scopes.source = Some(self.hir.contract(contract_id).source);
            let mut bases = SmallVec::<[_; 8]>::new();
            for base in ast_contract.bases.iter() {
                let name = &base.name;
                let Ok(base_id) = self
                    .resolver
                    .resolve_path_as::<hir::ContractId>(base.name, &scopes, "contract")
                else {
                    continue;
                };
                if base_id == contract_id {
                    let msg = "contracts cannot inherit from themselves";
                    self.dcx().err(msg).span(name.span()).emit();
                    continue;
                }
                bases.push(base_id);
            }
            self.hir.contracts[contract_id].bases = self.arena.alloc_slice_copy(&bases);
        }
    }
}

impl<'sess, 'ast, 'hir> super::LoweringContext<'sess, 'ast, 'hir> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn resolve(&mut self) {
        let next_id = &AtomicUsize::new(0);

        macro_rules! mk_resolver {
            ($e:expr) => {
                mk_resolver!(@scopes SymbolResolverScopes::new_in($e.source, $e.contract))
            };

            (@scopes $scopes:expr) => {
                ResolveContext {
                    scopes: $scopes,
                    sess: self.sess,
                    arena: self.arena,
                    hir: &mut self.hir,
                    resolver: &self.resolver,
                    next_id,
                }
            };
        }

        for id in self.hir.strukt_ids() {
            let ast_item = self.hir_to_ast[&hir::ItemId::Struct(id)];
            let ast::ItemKind::Struct(ast_struct) = &ast_item.kind else { unreachable!() };
            let strukt = self.hir.strukt(id);
            let mut cx = mk_resolver!(strukt);
            self.hir.structs[id].fields =
                self.arena.alloc_from_iter(ast_struct.fields.iter().map(|field| {
                    let name = field.name.unwrap_or_default();
                    let ty = cx.lower_type(&field.ty);
                    hir::StructField { ty, name }
                }));
        }

        for id in self.hir.error_ids() {
            let ast_item = self.hir_to_ast[&hir::ItemId::Error(id)];
            let ast::ItemKind::Error(ast_error) = &ast_item.kind else { unreachable!() };
            let error = self.hir.error(id);
            let mut cx = mk_resolver!(error);
            self.hir.errors[id].parameters =
                self.arena.alloc_from_iter(ast_error.parameters.iter().map(|param| {
                    let name = param.name;
                    let ty = cx.lower_type(&param.ty);
                    hir::ErrorParameter { ty, name }
                }));
        }

        for id in self.hir.event_ids() {
            let ast_item = self.hir_to_ast[&hir::ItemId::Event(id)];
            let ast::ItemKind::Event(ast_event) = &ast_item.kind else { unreachable!() };
            let event = self.hir.event(id);
            let mut cx = mk_resolver!(event);
            self.hir.events[id].parameters =
                self.arena.alloc_from_iter(ast_event.parameters.iter().map(|param| {
                    let name = param.name;
                    let ty = cx.lower_type(&param.ty);
                    hir::EventParameter { ty, indexed: param.indexed, name }
                }));
        }

        for id in self.hir.variable_ids() {
            let Some(&ast_item) = self.hir_to_ast.get(&hir::ItemId::Variable(id)) else { continue };
            let ast::ItemKind::Variable(ast_var) = &ast_item.kind else { unreachable!() };
            let var = self.hir.variable(id);
            let mut cx = mk_resolver!(var);
            let init = ast_var.initializer.as_deref().map(|init| cx.lower_expr(init));
            let ty = cx.lower_type(&ast_var.ty);
            self.hir.variables[id].initializer = init;
            self.hir.variables[id].ty = ty;
        }

        for id in self.hir.function_ids() {
            let ast_item = self.hir_to_ast[&hir::ItemId::Function(id)];
            let ast::ItemKind::Function(ast_func) = &ast_item.kind else { unreachable!() };

            let func = self.hir.function(id);
            let mut scopes = SymbolResolverScopes::new_in(func.source, func.contract);

            self.hir.functions[id].modifiers = {
                let mut modifiers = SmallVec::<[_; 8]>::new();
                for modifier in ast_func.header.modifiers.iter() {
                    let expected = if func.kind.is_constructor() {
                        "base class or modifier"
                    } else {
                        "modifier"
                    };
                    let Ok(id) = self.resolver.resolve_path_as(modifier.name, &scopes, expected)
                    else {
                        continue;
                    };
                    match id {
                        hir::ItemId::Contract(base)
                            if func.kind.is_constructor()
                                && func.contract.is_some_and(|c| {
                                    self.hir.contract(c).linearized_bases[1..].contains(&base)
                                }) => {}
                        hir::ItemId::Function(f) if self.hir.function(f).kind.is_modifier() => {}
                        _ => {
                            self.resolver.report_expected(
                                expected,
                                self.hir.item(id).description(),
                                modifier.name.span(),
                            );
                            continue;
                        }
                    }
                    modifiers.push(id);
                }
                self.arena.alloc_smallvec(modifiers)
            };

            let func = self.hir.function(id);
            self.hir.functions[id].overrides = {
                let mut overrides = SmallVec::<[_; 8]>::new();
                if let Some(ov) = &ast_func.header.override_ {
                    for path in ov.paths.iter() {
                        let Ok(id) = self.resolver.resolve_path_as(path, &scopes, "contract")
                        else {
                            continue;
                        };
                        // TODO: Move to override checker.
                        let Some(c) = func.contract else {
                            self.dcx().err("free functions cannot override").span(ov.span).emit();
                            continue;
                        };
                        if !self.hir.contract(c).linearized_bases[1..].contains(&id) {
                            self.dcx().err("override is not a base contract").span(ov.span).emit();
                            continue;
                        }
                        overrides.push(id);
                    }
                }
                self.arena.alloc_smallvec(overrides)
            };

            scopes.enter();
            let scope = scopes.current_scope();
            let func = self.hir.function(id);
            for &param in func.params.iter().chain(func.returns) {
                let Some(name) = self.hir.variable(param).name else { continue };
                let _ = self.declare_kind_in(
                    scope,
                    name,
                    DeclarationKind::Item(hir::ItemId::Variable(param)),
                );
            }

            if let Some(body) = &ast_func.body {
                let body = ResolveContext::new(self, scopes, next_id).lower_stmts(body);
                self.hir.functions[id].body = Some(body);
            }
        }
    }

    fn declare_kind_in(
        &self,
        scope: &mut Declarations,
        name: Ident,
        decl: DeclarationKind,
    ) -> Result<(), ErrorGuaranteed> {
        scope.declare_kind(self.sess, &self.hir, name, decl)
    }

    fn declare_in(
        &self,
        scope: &mut Declarations,
        name: Symbol,
        decl: Declaration,
    ) -> Result<(), ErrorGuaranteed> {
        scope.declare(self.sess, &self.hir, name, decl)
    }
}

/// Symbol resolution context.
struct ResolveContext<'sess, 'hir, 'a> {
    sess: &'sess Session,
    arena: &'hir hir::Arena,
    hir: &'a mut hir::Hir<'hir>,
    resolver: &'a SymbolResolver<'sess>,
    scopes: SymbolResolverScopes,
    next_id: &'a AtomicUsize,
}

impl<'sess, 'hir, 'a> ResolveContext<'sess, 'hir, 'a> {
    fn new(
        lcx: &'a mut super::LoweringContext<'sess, '_, 'hir>,
        scopes: SymbolResolverScopes,
        next_id: &'a AtomicUsize,
    ) -> Self {
        Self {
            sess: lcx.sess,
            arena: lcx.arena,
            hir: &mut lcx.hir,
            resolver: &lcx.resolver,
            scopes,
            next_id,
        }
    }

    fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    fn in_scope<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.scopes.enter();
        let t = f(self);
        self.scopes.exit();
        t
    }

    fn in_scope_if<T>(&mut self, cond: bool, f: impl FnOnce(&mut Self) -> T) -> T {
        if cond {
            self.in_scope(f)
        } else {
            f(self)
        }
    }

    fn resolve_path_as<T: TryFrom<DeclarationKind>>(
        &self,
        path: &ast::PathSlice,
        description: &str,
    ) -> Result<T, ErrorGuaranteed> {
        self.resolver.resolve_path_as(path, &self.scopes, description)
    }

    /// Lowers the given statements by first entering a new scope.
    fn lower_block(&mut self, block: &[ast::Stmt<'_>]) -> hir::Block<'hir> {
        self.in_scope_if(!block.is_empty(), |this| this.lower_stmts(block))
    }

    fn lower_stmts(&mut self, block: &[ast::Stmt<'_>]) -> hir::Block<'hir> {
        self.arena.alloc_from_iter(block.iter().map(|stmt| self.lower_stmt_full(stmt)))
    }

    fn lower_stmt(&mut self, stmt: &ast::Stmt<'_>) -> &'hir hir::Stmt<'hir> {
        self.arena.alloc(self.lower_stmt_full(stmt))
    }

    #[instrument(name = "lower_stmt", level = "debug", skip_all)]
    fn lower_stmt_full(&mut self, stmt: &ast::Stmt<'_>) -> hir::Stmt<'hir> {
        let kind = match &stmt.kind {
            ast::StmtKind::DeclSingle(var) => match self.lower_variable(var) {
                Ok(id) => hir::StmtKind::DeclSingle(id),
                Err(guar) => hir::StmtKind::Err(guar),
            },
            ast::StmtKind::DeclMulti(vars, expr) => {
                let ids = vars
                    .iter()
                    .map(|var| var.as_ref().and_then(|var| self.lower_variable(var).ok()))
                    .collect::<SmallVec<[_; 8]>>();
                hir::StmtKind::DeclMulti(self.arena.alloc_smallvec(ids), self.lower_expr(expr))
            }
            ast::StmtKind::Assembly(_) => hir::StmtKind::Err(
                self.dcx().err("assembly is not yet implemented").span(stmt.span).emit(),
            ),
            ast::StmtKind::Block(stmts) => hir::StmtKind::Block(self.lower_block(stmts)),
            ast::StmtKind::UncheckedBlock(stmts) => {
                hir::StmtKind::UncheckedBlock(self.lower_block(stmts))
            }
            ast::StmtKind::Break => hir::StmtKind::Break,
            ast::StmtKind::Continue => hir::StmtKind::Continue,
            ast::StmtKind::Return(expr) => {
                hir::StmtKind::Return(self.lower_expr_opt(expr.as_deref()))
            }
            ast::StmtKind::While(_, _)
            | ast::StmtKind::DoWhile(_, _)
            | ast::StmtKind::For { .. } => self.lower_loop_stmt(stmt),
            ast::StmtKind::Emit(path, args) => match self.resolve_path_as(path, "event") {
                Ok(id) => hir::StmtKind::Emit(id, self.lower_call_args(args)),
                Err(guar) => hir::StmtKind::Err(guar),
            },
            ast::StmtKind::Revert(path, args) => match self.resolve_path_as(path, "error") {
                Ok(id) => hir::StmtKind::Revert(id, self.lower_call_args(args)),
                Err(guar) => hir::StmtKind::Err(guar),
            },
            ast::StmtKind::Expr(expr) => hir::StmtKind::Expr(self.lower_expr(expr)),
            ast::StmtKind::If(cond, then, else_) => hir::StmtKind::If(
                self.lower_expr(cond),
                self.lower_stmt(then),
                else_.as_deref().map(|stmt| self.lower_stmt(stmt)),
            ),
            ast::StmtKind::Try(ast::StmtTry { expr, returns, block, catch }) => {
                hir::StmtKind::Try(self.arena.alloc(hir::StmtTry {
                    expr: self.lower_expr_full(expr),
                    returns: self.lower_variables(returns),
                    block: self.lower_block(block),
                    catch: self.arena.alloc_from_iter(catch.iter().map(|catch| hir::CatchClause {
                        name: catch.name,
                        args: self.lower_variables(catch.args),
                        block: self.lower_block(catch.block),
                    })),
                }))
            }
            ast::StmtKind::Placeholder => hir::StmtKind::Placeholder,
        };
        hir::Stmt { span: stmt.span, kind }
    }

    fn lower_variables(&mut self, vars: &[ast::VariableDefinition<'_>]) -> &'hir [hir::VariableId] {
        let vars = vars
            .iter()
            .filter_map(|var| self.lower_variable(var).ok())
            .collect::<SmallVec<[_; 8]>>();
        self.arena.alloc_smallvec(vars)
    }

    /// Lowers `var` to HIR and declares it in the current scope.
    fn lower_variable(
        &mut self,
        var: &ast::VariableDefinition<'_>,
    ) -> Result<hir::VariableId, ErrorGuaranteed> {
        let id = super::lower::lower_variable(
            self.hir,
            var,
            self.scopes.source.unwrap(),
            self.scopes.contract,
        );

        let Some(name) = var.name else {
            // Should've been reported already.
            return Err(ErrorGuaranteed::new_unchecked());
        };
        let decl = DeclarationKind::Item(hir::ItemId::Variable(id));
        let _ = self.scopes.current_scope().declare_kind(self.sess, self.hir, name, decl);
        Ok(id)
    }

    /// Desugars a `while`, `do while`, or `for` loop into a `loop` HIR statement.
    fn lower_loop_stmt(&mut self, stmt: &ast::Stmt<'_>) -> hir::StmtKind<'hir> {
        let span = stmt.span;
        match &stmt.kind {
            // loop {
            //     if (<cond>) <stmt> else break;
            // }
            ast::StmtKind::While(cond, stmt) => {
                let cond = self.lower_expr(cond);
                let stmt = self.lower_stmt(stmt);
                let break_stmt = self.arena.alloc(hir::Stmt { span, kind: hir::StmtKind::Break });
                let body = self.arena.alloc(hir::Stmt {
                    span,
                    kind: hir::StmtKind::If(cond, stmt, Some(break_stmt)),
                });
                hir::StmtKind::Loop(std::slice::from_ref(body), hir::LoopSource::While)
            }

            // loop {
            //     { <stmt> }
            //     if (<cond>) continue else break;
            // }
            ast::StmtKind::DoWhile(stmt, cond) => {
                let stmt = self.in_scope(|this| this.lower_stmt_full(stmt));
                let cond = self.lower_expr(cond);
                let cont_stmt = self.arena.alloc(hir::Stmt { span, kind: hir::StmtKind::Continue });
                let break_stmt = self.arena.alloc(hir::Stmt { span, kind: hir::StmtKind::Break });
                let check =
                    hir::Stmt { span, kind: hir::StmtKind::If(cond, cont_stmt, Some(break_stmt)) };

                let body = self.arena.alloc_array([stmt, check]);
                hir::StmtKind::Loop(body, hir::LoopSource::DoWhile)
            }

            // {
            //     <init>;
            //     loop {
            //         if (<cond>) {
            //             { <body> }
            //             <next>;
            //         } else break;
            //     }
            // }
            ast::StmtKind::For { init, cond, next, body } => {
                self.in_scope_if(init.is_some(), |this| {
                    let init = init.as_deref().map(|stmt| this.lower_stmt_full(stmt));
                    let cond = this.lower_expr_opt(cond.as_deref());
                    let mut body =
                        this.in_scope_if(next.is_some(), |this| this.lower_stmt_full(body));
                    let next = this.lower_expr_opt(next.as_deref());

                    // <body> = { <body>; <next>; }
                    if let Some(next) = next {
                        let next = hir::Stmt { span: next.span, kind: hir::StmtKind::Expr(next) };
                        body = hir::Stmt {
                            span: body.span,
                            kind: hir::StmtKind::Block(this.arena.alloc_array([body, next])),
                        };
                    }

                    // <body> = if (<cond>) { <body> } else break;
                    if let Some(cond) = cond {
                        let break_stmt =
                            this.arena.alloc(hir::Stmt { span, kind: hir::StmtKind::Break });
                        body = hir::Stmt {
                            span: body.span,
                            kind: hir::StmtKind::If(cond, self.arena.alloc(body), Some(break_stmt)),
                        };
                    }

                    let mut kind =
                        hir::StmtKind::Loop(self.arena.alloc_array([body]), hir::LoopSource::For);

                    if let Some(init) = init {
                        let s = hir::Stmt { span, kind };
                        kind = hir::StmtKind::Block(this.arena.alloc_array([init, s]));
                    }

                    kind
                })
            }

            _ => unreachable!(),
        }
    }

    fn lower_expr(&mut self, expr: &ast::Expr<'_>) -> &'hir hir::Expr<'hir> {
        self.arena.alloc(self.lower_expr_full(expr))
    }

    fn lower_expr_opt(&mut self, expr: Option<&ast::Expr<'_>>) -> Option<&'hir hir::Expr<'hir>> {
        expr.map(|expr| self.lower_expr(expr))
    }

    fn lower_exprs<'b, I, T>(&mut self, exprs: I) -> &'hir [hir::Expr<'hir>]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
        T: AsRef<ast::Expr<'b>>,
    {
        self.arena.alloc_from_iter(exprs.into_iter().map(|e| self.lower_expr_full(e.as_ref())))
    }

    #[instrument(name = "lower_expr", level = "debug", skip_all)]
    fn lower_expr_full(&mut self, expr: &ast::Expr<'_>) -> hir::Expr<'hir> {
        let kind = match &expr.kind {
            ast::ExprKind::Array(exprs) => hir::ExprKind::Array(self.lower_exprs(&**exprs)),
            ast::ExprKind::Assign(lhs, op, rhs) => {
                hir::ExprKind::Assign(self.lower_expr(lhs), *op, self.lower_expr(rhs))
            }
            ast::ExprKind::Binary(lhs, op, rhs) => {
                hir::ExprKind::Binary(self.lower_expr(lhs), *op, self.lower_expr(rhs))
            }
            ast::ExprKind::Call(callee, args) => {
                hir::ExprKind::Call(self.lower_expr(callee), self.lower_call_args(args))
            }
            ast::ExprKind::CallOptions(callee, options) => {
                hir::ExprKind::CallOptions(self.lower_expr(callee), self.lower_named_args(options))
            }
            ast::ExprKind::Delete(expr) => hir::ExprKind::Delete(self.lower_expr(expr)),
            ast::ExprKind::Ident(name) => {
                match self.resolve_path_as(ast::PathSlice::from_ref(name), "item") {
                    Ok(id) => hir::ExprKind::Ident(id),
                    Err(guar) => hir::ExprKind::Err(guar),
                }
            }
            ast::ExprKind::Index(expr, index) => match index {
                ast::IndexKind::Index(index) => hir::ExprKind::Index(
                    self.lower_expr(expr),
                    index.as_deref().map(|index| self.lower_expr(index)),
                ),
                ast::IndexKind::Range(start, end) => hir::ExprKind::Slice(
                    self.lower_expr(expr),
                    self.lower_expr_opt(start.as_deref()),
                    self.lower_expr_opt(end.as_deref()),
                ),
            },
            ast::ExprKind::Lit(lit, _) => {
                hir::ExprKind::Lit(self.arena.literals.alloc(ast::Lit::clone(lit)))
            }
            ast::ExprKind::Member(expr, member) => {
                hir::ExprKind::Member(self.lower_expr(expr), *member)
            }
            ast::ExprKind::New(ty) => hir::ExprKind::New(self.lower_type(ty)),
            ast::ExprKind::Payable(args) => 'b: {
                if let ast::CallArgs::Unnamed(args) = args {
                    if let [arg] = &args[..] {
                        break 'b hir::ExprKind::Payable(self.lower_expr(arg));
                    }
                }
                let msg = "expected exactly one unnamed argument";
                let guar = self.sess.dcx.err(msg).span(expr.span).emit();
                hir::ExprKind::Err(guar)
            }
            ast::ExprKind::Ternary(cond, then, r#else) => hir::ExprKind::Ternary(
                self.lower_expr(cond),
                self.lower_expr(then),
                self.lower_expr(r#else),
            ),
            ast::ExprKind::Tuple(exprs) => hir::ExprKind::Tuple(
                self.arena
                    .alloc_from_iter(exprs.iter().map(|expr| self.lower_expr_opt(expr.as_deref()))),
            ),
            ast::ExprKind::TypeCall(ty) => hir::ExprKind::TypeCall(self.lower_type(ty)),
            ast::ExprKind::Type(ty) => hir::ExprKind::Type(self.lower_type(ty)),
            ast::ExprKind::Unary(op, expr) => hir::ExprKind::Unary(*op, self.lower_expr(expr)),
        };
        hir::Expr { id: self.next_id(), kind, span: expr.span }
    }

    fn lower_named_args(&mut self, options: &[ast::NamedArg<'_>]) -> &'hir [hir::NamedArg<'hir>] {
        self.arena.alloc_from_iter(
            options.iter().map(|arg| hir::NamedArg {
                name: arg.name,
                value: self.lower_expr_full(arg.value),
            }),
        )
    }

    fn lower_call_args(&mut self, args: &ast::CallArgs<'_>) -> hir::CallArgs<'hir> {
        match args {
            ast::CallArgs::Unnamed(args) => hir::CallArgs::Unnamed(self.lower_exprs(&**args)),
            ast::CallArgs::Named(args) => hir::CallArgs::Named(self.lower_named_args(args)),
        }
    }

    #[instrument(name = "lower_stmt", level = "debug", skip_all)]
    fn lower_type(&mut self, ty: &ast::Type<'_>) -> hir::Type<'hir> {
        let kind = match &ty.kind {
            ast::TypeKind::Elementary(ty) => hir::TypeKind::Elementary(*ty),
            ast::TypeKind::Array(array) => hir::TypeKind::Array(self.arena.alloc(hir::TypeArray {
                element: self.lower_type(&array.element),
                size: array.size.as_deref().map(|size| self.lower_expr(size)),
            })),
            ast::TypeKind::Function(f) => hir::TypeKind::Function(
                self.arena.alloc(hir::TypeFunction {
                    parameters: self
                        .arena
                        .alloc_from_iter(f.parameters.iter().map(|p| self.lower_type(&p.ty))),
                    visibility: f.visibility,
                    state_mutability: f.state_mutability,
                    returns: self
                        .arena
                        .alloc_from_iter(f.returns.iter().map(|p| self.lower_type(&p.ty))),
                }),
            ),
            ast::TypeKind::Mapping(mapping) => {
                hir::TypeKind::Mapping(self.arena.alloc(hir::TypeMapping {
                    key: self.lower_type(&mapping.key),
                    key_name: mapping.key_name,
                    value: self.lower_type(&mapping.value),
                    value_name: mapping.value_name,
                }))
            }
            ast::TypeKind::Custom(path) => match self.resolve_path_as(path, "item") {
                Ok(id) => hir::TypeKind::Custom(id),
                Err(guar) => hir::TypeKind::Err(guar),
            },
        };
        hir::Type { kind, span: ty.span }
    }

    fn next_id<I: Idx>(&self) -> I {
        I::from_usize(self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

enum ResolverError {
    Unresolved(u32),
    NotAScope(u32),
}

impl fmt::Display for ResolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAScope(_) | Self::Unresolved(_) => f.write_str("unresolved symbol"),
        }
    }
}

impl ResolverError {
    fn span(&self, path: &ast::PathSlice) -> Span {
        match *self {
            Self::NotAScope(i) | Self::Unresolved(i) => path.segments()[i as usize].span,
        }
    }
}

pub(super) struct SymbolResolver<'sess> {
    dcx: &'sess DiagCtxt,
    pub(super) source_scopes: IndexVec<hir::SourceId, Declarations>,
    pub(super) contract_scopes: IndexVec<hir::ContractId, Declarations>,
}

impl<'sess> SymbolResolver<'sess> {
    pub(super) fn new(dcx: &'sess DiagCtxt) -> Self {
        Self { dcx, source_scopes: IndexVec::new(), contract_scopes: IndexVec::new() }
    }

    fn resolve_path_as<T: TryFrom<DeclarationKind>>(
        &self,
        path: &ast::PathSlice,
        scopes: &SymbolResolverScopes,
        description: &str,
    ) -> Result<T, ErrorGuaranteed> {
        let decl = self.resolve_path(path, scopes)?;
        if let DeclarationKind::Err(guard) = decl.kind {
            return Err(guard);
        }
        T::try_from(decl.kind)
            .map_err(|_| self.report_expected(description, decl.description(), path.span()))
    }

    fn resolve_path(
        &self,
        path: &ast::PathSlice,
        scopes: &SymbolResolverScopes,
    ) -> Result<Declaration, ErrorGuaranteed> {
        self.resolve_path_raw(path, scopes)
            .map_err(|e| self.dcx.err(e.to_string()).span(e.span(path)).emit())
    }

    fn resolve_path_raw(
        &self,
        path: &ast::PathSlice,
        scopes: &SymbolResolverScopes,
    ) -> Result<Declaration, ResolverError> {
        let mut segments = path.segments().iter();
        let mut decl = self
            .resolve_name_raw(*segments.next().unwrap(), scopes)
            .ok_or(ResolverError::Unresolved(0))?;
        if decl.is_err() {
            return Ok(decl);
        }
        for (i, &segment) in segments.enumerate() {
            let i = i as u32 + 1;
            let scope = self.scope_of(decl.kind).ok_or(ResolverError::NotAScope(i))?;
            decl = scope.resolve_single(segment).ok_or(ResolverError::Unresolved(i))?;
            if decl.is_err() {
                return Ok(decl);
            }
        }
        Ok(decl)
    }

    fn resolve_name_raw(&self, name: Ident, scopes: &SymbolResolverScopes) -> Option<Declaration> {
        scopes.get(self).find_map(move |scope| scope.resolve_single(name))
    }

    fn scope_of(&self, declaration: DeclarationKind) -> Option<&Declarations> {
        match declaration {
            DeclarationKind::Item(hir::ItemId::Contract(id)) => Some(&self.contract_scopes[id]),
            DeclarationKind::Namespace(id) => Some(&self.source_scopes[id]),
            _ => None,
        }
    }

    fn report_expected(&self, expected: &str, found: &str, span: Span) -> ErrorGuaranteed {
        self.dcx.err(format!("expected {expected}, found {found}")).span(span).emit()
    }
}

/// Mutable symbol resolution state.
#[derive(Debug)]
struct SymbolResolverScopes {
    source: Option<hir::SourceId>,
    contract: Option<hir::ContractId>,
    scopes: Vec<Declarations>,
}

impl SymbolResolverScopes {
    #[inline]
    fn new() -> Self {
        Self { source: None, contract: None, scopes: Vec::new() }
    }

    #[inline]
    fn new_in(source: hir::SourceId, contract: Option<hir::ContractId>) -> Self {
        Self { source: Some(source), contract, scopes: Vec::new() }
    }

    #[inline]
    fn clear(&mut self) {
        self.scopes.clear();
        self.source = None;
        self.contract = None;
    }

    #[inline]
    #[allow(clippy::filter_map_identity)] // More efficient than flatten.
    fn get<'a>(
        &'a self,
        resolver: &'a SymbolResolver<'_>,
    ) -> impl Iterator<Item = &'a Declarations> + Clone + 'a {
        debug_assert!(self.source.is_some() || self.contract.is_some());
        let scopes = self.scopes.iter().rev();
        let outer = [
            self.contract.map(|id| &resolver.contract_scopes[id]),
            self.source.map(|id| &resolver.source_scopes[id]),
        ]
        .into_iter()
        .filter_map(std::convert::identity);
        // let builtins = None::<&Declarations>;
        scopes.chain(outer) //.chain(builtins)
    }

    fn enter(&mut self) {
        self.scopes.push(Declarations::new());
    }

    #[track_caller]
    #[inline]
    fn current_scope(&mut self) -> &mut Declarations {
        self.scopes.last_mut().expect("missing initial scope")
    }

    #[track_caller]
    fn exit(&mut self) {
        self.scopes.pop().expect("unbalanced enter/exit");
    }
}

#[derive(Debug)]
pub(super) struct Declarations {
    pub(super) declarations: FxIndexMap<Symbol, DeclarationsInner>,
}

type DeclarationsInner = SmallVec<[Declaration; 1]>;

impl Declarations {
    fn new() -> Self {
        Self::with_capacity(0)
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self { declarations: FxIndexMap::with_capacity_and_hasher(capacity, Default::default()) }
    }

    pub(super) fn resolve(&self, name: Ident) -> Option<&[Declaration]> {
        self.declarations.get(&name.name).map(std::ops::Deref::deref)
    }

    pub(super) fn resolve_cloned(&self, name: Ident) -> Option<DeclarationsInner> {
        self.declarations.get(&name.name).cloned()
    }

    pub(super) fn resolve_single(&self, name: Ident) -> Option<Declaration> {
        let decls = self.resolve(name)?;
        if decls.len() != 1 {
            return None;
        }
        decls.first().copied()
    }

    /// Declares `Ident { name, span } => kind` by converting it to
    /// `name => Declaration { kind, span }`.
    #[inline]
    pub(super) fn declare_kind(
        &mut self,
        sess: &Session,
        hir: &hir::Hir<'_>,
        name: Ident,
        kind: DeclarationKind,
    ) -> Result<(), ErrorGuaranteed> {
        self.declare(sess, hir, name.name, Declaration { kind, span: name.span })
    }

    pub(super) fn declare(
        &mut self,
        sess: &Session,
        hir: &hir::Hir<'_>,
        name: Symbol,
        decl: Declaration,
    ) -> Result<(), ErrorGuaranteed> {
        self.try_declare(hir, name, decl)
            .map_err(|conflict| report_conflict(hir, sess, name, decl, conflict))
    }

    pub(super) fn try_declare(
        &mut self,
        hir: &hir::Hir<'_>,
        name: Symbol,
        decl: Declaration,
    ) -> Result<(), Declaration> {
        match self.declarations.entry(name) {
            IndexEntry::Occupied(entry) => {
                if let Some(conflict) = Self::conflicting_declaration(hir, decl, entry.get()) {
                    return Err(conflict);
                }
                let declarations = entry.into_mut();
                if !declarations.contains(&decl) {
                    declarations.push(decl);
                }
            }
            IndexEntry::Vacant(entry) => {
                entry.insert(smallvec![decl]);
            }
        }
        Ok(())
    }

    fn conflicting_declaration(
        hir: &hir::Hir<'_>,
        decl: Declaration,
        declarations: &[Declaration],
    ) -> Option<Declaration> {
        use hir::ItemId::*;
        use DeclarationKind::*;

        if declarations.is_empty() {
            return None;
        }

        // https://github.com/ethereum/solidity/blob/de1a017ccb935d149ed6bcbdb730d89883f8ce02/libsolidity/analysis/DeclarationContainer.cpp#L35
        if matches!(decl.kind, Item(Function(_) | Event(_))) {
            if let Item(Function(f)) = decl.kind {
                let f = hir.function(f);
                if !f.kind.is_ordinary() {
                    return Some(declarations[0]);
                }
            }
            let same_kind = |decl2: &Declaration| match &decl2.kind {
                Item(Function(f)) => hir.function(*f).kind.is_ordinary(),
                k => k.matches(&decl.kind),
            };
            declarations.iter().find(|&decl2| !same_kind(decl2)).copied()
        } else if declarations == [decl] {
            None
        } else {
            Some(declarations[0])
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(super) struct Declaration {
    pub(super) kind: DeclarationKind,
    pub(super) span: Span,
}

impl std::ops::Deref for Declaration {
    type Target = DeclarationKind;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.kind
    }
}

impl PartialEq for Declaration {
    #[inline]
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
    }
}

impl Eq for Declaration {}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum DeclarationKind {
    /// A resolved item.
    Item(hir::ItemId),
    /// Synthetic import namespace, X in `import * as X from "path"` or `import "path" as X`.
    Namespace(hir::SourceId),
    /// An error occurred while resolving the item. Silences further errors regarding this name.
    Err(ErrorGuaranteed),
}

impl fmt::Debug for DeclarationKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Declaration::")?;
        match self {
            Self::Item(id) => id.fmt(f),
            Self::Namespace(id) => id.fmt(f),
            Self::Err(_) => f.write_str("Err"),
        }
    }
}

impl From<hir::ItemId> for DeclarationKind {
    fn from(id: hir::ItemId) -> Self {
        Self::Item(id)
    }
}

macro_rules! impl_try_from {
    ($($t:ty => $pat:pat => $e:expr),* $(,)?) => {
        $(
            impl TryFrom<DeclarationKind> for $t {
                type Error = ();

                fn try_from(decl: DeclarationKind) -> Result<Self, ()> {
                    match decl {
                        $pat => $e,
                        _ => Err(()),
                    }
                }
            }
        )*
    };
}

impl_try_from!(
    hir::ItemId => DeclarationKind::Item(id) => Ok(id),
    hir::ContractId => DeclarationKind::Item(hir::ItemId::Contract(id)) => Ok(id),
    // hir::FunctionId => Declaration::Item(hir::ItemId::Function(id)) => Ok(id),
    hir::EventId => DeclarationKind::Item(hir::ItemId::Event(id)) => Ok(id),
    hir::ErrorId => DeclarationKind::Item(hir::ItemId::Error(id)) => Ok(id),
);

#[allow(dead_code)]
impl DeclarationKind {
    pub(super) fn description(&self) -> &'static str {
        match self {
            Self::Item(item) => item.description(),
            Self::Namespace(_) => "namespace",
            Self::Err(_) => "<error>",
        }
    }

    pub(super) fn item_id(&self) -> Option<hir::ItemId> {
        match self {
            Self::Item(id) => Some(*id),
            _ => None,
        }
    }

    pub(super) fn matches(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Item(a), Self::Item(b)) => a.matches(b),
            _ => std::mem::discriminant(self) == std::mem::discriminant(other),
        }
    }

    pub(super) fn is_err(&self) -> bool {
        matches!(self, Self::Err(_))
    }
}

pub(super) fn report_conflict(
    hir: &hir::Hir<'_>,
    sess: &Session,
    name: Symbol,
    decl: Declaration,
    mut previous: Declaration,
) -> ErrorGuaranteed {
    debug_assert_ne!(decl.span, previous.span);

    let mut err = sess.dcx.err(format!("identifier `{name}` already declared")).span(decl.span);

    // If `previous` is coming from an import, show both the import and the real span.
    if let DeclarationKind::Item(item_id) = previous.kind {
        if let Ok(snippet) = sess.source_map().span_to_snippet(previous.span) {
            if snippet.starts_with("import") {
                err = err.span_note(previous.span, "previous declaration imported here");
                let real_span = hir.item(item_id).span();
                previous.span = real_span;
            }
        }
    }

    err = err.span_note(previous.span, "previous declaration declared here");
    err.emit()
}

use crate::{hir, Sources};
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
    Ident, Session, Span,
};

type _Scopes = sulk_data_structures::scope::Scopes<
    Ident,
    SmallVec<[Declaration; 2]>,
    sulk_data_structures::map::FxBuildHasher,
>;

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
                    if let Some(name) = self.hir.item(item_id).name() {
                        scope.declare(name, Declaration::Item(item_id));
                    }
                }
                scope
            })
            .collect();
    }

    #[instrument(level = "debug", skip_all)]
    pub(super) fn perform_imports(&mut self, sources: &Sources<'_>) {
        for (source_id, source) in self.hir.sources_enumerated() {
            for &(item_id, import_id) in source.imports {
                let item = &sources[source_id].ast.as_ref().unwrap().items[item_id];
                let ast::ItemKind::Import(import) = &item.kind else { unreachable!() };
                let dcx = &self.sess.dcx;
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
                            source_scope.declare(alias, Declaration::Namespace(import_id));
                        } else if let Some(import_scope) = import_scope {
                            source_scope.import(import_scope);
                        }
                    }
                    ast::ImportItems::Aliases(ref aliases) => {
                        for &(import, alias) in aliases.iter() {
                            let name = alias.unwrap_or(import);
                            let resolved =
                                import_scope.and_then(|import_scope| import_scope.resolve(import));
                            if let Some(resolved) = resolved {
                                debug_assert!(!resolved.is_empty());
                                source_scope.declare_many(name, resolved.iter().copied());
                            } else {
                                let msg = format!(
                                    "declaration `{import}` not found in {}",
                                    self.sess
                                        .source_map()
                                        .filename_for_diagnostics(&source.file.name)
                                );
                                let guar = dcx.err(msg).span(import.span).emit();
                                source_scope.declare(name, Declaration::Err(guar));
                            }
                        }
                    }
                }
            }
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
                        scope.declare(name, Declaration::Item(item_id));
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
            scopes.source = Some(self.hir.contract(contract_id).source_id);
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
        let mut scopes = SymbolResolverScopes::new();

        for id in self.hir.variable_ids() {
            let Some(&ast_item) = self.hir_to_ast.get(&hir::ItemId::Variable(id)) else { continue };
            let ast::ItemKind::Variable(ast_var) = &ast_item.kind else { unreachable!() };
            let Some(init) = &ast_var.initializer else { continue };
            let var = self.hir.variable(id);
            scopes.clear();
            scopes.contract = var.contract;
            if let Some(c) = var.contract {
                scopes.source = Some(self.hir.contract(c).source_id);
            }
            // TODO
            let _init = init;
            // let _init = self.resolve_expr(init, &mut scopes);
        }

        for id in self.hir.function_ids() {
            let ast_item = self.hir_to_ast[&hir::ItemId::Function(id)];
            let ast::ItemKind::Function(ast_func) = &ast_item.kind else { unreachable!() };

            let func = self.hir.function(id);
            scopes.clear();
            scopes.contract = func.contract;
            if let Some(c) = func.contract {
                scopes.source = Some(self.hir.contract(c).source_id);
            }

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
                scope
                    .try_declare(name, Declaration::Item(hir::ItemId::Variable(param)))
                    .unwrap_or_else(|conflict| self.report_conflict(name, conflict));
            }

            // TODO
            let Some(_body) = &ast_func.body else { continue };
            // let _body = self.resolve_block(body, &mut scopes);
        }
    }

    fn report_conflict(&self, name: Ident, conflict: Declaration) {
        let second = match conflict {
            Declaration::Item(id) => Some(self.hir.item(id).span()),
            Declaration::Namespace(_) => None,
            Declaration::Err(_) => None,
        };
        report_conflict(self.dcx(), name.span, second)
    }
}

pub(super) fn report_conflict(dcx: &DiagCtxt, mut first: Span, mut second: Option<Span>) {
    if let Some(second) = &mut second {
        if first.lo() > second.lo() {
            std::mem::swap(&mut first, second);
        }
    }
    let mut err = dcx.err("identifier already declared");
    if let Some(second) = second {
        err = err.span_note(second, "previous declaration here");
    }
    err.emit();
}

/// Lowers and resolves a single AST body, meaning a variable or function's statements.
struct ResolveContext<'sess, 'hir, 'lcx> {
    sess: &'sess Session,
    arena: &'hir hir::Arena,
    hir: &'lcx hir::Hir<'hir>,
    // hir_to_ast: &'lcx FxIndexMap<hir::ItemId, &'ast ast::Item<'ast>>,
    resolver: &'lcx SymbolResolver<'sess>,
    scopes: SymbolResolverScopes,
    next_id: &'lcx AtomicUsize,
}

impl<'sess, 'hir, 'lcx> ResolveContext<'sess, 'hir, 'lcx> {
    fn lower_expr(&mut self, expr: &ast::Expr<'_>) -> &'hir hir::Expr<'hir> {
        self.arena.alloc(self.lower_expr_mut(expr))
    }

    fn lower_expr_opt(&mut self, expr: Option<&ast::Expr<'_>>) -> Option<&'hir hir::Expr<'hir>> {
        expr.map(|expr| self.lower_expr(expr))
    }

    fn lower_exprs<'a, I, T>(&mut self, exprs: I) -> &'hir [hir::Expr<'hir>]
    where
        I: IntoIterator<Item = T>,
        I::IntoIter: ExactSizeIterator,
        T: AsRef<ast::Expr<'a>>,
    {
        self.arena.alloc_from_iter(exprs.into_iter().map(|e| self.lower_expr_mut(e.as_ref())))
    }

    fn lower_expr_mut(&mut self, expr: &ast::Expr<'_>) -> hir::Expr<'hir> {
        hir::Expr { id: self.next_id(), kind: self.lower_expr_kind(&expr.kind), span: expr.span }
    }

    fn lower_expr_kind(&mut self, expr: &ast::ExprKind<'_>) -> hir::ExprKind<'hir> {
        match expr {
            ast::ExprKind::Array(exprs) => hir::ExprKind::Array(self.lower_exprs(&**exprs)),
            ast::ExprKind::Assign(lhs, op, rhs) => {
                hir::ExprKind::Assign(self.lower_expr(lhs), *op, self.lower_expr(rhs))
            }
            ast::ExprKind::Binary(lhs, op, rhs) => {
                hir::ExprKind::Binary(self.lower_expr(lhs), *op, self.lower_expr(rhs))
            }
            ast::ExprKind::Call(callee, args) => {
                hir::ExprKind::Call(self.lower_expr(callee), self.lower_exprs(&**args))
            }
            ast::ExprKind::CallOptions(callee, options) => {
                hir::ExprKind::CallOptions(self.lower_expr(callee), self.lower_exprs(&**options))
            }
            ast::ExprKind::Delete(expr) => hir::ExprKind::Delete(self.lower_expr(expr)),
            ast::ExprKind::Ident(name) => {
                match self.resolver.resolve_path_as(
                    ast::PathSlice::from_ref(name),
                    &self.scopes,
                    "item",
                ) {
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
                    start.as_deref().map(|start| self.lower_expr(start)),
                    end.as_deref().map(|end| self.lower_expr(end)),
                ),
            },
            ast::ExprKind::Lit(lit, _) => {
                hir::ExprKind::Lit(self.arena.literals.alloc((*lit).clone()))
            }
            ast::ExprKind::Member(expr, member) => {
                hir::ExprKind::Member(self.lower_expr(expr), *member)
            }
            ast::ExprKind::New(ty) => hir::ExprKind::New(self.lower_type(ty)),
            ast::ExprKind::Payable(expr) => hir::ExprKind::Payable(self.lower_expr(expr)),
            ast::ExprKind::Ternary(cond, then, r#else) => hir::ExprKind::Ternary(
                self.lower_expr(cond),
                self.lower_expr(then),
                self.lower_expr(r#else),
            ),
            ast::ExprKind::Tuple(exprs) => hir::ExprKind::Tuple(self.lower_exprs(&**exprs)),
            ast::ExprKind::TypeCall(ty) => hir::ExprKind::TypeCall(self.lower_type(ty)),
            ast::ExprKind::Type(ty) => hir::ExprKind::Type(self.lower_type(ty)),
            ast::ExprKind::Unary(op, expr) => hir::ExprKind::Unary(*op, self.lower_expr(expr)),
        }
    }

    fn lower_type(&mut self, ty: &ast::Type<'_>) -> hir::Type<'hir> {
        hir::Type {
            // id: self.next_id(),
            kind: self.lower_type_kind(&ty.kind),
            span: ty.span,
        }
    }

    fn lower_type_kind(&mut self, ty: &ast::TypeKind<'_>) -> hir::TypeKind<'hir> {
        match ty {
            ast::TypeKind::Elementary(ty) => hir::TypeKind::Elementary(*ty),
            ast::TypeKind::Array(array) => hir::TypeKind::Array(self.arena.alloc(hir::TypeArray {
                element: self.lower_type(&array.element),
                size: array.size.as_deref().map(|size| self.lower_expr(size)),
            })),
            ast::TypeKind::Function(f) => {
                hir::TypeKind::Function(self.arena.alloc(hir::TypeFunction {
                    parameters: self.arena.alloc_from_iter(f.parameters.iter().map(|p| {})),
                    visibility: f.visibility,
                    state_mutability: f.state_mutability,
                    returns: self.arena.alloc_from_iter(f.returns.iter().map(|p| {})),
                }))
            }
            ast::TypeKind::Mapping(mapping) => {
                hir::TypeKind::Mapping(self.arena.alloc(hir::TypeMapping {
                    key: self.lower_type(&mapping.key),
                    key_name: mapping.key_name,
                    value: self.lower_type(&mapping.value),
                    value_name: mapping.value_name,
                }))
            }
            ast::TypeKind::Custom(path) => {
                match self.resolver.resolve_path_as(path, &self.scopes, "item") {
                    Ok(id) => hir::TypeKind::Custom(id),
                    Err(guar) => hir::TypeKind::Err(guar),
                }
            }
        }
    }

    fn next_id<I: Idx>(&mut self) -> I {
        I::from_usize(self.next_id.fetch_add(1, std::sync::atomic::Ordering::Relaxed))
    }
}

enum ResolverError {
    Unresolved(u32),
    NotAScope(u32),
    Reported(ErrorGuaranteed),
}

impl fmt::Display for ResolverError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotAScope(_) | Self::Unresolved(_) => f.write_str("unresolved symbol"),
            Self::Reported(_) => unreachable!(),
        }
    }
}

impl ResolverError {
    fn span(&self, path: &ast::PathSlice) -> Span {
        match *self {
            Self::NotAScope(i) | Self::Unresolved(i) => path.segments()[i as usize].span,
            Self::Reported(_) => unreachable!(),
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

    fn resolve_path_as<T: TryFrom<Declaration>>(
        &self,
        path: &ast::PathSlice,
        scopes: &SymbolResolverScopes,
        description: &str,
    ) -> Result<T, ErrorGuaranteed> {
        let decl = self.resolve_path(path, scopes)?;
        if let Declaration::Err(guard) = decl {
            return Err(guard);
        }
        T::try_from(decl)
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
        if let Declaration::Err(_) = decl {
            return Ok(decl);
        }
        for (i, &segment) in segments.enumerate() {
            let i = i as u32 + 1;
            let scope = self.scope_of(decl).ok_or(ResolverError::NotAScope(i))?;
            decl = scope.resolve_single(segment).ok_or(ResolverError::Unresolved(i))?;
            if let Declaration::Err(_) = decl {
                return Ok(decl);
            }
        }
        Ok(decl)
    }

    fn resolve_name_raw(&self, name: Ident, scopes: &SymbolResolverScopes) -> Option<Declaration> {
        scopes.get(self).find_map(move |scope| scope.resolve_single(name))
    }

    fn scope_of(&self, declaration: Declaration) -> Option<&Declarations> {
        match declaration {
            Declaration::Item(hir::ItemId::Contract(id)) => Some(&self.contract_scopes[id]),
            Declaration::Namespace(id) => Some(&self.source_scopes[id]),
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
    ) -> impl Iterator<Item = &Declarations> + Clone + 'a {
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

    #[allow(dead_code)] // TODO
    #[track_caller]
    fn exit(&mut self) {
        self.scopes.pop().expect("unbalanced enter/exit");
    }
}

#[derive(Debug)]
pub(super) struct Declarations {
    pub(super) declarations: FxIndexMap<Ident, SmallVec<[Declaration; 2]>>,
}

impl Declarations {
    fn new() -> Self {
        Self::with_capacity(0)
    }

    pub(super) fn with_capacity(capacity: usize) -> Self {
        Self { declarations: FxIndexMap::with_capacity_and_hasher(capacity, Default::default()) }
    }

    pub(super) fn resolve(&self, name: Ident) -> Option<&[Declaration]> {
        self.declarations.get(&name).map(std::ops::Deref::deref)
    }

    pub(super) fn resolve_single(&self, name: Ident) -> Option<Declaration> {
        let decls = self.resolve(name)?;
        if decls.len() != 1 {
            return None;
        }
        decls.first().copied()
    }

    pub(super) fn declare(&mut self, name: Ident, decl: Declaration) {
        self.declare_many(name, std::iter::once(decl));
    }

    pub(super) fn declare_many(
        &mut self,
        name: Ident,
        decls: impl IntoIterator<Item = Declaration>,
    ) {
        let v = self.declarations.entry(name).or_default();
        for decl in decls {
            if !v.contains(&decl) {
                v.push(decl);
            }
        }
    }

    pub(super) fn try_declare(
        &mut self,
        name: Ident,
        decl: Declaration,
    ) -> Result<(), Declaration> {
        match self.declarations.entry(name) {
            IndexEntry::Occupied(entry) => {
                if let Some(conflict) = Self::conflicting_declarations(decl, entry.get()) {
                    return Err(conflict);
                }
                entry.into_mut().push(decl);
            }
            IndexEntry::Vacant(entry) => {
                entry.insert(smallvec![decl]);
            }
        }
        Ok(())
    }

    fn conflicting_declarations(
        decl: Declaration,
        declarations: &[Declaration],
    ) -> Option<Declaration> {
        use hir::ItemId::*;
        use Declaration::*;

        // https://github.com/ethereum/solidity/blob/de1a017ccb935d149ed6bcbdb730d89883f8ce02/libsolidity/analysis/DeclarationContainer.cpp#L101
        if matches!(decl, Item(Function(_) | Event(_))) {
            for &decl2 in declarations {
                if matches!(decl, Item(Function(_))) && !matches!(decl2, Item(Function(_))) {
                    return Some(decl2);
                }
                if matches!(decl, Item(Event(_))) && !matches!(decl2, Item(Event(_))) {
                    return Some(decl2);
                }
            }
            None
        } else if declarations == [decl] {
            None
        } else if !declarations.is_empty() {
            Some(declarations[0])
        } else {
            None
        }
    }

    pub(super) fn import(&mut self, other: &Self) {
        self.declarations.reserve(other.declarations.len());
        for (name, decls) in &other.declarations {
            self.declare_many(*name, decls.iter().copied());
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub(super) enum Declaration {
    /// A resolved item.
    Item(hir::ItemId),
    /// Synthetic import namespace, X in `import * as X from "path"` or `import "path" as X`.
    Namespace(hir::SourceId),
    /// An error occurred while resolving the item. Silences further errors regarding this name.
    Err(ErrorGuaranteed),
}

impl fmt::Debug for Declaration {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Declaration::")?;
        match self {
            Self::Item(id) => id.fmt(f),
            Self::Namespace(id) => id.fmt(f),
            Self::Err(_) => f.write_str("Err"),
        }
    }
}

impl From<hir::ItemId> for Declaration {
    fn from(id: hir::ItemId) -> Self {
        Self::Item(id)
    }
}

impl TryFrom<Declaration> for hir::ItemId {
    type Error = ();

    fn try_from(decl: Declaration) -> Result<Self, ()> {
        match decl {
            Declaration::Item(id) => Ok(id),
            _ => Err(()),
        }
    }
}

impl TryFrom<Declaration> for hir::FunctionId {
    type Error = ();

    fn try_from(decl: Declaration) -> Result<Self, Self::Error> {
        match decl {
            Declaration::Item(hir::ItemId::Function(id)) => Ok(id),
            _ => Err(()),
        }
    }
}

impl TryFrom<Declaration> for hir::ContractId {
    type Error = ();

    fn try_from(decl: Declaration) -> Result<Self, Self::Error> {
        match decl {
            Declaration::Item(hir::ItemId::Contract(id)) => Ok(id),
            _ => Err(()),
        }
    }
}

#[allow(dead_code)]
impl Declaration {
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
}

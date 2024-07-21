use crate::{hir, Sources};
use std::fmt;
use sulk_ast::ast;
use sulk_data_structures::{
    index::IndexVec,
    map::{FxIndexMap, IndexEntry},
    smallvec::{smallvec, SmallVec},
};
use sulk_interface::{diagnostics::ErrorGuaranteed, Ident};

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
                                let msg = format!("unresolved import `{import}`");
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
        for contract_id in self.hir.contract_ids() {
            let item = self.hir_to_ast[&hir::ItemId::Contract(contract_id)];
            let ast::ItemKind::Contract(ast_contract) = &item.kind else { unreachable!() };
            if ast_contract.bases.is_empty() {
                continue;
            }

            self.resolver.current_source_id = Some(self.hir.contract(contract_id).source_id);
            self.resolver.current_contract_id = None;
            let mut bases = SmallVec::<[_; 8]>::new();
            for base in ast_contract.bases.iter() {
                let name = &base.name;
                let Some(decl) = self.resolver.resolve_path(name) else {
                    let msg = format!("unresolved contract base `{name}`");
                    self.dcx().err(msg).span(name.span()).emit();
                    continue;
                };
                if let Declaration::Err(_) = decl {
                    continue;
                }
                let Declaration::Item(hir::ItemId::Contract(base_id)) = decl else {
                    let msg = format!("expected contract, found {}", decl.description());
                    self.dcx().err(msg).span(name.span()).emit();
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

    #[instrument(level = "debug", skip_all)]
    pub(super) fn resolve(&mut self) {
        // TODO
    }
}

pub(super) struct SymbolResolver {
    pub(super) source_scopes: IndexVec<hir::SourceId, Declarations>,
    pub(super) contract_scopes: IndexVec<hir::ContractId, Declarations>,

    pub(super) current_source_id: Option<hir::SourceId>,
    pub(super) current_contract_id: Option<hir::ContractId>,
}

impl SymbolResolver {
    pub(super) fn new() -> Self {
        Self {
            source_scopes: IndexVec::new(),
            contract_scopes: IndexVec::new(),
            current_source_id: None,
            current_contract_id: None,
        }
    }

    pub(super) fn resolve_path(&self, path: &ast::Path) -> Option<Declaration> {
        if let Some(&single) = path.get_ident() {
            return self.resolve_name(single);
        }

        let mut segments = path.segments().iter();
        let mut decl = self.resolve_name(*segments.next().unwrap())?;
        if let Declaration::Err(_) = decl {
            return Some(decl);
        }
        for &segment in segments {
            let scope = self.scope_of(decl)?;
            decl = scope.resolve_single(segment)?;
            if let Declaration::Err(_) = decl {
                return Some(decl);
            }
        }
        Some(decl)
    }

    pub(super) fn resolve_name(&self, name: Ident) -> Option<Declaration> {
        self.resolve_name_with(name, std::iter::empty())
    }

    fn resolve_name_with<'a>(
        &'a self,
        name: Ident,
        scopes: impl DoubleEndedIterator<Item = &'a Declarations>,
    ) -> Option<Declaration> {
        self.current_scopes().chain(scopes).rev().find_map(move |scope| scope.resolve_single(name))
    }

    fn scope_of(&self, declaration: Declaration) -> Option<&Declarations> {
        match declaration {
            Declaration::Item(hir::ItemId::Contract(id)) => Some(&self.contract_scopes[id]),
            Declaration::Namespace(id) => Some(&self.source_scopes[id]),
            _ => None,
        }
    }

    #[allow(clippy::filter_map_identity)] // More efficient than flatten.
    fn current_scopes(&self) -> impl DoubleEndedIterator<Item = &Declarations> {
        [
            self.current_source_id.map(|id| &self.source_scopes[id]),
            self.current_contract_id.map(|id| &self.contract_scopes[id]),
        ]
        .into_iter()
        .filter_map(std::convert::identity)
    }
}

#[derive(Debug)]
pub(super) struct Declarations {
    pub(super) declarations: FxIndexMap<Ident, SmallVec<[Declaration; 2]>>,
}

impl Declarations {
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

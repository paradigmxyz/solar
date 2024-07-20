use crate::{
    hir::{self, Hir},
    SmallSource, Sources,
};
use bumpalo::Bump;
use std::{fmt, marker::PhantomData};
use sulk_ast::ast;
use sulk_data_structures::{
    index::{Idx, IndexVec},
    map::FxIndexMap,
    smallvec::SmallVec,
    trustme,
};
use sulk_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    Ident, Session,
};

mod linearizer;

// type Scopes = sulk_data_structures::scope::Scopes<
//     Ident,
//     SmallVec<[Declaration; 2]>,
//     sulk_data_structures::map::FxBuildHasher,
// >;

#[instrument(name = "ast_lowering", level = "debug", skip_all)]
pub(crate) fn lower<'hir>(
    sess: &Session,
    sources: &Sources<'_>,
    hir_arena: &'hir Bump,
) -> Hir<'hir> {
    let mut lcx = LoweringContext::new(sess, hir_arena);

    // Lower AST to HIR.
    // SAFETY: `sources` outlives `lcx`, which does not outlive this function.
    let sources = unsafe { trustme::decouple_lt(sources) };
    lcx.lower_sources(sources);

    lcx.collect_exports();
    lcx.perform_imports(sources);
    lcx.collect_contract_declarations();
    lcx.resolve_base_contracts();
    lcx.linearize_contracts();
    lcx.resolve();

    // Clean up.
    lcx.shrink_to_fit();

    // eprintln!("{:#?}", lcx.hir);

    lcx.finish()
}

struct LoweringContext<'sess, 'ast, 'hir> {
    sess: &'sess Session,
    arena: &'hir Bump,
    hir: Hir<'hir>,
    hir_to_ast: FxIndexMap<hir::ItemId, &'ast ast::Item<'ast>>,

    resolver: SymbolResolver<'sess>,
}

impl<'sess, 'ast, 'hir> LoweringContext<'sess, 'ast, 'hir> {
    fn new(sess: &'sess Session, arena: &'hir Bump) -> Self {
        Self {
            sess,
            arena,
            hir: Hir::new(),
            hir_to_ast: FxIndexMap::default(),
            resolver: SymbolResolver::new(sess),
        }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    fn finish(self) -> Hir<'hir> {
        self.hir
    }

    #[instrument(level = "debug", skip_all)]
    fn lower_sources(&mut self, small_sources: &'ast IndexVec<hir::SourceId, SmallSource<'ast>>) {
        let new_sources = small_sources.iter_enumerated().map(|(id, source)| {
            let mut new_source = hir::Source {
                file: source.file.clone(),
                imports: self.arena.alloc_slice_copy(&source.imports),
                items: &[],
            };
            if let Some(ast) = &source.ast {
                let mut items = SmallVec::<[_; 16]>::new();
                for item in ast.items.iter() {
                    match &item.kind {
                        ast::ItemKind::Pragma(_)
                        | ast::ItemKind::Import(_)
                        | ast::ItemKind::Using(_) => {}
                        ast::ItemKind::Contract(_)
                        | ast::ItemKind::Function(_)
                        | ast::ItemKind::Variable(_)
                        | ast::ItemKind::Struct(_)
                        | ast::ItemKind::Enum(_)
                        | ast::ItemKind::Udvt(_)
                        | ast::ItemKind::Error(_)
                        | ast::ItemKind::Event(_) => {
                            let item_id = self.lower_item(item);
                            if let hir::ItemId::Contract(contract_id) = item_id {
                                self.hir.contracts[contract_id].source_id = id;
                            }
                            items.push(item_id)
                        }
                    }
                }
                new_source.items = self.arena.alloc_slice_copy(&items);
            };
            new_source
        });
        self.hir.sources = new_sources.collect();
    }

    fn lower_contract(
        &mut self,
        item: &ast::Item<'_>,
        contract: &'ast ast::ItemContract<'ast>,
    ) -> hir::ContractId {
        let mut ctor = None;
        let mut fallback = None;
        let mut receive = None;
        let mut items = SmallVec::<[_; 16]>::new();
        for item in contract.body.iter() {
            let id = match &item.kind {
                ast::ItemKind::Pragma(_)
                | ast::ItemKind::Import(_)
                | ast::ItemKind::Contract(_) => unreachable!("illegal item in contract body"),
                ast::ItemKind::Using(_) => continue,
                ast::ItemKind::Function(func) => {
                    let id = self.lower_function(item, func);
                    match func.kind {
                        ast::FunctionKind::Constructor
                        | ast::FunctionKind::Fallback
                        | ast::FunctionKind::Receive => {
                            let slot = match func.kind {
                                ast::FunctionKind::Constructor => &mut ctor,
                                ast::FunctionKind::Fallback => &mut fallback,
                                ast::FunctionKind::Receive => &mut receive,
                                _ => unreachable!(),
                            };
                            if let Some(prev) = *slot {
                                let msg = format!("{} function already declared", func.kind);
                                let note = "previous declaration here";
                                let prev_span = self.hir.function(prev).span;
                                self.dcx()
                                    .err(msg)
                                    .span(item.span)
                                    .span_note(prev_span, note)
                                    .emit();
                            } else {
                                *slot = Some(id);
                            }
                        }
                        ast::FunctionKind::Function | ast::FunctionKind::Modifier => {}
                    }
                    hir::ItemId::Function(id)
                }
                ast::ItemKind::Variable(_)
                | ast::ItemKind::Struct(_)
                | ast::ItemKind::Enum(_)
                | ast::ItemKind::Udvt(_)
                | ast::ItemKind::Error(_)
                | ast::ItemKind::Event(_) => self.lower_item(item),
            };
            items.push(id);
        }
        let id = self.hir.contracts.push(hir::Contract {
            name: contract.name,
            span: item.span,
            kind: contract.kind,

            // set later
            source_id: hir::SourceId::new(0),
            bases: &[],
            linearized_bases: &[],

            ctor,
            fallback,
            receive,
            items: self.arena.alloc_slice_copy(&items),
        });
        id
    }

    fn lower_item(&mut self, item: &'ast ast::Item<'ast>) -> hir::ItemId {
        let item_id = match &item.kind {
            ast::ItemKind::Pragma(_) | ast::ItemKind::Import(_) | ast::ItemKind::Using(_) => {
                unreachable!()
            }
            ast::ItemKind::Contract(i) => hir::ItemId::Contract(self.lower_contract(item, i)),
            ast::ItemKind::Function(i) => hir::ItemId::Function(self.lower_function(item, i)),
            ast::ItemKind::Variable(i) => hir::ItemId::Var(self.lower_variable(item, i)),
            ast::ItemKind::Struct(i) => hir::ItemId::Struct(self.lower_struct(item, i)),
            ast::ItemKind::Enum(i) => hir::ItemId::Enum(self.lower_enum(item, i)),
            ast::ItemKind::Udvt(i) => hir::ItemId::Udvt(self.lower_udvt(item, i)),
            ast::ItemKind::Error(i) => hir::ItemId::Error(self.lower_error(item, i)),
            ast::ItemKind::Event(i) => hir::ItemId::Event(self.lower_event(item, i)),
        };
        self.hir_to_ast.insert(item_id, item);
        item_id
    }

    fn lower_function(
        &mut self,
        item: &ast::Item<'_>,
        i: &ast::ItemFunction<'_>,
    ) -> hir::FunctionId {
        self.hir.functions.push(hir::Function {
            name: i.header.name,
            span: item.span,
            _tmp: PhantomData,
        })
    }

    fn lower_variable(
        &mut self,
        item: &ast::Item<'_>,
        i: &ast::VariableDefinition<'_>,
    ) -> hir::VarId {
        self.hir.vars.push(hir::Var { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_struct(&mut self, item: &ast::Item<'_>, i: &ast::ItemStruct<'_>) -> hir::StructId {
        self.hir.structs.push(hir::Struct { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_enum(&mut self, item: &ast::Item<'_>, i: &ast::ItemEnum<'_>) -> hir::EnumId {
        self.hir.enums.push(hir::Enum {
            name: i.name,
            span: item.span,
            variants: self.arena.alloc_slice_copy(&i.variants),
        })
    }

    fn lower_udvt(&mut self, item: &ast::Item<'_>, i: &ast::ItemUdvt<'_>) -> hir::UdvtId {
        self.hir.udvts.push(hir::Udvt { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_error(&mut self, item: &ast::Item<'_>, i: &ast::ItemError<'_>) -> hir::ErrorId {
        self.hir.errors.push(hir::Error { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_event(&mut self, item: &ast::Item<'_>, i: &ast::ItemEvent<'_>) -> hir::EventId {
        self.hir.events.push(hir::Event { name: i.name, span: item.span, _tmp: PhantomData })
    }

    #[instrument(level = "debug", skip_all)]
    fn collect_exports(&mut self) {
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
    fn perform_imports(&mut self, sources: &Sources<'_>) {
        for (source_id, source) in self.hir.sources_enumerated() {
            for &(item_id, import_id) in source.imports {
                let item = &sources[source_id].ast.as_ref().unwrap().items[item_id];
                let ast::ItemKind::Import(import) = &item.kind else { unreachable!() };
                let dcx = &self.sess.dcx;
                let (source_scope, import_scope) = if source_id != import_id {
                    let (a, b) = get_two_mut(
                        &mut self.resolver.source_scopes.raw,
                        source_id.index(),
                        import_id.index(),
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
                            let resolved =
                                import_scope.map(|import_scope| import_scope.resolve(import));
                            let name = alias.unwrap_or(import);
                            if resolved.is_none()
                                || resolved.as_ref().is_some_and(|resolved| resolved.len() == 0)
                            {
                                drop(resolved);
                                let msg = format!("unresolved import `{import}`");
                                let guar = dcx.err(msg).span(import.span).emit();
                                source_scope.declare(name, Declaration::Err(guar));
                            } else if let Some(resolved) = resolved {
                                source_scope.declare_many(name, resolved);
                            }
                        }
                    }
                }
            }
        }
    }

    #[instrument(level = "debug", skip_all)]
    fn collect_contract_declarations(&mut self) {
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
    fn resolve_base_contracts(&mut self) {
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
    fn resolve(&mut self) {
        // let mut scopes = Scopes::new();
        // TODO
    }

    #[instrument(level = "debug", skip_all)]
    fn shrink_to_fit(&mut self) {
        self.hir.shrink_to_fit();
    }
}

struct SymbolResolver<'sess> {
    #[allow(dead_code)]
    sess: &'sess Session,

    source_scopes: IndexVec<hir::SourceId, Declarations>,
    contract_scopes: IndexVec<hir::ContractId, Declarations>,

    current_source_id: Option<hir::SourceId>,
    current_contract_id: Option<hir::ContractId>,
}

impl<'sess> SymbolResolver<'sess> {
    fn new(sess: &'sess Session) -> Self {
        Self {
            sess,
            source_scopes: IndexVec::new(),
            contract_scopes: IndexVec::new(),
            current_source_id: None,
            current_contract_id: None,
        }
    }

    fn resolve_path(&self, path: &ast::Path) -> Option<Declaration> {
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

    fn resolve_name(&self, name: Ident) -> Option<Declaration> {
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

    fn current_scopes(&self) -> impl DoubleEndedIterator<Item = &Declarations> {
        [
            self.current_source_id.map(|id| &self.source_scopes[id]),
            self.current_contract_id.map(|id| &self.contract_scopes[id]),
        ]
        .into_iter()
        .flatten()
    }
}

#[derive(Debug)]
struct Declarations {
    declarations: FxIndexMap<Ident, SmallVec<[Declaration; 2]>>,
}

impl Declarations {
    fn with_capacity(capacity: usize) -> Self {
        Self { declarations: FxIndexMap::with_capacity_and_hasher(capacity, Default::default()) }
    }

    fn resolve(
        &self,
        name: Ident,
    ) -> impl ExactSizeIterator<Item = Declaration> + '_ + std::fmt::Debug {
        self.declarations.get(&name).map(std::ops::Deref::deref).unwrap_or_default().iter().copied()
    }

    fn resolve_single(&self, name: Ident) -> Option<Declaration> {
        let mut iter = self.resolve(name);
        if iter.len() != 1 {
            return None;
        }
        iter.next()
    }

    fn declare(&mut self, name: Ident, decl: Declaration) {
        self.declarations.entry(name).or_default().push(decl);
    }

    fn declare_many(&mut self, name: Ident, decls: impl IntoIterator<Item = Declaration>) {
        self.declarations.entry(name).or_default().extend(decls);
    }

    fn import(&mut self, other: &Self) {
        self.declarations.reserve(other.declarations.len());
        for (name, decls) in &other.declarations {
            self.declare_many(*name, decls.iter().copied());
        }
    }
}

#[derive(Clone, Copy)]
#[allow(dead_code)]
enum Declaration {
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

impl Declaration {
    fn description(&self) -> &'static str {
        match self {
            Self::Item(item) => item.description(),
            Self::Namespace(_) => "namespace",
            Self::Err(_) => "<error>",
        }
    }
}

fn get_two_mut<T>(sl: &mut [T], idx_1: usize, idx_2: usize) -> (&mut T, &mut T) {
    assert!(idx_1 != idx_2 && idx_1 < sl.len() && idx_2 < sl.len());
    let ptr = sl.as_mut_ptr();
    unsafe { (&mut *ptr.add(idx_1), &mut *ptr.add(idx_2)) }
}

use crate::{
    hir::{self, Hir},
    Sources,
};
use bumpalo::Bump;
use rayon::prelude::*;
use std::{fmt, marker::PhantomData};
use sulk_ast::ast;
use sulk_data_structures::{
    index::{Idx, IndexVec},
    map::FxIndexMap,
    smallvec::SmallVec,
};
use sulk_interface::{
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    Ident, Session,
};

#[instrument(name = "ast_lowering", level = "debug", skip_all)]
pub(crate) fn lower<'hir>(sess: &Session, sources: Sources<'hir>, arena: &'hir Bump) -> Hir<'hir> {
    let mut lcx = LoweringContext::new(sess, arena);

    // Lower AST to HIR.
    lcx.lower_sources(sources.sources);

    lcx.collect_exports();
    lcx.perform_imports();
    lcx.resolve();

    // Clean up.
    lcx.drop_asts();
    lcx.shrink_to_fit();

    lcx.hir
}

struct LoweringContext<'sess, 'hir> {
    sess: &'sess Session,
    arena: &'hir Bump,
    hir: Hir<'hir>,

    source_scopes: IndexVec<hir::SourceId, Scope>,
}

impl<'sess, 'hir> LoweringContext<'sess, 'hir> {
    fn new(sess: &'sess Session, arena: &'hir Bump) -> Self {
        Self { sess, arena, hir: Hir::new(), source_scopes: IndexVec::new() }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    #[instrument(level = "debug", skip_all)]
    fn lower_sources(&mut self, mut sources: IndexVec<hir::SourceId, hir::Source<'hir, 'hir>>) {
        for source in sources.iter_mut() {
            let Some(ast) = &source.ast else { continue };
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
                    | ast::ItemKind::Event(_) => items.push(self.lower_item(item)),
                }
            }
            source.items = self.arena.alloc_slice_copy(&items);
        }
        self.hir.sources = sources;
    }

    fn lower_contract(
        &mut self,
        item: &ast::Item<'_>,
        contract: &ast::ItemContract<'_>,
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
            bases: &[],
            ctor,
            fallback,
            receive,
            items: self.arena.alloc_slice_copy(&items),
        });
        id
    }

    fn lower_item(&mut self, item: &ast::Item<'_>) -> hir::ItemId {
        match &item.kind {
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
        }
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
        assert!(self.source_scopes.is_empty(), "exports already collected");
        self.source_scopes = self
            .hir
            .sources()
            .map(|source| {
                let mut scope = Scope::with_capacity(source.items.len());
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
    fn perform_imports(&mut self) {
        for (source_id, source) in self.hir.sources_enumerated() {
            for &(item_id, import_id) in &source.imports {
                let item = &source.ast.as_ref().unwrap().items[item_id];
                let ast::ItemKind::Import(import) = &item.kind else { unreachable!() };
                let dcx = &self.sess.dcx;
                let (source_scope, import_scope) =
                    get_two_mut(&mut self.source_scopes.raw, source_id.index(), import_id.index());
                match import.items {
                    ast::ImportItems::Plain(alias) | ast::ImportItems::Glob(alias) => {
                        if let Some(alias) = alias {
                            source_scope.declare(alias, Declaration::Namespace(import_id));
                        } else if source_id != import_id {
                            source_scope.import(import_scope);
                        }
                    }
                    ast::ImportItems::Aliases(ref aliases) => {
                        for &(import, alias) in aliases.iter() {
                            let resolved = import_scope.resolve(import);
                            let name = alias.unwrap_or(import);
                            if resolved.len() == 0 {
                                drop(resolved);
                                let msg = format!("unresolved import `{import}`");
                                let guar = dcx.err(msg).span(import.span).emit();
                                source_scope.declare(name, Declaration::Err(guar));
                            } else if source_id != import_id {
                                source_scope.declare_many(name, resolved);
                            }
                        }
                    }
                }
            }
        }
    }

    #[instrument(level = "debug", skip_all)]
    fn resolve(&mut self) {}

    #[instrument(level = "debug", skip_all)]
    fn shrink_to_fit(&mut self) {
        self.hir.shrink_to_fit();
    }

    #[instrument(level = "debug", skip_all)]
    fn drop_asts(&mut self) {
        // TODO: Switch back to sequential once the AST is using arenas.
        self.hir.sources.raw.par_iter_mut().for_each(|source| source.ast = None);
    }
}

#[derive(Clone, Debug)]
struct Scope {
    declarations: ScopeInner,
}

type ScopeInner = FxIndexMap<Ident, SmallVec<[Declaration; 2]>>;

impl Scope {
    fn with_capacity(capacity: usize) -> Self {
        Self { declarations: FxIndexMap::with_capacity_and_hasher(capacity, Default::default()) }
    }

    fn resolve(
        &self,
        name: Ident,
    ) -> impl ExactSizeIterator<Item = Declaration> + '_ + std::fmt::Debug {
        self.declarations.get(&name).map(std::ops::Deref::deref).unwrap_or_default().iter().copied()
    }

    fn declare(&mut self, name: Ident, decl: Declaration) {
        self.declarations.entry(name).or_default().push(decl);
    }

    fn declare_many(&mut self, name: Ident, decls: impl IntoIterator<Item = Declaration>) {
        self.declarations.entry(name).or_default().extend(decls);
    }

    fn import(&mut self, other: &Self) {
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
            Declaration::Item(id) => id.fmt(f),
            Declaration::Namespace(id) => id.fmt(f),
            Declaration::Err(_) => f.write_str("Err"),
        }
    }
}

fn get_two_mut<T>(sl: &mut [T], idx_1: usize, idx_2: usize) -> (&mut T, &mut T) {
    assert!(idx_1 != idx_2 && idx_1 < sl.len() && idx_2 < sl.len());
    let ptr = sl.as_mut_ptr();
    unsafe { (&mut *ptr.add(idx_1), &mut *ptr.add(idx_2)) }
}

#![allow(dead_code, unused_variables)] // TODO

use std::marker::PhantomData;

use crate::{
    hir::{self, Hir},
    Sources,
};
use bumpalo::Bump;
use sulk_ast::ast;
use sulk_data_structures::{index::IndexVec, map::FxIndexSet, smallvec::SmallVec};
use sulk_interface::{diagnostics::DiagCtxt, Ident, Session};

#[instrument(name = "hir_lowering", level = "debug", skip_all)]
pub(crate) fn lower<'hir>(sess: &Session, sources: Sources, arena: &'hir Bump) -> Hir<'hir> {
    let mut lcx = LoweringContext::new(sess, arena);
    lcx.lower(sources);
    lcx.hir.shrink_to_fit();
    lcx.resolve();
    lcx.hir
}

struct LoweringContext<'sess, 'hir> {
    sess: &'sess Session,
    arena: &'hir Bump,
    hir: Hir<'hir>,
}

impl<'sess, 'hir> LoweringContext<'sess, 'hir> {
    fn new(sess: &'sess Session, arena: &'hir Bump) -> Self {
        Self { sess, arena, hir: Hir::new() }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
    }

    #[instrument(level = "debug", skip_all)]
    fn lower(&mut self, sources: Sources) {
        let mut sources: IndexVec<hir::SourceId, hir::Source<'hir>> = sources.sources;
        for source in sources.iter_mut() {
            let Some(ast) = &source.ast else { continue };
            let mut items = SmallVec::<[_; 16]>::new();
            for item in &ast.items {
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
            source.ast = None;
        }
        self.hir.sources = sources;
    }

    fn lower_contract(
        &mut self,
        item: &ast::Item,
        contract: &ast::ItemContract,
    ) -> hir::ContractId {
        let mut ctor = None;
        let mut fallback = None;
        let mut receive = None;
        let mut items = SmallVec::<[_; 16]>::new();
        for item in &contract.body {
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

    fn lower_item(&mut self, item: &ast::Item) -> hir::ItemId {
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

    fn lower_function(&mut self, item: &ast::Item, i: &ast::ItemFunction) -> hir::FunctionId {
        self.hir.functions.push(hir::Function {
            name: i.header.name,
            span: item.span,
            _tmp: PhantomData,
        })
    }

    fn lower_variable(&mut self, item: &ast::Item, i: &ast::VariableDefinition) -> hir::VarId {
        self.hir.vars.push(hir::Var { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_struct(&mut self, item: &ast::Item, i: &ast::ItemStruct) -> hir::StructId {
        self.hir.structs.push(hir::Struct { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_enum(&mut self, item: &ast::Item, i: &ast::ItemEnum) -> hir::EnumId {
        self.hir.enums.push(hir::Enum {
            name: i.name,
            span: item.span,
            variants: self.arena.alloc_slice_copy(&i.variants),
        })
    }

    fn lower_udvt(&mut self, item: &ast::Item, i: &ast::ItemUdvt) -> hir::UdvtId {
        self.hir.udvts.push(hir::Udvt { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_error(&mut self, item: &ast::Item, i: &ast::ItemError) -> hir::ErrorId {
        self.hir.errors.push(hir::Error { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn lower_event(&mut self, item: &ast::Item, i: &ast::ItemEvent) -> hir::EventId {
        self.hir.events.push(hir::Event { name: i.name, span: item.span, _tmp: PhantomData })
    }

    fn resolve(&mut self) {}
}

fn collect_exports(ast: &ast::SourceUnit) -> FxIndexSet<Ident> {
    let mut exports = FxIndexSet::default();

    for item in &ast.items {
        match &item.kind {
            ast::ItemKind::Import(import) => match import.items {
                ast::ImportItems::Plain(alias) | ast::ImportItems::Glob(alias) => {
                    if let Some(alias) = alias {
                        exports.insert(alias);
                    }
                }
                ast::ImportItems::Aliases(ref aliases) => {
                    for &(_, alias) in aliases {
                        if let Some(alias) = alias {
                            exports.insert(alias);
                        }
                    }
                }
            },
            _ => {
                if let Some(name) = item.name() {
                    exports.insert(name);
                }
            }
        }
    }

    exports
}

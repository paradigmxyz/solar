use crate::{
    hir::{self, Hir},
    Sources,
};
use bumpalo::Bump;
use sulk_ast::ast;
use sulk_data_structures::{map::FxHashSet, smallvec::SmallVec};
use sulk_interface::{diagnostics::DiagCtxt, Session};

#[instrument(name = "hir_lowering", level = "debug", skip_all)]
pub(crate) fn lower<'hir>(sess: &Session, sources: Sources, arena: &'hir Bump) -> Hir<'hir> {
    let mut lcx = LoweringContext::new(sess, &sources, arena);
    lcx.collect();
    lcx.resolve();
    lcx.hir
}

struct LoweringContext<'a, 'hir> {
    sess: &'a Session,
    sources: &'a Sources,
    arena: &'hir Bump,
    hir: Hir<'hir>,
}

impl<'a, 'hir> LoweringContext<'a, 'hir> {
    fn new(sess: &'a Session, sources: &'a Sources, arena: &'hir Bump) -> Self {
        Self { sess, sources, arena, hir: Hir::new() }
    }

    /// Returns the diagnostic context.
    #[inline]
    fn dcx(&self) -> &'a DiagCtxt {
        &self.sess.dcx
    }

    fn collect(&mut self) {
        for source in self.sources.iter() {
            let Some(ast) = &source.ast else { continue };
            for item in &ast.items {
                match &item.kind {
                    ast::ItemKind::Pragma(_)
                    | ast::ItemKind::Import(_)
                    | ast::ItemKind::Using(_) => {}
                    ast::ItemKind::Contract(contract) => _ = self.lower_contract(contract),
                    ast::ItemKind::Function(_)
                    | ast::ItemKind::Variable(_)
                    | ast::ItemKind::Struct(_)
                    | ast::ItemKind::Enum(_)
                    | ast::ItemKind::Udvt(_)
                    | ast::ItemKind::Error(_)
                    | ast::ItemKind::Event(_) => _ = self.lower_item(item),
                }
            }
        }
    }

    fn lower_contract(&mut self, contract: &ast::ItemContract) -> hir::ContractId {
        let mut ctor = None;
        let mut fallback = None;
        let mut receive = None;
        let mut items = SmallVec::<[_; 16]>::new();
        for item in &contract.body {
            let id = match &item.kind {
                ast::ItemKind::Pragma(_)
                | ast::ItemKind::Import(_)
                | ast::ItemKind::Contract(_) => unreachable!(),
                ast::ItemKind::Using(_) => continue,
                ast::ItemKind::Function(func) => {
                    let id = self.lower_function(func);
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
                    hir::ContractItemId::Function(id)
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
            kind: contract.kind,
            bases: &[],
            ctor,
            fallback,
            receive,
            items: self.arena.alloc_slice_copy(&items),
        });
        id
    }

    fn lower_item(&mut self, item: &ast::Item) -> hir::ContractItemId {
        match &item.kind {
            ast::ItemKind::Pragma(_)
            | ast::ItemKind::Import(_)
            | ast::ItemKind::Using(_)
            | ast::ItemKind::Contract(_) => unreachable!(),
            ast::ItemKind::Function(item) => {
                hir::ContractItemId::Function(self.lower_function(item))
            }
            ast::ItemKind::Variable(item) => hir::ContractItemId::Var(self.lower_variable(item)),
            ast::ItemKind::Struct(item) => hir::ContractItemId::Struct(self.lower_struct(item)),
            ast::ItemKind::Enum(item) => hir::ContractItemId::Enum(self.lower_enum(item)),
            ast::ItemKind::Udvt(item) => hir::ContractItemId::Udvt(self.lower_udvt(item)),
            ast::ItemKind::Error(item) => hir::ContractItemId::Error(self.lower_error(item)),
            ast::ItemKind::Event(item) => hir::ContractItemId::Event(self.lower_event(item)),
        }
    }

    fn lower_function(&mut self, item: &ast::ItemFunction) -> hir::FunctionId {
        todo!()
    }

    fn lower_variable(&mut self, item: &ast::VariableDefinition) -> hir::VarId {
        todo!()
    }

    fn lower_struct(&mut self, item: &ast::ItemStruct) -> hir::StructId {
        todo!()
    }

    fn lower_enum(&mut self, item: &ast::ItemEnum) -> hir::EnumId {
        todo!()
    }

    fn lower_udvt(&mut self, item: &ast::ItemUdvt) -> hir::UdvtId {
        todo!()
    }

    fn lower_error(&mut self, item: &ast::ItemError) -> hir::ErrorId {
        todo!()
    }

    fn lower_event(&mut self, item: &ast::ItemEvent) -> hir::EventId {
        todo!()
    }

    fn resolve(&mut self) {}
}

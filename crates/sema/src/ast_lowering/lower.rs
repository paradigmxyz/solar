use crate::{hir, SmallSource};
use std::marker::PhantomData;
use sulk_ast::ast;
use sulk_data_structures::{index::IndexVec, smallvec::SmallVec};
use sulk_interface::Span;
use sulk_parse::BumpExt;

impl<'sess, 'ast, 'hir> super::LoweringContext<'sess, 'ast, 'hir> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn lower_sources(
        &mut self,
        small_sources: &'ast IndexVec<hir::SourceId, SmallSource<'ast>>,
    ) {
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
        self.current_contract_id = Some(self.hir.contracts.next_idx());
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
        self.current_contract_id = None;
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
            ast::ItemKind::Variable(i) => hir::ItemId::Variable(self.lower_variable(item.span, i)),
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
        // handled later: body, modifiers, override_
        let ast::ItemFunction { kind, ref header, body: _ } = *i;
        let ast::FunctionHeader {
            name,
            ref parameters,
            visibility,
            state_mutability,
            modifiers: _,
            virtual_,
            override_: _,
            ref returns,
        } = *header;
        let params = self.lower_variables(parameters);
        let returns = self.lower_variables(returns);
        self.hir.functions.push(hir::Function {
            span: item.span,
            contract: self.current_contract_id,
            name,
            kind,
            modifiers: &[],
            virtual_,
            overrides: &[],
            visibility,
            state_mutability,
            params,
            returns,
        })
    }

    fn lower_variables(
        &mut self,
        variables: &[ast::VariableDefinition<'_>],
    ) -> &'hir [hir::VariableId] {
        let mut vars = SmallVec::<[_; 16]>::new();
        for var in variables {
            // TODO: Span
            vars.push(self.lower_variable(Span::DUMMY, var));
        }
        self.arena.alloc_slice_copy(&vars)
    }

    fn lower_variable(&mut self, span: Span, i: &ast::VariableDefinition<'_>) -> hir::VariableId {
        // handled later: override_, initializer
        let ast::VariableDefinition {
            ty: _, // TODO
            visibility,
            mutability,
            data_location,
            override_: _,
            indexed,
            name,
            initializer: _,
        } = *i;
        self.hir.variables.push(hir::Variable {
            span,
            contract: self.current_contract_id,
            name,
            visibility,
            mutability,
            data_location,
            indexed,
            initializer: None,
        })
    }

    fn lower_struct(&mut self, item: &ast::Item<'_>, i: &ast::ItemStruct<'_>) -> hir::StructId {
        let ast::ItemStruct { name, ref fields } = *i;
        let mut fields2 = SmallVec::<[_; 8]>::new();
        for field in fields.iter() {
            let Some(name) = field.name else { continue };
            fields2.push(hir::StructField { name });
        }
        self.hir.structs.push(hir::Struct {
            span: item.span,
            contract: self.current_contract_id,
            name,
            fields: self.arena.alloc_smallvec(fields2),
        })
    }

    fn lower_enum(&mut self, item: &ast::Item<'_>, i: &ast::ItemEnum<'_>) -> hir::EnumId {
        let ast::ItemEnum { name, ref variants } = *i;
        self.hir.enums.push(hir::Enum {
            span: item.span,
            contract: self.current_contract_id,
            name,
            variants: self.arena.alloc_slice_copy(variants),
        })
    }

    fn lower_udvt(&mut self, item: &ast::Item<'_>, i: &ast::ItemUdvt<'_>) -> hir::UdvtId {
        // TODO: ty
        let ast::ItemUdvt { name, ty: _ } = *i;
        self.hir.udvts.push(hir::Udvt {
            span: item.span,
            contract: self.current_contract_id,
            name,
            _tmp: PhantomData,
        })
    }

    fn lower_error(&mut self, item: &ast::Item<'_>, i: &ast::ItemError<'_>) -> hir::ErrorId {
        // TODO: parameters
        let ast::ItemError { name, parameters: _ } = *i;
        self.hir.errors.push(hir::Error {
            span: item.span,
            contract: self.current_contract_id,
            name,
            _tmp: PhantomData,
        })
    }

    fn lower_event(&mut self, item: &ast::Item<'_>, i: &ast::ItemEvent<'_>) -> hir::EventId {
        // TODO: parameters
        let ast::ItemEvent { name, parameters: _, anonymous } = *i;
        self.hir.events.push(hir::Event {
            span: item.span,
            contract: self.current_contract_id,
            name,
            anonymous,
            _tmp: PhantomData,
        })
    }
}

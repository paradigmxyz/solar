use crate::{
    hir::{self, ContractId, SourceId},
    ParsedSource,
};
use solar_ast::ast;
use solar_data_structures::{index::IndexVec, smallvec::SmallVec};

impl<'ast, 'hir> super::LoweringContext<'_, 'ast, 'hir> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn lower_sources(
        &mut self,
        parsed_sources: &'ast IndexVec<hir::SourceId, ParsedSource<'ast>>,
    ) {
        let hir_sources = parsed_sources.iter_enumerated().map(|(id, source)| {
            let mut hir_source = hir::Source {
                file: source.file.clone(),
                imports: self.arena.alloc_slice_copy(&source.imports),
                items: &[],
            };
            if let Some(ast) = &source.ast {
                let mut items = SmallVec::<[_; 16]>::new();
                self.current_source_id = id;
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
                hir_source.items = self.arena.alloc_slice_copy(&items);
            };
            hir_source
        });
        self.hir.sources = hir_sources.collect();
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
                    let hir::ItemId::Function(id) = self.lower_item(item) else { unreachable!() };
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
            source: self.current_source_id,
            span: item.span,
            name: contract.name,
            kind: contract.kind,

            // Set later.
            bases: &[],
            linearized_bases: &[],

            ctor,
            fallback,
            receive,
            items: self.arena.alloc_slice_copy(&items),
        });
        debug_assert_eq!(Some(id), self.current_contract_id);
        self.current_contract_id = None;
        id
    }

    fn lower_item(&mut self, item: &'ast ast::Item<'ast>) -> hir::ItemId {
        let item_id = match &item.kind {
            ast::ItemKind::Pragma(_) | ast::ItemKind::Import(_) | ast::ItemKind::Using(_) => {
                unreachable!()
            }
            ast::ItemKind::Contract(i) => hir::ItemId::Contract(self.lower_contract(item, i)),
            ast::ItemKind::Function(i) => hir::ItemId::Function(self.lower_function(item, i)),
            ast::ItemKind::Variable(i) => hir::ItemId::Variable(self.lower_variable(i)),
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
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            kind,
            modifiers: &[],
            virtual_,
            overrides: &[],
            visibility,
            state_mutability,
            params,
            returns,
            body: None,
        })
    }

    fn lower_variables(
        &mut self,
        variables: &[ast::VariableDefinition<'_>],
    ) -> &'hir [hir::VariableId] {
        let mut vars = SmallVec::<[_; 16]>::new();
        for var in variables {
            vars.push(self.lower_variable(var));
        }
        self.arena.alloc_slice_copy(&vars)
    }

    fn lower_variable(&mut self, i: &ast::VariableDefinition<'_>) -> hir::VariableId {
        lower_variable(&mut self.hir, i, self.current_source_id, self.current_contract_id)
    }

    fn lower_struct(&mut self, item: &ast::Item<'_>, i: &ast::ItemStruct<'_>) -> hir::StructId {
        // handled later: fields
        let ast::ItemStruct { name, fields: _ } = *i;
        self.hir.structs.push(hir::Struct {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            fields: &[],
        })
    }

    fn lower_enum(&mut self, item: &ast::Item<'_>, i: &ast::ItemEnum<'_>) -> hir::EnumId {
        let ast::ItemEnum { name, ref variants } = *i;
        self.hir.enums.push(hir::Enum {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            variants: self.arena.alloc_slice_copy(variants),
        })
    }

    fn lower_udvt(&mut self, item: &ast::Item<'_>, i: &ast::ItemUdvt<'_>) -> hir::UdvtId {
        let ast::ItemUdvt { name, ref ty } = *i;
        self.hir.udvts.push(hir::Udvt {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            ty: hir::Type {
                span: ty.span,
                kind: if let ast::TypeKind::Elementary(kind) = ty.kind {
                    hir::TypeKind::Elementary(kind)
                } else {
                    let msg = "the underlying type of UDVTs must be an elementary value type";
                    hir::TypeKind::Err(self.dcx().err(msg).span(ty.span).emit())
                },
            },
        })
    }

    fn lower_error(&mut self, item: &ast::Item<'_>, i: &ast::ItemError<'_>) -> hir::ErrorId {
        // handled later: parameters
        let ast::ItemError { name, parameters: _ } = *i;
        self.hir.errors.push(hir::Error {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            parameters: &[],
        })
    }

    fn lower_event(&mut self, item: &ast::Item<'_>, i: &ast::ItemEvent<'_>) -> hir::EventId {
        // handled later: parameters
        let ast::ItemEvent { name, parameters: _, anonymous } = *i;
        self.hir.events.push(hir::Event {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            anonymous,
            parameters: &[],
        })
    }
}

pub(super) fn lower_variable(
    hir: &mut hir::Hir<'_>,
    i: &ast::VariableDefinition<'_>,
    source: SourceId,
    contract: Option<ContractId>,
) -> hir::VariableId {
    // handled later: ty, override_, initializer
    let ast::VariableDefinition {
        span,
        ty: _,
        visibility,
        mutability,
        data_location,
        override_: _,
        indexed,
        name,
        initializer: _,
    } = *i;
    hir.variables.push(hir::Variable {
        source,
        contract,
        span,
        ty: hir::Type::DUMMY,
        name,
        visibility,
        mutability,
        data_location,
        indexed,
        initializer: None,
    })
}

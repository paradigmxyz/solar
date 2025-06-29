use crate::{
    hir::{self, ContractId, SourceId},
    ParsedSource,
};
use solar_ast as ast;
use solar_data_structures::{index::IndexVec, smallvec::SmallVec};

impl<'ast> super::LoweringContext<'_, 'ast, '_> {
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
        let id = self.hir.contracts.push(hir::Contract {
            source: self.current_source_id,
            span: item.span,
            name: contract.name,
            kind: contract.kind,

            // Set later.
            bases: &mut [],
            bases_args: &[],
            linearized_bases: &[],
            linearized_bases_args: &[],

            ctor: None,
            fallback: None,
            receive: None,
            items: &[],
        });
        let prev_contract_id = Option::replace(&mut self.current_contract_id, id);
        debug_assert_eq!(prev_contract_id, None);

        let mut items = SmallVec::<[_; 16]>::new();
        for item in contract.body.iter() {
            let id = match &item.kind {
                ast::ItemKind::Pragma(_)
                | ast::ItemKind::Import(_)
                | ast::ItemKind::Contract(_) => unreachable!("illegal item in contract body"),
                ast::ItemKind::Using(_) => continue,
                ast::ItemKind::Variable(_) => {
                    let hir::ItemId::Variable(id) = self.lower_item(item) else { unreachable!() };
                    items.push(hir::ItemId::Variable(id));
                    if let Some(getter) = self.hir.variable(id).getter {
                        items.push(getter.into());
                    }
                    continue;
                }
                ast::ItemKind::Function(_)
                | ast::ItemKind::Struct(_)
                | ast::ItemKind::Enum(_)
                | ast::ItemKind::Udvt(_)
                | ast::ItemKind::Error(_)
                | ast::ItemKind::Event(_) => self.lower_item(item),
            };
            items.push(id);
        }
        self.hir.contracts[id].items = self.arena.alloc_slice_copy(&items);

        self.current_contract_id = prev_contract_id;

        id
    }

    fn lower_item(&mut self, item: &'ast ast::Item<'ast>) -> hir::ItemId {
        let item_id = match &item.kind {
            ast::ItemKind::Pragma(_) | ast::ItemKind::Import(_) | ast::ItemKind::Using(_) => {
                unreachable!()
            }
            ast::ItemKind::Contract(i) => hir::ItemId::Contract(self.lower_contract(item, i)),
            ast::ItemKind::Function(i) => hir::ItemId::Function(self.lower_function(item, i)),
            ast::ItemKind::Variable(i) => {
                let kind = if self.current_contract_id.is_some() {
                    hir::VarKind::State
                } else {
                    hir::VarKind::Global
                };
                hir::ItemId::Variable(self.lower_variable(i, kind))
            }
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
        // handled later: parameters, body, modifiers, override_, returns
        let ast::ItemFunction { kind, ref header, body: _, body_span } = *i;
        let ast::FunctionHeader {
            span: _,
            name,
            parameters: _,
            visibility,
            state_mutability,
            modifiers: _,
            virtual_,
            ref override_,
            returns: _,
        } = *header;
        self.hir.functions.push(hir::Function {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            kind,
            gettee: None,
            modifiers: &[],
            marked_virtual: virtual_,
            virtual_: virtual_
                || self
                    .current_contract_id
                    .is_some_and(|id| self.hir.contract(id).kind.is_interface()),
            override_: override_.is_some(),
            overrides: &[],
            visibility: visibility.unwrap_or_else(|| {
                let is_free = self.current_contract_id.is_none();
                if kind.is_modifier() || is_free {
                    ast::Visibility::Internal
                } else {
                    ast::Visibility::Public
                }
            }),
            state_mutability,
            parameters: &[],
            returns: &[],
            body: None,
            body_span,
        })
    }

    fn lower_variable(
        &mut self,
        i: &ast::VariableDefinition<'_>,
        kind: hir::VarKind,
    ) -> hir::VariableId {
        lower_variable_partial(
            &mut self.hir,
            i,
            self.current_source_id,
            self.current_contract_id,
            None,
            kind,
        )
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
        // Handled later: ty
        let ast::ItemUdvt { name, ty: _ } = *i;
        self.hir.udvts.push(hir::Udvt {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span: item.span,
            name,
            ty: hir::Type::DUMMY,
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

/// Lowers an AST `VariableDefinition` to a HIR `Variable`.
pub(super) fn lower_variable_partial(
    hir: &mut hir::Hir<'_>,
    i: &ast::VariableDefinition<'_>,
    source: SourceId,
    contract: Option<ContractId>,
    function: Option<hir::FunctionId>,
    kind: hir::VarKind,
) -> hir::VariableId {
    // handled later: ty, override_, initializer
    let ast::VariableDefinition {
        span,
        ty: _,
        visibility,
        mutability,
        data_location,
        ref override_,
        indexed,
        name,
        initializer: _,
    } = *i;
    let id = hir.variables.push(hir::Variable {
        source,
        contract,
        function,
        span,
        kind,
        ty: hir::Type::DUMMY,
        name,
        visibility,
        mutability,
        data_location,
        override_: override_.is_some(),
        overrides: &[],
        indexed,
        initializer: None,
        getter: None,
    });
    let v = hir.variable(id);
    if v.is_state_variable() && v.is_public() {
        hir.variables[id].getter = Some(generate_partial_getter(hir, id));
    }
    id
}

fn generate_partial_getter(hir: &mut hir::Hir<'_>, id: hir::VariableId) -> hir::FunctionId {
    let hir::Variable {
        source,
        contract,
        function: _,
        span,
        kind,
        ty: _,
        name,
        visibility,
        mutability: _,
        data_location: _,
        override_,
        overrides,
        indexed,
        initializer: _,
        getter,
    } = *hir.variable(id);
    debug_assert!(!indexed);
    debug_assert_eq!(visibility, Some(ast::Visibility::Public));
    debug_assert!(kind.is_state());
    debug_assert!(getter.is_none());
    hir.functions.push(hir::Function {
        source,
        contract,
        span,
        name,
        kind: ast::FunctionKind::Function,
        visibility: ast::Visibility::External,
        state_mutability: ast::StateMutability::View,
        modifiers: &[],
        marked_virtual: false,
        virtual_: false,
        override_,
        overrides,
        parameters: &[],
        returns: &[],
        body: None,
        gettee: Some(id),
        body_span: span,
    })
}

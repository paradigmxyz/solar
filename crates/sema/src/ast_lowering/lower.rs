use crate::hir::{self, ContractId, SourceId};
use solar_ast as ast;
use solar_ast::visit::Visit;
use solar_data_structures::{BumpExt, Never, smallvec::SmallVec};
use std::ops::ControlFlow;

impl<'gcx> super::LoweringContext<'gcx> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn lower_sources(&mut self) {
        let hir_sources = self.sources.iter_enumerated().map(|(id, source)| {
            let mut hir_source = hir::Source {
                file: source.file.clone(),
                imports: self.arena.alloc_slice_copy(&source.imports),
                items: &[],
                usings: &[],
                docs: &[],
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
                        | ast::ItemKind::Event(_) => {
                            let item_id = self.lower_item(item);
                            items.push(item_id);
                            if matches!(item.kind, ast::ItemKind::Function(_)) {
                                self.collect_yul_functions_in_item(item, &mut items);
                            }
                        }
                    }
                }
                hir_source.items = self.arena.alloc_slice_copy(&items);
            };
            hir_source
        });
        self.hir.sources = hir_sources.collect();
    }

    /// Lowers documentation comments from AST to HIR.
    ///
    /// Validation happens after parameters are lowered.
    fn lower_item_docs(&mut self, item: &'gcx ast::Item<'gcx>, item_id: hir::ItemId) -> hir::DocId {
        if item.docs.is_empty() {
            return hir::DocId::EMPTY;
        }
        let docs = self.copy_doc_comments(&item.docs);
        self.lower_docs(docs, item_id)
    }

    fn copy_doc_comments(&self, docs: &ast::DocComments<'_>) -> ast::DocComments<'gcx> {
        let docs = docs.iter().map(|doc| ast::DocComment {
            kind: doc.kind,
            span: doc.span,
            symbol: doc.symbol,
            natspec: self.arena.bump().alloc_thin_slice_copy((), doc.natspec),
        });
        self.arena.bump().alloc_from_iter_thin((), docs).into()
    }

    fn lower_docs(&mut self, docs: ast::DocComments<'gcx>, item_id: hir::ItemId) -> hir::DocId {
        if docs.is_empty() {
            return hir::DocId::EMPTY;
        }

        self.hir.docs.push(hir::Doc {
            source: self.current_source_id,
            item: item_id,
            ast_comments: docs,
        })
    }

    fn lower_contract(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        contract: &'gcx ast::ItemContract<'gcx>,
    ) -> hir::ContractId {
        let id = self.hir.contracts.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Contract(id));
        let pushed_id = self.hir.contracts.push(hir::Contract {
            source: self.current_source_id,
            span: item.span,
            name: contract.name,
            kind: contract.kind,

            // Set later.
            doc,
            bases: &mut [],
            bases_args: &[],
            linearized_bases: &[],
            linearized_bases_args: &[],

            ctor: None,
            fallback: None,
            receive: None,
            items: &[],
            usings: &[],
        });
        debug_assert_eq!(id, pushed_id);
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
            if matches!(item.kind, ast::ItemKind::Function(_)) {
                self.collect_yul_functions_in_item(item, &mut items);
            }
        }
        self.hir.contracts[id].items = self.arena.alloc_slice_copy(&items);

        self.current_contract_id = prev_contract_id;

        id
    }

    fn lower_item(&mut self, item: &'gcx ast::Item<'gcx>) -> hir::ItemId {
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
                hir::ItemId::Variable(self.lower_variable(item, i, kind))
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

    fn collect_yul_functions_in_item(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        items: &mut SmallVec<[hir::ItemId; 16]>,
    ) {
        let ast::ItemKind::Function(function) = &item.kind else { return };
        let Some(body) = &function.body else { return };
        let mut collector = YulFunctionCollector { lcx: self, items };
        let _ = collector.visit_block(body);
    }

    fn lower_function(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
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
        let id = self.hir.functions.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Function(id));
        let pushed_id = self.hir.functions.push(hir::Function {
            source: self.current_source_id,
            doc,
            contract: self.current_contract_id,
            span: item.span,
            name,
            kind,
            is_yul: false,
            gettee: None,
            modifiers: &[],
            marked_virtual: virtual_.is_some(),
            virtual_: virtual_.is_some()
                || self
                    .current_contract_id
                    .is_some_and(|id| self.hir.contract(id).kind.is_interface()),
            override_: override_.is_some(),
            overrides: &[],
            visibility: visibility.map(|vis| vis.data).unwrap_or_else(|| {
                let is_free = self.current_contract_id.is_none();
                if kind.is_modifier() || is_free {
                    ast::Visibility::Internal
                } else {
                    ast::Visibility::Public
                }
            }),
            state_mutability: state_mutability
                .map(|s| s.data)
                .unwrap_or(ast::StateMutability::NonPayable),
            parameters: &[],
            returns: &[],
            body: None,
            body_span,
        });
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_yul_function_decl(
        &mut self,
        function: &'gcx ast::yul::Function<'gcx>,
    ) -> hir::FunctionId {
        let id = self.hir.functions.next_idx();
        let span = function.name.span.with_hi(function.body.span.hi());
        let pushed_id = self.hir.functions.push(hir::Function {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
            contract: self.current_contract_id,
            span,
            name: Some(function.name),
            kind: hir::FunctionKind::Function,
            is_yul: true,
            visibility: hir::Visibility::Private,
            state_mutability: hir::StateMutability::NonPayable,
            modifiers: &[],
            marked_virtual: false,
            virtual_: false,
            override_: false,
            overrides: &[],
            parameters: &[],
            returns: &[],
            body: None,
            body_span: function.body.span,
            gettee: None,
        });
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_variable(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        i: &ast::VariableDefinition<'_>,
        kind: hir::VarKind,
    ) -> hir::VariableId {
        let id = self.hir.variables.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Variable(id));
        let pushed_id = lower_variable_partial(
            &mut self.hir,
            i,
            self.current_source_id,
            self.current_contract_id,
            None,
            kind,
            doc,
        );
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_struct(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        i: &ast::ItemStruct<'_>,
    ) -> hir::StructId {
        // handled later: fields
        let ast::ItemStruct { name, fields: _ } = *i;
        let id = self.hir.structs.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Struct(id));
        let pushed_id = self.hir.structs.push(hir::Struct {
            source: self.current_source_id,
            doc,
            contract: self.current_contract_id,
            span: item.span,
            name,
            fields: &[],
        });
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_enum(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemEnum<'_>) -> hir::EnumId {
        let ast::ItemEnum { name, ref variants } = *i;
        let id = self.hir.enums.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Enum(id));
        let pushed_id = self.hir.enums.push(hir::Enum {
            source: self.current_source_id,
            doc,
            contract: self.current_contract_id,
            span: item.span,
            name,
            variants: self.arena.alloc_slice_copy(variants),
        });
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_udvt(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemUdvt<'_>) -> hir::UdvtId {
        // Handled later: ty
        let ast::ItemUdvt { name, ty: _ } = *i;
        let id = self.hir.udvts.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Udvt(id));
        let pushed_id = self.hir.udvts.push(hir::Udvt {
            source: self.current_source_id,
            doc,
            contract: self.current_contract_id,
            span: item.span,
            name,
            ty: hir::Type::DUMMY,
        });
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_error(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemError<'_>) -> hir::ErrorId {
        // handled later: parameters
        let ast::ItemError { name, parameters: _ } = *i;
        let id = self.hir.errors.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Error(id));
        let pushed_id = self.hir.errors.push(hir::Error {
            source: self.current_source_id,
            doc,
            contract: self.current_contract_id,
            span: item.span,
            name,
            parameters: &[],
        });
        debug_assert_eq!(id, pushed_id);
        id
    }

    fn lower_event(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemEvent<'_>) -> hir::EventId {
        // handled later: parameters
        let ast::ItemEvent { name, parameters: _, anonymous } = *i;
        let id = self.hir.events.next_idx();
        let doc = self.lower_item_docs(item, hir::ItemId::Event(id));
        let pushed_id = self.hir.events.push(hir::Event {
            source: self.current_source_id,
            doc,
            contract: self.current_contract_id,
            span: item.span,
            name,
            anonymous,
            parameters: &[],
        });
        debug_assert_eq!(id, pushed_id);
        id
    }
}

struct YulFunctionCollector<'a, 'gcx> {
    lcx: &'a mut super::LoweringContext<'gcx>,
    items: &'a mut SmallVec<[hir::ItemId; 16]>,
}

impl<'gcx> Visit<'gcx> for YulFunctionCollector<'_, 'gcx> {
    type BreakValue = Never;

    // Yul function definitions only appear in statements. Short-circuit expressions.
    #[inline]
    fn visit_expr(&mut self, _expr: &'gcx ast::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }

    fn visit_yul_function(
        &mut self,
        function: &'gcx ast::yul::Function<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        let id = self.lcx.lower_yul_function_decl(function);
        self.lcx.yul_functions.insert(super::yul_function_key(function), id);
        self.items.push(hir::ItemId::Function(id));
        self.visit_yul_block(&function.body)
    }

    // Yul function definitions only appear in statements. Short-circuit expressions.
    #[inline]
    fn visit_yul_expr(
        &mut self,
        _expr: &'gcx ast::yul::Expr<'gcx>,
    ) -> ControlFlow<Self::BreakValue> {
        ControlFlow::Continue(())
    }
}

/// Lowers an AST `VariableDefinition` to a HIR `Variable`.
pub(super) fn lower_variable_partial(
    hir: &mut hir::Hir<'_>,
    i: &ast::VariableDefinition<'_>,
    source: SourceId,
    contract: Option<ContractId>,
    parent: Option<hir::ItemId>,
    kind: hir::VarKind,
    doc: hir::DocId,
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
        doc,
        contract,
        parent,
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
        doc: _,
        contract,
        parent: _,
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
        doc: hir::DocId::EMPTY, // Getters don't have docs
        contract,
        span,
        name,
        kind: ast::FunctionKind::Function,
        is_yul: false,
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

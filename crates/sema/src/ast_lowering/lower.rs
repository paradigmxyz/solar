use crate::hir::{self, ContractId, SourceId};
use solar_ast as ast;
use solar_data_structures::smallvec::SmallVec;

impl<'gcx> super::LoweringContext<'gcx> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn lower_sources(&mut self) {
        let hir_sources = self.sources.iter_enumerated().map(|(id, source)| {
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
                        | ast::ItemKind::Variable(_)
                        | ast::ItemKind::Struct(_)
                        | ast::ItemKind::Enum(_)
                        | ast::ItemKind::Udvt(_)
                        | ast::ItemKind::Error(_)
                        | ast::ItemKind::Event(_) => items.push(self.lower_item(item)),
                        ast::ItemKind::Function(_) => {
                            items.push(self.lower_item(item));
                            self.collect_yul_functions_in_item(item);
                        }
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
        contract: &'gcx ast::ItemContract<'gcx>,
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
            if matches!(item.kind, ast::ItemKind::Function(_)) {
                self.collect_yul_functions_in_item(item);
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

    fn collect_yul_functions_in_item(&mut self, item: &'gcx ast::Item<'gcx>) {
        let ast::ItemKind::Function(function) = &item.kind else { return };
        if let Some(body) = &function.body {
            self.collect_yul_functions_in_stmts(body.stmts);
        }
    }

    fn collect_yul_functions_in_stmts(&mut self, stmts: &'gcx [ast::Stmt<'gcx>]) {
        for stmt in stmts {
            self.collect_yul_functions_in_stmt(stmt);
        }
    }

    fn collect_yul_functions_in_stmt(&mut self, stmt: &'gcx ast::Stmt<'gcx>) {
        match &stmt.kind {
            ast::StmtKind::Assembly(assembly) => {
                self.collect_yul_functions_in_block(&assembly.block);
            }
            ast::StmtKind::Block(block) | ast::StmtKind::UncheckedBlock(block) => {
                self.collect_yul_functions_in_stmts(block.stmts);
            }
            ast::StmtKind::DoWhile(body, _)
            | ast::StmtKind::While(_, body)
            | ast::StmtKind::If(_, body, None) => {
                self.collect_yul_functions_in_stmt(body);
            }
            ast::StmtKind::If(_, true_, Some(false_)) => {
                self.collect_yul_functions_in_stmt(true_);
                self.collect_yul_functions_in_stmt(false_);
            }
            ast::StmtKind::For { init, body, .. } => {
                if let Some(init) = init {
                    self.collect_yul_functions_in_stmt(init);
                }
                self.collect_yul_functions_in_stmt(body);
            }
            ast::StmtKind::Try(try_) => {
                for clause in try_.clauses.iter() {
                    self.collect_yul_functions_in_stmts(clause.block.stmts);
                }
            }
            ast::StmtKind::Break
            | ast::StmtKind::Continue
            | ast::StmtKind::DeclSingle(_)
            | ast::StmtKind::DeclMulti(..)
            | ast::StmtKind::Emit(..)
            | ast::StmtKind::Expr(_)
            | ast::StmtKind::Placeholder
            | ast::StmtKind::Return(_)
            | ast::StmtKind::Revert(..) => {}
        }
    }

    fn collect_yul_functions_in_block(&mut self, block: &'gcx ast::yul::Block<'gcx>) {
        for stmt in block.stmts.iter() {
            match &stmt.kind {
                ast::yul::StmtKind::Block(block) => self.collect_yul_functions_in_block(block),
                ast::yul::StmtKind::For(for_) => {
                    self.collect_yul_functions_in_block(&for_.init);
                    self.collect_yul_functions_in_block(&for_.step);
                    self.collect_yul_functions_in_block(&for_.body);
                }
                ast::yul::StmtKind::Switch(switch) => {
                    for case in switch.cases.iter() {
                        self.collect_yul_functions_in_block(&case.body);
                    }
                }
                ast::yul::StmtKind::If(_, block) => self.collect_yul_functions_in_block(block),
                ast::yul::StmtKind::FunctionDef(function) => {
                    self.collect_yul_function(function);
                    self.collect_yul_functions_in_block(&function.body);
                }
                ast::yul::StmtKind::AssignSingle(..)
                | ast::yul::StmtKind::AssignMulti(..)
                | ast::yul::StmtKind::Break
                | ast::yul::StmtKind::Continue
                | ast::yul::StmtKind::Expr(_)
                | ast::yul::StmtKind::Leave
                | ast::yul::StmtKind::VarDecl(..) => {}
            }
        }
    }

    fn collect_yul_function(&mut self, function: &'gcx ast::yul::Function<'gcx>) {
        let span = Self::yul_function_span(function);
        let id = self.hir.functions.push(hir::Function {
            source: self.current_source_id,
            contract: self.current_contract_id,
            span,
            name: Some(function.name),
            kind: hir::FunctionKind::Function,
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
        self.yul_function_ids.insert(span, id);
    }

    pub(super) fn yul_function_span(function: &ast::yul::Function<'_>) -> solar_interface::Span {
        function.name.span.with_hi(function.body.span.hi())
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

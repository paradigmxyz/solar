use crate::hir::{self, ContractId, SourceId};
use solar_ast as ast;
use solar_data_structures::map::FxHashSet;
use solar_data_structures::smallvec::SmallVec;
use solar_interface::{Span, Symbol};

bitflags::bitflags! {
    /// Tracks which documentation tags are locally defined in `merge_natspec_tags`.
    struct LocalTags: u8 {
        const NOTICE = 1 << 0;
        const DEV    = 1 << 1;
        const TITLE  = 1 << 2;
        const AUTHOR = 1 << 3;
    }

    /// Tag permissions for different item types in natspec validation.
    #[derive(Clone, Copy)]
    struct TagPermissions: u8 {
        const TITLE_AUTHOR = 1 << 0;
        const INHERITDOC   = 1 << 1;
        const PARAM        = 1 << 2;
        const RETURN       = 1 << 3;
    }

    /// Tracks which tags have been seen during validation.
    #[derive(Clone, Copy, Default)]
    struct SeenTags: u8 {
        const TITLE      = 1 << 0;
        const AUTHOR     = 1 << 1;
        const INHERITDOC = 1 << 2;
    }
}

impl TagPermissions {
    fn from_item_id(item_id: hir::ItemId) -> Self {
        let mut perms = Self::empty();

        if matches!(
            item_id,
            hir::ItemId::Contract(_) | hir::ItemId::Struct(_) | hir::ItemId::Enum(_)
        ) {
            perms.insert(Self::TITLE_AUTHOR);
        }

        if matches!(
            item_id,
            hir::ItemId::Function(_)
                | hir::ItemId::Variable(_)
                | hir::ItemId::Event(_)
                | hir::ItemId::Error(_)
        ) {
            perms.insert(Self::PARAM);
        }

        if matches!(item_id, hir::ItemId::Function(_) | hir::ItemId::Variable(_)) {
            perms.insert(Self::INHERITDOC);
        }

        if !matches!(item_id, hir::ItemId::Event(_)) {
            perms.insert(Self::RETURN);
        }

        perms
    }
}

impl<'gcx> super::LoweringContext<'gcx> {
    #[instrument(level = "debug", skip_all)]
    pub(super) fn lower_sources(&mut self) {
        let hir_sources = self.sources.iter_enumerated().map(|(id, source)| {
            let mut hir_source = hir::Source {
                file: source.file.clone(),
                imports: self.arena.alloc_slice_copy(&source.imports),
                items: &[],
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
        item: &'gcx ast::Item<'gcx>,
        contract: &'gcx ast::ItemContract<'gcx>,
    ) -> hir::ContractId {
        let id = self.hir.contracts.push(hir::Contract {
            source: self.current_source_id,
            span: item.span,
            name: contract.name,
            kind: contract.kind,

            // Set later.
            doc: hir::DocId::EMPTY,
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

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Contract(id));
        self.hir.contracts[id].doc = doc_id;

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

    fn lower_function(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        i: &ast::ItemFunction<'_>,
    ) -> hir::FunctionId {
        // handled later: doc, parameters, body, modifiers, override_, returns
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
        let id = self.hir.functions.push(hir::Function {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
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
        });

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Function(id));
        self.hir.functions[id].doc = doc_id;

        id
    }

    fn lower_variable(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        i: &ast::VariableDefinition<'_>,
        kind: hir::VarKind,
    ) -> hir::VariableId {
        let id = lower_variable_partial(
            &mut self.hir,
            i,
            self.current_source_id,
            self.current_contract_id,
            None,
            kind,
        );

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Variable(id));
        self.hir.variables[id].doc = Some(doc_id);
        id
    }

    fn lower_struct(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        i: &ast::ItemStruct<'_>,
    ) -> hir::StructId {
        // handled later: doc, fields
        let ast::ItemStruct { name, fields: _ } = *i;
        let id = self.hir.structs.push(hir::Struct {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
            contract: self.current_contract_id,
            span: item.span,
            name,
            fields: &[],
        });

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Struct(id));
        self.hir.structs[id].doc = doc_id;

        id
    }

    fn lower_enum(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemEnum<'_>) -> hir::EnumId {
        // handled later: doc
        let ast::ItemEnum { name, ref variants } = *i;
        let id = self.hir.enums.push(hir::Enum {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
            contract: self.current_contract_id,
            span: item.span,
            name,
            variants: self.arena.alloc_slice_copy(variants),
        });

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Enum(id));
        self.hir.enums[id].doc = doc_id;

        id
    }

    fn lower_udvt(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemUdvt<'_>) -> hir::UdvtId {
        // Handled later: doc, ty
        let ast::ItemUdvt { name, ty: _ } = *i;
        let id = self.hir.udvts.push(hir::Udvt {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
            contract: self.current_contract_id,
            span: item.span,
            name,
            ty: hir::Type::DUMMY,
        });

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Udvt(id));
        self.hir.udvts[id].doc = doc_id;

        id
    }

    fn lower_error(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemError<'_>) -> hir::ErrorId {
        // handled later: doc, parameters
        let ast::ItemError { name, parameters: _ } = *i;
        let id = self.hir.errors.push(hir::Error {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
            contract: self.current_contract_id,
            span: item.span,
            name,
            parameters: &[],
        });

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Error(id));
        self.hir.errors[id].doc = doc_id;

        id
    }

    fn lower_event(&mut self, item: &'gcx ast::Item<'gcx>, i: &ast::ItemEvent<'_>) -> hir::EventId {
        // handled later: doc, parameters
        let ast::ItemEvent { name, parameters: _, anonymous } = *i;
        let id = self.hir.events.push(hir::Event {
            source: self.current_source_id,
            doc: hir::DocId::EMPTY,
            contract: self.current_contract_id,
            span: item.span,
            name,
            anonymous,
            parameters: &[],
        });

        // Lower docs after `ItemId` is available
        let doc_id = self.lower_docs(&item.docs, hir::ItemId::Event(id));
        self.hir.events[id].doc = doc_id;

        id
    }

    /// Lowers documentation comments from AST to HIR.
    ///
    /// Simply stores a reference to the AST doc comments. Validation happens after parameters are
    /// lowered.
    fn lower_docs(
        &mut self,
        docs: &'gcx ast::DocComments<'gcx>,
        item_id: hir::ItemId,
    ) -> hir::DocId {
        if docs.is_empty() {
            return hir::DocId::EMPTY;
        }

        self.hir.docs.push(hir::Doc {
            source: self.current_source_id,
            item: item_id,
            ast_comments: docs,
            comments: &[],
        })
    }

    /// Validates all NatSpec tags after parameters/returns have been lowered.
    ///
    /// This must be called after symbol resolution, since parameter validation requires the actual
    /// parameter lists to be populated.
    pub(super) fn validate_natspec_tags(&mut self) {
        for doc_id in self.hir.doc_ids() {
            if doc_id.is_empty() {
                continue;
            }
            let doc = self.hir.doc(doc_id);
            self.validate_item_natspec(doc.ast_comments, doc.item);
        }
    }

    /// Validates NatSpec tags for the given item.
    ///
    /// Checks:
    /// - Tag applicability
    /// - Duplicate tags
    /// - Parameter references
    fn validate_item_natspec(&mut self, docs: &'gcx ast::DocComments<'gcx>, item_id: hir::ItemId) {
        use ast::NatSpecKind;

        let dcx = self.dcx();
        let item = self.hir.item(item_id);
        let permissions = TagPermissions::from_item_id(item_id);

        #[derive(Default)]
        struct ValidationState {
            seen_tags: SeenTags,
            seen_params: SmallVec<[(Symbol, Span); 6]>,
            return_count: usize,
        }
        let mut state = ValidationState::default();

        // Get item parameters and returns for validation
        let parameters = item.parameters();
        let returns = if let hir::Item::Function(f) = item { Some(f.returns) } else { None };

        for doc_comment in docs.iter() {
            for natspec_item in doc_comment.natspec.iter() {
                let tag_span = natspec_item.span;

                match &natspec_item.kind {
                    NatSpecKind::Notice
                    | NatSpecKind::Dev
                    | NatSpecKind::Custom { .. }
                    | NatSpecKind::Internal { .. } => {
                        // Allowed on all items, no validation needed
                    }
                    NatSpecKind::Title => {
                        self.validate_tag_once(
                            "@title",
                            tag_span,
                            permissions.contains(TagPermissions::TITLE_AUTHOR),
                            &mut state.seen_tags,
                            SeenTags::TITLE,
                            item.description(),
                        );
                    }
                    NatSpecKind::Author => {
                        self.validate_tag_once(
                            "@author",
                            tag_span,
                            permissions.contains(TagPermissions::TITLE_AUTHOR),
                            &mut state.seen_tags,
                            SeenTags::AUTHOR,
                            item.description(),
                        );
                    }
                    NatSpecKind::Inheritdoc { contract } => {
                        self.validate_tag_once(
                            "@inheritdoc",
                            tag_span,
                            permissions.contains(TagPermissions::INHERITDOC),
                            &mut state.seen_tags,
                            SeenTags::INHERITDOC,
                            item.description(),
                        );

                        // Validate that the contract exists
                        if permissions.contains(TagPermissions::INHERITDOC) {
                            self.validate_inheritdoc_contract(contract, tag_span, item_id);
                        }
                    }
                    NatSpecKind::Param { name } => {
                        if !self.validate_tag_permission(
                            "@param",
                            tag_span,
                            permissions.contains(TagPermissions::PARAM),
                            item.description(),
                        ) {
                            continue;
                        }

                        // Check for duplicate `@param` tags
                        if let Some(&(_, prev_span)) =
                            state.seen_params.iter().find(|(sym, _)| *sym == name.name)
                        {
                            dcx.err(format!(
                                "duplicate documentation for parameter '{}'",
                                name.name
                            ))
                            .span(tag_span)
                            .span_note(prev_span, "previous documentation here")
                            .emit();
                            continue;
                        }
                        state.seen_params.push((name.name, tag_span));

                        // Validate parameter exists
                        if let Some(params) = parameters {
                            let param_name = name.name;
                            let param_exists = params.iter().any(|&param_id| {
                                self.hir
                                    .variable(param_id)
                                    .name
                                    .is_some_and(|n| n.name == param_name)
                            });

                            if !param_exists {
                                self.dcx()
                                    .err(format!(
                                        "documentation tag `@param` references non-existent parameter '{param_name}'"
                                    ))
                                    .span(tag_span)
                                    .emit();
                            }
                        }
                    }
                    NatSpecKind::Return { .. } => {
                        if !self.validate_tag_permission(
                            "@return",
                            tag_span,
                            permissions.contains(TagPermissions::RETURN),
                            item.description(),
                        ) {
                            continue;
                        }

                        state.return_count += 1;

                        // Validate return count if this is a function
                        if let Some(rets) = returns
                            && state.return_count > rets.len()
                        {
                            dcx.err(format!(
                                "too many `@return` tags: function has {} return value{}, found {}",
                                rets.len(),
                                if rets.len() == 1 { "" } else { "s" },
                                state.return_count
                            ))
                            .span(tag_span)
                            .emit();
                        }
                    }
                }
            }
        }
    }

    /// Helper to validate that a tag is allowed for the item type.
    /// Returns `true` if the tag is allowed, `false` otherwise.
    fn validate_tag_permission(
        &self,
        tag_name: &str,
        tag_span: Span,
        allowed: bool,
        item_desc: &str,
    ) -> bool {
        if !allowed {
            self.dcx()
                .err(format!("documentation tag `{tag_name}` not valid for {item_desc}s"))
                .span(tag_span)
                .emit();
            false
        } else {
            true
        }
    }

    /// Helper to validate tags that can only be defined once.
    fn validate_tag_once(
        &self,
        tag_name: &str,
        tag_span: Span,
        allowed: bool,
        seen_tags: &mut SeenTags,
        tag_flag: SeenTags,
        item_desc: &str,
    ) {
        if !self.validate_tag_permission(tag_name, tag_span, allowed, item_desc) {
            return;
        }
        if seen_tags.contains(tag_flag) {
            self.dcx()
                .err(format!("documentation tag {tag_name} can only be given once"))
                .span(tag_span)
                .emit();
        }
        seen_tags.insert(tag_flag);
    }

    /// Validates that a contract referenced in `@inheritdoc` exists and contains a matching item.
    fn validate_inheritdoc_contract(
        &self,
        contract_ident: &solar_interface::Ident,
        tag_span: Span,
        item_id: hir::ItemId,
    ) {
        let dcx = self.dcx();

        // Get the source where this item is defined
        let source_id = match item_id {
            hir::ItemId::Function(id) => self.hir.function(id).source,
            hir::ItemId::Variable(id) => self.hir.variable(id).source,
            _ => return,
        };

        // Look up the contract in the source scope
        let Some(contract_id) = self.resolve_contract_in_source(contract_ident.name, source_id)
        else {
            dcx.err(format!(
                "documentation tag `@inheritdoc` references inexistent contract \"{}\"",
                contract_ident.name
            ))
            .span(tag_span)
            .emit();
            return;
        };

        // Verify that the contract contains a matching item that is overridden
        let overrides = match item_id {
            hir::ItemId::Function(id) => self.hir.function(id).overrides,
            hir::ItemId::Variable(id) => self.hir.variable(id).overrides,
            _ => return,
        };

        if !overrides.contains(&contract_id) {
            dcx.err(format!(
                "documentation tag `@inheritdoc` references contract \"{}\", but the contract does not contain an item that is overridden by this one",
                contract_ident.name
            ))
            .span(tag_span)
            .emit();
        }
    }

    /// Resolves a contract name within a source's scope.
    fn resolve_contract_in_source(
        &self,
        name: Symbol,
        source_id: hir::SourceId,
    ) -> Option<hir::ContractId> {
        let source_scope = &self.resolver.source_scopes[source_id];

        // Use the resolve method with an ident
        let ident = solar_interface::Ident { name, span: Span::DUMMY };
        source_scope.resolve(ident).and_then(|decls| {
            decls.iter().find_map(|decl| match decl.res {
                hir::Res::Item(hir::ItemId::Contract(id)) => Some(id),
                _ => None,
            })
        })
    }

    /// Resolves `@inheritdoc` tags across all documentation and converts AST natspec to HIR.
    ///
    /// This must be called after validation to ensure all references are valid.
    /// Populates `doc.natspec` for all docs, making it the single source of truth for natspec in
    /// HIR.
    pub(super) fn resolve_inheritdoc(&mut self) {
        for doc_id in self.hir.doc_ids() {
            if doc_id.is_empty() {
                continue;
            }

            let mut doc_items =
                self.hir.doc(doc_id).ast_comments.iter().flat_map(|c| c.natspec.iter());
            let has_inheritdoc =
                doc_items.any(|item| matches!(item.kind, ast::NatSpecKind::Inheritdoc { .. }));

            if has_inheritdoc {
                self.resolve_doc_inheritdoc(doc_id);
            } else {
                // No @inheritdoc - convert AST natspec directly to resolved tags
                self.resolve_doc_simple(doc_id);
            }
        }
    }

    /// Converts AST natspec to resolved tags for docs without `@inheritdoc`.
    fn resolve_doc_simple(&mut self, doc_id: hir::DocId) {
        let doc = self.hir.doc(doc_id);
        let mut resolved = SmallVec::<[hir::NatSpecItem; 6]>::new();

        for doc_comment in doc.ast_comments.iter() {
            for item in doc_comment.natspec.iter() {
                if let Some(resolved_item) = hir::NatSpecItem::from_ast(*item, doc_comment.symbol) {
                    resolved.push(resolved_item);
                }
            }
        }

        let resolved_slice = self.arena.alloc_slice_copy(&resolved);
        self.hir.docs[doc_id].comments = resolved_slice;
    }

    /// Resolves `@inheritdoc` for a single doc.
    fn resolve_doc_inheritdoc(&mut self, doc_id: hir::DocId) {
        if !self.hir.doc(doc_id).comments().is_empty() {
            return;
        }

        let doc = self.hir.doc(doc_id);
        let inheritdoc_contract =
            doc.ast_comments.iter().flat_map(|c| c.natspec.iter()).find_map(|item| {
                match &item.kind {
                    ast::NatSpecKind::Inheritdoc { contract } => Some(*contract),
                    _ => None,
                }
            });

        let Some(inheritdoc_contract) = inheritdoc_contract else {
            return;
        };

        // Look up the contract in the source scope
        let Some(contract_id) =
            self.resolve_contract_in_source(inheritdoc_contract.name, doc.source)
        else {
            return;
        };

        // Find the matching item in the contract and get inherited doc id.
        let inherited_item = self.find_inherited_item(doc.item, contract_id);
        let Some(inherited_item_id) = inherited_item else {
            return;
        };
        let inherited_doc_id = match inherited_item_id {
            hir::ItemId::Function(id) => self.hir.function(id).doc,
            hir::ItemId::Variable(id) => self.hir.variable(id).doc.unwrap_or(hir::DocId::EMPTY),
            _ => return,
        };

        // Recursively resolve if the inherited item also has `@inheritdoc`. Previous contract
        // linearization ensures no cycle deps.
        if !inherited_doc_id.is_empty() {
            let inherited_doc = self.hir.doc(inherited_doc_id);
            if inherited_doc.comments().is_empty() {
                let has_inheritdoc = inherited_doc
                    .ast_comments
                    .iter()
                    .flat_map(|c| c.natspec.iter())
                    .any(|item| matches!(item.kind, ast::NatSpecKind::Inheritdoc { .. }));

                if has_inheritdoc {
                    self.resolve_doc_inheritdoc(inherited_doc_id);
                }
            }
        }

        // Merge tags and store resolved natspec.
        let resolved = self.merge_natspec_tags(doc_id, inherited_doc_id);
        let resolved_slice = self.arena.alloc_slice_copy(&resolved);
        self.hir.docs[doc_id].comments = resolved_slice;
    }

    /// Finds the item in a contract that matches the given item (for inheritance).
    fn find_inherited_item(
        &self,
        item_id: hir::ItemId,
        contract_id: hir::ContractId,
    ) -> Option<hir::ItemId> {
        let item_name = self.hir.item(item_id).name()?;

        // Search through contract items for a match
        for base_item_id in self.hir.contract_item_ids(contract_id) {
            if let Some(base_name) = self.hir.item(base_item_id).name()
                && base_name.name == item_name.name
            {
                // For functions, we should also match signatures, but for now
                // just matching by name is sufficient as overrides are already validated
                return Some(base_item_id);
            }
        }

        None
    }

    /// Merges natspec tags from the current doc and inherited doc.
    ///
    /// Rules:
    /// - `@notice`, `@dev`, `@title`, `@author`: local overrides inherited
    /// - `@param`, `@return`: inherit missing ones, keep local ones
    /// - `@custom`: merge both
    /// - `@inheritdoc` is removed (replaced with actual tags)
    fn merge_natspec_tags(
        &self,
        doc_id: hir::DocId,
        inherited_doc_id: hir::DocId,
    ) -> SmallVec<[hir::NatSpecItem; 6]> {
        use hir::NatSpecKind as HirKind;

        let doc = self.hir.doc(doc_id);
        let mut merged = SmallVec::new();
        let mut local_tags = LocalTags::empty();
        let mut local_params = FxHashSet::<Symbol>::default();
        let mut local_returns = FxHashSet::<Option<Symbol>>::default();

        // Collect local tags, excluding `@inheritdoc`
        for doc_comment in doc.ast_comments.iter() {
            for item in doc_comment.natspec.iter() {
                if let Some(resolved_item) = hir::NatSpecItem::from_ast(*item, doc_comment.symbol) {
                    match &resolved_item.kind {
                        // Only inherit if not locally defined
                        HirKind::Notice => local_tags.insert(LocalTags::NOTICE),
                        HirKind::Dev => local_tags.insert(LocalTags::DEV),
                        HirKind::Title => local_tags.insert(LocalTags::TITLE),
                        HirKind::Author => local_tags.insert(LocalTags::AUTHOR),
                        HirKind::Param { name } => {
                            local_params.insert(name.name);
                        }
                        HirKind::Return { name } => {
                            local_returns.insert(name.map(|i| i.name));
                        }
                        // Always merge
                        HirKind::Custom { .. } | HirKind::Internal { .. } => {}
                    }
                    merged.push(resolved_item);
                }
            }
        }

        // If nothing to inherit, return directly.
        if inherited_doc_id.is_empty() {
            return merged;
        }

        // Inherit tags from base
        let inherited_doc = self.hir.doc(inherited_doc_id);
        let inherited_items = inherited_doc.comments();

        for item in inherited_items.iter() {
            let should_inherit = match &item.kind {
                // Only inherit if not locally defined
                HirKind::Notice => !local_tags.contains(LocalTags::NOTICE),
                HirKind::Dev => !local_tags.contains(LocalTags::DEV),
                HirKind::Title => !local_tags.contains(LocalTags::TITLE),
                HirKind::Author => !local_tags.contains(LocalTags::AUTHOR),
                HirKind::Param { name } => !local_params.contains(&name.name),
                HirKind::Return { name } => !local_returns.contains(&name.map(|n| n.name)),
                // Always merge
                HirKind::Custom { .. } | HirKind::Internal { .. } => true,
            };

            if should_inherit {
                merged.push(*item);
            }
        }

        merged
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
) -> hir::VariableId {
    // handled later: doc, ty, override_, initializer
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
        doc: None,
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

#[cfg(test)]
mod tests {
    use crate::Compiler;
    use solar_interface::{ColorChoice, Session, sym};
    use std::path::PathBuf;

    #[test]
    fn natspec_inheritdoc_comprehensive() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract Base {
    /**
     * @notice Base function notice
     * @dev Base function dev
     * @param x The x parameter from base
     * @param y The y parameter from base
     * @return success Whether the operation succeeded
     * @return value The result value
     * @custom:security Audited by Base team
     **/
    function foo(uint x, uint y) public virtual returns (bool success, uint value) {
        return (true, x + y);
    }
}

contract Child1 is Base {
    /// @inheritdoc Base
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, x * y);
    }
}

contract Child2 is Base {
    /// @notice Child2 notice - overrides base
    /// @dev Child2 dev - overrides base
    /// @inheritdoc Base
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (false, 0);
    }
}

contract Child3 is Base {
    /**
     * @param x The x parameter from child3
     * @inheritdoc Base
     **/
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, x);
    }
}

contract Child4 is Base {
    /// @return success Child4 override for success
    /// @inheritdoc Base
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (false, x + y);
    }
}

contract Child5 is Base {
    /**
    * @custom:audit Reviewed by Child5 auditor
    * @inheritdoc Base
    **/
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, y);
    }
}

contract GrandChild is Child1 {
    /// @inheritdoc Child1
    function foo(uint x, uint y) public virtual override returns (bool success, uint value) {
        return (true, x - y);
    }
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let gcx = c.gcx();

            let get_comments = |contract_name: &str, func_name: &str| {
                gcx.hir
                    .functions()
                    .find(|f| {
                        f.contract.is_some_and(|cid| {
                            gcx.hir.contract(cid).name.as_str() == contract_name
                                && f.name.is_some_and(|n| n.as_str() == func_name)
                        })
                    })
                    .map(|func| gcx.hir.doc(func.doc).comments())
                    .unwrap_or_else(|| panic!("{contract_name}.{func_name} not found"))
            };


            // Base contract natspec
            let base = get_comments("Base", "foo");
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Notice)), 1);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Dev)), 1);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Param { .. })), 2);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Return { .. })), 2);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Custom { .. })), 1);
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Notice), "Base function notice", "Base @notice");
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Dev), "Base function dev", "Base @dev");
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x), "x parameter from base", "Base @param x");
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "y"), "y parameter from base", "Base @param y");
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"), "operation succeeded", "Base @return success");
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value"), "result value", "Base @return value");
            assert_tag_contains(base, |k| matches!(k, NatSpecKind::Custom { name } if name.name.as_str() == "security"), "Audited by Base team", "Base @custom:security");

            // Simple inheritance (all tags)
            let c = get_comments("Child1", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Notice)), 1);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Dev)), 1);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 2);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Return { .. })), 2);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 1);

            // Local tag override (@notice, @dev)
            let c = get_comments("Child2", "foo");
            assert_tag_contains(c, |k| matches!(k, NatSpecKind::Notice), "Child2 notice", "Child2 @notice");
            assert_tag_contains(c, |k| matches!(k, NatSpecKind::Dev), "Child2 dev", "Child2 @dev");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 2);

            // Partial @param override
            let c = get_comments("Child3", "foo");
            assert_tag_contains(c, |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x), "from child3", "Child3 @param x");
            assert_tag_contains(c, |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "y"), "from base", "Child3 @param y");

            // Partial @return override
            let c = get_comments("Child4", "foo");
            assert_tag_contains(c, |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"), "Child4 override", "Child4 @return success");
            assert_tag_contains(c, |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value"), "result value", "Child4 @return value");

            // @custom tag merging
            let c = get_comments("Child5", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 2);
            assert!(c.iter().any(|i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "security")));
            assert!(c.iter().any(|i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "audit")));

            // Inheritance chain (`GrandChild` -> `Child` -> `Base`)
            let g = get_comments("GrandChild", "foo");
            assert_tag_contains(g, |k| matches!(k, NatSpecKind::Notice), "Base function notice", "GrandChild @notice");
            assert_eq!(count_tags(g, |k| matches!(k, NatSpecKind::Param { .. })), 2);
        });
    }

    fn lower_source(src: &str) -> Compiler {
        let sess =
            Session::builder().with_buffer_emitter(ColorChoice::Never).single_threaded().build();
        let mut compiler = Compiler::new(sess);

        let _ = compiler.enter_mut(|compiler| -> solar_interface::Result<_> {
            let mut parsing_context = compiler.parse();
            let file = compiler
                .sess()
                .source_map()
                .new_source_file(PathBuf::from("test.sol"), src.to_string())
                .unwrap();
            parsing_context.add_file(file);
            parsing_context.parse();
            let _ = compiler.lower_asts()?;
            Ok(())
        });

        compiler
    }

    fn count_tags(
        comments: &[crate::hir::NatSpecItem],
        kind: fn(&crate::hir::NatSpecKind) -> bool,
    ) -> usize {
        comments.iter().filter(|i| kind(&i.kind)).count()
    }

    fn assert_tag_contains(
        comments: &[crate::hir::NatSpecItem],
        kind_matcher: fn(&crate::hir::NatSpecKind) -> bool,
        expected_content: &str,
        msg: &str,
    ) {
        let tag = comments.iter().find(|i| kind_matcher(&i.kind)).expect(msg);
        assert!(tag.content().contains(expected_content), "{}: Got: {}", msg, tag.content());
    }
}

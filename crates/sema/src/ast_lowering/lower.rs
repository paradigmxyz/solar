use crate::hir::{self, ContractId, SourceId};
use solar_ast as ast;
use solar_data_structures::{
    map::{FxHashMap, FxHashSet},
    smallvec::SmallVec,
};
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
        match item_id {
            hir::ItemId::Contract(_) | hir::ItemId::Struct(_) | hir::ItemId::Enum(_) => {
                Self::TITLE_AUTHOR | Self::RETURN
            }
            hir::ItemId::Function(_) => Self::PARAM | Self::INHERITDOC | Self::RETURN,
            hir::ItemId::Variable(_) => Self::PARAM | Self::INHERITDOC,
            hir::ItemId::Event(_) => Self::PARAM,
            hir::ItemId::Error(_) => Self::PARAM,
            hir::ItemId::Udvt(_) => Self::PARAM,
        }
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

    /// Processes NatSpec tags, validating all of them, and resolving `@inheritdoc` references.
    ///
    /// Must be called after symbol resolution, since parameter validation requires the actual
    /// parameter lists to be populated.
    pub(super) fn validate_and_resolve_docs(&mut self) {
        let mut processed = FxHashSet::<hir::DocId>::default();
        let mut contract_cache =
            FxHashMap::<(Symbol, hir::SourceId), Option<hir::ContractId>>::default();

        for doc_id in self.hir.doc_ids() {
            if doc_id.is_empty() {
                continue;
            }
            self.process_doc(doc_id, &mut processed, &mut contract_cache);
        }
    }

    /// Processes a NatSpec doc, validating all tags and resolving `@inheritdoc`.
    fn process_doc(
        &mut self,
        doc_id: hir::DocId,
        processed: &mut FxHashSet<hir::DocId>,
        contract_cache: &mut FxHashMap<(Symbol, hir::SourceId), Option<hir::ContractId>>,
    ) {
        if processed.contains(&doc_id) {
            return;
        }
        processed.insert(doc_id);

        let doc = self.hir.doc(doc_id);
        let (item_id, source_id) = (doc.item, doc.source);
        let (local_tags, inheritdoc) =
            self.validate_item_natspec(doc.ast_comments, item_id, source_id, contract_cache);

        // Resolve inheritdoc if present
        let resolved_tags = if let Some((contract_id, item_id)) = inheritdoc {
            let inherit_doc_id = self.find_inherited_item(item_id, contract_id).and_then(
                |inherited| match inherited {
                    hir::ItemId::Function(id) => Some(self.hir.function(id).doc),
                    hir::ItemId::Variable(id) => self.hir.variable(id).doc,
                    _ => None,
                },
            );

            if let Some(inherit_doc_id) = inherit_doc_id
                && !inherit_doc_id.is_empty()
            {
                // Recursively process inherited doc, then merge local and inherited tags
                self.process_doc(inherit_doc_id, processed, contract_cache);
                self.merge_natspec_tags(&local_tags, self.hir.doc(inherit_doc_id).comments())
            } else {
                self.arena.alloc_slice_copy(&local_tags)
            }
        } else {
            self.arena.alloc_slice_copy(&local_tags)
        };

        // Store resolved tags
        self.hir.docs[doc_id].comments = resolved_tags;
    }

    /// Validates NatSpec tags for the given item.
    ///
    /// Checks:
    /// - Tag applicability
    /// - Duplicate tags
    /// - Parameter references
    fn validate_item_natspec(
        &mut self,
        docs: &'gcx ast::DocComments<'gcx>,
        item_id: hir::ItemId,
        source_id: hir::SourceId,
        contract_cache: &mut FxHashMap<(Symbol, hir::SourceId), Option<hir::ContractId>>,
    ) -> (SmallVec<[hir::NatSpecItem; 8]>, Option<(hir::ContractId, hir::ItemId)>) {
        use ast::NatSpecKind;
        use hir::NatSpecItem;

        // Get required info for validation
        let permissions = TagPermissions::from_item_id(item_id);
        // Parameters, returns, and description are lazily initialized
        let mut parameters: Option<FxHashSet<Symbol>> = None;
        let mut returns: Option<&[hir::VariableId]> = None;

        #[derive(Default)]
        struct ValidationState {
            seen_tags: SeenTags,
            seen_params: SmallVec<[(Symbol, Span); 6]>,
            return_count: usize,
        }

        let mut inheritdoc = None;
        let mut state = ValidationState::default();
        let mut local_tags = SmallVec::<[NatSpecItem; 8]>::new();

        for doc_comment in docs.iter() {
            for natspec in doc_comment.natspec.iter() {
                let tag_span = natspec.span;

                match &natspec.kind {
                    NatSpecKind::Notice
                    | NatSpecKind::Dev
                    | NatSpecKind::Custom { .. }
                    | NatSpecKind::Internal { .. } => {
                        local_tags.push(NatSpecItem::from_ast(*natspec, doc_comment.symbol));
                    }
                    NatSpecKind::Title => {
                        if self.validate_tag_once(
                            "@title",
                            tag_span,
                            permissions.contains(TagPermissions::TITLE_AUTHOR),
                            &mut state.seen_tags,
                            SeenTags::TITLE,
                            item_id,
                        ) {
                            local_tags.push(NatSpecItem::from_ast(*natspec, doc_comment.symbol));
                        }
                    }
                    NatSpecKind::Author => {
                        if !permissions.contains(TagPermissions::TITLE_AUTHOR) {
                            self.emit_forbidden_tag_error("@author", tag_span, item_id);
                        } else {
                            local_tags.push(NatSpecItem::from_ast(*natspec, doc_comment.symbol));
                        }
                    }
                    NatSpecKind::Inheritdoc { contract } => {
                        if !self.validate_tag_once(
                            "@inheritdoc",
                            tag_span,
                            permissions.contains(TagPermissions::INHERITDOC),
                            &mut state.seen_tags,
                            SeenTags::INHERITDOC,
                            item_id,
                        ) {
                            continue;
                        }

                        // Validate and cache contract resolution
                        if let Some(contract_id) = self.validate_and_cache_inheritdoc_contract(
                            contract,
                            tag_span,
                            item_id,
                            source_id,
                            contract_cache,
                        ) {
                            // Cache info for later resolution
                            inheritdoc = Some((contract_id, item_id));
                        }
                    }
                    NatSpecKind::Param { name } => {
                        if !permissions.contains(TagPermissions::PARAM) {
                            self.emit_forbidden_tag_error("@param", tag_span, item_id);
                            continue;
                        }

                        // Check for duplicate `@param` tags
                        if let Some(&(_, prev_span)) =
                            state.seen_params.iter().find(|(sym, _)| *sym == name.name)
                        {
                            self.emit_duplicate_param_error(name.name, tag_span, prev_span);
                            continue;
                        }
                        state.seen_params.push((name.name, tag_span));

                        // Lazy initialization of parameters
                        let params = parameters.get_or_insert_with(|| {
                            self.hir.item(item_id).parameters().map_or(FxHashSet::default(), |p| {
                                p.iter()
                                    .filter_map(|&id| {
                                        self.hir.variable(id).name.map(|ident| ident.name)
                                    })
                                    .collect()
                            })
                        });

                        // Convert to HIR if validation passed
                        if params.contains(&name.name) {
                            local_tags.push(NatSpecItem::from_ast(*natspec, doc_comment.symbol));
                        } else {
                            self.dcx()
                                .err(format!(
                                    "tag `@param` references non-existent parameter '{}'",
                                    name.name
                                ))
                                .span(tag_span)
                                .emit();
                        };
                    }
                    NatSpecKind::Return { .. } => {
                        if !permissions.contains(TagPermissions::RETURN) {
                            self.emit_forbidden_tag_error("@return", tag_span, item_id);
                            continue;
                        }

                        state.return_count += 1;

                        // Lazy initialization of returns
                        let rets = returns.get_or_insert_with(|| {
                            if let hir::ItemId::Function(id) = item_id {
                                self.hir.function(id).returns
                            } else {
                                &[]
                            }
                        });

                        // Validate return count
                        let return_valid = if state.return_count > rets.len() {
                            self.dcx().err(format!(
                                "too many `@return` tags: function has {} return value{}, found {}",
                                rets.len(),
                                if rets.len() == 1 { "" } else { "s" },
                                state.return_count
                            ))
                            .span(tag_span)
                            .emit();
                            false
                        } else {
                            true
                        };

                        // Convert to HIR if validation passed
                        if return_valid {
                            local_tags.push(NatSpecItem::from_ast(*natspec, doc_comment.symbol));
                        }
                    }
                }
            }
        }

        (local_tags, inheritdoc)
    }

    #[cold]
    fn emit_forbidden_tag_error(&self, tag_name: &str, tag_span: Span, item_id: hir::ItemId) {
        let item_desc = self.hir.item(item_id).description();
        self.dcx()
            .err(format!("tag `{tag_name}` not valid for {item_desc}s"))
            .span(tag_span)
            .emit();
    }

    #[cold]
    fn emit_duplicate_tag_error(&self, tag_name: &str, tag_span: Span) {
        self.dcx().err(format!("tag {tag_name} can only be given once")).span(tag_span).emit();
    }

    #[cold]
    fn emit_duplicate_param_error(&self, param_name: Symbol, tag_span: Span, prev_span: Span) {
        self.dcx()
            .err(format!("duplicate documentation for parameter '{param_name}'"))
            .span(tag_span)
            .span_note(prev_span, "previously documented here")
            .emit();
    }

    /// Helper to validate tags that can only be defined once.
    /// Returns `true` if validation passed, `false` otherwise.
    #[inline]
    fn validate_tag_once(
        &self,
        tag_name: &str,
        tag_span: Span,
        allowed: bool,
        seen_tags: &mut SeenTags,
        tag_flag: SeenTags,
        item_id: hir::ItemId,
    ) -> bool {
        if !allowed {
            self.emit_forbidden_tag_error(tag_name, tag_span, item_id);
            return false;
        }
        if seen_tags.contains(tag_flag) {
            self.emit_duplicate_tag_error(tag_name, tag_span);
            return false;
        }
        seen_tags.insert(tag_flag);
        true
    }

    /// Validates and caches contract resolution for `@inheritdoc`.
    /// Returns the resolved contract ID if validation passed.
    #[inline]
    fn validate_and_cache_inheritdoc_contract(
        &mut self,
        contract_ident: &solar_interface::Ident,
        tag_span: Span,
        item_id: hir::ItemId,
        source_id: hir::SourceId,
        contract_cache: &mut FxHashMap<(Symbol, hir::SourceId), Option<hir::ContractId>>,
    ) -> Option<hir::ContractId> {
        let dcx = self.dcx();

        // Try to get from cache first
        let cache_key = (contract_ident.name, source_id);
        let contract_id = *contract_cache
            .entry(cache_key)
            .or_insert_with(|| self.resolve_contract_in_source(contract_ident.name, source_id));

        let Some(contract_id) = contract_id else {
            dcx.err(format!(
                "tag `@inheritdoc` references inexistent contract \"{}\"",
                contract_ident.name
            ))
            .span(tag_span)
            .emit();
            return None;
        };

        // Verify that the item's contract inherits from the referenced contract
        // and that the referenced contract contains a matching item.
        // This works for both explicit `override(Base)` and implicit `override`.
        let item_contract = match item_id {
            hir::ItemId::Function(id) => self.hir.function(id).contract,
            hir::ItemId::Variable(id) => self.hir.variable(id).contract,
            _ => return None,
        };

        if let Some(contract) = item_contract {
            let linearized_bases = &self.hir.contract(contract).linearized_bases;
            if !linearized_bases.contains(&contract_id) {
                dcx.err(format!(
                    "tag `@inheritdoc` references contract \"{}\", which is not a base of this contract",
                    contract_ident.name
                ))
                .span(tag_span)
                .emit();
                return None;
            }
        }

        // Verify that the referenced contract contains a matching item
        if self.find_inherited_item(item_id, contract_id).is_none() {
            dcx.err(format!(
                "tag `@inheritdoc` references contract \"{}\", but the contract does not contain a matching item that can be inherited",
                contract_ident.name
            ))
            .span(tag_span)
            .emit();
            return None;
        }

        Some(contract_id)
    }

    /// Resolves a contract name within a source's scope.
    fn resolve_contract_in_source(
        &self,
        name: Symbol,
        source_id: hir::SourceId,
    ) -> Option<hir::ContractId> {
        self.resolver.source_scopes[source_id]
            .resolve(solar_interface::Ident { name, span: Span::DUMMY })
            .and_then(|decls| {
                decls.iter().find_map(|decl| match decl.res {
                    hir::Res::Item(hir::ItemId::Contract(id)) => Some(id),
                    _ => None,
                })
            })
    }

    /// Merges local and inherited natspec tags.
    ///
    /// Rules:
    /// - `@notice`, `@dev`, `@title`, `@author`: local overrides inherited
    /// - `@param`, `@return`: inherit missing ones, keep local ones
    /// - `@custom`: merge both
    fn merge_natspec_tags(
        &self,
        items: &[hir::NatSpecItem],
        inherited_tags: &'gcx [hir::NatSpecItem],
    ) -> &'gcx [hir::NatSpecItem] {
        use hir::NatSpecKind as HirKind;

        let mut local_tags = LocalTags::empty();
        let mut local_params = FxHashSet::<Symbol>::default();
        let mut local_returns = FxHashSet::<Option<Symbol>>::default();
        let mut merged = SmallVec::<[hir::NatSpecItem; 8]>::from_slice(items);

        // Build local tag index
        for item in items.iter() {
            match &item.kind {
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
                HirKind::Custom { .. } | HirKind::Internal { .. } => {}
            }
        }

        // Merge inherited tags
        for item in inherited_tags.iter() {
            let should_inherit = match &item.kind {
                HirKind::Notice => !local_tags.contains(LocalTags::NOTICE),
                HirKind::Dev => !local_tags.contains(LocalTags::DEV),
                HirKind::Title => !local_tags.contains(LocalTags::TITLE),
                HirKind::Author => !local_tags.contains(LocalTags::AUTHOR),
                HirKind::Param { name } => !local_params.contains(&name.name),
                HirKind::Return { name } => !local_returns.contains(&name.map(|n| n.name)),
                HirKind::Custom { .. } | HirKind::Internal { .. } => true,
            };

            if should_inherit {
                merged.push(*item);
            }
        }

        self.arena.alloc_slice_copy(&merged)
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

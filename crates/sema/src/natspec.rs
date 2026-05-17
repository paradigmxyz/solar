use crate::{
    hir,
    ty::{Gcx, GcxMut},
};
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
            hir::ItemId::Variable(_) => Self::INHERITDOC | Self::RETURN,
            hir::ItemId::Event(_) => Self::PARAM,
            hir::ItemId::Error(_) => Self::PARAM,
            hir::ItemId::Udvt(_) => Self::PARAM,
        }
    }
}

pub(crate) fn validate_and_resolve_docs<'gcx>(gcx: &mut GcxMut<'gcx>) {
    let resolved = Resolver::new(gcx.get()).validate_and_resolve_docs();
    let hir = &mut gcx.get_mut().hir;
    for (doc_id, comments) in resolved {
        hir.docs[doc_id].comments = comments;
    }
}

struct Resolver<'gcx> {
    gcx: Gcx<'gcx>,
    processed: FxHashSet<hir::DocId>,
    resolved: FxHashMap<hir::DocId, &'gcx [hir::NatSpecItem]>,
    contract_cache: FxHashMap<(Symbol, hir::SourceId), Option<hir::ContractId>>,
}

impl<'gcx> Resolver<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self {
            gcx,
            processed: FxHashSet::default(),
            resolved: FxHashMap::default(),
            contract_cache: FxHashMap::default(),
        }
    }

    /// Processes NatSpec tags, validating all of them, and resolving `@inheritdoc` references.
    fn validate_and_resolve_docs(mut self) -> FxHashMap<hir::DocId, &'gcx [hir::NatSpecItem]> {
        for doc_id in self.gcx.hir.doc_ids() {
            if doc_id.is_empty() {
                continue;
            }
            self.process_doc(doc_id);
        }
        self.resolved
    }

    /// Processes a NatSpec doc, validating all tags and resolving `@inheritdoc`.
    fn process_doc(&mut self, doc_id: hir::DocId) {
        if !self.processed.insert(doc_id) {
            return;
        }

        let doc = self.gcx.hir.doc(doc_id);
        let (item_id, source_id) = (doc.item, doc.source);
        let (local_tags, inheritdoc) =
            self.validate_item_natspec(&doc.ast_comments, item_id, source_id);

        let resolved_tags = if let Some((contract_id, item_id)) = inheritdoc {
            let inherit_doc_id = self.find_inherited_item(item_id, contract_id).and_then(
                |inherited| match inherited {
                    hir::ItemId::Function(id) => Some(self.gcx.hir.function(id).doc),
                    hir::ItemId::Variable(id) => self.gcx.hir.variable(id).doc,
                    _ => None,
                },
            );

            if let Some(inherit_doc_id) = inherit_doc_id
                && !inherit_doc_id.is_empty()
            {
                self.process_doc(inherit_doc_id);
                self.merge_natspec_tags(
                    &local_tags,
                    self.resolved.get(&inherit_doc_id).copied().unwrap_or(&[]),
                )
            } else {
                self.gcx.arena().alloc_slice_copy(&local_tags)
            }
        } else {
            self.gcx.arena().alloc_slice_copy(&local_tags)
        };

        self.resolved.insert(doc_id, resolved_tags);
    }

    /// Validates NatSpec tags for the given item.
    ///
    /// Checks:
    /// - Tag applicability
    /// - Duplicate tags
    /// - Parameter references
    fn validate_item_natspec(
        &mut self,
        docs: &[ast::DocComment<'gcx>],
        item_id: hir::ItemId,
        source_id: hir::SourceId,
    ) -> (SmallVec<[hir::NatSpecItem; 8]>, Option<(hir::ContractId, hir::ItemId)>) {
        use ast::NatSpecKind;
        use hir::NatSpecItem;

        let permissions = TagPermissions::from_item_id(item_id);
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
                        local_tags.push(*natspec);
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
                            local_tags.push(*natspec);
                        }
                    }
                    NatSpecKind::Author => {
                        if !permissions.contains(TagPermissions::TITLE_AUTHOR) {
                            self.emit_forbidden_tag_error("@author", tag_span, item_id);
                        } else {
                            local_tags.push(*natspec);
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

                        if let Some(contract_id) = self.validate_and_cache_inheritdoc_contract(
                            contract, tag_span, item_id, source_id,
                        ) {
                            inheritdoc = Some((contract_id, item_id));
                        }
                    }
                    NatSpecKind::Param { name } => {
                        if !permissions.contains(TagPermissions::PARAM) {
                            self.emit_forbidden_tag_error("@param", tag_span, item_id);
                            continue;
                        }

                        if let Some(&(_, prev_span)) =
                            state.seen_params.iter().find(|(sym, _)| *sym == name.name)
                        {
                            self.emit_duplicate_param_error(name.name, tag_span, prev_span);
                            continue;
                        }
                        state.seen_params.push((name.name, tag_span));

                        let params = parameters.get_or_insert_with(|| {
                            self.gcx.hir.item(item_id).parameters().map_or(
                                FxHashSet::default(),
                                |p| {
                                    p.iter()
                                        .filter_map(|&id| {
                                            self.gcx.hir.variable(id).name.map(|ident| ident.name)
                                        })
                                        .collect()
                                },
                            )
                        });

                        if params.contains(&name.name) {
                            local_tags.push(*natspec);
                        } else {
                            self.gcx
                                .dcx()
                                .err(format!(
                                    "tag `@param` references non-existent parameter '{}'",
                                    name.name
                                ))
                                .span(tag_span)
                                .emit();
                        };
                    }
                    NatSpecKind::Return { .. } => {
                        if !permissions.contains(TagPermissions::RETURN)
                            || item_id
                                .as_variable()
                                .is_some_and(|id| !self.gcx.hir.variable(id).is_public())
                        {
                            self.emit_forbidden_tag_error("@return", tag_span, item_id);
                            continue;
                        }

                        state.return_count += 1;

                        let rets = returns.get_or_insert_with(|| {
                            if let hir::ItemId::Function(id) = item_id {
                                self.gcx.hir.function(id).returns
                            } else {
                                &[]
                            }
                        });
                        let return_count = match item_id {
                            hir::ItemId::Variable(_) => 1,
                            _ => rets.len(),
                        };

                        let return_valid = if state.return_count > return_count {
                            self.gcx.dcx().err(format!(
                                "too many `@return` tags: function has {} return value{}, found {}",
                                return_count,
                                if return_count == 1 { "" } else { "s" },
                                state.return_count
                            ))
                            .span(tag_span)
                            .emit();
                            false
                        } else {
                            true
                        };

                        if return_valid
                            && let Some(item) = self.lower_return_natspec(*natspec, rets)
                        {
                            local_tags.push(item);
                        }
                    }
                }
            }
        }

        (local_tags, inheritdoc)
    }

    fn lower_return_natspec(
        &self,
        natspec: ast::NatSpecItem,
        rets: &[hir::VariableId],
    ) -> Option<hir::NatSpecItem> {
        if !rets.iter().any(|&id| self.gcx.hir.variable(id).name.is_some()) {
            return Some(natspec);
        }

        let Some((name, content_start)) = solar_parse::natspec::first_word(
            natspec.symbol.as_str(),
            natspec.content_start as usize,
            natspec.content_end as usize,
        ) else {
            self.gcx
                .dcx()
                .err("tag `@return` does not contain the name of its return parameter")
                .span(natspec.span)
                .emit();
            return None;
        };

        let name = Symbol::intern(name);
        if !rets.iter().any(|&id| self.gcx.hir.variable(id).name.is_some_and(|n| n.name == name)) {
            self.gcx
                .dcx()
                .err(format!("tag `@return` references non-existent return parameter '{name}'"))
                .span(natspec.span)
                .emit();
            return None;
        }

        let mut item = natspec;
        item.kind = ast::NatSpecKind::Return { name: Some(name) };
        item.content_start = content_start as u32;
        Some(item)
    }

    #[cold]
    fn emit_forbidden_tag_error(&self, tag_name: &str, tag_span: Span, item_id: hir::ItemId) {
        let item_desc = self.gcx.hir.item(item_id).description();
        self.gcx
            .dcx()
            .err(format!("tag `{tag_name}` not valid for {item_desc}s"))
            .span(tag_span)
            .emit();
    }

    #[cold]
    fn emit_duplicate_tag_error(&self, tag_name: &str, tag_span: Span) {
        self.gcx.dcx().err(format!("tag {tag_name} can only be given once")).span(tag_span).emit();
    }

    #[cold]
    fn emit_duplicate_param_error(&self, param_name: Symbol, tag_span: Span, prev_span: Span) {
        self.gcx
            .dcx()
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
    ) -> Option<hir::ContractId> {
        let dcx = self.gcx.dcx();

        let cache_key = (contract_ident.name, source_id);
        let contract_id = if let Some(contract_id) = self.contract_cache.get(&cache_key) {
            *contract_id
        } else {
            let contract_id = self.resolve_contract_in_source(contract_ident.name, source_id);
            self.contract_cache.insert(cache_key, contract_id);
            contract_id
        };

        let Some(contract_id) = contract_id else {
            dcx.err(format!(
                "tag `@inheritdoc` references inexistent contract \"{}\"",
                contract_ident.name
            ))
            .span(tag_span)
            .emit();
            return None;
        };

        let item_contract = match item_id {
            hir::ItemId::Function(id) => self.gcx.hir.function(id).contract,
            hir::ItemId::Variable(id) => self.gcx.hir.variable(id).contract,
            _ => return None,
        };

        if let Some(contract) = item_contract {
            let linearized_bases = &self.gcx.hir.contract(contract).linearized_bases;
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
        self.gcx.symbol_resolver.source_scopes[source_id]
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
                    local_returns.insert(*name);
                }
                HirKind::Custom { .. } | HirKind::Internal { .. } | HirKind::Inheritdoc { .. } => {}
            }
        }

        for item in inherited_tags.iter() {
            let should_inherit = match &item.kind {
                HirKind::Notice => !local_tags.contains(LocalTags::NOTICE),
                HirKind::Dev => !local_tags.contains(LocalTags::DEV),
                HirKind::Title => !local_tags.contains(LocalTags::TITLE),
                HirKind::Author => !local_tags.contains(LocalTags::AUTHOR),
                HirKind::Param { name } => !local_params.contains(&name.name),
                HirKind::Return { name } => !local_returns.contains(name),
                HirKind::Custom { .. } | HirKind::Internal { .. } => true,
                HirKind::Inheritdoc { .. } => false,
            };

            if should_inherit {
                merged.push(*item);
            }
        }

        self.gcx.arena().alloc_slice_copy(&merged)
    }

    /// Finds the item in a contract that matches the given item (for inheritance).
    fn find_inherited_item(
        &self,
        item_id: hir::ItemId,
        contract_id: hir::ContractId,
    ) -> Option<hir::ItemId> {
        let item_name = self.gcx.hir.item(item_id).name()?;

        for base_item_id in self.gcx.hir.contract_item_ids(contract_id) {
            if let Some(base_name) = self.gcx.hir.item(base_item_id).name()
                && base_name.name == item_name.name
                && self.items_have_matching_signature(item_id, base_item_id)
            {
                return Some(base_item_id);
            }
        }

        None
    }

    fn items_have_matching_signature(
        &self,
        item_id: hir::ItemId,
        base_item_id: hir::ItemId,
    ) -> bool {
        match (item_id, base_item_id) {
            (hir::ItemId::Function(id), hir::ItemId::Function(base_id)) => {
                let item = self.gcx.hir.function(id);
                let base = self.gcx.hir.function(base_id);
                item.kind == base.kind
                    && self.gcx.item_parameter_types(item_id)
                        == self.gcx.item_parameter_types(base_item_id)
            }
            (hir::ItemId::Variable(_), hir::ItemId::Variable(_)) => true,
            _ => false,
        }
    }
}

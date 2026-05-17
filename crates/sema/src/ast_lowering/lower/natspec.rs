use crate::hir;
use solar_ast as ast;
use solar_data_structures::{
    BumpExt,
    map::{FxHashMap, FxHashSet},
    smallvec::SmallVec,
};
use solar_interface::{Ident, Span, Symbol};

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

impl<'gcx> super::super::LoweringContext<'gcx> {
    /// Lowers documentation comments from AST to HIR.
    ///
    /// Validation happens after parameters are lowered.
    pub(super) fn lower_item_docs(
        &mut self,
        item: &'gcx ast::Item<'gcx>,
        item_id: hir::ItemId,
    ) -> hir::DocId {
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
            comments: &[],
        })
    }

    /// Processes NatSpec tags, validating all of them, and resolving `@inheritdoc` references.
    ///
    /// Must be called after symbol resolution, since parameter validation requires the actual
    /// parameter lists to be populated.
    pub(in crate::ast_lowering) fn validate_and_resolve_docs(&mut self) {
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
        if !processed.insert(doc_id) {
            return;
        }

        let doc = self.hir.doc(doc_id);
        let (item_id, source_id) = (doc.item, doc.source);
        let (local_tags, inheritdoc) =
            self.validate_item_natspec(&doc.ast_comments, item_id, source_id, contract_cache);

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
                self.process_doc(inherit_doc_id, processed, contract_cache);
                self.merge_natspec_tags(&local_tags, self.hir.doc(inherit_doc_id).comments())
            } else {
                self.arena.alloc_slice_copy(&local_tags)
            }
        } else {
            self.arena.alloc_slice_copy(&local_tags)
        };

        self.hir.docs[doc_id].comments = resolved_tags;
    }

    /// Validates NatSpec tags for the given item.
    ///
    /// Checks:
    /// - Tag applicability
    /// - Duplicate tags
    /// - Parameter references
    fn validate_item_natspec(
        &self,
        docs: &[ast::DocComment<'gcx>],
        item_id: hir::ItemId,
        source_id: hir::SourceId,
        contract_cache: &mut FxHashMap<(Symbol, hir::SourceId), Option<hir::ContractId>>,
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

                        if let Some(contract_id) = self.validate_and_cache_inheritdoc_contract(
                            contract,
                            tag_span,
                            item_id,
                            source_id,
                            contract_cache,
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
                            self.hir.item(item_id).parameters().map_or(FxHashSet::default(), |p| {
                                p.iter()
                                    .filter_map(|&id| {
                                        self.hir.variable(id).name.map(|ident| ident.name)
                                    })
                                    .collect()
                            })
                        });

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
                        if !permissions.contains(TagPermissions::RETURN)
                            || item_id
                                .as_variable()
                                .is_some_and(|id| !self.hir.variable(id).is_public())
                        {
                            self.emit_forbidden_tag_error("@return", tag_span, item_id);
                            continue;
                        }

                        state.return_count += 1;

                        let rets = returns.get_or_insert_with(|| {
                            if let hir::ItemId::Function(id) = item_id {
                                self.hir.function(id).returns
                            } else {
                                &[]
                            }
                        });
                        let return_count = match item_id {
                            hir::ItemId::Variable(_) => 1,
                            _ => rets.len(),
                        };

                        let return_valid = if state.return_count > return_count {
                            self.dcx().err(format!(
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
                            && let Some(item) =
                                self.lower_return_natspec(*natspec, doc_comment.symbol, rets)
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
        symbol: Symbol,
        rets: &[hir::VariableId],
    ) -> Option<hir::NatSpecItem> {
        if !rets.iter().any(|&id| self.hir.variable(id).name.is_some()) {
            return Some(hir::NatSpecItem::from_ast(natspec, symbol));
        }

        let Some((name, content_start)) = first_word(symbol, natspec) else {
            self.dcx()
                .err("tag `@return` does not contain the name of its return parameter")
                .span(natspec.span)
                .emit();
            return None;
        };

        if !rets.iter().any(|&id| self.hir.variable(id).name.is_some_and(|n| n.name == name)) {
            self.dcx()
                .err(format!("tag `@return` references non-existent return parameter '{name}'"))
                .span(natspec.span)
                .emit();
            return None;
        }

        let mut item = hir::NatSpecItem::from_ast(natspec, symbol);
        item.kind = hir::NatSpecKind::Return { name: Some(Ident::new(name, natspec.span)) };
        item.content_start = content_start;
        Some(item)
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
        &self,
        contract_ident: &solar_interface::Ident,
        tag_span: Span,
        item_id: hir::ItemId,
        source_id: hir::SourceId,
        contract_cache: &mut FxHashMap<(Symbol, hir::SourceId), Option<hir::ContractId>>,
    ) -> Option<hir::ContractId> {
        let dcx = self.dcx();

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

        for base_item_id in self.hir.contract_item_ids(contract_id) {
            if let Some(base_name) = self.hir.item(base_item_id).name()
                && base_name.name == item_name.name
                && self.inherited_item_matches(item_id, base_item_id)
            {
                return Some(base_item_id);
            }
        }

        None
    }

    fn inherited_item_matches(&self, item_id: hir::ItemId, base_item_id: hir::ItemId) -> bool {
        match (item_id, base_item_id) {
            (hir::ItemId::Function(id), hir::ItemId::Function(base_id)) => {
                let item = self.hir.function(id);
                let base = self.hir.function(base_id);
                item.kind == base.kind
                    && self.variable_types_match(item.parameters, base.parameters)
            }
            (hir::ItemId::Variable(_), hir::ItemId::Variable(_)) => true,
            _ => false,
        }
    }

    fn variable_types_match(&self, a: &[hir::VariableId], b: &[hir::VariableId]) -> bool {
        a.len() == b.len()
            && a.iter().zip(b).all(|(&a, &b)| {
                self.types_match(&self.hir.variable(a).ty, &self.hir.variable(b).ty)
            })
    }

    fn types_match(&self, a: &hir::Type<'_>, b: &hir::Type<'_>) -> bool {
        match (&a.kind, &b.kind) {
            (hir::TypeKind::Elementary(a), hir::TypeKind::Elementary(b)) => a == b,
            (hir::TypeKind::Custom(a), hir::TypeKind::Custom(b)) => a == b,
            (hir::TypeKind::Array(a), hir::TypeKind::Array(b)) => {
                self.types_match(&a.element, &b.element) && self.array_sizes_match(a.size, b.size)
            }
            (hir::TypeKind::Function(a), hir::TypeKind::Function(b)) => {
                a.visibility == b.visibility
                    && a.state_mutability == b.state_mutability
                    && self.variable_types_match(a.parameters, b.parameters)
                    && self.variable_types_match(a.returns, b.returns)
            }
            (hir::TypeKind::Mapping(a), hir::TypeKind::Mapping(b)) => {
                self.types_match(&a.key, &b.key) && self.types_match(&a.value, &b.value)
            }
            _ => false,
        }
    }

    fn array_sizes_match(&self, a: Option<&hir::Expr<'_>>, b: Option<&hir::Expr<'_>>) -> bool {
        match (a, b) {
            (None, None) => true,
            (Some(a), Some(b)) => self
                .sess
                .source_map()
                .span_to_snippet(a.span)
                .is_ok_and(|a| self.sess.source_map().span_to_snippet(b.span) == Ok(a)),
            _ => false,
        }
    }
}

fn first_word(symbol: Symbol, natspec: ast::NatSpecItem) -> Option<(Symbol, u32)> {
    let content = symbol.as_str();
    let range = natspec.content_range();
    let bytes = &content.as_bytes()[range.clone()];
    let start = range.start + (bytes.len() - bytes.trim_ascii_start().len());
    let bytes = &content.as_bytes()[start..range.end];
    let len = bytes.iter().position(u8::is_ascii_whitespace).unwrap_or(bytes.len());
    if len == 0 {
        return None;
    }

    let end = start + len;
    let rest = &content.as_bytes()[end..range.end];
    let content_start = end + (rest.len() - rest.trim_ascii_start().len());
    Some((Symbol::intern(&content[start..end]), content_start as u32))
}

#[cfg(test)]
mod tests {
    use crate::Compiler;
    use solar_interface::{ColorChoice, Session, sym};
    use std::path::PathBuf;

    #[test]
    fn natspec_inheritdoc_merges_tags() {
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
            let base = get_comments("Base", "foo");
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Notice)), 1);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Dev)), 1);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Param { .. })), 2);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Return { .. })), 2);
            assert_eq!(count_tags(base, |k| matches!(k, NatSpecKind::Custom { .. })), 1);
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Notice),
                "Base function notice",
                "Base @notice",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Dev),
                "Base function dev",
                "Base @dev",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x),
                "x parameter from base",
                "Base @param x",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "y"),
                "y parameter from base",
                "Base @param y",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"),
                "operation succeeded",
                "Base @return success",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value"),
                "result value",
                "Base @return value",
            );
            assert_tag_contains(
                base,
                |k| matches!(k, NatSpecKind::Custom { name } if name.name.as_str() == "security"),
                "Audited by Base team",
                "Base @custom:security",
            );

            let c = get_comments("Child1", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Notice)), 1);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Dev)), 1);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 2);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Return { .. })), 2);
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 1);

            let c = get_comments("Child2", "foo");
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Notice),
                "Child2 notice",
                "Child2 @notice",
            );
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Dev),
                "Child2 dev",
                "Child2 @dev",
            );
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 2);

            let c = get_comments("Child3", "foo");
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x),
                "from child3",
                "Child3 @param x",
            );
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "y"),
                "from base",
                "Child3 @param y",
            );

            let c = get_comments("Child4", "foo");
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"),
                "Child4 override",
                "Child4 @return success",
            );
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value"),
                "result value",
                "Child4 @return value",
            );

            let c = get_comments("Child5", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 2);
            assert!(
                c.iter().any(
                    |i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "security")
                )
            );
            assert!(
                c.iter().any(
                    |i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "audit")
                )
            );

            let g = get_comments("GrandChild", "foo");
            assert_tag_contains(
                g,
                |k| matches!(k, NatSpecKind::Notice),
                "Base function notice",
                "GrandChild @notice",
            );
            assert_eq!(count_tags(g, |k| matches!(k, NatSpecKind::Param { .. })), 2);

        });
    }

    #[test]
    fn natspec_inheritdoc_matches_overload_signature() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract OverloadBase {
    /// @notice address overload
    /// @param a address parameter
    function overloaded(address a) public virtual {}

    /// @notice uint overload
    /// @param a uint parameter
    function overloaded(uint a) public virtual {}
}

contract OverloadChild is OverloadBase {
    /// @inheritdoc OverloadBase
    function overloaded(uint a) public override {}
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let overloaded = function_comments(c.gcx(), "OverloadChild", "overloaded");
            assert_tag_contains(
                overloaded,
                |k| matches!(k, NatSpecKind::Notice),
                "uint overload",
                "OverloadChild @notice",
            );
            assert_tag_contains(
                overloaded,
                |k| matches!(k, NatSpecKind::Param { name } if name.name.as_str() == "a"),
                "uint parameter",
                "OverloadChild @param a",
            );
            assert!(
                !overloaded.iter().any(|item| item.content().contains("address")),
                "OverloadChild inherited docs from the wrong overload"
            );
        });
    }

    #[test]
    fn natspec_public_variable_return_docs() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract VariableDocs {
    /// @return The number of decimals
    uint8 public decimals;
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let decimals = variable_comments(c.gcx(), "VariableDocs", "decimals");
            assert_tag_contains(
                decimals,
                |k| matches!(k, NatSpecKind::Return { name: None }),
                "The number of decimals",
                "VariableDocs.decimals @return",
            );
        });
    }

    #[test]
    fn natspec_return_docs_resolve_names() {
        use crate::hir::NatSpecKind;

        let src = r#"
contract ReturnDocs {
    /// @return the value
    function unnamedReturn() public pure returns (uint) {
        return 1;
    }

    /// @return result The value
    function namedReturn() public pure returns (uint result) {
        return 1;
    }
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let unnamed = function_comments(c.gcx(), "ReturnDocs", "unnamedReturn");
            assert_tag_contains(
                unnamed,
                |k| matches!(k, NatSpecKind::Return { name: None }),
                "the value",
                "ReturnDocs.unnamedReturn @return",
            );

            let named = function_comments(c.gcx(), "ReturnDocs", "namedReturn");
            assert_tag_contains(
                named,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "result"),
                "The value",
                "ReturnDocs.namedReturn @return result",
            );
        });
    }

    #[test]
    fn statement_variables_preserve_function_parent() {
        let src = r#"
contract LocalParent {
    function locals() public pure {
        uint a = 1;
        (uint b, uint c) = (2, 3);
    }
}
"#;
        let compiler = lower_source(src);
        compiler.enter_sequential(|c| {
            let gcx = c.gcx();
            let locals = function_id(gcx, "LocalParent", "locals");
            let local_parent = Some(crate::hir::ItemId::Function(locals));
            let local_names: Vec<_> = gcx
                .hir
                .variables()
                .filter(|v| v.kind == crate::hir::VarKind::Statement)
                .filter_map(|v| v.name.map(|name| (name.name, v.parent)))
                .collect();
            for name in ["a", "b", "c"] {
                assert!(
                    local_names.iter().any(|&(local, parent)| {
                        local.as_str() == name && parent == local_parent
                    }),
                    "local variable {name} did not preserve its function parent"
                );
            }
        });
    }

    fn function_id(
        gcx: crate::ty::Gcx<'_>,
        contract_name: &str,
        func_name: &str,
    ) -> crate::hir::FunctionId {
        gcx.hir
            .functions_enumerated()
            .find(|(_, f)| {
                f.contract.is_some_and(|cid| {
                    gcx.hir.contract(cid).name.as_str() == contract_name
                        && f.name.is_some_and(|n| n.as_str() == func_name)
                })
            })
            .map(|(id, _)| id)
            .unwrap_or_else(|| panic!("{contract_name}.{func_name} not found"))
    }

    fn function_comments<'gcx>(
        gcx: crate::ty::Gcx<'gcx>,
        contract_name: &str,
        func_name: &str,
    ) -> &'gcx [crate::hir::NatSpecItem] {
        let id = function_id(gcx, contract_name, func_name);
        gcx.hir.doc(gcx.hir.function(id).doc).comments()
    }

    fn variable_comments<'gcx>(
        gcx: crate::ty::Gcx<'gcx>,
        contract_name: &str,
        var_name: &str,
    ) -> &'gcx [crate::hir::NatSpecItem] {
        gcx.hir
            .variables()
            .find(|v| {
                v.contract.is_some_and(|cid| {
                    gcx.hir.contract(cid).name.as_str() == contract_name
                        && v.name.is_some_and(|n| n.as_str() == var_name)
                })
            })
            .and_then(|var| var.doc.map(|doc| gcx.hir.doc(doc).comments()))
            .unwrap_or_else(|| panic!("{contract_name}.{var_name} not found"))
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

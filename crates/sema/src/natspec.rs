use crate::{hir, ty::Gcx};
use solar_ast as ast;
use solar_data_structures::{map::FxHashSet, smallvec::SmallVec};
use solar_interface::{Ident, Span, Symbol};

bitflags::bitflags! {
    /// Tracks which documentation tags are locally defined in `merge_natspec_tags`.
    struct LocalTags: u8 {
        const NOTICE = 1 << 0;
        const DEV    = 1 << 1;
        const TITLE  = 1 << 2;
        const AUTHOR = 1 << 3;
        const PARAM  = 1 << 4;
        const RETURN = 1 << 5;
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

pub(crate) fn validate_item_docs(gcx: Gcx<'_>, item_id: hir::ItemId) {
    let doc_id = gcx.hir.item(item_id).doc();
    if !doc_id.is_empty() {
        let _ = gcx.natspec_doc_comments(doc_id);
    }
}

pub(crate) fn resolve_doc_comments<'gcx>(
    gcx: Gcx<'gcx>,
    doc_id: hir::DocId,
) -> &'gcx [hir::NatSpecItem] {
    if doc_id.is_empty() {
        return &[];
    }
    Resolver::new(gcx).resolve_doc(doc_id)
}

struct Resolver<'gcx> {
    gcx: Gcx<'gcx>,
}

impl<'gcx> Resolver<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx }
    }

    /// Resolves a NatSpec doc, validating all tags and expanding `@inheritdoc`.
    fn resolve_doc(&self, doc_id: hir::DocId) -> &'gcx [hir::NatSpecItem] {
        let doc = self.gcx.hir.doc(doc_id);
        let (item_id, source_id) = (doc.item, doc.source);
        let (local_tags, inheritdoc) =
            self.validate_item_natspec(&doc.ast_comments, item_id, source_id);

        if let Some((contract_id, item_id)) = inheritdoc {
            let inherit_doc_id = self.find_inherited_item(item_id, contract_id).and_then(
                |inherited| match inherited {
                    hir::ItemId::Function(id) => Some(self.gcx.hir.function(id).doc),
                    hir::ItemId::Variable(id) => Some(self.gcx.hir.variable(id).doc),
                    _ => None,
                },
            );

            if let Some(inherit_doc_id) = inherit_doc_id
                && !inherit_doc_id.is_empty()
            {
                self.merge_natspec_tags(&local_tags, self.gcx.natspec_doc_comments(inherit_doc_id))
            } else {
                self.gcx.arena().alloc_slice_copy(&local_tags)
            }
        } else {
            self.gcx.arena().alloc_slice_copy(&local_tags)
        }
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

                        if let Some(contract_id) = self
                            .validate_inheritdoc_contract(contract, tag_span, item_id, source_id)
                        {
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
                            self.gcx.dcx().emit_err(
                                tag_span,
                                format!(
                                    "tag `@param` references non-existent parameter '{}'",
                                    name.name
                                ),
                            );
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
                            self.gcx.dcx().emit_err(tag_span, format!(
                                "too many `@return` tags: function has {} return value{}, found {}",
                                return_count,
                                if return_count == 1 { "" } else { "s" },
                                state.return_count
                            ));
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
            self.gcx.dcx().emit_err(
                natspec.span,
                "tag `@return` does not contain the name of its return parameter",
            );
            return None;
        };

        let name = Symbol::intern(name);
        if !rets.iter().any(|&id| self.gcx.hir.variable(id).name.is_some_and(|n| n.name == name)) {
            self.gcx.dcx().emit_err(
                natspec.span,
                format!("tag `@return` references non-existent return parameter '{name}'"),
            );
            return None;
        }

        let mut item = natspec;
        item.kind = ast::NatSpecKind::Return { name: Some(Ident::new(name, natspec.span)) };
        item.content_start = content_start as u32;
        Some(item)
    }

    #[cold]
    fn emit_forbidden_tag_error(&self, tag_name: &str, tag_span: Span, item_id: hir::ItemId) {
        let item_desc = self.gcx.hir.item(item_id).description();
        self.gcx.dcx().emit_err(tag_span, format!("tag `{tag_name}` not valid for {item_desc}s"));
    }

    #[cold]
    fn emit_duplicate_tag_error(&self, tag_name: &str, tag_span: Span) {
        self.gcx.dcx().emit_err(tag_span, format!("tag {tag_name} can only be given once"));
    }

    #[cold]
    fn emit_duplicate_param_error(&self, param_name: Symbol, tag_span: Span, prev_span: Span) {
        self.gcx.dcx().emit_err_span_note(
            tag_span,
            format!("duplicate documentation for parameter '{param_name}'"),
            prev_span,
            "previously documented here",
        );
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

    /// Validates contract resolution for `@inheritdoc`.
    /// Returns the resolved contract ID if validation passed.
    #[inline]
    fn validate_inheritdoc_contract(
        &self,
        contract_ident: &solar_interface::Ident,
        tag_span: Span,
        item_id: hir::ItemId,
        source_id: hir::SourceId,
    ) -> Option<hir::ContractId> {
        let dcx = self.gcx.dcx();

        let cache_key = (contract_ident.name, source_id);
        let contract_id = self.gcx.natspec_contract_in_source(cache_key);

        let Some(contract_id) = contract_id else {
            dcx.emit_err(
                tag_span,
                format!(
                    "tag `@inheritdoc` references inexistent contract \"{}\"",
                    contract_ident.name
                ),
            );
            return None;
        };

        let item_contract = match item_id {
            hir::ItemId::Function(id) => self.gcx.hir.function(id).contract,
            hir::ItemId::Variable(id) => self.gcx.hir.variable(id).contract,
            _ => return None,
        };

        if let Some(contract) = item_contract
            && !self.gcx.hir.contract_is_or_inherits_from(contract, contract_id)
        {
            dcx.emit_err(tag_span, format!(
                "tag `@inheritdoc` references contract \"{}\", which is not a base of this contract",
                contract_ident.name
            ));
            return None;
        }

        if self.find_inherited_item(item_id, contract_id).is_none() {
            dcx.emit_err(tag_span, format!(
                "tag `@inheritdoc` references contract \"{}\", but the contract does not contain a matching item that can be inherited",
                contract_ident.name
            ));
            return None;
        }

        Some(contract_id)
    }

    /// Merges local and inherited natspec tags.
    ///
    /// Rules:
    /// - `@notice`, `@dev`, `@title`, `@author`: local overrides inherited.
    /// - `@param`, `@return`: local section overrides inherited section.
    /// - `@custom`: not inherited.
    fn merge_natspec_tags(
        &self,
        items: &[hir::NatSpecItem],
        inherited_tags: &'gcx [hir::NatSpecItem],
    ) -> &'gcx [hir::NatSpecItem] {
        use hir::NatSpecKind as HirKind;

        let mut local_tags = LocalTags::empty();
        let mut merged = SmallVec::<[hir::NatSpecItem; 8]>::from_slice(items);

        for item in items.iter() {
            match &item.kind {
                HirKind::Notice => local_tags.insert(LocalTags::NOTICE),
                HirKind::Dev => local_tags.insert(LocalTags::DEV),
                HirKind::Title => local_tags.insert(LocalTags::TITLE),
                HirKind::Author => local_tags.insert(LocalTags::AUTHOR),
                HirKind::Param { .. } => local_tags.insert(LocalTags::PARAM),
                HirKind::Return { .. } => local_tags.insert(LocalTags::RETURN),
                HirKind::Custom { .. } | HirKind::Internal { .. } | HirKind::Inheritdoc { .. } => {}
            }
        }

        for item in inherited_tags.iter() {
            let should_inherit = match &item.kind {
                HirKind::Notice => !local_tags.contains(LocalTags::NOTICE),
                HirKind::Dev => !local_tags.contains(LocalTags::DEV),
                HirKind::Title => !local_tags.contains(LocalTags::TITLE),
                HirKind::Author => !local_tags.contains(LocalTags::AUTHOR),
                HirKind::Param { .. } => !local_tags.contains(LocalTags::PARAM),
                HirKind::Return { .. } => !local_tags.contains(LocalTags::RETURN),
                HirKind::Custom { .. } | HirKind::Internal { .. } => false,
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
                    .map(|func| gcx.natspec_doc_comments(func.doc))
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
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 0);

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
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Param { .. })), 1);
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Param { name } if name.name == sym::x),
                "from child3",
                "Child3 @param x",
            );
            assert!(!c.iter().any(
                |i| matches!(i.kind, NatSpecKind::Param { name } if name.name.as_str() == "y")
            ));

            let c = get_comments("Child4", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Return { .. })), 1);
            assert_tag_contains(
                c,
                |k| matches!(k, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "success"),
                "Child4 override",
                "Child4 @return success",
            );
            assert!(!c.iter().any(
                |i| matches!(i.kind, NatSpecKind::Return { name: Some(n) } if n.name.as_str() == "value")
            ));

            let c = get_comments("Child5", "foo");
            assert_eq!(count_tags(c, |k| matches!(k, NatSpecKind::Custom { .. })), 1);
            assert!(!c.iter().any(
                |i| matches!(i.kind, NatSpecKind::Custom { name } if name.name.as_str() == "security")
            ));
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
            assert_eq!(count_tags(g, |k| matches!(k, NatSpecKind::Custom { .. })), 0);
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
        gcx.natspec_doc_comments(gcx.hir.function(id).doc)
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
            .map(|var| gcx.natspec_doc_comments(var.doc))
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
            let _ = compiler.analysis()?;
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

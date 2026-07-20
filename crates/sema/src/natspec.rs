use crate::{
    hir,
    ty::{Gcx, Ty, TyAbiPrinter, TyAbiPrinterMode, TyKind},
};
use solar_ast as ast;
use solar_data_structures::{BumpExt, map::FxHashSet, smallvec::SmallVec};
use solar_interface::{Ident, Span, Symbol};
use std::ops::Range;

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct ResolvedNatSpec<'gcx> {
    items: &'gcx [hir::NatSpecItem],
    local_len: usize,
    local_tags: LocalTags,
    inheritdoc_target: Option<(Span, hir::ItemId)>,
}

impl<'gcx> ResolvedNatSpec<'gcx> {
    pub(crate) fn items(self) -> &'gcx [hir::NatSpecItem] {
        self.items
    }
}

/// Validated and resolved NatSpec for an HIR item.
///
/// [`Self::items`] preserves the existing flat resolved representation, including the names used
/// by inherited declarations. [`Self::parameter`] and [`Self::return_`] instead align each NatSpec
/// section with the corresponding position in the current item. This keeps inherited
/// documentation correct when an override renames parameters or return values.
///
/// A local `@param` or `@return` tag replaces its entire inherited section. Each position contains
/// a slice because some item kinds permit more than one tag for the same name.
#[derive(Clone, Copy, Debug, Default)]
pub struct NatSpecView<'gcx> {
    items: &'gcx [hir::NatSpecItem],
    parameters: PositionalNatSpec<'gcx>,
    returns: PositionalNatSpec<'gcx>,
}

impl<'gcx> NatSpecView<'gcx> {
    /// Returns all validated and resolved NatSpec items in their flat representation.
    pub fn items(self) -> &'gcx [hir::NatSpecItem] {
        self.items
    }

    /// Returns the NatSpec items for the parameter at `index` in the current item.
    ///
    /// Returns an empty slice when the parameter has no documentation or `index` is out of bounds.
    pub fn parameter(self, index: usize) -> &'gcx [hir::NatSpecItem] {
        self.parameters.get(index)
    }

    /// Returns the NatSpec items for the return value at `index` in the current item.
    ///
    /// Returns an empty slice when the return value has no documentation or `index` is out of
    /// bounds.
    pub fn return_(self, index: usize) -> &'gcx [hir::NatSpecItem] {
        self.returns.get(index)
    }
}

#[derive(Clone, Copy, Debug, Default)]
struct PositionalNatSpec<'gcx> {
    items: &'gcx [hir::NatSpecItem],
    ranges: &'gcx [Range<usize>],
}

impl<'gcx> PositionalNatSpec<'gcx> {
    fn get(self, index: usize) -> &'gcx [hir::NatSpecItem] {
        let Some(range) = self.ranges.get(index) else { return &[] };
        &self.items[range.clone()]
    }
}

#[derive(Default)]
struct PositionalNatSpecBuilder {
    items: SmallVec<[hir::NatSpecItem; 8]>,
    ranges: SmallVec<[Range<usize>; 8]>,
}

impl PositionalNatSpecBuilder {
    fn push(&mut self, items: impl IntoIterator<Item = hir::NatSpecItem>) {
        let start = self.items.len();
        self.items.extend(items);
        self.ranges.push(start..self.items.len());
    }

    fn finish<'gcx>(self, gcx: Gcx<'gcx>) -> PositionalNatSpec<'gcx> {
        if self.items.is_empty() {
            return PositionalNatSpec::default();
        }
        PositionalNatSpec {
            items: gcx.bump().alloc_smallvec(self.items),
            ranges: gcx.bump().alloc_smallvec(self.ranges),
        }
    }
}

#[derive(Clone, Copy, Default)]
struct CallableVariables<'gcx> {
    parameters: &'gcx [hir::VariableId],
    returns: &'gcx [hir::VariableId],
}

bitflags::bitflags! {
    /// Tracks which documentation tags are locally defined.
    #[derive(Clone, Copy, Debug, Default)]
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

impl LocalTags {
    fn from_items(items: &[hir::NatSpecItem]) -> Self {
        let mut tags = Self::empty();
        for item in items {
            match item.kind {
                hir::NatSpecKind::Notice => tags.insert(Self::NOTICE),
                hir::NatSpecKind::Dev => tags.insert(Self::DEV),
                hir::NatSpecKind::Title => tags.insert(Self::TITLE),
                hir::NatSpecKind::Author => tags.insert(Self::AUTHOR),
                hir::NatSpecKind::Param { .. } => tags.insert(Self::PARAM),
                hir::NatSpecKind::Return { .. } => tags.insert(Self::RETURN),
                hir::NatSpecKind::Custom { .. }
                | hir::NatSpecKind::Internal { .. }
                | hir::NatSpecKind::Inheritdoc { .. } => {}
            }
        }
        tags
    }
}

impl TagPermissions {
    fn from_item_id(item_id: hir::ItemId) -> Self {
        match item_id {
            hir::ItemId::Contract(_) | hir::ItemId::Enum(_) => Self::TITLE_AUTHOR | Self::RETURN,
            hir::ItemId::Struct(_) => Self::TITLE_AUTHOR | Self::PARAM | Self::RETURN,
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
        let resolved = gcx.natspec_resolution(item_id);
        if gcx.sess.opts.unstable.print_natspec {
            emit_natspec_debug(gcx, item_id, resolved);
        }
    }
}

pub(crate) fn resolve_item<'gcx>(gcx: Gcx<'gcx>, item_id: hir::ItemId) -> ResolvedNatSpec<'gcx> {
    Resolver::new(gcx).resolve_item(item_id)
}

pub(crate) fn resolve_view<'gcx>(gcx: Gcx<'gcx>, item_id: hir::ItemId) -> NatSpecView<'gcx> {
    let resolved = gcx.natspec_resolution(item_id);
    let inherited = resolved.inheritdoc_target.map(|(_, target)| gcx.natspec_view(target));
    let local_items = &resolved.items[..resolved.local_len];
    let (parameters, returns) = Resolver::new(gcx).positional_sections(
        item_id,
        local_items,
        resolved.local_tags,
        inherited,
    );
    NatSpecView { items: resolved.items, parameters, returns }
}

fn emit_natspec_debug(gcx: Gcx<'_>, item_id: hir::ItemId, resolved: ResolvedNatSpec<'_>) {
    let mut docs = resolved
        .items
        .iter()
        .filter(|item| !matches!(item.kind, hir::NatSpecKind::Inheritdoc { .. }))
        .peekable();
    if docs.peek().is_none() {
        return;
    }

    let item = gcx.hir.item(item_id);
    let item_desc = item.description();
    let item_name = if item.name().is_some() {
        format!("`{}`", gcx.item_canonical_name(item_id))
    } else {
        "<unnamed>".into()
    };
    let mut diag =
        gcx.dcx().err(format!("resolved NatSpec for {item_desc} {item_name}")).span(item.span());
    let resolver = Resolver::new(gcx);
    if let Some((span, inherited_item)) = resolved.inheritdoc_target {
        diag = diag.span_note(
            span,
            format!("inherits NatSpec from {}", resolver.format_inherited_item(inherited_item)),
        );
    }
    for item in docs {
        diag = diag.span_note(item.span, item.to_string());
    }
    diag.emit();
}

struct Resolver<'gcx> {
    gcx: Gcx<'gcx>,
}

impl<'gcx> Resolver<'gcx> {
    fn new(gcx: Gcx<'gcx>) -> Self {
        Self { gcx }
    }

    /// Resolves a NatSpec doc, validating all tags and expanding `@inheritdoc`.
    fn resolve_item(&self, item_id: hir::ItemId) -> ResolvedNatSpec<'gcx> {
        let doc_id = self.gcx.hir.item(item_id).doc();
        if doc_id.is_empty() {
            return ResolvedNatSpec::default();
        }

        let doc = self.gcx.hir.doc(doc_id);
        let (local_tags, inheritdoc_target) =
            self.validate_item_natspec(&doc.ast_comments, item_id, doc.source);
        let local_sections = LocalTags::from_items(&local_tags);
        let inherited = inheritdoc_target.map(|(_, target)| self.gcx.natspec_resolution(target));
        let items: &'gcx [hir::NatSpecItem] = if let Some(inherited) = inherited {
            self.merge_natspec_tags(&local_tags, local_sections, inherited.items)
        } else {
            &*self.gcx.arena().alloc_slice_copy(&local_tags)
        };

        ResolvedNatSpec {
            items,
            local_len: local_tags.len(),
            local_tags: local_sections,
            inheritdoc_target,
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
    ) -> (SmallVec<[hir::NatSpecItem; 8]>, Option<(Span, hir::ItemId)>) {
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

                        if let Some(inherited_item) = self
                            .validate_inheritdoc_contract(contract, tag_span, item_id, source_id)
                        {
                            local_tags.push(*natspec);
                            inheritdoc = Some((tag_span, inherited_item));
                        }
                    }
                    NatSpecKind::Param { name } => {
                        if !permissions.contains(TagPermissions::PARAM) {
                            self.emit_forbidden_tag_error("@param", tag_span, item_id);
                            continue;
                        }

                        if matches!(item_id, hir::ItemId::Struct(_)) {
                            local_tags.push(*natspec);
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

                        let rets = returns.get_or_insert_with(|| match item_id {
                            hir::ItemId::Function(id) => self.gcx.hir.function(id).returns,
                            hir::ItemId::Variable(id) => self
                                .gcx
                                .hir
                                .variable(id)
                                .getter
                                .map_or(&[], |getter| self.gcx.hir.function(getter).returns),
                            _ => &[],
                        });
                        let return_count = rets.len();

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
                            && let Some(item) =
                                self.lower_return_natspec(*natspec, rets, state.return_count - 1)
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
        index: usize,
    ) -> Option<hir::NatSpecItem> {
        let &return_id = rets.get(index)?;
        let Some(expected) = self.gcx.hir.variable(return_id).name else {
            return Some(natspec);
        };

        let content = natspec.symbol.as_str();
        let Some((documented, content_start)) = solar_parse::natspec::first_word(
            content,
            natspec.content_start as usize,
            natspec.content_end as usize,
        ) else {
            self.gcx.dcx().emit_err(
                natspec.span,
                "tag `@return` does not contain the name of its return parameter",
            );
            return None;
        };

        let documented_name = Symbol::intern(documented);
        if documented_name != expected.name {
            if rets.iter().any(|&id| {
                self.gcx.hir.variable(id).name.is_some_and(|name| name.name == documented_name)
            }) {
                self.gcx.dcx().emit_err(
                    natspec.span,
                    "tag `@return` does not contain the name of its return parameter",
                );
            } else {
                self.gcx.dcx().emit_err(
                    natspec.span,
                    format!(
                        "tag `@return` references non-existent return parameter '{documented_name}'"
                    ),
                );
            }
            return None;
        }

        // Content offsets are relative to the comment, while `span` covers only the tag name.
        let tag_end = content[..natspec.content_start as usize].trim_ascii_end().len();
        let name_end = content[..content_start].trim_ascii_end().len();
        let name_start = name_end - documented.len();
        let name_lo = natspec.span.hi() + (name_start - tag_end) as u32;
        let name_span = Span::new(name_lo, name_lo + documented.len() as u32);

        let mut item = natspec;
        item.kind = ast::NatSpecKind::Return { name: Some(Ident::new(expected.name, name_span)) };
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
    /// Returns the inherited item if validation passed.
    #[inline]
    fn validate_inheritdoc_contract(
        &self,
        contract_ident: &solar_interface::Ident,
        tag_span: Span,
        item_id: hir::ItemId,
        source_id: hir::SourceId,
    ) -> Option<hir::ItemId> {
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

        if let Some(contract) = item_contract {
            let linearized_bases = &self.gcx.hir.contract(contract).linearized_bases;
            if contract == contract_id || !linearized_bases.contains(&contract_id) {
                dcx.emit_err(tag_span, format!(
                    "tag `@inheritdoc` references contract \"{}\", which is not a base of this contract",
                    contract_ident.name
                ));
                return None;
            }
        }

        let Some(inherited_item) = self.find_inherited_item(item_id, contract_id) else {
            dcx.emit_err(tag_span, format!(
                "tag `@inheritdoc` references contract \"{}\", but the contract does not contain a matching item that can be inherited",
                contract_ident.name
            ));
            return None;
        };

        Some(inherited_item)
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
        local_tags: LocalTags,
        inherited_tags: &'gcx [hir::NatSpecItem],
    ) -> &'gcx [hir::NatSpecItem] {
        use hir::NatSpecKind as HirKind;

        let mut merged = SmallVec::<[hir::NatSpecItem; 8]>::from_slice(items);

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

    fn positional_sections(
        &self,
        item_id: hir::ItemId,
        local_items: &[hir::NatSpecItem],
        local_tags: LocalTags,
        inherited: Option<NatSpecView<'gcx>>,
    ) -> (PositionalNatSpec<'gcx>, PositionalNatSpec<'gcx>) {
        let variables = self.callable_variables(item_id);

        let mut parameters = PositionalNatSpecBuilder::default();
        for (index, &parameter_id) in variables.parameters.iter().enumerate() {
            if local_tags.contains(LocalTags::PARAM) {
                let name = self.gcx.hir.variable(parameter_id).name.map(|name| name.name);
                parameters.push(local_items.iter().copied().filter(|item| {
                    matches!(item.kind, hir::NatSpecKind::Param { name: documented } if Some(documented.name) == name)
                }));
            } else if let Some(inherited) = inherited {
                parameters.push(inherited.parameter(index).iter().copied());
            } else {
                parameters.push(std::iter::empty());
            }
        }

        let mut returns = PositionalNatSpecBuilder::default();
        let local_returns = local_tags.contains(LocalTags::RETURN);
        let return_items = local_returns.then(|| {
            local_items
                .iter()
                .copied()
                .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
                .collect::<SmallVec<[_; 8]>>()
        });
        let named_returns =
            variables.returns.iter().any(|&id| self.gcx.hir.variable(id).name.is_some());
        let mut unnamed_return_items = return_items
            .iter()
            .flatten()
            .copied()
            .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { name: None }));

        for (index, &return_id) in variables.returns.iter().enumerate() {
            if let Some(local_returns) = &return_items {
                if named_returns {
                    let name = self.gcx.hir.variable(return_id).name.map(|name| name.name);
                    if let Some(name) = name {
                        returns.push(local_returns.iter().copied().filter(|item| {
                            matches!(item.kind, hir::NatSpecKind::Return { name: Some(documented) } if documented.name == name)
                        }));
                    } else {
                        returns.push(unnamed_return_items.next());
                    }
                } else {
                    returns.push(local_returns.get(index).copied());
                }
            } else if let Some(inherited) = inherited {
                returns.push(inherited.return_(index).iter().copied());
            } else {
                returns.push(std::iter::empty());
            }
        }

        (parameters.finish(self.gcx), returns.finish(self.gcx))
    }

    fn callable_variables(&self, item_id: hir::ItemId) -> CallableVariables<'gcx> {
        match item_id {
            hir::ItemId::Function(id) => {
                let function = self.gcx.hir.function(id);
                CallableVariables { parameters: function.parameters, returns: function.returns }
            }
            hir::ItemId::Struct(id) => {
                CallableVariables { parameters: self.gcx.hir.strukt(id).fields, returns: &[] }
            }
            hir::ItemId::Event(id) => {
                CallableVariables { parameters: self.gcx.hir.event(id).parameters, returns: &[] }
            }
            hir::ItemId::Error(id) => {
                CallableVariables { parameters: self.gcx.hir.error(id).parameters, returns: &[] }
            }
            hir::ItemId::Variable(id) => {
                let variable = self.gcx.hir.variable(id);
                if let Some(getter) = variable.getter {
                    let function = self.gcx.hir.function(getter);
                    CallableVariables { parameters: function.parameters, returns: function.returns }
                } else if let hir::TypeKind::Function(function) = variable.ty.kind {
                    CallableVariables { parameters: function.parameters, returns: function.returns }
                } else {
                    CallableVariables::default()
                }
            }
            hir::ItemId::Contract(_) | hir::ItemId::Enum(_) | hir::ItemId::Udvt(_) => {
                CallableVariables::default()
            }
        }
    }

    fn format_inherited_item(&self, item_id: hir::ItemId) -> String {
        let item = self.gcx.hir.item(item_id);
        let mut name = self.gcx.item_canonical_name(item_id).to_string();
        if let Some(params) = self.item_callable_parameter_types(item_id) {
            TyAbiPrinter::new(self.gcx, &mut name, TyAbiPrinterMode::Signature)
                .print_tuple(params.iter().copied())
                .unwrap();
        }
        format!("{} `{name}`", item.description())
    }

    /// Finds the item in a contract that matches the given item (for inheritance).
    fn find_inherited_item(
        &self,
        item_id: hir::ItemId,
        contract_id: hir::ContractId,
    ) -> Option<hir::ItemId> {
        for &base_item_id in self.gcx.base_override_items(item_id) {
            if self.gcx.hir.item(base_item_id).contract() == Some(contract_id) {
                return Some(base_item_id);
            }
            if let Some(inherited_item) = self.find_inherited_item(base_item_id, contract_id) {
                return Some(inherited_item);
            }
        }

        None
    }

    fn item_callable_parameter_types(&self, item_id: hir::ItemId) -> Option<&'gcx [Ty<'gcx>]> {
        let ty = match item_id {
            hir::ItemId::Function(id) => self.gcx.type_of_item(id.into()),
            hir::ItemId::Variable(id) => {
                let getter_id = self.gcx.hir.variable(id).getter?;
                self.gcx.type_of_item(getter_id.into())
            }
            _ => return None,
        };
        let ty = ty.as_externally_callable_function(false, self.gcx);
        if let TyKind::Fn(fn_ty) = ty.kind { Some(fn_ty.parameters) } else { None }
    }
}

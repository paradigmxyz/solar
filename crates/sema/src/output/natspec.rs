//! NatSpec outputs matching solc's Standard JSON output fields.

use super::DocumentationKind;
use crate::{
    hir,
    natspec::{LocalTags, is_untagged, is_untagged_line, starts_group},
    ty::Gcx,
};
use serde::Serialize;
use solar_data_structures::map::{FxHashMap, FxHashSet, FxIndexMap};
use solar_interface::Span;

/// NatSpec documentation in solc's Standard JSON `userdoc` and `devdoc` output fields.
///
/// Created by [`Gcx::user_documentation`] and [`Gcx::dev_documentation`].
#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Documentation {
    pub kind: DocumentationKind,
    pub methods: FxIndexMap<String, DocumentationItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub events: FxIndexMap<String, DocumentationItem>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub errors: FxIndexMap<String, Vec<DocumentationItem>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub state_variables: FxIndexMap<String, DocumentationItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(flatten)]
    pub custom: FxIndexMap<String, String>,
    pub version: u8,
}

#[derive(Debug, Default, Serialize)]
pub struct DocumentationItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub params: FxIndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#return: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    pub returns: FxIndexMap<String, String>,
    #[serde(flatten)]
    pub custom: FxIndexMap<String, String>,
}

impl Documentation {
    fn new(kind: DocumentationKind) -> Self {
        Self {
            kind,
            methods: FxIndexMap::default(),
            author: None,
            details: None,
            events: FxIndexMap::default(),
            errors: FxIndexMap::default(),
            state_variables: FxIndexMap::default(),
            notice: None,
            title: None,
            custom: FxIndexMap::default(),
            version: 1,
        }
    }
}

impl DocumentationItem {
    fn is_empty(&self) -> bool {
        self.notice.is_none()
            && self.author.is_none()
            && self.details.is_none()
            && self.params.is_empty()
            && self.r#return.is_none()
            && self.returns.is_empty()
            && self.custom.is_empty()
    }
}

impl<'gcx> Gcx<'gcx> {
    pub(crate) fn build_dev_documentation(self, contract_id: hir::ContractId) -> Documentation {
        let contract = self.hir.contract(contract_id);
        let contract_doc = dev_doc_item(self, contract_id.into());
        let mut documentation = Documentation::new(DocumentationKind::Dev);
        documentation.author = contract_doc.author;
        documentation.details = contract_doc.details;
        documentation.title =
            natspec_text(self, contract_id.into(), |kind| matches!(kind, hir::NatSpecKind::Title));
        documentation.custom = contract_doc.custom;

        if let Some(constructor) = contract.ctor {
            let documentation_item = dev_doc_item(self, constructor.into());
            if !documentation_item.is_empty() {
                documentation.methods.insert("constructor".into(), documentation_item);
            }
        }

        for interface_function in self.interface_functions(contract_id) {
            let function_id = interface_function.id;
            let function = self.hir.function(function_id);
            if function.is_getter() {
                continue;
            }

            let mut documentation_item = dev_doc_item(self, function_id.into());
            documentation_item.returns = return_docs(self, function_id.into(), function.returns);
            if !documentation_item.is_empty() {
                documentation.methods.insert(
                    self.item_signature(function_id.into()).to_string(),
                    documentation_item,
                );
            }
        }

        for variable_id in contract.variables() {
            let variable = self.hir.variable(variable_id);
            let mut documentation_item = dev_doc_item(self, variable_id.into());
            // solc writes `return` only for a single `@return` tag.
            let mut returns = doc_items(self, variable_id.into())
                .into_iter()
                .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }));
            if let (Some(item), None) = (returns.next(), returns.next()) {
                documentation_item.r#return = Some(item.content);
            }
            if let Some(getter) = variable.getter {
                documentation_item.returns =
                    return_docs(self, variable_id.into(), self.hir.function(getter).returns);
            }
            if !documentation_item.is_empty() {
                documentation
                    .state_variables
                    .insert(variable.name.unwrap().to_string(), documentation_item);
            }
        }

        for (signature, event_id) in unique_interface_events(self, contract_id) {
            let documentation_item = dev_doc_item(self, event_id.into());
            if !documentation_item.is_empty() {
                documentation.events.insert(signature, documentation_item);
            }
        }
        for error_id in interface_errors(self, contract_id) {
            let documentation_item = dev_doc_item(self, error_id.into());
            if !documentation_item.is_empty() {
                documentation
                    .errors
                    .entry(self.item_signature(error_id.into()).to_string())
                    .or_default()
                    .push(documentation_item);
            }
        }

        documentation
    }

    pub(crate) fn build_user_documentation(self, contract_id: hir::ContractId) -> Documentation {
        let contract = self.hir.contract(contract_id);
        let mut documentation = Documentation::new(DocumentationKind::User);
        documentation.notice =
            natspec_text(self, contract_id.into(), |kind| matches!(kind, hir::NatSpecKind::Notice));

        if let Some(constructor) = contract.ctor
            && let Some(notice) = natspec_text(self, constructor.into(), |kind| {
                matches!(kind, hir::NatSpecKind::Notice)
            })
        {
            documentation.methods.insert(
                "constructor".into(),
                DocumentationItem { notice: Some(notice), ..Default::default() },
            );
        }

        for interface_function in self.interface_functions(contract_id) {
            let function_id = interface_function.id;
            let function = self.hir.function(function_id);
            let item = function.gettee.map_or(function_id.into(), hir::ItemId::from);
            if let Some(notice) =
                natspec_text(self, item, |kind| matches!(kind, hir::NatSpecKind::Notice))
            {
                documentation.methods.insert(
                    self.item_signature(function_id.into()).to_string(),
                    DocumentationItem { notice: Some(notice), ..Default::default() },
                );
            }
        }

        for (signature, event_id) in unique_interface_events(self, contract_id) {
            if let Some(notice) =
                natspec_text(self, event_id.into(), |kind| matches!(kind, hir::NatSpecKind::Notice))
            {
                documentation.events.insert(
                    signature,
                    DocumentationItem { notice: Some(notice), ..Default::default() },
                );
            }
        }
        for error_id in interface_errors(self, contract_id) {
            if let Some(notice) =
                natspec_text(self, error_id.into(), |kind| matches!(kind, hir::NatSpecKind::Notice))
            {
                documentation
                    .errors
                    .entry(self.item_signature(error_id.into()).to_string())
                    .or_default()
                    .push(DocumentationItem { notice: Some(notice), ..Default::default() });
            }
        }

        documentation
    }
}

fn natspec_text(
    gcx: Gcx<'_>,
    item: hir::ItemId,
    mut matches: impl FnMut(hir::NatSpecKind) -> bool,
) -> Option<String> {
    let text = doc_items(gcx, item)
        .into_iter()
        .filter(|item| matches(item.kind))
        .map(|item| item.content)
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn dev_doc_item(gcx: Gcx<'_>, item: hir::ItemId) -> DocumentationItem {
    let mut documentation = DocumentationItem::default();
    for item in doc_items(gcx, item) {
        let content = item.content.as_str();
        match item.kind {
            hir::NatSpecKind::Author => append_doc(&mut documentation.author, content),
            hir::NatSpecKind::Dev => append_doc(&mut documentation.details, content),
            hir::NatSpecKind::Param { name } => {
                documentation.params.entry(name.name.to_string()).or_default().push_str(content);
            }
            hir::NatSpecKind::Custom { name } => {
                documentation
                    .custom
                    .entry(format!("custom:{}", name.name))
                    .or_default()
                    .push_str(content);
            }
            hir::NatSpecKind::Title
            | hir::NatSpecKind::Notice
            | hir::NatSpecKind::Return { .. }
            | hir::NatSpecKind::Inheritdoc { .. }
            | hir::NatSpecKind::Internal { .. } => {}
        }
    }
    documentation
}

/// Appends a tag's text, like solc, which omits a tag without any.
fn append_doc(target: &mut Option<String>, content: &str) {
    if !content.is_empty() {
        target.get_or_insert_default().push_str(content);
    }
}

fn return_docs(
    gcx: Gcx<'_>,
    item: hir::ItemId,
    returns: &[hir::VariableId],
) -> FxIndexMap<String, String> {
    doc_items(gcx, item)
        .into_iter()
        .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
        .enumerate()
        .filter_map(|(index, item)| {
            let variable = gcx.hir.variable(*returns.get(index)?);
            let name = variable.name.map_or_else(|| format!("_{index}"), |name| name.to_string());
            Some((name, item.content))
        })
        .collect()
}

/// A NatSpec item with its text as solc's documentation output has it.
struct DocItem {
    kind: hir::NatSpecKind,
    content: String,
    /// Whether the line of the tag ends right after its name, so that the next line's leading
    /// whitespace goes.
    bare: bool,
}

/// Returns the NatSpec items of `item_id` with their text joined and trimmed like solc.
///
/// solc's scanner joins the lines of a doc comment with newlines, dropping the leading `///` of
/// each line and the leading whitespace and first `*` of each line of a block comment, and
/// skipping empty lines. Its `DocStringParser` then strips the whitespace that starts a tag and
/// appends every later line to the tag before it, with a space unless the line starts with one.
/// A `///` line without a tag is such a continuation, so it extends the item before it instead of
/// being a notice of its own.
///
/// Only the last group of doc comments counts, for the tags, the `@inheritdoc` target and whether
/// the item inherits implicitly. The target's items are joined on their own and follow the local
/// ones, without the kinds the local ones replace.
fn doc_items(gcx: Gcx<'_>, item_id: hir::ItemId) -> Vec<DocItem> {
    let resolved = gcx.natspec_resolution(item_id);
    let ignored = ignored_doc_comments(gcx, gcx.hir.item(item_id).doc());
    let read = |span: Span| !ignored.is_some_and(|ignored| ignored.contains(span));
    let local =
        resolved.local_items().iter().copied().filter(|item| read(item.span)).collect::<Vec<_>>();
    let target = resolved.inheritdoc_target().filter(|&(span, _)| read(span));
    if local.is_empty() && target.is_none() {
        return inherited_doc_items(gcx, item_id);
    }
    let mut items: Vec<DocItem> = Vec::new();
    for &item in &local {
        let untagged = is_untagged(&item);
        let bare = !item.symbol.as_str()[..item.content_start as usize].ends_with([' ', '\t']);
        if is_untagged_line(gcx, &item)
            && let Some(previous) = items.last_mut()
        {
            // A tag with nothing after it on its line takes the next line, without its leading
            // whitespace.
            if previous.content.is_empty() && previous.bare {
                previous.content.push_str(item.content().trim_start_matches([' ', '\t']));
            } else {
                append_doc_line(&mut previous.content, item.content());
            }
            continue;
        }
        let mut content = String::new();
        for (i, line) in item.content().split(['\n', '\r']).enumerate() {
            if i == 0 {
                content.push_str(if untagged {
                    line.trim_start_matches([' ', '\t'])
                } else {
                    line
                });
                continue;
            }
            // Block comment decoration: whitespace, then one `*`.
            let line = line.trim_ascii_start();
            let line =
                if line.starts_with("**") { line } else { line.strip_prefix('*').unwrap_or(line) };
            if line.is_empty() {
                continue;
            }
            if content.is_empty() && bare {
                content.push_str(line.trim_start_matches([' ', '\t']));
            } else {
                append_doc_line(&mut content, line);
            }
        }
        items.push(DocItem { kind: item.kind, content, bare });
    }
    if let Some((_, target)) = target {
        let local_tags = LocalTags::from_group(gcx, &local);
        items.extend(
            doc_items(gcx, target).into_iter().filter(|item| local_tags.inherits(&item.kind)),
        );
    }
    items
}

/// Returns the documentation an undocumented item inherits, like solc's `DocStringAnalyser`.
///
/// An item without NatSpec tags that overrides exactly one base takes the base's tags, except
/// custom ones. A function also needs the same parameter names as its base.
fn inherited_doc_items(gcx: Gcx<'_>, item_id: hir::ItemId) -> Vec<DocItem> {
    let &[base] = gcx.base_override_items(item_id) else { return Vec::new() };
    if let (hir::ItemId::Function(function), hir::ItemId::Function(base_function)) = (item_id, base)
    {
        let names = |id| gcx.hir.function(id).parameters.iter().map(|&p| gcx.hir.variable(p).name);
        if !names(function).eq(names(base_function)) {
            return Vec::new();
        }
    }
    let mut items = doc_items(gcx, base);
    items.retain(|item| !matches!(item.kind, hir::NatSpecKind::Custom { .. }));
    items
}

/// Appends a continuation line to a tag's text, as solc's `DocStringParser` does.
fn append_doc_line(content: &mut String, line: &str) {
    if line.is_empty() {
        return;
    }
    if !line.starts_with([' ', '\t']) {
        content.push(' ');
    }
    content.push_str(line);
}

/// Returns the span of the doc comments before the last group of an item's doc comments.
///
/// solc's scanner keeps only the last doc comment before a declaration, where consecutive `///`
/// lines form one comment, so it ignores the NatSpec of every earlier comment.
fn ignored_doc_comments(gcx: Gcx<'_>, doc_id: hir::DocId) -> Option<Span> {
    if doc_id.is_empty() {
        return None;
    }
    let docs = gcx.hir.doc(doc_id).ast_comments();
    let start = (1..docs.len()).rev().find(|&i| starts_group(gcx, &docs[i - 1], &docs[i]))?;
    Some(docs[0].span.with_hi(docs[start].span.lo()))
}

/// Returns the errors documented for a contract in solc's order, by AST ID: errors of the same
/// signature from different sources are listed by the order solc parses the sources in, then by
/// position.
fn interface_errors(gcx: Gcx<'_>, contract_id: hir::ContractId) -> Vec<hir::ErrorId> {
    let mut errors = gcx.interface_errors(contract_id).iter().collect::<Vec<_>>();
    let imported = errors.iter().any(|&error| gcx.hir.source(gcx.hir.error(error).source).imported);
    // Without imported sources, the order is by source name.
    let order = imported.then(|| gcx.source_parse_order(()));
    errors.sort_by(|&a, &b| {
        let (a, b) = (gcx.hir.error(a), gcx.hir.error(b));
        let source = match &order {
            Some(order) => order[a.source].cmp(&order[b.source]),
            None => {
                let name = |error: &hir::Error<'_>| &gcx.hir.source(error.source).file.name;
                name(a).cmp_source_unit_name(name(b))
            }
        };
        source.then(a.span.lo().cmp(&b.span.lo()))
    });
    errors
}

/// Returns the events documented for a contract with their signatures, like solc's
/// `Natspec::uniqueInterfaceEvents`.
///
/// The events the contract and its bases define come first, the first of each signature winning.
/// Events it emits from elsewhere follow, unless it emits several with the same signature.
fn unique_interface_events(
    gcx: Gcx<'_>,
    contract_id: hir::ContractId,
) -> Vec<(String, hir::EventId)> {
    let mut events = Vec::new();
    let mut signatures = FxHashSet::default();
    let mut defined = FxHashSet::default();
    for item in gcx.hir.contract_item_ids(contract_id) {
        if let hir::ItemId::Event(event_id) = item {
            defined.insert(event_id);
            let signature = gcx.item_signature(event_id.into()).to_string();
            if signatures.insert(signature.clone()) {
                events.push((signature, event_id));
            }
        }
    }

    let mut used = FxIndexMap::default();
    let mut used_counts = FxHashMap::<String, usize>::default();
    for event_id in gcx.interface_events(contract_id).iter() {
        if defined.contains(&event_id) {
            continue;
        }
        let signature = gcx.item_signature(event_id.into()).to_string();
        *used_counts.entry(signature.clone()).or_default() += 1;
        used.entry(signature).or_insert(event_id);
    }
    for (signature, event_id) in used {
        if used_counts[&signature] == 1 && !signatures.contains(&signature) {
            events.push((signature, event_id));
        }
    }
    events
}

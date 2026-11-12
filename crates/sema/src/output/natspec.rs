//! NatSpec outputs matching solc's Standard JSON output fields.

use super::DocumentationKind;
use crate::{hir, natspec::follows_as_is, ty::Gcx};
use serde::Serialize;
use solar_data_structures::map::{FxHashSet, FxIndexMap};
use std::borrow::Cow;

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
        let contract_doc = dev_doc_item(self, contract.doc);
        let mut documentation = Documentation::new(DocumentationKind::Dev);
        documentation.author = contract_doc.author;
        documentation.details = contract_doc.details;
        documentation.title =
            natspec_text(self, contract.doc, |kind| matches!(kind, hir::NatSpecKind::Title));
        documentation.custom = contract_doc.custom;

        if let Some(constructor) = contract.ctor {
            let documentation_item = dev_doc_item(self, self.hir.function(constructor).doc);
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

            let mut documentation_item = dev_doc_item(self, function.doc);
            documentation_item.returns = return_docs(self, function.doc, function.returns);
            if !documentation_item.is_empty() {
                documentation.methods.insert(
                    self.item_signature(function_id.into()).to_string(),
                    documentation_item,
                );
            }
        }

        for variable_id in contract.variables() {
            let variable = self.hir.variable(variable_id);
            let mut documentation_item = dev_doc_item(self, variable.doc);
            let return_text = self
                .natspec_doc_comments(variable.doc)
                .iter()
                .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
                .map(raw_return_content)
                .collect::<String>();
            if !return_text.is_empty() {
                documentation_item.r#return = Some(return_text);
            }
            if let Some(getter) = variable.getter {
                documentation_item.returns =
                    return_docs(self, variable.doc, self.hir.function(getter).returns);
            }
            if !documentation_item.is_empty() {
                documentation
                    .state_variables
                    .insert(variable.name.unwrap().to_string(), documentation_item);
            }
        }

        let mut event_signatures = FxHashSet::default();
        for item in self.hir.contract_item_ids(contract_id) {
            match item {
                hir::ItemId::Event(event_id) => {
                    let event = self.hir.event(event_id);
                    let signature = self.item_signature(event_id.into()).to_string();
                    if !event_signatures.insert(signature.clone()) {
                        continue;
                    }
                    let documentation_item = dev_doc_item(self, event.doc);
                    if !documentation_item.is_empty() {
                        documentation.events.insert(signature, documentation_item);
                    }
                }
                hir::ItemId::Error(error_id) => {
                    let error = self.hir.error(error_id);
                    let documentation_item = dev_doc_item(self, error.doc);
                    if !documentation_item.is_empty() {
                        documentation
                            .errors
                            .entry(self.item_signature(error_id.into()).to_string())
                            .or_default()
                            .push(documentation_item);
                    }
                }
                _ => {}
            }
        }

        documentation
    }

    pub(crate) fn build_user_documentation(self, contract_id: hir::ContractId) -> Documentation {
        let contract = self.hir.contract(contract_id);
        let mut documentation = Documentation::new(DocumentationKind::User);
        documentation.notice =
            natspec_text(self, contract.doc, |kind| matches!(kind, hir::NatSpecKind::Notice));

        if let Some(constructor) = contract.ctor
            && let Some(notice) = natspec_text(self, self.hir.function(constructor).doc, |kind| {
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
            let doc = function.gettee.map_or(function.doc, |gettee| self.hir.variable(gettee).doc);
            if let Some(notice) =
                natspec_text(self, doc, |kind| matches!(kind, hir::NatSpecKind::Notice))
            {
                documentation.methods.insert(
                    self.item_signature(function_id.into()).to_string(),
                    DocumentationItem { notice: Some(notice), ..Default::default() },
                );
            }
        }

        let mut event_signatures = FxHashSet::default();
        for item in self.hir.contract_item_ids(contract_id) {
            match item {
                hir::ItemId::Event(event_id) => {
                    let event = self.hir.event(event_id);
                    let signature = self.item_signature(event_id.into()).to_string();
                    if !event_signatures.insert(signature.clone()) {
                        continue;
                    }
                    if let Some(notice) = natspec_text(self, event.doc, |kind| {
                        matches!(kind, hir::NatSpecKind::Notice)
                    }) {
                        documentation.events.insert(
                            signature,
                            DocumentationItem { notice: Some(notice), ..Default::default() },
                        );
                    }
                }
                hir::ItemId::Error(error_id) => {
                    let error = self.hir.error(error_id);
                    if let Some(notice) = natspec_text(self, error.doc, |kind| {
                        matches!(kind, hir::NatSpecKind::Notice)
                    }) {
                        documentation
                            .errors
                            .entry(self.item_signature(error_id.into()).to_string())
                            .or_default()
                            .push(DocumentationItem { notice: Some(notice), ..Default::default() });
                    }
                }
                _ => {}
            }
        }

        documentation
    }
}

fn natspec_text(
    gcx: Gcx<'_>,
    doc_id: hir::DocId,
    mut matches: impl FnMut(hir::NatSpecKind) -> bool,
) -> Option<String> {
    let text = gcx
        .natspec_doc_comments(doc_id)
        .iter()
        .copied()
        .filter(|item| matches(item.kind))
        .map(|item| solc_content(item.content()).into_owned())
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn dev_doc_item(gcx: Gcx<'_>, doc_id: hir::DocId) -> DocumentationItem {
    let mut documentation = DocumentationItem::default();
    for item in gcx.natspec_doc_comments(doc_id).iter().copied() {
        let content = solc_content(item.content());
        let content = &*content;
        match item.kind {
            hir::NatSpecKind::Author => append_doc(&mut documentation.author, content),
            hir::NatSpecKind::Dev => append_doc(&mut documentation.details, content),
            // Like solc, the last `@param` of a name wins.
            hir::NatSpecKind::Param { name } => {
                documentation.params.insert(name.name.to_string(), content.to_string());
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

fn append_doc(target: &mut Option<String>, content: &str) {
    target.get_or_insert_default().push_str(content);
}

fn return_docs(
    gcx: Gcx<'_>,
    doc_id: hir::DocId,
    returns: &[hir::VariableId],
) -> FxIndexMap<String, String> {
    gcx.natspec_doc_comments(doc_id)
        .iter()
        .copied()
        .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
        .enumerate()
        .filter_map(|(index, item)| {
            let variable = gcx.hir.variable(*returns.get(index)?);
            let name = variable.name.map_or_else(|| format!("_{index}"), |name| name.to_string());
            Some((name, solc_content(item.content()).into_owned()))
        })
        .collect()
}

/// Returns the content of a `@return` tag with the name that lowering takes off its description,
/// as solc keeps it for the `return` of a state variable.
fn raw_return_content(item: &hir::NatSpecItem) -> Cow<'_, str> {
    let symbol = item.symbol.as_str();
    if let hir::NatSpecKind::Return { name: Some(name) } = item.kind {
        let name = name.name.as_str();
        let start = item.content_start as usize;
        // Lowering took the name off if blanks separate it from the content. The name is a word of
        // its own, so it is not the end of `@return` itself.
        let before = symbol[..start].trim_end_matches(|c: char| c.is_ascii_whitespace());
        if before.len() < start
            && let Some(name_start) = before.len().checked_sub(name.len())
            && before.ends_with(name)
            && name_start.checked_sub(1).is_none_or(|b| symbol.as_bytes()[b].is_ascii_whitespace())
        {
            return solc_content(&symbol[name_start..item.content_end as usize]);
        }
    }
    solc_content(item.content())
}

/// Returns the content of a tag as solc's NatSpec parser reads it. The content has the lines of
/// the tag. Solc joins them: an empty line vanishes, and a line has a space put before it unless
/// it follows as it is.
fn solc_content(content: &str) -> Cow<'_, str> {
    if !content.contains('\n') {
        return Cow::Borrowed(content);
    }
    let mut lines = content.split('\n');
    let mut joined = lines.next().unwrap_or_default().to_owned();
    for line in lines.filter(|line| !line.is_empty()) {
        if !follows_as_is(line) {
            joined.push(' ');
        }
        joined.push_str(line);
    }
    Cow::Owned(joined)
}

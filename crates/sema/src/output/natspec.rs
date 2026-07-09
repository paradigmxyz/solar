//! NatSpec outputs matching solc's Standard JSON output fields.

use super::DocumentationKind;
use crate::{hir, ty::Gcx};
use serde::Serialize;
use solar_data_structures::map::{FxHashSet, FxIndexMap};

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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state_variables: Option<FxIndexMap<String, DocumentationItem>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub notice: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub custom: Option<FxIndexMap<String, String>>,
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
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<FxIndexMap<String, String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r#return: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub returns: Option<FxIndexMap<String, String>>,
    #[serde(flatten, skip_serializing_if = "Option::is_none")]
    pub custom: Option<FxIndexMap<String, String>>,
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
            state_variables: None,
            notice: None,
            title: None,
            custom: None,
            version: 1,
        }
    }
}

impl DocumentationItem {
    fn is_empty(&self) -> bool {
        self.notice.is_none()
            && self.author.is_none()
            && self.details.is_none()
            && self.params.is_none()
            && self.r#return.is_none()
            && self.returns.is_none()
            && self.custom.is_none()
    }
}

impl<'gcx> Gcx<'gcx> {
    /// Returns the developer documentation for the given contract.
    pub fn dev_documentation(self, contract_id: hir::ContractId) -> Documentation {
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
                .copied()
                .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
                .map(|item| item.content().to_string())
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
                    .get_or_insert_default()
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

    /// Returns the user documentation for the given contract.
    pub fn user_documentation(self, contract_id: hir::ContractId) -> Documentation {
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
        .map(|item| item.content().to_string())
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn dev_doc_item(gcx: Gcx<'_>, doc_id: hir::DocId) -> DocumentationItem {
    let mut documentation = DocumentationItem::default();
    for item in gcx.natspec_doc_comments(doc_id).iter().copied() {
        let content = item.content();
        match item.kind {
            hir::NatSpecKind::Author => append_doc(&mut documentation.author, content),
            hir::NatSpecKind::Dev => append_doc(&mut documentation.details, content),
            hir::NatSpecKind::Param { name } => {
                documentation
                    .params
                    .get_or_insert_default()
                    .entry(name.name.to_string())
                    .or_default()
                    .push_str(content);
            }
            hir::NatSpecKind::Custom { name } => {
                documentation
                    .custom
                    .get_or_insert_default()
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
) -> Option<FxIndexMap<String, String>> {
    let docs = gcx
        .natspec_doc_comments(doc_id)
        .iter()
        .copied()
        .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
        .enumerate()
        .filter_map(|(index, item)| {
            let variable = gcx.hir.variable(*returns.get(index)?);
            let name = variable.name.map_or_else(|| format!("_{index}"), |name| name.to_string());
            (!item.content().is_empty()).then_some((name, item.content().to_string()))
        })
        .collect::<FxIndexMap<_, _>>();
    (!docs.is_empty()).then_some(docs)
}

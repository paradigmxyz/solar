use super::DocumentationKind;
use crate::{hir, ty::Gcx};
use serde::Serialize;
use solar_data_structures::map::{FxHashSet, FxIndexMap};

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DevDocumentation {
    kind: DocumentationKind,
    methods: FxIndexMap<String, DevDocItem>,
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    events: FxIndexMap<String, DevDocItem>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    errors: FxIndexMap<String, Vec<DevDocItem>>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    state_variables: FxIndexMap<String, StateVariableDoc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    title: Option<String>,
    #[serde(flatten)]
    custom: FxIndexMap<String, String>,
    version: u8,
}

#[derive(Debug, Default, Serialize)]
struct DevDocItem {
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    params: FxIndexMap<String, String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    returns: FxIndexMap<String, String>,
    #[serde(flatten)]
    custom: FxIndexMap<String, String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
struct StateVariableDoc {
    #[serde(skip_serializing_if = "Option::is_none")]
    author: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    details: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    params: FxIndexMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    r#return: Option<String>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    returns: FxIndexMap<String, String>,
    #[serde(flatten)]
    custom: FxIndexMap<String, String>,
}

impl<'gcx> Gcx<'gcx> {
    /// Returns the developer documentation for the given contract.
    pub fn dev_documentation(self, contract_id: hir::ContractId) -> DevDocumentation {
        let contract = self.hir.contract(contract_id);
        let contract_doc = dev_doc_item(self, contract.doc);
        let mut documentation = DevDocumentation {
            kind: DocumentationKind::Dev,
            methods: FxIndexMap::default(),
            author: contract_doc.author,
            details: contract_doc.details,
            events: FxIndexMap::default(),
            errors: FxIndexMap::default(),
            state_variables: FxIndexMap::default(),
            title: natspec_text(self, contract.doc, |kind| matches!(kind, hir::NatSpecKind::Title)),
            custom: contract_doc.custom,
            version: 1,
        };

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
            let mut documentation_item =
                StateVariableDoc::from_dev_doc_item(dev_doc_item(self, variable.doc));
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

fn dev_doc_item(gcx: Gcx<'_>, doc_id: hir::DocId) -> DevDocItem {
    let mut documentation = DevDocItem::default();
    for item in gcx.natspec_doc_comments(doc_id).iter().copied() {
        let content = item.content();
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
            (!item.content().is_empty()).then_some((name, item.content().to_string()))
        })
        .collect()
}

impl DevDocItem {
    fn is_empty(&self) -> bool {
        self.author.is_none()
            && self.details.is_none()
            && self.params.is_empty()
            && self.returns.is_empty()
            && self.custom.is_empty()
    }
}

impl StateVariableDoc {
    fn from_dev_doc_item(documentation: DevDocItem) -> Self {
        Self {
            author: documentation.author,
            details: documentation.details,
            params: documentation.params,
            r#return: None,
            returns: documentation.returns,
            custom: documentation.custom,
        }
    }

    fn is_empty(&self) -> bool {
        self.author.is_none()
            && self.details.is_none()
            && self.params.is_empty()
            && self.r#return.is_none()
            && self.returns.is_empty()
            && self.custom.is_empty()
    }
}

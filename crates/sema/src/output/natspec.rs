//! NatSpec outputs matching solc's Standard JSON output fields.

use super::DocumentationKind;
use crate::{hir, ty::Gcx};
use serde::Serialize;
use solar_data_structures::map::FxIndexMap;

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
            documentation_item.r#return = state_variable_return(self, variable_id);
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
        for error_id in self.interface_errors(contract_id).iter() {
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
            let item = function.gettee.map_or(function_id.into(), hir::ItemId::Variable);
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
        for error_id in self.interface_errors(contract_id).iter() {
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

/// Returns the events of a contract's interface by signature, as solc's `uniqueInterfaceEvents`.
///
/// An event defined in the contract or its bases wins over one with the same signature, and used
/// events that share a signature are all dropped.
fn unique_interface_events(
    gcx: Gcx<'_>,
    contract_id: hir::ContractId,
) -> FxIndexMap<String, hir::EventId> {
    let mut events = FxIndexMap::default();
    for item in gcx.hir.contract_item_ids(contract_id) {
        if let hir::ItemId::Event(event_id) = item {
            events.entry(gcx.item_signature(event_id.into()).to_string()).or_insert(event_id);
        }
    }

    let bases = &gcx.hir.contract(contract_id).linearized_bases;
    let mut used = FxIndexMap::<String, Option<hir::EventId>>::default();
    for event_id in gcx.interface_events(contract_id).iter() {
        if gcx.hir.event(event_id).contract.is_some_and(|contract| bases.contains(&contract)) {
            continue;
        }
        used.entry(gcx.item_signature(event_id.into()).to_string())
            .and_modify(|event| *event = None)
            .or_insert(Some(event_id));
    }
    for (signature, event_id) in used {
        if let Some(event_id) = event_id {
            events.entry(signature).or_insert(event_id);
        }
    }
    events
}

fn natspec_text(
    gcx: Gcx<'_>,
    item_id: hir::ItemId,
    mut matches: impl FnMut(hir::NatSpecKind) -> bool,
) -> Option<String> {
    let text = gcx
        .natspec_resolution(item_id)
        .items()
        .iter()
        .copied()
        .filter(|item| matches(item.kind))
        .map(|item| item.content().to_string())
        .collect::<String>();
    (!text.is_empty()).then_some(text)
}

fn dev_doc_item(gcx: Gcx<'_>, item_id: hir::ItemId) -> DocumentationItem {
    let mut documentation = DocumentationItem::default();
    for item in gcx.natspec_resolution(item_id).items().iter().copied() {
        let content = item.content();
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
    item_id: hir::ItemId,
    returns: &[hir::VariableId],
) -> FxIndexMap<String, String> {
    gcx.natspec_resolution(item_id)
        .items()
        .iter()
        .copied()
        .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }))
        .enumerate()
        .filter_map(|(index, item)| {
            let variable = gcx.hir.variable(*returns.get(index)?);
            let mut content = item.content();
            // Like solc, an inherited tag from an unnamed return that starts with the name of this
            // return keeps that name as its own.
            if matches!(item.kind, hir::NatSpecKind::Return { name: None })
                && let Some(name) = variable.name
                && let Some((word, rest)) = content.split_once([' ', '\t'])
                && word == name.name.as_str()
            {
                content = rest;
            }
            let name = variable.name.map_or_else(|| format!("_{index}"), |name| name.to_string());
            Some((name, content.to_string()))
        })
        .collect()
}

/// Returns the `return` of a state variable: the content of its only `@return` tag, headed by the
/// name of the first value the getter returns like solc does, including for an inherited tag.
fn state_variable_return(gcx: Gcx<'_>, variable_id: hir::VariableId) -> Option<String> {
    let mut items = gcx
        .natspec_resolution(variable_id.into())
        .items()
        .iter()
        .filter(|item| matches!(item.kind, hir::NatSpecKind::Return { .. }));
    let item = items.next()?;
    if items.next().is_some() {
        return None;
    }
    let expected = gcx
        .hir
        .variable(variable_id)
        .getter
        .and_then(|getter| gcx.hir.function(getter).returns.first())
        .and_then(|&id| gcx.hir.variable(id).name);
    let documented = match item.kind {
        hir::NatSpecKind::Return { name } => name,
        _ => None,
    };
    let content = match (documented, expected) {
        (Some(documented), Some(expected)) if documented.name == expected.name => {
            raw_return_content(item).to_string()
        }
        (None, Some(expected))
            if item.content().split([' ', '\t']).next() == Some(expected.name.as_str()) =>
        {
            item.content().to_string()
        }
        (_, Some(expected)) => format!("{} {}", expected.name, item.content()),
        (_, None) => item.content().to_string(),
    };
    (!content.is_empty()).then_some(content)
}

/// Returns the content of a `@return` tag with the name that lowering takes off its description,
/// as solc keeps it for the `return` of a state variable.
fn raw_return_content(item: &hir::NatSpecItem) -> &str {
    let symbol = item.symbol.as_str();
    if let hir::NatSpecKind::Return { name: Some(name) } = item.kind {
        let name = name.name.as_str();
        let start = item.content_start as usize;
        // The name is a word of its own, so it is not the end of `@return` itself.
        let name_start = start.checked_sub(1 + name.len());
        if let Some(name_start) = name_start
            && symbol.get(name_start..start - 1) == Some(name)
            && name_start
                .checked_sub(1)
                .is_none_or(|before| symbol.as_bytes()[before].is_ascii_whitespace())
        {
            return &symbol[name_start..item.content_end as usize];
        }
    }
    item.content()
}

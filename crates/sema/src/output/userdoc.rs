use super::DocumentationKind;
use crate::{hir, ty::Gcx};
use serde::Serialize;
use solar_data_structures::map::{FxHashSet, FxIndexMap};

#[derive(Debug, Serialize)]
pub struct UserDocumentation {
    kind: DocumentationKind,
    methods: FxIndexMap<String, UserDocNotice>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    events: FxIndexMap<String, UserDocNotice>,
    #[serde(default, skip_serializing_if = "FxIndexMap::is_empty")]
    errors: FxIndexMap<String, Vec<UserDocNotice>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    notice: Option<String>,
    version: u8,
}

#[derive(Debug, Default, Serialize)]
struct UserDocNotice {
    notice: String,
}

impl<'gcx> Gcx<'gcx> {
    /// Returns the user documentation for the given contract.
    pub fn user_documentation(self, contract_id: hir::ContractId) -> UserDocumentation {
        let contract = self.hir.contract(contract_id);
        let mut documentation = UserDocumentation {
            kind: DocumentationKind::User,
            methods: FxIndexMap::default(),
            events: FxIndexMap::default(),
            errors: FxIndexMap::default(),
            notice: natspec_text(self, contract.doc, |kind| {
                matches!(kind, hir::NatSpecKind::Notice)
            }),
            version: 1,
        };

        if let Some(constructor) = contract.ctor
            && let Some(notice) = natspec_text(self, self.hir.function(constructor).doc, |kind| {
                matches!(kind, hir::NatSpecKind::Notice)
            })
        {
            documentation.methods.insert("constructor".into(), UserDocNotice { notice });
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
                    UserDocNotice { notice },
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
                        documentation.events.insert(signature, UserDocNotice { notice });
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
                            .push(UserDocNotice { notice });
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

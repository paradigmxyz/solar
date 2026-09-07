//! Shared hierarchy storage, converted to owned LSP items only at the response boundary.

use lsp_types::{CallHierarchyItem, Range, SymbolKind, TypeHierarchyItem, Url};
use serde::Deserialize;
use std::{cmp::Ordering, sync::Arc};

const DATA_VERSION: u8 = 1;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct HierarchyKey {
    pub(crate) uri: Arc<Url>,
    pub(crate) selection_range: Range,
}

impl HierarchyKey {
    pub(crate) fn from_data(data: &serde_json::Value) -> Option<Self> {
        let data = HierarchyData::deserialize(data).ok()?;
        (data.version == DATA_VERSION)
            .then(|| Self { uri: Arc::new(data.uri), selection_range: data.selection_range })
    }
}

impl Ord for HierarchyKey {
    fn cmp(&self, other: &Self) -> Ordering {
        let range_key = |range: Range| {
            (range.start.line, range.start.character, range.end.line, range.end.character)
        };
        self.uri
            .as_str()
            .cmp(other.uri.as_str())
            .then_with(|| range_key(self.selection_range).cmp(&range_key(other.selection_range)))
    }
}

impl PartialOrd for HierarchyKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct HierarchyData {
    version: u8,
    uri: Url,
    selection_range: Range,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HierarchyItem {
    pub(crate) key: HierarchyKey,
    pub(crate) name: String,
    pub(crate) kind: SymbolKind,
    pub(crate) detail: Option<String>,
    pub(crate) range: Range,
}

impl HierarchyItem {
    fn data(&self) -> serde_json::Value {
        serde_json::json!({
            "version": DATA_VERSION,
            "uri": self.key.uri.as_str(),
            "selectionRange": self.key.selection_range,
        })
    }

    pub(crate) fn matches_type_item(&self, item: &TypeHierarchyItem) -> bool {
        self.name == item.name
            && self.kind == item.kind
            && item.tags.is_none()
            && self.detail == item.detail
            && *self.key.uri == item.uri
            && self.range == item.range
            && self.key.selection_range == item.selection_range
            && item.data.as_ref().is_some_and(|data| *data == self.data())
    }

    pub(crate) fn to_type_item(&self) -> TypeHierarchyItem {
        TypeHierarchyItem {
            name: self.name.clone(),
            kind: self.kind,
            tags: None,
            detail: self.detail.clone(),
            uri: self.key.uri.as_ref().clone(),
            range: self.range,
            selection_range: self.key.selection_range,
            data: Some(self.data()),
        }
    }

    pub(crate) fn to_call_item(&self) -> CallHierarchyItem {
        CallHierarchyItem {
            name: self.name.clone(),
            kind: self.kind,
            tags: None,
            detail: self.detail.clone(),
            uri: self.key.uri.as_ref().clone(),
            range: self.range,
            selection_range: self.key.selection_range,
            data: Some(self.data()),
        }
    }
}

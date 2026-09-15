//! Shared hierarchy storage, converted to owned LSP items only at the response boundary.

use lsp_types::{CallHierarchyItem, Position, Range, SymbolKind, TypeHierarchyItem, Url};
use serde::Deserialize;
use std::{cmp::Ordering, sync::Arc};

const DATA_VERSION: u8 = 2;

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct HierarchyKey {
    pub(crate) uri: Arc<Url>,
    pub(crate) selection_range: Range,
}

impl HierarchyKey {
    pub(crate) fn from_data(data: &serde_json::Value) -> Option<Self> {
        let (version, uri, start_line, start_column, end_line, end_column) =
            <(u8, Url, u32, u32, u32, u32)>::deserialize(data).ok()?;
        (version == DATA_VERSION).then(|| Self {
            uri: Arc::new(uri),
            selection_range: Range::new(
                Position::new(start_line, start_column),
                Position::new(end_line, end_column),
            ),
        })
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

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct HierarchyItem {
    pub(crate) key: HierarchyKey,
    pub(crate) name: String,
    pub(crate) kind: SymbolKind,
    pub(crate) detail: Option<String>,
    pub(crate) range: Range,
}

impl HierarchyItem {
    /// Encode opaque data as a flat array, avoiding JSON object maps and nested allocations.
    fn data(&self) -> serde_json::Value {
        let range = self.key.selection_range;
        serde_json::json!([
            DATA_VERSION,
            self.key.uri.as_str(),
            range.start.line,
            range.start.character,
            range.end.line,
            range.end.character,
        ])
    }

    /// Compare echoed fields without allocating another serialized key.
    fn matches_data(&self, data: &serde_json::Value) -> bool {
        let Some([version, uri, start_line, start_column, end_line, end_column]) =
            data.as_array().map(Vec::as_slice)
        else {
            return false;
        };
        let range = self.key.selection_range;
        // Check the raw URI spelling and integer representation, preserving exact JSON equality.
        version.as_u64() == Some(u64::from(DATA_VERSION))
            && uri.as_str() == Some(self.key.uri.as_str())
            && start_line.as_u64() == Some(u64::from(range.start.line))
            && start_column.as_u64() == Some(u64::from(range.start.character))
            && end_line.as_u64() == Some(u64::from(range.end.line))
            && end_column.as_u64() == Some(u64::from(range.end.character))
    }

    pub(crate) fn matches_type_item(&self, item: &TypeHierarchyItem) -> bool {
        self.name == item.name
            && self.kind == item.kind
            && item.tags.is_none()
            && self.detail == item.detail
            && *self.key.uri == item.uri
            && self.range == item.range
            && self.key.selection_range == item.selection_range
            && item.data.as_ref().is_some_and(|data| self.matches_data(data))
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

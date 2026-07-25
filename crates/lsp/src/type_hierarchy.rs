//! Type hierarchy indexing.
//!
//! The index retains declaration items and direct hierarchy edges as facts that can be merged
//! across analysis batches. After merging, [`TypeHierarchyIndex::rebuild`] derives the canonical
//! query indexes that are published to request handlers.

use crate::symbols::{DeclarationSymbol, SymbolId};
use lsp_types::{Range, SymbolKind, TypeHierarchyItem, Url};
use serde::{Deserialize, Serialize};
use solar_interface::data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::{
    Gcx,
    hir::{ContractKind, FunctionKind, ItemId},
};
use std::{cmp::Ordering, fmt::Write as _};

const DATA_VERSION: u8 = 1;

#[derive(Clone, Debug, Default)]
pub(crate) struct TypeHierarchyIndex {
    items_by_symbol: FxHashMap<SymbolId, TypeHierarchyItem>,
    direct_edges: Vec<HierarchyEdge>,
    canonical_symbol_by_key: FxHashMap<NodeKey, SymbolId>,
    key_by_symbol: FxHashMap<SymbolId, NodeKey>,
    bases_by_key: FxHashMap<NodeKey, Vec<NodeKey>>,
    children_by_key: FxHashMap<NodeKey, Vec<NodeKey>>,
}

#[derive(Clone, Copy, Debug)]
struct HierarchyEdge {
    derived: SymbolId,
    base: SymbolId,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct NodeKey {
    uri: Url,
    selection_range: Range,
}

impl Ord for NodeKey {
    fn cmp(&self, other: &Self) -> Ordering {
        self.uri
            .as_str()
            .cmp(other.uri.as_str())
            .then_with(|| range_key(self.selection_range).cmp(&range_key(other.selection_range)))
    }
}

impl PartialOrd for NodeKey {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TypeHierarchyData {
    version: u8,
    uri: Url,
    selection_range: Range,
}

impl TypeHierarchyIndex {
    pub(crate) fn build(
        gcx: Gcx<'_>,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) -> Self {
        let mut index = Self::default();

        for item_id in gcx.hir.item_ids() {
            let Some(&symbol_id) = item_symbols.get(&item_id) else { continue };
            let Some((name, kind)) = node_presentation(gcx, item_id) else { continue };
            let declaration = &declarations[symbol_id];
            let uri = declaration.location.uri.clone();
            let selection_range = declaration.name_range;
            let data =
                TypeHierarchyData { version: DATA_VERSION, uri: uri.clone(), selection_range };
            let item = TypeHierarchyItem {
                name,
                kind,
                tags: None,
                detail: None,
                uri,
                range: declaration.location.range,
                selection_range,
                data: Some(
                    serde_json::to_value(data).expect("type hierarchy data is serializable"),
                ),
            };
            index.items_by_symbol.insert(symbol_id, item);

            match item_id {
                ItemId::Contract(id) => {
                    for &base_id in gcx.hir.contract(id).bases {
                        if let Some(&base) = item_symbols.get(&ItemId::Contract(base_id)) {
                            index.direct_edges.push(HierarchyEdge { derived: symbol_id, base });
                        }
                    }
                }
                ItemId::Function(_) | ItemId::Variable(_) => {
                    for &base_item in gcx.base_override_items(item_id) {
                        if let Some(&base) = item_symbols.get(&base_item) {
                            index.direct_edges.push(HierarchyEdge { derived: symbol_id, base });
                        }
                    }
                }
                ItemId::Struct(_)
                | ItemId::Enum(_)
                | ItemId::Udvt(_)
                | ItemId::Error(_)
                | ItemId::Event(_) => {}
            }
        }

        index
    }

    pub(crate) fn extend(&mut self, other: Self, symbol_offset: usize) {
        self.items_by_symbol.extend(
            other
                .items_by_symbol
                .into_iter()
                .map(|(symbol_id, item)| (remap_symbol_id(symbol_id, symbol_offset), item)),
        );
        self.direct_edges.extend(other.direct_edges.into_iter().map(|edge| HierarchyEdge {
            derived: remap_symbol_id(edge.derived, symbol_offset),
            base: remap_symbol_id(edge.base, symbol_offset),
        }));
        self.invalidate_query_indexes();
    }

    pub(crate) fn rebuild(&mut self, conflicting_contents: &FxHashSet<Url>) {
        self.invalidate_query_indexes();

        for (&symbol_id, item) in &self.items_by_symbol {
            if conflicting_contents.contains(&item.uri) {
                continue;
            }
            let key = NodeKey { uri: item.uri.clone(), selection_range: item.selection_range };
            self.key_by_symbol.insert(symbol_id, key.clone());
            match self.canonical_symbol_by_key.get_mut(&key) {
                Some(existing)
                    if canonical_item_order(item, &self.items_by_symbol[existing]).is_lt() =>
                {
                    *existing = symbol_id;
                }
                Some(_) => {}
                None => {
                    self.canonical_symbol_by_key.insert(key, symbol_id);
                }
            }
        }

        for edge in &self.direct_edges {
            let (Some(derived), Some(base)) =
                (self.key_by_symbol.get(&edge.derived), self.key_by_symbol.get(&edge.base))
            else {
                continue;
            };
            if derived == base {
                continue;
            }
            self.bases_by_key.entry(derived.clone()).or_default().push(base.clone());
            self.children_by_key.entry(base.clone()).or_default().push(derived.clone());
        }

        for bases in self.bases_by_key.values_mut() {
            sort_and_dedup_keys(bases);
        }
        for children in self.children_by_key.values_mut() {
            sort_and_dedup_keys(children);
        }
    }

    pub(crate) fn prepare(&self, symbol_ids: &[SymbolId]) -> Option<Vec<TypeHierarchyItem>> {
        let mut keys = symbol_ids
            .iter()
            .filter_map(|symbol_id| self.key_by_symbol.get(symbol_id))
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return None;
        }
        sort_and_dedup_keys(&mut keys);
        Some(
            keys.into_iter()
                .map(|key| self.items_by_symbol[&self.canonical_symbol_by_key[key]].clone())
                .collect(),
        )
    }

    pub(crate) fn supertypes(&self, item: &TypeHierarchyItem) -> Option<Vec<TypeHierarchyItem>> {
        self.neighbors(item, &self.bases_by_key)
    }

    pub(crate) fn subtypes(&self, item: &TypeHierarchyItem) -> Option<Vec<TypeHierarchyItem>> {
        self.neighbors(item, &self.children_by_key)
    }

    fn neighbors(
        &self,
        item: &TypeHierarchyItem,
        adjacency: &FxHashMap<NodeKey, Vec<NodeKey>>,
    ) -> Option<Vec<TypeHierarchyItem>> {
        let key = self.resolve_item(item)?;
        Some(
            adjacency
                .get(&key)
                .into_iter()
                .flatten()
                .map(|neighbor| {
                    self.items_by_symbol[&self.canonical_symbol_by_key[neighbor]].clone()
                })
                .collect(),
        )
    }

    fn resolve_item(&self, item: &TypeHierarchyItem) -> Option<NodeKey> {
        let data = TypeHierarchyData::deserialize(item.data.as_ref()?).ok()?;
        if data.version != DATA_VERSION {
            return None;
        }
        let key = NodeKey { uri: data.uri, selection_range: data.selection_range };
        let symbol_id = self.canonical_symbol_by_key.get(&key)?;
        let canonical_item = self.items_by_symbol.get(symbol_id)?;
        (canonical_item == item).then_some(key)
    }

    fn invalidate_query_indexes(&mut self) {
        self.canonical_symbol_by_key.clear();
        self.key_by_symbol.clear();
        self.bases_by_key.clear();
        self.children_by_key.clear();
    }
}

fn node_presentation(gcx: Gcx<'_>, item_id: ItemId) -> Option<(String, SymbolKind)> {
    Some(match item_id {
        ItemId::Contract(id) => {
            let contract = gcx.hir.contract(id);
            let kind = match contract.kind {
                ContractKind::Contract | ContractKind::AbstractContract => SymbolKind::CLASS,
                ContractKind::Interface => SymbolKind::INTERFACE,
                ContractKind::Library => SymbolKind::MODULE,
            };
            (contract.name.to_string(), kind)
        }
        ItemId::Function(id) => {
            let function = gcx.hir.function(id);
            if function.is_yul || function.is_getter() {
                return None;
            }
            let mut name = String::new();
            if let Some(contract_id) = function.contract {
                name.push_str(gcx.hir.contract(contract_id).name.as_str());
                name.push('.');
            }
            if let Some(function_name) = function.name {
                name.push_str(function_name.as_str());
            } else {
                name.push_str(function.kind.to_str());
            }

            let kind = if function.kind == FunctionKind::Modifier {
                SymbolKind::FUNCTION
            } else {
                name.push('(');
                for (index, &ty) in gcx.item_parameter_types(item_id).iter().enumerate() {
                    if index > 0 {
                        name.push(',');
                    }
                    write!(name, "{}", ty.peel_refs().display(gcx))
                        .expect("writing a type hierarchy name to a string cannot fail");
                }
                name.push(')');
                if function.contract.is_some() { SymbolKind::METHOD } else { SymbolKind::FUNCTION }
            };
            (name, kind)
        }
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            if !variable.is_state_variable()
                || !variable.is_public()
                || !variable.override_
                || variable.getter.is_none()
            {
                return None;
            }
            let contract_id = variable.contract?;
            let name = variable.name?;
            (format!("{}.{}", gcx.hir.contract(contract_id).name, name), SymbolKind::VARIABLE)
        }
        ItemId::Struct(_)
        | ItemId::Enum(_)
        | ItemId::Udvt(_)
        | ItemId::Error(_)
        | ItemId::Event(_) => return None,
    })
}

fn remap_symbol_id(symbol_id: SymbolId, offset: usize) -> SymbolId {
    SymbolId::from_usize(symbol_id.index() + offset)
}

fn sort_and_dedup_keys<T: Ord>(keys: &mut Vec<T>) {
    keys.sort_unstable();
    keys.dedup();
}

fn canonical_item_order(a: &TypeHierarchyItem, b: &TypeHierarchyItem) -> Ordering {
    range_key(a.range)
        .cmp(&range_key(b.range))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| hierarchy_kind_order(a.kind).cmp(&hierarchy_kind_order(b.kind)))
}

fn hierarchy_kind_order(kind: SymbolKind) -> u8 {
    match kind {
        SymbolKind::CLASS => 0,
        SymbolKind::INTERFACE => 1,
        SymbolKind::MODULE => 2,
        SymbolKind::METHOD => 3,
        SymbolKind::FUNCTION => 4,
        _ => 5,
    }
}

fn range_key(range: Range) -> (u32, u32, u32, u32) {
    (range.start.line, range.start.character, range.end.line, range.end.character)
}

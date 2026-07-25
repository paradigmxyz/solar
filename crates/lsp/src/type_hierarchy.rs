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
    raw_nodes: FxHashMap<SymbolId, TypeHierarchyItem>,
    raw_edges: Vec<(SymbolId, SymbolId)>,
    nodes: FxHashMap<NodeKey, TypeHierarchyItem>,
    symbol_keys: FxHashMap<SymbolId, NodeKey>,
    direct_bases: FxHashMap<NodeKey, Vec<NodeKey>>,
    direct_children: FxHashMap<NodeKey, Vec<NodeKey>>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct NodeKey {
    uri: Url,
    selection_range: Range,
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
            let Some((name, kind)) = node_presentation(gcx, item_id) else { continue };
            let Some(&symbol_id) = item_symbols.get(&item_id) else { continue };
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
            index.raw_nodes.insert(symbol_id, item);
        }

        for item_id in gcx.hir.item_ids() {
            if !is_eligible_item(gcx, item_id) {
                continue;
            }
            let Some(&derived) = item_symbols.get(&item_id) else { continue };
            if !index.raw_nodes.contains_key(&derived) {
                continue;
            }

            match item_id {
                ItemId::Contract(id) => {
                    for &base_id in gcx.hir.contract(id).bases {
                        if let Some(&base) = item_symbols.get(&ItemId::Contract(base_id)) {
                            index.raw_edges.push((derived, base));
                        }
                    }
                }
                ItemId::Function(_) | ItemId::Variable(_) => {
                    for &base_item in gcx.base_override_items(item_id) {
                        if let Some(&base) = item_symbols.get(&base_item) {
                            index.raw_edges.push((derived, base));
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
        self.raw_nodes.extend(
            other
                .raw_nodes
                .into_iter()
                .map(|(symbol_id, item)| (remap_symbol_id(symbol_id, symbol_offset), item)),
        );
        self.raw_edges.extend(other.raw_edges.into_iter().map(|(derived, base)| {
            (remap_symbol_id(derived, symbol_offset), remap_symbol_id(base, symbol_offset))
        }));
        self.clear_derived();
    }

    pub(crate) fn rebuild(&mut self, conflicting_contents: &FxHashSet<Url>) {
        self.clear_derived();

        for (&symbol_id, item) in &self.raw_nodes {
            if conflicting_contents.contains(&item.uri) {
                continue;
            }
            let key = NodeKey { uri: item.uri.clone(), selection_range: item.selection_range };
            self.symbol_keys.insert(symbol_id, key.clone());
            match self.nodes.get_mut(&key) {
                Some(existing) if item_order(item, existing).is_lt() => *existing = item.clone(),
                Some(_) => {}
                None => {
                    self.nodes.insert(key, item.clone());
                }
            }
        }

        for &(derived, base) in &self.raw_edges {
            let (Some(derived), Some(base)) =
                (self.symbol_keys.get(&derived), self.symbol_keys.get(&base))
            else {
                continue;
            };
            if derived == base {
                continue;
            }
            self.direct_bases.entry(derived.clone()).or_default().push(base.clone());
            self.direct_children.entry(base.clone()).or_default().push(derived.clone());
        }

        for bases in self.direct_bases.values_mut() {
            sort_and_dedup_keys(&self.nodes, bases);
        }
        for children in self.direct_children.values_mut() {
            sort_and_dedup_keys(&self.nodes, children);
        }
    }

    pub(crate) fn prepare(&self, symbol_ids: &[SymbolId]) -> Option<Vec<TypeHierarchyItem>> {
        let mut keys = symbol_ids
            .iter()
            .filter_map(|symbol_id| self.symbol_keys.get(symbol_id).cloned())
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return None;
        }
        sort_and_dedup_keys(&self.nodes, &mut keys);
        Some(keys.into_iter().map(|key| self.nodes[&key].clone()).collect())
    }

    pub(crate) fn supertypes(&self, item: &TypeHierarchyItem) -> Option<Vec<TypeHierarchyItem>> {
        self.neighbors(item, &self.direct_bases)
    }

    pub(crate) fn subtypes(&self, item: &TypeHierarchyItem) -> Option<Vec<TypeHierarchyItem>> {
        self.neighbors(item, &self.direct_children)
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
                .map(|neighbor| self.nodes[neighbor].clone())
                .collect(),
        )
    }

    fn resolve_item(&self, item: &TypeHierarchyItem) -> Option<NodeKey> {
        let data = serde_json::from_value::<TypeHierarchyData>(item.data.clone()?).ok()?;
        if data.version != DATA_VERSION {
            return None;
        }
        let key = NodeKey { uri: data.uri, selection_range: data.selection_range };
        (self.nodes.get(&key)? == item).then_some(key)
    }

    fn clear_derived(&mut self) {
        self.nodes.clear();
        self.symbol_keys.clear();
        self.direct_bases.clear();
        self.direct_children.clear();
    }
}

fn is_eligible_item(gcx: Gcx<'_>, item_id: ItemId) -> bool {
    match item_id {
        ItemId::Contract(_) => true,
        ItemId::Function(id) => {
            let function = gcx.hir.function(id);
            !function.is_yul && !function.is_getter()
        }
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            variable.is_state_variable()
                && variable.is_public()
                && variable.override_
                && variable.getter.is_some()
        }
        ItemId::Struct(_)
        | ItemId::Enum(_)
        | ItemId::Udvt(_)
        | ItemId::Error(_)
        | ItemId::Event(_) => false,
    }
}

fn node_presentation(gcx: Gcx<'_>, item_id: ItemId) -> Option<(String, SymbolKind)> {
    if !is_eligible_item(gcx, item_id) {
        return None;
    }
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

fn sort_and_dedup_keys(nodes: &FxHashMap<NodeKey, TypeHierarchyItem>, keys: &mut Vec<NodeKey>) {
    keys.sort_by(|a, b| item_order(&nodes[a], &nodes[b]));
    keys.dedup();
}

fn item_order(a: &TypeHierarchyItem, b: &TypeHierarchyItem) -> Ordering {
    a.uri
        .as_str()
        .cmp(b.uri.as_str())
        .then_with(|| range_key(a.selection_range).cmp(&range_key(b.selection_range)))
        .then_with(|| range_key(a.range).cmp(&range_key(b.range)))
        .then_with(|| a.name.cmp(&b.name))
        .then_with(|| hierarchy_kind_order(a.kind).cmp(&hierarchy_kind_order(b.kind)))
}

fn hierarchy_kind_order(kind: SymbolKind) -> u8 {
    if kind == SymbolKind::CLASS {
        0
    } else if kind == SymbolKind::INTERFACE {
        1
    } else if kind == SymbolKind::MODULE {
        2
    } else if kind == SymbolKind::METHOD {
        3
    } else if kind == SymbolKind::FUNCTION {
        4
    } else {
        5
    }
}

fn range_key(range: Range) -> (u32, u32, u32, u32) {
    (range.start.line, range.start.character, range.end.line, range.end.character)
}

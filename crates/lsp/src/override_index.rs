use crate::symbols::{DeclarationSymbol, SymbolId};
use solar_interface::data_structures::{index::IndexVec, map::FxHashMap};

/// The connected override families used by navigation and rename.
///
/// HIR item IDs are only valid for the analysis that produced them, so this index stores the
/// copied LSP symbol IDs and can be remapped when analysis batches are merged.
#[derive(Clone, Debug, Default)]
pub(crate) struct OverrideFamilyIndex {
    edges: Vec<(SymbolId, SymbolId)>,
    families: IndexVec<SymbolId, SymbolId>,
}

impl OverrideFamilyIndex {
    pub(crate) fn add_edge(&mut self, derived: SymbolId, base: SymbolId) {
        self.edges.push((derived, base));
    }

    pub(crate) fn extend(&mut self, other: Self, symbol_offset: usize) {
        self.edges.extend(other.edges.into_iter().map(|(derived, base)| {
            (remap_symbol_id(derived, symbol_offset), remap_symbol_id(base, symbol_offset))
        }));
        self.families.clear();
    }

    pub(crate) fn rebuild(&mut self, declarations: &IndexVec<SymbolId, DeclarationSymbol>) {
        self.families.clear();
        self.families.extend(declarations.indices());
        let mut declarations_by_location = FxHashMap::<_, SymbolId>::default();
        declarations_by_location.reserve(declarations.len());

        // The same source may be analyzed in more than one batch. Merge those copies before
        // applying semantic override edges so navigation remains stable after batch extension.
        for symbol_id in declarations.indices() {
            let declaration = &declarations[symbol_id];
            let key = (
                declaration.name.as_str(),
                declaration.location.uri.as_str(),
                declaration.name_range,
            );
            if let Some(&other) = declarations_by_location.get(&key) {
                union(&mut self.families, symbol_id, other);
            } else {
                declarations_by_location.insert(key, symbol_id);
            }
        }

        for &(derived, base) in &self.edges {
            union(&mut self.families, derived, base);
        }

        for symbol_id in declarations.indices() {
            let family = find(&mut self.families, symbol_id);
            self.families[symbol_id] = family;
        }
    }

    pub(crate) fn same_family(&self, a: SymbolId, b: SymbolId) -> bool {
        self.family(a).is_some_and(|family| self.family(b) == Some(family))
    }

    pub(crate) fn family(&self, symbol_id: SymbolId) -> Option<SymbolId> {
        self.families.get(symbol_id).copied()
    }

    pub(crate) fn members(&self, symbol_ids: Vec<SymbolId>) -> impl Iterator<Item = SymbolId> + '_ {
        let mut families = symbol_ids;
        families.retain_mut(|symbol_id| {
            let Some(family) = self.family(*symbol_id) else { return false };
            *symbol_id = family;
            true
        });
        families.sort_unstable();
        families.dedup();

        let candidate_count = if families.is_empty() { 0 } else { self.families.len() };
        self.families.iter_enumerated().take(candidate_count).filter_map(
            move |(candidate, family)| families.binary_search(family).is_ok().then_some(candidate),
        )
    }
}

fn remap_symbol_id(symbol_id: SymbolId, offset: usize) -> SymbolId {
    SymbolId::from_usize(symbol_id.index() + offset)
}

fn find(parents: &mut IndexVec<SymbolId, SymbolId>, index: SymbolId) -> SymbolId {
    let parent = parents[index];
    if parent == index {
        index
    } else {
        let root = find(parents, parent);
        parents[index] = root;
        root
    }
}

fn union(parents: &mut IndexVec<SymbolId, SymbolId>, a: SymbolId, b: SymbolId) {
    let a = find(parents, a);
    let b = find(parents, b);
    parents[a] = b;
}

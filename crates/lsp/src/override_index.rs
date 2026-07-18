use crate::symbols::{DeclarationSymbol, SymbolId};
use solar_interface::data_structures::{index::IndexVec, map::FxHashMap};

/// The connected override families used by navigation and rename.
///
/// HIR item IDs are only valid for the analysis that produced them, so this index stores the
/// copied LSP symbol IDs and can be remapped when analysis batches are merged.
#[derive(Clone, Debug, Default)]
pub(crate) struct OverrideFamilyIndex {
    edges: Vec<(SymbolId, SymbolId)>,
    families: FxHashMap<SymbolId, usize>,
}

impl OverrideFamilyIndex {
    pub(crate) fn add_edge(&mut self, derived: SymbolId, base: SymbolId) {
        self.edges.push((derived, base));
    }

    pub(crate) fn extend(&mut self, mut other: Self, symbol_offset: usize) {
        self.edges.extend(other.edges.drain(..).map(|(derived, base)| {
            (remap_symbol_id(derived, symbol_offset), remap_symbol_id(base, symbol_offset))
        }));
        self.families.clear();
    }

    pub(crate) fn rebuild(&mut self, declarations: &IndexVec<SymbolId, DeclarationSymbol>) {
        self.families.clear();
        let mut parents = (0..declarations.len()).collect::<Vec<_>>();
        let mut declarations_by_location = FxHashMap::<_, SymbolId>::default();

        // The same source may be analyzed in more than one batch. Merge those copies before
        // applying semantic override edges so navigation remains stable after batch extension.
        for symbol_id in declarations.indices() {
            let declaration = &declarations[symbol_id];
            let range = declaration.name_range;
            let key = (
                declaration.name.as_str(),
                declaration.location.uri.as_str(),
                range.start.line,
                range.start.character,
                range.end.line,
                range.end.character,
            );
            if let Some(&other) = declarations_by_location.get(&key) {
                union(&mut parents, symbol_id.index(), other.index());
            } else {
                declarations_by_location.insert(key, symbol_id);
            }
        }

        for &(derived, base) in &self.edges {
            union(&mut parents, derived.index(), base.index());
        }

        for symbol_id in declarations.indices() {
            self.families.insert(symbol_id, find(&mut parents, symbol_id.index()));
        }
    }

    pub(crate) fn same_family(&self, a: SymbolId, b: SymbolId) -> bool {
        match (self.families.get(&a), self.families.get(&b)) {
            (Some(a), Some(b)) => a == b,
            _ => false,
        }
    }

    pub(crate) fn family(&self, symbol_id: SymbolId) -> Option<usize> {
        self.families.get(&symbol_id).copied()
    }

    pub(crate) fn members(
        &self,
        symbol_id: SymbolId,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) -> Vec<SymbolId> {
        let Some(&family) = self.families.get(&symbol_id) else { return Vec::new() };
        declarations
            .indices()
            .filter(|candidate| self.families.get(candidate) == Some(&family))
            .collect()
    }
}

fn remap_symbol_id(symbol_id: SymbolId, offset: usize) -> SymbolId {
    SymbolId::from_usize(symbol_id.index() + offset)
}

fn find(parents: &mut [usize], index: usize) -> usize {
    let parent = parents[index];
    if parent == index {
        index
    } else {
        let root = find(parents, parent);
        parents[index] = root;
        root
    }
}

fn union(parents: &mut [usize], a: usize, b: usize) {
    let a = find(parents, a);
    let b = find(parents, b);
    parents[a] = b;
}

use crate::symbols::{DeclarationSymbol, SymbolId};
use lsp_types::Url;
use solar_interface::data_structures::{
    bit_set::{DenseBitSet, GrowableBitSet},
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};

/// Override relationships used by directional navigation and family-wide rename.
///
/// HIR item IDs are only valid for the analysis that produced them, so this index stores the
/// copied LSP symbol IDs and can be remapped when analysis batches are merged.
#[derive(Clone, Debug, Default)]
pub(crate) struct OverrideFamilyIndex {
    edges: Vec<(SymbolId, SymbolId)>,
    overridable: Vec<SymbolId>,
    families: IndexVec<SymbolId, SymbolId>,
    canonical: IndexVec<SymbolId, SymbolId>,
    derived: IndexVec<SymbolId, Vec<SymbolId>>,
    override_symbols: GrowableBitSet<SymbolId>,
}

impl OverrideFamilyIndex {
    pub(crate) fn add_edge(&mut self, derived: SymbolId, base: SymbolId) {
        self.edges.push((derived, base));
    }

    pub(crate) fn add_overridable(&mut self, symbol_id: SymbolId) {
        self.overridable.push(symbol_id);
    }

    pub(crate) fn extend(&mut self, other: Self, symbol_offset: usize) {
        self.edges.extend(other.edges.into_iter().map(|(derived, base)| {
            (remap_symbol_id(derived, symbol_offset), remap_symbol_id(base, symbol_offset))
        }));
        self.overridable.extend(
            other
                .overridable
                .into_iter()
                .map(|symbol_id| remap_symbol_id(symbol_id, symbol_offset)),
        );
        self.families.clear();
        self.canonical.clear();
        self.derived.clear();
        self.override_symbols.clear();
    }

    pub(crate) fn rebuild(
        &mut self,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
    ) {
        self.families.clear();
        self.families.extend(declarations.indices());
        self.canonical.clear();
        self.canonical.extend(declarations.indices());
        self.derived.clear();
        self.derived.extend((0..declarations.len()).map(|_| Vec::new()));
        self.override_symbols.clear();
        let mut declarations_by_location = FxHashMap::<_, SymbolId>::default();
        declarations_by_location.reserve(declarations.len());

        // The same source may be analyzed in more than one batch. Merge identical snapshots before
        // applying semantic override edges, but keep conflicting snapshots isolated.
        for symbol_id in declarations.indices() {
            let declaration = &declarations[symbol_id];
            if conflicting_contents.contains(&declaration.location.uri) {
                continue;
            }
            let key = (
                declaration.name.as_str(),
                declaration.location.uri.as_str(),
                declaration.name_range,
            );
            if let Some(&other) = declarations_by_location.get(&key) {
                union(&mut self.families, symbol_id, other);
                self.canonical[symbol_id] = other;
            } else {
                declarations_by_location.insert(key, symbol_id);
            }
        }

        for &symbol_id in &self.overridable {
            self.override_symbols.insert(self.canonical[symbol_id]);
        }
        for &(derived, base) in &self.edges {
            union(&mut self.families, derived, base);
            let derived = self.canonical[derived];
            let base = self.canonical[base];
            self.derived[base].push(derived);
            self.override_symbols.insert(derived);
            self.override_symbols.insert(base);
        }

        for derived in &mut self.derived {
            derived.sort_unstable();
            derived.dedup();
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

    pub(crate) fn is_override_symbol(&self, symbol_id: SymbolId) -> bool {
        self.canonical
            .get(symbol_id)
            .is_some_and(|&symbol_id| self.override_symbols.contains(symbol_id))
    }

    pub(crate) fn descendants(&self, symbol_id: SymbolId) -> Vec<SymbolId> {
        let mut seen = DenseBitSet::new_empty(self.canonical.len());
        let mut descendants = DenseBitSet::new_empty(self.canonical.len());
        let Some(&symbol_id) = self.canonical.get(symbol_id) else { return Vec::new() };
        seen.insert(symbol_id);
        let mut pending = self.derived[symbol_id].clone();

        while let Some(symbol_id) = pending.pop() {
            if seen.insert(symbol_id) {
                descendants.insert(symbol_id);
                pending.extend(self.derived[symbol_id].iter().copied());
            }
        }
        descendants.iter().collect()
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

//! CodeLens declaration facts and their cross-analysis merge index.
//!
//! The semantic analysis context is short-lived, so this module copies only the facts needed to
//! render CodeLens items. Query-time reference locations remain owned by [`SymbolTables`].

use crate::symbols::{DeclarationSymbol, SymbolId};
use lsp_types::{Range, Url};
use solar_interface::data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
};
use solar_sema::{
    Gcx,
    hir::{ItemId, VarKind},
};
use std::cmp::Ordering;

#[derive(Clone, Debug, Default)]
pub(crate) struct CodeLensIndex {
    candidates: Vec<CodeLensCandidate>,
    entries_by_uri: FxHashMap<Url, Vec<CodeLensEntry>>,
}

#[derive(Clone, Debug)]
struct CodeLensCandidate {
    symbol_id: SymbolId,
    key: NodeKey,
    selector: Option<[u8; 4]>,
    inheritance: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeLensEntry {
    pub(crate) range: Range,
    pub(crate) symbol_ids: Vec<SymbolId>,
    pub(crate) selector: Option<[u8; 4]>,
    pub(crate) inheritance: bool,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct NodeKey {
    uri: Url,
    range: Range,
}

impl CodeLensIndex {
    pub(crate) fn build(
        gcx: Gcx<'_>,
        item_symbols: &FxHashMap<ItemId, SymbolId>,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
    ) -> Self {
        let mut index = Self::default();
        for item_id in gcx.hir.item_ids() {
            let Some(&symbol_id) = item_symbols.get(&item_id) else { continue };
            if !has_reference_lens(gcx, item_id) {
                continue;
            }

            let declaration = &declarations[symbol_id];
            index.candidates.push(CodeLensCandidate {
                symbol_id,
                key: NodeKey {
                    uri: declaration.location.uri.clone(),
                    range: declaration.name_range,
                },
                selector: selector(gcx, item_id),
                inheritance: matches!(item_id, ItemId::Contract(_)),
            });
        }
        index
    }

    pub(crate) fn extend(&mut self, other: Self, symbol_offset: usize) {
        self.candidates.extend(other.candidates.into_iter().map(|mut candidate| {
            candidate.symbol_id = candidate.symbol_id.offset_by(symbol_offset);
            candidate
        }));
        self.entries_by_uri.clear();
    }

    pub(crate) fn rebuild(&mut self, conflicting_contents: &FxHashSet<Url>) {
        self.entries_by_uri.clear();

        let mut grouped = FxHashMap::<NodeKey, Vec<&CodeLensCandidate>>::default();
        for candidate in &self.candidates {
            if !conflicting_contents.contains(&candidate.key.uri) {
                grouped.entry(candidate.key.clone()).or_default().push(candidate);
            }
        }

        for (key, candidates) in grouped {
            let Some(first) = candidates.first() else { continue };
            if candidates.iter().any(|candidate| {
                candidate.selector != first.selector || candidate.inheritance != first.inheritance
            }) {
                // A source declaration compiled under incompatible contexts must not expose a
                // potentially stale or contradictory lens.
                continue;
            }

            let mut symbol_ids: Vec<_> =
                candidates.iter().map(|candidate| candidate.symbol_id).collect();
            symbol_ids.sort_unstable_by_key(|symbol_id| symbol_id.index());
            symbol_ids.dedup();
            self.entries_by_uri.entry(key.uri.clone()).or_default().push(CodeLensEntry {
                range: key.range,
                symbol_ids,
                selector: first.selector,
                inheritance: first.inheritance,
            });
        }

        for entries in self.entries_by_uri.values_mut() {
            entries.sort_unstable_by(|lhs, rhs| range_cmp(lhs.range, rhs.range));
        }
    }

    pub(crate) fn entries(&self, uri: &Url) -> &[CodeLensEntry] {
        self.entries_by_uri.get(uri).map_or(&[], Vec::as_slice)
    }
}

fn has_reference_lens(gcx: Gcx<'_>, item_id: ItemId) -> bool {
    match item_id {
        ItemId::Function(id) => !gcx.hir.function(id).is_yul,
        ItemId::Contract(_)
        | ItemId::Struct(_)
        | ItemId::Enum(_)
        | ItemId::Udvt(_)
        | ItemId::Error(_)
        | ItemId::Event(_) => true,
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            let yul_parameter = matches!(
                variable.parent,
                Some(ItemId::Function(function)) if gcx.hir.function(function).is_yul
            );
            !yul_parameter
                && matches!(
                    variable.kind,
                    VarKind::Global
                        | VarKind::State
                        | VarKind::Struct
                        | VarKind::Enum
                        | VarKind::FunctionParam
                )
        }
    }
}

fn selector(gcx: Gcx<'_>, item_id: ItemId) -> Option<[u8; 4]> {
    match item_id {
        ItemId::Function(id) => {
            let function = gcx.hir.function(id);
            (function.is_part_of_external_interface()
                && !function.is_getter()
                && !function.is_yul
                && selector_is_valid(gcx, id))
            .then(|| gcx.function_selector(id).0)
        }
        ItemId::Variable(id) => {
            let variable = gcx.hir.variable(id);
            variable
                .is_state_variable()
                .then_some(variable.getter)
                .flatten()
                .filter(|&getter| selector_is_valid(gcx, getter))
                .map(|getter| gcx.function_selector(getter).0)
        }
        ItemId::Contract(_)
        | ItemId::Struct(_)
        | ItemId::Enum(_)
        | ItemId::Udvt(_)
        | ItemId::Error(_)
        | ItemId::Event(_) => None,
    }
}

fn selector_is_valid(gcx: Gcx<'_>, id: solar_sema::hir::FunctionId) -> bool {
    gcx.item_parameter_types(id).iter().copied().all(|ty| ty.can_be_exported(gcx))
}

fn range_cmp(lhs: Range, rhs: Range) -> Ordering {
    (lhs.start.line, lhs.start.character, lhs.end.line, lhs.end.character).cmp(&(
        rhs.start.line,
        rhs.start.character,
        rhs.end.line,
        rhs.end.character,
    ))
}

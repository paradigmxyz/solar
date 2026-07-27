//! CodeLens declaration facts and their cross-analysis merge index.
//!
//! The semantic analysis context is short-lived, so this module copies only the facts needed to
//! render CodeLens items. Query-time reference locations remain owned by
//! [`SymbolTables`](crate::symbols::SymbolTables).

use crate::symbols::{DeclarationSymbol, SymbolId};
use lsp_types::{Range, Url};
use solar_interface::data_structures::{
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
    smallvec::SmallVec,
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
    selector: Option<[u8; 4]>,
    inheritance: bool,
}

#[derive(Clone, Debug)]
pub(crate) struct CodeLensEntry {
    pub(crate) range: Range,
    pub(crate) symbol_ids: SmallVec<[SymbolId; 1]>,
    pub(crate) selector: Option<[u8; 4]>,
    pub(crate) inheritance: bool,
}

#[derive(Debug)]
struct CodeLensGroup {
    symbol_ids: SmallVec<[SymbolId; 1]>,
    selector: Option<[u8; 4]>,
    inheritance: bool,
    compatible: bool,
}

impl CodeLensIndex {
    pub(crate) fn build(gcx: Gcx<'_>, item_symbols: &FxHashMap<ItemId, SymbolId>) -> Self {
        let mut index = Self::default();
        for item_id in gcx.hir.item_ids() {
            let Some(&symbol_id) = item_symbols.get(&item_id) else { continue };
            if !has_reference_lens(gcx, item_id) {
                continue;
            }
            index.candidates.push(CodeLensCandidate {
                symbol_id,
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

    pub(crate) fn rebuild(
        &mut self,
        declarations: &IndexVec<SymbolId, DeclarationSymbol>,
        conflicting_contents: &FxHashSet<Url>,
    ) {
        self.entries_by_uri.clear();

        let mut grouped = FxHashMap::<&Url, FxHashMap<Range, CodeLensGroup>>::default();
        for candidate in &self.candidates {
            let declaration = &declarations[candidate.symbol_id];
            if conflicting_contents.contains(&declaration.location.uri) {
                continue;
            }

            let group = grouped
                .entry(&declaration.location.uri)
                .or_default()
                .entry(declaration.name_range)
                .or_insert_with(|| CodeLensGroup {
                    symbol_ids: SmallVec::new(),
                    selector: candidate.selector,
                    inheritance: candidate.inheritance,
                    compatible: true,
                });
            if !group.compatible {
                continue;
            }
            if candidate.selector != group.selector || candidate.inheritance != group.inheritance {
                // A source declaration compiled under incompatible contexts must not expose a
                // potentially stale or contradictory lens.
                group.compatible = false;
                group.symbol_ids.clear();
            } else {
                group.symbol_ids.push(candidate.symbol_id);
            }
        }

        for (uri, groups) in grouped {
            let mut entries = Vec::with_capacity(groups.len());
            for (range, mut group) in groups {
                if group.compatible {
                    group.symbol_ids.sort_unstable_by_key(|symbol_id| symbol_id.index());
                    group.symbol_ids.dedup();
                    entries.push(CodeLensEntry {
                        range,
                        symbol_ids: group.symbol_ids,
                        selector: group.selector,
                        inheritance: group.inheritance,
                    });
                }
            }
            if !entries.is_empty() {
                entries.sort_unstable_by(|lhs, rhs| range_cmp(lhs.range, rhs.range));
                self.entries_by_uri.insert(uri.clone(), entries);
            }
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

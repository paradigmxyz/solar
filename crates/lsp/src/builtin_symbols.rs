//! Retains typed builtin occurrences after the compiler's analysis arena is released.
//!
//! Builtins have no source declarations, so their hover documentation lives outside the user
//! declaration index and cannot become a definition, rename, or workspace-symbol target.
//! Merged analysis contexts must agree on the builtin and its displayed signature. User-defined
//! types may come from different declarations, but their source files must have consistent
//! contents across all matching occurrences.

use crate::proto::{self, LocationConverter, PositionIndex};
use lsp_types::{Location, Position, Url};
use solar_interface::{
    Span,
    data_structures::{
        Never,
        map::{FxHashMap, FxHashSet},
    },
};
use solar_sema::{
    Gcx,
    hir::{self, ItemId},
    ty::{Ty, TyKind},
};
use std::{ops::ControlFlow, sync::Arc};

mod documentation;
pub(crate) use documentation::BuiltinDocumentation;

#[derive(Clone, Debug, Default)]
pub(crate) struct BuiltinIndex {
    occurrences: Vec<BuiltinOccurrence>,
    files: FxHashMap<Url, PositionIndex<usize>>,
}

#[derive(Clone, Debug)]
pub(crate) struct BuiltinOccurrence {
    pub(crate) location: Location,
    pub(crate) documentation: Arc<BuiltinDocumentation>,
    type_sources: Vec<Url>,
}

impl BuiltinIndex {
    pub(crate) fn record<'gcx>(
        &mut self,
        gcx: Gcx<'gcx>,
        locations: &LocationConverter,
        expr: &hir::Expr<'gcx>,
        span: Span,
    ) {
        let Some(builtin) = gcx.resolved_builtin(expr) else { return };
        let Some(location) = locations.location(span) else { return };
        let Some(documentation) = BuiltinDocumentation::for_expr(gcx, expr, builtin) else {
            return;
        };
        let documentation = Arc::new(documentation);
        let mut type_sources = Vec::new();
        if let Some(ty) = gcx.type_of_expr(expr.id) {
            collect_type_sources(gcx, locations, ty, &mut type_sources);
        }
        // Properties such as `type(C).name` have an elementary result but a contextual receiver.
        if let hir::ExprKind::Member(receiver, _) = expr.kind
            && let Some(ty) = gcx.type_of_expr(receiver.id)
        {
            collect_type_sources(gcx, locations, ty, &mut type_sources);
        }
        type_sources.sort_unstable();
        type_sources.dedup();
        self.occurrences.push(BuiltinOccurrence { location, documentation, type_sources });
    }

    pub(crate) fn extend(&mut self, other: Self) {
        self.occurrences.extend(other.occurrences);
    }

    pub(crate) fn rebuild(&mut self) {
        self.files.clear();
        for (index, occurrence) in self.occurrences.iter().enumerate() {
            self.files.entry(occurrence.location.uri.clone()).or_default().entries.push(index);
        }
        for positions in self.files.values_mut() {
            positions.rebuild(|index| self.occurrences[index].location.range);
        }
    }

    pub(crate) fn contains(&self, uri: &Url, position: Position) -> bool {
        self.candidates(uri, position).next().is_some()
    }

    pub(crate) fn at_position(
        &self,
        uri: &Url,
        position: Position,
        conflicting_contents: &FxHashSet<Url>,
    ) -> Option<&BuiltinOccurrence> {
        if conflicting_contents.contains(uri) {
            return None;
        }
        let occurrence = self
            .candidates(uri, position)
            .min_by_key(|entry| proto::range_size_key(entry.location.range))?;
        // Equal signatures can depend on different files, so check every matching occurrence.
        if self.candidates(uri, position).any(|other| {
            other.location.range == occurrence.location.range
                && (other.documentation != occurrence.documentation
                    || other
                        .type_sources
                        .iter()
                        .any(|source| conflicting_contents.contains(source)))
        }) {
            return None;
        }
        Some(occurrence)
    }

    fn candidates<'a>(
        &'a self,
        uri: &Url,
        position: Position,
    ) -> impl Iterator<Item = &'a BuiltinOccurrence> {
        self.files
            .get(uri)
            .into_iter()
            .flat_map(move |positions| {
                positions.candidates_at(position, |index| self.occurrences[index].location.range)
            })
            .map(|index| &self.occurrences[index])
    }
}

fn collect_type_sources(
    gcx: Gcx<'_>,
    locations: &LocationConverter,
    ty: Ty<'_>,
    sources: &mut Vec<Url>,
) {
    let _: ControlFlow<Never> = ty.visit(&mut |ty| {
        let item = match ty.kind {
            TyKind::Contract(id) | TyKind::Super(id) => Some(ItemId::Contract(id)),
            TyKind::Struct(id) => Some(ItemId::Struct(id)),
            TyKind::Enum(id) => Some(ItemId::Enum(id)),
            TyKind::Udvt(_, id) => Some(ItemId::Udvt(id)),
            TyKind::Event(_, id) => Some(ItemId::Event(id)),
            TyKind::Error(_, id) => Some(ItemId::Error(id)),
            TyKind::Fn(function) => {
                for &ty in function.parameters.iter().chain(function.returns) {
                    collect_type_sources(gcx, locations, ty, sources);
                }
                None
            }
            _ => None,
        };
        if let Some(item) = item
            && let Some(uri) = locations.file_uri(&gcx.hir.source(gcx.hir.item(item).source()).file)
        {
            sources.push(uri.clone());
        }
        ControlFlow::Continue(())
    });
}

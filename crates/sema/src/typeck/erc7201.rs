//! ERC-7201 namespaces: structs documented with `@custom:storage-location erc7201:<id>`.
//!
//! A namespaced struct lives at the slot ERC-7201 derives from its id, and code reaches it through
//! an accessor that points a storage reference there in inline assembly, `$.slot := LOCATION`:
//! the one part of namespaced storage the language cannot type. This compiler checks that part:
//! an assembly assignment of a constant to the `.slot` of a reference to a namespaced struct must
//! assign the namespace's location, and no contract may see two structs in one namespace, which
//! would overlap. Other compilers read the annotation as documentation.
//!
//! An assignment whose value is not a compile-time constant is not checked.

use crate::{
    eval::erc7201_slot,
    hir::{self, ExprKind, ItemId, Visit},
    ty::Gcx,
};
use alloy_primitives::U256;
use solar_ast::{DataLocation, NatSpecKind};
use solar_data_structures::{
    Never,
    map::{FxHashMap, FxHashSet},
};
use solar_interface::{Span, Symbol, sym};
use std::ops::ControlFlow;

/// The ERC-7201 namespace of a struct: its id and the span of the tag declaring it.
#[derive(Clone, Copy)]
struct Namespace {
    id: Symbol,
    tag: Span,
}

pub(super) fn check(gcx: Gcx<'_>) {
    let namespaces = gcx
        .hir
        .strukt_ids()
        .filter_map(|id| namespace(gcx, id).map(|namespace| (id, namespace)))
        .collect::<FxHashMap<_, _>>();
    if namespaces.is_empty() {
        return;
    }
    check_overlaps(gcx, &namespaces);
    let mut accessors = Accessors { gcx, namespaces: &namespaces };
    for id in gcx.hir.function_ids() {
        let _ = accessors.visit_nested_function(id);
    }
}

/// The namespace `@custom:storage-location erc7201:<id>` on the struct `id` declares.
fn namespace(gcx: Gcx<'_>, id: hir::StructId) -> Option<Namespace> {
    let doc = gcx.hir.doc(gcx.hir.strukt(id).doc);
    doc.ast_comments.iter().flat_map(|comment| comment.natspec.iter()).find_map(|natspec| {
        let NatSpecKind::Custom { name } = natspec.kind else { return None };
        if name.name != sym::storage_dash_location {
            return None;
        }
        let id = natspec.content().trim().strip_prefix("erc7201:")?.trim();
        (!id.is_empty()).then(|| Namespace { id: Symbol::intern(id), tag: natspec.span })
    })
}

/// Rejects two structs in one namespace that a contract declares or inherits.
fn check_overlaps(gcx: Gcx<'_>, namespaces: &FxHashMap<hir::StructId, Namespace>) {
    let mut reported = FxHashSet::default();
    for contract in gcx.hir.contracts() {
        let mut seen = FxHashMap::<Symbol, hir::StructId>::default();
        for &base in contract.linearized_bases.iter().rev() {
            let items = gcx.hir.contract(base).items.iter().filter_map(ItemId::as_struct);
            for id in items {
                let Some(namespace) = namespaces.get(&id) else { continue };
                let first = *seen.entry(namespace.id).or_insert(id);
                if first == id || !reported.insert(id) {
                    continue;
                }
                gcx.dcx()
                    .err(format!("ERC-7201 namespace `{}` is declared twice", namespace.id))
                    .span(namespace.tag)
                    .span_note(
                        namespaces[&first].tag,
                        format!("`{}` declares it here", gcx.hir.strukt(first).name),
                    )
                    .note("both structs would live at the same storage location")
                    .emit();
            }
        }
    }
}

/// Checks the assembly assignments to the `.slot` of namespaced storage references.
struct Accessors<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    namespaces: &'a FxHashMap<hir::StructId, Namespace>,
}

impl<'gcx> Accessors<'gcx, '_> {
    /// The namespace of the struct `expr` refers to in storage, when it has one.
    fn namespace_of(&self, expr: &hir::Expr<'_>) -> Option<(hir::StructId, Namespace)> {
        let ty = match self.gcx.type_of_expr(expr.id) {
            Some(ty) => ty,
            None => self.gcx.type_of_item(self.gcx.resolved_variable(expr)?.into()),
        };
        if !ty.is_ref_at(DataLocation::Storage) {
            return None;
        }
        let crate::ty::TyKind::Struct(id) = ty.peel_refs().kind else { return None };
        Some((id, *self.namespaces.get(&id)?))
    }
}

impl<'gcx> Visit<'gcx> for Accessors<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        // $.slot := LOCATION
        if let ExprKind::Assign(lhs, None, rhs) = expr.kind
            && let ExprKind::YulMember(base, member) = lhs.peel_parens().kind
            && member.name == sym::slot
            && let Some((id, namespace)) = self.namespace_of(base)
            && let Ok(value) = self.gcx.try_eval_const(rhs)
        {
            let location = erc7201_slot(namespace.id.as_str().as_bytes());
            if value.as_u256() != Some(U256::from_be_bytes(location.0)) {
                self.gcx
                    .dcx()
                    .err(format!(
                        "this is not the storage location of ERC-7201 namespace `{}`",
                        namespace.id
                    ))
                    .span(rhs.span)
                    .span_note(
                        namespace.tag,
                        format!(
                            "`{}` is declared in the namespace here",
                            self.gcx.hir.strukt(id).name
                        ),
                    )
                    .help(format!("the namespace's location is `{location}`"))
                    .emit();
            }
        }
        self.walk_expr(expr)
    }
}

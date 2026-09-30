//! Storage layout tags: `@custom:solar-fuse`.
//!
//! A layout tag changes where the values of a state variable live in storage, never what the
//! program computes. Every Solidity-level read and write behaves as without the tag; what differs
//! is the raw layout: the slots that inline assembly, `eth_getStorageAt` or a raw-slot reader see,
//! and the storage credits a chain such as Tempo grants for clearing slots. Other compilers read
//! the tags as documentation and use the standard layout, so a contract built both ways must not
//! share storage, as an upgrade would.
//!
//! `@custom:solar-fuse <group>` on mapping state variables of one contract that share a key type
//! keeps the values of the whole group for one key in one record, laid out like the fields of a
//! struct in declaration order, at `keccak256(key . slot)` with the slot of the group's first
//! mapping. Three mappings of `address` to `uint128`, `uint64` and `uint64` then fill one word per
//! address instead of three, and code that reads or writes them for one key shares the hash and
//! the word. Each mapping keeps its declared slot, so the layout of the contract's state
//! variables is the standard one; only the values move.
//!
//! The standard layout stays observable through a mapping's slot, so a fused mapping may only be
//! indexed: it cannot be bound to a storage reference, passed to a function or returned, and
//! inline assembly cannot take its `.slot` or `.offset`. Its values must be value types, so a
//! record never holds another mapping or an array whose data would hash from the record.

use crate::{
    hir::{self, ExprKind, Visit},
    ty::{Gcx, Ty, TyKind},
};
use solar_data_structures::{
    Never,
    map::{FxHashSet, FxIndexMap},
};
use solar_interface::{Span, Symbol, kw};
use std::ops::ControlFlow;

pub(super) fn check(gcx: Gcx<'_>) {
    let fused = check_fused_groups(gcx);
    if fused.is_empty() {
        return;
    }
    let mut uses = Uses { gcx, fused: &fused };
    for id in gcx.hir.function_ids() {
        let _ = uses.visit_nested_function(id);
    }
}

/// Checks every `@custom:solar-fuse` group and returns the mappings the tags fuse.
fn check_fused_groups(gcx: Gcx<'_>) -> FxHashSet<hir::VariableId> {
    let mut fused = FxHashSet::default();
    for contract in gcx.hir.contracts() {
        let mut groups = FxIndexMap::<Symbol, Vec<(hir::VariableId, Span)>>::default();
        for id in contract.variables() {
            // A tag on anything but a mapping state variable is reported as misplaced.
            let Some((group, tag)) = gcx.hir.solar_fuse(id) else { continue };
            if mapping_types(gcx, id).is_none() {
                continue;
            }
            fused.insert(id);
            if group == kw::Empty {
                gcx.dcx()
                    .err("`@custom:solar-fuse` must name the group of its mapping")
                    .span(tag)
                    .help("name the group on every mapping of it: `@custom:solar-fuse account`")
                    .emit();
                continue;
            }
            groups.entry(group).or_default().push((id, tag));
        }
        for (group, members) in &groups {
            check_group(gcx, *group, members);
        }
    }
    fused
}

/// Checks the mappings `members` of the fused group `group`, each with its tag's span.
fn check_group(gcx: Gcx<'_>, group: Symbol, members: &[(hir::VariableId, Span)]) {
    let &[(first, first_tag), ..] = members else { return };
    if members.len() == 1 {
        gcx.dcx()
            .err(format!("fused group `{group}` has only one mapping"))
            .span(first_tag)
            .note("a group keeps the values that several mappings hold for one key in one record")
            .help("tag the other mappings of the group, or remove the tag")
            .emit();
        return;
    }
    let first_key = mapping_types(gcx, first).map(|(key, _)| key);
    for &(id, tag) in members {
        let Some((key, value)) = mapping_types(gcx, id) else { continue };
        if Some(key) != first_key {
            gcx.dcx()
                .err(format!("the mappings of fused group `{group}` must have the same key type"))
                .span(tag)
                .span_note(
                    gcx.hir.variable(first).span,
                    format!(
                        "the group's first mapping takes `{}` keys",
                        first_key.unwrap_or(key).display(gcx)
                    ),
                )
                .emit();
        }
        if !value.is_value_type() {
            gcx.dcx()
                .err("a fused mapping must map to a value type")
                .span(tag)
                .span_note(
                    gcx.hir.variable(id).span,
                    format!("it maps to `{}`", value.display(gcx)),
                )
                .note(
                    "a record holds the values of the group's mappings side by side, like a \
                     struct's fields",
                )
                .emit();
        }
    }
}

/// The key and value types of the mapping state variable `id`.
fn mapping_types<'gcx>(gcx: Gcx<'gcx>, id: hir::VariableId) -> Option<(Ty<'gcx>, Ty<'gcx>)> {
    let TyKind::Mapping(key, value) = gcx.type_of_item(id.into()).peel_refs().kind else {
        return None;
    };
    Some((key, value))
}

/// Rejects the uses of fused mappings other than indexing them.
struct Uses<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    fused: &'a FxHashSet<hir::VariableId>,
}

impl Uses<'_, '_> {
    fn is_fused(&self, expr: &hir::Expr<'_>) -> bool {
        self.gcx.resolved_variable(expr.peel_parens()).is_some_and(|id| self.fused.contains(&id))
    }
}

impl<'gcx> Visit<'gcx> for Uses<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        match expr.kind {
            // mapping[key]
            ExprKind::Index(base, index) if self.is_fused(base) => {
                if let Some(index) = index {
                    self.visit_expr(index)?;
                }
                return ControlFlow::Continue(());
            }
            // mapping.slot, mapping.offset
            ExprKind::YulMember(base, member) if self.is_fused(base) => {
                self.gcx
                    .dcx()
                    .err(format!("inline assembly cannot take the `.{member}` of a fused mapping"))
                    .span(expr.span)
                    .note(
                        "`@custom:solar-fuse` moves the mapping's values out of their standard \
                         slots",
                    )
                    .emit();
                return ControlFlow::Continue(());
            }
            _ if self.is_fused(expr) => {
                self.gcx
                    .dcx()
                    .err("a fused mapping can only be indexed")
                    .span(expr.span)
                    .note(
                        "`@custom:solar-fuse` moves the mapping's values out of the standard \
                         slots that a storage reference reaches",
                    )
                    .emit();
                return ControlFlow::Continue(());
            }
            _ => {}
        }
        self.walk_expr(expr)
    }
}

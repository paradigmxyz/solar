//! ERC-7201 namespaces: structs documented with `@custom:storage-location erc7201:<id>`.
//!
//! A namespaced struct lives at the slot ERC-7201 derives from its id, and code reaches it through
//! an accessor that points a storage reference there in inline assembly, `$.slot := LOCATION`:
//! the one part of namespaced storage the language cannot type. This compiler checks that part:
//! an assembly assignment of a constant to the `.slot` of a reference to a namespaced struct must
//! assign the namespace's location, and no contract may see two structs in one namespace, which
//! would overlap. A contract sees the namespaced structs it declares or inherits, and those that
//! the accessors in the code it runs point at, which places a library's or a free function's
//! struct in the storage of every contract that runs its accessor. Other compilers read the
//! annotation as documentation.
//!
//! An assignment whose value is not a compile-time constant is not checked.

use crate::{
    eval::erc7201_slot,
    hir::{self, ExprKind, ItemId, StmtKind, Visit},
    ty::{Gcx, traced_functions},
};
use alloy_primitives::U256;
use solar_ast::DataLocation;
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
        .filter_map(|id| {
            let (namespace, tag) = gcx.hir.erc7201_namespace(id)?;
            Some((id, Namespace { id: namespace, tag }))
        })
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

/// Rejects two structs in one namespace that a contract declares or inherits, or that the
/// accessors in the code it runs point at.
fn check_overlaps(gcx: Gcx<'_>, namespaces: &FxHashMap<hir::StructId, Namespace>) {
    let mut reported = FxHashSet::default();
    for contract_id in gcx.hir.contract_ids() {
        let contract = gcx.hir.contract(contract_id);
        if contract.kind == hir::ContractKind::Interface {
            continue;
        }
        let declared = contract
            .linearized_bases
            .iter()
            .rev()
            .flat_map(|&base| gcx.hir.contract(base).items.iter().filter_map(ItemId::as_struct))
            .filter(|id| namespaces.contains_key(id))
            .collect::<Vec<_>>();
        let library = contract.kind == hir::ContractKind::Library;
        let mut pointed = Pointed { gcx, namespaces, structs: Vec::new() };
        for &function in traced_functions(gcx, contract_id, library, &|_| false).keys() {
            let _ = pointed.visit_nested_function(function);
        }
        let mut seen = FxHashMap::<Symbol, hir::StructId>::default();
        for &id in declared.iter().chain(&pointed.structs) {
            let namespace = namespaces[&id];
            let first = *seen.entry(namespace.id).or_insert(id);
            if first == id || !reported.insert(id) {
                continue;
            }
            let mut err = gcx
                .dcx()
                .err(format!("ERC-7201 namespace `{}` is declared twice", namespace.id))
                .span(namespace.tag)
                .span_note(
                    namespaces[&first].tag,
                    format!("`{}` declares it here", struct_name(gcx, first)),
                )
                .note("both structs would live at the same storage location");
            if !declared.contains(&id) || !declared.contains(&first) {
                err = err.note(format!("`{}` runs code that uses both", contract.name));
            }
            err.emit();
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
        let id = storage_struct(self.gcx, expr)?;
        Some((id, *self.namespaces.get(&id)?))
    }
}

/// The struct `expr` refers to in storage, if it is a storage reference to a struct.
fn storage_struct(gcx: Gcx<'_>, expr: &hir::Expr<'_>) -> Option<hir::StructId> {
    let ty = match gcx.type_of_expr(expr.id) {
        Some(ty) => ty,
        None => gcx.type_of_item(gcx.resolved_variable(expr)?.into()),
    };
    if !ty.is_ref_at(DataLocation::Storage) {
        return None;
    }
    let crate::ty::TyKind::Struct(id) = ty.peel_refs().kind else { return None };
    Some(id)
}

/// Whether the inline assembly block `block` only points storage references at the ERC-7201
/// namespaces of their structs: each statement assigns a namespace's location, as a constant, to
/// the `.slot` of a reference to a struct in it, which is what this check proves, and nothing else
/// in the block reads or writes memory or storage.
pub(super) fn is_namespace_accessor(gcx: Gcx<'_>, block: &hir::Block<'_>) -> bool {
    !block.stmts.is_empty()
        && block.stmts.iter().all(|stmt| {
            let StmtKind::Expr(expr) = stmt.kind else { return false };
            let ExprKind::Assign(lhs, None, rhs) = expr.kind else { return false };
            let ExprKind::YulMember(base, member) = lhs.peel_parens().kind else { return false };
            if member.name != sym::slot {
                return false;
            }
            let Some((namespace, _)) =
                storage_struct(gcx, base).and_then(|id| gcx.hir.erc7201_namespace(id))
            else {
                return false;
            };
            let location = erc7201_slot(namespace.as_str().as_bytes());
            gcx.try_eval_const(rhs)
                .is_ok_and(|value| value.as_u256() == Some(U256::from_be_bytes(location.0)))
        })
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

/// Collects the namespaced structs whose storage the accessors in the functions it visits point
/// at, in the order it reaches them.
struct Pointed<'gcx, 'a> {
    gcx: Gcx<'gcx>,
    namespaces: &'a FxHashMap<hir::StructId, Namespace>,
    structs: Vec<hir::StructId>,
}

impl<'gcx> Visit<'gcx> for Pointed<'gcx, '_> {
    type BreakValue = Never;

    fn hir(&self) -> &'gcx hir::Hir<'gcx> {
        &self.gcx.hir
    }

    fn visit_expr(&mut self, expr: &'gcx hir::Expr<'gcx>) -> ControlFlow<Self::BreakValue> {
        // $.slot := location
        if let ExprKind::Assign(lhs, None, _) = expr.kind
            && let ExprKind::YulMember(base, member) = lhs.peel_parens().kind
            && member.name == sym::slot
            && let Some(id) = storage_struct(self.gcx, base)
            && self.namespaces.contains_key(&id)
            && !self.structs.contains(&id)
        {
            self.structs.push(id);
        }
        self.walk_expr(expr)
    }
}

/// The name of the struct `id`, qualified by the contract or library that declares it.
fn struct_name(gcx: Gcx<'_>, id: hir::StructId) -> String {
    let strukt = gcx.hir.strukt(id);
    match strukt.contract {
        Some(contract) => format!("{}.{}", gcx.hir.contract(contract).name, strukt.name),
        None => strukt.name.to_string(),
    }
}

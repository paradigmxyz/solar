use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    hir::{self, Item, ItemId, Res},
    ty::{Gcx, Ty},
};
use alloy_primitives::B256;
use rayon::prelude::*;
use solar_ast::{StateMutability, Visibility};
use solar_data_structures::{
    map::{FxHashMap, FxHashSet},
    parallel,
};
use solar_interface::error_code;

pub(crate) fn check(gcx: Gcx<'_>) {
    parallel!(
        gcx.sess,
        gcx.hir.par_contract_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.contract_scopes[id]);
            check_payable_fallback_without_receive(gcx, id);
            check_external_type_clashes(gcx, id);
            check_receive_function(gcx, id);
        }),
        gcx.hir.par_source_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.source_scopes[id]);
        }),
    );
}

fn check_external_type_clashes(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    if gcx.hir.contract(contract_id).kind.is_library() {
        return;
    }

    let mut external_declarations: FxHashMap<B256, Vec<ItemId>> = FxHashMap::default();

    for item_id in gcx.hir.contract_item_ids(contract_id) {
        match gcx.hir.item(item_id) {
            Item::Function(f) if f.is_part_of_external_interface() => {
                let s = gcx.item_selector(item_id);
                external_declarations.entry(s).or_default().push(item_id);
            }
            _ => {}
        }
    }
    for items in external_declarations.values() {
        for (i, &item) in items.iter().enumerate() {
            if let Some(&dup) = items.iter().skip(i + 1).find(|&&other| {
                !same_external_params(gcx, gcx.type_of_item(item), gcx.type_of_item(other))
            }) {
                gcx.dcx()
                    .err(
                        "function overload clash during conversion to external types for arguments",
                    )
                    .code(error_code!(9914))
                    .span(gcx.item_span(item))
                    .span_help(gcx.item_span(dup), "other declaration is here")
                    .emit();
            }
        }
    }
}

fn check_payable_fallback_without_receive(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);

    if let Some(fallback) = contract.fallback {
        let fallback = gcx.hir.function(fallback);
        if fallback.state_mutability.is_payable()
            && contract.receive.is_none()
            && !gcx.interface_functions(contract_id).is_empty()
        {
            gcx.dcx()
                .warn("contract has a payable fallback function, but no receive ether function")
                .span(contract.name.span)
                .code(error_code!(3628))
                .span_help(fallback.keyword_span(), "consider changing `fallback` to `receive`")
                .emit();
        }
    }
}

/// Checks for definitions that have the same name and parameter types in the given scope.
fn check_duplicate_definitions(gcx: Gcx<'_>, scope: &Declarations) {
    let is_duplicate = |a: Declaration, b: Declaration| -> bool {
        let (Res::Item(a), Res::Item(b)) = (a.res, b.res) else { return false };
        if !a.matches(&b) {
            return false;
        }
        if !(a.is_function() || a.is_event()) {
            return false;
        }
        // Don't check inheritance since this check would be incorrect with virtual/override.
        if let (hir::ItemId::Function(f1), hir::ItemId::Function(f2)) = (a, b) {
            let f1 = gcx.hir.function(f1);
            let f2 = gcx.hir.function(f2);
            if f1.contract != f2.contract {
                return false;
            }
        }
        if !same_external_params(gcx, gcx.type_of_item(a), gcx.type_of_item(b)) {
            return false;
        }
        true
    };

    let mut reported = FxHashSet::default();
    for (_name, decls) in scope.iter() {
        if decls.len() <= 1 {
            continue;
        }
        reported.clear();
        for (i, &decl) in decls.iter().enumerate() {
            if reported.contains(&i) {
                continue;
            }

            let mut duplicates = Vec::new();
            for (j, &other_decl) in decls.iter().enumerate().skip(i + 1) {
                if is_duplicate(decl, other_decl) {
                    reported.insert(j);
                    duplicates.push(other_decl.span);
                }
            }
            if !duplicates.is_empty() {
                let msg = format!(
                    "{} with same name and parameter types declared twice",
                    decl.description()
                );
                let mut err = gcx.dcx().err(msg).span(decl.span);
                for duplicate in duplicates {
                    err = err.span_note(duplicate, "other declaration");
                }
                err.emit();
            }
        }
    }
}

fn same_external_params<'gcx>(gcx: Gcx<'gcx>, a: Ty<'gcx>, b: Ty<'gcx>) -> bool {
    let key = |ty: Ty<'gcx>| ty.as_externally_callable_function(gcx).parameters().unwrap();
    key(a) == key(b)
}

fn check_receive_function(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);

    // Libraries cannot have receive functions
    if contract.kind.is_library() {
        if let Some(receive) = contract.receive {
            gcx.dcx()
                .err("libraries cannot have receive ether functions")
                .span(gcx.item_span(receive))
                .emit();
        }
        return;
    }
    if let Some(receive) = contract.receive {
        let f = gcx.hir.function(receive);
        // Check visibility
        if f.visibility != Visibility::External {
            gcx.dcx()
                .err("receive ether function must be defined as \"external\"")
                .span(gcx.item_span(receive))
                .emit();
        }

        // Check state mutability
        if f.state_mutability != StateMutability::Payable {
            gcx.dcx()
                .err("receive ether function must be payable")
                .span(gcx.item_span(receive))
                .help("add `payable` state mutability")
                .emit();
        }

        // Check parameters
        if !f.parameters.is_empty() {
            gcx.dcx()
                .err("receive ether function cannot take parameters")
                .span(gcx.item_span(receive))
                .emit();
        }

        // Check return values
        if !f.returns.is_empty() {
            gcx.dcx()
                .err("receive ether function cannot return values")
                .span(gcx.item_span(receive))
                .emit();
        }
    }
}

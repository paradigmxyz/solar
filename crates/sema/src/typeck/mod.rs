use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    hir::{self, Item, ItemId, Res},
    ty::{Gcx, Ty},
};
use alloy_primitives::B256;
use rayon::prelude::*;
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
        if fallback.state_mutability.is_payable() && contract.receive.is_none() {
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
    for (_name, decls) in &scope.declarations {
        let decls = &decls[..];
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

/// Checks for type clashes in external functions across inheritance chains.
fn check_external_type_clashes(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);
    let mut external_declarations = FxHashMap::<&str, Vec<(hir::ItemId, Ty<'_>)>>::default();

    // Collect all external declarations (functions and state variables) from the contract and its bases
    for &base_id in contract.linearized_bases {
        let base = gcx.hir.contract(base_id);
        
        // Check functions
        for &func_id in base.functions() {
            let func = gcx.hir.function(func_id);
            if !func.is_part_of_external_interface() {
                continue;
            }

            let func_ty = gcx.type_of_item(func_id.into());
            if let Some(external_sig) = func_ty.as_externally_callable_function(gcx).map(|f| f.external_signature()) {
                external_declarations
                    .entry(external_sig)
                    .or_default()
                    .push((func_id.into(), func_ty));
            }
        }

        // Check state variables
        for &var_id in base.variables() {
            let var = gcx.hir.variable(var_id);
            if !var.is_public() {
                continue;
            }

            let var_ty = gcx.type_of_item(var_id.into());
            if let Some(external_sig) = var_ty.as_externally_callable_function(gcx).map(|f| f.external_signature()) {
                external_declarations
                    .entry(external_sig)
                    .or_default()
                    .push((var_id.into(), var_ty));
            }
        }
    }

    // Check for type clashes in each group of declarations with the same external signature
    for (_, declarations) in external_declarations {
        for i in 0..declarations.len() {
            for j in (i + 1)..declarations.len() {
                let (item1, ty1) = &declarations[i];
                let (item2, ty2) = &declarations[j];

                if !same_external_params(gcx, *ty1, *ty2) {
                    let msg = "Function overload clash during conversion to external types for arguments";
                    let mut err = gcx.dcx().err(msg).span(gcx.item_span(*item2));
                    err = err.span_note(gcx.item_span(*item1), "conflicting declaration");
                    err.emit();
                }
            }
        }
    }
}

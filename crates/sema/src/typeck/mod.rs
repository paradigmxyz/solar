use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    hir::{self, Res},
    ty::{Gcx, Ty},
};
use rayon::prelude::*;
use solar_data_structures::{map::FxHashSet, parallel};
use solar_interface::error_code;

pub(crate) fn check(gcx: Gcx<'_>) {
    parallel!(
        gcx.sess,
        gcx.hir.par_contract_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.contract_scopes[id]);
            check_payable_fallback_without_receive(gcx, id);
        }),
        gcx.hir.par_source_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.source_scopes[id]);
        }),
    );
}

fn check_payable_fallback_without_receive(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract = gcx.hir.contract(contract_id);

    let payable_fallback_found =
        contract.fallback.is_some_and(|f| gcx.hir.function(f).state_mutability.is_payable());

    let receive_found = contract.receive.is_some();

    if payable_fallback_found && !receive_found {
        gcx.dcx()
            .warn("contract has a payable fallback function, but no receive ether function")
            .code(error_code!(3628))
            .span_help(contract.name.span, "consider changing fallback to receive")
            .emit();
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

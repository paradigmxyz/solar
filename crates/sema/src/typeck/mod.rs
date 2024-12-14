use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    eval::ConstantEvaluator,
    hir::{self, Res},
    ty::{Gcx, Ty},
};
use alloy_primitives::U256;
use rayon::prelude::*;
use solar_data_structures::{map::FxHashSet, parallel};

pub(crate) fn check(gcx: Gcx<'_>) {
    parallel!(
        gcx.sess,
        gcx.hir.par_contract_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.contract_scopes[id]);
        }),
        gcx.hir.par_source_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.source_scopes[id]);
        }),
        gcx.hir.par_contract_ids().for_each(|id| {
            check_storage_size_upper_bound(gcx, id);
        }),
    );
}

/// Checks for violation of maximum storage size to ensure slot allocation algorithms works.
/// Reference: https://github.com/ethereum/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/analysis/ContractLevelChecker.cpp#L556C1-L570C2
fn check_storage_size_upper_bound(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract_items = gcx.hir.contract_items(contract_id);
    let mut total_size = U256::ZERO;
    for item in contract_items {
        if let hir::Item::Variable(variable) = item {
            // Skip constant and immutable variables
            if variable.mutability.is_none() {
                let Some(size_contribution) = variable_ty_upper_bound_size(&variable.ty.kind, gcx)
                else {
                    gcx.dcx().err("overflowed storage slots").emit();
                    return;
                };
                let Some(sz) = total_size.checked_add(size_contribution) else {
                    gcx.dcx().err("overflowed storage slots").emit();
                    return;
                };
                total_size = sz;
            }
        }
    }
    //let c = gcx.hir.contract(contract_id).name;
    //println!("{total_size} - {c}");
}

fn item_ty_upper_bound_size(item: &hir::Item<'_, '_>, gcx: Gcx<'_>) -> Option<U256> {
    match item {
        hir::Item::Function(_) | hir::Item::Contract(_) => Some(U256::from(1)),
        hir::Item::Variable(variable) => variable_ty_upper_bound_size(&variable.ty.kind, gcx),
        hir::Item::Udvt(udvt) => variable_ty_upper_bound_size(&udvt.ty.kind, gcx),
        hir::Item::Struct(strukt) => {
            // Reference https://github.com/ethereum/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L2303C1-L2309C2
            let mut total_size = U256::from(1);
            for field_id in strukt.fields {
                let variable = gcx.hir.variable(*field_id);
                let size_contribution = variable_ty_upper_bound_size(&variable.ty.kind, gcx)?;
                total_size = total_size.checked_add(size_contribution)?;
            }
            Some(total_size)
        }
        hir::Item::Enum(_) | hir::Item::Event(_) | hir::Item::Error(_) => {
            // Enum and events cannot be types of storage variables
            unreachable!("illegal values")
        }
    }
}

fn variable_ty_upper_bound_size(var_ty: &hir::TypeKind<'_>, gcx: Gcx<'_>) -> Option<U256> {
    match &var_ty {
        hir::TypeKind::Elementary(_) | hir::TypeKind::Function(_) | hir::TypeKind::Mapping(_) => {
            Some(U256::from(1))
        }
        hir::TypeKind::Array(array) => {
            // Reference: https://github.com/ethereum/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L1800C1-L1806C2
            if let Some(len_expr) = array.size {
                // Evaluate the length expression in array declaration
                let mut e = ConstantEvaluator::new(gcx);
                let arr_len = e.eval(len_expr).unwrap().data; // `.eval()` emits errors beforehand

                // Estimate the upper bound size of each individual element
                let elem_size = variable_ty_upper_bound_size(&array.element.kind, gcx)?;
                arr_len.checked_mul(elem_size)
            } else {
                // For dynamic size arrays
                Some(U256::from(1))
            }
        }
        hir::TypeKind::Custom(item_id) => {
            let item = gcx.hir.item(*item_id);
            item_ty_upper_bound_size(&item, gcx)
        }
        hir::TypeKind::Err(_) => {
            unreachable!()
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

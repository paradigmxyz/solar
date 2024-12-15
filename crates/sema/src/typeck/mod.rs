use crate::{
    ast_lowering::resolve::{Declaration, Declarations},
    hir::{self, Res},
    ty::{Gcx, Ty, TyKind},
};
use alloy_primitives::U256;
use rayon::prelude::*;
use solar_data_structures::{map::FxHashSet, parallel};

pub(crate) fn check(gcx: Gcx<'_>) {
    parallel!(
        gcx.sess,
        gcx.hir.par_contract_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.contract_scopes[id]);
            check_storage_size_upper_bound(gcx, id);
        }),
        gcx.hir.par_source_ids().for_each(|id| {
            check_duplicate_definitions(gcx, &gcx.symbol_resolver.source_scopes[id]);
        }),
    );
}

/// Checks for violation of maximum storage size to ensure slot allocation algorithms works.
/// Reference: https://github.com/ethereum/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/analysis/ContractLevelChecker.cpp#L556C1-L570C2
fn check_storage_size_upper_bound(gcx: Gcx<'_>, contract_id: hir::ContractId) {
    let contract_span = gcx.hir.contract(contract_id).span;
    let contract_items = gcx.hir.contract_items(contract_id);
    let mut total_size = U256::ZERO;
    for item in contract_items {
        if let hir::Item::Variable(variable) = item {
            // Skip constant and immutable variables
            if variable.mutability.is_none() {
                let t = gcx.type_of_hir_ty(&variable.ty);
                match ty_upper_bound_storage_var_size(t, gcx)
                    .and_then(|size_contribution| total_size.checked_add(size_contribution))
                {
                    Some(sz) => {
                        total_size = sz;
                    }
                    None => {
                        gcx.dcx()
                            .err("contract requires too much storage")
                            .span(contract_span)
                            .emit();
                        return;
                    }
                }
            }
        }
    }
    if cfg!(debug_assertions) {
        let full_contract_name = format!("{}", gcx.contract_fully_qualified_name(contract_id));
        if full_contract_name.contains("contract_storage_size_check") {
            eprintln!("{full_contract_name} requires {total_size} maximum storage");
        }
    }
}

fn ty_upper_bound_storage_var_size(ty: Ty<'_>, gcx: Gcx<'_>) -> Option<U256> {
    match ty.kind {
        TyKind::Elementary(..)
        | TyKind::StringLiteral(..)
        | TyKind::IntLiteral(..)
        | TyKind::Mapping(..)
        | TyKind::Contract(..)
        | TyKind::Udvt(..)
        | TyKind::Enum(..)
        | TyKind::DynArray(..) => Some(U256::from(1)),
        TyKind::Ref(..)
        | TyKind::Tuple(..)
        | TyKind::FnPtr(..)
        | TyKind::Module(..)
        | TyKind::BuiltinModule(..)
        | TyKind::Event(..)
        | TyKind::Meta(..)
        | TyKind::Err(..)
        | TyKind::Error(..) => {
            unreachable!()
        }
        TyKind::Array(ty, uint) => {
            // Reference: https://github.com/ethereum/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L1800C1-L1806C2
            let elem_size = ty_upper_bound_storage_var_size(ty, gcx)?;
            uint.checked_mul(elem_size)
        }
        TyKind::Struct(struct_id) => {
            let strukt = gcx.hir.strukt(struct_id);
            // Reference https://github.com/ethereum/solidity/blob/03e2739809769ae0c8d236a883aadc900da60536/libsolidity/ast/Types.cpp#L2303C1-L2309C2
            let mut total_size = U256::from(1);
            for field_id in strukt.fields {
                let variable = gcx.hir.variable(*field_id);
                let t = gcx.type_of_hir_ty(&variable.ty);
                let size_contribution = ty_upper_bound_storage_var_size(t, gcx)?;
                total_size = total_size.checked_add(size_contribution)?;
            }
            Some(total_size)
        }
        TyKind::Type(ty) => ty_upper_bound_storage_var_size(ty, gcx),
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

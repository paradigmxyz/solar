//! Override checker for function and modifier overrides in inheritance hierarchies.
//!
//! This module validates:
//! - Functions with `override` actually override something
//! - Base functions must be `virtual` to be overridden
//! - `override(Base1, Base2)` must list all bases being overridden in multi-inheritance
//! - Modifier override validation (same rules as functions)
//! - Multi-inheritance conflict detection (diamond inheritance)
//! - Abstract function checks (non-abstract contracts cannot have unimplemented functions)
//! - Visibility compatibility (override cannot be more restrictive)
//! - State mutability compatibility
//! - Return type compatibility
//!
//! Reference: solc OverrideChecker.cpp

use crate::{
    hir::{self, ContractId, FunctionId},
    ty::{Gcx, Ty, TyKind},
};

use solar_ast::{StateMutability, Visibility};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{Ident, Span, diagnostics::DiagCtxt, error_code};
use std::collections::BTreeSet;

pub(crate) fn check(gcx: Gcx<'_>, contract_id: ContractId) {
    let checker = OverrideChecker::new(gcx, contract_id);
    checker.check();
}

struct OverrideChecker<'gcx> {
    gcx: Gcx<'gcx>,
    contract_id: ContractId,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum OverrideProxy {
    Function(FunctionId),
    Variable(hir::VariableId),
}

impl OverrideProxy {
    fn span(self, gcx: Gcx<'_>) -> Span {
        match self {
            Self::Function(id) => gcx.hir.function(id).span,
            Self::Variable(id) => gcx.hir.variable(id).span,
        }
    }

    fn name(self, gcx: Gcx<'_>) -> Option<Ident> {
        gcx.item_name_opt(self.into_item_id())
    }

    fn name_string(self, gcx: Gcx<'_>) -> String {
        self.name(gcx).map_or_else(|| "<unnamed>".to_string(), |n| n.to_string())
    }

    fn into_item_id(self) -> hir::ItemId {
        match self {
            Self::Function(id) => id.into(),
            Self::Variable(id) => id.into(),
        }
    }

    fn contract(self, gcx: Gcx<'_>) -> Option<ContractId> {
        match self {
            Self::Function(id) => gcx.hir.function(id).contract,
            Self::Variable(id) => gcx.hir.variable(id).contract,
        }
    }

    fn is_variable(self) -> bool {
        matches!(self, Self::Variable(_))
    }

    fn is_modifier(self, gcx: Gcx<'_>) -> bool {
        match self {
            Self::Function(id) => gcx.hir.function(id).kind.is_modifier(),
            Self::Variable(_) => false,
        }
    }

    fn is_virtual(self, gcx: Gcx<'_>) -> bool {
        match self {
            Self::Function(id) => gcx.hir.function(id).virtual_,
            Self::Variable(_) => false,
        }
    }

    fn has_override(self, gcx: Gcx<'_>) -> bool {
        match self {
            Self::Function(id) => gcx.hir.function(id).override_,
            Self::Variable(id) => gcx.hir.variable(id).override_,
        }
    }

    fn overrides(self, gcx: Gcx<'_>) -> &[ContractId] {
        match self {
            Self::Function(id) => gcx.hir.function(id).overrides,
            Self::Variable(id) => gcx.hir.variable(id).overrides,
        }
    }

    fn visibility(self, gcx: Gcx<'_>) -> Visibility {
        match self {
            Self::Function(id) => gcx.hir.function(id).visibility,
            Self::Variable(id) => gcx.hir.variable(id).visibility.unwrap_or(Visibility::Internal),
        }
    }

    fn state_mutability(self, gcx: Gcx<'_>) -> StateMutability {
        match self {
            Self::Function(id) => gcx.hir.function(id).state_mutability,
            Self::Variable(id) => {
                let v = gcx.hir.variable(id);
                if v.is_constant() { StateMutability::Pure } else { StateMutability::View }
            }
        }
    }

    fn is_implemented(self, gcx: Gcx<'_>) -> bool {
        match self {
            Self::Function(id) => gcx.hir.function(id).body.is_some(),
            Self::Variable(_) => true,
        }
    }

    fn ast_node_name(self, gcx: Gcx<'_>) -> &'static str {
        match self {
            Self::Function(id) => {
                let f = gcx.hir.function(id);
                if f.kind.is_modifier() { "modifier" } else { "function" }
            }
            Self::Variable(_) => "public state variable",
        }
    }

    fn ty(self, gcx: Gcx<'_>) -> Ty<'_> {
        match self {
            Self::Function(id) => gcx.type_of_item(id.into()),
            Self::Variable(id) => {
                let v = gcx.hir.variable(id);
                if let Some(getter_id) = v.getter {
                    gcx.type_of_item(getter_id.into())
                } else {
                    gcx.type_of_item(id.into())
                }
            }
        }
    }

    fn function_id(self) -> Option<FunctionId> {
        match self {
            Self::Function(id) => Some(id),
            Self::Variable(_) => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct OverrideSignature {
    name: String,
    is_modifier: bool,
    param_types: Option<Vec<String>>,
}

impl<'gcx> OverrideChecker<'gcx> {
    fn new(gcx: Gcx<'gcx>, contract_id: ContractId) -> Self {
        Self { gcx, contract_id }
    }

    fn dcx(&self) -> &'gcx DiagCtxt {
        self.gcx.dcx()
    }

    fn contract(&self) -> &'gcx hir::Contract<'gcx> {
        self.gcx.hir.contract(self.contract_id)
    }

    fn check(&self) {
        let contract = self.contract();
        if contract.linearization_failed() {
            return;
        }

        self.check_illegal_overrides();
        self.check_ambiguous_overrides();
        self.check_abstract_definitions();
    }

    fn signature(&self, proxy: OverrideProxy) -> OverrideSignature {
        let name = proxy.name_string(self.gcx);
        let is_modifier = proxy.is_modifier(self.gcx);
        let param_types = if is_modifier {
            None
        } else {
            let ty = proxy.ty(self.gcx);
            if let TyKind::FnPtr(fn_ptr) = ty.kind {
                Some(fn_ptr.parameters.iter().map(|t| format!("{t:?}")).collect())
            } else {
                None
            }
        };
        OverrideSignature { name, is_modifier, param_types }
    }

    fn inherited_functions(&self) -> FxHashMap<OverrideSignature, Vec<OverrideProxy>> {
        let contract = self.contract();
        let mut result: FxHashMap<OverrideSignature, Vec<OverrideProxy>> = FxHashMap::default();
        let mut seen_signatures: FxHashSet<OverrideSignature> = FxHashSet::default();

        for &base_id in contract.bases.iter() {
            let base = self.gcx.hir.contract(base_id);

            for f_id in base.functions() {
                let f = self.gcx.hir.function(f_id);
                if f.kind.is_constructor() || f.name.is_none() {
                    continue;
                }
                let proxy = OverrideProxy::Function(f_id);
                let sig = self.signature(proxy);

                result.entry(sig.clone()).or_default().push(proxy);
                seen_signatures.insert(sig);
            }

            for v_id in base.variables() {
                let v = self.gcx.hir.variable(v_id);
                if !v.is_public() {
                    continue;
                }
                let proxy = OverrideProxy::Variable(v_id);
                let sig = self.signature(proxy);

                result.entry(sig.clone()).or_default().push(proxy);
                seen_signatures.insert(sig);
            }

            for &ancestor_id in &base.linearized_bases[1..] {
                let ancestor = self.gcx.hir.contract(ancestor_id);

                for f_id in ancestor.functions() {
                    let f = self.gcx.hir.function(f_id);
                    if f.kind.is_constructor() || f.name.is_none() {
                        continue;
                    }
                    let proxy = OverrideProxy::Function(f_id);
                    let sig = self.signature(proxy);

                    if !seen_signatures.contains(&sig) {
                        result.entry(sig.clone()).or_default().push(proxy);
                    }
                }

                for v_id in ancestor.variables() {
                    let v = self.gcx.hir.variable(v_id);
                    if !v.is_public() {
                        continue;
                    }
                    let proxy = OverrideProxy::Variable(v_id);
                    let sig = self.signature(proxy);

                    if !seen_signatures.contains(&sig) {
                        result.entry(sig.clone()).or_default().push(proxy);
                    }
                }
            }
        }

        result
    }

    fn check_illegal_overrides(&self) {
        let contract = self.contract();
        let inherited = self.inherited_functions();

        let inherited_modifiers: FxHashSet<String> =
            inherited.keys().filter(|s| s.is_modifier).map(|s| s.name.clone()).collect();

        let inherited_functions: FxHashSet<String> =
            inherited.keys().filter(|s| !s.is_modifier).map(|s| s.name.clone()).collect();

        for f_id in contract.functions() {
            let f = self.gcx.hir.function(f_id);
            if f.kind.is_constructor() || f.name.is_none() {
                continue;
            }
            let proxy = OverrideProxy::Function(f_id);
            let name = proxy.name_string(self.gcx);

            if f.kind.is_modifier() {
                if inherited_functions.contains(&name) {
                    self.dcx()
                        .err("override changes function or public state variable to modifier")
                        .code(error_code!(5631))
                        .span(f.span)
                        .emit();
                }
            } else if inherited_modifiers.contains(&name) {
                self.dcx()
                    .err("override changes modifier to function")
                    .code(error_code!(1469))
                    .span(f.span)
                    .emit();
            }

            let sig = self.signature(proxy);
            if let Some(bases) = inherited.get(&sig) {
                self.check_override_list(proxy, bases);
            } else if f.override_ {
                self.dcx()
                    .err(format!(
                        "{} has override specified but does not override anything",
                        proxy.ast_node_name(self.gcx).to_string().replace(
                            proxy.ast_node_name(self.gcx).chars().next().unwrap(),
                            &proxy
                                .ast_node_name(self.gcx)
                                .chars()
                                .next()
                                .unwrap()
                                .to_uppercase()
                                .to_string()
                        )
                    ))
                    .code(error_code!(7792))
                    .span(f.span)
                    .emit();
            }
        }

        for v_id in contract.variables() {
            let v = self.gcx.hir.variable(v_id);

            if !v.is_public() {
                if v.override_ {
                    self.dcx()
                        .err("override can only be used with public state variables")
                        .code(error_code!(8022))
                        .span(v.span)
                        .emit();
                }
                continue;
            }

            let proxy = OverrideProxy::Variable(v_id);
            let name = proxy.name_string(self.gcx);

            if inherited_modifiers.contains(&name) {
                self.dcx()
                    .err("override changes modifier to public state variable")
                    .code(error_code!(1456))
                    .span(v.span)
                    .emit();
            }

            let sig = self.signature(proxy);
            if let Some(bases) = inherited.get(&sig) {
                self.check_override_list(proxy, bases);
            } else if v.override_ {
                self.dcx()
                    .err("public state variable has override specified but does not override anything")
                    .code(error_code!(7792))
                    .span(v.span)
                    .emit();
            }
        }
    }

    fn check_override_list(&self, overriding: OverrideProxy, bases: &[OverrideProxy]) {
        let specified_contracts: FxHashSet<ContractId> =
            overriding.overrides(self.gcx).iter().copied().collect();

        if overriding.overrides(self.gcx).len() != specified_contracts.len() {
            self.dcx()
                .err(format!(
                    "duplicate contract found in override list of \"{}\"",
                    overriding.name_string(self.gcx)
                ))
                .code(error_code!(4520))
                .span(overriding.span(self.gcx))
                .emit();
        }

        let mut expected_contracts: BTreeSet<ContractId> = BTreeSet::new();

        for &base in bases {
            self.check_override(overriding, base);
            if let Some(contract) = base.contract(self.gcx) {
                expected_contracts.insert(contract);
            }
        }

        if expected_contracts.len() > 1 {
            let missing: Vec<_> = expected_contracts
                .iter()
                .filter(|c| !specified_contracts.contains(*c))
                .copied()
                .collect();

            if !missing.is_empty() {
                let missing_names: Vec<_> = missing
                    .iter()
                    .map(|&c| format!("\"{}\"", self.gcx.hir.contract(c).name.as_str()))
                    .collect();
                self.dcx()
                    .err(format!(
                        "{} needs to specify overridden contracts {}",
                        capitalize(overriding.ast_node_name(self.gcx)),
                        missing_names.join(" and ")
                    ))
                    .code(error_code!(4327))
                    .span(overriding.span(self.gcx))
                    .emit();
            }
        }

        let surplus: Vec<_> = specified_contracts
            .iter()
            .filter(|c| !expected_contracts.contains(*c))
            .copied()
            .collect();

        if !surplus.is_empty() {
            let surplus_names: Vec<_> = surplus
                .iter()
                .map(|&c| format!("\"{}\"", self.gcx.hir.contract(c).name.as_str()))
                .collect();
            self.dcx()
                .err(format!(
                    "invalid contract{} specified in override list: {}",
                    if surplus.len() > 1 { "s" } else { "" },
                    surplus_names.join(", ")
                ))
                .code(error_code!(2353))
                .span(overriding.span(self.gcx))
                .emit();
        }
    }

    fn check_override(&self, overriding: OverrideProxy, base: OverrideProxy) {
        let gcx = self.gcx;
        let base_in_interface =
            base.contract(gcx).map(|c| gcx.hir.contract(c).kind.is_interface()).unwrap_or(false);

        if !overriding.has_override(gcx) && !base_in_interface {
            self.dcx()
                .err(format!(
                    "overriding {} is missing \"override\" specifier",
                    overriding.ast_node_name(gcx)
                ))
                .code(error_code!(9456))
                .span(overriding.span(gcx))
                .span_note(
                    base.span(gcx),
                    format!("overridden {} is here", base.ast_node_name(gcx)),
                )
                .emit();
        }

        if base.is_variable() {
            self.dcx()
                .err("cannot override public state variable")
                .code(error_code!(1452))
                .span(base.span(gcx))
                .span_note(
                    overriding.span(gcx),
                    format!("overriding {} is here", overriding.ast_node_name(gcx)),
                )
                .emit();
            return;
        }

        if !base.is_virtual(gcx) {
            self.dcx()
                .err(format!(
                    "trying to override non-virtual {}. Did you forget to add \"virtual\"?",
                    base.ast_node_name(gcx)
                ))
                .code(error_code!(4334))
                .span(base.span(gcx))
                .span_note(
                    overriding.span(gcx),
                    format!("overriding {} is here", overriding.ast_node_name(gcx)),
                )
                .emit();
        }

        self.check_visibility_compatibility(overriding, base);
        self.check_mutability_compatibility(overriding, base);

        if !base.is_modifier(gcx) && !overriding.is_variable() {
            self.check_return_type_compatibility(overriding, base);
        }

        if base.is_modifier(gcx) {
            self.check_modifier_signature_compatibility(overriding, base);
        }

        if !overriding.is_implemented(gcx) && base.is_implemented(gcx) {
            self.dcx()
                .err(format!(
                    "overriding an implemented {} with an unimplemented {} is not allowed",
                    base.ast_node_name(gcx),
                    overriding.ast_node_name(gcx)
                ))
                .code(error_code!(4593))
                .span(overriding.span(gcx))
                .span_note(
                    base.span(gcx),
                    format!("overridden {} is here", base.ast_node_name(gcx)),
                )
                .emit();
        }
    }

    fn check_visibility_compatibility(&self, overriding: OverrideProxy, base: OverrideProxy) {
        let gcx = self.gcx;
        let overriding_vis = overriding.visibility(gcx);
        let base_vis = base.visibility(gcx);

        if overriding.is_variable() {
            if base_vis != Visibility::External {
                self.dcx()
                    .err("public state variables can only override functions with external visibility")
                    .code(error_code!(5225))
                    .span(overriding.span(gcx))
                    .span_note(base.span(gcx), "overridden function is here")
                    .emit();
            }
            return;
        }

        if overriding_vis != base_vis {
            let allowed = base_vis == Visibility::External && overriding_vis == Visibility::Public;
            if !allowed {
                self.dcx()
                    .err(format!("overriding {} visibility differs", overriding.ast_node_name(gcx)))
                    .code(error_code!(9098))
                    .span(overriding.span(gcx))
                    .span_note(
                        base.span(gcx),
                        format!("overridden {} is here", base.ast_node_name(gcx)),
                    )
                    .emit();
            }
        }
    }

    fn check_mutability_compatibility(&self, overriding: OverrideProxy, base: OverrideProxy) {
        let gcx = self.gcx;

        if base.is_modifier(gcx) {
            return;
        }

        let overriding_mut = overriding.state_mutability(gcx);
        let base_mut = base.state_mutability(gcx);

        let is_stricter = |a: StateMutability, b: StateMutability| -> bool {
            matches!(
                (a, b),
                (StateMutability::Pure, StateMutability::View)
                    | (StateMutability::Pure, StateMutability::NonPayable)
                    | (StateMutability::View, StateMutability::NonPayable)
            )
        };

        let is_less_strict =
            |a: StateMutability, b: StateMutability| -> bool { a != b && !is_stricter(a, b) };

        if (base_mut == StateMutability::Payable && overriding_mut != StateMutability::Payable)
            || is_less_strict(overriding_mut, base_mut)
        {
            self.dcx()
                .err(format!(
                    "overriding {} changes state mutability from \"{}\" to \"{}\"",
                    overriding.ast_node_name(gcx),
                    base_mut.to_str(),
                    overriding_mut.to_str()
                ))
                .code(error_code!(6959))
                .span(overriding.span(gcx))
                .emit();
        }
    }

    fn check_return_type_compatibility(&self, overriding: OverrideProxy, base: OverrideProxy) {
        let gcx = self.gcx;

        let overriding_ty = overriding.ty(gcx);
        let base_ty = base.ty(gcx);

        let (TyKind::FnPtr(overriding_fn), TyKind::FnPtr(base_fn)) =
            (overriding_ty.kind, base_ty.kind)
        else {
            return;
        };

        if overriding_fn.returns != base_fn.returns {
            self.dcx()
                .err(format!("overriding {} return types differ", overriding.ast_node_name(gcx)))
                .code(error_code!(4822))
                .span(overriding.span(gcx))
                .span_note(
                    base.span(gcx),
                    format!("overridden {} is here", base.ast_node_name(gcx)),
                )
                .emit();
        }
    }

    fn check_modifier_signature_compatibility(
        &self,
        overriding: OverrideProxy,
        base: OverrideProxy,
    ) {
        let gcx = self.gcx;

        let Some(overriding_id) = overriding.function_id() else { return };
        let Some(base_id) = base.function_id() else { return };

        let overriding_f = gcx.hir.function(overriding_id);
        let base_f = gcx.hir.function(base_id);

        if overriding_f.parameters.len() != base_f.parameters.len() {
            self.dcx()
                .err("override changes modifier signature")
                .code(error_code!(1078))
                .span(overriding.span(gcx))
                .emit();
            return;
        }

        for (&over_param, &base_param) in
            overriding_f.parameters.iter().zip(base_f.parameters.iter())
        {
            let over_ty = gcx.type_of_item(over_param.into());
            let base_ty = gcx.type_of_item(base_param.into());

            if over_ty != base_ty {
                self.dcx()
                    .err("override changes modifier signature")
                    .code(error_code!(1078))
                    .span(overriding.span(gcx))
                    .emit();
                return;
            }
        }
    }

    fn check_ambiguous_overrides(&self) {
        let contract = self.contract();
        let inherited = self.inherited_functions();

        let own_functions: FxHashSet<OverrideSignature> = contract
            .functions()
            .filter_map(|f_id| {
                let f = self.gcx.hir.function(f_id);
                if f.kind.is_constructor() || f.name.is_none() {
                    return None;
                }
                Some(self.signature(OverrideProxy::Function(f_id)))
            })
            .collect();

        let own_variables: FxHashSet<OverrideSignature> = contract
            .variables()
            .filter_map(|v_id| {
                let v = self.gcx.hir.variable(v_id);
                if !v.is_public() {
                    return None;
                }
                Some(self.signature(OverrideProxy::Variable(v_id)))
            })
            .collect();

        for (sig, bases) in &inherited {
            if own_functions.contains(sig) || own_variables.contains(sig) {
                continue;
            }

            let unique_contracts: BTreeSet<ContractId> =
                bases.iter().filter_map(|p| p.contract(self.gcx)).collect();

            if unique_contracts.len() > 1 {
                let base_names: Vec<_> = unique_contracts
                    .iter()
                    .map(|&c| format!("\"{}\"", self.gcx.hir.contract(c).name.as_str()))
                    .collect();

                let kind = if sig.is_modifier { "modifier" } else { "function" };

                self.dcx()
                    .err(format!(
                        "derived contract must override {} \"{}\". Two or more base classes define {} with same {}",
                        kind,
                        sig.name,
                        kind,
                        if sig.is_modifier { "name" } else { "name and parameter types" }
                    ))
                    .code(error_code!(6480))
                    .span(contract.name.span)
                    .note(format!("defined in: {}", base_names.join(", ")))
                    .emit();
            }
        }
    }

    fn check_abstract_definitions(&self) {
        let contract = self.contract();

        if contract.is_abstract() || contract.kind.is_interface() {
            return;
        }

        let mut unimplemented: Vec<(OverrideProxy, ContractId)> = Vec::new();

        for &base_id in contract.linearized_bases.iter() {
            let base = self.gcx.hir.contract(base_id);

            for f_id in base.functions() {
                let f = self.gcx.hir.function(f_id);
                if f.kind.is_constructor() || f.name.is_none() {
                    continue;
                }
                if f.body.is_none() {
                    let sig = self.signature(OverrideProxy::Function(f_id));
                    let is_implemented = contract.linearized_bases.iter().any(|&impl_base_id| {
                        let impl_base = self.gcx.hir.contract(impl_base_id);
                        impl_base.functions().any(|impl_f_id| {
                            let impl_f = self.gcx.hir.function(impl_f_id);
                            if impl_f.body.is_none() || impl_f.name.is_none() {
                                return false;
                            }
                            let impl_sig = self.signature(OverrideProxy::Function(impl_f_id));
                            impl_sig == sig
                        })
                    });
                    if !is_implemented {
                        unimplemented.push((OverrideProxy::Function(f_id), base_id));
                    }
                }
            }
        }

        let mut seen_sigs: FxHashSet<OverrideSignature> = FxHashSet::default();
        for (proxy, base_id) in unimplemented {
            let sig = self.signature(proxy);
            if seen_sigs.contains(&sig) {
                continue;
            }
            seen_sigs.insert(sig);

            let base_name = self.gcx.hir.contract(base_id).name.as_str();
            self.dcx()
                .err(format!(
                    "contract \"{}\" should be marked as abstract",
                    contract.name.as_str()
                ))
                .code(error_code!(3656))
                .span(contract.name.span)
                .span_note(
                    proxy.span(self.gcx),
                    format!(
                        "unimplemented {} \"{}\" defined in \"{}\"",
                        proxy.ast_node_name(self.gcx),
                        proxy.name_string(self.gcx),
                        base_name
                    ),
                )
                .emit();
        }
    }
}

fn capitalize(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        None => String::new(),
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capitalize() {
        assert_eq!(capitalize("function"), "Function");
        assert_eq!(capitalize("modifier"), "Modifier");
        assert_eq!(capitalize(""), "");
    }
}

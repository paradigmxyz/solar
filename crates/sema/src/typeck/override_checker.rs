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

use solar_ast::{FunctionKind, StateMutability, Visibility};
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::{Ident, Span, diagnostics::DiagCtxt, error_code};
use std::{cmp, collections::BTreeSet};

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

    fn is_function(self, gcx: Gcx<'_>) -> bool {
        match self {
            Self::Function(id) => !gcx.hir.function(id).kind.is_modifier(),
            Self::Variable(_) => false,
        }
    }

    fn is_modifier(self, gcx: Gcx<'_>) -> bool {
        match self {
            Self::Function(id) => gcx.hir.function(id).kind.is_modifier(),
            Self::Variable(_) => false,
        }
    }

    fn function_kind(self, gcx: Gcx<'_>) -> FunctionKind {
        match self {
            Self::Function(id) => gcx.hir.function(id).kind,
            Self::Variable(_) => FunctionKind::Function,
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
struct OverrideSignature<'gcx> {
    name: Option<Ident>,
    kind: FunctionKind,
    param_types: Option<&'gcx [Ty<'gcx>]>,
}

/// Override graph for detecting cut vertices in the inheritance hierarchy.
///
/// Node 0 represents the current contract (implicit).
/// Node 1 represents an artificial "top" node that all roots connect to.
/// Nodes 2+ represent actual functions/modifiers in the inheritance chain.
struct OverrideGraph<'gcx> {
    nodes: FxHashMap<OverrideProxy, usize>,
    node_inv: FxHashMap<usize, OverrideProxy>,
    edges: FxHashMap<usize, FxHashSet<usize>>,
    num_nodes: usize,
    gcx: Gcx<'gcx>,
    signature: OverrideSignature<'gcx>,
}

impl<'gcx> OverrideGraph<'gcx> {
    fn new(
        gcx: Gcx<'gcx>,
        signature: OverrideSignature<'gcx>,
        base_callables: &[OverrideProxy],
    ) -> Self {
        let mut graph = Self {
            nodes: FxHashMap::default(),
            node_inv: FxHashMap::default(),
            edges: FxHashMap::default(),
            num_nodes: 2,
            gcx,
            signature,
        };

        for &proxy in base_callables {
            let node = graph.visit(proxy);
            graph.add_edge(0, node);
        }

        graph
    }

    fn add_edge(&mut self, a: usize, b: usize) {
        self.edges.entry(a).or_default().insert(b);
        self.edges.entry(b).or_default().insert(a);
    }

    fn visit(&mut self, proxy: OverrideProxy) -> usize {
        if let Some(&node) = self.nodes.get(&proxy) {
            return node;
        }

        let current_node = self.num_nodes;
        self.num_nodes += 1;
        self.nodes.insert(proxy, current_node);
        self.node_inv.insert(current_node, proxy);

        let base_functions = self.find_base_functions(proxy);
        if base_functions.is_empty() {
            self.add_edge(current_node, 1);
        } else {
            for base in base_functions {
                let base_node = self.visit(base);
                self.add_edge(current_node, base_node);
            }
        }

        current_node
    }

    /// Find functions in parent contracts that this proxy overrides.
    fn find_base_functions(&self, proxy: OverrideProxy) -> Vec<OverrideProxy> {
        let Some(contract_id) = proxy.contract(self.gcx) else {
            return Vec::new();
        };
        let contract = self.gcx.hir.contract(contract_id);

        let mut result = Vec::new();

        for &base_id in contract.bases.iter() {
            if let Some(base_proxy) = self.find_matching_in_contract(base_id) {
                result.push(base_proxy);
            }
        }

        result
    }

    /// Find a function/variable in the given contract (or its ancestors) matching our signature.
    fn find_matching_in_contract(&self, contract_id: ContractId) -> Option<OverrideProxy> {
        let contract = self.gcx.hir.contract(contract_id);

        for f_id in contract.functions() {
            let f = self.gcx.hir.function(f_id);
            if f.kind.is_constructor() || f.name.is_none() {
                continue;
            }
            let proxy = OverrideProxy::Function(f_id);
            if self.matches_signature(proxy) {
                return Some(proxy);
            }
        }

        for v_id in contract.variables() {
            let v = self.gcx.hir.variable(v_id);
            if !v.is_public() {
                continue;
            }
            let proxy = OverrideProxy::Variable(v_id);
            if self.matches_signature(proxy) {
                return Some(proxy);
            }
        }

        for &ancestor_id in &contract.linearized_bases[1..] {
            if let Some(proxy) = self.find_matching_in_contract(ancestor_id) {
                return Some(proxy);
            }
        }

        None
    }

    fn matches_signature(&self, proxy: OverrideProxy) -> bool {
        let name = proxy.name(self.gcx);
        let kind = proxy.function_kind(self.gcx);

        if name != self.signature.name {
            return false;
        }
        if kind != self.signature.kind {
            return false;
        }

        if !kind.is_function() {
            return true;
        }

        let ty = proxy.ty(self.gcx);
        let ext_ty = ty.as_externally_callable_function(self.gcx);
        let param_types =
            if let TyKind::FnPtr(fn_ptr) = ext_ty.kind { Some(fn_ptr.parameters) } else { None };
        param_types == self.signature.param_types
    }
}

/// Detect cut vertices using Tarjan's algorithm.
/// Reference: <https://en.wikipedia.org/wiki/Biconnected_component#Pseudocode>
struct CutVertexFinder<'a, 'gcx> {
    graph: &'a OverrideGraph<'gcx>,
    visited: Vec<bool>,
    depths: Vec<i32>,
    low: Vec<i32>,
    parent: Vec<i32>,
    cut_vertices: FxHashSet<OverrideProxy>,
}

impl<'a, 'gcx> CutVertexFinder<'a, 'gcx> {
    fn find(graph: &'a OverrideGraph<'gcx>) -> FxHashSet<OverrideProxy> {
        let mut finder = Self {
            graph,
            visited: vec![false; graph.num_nodes],
            depths: vec![-1; graph.num_nodes],
            low: vec![-1; graph.num_nodes],
            parent: vec![-1; graph.num_nodes],
            cut_vertices: FxHashSet::default(),
        };
        finder.run(0, 0);
        finder.cut_vertices
    }

    fn run(&mut self, u: usize, depth: i32) {
        self.visited[u] = true;
        self.depths[u] = depth;
        self.low[u] = depth;

        let neighbors: Vec<usize> =
            self.graph.edges.get(&u).map(|s| s.iter().copied().collect()).unwrap_or_default();

        for v in neighbors {
            if !self.visited[v] {
                self.parent[v] = u as i32;
                self.run(v, depth + 1);

                if self.low[v] >= self.depths[u]
                    && self.parent[u] != -1
                    && let Some(&proxy) = self.graph.node_inv.get(&u)
                {
                    self.cut_vertices.insert(proxy);
                }
                self.low[u] = cmp::min(self.low[u], self.low[v]);
            } else if v as i32 != self.parent[u] {
                self.low[u] = cmp::min(self.low[u], self.depths[v]);
            }
        }
    }
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

    fn signature(&self, proxy: OverrideProxy) -> OverrideSignature<'gcx> {
        let name = proxy.name(self.gcx);
        let kind = proxy.function_kind(self.gcx);
        let param_types = if kind.is_function() {
            let ty = proxy.ty(self.gcx);
            let ext_ty = ty.as_externally_callable_function(self.gcx);
            if let TyKind::FnPtr(fn_ptr) = ext_ty.kind { Some(fn_ptr.parameters) } else { None }
        } else {
            None
        };
        OverrideSignature { name, kind, param_types }
    }

    fn inherited_functions(&self) -> FxHashMap<OverrideSignature<'gcx>, Vec<OverrideProxy>> {
        let contract = self.contract();
        let mut result: FxHashMap<OverrideSignature<'gcx>, Vec<OverrideProxy>> =
            FxHashMap::default();

        for &base_id in contract.bases.iter() {
            let base = self.gcx.hir.contract(base_id);

            // Collect functions/variables defined directly in this base.
            let mut defined_in_base: FxHashSet<OverrideSignature<'gcx>> = FxHashSet::default();

            for f_id in base.functions() {
                let f = self.gcx.hir.function(f_id);
                if f.kind.is_constructor() {
                    continue;
                }
                let proxy = OverrideProxy::Function(f_id);
                let sig = self.signature(proxy);

                result.entry(sig).or_default().push(proxy);
                defined_in_base.insert(sig);
            }

            for v_id in base.variables() {
                let v = self.gcx.hir.variable(v_id);
                if !v.is_public() {
                    continue;
                }
                let proxy = OverrideProxy::Variable(v_id);
                let sig = self.signature(proxy);

                result.entry(sig).or_default().push(proxy);
                defined_in_base.insert(sig);
            }

            // Get inherited functions from ancestors and add those NOT defined in base.
            // This matches solc's logic: inherited functions are added unless the direct
            // base defines a function with the same signature.
            let inherited = self.inherited_functions_for(base_id);
            for (sig, proxies) in inherited {
                if !defined_in_base.contains(&sig) {
                    result.entry(sig).or_default().extend(proxies);
                }
            }
        }

        result
    }

    /// Get inherited functions for a specific contract (used recursively).
    fn inherited_functions_for(
        &self,
        contract_id: ContractId,
    ) -> FxHashMap<OverrideSignature<'gcx>, Vec<OverrideProxy>> {
        let contract = self.gcx.hir.contract(contract_id);
        let mut result: FxHashMap<OverrideSignature<'gcx>, Vec<OverrideProxy>> =
            FxHashMap::default();

        for &base_id in contract.bases.iter() {
            let base = self.gcx.hir.contract(base_id);

            let mut defined_in_base: FxHashSet<OverrideSignature<'gcx>> = FxHashSet::default();

            for f_id in base.functions() {
                let f = self.gcx.hir.function(f_id);
                if f.kind.is_constructor() {
                    continue;
                }
                let proxy = OverrideProxy::Function(f_id);
                let sig = self.signature(proxy);

                result.entry(sig).or_default().push(proxy);
                defined_in_base.insert(sig);
            }

            for v_id in base.variables() {
                let v = self.gcx.hir.variable(v_id);
                if !v.is_public() {
                    continue;
                }
                let proxy = OverrideProxy::Variable(v_id);
                let sig = self.signature(proxy);

                result.entry(sig).or_default().push(proxy);
                defined_in_base.insert(sig);
            }

            let inherited = self.inherited_functions_for(base_id);
            for (sig, proxies) in inherited {
                if !defined_in_base.contains(&sig) {
                    result.entry(sig).or_default().extend(proxies);
                }
            }
        }

        result
    }

    fn check_illegal_overrides(&self) {
        let contract = self.contract();
        let inherited = self.inherited_functions();

        let inherited_modifiers: FxHashSet<Ident> =
            inherited.keys().filter(|s| s.kind.is_modifier()).filter_map(|s| s.name).collect();

        let inherited_functions: FxHashSet<Ident> =
            inherited.keys().filter(|s| !s.kind.is_modifier()).filter_map(|s| s.name).collect();

        for f_id in contract.functions() {
            let f = self.gcx.hir.function(f_id);
            // Skip constructors and getter functions (which are handled via variables).
            if f.kind.is_constructor() || f.is_getter() {
                continue;
            }
            let proxy = OverrideProxy::Function(f_id);

            if let Some(name) = proxy.name(self.gcx) {
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
            }

            let sig = self.signature(proxy);
            if let Some(bases) = inherited.get(&sig) {
                self.check_override_list(proxy, bases);
            } else if f.override_ {
                self.dcx()
                    .err(format!(
                        "{} has override specified but does not override anything",
                        capitalize(proxy.ast_node_name(self.gcx))
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
            let name = proxy.name(self.gcx).unwrap();

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
                    overriding.name(self.gcx).unwrap()
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

        if base.is_function(gcx) {
            let is_fallback = overriding.function_kind(gcx).is_fallback();
            let return_types_differ = if !is_fallback {
                self.check_return_type_compatibility(overriding, base)
            } else {
                false
            };
            if !return_types_differ && overriding.is_function(gcx) && !is_fallback {
                self.check_data_location_compatibility(overriding, base);
            }
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

    fn check_return_type_compatibility(
        &self,
        overriding: OverrideProxy,
        base: OverrideProxy,
    ) -> bool {
        let gcx = self.gcx;

        let overriding_ty = overriding.ty(gcx);
        let base_ty = base.ty(gcx);

        let overriding_ext = overriding_ty.as_externally_callable_function(gcx);
        let base_ext = base_ty.as_externally_callable_function(gcx);

        let (TyKind::FnPtr(overriding_fn), TyKind::FnPtr(base_fn)) =
            (overriding_ext.kind, base_ext.kind)
        else {
            return false;
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
            return true;
        }
        false
    }

    fn check_data_location_compatibility(&self, overriding: OverrideProxy, base: OverrideProxy) {
        let gcx = self.gcx;

        let base_vis = base.visibility(gcx);
        if base_vis == Visibility::External {
            return;
        }

        let Some(overriding_id) = overriding.function_id() else { return };
        let Some(base_id) = base.function_id() else { return };

        let overriding_f = gcx.hir.function(overriding_id);
        let base_f = gcx.hir.function(base_id);

        for (&over_param, &base_param) in
            overriding_f.parameters.iter().zip(base_f.parameters.iter())
        {
            let over_ty = gcx.type_of_item(over_param.into());
            let base_ty = gcx.type_of_item(base_param.into());

            if over_ty.peel_refs() == base_ty.peel_refs() && over_ty.loc() != base_ty.loc() {
                self.dcx()
                    .err(
                        "data locations of parameters have to be the same when overriding \
                         non-external functions, but they differ",
                    )
                    .code(error_code!(7723))
                    .span(overriding.span(gcx))
                    .span_note(
                        base.span(gcx),
                        format!("overridden {} is here", base.ast_node_name(gcx)),
                    )
                    .emit();
                return;
            }
        }

        for (&over_ret, &base_ret) in overriding_f.returns.iter().zip(base_f.returns.iter()) {
            let over_ty = gcx.type_of_item(over_ret.into());
            let base_ty = gcx.type_of_item(base_ret.into());

            if over_ty.peel_refs() == base_ty.peel_refs() && over_ty.loc() != base_ty.loc() {
                self.dcx()
                    .err(
                        "data locations of return variables have to be the same when overriding \
                         non-external functions, but they differ",
                    )
                    .code(error_code!(1443))
                    .span(overriding.span(gcx))
                    .span_note(
                        base.span(gcx),
                        format!("overridden {} is here", base.ast_node_name(gcx)),
                    )
                    .emit();
                return;
            }
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

        let own_functions: FxHashSet<OverrideSignature<'gcx>> = contract
            .functions()
            .filter_map(|f_id| {
                let f = self.gcx.hir.function(f_id);
                if f.kind.is_constructor() || f.name.is_none() {
                    return None;
                }
                Some(self.signature(OverrideProxy::Function(f_id)))
            })
            .collect();

        let own_variables: FxHashSet<OverrideSignature<'gcx>> = contract
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

            self.check_ambiguous_overrides_internal(*sig, bases);
        }
    }

    /// Check if base callables require an override in the current contract.
    ///
    /// Uses the cut vertex algorithm to determine if an intermediate base contract
    /// already provides an override that satisfies all conflicting definitions.
    fn check_ambiguous_overrides_internal(
        &self,
        sig: OverrideSignature<'gcx>,
        bases: &[OverrideProxy],
    ) {
        if bases.len() <= 1 {
            return;
        }

        let graph = OverrideGraph::new(self.gcx, sig, bases);
        let cut_vertices = CutVertexFinder::find(&graph);

        let mut remaining: FxHashSet<OverrideProxy> = bases.iter().copied().collect();

        for cut_vertex in &cut_vertices {
            let base_functions = graph.find_base_functions(*cut_vertex);
            let mut to_remove: Vec<OverrideProxy> = Vec::new();

            let mut stack = base_functions;
            while let Some(base) = stack.pop() {
                to_remove.push(base);
                stack.extend(graph.find_base_functions(base));
            }

            for proxy in to_remove {
                remaining.remove(&proxy);
            }

            if !cut_vertex.is_implemented(self.gcx) {
                remaining.remove(cut_vertex);
            }
        }

        if remaining.len() <= 1 {
            return;
        }

        let unique_contracts: BTreeSet<ContractId> =
            remaining.iter().filter_map(|p| p.contract(self.gcx)).collect();

        if unique_contracts.len() <= 1 {
            return;
        }

        let base_names: Vec<_> = unique_contracts
            .iter()
            .map(|&c| format!("\"{}\"", self.gcx.hir.contract(c).name.as_str()))
            .collect();

        let kind = if sig.kind.is_modifier() { "modifier" } else { "function" };

        let has_variable = remaining.iter().any(|p| p.is_variable());
        let name_display = sig.name.map(|n| n.to_string()).unwrap_or_else(|| sig.kind.to_string());
        let mut msg = format!(
            "derived contract must override {} \"{}\". Two or more base classes define {} with same {}",
            kind,
            name_display,
            kind,
            if sig.kind.is_modifier() { "name" } else { "name and parameter types" }
        );

        if has_variable {
            msg.push_str(
                ". Since one of the bases defines a public state variable which cannot be \
                 overridden, you have to change the inheritance layout or the names of the functions",
            );
        }

        self.dcx()
            .err(msg)
            .code(error_code!(6480))
            .span(self.contract().name.span)
            .note(format!("defined in: {}", base_names.join(", ")))
            .emit();
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

        let mut seen_sigs: FxHashSet<OverrideSignature<'gcx>> = FxHashSet::default();
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
                        proxy.name(self.gcx).unwrap(),
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

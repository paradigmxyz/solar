//! Contract-level lowering and function discovery.

use super::{function, storage::StorageLayout, types::TypeLowerer};
use alloy_primitives::Bytes;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::Ident;
use solar_sema::{
    Gcx,
    hir::{self, ContractId},
};

use crate::mir::{Function, FunctionAttributes, FunctionBuilder, Module};

/// Builds a typed MIR module from one HIR contract.
pub(super) fn lower(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    _child_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    let contract = gcx.hir.contract(contract_id);
    let mut module = Module::new(contract.name);
    let storage = StorageLayout::for_contract(gcx, contract_id);

    let mut function_ids = Vec::new();
    let mut seen_selectors = FxHashSet::default();
    let mut has_fallback = false;
    let mut has_receive = false;
    for &base in contract.linearized_bases {
        for function_id in gcx.hir.contract(base).all_functions() {
            let function = gcx.hir.function(function_id);
            match function.kind {
                hir::FunctionKind::Constructor => {
                    if base == contract_id {
                        function_ids.push(function_id);
                    }
                }
                hir::FunctionKind::Fallback => {
                    if !has_fallback {
                        has_fallback = true;
                        function_ids.push(function_id);
                    }
                }
                hir::FunctionKind::Receive => {
                    if !has_receive {
                        has_receive = true;
                        function_ids.push(function_id);
                    }
                }
                hir::FunctionKind::Function | hir::FunctionKind::Modifier => {
                    if function.kind == hir::FunctionKind::Modifier
                        || (base != contract_id && function.visibility == hir::Visibility::Private)
                    {
                        continue;
                    }
                    if matches!(
                        function.visibility,
                        hir::Visibility::External | hir::Visibility::Public
                    ) {
                        if seen_selectors.insert(gcx.function_selector(function_id)) {
                            function_ids.push(function_id);
                        }
                    } else {
                        function_ids.push(function_id);
                    }
                }
            }
        }
    }
    let mut seen_ids = FxHashSet::default();
    let function_ids =
        function_ids.into_iter().filter(|id| seen_ids.insert(*id)).collect::<Vec<_>>();
    let mut mir_ids = FxHashMap::default();
    for &function_id in &function_ids {
        let function = gcx.hir.function(function_id);
        let mut declaration = declaration(gcx, function_id, function);
        {
            let mut builder = FunctionBuilder::new(&mut declaration);
            for &param in function.parameters {
                builder.add_param(TypeLowerer::mir_type(gcx.type_of_item(param.into())));
            }
            for &ret in function.returns {
                builder.add_return(TypeLowerer::mir_type(gcx.type_of_item(ret.into())));
            }
        }
        let mir_id = module.add_function(declaration);
        mir_ids.insert(function_id, mir_id);
    }

    for function_id in function_ids {
        let mir_id = mir_ids[&function_id];
        let name = module.function(mir_id).name;
        let Some(mut mir) = function::lower(gcx, &mut module, &storage, function_id, &mir_ids)
        else {
            FunctionBuilder::new(module.function_mut(mir_id)).invalid();
            continue;
        };
        mir.name = name;
        *module.function_mut(mir_id) = mir;
    }

    let has_state_initializers = contract.linearized_bases.iter().rev().any(|&base| {
        gcx.hir.contract(base).variables().any(|id| gcx.hir.variable(id).initializer.is_some())
    });
    let has_implicit_base_constructors =
        contract.linearized_bases.iter().skip(1).any(|&base| gcx.hir.contract(base).ctor.is_some())
            || contract.linearized_bases_args.iter().any(Option::is_some);
    if contract.ctor.is_none() && (has_state_initializers || has_implicit_base_constructors) {
        let mir_id = module.add_function(Function::new(solar_interface::Ident::with_dummy_span(
            solar_interface::kw::Constructor,
        )));
        let Some(mut mir) =
            function::lower_synthetic_constructor(gcx, &storage, contract_id, &mir_ids)
        else {
            FunctionBuilder::new(module.function_mut(mir_id)).invalid();
            return module;
        };
        mir.name = module.function(mir_id).name;
        *module.function_mut(mir_id) = mir;
    }

    if contract.kind == hir::ContractKind::Interface {
        module.is_interface = true;
    }
    module
}

/// Creates the MIR declaration for a HIR function.
pub(super) fn declaration(
    gcx: Gcx<'_>,
    function_id: hir::FunctionId,
    function: &hir::Function<'_>,
) -> Function {
    let name =
        function.name.unwrap_or_else(|| Ident::with_dummy_span(solar_interface::sym::_anonymous));
    let mut mir = Function::new(name);
    mir.attributes = FunctionAttributes {
        visibility: function.visibility,
        state_mutability: function.state_mutability,
        is_constructor: function.kind == hir::FunctionKind::Constructor,
        is_fallback: function.kind == hir::FunctionKind::Fallback,
        is_receive: function.kind == hir::FunctionKind::Receive,
        is_dispatch_entry: false,
        no_inline: false,
    };

    if function.kind == hir::FunctionKind::Function
        && matches!(function.visibility, hir::Visibility::Public | hir::Visibility::External)
    {
        mir.selector = Some(gcx.function_selector(function_id).0);
    }

    mir
}

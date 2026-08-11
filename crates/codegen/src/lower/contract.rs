//! Contract-level lowering and function discovery.

use super::{function, storage::StorageLayout, types::TypeLowerer};
use alloy_primitives::Bytes;
use solar_data_structures::map::{FxHashMap, FxHashSet};
use solar_interface::Ident;
use solar_sema::{
    Gcx,
    hir::{self, ContractId},
    ty::TyKind,
};

use crate::mir::{Function, FunctionAttributes, FunctionBuilder, Module};

/// Builds a typed MIR module from one HIR contract.
pub(super) fn lower(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Bytes>,
    child_runtime_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    let contract = gcx.hir.contract(contract_id);
    let mut module = Module::new(contract.name);
    let storage = StorageLayout::for_contract(gcx, contract_id);
    let mut immutable_ids = FxHashMap::default();
    for &base in contract.linearized_bases.iter().rev() {
        for id in gcx.hir.contract(base).variables() {
            let variable = gcx.hir.variable(id);
            if !variable.is_state_variable() || !variable.is_immutable() {
                continue;
            }
            let Some(name) = variable.name else { continue };
            let mir_id = module.add_immutable(
                name,
                TypeLowerer::mir_type(gcx.type_of_item(id.into())),
                Some(id),
            );
            immutable_ids.insert(id, mir_id);
        }
    }

    let mut function_ids = Vec::new();
    let mut seen_selectors = FxHashSet::default();
    let reachable = gcx.contract_reachable_functions(contract_id);
    let prune_unreachable = !gcx.sess.opts.unstable.codegen_all_functions;
    let mut invalid_event_topics = FxHashSet::default();
    let mut has_fallback = false;
    let mut has_receive = false;
    for &base in contract.linearized_bases {
        for function_id in gcx.hir.contract(base).all_functions() {
            if prune_unreachable && !reachable.contains(function_id) {
                continue;
            }
            let function = gcx.hir.function(function_id);
            match function.kind {
                hir::FunctionKind::Constructor => {
                    if base == contract_id {
                        function_ids.push((function_id, false));
                    }
                }
                hir::FunctionKind::Fallback => {
                    if !has_fallback {
                        has_fallback = true;
                        function_ids.push((function_id, false));
                    }
                }
                hir::FunctionKind::Receive => {
                    if !has_receive {
                        has_receive = true;
                        function_ids.push((function_id, false));
                    }
                }
                hir::FunctionKind::Function | hir::FunctionKind::Modifier => {
                    if function.kind == hir::FunctionKind::Modifier
                        || (function.visibility == hir::Visibility::Private
                            && (base != contract_id || contract.kind != hir::ContractKind::Library))
                    {
                        continue;
                    }
                    if matches!(
                        function.visibility,
                        hir::Visibility::External | hir::Visibility::Public
                    ) {
                        let expose_selector =
                            seen_selectors.insert(gcx.function_selector(function_id));
                        if expose_selector
                            || (function.visibility == hir::Visibility::Public
                                && function.body.is_some())
                        {
                            function_ids.push((function_id, expose_selector));
                        }
                    } else {
                        function_ids.push((function_id, false));
                    }
                }
            }
        }
    }
    for function_id in reachable.iter() {
        let function = gcx.hir.function(function_id);
        let library_function = function
            .contract
            .is_some_and(|id| gcx.hir.contract(id).kind == hir::ContractKind::Library)
            && matches!(function.visibility, hir::Visibility::Public | hir::Visibility::External)
            && function.body.is_some();
        if function.kind == hir::FunctionKind::Function
            && (matches!(function.visibility, hir::Visibility::Internal | hir::Visibility::Private)
                || library_function)
        {
            function_ids.push((function_id, false));
        }
    }
    for library_id in gcx.hir.contract_ids() {
        let library = gcx.hir.contract(library_id);
        if library.kind != hir::ContractKind::Library {
            continue;
        }
        let source = gcx.hir.source(library.source).file.name.display().to_string();
        let linked = gcx.sess.opts.libraries.iter().any(|spec| {
            spec.name == library.name.as_str_in(gcx.sess)
                && spec.source.as_ref().is_none_or(|path| source.ends_with(path))
        });
        if linked {
            continue;
        }
        for function_id in library.functions() {
            if library_id != contract_id && !reachable.contains(function_id) {
                continue;
            }
            let function = gcx.hir.function(function_id);
            if matches!(function.visibility, hir::Visibility::Public | hir::Visibility::External)
                && function.body.is_some()
            {
                function_ids.push((function_id, false));
            }
        }
    }
    let mut seen_ids = FxHashSet::default();
    let function_ids =
        function_ids.into_iter().filter(|(id, _)| seen_ids.insert(*id)).collect::<Vec<_>>();
    let mut mir_ids = FxHashMap::default();
    let mut pointer_registry = function::InternalFunctionPointerRegistry::default();
    let mut storage_bytes_helper = None;
    let mut visiting_storage_structs = FxHashSet::default();
    let share_storage_bytes = contract
        .linearized_bases
        .iter()
        .flat_map(|&base| gcx.hir.contract(base).variables())
        .filter(|&id| gcx.hir.variable(id).is_state_variable())
        .map(|id| {
            storage_bytes_count(gcx.type_of_item(id.into()), gcx, &mut visiting_storage_structs)
        })
        .sum::<usize>()
        >= 2;
    for &(function_id, expose_selector) in &function_ids {
        let function = gcx.hir.function(function_id);
        let mut declaration = declaration(gcx, function_id, function);
        if !expose_selector {
            declaration.selector = None;
        }
        {
            let mut builder = FunctionBuilder::new(&mut declaration);
            for &param in function.parameters {
                builder.add_param(TypeLowerer::mir_type(gcx.type_of_item(param.into())));
            }
            for &ret in function.returns {
                builder.add_return(TypeLowerer::mir_return_type(gcx.type_of_item(ret.into())));
            }
        }
        let mir_id = module.add_function(declaration);
        mir_ids.insert(function_id, mir_id);
    }

    for (function_id, expose_selector) in function_ids {
        let mir_id = mir_ids[&function_id];
        let name = module.function(mir_id).name;
        let context = function::LoweringContext {
            gcx,
            module: &mut module,
            storage: &storage,
            contract_id,
            function_ids: &mir_ids,
            immutable_ids: &immutable_ids,
            child_bytecodes,
            child_runtime_bytecodes,
            invalid_event_topics: &mut invalid_event_topics,
            pointer_registry: &mut pointer_registry,
            storage_bytes_helper: &mut storage_bytes_helper,
            share_storage_bytes,
        };
        let Some(mut mir) = function::lower(context, function_id, expose_selector) else {
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
        let context = function::LoweringContext {
            gcx,
            module: &mut module,
            storage: &storage,
            contract_id,
            function_ids: &mir_ids,
            immutable_ids: &immutable_ids,
            child_bytecodes,
            child_runtime_bytecodes,
            invalid_event_topics: &mut invalid_event_topics,
            pointer_registry: &mut pointer_registry,
            storage_bytes_helper: &mut storage_bytes_helper,
            share_storage_bytes,
        };
        let Some(mut mir) = function::lower_synthetic_constructor(context, contract_id) else {
            FunctionBuilder::new(module.function_mut(mir_id)).invalid();
            return module;
        };
        mir.name = module.function(mir_id).name;
        *module.function_mut(mir_id) = mir;
    }

    function::generate_internal_function_pointer_dispatchers(
        gcx,
        &mut module,
        &mir_ids,
        &pointer_registry,
    );

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
        is_function_pointer_dispatcher: false,
    };

    if function.kind == hir::FunctionKind::Function
        && matches!(function.visibility, hir::Visibility::Public | hir::Visibility::External)
    {
        mir.selector = Some(gcx.function_selector(function_id).0);
    }

    mir
}

fn storage_bytes_count<'gcx>(
    ty: solar_sema::ty::Ty<'gcx>,
    gcx: Gcx<'gcx>,
    visiting: &mut FxHashSet<hir::StructId>,
) -> usize {
    match ty.peel_refs().kind {
        TyKind::Elementary(hir::ElementaryType::Bytes | hir::ElementaryType::String) => 1,
        TyKind::Array(element, _) | TyKind::DynArray(element) => {
            storage_bytes_count(element, gcx, visiting)
        }
        TyKind::Mapping(_, value) => storage_bytes_count(value, gcx, visiting),
        TyKind::Struct(id) if visiting.insert(id) => {
            let count = gcx
                .hir
                .strukt(id)
                .fields
                .iter()
                .map(|&field| storage_bytes_count(gcx.type_of_item(field.into()), gcx, visiting))
                .sum();
            visiting.remove(&id);
            count
        }
        _ => 0,
    }
}

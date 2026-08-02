//! Contract-level lowering and function discovery.

use super::{function, storage::StorageLayout};
use alloy_primitives::Bytes;
use solar_data_structures::map::FxHashMap;
use solar_interface::Ident;
use solar_sema::{
    Gcx,
    hir::{self, ContractId},
};

use crate::mir::{Function, FunctionAttributes, Module};

/// Builds a typed MIR module from one HIR contract.
pub(super) fn lower(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    _child_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    let contract = gcx.hir.contract(contract_id);
    let mut module = Module::new(contract.name);
    let storage = StorageLayout::for_contract(gcx, contract_id);

    for function_id in contract.all_functions() {
        let function = gcx.hir.function(function_id);
        if function.kind == hir::FunctionKind::Modifier {
            continue;
        }
        let Some(mir) = function::lower(gcx, &mut module, &storage, function_id) else { continue };
        module.add_function(mir);
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

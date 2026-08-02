//! HIR to MIR lowering.
//!
//! This module transforms the high-level IR from solar-sema into MIR.

/// Lowers a contract from HIR to MIR.
pub fn lower_contract(gcx: Gcx<'_>, contract_id: ContractId) -> Module {
    lower_contract_with_bytecodes(gcx, contract_id, &FxHashMap::default())
}

/// Lowers a contract from HIR to MIR with pre-compiled bytecodes available for `new` expressions.
#[tracing::instrument(name = "mir_lower_contract", level = "debug", skip_all, fields(?contract_id))]
pub fn lower_contract_with_bytecodes(
    gcx: Gcx<'_>,
    contract_id: ContractId,
    child_bytecodes: &FxHashMap<ContractId, Bytes>,
) -> Module {
    let contract = gcx.hir.contract(contract_id);
    let mut lowerer = Lowerer::new(gcx, contract.name);

    // Register all child contract bytecodes
    for (&child_id, bytecode) in child_bytecodes {
        lowerer.register_contract_bytecode(child_id, bytecode.clone());
    }

    lowerer.lower_contract(contract_id);
    lowerer.finish()
}

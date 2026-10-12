//! EVM backend IR.
//!
//! The IR itself, its text format, and its verifier live in `solar_evmir`, re-exported here. This
//! module adds what compiles it: the EVM IR passes, which run on the program stack scheduling
//! emits, and its lowering to bytecode through the assembler.

use crate::{
    backend::{
        assembler,
        evm::op::{self, StackOp},
    },
    link::{EmbeddedBytecodes, LibraryTable, RelocatableBytecode},
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub use solar_evmir::ir::*;

pub(in crate::backend) mod builder;
mod passes;
pub(crate) use passes::compact_pushes;

pub(crate) use passes::compact_pushes::immediate_materialization_cost;
pub use passes::{
    ALL_PASSES, EvmPass, lookup_pass, pipeline_label, run_passes, run_passes_no_validate,
    run_pipeline,
};
pub(in crate::backend) use passes::{
    compact_pushes::{
        ImmediateMaterialization, ImmediateMaterializationOp, immediate_materialization_len,
    },
    data::pack_linked_data,
    legalize_shifts,
};

/// Validates the target-independent invariants of an EVM IR module.
pub fn validate(gcx: Gcx<'_>, module: &Module) {
    verifier(gcx).verify_module(module);
}

/// Returns a verifier for the session's target that reports to its diagnostics.
pub(in crate::backend) fn verifier(gcx: Gcx<'_>) -> verify::Verifier<'_> {
    verify::Verifier::for_evm_version(gcx.dcx(), gcx.sess.opts.evm_version)
}

/// Returns whether data references in `module` can observe entry boundaries or order.
pub(in crate::backend) fn data_layout_is_observable(module: &Module) -> bool {
    passes::data::data_layout_is_observable(module)
}

/// Lowers an EVM IR module to bytecode, running the EVM IR pipeline and retaining unresolved
/// library addresses.
pub fn into_bytecode(gcx: Gcx<'_>, module: Module) -> solar_interface::Result<RelocatableBytecode> {
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    let result = assembler.assemble_with_evm_ir(true);
    gcx.dcx().has_errors()?;
    Ok(RelocatableBytecode {
        libraries: result.evm_ir.expect("EVM IR capture requested").libraries,
        bytes: result.bytecode.into(),
        relocations: result.library_relocations,
    })
}

/// Creation and runtime bytecode assembled from one EVM IR program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DeployableBytecode {
    /// Creation bytecode, which returns the runtime bytecode.
    pub deployment: Vec<u8>,
    /// Runtime bytecode.
    pub runtime: Vec<u8>,
}

/// Assembles a program, whose EVM IR pipeline already ran, as runtime bytecode, together with
/// creation bytecode that deploys it unchanged.
///
/// Nothing links library addresses into the result, so a program that pushes one is an error;
/// immutables keep their zero placeholders.
pub fn into_deployable_bytecode(
    gcx: Gcx<'_>,
    module: Module,
) -> solar_interface::Result<DeployableBytecode> {
    let name = module.name;
    let runtime = assemble_legalized(gcx, module)?;
    let mut deployment = Module::new(name);
    deployment.code_follows = true;
    let runtime_data = deployment.data.push(Data::new(runtime.clone().into(), None));
    let mut block = Block::new(0);
    // push len(runtime)
    // dup 1
    // push_data runtime
    // push 0
    // codecopy
    // push 0
    // return
    block.instructions = vec![
        Instruction::push_value(U256::from(runtime.len())),
        Instruction::stack_op(StackOp::Dup(1)),
        Instruction::push_data(DataRef::new(runtime_data, 0)),
        Instruction::push_value(U256::ZERO),
        Instruction::opcode(op::CODECOPY),
        Instruction::push_value(U256::ZERO),
    ];
    block.terminator = Some(Terminator::new(TerminatorKind::Op(op::RETURN)));
    deployment.add_block(block);
    let deployment = assemble_legalized(gcx, deployment)?;
    Ok(DeployableBytecode { deployment, runtime })
}

/// Assembles `module` without running the EVM IR pipeline, only legalizing it for the target.
/// Fails when it pushes a library address, which nothing links.
fn assemble_legalized(gcx: Gcx<'_>, module: Module) -> solar_interface::Result<Vec<u8>> {
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    assembler.legalize();
    let assembled = assembler.assemble_linked(
        &EmbeddedBytecodes::default(),
        &mut LibraryTable::default(),
        false,
        false,
    );
    gcx.dcx().has_errors()?;
    if !assembled.library_relocations.is_empty() {
        let message = "EVM IR input that pushes library addresses has no bytecode to emit";
        return Err(gcx.dcx().err(message).emit());
    }
    Ok(assembled.bytecode)
}

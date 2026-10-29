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
    target::Target,
};
use alloy_primitives::U256;
use solar_sema::Gcx;

pub use solar_evmir::ir::*;

pub(in crate::backend) mod builder;
mod passes;
pub(in crate::backend::evm) use passes::SharedSchedules;
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
    legalize_revert,
};

/// The cost model of a module's code.
pub(crate) trait ModuleTarget {
    /// The cost model of this module's code: the creation model for creation code, which the
    /// runtime artifact follows.
    fn target(&self, gcx: Gcx<'_>) -> Target;
}

impl ModuleTarget for Module {
    fn target(&self, gcx: Gcx<'_>) -> Target {
        Target::new(gcx).priced_for(self.code_follows)
    }
}

/// The cost model of a block's code.
pub(crate) trait BlockTarget {
    /// The cost model of this block: the creation model for creation code, which the runtime
    /// artifact follows (`code_follows`), unless the block may run more than once.
    fn target(&self, gcx: Gcx<'_>, code_follows: bool) -> Target;
}

impl BlockTarget for Block {
    fn target(&self, gcx: Gcx<'_>, code_follows: bool) -> Target {
        Target::new(gcx).priced_for(code_follows && self.runs_once())
    }
}

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

/// Assembles a program, whose EVM IR pipeline already ran, as bytecode that retains its unresolved
/// library addresses.
pub fn into_bytecode(gcx: Gcx<'_>, module: Module) -> solar_interface::Result<RelocatableBytecode> {
    let assembled = assemble_legalized(gcx, module)?;
    Ok(RelocatableBytecode {
        libraries: assembled.evm_ir.expect("EVM IR capture requested").libraries,
        bytes: assembled.bytecode.into(),
        relocations: assembled.library_relocations,
        trailers: assembled.trailers,
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
/// Nothing links library addresses into the result, and the creation code sets no immutables, so
/// a program that pushes either is an error. Bytecode past the target's size limits gets the same
/// warnings as a Solidity contract's.
pub fn into_deployable_bytecode(
    gcx: Gcx<'_>,
    module: Module,
) -> solar_interface::Result<DeployableBytecode> {
    let name = module.name;
    let runtime = deployable(gcx, assemble_legalized(gcx, module)?)?;
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
    let deployment = deployable(gcx, assemble_legalized(gcx, deployment)?)?;
    crate::contract::warn_code_size_limits(gcx, None, deployment.len(), runtime.len());
    Ok(DeployableBytecode { deployment, runtime })
}

/// Assembles `module` without running the EVM IR pipeline, only legalizing it for the target.
fn assemble_legalized(
    gcx: Gcx<'_>,
    module: Module,
) -> solar_interface::Result<assembler::AssembledCode> {
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    assembler.legalize();
    let assembled = assembler.assemble_linked(
        &EmbeddedBytecodes::default(),
        &mut LibraryTable::default(),
        true,
        false,
    );
    gcx.dcx().has_errors()?;
    Ok(assembled)
}

/// Returns the bytecode of an assembled program that deploys as it is. Fails when it pushes a
/// library address, which nothing links, or an immutable, which nothing sets.
fn deployable(
    gcx: Gcx<'_>,
    assembled: assembler::AssembledCode,
) -> solar_interface::Result<Vec<u8>> {
    if !assembled.library_relocations.is_empty() {
        let message = "EVM IR input that pushes library addresses has no bytecode to emit";
        return Err(gcx.dcx().err(message).emit());
    }
    if !assembled.immutable_refs.is_empty() {
        let message = "EVM IR input that pushes immutables has no bytecode to emit";
        let note = "its creation code only copies the runtime, so every immutable would read zero";
        return Err(gcx.dcx().err(message).note(note).emit());
    }
    Ok(assembled.bytecode)
}

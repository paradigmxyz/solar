//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: MIR-to-EVM stack scheduling for DUP/SWAP generation

mod codegen;
pub use codegen::{EvmArtifact, EvmCodegen};

mod disasm;
pub use disasm::{disassemble, disassemble_standard_json};

mod layout;

pub mod ir;

pub(crate) mod op;

pub(crate) mod assembler;

pub(crate) mod stack;

/// Generates bytecode from finalized EVM IR through the backend pipeline.
pub fn generate_evm_ir_bytecode(
    gcx: solar_sema::Gcx<'_>,
    module: ir::Module,
) -> solar_interface::Result<Vec<u8>> {
    ir::verify::validate(gcx, &module, ir::verify::Validation::Structural);
    gcx.dcx().has_errors()?;
    ir::verify::validate(gcx, &module, ir::verify::Validation::StackOps);
    gcx.dcx().has_errors()?;
    ir::verify::validate(gcx, &module, ir::verify::Validation::OpcodesBeforeLegalization);
    gcx.dcx().has_errors()?;
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    let result = assembler.assemble_with_evm_ir(true);
    gcx.dcx().has_errors()?;
    Ok(result.bytecode)
}

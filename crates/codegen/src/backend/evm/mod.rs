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
pub use disasm::disassemble;

mod layout;

pub mod ir;

pub(crate) mod op;

pub(crate) mod assembler;

pub(crate) mod stack;

/// Assembles an EVM IR module.
pub fn assemble_evm_ir(
    gcx: solar_sema::Gcx<'_>,
    module: ir::Module,
) -> solar_interface::Result<Vec<u8>> {
    Ok(assembler::Assembler::assemble_evm_ir(gcx, module)?.bytecode)
}

//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: MIR-to-EVM stack scheduling for DUP/SWAP generation

use alloy_primitives::U256;
use solar_config::EvmVersion;

/// Number of bytes in an EVM word.
pub(super) const EVM_WORD_BYTES: usize = 32;

/// EIP-170 deployed bytecode size limit.
pub(crate) const EIP170_RUNTIME_CODE_SIZE_LIMIT: usize = 24_576;

/// Returns the encoded length of a minimally sized PUSH for an EVM version.
pub(super) fn push_len(evm_version: EvmVersion, value: U256) -> usize {
    if value.is_zero() && evm_version.has_push0() { 1 } else { value.byte_len().max(1) + 1 }
}

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
    ir::validate_for_evm_version(gcx.dcx(), &module, gcx.sess.opts.evm_version);
    gcx.dcx().has_errors()?;
    ir::validate_evm_version_before_legalization(gcx.dcx(), &module, gcx.sess.opts.evm_version);
    gcx.dcx().has_errors()?;
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    let result = assembler.assemble_with_evm_ir(true);
    ir::validate(gcx.dcx(), result.evm_ir.as_ref().expect("requested EVM IR should be captured"));
    gcx.dcx().has_errors()?;
    Ok(result.bytecode)
}

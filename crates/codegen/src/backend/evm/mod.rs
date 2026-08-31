//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: MIR-to-EVM stack scheduling for DUP/SWAP generation

use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};

/// Number of bytes in an EVM word.
pub(super) const EVM_WORD_BYTES: usize = 32;

/// Returns the encoded length of a minimally sized PUSH for an EVM version.
pub(crate) fn push_len(evm_version: EvmVersion, value: U256) -> usize {
    if value.is_zero() && evm_version.has_push0() { 1 } else { value.byte_len().max(1) + 1 }
}

/// Returns the encoded size and runtime gas of one program-data copy site.
pub(crate) fn data_copy_cost(evm_version: EvmVersion, size: usize) -> (usize, usize) {
    (push_len(evm_version, U256::from(size)) + 6, data_copy_gas(size))
}

/// Returns the runtime gas of one program-data copy site.
pub(crate) fn data_copy_gas(size: usize) -> usize {
    12 + 3 * size.div_ceil(EVM_WORD_BYTES)
}

/// Returns whether a program-data copy improves the selected objective.
pub(crate) fn data_copy_is_profitable(
    optimization: OptimizationMode,
    runtime_gas_saving: i128,
    byte_saving: i128,
) -> bool {
    if optimization.is_gas() { runtime_gas_saving > 0 && byte_saving >= 0 } else { byte_saving > 0 }
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

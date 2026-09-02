//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: MIR-to-EVM stack scheduling for DUP/SWAP generation

use crate::target::Target;
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};

/// Returns the encoded size and runtime gas of one program-data copy site.
pub(crate) fn data_copy_cost(evm_version: EvmVersion, size: usize) -> (usize, usize) {
    (op::push_len(evm_version, U256::from(size)) + 6, data_copy_gas(evm_version, size))
}

/// Returns the runtime gas of one program-data copy site.
pub(crate) fn data_copy_gas(evm_version: EvmVersion, size: usize) -> usize {
    Target::with(evm_version, OptimizationMode::Gas, Target::DEFAULT_EXPECTED_EXECUTIONS)
        .data_copy_gas(size) as usize
}

/// Returns whether a program-data copy improves the selected objective.
pub(crate) fn data_copy_is_profitable(
    optimization: OptimizationMode,
    runtime_gas_saving: i128,
    byte_saving: i128,
) -> bool {
    Target::with(EvmVersion::default(), optimization, Target::DEFAULT_EXPECTED_EXECUTIONS)
        .improves(runtime_gas_saving, byte_saving)
}

mod codegen;
pub(crate) use codegen::select;
pub use codegen::{EvmArtifact, EvmCodegen};

mod debug_info;
pub use debug_info::{
    DebugFunction, DebugFunctionExit, DebugInstruction, DebugSpans, MAX_DEBUG_SPANS,
};

mod disasm;
pub use disasm::{disassemble, disassemble_standard_json};

mod layout;

pub mod ir;

pub(crate) mod op;

pub(crate) mod assembler;

pub(crate) mod stack;

/// Returns the canonical mnemonic for an EVM opcode.
#[must_use]
pub const fn opcode_mnemonic(opcode: u8) -> Option<&'static str> {
    op::mnemonic(opcode)
}

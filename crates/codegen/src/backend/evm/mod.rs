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

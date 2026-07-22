//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: MIR-to-EVM stack scheduling for DUP/SWAP generation

/// Number of bytes in an EVM word.
pub(super) const EVM_WORD_BYTES: usize = 32;

mod codegen;
pub use codegen::{EvmArtifact, EvmCodegen, EvmCodegenConfig};

pub mod ir;

pub(crate) mod op;

pub(crate) mod assembler;

pub(crate) mod stack;

#[cfg(test)]
pub(crate) mod test_utils;

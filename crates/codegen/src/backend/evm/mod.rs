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

mod layout;

pub mod ir;

pub(crate) mod op;

pub(crate) mod assembler;

pub(crate) mod stack;

mod switch;

#[cfg(test)]
pub(crate) mod test_utils;

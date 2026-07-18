//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: Stack scheduling for DUP/SWAP generation

mod codegen;
pub use codegen::{EvmArtifact, EvmCodegen, EvmCodegenConfig};

pub mod ir;
mod stack_schedule;

pub mod opcode;

pub mod assembler;
pub use assembler::{AssembledCode, Assembler, AssemblerConfig, Label};

pub mod stack;
pub use stack::{SpillManager, SpillSlot, StackModel, StackScheduler};

#[cfg(test)]
pub(crate) mod test_utils;

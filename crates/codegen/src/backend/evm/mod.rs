//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Two-pass assembler with label resolution and instruction peepholes
//! - `stack`: Stack scheduling for DUP/SWAP generation

mod codegen;
pub use codegen::{EvmArtifact, EvmCodegen, EvmCodegenConfig};

pub mod ir;
mod ir_stack_schedule;

pub mod assembler;
pub use assembler::{AssembledCode, Assembler, AssemblerConfig, Label};

mod peephole;

pub mod stack;
pub use stack::{SpillManager, SpillSlot, StackModel, StackScheduler};

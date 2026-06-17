//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `Assembler`: Two-pass assembler with label resolution and instruction peepholes
//! - `stack`: Stack scheduling for DUP/SWAP generation

mod codegen;
pub use codegen::{EvmArtifact, EvmCodegen, EvmCodegenConfig};

pub mod assembler;
pub use assembler::{AssembledCode, Assembler, AssemblerConfig, Label};

pub mod peephole;
pub use peephole::PeepholeOptimizer;

pub mod stack;
pub use stack::{SpillManager, SpillSlot, StackModel, StackScheduler};

//! Code generation from MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `Assembler`: Two-pass assembler with label resolution
//! - `PeepholeOptimizer`: Bytecode-level pattern optimizations (not yet integrated)
//! - `stack`: Stack scheduling for DUP/SWAP generation

mod evm;
pub use evm::EvmCodegen;

pub mod assembler;
pub use assembler::{AssembledCode, Assembler, Label};

pub mod peephole;
pub use peephole::PeepholeOptimizer;

pub mod stack;
pub use stack::{SpillManager, SpillSlot, StackModel, StackScheduler};

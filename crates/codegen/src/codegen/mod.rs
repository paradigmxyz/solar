//! Code generation from MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `Assembler`: Two-pass assembler with label resolution
//! - `stack`: Stack scheduling for DUP/SWAP generation

mod evm;
pub use evm::EvmCodegen;

pub mod assembler;
pub use assembler::{AssembledCode, Assembler, Label};

pub mod stack;
pub use stack::{SpillManager, SpillSlot, StackModel, StackScheduler};

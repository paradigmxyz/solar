//! Mid-level Intermediate Representation (MIR).
//!
//! MIR is an SSA-form IR that sits between HIR and EVM bytecode.

use solar_data_structures::newtype_index;

mod types;
pub use types::MirType;

mod value;
pub use value::{Immediate, Value};

mod inst;
pub use inst::{InstKind, Instruction};

mod block;
pub use block::{BasicBlock, Terminator};

mod function;
pub use function::{Function, FunctionAttributes};

mod module;
pub use module::{DataSegment, Module, StorageSlot};

mod builder;
pub use builder::FunctionBuilder;

mod display;
pub use display::{function_to_dot, module_to_dot};

newtype_index! {
    /// A unique identifier for a value in the MIR.
    pub struct ValueId;
}

newtype_index! {
    /// A unique identifier for an instruction in the MIR.
    pub struct InstId;
}

newtype_index! {
    /// A unique identifier for a basic block in the MIR.
    pub struct BlockId;
}

newtype_index! {
    /// A unique identifier for a function in the MIR.
    pub struct FunctionId;
}

#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

use solar_data_structures::newtype_index;

pub use solar_ir::link::{Data, DataBytes, DataId, DataRef, DataSize, ImmutableId};

pub mod analysis;
pub mod immutable;
pub mod memory;
pub mod utils;

mod types;
pub mod typing;
pub use types::{
    FrameMode, FrameSlotKind, ImmutableEncoding, MemoryObjectKind, MemoryObjectLayout, MirType,
    PtrLocation, StructType, TypeSize, ValueLayout,
};

mod abi;
pub use abi::{
    AbiLayout, AbiLayoutRef, AbiParamLayout, AbiParamLayoutRef, AbiParamLocation, AbiParamType,
    AbiType, AbiWordValidator,
};

mod bit_width;
pub use bit_width::{BitTestPricing, BitWidthTest, ConditionUse};

mod packed;
pub use packed::{PackedArraySource, PackedPart, packed_element_bytes};

mod value;
pub use value::{Immediate, Value};

mod inst;
pub use inst::{
    AbiEncodeMode, AddressCallKind, AllocationFailure, AllocationInitialization,
    AllocationSemantics, EffectKind, Instruction, InstructionMetadata, MemoryRegion, StorageAlias,
};

mod arithmetic;
pub use arithmetic::{ArithmeticKind, CheckedOp};

mod checks;
pub use checks::{
    CalldataTail, MAX_SCRATCH_ERROR_WORDS, PanicCode, RevertKind, RevertPayload, RevertReason,
};

mod effects;
pub use effects::{ControlEffects, InstructionEffects};
mod op_schema;
pub use op_schema::{
    InstKind, LogTopics, Op, OpDef, OpTraits, Operands, PhaseCategory, PhaseSet, RawMemoryAccess,
    RawMemorySize, ResultKind,
};

mod block;
pub use block::{BasicBlock, Terminator};

mod mangling;
pub use mangling::{Disambiguator, MangledSymbol};

mod function;
pub use function::{Function, FunctionAttributes};

mod module;
pub use module::{Immutable, LoweredModule, MirPhase, Module, should_validate_ir};

mod builtin;
pub use builtin::{Builtin, Callee, RequireKind};

mod builder;
pub use builder::{CountedLoop, ERROR_SELECTOR, FunctionBuilder, ToUint};

mod display;

mod parser;

pub use analysis::validate;

newtype_index! {
    /// A function argument index in the MIR.
    pub struct ArgIdx;

    /// A unique identifier for a value in the MIR.
    pub struct ValueId;

    /// A unique identifier for an instruction in the MIR.
    pub struct InstId;

    /// A unique identifier for a basic block in the MIR.
    pub struct BlockId;

    /// A fixed aggregate type declared in a MIR module.
    pub struct StructId;

    /// A unique identifier for a function in the MIR.
    pub struct FunctionId;

    /// A HIR expression an instruction originates from, opaque to MIR.
    pub struct HirExprId;

    /// A HIR variable a MIR immutable stands for, opaque to MIR.
    pub struct HirVariableId;
}

impl BlockId {
    /// The first block in every function.
    pub const ENTRY: Self = Self::new(0);
}

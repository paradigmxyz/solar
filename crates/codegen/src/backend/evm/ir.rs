//! EVM backend IR.
//!
//! This module defines the target-specific Machine-IR-like boundary between
//! MIR lowering and final EVM assembly. EVM IR is intentionally untyped: values
//! are EVM stack words, not Solidity or MIR values with a [`crate::mir::MirType`].
//! It models backend basic blocks, opcode-like instructions, explicit physical
//! stack operations, terminators, and metadata. The parser/printer at the bottom
//! of the file provide a text format for tests and debugging; the IR itself is
//! not defined by that serialization.

use alloy_primitives::U256;
use solar_data_structures::{fmt, index::IndexVec, newtype_index};

mod display;
mod parse;
mod passes;
mod verify;

pub use parse::{EvmIrParseError, parse_evm_ir_module};
pub use passes::{EVM_IR_PASSES, EvmIrPass};
pub use verify::{EvmIrVerifyError, verify_evm_ir_module};

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub struct EvmIrBlockId;

    /// A unique identifier for an untyped stack word in EVM IR.
    pub struct EvmIrValueId;
}

/// An EVM IR module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvmIrModule {
    /// Program name used by tools and diagnostics.
    pub name: String,
    /// Basic blocks in layout order.
    pub blocks: IndexVec<EvmIrBlockId, EvmIrBlock>,
    /// The entry block, if one has been created.
    pub entry_block: Option<EvmIrBlockId>,
    /// Untyped stack words known to this program.
    pub values: IndexVec<EvmIrValueId, EvmIrValue>,
}

impl EvmIrModule {
    /// Creates an empty EVM IR program.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(is_valid_ident(&name), "invalid EVM IR program name `{name}`");
        Self { name, blocks: IndexVec::new(), entry_block: None, values: IndexVec::new() }
    }

    /// Adds a block to the program.
    pub fn add_block(&mut self, block: EvmIrBlock) -> EvmIrBlockId {
        let id = self.blocks.push(block);
        if self.entry_block.is_none() {
            self.entry_block = Some(id);
        }
        id
    }

    /// Allocates a named untyped stack word.
    pub fn add_value(&mut self, name: impl Into<String>) -> EvmIrValueId {
        let name = name.into();
        assert!(is_valid_value_name(&name), "invalid EVM IR value name `%{name}`");
        self.values.push(EvmIrValue { name })
    }

    /// Returns the block for the given ID.
    #[must_use]
    pub fn block(&self, id: EvmIrBlockId) -> &EvmIrBlock {
        &self.blocks[id]
    }

    /// Returns a mutable reference to the block for the given ID.
    pub fn block_mut(&mut self, id: EvmIrBlockId) -> &mut EvmIrBlock {
        &mut self.blocks[id]
    }

    /// Returns the value for the given ID.
    #[must_use]
    pub fn value(&self, id: EvmIrValueId) -> &EvmIrValue {
        &self.values[id]
    }
}

/// A basic block in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrBlock {
    /// Stable textual label for this block.
    pub label: String,
    /// Block metadata. The hot/cold field is present before it is consumed by
    /// layout or scheduling so fixtures can pin the format early.
    pub metadata: EvmIrBlockMetadata,
    /// Non-terminating EVM backend instructions.
    pub instructions: Vec<EvmIrInstruction>,
    /// Optional control-flow terminator.
    pub terminator: Option<EvmIrTerminator>,
    /// Values present on the stack at block entry, top first.
    ///
    /// This is the block's incoming stack-word signature: values produced by a
    /// predecessor and consumed here. It is empty for the entry block and for
    /// blocks that begin from a clean stack. Stack scheduling seeds its model
    /// stack from this so blocks that consume predecessor values can be
    /// scheduled instead of bailing.
    pub entry_stack: Vec<EvmIrValueId>,
}

impl EvmIrBlock {
    /// Creates an empty hot block.
    #[must_use]
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        assert!(is_valid_block_label(&label), "invalid EVM IR block label `{label}`");
        Self {
            label,
            metadata: EvmIrBlockMetadata::default(),
            instructions: Vec::new(),
            terminator: None,
            entry_stack: Vec::new(),
        }
    }
}

/// Block-level metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EvmIrBlockMetadata {
    /// Estimated block hotness for future layout and scheduling decisions.
    pub hotness: EvmIrBlockHotness,
}

/// Block hotness metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum EvmIrBlockHotness {
    /// The block is expected to be frequently executed.
    #[default]
    Hot,
    /// The block is expected to be infrequently executed.
    Cold,
}

impl EvmIrBlockHotness {
    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "hot" => Self::Hot,
            "cold" => Self::Cold,
            _ => return None,
        })
    }
}

/// A named untyped stack word.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrValue {
    /// Stable textual stack-word name, without the leading `%`.
    pub name: String,
}

/// A non-terminating untyped backend instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrInstruction {
    /// Optional stack word produced by this instruction.
    pub result: Option<EvmIrValueId>,
    /// EVM opcode, backend pseudo-op, or physical stack operation.
    pub kind: EvmIrInstructionKind,
    /// Instruction operands.
    pub operands: Vec<EvmIrOperand>,
    /// Instruction metadata.
    pub metadata: EvmIrMetadata,
}

impl EvmIrInstruction {
    /// Creates an instruction.
    #[must_use]
    pub fn new(mnemonic: impl Into<String>, operands: Vec<EvmIrOperand>) -> Self {
        Self {
            result: None,
            kind: EvmIrInstructionKind::Operation(mnemonic.into()),
            operands,
            metadata: EvmIrMetadata::default(),
        }
    }

    /// Creates a physical stack operation.
    #[must_use]
    pub fn stack_op(op: EvmIrStackOp) -> Self {
        Self {
            result: None,
            kind: EvmIrInstructionKind::Stack(op),
            operands: Vec::new(),
            metadata: EvmIrMetadata::default(),
        }
    }

    /// Returns the instruction mnemonic as printed in EVM IR.
    #[must_use]
    pub fn mnemonic(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match &self.kind {
            EvmIrInstructionKind::Operation(mnemonic) => write!(f, "{mnemonic}"),
            EvmIrInstructionKind::Stack(op) => write!(f, "{}", op.mnemonic()),
        })
    }

    /// Returns whether this instruction materializes a physical EVM stack op.
    #[must_use]
    pub const fn is_physical_stack_op(&self) -> bool {
        matches!(self.kind, EvmIrInstructionKind::Stack(_))
    }
}

/// A non-terminating EVM IR instruction kind.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EvmIrInstructionKind {
    /// Untyped EVM opcode or backend pseudo-op mnemonic.
    Operation(String),
    /// Materialized physical EVM stack operation.
    Stack(EvmIrStackOp),
}

/// A materialized physical EVM stack operation.
///
/// These are modeled distinctly from generic operation mnemonics so stack
/// scheduling can target EVM IR and later EVM IR passes can optimize the exact
/// `DUP`/`SWAP`/`POP` sequence before final assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvmIrStackOp {
    /// EVM `DUP1` through `DUP16`.
    Dup(u8),
    /// EVM `SWAP1` through `SWAP16`.
    Swap(u8),
    /// EVM `POP`.
    Pop,
}

impl EvmIrStackOp {
    /// Creates a `DUP<N>` operation.
    #[must_use]
    pub const fn dup(n: u8) -> Option<Self> {
        if n >= 1 && n <= 16 { Some(Self::Dup(n)) } else { None }
    }

    /// Creates a `SWAP<N>` operation.
    #[must_use]
    pub const fn swap(n: u8) -> Option<Self> {
        if n >= 1 && n <= 16 { Some(Self::Swap(n)) } else { None }
    }

    /// Returns this operation's stack effect.
    #[must_use]
    pub const fn stack_effect(self) -> EvmIrStackEffect {
        match self {
            Self::Dup(_) => EvmIrStackEffect::new(0, 1),
            Self::Swap(_) => EvmIrStackEffect::new(0, 0),
            Self::Pop => EvmIrStackEffect::new(1, 0),
        }
    }

    fn parse(mnemonic: &str) -> Option<Self> {
        if mnemonic == "pop" {
            return Some(Self::Pop);
        }
        if let Some(n) = mnemonic.strip_prefix("dup").and_then(|s| s.parse::<u8>().ok()) {
            return Self::dup(n);
        }
        if let Some(n) = mnemonic.strip_prefix("swap").and_then(|s| s.parse::<u8>().ok()) {
            return Self::swap(n);
        }
        None
    }

    fn mnemonic(self) -> impl fmt::Display {
        fmt::from_fn(move |f| match self {
            Self::Dup(n) => write!(f, "dup{n}"),
            Self::Swap(n) => write!(f, "swap{n}"),
            Self::Pop => write!(f, "pop"),
        })
    }
}

/// A control-flow terminator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrTerminator {
    /// The terminator kind.
    pub kind: EvmIrTerminatorKind,
    /// Terminator metadata.
    pub metadata: EvmIrMetadata,
}

impl EvmIrTerminator {
    /// Creates a terminator without metadata.
    #[must_use]
    pub const fn new(kind: EvmIrTerminatorKind) -> Self {
        Self { kind, metadata: EvmIrMetadata::EMPTY }
    }
}

/// Control-flow terminators in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum EvmIrTerminatorKind {
    /// Physical fallthrough into the next laid-out block.
    Fallthrough(EvmIrBlockId),
    /// Physical fallthrough into the next separately captured program segment.
    FallthroughNext,
    /// Unconditional jump.
    Jump(EvmIrBlockId),
    /// Conditional branch.
    Branch {
        /// Branch condition.
        condition: EvmIrOperand,
        /// Target when condition is non-zero.
        then_block: EvmIrBlockId,
        /// Target when condition is zero.
        else_block: EvmIrBlockId,
    },
    /// Multi-way branch.
    Switch {
        /// Discriminant value.
        value: EvmIrOperand,
        /// Default target.
        default: EvmIrBlockId,
        /// Case value and target pairs.
        cases: Vec<(EvmIrOperand, EvmIrBlockId)>,
    },
    /// EVM `RETURN(offset, size)`.
    Return {
        /// Memory offset.
        offset: EvmIrOperand,
        /// Byte length.
        size: EvmIrOperand,
    },
    /// EVM `REVERT(offset, size)`.
    Revert {
        /// Memory offset.
        offset: EvmIrOperand,
        /// Byte length.
        size: EvmIrOperand,
    },
    /// EVM `STOP`.
    Stop,
    /// EVM `INVALID`.
    Invalid,
    /// EVM `SELFDESTRUCT(recipient)`.
    SelfDestruct {
        /// Beneficiary address.
        recipient: EvmIrOperand,
    },
    /// Raw terminal opcode for already stack-scheduled machine-level code.
    RawOpcode(u8),
}

/// An instruction or terminator operand.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EvmIrOperand {
    /// Untyped stack-word reference.
    Value(EvmIrValueId),
    /// Immediate EVM word.
    Immediate(U256),
    /// Basic block reference.
    Block(EvmIrBlockId),
    /// Opaque backend symbol, such as a helper, data object, or future label kind.
    Symbol(String),
}

/// Generic metadata carried by instructions and terminators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvmIrMetadata {
    /// Optional stack effect.
    pub stack: Option<EvmIrStackEffect>,
    /// Extra key-value metadata fields, in textual order.
    pub attrs: Vec<EvmIrMetadataItem>,
}

impl EvmIrMetadata {
    /// Empty metadata value.
    pub const EMPTY: Self = Self { stack: None, attrs: Vec::new() };

    /// Returns whether no metadata fields are present.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.stack.is_none() && self.attrs.is_empty()
    }
}

/// A metadata key-value item.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrMetadataItem {
    /// Metadata key.
    pub key: String,
    /// Metadata value, if the field is not a marker.
    pub value: Option<String>,
}

/// Stack effect metadata for one EVM IR operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct EvmIrStackEffect {
    /// Number of stack items consumed.
    pub inputs: u16,
    /// Number of stack items produced.
    pub outputs: u16,
}

impl EvmIrStackEffect {
    /// Creates a stack effect descriptor.
    #[must_use]
    pub const fn new(inputs: u16, outputs: u16) -> Self {
        Self { inputs, outputs }
    }
}

pub(super) fn default_instruction_stack_effect(inst: &EvmIrInstruction) -> EvmIrStackEffect {
    match &inst.kind {
        EvmIrInstructionKind::Stack(op) => op.stack_effect(),
        EvmIrInstructionKind::Operation(_) if is_encoded_push_instruction(inst) => {
            EvmIrStackEffect::new(0, 1)
        }
        EvmIrInstructionKind::Operation(mnemonic) => {
            if let Some(effect) = opcode_stack_effect(mnemonic) {
                effect
            } else {
                EvmIrStackEffect::new(
                    inst.operands.len().try_into().unwrap_or(u16::MAX),
                    u16::from(inst.result.is_some()),
                )
            }
        }
    }
}

fn opcode_stack_effect(mnemonic: &str) -> Option<EvmIrStackEffect> {
    let opcode = super::assembler::op::from_mnemonic(mnemonic)?;
    let (inputs, outputs) = super::assembler::op::stack_io(opcode)?;
    Some(EvmIrStackEffect::new(inputs, outputs))
}

fn default_terminator_stack_effect(kind: &EvmIrTerminatorKind) -> EvmIrStackEffect {
    match kind {
        EvmIrTerminatorKind::Branch { .. } => EvmIrStackEffect::new(1, 0),
        EvmIrTerminatorKind::Switch { .. } => EvmIrStackEffect::new(1, 0),
        EvmIrTerminatorKind::Return { .. } | EvmIrTerminatorKind::Revert { .. } => {
            EvmIrStackEffect::new(2, 0)
        }
        EvmIrTerminatorKind::SelfDestruct { .. } => EvmIrStackEffect::new(1, 0),
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::FallthroughNext
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid => EvmIrStackEffect::new(0, 0),
        EvmIrTerminatorKind::RawOpcode(opcode) => super::assembler::op::stack_io(*opcode)
            .map(|(inputs, outputs)| EvmIrStackEffect::new(inputs, outputs))
            .unwrap_or_else(|| EvmIrStackEffect::new(0, 0)),
    }
}

pub(super) fn is_encoded_push_instruction(inst: &EvmIrInstruction) -> bool {
    matches!(
        &inst.kind,
        EvmIrInstructionKind::Operation(mnemonic)
            if matches!(mnemonic.as_str(), "push" | "push_deferred" | "push_immutable")
    )
}

fn is_ident_start(c: char) -> bool {
    c.is_ascii_alphabetic() || c == '_' || c == '$' || c == '.'
}

fn is_ident_continue(c: char) -> bool {
    is_ident_start(c) || c.is_ascii_digit()
}

fn is_valid_ident(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if is_ident_start(c)) && chars.all(is_ident_continue)
}

fn is_valid_value_name(name: &str) -> bool {
    let mut chars = name.chars();
    matches!(chars.next(), Some(c) if is_ident_start(c) || c.is_ascii_digit())
        && chars.all(is_ident_continue)
}

fn is_valid_block_label(label: &str) -> bool {
    let Some(digits) = label.strip_prefix("bb") else {
        return false;
    };
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

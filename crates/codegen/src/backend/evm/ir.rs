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

pub use parse::ParseError;
pub use passes::{
    BLOCK_LAYOUT_PASS, DEFAULT_LAYOUT_PIPELINE, PASS_REGISTRY, PassInfo, PassOptions,
    STACK_SCHEDULE_PASS, TERMINAL_DEDUP_PASS, lookup_pass, run_pass,
};
pub use verify::Verifier;

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub struct BlockId;

    /// A unique identifier for an untyped stack word in EVM IR.
    pub struct ValueId;
}

/// A compact inline IR identifier with no heap allocation.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct InlineName {
    bytes: [u8; 31],
    len: u8,
}

impl InlineName {
    fn new(name: &str) -> Self {
        assert!(name.len() <= 31, "EVM IR identifier is too long");
        let mut bytes = [0; 31];
        bytes[..name.len()].copy_from_slice(name.as_bytes());
        Self { bytes, len: name.len() as u8 }
    }

    fn as_str(&self) -> &str {
        std::str::from_utf8(&self.bytes[..usize::from(self.len)]).expect("validated identifier")
    }
}

impl std::fmt::Display for InlineName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// An EVM IR module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// Program name used by tools and diagnostics.
    pub name: InlineName,
    /// Basic blocks in layout order.
    pub blocks: IndexVec<BlockId, Block>,
    /// The entry block, if one has been created.
    pub entry_block: Option<BlockId>,
    /// Untyped stack words known to this program.
    pub values: IndexVec<ValueId, Value>,
}

impl Module {
    /// Parses textual EVM IR.
    pub fn parse(input: &str) -> Result<Self, ParseError> {
        parse::parse(input)
    }

    /// Creates an empty EVM IR program.
    #[must_use]
    pub fn new(name: impl AsRef<str>) -> Self {
        let name = InlineName::new(name.as_ref());
        assert!(is_valid_ident(name.as_str()), "invalid EVM IR program name `{name}`");
        Self { name, blocks: IndexVec::new(), entry_block: None, values: IndexVec::new() }
    }

    /// Changes the program name.
    pub fn set_name(&mut self, name: &str) {
        assert!(is_valid_ident(name), "invalid EVM IR program name `{name}`");
        self.name = InlineName::new(name);
    }

    /// Returns the program name.
    #[must_use]
    pub fn name(&self) -> &str {
        self.name.as_str()
    }

    /// Adds a block to the program.
    pub fn add_block(&mut self, block: Block) -> BlockId {
        let id = self.blocks.push(block);
        if self.entry_block.is_none() {
            self.entry_block = Some(id);
        }
        id
    }

    /// Allocates a named untyped stack word.
    pub fn add_value(&mut self, name: impl AsRef<str>) -> ValueId {
        assert!(
            is_valid_value_name(name.as_ref()),
            "invalid EVM IR value name `%{}`",
            name.as_ref()
        );
        self.values.push(Value { name: InlineName::new(name.as_ref()) })
    }

    /// Returns the block for the given ID.
    #[must_use]
    pub fn block(&self, id: BlockId) -> &Block {
        &self.blocks[id]
    }

    /// Returns a mutable reference to the block for the given ID.
    pub fn block_mut(&mut self, id: BlockId) -> &mut Block {
        &mut self.blocks[id]
    }

    /// Returns the value for the given ID.
    #[must_use]
    pub fn value(&self, id: ValueId) -> &Value {
        &self.values[id]
    }
}

/// A basic block in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Stable textual label for this block.
    pub label: u32,
    /// Block metadata. The hot/cold field is present before it is consumed by
    /// layout or scheduling so fixtures can pin the format early.
    pub metadata: BlockMetadata,
    /// Non-terminating EVM backend instructions.
    pub instructions: Vec<Instruction>,
    /// Optional control-flow terminator.
    pub terminator: Option<Terminator>,
    /// Values present on the stack at block entry, top first.
    ///
    /// This is the block's incoming stack-word signature: values produced by a
    /// predecessor and consumed here. It is empty for the entry block and for
    /// blocks that begin from a clean stack. Stack scheduling seeds its model
    /// stack from this so blocks that consume predecessor values can be
    /// scheduled instead of bailing.
    pub entry_stack: Vec<ValueId>,
}

impl Block {
    /// Creates an empty hot block.
    #[must_use]
    pub fn new(label: u32) -> Self {
        Self {
            label,
            metadata: BlockMetadata::default(),
            instructions: Vec::new(),
            terminator: None,
            entry_stack: Vec::new(),
        }
    }
}

/// Block-level metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockMetadata {
    /// Estimated block hotness for future layout and scheduling decisions.
    pub hotness: Hotness,
}

/// Block hotness metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Hotness {
    /// The block is expected to be frequently executed.
    #[default]
    Hot,
    /// The block is expected to be infrequently executed.
    Cold,
}

impl Hotness {
    /// Returns whether this is cold code.
    #[must_use]
    pub const fn is_cold(self) -> bool {
        matches!(self, Self::Cold)
    }

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
pub struct Value {
    /// Stable textual stack-word name, without the leading `%`.
    pub name: InlineName,
}

/// A non-terminating untyped backend instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// Optional stack word produced by this instruction.
    pub result: Option<ValueId>,
    /// Raw EVM opcode byte.
    pub opcode: u8,
    /// Internal encoding flags for instructions resolved during assembly.
    encoding: u8,
    /// Instruction operands.
    pub operands: Vec<Operand>,
    /// Instruction metadata.
    pub metadata: Metadata,
}

impl Instruction {
    const ENCODED_PUSH: u8 = 1;
    const DEFERRED: u8 = 2;
    const IMMUTABLE: u8 = 4;

    /// Creates an instruction for an EVM opcode.
    #[must_use]
    pub const fn opcode(opcode: u8) -> Self {
        Self { result: None, opcode, encoding: 0, operands: Vec::new(), metadata: Metadata::EMPTY }
    }

    /// Creates an encoded push instruction.
    #[must_use]
    pub fn push(operand: Operand) -> Self {
        Self::encoded_push(operand, Self::ENCODED_PUSH)
    }

    /// Creates an encoded deferred push instruction.
    #[must_use]
    pub fn push_deferred(operand: Operand) -> Self {
        Self::encoded_push(operand, Self::ENCODED_PUSH | Self::DEFERRED)
    }

    /// Creates an encoded immutable push instruction.
    #[must_use]
    pub fn push_immutable(operand: Operand) -> Self {
        Self::encoded_push(operand, Self::ENCODED_PUSH | Self::IMMUTABLE)
    }

    fn encoded_push(operand: Operand, encoding: u8) -> Self {
        Self {
            result: None,
            opcode: super::assembler::op::PUSH32,
            encoding,
            operands: vec![operand],
            metadata: Metadata { stack: Some(StackEffect::new(0, 1)), attrs: Vec::new() },
        }
    }

    /// Returns the instruction mnemonic as printed in EVM IR.
    #[must_use]
    pub fn mnemonic(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match self.encoding {
            Self::ENCODED_PUSH => f.write_str("push"),
            encoding if encoding == Self::ENCODED_PUSH | Self::DEFERRED => {
                f.write_str("push_deferred")
            }
            encoding if encoding == Self::ENCODED_PUSH | Self::IMMUTABLE => {
                f.write_str("push_immutable")
            }
            _ => super::assembler::op::fmt(self.opcode, f),
        })
    }

    /// Returns whether this is an encoded push.
    #[must_use]
    pub const fn is_encoded_push(&self) -> bool {
        self.encoding & Self::ENCODED_PUSH != 0
    }

    /// Returns whether this is a deferred push.
    #[must_use]
    pub const fn is_deferred_push(&self) -> bool {
        self.encoding & Self::DEFERRED != 0
    }

    /// Returns whether this is an immutable push.
    #[must_use]
    pub const fn is_immutable_push(&self) -> bool {
        self.encoding & Self::IMMUTABLE != 0
    }

    /// Returns whether this instruction materializes a physical EVM stack op.
    #[must_use]
    pub const fn is_physical_stack_op(&self) -> bool {
        !self.is_encoded_push()
            && matches!(
                self.opcode,
                super::assembler::op::POP
                    | super::assembler::op::DUP1..=super::assembler::op::DUP16
                    | super::assembler::op::SWAP1..=super::assembler::op::SWAP16
            )
    }
}

/// A control-flow terminator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terminator {
    /// The terminator kind.
    pub kind: TerminatorKind,
    /// Terminator metadata.
    pub metadata: Metadata,
}

impl Terminator {
    /// Creates a terminator without metadata.
    #[must_use]
    pub const fn new(kind: TerminatorKind) -> Self {
        Self { kind, metadata: Metadata::EMPTY }
    }
}

/// Control-flow terminators in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TerminatorKind {
    /// Unconditional jump.
    Jump(BlockId),
    /// Conditional branch.
    Branch {
        /// Branch condition.
        condition: Operand,
        /// Target when condition is non-zero.
        then_block: BlockId,
        /// Target when condition is zero.
        else_block: BlockId,
    },
    /// Multi-way branch.
    Switch {
        /// Discriminant value.
        value: Operand,
        /// Default target.
        default: BlockId,
        /// Case value and target pairs.
        cases: Vec<(Operand, BlockId)>,
    },
    /// EVM `RETURN(offset, size)`.
    Return {
        /// Memory offset.
        offset: Operand,
        /// Byte length.
        size: Operand,
    },
    /// EVM `REVERT(offset, size)`.
    Revert {
        /// Memory offset.
        offset: Operand,
        /// Byte length.
        size: Operand,
    },
    /// EVM `STOP`.
    Stop,
    /// EVM `INVALID`.
    Invalid,
    /// EVM `SELFDESTRUCT(recipient)`.
    SelfDestruct {
        /// Beneficiary address.
        recipient: Operand,
    },
    /// Raw terminal opcode for already stack-scheduled machine-level code.
    RawOpcode(u8),
}

/// An instruction or terminator operand.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Operand {
    /// Untyped stack-word reference.
    Value(ValueId),
    /// Immediate EVM word.
    Immediate(U256),
    /// Basic block reference.
    Block(BlockId),
    /// Opaque backend symbol, such as a helper, data object, or future label kind.
    Symbol(InlineName),
}

/// Generic metadata carried by instructions and terminators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Optional stack effect.
    pub stack: Option<StackEffect>,
    /// Extra key-value metadata fields, in textual order.
    pub attrs: Vec<MetadataItem>,
}

impl Metadata {
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
pub struct MetadataItem {
    /// Metadata key.
    pub key: InlineName,
    /// Metadata value, if the field is not a marker.
    pub value: Option<InlineName>,
}

/// Stack effect metadata for one EVM IR operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StackEffect {
    /// Number of stack items consumed.
    pub inputs: u16,
    /// Number of stack items produced.
    pub outputs: u16,
}

impl StackEffect {
    /// Creates a stack effect descriptor.
    #[must_use]
    pub const fn new(inputs: u16, outputs: u16) -> Self {
        Self { inputs, outputs }
    }
}

pub(super) fn default_instruction_stack_effect(inst: &Instruction) -> StackEffect {
    if inst.is_encoded_push() {
        StackEffect::new(0, 1)
    } else if let Some((inputs, outputs)) = super::assembler::op::stack_io(inst.opcode) {
        StackEffect::new(inputs, outputs)
    } else {
        StackEffect::new(
            inst.operands.len().try_into().unwrap_or(u16::MAX),
            u16::from(inst.result.is_some()),
        )
    }
}

fn default_terminator_stack_effect(kind: &TerminatorKind) -> StackEffect {
    match kind {
        TerminatorKind::Branch { .. } => StackEffect::new(1, 0),
        TerminatorKind::Switch { .. } => StackEffect::new(1, 0),
        TerminatorKind::Return { .. } | TerminatorKind::Revert { .. } => StackEffect::new(2, 0),
        TerminatorKind::SelfDestruct { .. } => StackEffect::new(1, 0),
        TerminatorKind::Jump(_) | TerminatorKind::Stop | TerminatorKind::Invalid => {
            StackEffect::new(0, 0)
        }
        TerminatorKind::RawOpcode(opcode) => super::assembler::op::stack_io(*opcode)
            .map(|(inputs, outputs)| StackEffect::new(inputs, outputs))
            .unwrap_or_else(|| StackEffect::new(0, 0)),
    }
}

pub(super) fn is_encoded_push_instruction(inst: &Instruction) -> bool {
    inst.is_encoded_push()
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

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
pub use passes::{PASSES, Pass, PassOptions};
pub use verify::Verifier;

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub struct BlockId;

    /// A unique identifier for an untyped stack word in EVM IR.
    pub struct ValueId;
}

/// An EVM IR module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// Program name used by tools and diagnostics.
    pub name: String,
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
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(is_valid_ident(&name), "invalid EVM IR program name `{name}`");
        Self { name, blocks: IndexVec::new(), entry_block: None, values: IndexVec::new() }
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
    pub fn add_value(&mut self, name: impl Into<String>) -> ValueId {
        let name = name.into();
        assert!(is_valid_value_name(&name), "invalid EVM IR value name `%{name}`");
        self.values.push(Value { name })
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
    pub label: String,
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
    pub fn new(label: impl Into<String>) -> Self {
        let label = label.into();
        assert!(is_valid_block_label(&label), "invalid EVM IR block label `{label}`");
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
    pub name: String,
}

/// A non-terminating untyped backend instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// Optional stack word produced by this instruction.
    pub result: Option<ValueId>,
    /// EVM opcode, backend pseudo-op, or physical stack operation.
    pub kind: InstructionKind,
    /// Instruction operands.
    pub operands: Vec<Operand>,
    /// Instruction metadata.
    pub metadata: Metadata,
}

impl Instruction {
    /// Creates an instruction.
    #[must_use]
    pub fn new(mnemonic: impl Into<String>, operands: Vec<Operand>) -> Self {
        Self {
            result: None,
            kind: InstructionKind::Operation(mnemonic.into()),
            operands,
            metadata: Metadata::default(),
        }
    }

    /// Creates a physical stack operation.
    #[must_use]
    pub fn stack_op(op: StackOp) -> Self {
        Self {
            result: None,
            kind: InstructionKind::Stack(op),
            operands: Vec::new(),
            metadata: Metadata::default(),
        }
    }

    /// Returns the instruction mnemonic as printed in EVM IR.
    #[must_use]
    pub fn mnemonic(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match &self.kind {
            InstructionKind::Operation(mnemonic) => write!(f, "{mnemonic}"),
            InstructionKind::Stack(op) => write!(f, "{}", op.mnemonic()),
        })
    }

    /// Returns whether this instruction materializes a physical EVM stack op.
    #[must_use]
    pub const fn is_physical_stack_op(&self) -> bool {
        matches!(self.kind, InstructionKind::Stack(_))
    }
}

/// A non-terminating EVM IR instruction kind.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum InstructionKind {
    /// Untyped EVM opcode or backend pseudo-op mnemonic.
    Operation(String),
    /// Materialized physical EVM stack operation.
    Stack(StackOp),
}

/// A materialized physical EVM stack operation.
///
/// These are modeled distinctly from generic operation mnemonics so stack
/// scheduling can target EVM IR and later EVM IR passes can optimize the exact
/// `DUP`/`SWAP`/`POP` sequence before final assembly.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StackOp {
    /// EVM `DUP1` through `DUP16`.
    Dup(u8),
    /// EVM `SWAP1` through `SWAP16`.
    Swap(u8),
    /// EVM `POP`.
    Pop,
}

impl StackOp {
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
    pub const fn stack_effect(self) -> StackEffect {
        match self {
            Self::Dup(_) => StackEffect::new(0, 1),
            Self::Swap(_) => StackEffect::new(0, 0),
            Self::Pop => StackEffect::new(1, 0),
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
    /// Physical fallthrough into the next laid-out block.
    Fallthrough(BlockId),
    /// Physical fallthrough into the next separately captured program segment.
    FallthroughNext,
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
    Symbol(String),
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
    pub key: String,
    /// Metadata value, if the field is not a marker.
    pub value: Option<String>,
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
    match &inst.kind {
        InstructionKind::Stack(op) => op.stack_effect(),
        InstructionKind::Operation(_) if is_encoded_push_instruction(inst) => {
            StackEffect::new(0, 1)
        }
        InstructionKind::Operation(mnemonic) => {
            if let Some(effect) = opcode_stack_effect(mnemonic) {
                effect
            } else {
                StackEffect::new(
                    inst.operands.len().try_into().unwrap_or(u16::MAX),
                    u16::from(inst.result.is_some()),
                )
            }
        }
    }
}

fn opcode_stack_effect(mnemonic: &str) -> Option<StackEffect> {
    let opcode = super::assembler::op::from_mnemonic(mnemonic)?;
    let (inputs, outputs) = super::assembler::op::stack_io(opcode)?;
    Some(StackEffect::new(inputs, outputs))
}

fn default_terminator_stack_effect(kind: &TerminatorKind) -> StackEffect {
    match kind {
        TerminatorKind::Branch { .. } => StackEffect::new(1, 0),
        TerminatorKind::Switch { .. } => StackEffect::new(1, 0),
        TerminatorKind::Return { .. } | TerminatorKind::Revert { .. } => StackEffect::new(2, 0),
        TerminatorKind::SelfDestruct { .. } => StackEffect::new(1, 0),
        TerminatorKind::Fallthrough(_)
        | TerminatorKind::FallthroughNext
        | TerminatorKind::Jump(_)
        | TerminatorKind::Stop
        | TerminatorKind::Invalid => StackEffect::new(0, 0),
        TerminatorKind::RawOpcode(opcode) => super::assembler::op::stack_io(*opcode)
            .map(|(inputs, outputs)| StackEffect::new(inputs, outputs))
            .unwrap_or_else(|| StackEffect::new(0, 0)),
    }
}

pub(super) fn is_encoded_push_instruction(inst: &Instruction) -> bool {
    matches!(
        &inst.kind,
        InstructionKind::Operation(mnemonic)
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

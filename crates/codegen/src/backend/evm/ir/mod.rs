//! EVM backend IR.
//!
//! This module defines the target-specific Machine-IR-like boundary between
//! MIR lowering and final EVM assembly. It contains only scheduled machine
//! instructions: MIR value identities and virtual stack operands remain private
//! to the stack scheduler. EVM IR models backend basic blocks, opcode-like
//! instructions, explicit physical stack operations, terminators, and metadata.
//! All backend optimization and layout decisions remain here. After block
//! layout, it lowers once to the assembler's primitive label-bearing encoding
//! stream. The parser/printer at the bottom of the file provide a text format for
//! tests and debugging; the IR itself is not defined by that serialization.

use super::op;
use alloy_primitives::U256;
use solar_data_structures::{fmt, index::IndexVec, newtype_index};
use solar_parse::lexer::is_ident;

mod display;
mod parse;
mod passes;
mod verify;

pub(in crate::backend::evm) mod assembly;

pub use passes::{PASS_REGISTRY, PassInfo, lookup_pass, run_pass};

pub(crate) use passes::DEFAULT_PIPELINE;

/// Validates the invariants of an EVM IR module.
pub fn validate(dcx: &solar_interface::diagnostics::DiagCtxt, module: &Module) {
    verify::validate(dcx, module);
}

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub(crate) struct BlockId;
}

impl BlockId {
    /// The first block in every non-empty module.
    pub(crate) const ENTRY: Self = Self::new(0);
}

/// An EVM IR module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// Program name used by tools and diagnostics.
    pub(crate) name: String,
    /// Basic blocks in layout order.
    pub(crate) blocks: IndexVec<BlockId, Block>,
}

impl Module {
    /// Parses textual EVM IR.
    pub fn parse(
        sess: &solar_interface::Session,
        source: &solar_interface::source_map::SourceFile,
    ) -> solar_interface::Result<Self> {
        parse::parse(sess, source)
    }

    /// Creates an empty EVM IR program.
    #[must_use]
    pub(crate) fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(is_ident(&name), "invalid EVM IR program name `{name}`");
        Self { name, blocks: IndexVec::new() }
    }

    /// Changes the program name.
    pub(crate) fn set_name(&mut self, name: impl Into<String>) {
        let name = name.into();
        assert!(is_ident(&name), "invalid EVM IR program name `{name}`");
        self.name = name;
    }

    /// Returns the program name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Adds a block to the program.
    pub(crate) fn add_block(&mut self, block: Block) -> BlockId {
        self.blocks.push(block)
    }
}

/// A basic block in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Block {
    /// Stable textual label for this block.
    pub(crate) label: u32,
    /// Block metadata. The hot/cold field is present before it is consumed by
    /// layout so fixtures can pin the format early.
    pub(crate) metadata: BlockMetadata,
    /// Non-terminating EVM backend instructions.
    pub(crate) instructions: Vec<Instruction>,
    /// Optional control-flow terminator.
    pub(crate) terminator: Option<Terminator>,
}

impl Block {
    /// Creates an empty hot block.
    #[must_use]
    pub(crate) fn new(label: u32) -> Self {
        Self {
            label,
            metadata: BlockMetadata::default(),
            instructions: Vec::new(),
            terminator: None,
        }
    }
}

/// Block-level metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct BlockMetadata {
    /// Estimated block hotness for layout decisions.
    pub(crate) hotness: Hotness,
}

/// Block hotness metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum Hotness {
    /// The block is expected to be frequently executed.
    #[default]
    Hot,
    /// The block is expected to be infrequently executed.
    Cold,
}

impl Hotness {
    /// Returns whether this is cold code.
    #[must_use]
    pub(crate) const fn is_cold(self) -> bool {
        matches!(self, Self::Cold)
    }
}

/// A non-terminating scheduled EVM instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Instruction {
    /// Raw EVM opcode byte.
    pub(crate) opcode: u8,
    /// Internal encoding flags for instructions resolved during assembly.
    encoding: u8,
    /// Encoded value carried by a push instruction.
    value: Option<PushValue>,
    /// Instruction metadata.
    pub(crate) metadata: Metadata,
}

impl Instruction {
    const ENCODED_PUSH: u8 = 1;
    const DEFERRED: u8 = 2;
    const IMMUTABLE: u8 = 4;

    /// Creates an instruction for an EVM opcode.
    #[must_use]
    pub(crate) fn opcode(opcode: u8) -> Self {
        Self { opcode, encoding: 0, value: None, metadata: Metadata::EMPTY }
    }

    /// Creates an encoded immediate push instruction.
    #[must_use]
    pub(crate) fn push_value(value: U256) -> Self {
        Self::encoded_push(PushValue::Immediate(value), Self::ENCODED_PUSH)
    }

    /// Creates an encoded block-address push instruction.
    #[must_use]
    pub(crate) fn push_block(block: BlockId) -> Self {
        Self::encoded_push(PushValue::Block(block), Self::ENCODED_PUSH)
    }

    /// Creates an encoded deferred push instruction.
    #[must_use]
    pub(in crate::backend::evm) fn push_deferred(id: assembly::DeferredConst) -> Self {
        assert!(
            id.index() <= assembly::AsmInst::PAYLOAD_MASK as usize,
            "deferred constant ID overflow"
        );
        Self::encoded_push(
            PushValue::Immediate(U256::from(id.index())),
            Self::ENCODED_PUSH | Self::DEFERRED,
        )
    }

    /// Creates an encoded immutable push instruction.
    #[must_use]
    pub(in crate::backend::evm) fn push_immutable(id: u32) -> Self {
        assert!(id <= assembly::AsmInst::PAYLOAD_MASK, "immutable ID overflow");
        Self::encoded_push(
            PushValue::Immediate(U256::from(id)),
            Self::ENCODED_PUSH | Self::IMMUTABLE,
        )
    }

    fn encoded_push(value: PushValue, encoding: u8) -> Self {
        Self {
            opcode: op::PUSH32,
            encoding,
            value: Some(value),
            metadata: Metadata { stack: Some(StackEffect::new(0, 1)) },
        }
    }

    /// Returns the immediate carried by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) const fn pushed_value(&self) -> Option<U256> {
        match self.value {
            Some(PushValue::Immediate(value)) => Some(value),
            _ => None,
        }
    }

    /// Returns the block carried by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) const fn pushed_block(&self) -> Option<BlockId> {
        match self.value {
            Some(PushValue::Block(block)) => Some(block),
            _ => None,
        }
    }

    /// Returns the instruction mnemonic as printed in EVM IR.
    #[must_use]
    pub(crate) fn mnemonic(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match self.encoding {
            Self::ENCODED_PUSH => f.write_str("push"),
            encoding if encoding == Self::ENCODED_PUSH | Self::DEFERRED => {
                f.write_str("push_deferred")
            }
            encoding if encoding == Self::ENCODED_PUSH | Self::IMMUTABLE => {
                f.write_str("push_immutable")
            }
            _ => op::fmt(self.opcode, f),
        })
    }

    /// Returns whether this is an encoded push.
    #[must_use]
    pub(crate) const fn is_encoded_push(&self) -> bool {
        self.encoding & Self::ENCODED_PUSH != 0
    }

    /// Returns the deferred constant referenced by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) fn deferred_push(&self) -> Option<assembly::DeferredConst> {
        if self.encoding & Self::DEFERRED == 0 {
            return None;
        }
        let value = self.pushed_value().expect("deferred push must carry an immediate");
        Some(assembly::DeferredConst::from_usize(
            usize::try_from(value).expect("deferred constant ID must fit usize"),
        ))
    }

    /// Returns the immutable identifier carried by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) const fn immutable_push(&self) -> Option<U256> {
        if self.encoding & Self::IMMUTABLE == 0 {
            return None;
        }
        self.pushed_value()
    }

    /// Returns whether this instruction materializes a physical EVM stack op.
    #[must_use]
    pub(crate) const fn is_physical_stack_op(&self) -> bool {
        !self.is_encoded_push()
            && matches!(
                self.opcode,
                op::POP | op::DUP1..=op::DUP16 | op::SWAP1..=op::SWAP16
            )
    }
}

/// A control-flow terminator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Terminator {
    /// The terminator kind.
    pub(crate) kind: TerminatorKind,
    /// Terminator metadata.
    pub(crate) metadata: Metadata,
}

impl Terminator {
    /// Creates a terminator without metadata.
    #[must_use]
    pub(crate) const fn new(kind: TerminatorKind) -> Self {
        Self { kind, metadata: Metadata::EMPTY }
    }
}

/// Control-flow terminators in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TerminatorKind {
    /// Unconditional jump.
    Jump(BlockId),
    /// Conditional branch.
    JumpI {
        /// Target when condition is non-zero.
        then_block: BlockId,
        /// Target when condition is zero.
        else_block: BlockId,
    },
    /// Jump through a dense zero-based table using an index from the stack.
    IndexedJump(Box<[BlockId]>),
    /// Terminal EVM opcode.
    Op(u8),
}

impl TerminatorKind {
    /// Visits every basic block target.
    pub(crate) fn visit_targets(&self, mut visit: impl FnMut(BlockId)) {
        match self {
            Self::Jump(target) => visit(*target),
            Self::JumpI { then_block, else_block } => {
                visit(*then_block);
                visit(*else_block);
            }
            Self::IndexedJump(targets) => targets.iter().copied().for_each(visit),
            Self::Op(_) => {}
        }
    }

    /// Visits block targets that require a physical label in the given layout.
    pub(crate) fn visit_label_targets(
        &self,
        next_block: Option<BlockId>,
        mut visit: impl FnMut(BlockId),
    ) {
        match self {
            Self::Jump(target) => {
                if Some(*target) != next_block {
                    visit(*target);
                }
            }
            Self::JumpI { then_block, else_block } => {
                if Some(*else_block) == next_block {
                    visit(*then_block);
                } else if Some(*then_block) == next_block {
                    visit(*else_block);
                } else {
                    visit(*then_block);
                    visit(*else_block);
                }
            }
            Self::IndexedJump(targets) => targets.iter().copied().for_each(visit),
            Self::Op(_) => {}
        }
    }

    /// Visits every basic block target mutably.
    pub(crate) fn visit_targets_mut(&mut self, mut visit: impl FnMut(&mut BlockId)) {
        match self {
            Self::Jump(target) => visit(target),
            Self::JumpI { then_block, else_block } => {
                visit(then_block);
                visit(else_block);
            }
            Self::IndexedJump(targets) => targets.iter_mut().for_each(visit),
            Self::Op(_) => {}
        }
    }
}

/// A value encoded by a push instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum PushValue {
    /// Immediate EVM word.
    Immediate(U256),
    /// Basic block reference.
    Block(BlockId),
}

/// Metadata carried by instructions and terminators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Metadata {
    /// Optional stack effect.
    pub(crate) stack: Option<StackEffect>,
}

impl Metadata {
    /// Empty metadata value.
    pub(crate) const EMPTY: Self = Self { stack: None };
}

/// Stack effect metadata for one EVM IR operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct StackEffect {
    /// Number of stack items consumed.
    pub(crate) inputs: u8,
    /// Number of stack items produced.
    pub(crate) outputs: u8,
}

impl StackEffect {
    /// Creates a stack effect descriptor.
    #[must_use]
    pub(crate) const fn new(inputs: u8, outputs: u8) -> Self {
        Self { inputs, outputs }
    }
}

pub(super) fn default_instruction_stack_effect(inst: &Instruction) -> Option<StackEffect> {
    if inst.is_encoded_push() {
        Some(StackEffect::new(0, 1))
    } else if let Some((inputs, outputs)) = op::stack_io(inst.opcode) {
        Some(StackEffect::new(inputs, outputs))
    } else {
        None
    }
}

fn default_terminator_stack_effect(kind: &TerminatorKind) -> Option<StackEffect> {
    match kind {
        TerminatorKind::JumpI { .. } => Some(StackEffect::new(1, 0)),
        TerminatorKind::IndexedJump(_) => Some(StackEffect::new(1, 0)),
        TerminatorKind::Jump(_) => Some(StackEffect::new(0, 0)),
        TerminatorKind::Op(opcode) => {
            op::stack_io(*opcode).map(|(inputs, outputs)| StackEffect::new(inputs, outputs))
        }
    }
}

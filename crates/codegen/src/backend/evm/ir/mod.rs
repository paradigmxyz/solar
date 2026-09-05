//! Physical EVM programs with explicit blocks, stack operations and relocations.
//!
//! MIR values and call conventions do not survive into this representation. Block
//! identities remain explicit until assembly; byte offsets are never CFG identities.

use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{index::IndexVec, map::FxHashMap, newtype_index};
use solar_interface::{Result, Session, Symbol, source_map::SourceFile};
use solar_sema::Gcx;
use std::fmt::Display;

pub use crate::pass_manager::pipeline_label;

mod cfg;
mod data;
mod immediate;
mod legalize;
mod local;
mod outline;
mod passes;
pub use passes::*;
mod text;
mod verify;

newtype_index! {
    /// A physical basic block identity.
    pub(crate) struct BlockId;
    /// A program data identity independent of its encoded byte offset.
    pub(crate) struct DataId;
}

/// A physical EVM program.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub(crate) name: Symbol,
    pub(crate) blocks: IndexVec<BlockId, Block>,
    pub(crate) layout: Option<Vec<BlockId>>,
    pub(crate) labels: FxHashMap<BlockId, u32>,
    pub(crate) data: IndexVec<DataId, Data>,
    pub(crate) deferred: FxHashMap<u32, U256>,
    /// Generated relocation for the complete encoded deployment size.
    pub(crate) program_size_id: Option<u32>,
    /// Opaque appended runtime bytes, excluded from deployment-prefix captures.
    pub(crate) appendix: Vec<u8>,
    /// Generated relocation for the first appended runtime byte.
    pub(crate) appendix_start_id: Option<u32>,
}

/// One contiguous sequence of scheduled physical instructions.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Block {
    pub(crate) insts: Vec<Instruction>,
    pub(crate) terminator: Terminator,
    pub(crate) cold: bool,
    pub(crate) loop_header: bool,
}

/// A relocatable region of immutable program bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Data {
    pub(crate) name: Option<Symbol>,
    pub(crate) bytes: Vec<u8>,
}

/// An already scheduled EVM instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Instruction {
    pub(crate) kind: InstKind,
    pub(crate) stack_effect: Option<(u8, u8)>,
}

impl From<InstKind> for Instruction {
    fn from(kind: InstKind) -> Self {
        Self { kind, stack_effect: None }
    }
}

/// Physical operations and unresolved immediate values.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum InstKind {
    Op(u8),
    Push(U256),
    PushLabel(BlockId),
    PushData { id: DataId, offset: u32 },
    PushDeferred(u32),
    PushImmutable { id: crate::mir::ImmutableId, width: u8 },
    Dup(u16),
    Swap(u16),
    Exchange(u16, u16),
}

/// A block's explicit control transfer and optional interchange metadata.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Terminator {
    pub(crate) kind: TerminatorKind,
    pub(crate) stack_effect: Option<(u8, u8)>,
}

impl From<TerminatorKind> for Terminator {
    fn from(kind: TerminatorKind) -> Self {
        Self { kind, stack_effect: None }
    }
}

/// A physical control transfer; indexed jumps carry possible computed targets.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) enum TerminatorKind {
    Jump(BlockId),
    JumpI(BlockId, BlockId),
    DynamicJump,
    IndexedJump(Vec<BlockId>),
    Stop,
    Return,
    Revert,
    Invalid,
    SelfDestruct,
    #[default]
    Unreachable,
}

impl Module {
    /// Appends a block with a unique textual label even after source-label wraparound.
    pub(crate) fn append_block(&mut self, block: Block) -> BlockId {
        let mut label = self
            .blocks
            .indices()
            .map(|id| self.block_label(id) as u32)
            .max()
            .map_or(0, |label| label.wrapping_add(1));
        while self.blocks.indices().any(|id| self.block_label(id) as u32 == label) {
            label = label.wrapping_add(1);
        }
        let id = self.blocks.push(block);
        if label as usize != id.index() {
            self.labels.insert(id, label);
        }
        id
    }

    /// Returns the source interchange label, retaining noncanonical input names.
    pub(crate) fn block_label(&self, id: BlockId) -> usize {
        self.labels.get(&id).map_or(id.index(), |label| *label as usize)
    }

    /// Iterates live blocks in their current physical layout order.
    pub(crate) fn block_ids(&self) -> impl Iterator<Item = BlockId> + '_ {
        self.layout.iter().flatten().copied().chain(
            self.blocks.indices().take(if self.layout.is_none() { self.blocks.len() } else { 0 }),
        )
    }

    /// Parses textual physical EVM IR.
    pub fn parse(sess: &Session, source: &SourceFile) -> Result<Self> {
        text::parse(sess, source)
    }

    /// Assembles this module into bytecode.
    pub fn into_bytecode(mut self, gcx: Gcx<'_>) -> Result<Vec<u8>> {
        let _changed = run_pipeline(gcx, &mut self, None);
        validate(gcx, &self);
        verify::validate_target(gcx, &self);
        gcx.dcx().has_errors()?;
        super::assembly::encode(gcx, &self)
    }

    /// Returns the module name.
    pub const fn name(&self) -> Symbol {
        self.name
    }

    /// Returns canonical textual physical EVM IR.
    pub fn to_text(&self) -> impl Display + '_ {
        text::PrintedModule(self)
    }
}

/// Validates a physical EVM program.
pub fn validate(gcx: Gcx<'_>, module: &Module) {
    verify::validate(gcx, module);
}

/// Returns the encoded byte count and static gas of compact literal materialization.
pub(crate) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    immediate::cost(evm_version, &immediate::materialize(evm_version, value))
}

/// Estimates private spill-copy schedules using target copy-run recognition.
pub(crate) fn copy_cost(version: EvmVersion, instructions: &[Instruction]) -> (usize, usize) {
    data::copy_cost(version, instructions)
}

/// Estimates local physical scheduling cost without changing the IR module.
pub(crate) fn scheduling_cost(
    version: solar_config::EvmVersion,
    instructions: &[Instruction],
) -> (usize, usize) {
    local::scheduling_cost(version, instructions)
}

/// Validates final layout and indexed-lowering encoding stack peaks.
pub(crate) fn validate_encoding(gcx: Gcx<'_>, module: &Module) -> Result<()> {
    verify::validate_encoding(gcx, module)
}

/// Required input, net height change and relative peak of a physical scheduling trial.
pub(crate) fn scheduling_usage(instructions: &[Instruction]) -> Option<(i64, i64, i64)> {
    local::stack_usage(instructions)
}

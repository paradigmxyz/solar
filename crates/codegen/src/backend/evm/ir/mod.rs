//! Physical EVM programs with explicit blocks, stack operations and relocations.
//!
//! MIR values and call conventions do not survive into this representation. Block
//! identities remain explicit until assembly; byte offsets are never CFG identities.
//! The semantic `keep_with_next` constraint preserves the adjacent GAS/SUB/CALL
//! reserve sequence; unlike source provenance, optimizations must respect it.

use super::debug_info::{DebugFunction, DebugFunctionExit};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_data_structures::{index::IndexVec, map::FxHashMap, newtype_index};
use solar_interface::{Result, Session, Span, Symbol, source_map::SourceFile};
use solar_sema::Gcx;
use std::fmt::Display;

pub use crate::mir::pass_manager::pipeline_label;

mod blocks;
mod cfg;
mod cold;
mod data;
mod diamond;
mod immediate;
mod legalize;
mod local;
mod outline;
mod passes;
pub use passes::*;
mod text;
mod verify;
pub(crate) use verify::validate as validate_for_encoding;

newtype_index! {
    /// A physical basic block identity.
    pub(crate) struct BlockId;
    /// A program data identity independent of its encoded byte offset.
    pub(crate) struct DataId;
}

/// A physical EVM program.
#[derive(Clone, Debug, Default, Eq)]
pub struct Module {
    pub(crate) name: Symbol,
    /// Source provenance is requested; it never changes physical program semantics.
    pub(crate) debug_info_tracked: bool,
    pub(crate) blocks: IndexVec<BlockId, Block>,
    pub(crate) layout: Option<Vec<BlockId>>,
    pub(crate) labels: FxHashMap<BlockId, u32>,
    /// Every pushed label is private control state, never an observable numeric value.
    ///
    /// Machine lowering establishes this invariant by keeping continuation and return
    /// labels separate from MIR values. Physical rewrites must preserve it: outlining
    /// places new continuations below the body's inputs and consumes them with a jump.
    /// Parsed modules do not carry this proof; exported captures clear it as well.
    pub(crate) private_control_labels: bool,
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
#[derive(Clone, Debug, Default, Eq)]
pub(crate) struct Block {
    pub(crate) insts: Vec<Instruction>,
    pub(crate) terminator: Terminator,
    pub(crate) cold: bool,
    pub(crate) loop_header: bool,
    pub(crate) function_invoke: Option<DebugFunction>,
}

/// A relocatable region of immutable program bytes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Data {
    pub(crate) name: Option<Symbol>,
    pub(crate) bytes: Vec<u8>,
}

/// An already scheduled EVM instruction.
#[derive(Clone, Debug, Eq)]
pub(crate) struct Instruction {
    pub(crate) kind: InstKind,
    pub(crate) stack_effect: Option<(u8, u8)>,
    pub(crate) debug: Option<Box<DebugMetadata>>,
    /// The following instruction must remain adjacent in this block.
    pub(crate) keep_with_next: bool,
}

impl From<InstKind> for Instruction {
    fn from(kind: InstKind) -> Self {
        Self { kind, stack_effect: None, debug: None, keep_with_next: false }
    }
}

/// Whether a rewrite may create a boundary before this instruction.
pub(crate) fn split_allowed(insts: &[Instruction], index: usize) -> bool {
    index == 0 || !insts[index - 1].keep_with_next
}

/// Optional provenance, kept separate from semantic instruction and block equality.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct DebugMetadata {
    pub(crate) source_spans: Vec<Span>,
    pub(crate) modifier_depth: u32,
    pub(crate) function_invoke: Option<DebugFunction>,
    pub(crate) function_exit: Option<DebugFunctionExit>,
    pub(crate) dropped: bool,
}

impl DebugMetadata {
    /// Retains a bounded union of origins; exceeding the bound drops the entire set.
    pub(crate) fn add_span(&mut self, span: Span) {
        if !self.dropped && !self.source_spans.contains(&span) {
            if self.source_spans.len() == crate::source_info::MAX_DEBUG_SPANS {
                self.source_spans.clear();
                self.dropped = true;
            } else {
                self.source_spans.push(span);
                self.source_spans.sort_unstable();
            }
        }
    }

    /// Combines origins without choosing one of conflicting function events.
    pub(crate) fn merge(&mut self, other: &Self) {
        if other.dropped {
            self.source_spans.clear();
            self.dropped = true;
        }
        for &span in &other.source_spans {
            self.add_span(span);
        }
        if self.modifier_depth != other.modifier_depth {
            self.modifier_depth = 0;
        }
        if self.function_invoke != other.function_invoke {
            self.function_invoke = None;
        }
        if self.function_exit != other.function_exit {
            self.function_exit = None;
        }
    }
}

impl PartialEq for Instruction {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.stack_effect == other.stack_effect
            && self.keep_with_next == other.keep_with_next
    }
}

impl PartialEq for Terminator {
    fn eq(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.stack_effect == other.stack_effect
            && self.keep_with_next == other.keep_with_next
    }
}

impl PartialEq for Block {
    fn eq(&self, other: &Self) -> bool {
        self.insts == other.insts
            && self.terminator == other.terminator
            && self.cold == other.cold
            && self.loop_header == other.loop_header
    }
}

impl PartialEq for Module {
    fn eq(&self, other: &Self) -> bool {
        self.name == other.name
            && self.blocks == other.blocks
            && self.layout == other.layout
            && self.labels == other.labels
            && self.private_control_labels == other.private_control_labels
            && self.data == other.data
            && self.deferred == other.deferred
            && self.program_size_id == other.program_size_id
            && self.appendix == other.appendix
            && self.appendix_start_id == other.appendix_start_id
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
#[derive(Clone, Debug, Default, Eq)]
pub(crate) struct Terminator {
    pub(crate) kind: TerminatorKind,
    pub(crate) stack_effect: Option<(u8, u8)>,
    pub(crate) debug: Option<Box<DebugMetadata>>,
    /// The following instruction must remain adjacent in this block.
    pub(crate) keep_with_next: bool,
}

impl From<TerminatorKind> for Terminator {
    fn from(kind: TerminatorKind) -> Self {
        Self { kind, stack_effect: None, debug: None, keep_with_next: false }
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
        validate(gcx, &self);
        gcx.dcx().has_errors()?;
        let _changed = run_pipeline(gcx, &mut self, None);
        finish_lowering(gcx, &mut self)?;
        validate(gcx, &self);
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

/// Completes mandatory target lowering independently of optional pass selection.
/// Explicit pipelines control optimization and dumps, but cannot omit instruction
/// legalization required for executable bytes. The default pipeline already runs
/// the required pass; custom old-target pipelines finish silently before capture.
/// Target availability is checked for both generated and parsed programs.
pub(crate) fn finish_lowering(gcx: Gcx<'_>, module: &mut Module) -> Result<()> {
    gcx.dcx().has_errors()?;
    if gcx.sess.opts.unstable.evm_ir_pipeline.is_some()
        && !gcx.sess.opts.evm_version.has_bitwise_shifting()
    {
        // shl / shr / sar -> target-legal arithmetic and physical stack operations
        let _changed = legalize::LegalizeShifts.run_pass(gcx, module);
    }
    legalize::lower_unavailable_reverts(gcx, module);
    verify::validate_target(gcx, module);
    gcx.dcx().has_errors()
}

/// Validates a physical EVM program.
pub fn validate(gcx: Gcx<'_>, module: &Module) {
    let _ = verify::validate(gcx, module);
}

/// Returns the encoded byte count and static gas of compact literal materialization.
pub(crate) fn immediate_materialization_cost(
    evm_version: EvmVersion,
    value: U256,
) -> (usize, usize) {
    immediate::materialization_cost(evm_version, value)
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
pub(crate) fn validate_encoding(
    gcx: Gcx<'_>,
    module: &Module,
    heights: Option<&verify::StackHeights>,
) -> Result<()> {
    verify::validate_encoding(gcx, module, heights)
}

/// Required input, net height change and relative peak of a physical scheduling trial.
pub(crate) fn scheduling_usage(instructions: &[Instruction]) -> Option<(i64, i64, i64)> {
    local::stack_usage(instructions)
}

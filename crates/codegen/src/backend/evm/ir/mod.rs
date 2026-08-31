//! EVM backend IR.
//!
//! This module defines the target-specific Machine-IR-like boundary between
//! MIR lowering and final EVM assembly. It contains only scheduled machine
//! instructions: MIR value identities and virtual stack operands remain private
//! to the stack scheduler. EVM IR models backend basic blocks, opcode-like
//! instructions, explicit physical stack operations, terminators, opaque
//! program data, and metadata.
//! All backend optimization and layout decisions remain here. After block
//! layout, it lowers once to the assembler's primitive label-bearing encoding
//! stream. The parser/printer at the bottom of the file provide a text format for
//! tests and debugging; the IR itself is not defined by that serialization.

use super::{
    DebugFunction, DebugFunctionExit, DebugSpans, MAX_DEBUG_SPANS,
    op::{self, StackOp},
};
use crate::mir::{ImmutableId, TypeSize};
use alloy_primitives::{Bytes, U256};
use solar_data_structures::{fmt, index::IndexVec, newtype_index};
use solar_interface::{Span, Symbol};

pub(in crate::backend::evm) mod builder;
mod display;
mod parse;
mod passes;
pub(in crate::backend::evm) mod verify;

pub(in crate::backend::evm) mod assembly;

pub(crate) use passes::compact_pushes::immediate_materialization_cost;
pub use passes::{
    ALL_PASSES, EvmPass, lookup_pass, pipeline_label, run_passes, run_passes_no_validate,
    run_pipeline,
};
pub(in crate::backend::evm) use passes::{
    compact_pushes::ImmediateMaterialization, legalize_shifts,
};

/// Validates the target-independent invariants of an EVM IR module.
pub fn validate(gcx: solar_sema::Gcx<'_>, module: &Module) {
    verify::Verifier::new(gcx).verify_module(module);
}

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub(crate) struct BlockId;

    /// A constant byte string appended to the assembled program.
    pub(crate) struct DataId;
}

/// A relocatable reference to a byte within an EVM IR data entry.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct DataRef {
    pub(crate) id: DataId,
    pub(crate) offset: u32,
}

/// One constant byte string and its optional display name.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Data {
    pub(crate) bytes: Bytes,
    pub(crate) name: Option<Symbol>,
    pub(crate) emit_in_runtime: bool,
}

impl DataRef {
    pub(crate) const fn new(id: DataId, offset: u32) -> Self {
        Self { id, offset }
    }
}

impl BlockId {
    /// The first block in every non-empty module.
    pub(crate) const ENTRY: Self = Self::new(0);
}

/// An EVM IR module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    /// Program name used by tools and diagnostics.
    pub(crate) name: Symbol,
    /// Basic blocks in layout order.
    pub(crate) blocks: IndexVec<BlockId, Block>,
    /// Constant byte strings addressable by `push_data`.
    pub(crate) data: IndexVec<DataId, Data>,
    /// Whether gas mode is rescuing a runtime that exceeds EIP-170.
    pub(crate) enable_size_outlining: bool,
    /// Whether passes must account for every operation's source debug information.
    debug_info_tracked: bool,
}

impl Module {
    /// Lowers this EVM IR module to bytecode.
    pub fn into_bytecode(self, gcx: solar_sema::Gcx<'_>) -> solar_interface::Result<Vec<u8>> {
        let mut assembler = super::assembler::Assembler::from_evm_ir(gcx, self)?;
        let result = assembler.assemble_with_evm_ir(true);
        gcx.dcx().has_errors()?;
        Ok(result.bytecode)
    }

    /// Parses textual EVM IR.
    pub fn parse(
        sess: &solar_interface::Session,
        source: &solar_interface::source_map::SourceFile,
    ) -> solar_interface::Result<Self> {
        parse::parse(sess, source)
    }

    /// Creates an empty EVM IR program.
    #[must_use]
    pub(crate) fn new(name: Symbol) -> Self {
        Self {
            name,
            blocks: IndexVec::new(),
            data: IndexVec::new(),
            enable_size_outlining: false,
            debug_info_tracked: false,
        }
    }

    /// Clears the module while retaining its outer allocations.
    pub(in crate::backend::evm) fn clear(&mut self) {
        self.blocks.clear();
        self.data.clear();
        self.enable_size_outlining = false;
        self.debug_info_tracked = false;
    }

    /// Enables source debug information auditing for optimization passes.
    pub(crate) fn track_debug_info(&mut self) {
        self.debug_info_tracked = true;
    }

    /// Returns whether optimization passes must account for source debug information.
    #[must_use]
    pub(crate) const fn debug_info_is_tracked(&self) -> bool {
        self.debug_info_tracked
    }

    /// Changes the program name without clearing emitted IR.
    pub(in crate::backend::evm) fn set_name(&mut self, name: Symbol) {
        self.name = name;
    }

    /// Returns the program name.
    #[must_use]
    pub const fn name(&self) -> Symbol {
        self.name
    }

    /// Returns whether data references can observe entry boundaries or order.
    pub(in crate::backend::evm) fn data_layout_is_observable(&self) -> bool {
        passes::data::data_layout_is_observable(self)
    }

    /// Adds a block to the program.
    pub(crate) fn add_block(&mut self, block: Block) -> BlockId {
        self.blocks.push(block)
    }

    /// Returns the block after `block` in layout order.
    #[must_use]
    pub(crate) fn next_block(&self, block: BlockId) -> Option<BlockId> {
        let next = block.index() + 1;
        (next < self.blocks.len()).then(|| BlockId::from_usize(next))
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
    /// Whether the block belongs to a natural loop.
    pub(crate) in_loop: bool,
    /// Source function entered by this block's leading `JUMPDEST`.
    pub(crate) function_invoke: Option<DebugFunction>,
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
    /// Logical stack operation selected during final assembly lowering.
    stack_op: Option<StackOp>,
    /// Instruction metadata.
    pub(crate) metadata: Metadata,
}

impl Instruction {
    const ENCODED_PUSH: u8 = 1;
    const DEFERRED: u8 = 2;
    const IMMUTABLE: u8 = 4;
    const DATA: u8 = 8;

    /// Creates an instruction for an EVM opcode.
    #[must_use]
    pub(crate) fn opcode(opcode: u8) -> Self {
        if let Some(stack_op) = StackOp::from_single_byte_evm_opcode(opcode) {
            return Self::stack_op(stack_op);
        }
        Self { opcode, encoding: 0, value: None, stack_op: None, metadata: Metadata::default() }
    }

    /// Replaces this instruction's metadata explicitly.
    #[must_use]
    pub(crate) fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Replaces this instruction's source metadata with another operation's.
    pub(crate) fn with_source_debug(mut self, metadata: &Metadata) -> Self {
        self.metadata.copy_source_debug_from(metadata);
        self
    }

    /// Replaces this instruction while preserving its debug metadata.
    pub(crate) fn replace_preserving_metadata(&mut self, replacement: Self) {
        let metadata = std::mem::take(&mut self.metadata);
        *self = replacement.with_metadata(metadata);
    }

    /// Creates a logical stack operation.
    #[must_use]
    pub(crate) fn stack_op(stack_op: StackOp) -> Self {
        Self {
            opcode: stack_op.ir_opcode(),
            encoding: 0,
            value: None,
            stack_op: Some(stack_op),
            metadata: Metadata::default(),
        }
    }

    /// Returns the equivalent one-byte EVM opcode for a non-push instruction.
    #[must_use]
    pub(crate) fn as_evm_opcode(&self) -> Option<u8> {
        if self.is_encoded_push() {
            None
        } else if let Some(stack_op) = self.stack_op {
            stack_op.single_byte_evm_opcode()
        } else {
            Some(self.opcode)
        }
    }

    /// Returns whether this instruction has a raw branch target outside its block.
    #[must_use]
    pub(crate) const fn has_raw_branch_target(&self) -> bool {
        matches!(self.opcode, op::JUMPI | op::RJUMPI | op::RJUMPV)
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

    /// Creates an encoded program-data-address push instruction.
    #[must_use]
    pub(crate) fn push_data(data: DataRef) -> Self {
        Self::encoded_push(PushValue::Data(data), Self::ENCODED_PUSH | Self::DATA)
    }

    /// Creates an encoded push whose operand will be supplied by an assembler
    /// relocation before EVM IR validation.
    #[must_use]
    pub(in crate::backend::evm) fn push_relocation() -> Self {
        Self {
            opcode: op::PUSH32,
            encoding: Self::ENCODED_PUSH,
            value: None,
            stack_op: None,
            metadata: Metadata { stack: Some(StackEffect::new(0, 1)), ..Metadata::default() },
        }
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

    /// Creates an encoded immutable push instruction with a fixed immediate width.
    #[must_use]
    pub(in crate::backend::evm) fn push_immutable(id: ImmutableId, type_size: TypeSize) -> Self {
        let mut inst = Self::encoded_push(
            PushValue::Immediate(U256::from(id.index())),
            Self::ENCODED_PUSH | Self::IMMUTABLE,
        );
        inst.opcode = op::push(type_size.bytes());
        inst
    }

    fn encoded_push(value: PushValue, encoding: u8) -> Self {
        Self {
            opcode: op::PUSH32,
            encoding,
            value: Some(value),
            stack_op: None,
            metadata: Metadata { stack: Some(StackEffect::new(0, 1)), ..Metadata::default() },
        }
    }

    /// Marks this synthetic instruction as intentionally having no source location.
    #[must_use]
    pub(crate) fn with_debug_info_dropped(mut self) -> Self {
        self.metadata.mark_debug_info_dropped();
        self
    }

    /// Returns the immediate carried by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) const fn pushed_value(&self) -> Option<U256> {
        match self.value {
            Some(PushValue::Immediate(value)) => Some(value),
            _ => None,
        }
    }

    /// Returns a literal runtime word carried by an ordinary immediate push.
    ///
    /// Deferred and immutable pushes encode internal IDs in the same payload variant, but their
    /// runtime values are supplied later and must not participate in constant-value reasoning.
    #[must_use]
    pub(in crate::backend::evm) const fn concrete_immediate(&self) -> Option<U256> {
        if self.encoding != Self::ENCODED_PUSH {
            return None;
        }
        self.pushed_value()
    }

    /// Returns the block carried by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) const fn pushed_block(&self) -> Option<BlockId> {
        match self.value {
            Some(PushValue::Block(block)) => Some(block),
            _ => None,
        }
    }

    /// Returns the program data carried by this push instruction, if any.
    #[must_use]
    pub(in crate::backend::evm) const fn pushed_data(&self) -> Option<DataRef> {
        match self.value {
            Some(PushValue::Data(data)) => Some(data),
            _ => None,
        }
    }

    /// Returns the instruction mnemonic as printed in EVM IR.
    #[must_use]
    pub(crate) fn mnemonic(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match self.stack_op {
            Some(StackOp::Dup(_)) => f.write_str("dup"),
            Some(StackOp::Swap(_)) => f.write_str("swap"),
            Some(StackOp::Exchange(_, _)) => f.write_str("exchange"),
            Some(StackOp::Pop) => f.write_str("pop"),
            None => match self.encoding {
                Self::ENCODED_PUSH => f.write_str("push"),
                encoding if encoding == Self::ENCODED_PUSH | Self::DEFERRED => {
                    f.write_str("push_deferred")
                }
                encoding if encoding == Self::ENCODED_PUSH | Self::IMMUTABLE => {
                    f.write_str("push_immutable")
                }
                encoding if encoding == Self::ENCODED_PUSH | Self::DATA => f.write_str("push_data"),
                _ => match self.opcode {
                    opcode @ op::DUP1..=op::DUP16 => {
                        write!(f, "dup {}", opcode - op::DUP1 + 1)
                    }
                    opcode @ op::SWAP1..=op::SWAP16 => {
                        write!(f, "swap {}", opcode - op::SWAP1 + 1)
                    }
                    _ => op::fmt(self.opcode, f),
                },
            },
        })
    }

    /// Returns whether this is an encoded push.
    #[must_use]
    pub(crate) const fn is_encoded_push(&self) -> bool {
        self.encoding & Self::ENCODED_PUSH != 0
    }

    /// Returns the logical stack operation, if present.
    #[must_use]
    pub(crate) const fn as_stack_op(&self) -> Option<StackOp> {
        self.stack_op
    }

    /// Returns metadata's stack effect override or the opcode's default effect.
    #[must_use]
    pub(crate) fn effective_stack_effect(&self) -> Option<StackEffect> {
        self.metadata.stack.or_else(|| default_instruction_stack_effect(self))
    }

    /// Returns whether metadata preserves the opcode's default stack effect.
    #[must_use]
    pub(crate) fn has_canonical_stack_effect(&self) -> bool {
        self.metadata
            .stack
            .is_none_or(|effect| Some(effect) == default_instruction_stack_effect(self))
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
    pub(in crate::backend::evm) fn immutable_push(&self) -> Option<ImmutableId> {
        if self.encoding & Self::IMMUTABLE == 0 {
            return None;
        }
        let value = self.pushed_value().expect("immutable push must carry an immediate");
        Some(ImmutableId::new(
            usize::try_from(value).expect("validated immutable ID must fit usize"),
        ))
    }

    /// Returns the immutable placeholder's type size, if this is an immutable push.
    #[must_use]
    pub(in crate::backend::evm) fn immutable_type_size(&self) -> Option<TypeSize> {
        if self.encoding & Self::IMMUTABLE == 0 {
            return None;
        }
        let width = self.opcode.checked_sub(op::PUSH1)? + 1;
        TypeSize::try_new_fb_bytes(width)
    }

    /// Returns whether this instruction materializes a physical EVM stack op.
    #[must_use]
    pub(crate) const fn is_physical_stack_op(&self) -> bool {
        self.stack_op.is_some()
    }
}

/// A control-flow terminator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Terminator {
    /// The terminator kind.
    pub(crate) kind: TerminatorKind,
    /// Terminator metadata.
    pub(crate) metadata: Metadata,
    /// Whether the builder inserted `STOP` for a raw assembler fragment that ran out of blocks.
    pub(crate) implicit_stop: bool,
}

impl Terminator {
    /// Creates a terminator without metadata.
    #[must_use]
    pub(crate) fn new(kind: TerminatorKind) -> Self {
        Self { kind, metadata: Metadata::default(), implicit_stop: false }
    }

    /// Creates the artificial `STOP` that closes a raw assembler fragment.
    #[must_use]
    pub(in crate::backend::evm) fn implicit_stop() -> Self {
        Self {
            kind: TerminatorKind::Op(op::STOP),
            metadata: Metadata::default(),
            implicit_stop: true,
        }
    }

    /// Marks this synthetic terminator as intentionally having no source location.
    #[must_use]
    pub(crate) fn with_debug_info_dropped(mut self) -> Self {
        self.metadata.mark_debug_info_dropped();
        self
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
    ///
    /// The index must be in range; lowering intentionally emits no bounds check.
    IndexedJump(Box<[BlockId]>),
    /// Terminal EVM opcode.
    Op(u8),
}

impl TerminatorKind {
    /// Returns the temporary stack growth introduced when lowering this terminator.
    #[must_use]
    pub(crate) fn lowering_stack_growth(&self, next: Option<BlockId>) -> usize {
        match self {
            Self::IndexedJump(_) => 3,
            Self::Jump(target) => usize::from(Some(*target) != next),
            Self::JumpI { .. } => 1,
            Self::Op(_) => 0,
        }
    }

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

impl fmt::Display for TerminatorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Jump(_) => f.write_str("jump"),
            Self::JumpI { .. } => f.write_str("jumpi"),
            Self::IndexedJump(_) => f.write_str("indexed_jump"),
            Self::Op(opcode) => f.write_str(op::mnemonic(*opcode).unwrap_or("terminal")),
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
    /// Constant program-data reference.
    Data(DataRef),
}

/// Metadata carried by instructions and terminators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct Metadata {
    /// Optional stack effect.
    pub(crate) stack: Option<StackEffect>,
    /// Solidity source span associated with this machine operation.
    source_spans: DebugSpans,
    /// Function activation entered after this operation.
    function_invoke: Option<DebugFunction>,
    /// Function activation closed after this operation.
    function_exit: Option<DebugFunctionExit>,
    /// Legacy source-map modifier nesting depth for this operation.
    modifier_depth: u32,
    /// Whether the source location was preserved or intentionally dropped.
    debug_info_handled: bool,
}

impl Metadata {
    /// Returns the source span associated with this operation.
    #[must_use]
    pub(crate) fn source_span(&self) -> Option<Span> {
        self.source_spans.first().copied()
    }

    /// Returns every source origin associated with this operation.
    #[must_use]
    pub(crate) fn source_spans(&self) -> &[Span] {
        &self.source_spans
    }

    /// Sets the source span associated with this operation.
    pub(crate) fn set_source_span(&mut self, span: Option<Span>) {
        self.source_spans.clear();
        self.source_spans.extend(span.filter(|span| !span.is_dummy()));
        self.debug_info_handled = true;
    }

    /// Sets all source origins associated with this operation.
    pub(crate) fn set_source_spans(&mut self, spans: impl IntoIterator<Item = Span>) {
        self.source_spans.clear();
        for span in spans {
            if !span.is_dummy() && !self.source_spans.contains(&span) {
                self.source_spans.push(span);
                if self.source_spans.len() == MAX_DEBUG_SPANS {
                    break;
                }
            }
        }
        self.debug_info_handled = true;
    }

    /// Returns the legacy source-map modifier nesting depth for this operation.
    #[must_use]
    pub(crate) const fn modifier_depth(&self) -> u32 {
        self.modifier_depth
    }

    /// Sets the legacy source-map modifier nesting depth for this operation.
    pub(crate) fn set_modifier_depth(&mut self, depth: u32) {
        self.modifier_depth = depth;
        self.debug_info_handled = true;
    }

    /// Copies source location metadata, including legacy modifier depth.
    pub(crate) fn copy_source_debug_from(&mut self, other: &Self) {
        self.set_source_spans(other.source_spans().iter().copied());
        self.modifier_depth = other.modifier_depth;
    }

    /// Adds origins from another operation without changing machine semantics.
    pub(crate) fn merge_source_spans(&mut self, other: &Self) {
        let had_source_spans = !self.source_spans.is_empty();
        for &span in other.source_spans() {
            if self.source_spans.len() == MAX_DEBUG_SPANS {
                break;
            }
            if !self.source_spans.contains(&span) {
                self.source_spans.push(span);
            }
        }
        if !had_source_spans && !self.source_spans.is_empty() {
            self.modifier_depth = other.modifier_depth;
        }
        self.debug_info_handled |= other.debug_info_handled;
    }

    /// Merges all compatible debug information from an equivalent operation.
    pub(crate) fn merge_equivalent_debug_info(&mut self, other: &Self) {
        self.merge_source_spans(other);
        debug_assert!(
            self.function_invoke.is_none()
                || other.function_invoke.is_none()
                || self.function_invoke == other.function_invoke,
            "cannot merge different function invocations"
        );
        debug_assert!(
            self.function_exit.is_none()
                || other.function_exit.is_none()
                || self.function_exit == other.function_exit,
            "cannot merge different function exits"
        );
        self.function_invoke = self.function_invoke.or(other.function_invoke);
        self.function_exit = self.function_exit.or(other.function_exit);
    }

    /// Returns the function entered after this operation.
    #[must_use]
    pub(crate) const fn function_invoke(&self) -> Option<DebugFunction> {
        self.function_invoke
    }

    /// Marks this operation as entering a function.
    pub(crate) fn set_function_invoke(&mut self, function: DebugFunction) {
        self.function_invoke = Some(function);
        self.debug_info_handled = true;
    }

    /// Removes and returns the function entered after this operation.
    pub(crate) fn take_function_invoke(&mut self) -> Option<DebugFunction> {
        self.debug_info_handled = true;
        self.function_invoke.take()
    }

    /// Returns the function activation transition on this operation.
    #[must_use]
    pub(crate) const fn function_exit(&self) -> Option<DebugFunctionExit> {
        self.function_exit
    }

    /// Marks this operation as closing the active function.
    pub(crate) fn set_function_exit(&mut self, exit: DebugFunctionExit) {
        self.function_exit = Some(exit);
        self.debug_info_handled = true;
    }

    /// Marks this operation as intentionally having no source location.
    pub(crate) fn mark_debug_info_dropped(&mut self) {
        self.set_source_span(None);
    }

    /// Returns whether source debug information was preserved or intentionally dropped.
    #[must_use]
    pub(crate) const fn debug_info_is_handled(&self) -> bool {
        self.debug_info_handled
    }
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

pub(super) fn default_terminator_stack_effect(kind: &TerminatorKind) -> Option<StackEffect> {
    match kind {
        TerminatorKind::JumpI { .. } => Some(StackEffect::new(1, 0)),
        TerminatorKind::IndexedJump(_) => Some(StackEffect::new(1, 0)),
        TerminatorKind::Jump(_) => Some(StackEffect::new(0, 0)),
        TerminatorKind::Op(opcode) => {
            op::stack_io(*opcode).map(|(inputs, outputs)| StackEffect::new(inputs, outputs))
        }
    }
}

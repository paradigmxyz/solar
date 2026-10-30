//! EVM backend IR.
//!
//! This module defines the target-specific Machine-IR-like boundary between
//! MIR lowering and final EVM assembly. It contains only scheduled machine
//! instructions: MIR value identities and virtual stack operands remain private
//! to the stack scheduler. EVM IR models backend basic blocks, opcode-like
//! instructions, explicit physical stack operations, terminators, opaque
//! program data, and metadata.
//! The compiler makes every backend optimization and layout decision on this IR,
//! in the EVM IR passes of `solar-codegen`, and after block layout lowers it once
//! to the assembler's primitive label-bearing encoding stream. The `display` and
//! `parse` submodules provide a text format for tests and debugging; the IR itself
//! is not defined by that serialization, and `verify` checks its invariants.

use crate::{
    debug::{DebugFunction, DebugFunctionExit, DebugSpans, MAX_DEBUG_SPANS},
    link::{EmbeddedBytecodes, ImmutableId, LibraryId, LibraryTable},
    op::{self, StackOp},
};
use alloy_primitives::U256;
use solar_ast::TypeSize;
use solar_config::{EvmVersion, OptimizationMode};
use solar_data_structures::{
    index::{IndexVec, index_vec},
    map::FxHashMap,
    newtype_index,
};
use solar_interface::{Span, Symbol, diagnostics::DiagCtxt};
use std::fmt;

pub use crate::link::{Data, DataBytes, DataId, DataRef, DataSize};
pub use solar_ir::Hotness;

mod display;

mod parse;

pub mod verify;

/// The largest ID of a data entry or an immutable that assembly can encode.
pub const MAX_ASSEMBLY_ID: u32 = 0x0fff_ffff;

/// Validates the target-independent invariants of an EVM IR module for `evm_version`, reporting
/// violations to `dcx`.
pub fn validate(dcx: &DiagCtxt, evm_version: EvmVersion, module: &Module) {
    verify::Verifier::for_evm_version(dcx, evm_version).verify_module(module);
}

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub struct BlockId;
}

impl BlockId {
    /// The first block in every non-empty module.
    pub const ENTRY: Self = Self::new(0);
}

/// An EVM IR module.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Module {
    pub libraries: LibraryTable,
    /// Program name used by tools and diagnostics.
    pub name: Symbol,
    /// Basic blocks in layout order.
    pub blocks: IndexVec<BlockId, Block>,
    /// Constant byte strings addressable by `push_data`.
    pub data: IndexVec<DataId, Data>,
    /// Whether bytes that execution must not fall into follow this code: the runtime
    /// artifact after creation code. A final `STOP` is then kept instead of being
    /// left implicit at the end of the bytecode.
    pub code_follows: bool,
    /// Whether passes must account for every operation's source debug information.
    debug_info_tracked: bool,
    /// Block contents in which an earlier peephole run found nothing to rewrite.
    pub peephole_clean: CleanBlocks,
    /// The objective this module's code is optimized for: the build's, or the one the source
    /// contract's `@custom:solar-optimize` tag selects.
    pub optimization: OptimizationMode,
}

impl Module {
    /// Links embedded contract bytecode into deferred data, interning the bytecode's libraries
    /// into `libraries`.
    ///
    /// Returns whether any data was linked.
    pub fn link(&mut self, bytecodes: &EmbeddedBytecodes, libraries: &mut LibraryTable) -> bool {
        let mut linked = false;
        for data in &mut self.data {
            let DataBytes::Deferred(code) = data.bytes else { continue };
            // d: creation_code|runtime_code C
            // => d: hex"<code(C)>" library_relocations [..] trailers [..]
            let bytecode = code.bytecode(bytecodes);
            assert!(
                u32::try_from(bytecode.bytes.len()).is_ok(),
                "embedded bytecode length exceeds `u32`"
            );
            data.bytes = DataBytes::Known(bytecode.bytes.clone());
            data.library_relocations = bytecode.relocations_in(libraries);
            data.trailers.clone_from(&bytecode.trailers);
            linked = true;
        }
        if linked {
            self.libraries = libraries.clone();
        }
        linked
    }

    /// Pushes each size derived from a data length as a literal. All data must be linked.
    pub fn fold_data_sizes(&mut self) {
        for block in &mut self.blocks {
            for inst in &mut block.instructions {
                // push_data_size d, addend[, aligned] => push (len(d) + addend) [& ~31]
                if let Some(size) = inst.pushed_data_size() {
                    let value = size.value(self.data[size.data].bytes.linked().len());
                    inst.replace_preserving_metadata(Instruction::push_value(value));
                }
            }
        }
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
    pub fn new(name: Symbol) -> Self {
        Self {
            name,
            libraries: LibraryTable::default(),
            blocks: IndexVec::new(),
            data: IndexVec::new(),
            code_follows: false,
            debug_info_tracked: false,
            peephole_clean: CleanBlocks::default(),
            optimization: OptimizationMode::default(),
        }
    }

    /// Clears the module while retaining its outer allocations.
    pub fn clear(&mut self) {
        self.blocks.clear();
        self.data.clear();
        self.libraries.clear();
        self.code_follows = false;
        self.debug_info_tracked = false;
        self.peephole_clean.clear();
        self.optimization = OptimizationMode::default();
    }

    /// Enables source debug information auditing for optimization passes.
    pub fn track_debug_info(&mut self) {
        self.debug_info_tracked = true;
    }

    /// Returns whether optimization passes must account for source debug information.
    #[must_use]
    pub const fn debug_info_is_tracked(&self) -> bool {
        self.debug_info_tracked
    }

    /// Changes the program name without clearing emitted IR.
    pub fn set_name(&mut self, name: Symbol) {
        self.name = name;
    }

    /// Returns the program name.
    #[must_use]
    pub const fn name(&self) -> Symbol {
        self.name
    }

    /// Adds a block to the program.
    pub fn add_block(&mut self, block: Block) -> BlockId {
        self.blocks.push(block)
    }

    /// Returns the block after `block` in layout order.
    #[must_use]
    pub fn next_block(&self, block: BlockId) -> Option<BlockId> {
        let next = block.index() + 1;
        (next < self.blocks.len()).then(|| BlockId::from_usize(next))
    }
}

/// A basic block in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Block {
    /// Stable textual label for this block.
    pub label: u32,
    /// Block metadata. The hot/cold field is present before it is consumed by
    /// layout so fixtures can pin the format early.
    pub metadata: BlockMetadata,
    /// Non-terminating EVM backend instructions.
    pub instructions: Vec<Instruction>,
    /// Optional control-flow terminator.
    pub terminator: Option<Terminator>,
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
        }
    }

    /// Whether the block runs at most once per call of its code, outside every loop.
    #[must_use]
    pub fn runs_once(&self) -> bool {
        !self.metadata.in_loop && !self.metadata.repeats
    }
}

/// Block-level metadata.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct BlockMetadata {
    /// Estimated block hotness for layout decisions.
    pub hotness: Hotness,
    /// Whether the block contains code from a known natural loop.
    pub in_loop: bool,
    /// Whether the block may run more than once per deployment although it is not in a loop of
    /// its own function: it belongs to creation code that a loop or recursion calls. Pricing
    /// treats such blocks as runtime code.
    pub repeats: bool,
    /// The objective this block's code is optimized for when it differs from its module's: the
    /// source function's `@custom:solar-optimize` tag.
    pub optimization: Option<OptimizationMode>,
    /// This label's address is used only for control transfer, never as an observable word.
    /// Hidden return targets and branches separated from their pushed labels by sharing carry
    /// this property. References may be redirected to an
    /// equivalent continuation, unlike ordinary address-taken labels whose identity is opaque.
    /// This is an executable-code property, independent of optional debug information.
    pub is_continuation: bool,
    /// Source function entered by this block's leading `JUMPDEST`.
    pub function_invoke: Option<DebugFunction>,
    /// Stack depth at which verification enters this block, in addition to its modeled edges.
    ///
    /// Only textual EVM IR declares it, so that a test can start a block with a deep stack; the
    /// entry block is otherwise entered at depth zero.
    pub entry_depth: Option<u16>,
    /// How the code from this block's `JUMPDEST` returns to its caller, when it is the entry of a
    /// function or of a shared body that returns through a dynamic `jump`.
    ///
    /// Direct jumps to the block are calls, and the return address is a block's address that the
    /// caller pushed before them. See [`CallEffect`].
    pub call_effect: Option<CallEffect>,
}

/// How code returns to its caller through a dynamic `jump`.
///
/// The code takes the word `ret` positions below the top of the stack on entry, counting the top
/// as zero, as its return address, and the jump to that word leaves the stack with `inputs` words
/// fewer and `outputs` words more than on entry. `inputs` counts the return address and any
/// arguments, and the words the code leaves in their place are its results. This is a property
/// of the code, not of its caller: whichever block's address a caller leaves in that slot is the
/// block that runs next, at the entry depth less `inputs` plus `outputs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CallEffect {
    /// Position of the return address from the top of the stack on entry.
    pub ret: u8,
    /// The words consumed on entry and the words left on return.
    pub effect: StackEffect,
}

impl CallEffect {
    /// Returns the effect of the code that follows `instructions` and then an operation with the
    /// effect `last`, which run first and leave the return address on the stack, or `None` when
    /// they consume or copy it.
    pub fn after(self, instructions: &[Instruction], last: StackEffect) -> Option<Self> {
        let mut ret = usize::from(self.ret);
        // The words the code pushes, less the words it consumes.
        let mut growth = 0isize;
        let swapped = |slot: usize, a: usize, b: usize| match slot {
            slot if slot == a => b,
            slot if slot == b => a,
            slot => slot,
        };
        let apply = |effect: StackEffect, ret: &mut usize, growth: &mut isize| {
            *ret = ret.checked_sub(usize::from(effect.inputs))? + usize::from(effect.outputs);
            *growth += isize::from(effect.outputs) - isize::from(effect.inputs);
            Some(())
        };
        for inst in instructions {
            match inst.as_stack_op() {
                Some(StackOp::Dup(n)) => {
                    if ret + 1 == usize::from(n) {
                        return None;
                    }
                    ret += 1;
                    growth += 1;
                }
                Some(StackOp::Swap(n)) => ret = swapped(ret, 0, usize::from(n)),
                Some(StackOp::Exchange(n, m)) => ret = swapped(ret, usize::from(n), usize::from(m)),
                _ => apply(inst.stack_effect(), &mut ret, &mut growth)?,
            }
        }
        apply(last, &mut ret, &mut growth)?;
        // The code after entered `growth` words higher, so it returns that many words lower.
        let StackEffect { inputs, outputs } = self.effect;
        let (inputs, outputs) = if growth >= 0 {
            (u8::try_from(isize::from(inputs) + growth).ok()?, outputs)
        } else {
            (inputs, u8::try_from(isize::from(outputs) - growth).ok()?)
        };
        Some(Self { ret: u8::try_from(ret).ok()?, effect: StackEffect::new(inputs, outputs) })
    }
}

/// A non-terminating scheduled EVM instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Instruction {
    /// Raw EVM opcode byte.
    pub opcode: u8,
    /// Internal encoding flags for instructions resolved during assembly.
    ///
    /// The flags must agree with `value`: only the constructors set them.
    pub encoding: u8,
    /// Encoded value carried by a push instruction.
    ///
    /// Transforms may retarget a pushed block in place; other changes need a constructor.
    pub value: Option<PushValue>,
    /// Logical stack operation selected during final assembly lowering.
    stack_op: Option<StackOp>,
    /// Instruction metadata.
    pub metadata: Metadata,
}

impl Instruction {
    const ENCODED_PUSH: u8 = 1;
    const IMMUTABLE: u8 = 4;
    const DATA: u8 = 8;
    const DATA_SIZE: u8 = 16;

    /// Creates an instruction for an EVM opcode.
    #[must_use]
    pub fn opcode(opcode: u8) -> Self {
        if let Some(stack_op) = StackOp::from_single_byte_evm_opcode(opcode) {
            return Self::stack_op(stack_op);
        }
        Self { opcode, encoding: 0, value: None, stack_op: None, metadata: Metadata::default() }
    }

    /// Replaces this instruction's metadata explicitly.
    #[must_use]
    pub fn with_metadata(mut self, metadata: Metadata) -> Self {
        self.metadata = metadata;
        self
    }

    /// Replaces this instruction's source metadata with another operation's.
    pub fn with_source_debug(mut self, metadata: &Metadata) -> Self {
        self.metadata.copy_source_debug_from(metadata);
        self
    }

    /// Replaces this instruction while preserving its debug metadata.
    pub fn replace_preserving_metadata(&mut self, replacement: Self) {
        let metadata = std::mem::take(&mut self.metadata);
        *self = replacement.with_metadata(metadata);
    }

    /// Creates a logical stack operation.
    #[must_use]
    pub fn stack_op(stack_op: StackOp) -> Self {
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
    pub fn as_evm_opcode(&self) -> Option<u8> {
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
    pub const fn has_raw_branch_target(&self) -> bool {
        self.opcode == op::JUMPI
    }

    /// Creates an encoded immediate push instruction.
    #[must_use]
    pub fn push_value(value: U256) -> Self {
        Self::encoded_push(PushValue::Immediate(value), Self::ENCODED_PUSH)
    }

    /// Creates an opaque library-address push instruction.
    #[must_use]
    pub fn push_library(value: LibraryId) -> Self {
        Self::encoded_push(PushValue::Library(value), Self::ENCODED_PUSH)
    }

    /// Creates an encoded block-address push instruction.
    #[must_use]
    pub fn push_block(block: BlockId) -> Self {
        Self::encoded_push(PushValue::Block(block), Self::ENCODED_PUSH)
    }

    /// Creates an encoded program-data-address push instruction.
    #[must_use]
    pub fn push_data(data: DataRef) -> Self {
        Self::encoded_push(PushValue::Data(data), Self::ENCODED_PUSH | Self::DATA)
    }

    /// Creates an encoded push of a size derived from a program-data length.
    #[must_use]
    pub fn push_data_size(size: DataSize) -> Self {
        Self::encoded_push(PushValue::DataSize(size), Self::ENCODED_PUSH | Self::DATA_SIZE)
    }

    /// Creates an encoded push whose operand will be supplied by an assembler
    /// relocation before EVM IR validation.
    #[must_use]
    pub fn push_relocation() -> Self {
        Self {
            opcode: op::PUSH32,
            encoding: Self::ENCODED_PUSH,
            value: None,
            stack_op: None,
            metadata: Metadata::default(),
        }
    }

    /// Creates an encoded immutable push instruction with a fixed immediate width.
    #[must_use]
    pub fn push_immutable(id: ImmutableId, type_size: TypeSize) -> Self {
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
            metadata: Metadata::default(),
        }
    }

    /// Marks this synthetic instruction as intentionally having no source location.
    #[must_use]
    pub fn with_debug_info_dropped(mut self) -> Self {
        self.metadata.mark_debug_info_dropped();
        self
    }

    /// Returns the immediate carried by this push instruction, if any.
    #[must_use]
    pub const fn pushed_value(&self) -> Option<U256> {
        match self.value {
            Some(PushValue::Immediate(value)) => Some(value),
            _ => None,
        }
    }

    /// Returns the source-qualified library identity, if any.
    pub const fn pushed_library(&self) -> Option<LibraryId> {
        match self.value {
            Some(PushValue::Library(value)) => Some(value),
            _ => None,
        }
    }

    /// Returns a literal runtime word carried by an ordinary immediate push.
    ///
    /// Immutable pushes encode internal IDs in the same payload variant, but their
    /// runtime values are supplied later and must not participate in constant-value reasoning.
    #[must_use]
    pub const fn concrete_immediate(&self) -> Option<U256> {
        if self.encoding != Self::ENCODED_PUSH {
            return None;
        }
        self.pushed_value()
    }

    /// Returns the block carried by this push instruction, if any.
    #[must_use]
    pub const fn pushed_block(&self) -> Option<BlockId> {
        match self.value {
            Some(PushValue::Block(block)) => Some(block),
            _ => None,
        }
    }

    /// Returns the program data carried by this push instruction, if any.
    #[must_use]
    pub const fn pushed_data(&self) -> Option<DataRef> {
        match self.value {
            Some(PushValue::Data(data)) => Some(data),
            _ => None,
        }
    }

    /// Returns whether a repeated push of this value may reuse an earlier copy through `DUP`:
    /// an immediate other than a native `PUSH0`, or a size derived from a data length.
    #[must_use]
    pub fn is_duplicable_push(&self, evm_version: EvmVersion) -> bool {
        match self.value {
            Some(PushValue::Immediate(value)) => !value.is_zero() || !evm_version.has_push0(),
            Some(PushValue::DataSize(_)) => true,
            _ => false,
        }
    }

    /// Returns the program-data size carried by this push instruction, if any.
    #[must_use]
    pub const fn pushed_data_size(&self) -> Option<DataSize> {
        match self.value {
            Some(PushValue::DataSize(size)) => Some(size),
            _ => None,
        }
    }

    /// Returns the generated opcode definition for this instruction.
    #[must_use]
    pub const fn definition(&self) -> Option<&'static op::OpDef> {
        op::definition(self.opcode)
    }

    /// Returns the instruction mnemonic as printed in EVM IR.
    #[must_use]
    pub fn mnemonic(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match self.stack_op {
            Some(stack_op) => f.write_str(stack_op.definition().mnemonic),
            None => match self.encoding {
                Self::ENCODED_PUSH if self.pushed_library().is_some() => {
                    f.write_str("push_library")
                }
                Self::ENCODED_PUSH => f.write_str("push"),
                encoding if encoding == Self::ENCODED_PUSH | Self::IMMUTABLE => {
                    f.write_str("push_immutable")
                }
                encoding if encoding == Self::ENCODED_PUSH | Self::DATA => f.write_str("push_data"),
                encoding if encoding == Self::ENCODED_PUSH | Self::DATA_SIZE => {
                    f.write_str("push_data_size")
                }
                _ => op::fmt(self.opcode, f),
            },
        })
    }

    /// Returns whether this is an encoded push.
    #[must_use]
    pub const fn is_encoded_push(&self) -> bool {
        self.encoding & Self::ENCODED_PUSH != 0
    }

    /// Returns the logical stack operation, if present.
    #[must_use]
    pub const fn as_stack_op(&self) -> Option<StackOp> {
        self.stack_op
    }

    /// Returns the words the instruction reads from the top of the stack and the words it leaves
    /// there. A stack operation reads down to its deepest operand: `dup 2` is 2 -> 3.
    ///
    /// Verification rejects every instruction without one: an unknown opcode, a raw extended stack
    /// opcode, or a stack operation deeper than the verifier allows.
    #[must_use]
    pub fn stack_effect(&self) -> StackEffect {
        if let Some(stack_op) = self.stack_op {
            let inputs = stack_op.required_depth();
            let outputs = inputs.checked_add_signed(stack_op.net_growth()).unwrap();
            let depth = |words| u8::try_from(words).expect("verified stack operation depth");
            return StackEffect::new(depth(inputs), depth(outputs));
        }
        if self.is_encoded_push() {
            return StackEffect::new(0, 1);
        }
        let (inputs, outputs) = self
            .definition()
            .and_then(|definition| definition.stack_io)
            .expect("verified instruction stack effect");
        StackEffect::new(inputs, outputs)
    }

    /// Returns the immutable identifier carried by this push instruction, if any.
    #[must_use]
    pub fn immutable_push(&self) -> Option<ImmutableId> {
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
    pub fn immutable_type_size(&self) -> Option<TypeSize> {
        if self.encoding & Self::IMMUTABLE == 0 {
            return None;
        }
        let width = self.opcode.checked_sub(op::PUSH1)? + 1;
        TypeSize::try_new_fb_bytes(width)
    }

    /// Returns whether this instruction materializes a physical EVM stack op.
    #[must_use]
    pub const fn is_physical_stack_op(&self) -> bool {
        self.stack_op.is_some()
    }

    /// Returns whether this instruction must stay immediately before the next one in its block,
    /// so the boundary after it may not be disturbed.
    ///
    /// See [`Metadata::keep_with_next`].
    #[must_use]
    pub const fn keeps_with_next(&self) -> bool {
        self.metadata.keep_with_next
    }

    /// Requires this instruction to stay immediately before the next one in its block.
    pub const fn keep_with_next(&mut self) {
        self.metadata.keep_with_next = true;
    }
}

/// A control-flow terminator.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Terminator {
    /// The terminator kind.
    pub kind: TerminatorKind,
    /// Terminator metadata.
    pub metadata: Metadata,
    /// Whether the builder inserted `STOP` for a raw assembler fragment that ran out of blocks.
    pub implicit_stop: bool,
}

impl Terminator {
    /// Creates a terminator without metadata.
    #[must_use]
    pub fn new(kind: TerminatorKind) -> Self {
        Self { kind, metadata: Metadata::default(), implicit_stop: false }
    }

    /// Creates the artificial `STOP` that closes a raw assembler fragment.
    #[must_use]
    pub fn implicit_stop() -> Self {
        Self {
            kind: TerminatorKind::Op(op::STOP),
            metadata: Metadata::default(),
            implicit_stop: true,
        }
    }

    /// Marks this synthetic terminator as intentionally having no source location.
    #[must_use]
    pub fn with_debug_info_dropped(mut self) -> Self {
        self.metadata.mark_debug_info_dropped();
        self
    }
}

/// Control-flow terminators in EVM IR.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum TerminatorKind {
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

/// Most words an indexed jump holds above its index while it reads its target from a packed
/// table.
pub const INDEXED_JUMP_STACK_GROWTH: usize = 3;

impl TerminatorKind {
    /// Returns the temporary stack growth introduced when lowering this terminator.
    #[must_use]
    pub fn lowering_stack_growth(&self, next: Option<BlockId>) -> usize {
        match self {
            Self::IndexedJump(_) => INDEXED_JUMP_STACK_GROWTH,
            Self::Jump(target) => usize::from(Some(*target) != next),
            Self::JumpI { .. } => 1,
            Self::Op(_) => 0,
        }
    }

    /// Returns the number of stack items consumed and produced, when fixed.
    #[must_use]
    pub const fn stack_io(&self) -> Option<(u8, u8)> {
        match self {
            Self::Jump(_) => Some((0, 0)),
            Self::JumpI { .. } | Self::IndexedJump(_) => Some((1, 0)),
            Self::Op(opcode) => op::stack_io(*opcode),
        }
    }

    /// Visits every basic block target.
    pub fn visit_targets(&self, mut visit: impl FnMut(BlockId)) {
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
    pub fn visit_label_targets(&self, next_block: Option<BlockId>, mut visit: impl FnMut(BlockId)) {
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
    pub fn visit_targets_mut(&mut self, mut visit: impl FnMut(&mut BlockId)) {
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
pub enum PushValue {
    /// Immediate EVM word.
    Immediate(U256),
    /// Opaque library address supplied by the linker.
    Library(LibraryId),
    /// Basic block reference.
    Block(BlockId),
    /// Constant program-data reference.
    Data(DataRef),
    /// Size derived from a program-data length, supplied during final assembly.
    DataSize(DataSize),
}

/// Metadata carried by instructions and terminators.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    /// Whether this instruction must stay immediately before the next instruction of its block.
    ///
    /// Only meaningful on instructions. It forbids every transform both from turning the boundary
    /// after this instruction into a block boundary, so no jump, label, or block-layout decision
    /// can come between the two, and from inserting an instruction there. Lowering sets it where
    /// the gas consumed between two instructions is observable, such as a pre-EIP-150 call's
    /// `GAS`-relative gas reserve.
    ///
    /// The flag records only that the pair is glued, not what it is glued to, so verification
    /// checks the reserve by shape instead: a glued `GAS` has to reach a glued `SUB` and a glued
    /// `SUB` the call itself, which rejects an instruction inserted into the sequence as well as
    /// a block that ends inside it. Transforms still check the boundary themselves, through
    /// `is_split_point`, rather than leaving a split for the verifier to catch.
    pub keep_with_next: bool,
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

fn common_debug_event<T: Eq>(mut events: impl Iterator<Item = T>) -> Option<T> {
    let first = events.next()?;
    events.all(|event| event == first).then_some(first)
}

impl Metadata {
    /// Returns the source span associated with this operation.
    #[must_use]
    pub fn source_span(&self) -> Option<Span> {
        self.source_spans.first().copied()
    }

    /// Returns every source origin associated with this operation.
    #[must_use]
    pub fn source_spans(&self) -> &[Span] {
        &self.source_spans
    }

    /// Sets the source span associated with this operation.
    pub fn set_source_span(&mut self, span: Option<Span>) {
        self.source_spans.clear();
        self.source_spans.extend(span.filter(|span| !span.is_dummy()));
        self.debug_info_handled = true;
    }

    /// Sets all source origins associated with this operation.
    pub fn set_source_spans(&mut self, spans: impl IntoIterator<Item = Span>) {
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
    pub const fn modifier_depth(&self) -> u32 {
        self.modifier_depth
    }

    /// Sets the legacy source-map modifier nesting depth for this operation.
    pub fn set_modifier_depth(&mut self, depth: u32) {
        self.modifier_depth = depth;
        self.debug_info_handled = true;
    }

    /// Copies source location metadata, including legacy modifier depth.
    pub fn copy_source_debug_from(&mut self, other: &Self) {
        self.set_source_spans(other.source_spans().iter().copied());
        self.modifier_depth = other.modifier_depth;
    }

    /// Copies debug information without copying machine properties such as `keep_with_next`.
    pub fn copy_debug_info_from(&mut self, other: &Self) {
        self.copy_source_debug_from(other);
        self.function_invoke = other.function_invoke;
        self.function_exit = other.function_exit;
    }

    /// Absorbs another operation's debug information: its origins are added and its function
    /// events are taken when this operation has none of its own.
    pub fn absorb_debug_info(&mut self, other: &Self) {
        self.merge_source_spans(other);
        if self.function_invoke.is_none()
            && let Some(function) = other.function_invoke()
        {
            self.set_function_invoke(function);
        }
        if self.function_exit.is_none()
            && let Some(exit) = other.function_exit()
        {
            self.set_function_exit(exit);
        }
    }

    /// Adds origins from another operation without changing machine semantics.
    pub fn merge_source_spans(&mut self, other: &Self) {
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

    /// Merges source origins and function events across equivalent operations.
    pub fn merge_equivalent_debug_info<'a>(
        &mut self,
        others: impl Iterator<Item = &'a Self> + Clone,
    ) {
        // NOTE: Missing events do not conflict with known ones. Conflicting events become
        // unknown across the whole group; a later known event must not resurrect them.
        self.function_invoke = common_debug_event(
            self.function_invoke
                .into_iter()
                .chain(others.clone().filter_map(Self::function_invoke)),
        );
        self.function_exit = common_debug_event(
            self.function_exit.into_iter().chain(others.clone().filter_map(Self::function_exit)),
        );
        for other in others {
            self.merge_source_spans(other);
        }
    }

    /// Returns the function entered after this operation.
    #[must_use]
    pub const fn function_invoke(&self) -> Option<DebugFunction> {
        self.function_invoke
    }

    /// Marks this operation as entering a function.
    pub fn set_function_invoke(&mut self, function: DebugFunction) {
        self.function_invoke = Some(function);
        self.debug_info_handled = true;
    }

    /// Removes and returns the function entered after this operation.
    pub fn take_function_invoke(&mut self) -> Option<DebugFunction> {
        self.debug_info_handled = true;
        self.function_invoke.take()
    }

    /// Returns the function activation transition on this operation.
    #[must_use]
    pub const fn function_exit(&self) -> Option<DebugFunctionExit> {
        self.function_exit
    }

    /// Marks this operation as closing the active function.
    pub fn set_function_exit(&mut self, exit: DebugFunctionExit) {
        self.function_exit = Some(exit);
        self.debug_info_handled = true;
    }

    /// Marks this operation as intentionally having no source location.
    pub fn mark_debug_info_dropped(&mut self) {
        self.set_source_span(None);
    }

    /// Returns whether source debug information was preserved or intentionally dropped.
    #[must_use]
    pub const fn debug_info_is_handled(&self) -> bool {
        self.debug_info_handled
    }
}

/// Stack effect metadata for one EVM IR operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct StackEffect {
    /// Number of stack items consumed.
    pub inputs: u8,
    /// Number of stack items produced.
    pub outputs: u8,
}

impl StackEffect {
    /// Creates a stack effect descriptor.
    #[must_use]
    pub const fn new(inputs: u8, outputs: u8) -> Self {
        Self { inputs, outputs }
    }
}

pub fn default_terminator_stack_effect(kind: &TerminatorKind) -> Option<StackEffect> {
    let (inputs, outputs) = kind.stack_io()?;
    Some(StackEffect::new(inputs, outputs))
}

/// Reorders the blocks of `module` into `order`, which lists each block once, remapping every
/// reference.
pub fn remap_block_order(module: &mut Module, order: &[BlockId]) {
    debug_assert_eq!(order.len(), module.blocks.len());
    remap_blocks(module, order);
}

/// Keeps the blocks of `module` that `order` lists, in its order, remapping every reference.
/// Removed blocks must be unreferenced.
pub fn retain_blocks(module: &mut Module, order: &[BlockId]) {
    debug_assert!(order.len() <= module.blocks.len());
    remap_blocks(module, order);
}

fn remap_blocks(module: &mut Module, order: &[BlockId]) {
    let mut remap = index_vec![None; module.blocks.len()];
    let mut old_blocks =
        std::mem::take(&mut module.blocks).into_iter().map(Some).collect::<IndexVec<BlockId, _>>();
    let mut blocks = IndexVec::with_capacity(order.len());
    for &old_block in order {
        let block =
            old_blocks[old_block].take().expect("block order must contain each block exactly once");
        let new_block = blocks.push(block);
        remap[old_block] = Some(new_block);
    }
    module.blocks = blocks;
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            if let Some(PushValue::Block(block)) = &mut inst.value {
                *block = remap[*block].expect("referenced block must be retained");
            }
        }
        if let Some(term) = &mut block.terminator {
            remap_terminator_blocks(&mut term.kind, &remap);
        }
    }
}

fn remap_terminator_blocks(kind: &mut TerminatorKind, remap: &IndexVec<BlockId, Option<BlockId>>) {
    kind.visit_targets_mut(|target| {
        *target = remap[*target].expect("terminator target must be retained");
    });
}

/// Records of block contents on which a peephole run found no rewrite, keyed by a hash of the
/// contents, so a later run can skip the same contents.
///
/// Module clones start without the records, and they never affect module equality.
#[derive(Default)]
pub struct CleanBlocks(FxHashMap<(u32, u64), bool>);

impl CleanBlocks {
    /// Forgets every record.
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Returns the record of contents with `key`: whether they are clean under every rule set
    /// the pass distinguishes, or only under some.
    pub fn get(&self, key: (u32, u64)) -> Option<bool> {
        self.0.get(&key).copied()
    }

    /// Records contents with `key` as clean, under every rule set when `all` is set.
    pub fn record(&mut self, key: (u32, u64), all: bool) {
        *self.0.entry(key).or_default() |= all;
    }
}

impl Clone for CleanBlocks {
    fn clone(&self) -> Self {
        Self::default()
    }
}

impl PartialEq for CleanBlocks {
    fn eq(&self, _other: &Self) -> bool {
        true
    }
}

impl Eq for CleanBlocks {}

impl fmt::Debug for CleanBlocks {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("CleanBlocks")
    }
}

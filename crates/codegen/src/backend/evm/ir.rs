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
use solar_data_structures::{
    fmt::{self, FmtIteratorExt},
    index::IndexVec,
    map::{FxHashMap, FxHashSet},
    newtype_index,
};
use std::fmt as std_fmt;

newtype_index! {
    /// A unique identifier for a basic block in EVM IR.
    pub struct EvmIrBlockId;
}

newtype_index! {
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

    /// Returns the canonical EVM IR text-format representation.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            write!(
                f,
                "{}",
                self.blocks.iter_enumerated().format_with("", |f, (block_id, block)| {
                    write!(f, "{}", display_block(self, block_id, block))
                })
            )
        })
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
    /// Creates a `DUPn` operation.
    #[must_use]
    pub const fn dup(n: u8) -> Option<Self> {
        if n >= 1 && n <= 16 { Some(Self::Dup(n)) } else { None }
    }

    /// Creates a `SWAPn` operation.
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

/// Parses an EVM IR module from the text format.
///
/// # Errors
///
/// Returns an [`EvmIrParseError`] if `input` is malformed.
pub fn parse_evm_ir_module(input: &str) -> Result<EvmIrModule, EvmIrParseError> {
    Parser::new(input).parse_module()
}

/// A named EVM IR pass exposed to `solar evm-opt`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvmIrPass {
    /// No transform; validate and print the module.
    None,
    /// Materialize virtual instruction operands with physical stack operations.
    StackSchedule,
    /// Move cold terminal blocks after hot fallthrough code when this preserves fallthrough edges.
    ColdLayout,
    /// Replace duplicate terminal block bodies with jumps to the first copy when profitable.
    TerminalDedup,
}

impl EvmIrPass {
    /// Stable command-line pass name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::None => "none",
            Self::StackSchedule => "stack-schedule",
            Self::ColdLayout => "cold-layout",
            Self::TerminalDedup => "terminal-dedup",
        }
    }

    /// Runs this pass on an EVM IR module.
    pub fn run(self, module: &mut EvmIrModule) -> bool {
        match self {
            Self::None => false,
            Self::StackSchedule => super::ir_stack_schedule::schedule_stack_ops(module),
            Self::ColdLayout => move_cold_terminal_blocks(module),
            Self::TerminalDedup => deduplicate_terminal_blocks(module),
        }
    }

    /// Looks up a pass by command-line name.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "stack-schedule" => Self::StackSchedule,
            "cold-layout" => Self::ColdLayout,
            "terminal-dedup" => Self::TerminalDedup,
            _ => return None,
        })
    }
}

/// All EVM IR passes exposed by `solar evm-opt`.
pub const EVM_IR_PASSES: &[EvmIrPass] =
    &[EvmIrPass::None, EvmIrPass::StackSchedule, EvmIrPass::ColdLayout, EvmIrPass::TerminalDedup];

/// Verifies basic EVM IR invariants.
///
/// # Errors
///
/// Returns an [`EvmIrVerifyError`] if the module contains invalid references,
/// duplicate definitions, missing terminators, or malformed identifiers.
pub fn verify_evm_ir_module(module: &EvmIrModule) -> Result<(), EvmIrVerifyError> {
    if !is_valid_ident(&module.name) {
        return Err(EvmIrVerifyError::new(format!("invalid program name `{}`", module.name)));
    }
    if module.blocks.is_empty() {
        return Err(EvmIrVerifyError::new("program has no blocks"));
    }
    let Some(entry) = module.entry_block else {
        return Err(EvmIrVerifyError::new("program has no entry block"));
    };
    if !block_exists(module, entry) {
        return Err(EvmIrVerifyError::new(format!(
            "entry block `{}` is out of range",
            entry.index()
        )));
    }

    let mut labels = FxHashSet::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        if !is_valid_block_label(&block.label) {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("invalid block label `{}`", block.label),
            ));
        }
        if !labels.insert(block.label.as_str()) {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("duplicate block label `{}`", block.label),
            ));
        }
        if block.terminator.is_none() {
            return Err(EvmIrVerifyError::in_block(block_id, "missing terminator"));
        }
    }

    let mut value_names = FxHashSet::default();
    for (_, value) in module.values.iter_enumerated() {
        if !is_valid_value_name(&value.name) {
            return Err(EvmIrVerifyError::new(format!("invalid value name `%{}`", value.name)));
        }
        if !value_names.insert(value.name.as_str()) {
            return Err(EvmIrVerifyError::new(format!("duplicate value name `%{}`", value.name)));
        }
    }

    let mut defined_values = FxHashSet::default();
    for (block_id, block) in module.blocks.iter_enumerated() {
        for inst in &block.instructions {
            verify_instruction_shape(block_id, inst)?;
            if let Some(result) = inst.result {
                if !value_exists(module, result) {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        format!("result value `{}` is out of range", result.index()),
                    ));
                }
                if !defined_values.insert(result) {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        format!("value `%{}` is defined more than once", module.value(result).name),
                    ));
                }
            }
            for operand in &inst.operands {
                verify_operand(block_id, module, operand)?;
            }
            verify_metadata_is_untyped(block_id, &inst.metadata)?;
        }
        let term = block.terminator.as_ref().expect("checked above");
        verify_terminator_shape(block_id, &term.kind)?;
        visit_terminator_operands(&term.kind, |operand| {
            verify_operand(block_id, module, operand)?;
            Ok(())
        })?;
        visit_terminator_targets(&term.kind, |target| {
            if !block_exists(module, target) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("target block `{}` is out of range", target.index()),
                ));
            }
            Ok(())
        })?;
        verify_metadata_is_untyped(block_id, &term.metadata)?;
    }

    for (block_id, block) in module.blocks.iter_enumerated() {
        for &value in &block.entry_stack {
            if !value_exists(module, value) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("entry stack value `{}` is out of range", value.index()),
                ));
            }
            if !defined_values.contains(&value) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("entry stack value `%{}` is never defined", module.value(value).name),
                ));
            }
        }
        for inst in &block.instructions {
            for operand in &inst.operands {
                verify_value_defined(block_id, module, operand, &defined_values)?;
            }
        }
        let term = block.terminator.as_ref().expect("checked above");
        visit_terminator_operands(&term.kind, |operand| {
            verify_value_defined(block_id, module, operand, &defined_values)?;
            Ok(())
        })?;
    }

    verify_stack_consistency(module)?;

    Ok(())
}

/// One abstract stack word tracked by the consistency simulator.
///
/// Words carry their value identity when known so cross-block edges can compare
/// the exact words a predecessor leaves with those a successor declares. Words
/// produced by `push` or by an extra output of a multi-result op have no SSA
/// name and are modeled as [`AbstractWord::Unknown`]; two `Unknown` words are
/// never considered equal across an edge.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum AbstractWord {
    /// A word with a known SSA value identity.
    Value(EvmIrValueId),
    /// An anonymous word (a `push` immediate or a synthesized output) whose
    /// identity is not an SSA value.
    Unknown,
}

/// An abstract model stack: known words (top first) over an optional implicit
/// floor of predecessor-inherited words.
///
/// EVM basic blocks share one runtime stack. A block that is reachable from
/// predecessors may consume words those predecessors left below the portion the
/// block declares as its `entry_stack`. The production backend exploits this:
/// it does not declare an `entry_stack` and models opcodes as stack-neutral,
/// relying on physical `push`/`dup`/`swap`/`pop` to thread an implicitly
/// inherited stack between blocks. To stay sound without rejecting that
/// convention, non-entry blocks model an **unbounded floor** of unknown words
/// below `words`: an op that reaches past the known words draws fresh
/// [`AbstractWord::Unknown`] floor words instead of underflowing.
///
/// The entry block has no predecessors, so its floor is empty (`infinite_floor`
/// is `false`) and physical-op underflow is a real error and is rejected.
struct ModelStack {
    /// Known words, index 0 is the top of stack.
    words: Vec<AbstractWord>,
    /// Whether unknown words may be drawn from below `words`.
    infinite_floor: bool,
}

impl ModelStack {
    fn len(&self) -> usize {
        self.words.len()
    }

    /// Ensures at least `depth` words are modeled, materializing implicit floor
    /// words when allowed. Returns `false` if the stack is genuinely too shallow.
    fn ensure_depth(&mut self, depth: usize) -> bool {
        if self.words.len() >= depth {
            return true;
        }
        if !self.infinite_floor {
            return false;
        }
        while self.words.len() < depth {
            self.words.push(AbstractWord::Unknown);
        }
        true
    }

    fn push(&mut self, word: AbstractWord) {
        self.words.insert(0, word);
    }

    fn contains(&self, word: AbstractWord) -> bool {
        // A live word may be sitting in the implicit floor; over-approximate by
        // treating any value as reachable when a floor is present.
        self.words.contains(&word) || self.infinite_floor
    }
}

/// Simulates each block's stack and checks cross-block edge consistency.
///
/// For every block we start from its declared `entry_stack` (top first), apply
/// each instruction's stack effect to a [`ModelStack`] of word identities, apply
/// the terminator's effect, and record the resulting exit stack. Physical stack
/// ops (`dupN`/`swapN`/`pop`) are applied precisely; on the entry block (which
/// has no implicit floor) underflows and out-of-range depths are rejected.
///
/// Then, for every CFG edge `pred -> succ`, the successor's declared
/// `entry_stack` must be a **prefix** of the predecessor's exit stack (both top
/// first): the words a successor declares as incoming are exactly the top `k`
/// words the predecessor leaves, in order. The predecessor may leave additional
/// words below them — a successor only names the prefix it consumes. The entry
/// block must start from an empty stack.
///
/// Limitation: a *scheduled* operation has its operands cleared and does not
/// record a stack effect, so its input arity is no longer recoverable from the
/// instruction alone; such ops fall back to [`default_instruction_stack_effect`]
/// (inputs = 0). This does not affect the cross-block check, which compares a
/// successor's declared value identities against the predecessor's exit words.
fn verify_stack_consistency(module: &EvmIrModule) -> Result<(), EvmIrVerifyError> {
    if let Some(entry) = module.entry_block
        && !module.blocks[entry].entry_stack.is_empty()
    {
        return Err(EvmIrVerifyError::in_block(
            entry,
            "entry block must start from an empty stack",
        ));
    }

    let mut exit_stacks: IndexVec<EvmIrBlockId, Vec<AbstractWord>> =
        IndexVec::with_capacity(module.blocks.len());
    for (block_id, block) in module.blocks.iter_enumerated() {
        let is_entry = module.entry_block == Some(block_id);
        exit_stacks.push(simulate_block(module, block_id, block, is_entry)?);
    }

    for (block_id, block) in module.blocks.iter_enumerated() {
        let exit = &exit_stacks[block_id];
        let term = block.terminator.as_ref().expect("checked above");
        let mut result = Ok(());
        visit_terminator_targets(&term.kind, |succ| {
            let succ_entry: Vec<AbstractWord> = module.blocks[succ]
                .entry_stack
                .iter()
                .map(|&value| AbstractWord::Value(value))
                .collect();
            if !exit.starts_with(&succ_entry) {
                result = Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!(
                        "stack on edge to `{}` is inconsistent: successor declares incoming \
                         stack [{}] but predecessor leaves [{}]",
                        module.blocks[succ].label,
                        format_entry_stack(module, &module.blocks[succ].entry_stack),
                        format_abstract_stack(module, exit),
                    ),
                ));
            }
            Ok::<(), EvmIrVerifyError>(())
        })?;
        result?;
    }

    Ok(())
}

/// Computes a block's exit stack, rejecting any entry-block physical-stack-op
/// underflow or out-of-range depth and any reference to a word not live on the
/// model stack.
fn simulate_block(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    block: &EvmIrBlock,
    is_entry: bool,
) -> Result<Vec<AbstractWord>, EvmIrVerifyError> {
    let mut stack = ModelStack {
        words: block.entry_stack.iter().map(|&value| AbstractWord::Value(value)).collect(),
        infinite_floor: !is_entry,
    };

    for inst in &block.instructions {
        simulate_instruction(module, block_id, inst, &mut stack)?;
    }

    let term = block.terminator.as_ref().expect("checked above");
    simulate_terminator(module, block_id, &term.kind, &mut stack)?;
    Ok(stack.words)
}

fn simulate_instruction(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    inst: &EvmIrInstruction,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    match &inst.kind {
        EvmIrInstructionKind::Stack(op) => apply_physical_stack_op(block_id, *op, stack),
        EvmIrInstructionKind::Operation(_) if is_encoded_push_instruction(inst) => {
            // An encoded `push` adds one word: its SSA result if it has one,
            // otherwise an anonymous immediate word.
            stack.push(result_word(inst));
            Ok(())
        }
        EvmIrInstructionKind::Operation(_) if !inst.operands.is_empty() => {
            // Unscheduled op: its value operands are still present, so they must
            // be live on the model stack. They are not consumed (the operands
            // sit on the stack until scheduling clears them); the result, if
            // any, is pushed on top.
            for operand in &inst.operands {
                if let EvmIrOperand::Value(value) = operand
                    && !stack.contains(AbstractWord::Value(*value))
                {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        format!(
                            "operand `%{}` of `{}` is not live on the stack",
                            module.value(*value).name,
                            inst.mnemonic()
                        ),
                    ));
                }
            }
            if inst.result.is_some() {
                stack.push(result_word(inst));
            }
            Ok(())
        }
        EvmIrInstructionKind::Operation(_) => {
            // Scheduled op: operands cleared. Pop its declared inputs and push
            // its outputs.
            let effect =
                inst.metadata.stack.unwrap_or_else(|| default_instruction_stack_effect(inst));
            apply_effect(block_id, inst, effect, stack)
        }
    }
}

fn apply_effect(
    block_id: EvmIrBlockId,
    inst: &EvmIrInstruction,
    effect: EvmIrStackEffect,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    let inputs = usize::from(effect.inputs);
    if !stack.ensure_depth(inputs) {
        return Err(EvmIrVerifyError::in_block(
            block_id,
            format!(
                "`{}` consumes {} stack words but only {} are available",
                inst.mnemonic(),
                effect.inputs,
                stack.len()
            ),
        ));
    }
    stack.words.drain(0..inputs);
    for index in 0..effect.outputs {
        let word = if index == 0 { result_word(inst) } else { AbstractWord::Unknown };
        stack.push(word);
    }
    Ok(())
}

fn apply_physical_stack_op(
    block_id: EvmIrBlockId,
    op: EvmIrStackOp,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    match op {
        EvmIrStackOp::Dup(n) => {
            let depth = usize::from(n);
            if !stack.ensure_depth(depth) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("`dup{n}` reaches depth {n} but the stack has {}", stack.len()),
                ));
            }
            let word = stack.words[depth - 1];
            stack.push(word);
        }
        EvmIrStackOp::Swap(n) => {
            let depth = usize::from(n);
            if !stack.ensure_depth(depth + 1) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    format!("`swap{n}` reaches depth {n} but the stack has {}", stack.len()),
                ));
            }
            stack.words.swap(0, depth);
        }
        EvmIrStackOp::Pop => {
            if !stack.ensure_depth(1) {
                return Err(EvmIrVerifyError::in_block(block_id, "`pop` on an empty stack"));
            }
            stack.words.remove(0);
        }
    }
    Ok(())
}

fn simulate_terminator(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    kind: &EvmIrTerminatorKind,
    stack: &mut ModelStack,
) -> Result<(), EvmIrVerifyError> {
    // A terminator that still carries value operands is unscheduled: those
    // operands must be live, and they are not consumed from the model stack
    // (scheduling clears them and emits the consuming stack ops). A terminator
    // whose value operands have been cleared has already had its inputs
    // arranged and consumed by the scheduler, so we leave the model stack as is.
    let mut result = Ok(());
    visit_terminator_operands(kind, |operand| {
        if let EvmIrOperand::Value(value) = operand
            && !stack.contains(AbstractWord::Value(*value))
        {
            result = Err(EvmIrVerifyError::in_block(
                block_id,
                format!(
                    "terminator operand `%{}` is not live on the stack",
                    module.value(*value).name
                ),
            ));
        }
        Ok::<(), EvmIrVerifyError>(())
    })?;
    result
}

/// The word a result-producing instruction leaves on top.
fn result_word(inst: &EvmIrInstruction) -> AbstractWord {
    inst.result.map(AbstractWord::Value).unwrap_or(AbstractWord::Unknown)
}

fn format_entry_stack(module: &EvmIrModule, stack: &[EvmIrValueId]) -> String {
    stack
        .iter()
        .map(|&value| format!("%{}", module.value(value).name))
        .collect::<Vec<_>>()
        .join(", ")
}

fn format_abstract_stack(module: &EvmIrModule, stack: &[AbstractWord]) -> String {
    stack
        .iter()
        .map(|word| match word {
            AbstractWord::Value(value) => format!("%{}", module.value(*value).name),
            AbstractWord::Unknown => "<word>".to_string(),
        })
        .collect::<Vec<_>>()
        .join(", ")
}

fn verify_instruction_shape(
    block_id: EvmIrBlockId,
    inst: &EvmIrInstruction,
) -> Result<(), EvmIrVerifyError> {
    if let EvmIrInstructionKind::Stack(op) = &inst.kind {
        let expected = op.stack_effect();
        if inst.result.is_some() {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("physical stack op `{}` cannot define an SSA value", op.mnemonic()),
            ));
        }
        if !inst.operands.is_empty() {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("physical stack op `{}` cannot have operands", op.mnemonic()),
            ));
        }
        if let Some(effect) = inst.metadata.stack
            && effect != expected
        {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!(
                    "physical stack op `{}` has stack effect {}->{}, expected {}->{}",
                    op.mnemonic(),
                    effect.inputs,
                    effect.outputs,
                    expected.inputs,
                    expected.outputs
                ),
            ));
        }
    } else if is_encoded_push_instruction(inst) {
        if inst.operands.len() != 1 {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("`{}` must have one operand", inst.mnemonic()),
            ));
        }
        if matches!(inst.operands[0], EvmIrOperand::Value(_)) {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("`{}` cannot take a stack value operand", inst.mnemonic()),
            ));
        }
    } else {
        for operand in &inst.operands {
            if !matches!(operand, EvmIrOperand::Value(_)) {
                return Err(EvmIrVerifyError::in_block(
                    block_id,
                    "non-`push` instruction operands must be stack values",
                ));
            }
        }
    }
    Ok(())
}

fn verify_terminator_shape(
    block_id: EvmIrBlockId,
    kind: &EvmIrTerminatorKind,
) -> Result<(), EvmIrVerifyError> {
    match kind {
        EvmIrTerminatorKind::Branch { condition, .. } => {
            verify_stack_value_operand(block_id, condition, "branch condition")?
        }
        EvmIrTerminatorKind::Switch { value, cases, .. } => {
            verify_stack_value_operand(block_id, value, "switch value")?;
            for (case, _) in cases {
                if !matches!(case, EvmIrOperand::Immediate(_)) {
                    return Err(EvmIrVerifyError::in_block(
                        block_id,
                        "switch case values must be immediates",
                    ));
                }
            }
        }
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => {
            verify_stack_value_operand(block_id, offset, "memory offset")?;
            verify_stack_value_operand(block_id, size, "memory size")?;
        }
        EvmIrTerminatorKind::SelfDestruct { recipient } => {
            verify_stack_value_operand(block_id, recipient, "selfdestruct recipient")?
        }
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
    Ok(())
}

fn verify_stack_value_operand(
    block_id: EvmIrBlockId,
    operand: &EvmIrOperand,
    what: &str,
) -> Result<(), EvmIrVerifyError> {
    if matches!(operand, EvmIrOperand::Value(_)) {
        return Ok(());
    }
    Err(EvmIrVerifyError::in_block(block_id, format!("{what} must be a stack value")))
}

fn verify_metadata_is_untyped(
    block_id: EvmIrBlockId,
    metadata: &EvmIrMetadata,
) -> Result<(), EvmIrVerifyError> {
    for item in &metadata.attrs {
        if matches!(item.key.as_str(), "type" | "ty" | "result_ty" | "mir_type") {
            return Err(EvmIrVerifyError::in_block(
                block_id,
                format!("EVM IR is untyped; metadata key `{}` is not allowed", item.key),
            ));
        }
    }
    Ok(())
}

fn verify_operand(
    block_id: EvmIrBlockId,
    module: &EvmIrModule,
    operand: &EvmIrOperand,
) -> Result<(), EvmIrVerifyError> {
    match operand {
        EvmIrOperand::Value(value) if !value_exists(module, *value) => {
            Err(EvmIrVerifyError::in_block(
                block_id,
                format!("value `{}` is out of range", value.index()),
            ))
        }
        EvmIrOperand::Block(block) if !block_exists(module, *block) => {
            Err(EvmIrVerifyError::in_block(
                block_id,
                format!("block `{}` is out of range", block.index()),
            ))
        }
        _ => Ok(()),
    }
}

fn verify_value_defined(
    block_id: EvmIrBlockId,
    module: &EvmIrModule,
    operand: &EvmIrOperand,
    defined_values: &FxHashSet<EvmIrValueId>,
) -> Result<(), EvmIrVerifyError> {
    if let EvmIrOperand::Value(value) = operand
        && !defined_values.contains(value)
    {
        return Err(EvmIrVerifyError::in_block(
            block_id,
            format!("value `%{}` is used but never defined", module.value(*value).name),
        ));
    }
    Ok(())
}

fn block_exists(module: &EvmIrModule, block: EvmIrBlockId) -> bool {
    block.index() < module.blocks.len()
}

fn value_exists(module: &EvmIrModule, value: EvmIrValueId) -> bool {
    value.index() < module.values.len()
}

fn visit_terminator_operands<E>(
    kind: &EvmIrTerminatorKind,
    mut visit: impl FnMut(&EvmIrOperand) -> Result<(), E>,
) -> Result<(), E> {
    match kind {
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => {}
        EvmIrTerminatorKind::Branch { condition, .. } => visit(condition)?,
        EvmIrTerminatorKind::Switch { value, cases, .. } => {
            visit(value)?;
            for (case, _) in cases {
                visit(case)?;
            }
        }
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => {
            visit(offset)?;
            visit(size)?;
        }
        EvmIrTerminatorKind::SelfDestruct { recipient } => visit(recipient)?,
    }
    Ok(())
}

fn visit_terminator_targets<E>(
    kind: &EvmIrTerminatorKind,
    mut visit: impl FnMut(EvmIrBlockId) -> Result<(), E>,
) -> Result<(), E> {
    match kind {
        EvmIrTerminatorKind::Fallthrough(target) | EvmIrTerminatorKind::Jump(target) => {
            visit(*target)?
        }
        EvmIrTerminatorKind::Branch { then_block, else_block, .. } => {
            visit(*then_block)?;
            visit(*else_block)?;
        }
        EvmIrTerminatorKind::Switch { default, cases, .. } => {
            visit(*default)?;
            for (_, target) in cases {
                visit(*target)?;
            }
        }
        EvmIrTerminatorKind::Return { .. }
        | EvmIrTerminatorKind::Revert { .. }
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::SelfDestruct { .. }
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
    Ok(())
}

fn move_cold_terminal_blocks(module: &mut EvmIrModule) -> bool {
    let mut kept = Vec::with_capacity(module.blocks.len());
    let mut moved = Vec::new();

    for (block_id, block) in module.blocks.iter_enumerated() {
        if is_movable_cold_terminal_block(module, block_id, block) {
            moved.push(block_id);
        } else {
            kept.push(block_id);
        }
    }

    if moved.is_empty() {
        return false;
    }

    kept.extend(moved);
    remap_block_order(module, &kept);
    true
}

fn is_movable_cold_terminal_block(
    module: &EvmIrModule,
    block_id: EvmIrBlockId,
    block: &EvmIrBlock,
) -> bool {
    if module.entry_block == Some(block_id) || block_id.index() == 0 {
        return false;
    }
    let Some(term) = &block.terminator else {
        return false;
    };
    if block.metadata.hotness != EvmIrBlockHotness::Cold || !is_evm_terminal(&term.kind) {
        return false;
    }
    let previous = EvmIrBlockId::from_usize(block_id.index() - 1);
    module.blocks[previous].terminator.as_ref().is_some_and(|term| is_layout_barrier(&term.kind))
}

fn is_layout_barrier(kind: &EvmIrTerminatorKind) -> bool {
    matches!(kind, EvmIrTerminatorKind::Jump(_)) || is_evm_terminal(kind)
}

fn deduplicate_terminal_blocks(module: &mut EvmIrModule) -> bool {
    let mut canonical = Vec::<(TerminalBlockKey, EvmIrBlockId)>::new();
    let mut changed = false;

    let block_ids: Vec<_> = module.blocks.indices().collect();
    for block_id in block_ids {
        let block = &module.blocks[block_id];
        if !terminal_block_dedup_is_profitable(block) {
            continue;
        }
        let Some(key) = terminal_block_key(block) else { continue };
        if let Some((_, target)) = canonical.iter().find(|(known, _)| *known == key) {
            module.blocks[block_id].instructions.clear();
            module.blocks[block_id].terminator =
                Some(EvmIrTerminator::new(EvmIrTerminatorKind::Jump(*target)));
            changed = true;
        } else {
            canonical.push((key, block_id));
        }
    }

    changed
}

fn terminal_block_dedup_is_profitable(block: &EvmIrBlock) -> bool {
    let Some(term) = &block.terminator else { return false };
    if !is_evm_terminal(&term.kind) {
        return false;
    }
    // A replacement block still needs `JUMPDEST PUSH2(label) JUMP`. Avoid
    // rewriting tiny revert blocks where size is equal and revert-path gas
    // would get worse.
    let current_size = 1
        + block.instructions.iter().map(estimated_instruction_size).sum::<usize>()
        + estimated_terminator_size(&term.kind);
    let replacement_size = 1 + 3 + 1;
    current_size > replacement_size
}

fn estimated_instruction_size(inst: &EvmIrInstruction) -> usize {
    match &inst.kind {
        EvmIrInstructionKind::Stack(_) => 1,
        EvmIrInstructionKind::Operation(mnemonic) if mnemonic == "push" => {
            match inst.operands.as_slice() {
                [operand] => estimated_push_size(operand),
                _ => 1,
            }
        }
        EvmIrInstructionKind::Operation(mnemonic) if mnemonic == "push_immutable" => 33,
        EvmIrInstructionKind::Operation(_) => 1,
    }
}

fn estimated_terminator_size(kind: &EvmIrTerminatorKind) -> usize {
    let operand_pushes = |operands: &[&EvmIrOperand]| {
        operands.iter().map(|operand| estimated_push_size(operand)).sum::<usize>() + 1
    };
    match kind {
        EvmIrTerminatorKind::Return { offset, size }
        | EvmIrTerminatorKind::Revert { offset, size } => operand_pushes(&[offset, size]),
        EvmIrTerminatorKind::SelfDestruct { recipient } => operand_pushes(&[recipient]),
        EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => 1,
        EvmIrTerminatorKind::Fallthrough(_)
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Branch { .. }
        | EvmIrTerminatorKind::Switch { .. } => 0,
    }
}

fn estimated_push_size(operand: &EvmIrOperand) -> usize {
    match operand {
        EvmIrOperand::Immediate(value) if *value == U256::ZERO => 1,
        EvmIrOperand::Immediate(value) => value.byte_len() + 1,
        EvmIrOperand::Block(_) | EvmIrOperand::Symbol(_) => 3,
        EvmIrOperand::Value(_) => 0,
    }
}

fn is_evm_terminal(kind: &EvmIrTerminatorKind) -> bool {
    matches!(
        kind,
        EvmIrTerminatorKind::Return { .. }
            | EvmIrTerminatorKind::Revert { .. }
            | EvmIrTerminatorKind::Stop
            | EvmIrTerminatorKind::Invalid
            | EvmIrTerminatorKind::SelfDestruct { .. }
    ) || matches!(kind, EvmIrTerminatorKind::RawOpcode(opcode) if super::assembler::op::is_terminal(*opcode))
}

fn terminal_block_key(block: &EvmIrBlock) -> Option<TerminalBlockKey> {
    let mut locals = FxHashMap::default();
    let mut instructions = Vec::with_capacity(block.instructions.len());

    for inst in &block.instructions {
        let operands =
            inst.operands.iter().map(|operand| terminal_operand_key(operand, &locals)).collect();
        let result = inst.result.map(|value| {
            let index = locals.len();
            locals.insert(value, index);
            index
        });
        instructions.push(TerminalInstructionKey { result, kind: inst.kind.clone(), operands });
    }

    let term = block.terminator.as_ref()?;
    Some(TerminalBlockKey {
        instructions,
        terminator: terminal_terminator_key(&term.kind, &locals),
    })
}

fn terminal_operand_key(
    operand: &EvmIrOperand,
    locals: &FxHashMap<EvmIrValueId, usize>,
) -> TerminalOperandKey {
    match operand {
        EvmIrOperand::Value(value) => locals
            .get(value)
            .copied()
            .map(TerminalOperandKey::LocalValue)
            .unwrap_or(TerminalOperandKey::ExternalValue(*value)),
        EvmIrOperand::Immediate(value) => TerminalOperandKey::Immediate(*value),
        EvmIrOperand::Block(block) => TerminalOperandKey::Block(*block),
        EvmIrOperand::Symbol(symbol) => TerminalOperandKey::Symbol(symbol.clone()),
    }
}

fn terminal_terminator_key(
    kind: &EvmIrTerminatorKind,
    locals: &FxHashMap<EvmIrValueId, usize>,
) -> TerminalTerminatorKey {
    match kind {
        EvmIrTerminatorKind::Fallthrough(target) => TerminalTerminatorKey::Fallthrough(*target),
        EvmIrTerminatorKind::Jump(target) => TerminalTerminatorKey::Jump(*target),
        EvmIrTerminatorKind::Branch { condition, then_block, else_block } => {
            TerminalTerminatorKey::Branch {
                condition: terminal_operand_key(condition, locals),
                then_block: *then_block,
                else_block: *else_block,
            }
        }
        EvmIrTerminatorKind::Switch { value, default, cases } => TerminalTerminatorKey::Switch {
            value: terminal_operand_key(value, locals),
            default: *default,
            cases: cases
                .iter()
                .map(|(case, target)| (terminal_operand_key(case, locals), *target))
                .collect(),
        },
        EvmIrTerminatorKind::Return { offset, size } => TerminalTerminatorKey::Return {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        EvmIrTerminatorKind::Revert { offset, size } => TerminalTerminatorKey::Revert {
            offset: terminal_operand_key(offset, locals),
            size: terminal_operand_key(size, locals),
        },
        EvmIrTerminatorKind::Stop => TerminalTerminatorKey::Stop,
        EvmIrTerminatorKind::Invalid => TerminalTerminatorKey::Invalid,
        EvmIrTerminatorKind::SelfDestruct { recipient } => TerminalTerminatorKey::SelfDestruct {
            recipient: terminal_operand_key(recipient, locals),
        },
        EvmIrTerminatorKind::RawOpcode(opcode) => TerminalTerminatorKey::RawOpcode(*opcode),
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalBlockKey {
    instructions: Vec<TerminalInstructionKey>,
    terminator: TerminalTerminatorKey,
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct TerminalInstructionKey {
    result: Option<usize>,
    kind: EvmIrInstructionKind,
    operands: Vec<TerminalOperandKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalTerminatorKey {
    Fallthrough(EvmIrBlockId),
    Jump(EvmIrBlockId),
    Branch {
        condition: TerminalOperandKey,
        then_block: EvmIrBlockId,
        else_block: EvmIrBlockId,
    },
    Switch {
        value: TerminalOperandKey,
        default: EvmIrBlockId,
        cases: Vec<(TerminalOperandKey, EvmIrBlockId)>,
    },
    Return {
        offset: TerminalOperandKey,
        size: TerminalOperandKey,
    },
    Revert {
        offset: TerminalOperandKey,
        size: TerminalOperandKey,
    },
    Stop,
    Invalid,
    SelfDestruct {
        recipient: TerminalOperandKey,
    },
    RawOpcode(u8),
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalOperandKey {
    LocalValue(usize),
    ExternalValue(EvmIrValueId),
    Immediate(U256),
    Block(EvmIrBlockId),
    Symbol(String),
}

fn remap_block_order(module: &mut EvmIrModule, order: &[EvmIrBlockId]) {
    debug_assert_eq!(order.len(), module.blocks.len());
    let mut remap = vec![EvmIrBlockId::from_usize(0); module.blocks.len()];
    let mut blocks = IndexVec::new();
    for &old_block in order {
        let new_block = blocks.push(module.blocks[old_block].clone());
        remap[old_block.index()] = new_block;
    }
    module.blocks = blocks;
    module.entry_block = module.entry_block.map(|block| remap[block.index()]);
    for block in &mut module.blocks {
        for inst in &mut block.instructions {
            for operand in &mut inst.operands {
                remap_operand_blocks(operand, &remap);
            }
        }
        if let Some(term) = &mut block.terminator {
            remap_terminator_blocks(&mut term.kind, &remap);
        }
    }
}

fn remap_operand_blocks(operand: &mut EvmIrOperand, remap: &[EvmIrBlockId]) {
    if let EvmIrOperand::Block(block) = operand {
        *block = remap[block.index()];
    }
}

fn remap_terminator_blocks(kind: &mut EvmIrTerminatorKind, remap: &[EvmIrBlockId]) {
    visit_terminator_targets_mut(kind, |target| *target = remap[target.index()]);
}

fn visit_terminator_targets_mut(
    kind: &mut EvmIrTerminatorKind,
    mut visit: impl FnMut(&mut EvmIrBlockId),
) {
    match kind {
        EvmIrTerminatorKind::Fallthrough(target) | EvmIrTerminatorKind::Jump(target) => {
            visit(target)
        }
        EvmIrTerminatorKind::Branch { then_block, else_block, .. } => {
            visit(then_block);
            visit(else_block);
        }
        EvmIrTerminatorKind::Switch { default, cases, .. } => {
            visit(default);
            for (_, target) in cases {
                visit(target);
            }
        }
        EvmIrTerminatorKind::Return { .. }
        | EvmIrTerminatorKind::Revert { .. }
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::SelfDestruct { .. }
        | EvmIrTerminatorKind::RawOpcode(_) => {}
    }
}

/// An error produced while parsing the EVM IR text format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrParseError {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub col: usize,
    /// Human-readable message.
    pub msg: String,
    /// Source line captured for snippet rendering.
    pub line_text: String,
}

impl std_fmt::Display for EvmIrParseError {
    fn fmt(&self, f: &mut std_fmt::Formatter<'_>) -> std_fmt::Result {
        writeln!(f, "EVM IR parse error at line {}, col {}: {}", self.line, self.col, self.msg)?;
        if !self.line_text.is_empty() {
            writeln!(f, "   |")?;
            writeln!(f, "{:>3} | {}", self.line, self.line_text)?;
            let caret_pad = " ".repeat(self.col.saturating_sub(1));
            write!(f, "   | {caret_pad}^")?;
        }
        Ok(())
    }
}

impl std::error::Error for EvmIrParseError {}

/// An error produced while validating EVM IR.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrVerifyError {
    /// Human-readable validation failure.
    pub msg: String,
}

impl EvmIrVerifyError {
    fn new(msg: impl Into<String>) -> Self {
        Self { msg: msg.into() }
    }

    fn in_block(block: EvmIrBlockId, msg: impl Into<String>) -> Self {
        Self::new(format!("block {}: {}", block.index(), msg.into()))
    }
}

impl std_fmt::Display for EvmIrVerifyError {
    fn fmt(&self, f: &mut std_fmt::Formatter<'_>) -> std_fmt::Result {
        write!(f, "EVM IR verification failed: {}", self.msg)
    }
}

impl std::error::Error for EvmIrVerifyError {}

fn display_block<'a>(
    module: &'a EvmIrModule,
    block_id: EvmIrBlockId,
    block: &'a EvmIrBlock,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let entry = if module.entry_block == Some(block_id) { " (entry)" } else { "" };
        let cold = if block.metadata.hotness == EvmIrBlockHotness::Cold { " [cold]" } else { "" };
        write!(f, "{}{}{}", block.label, entry, cold)?;
        if !block.entry_stack.is_empty() {
            write!(f, " (in ")?;
            for (i, &value) in block.entry_stack.iter().enumerate() {
                if i != 0 {
                    write!(f, ", ")?;
                }
                write!(f, "{}", display_value(module, value))?;
            }
            write!(f, ")")?;
        }
        writeln!(f, ":")?;
        for inst in &block.instructions {
            writeln!(f, "  {}", display_instruction(module, inst))?;
        }
        if let Some(term) = &block.terminator {
            writeln!(f, "  {}", display_terminator(module, term))?;
        }
        Ok(())
    })
}

fn display_instruction<'a>(
    module: &'a EvmIrModule,
    inst: &'a EvmIrInstruction,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        if let Some(result) = inst.result {
            write!(f, "{} = ", display_value(module, result))?;
        }
        write!(f, "{}", inst.mnemonic())?;
        if !inst.operands.is_empty() {
            write!(
                f,
                " {}",
                inst.operands.iter().map(|operand| display_operand(module, operand)).format(", ")
            )?;
        }
        write!(
            f,
            "{}",
            display_metadata(&inst.metadata, Some(default_instruction_stack_effect(inst)))
        )
    })
}

fn display_terminator<'a>(
    module: &'a EvmIrModule,
    term: &'a EvmIrTerminator,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        match &term.kind {
            EvmIrTerminatorKind::Fallthrough(target) => {
                write!(f, "fallthrough {}", display_block_id(module, *target))?;
            }
            EvmIrTerminatorKind::Jump(target) => {
                write!(f, "jump {}", display_block_id(module, *target))?;
            }
            EvmIrTerminatorKind::Branch { condition, then_block, else_block } => {
                write!(
                    f,
                    "br {}, {}, {}",
                    display_operand(module, condition),
                    display_block_id(module, *then_block),
                    display_block_id(module, *else_block)
                )?;
            }
            EvmIrTerminatorKind::Switch { value, default, cases } => {
                write!(
                    f,
                    "switch {}, default {}, [",
                    display_operand(module, value),
                    display_block_id(module, *default)
                )?;
                write!(
                    f,
                    "{}",
                    cases.iter().format_with(", ", |f, (case, target)| {
                        write!(
                            f,
                            "{} => {}",
                            display_operand(module, case),
                            display_block_id(module, *target)
                        )
                    })
                )?;
                write!(f, "]")?;
            }
            EvmIrTerminatorKind::Return { offset, size } => {
                write!(
                    f,
                    "return {}, {}",
                    display_operand(module, offset),
                    display_operand(module, size)
                )?;
            }
            EvmIrTerminatorKind::Revert { offset, size } => {
                write!(
                    f,
                    "revert {}, {}",
                    display_operand(module, offset),
                    display_operand(module, size)
                )?;
            }
            EvmIrTerminatorKind::Stop => write!(f, "stop")?,
            EvmIrTerminatorKind::Invalid => write!(f, "invalid")?,
            EvmIrTerminatorKind::SelfDestruct { recipient } => {
                write!(f, "selfdestruct {}", display_operand(module, recipient))?;
            }
            EvmIrTerminatorKind::RawOpcode(opcode) => {
                write!(f, "terminal 0x{opcode:02x}")?;
            }
        }
        write!(
            f,
            "{}",
            display_metadata(&term.metadata, Some(default_terminator_stack_effect(&term.kind)))
        )
    })
}

fn display_metadata(
    metadata: &EvmIrMetadata,
    default_stack: Option<EvmIrStackEffect>,
) -> impl fmt::Display + '_ {
    enum Field<'a> {
        Stack(EvmIrStackEffect),
        Attr(&'a EvmIrMetadataItem),
    }

    fn display_field(field: Field<'_>) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| match field {
            Field::Stack(effect) => write!(f, "stack={}->{}", effect.inputs, effect.outputs),
            Field::Attr(item) => {
                write!(f, "{}", item.key)?;
                if let Some(value) = &item.value {
                    write!(f, "={value}")?;
                }
                Ok(())
            }
        })
    }

    fmt::from_fn(move |f| {
        if metadata.is_empty() {
            return Ok(());
        }
        let mut fields =
            Vec::with_capacity(metadata.attrs.len() + usize::from(metadata.stack.is_some()));
        if let Some(stack) = metadata.stack
            && Some(stack) != default_stack
        {
            fields.push(Field::Stack(stack));
        }
        fields.extend(metadata.attrs.iter().map(Field::Attr));
        if fields.is_empty() {
            return Ok(());
        }
        write!(f, " !meta({})", fields.into_iter().map(display_field).format(", "))
    })
}

pub(super) fn default_instruction_stack_effect(inst: &EvmIrInstruction) -> EvmIrStackEffect {
    match &inst.kind {
        EvmIrInstructionKind::Stack(op) => op.stack_effect(),
        EvmIrInstructionKind::Operation(_) if is_encoded_push_instruction(inst) => {
            EvmIrStackEffect::new(0, 1)
        }
        EvmIrInstructionKind::Operation(_) => EvmIrStackEffect::new(
            inst.operands.len().try_into().unwrap_or(u16::MAX),
            u16::from(inst.result.is_some()),
        ),
    }
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
        | EvmIrTerminatorKind::Jump(_)
        | EvmIrTerminatorKind::Stop
        | EvmIrTerminatorKind::Invalid
        | EvmIrTerminatorKind::RawOpcode(_) => EvmIrStackEffect::new(0, 0),
    }
}

pub(super) fn is_encoded_push_instruction(inst: &EvmIrInstruction) -> bool {
    matches!(
        &inst.kind,
        EvmIrInstructionKind::Operation(mnemonic)
            if matches!(mnemonic.as_str(), "push" | "push_deferred" | "push_immutable")
    )
}

fn display_operand<'a>(
    module: &'a EvmIrModule,
    operand: &'a EvmIrOperand,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match operand {
        EvmIrOperand::Value(value) => write!(f, "{}", display_value(module, *value)),
        EvmIrOperand::Immediate(value) => write!(f, "{}", display_u256(*value)),
        EvmIrOperand::Block(block) => write!(f, "{}", display_block_id(module, *block)),
        EvmIrOperand::Symbol(symbol) => write!(f, "{symbol}"),
    })
}

fn display_value(module: &EvmIrModule, value: EvmIrValueId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "%{}", module.values[value].name))
}

fn display_block_id(module: &EvmIrModule, block: EvmIrBlockId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "{}", module.blocks[block].label))
}

fn display_u256(value: U256) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        if let Ok(value) = u64::try_from(value)
            && value < 1000
        {
            write!(f, "{value}")
        } else {
            write!(f, "{value:#x}")
        }
    })
}

#[derive(Clone, Debug)]
struct ParsedBlockHeader {
    label: String,
    entry: bool,
    hotness: EvmIrBlockHotness,
    /// Incoming stack-word names from an `(in %a, %b)` signature, top first.
    entry_stack: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyEnd {
    Eof,
    Brace,
}

struct Parser<'a> {
    input: &'a str,
    pos: usize,
    line: usize,
    col: usize,
}

impl<'a> Parser<'a> {
    fn new(input: &'a str) -> Self {
        Self { input, pos: 0, line: 1, col: 1 }
    }

    fn is_eof(&self) -> bool {
        self.pos >= self.input.len()
    }

    fn peek_char(&self) -> Option<char> {
        self.input[self.pos..].chars().next()
    }

    fn advance(&mut self) -> Option<char> {
        let c = self.peek_char()?;
        self.pos += c.len_utf8();
        if c == '\n' {
            self.line += 1;
            self.col = 1;
        } else {
            self.col += 1;
        }
        Some(c)
    }

    fn skip_inline_whitespace(&mut self) {
        while matches!(self.peek_char(), Some(' ' | '\t')) {
            self.advance();
        }
    }

    fn skip_inline(&mut self) {
        self.skip_inline_whitespace();
    }

    fn skip_to_eol(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn skip_blank_and_comments(&mut self) {
        loop {
            self.skip_inline_whitespace();
            match self.peek_char() {
                Some('\n' | '\r') => {
                    self.advance();
                }
                Some('/') if self.input[self.pos..].starts_with("//") => self.skip_to_eol(),
                Some(';') => {
                    let rest = self.input[self.pos..].trim_start_matches(';').trim_start();
                    if rest.starts_with("evm module") {
                        break;
                    }
                    self.skip_to_eol();
                }
                _ => break,
            }
        }
    }

    fn error(&self, msg: impl Into<String>) -> EvmIrParseError {
        EvmIrParseError {
            line: self.line,
            col: self.col,
            msg: msg.into(),
            line_text: self.current_line_text(),
        }
    }

    fn current_line_text(&self) -> String {
        let bytes = self.input.as_bytes();
        let pos = self.pos.min(bytes.len());
        let mut start = pos;
        while start > 0 && bytes[start - 1] != b'\n' {
            start -= 1;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        self.input[start..end].trim_end_matches('\r').to_string()
    }

    fn expect_keyword(&mut self, kw: &str) -> Result<(), EvmIrParseError> {
        self.skip_inline();
        if self.input[self.pos..].starts_with(kw) {
            for _ in 0..kw.chars().count() {
                self.advance();
            }
            Ok(())
        } else {
            Err(self.error(format!("expected `{kw}`")))
        }
    }

    fn expect_punct(&mut self, expected: char) -> Result<(), EvmIrParseError> {
        self.skip_inline();
        match self.peek_char() {
            Some(c) if c == expected => {
                self.advance();
                Ok(())
            }
            Some(c) => Err(self.error(format!("expected `{expected}`, found `{c}`"))),
            None => Err(self.error(format!("expected `{expected}`, found EOF"))),
        }
    }

    fn try_punct(&mut self, expected: char) -> bool {
        self.skip_inline();
        if self.peek_char() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn parse_ident(&mut self) -> Result<&'a str, EvmIrParseError> {
        self.skip_inline();
        let start = self.pos;
        match self.peek_char() {
            Some(c) if is_ident_start(c) => {
                self.advance();
            }
            _ => return Err(self.error("expected identifier")),
        }
        while let Some(c) = self.peek_char() {
            if is_ident_continue(c) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(&self.input[start..self.pos])
    }

    fn parse_uint_literal(&mut self) -> Result<U256, EvmIrParseError> {
        self.skip_inline();
        let start = self.pos;
        if self.input[self.pos..].starts_with("0x") || self.input[self.pos..].starts_with("0X") {
            self.advance();
            self.advance();
            while let Some(c) = self.peek_char() {
                if c.is_ascii_hexdigit() {
                    self.advance();
                } else {
                    break;
                }
            }
            let s = &self.input[start..self.pos];
            if s.len() == 2 {
                return Err(self.error("expected hex digits"));
            }
            U256::from_str_radix(&s[2..], 16).map_err(|e| self.error(format!("invalid hex: {e}")))
        } else if matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    self.advance();
                } else {
                    break;
                }
            }
            let s = &self.input[start..self.pos];
            s.parse::<U256>().map_err(|e| self.error(format!("invalid integer: {e}")))
        } else {
            Err(self.error("expected integer literal"))
        }
    }

    fn parse_module(&mut self) -> Result<EvmIrModule, EvmIrParseError> {
        self.skip_blank_and_comments();
        let name = if self.try_punct(';') {
            self.expect_keyword("evm")?;
            self.expect_keyword("module")?;
            self.expect_punct('@')?;
            let name = self.parse_ident()?.to_string();
            self.skip_to_eol();
            name
        } else {
            "module".to_string()
        };

        let mut module = EvmIrModule::new(name);
        self.skip_blank_and_comments();
        let legacy_function_wrapper = self.input[self.pos..].starts_with("fn");
        if legacy_function_wrapper {
            self.expect_keyword("fn")?;
            self.expect_punct('@')?;
            let _legacy_function_name = self.parse_ident()?;
            self.expect_punct('{')?;
            self.parse_program_body(&mut module, BodyEnd::Brace)?;
        } else {
            self.parse_program_body(&mut module, BodyEnd::Eof)?;
        }
        Ok(module)
    }

    fn parse_program_body(
        &mut self,
        module: &mut EvmIrModule,
        body_end: BodyEnd,
    ) -> Result<(), EvmIrParseError> {
        self.skip_blank_and_comments();
        let body_pos = self.pos;
        let body_line = self.line;
        let body_col = self.col;
        let mut block_labels = FxHashMap::default();

        loop {
            self.skip_blank_and_comments();
            if self.is_eof() {
                if body_end == BodyEnd::Brace {
                    return Err(self.error("unterminated EVM IR block body"));
                }
                break;
            }
            if body_end == BodyEnd::Brace && self.peek_char() == Some('}') {
                break;
            }
            if let Some(header) = self.try_parse_block_header()? {
                if block_labels.contains_key(&header.label) {
                    return Err(self.error(format!("duplicate block `{}`", header.label)));
                }
                let block_id = module.add_block(EvmIrBlock::new(header.label.clone()));
                block_labels.insert(header.label, block_id);
                self.skip_to_eol();
            } else {
                self.skip_to_eol();
            }
        }

        if block_labels.is_empty() {
            return Err(self.error("program must contain at least one block"));
        }

        self.pos = body_pos;
        self.line = body_line;
        self.col = body_col;

        let mut current_block = None;
        let mut value_labels = FxHashMap::default();
        let mut defined_values = FxHashSet::default();
        loop {
            self.skip_blank_and_comments();
            if self.is_eof() {
                if body_end == BodyEnd::Brace {
                    return Err(self.error("unterminated EVM IR block body"));
                }
                break;
            }
            if body_end == BodyEnd::Brace && self.try_punct('}') {
                break;
            }
            if let Some(header) = self.try_parse_block_header()? {
                let block_id = block_labels[&header.label];
                if header.entry {
                    module.entry_block = Some(block_id);
                }
                module.blocks[block_id].metadata.hotness = header.hotness;
                let mut entry_stack = Vec::with_capacity(header.entry_stack.len());
                for name in &header.entry_stack {
                    entry_stack.push(value_id(module, &mut value_labels, name));
                }
                module.blocks[block_id].entry_stack = entry_stack;
                current_block = Some(block_id);
                self.skip_to_eol();
                continue;
            }

            let block =
                current_block.ok_or_else(|| self.error("instruction outside of any block"))?;
            self.parse_instruction_or_terminator(
                module,
                block,
                &block_labels,
                &mut value_labels,
                &mut defined_values,
            )?;
        }

        Ok(())
    }

    fn try_parse_block_header(&mut self) -> Result<Option<ParsedBlockHeader>, EvmIrParseError> {
        let save = (self.pos, self.line, self.col);
        self.skip_inline_whitespace();
        let Some(label) = self.try_parse_block_label_text()? else {
            self.restore(save);
            return Ok(None);
        };

        let mut entry = false;
        self.skip_inline_whitespace();
        if self.input[self.pos..].starts_with("(entry)") {
            for _ in 0.."(entry)".len() {
                self.advance();
            }
            entry = true;
        }

        let mut hotness = EvmIrBlockHotness::Hot;
        self.skip_inline_whitespace();
        if self.try_punct('[') {
            let key = self.parse_ident()?;
            if key == "cold" {
                hotness = EvmIrBlockHotness::Cold;
            } else if key == "hot" {
                hotness = EvmIrBlockHotness::Hot;
            } else if key == "hotness" {
                self.expect_punct('=')?;
                let value = self.parse_ident()?;
                hotness = EvmIrBlockHotness::parse(value)
                    .ok_or_else(|| self.error(format!("unknown block hotness `{value}`")))?;
            } else {
                return Err(self.error(format!("unknown block metadata `{key}`")));
            }
            self.expect_punct(']')?;
        }

        // Optional incoming stack signature: `(in %a, %b)`.
        let mut entry_stack = Vec::new();
        self.skip_inline_whitespace();
        let save_in = (self.pos, self.line, self.col);
        if self.try_punct('(') {
            self.skip_inline_whitespace();
            let keyword = if matches!(self.peek_char(), Some(c) if is_ident_start(c)) {
                Some(self.parse_ident()?)
            } else {
                None
            };
            if keyword == Some("in") {
                loop {
                    self.skip_inline_whitespace();
                    if self.try_punct(')') {
                        break;
                    }
                    entry_stack.push(self.parse_value_name()?);
                    self.skip_inline_whitespace();
                    if self.try_punct(',') {
                        continue;
                    }
                    self.expect_punct(')')?;
                    break;
                }
            } else {
                self.restore(save_in);
            }
        }

        self.skip_inline_whitespace();
        if self.peek_char() != Some(':') {
            self.restore(save);
            return Ok(None);
        }
        self.advance();

        Ok(Some(ParsedBlockHeader { label, entry, hotness, entry_stack }))
    }

    fn try_parse_block_label_text(&mut self) -> Result<Option<String>, EvmIrParseError> {
        self.skip_inline();
        if !self.input[self.pos..].starts_with("bb") {
            return Ok(None);
        }
        let start = self.pos;
        self.advance();
        self.advance();
        let digits_start = self.pos;
        while matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            self.advance();
        }
        if self.pos == digits_start {
            return Err(self.error("expected block number after `bb`"));
        }
        Ok(Some(self.input[start..self.pos].to_string()))
    }

    fn restore(&mut self, saved: (usize, usize, usize)) {
        (self.pos, self.line, self.col) = saved;
    }

    fn parse_instruction_or_terminator(
        &mut self,
        module: &mut EvmIrModule,
        block: EvmIrBlockId,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<(), EvmIrParseError> {
        self.skip_inline_whitespace();
        if module.blocks[block].terminator.is_some() {
            return Err(self.error(format!(
                "instruction after terminator in block `{}`",
                module.blocks[block].label
            )));
        }

        let result = self.try_parse_result(module, value_labels, defined_values)?;
        let mnemonic = self.parse_ident()?.to_string();
        if let Some(kind) = self.parse_terminator_kind(
            &mnemonic,
            module,
            block_labels,
            value_labels,
            defined_values,
        )? {
            if result.is_some() {
                return Err(self.error("terminator cannot produce a result"));
            }
            let metadata = self.parse_metadata()?;
            module.blocks[block].terminator = Some(EvmIrTerminator { kind, metadata });
            self.skip_to_eol();
            return Ok(());
        }

        let operands =
            self.parse_operand_list(module, block_labels, value_labels, defined_values)?;
        let metadata = self.parse_metadata()?;
        let kind = EvmIrStackOp::parse(&mnemonic)
            .map(EvmIrInstructionKind::Stack)
            .unwrap_or(EvmIrInstructionKind::Operation(mnemonic));
        module.blocks[block].instructions.push(EvmIrInstruction {
            result,
            kind,
            operands,
            metadata,
        });
        self.skip_to_eol();
        Ok(())
    }

    fn try_parse_result(
        &mut self,
        module: &mut EvmIrModule,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<Option<EvmIrValueId>, EvmIrParseError> {
        let save = (self.pos, self.line, self.col);
        if self.peek_char() != Some('%') {
            return Ok(None);
        }
        let name = self.parse_value_name()?;
        self.skip_inline_whitespace();
        if self.peek_char() != Some('=') {
            self.restore(save);
            return Ok(None);
        }
        self.advance();
        let value = value_id(module, value_labels, &name);
        if !defined_values.insert(value) {
            return Err(self.error(format!("duplicate value `%{name}`")));
        }
        Ok(Some(value))
    }

    fn parse_value_name(&mut self) -> Result<String, EvmIrParseError> {
        self.skip_inline();
        self.expect_punct('%')?;
        let start = self.pos;
        match self.peek_char() {
            Some(c) if is_ident_start(c) || c.is_ascii_digit() => {
                self.advance();
            }
            _ => return Err(self.error("expected value name")),
        }
        while let Some(c) = self.peek_char() {
            if is_ident_continue(c) {
                self.advance();
            } else {
                break;
            }
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_terminator_kind(
        &mut self,
        mnemonic: &str,
        module: &mut EvmIrModule,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<Option<EvmIrTerminatorKind>, EvmIrParseError> {
        let kind = match mnemonic {
            "fallthrough" => EvmIrTerminatorKind::Fallthrough(self.parse_block_ref(block_labels)?),
            "jump" => EvmIrTerminatorKind::Jump(self.parse_block_ref(block_labels)?),
            "br" => {
                let condition =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let then_block = self.parse_block_ref(block_labels)?;
                self.expect_punct(',')?;
                let else_block = self.parse_block_ref(block_labels)?;
                EvmIrTerminatorKind::Branch { condition, then_block, else_block }
            }
            "switch" => {
                let value =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                self.expect_keyword("default")?;
                let default = self.parse_block_ref(block_labels)?;
                self.expect_punct(',')?;
                self.expect_punct('[')?;
                let mut cases = Vec::new();
                if !self.try_punct(']') {
                    loop {
                        let case =
                            self.parse_operand(module, block_labels, value_labels, defined_values)?;
                        self.expect_keyword("=>")?;
                        let target = self.parse_block_ref(block_labels)?;
                        cases.push((case, target));
                        if self.try_punct(',') {
                            continue;
                        }
                        self.expect_punct(']')?;
                        break;
                    }
                }
                EvmIrTerminatorKind::Switch { value, default, cases }
            }
            "return" => {
                let offset =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let size =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                EvmIrTerminatorKind::Return { offset, size }
            }
            "revert" => {
                let offset =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let size =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                EvmIrTerminatorKind::Revert { offset, size }
            }
            "stop" => EvmIrTerminatorKind::Stop,
            "invalid" => EvmIrTerminatorKind::Invalid,
            "selfdestruct" => {
                let recipient =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                EvmIrTerminatorKind::SelfDestruct { recipient }
            }
            "terminal" => {
                let opcode = self.parse_uint_literal()?;
                let Ok(opcode) = u8::try_from(opcode) else {
                    return Err(self.error("raw terminal opcode must fit in one byte"));
                };
                EvmIrTerminatorKind::RawOpcode(opcode)
            }
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }

    fn parse_operand_list(
        &mut self,
        module: &mut EvmIrModule,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<Vec<EvmIrOperand>, EvmIrParseError> {
        let mut operands = Vec::new();
        self.skip_inline();
        if self.at_end_of_operation() {
            return Ok(operands);
        }
        loop {
            operands.push(self.parse_operand(
                module,
                block_labels,
                value_labels,
                defined_values,
            )?);
            self.skip_inline();
            if !self.try_punct(',') {
                break;
            }
        }
        Ok(operands)
    }

    fn parse_operand(
        &mut self,
        module: &mut EvmIrModule,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        _defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<EvmIrOperand, EvmIrParseError> {
        self.skip_inline();
        if self.peek_char() == Some('%') {
            let name = self.parse_value_name()?;
            return Ok(EvmIrOperand::Value(value_id(module, value_labels, &name)));
        }
        if matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            return Ok(EvmIrOperand::Immediate(self.parse_uint_literal()?));
        }
        if self.peek_char() == Some('@') {
            self.advance();
            let symbol = self.parse_ident()?;
            return Ok(EvmIrOperand::Symbol(format!("@{symbol}")));
        }
        if self.input[self.pos..].starts_with("bb") {
            let save = (self.pos, self.line, self.col);
            if let Some(label) = self.try_parse_block_label_text()? {
                if let Some(block) = block_labels.get(&label).copied() {
                    return Ok(EvmIrOperand::Block(block));
                }
                return Err(self.error(format!("unknown block `{label}`")));
            }
            self.restore(save);
        }
        Ok(EvmIrOperand::Symbol(self.parse_ident()?.to_string()))
    }

    fn parse_block_ref(
        &mut self,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
    ) -> Result<EvmIrBlockId, EvmIrParseError> {
        let label =
            self.try_parse_block_label_text()?.ok_or_else(|| self.error("expected block label"))?;
        block_labels
            .get(&label)
            .copied()
            .ok_or_else(|| self.error(format!("unknown block `{label}`")))
    }

    fn parse_metadata(&mut self) -> Result<EvmIrMetadata, EvmIrParseError> {
        let mut metadata = EvmIrMetadata::default();
        self.skip_inline();
        if !self.try_punct('!') {
            return Ok(metadata);
        }
        self.expect_keyword("meta")?;
        self.expect_punct('(')?;
        if self.try_punct(')') {
            return Ok(metadata);
        }

        loop {
            let key = self.parse_ident()?.to_string();
            if key == "stack" {
                self.expect_punct('=')?;
                let inputs = self.parse_u16()?;
                self.expect_keyword("->")?;
                let outputs = self.parse_u16()?;
                metadata.stack = Some(EvmIrStackEffect::new(inputs, outputs));
            } else if self.try_punct('=') {
                let value = self.parse_metadata_value()?;
                metadata.attrs.push(EvmIrMetadataItem { key, value: Some(value) });
            } else {
                metadata.attrs.push(EvmIrMetadataItem { key, value: None });
            }

            if self.try_punct(',') {
                continue;
            }
            self.expect_punct(')')?;
            break;
        }
        Ok(metadata)
    }

    fn parse_metadata_value(&mut self) -> Result<String, EvmIrParseError> {
        self.skip_inline();
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c == ',' || c == ')' || c == '\n' || c == '\r' {
                break;
            }
            self.advance();
        }
        let value = self.input[start..self.pos].trim();
        if value.is_empty() {
            return Err(self.error("expected metadata value"));
        }
        Ok(value.to_string())
    }

    fn parse_u16(&mut self) -> Result<u16, EvmIrParseError> {
        let value = self.parse_uint_literal()?;
        value.try_into().map_err(|_| self.error(format!("integer `{value}` does not fit in u16")))
    }

    fn at_end_of_operation(&self) -> bool {
        matches!(self.peek_char(), None | Some('\n' | '\r' | '!' | '}'))
    }
}

fn value_id(
    module: &mut EvmIrModule,
    value_labels: &mut FxHashMap<String, EvmIrValueId>,
    name: &str,
) -> EvmIrValueId {
    if let Some(value) = value_labels.get(name).copied() {
        return value;
    }
    let value = module.add_value(name.to_string());
    value_labels.insert(name.to_string(), value);
    value
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

#[cfg(test)]
mod tests {
    use super::{parse_evm_ir_module, verify_evm_ir_module};
    use std::path::{Path, PathBuf};

    fn verify_text(input: &str) -> Result<(), String> {
        let module = parse_evm_ir_module(input).map_err(|err| err.to_string())?;
        verify_evm_ir_module(&module).map_err(|err| err.to_string())
    }

    #[test]
    fn rejects_lying_entry_signature() {
        // bb0 really exits with [%b, %a] (top %b) but bb1 declares `(in %a)`.
        let input = "\
bb0 (entry):
  %a = push 1
  %b = push 2
  jump bb1
bb1 (in %a):
  %c = add %a, %a
  stop
";
        let err = verify_text(input).unwrap_err();
        assert!(err.contains("inconsistent"), "{err}");
        assert!(err.contains("edge to `bb1`"), "{err}");
    }

    #[test]
    fn accepts_prefix_consistent_edge() {
        // bb0 exits [%top, %keep]; bb1 declares only the consumed prefix [%top].
        let input = "\
bb0 (entry):
  %keep = push 1
  %top = push 2
  jump bb1
bb1 (in %top):
  %r = iszero %top
  stop
";
        verify_text(input).unwrap();
    }

    #[test]
    fn rejects_entry_block_with_incoming_stack() {
        // %a is defined in bb1 but the entry block declares it as incoming.
        let input = "\
bb0 (entry) (in %a):
  stop
bb1:
  %a = push 1
  stop
";
        let err = verify_text(input).unwrap_err();
        assert!(err.contains("entry block must start from an empty stack"), "{err}");
    }

    #[test]
    fn rejects_physical_stack_op_underflow() {
        // `dup2` reaches depth 2 but only one word is on the stack.
        let input = "\
bb0 (entry):
  push 1
  dup2
  stop
";
        let err = verify_text(input).unwrap_err();
        assert!(err.contains("dup2"), "{err}");
        assert!(err.contains("stack has 1"), "{err}");
    }

    #[test]
    fn rejects_swap_out_of_range() {
        let input = "\
bb0 (entry):
  push 1
  push 2
  swap2
  stop
";
        let err = verify_text(input).unwrap_err();
        assert!(err.contains("swap2"), "{err}");
    }

    fn evm_ir_fixture_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("..")
            .join("..")
            .join("tests")
            .join("ui")
            .join("codegen")
            .join("evm-ir")
    }

    #[test]
    fn round_trip_all_evm_ir_fixtures() {
        let dir = evm_ir_fixture_dir();
        assert!(dir.exists(), "EVM IR fixture dir not found: {}", dir.display());

        let mut failures = Vec::new();
        let mut count = 0usize;
        for entry in std::fs::read_dir(&dir).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().and_then(|s| s.to_str()) != Some("evmir") {
                continue;
            }
            count += 1;
            if let Err(err) = round_trip_fixture(&path) {
                let name = path.file_name().unwrap().to_string_lossy();
                failures.push(format!("{name}: {err}"));
            }
        }

        assert!(count > 0, "no .evmir fixtures found in {}", dir.display());
        assert!(
            failures.is_empty(),
            "{} EVM IR round-trip failure(s):\n  {}",
            failures.len(),
            failures.join("\n  ")
        );
    }

    #[test]
    fn parser_rejects_instructions_after_terminator() {
        let input = "\
; evm module @m

fn @f {
  bb0 (entry):
    stop
    invalid
}
";
        let err = parse_evm_ir_module(input).unwrap_err().to_string();
        assert!(err.contains("instruction after terminator"), "{err}");
    }

    fn round_trip_fixture(path: &Path) -> Result<(), String> {
        #[allow(clippy::disallowed_methods)]
        let input = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        let print1 =
            parse_evm_ir_module(&input).map_err(|err| err.to_string())?.to_text().to_string();
        let print2 =
            parse_evm_ir_module(&print1).map_err(|err| err.to_string())?.to_text().to_string();
        if print1 != print2 {
            return Err(first_diff(&print1, &print2)
                .map(|(line, a, b)| {
                    format!("first diff at line {line}:\n  first:  {a}\n  second: {b}")
                })
                .unwrap_or_else(|| "printed text differs".to_string()));
        }
        Ok(())
    }

    fn first_diff<'a>(a: &'a str, b: &'a str) -> Option<(usize, &'a str, &'a str)> {
        a.lines()
            .zip(b.lines())
            .enumerate()
            .find(|(_, (lhs, rhs))| lhs != rhs)
            .map(|(index, (lhs, rhs))| (index + 1, lhs, rhs))
    }
}

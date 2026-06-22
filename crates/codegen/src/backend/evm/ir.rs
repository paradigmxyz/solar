//! EVM backend IR.
//!
//! This module defines the target-specific Machine-IR-like boundary between
//! MIR lowering and final EVM assembly. It models backend basic blocks,
//! stack-value instructions, terminators, and metadata. The parser/printer at
//! the bottom of the file provide a text format for tests and debugging; the IR
//! itself is not defined by that serialization.

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
    /// A unique identifier for a stack value in EVM IR.
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
    /// Stack values known to this program.
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

    /// Allocates a named stack value.
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

/// A named stack value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrValue {
    /// Stable textual value name, without the leading `%`.
    pub name: String,
}

/// A non-terminating backend instruction.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EvmIrInstruction {
    /// Optional stack value produced by this instruction.
    pub result: Option<EvmIrValueId>,
    /// EVM opcode or backend pseudo-op mnemonic.
    pub mnemonic: String,
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
            mnemonic: mnemonic.into(),
            operands,
            metadata: EvmIrMetadata::default(),
        }
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
}

/// An instruction or terminator operand.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum EvmIrOperand {
    /// Stack value reference.
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
            Self::ColdLayout => "cold-layout",
            Self::TerminalDedup => "terminal-dedup",
        }
    }

    /// Runs this pass on an EVM IR module.
    pub fn run(self, module: &mut EvmIrModule) -> bool {
        match self {
            Self::None => false,
            Self::ColdLayout => move_cold_terminal_blocks(module),
            Self::TerminalDedup => deduplicate_terminal_blocks(module),
        }
    }

    /// Looks up a pass by command-line name.
    #[must_use]
    pub fn by_name(name: &str) -> Option<Self> {
        Some(match name {
            "none" => Self::None,
            "cold-layout" => Self::ColdLayout,
            "terminal-dedup" => Self::TerminalDedup,
            _ => return None,
        })
    }
}

/// All EVM IR passes exposed by `solar evm-opt`.
pub const EVM_IR_PASSES: &[EvmIrPass] =
    &[EvmIrPass::None, EvmIrPass::ColdLayout, EvmIrPass::TerminalDedup];

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
        }
        let term = block.terminator.as_ref().expect("checked above");
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
    }

    for (block_id, block) in module.blocks.iter_enumerated() {
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
        EvmIrTerminatorKind::Jump(_) | EvmIrTerminatorKind::Stop | EvmIrTerminatorKind::Invalid => {
        }
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
        EvmIrTerminatorKind::Jump(target) => visit(*target)?,
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
        | EvmIrTerminatorKind::SelfDestruct { .. } => {}
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
    if block.metadata.hotness != EvmIrBlockHotness::Cold || block.terminator.is_none() {
        return false;
    }
    let previous = EvmIrBlockId::from_usize(block_id.index() - 1);
    module.blocks[previous].terminator.is_some()
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
    // A replacement block still needs one jump terminator. Avoid growing tiny
    // blocks such as `invalid` or `revert 0, 0`; use this pass only when a
    // repeated block has enough instruction body to amortize the jump.
    block.instructions.len() >= 2
}

fn is_evm_terminal(kind: &EvmIrTerminatorKind) -> bool {
    matches!(
        kind,
        EvmIrTerminatorKind::Return { .. }
            | EvmIrTerminatorKind::Revert { .. }
            | EvmIrTerminatorKind::Stop
            | EvmIrTerminatorKind::Invalid
            | EvmIrTerminatorKind::SelfDestruct { .. }
    )
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
        instructions.push(TerminalInstructionKey {
            result,
            mnemonic: inst.mnemonic.clone(),
            operands,
        });
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
    mnemonic: String,
    operands: Vec<TerminalOperandKey>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
enum TerminalTerminatorKey {
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
        EvmIrTerminatorKind::Jump(target) => visit(target),
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
        | EvmIrTerminatorKind::SelfDestruct { .. } => {}
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
        writeln!(f, "{}{}{}:", block.label, entry, cold)?;
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
        write!(f, "{}", inst.mnemonic)?;
        if !inst.operands.is_empty() {
            write!(
                f,
                " {}",
                inst.operands.iter().map(|operand| display_operand(module, operand)).format(", ")
            )?;
        }
        write!(f, "{}", display_metadata(&inst.metadata))
    })
}

fn display_terminator<'a>(
    module: &'a EvmIrModule,
    term: &'a EvmIrTerminator,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        match &term.kind {
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
        }
        write!(f, "{}", display_metadata(&term.metadata))
    })
}

fn display_metadata(metadata: &EvmIrMetadata) -> impl fmt::Display + '_ {
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
        if let Some(stack) = metadata.stack {
            fields.push(Field::Stack(stack));
        }
        fields.extend(metadata.attrs.iter().map(Field::Attr));
        write!(f, " !meta({})", fields.into_iter().map(display_field).format(", "))
    })
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

        self.skip_inline_whitespace();
        if self.peek_char() != Some(':') {
            self.restore(save);
            return Ok(None);
        }
        self.advance();

        Ok(Some(ParsedBlockHeader { label, entry, hotness }))
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
        module.blocks[block].instructions.push(EvmIrInstruction {
            result,
            mnemonic,
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
    use super::parse_evm_ir_module;
    use std::path::{Path, PathBuf};

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

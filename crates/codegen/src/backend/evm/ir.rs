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
    /// A unique identifier for a function in EVM IR.
    pub struct EvmIrFunctionId;
}

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
    /// Module/contract name.
    pub name: String,
    /// Functions in insertion order.
    pub functions: IndexVec<EvmIrFunctionId, EvmIrFunction>,
}

impl EvmIrModule {
    /// Creates an empty EVM IR module.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(is_valid_ident(&name), "invalid EVM IR module name `{name}`");
        Self { name, functions: IndexVec::new() }
    }

    /// Adds a function to the module.
    pub fn add_function(&mut self, function: EvmIrFunction) -> EvmIrFunctionId {
        self.functions.push(function)
    }

    /// Returns the function for the given ID.
    #[must_use]
    pub fn function(&self, id: EvmIrFunctionId) -> &EvmIrFunction {
        &self.functions[id]
    }

    /// Returns a mutable reference to the function for the given ID.
    pub fn function_mut(&mut self, id: EvmIrFunctionId) -> &mut EvmIrFunction {
        &mut self.functions[id]
    }

    /// Returns the canonical EVM IR text-format representation.
    pub fn to_text(&self) -> impl fmt::Display + '_ {
        fmt::from_fn(move |f| {
            writeln!(f, "; evm module @{}", self.name)?;
            if !self.functions.is_empty() {
                writeln!(f)?;
            }
            write!(f, "{}", self.functions.iter().map(EvmIrFunction::to_text).format("\n"))
        })
    }
}

/// An EVM IR function.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct EvmIrFunction {
    /// Function name.
    pub name: String,
    /// Basic blocks in layout order.
    pub blocks: IndexVec<EvmIrBlockId, EvmIrBlock>,
    /// The entry block, if one has been created.
    pub entry_block: Option<EvmIrBlockId>,
    /// Stack values known to this function.
    pub values: IndexVec<EvmIrValueId, EvmIrValue>,
}

impl EvmIrFunction {
    /// Creates an empty EVM IR function.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        let name = name.into();
        assert!(is_valid_ident(&name), "invalid EVM IR function name `{name}`");
        Self { name, blocks: IndexVec::new(), entry_block: None, values: IndexVec::new() }
    }

    /// Adds a block to the function.
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
            writeln!(f, "fn @{} {{", self.name)?;
            write!(
                f,
                "{}",
                self.blocks.iter_enumerated().format_with("", |f, (block_id, block)| {
                    write!(f, "{}", display_block(self, block_id, block))
                })
            )?;
            writeln!(f, "}}")
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
    /// Creates an empty block with unknown hotness.
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
    /// No block-frequency information is available.
    #[default]
    Unknown,
    /// The block is expected to be frequently executed.
    Hot,
    /// The block is expected to be infrequently executed.
    Cold,
}

impl EvmIrBlockHotness {
    /// Stable textual name.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::Unknown => "unknown",
            Self::Hot => "hot",
            Self::Cold => "cold",
        }
    }

    fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "unknown" => Self::Unknown,
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

/// Parses a single EVM IR function from the text format.
///
/// # Errors
///
/// Returns an [`EvmIrParseError`] if `input` is malformed.
pub fn parse_evm_ir_function(input: &str) -> Result<EvmIrFunction, EvmIrParseError> {
    let mut parser = Parser::new(input);
    parser.skip_blank_and_comments();
    let function = parser.parse_function()?;
    parser.skip_blank_and_comments();
    if !parser.is_eof() {
        return Err(parser.error("trailing input after function"));
    }
    Ok(function)
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

fn display_block<'a>(
    func: &'a EvmIrFunction,
    block_id: EvmIrBlockId,
    block: &'a EvmIrBlock,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        let entry = if func.entry_block == Some(block_id) { " (entry)" } else { "" };
        writeln!(f, "  {}{} [hotness={}]:", block.label, entry, block.metadata.hotness.name())?;
        for inst in &block.instructions {
            writeln!(f, "    {}", display_instruction(func, inst))?;
        }
        if let Some(term) = &block.terminator {
            writeln!(f, "    {}", display_terminator(func, term))?;
        }
        Ok(())
    })
}

fn display_instruction<'a>(
    func: &'a EvmIrFunction,
    inst: &'a EvmIrInstruction,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        if let Some(result) = inst.result {
            write!(f, "{} = ", display_value(func, result))?;
        }
        write!(f, "{}", inst.mnemonic)?;
        if !inst.operands.is_empty() {
            write!(
                f,
                " {}",
                inst.operands.iter().map(|operand| display_operand(func, operand)).format(", ")
            )?;
        }
        write!(f, "{}", display_metadata(&inst.metadata))
    })
}

fn display_terminator<'a>(
    func: &'a EvmIrFunction,
    term: &'a EvmIrTerminator,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| {
        match &term.kind {
            EvmIrTerminatorKind::Jump(target) => {
                write!(f, "jump {}", display_block_id(func, *target))?;
            }
            EvmIrTerminatorKind::Branch { condition, then_block, else_block } => {
                write!(
                    f,
                    "br {}, {}, {}",
                    display_operand(func, condition),
                    display_block_id(func, *then_block),
                    display_block_id(func, *else_block)
                )?;
            }
            EvmIrTerminatorKind::Switch { value, default, cases } => {
                write!(
                    f,
                    "switch {}, default {}, [",
                    display_operand(func, value),
                    display_block_id(func, *default)
                )?;
                write!(
                    f,
                    "{}",
                    cases.iter().format_with(", ", |f, (case, target)| {
                        write!(
                            f,
                            "{} => {}",
                            display_operand(func, case),
                            display_block_id(func, *target)
                        )
                    })
                )?;
                write!(f, "]")?;
            }
            EvmIrTerminatorKind::Return { offset, size } => {
                write!(
                    f,
                    "return {}, {}",
                    display_operand(func, offset),
                    display_operand(func, size)
                )?;
            }
            EvmIrTerminatorKind::Revert { offset, size } => {
                write!(
                    f,
                    "revert {}, {}",
                    display_operand(func, offset),
                    display_operand(func, size)
                )?;
            }
            EvmIrTerminatorKind::Stop => write!(f, "stop")?,
            EvmIrTerminatorKind::Invalid => write!(f, "invalid")?,
            EvmIrTerminatorKind::SelfDestruct { recipient } => {
                write!(f, "selfdestruct {}", display_operand(func, recipient))?;
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
    func: &'a EvmIrFunction,
    operand: &'a EvmIrOperand,
) -> impl fmt::Display + 'a {
    fmt::from_fn(move |f| match operand {
        EvmIrOperand::Value(value) => write!(f, "{}", display_value(func, *value)),
        EvmIrOperand::Immediate(value) => write!(f, "{}", display_u256(*value)),
        EvmIrOperand::Block(block) => write!(f, "{}", display_block_id(func, *block)),
        EvmIrOperand::Symbol(symbol) => write!(f, "{symbol}"),
    })
}

fn display_value(func: &EvmIrFunction, value: EvmIrValueId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "%{}", func.values[value].name))
}

fn display_block_id(func: &EvmIrFunction, block: EvmIrBlockId) -> impl fmt::Display + '_ {
    fmt::from_fn(move |f| write!(f, "{}", func.blocks[block].label))
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
        loop {
            self.skip_blank_and_comments();
            if self.is_eof() {
                break;
            }
            module.add_function(self.parse_function()?);
        }
        Ok(module)
    }

    fn parse_function(&mut self) -> Result<EvmIrFunction, EvmIrParseError> {
        self.skip_blank_and_comments();
        self.expect_keyword("fn")?;
        self.expect_punct('@')?;
        let name = self.parse_ident()?.to_string();
        self.expect_punct('{')?;
        self.skip_blank_and_comments();

        let mut func = EvmIrFunction::new(name);
        let body_pos = self.pos;
        let body_line = self.line;
        let body_col = self.col;
        let mut block_labels = FxHashMap::default();

        loop {
            self.skip_blank_and_comments();
            if self.is_eof() {
                return Err(self.error("unterminated function body"));
            }
            if self.peek_char() == Some('}') {
                break;
            }
            if let Some(header) = self.try_parse_block_header()? {
                if block_labels.contains_key(&header.label) {
                    return Err(self.error(format!("duplicate block `{}`", header.label)));
                }
                let block_id = func.add_block(EvmIrBlock::new(header.label.clone()));
                block_labels.insert(header.label, block_id);
                self.skip_to_eol();
            } else {
                self.skip_to_eol();
            }
        }

        if block_labels.is_empty() {
            return Err(self.error("function must contain at least one block"));
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
                return Err(self.error("unterminated function body"));
            }
            if self.try_punct('}') {
                break;
            }
            if let Some(header) = self.try_parse_block_header()? {
                let block_id = block_labels[&header.label];
                if header.entry {
                    func.entry_block = Some(block_id);
                }
                func.blocks[block_id].metadata.hotness = header.hotness;
                current_block = Some(block_id);
                self.skip_to_eol();
                continue;
            }

            let block =
                current_block.ok_or_else(|| self.error("instruction outside of any block"))?;
            self.parse_instruction_or_terminator(
                &mut func,
                block,
                &block_labels,
                &mut value_labels,
                &mut defined_values,
            )?;
        }

        Ok(func)
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

        let mut hotness = EvmIrBlockHotness::Unknown;
        self.skip_inline_whitespace();
        if self.try_punct('[') {
            let key = self.parse_ident()?.to_string();
            if key != "hotness" {
                return Err(self.error(format!("unknown block metadata `{key}`")));
            }
            self.expect_punct('=')?;
            let value = self.parse_ident()?;
            hotness = EvmIrBlockHotness::parse(value)
                .ok_or_else(|| self.error(format!("unknown block hotness `{value}`")))?;
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
        func: &mut EvmIrFunction,
        block: EvmIrBlockId,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<(), EvmIrParseError> {
        self.skip_inline_whitespace();
        if func.blocks[block].terminator.is_some() {
            return Err(self.error(format!(
                "instruction after terminator in block `{}`",
                func.blocks[block].label
            )));
        }

        let result = self.try_parse_result(func, value_labels, defined_values)?;
        let mnemonic = self.parse_ident()?.to_string();
        if let Some(kind) =
            self.parse_terminator_kind(&mnemonic, func, block_labels, value_labels, defined_values)?
        {
            if result.is_some() {
                return Err(self.error("terminator cannot produce a result"));
            }
            let metadata = self.parse_metadata()?;
            func.blocks[block].terminator = Some(EvmIrTerminator { kind, metadata });
            self.skip_to_eol();
            return Ok(());
        }

        let operands = self.parse_operand_list(func, block_labels, value_labels, defined_values)?;
        let metadata = self.parse_metadata()?;
        func.blocks[block].instructions.push(EvmIrInstruction {
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
        func: &mut EvmIrFunction,
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
        let value = value_id(func, value_labels, &name);
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
        func: &mut EvmIrFunction,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<Option<EvmIrTerminatorKind>, EvmIrParseError> {
        let kind = match mnemonic {
            "jump" => EvmIrTerminatorKind::Jump(self.parse_block_ref(block_labels)?),
            "br" => {
                let condition =
                    self.parse_operand(func, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let then_block = self.parse_block_ref(block_labels)?;
                self.expect_punct(',')?;
                let else_block = self.parse_block_ref(block_labels)?;
                EvmIrTerminatorKind::Branch { condition, then_block, else_block }
            }
            "switch" => {
                let value = self.parse_operand(func, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                self.expect_keyword("default")?;
                let default = self.parse_block_ref(block_labels)?;
                self.expect_punct(',')?;
                self.expect_punct('[')?;
                let mut cases = Vec::new();
                if !self.try_punct(']') {
                    loop {
                        let case =
                            self.parse_operand(func, block_labels, value_labels, defined_values)?;
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
                    self.parse_operand(func, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let size = self.parse_operand(func, block_labels, value_labels, defined_values)?;
                EvmIrTerminatorKind::Return { offset, size }
            }
            "revert" => {
                let offset =
                    self.parse_operand(func, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let size = self.parse_operand(func, block_labels, value_labels, defined_values)?;
                EvmIrTerminatorKind::Revert { offset, size }
            }
            "stop" => EvmIrTerminatorKind::Stop,
            "invalid" => EvmIrTerminatorKind::Invalid,
            "selfdestruct" => {
                let recipient =
                    self.parse_operand(func, block_labels, value_labels, defined_values)?;
                EvmIrTerminatorKind::SelfDestruct { recipient }
            }
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }

    fn parse_operand_list(
        &mut self,
        func: &mut EvmIrFunction,
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
            operands.push(self.parse_operand(func, block_labels, value_labels, defined_values)?);
            self.skip_inline();
            if !self.try_punct(',') {
                break;
            }
        }
        Ok(operands)
    }

    fn parse_operand(
        &mut self,
        func: &mut EvmIrFunction,
        block_labels: &FxHashMap<String, EvmIrBlockId>,
        value_labels: &mut FxHashMap<String, EvmIrValueId>,
        _defined_values: &mut FxHashSet<EvmIrValueId>,
    ) -> Result<EvmIrOperand, EvmIrParseError> {
        self.skip_inline();
        if self.peek_char() == Some('%') {
            let name = self.parse_value_name()?;
            return Ok(EvmIrOperand::Value(value_id(func, value_labels, &name)));
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
    func: &mut EvmIrFunction,
    value_labels: &mut FxHashMap<String, EvmIrValueId>,
    name: &str,
) -> EvmIrValueId {
    if let Some(value) = value_labels.get(name).copied() {
        return value;
    }
    let value = func.add_value(name.to_string());
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

//! Parser for the textual MIR format produced by [`Function::to_text`] and [`Module::to_text`].
//!
//! # Format
//!
//! ```text
//! ; module @Counter
//! fn @increment() {
//!   bb0 (entry):
//!     v0 = sload 0
//!     v1 = add v0, 1
//!     sstore 0, v1
//!     stop
//! }
//! ```
//!
//! # Session requirement
//!
//! Both [`parse_module`] and [`parse_function`] intern function and module
//! names via [`Symbol::intern`], which requires an active
//! [`solar_interface::Session`]. Wrap calls in `sess.enter(|| ...)`.
//!
//! # Caveats
//!
//! - This parser produces a *semantically* equivalent [`Function`]; the actual `ValueId` numbers in
//!   the result may differ from the labels in the source text. Round-tripping `parse →
//!   Function::to_text → parse` is supported, but the textual form may shift on the second print
//!   (different v-numbers).
//! - Address and fixed-bytes immediate literals are not currently parsed — they're allocated as
//!   `Immediate::uint256(0)`. If you need them, extend `parse_value`.
//! - Phi nodes are represented only as phi *instructions* (`InstKind::Phi`).

use super::{
    BasicBlock, BlockId, EffectKind, Function, FunctionId, InstKind, Instruction,
    InstructionMetadata, MemoryRegion, Module, StorageAlias, Terminator, Value, ValueId,
};
use crate::mir::{Immediate, MirType};
use alloy_primitives::U256;
use solar_data_structures::map::FxHashMap;
use solar_interface::{BytePos, Ident, Span, Symbol};
use solar_sema::hir;
use std::fmt;

// =============================================================================
// Public API
// =============================================================================

/// Parses a textual MIR module.
///
/// # Errors
///
/// Returns a [`ParseError`] if the input does not conform to the MIR
/// textual format produced by [`Module::to_text`](super::Module::to_text).
///
/// # Session
///
/// Must be called inside an active `solar_interface::Session::enter`,
/// because module and function names are interned via [`Symbol::intern`].
pub fn parse_module(input: &str) -> Result<Module, ParseError> {
    let mut p = Parser::new(input);
    p.parse_module()
}

/// Parses a single textual MIR function.
///
/// # Errors
///
/// Returns a [`ParseError`] on malformed input.
///
/// # Session
///
/// Must be called inside an active `solar_interface::Session::enter`.
pub fn parse_function(input: &str) -> Result<Function, ParseError> {
    let mut p = Parser::new(input);
    p.skip_blank_and_comments();
    let func = p.parse_function()?;
    p.skip_blank_and_comments();
    if !p.is_eof() {
        return Err(p.error("trailing input after function"));
    }
    Ok(func)
}

/// An error produced while parsing textual MIR.
#[derive(Clone, Debug)]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number (codepoints, not bytes).
    pub col: usize,
    /// Human-readable message.
    pub msg: String,
    /// The offending source line (without trailing newline), captured at
    /// the time the error was constructed. Used by [`fmt::Display`] to render
    /// a rustc/clang-style snippet with a caret.
    pub line_text: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "MIR parse error at line {}, col {}: {}", self.line, self.col, self.msg)?;
        if !self.line_text.is_empty() {
            writeln!(f, "   |")?;
            writeln!(f, "{:>3} | {}", self.line, self.line_text)?;
            // Caret aligned under col-1 spaces. This isn't tab-aware, but
            // the printer never produces tabs in MIR text so it's fine.
            let caret_pad = " ".repeat(self.col.saturating_sub(1));
            write!(f, "   | {caret_pad}^")?;
        }
        Ok(())
    }
}

impl std::error::Error for ParseError {}

// =============================================================================
// Parser implementation
// =============================================================================

/// A simple line-and-column-tracking parser over a `&str`.
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

    // ----- low-level cursor primitives -----

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
        while let Some(c) = self.peek_char() {
            if c == ' ' || c == '\t' {
                self.advance();
            } else {
                break;
            }
        }
    }

    /// Skip whitespace, newlines, and comments (`//...` and `;...`).
    ///
    /// Note: `;` is treated as a comment EXCEPT when followed by ` module @`,
    /// which is the module header marker (`; module @Name`). This lets the
    /// parser recover the module name even though `;` is otherwise comment-only.
    fn skip_blank_and_comments(&mut self) {
        loop {
            self.skip_inline_whitespace();
            match self.peek_char() {
                Some('\n') | Some('\r') => {
                    self.advance();
                }
                Some('/') if self.input[self.pos..].starts_with("//") => {
                    self.skip_to_eol();
                }
                Some(';') => {
                    // Don't eat the module header — let parse_module handle it.
                    if self.input[self.pos..]
                        .trim_start_matches(';')
                        .trim_start()
                        .starts_with("module")
                    {
                        break;
                    }
                    self.skip_to_eol();
                }
                _ => break,
            }
        }
    }

    fn skip_to_eol(&mut self) {
        while let Some(c) = self.peek_char() {
            if c == '\n' {
                break;
            }
            self.advance();
        }
    }

    fn error(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
            line: self.line,
            col: self.col,
            msg: msg.into(),
            line_text: self.current_line_text(),
        }
    }

    /// Like [`Self::error`] but uses the supplied (line, col, pos) instead of
    /// `self.line/col/pos`. Useful when the parser has already advanced past
    /// the offending token (e.g. an unknown mnemonic) and we want the caret
    /// to point back to its start.
    fn error_at(&self, line: usize, col: usize, pos: usize, msg: impl Into<String>) -> ParseError {
        // Capture the line text at `pos`, not at `self.pos`.
        let bytes = self.input.as_bytes();
        let p = pos.min(bytes.len());
        let mut start = p;
        while start > 0 && bytes[start - 1] != b'\n' {
            start -= 1;
        }
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        let line_text = self.input[start..end].trim_end_matches('\r').to_string();
        ParseError { line, col, msg: msg.into(), line_text }
    }

    /// Returns the contents of the current source line (without the trailing
    /// newline). Used by [`Self::error`] to populate
    /// [`ParseError::line_text`] for snippet rendering.
    fn current_line_text(&self) -> String {
        let bytes = self.input.as_bytes();
        let pos = self.pos.min(bytes.len());
        // Walk backward to start-of-line.
        let mut start = pos;
        while start > 0 && bytes[start - 1] != b'\n' {
            start -= 1;
        }
        // Walk forward to end-of-line.
        let mut end = start;
        while end < bytes.len() && bytes[end] != b'\n' {
            end += 1;
        }
        self.input[start..end].trim_end_matches('\r').to_string()
    }

    // ----- token-level helpers -----

    /// Skip non-newline whitespace.
    /// Used between tokens *within* a logical line (instruction).
    /// Inline comments are NOT supported on instruction lines (the printer
    /// never produces them).
    fn skip_inline(&mut self) {
        self.skip_inline_whitespace();
    }

    /// Consume an exact literal string. Returns an error if it doesn't match.
    fn expect_keyword(&mut self, kw: &str) -> Result<(), ParseError> {
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

    /// Consume one of the given punctuation characters. Returns it on success.
    fn expect_punct(&mut self, expected: char) -> Result<(), ParseError> {
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

    /// Try to consume a punctuation character. Returns true on success.
    fn try_punct(&mut self, expected: char) -> bool {
        self.skip_inline();
        if self.peek_char() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume an identifier: `[a-zA-Z_][a-zA-Z0-9_]*`.
    /// Parses a phase name such as `evm-shaped`. Unlike an identifier, a phase
    /// name may contain internal hyphens.
    fn parse_phase_name(&mut self) -> Result<String, ParseError> {
        self.skip_inline();
        let start = self.pos;
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                self.advance();
            } else {
                break;
            }
        }
        if self.pos == start {
            return Err(self.error("expected phase name"));
        }
        Ok(self.input[start..self.pos].to_string())
    }

    fn parse_ident(&mut self) -> Result<&'a str, ParseError> {
        self.skip_inline();
        let start = self.pos;
        match self.peek_char() {
            Some(c) if c.is_ascii_alphabetic() || c == '_' => {
                self.advance();
            }
            _ => return Err(self.error("expected identifier")),
        }
        while let Some(c) = self.peek_char() {
            if c.is_ascii_alphanumeric() || c == '_' {
                self.advance();
            } else {
                break;
            }
        }
        Ok(&self.input[start..self.pos])
    }

    /// Consume an unsigned integer literal: decimal `123` or hex `0xABC`.
    fn parse_uint_literal(&mut self) -> Result<U256, ParseError> {
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

    // ----- module / function parsing -----

    fn parse_module(&mut self) -> Result<Module, ParseError> {
        self.skip_blank_and_comments();

        // Optional `; module @Name [phase = ...]` header (the printer always
        // emits the name; the phase is printed only when not the default).
        let mut phase = super::MirPhase::default();
        let module_name = if self.try_punct(';') {
            self.expect_keyword("module")?;
            self.skip_inline();
            self.expect_punct('@')?;
            let name = self.parse_ident()?.to_string();
            self.skip_inline_whitespace();
            if self.try_punct('[') {
                self.expect_keyword("phase")?;
                self.skip_inline_whitespace();
                self.expect_punct('=')?;
                self.skip_inline_whitespace();
                let phase_name = self.parse_phase_name()?;
                phase = super::MirPhase::by_name(&phase_name)
                    .ok_or_else(|| self.error(format!("unknown MIR phase `{phase_name}`")))?;
                self.expect_punct(']')?;
            }
            self.skip_to_eol();
            self.skip_blank_and_comments();
            name
        } else {
            "module".to_string()
        };

        let module_ident = Ident::with_dummy_span(Symbol::intern(&module_name));
        let mut module = Module::new(module_ident);
        module.phase = phase;

        while !self.is_eof() {
            self.skip_blank_and_comments();
            if self.is_eof() {
                break;
            }
            // Skip stray `// === ... ===` comment headers between functions.
            if self.input[self.pos..].starts_with("//") {
                self.skip_to_eol();
                continue;
            }
            let func = self.parse_function()?;
            module.add_function(func);
        }

        Ok(module)
    }

    fn parse_function(&mut self) -> Result<Function, ParseError> {
        self.skip_blank_and_comments();
        self.expect_keyword("fn")?;
        self.skip_inline();
        self.expect_punct('@')?;
        let name = self.parse_ident()?.to_string();
        let func_ident = Ident::with_dummy_span(Symbol::intern(&name));
        let mut func = Function::new(func_ident);

        // Parse parameters: `(arg0: ty, arg1: ty, ...)` or `()`
        self.expect_punct('(')?;
        let mut arg_values: Vec<ValueId> = Vec::new();
        if !self.try_punct(')') {
            loop {
                let arg_name = self.parse_ident()?;
                if !arg_name.starts_with("arg") {
                    return Err(self.error(format!("expected `argN`, got `{arg_name}`")));
                }
                let _idx: u32 = arg_name[3..]
                    .parse()
                    .map_err(|_| self.error(format!("invalid arg index in `{arg_name}`")))?;
                self.expect_punct(':')?;
                let ty = self.parse_type()?;
                let index = func.params.len() as u32;
                func.params.push(ty);
                let val = func.alloc_value(Value::Arg { index, ty });
                arg_values.push(val);
                if self.try_punct(',') {
                    continue;
                }
                self.expect_punct(')')?;
                break;
            }
        }

        // Optional return type: `-> ty` or `-> (ty, ty, ...)`
        self.skip_inline();
        if self.input[self.pos..].starts_with("->") {
            self.advance();
            self.advance();
            self.skip_inline();
            if self.try_punct('(') {
                if !self.try_punct(')') {
                    loop {
                        let ty = self.parse_type()?;
                        func.returns.push(ty);
                        if self.try_punct(',') {
                            continue;
                        }
                        self.expect_punct(')')?;
                        break;
                    }
                }
            } else {
                let ty = self.parse_type()?;
                func.returns.push(ty);
            }
        }

        self.parse_function_attributes(&mut func)?;
        self.expect_punct('{')?;
        self.skip_blank_and_comments();

        // Two-pass: first scan for all `bbN:` headers to pre-allocate
        // BlockIds (so jumps to later blocks resolve correctly).
        let mut block_labels: FxHashMap<u32, BlockId> = FxHashMap::default();

        // Save position so we can rewind after the scan.
        let scan_start = self.pos;
        let scan_line = self.line;
        let scan_col = self.col;

        // First pass: walk lines, find `bbN:` patterns.
        let mut first_block: Option<u32> = None;
        loop {
            self.skip_blank_and_comments();
            if self.is_eof() {
                return Err(self.error("unterminated function body"));
            }
            if self.peek_char() == Some('}') {
                break;
            }
            // Try to parse a block header at the start of a line.
            let line_start_pos = self.pos;
            let line_start_line = self.line;
            let line_start_col = self.col;
            self.skip_inline_whitespace();
            if let Some('b') = self.peek_char()
                && self.input[self.pos..].starts_with("bb")
            {
                let save = self.pos;
                let save_line = self.line;
                let save_col = self.col;
                self.advance();
                self.advance();
                let mut idx_str = String::new();
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_digit() {
                        idx_str.push(c);
                        self.advance();
                    } else {
                        break;
                    }
                }
                if idx_str.is_empty() {
                    self.pos = save;
                    self.line = save_line;
                    self.col = save_col;
                } else {
                    // Skip optional ` (entry)` marker
                    self.skip_inline_whitespace();
                    if self.peek_char() == Some('(') {
                        while let Some(c) = self.peek_char() {
                            self.advance();
                            if c == ')' {
                                break;
                            }
                        }
                    }
                    if self.peek_char() == Some(':') {
                        let idx: u32 =
                            idx_str.parse().map_err(|_| self.error("invalid block index"))?;
                        if first_block.is_none() {
                            first_block = Some(idx);
                        }
                        // Don't allocate the entry block again — Function::new
                        // already created bb0. We need to map any first-encountered
                        // bbN to the entry block's id.
                        if block_labels.is_empty() {
                            block_labels.insert(idx, func.entry_block);
                        } else {
                            let id = func.alloc_block();
                            block_labels.insert(idx, id);
                        }
                        // Skip to end of line; the `:` consumes block header.
                        self.advance();
                        self.skip_to_eol();
                        continue;
                    } else {
                        self.pos = save;
                        self.line = save_line;
                        self.col = save_col;
                    }
                }
            }
            // Not a block header — restore to start of line and skip the line.
            self.pos = line_start_pos;
            self.line = line_start_line;
            self.col = line_start_col;
            self.skip_to_eol();
        }

        // Rewind to start of function body for the real parsing pass.
        self.pos = scan_start;
        self.line = scan_line;
        self.col = scan_col;

        // Second pass: parse blocks, instructions, and terminators.
        let mut value_labels: FxHashMap<u32, ValueId> = FxHashMap::default();
        let mut current_block: Option<BlockId> = None;

        loop {
            self.skip_blank_and_comments();
            if self.is_eof() {
                return Err(self.error("unterminated function body"));
            }
            if self.try_punct('}') {
                break;
            }

            // Try to parse a block header.
            self.skip_inline_whitespace();
            if self.input[self.pos..].starts_with("bb") {
                let save = self.pos;
                let save_line = self.line;
                let save_col = self.col;
                self.advance();
                self.advance();
                let mut idx_str = String::new();
                while let Some(c) = self.peek_char() {
                    if c.is_ascii_digit() {
                        idx_str.push(c);
                        self.advance();
                    } else {
                        break;
                    }
                }
                if !idx_str.is_empty() {
                    self.skip_inline_whitespace();
                    // Optional ` (entry)`
                    if self.peek_char() == Some('(') {
                        while let Some(c) = self.peek_char() {
                            self.advance();
                            if c == ')' {
                                break;
                            }
                        }
                        self.skip_inline_whitespace();
                    }
                    if self.peek_char() == Some(':') {
                        self.advance();
                        let idx: u32 = idx_str.parse().unwrap();
                        let bid = *block_labels
                            .get(&idx)
                            .ok_or_else(|| self.error(format!("unknown block bb{idx}")))?;
                        current_block = Some(bid);
                        self.skip_to_eol();
                        continue;
                    }
                    self.pos = save;
                    self.line = save_line;
                    self.col = save_col;
                } else {
                    self.pos = save;
                    self.line = save_line;
                    self.col = save_col;
                }
            }

            // Not a block header — must be an instruction or terminator.
            let block =
                current_block.ok_or_else(|| self.error("instruction outside of any block"))?;
            self.parse_instruction_or_terminator(
                &mut func,
                block,
                &arg_values,
                &block_labels,
                &mut value_labels,
            )?;
        }

        self.reject_unresolved_value_labels(&func, &value_labels)?;

        Ok(func)
    }

    fn parse_function_attributes(&mut self, func: &mut Function) -> Result<(), ParseError> {
        self.skip_inline();
        if !self.try_punct('[') {
            return Ok(());
        }

        loop {
            let key = self.parse_ident()?.to_string();
            match key.as_str() {
                "selector" => {
                    self.expect_punct('=')?;
                    let selector = self.parse_uint_literal()?;
                    let selector = self.u256_to_u32(selector)?;
                    func.selector = Some(selector.to_be_bytes());
                }
                _ => return Err(self.error(format!("unknown function attribute `{key}`"))),
            }

            if self.try_punct(',') {
                continue;
            }
            self.expect_punct(']')?;
            break;
        }

        Ok(())
    }

    fn parse_type(&mut self) -> Result<MirType, ParseError> {
        self.skip_inline();
        let id = self.parse_ident()?;
        // u8..u256, i8..i256, bytes1..bytes32 — split into prefix + number.
        let ty = if let Some(rest) = id.strip_prefix('u') {
            let bits: u16 =
                rest.parse().map_err(|_| self.error(format!("invalid u-type `{id}`")))?;
            MirType::UInt(bits)
        } else if let Some(rest) = id.strip_prefix('i') {
            let bits: u16 =
                rest.parse().map_err(|_| self.error(format!("invalid i-type `{id}`")))?;
            MirType::Int(bits)
        } else if let Some(rest) = id.strip_prefix("bytes") {
            let n: u8 =
                rest.parse().map_err(|_| self.error(format!("invalid bytes type `{id}`")))?;
            MirType::FixedBytes(n)
        } else {
            match id {
                "bool" => MirType::Bool,
                "address" => MirType::Address,
                "memptr" => MirType::MemPtr,
                "storageptr" => MirType::StoragePtr,
                "calldataptr" => MirType::CalldataPtr,
                "function" => MirType::Function,
                "void" => MirType::Void,
                _ => return Err(self.error(format!("unknown type `{id}`"))),
            }
        };
        Ok(ty)
    }

    /// Parses a single value reference: `argN`, `vN`, integer literal, or `true`/`false`.
    /// Allocates a fresh `Immediate` for literals.
    fn parse_value(
        &mut self,
        func: &mut Function,
        arg_values: &[ValueId],
        value_labels: &mut FxHashMap<u32, ValueId>,
    ) -> Result<ValueId, ParseError> {
        self.skip_inline();
        // Integer literal? (decimal or 0x…)
        if matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            let v = self.parse_uint_literal()?;
            return Ok(func.alloc_value(Value::Immediate(Immediate::uint256(v))));
        }
        // Identifier-like — could be argN, vN, true, false.
        let ident = self.parse_ident()?;
        if ident == "true" {
            return Ok(func.alloc_value(Value::Immediate(Immediate::bool(true))));
        }
        if ident == "false" {
            return Ok(func.alloc_value(Value::Immediate(Immediate::bool(false))));
        }
        if ident == "err" {
            // Reconstructing an already-reported error state from text: there
            // is no live diagnostic to propagate here.
            let guar = solar_interface::diagnostics::ErrorGuaranteed::new_unchecked();
            return Ok(func.alloc_value(Value::Error(guar)));
        }
        if let Some(rest) = ident.strip_prefix("arg") {
            let idx: usize =
                rest.parse().map_err(|_| self.error(format!("invalid arg `{ident}`")))?;
            return arg_values
                .get(idx)
                .copied()
                .ok_or_else(|| self.error(format!("arg{idx} out of range")));
        }
        if let Some(rest) = ident.strip_prefix('v') {
            let idx: u32 = rest
                .parse()
                .map_err(|_| self.error(format!("invalid value reference `{ident}`")))?;
            if let Some(value) = value_labels.get(&idx).copied() {
                return Ok(value);
            }
            let value = func.alloc_value(Value::Undef(MirType::uint256()));
            value_labels.insert(idx, value);
            return Ok(value);
        }
        Err(self.error(format!("expected value reference, got `{ident}`")))
    }

    fn resolve_result_label(
        &self,
        func: &mut Function,
        value_labels: &mut FxHashMap<u32, ValueId>,
        label: u32,
        inst_id: super::InstId,
    ) -> Result<(), ParseError> {
        if let Some(value) = value_labels.get(&label).copied() {
            if matches!(func.values[value], Value::Undef(_)) {
                func.values[value] = Value::Inst(inst_id);
                return Ok(());
            }
            return Err(self.error(format!("duplicate value `v{label}`")));
        }

        let value = func.alloc_value(Value::Inst(inst_id));
        value_labels.insert(label, value);
        Ok(())
    }

    fn reject_unresolved_value_labels(
        &self,
        func: &Function,
        value_labels: &FxHashMap<u32, ValueId>,
    ) -> Result<(), ParseError> {
        let mut unresolved: Vec<_> = value_labels
            .iter()
            .filter_map(|(&label, &value)| {
                matches!(func.values[value], Value::Undef(_)).then_some(label)
            })
            .collect();
        unresolved.sort_unstable();
        if let Some(label) = unresolved.first() {
            return Err(self.error(format!("undefined value `v{label}`")));
        }
        Ok(())
    }

    fn parse_block_id(
        &mut self,
        block_labels: &FxHashMap<u32, BlockId>,
    ) -> Result<BlockId, ParseError> {
        self.skip_inline();
        let id = self.parse_ident()?;
        let rest = id
            .strip_prefix("bb")
            .ok_or_else(|| self.error(format!("expected `bbN`, got `{id}`")))?;
        let idx: u32 =
            rest.parse().map_err(|_| self.error(format!("invalid block index `{id}`")))?;
        block_labels.get(&idx).copied().ok_or_else(|| self.error(format!("unknown block `{id}`")))
    }

    fn parse_function_id(&mut self) -> Result<FunctionId, ParseError> {
        self.skip_inline();
        let id = self.parse_ident()?;
        let rest = id
            .strip_prefix("fn")
            .ok_or_else(|| self.error(format!("expected `fnN`, got `{id}`")))?;
        let idx: usize =
            rest.parse().map_err(|_| self.error(format!("invalid function index `{id}`")))?;
        Ok(FunctionId::from_usize(idx))
    }

    /// Parses one instruction line (with optional `vN =` result) or a terminator.
    fn parse_instruction_or_terminator(
        &mut self,
        func: &mut Function,
        block: BlockId,
        arg_values: &[ValueId],
        block_labels: &FxHashMap<u32, BlockId>,
        value_labels: &mut FxHashMap<u32, ValueId>,
    ) -> Result<(), ParseError> {
        self.skip_inline_whitespace();

        // Optional result: `vN = ...`
        let result_label: Option<u32> = if self.input[self.pos..].starts_with('v')
            && self.input[self.pos..].chars().nth(1).is_some_and(|c| c.is_ascii_digit())
        {
            // Try to parse as `vN =`. If no `=` follows, it's a terminator using vN.
            let save_pos = self.pos;
            let save_line = self.line;
            let save_col = self.col;
            self.advance();
            let mut idx_str = String::new();
            while let Some(c) = self.peek_char() {
                if c.is_ascii_digit() {
                    idx_str.push(c);
                    self.advance();
                } else {
                    break;
                }
            }
            self.skip_inline_whitespace();
            if self.peek_char() == Some('=') {
                self.advance();
                Some(idx_str.parse().unwrap())
            } else {
                self.pos = save_pos;
                self.line = save_line;
                self.col = save_col;
                None
            }
        } else {
            None
        };

        self.skip_inline_whitespace();
        // Save the position of the mnemonic so we can produce a snippet
        // pointing at it (instead of just past it) if it turns out to be
        // unknown.
        let mnemonic_line = self.line;
        let mnemonic_col = self.col;
        let mnemonic_pos = self.pos;
        let mnemonic = self.parse_ident()?.to_string();

        // Terminators (no result).
        match mnemonic.as_str() {
            "jump" => {
                let target = self.parse_block_id(block_labels)?;
                self.set_terminator(func, block, Terminator::Jump(target));
                self.skip_to_eol();
                return Ok(());
            }
            "br" => {
                let condition = self.parse_value(func, arg_values, value_labels)?;
                self.expect_punct(',')?;
                let then_block = self.parse_block_id(block_labels)?;
                self.expect_punct(',')?;
                let else_block = self.parse_block_id(block_labels)?;
                self.set_terminator(
                    func,
                    block,
                    Terminator::Branch { condition, then_block, else_block },
                );
                self.skip_to_eol();
                return Ok(());
            }
            "switch" => {
                let value = self.parse_value(func, arg_values, value_labels)?;
                self.expect_punct(',')?;
                self.expect_keyword("default")?;
                let default = self.parse_block_id(block_labels)?;
                self.expect_punct(',')?;
                self.expect_punct('[')?;
                let mut cases = Vec::new();
                if !self.try_punct(']') {
                    loop {
                        let val = self.parse_value(func, arg_values, value_labels)?;
                        self.expect_keyword("=>")?;
                        let bid = self.parse_block_id(block_labels)?;
                        cases.push((val, bid));
                        if self.try_punct(',') {
                            continue;
                        }
                        self.expect_punct(']')?;
                        break;
                    }
                }
                self.set_terminator(func, block, Terminator::Switch { value, default, cases });
                self.skip_to_eol();
                return Ok(());
            }
            "ret" => {
                use smallvec::SmallVec;
                let mut values: SmallVec<[ValueId; 2]> = SmallVec::new();
                self.skip_inline_whitespace();
                // Empty ret?
                if self.peek_char() != Some('\n') && !self.is_eof() {
                    loop {
                        values.push(self.parse_value(func, arg_values, value_labels)?);
                        if !self.try_punct(',') {
                            break;
                        }
                    }
                }
                self.set_terminator(func, block, Terminator::Return { values });
                self.skip_to_eol();
                return Ok(());
            }
            "revert" => {
                let offset = self.parse_value(func, arg_values, value_labels)?;
                self.expect_punct(',')?;
                let size = self.parse_value(func, arg_values, value_labels)?;
                self.set_terminator(func, block, Terminator::Revert { offset, size });
                self.skip_to_eol();
                return Ok(());
            }
            "returndata" => {
                let offset = self.parse_value(func, arg_values, value_labels)?;
                self.expect_punct(',')?;
                let size = self.parse_value(func, arg_values, value_labels)?;
                self.set_terminator(func, block, Terminator::ReturnData { offset, size });
                self.skip_to_eol();
                return Ok(());
            }
            "stop" => {
                self.set_terminator(func, block, Terminator::Stop);
                self.skip_to_eol();
                return Ok(());
            }
            "selfdestruct" => {
                let recipient = self.parse_value(func, arg_values, value_labels)?;
                self.set_terminator(func, block, Terminator::SelfDestruct { recipient });
                self.skip_to_eol();
                return Ok(());
            }
            "invalid" => {
                self.set_terminator(func, block, Terminator::Invalid);
                self.skip_to_eol();
                return Ok(());
            }
            "tail_call" => {
                let function = self.parse_function_id()?;
                let mut args = smallvec::SmallVec::new();
                while self.try_punct(',') {
                    args.push(self.parse_value(func, arg_values, value_labels)?);
                }
                self.set_terminator(func, block, Terminator::TailCall { function, args });
                self.skip_to_eol();
                return Ok(());
            }
            _ => {}
        }

        // Otherwise — instruction.
        let (kind, result_ty) = self
            .parse_inst_kind(&mnemonic, func, arg_values, block_labels, value_labels)
            .map_err(|e| {
                // For "unknown instruction" errors, repoint the caret at the
                // start of the mnemonic instead of after it.
                if e.msg.starts_with("unknown instruction") {
                    self.error_at(mnemonic_line, mnemonic_col, mnemonic_pos, e.msg)
                } else {
                    e
                }
            })?;

        let metadata = self.parse_metadata(func, arg_values, value_labels)?;
        let mut inst = Instruction::new(kind, result_ty);
        inst.metadata = metadata;
        let inst_id = func.alloc_inst(inst);
        func.blocks[block].instructions.push(inst_id);
        if let Some(label) = result_label {
            self.resolve_result_label(func, value_labels, label, inst_id)?;
        }
        self.skip_to_eol();
        Ok(())
    }

    fn parse_metadata(
        &mut self,
        func: &mut Function,
        arg_values: &[ValueId],
        value_labels: &mut FxHashMap<u32, ValueId>,
    ) -> Result<InstructionMetadata, ParseError> {
        let mut metadata = InstructionMetadata::EMPTY;
        self.skip_inline();
        if !self.try_punct('!') {
            return Ok(metadata);
        }
        self.expect_keyword("metadata")?;
        self.expect_punct('(')?;
        if self.try_punct(')') {
            return Ok(metadata);
        }

        loop {
            let key = self.parse_ident()?.to_string();
            match key.as_str() {
                "unchecked" => {
                    metadata.set_unchecked(true);
                }
                "storage" => {
                    self.expect_punct('=')?;
                    metadata.set_storage_alias(Some(self.parse_storage_alias(
                        func,
                        arg_values,
                        value_labels,
                    )?));
                }
                "memory" => {
                    self.expect_punct('=')?;
                    let value = self.parse_ident()?;
                    metadata.set_memory_region(Some(self.parse_memory_region(value)?));
                }
                "effect" => {
                    self.expect_punct('=')?;
                    let value = self.parse_ident()?;
                    metadata.set_effect(Some(self.parse_effect_kind(value)?));
                }
                "loop_depth" => {
                    self.expect_punct('=')?;
                    let value = self.parse_uint_literal()?;
                    metadata.loop_depth = self.u256_to_u16(value)?;
                }
                "hir" => {
                    self.expect_punct('=')?;
                    let value = self.parse_uint_literal()?;
                    metadata.set_hir_expr(Some(hir::ExprId::from_usize(
                        self.u256_to_u32(value)? as usize
                    )));
                }
                "span" => {
                    self.expect_punct('=')?;
                    let lo = self.parse_uint_literal()?;
                    let lo = self.u256_to_u32(lo)?;
                    self.expect_punct('.')?;
                    self.expect_punct('.')?;
                    let hi = self.parse_uint_literal()?;
                    let hi = self.u256_to_u32(hi)?;
                    metadata.set_source_span(Some(Span::new(BytePos(lo), BytePos(hi))));
                }
                _ => return Err(self.error(format!("unknown metadata key `{key}`"))),
            }

            if self.try_punct(',') {
                continue;
            }
            self.expect_punct(')')?;
            break;
        }

        Ok(metadata)
    }

    fn parse_storage_alias(
        &mut self,
        func: &mut Function,
        arg_values: &[ValueId],
        value_labels: &mut FxHashMap<u32, ValueId>,
    ) -> Result<StorageAlias, ParseError> {
        let kind = self.parse_ident()?.to_string();
        self.expect_punct('(')?;
        let alias = match kind.as_str() {
            "slot" => StorageAlias::Slot(self.parse_uint_literal()?),
            "symbolic" => {
                StorageAlias::Symbolic(self.parse_value(func, arg_values, value_labels)?)
            }
            "offset" => {
                let base = self.parse_value(func, arg_values, value_labels)?;
                self.expect_punct(',')?;
                let offset = self.parse_uint_literal()?;
                StorageAlias::Offset { base, offset }
            }
            _ => return Err(self.error(format!("unknown storage metadata value `{kind}`"))),
        };
        self.expect_punct(')')?;
        Ok(alias)
    }

    fn parse_memory_region(&self, value: &str) -> Result<MemoryRegion, ParseError> {
        Ok(match value {
            "scratch" => MemoryRegion::Scratch,
            "abi_return" => MemoryRegion::AbiReturn,
            "heap" => MemoryRegion::Heap,
            "internal_frame" => MemoryRegion::InternalFrame,
            "unknown" => MemoryRegion::Unknown,
            _ => return Err(self.error(format!("unknown memory metadata value `{value}`"))),
        })
    }

    fn parse_effect_kind(&self, value: &str) -> Result<EffectKind, ParseError> {
        Ok(match value {
            "pure" => EffectKind::Pure,
            "memory_read" => EffectKind::MemoryRead,
            "memory_write" => EffectKind::MemoryWrite,
            "storage_read" => EffectKind::StorageRead,
            "storage_write" => EffectKind::StorageWrite,
            "transient_read" => EffectKind::TransientRead,
            "transient_write" => EffectKind::TransientWrite,
            "environment_read" => EffectKind::EnvironmentRead,
            "external_call" => EffectKind::ExternalCall,
            "internal_call" => EffectKind::InternalCall,
            "create" => EffectKind::Create,
            "log" => EffectKind::Log,
            _ => return Err(self.error(format!("unknown effect metadata value `{value}`"))),
        })
    }

    fn u256_to_u32(&self, value: U256) -> Result<u32, ParseError> {
        value.try_into().map_err(|_| self.error(format!("integer `{value}` does not fit in u32")))
    }

    fn u256_to_u16(&self, value: U256) -> Result<u16, ParseError> {
        value.try_into().map_err(|_| self.error(format!("integer `{value}` does not fit in u16")))
    }

    fn set_terminator(&self, func: &mut Function, block: BlockId, term: Terminator) {
        // Update predecessors so downstream passes see a valid CFG.
        let succs = term.successors();
        for s in succs {
            func.blocks[s].predecessors.push(block);
        }
        func.blocks[block].terminator = Some(term);
    }

    /// Parses one instruction by mnemonic. Returns the constructed [`InstKind`]
    /// and its result type (or `None` if it produces no value).
    #[allow(clippy::too_many_lines)]
    fn parse_inst_kind(
        &mut self,
        mnemonic: &str,
        func: &mut Function,
        arg_values: &[ValueId],
        block_labels: &FxHashMap<u32, BlockId>,
        value_labels: &mut FxHashMap<u32, ValueId>,
    ) -> Result<(InstKind, Option<MirType>), ParseError> {
        // Operand parsing helpers.
        macro_rules! v {
            () => {
                self.parse_value(func, arg_values, value_labels)?
            };
        }
        macro_rules! comma {
            () => {
                self.expect_punct(',')?
            };
        }

        Ok(match mnemonic {
            // ----- arithmetic (all uint256 result) -----
            "add" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Add(a, b), Some(MirType::uint256()))
            }
            "sub" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Sub(a, b), Some(MirType::uint256()))
            }
            "mul" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Mul(a, b), Some(MirType::uint256()))
            }
            "div" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Div(a, b), Some(MirType::uint256()))
            }
            "sdiv" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SDiv(a, b), Some(MirType::int256()))
            }
            "mod" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Mod(a, b), Some(MirType::uint256()))
            }
            "smod" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SMod(a, b), Some(MirType::int256()))
            }
            "exp" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Exp(a, b), Some(MirType::uint256()))
            }
            "addmod" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::AddMod(a, b, c), Some(MirType::uint256()))
            }
            "mulmod" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::MulMod(a, b, c), Some(MirType::uint256()))
            }

            // ----- bitwise -----
            "and" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::And(a, b), Some(MirType::uint256()))
            }
            "or" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Or(a, b), Some(MirType::uint256()))
            }
            "xor" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Xor(a, b), Some(MirType::uint256()))
            }
            "not" => {
                let a = v!();
                (InstKind::Not(a), Some(MirType::uint256()))
            }
            "shl" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Shl(a, b), Some(MirType::uint256()))
            }
            "shr" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Shr(a, b), Some(MirType::uint256()))
            }
            "sar" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Sar(a, b), Some(MirType::int256()))
            }
            "byte" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Byte(a, b), Some(MirType::uint256()))
            }
            "signextend" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SignExtend(a, b), Some(MirType::int256()))
            }

            // ----- comparison -----
            "lt" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Lt(a, b), Some(MirType::Bool))
            }
            "gt" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Gt(a, b), Some(MirType::Bool))
            }
            "slt" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SLt(a, b), Some(MirType::Bool))
            }
            "sgt" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SGt(a, b), Some(MirType::Bool))
            }
            "eq" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Eq(a, b), Some(MirType::Bool))
            }
            "iszero" => {
                let a = v!();
                (InstKind::IsZero(a), Some(MirType::Bool))
            }

            // ----- memory -----
            "mload" => {
                let a = v!();
                (InstKind::MLoad(a), Some(MirType::uint256()))
            }
            "mstore" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::MStore(a, b), None)
            }
            "mstore8" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::MStore8(a, b), None)
            }
            "msize" => (InstKind::MSize, Some(MirType::uint256())),
            "mcopy" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::MCopy(a, b, c), None)
            }

            // ----- storage -----
            "sload" => {
                let a = v!();
                (InstKind::SLoad(a), Some(MirType::uint256()))
            }
            "sstore" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SStore(a, b), None)
            }
            "tload" => {
                let a = v!();
                (InstKind::TLoad(a), Some(MirType::uint256()))
            }
            "tstore" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::TStore(a, b), None)
            }

            // ----- calldata -----
            "calldataload" => {
                let a = v!();
                (InstKind::CalldataLoad(a), Some(MirType::uint256()))
            }
            "calldatasize" => (InstKind::CalldataSize, Some(MirType::uint256())),
            "calldatacopy" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::CalldataCopy(a, b, c), None)
            }

            // ----- code -----
            "codesize" => (InstKind::CodeSize, Some(MirType::uint256())),
            "codecopy" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::CodeCopy(a, b, c), None)
            }
            "loadimmutable" => {
                let offset = self.parse_uint_literal()?;
                let offset = self.u256_to_u32(offset)?;
                (InstKind::LoadImmutable(offset), Some(MirType::uint256()))
            }
            "extcodesize" => {
                let a = v!();
                (InstKind::ExtCodeSize(a), Some(MirType::uint256()))
            }
            "extcodecopy" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                (InstKind::ExtCodeCopy(a, b, c, d), None)
            }
            "extcodehash" => {
                let a = v!();
                (InstKind::ExtCodeHash(a), Some(MirType::uint256()))
            }

            // ----- return data -----
            "returndatasize" => (InstKind::ReturnDataSize, Some(MirType::uint256())),
            "returndatacopy" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::ReturnDataCopy(a, b, c), None)
            }

            // ----- environment (nullary) -----
            "caller" => (InstKind::Caller, Some(MirType::Address)),
            "callvalue" => (InstKind::CallValue, Some(MirType::uint256())),
            "origin" => (InstKind::Origin, Some(MirType::Address)),
            "gasprice" => (InstKind::GasPrice, Some(MirType::uint256())),
            "coinbase" => (InstKind::Coinbase, Some(MirType::Address)),
            "timestamp" => (InstKind::Timestamp, Some(MirType::uint256())),
            "number" => (InstKind::BlockNumber, Some(MirType::uint256())),
            "prevrandao" => (InstKind::PrevRandao, Some(MirType::uint256())),
            "gaslimit" => (InstKind::GasLimit, Some(MirType::uint256())),
            "chainid" => (InstKind::ChainId, Some(MirType::uint256())),
            "address" => (InstKind::Address, Some(MirType::Address)),
            "selfbalance" => (InstKind::SelfBalance, Some(MirType::uint256())),
            "gas" => (InstKind::Gas, Some(MirType::uint256())),
            "basefee" => (InstKind::BaseFee, Some(MirType::uint256())),
            "blobbasefee" => (InstKind::BlobBaseFee, Some(MirType::uint256())),

            // ----- environment (unary) -----
            "blockhash" => {
                let a = v!();
                (InstKind::BlockHash(a), Some(MirType::FixedBytes(32)))
            }
            "balance" => {
                let a = v!();
                (InstKind::Balance(a), Some(MirType::uint256()))
            }
            "blobhash" => {
                let a = v!();
                (InstKind::BlobHash(a), Some(MirType::FixedBytes(32)))
            }

            // ----- hashing -----
            "keccak256" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Keccak256(a, b), Some(MirType::bytes32()))
            }

            // ----- calls -----
            "call" => {
                let gas = v!();
                comma!();
                let addr = v!();
                comma!();
                let value = v!();
                comma!();
                let args_offset = v!();
                comma!();
                let args_size = v!();
                comma!();
                let ret_offset = v!();
                comma!();
                let ret_size = v!();
                (
                    InstKind::Call {
                        gas,
                        addr,
                        value,
                        args_offset,
                        args_size,
                        ret_offset,
                        ret_size,
                    },
                    Some(MirType::uint256()),
                )
            }
            "staticcall" => {
                let gas = v!();
                comma!();
                let addr = v!();
                comma!();
                let args_offset = v!();
                comma!();
                let args_size = v!();
                comma!();
                let ret_offset = v!();
                comma!();
                let ret_size = v!();
                (
                    InstKind::StaticCall {
                        gas,
                        addr,
                        args_offset,
                        args_size,
                        ret_offset,
                        ret_size,
                    },
                    Some(MirType::uint256()),
                )
            }
            "delegatecall" => {
                let gas = v!();
                comma!();
                let addr = v!();
                comma!();
                let args_offset = v!();
                comma!();
                let args_size = v!();
                comma!();
                let ret_offset = v!();
                comma!();
                let ret_size = v!();
                (
                    InstKind::DelegateCall {
                        gas,
                        addr,
                        args_offset,
                        args_size,
                        ret_offset,
                        ret_size,
                    },
                    Some(MirType::uint256()),
                )
            }
            "internal_call" => {
                let function = self.parse_function_id()?;
                comma!();
                let returns = self.parse_uint_literal()?.to::<u32>();
                let mut args = Vec::new();
                while self.try_punct(',') {
                    args.push(v!());
                }
                let result_ty = (returns > 0).then(MirType::uint256);
                (InstKind::InternalCall { function, args: args.into(), returns }, result_ty)
            }
            "internal_frame_addr" => {
                let offset = self.parse_uint_literal()?.to::<u64>();
                (InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
            }

            // ----- create -----
            "create" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::Create(a, b, c), Some(MirType::Address))
            }
            "create2" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                (InstKind::Create2(a, b, c, d), Some(MirType::Address))
            }

            // ----- logs -----
            "log0" => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Log0(a, b), None)
            }
            "log1" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::Log1(a, b, c), None)
            }
            "log2" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                (InstKind::Log2(a, b, c, d), None)
            }
            "log3" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                comma!();
                let e = v!();
                (InstKind::Log3(a, b, c, d, e), None)
            }
            "log4" => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                comma!();
                let e = v!();
                comma!();
                let f = v!();
                (InstKind::Log4(a, b, c, d, e, f), None)
            }

            // ----- ssa -----
            "select" => {
                let cond = v!();
                comma!();
                let then_v = v!();
                comma!();
                let else_v = v!();
                (InstKind::Select(cond, then_v, else_v), Some(MirType::uint256()))
            }
            "phi" => {
                // Format: `phi [bb0: v1], [bb1: v2]` — matches the printer in display.rs.
                // Each pair is `[blockId: valueId]` separated by commas.
                let mut incoming: Vec<(BlockId, ValueId)> = Vec::new();
                loop {
                    self.expect_punct('[')?;
                    let bid = self.parse_block_id(block_labels)?;
                    self.expect_punct(':')?;
                    let val = v!();
                    self.expect_punct(']')?;
                    incoming.push((bid, val));
                    if !self.try_punct(',') {
                        break;
                    }
                }
                (InstKind::Phi(incoming), Some(MirType::uint256()))
            }

            other => return Err(self.error(format!("unknown instruction `{other}`"))),
        })
    }
}

// =============================================================================
// Suppress the unused `BasicBlock` import warning when this module is built
// without tests. The struct is used transitively through `Function`, but the
// import keeps the file self-documenting.
// =============================================================================

#[allow(dead_code)]
const _BLOCK_TYPE_REFERENCE: Option<BasicBlock> = None;

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use solar_interface::{ColorChoice, Session};

    fn with_session<F: FnOnce() + Send>(f: F) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(f);
    }

    #[test]
    fn parse_module_phase_header() {
        with_session(|| {
            let src =
                "; module @Phased [phase = optimized]\nfn @f() {\n  bb0 (entry):\n    stop\n}\n";
            let module = parse_module(src).unwrap();
            assert_eq!(module.phase, crate::mir::MirPhase::Optimized);
            // Round-trips through the printer.
            let printed = module.to_text().to_string();
            assert!(printed.starts_with("; module @Phased [phase = optimized]"), "{printed}");
            let reparsed = parse_module(&printed).unwrap();
            assert_eq!(reparsed.phase, crate::mir::MirPhase::Optimized);

            // The default phase is not printed, and parses back as built.
            let src = "; module @Fresh\nfn @f() {\n  bb0 (entry):\n    stop\n}\n";
            let module = parse_module(src).unwrap();
            assert_eq!(module.phase, crate::mir::MirPhase::Built);
            assert!(module.to_text().to_string().starts_with("; module @Fresh\n"));

            // Unknown phase names are rejected.
            let src = "; module @Bogus [phase = shiny]\nfn @f() {\n  bb0 (entry):\n    stop\n}\n";
            let err = parse_module(src).unwrap_err();
            assert!(err.to_string().contains("unknown MIR phase `shiny`"), "{err}");

            // Every phase name round-trips through parse and print.
            for phase in [
                crate::mir::MirPhase::Built,
                crate::mir::MirPhase::Optimized,
                crate::mir::MirPhase::Abi,
                crate::mir::MirPhase::Dispatch,
                crate::mir::MirPhase::EvmShaped,
            ] {
                let src = format!(
                    "; module @P [phase = {}]\nfn @f() {{\n  bb0 (entry):\n    stop\n}}\n",
                    phase.name()
                );
                let module = parse_module(&src).unwrap();
                assert_eq!(module.phase, phase, "parse `{}`", phase.name());
                let reparsed = parse_module(&module.to_text().to_string()).unwrap();
                assert_eq!(reparsed.phase, phase, "round-trip `{}`", phase.name());
            }
        });
    }

    #[test]
    fn parse_linear_function() {
        with_session(|| {
            let src = "\
fn @add(arg0: u256, arg1: u256) -> u256 {
  bb0 (entry):
    v2 = add arg0, arg1
    ret v2
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.blocks.len(), 1);
            assert_eq!(func.params.len(), 2);
            assert_eq!(func.returns.len(), 1);
            // Round-trip: print and re-parse should not error.
            let printed = func.to_text().to_string();
            let _func2 = parse_function(&printed).unwrap();
        });
    }

    #[test]
    fn parse_storage_ops() {
        with_session(|| {
            let src = "\
fn @increment() {
  bb0 (entry):
    v0 = sload 0
    v1 = add v0, 1
    sstore 0, v1
    stop
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.blocks.len(), 1);
            assert_eq!(func.params.len(), 0);
            // sstore + stop are the only "no result" things; sload, add produce results.
            // So we expect 4 instructions total.
            assert_eq!(func.instructions.len(), 3);
            // 0 + 1 are immediates; v0, v1 are inst results.
            assert!(func.values.len() >= 4);
        });
    }

    #[test]
    fn parse_branch() {
        with_session(|| {
            let src = "\
fn @max(arg0: u256, arg1: u256) -> u256 {
  bb0 (entry):
    v2 = gt arg0, arg1
    br v2, bb1, bb2
  bb1:
    ret arg0
  bb2:
    ret arg1
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.blocks.len(), 3);
            // bb0 should have 2 successors.
            assert_eq!(func.blocks[func.entry_block].terminator().unwrap().successors().len(), 2);
        });
    }

    #[test]
    fn parse_loop_with_jump() {
        with_session(|| {
            let src = "\
fn @count_down(arg0: u256) -> u256 {
  bb0 (entry):
    jump bb1
  bb1:
    v1 = lt 0, arg0
    br v1, bb2, bb3
  bb2:
    jump bb1
  bb3:
    ret arg0
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.blocks.len(), 4);
        });
    }

    #[test]
    fn parse_call_instruction() {
        with_session(|| {
            let src = "\
fn @do_call(arg0: address, arg1: u256) -> u256 {
  bb0 (entry):
    v2 = call 100, arg0, arg1, 0, 0, 0, 0
    ret v2
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.instructions.len(), 1);
        });
    }

    #[test]
    fn parse_keccak_and_mload_mstore() {
        with_session(|| {
            let src = "\
fn @hash() -> u256 {
  bb0 (entry):
    mstore 0, 1
    mstore 32, 2
    v1 = keccak256 0, 64
    ret v1
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.instructions.len(), 3);
        });
    }

    #[test]
    fn parse_round_trip_module() {
        with_session(|| {
            let src = "\
; module @Counter
fn @count() -> u256 {
  bb0 (entry):
    v1 = sload 0
    ret v1
}

fn @set(arg0: u256) {
  bb0 (entry):
    sstore 0, arg0
    stop
}
";
            let module = parse_module(src).unwrap();
            assert_eq!(module.functions.len(), 2);
            // Round-trip the printed form.
            let printed = module.to_text().to_string();
            let module2 = parse_module(&printed).unwrap();
            assert_eq!(module2.functions.len(), 2);
        });
    }

    #[test]
    fn parse_unknown_instruction_errors() {
        with_session(|| {
            let src = "\
fn @bad() {
  bb0 (entry):
    v1 = bogus arg0
    stop
}
";
            let err = parse_function(src).unwrap_err();
            assert!(err.msg.contains("bogus") || err.msg.contains("unknown"));
        });
    }

    #[test]
    fn error_includes_source_snippet() {
        with_session(|| {
            let src = "\
fn @bad() {
  bb0 (entry):
    v1 = bogus arg0
    stop
}
";
            let err = parse_function(src).unwrap_err();
            // line_text should contain the offending line.
            assert!(err.line_text.contains("bogus arg0"), "got line_text: {:?}", err.line_text);
            // Display should include both the line and a caret marker.
            let formatted = err.to_string();
            assert!(formatted.contains("bogus"), "missing line in:\n{formatted}");
            assert!(formatted.contains("|"), "missing snippet bar in:\n{formatted}");
            assert!(formatted.contains("^"), "missing caret in:\n{formatted}");
            // The line number should appear in the snippet header.
            assert!(formatted.contains(&format!("{} | ", err.line)), "missing line number");
        });
    }

    #[test]
    fn error_snippet_format_is_clang_like() {
        // Verify the precise format users will see, end-to-end.
        with_session(|| {
            let src = "fn @x() -> notatype {\n  bb0 (entry):\n    stop\n}\n";
            let err = parse_function(src).unwrap_err();
            let formatted = err.to_string();
            // Roughly:
            //   MIR parse error at line 1, col N: ...
            //      |
            //    1 | fn @x() -> notatype {
            //      |            ^
            assert!(formatted.starts_with("MIR parse error at line "));
            assert!(formatted.contains("\n   |\n"));
            assert!(formatted.contains("notatype"));
        });
    }

    #[test]
    fn parse_revert_terminator() {
        with_session(|| {
            let src = "\
fn @oops() {
  bb0 (entry):
    revert 0, 0
}
";
            let func = parse_function(src).unwrap();
            assert!(matches!(
                func.blocks[func.entry_block].terminator,
                Some(Terminator::Revert { .. })
            ));
        });
    }

    #[test]
    fn parse_environment_nullary() {
        with_session(|| {
            let src = "\
fn @env() -> u256 {
  bb0 (entry):
    v0 = caller
    v1 = callvalue
    v2 = gas
    v3 = chainid
    ret v3
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.instructions.len(), 4);
        });
    }

    #[test]
    fn parse_select_and_logs() {
        with_session(|| {
            let src = "\
fn @sel(arg0: bool, arg1: u256, arg2: u256) -> u256 {
  bb0 (entry):
    v3 = select arg0, arg1, arg2
    log1 0, 32, v3
    ret v3
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.instructions.len(), 2);
        });
    }

    #[test]
    fn parse_phi_node() {
        with_session(|| {
            let src = "\
fn @diamond(arg0: bool) -> u256 {
  bb0 (entry):
    br arg0, bb1, bb2
  bb1:
    jump bb3
  bb2:
    jump bb3
  bb3:
    v1 = phi [bb1: 10], [bb2: 20]
    ret v1
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.blocks.len(), 4);
            // Find the phi instruction.
            let phi_inst =
                func.instructions.iter().find(|i| matches!(i.kind, InstKind::Phi(_))).unwrap();
            if let InstKind::Phi(args) = &phi_inst.kind {
                assert_eq!(args.len(), 2);
            } else {
                panic!("expected phi");
            }
        });
    }

    #[test]
    fn parse_switch_terminator() {
        with_session(|| {
            let src = "\
fn @dispatch(arg0: u256) -> u256 {
  bb0 (entry):
    switch arg0, default bb4, [1 => bb1, 2 => bb2, 3 => bb3]
  bb1:
    ret arg0
  bb2:
    ret 0
  bb3:
    ret 1
  bb4:
    ret 2
}
";
            let func = parse_function(src).unwrap();
            assert_eq!(func.blocks.len(), 5);
            let term = func.blocks[func.entry_block].terminator.as_ref().unwrap();
            if let Terminator::Switch { cases, .. } = term {
                assert_eq!(cases.len(), 3);
            } else {
                panic!("expected switch terminator");
            }
        });
    }

    #[test]
    fn parse_phi_round_trip_with_printer() {
        // Build a function with InstKind::Phi via the parser, print it, and
        // verify the printer output uses the `[bbN: vN]` format we expect.
        with_session(|| {
            let src = "\
fn @diamond(arg0: bool) -> u256 {
  bb0 (entry):
    br arg0, bb1, bb2
  bb1:
    jump bb3
  bb2:
    jump bb3
  bb3:
    v1 = phi [bb1: 10], [bb2: 20]
    ret v1
}
";
            let func = parse_function(src).unwrap();
            let printed = func.to_text().to_string();
            // The printer's exact format: `[bbN: <val>]`.
            assert!(
                printed.contains("phi [bb1:") && printed.contains("], [bb2:"),
                "expected `phi [bb1: ..], [bb2: ..]`, got:\n{printed}"
            );
            // Round-trip: re-parse the printer output, must succeed.
            let _func2 = parse_function(&printed).unwrap();
        });
    }
}

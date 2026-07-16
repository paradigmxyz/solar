//! EVM IR text parser.

use super::*;
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use std::fmt as std_fmt;

pub(super) fn parse(input: &str) -> Result<Module, ParseError> {
    Parser::new(input).parse_module()
}

/// An error produced while parsing the EVM IR text format.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseError {
    /// 1-based line number.
    pub line: usize,
    /// 1-based column number.
    pub col: usize,
    /// Human-readable message.
    pub msg: String,
    /// Source line captured for snippet rendering.
    pub line_text: String,
}

impl std_fmt::Display for ParseError {
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

impl std::error::Error for ParseError {}

#[derive(Clone, Debug)]
struct ParsedBlockHeader {
    label: String,
    entry: bool,
    hotness: BlockHotness,
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

    fn error(&self, msg: impl Into<String>) -> ParseError {
        ParseError {
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

    fn try_punct(&mut self, expected: char) -> bool {
        self.skip_inline();
        if self.peek_char() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn parse_ident(&mut self) -> Result<&'a str, ParseError> {
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

    fn parse_module(&mut self) -> Result<Module, ParseError> {
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

        let mut module = Module::new(name);
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
        module: &mut Module,
        body_end: BodyEnd,
    ) -> Result<(), ParseError> {
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
                let block_id = module.add_block(Block::new(header.label.clone()));
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
        let mut defined_values = GrowableBitSet::with_capacity(module.values.len());
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

    fn try_parse_block_header(&mut self) -> Result<Option<ParsedBlockHeader>, ParseError> {
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

        let mut hotness = BlockHotness::Hot;
        self.skip_inline_whitespace();
        if self.try_punct('[') {
            let key = self.parse_ident()?;
            if key == "cold" {
                hotness = BlockHotness::Cold;
            } else if key == "hot" {
                hotness = BlockHotness::Hot;
            } else if key == "hotness" {
                self.expect_punct('=')?;
                let value = self.parse_ident()?;
                hotness = BlockHotness::parse(value)
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

    fn try_parse_block_label_text(&mut self) -> Result<Option<String>, ParseError> {
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
        module: &mut Module,
        block: BlockId,
        block_labels: &FxHashMap<String, BlockId>,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> Result<(), ParseError> {
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
            module.blocks[block].terminator = Some(Terminator { kind, metadata });
            self.skip_to_eol();
            return Ok(());
        }

        let operands =
            self.parse_operand_list(module, block_labels, value_labels, defined_values)?;
        let metadata = self.parse_metadata()?;
        let kind = StackOp::parse(&mnemonic)
            .map(InstructionKind::Stack)
            .unwrap_or(InstructionKind::Operation(mnemonic));
        module.blocks[block].instructions.push(Instruction { result, kind, operands, metadata });
        self.skip_to_eol();
        Ok(())
    }

    fn try_parse_result(
        &mut self,
        module: &mut Module,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> Result<Option<ValueId>, ParseError> {
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

    fn parse_value_name(&mut self) -> Result<String, ParseError> {
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
        module: &mut Module,
        block_labels: &FxHashMap<String, BlockId>,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> Result<Option<TerminatorKind>, ParseError> {
        let kind = match mnemonic {
            "fallthrough" => TerminatorKind::Fallthrough(self.parse_block_ref(block_labels)?),
            "fallthrough_next" => TerminatorKind::FallthroughNext,
            "jump" => TerminatorKind::Jump(self.parse_block_ref(block_labels)?),
            "br" => {
                let condition =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let then_block = self.parse_block_ref(block_labels)?;
                self.expect_punct(',')?;
                let else_block = self.parse_block_ref(block_labels)?;
                TerminatorKind::Branch { condition, then_block, else_block }
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
                TerminatorKind::Switch { value, default, cases }
            }
            "return" => {
                let offset =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let size =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                TerminatorKind::Return { offset, size }
            }
            "revert" => {
                let offset =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.expect_punct(',')?;
                let size =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                TerminatorKind::Revert { offset, size }
            }
            "stop" => TerminatorKind::Stop,
            "invalid" => TerminatorKind::Invalid,
            "selfdestruct" => {
                let recipient =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                TerminatorKind::SelfDestruct { recipient }
            }
            "terminal" => {
                self.skip_inline();
                let opcode = if self.peek_char().is_some_and(|c| c.is_ascii_digit()) {
                    let opcode = self.parse_uint_literal()?;
                    let Ok(opcode) = u8::try_from(opcode) else {
                        return Err(self.error("raw terminal opcode must fit in one byte"));
                    };
                    opcode
                } else {
                    let mnemonic = self.parse_ident()?;
                    let Some(opcode) = super::super::assembler::op::from_mnemonic(mnemonic) else {
                        return Err(self.error(format!("unknown terminal opcode `{mnemonic}`")));
                    };
                    opcode
                };
                TerminatorKind::RawOpcode(opcode)
            }
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }

    fn parse_operand_list(
        &mut self,
        module: &mut Module,
        block_labels: &FxHashMap<String, BlockId>,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> Result<Vec<Operand>, ParseError> {
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
        module: &mut Module,
        block_labels: &FxHashMap<String, BlockId>,
        value_labels: &mut FxHashMap<String, ValueId>,
        _defined_values: &mut GrowableBitSet<ValueId>,
    ) -> Result<Operand, ParseError> {
        self.skip_inline();
        if self.peek_char() == Some('%') {
            let name = self.parse_value_name()?;
            return Ok(Operand::Value(value_id(module, value_labels, &name)));
        }
        if matches!(self.peek_char(), Some(c) if c.is_ascii_digit()) {
            return Ok(Operand::Immediate(self.parse_uint_literal()?));
        }
        if self.peek_char() == Some('@') {
            self.advance();
            let symbol = self.parse_ident()?;
            return Ok(Operand::Symbol(format!("@{symbol}")));
        }
        if self.input[self.pos..].starts_with("bb") {
            let save = (self.pos, self.line, self.col);
            if let Some(label) = self.try_parse_block_label_text()? {
                if let Some(block) = block_labels.get(&label).copied() {
                    return Ok(Operand::Block(block));
                }
                return Err(self.error(format!("unknown block `{label}`")));
            }
            self.restore(save);
        }
        Ok(Operand::Symbol(self.parse_ident()?.to_string()))
    }

    fn parse_block_ref(
        &mut self,
        block_labels: &FxHashMap<String, BlockId>,
    ) -> Result<BlockId, ParseError> {
        let label =
            self.try_parse_block_label_text()?.ok_or_else(|| self.error("expected block label"))?;
        block_labels
            .get(&label)
            .copied()
            .ok_or_else(|| self.error(format!("unknown block `{label}`")))
    }

    fn parse_metadata(&mut self) -> Result<Metadata, ParseError> {
        let mut metadata = Metadata::default();
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
                metadata.stack = Some(StackEffect::new(inputs, outputs));
            } else if self.try_punct('=') {
                let value = self.parse_metadata_value()?;
                metadata.attrs.push(MetadataItem { key, value: Some(value) });
            } else {
                metadata.attrs.push(MetadataItem { key, value: None });
            }

            if self.try_punct(',') {
                continue;
            }
            self.expect_punct(')')?;
            break;
        }
        Ok(metadata)
    }

    fn parse_metadata_value(&mut self) -> Result<String, ParseError> {
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

    fn parse_u16(&mut self) -> Result<u16, ParseError> {
        let value = self.parse_uint_literal()?;
        value.try_into().map_err(|_| self.error(format!("integer `{value}` does not fit in u16")))
    }

    fn at_end_of_operation(&self) -> bool {
        matches!(self.peek_char(), None | Some('\n' | '\r' | '!' | '}'))
    }
}

fn value_id(
    module: &mut Module,
    value_labels: &mut FxHashMap<String, ValueId>,
    name: &str,
) -> ValueId {
    if let Some(value) = value_labels.get(name).copied() {
        return value;
    }
    let value = module.add_value(name.to_string());
    value_labels.insert(name.to_string(), value);
    value
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};
    use std::path::{Path, PathBuf};

    fn parse_module(input: &str) -> Result<Module, ParseError> {
        Module::parse(input)
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
        assert_data_eq!(
            parse_module(input).unwrap_err().to_string(),
            str![[r#"
EVM IR parse error at line 6, col 5: instruction after terminator in block `bb0`
   |
  6 |     invalid
   |     ^
"#]]
        );
    }

    fn round_trip_fixture(path: &Path) -> Result<(), String> {
        #[allow(clippy::disallowed_methods)]
        let input = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        let print1 = parse_module(&input).map_err(|err| err.to_string())?.to_text().to_string();
        let print2 = parse_module(&print1).map_err(|err| err.to_string())?.to_text().to_string();
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

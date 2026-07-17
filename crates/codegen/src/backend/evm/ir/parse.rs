//! EVM IR text parser.

use super::*;
use crate::{backend::evm::assembler::op, ir_parse::Checkpoint};
use solar_ast::{
    Arena,
    token::{BinOpToken, Delimiter, TokenKind},
};
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use solar_interface::{Result, Session, Symbol, kw, source_map::SourceFile, sym};
use solar_parse::{PErr, PResult};

pub(super) fn parse(sess: &Session, source: &SourceFile) -> Result<Module> {
    let errors = sess.dcx.err_count();
    let arena = Arena::new();
    let mut parser = Parser::new(sess, &arena, source);
    if sess.dcx.err_count() > errors {
        sess.dcx.has_errors()?;
    }
    parser.parse_module().map_err(PErr::emit)
}

#[derive(Clone, Debug)]
struct ParsedBlockHeader {
    label: String,
    entry: bool,
    hotness: Hotness,
    /// Incoming stack-word names from an `(in %a, %b)` signature, top first.
    entry_stack: Vec<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum BodyEnd {
    Eof,
    Brace,
}

struct Parser<'sess, 'ast, 'src> {
    parser: crate::ir_parse::Parser<'sess, 'ast, 'src>,
}

impl<'sess, 'ast, 'src> Parser<'sess, 'ast, 'src> {
    fn new(sess: &'sess Session, arena: &'ast Arena, source: &'src SourceFile) -> Self {
        Self { parser: crate::ir_parse::Parser::new(sess, arena, source) }
    }

    fn is_eof(&self) -> bool {
        self.parser.is_eof()
    }

    fn skip_to_eol(&mut self) {
        self.parser.skip_to_eol();
    }

    fn error(&self, msg: impl Into<String>) -> PErr<'sess> {
        self.parser.error(msg)
    }

    fn expect_keyword(&mut self, kw: Symbol) -> PResult<'sess, ()> {
        self.parser.expect_keyword(kw)
    }

    fn parse_symbol(&mut self) -> PResult<'sess, Symbol> {
        self.parser.parse_ident()
    }

    fn parse_ident(&mut self) -> PResult<'sess, String> {
        let mut ident = self.parser.parse_ident()?.to_string();
        loop {
            let separator = if self.parser.eat(TokenKind::Dot) {
                '.'
            } else if self.parser.eat(TokenKind::BinOp(BinOpToken::Minus)) {
                '-'
            } else {
                break;
            };
            ident.push(separator);
            ident.push_str(self.parser.parse_ident()?.as_str());
        }
        Ok(ident)
    }

    fn parse_uint_literal(&mut self) -> PResult<'sess, U256> {
        self.parser.parse_uint()
    }

    fn parse_module(&mut self) -> PResult<'sess, Module> {
        let mut name = sym::module.to_string();
        while self.parser.eat(TokenKind::At) {
            let attr = self.parse_symbol()?;
            if attr == sym::module {
                name = self.parse_ident()?;
            } else {
                return Err(self.error(format!("unknown module attribute `@{attr}`")));
            }
            self.skip_to_eol();
        }

        let mut module = Module::new(name);
        let legacy_function_wrapper = self.parser.check_keyword(sym::fn_);
        if legacy_function_wrapper {
            self.expect_keyword(sym::fn_)?;
            self.parser.expect(TokenKind::At)?;
            let _legacy_function_name = self.parse_ident()?;
            self.parser.expect(TokenKind::OpenDelim(Delimiter::Brace))?;
            self.parse_program_body(&mut module, BodyEnd::Brace)?;
        } else {
            self.parse_program_body(&mut module, BodyEnd::Eof)?;
        }
        Ok(module)
    }

    fn parse_program_body(&mut self, module: &mut Module, body_end: BodyEnd) -> PResult<'sess, ()> {
        let body_start = self.parser.checkpoint();
        let mut block_labels = FxHashMap::default();

        loop {
            if self.is_eof() {
                if body_end == BodyEnd::Brace {
                    return Err(self.error("unterminated EVM IR block body"));
                }
                break;
            }
            if body_end == BodyEnd::Brace
                && self.parser.check(TokenKind::CloseDelim(Delimiter::Brace))
            {
                break;
            }
            if let Some(header) = self.try_parse_block_header()? {
                if block_labels.contains_key(&header.label) {
                    return Err(self.error(format!("duplicate block `{}`", header.label)));
                }
                let block_id = module.add_block(Block::new(header.label.clone()));
                block_labels.insert(header.label, block_id);
            } else {
                self.parser.skip_current_line();
            }
        }

        if block_labels.is_empty() {
            return Err(self.error("program must contain at least one block"));
        }

        self.parser.restore(body_start);

        let mut current_block = None;
        let mut value_labels = FxHashMap::default();
        let mut defined_values = GrowableBitSet::with_capacity(module.values.len());
        loop {
            if self.is_eof() {
                if body_end == BodyEnd::Brace {
                    return Err(self.error("unterminated EVM IR block body"));
                }
                break;
            }
            if body_end == BodyEnd::Brace
                && self.parser.eat(TokenKind::CloseDelim(Delimiter::Brace))
            {
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

    fn try_parse_block_header(&mut self) -> PResult<'sess, Option<ParsedBlockHeader>> {
        let save = self.parser.checkpoint();
        let Some(label) = self.try_parse_block_label_text()? else {
            self.restore(save);
            return Ok(None);
        };

        if self.parser.eat(TokenKind::OpenDelim(Delimiter::Parenthesis))
            && self.parser.eat_keyword(sym::entry)
        {
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            self.finish_block_header(save, label, true)
        } else {
            self.restore(save.clone());
            let Some(label) = self.try_parse_block_label_text()? else { return Ok(None) };
            self.finish_block_header(save, label, false)
        }
    }

    fn finish_block_header(
        &mut self,
        save: Checkpoint,
        label: String,
        entry: bool,
    ) -> PResult<'sess, Option<ParsedBlockHeader>> {
        let mut hotness = Hotness::Hot;
        if self.parser.eat(TokenKind::OpenDelim(Delimiter::Bracket)) {
            let key = self.parse_symbol()?;
            if key == sym::cold {
                hotness = Hotness::Cold;
            } else if key == sym::hot {
                hotness = Hotness::Hot;
            } else if key == sym::hotness {
                self.parser.expect(TokenKind::Eq)?;
                let value = self.parse_symbol()?;
                hotness = match value {
                    sym::cold => Hotness::Cold,
                    sym::hot => Hotness::Hot,
                    _ => return Err(self.error(format!("unknown block hotness `{value}`"))),
                };
            } else {
                return Err(self.error(format!("unknown block metadata `{key}`")));
            }
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
        }

        // Optional incoming stack signature: `(in %a, %b)`.
        let mut entry_stack = Vec::new();
        let save_in = self.parser.checkpoint();
        if self.parser.eat(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            if self.parser.eat_keyword(kw::In) {
                loop {
                    if self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                        break;
                    }
                    entry_stack.push(self.parse_value_name()?);
                    if self.parser.eat(TokenKind::Comma) {
                        continue;
                    }
                    self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
                    break;
                }
            } else {
                self.restore(save_in);
            }
        }

        if !self.parser.eat(TokenKind::Colon) {
            self.restore(save);
            return Ok(None);
        }

        Ok(Some(ParsedBlockHeader { label, entry, hotness, entry_stack }))
    }

    fn try_parse_block_label_text(&mut self) -> PResult<'sess, Option<String>> {
        let TokenKind::Ident(_) = self.parser.token().kind else { return Ok(None) };
        let label = self.parse_ident()?;
        let Some(number) = label.strip_prefix("bb") else { return Ok(None) };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(self.error("expected block number after `bb`"));
        }
        Ok(Some(label))
    }

    fn restore(&mut self, saved: Checkpoint) {
        self.parser.restore(saved);
    }

    fn parse_instruction_or_terminator(
        &mut self,
        module: &mut Module,
        block: BlockId,
        block_labels: &FxHashMap<String, BlockId>,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> PResult<'sess, ()> {
        if module.blocks[block].terminator.is_some() {
            return Err(self.error(format!(
                "instruction after terminator in block `{}`",
                module.blocks[block].label
            )));
        }

        let result = self.try_parse_result(module, value_labels, defined_values)?;
        let mnemonic = self.parse_symbol()?;
        if let Some(kind) = self.parse_terminator_kind(
            mnemonic,
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
        let kind = StackOp::parse(mnemonic)
            .map(InstructionKind::Stack)
            .unwrap_or_else(|| InstructionKind::Operation(mnemonic.to_string()));
        module.blocks[block].instructions.push(Instruction { result, kind, operands, metadata });
        self.skip_to_eol();
        Ok(())
    }

    fn try_parse_result(
        &mut self,
        module: &mut Module,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> PResult<'sess, Option<ValueId>> {
        let save = self.parser.checkpoint();
        if !self.parser.check(TokenKind::BinOp(BinOpToken::Percent)) {
            return Ok(None);
        }
        let name = self.parse_value_name()?;
        if !self.parser.eat(TokenKind::Eq) {
            self.restore(save);
            return Ok(None);
        }
        let value = value_id(module, value_labels, &name);
        if !defined_values.insert(value) {
            return Err(self.error(format!("duplicate value `%{name}`")));
        }
        Ok(Some(value))
    }

    fn parse_value_name(&mut self) -> PResult<'sess, String> {
        self.parser.expect(TokenKind::BinOp(BinOpToken::Percent))?;
        let name = match self.parser.token().kind {
            TokenKind::Ident(_) | TokenKind::Literal(..) => self.parser.token_text().to_string(),
            _ => return Err(self.error("expected value name")),
        };
        self.parser.bump();
        Ok(name)
    }

    fn parse_terminator_kind(
        &mut self,
        mnemonic: Symbol,
        module: &mut Module,
        block_labels: &FxHashMap<String, BlockId>,
        value_labels: &mut FxHashMap<String, ValueId>,
        defined_values: &mut GrowableBitSet<ValueId>,
    ) -> PResult<'sess, Option<TerminatorKind>> {
        let kind = match mnemonic {
            sym::jump => TerminatorKind::Jump(self.parse_block_ref(block_labels)?),
            sym::br => {
                let condition =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.parser.expect(TokenKind::Comma)?;
                let then_block = self.parse_block_ref(block_labels)?;
                self.parser.expect(TokenKind::Comma)?;
                let else_block = self.parse_block_ref(block_labels)?;
                TerminatorKind::Branch { condition, then_block, else_block }
            }
            kw::Switch => {
                let value =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.parser.expect(TokenKind::Comma)?;
                self.expect_keyword(kw::Default)?;
                let default = self.parse_block_ref(block_labels)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
                let mut cases = Vec::new();
                if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                    loop {
                        let case =
                            self.parse_operand(module, block_labels, value_labels, defined_values)?;
                        self.parser.expect(TokenKind::FatArrow)?;
                        let target = self.parse_block_ref(block_labels)?;
                        cases.push((case, target));
                        if self.parser.eat(TokenKind::Comma) {
                            continue;
                        }
                        self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
                        break;
                    }
                }
                TerminatorKind::Switch { value, default, cases }
            }
            kw::Return if self.at_end_of_operation() => TerminatorKind::RawOpcode(op::RETURN),
            kw::Return => {
                let offset =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.parser.expect(TokenKind::Comma)?;
                let size =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                TerminatorKind::Return { offset, size }
            }
            kw::Revert if self.at_end_of_operation() => TerminatorKind::RawOpcode(op::REVERT),
            kw::Revert => {
                let offset =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                self.parser.expect(TokenKind::Comma)?;
                let size =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                TerminatorKind::Revert { offset, size }
            }
            kw::Stop => TerminatorKind::Stop,
            kw::Invalid => TerminatorKind::Invalid,
            kw::Selfdestruct if self.at_end_of_operation() => {
                TerminatorKind::RawOpcode(op::SELFDESTRUCT)
            }
            kw::Selfdestruct => {
                let recipient =
                    self.parse_operand(module, block_labels, value_labels, defined_values)?;
                TerminatorKind::SelfDestruct { recipient }
            }
            sym::terminal => {
                let opcode = if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
                    let opcode = self.parse_uint_literal()?;
                    let Ok(opcode) = u8::try_from(opcode) else {
                        return Err(self.error("raw terminal opcode must fit in one byte"));
                    };
                    opcode
                } else {
                    let mnemonic = self.parse_symbol()?;
                    let Some(opcode) = op::from_mnemonic(mnemonic.as_str()) else {
                        return Err(self.error(format!("unknown terminal opcode `{mnemonic}`")));
                    };
                    opcode
                };
                TerminatorKind::RawOpcode(opcode)
            }
            sym::raw => {
                let opcode = self.parse_uint_literal()?;
                let Ok(opcode) = u8::try_from(opcode) else {
                    return Err(self.error("raw opcode must fit in one byte"));
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
    ) -> PResult<'sess, Vec<Operand>> {
        let mut operands = Vec::new();
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
            if !self.parser.eat(TokenKind::Comma) {
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
    ) -> PResult<'sess, Operand> {
        if self.parser.check(TokenKind::BinOp(BinOpToken::Percent)) {
            let name = self.parse_value_name()?;
            return Ok(Operand::Value(value_id(module, value_labels, &name)));
        }
        if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
            return Ok(Operand::Immediate(self.parse_uint_literal()?));
        }
        if self.parser.eat(TokenKind::At) {
            let symbol = self.parse_ident()?;
            return Ok(Operand::Symbol(format!("@{symbol}")));
        }
        if self.parser.token_text().starts_with("bb") {
            let save = self.parser.checkpoint();
            if let Some(label) = self.try_parse_block_label_text()? {
                if let Some(block) = block_labels.get(&label).copied() {
                    return Ok(Operand::Block(block));
                }
                return Err(self.error(format!("unknown block `{label}`")));
            }
            self.restore(save);
        }
        Ok(Operand::Symbol(self.parse_ident()?))
    }

    fn parse_block_ref(
        &mut self,
        block_labels: &FxHashMap<String, BlockId>,
    ) -> PResult<'sess, BlockId> {
        let label =
            self.try_parse_block_label_text()?.ok_or_else(|| self.error("expected block label"))?;
        block_labels
            .get(&label)
            .copied()
            .ok_or_else(|| self.error(format!("unknown block `{label}`")))
    }

    fn parse_metadata(&mut self) -> PResult<'sess, Metadata> {
        let mut metadata = Metadata::default();
        if !self.parser.eat(TokenKind::Not) {
            return Ok(metadata);
        }
        self.expect_keyword(sym::meta)?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        if self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            return Ok(metadata);
        }

        loop {
            let key = self.parse_symbol()?;
            if key == sym::stack {
                self.parser.expect(TokenKind::Eq)?;
                let inputs = self.parse_u16()?;
                self.parser.expect(TokenKind::Arrow)?;
                let outputs = self.parse_u16()?;
                metadata.stack = Some(StackEffect::new(inputs, outputs));
            } else if self.parser.eat(TokenKind::Eq) {
                let value = self.parse_metadata_value()?;
                metadata.attrs.push(MetadataItem { key: key.to_string(), value: Some(value) });
            } else {
                metadata.attrs.push(MetadataItem { key: key.to_string(), value: None });
            }

            if self.parser.eat(TokenKind::Comma) {
                continue;
            }
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            break;
        }
        Ok(metadata)
    }

    fn parse_metadata_value(&mut self) -> PResult<'sess, String> {
        let start = self.parser.checkpoint();
        while !self.is_eof()
            && !self.parser.check(TokenKind::Comma)
            && !self.parser.check(TokenKind::CloseDelim(Delimiter::Parenthesis))
            && !self.parser.at_newline()
        {
            self.parser.bump();
        }
        let span = self.parser.span_from(start);
        let value = self.parser.span_text(span).trim();
        if value.is_empty() {
            return Err(self.error("expected metadata value"));
        }
        Ok(value.to_string())
    }

    fn parse_u16(&mut self) -> PResult<'sess, u16> {
        let value = self.parse_uint_literal()?;
        value.try_into().map_err(|_| self.error(format!("integer `{value}` does not fit in u16")))
    }

    fn at_end_of_operation(&self) -> bool {
        self.is_eof()
            || self.parser.at_newline()
            || self.parser.check(TokenKind::Not)
            || self.parser.check(TokenKind::CloseDelim(Delimiter::Brace))
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
    use solar_interface::{ColorChoice, source_map::FileName};
    use std::path::{Path, PathBuf};

    fn parse_module(sess: &Session, input: &str) -> Result<Module> {
        let id =
            input.bytes().fold(0u64, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte.into()));
        let source = sess
            .source_map()
            .new_source_file(FileName::Custom(format!("evmir-test-{id}")), input)
            .unwrap();
        Module::parse(sess, &source)
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
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let input = "\
@module m

fn @f {
  bb0 (entry):
    stop
    invalid
}
";
        sess.enter(|| assert!(parse_module(&sess, input).is_err()));
        assert_data_eq!(
            sess.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
error: instruction after terminator in block `bb0`
  ╭▸ <evmir-test-8131681028095984083>:6:5
  │
6 │     invalid
  ╰╴    ━━━━━━━


"#]]
        );
    }

    fn round_trip_fixture(path: &Path) -> Result<(), String> {
        #[allow(clippy::disallowed_methods)]
        let input = std::fs::read_to_string(path).map_err(|err| err.to_string())?;
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let (print1, print2) = sess
            .enter(|| {
                let print1 = parse_module(&sess, &input)?.to_text().to_string();
                let print2 = parse_module(&sess, &print1)?.to_text().to_string();
                Ok::<_, solar_interface::diagnostics::ErrorGuaranteed>((print1, print2))
            })
            .map_err(|_| sess.emitted_diagnostics().unwrap().to_string())?;
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

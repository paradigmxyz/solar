//! EVM IR text parser.

use super::*;
use crate::backend::evm::assembler::op;
use solar_ast::{
    Arena,
    token::{BinOpToken, Delimiter, TokenKind},
};
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use solar_interface::{Result, Session, Span, Symbol, kw, source_map::SourceFile, sym};
use solar_parse::{PErr, PResult};

pub(super) fn parse(sess: &Session, source: &SourceFile) -> Result<Module> {
    let arena = Arena::new();
    let mut parser = Parser::new(sess, &arena, source);
    parser.parse_module().map_err(PErr::emit)
}

#[derive(Clone, Debug)]
struct ParsedBlockHeader {
    label: Symbol,
    entry: bool,
    hotness: Hotness,
    /// Incoming stack-word names from an `(in %a, %b)` signature, top first.
    entry_stack: Vec<Symbol>,
}

#[derive(Clone, Copy, Debug)]
struct BlockLabel {
    id: BlockId,
    defined: bool,
    reference_span: Option<Span>,
}

struct Parser<'sess, 'ast, 'src> {
    parser: crate::ir_parse::Parser<'sess, 'ast>,
    source: &'src SourceFile,
    block_labels: FxHashMap<Symbol, BlockLabel>,
    block_order: Vec<BlockId>,
    value_labels: FxHashMap<Symbol, ValueId>,
    defined_values: GrowableBitSet<ValueId>,
}

impl<'sess, 'ast, 'src> Parser<'sess, 'ast, 'src> {
    fn new(sess: &'sess Session, arena: &'ast Arena, source: &'src SourceFile) -> Self {
        Self {
            parser: crate::ir_parse::Parser::new(sess, arena, source),
            source,
            block_labels: FxHashMap::default(),
            block_order: Vec::new(),
            value_labels: FxHashMap::default(),
            defined_values: GrowableBitSet::new_empty(),
        }
    }

    fn parse_module(&mut self) -> PResult<'sess, Module> {
        self.parser.expect(TokenKind::At)?;
        self.parser.expect_keyword(sym::module)?;
        let name = self.parser.parse_ident()?;

        let mut module = Module::new(name);
        self.parse_program_body(&mut module)?;
        Ok(module)
    }

    fn parse_program_body(&mut self, module: &mut Module) -> PResult<'sess, ()> {
        let mut current_block = None;
        while !self.parser.is_eof() {
            if let Some(header) = self.try_parse_block_header()? {
                let block_id = self.define_block(module, header.label)?;
                if header.entry {
                    module.entry_block = Some(block_id);
                }
                module.blocks[block_id].metadata.hotness = header.hotness;
                let mut entry_stack = Vec::with_capacity(header.entry_stack.len());
                for name in &header.entry_stack {
                    entry_stack.push(self.value_id(module, *name));
                }
                module.blocks[block_id].entry_stack = entry_stack;
                current_block = Some(block_id);
                continue;
            }

            let block = current_block
                .ok_or_else(|| self.parser.error("instruction outside of any block"))?;
            self.parse_instruction_or_terminator(module, block)?;
        }

        if self.block_labels.is_empty() {
            return Err(self.parser.error("program must contain at least one block"));
        }
        self.reject_unresolved_blocks()?;
        super::passes::utils::remap_block_order(module, &self.block_order);

        Ok(())
    }

    fn try_parse_block_header(&mut self) -> PResult<'sess, Option<ParsedBlockHeader>> {
        let Some(label) = self.current_block_label()? else { return Ok(None) };
        self.parser.bump();
        let entry = self.parser.check(TokenKind::OpenDelim(Delimiter::Parenthesis))
            && self.parser.look_ahead(1).is_keyword(sym::entry);
        if entry {
            self.parser.bump();
            self.parser.bump();
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        }
        let mut hotness = Hotness::Hot;
        if self.parser.eat(TokenKind::OpenDelim(Delimiter::Bracket)) {
            self.parser.expect_keyword(sym::cold)?;
            hotness = Hotness::Cold;
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
        }

        // Optional incoming stack signature: `(in %a, %b)`.
        let mut entry_stack = Vec::new();
        if self.parser.check(TokenKind::OpenDelim(Delimiter::Parenthesis))
            && self.parser.look_ahead(1).is_keyword(kw::In)
        {
            self.parser.bump();
            self.parser.bump();
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
        }

        self.parser.expect(TokenKind::Colon)?;

        Ok(Some(ParsedBlockHeader { label, entry, hotness, entry_stack }))
    }

    fn current_block_label(&self) -> PResult<'sess, Option<Symbol>> {
        let TokenKind::Ident(symbol) = self.parser.token().kind else { return Ok(None) };
        let label = symbol.as_str();
        let Some(number) = label.strip_prefix("bb") else { return Ok(None) };
        if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
            return Err(self.parser.error("expected block number after `bb`"));
        }
        Ok(Some(symbol))
    }

    fn define_block(&mut self, module: &mut Module, label: Symbol) -> PResult<'sess, BlockId> {
        if let Some(block) = self.block_labels.get_mut(&label) {
            if block.defined {
                return Err(self.parser.error(format!("duplicate block `{label}`")));
            }
            block.defined = true;
            self.block_order.push(block.id);
            return Ok(block.id);
        }
        let id = module.add_block(Block::new(self.block_number(label)?));
        self.block_labels.insert(label, BlockLabel { id, defined: true, reference_span: None });
        self.block_order.push(id);
        Ok(id)
    }

    fn block_id(
        &mut self,
        module: &mut Module,
        label: Symbol,
        span: Span,
    ) -> PResult<'sess, BlockId> {
        if let Some(block) = self.block_labels.get(&label) {
            return Ok(block.id);
        }
        let id = module.add_block(Block::new(self.block_number(label)?));
        self.block_labels
            .insert(label, BlockLabel { id, defined: false, reference_span: Some(span) });
        Ok(id)
    }

    fn block_number(&self, label: Symbol) -> PResult<'sess, u32> {
        label.as_str()[2..]
            .parse()
            .map_err(|_| self.parser.error(format!("block label `{label}` is out of range")))
    }

    fn reject_unresolved_blocks(&self) -> PResult<'sess, ()> {
        let mut unresolved = self
            .block_labels
            .iter()
            .filter_map(|(label, block)| (!block.defined).then_some((label, block.reference_span)))
            .collect::<Vec<_>>();
        unresolved.sort_unstable_by_key(|(label, _)| label.as_str());
        if let Some((label, span)) = unresolved.first() {
            let message = format!("unknown block `{label}`");
            return Err(match span {
                Some(span) => self.parser.error_at(*span, message),
                None => self.parser.error(message),
            });
        }
        Ok(())
    }

    fn parse_instruction_or_terminator(
        &mut self,
        module: &mut Module,
        block: BlockId,
    ) -> PResult<'sess, ()> {
        if module.blocks[block].terminator.is_some() {
            return Err(self.parser.error(format!(
                "instruction after terminator in block `bb{}`",
                module.blocks[block].label
            )));
        }

        let result = self.try_parse_result(module)?;
        let mnemonic = self.parser.parse_ident()?;
        if let Some(kind) = self.parse_terminator_kind(mnemonic, module)? {
            if result.is_some() {
                return Err(self.parser.error("terminator cannot produce a result"));
            }
            let metadata = self.parse_metadata()?;
            module.blocks[block].terminator = Some(Terminator { kind, metadata });
            return Ok(());
        }

        let (opcode, encoding) = match mnemonic {
            sym::push => (op::PUSH32, Instruction::ENCODED_PUSH),
            sym::push_deferred => (op::PUSH32, Instruction::ENCODED_PUSH | Instruction::DEFERRED),
            sym::push_immutable => (op::PUSH32, Instruction::ENCODED_PUSH | Instruction::IMMUTABLE),
            _ => (
                op::from_ir_symbol(mnemonic).ok_or_else(|| {
                    self.parser.error(format!("unknown instruction opcode `{mnemonic}`"))
                })?,
                0,
            ),
        };
        let encoded_push = encoding & Instruction::ENCODED_PUSH != 0;
        let has_operands = encoded_push
            || match op::stack_io(opcode) {
                Some((inputs, _)) => inputs > 0 && (result.is_some() || self.operand_starts_here()),
                None => self.operand_starts_here(),
            };
        let operands = if has_operands { self.parse_operand_list(module)? } else { Vec::new() };
        let metadata = self.parse_metadata()?;
        module.blocks[block].instructions.push(Instruction {
            result,
            opcode,
            encoding,
            operands,
            metadata,
        });
        Ok(())
    }

    fn try_parse_result(&mut self, module: &mut Module) -> PResult<'sess, Option<ValueId>> {
        if !self.parser.check(TokenKind::BinOp(BinOpToken::Percent))
            || !matches!(
                self.parser.look_ahead(1).kind,
                TokenKind::Ident(_) | TokenKind::Literal(..)
            )
            || self.parser.look_ahead(2).kind != TokenKind::Eq
        {
            return Ok(None);
        }
        let name = self.parse_value_name()?;
        self.parser.bump();
        let value = self.value_id(module, name);
        if !self.defined_values.insert(value) {
            return Err(self.parser.error(format!("duplicate value `%{name}`")));
        }
        Ok(Some(value))
    }

    fn parse_value_name(&mut self) -> PResult<'sess, Symbol> {
        self.parser.expect(TokenKind::BinOp(BinOpToken::Percent))?;
        let name = match self.parser.token().kind {
            TokenKind::Ident(symbol) | TokenKind::Literal(_, symbol) => symbol,
            _ => return Err(self.parser.error("expected value name")),
        };
        self.parser.bump();
        Ok(name)
    }

    fn parse_terminator_kind(
        &mut self,
        mnemonic: Symbol,
        module: &mut Module,
    ) -> PResult<'sess, Option<TerminatorKind>> {
        let kind = match mnemonic {
            sym::jump if !self.operand_starts_here() => TerminatorKind::RawOpcode(op::JUMP),
            sym::jump => TerminatorKind::Jump(self.parse_block_ref(module)?),
            sym::br => {
                let condition = self.parse_operand(module)?;
                self.parser.expect(TokenKind::Comma)?;
                let then_block = self.parse_block_ref(module)?;
                self.parser.expect(TokenKind::Comma)?;
                let else_block = self.parse_block_ref(module)?;
                TerminatorKind::Branch { condition, then_block, else_block }
            }
            kw::Switch => {
                let value = self.parse_operand(module)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect_keyword(kw::Default)?;
                let default = self.parse_block_ref(module)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
                let mut cases = Vec::new();
                if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                    loop {
                        let case = self.parse_operand(module)?;
                        self.parser.expect(TokenKind::FatArrow)?;
                        let target = self.parse_block_ref(module)?;
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
            kw::Return if !self.operand_starts_here() => TerminatorKind::RawOpcode(op::RETURN),
            kw::Return => {
                let offset = self.parse_operand(module)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_operand(module)?;
                TerminatorKind::Return { offset, size }
            }
            kw::Revert if !self.operand_starts_here() => TerminatorKind::RawOpcode(op::REVERT),
            kw::Revert => {
                let offset = self.parse_operand(module)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_operand(module)?;
                TerminatorKind::Revert { offset, size }
            }
            kw::Stop => TerminatorKind::Stop,
            kw::Invalid => TerminatorKind::Invalid,
            kw::Selfdestruct if !self.operand_starts_here() => {
                TerminatorKind::RawOpcode(op::SELFDESTRUCT)
            }
            kw::Selfdestruct => {
                let recipient = self.parse_operand(module)?;
                TerminatorKind::SelfDestruct { recipient }
            }
            sym::terminal => {
                let opcode = if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
                    let opcode = self.parser.parse_uint()?;
                    let Ok(opcode) = u8::try_from(opcode) else {
                        return Err(self.parser.error("raw terminal opcode must fit in one byte"));
                    };
                    opcode
                } else {
                    let mnemonic = self.parser.parse_ident()?;
                    let Some(opcode) = op::from_ir_symbol(mnemonic) else {
                        return Err(self
                            .parser
                            .error(format!("unknown terminal opcode `{mnemonic}`")));
                    };
                    opcode
                };
                TerminatorKind::RawOpcode(opcode)
            }
            sym::raw => {
                let opcode = self.parser.parse_uint()?;
                let Ok(opcode) = u8::try_from(opcode) else {
                    return Err(self.parser.error("raw opcode must fit in one byte"));
                };
                TerminatorKind::RawOpcode(opcode)
            }
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }

    fn parse_operand_list(&mut self, module: &mut Module) -> PResult<'sess, Vec<Operand>> {
        let mut operands = Vec::new();
        loop {
            operands.push(self.parse_operand(module)?);
            if !self.parser.eat(TokenKind::Comma) {
                break;
            }
        }
        Ok(operands)
    }

    fn parse_operand(&mut self, module: &mut Module) -> PResult<'sess, Operand> {
        if self.parser.check(TokenKind::BinOp(BinOpToken::Percent)) {
            let name = self.parse_value_name()?;
            return Ok(Operand::Value(self.value_id(module, name)));
        }
        if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
            return Ok(Operand::Immediate(self.parser.parse_uint()?));
        }
        if let Some(label) = self.current_block_label()? {
            let span = self.parser.token().span;
            self.parser.bump();
            return Ok(Operand::Block(self.block_id(module, label, span)?));
        }
        Err(self.parser.error("expected operand"))
    }

    fn parse_block_ref(&mut self, module: &mut Module) -> PResult<'sess, BlockId> {
        let label =
            self.current_block_label()?.ok_or_else(|| self.parser.error("expected block label"))?;
        let span = self.parser.token().span;
        self.parser.bump();
        self.block_id(module, label, span)
    }

    fn parse_metadata(&mut self) -> PResult<'sess, Metadata> {
        let mut metadata = Metadata::default();
        if !self.parser.eat(TokenKind::Not) {
            return Ok(metadata);
        }
        self.parser.expect_keyword(sym::meta)?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        if self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            return Ok(metadata);
        }

        loop {
            let key = self.parser.parse_ident()?;
            if key == sym::stack {
                self.parser.expect(TokenKind::Eq)?;
                let inputs = self.parse_u16()?;
                self.parser.expect(TokenKind::Arrow)?;
                let outputs = self.parse_u16()?;
                metadata.stack = Some(StackEffect::new(inputs, outputs));
            } else if self.parser.eat(TokenKind::Eq) {
                let value = self.parse_metadata_value()?;
                metadata.attrs.push(MetadataItem { key, value: Some(value) });
            } else {
                metadata.attrs.push(MetadataItem { key, value: None });
            }

            if self.parser.eat(TokenKind::Comma) {
                continue;
            }
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            break;
        }
        Ok(metadata)
    }

    fn parse_metadata_value(&mut self) -> PResult<'sess, Symbol> {
        let start = self.parser.token().span.lo();
        let mut end = start;
        while !self.parser.is_eof()
            && !self.parser.check(TokenKind::Comma)
            && !self.parser.check(TokenKind::CloseDelim(Delimiter::Parenthesis))
        {
            end = self.parser.token().span.hi();
            self.parser.bump();
        }
        let start = (start - self.source.start_pos).to_usize();
        let end = (end - self.source.start_pos).to_usize();
        let value = self.source.src[start..end].trim();
        if value.is_empty() {
            return Err(self.parser.error("expected metadata value"));
        }
        Ok(Symbol::intern(value))
    }

    fn parse_u16(&mut self) -> PResult<'sess, u16> {
        let value = self.parser.parse_uint()?;
        value
            .try_into()
            .map_err(|_| self.parser.error(format!("integer `{value}` does not fit in u16")))
    }

    fn operand_starts_here(&self) -> bool {
        match self.parser.token().kind {
            TokenKind::Literal(..) => true,
            TokenKind::BinOp(BinOpToken::Percent) => {
                self.parser.look_ahead(2).kind != TokenKind::Eq
            }
            TokenKind::Ident(symbol) => {
                symbol.as_str().starts_with("bb")
                    && !matches!(
                        self.parser.look_ahead(1).kind,
                        TokenKind::Colon
                            | TokenKind::OpenDelim(Delimiter::Parenthesis | Delimiter::Bracket)
                    )
            }
            _ => false,
        }
    }

    fn value_id(&mut self, module: &mut Module, name: Symbol) -> ValueId {
        if let Some(value) = self.value_labels.get(&name).copied() {
            return value;
        }
        let value = module.add_value(name);
        self.value_labels.insert(name, value);
        value
    }
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
    fn parser_does_not_treat_newlines_as_syntax() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let input = "@module m bb0 (entry): %a = push 1 %b = push 2 %c = add %a, %b jump bb1 bb1 [cold]: jump";
        sess.enter(|| {
            let module = parse_module(&sess, input).unwrap();
            assert_data_eq!(
                module.to_text().to_string(),
                str![[r#"
@module m
bb0 (entry):
  %a = push 1
  %b = push 2
  %c = add %a, %b
  jump bb1
bb1 [cold]:
  jump

"#]]
            );
        });
    }

    #[test]
    fn parser_rejects_instructions_after_terminator() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let input = "\
@module m

bb0 (entry):
  stop
  invalid
";
        sess.enter(|| assert!(parse_module(&sess, input).is_err()));
        assert_data_eq!(
            sess.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
error: instruction after terminator in block `bb0`
  ╭▸ <evmir-test-10709633122247444245>:5:3
  │
5 │   invalid
  ╰╴  ━━━━━━━


"#]]
        );
    }

    #[test]
    fn parser_handles_doc_comments_and_empty_metadata_values() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let input = "\
/// module docs
@module m

bb0 (entry):
  %v0 = add 1 !meta(foo= )
";
        let source = sess
            .source_map()
            .new_source_file(FileName::Custom("empty-metadata.evmir".into()), input)
            .unwrap();
        sess.enter(|| assert!(Module::parse(&sess, &source).is_err()));
        assert_data_eq!(
            sess.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
error: expected metadata value
  ╭▸ <empty-metadata.evmir>:5:26
  │
5 │   %v0 = add 1 !meta(foo= )
  ╰╴                         ━


"#]]
        );
    }

    #[test]
    fn unresolved_block_reports_reference_span() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let input = "@module m\n\nbb0 (entry):\n  jump bb9\n";
        let source = sess
            .source_map()
            .new_source_file(FileName::Custom("unknown-block.evmir".into()), input)
            .unwrap();
        sess.enter(|| assert!(Module::parse(&sess, &source).is_err()));
        assert_data_eq!(
            sess.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
error: unknown block `bb9`
  ╭▸ <unknown-block.evmir>:4:8
  │
4 │   jump bb9
  ╰╴       ━━━


"#]]
        );
    }

    #[test]
    fn parser_rejects_hotness_block_metadata() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let input = "@module m\n\nbb0 (entry) [hotness=cold]:\n  stop\n";
        let source = sess
            .source_map()
            .new_source_file(FileName::Custom("hotness.evmir".into()), input)
            .unwrap();
        sess.enter(|| assert!(Module::parse(&sess, &source).is_err()));
        assert_data_eq!(
            sess.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
error: expected `cold`
  ╭▸ <hotness.evmir>:3:14
  │
3 │ bb0 (entry) [hotness=cold]:
  ╰╴             ━━━━━━━


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

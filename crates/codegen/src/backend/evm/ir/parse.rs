//! EVM IR text parser.

use super::*;
use crate::backend::evm::op;
use solar_ast::{
    Arena,
    token::{Delimiter, TokenKind},
};
use solar_data_structures::map::FxHashMap;
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
}

#[derive(Clone, Copy, Debug)]
struct BlockLabel {
    id: BlockId,
    defined: bool,
    reference_span: Option<Span>,
}

struct Parser<'sess, 'ast> {
    parser: crate::ir_parse::Parser<'sess, 'ast>,
    block_labels: FxHashMap<Symbol, BlockLabel>,
    block_order: Vec<BlockId>,
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    fn new(sess: &'sess Session, arena: &'ast Arena, source: &SourceFile) -> Self {
        Self {
            parser: crate::ir_parse::Parser::new(sess, arena, source),
            block_labels: FxHashMap::default(),
            block_order: Vec::new(),
        }
    }

    fn parse_module(&mut self) -> PResult<'sess, Module> {
        self.parser.expect(TokenKind::At)?;
        self.parser.expect_keyword(sym::module)?;
        let name = self.parser.parse_ident()?;

        let mut module = Module::new(name.as_str());
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

        self.parser.expect(TokenKind::Colon)?;

        Ok(Some(ParsedBlockHeader { label, entry, hotness }))
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

        let mnemonic = self.parser.parse_ident()?;
        if let Some(kind) = self.parse_terminator_kind(mnemonic, module)? {
            let metadata = self.parse_metadata()?;
            module.blocks[block].terminator = Some(Terminator { kind, metadata });
            return Ok(());
        }

        let mut inst = match mnemonic {
            sym::push => match self.parse_push_value(module)? {
                PushValue::Immediate(value) => Instruction::push_value(value),
                PushValue::Block(block) => Instruction::push_block(block),
            },
            sym::push_deferred => {
                let id = self.parse_assembly_id("deferred constant")?;
                Instruction::push_deferred(assembly::DeferredConst::from_usize(id as usize))
            }
            sym::push_immutable => {
                let id = self.parse_immutable_id()?;
                self.parser.expect(TokenKind::Comma)?;
                let width = self.parse_u8()?;
                let type_size = TypeSize::try_new_fb_bytes(width).ok_or_else(|| {
                    self.parser.error("immutable width must be between 1 and 32 bytes")
                })?;
                Instruction::push_immutable(id, type_size)
            }
            _ => Instruction::opcode(op::from_ir_symbol(mnemonic).ok_or_else(|| {
                self.parser.error(format!("unknown instruction opcode `{mnemonic}`"))
            })?),
        };
        inst.metadata = self.parse_metadata()?;
        module.blocks[block].instructions.push(inst);
        Ok(())
    }

    fn parse_terminator_kind(
        &mut self,
        mnemonic: Symbol,
        module: &mut Module,
    ) -> PResult<'sess, Option<TerminatorKind>> {
        let kind = match mnemonic {
            sym::jump if !self.block_ref_starts_here()? => TerminatorKind::Op(op::JUMP),
            sym::jump => TerminatorKind::Jump(self.parse_block_ref(module)?),
            sym::jumpi if self.block_ref_starts_here()? => {
                let then_block = self.parse_block_ref(module)?;
                self.parser.expect(TokenKind::Comma)?;
                let else_block = self.parse_block_ref(module)?;
                TerminatorKind::JumpI { then_block, else_block }
            }
            kw::Return => TerminatorKind::Op(op::RETURN),
            kw::Revert => TerminatorKind::Op(op::REVERT),
            kw::Stop => TerminatorKind::Op(op::STOP),
            kw::Invalid => TerminatorKind::Op(op::INVALID),
            kw::Selfdestruct => TerminatorKind::Op(op::SELFDESTRUCT),
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
                TerminatorKind::Op(opcode)
            }
            sym::raw => {
                let opcode = self.parser.parse_uint()?;
                let Ok(opcode) = u8::try_from(opcode) else {
                    return Err(self.parser.error("raw opcode must fit in one byte"));
                };
                TerminatorKind::Op(opcode)
            }
            _ => return Ok(None),
        };
        Ok(Some(kind))
    }

    fn parse_push_value(&mut self, module: &mut Module) -> PResult<'sess, PushValue> {
        if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
            return Ok(PushValue::Immediate(self.parser.parse_uint()?));
        }
        if let Some(label) = self.current_block_label()? {
            let span = self.parser.token().span;
            self.parser.bump();
            return Ok(PushValue::Block(self.block_id(module, label, span)?));
        }
        Err(self.parser.error("expected push value"))
    }

    fn parse_assembly_id(&mut self, name: &str) -> PResult<'sess, u32> {
        let span = self.parser.token().span;
        let value = self.parser.parse_uint()?;
        let Ok(value) = u32::try_from(value) else {
            return Err(self
                .parser
                .error_at(span, format!("{name} ID exceeds the assembler limit")));
        };
        if value > assembly::AsmInst::PAYLOAD_MASK {
            return Err(self
                .parser
                .error_at(span, format!("{name} ID exceeds the assembler limit")));
        }
        Ok(value)
    }

    fn parse_immutable_id(&mut self) -> PResult<'sess, ImmutableId> {
        let span = self.parser.token().span;
        let value = self.parser.parse_uint()?;
        let Ok(value) = u32::try_from(value) else {
            return Err(self.parser.error_at(span, "immutable ID exceeds the index limit"));
        };
        if value == u32::MAX {
            return Err(self.parser.error_at(span, "immutable ID exceeds the index limit"));
        }
        Ok(ImmutableId::new(value as usize))
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
                let inputs = self.parse_u8()?;
                self.parser.expect(TokenKind::Arrow)?;
                let outputs = self.parse_u8()?;
                metadata.stack = Some(StackEffect::new(inputs, outputs));
            } else if self.parser.eat(TokenKind::Eq) {
                self.skip_metadata_value()?;
            }

            if self.parser.eat(TokenKind::Comma) {
                continue;
            }
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            break;
        }
        Ok(metadata)
    }

    fn skip_metadata_value(&mut self) -> PResult<'sess, ()> {
        let mut has_value = false;
        while !self.parser.is_eof()
            && !self.parser.check(TokenKind::Comma)
            && !self.parser.check(TokenKind::CloseDelim(Delimiter::Parenthesis))
        {
            has_value = true;
            self.parser.bump();
        }
        if !has_value {
            return Err(self.parser.error("expected metadata value"));
        }
        Ok(())
    }

    fn parse_u8(&mut self) -> PResult<'sess, u8> {
        let value = self.parser.parse_uint()?;
        value
            .try_into()
            .map_err(|_| self.parser.error(format!("integer `{value}` does not fit in u8")))
    }

    fn block_ref_starts_here(&self) -> PResult<'sess, bool> {
        Ok(self.current_block_label()?.is_some()
            && !matches!(
                self.parser.look_ahead(1).kind,
                TokenKind::Colon
                    | TokenKind::OpenDelim(Delimiter::Parenthesis | Delimiter::Bracket)
            ))
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
        let mut dirs = vec![dir.clone()];
        while let Some(dir) = dirs.pop() {
            for entry in std::fs::read_dir(dir).unwrap() {
                let path = entry.unwrap().path();
                if path.is_dir() {
                    dirs.push(path);
                    continue;
                }
                if path.extension().and_then(|s| s.to_str()) != Some("evmir") {
                    continue;
                }
                count += 1;
                if let Err(err) = round_trip_fixture(&path) {
                    let name = path.file_name().unwrap().to_string_lossy();
                    failures.push(format!("{name}: {err}"));
                }
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
        let input = "@module m bb0 (entry): push 1 push 2 add jump bb1 bb1 [cold]: jump";
        sess.enter(|| {
            let module = parse_module(&sess, input).unwrap();
            assert_data_eq!(
                module.to_text().to_string(),
                str![[r#"
@module m
bb0 (entry):
  push 1
  push 2
  add
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
  push 1
  add !meta(foo= )
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
  ╭▸ <empty-metadata.evmir>:6:18
  │
6 │   add !meta(foo= )
  ╰╴                 ━


"#]]
        );
    }

    #[test]
    fn parser_rejects_invalid_special_pushes() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let cases = [
            ("deferred-block.evmir", "push_deferred bb1"),
            ("immutable-block.evmir", "push_immutable bb1"),
            ("deferred-overflow.evmir", "push_deferred 0x10000000"),
            ("immutable-overflow.evmir", "push_immutable 0xffffffff, 32"),
        ];
        sess.enter(|| {
            for (name, instruction) in cases {
                let input = format!("@module m\n\nbb0 (entry):\n  {instruction}\n  stop\n");
                let source = sess
                    .source_map()
                    .new_source_file(FileName::Custom(name.into()), input)
                    .unwrap();
                assert!(Module::parse(&sess, &source).is_err());
            }
        });
        assert_data_eq!(
            sess.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
error: expected integer literal
  ╭▸ <deferred-block.evmir>:4:17
  │
4 │   push_deferred bb1
  ╰╴                ━━━

error: expected integer literal
  ╭▸ <immutable-block.evmir>:4:18
  │
4 │   push_immutable bb1
  ╰╴                 ━━━

error: deferred constant ID exceeds the assembler limit
  ╭▸ <deferred-overflow.evmir>:4:17
  │
4 │   push_deferred 0x10000000
  ╰╴                ━━━━━━━━━━

error: immutable ID exceeds the index limit
  ╭▸ <immutable-overflow.evmir>:4:18
  │
4 │   push_immutable 0xffffffff, 32
  ╰╴                 ━━━━━━━━━━


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

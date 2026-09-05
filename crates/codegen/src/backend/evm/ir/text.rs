//! Parsing and canonical printing of physical EVM blocks and relocations.

use super::{Block, BlockId, Data, DataId, InstKind, Instruction, Module, TerminatorKind};
use crate::{backend::evm::op, ir_parse::Parser, mir::ImmutableId};
use alloy_primitives::U256;
use solar_ast::{
    Arena,
    token::{Delimiter, TokenKind},
};
use solar_data_structures::map::FxHashMap;
use solar_interface::{Result, Session, Span, source_map::SourceFile, sym};
use solar_parse::{PErr, PResult};
use std::fmt::{self, Write};

pub(super) fn parse(sess: &Session, source: &SourceFile) -> Result<Module> {
    let arena = Arena::new();
    let parser = Parser::new(sess, &arena, source);
    Reader { parser, references: Vec::new(), reference_ids: FxHashMap::default() }
        .module()
        .map_err(PErr::emit)
}

struct Reader<'sess, 'ast> {
    parser: Parser<'sess, 'ast>,
    references: Vec<(u32, Span)>,
    reference_ids: FxHashMap<u32, BlockId>,
}

impl<'sess> Reader<'sess, '_> {
    fn module(mut self) -> PResult<'sess, Module> {
        self.parser.expect(TokenKind::At)?;
        self.parser.expect_keyword(sym::module)?;
        let mut module = Module { name: self.parser.parse_ident()?, ..Module::default() };
        let mut labels = FxHashMap::default();
        let mut data_labels = FxHashMap::default();
        while !self.parser.is_eof() {
            if self.parser.eat(TokenKind::At) {
                self.parser.expect_keyword(sym::data)?;
                let (id, name) = self.parser.parse_data_id()?;
                let id = self.data_id(id)?;
                let bytes = self.parser.parse_data_bytes()?.to_vec();
                if data_labels.insert(id, module.data.next_idx()).is_some() {
                    return Err(self.parser.error("duplicate program data identifier"));
                }
                // @data id bytes
                module.data.push(Data { name, bytes });
                continue;
            }
            let label_span = self.parser.token().span;
            let label = self.block_label()?;
            if labels.insert(label, module.blocks.next_idx()).is_some() {
                return Err(self.parser.error_at(label_span, "duplicate block identifier"));
            }
            if label as usize != module.blocks.next_idx().index() {
                module.labels.insert(module.blocks.next_idx(), label);
            }
            let mut block = Block::default();
            if self.parser.eat(TokenKind::OpenDelim(Delimiter::Bracket)) {
                loop {
                    if self.parser.eat_keyword(sym::cold) {
                        block.cold = true;
                    } else if self.parser.eat_keyword(sym::Loop) {
                        block.loop_header = true;
                    } else {
                        return Err(self.parser.error("expected `cold` or `loop` block attribute"));
                    }
                    if !self.parser.eat(TokenKind::Comma) {
                        break;
                    }
                }
                self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
            }
            self.parser.expect(TokenKind::Colon)?;
            let mut terminated = false;
            while !self.parser.is_eof() && !self.parser.check(TokenKind::At) && !self.at_label() {
                if terminated {
                    return Err(self
                        .parser
                        .error(format!("instruction after terminator in block `bb{}`", label)));
                }
                let name = self.parser.parse_ident()?;
                let kind = if name == sym::push {
                    if self.is_block_ref() {
                        InstKind::PushLabel(self.block_ref()?)
                    } else {
                        InstKind::Push(self.parser.parse_uint()?)
                    }
                } else if name == sym::push_data {
                    let (id, offset, _) = self.parser.parse_data_ref()?;
                    InstKind::PushData { id: self.data_id(id)?, offset }
                } else if name == sym::push_deferred {
                    let span = self.parser.token().span;
                    let value = self.parser.parse_uint()?;
                    if value >= U256::from(1u32 << 28) {
                        return Err(self
                            .parser
                            .error_at(span, "deferred constant ID exceeds the assembler limit"));
                    }
                    InstKind::PushDeferred(value.to())
                } else if name == sym::push_immutable {
                    let span = self.parser.token().span;
                    let value = self.parser.parse_uint()?;
                    if value >= U256::from(u32::MAX) {
                        return Err(self
                            .parser
                            .error_at(span, "immutable ID exceeds the index limit"));
                    }
                    self.parser.expect(TokenKind::Comma)?;
                    let width = self.small_uint()?;
                    let width = u8::try_from(width)
                        .map_err(|_| self.parser.error("immutable width exceeds `u8`"))?;
                    InstKind::PushImmutable { id: ImmutableId::new(value.to::<usize>()), width }
                } else if name == sym::dup {
                    InstKind::Dup(self.small_uint()?)
                } else if name == sym::swap {
                    InstKind::Swap(self.small_uint()?)
                } else if name == sym::exchange {
                    let first = self.small_uint()?;
                    self.parser.expect(TokenKind::Comma)?;
                    InstKind::Exchange(first, self.small_uint()?)
                } else if name == sym::jump || name == sym::jumpi || name == sym::indexed_jump {
                    let term = if name == sym::jump {
                        Some(if self.is_block_ref() {
                            TerminatorKind::Jump(self.block_ref()?)
                        } else {
                            TerminatorKind::DynamicJump
                        })
                    } else if name == sym::indexed_jump {
                        let mut targets = vec![self.block_ref()?];
                        while self.parser.eat(TokenKind::Comma) {
                            targets.push(self.block_ref()?);
                        }
                        Some(TerminatorKind::IndexedJump(targets))
                    } else if self.is_block_ref() {
                        let yes = self.block_ref()?;
                        self.parser.expect(TokenKind::Comma)?;
                        Some(TerminatorKind::JumpI(yes, self.block_ref()?))
                    } else {
                        None
                    };
                    if let Some(term) = term {
                        // terminator targets !meta(stack=inputs->outputs)
                        block.terminator = term.into();
                        block.terminator.stack_effect = self.metadata()?;
                        terminated = true;
                        continue;
                    }
                    InstKind::Op(op::JUMPI)
                } else {
                    let opcode = op::parse_name(name.as_str())
                        .or_else(|| {
                            name.as_str()
                                .strip_prefix("op_")
                                .and_then(|value| u8::from_str_radix(value, 16).ok())
                        })
                        .ok_or_else(|| {
                            self.parser.error(format!("unknown EVM instruction `{name}`"))
                        })?;
                    let term = match opcode {
                        op::STOP => Some(TerminatorKind::Stop),
                        op::RETURN => Some(TerminatorKind::Return),
                        op::REVERT => Some(TerminatorKind::Revert),
                        op::INVALID => Some(TerminatorKind::Invalid),
                        op::SELFDESTRUCT => Some(TerminatorKind::SelfDestruct),
                        _ => None,
                    };
                    if let Some(term) = term {
                        // terminal_opcode !meta(stack=inputs->outputs)
                        block.terminator = term.into();
                        block.terminator.stack_effect = self.metadata()?;
                        terminated = true;
                        continue;
                    }
                    if opcode == op::PUSH0 {
                        InstKind::Push(U256::ZERO)
                    } else {
                        InstKind::Op(opcode)
                    }
                };
                // physical_instruction !meta(stack=inputs->outputs)
                block.insts.push(Instruction { kind, stack_effect: self.metadata()? });
            }
            if !terminated {
                return Err(self.parser.error("expected block terminator"));
            }
            // block:
            //   instructions
            //   terminator
            module.blocks.push(block);
        }
        for (label, span) in self.references {
            if !labels.contains_key(&label) {
                return Err(self.parser.error_at(span, format!("unknown block `bb{}`", label)));
            }
        }
        let mut resolved = solar_data_structures::index::IndexVec::<BlockId, BlockId>::from_vec(
            vec![BlockId::new(0); self.reference_ids.len()],
        );
        for (source, placeholder) in self.reference_ids {
            resolved[placeholder] = labels[&source];
        }
        // push source_label -> push canonical_label
        // jump source_label -> jump canonical_label
        for block in &mut module.blocks {
            for inst in &mut block.insts {
                match &mut inst.kind {
                    InstKind::PushLabel(label) => *label = resolved[*label],
                    InstKind::PushData { id, .. } => {
                        *id = *data_labels
                            .get(id)
                            .ok_or_else(|| self.parser.error("unknown program data identifier"))?;
                    }
                    _ => {}
                }
            }
            match &mut block.terminator.kind {
                TerminatorKind::Jump(label) => *label = resolved[*label],
                TerminatorKind::JumpI(yes, no) => {
                    *yes = resolved[*yes];
                    *no = resolved[*no];
                }
                TerminatorKind::IndexedJump(targets) => {
                    for label in targets {
                        *label = resolved[*label];
                    }
                }
                _ => {}
            }
        }
        Ok(module)
    }

    fn at_label(&self) -> bool {
        matches!(self.parser.token().kind, TokenKind::Ident(_))
            && matches!(
                self.parser.look_ahead(1).kind,
                TokenKind::Colon | TokenKind::OpenDelim(Delimiter::Bracket)
            )
    }

    fn is_block_ref(&self) -> bool {
        !self.at_label()
            && matches!(self.parser.token().kind, TokenKind::Ident(name) if name.as_str().strip_prefix("bb").is_some_and(|id| id.parse::<u32>().is_ok()))
    }

    fn block_label(&mut self) -> PResult<'sess, u32> {
        let name = self.parser.parse_ident()?;
        let id = name
            .as_str()
            .strip_prefix("bb")
            .and_then(|id| id.parse::<u32>().ok())
            .ok_or_else(|| self.parser.error("expected block identifier"))?;
        Ok(id)
    }

    fn block_ref(&mut self) -> PResult<'sess, BlockId> {
        let span = self.parser.token().span;
        let id = self.block_label()?;
        self.references.push((id, span));
        let next = BlockId::new(self.reference_ids.len());
        Ok(*self.reference_ids.entry(id).or_insert(next))
    }

    fn data_id(&self, value: U256) -> PResult<'sess, DataId> {
        let id = u32::try_from(value)
            .ok()
            .filter(|&id| id < u32::MAX)
            .ok_or_else(|| self.parser.error("program data ID exceeds the index limit"))?;
        Ok(DataId::new(id as usize))
    }

    fn small_uint(&mut self) -> PResult<'sess, u16> {
        u16::try_from(self.parser.parse_uint()?)
            .map_err(|_| self.parser.error("integer exceeds `u16`"))
    }

    fn metadata(&mut self) -> PResult<'sess, Option<(u8, u8)>> {
        if !self.parser.eat(TokenKind::Not) {
            return Ok(None);
        }
        self.parser.expect_keyword(sym::meta)?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let mut stack = None;
        while !self.parser.check(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            let key = self.parser.parse_ident()?;
            if self.parser.eat(TokenKind::Eq) {
                if key == sym::stack {
                    let inputs = self.small_uint()?;
                    self.parser.expect(TokenKind::Arrow)?;
                    let outputs = self.small_uint()?;
                    stack = Some((
                        u8::try_from(inputs)
                            .map_err(|_| self.parser.error("stack effect exceeds `u8`"))?,
                        u8::try_from(outputs)
                            .map_err(|_| self.parser.error("stack effect exceeds `u8`"))?,
                    ));
                } else {
                    if matches!(
                        self.parser.token().kind,
                        TokenKind::CloseDelim(_) | TokenKind::Comma | TokenKind::Eof
                    ) {
                        return Err(self.parser.error("expected metadata value"));
                    }
                    while !matches!(
                        self.parser.token().kind,
                        TokenKind::CloseDelim(_) | TokenKind::Comma | TokenKind::Eof
                    ) {
                        self.parser.bump();
                    }
                }
            }
            if !self.parser.eat(TokenKind::Comma) {
                break;
            }
        }
        self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        Ok(stack)
    }
}

pub(super) struct PrintedModule<'a>(pub(super) &'a Module);

impl fmt::Display for PrintedModule<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let module = self.0;
        writeln!(f, "@module {}", module.name)?;
        for id in module.block_ids() {
            let block = &module.blocks[id];
            write!(f, "bb{}", module.block_label(id))?;
            if block.cold || block.loop_header {
                f.write_str(" [")?;
                if block.cold {
                    f.write_str("cold")?;
                }
                if block.cold && block.loop_header {
                    f.write_str(", ")?;
                }
                if block.loop_header {
                    f.write_str("loop")?;
                }
                f.write_char(']')?;
            }
            f.write_str(":\n")?;
            for inst in &block.insts {
                write!(f, "  {}", PrintedInst(module, &inst.kind))?;
                print_effect(
                    f,
                    inst.stack_effect
                        .filter(|&effect| Some(effect) != super::verify::effect(&inst.kind)),
                )?;
                f.write_char('\n')?;
            }
            write!(f, "  {}", PrintedTerm(module, &block.terminator.kind))?;
            print_effect(
                f,
                block
                    .terminator
                    .stack_effect
                    .filter(|&effect| effect != super::verify::term_effect(&block.terminator.kind)),
            )?;
            f.write_char('\n')?;
        }
        if !module.data.is_empty() {
            f.write_char('\n')?;
            for (id, data) in module.data.iter_enumerated() {
                write!(f, "@data ")?;
                print_data_id(f, module, id)?;
                writeln!(f, " hex\"{}\"", alloy_primitives::hex::encode(&data.bytes))?;
            }
        }
        Ok(())
    }
}

fn print_effect(f: &mut fmt::Formatter<'_>, effect: Option<(u8, u8)>) -> fmt::Result {
    if let Some((inputs, outputs)) = effect {
        write!(f, " !meta(stack={inputs}->{outputs})")?;
    }
    Ok(())
}

fn print_data_id(f: &mut fmt::Formatter<'_>, module: &Module, id: DataId) -> fmt::Result {
    if let Some(name) = module.data.get(id).and_then(|data| data.name) {
        write!(f, "{name}_")?;
    }
    write!(f, "{}", id.index())
}

pub(crate) struct PrintedInst<'a>(pub(crate) &'a Module, pub(crate) &'a InstKind);

impl fmt::Display for PrintedInst<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            InstKind::Op(opcode) => match op::name(*opcode) {
                Some(name) => f.write_str(&name.to_ascii_lowercase()),
                None => write!(f, "op_{opcode:02x}"),
            },
            InstKind::Push(value) => write!(f, "push {}", Number(*value)),
            InstKind::PushLabel(id) => write!(f, "push bb{}", self.0.block_label(*id)),
            InstKind::PushData { id, offset } => {
                f.write_str("push_data ")?;
                print_data_id(f, self.0, *id)?;
                if *offset != 0 {
                    write!(f, "+{offset}")?;
                }
                Ok(())
            }
            InstKind::PushDeferred(id) => write!(f, "push_deferred {}", Number(U256::from(*id))),
            InstKind::PushImmutable { id, width } => {
                write!(f, "push_immutable {}, {width}", Number(U256::from(id.index())))
            }
            InstKind::Dup(depth) => write!(f, "dup {depth}"),
            InstKind::Swap(depth) => write!(f, "swap {depth}"),
            InstKind::Exchange(a, b) => write!(f, "exchange {a}, {b}"),
        }
    }
}

pub(crate) struct PrintedTerm<'a>(pub(crate) &'a Module, pub(crate) &'a TerminatorKind);

impl fmt::Display for PrintedTerm<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.1 {
            TerminatorKind::Jump(id) => write!(f, "jump bb{}", self.0.block_label(*id)),
            TerminatorKind::JumpI(yes, no) => {
                write!(f, "jumpi bb{}, bb{}", self.0.block_label(*yes), self.0.block_label(*no))
            }
            TerminatorKind::DynamicJump => f.write_str("jump"),
            TerminatorKind::IndexedJump(targets) => {
                f.write_str("indexed_jump ")?;
                for (index, id) in targets.iter().enumerate() {
                    if index > 0 {
                        f.write_str(", ")?;
                    }
                    write!(f, "bb{}", self.0.block_label(*id))?;
                }
                Ok(())
            }
            TerminatorKind::Stop => f.write_str("stop"),
            TerminatorKind::Return => f.write_str("return"),
            TerminatorKind::Revert => f.write_str("revert"),
            TerminatorKind::Invalid => f.write_str("invalid"),
            TerminatorKind::SelfDestruct => f.write_str("selfdestruct"),
            TerminatorKind::Unreachable => f.write_str("unreachable"),
        }
    }
}

struct Number(U256);

impl fmt::Display for Number {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.0 < U256::from(1000) { write!(f, "{}", self.0) } else { write!(f, "{:#x}", self.0) }
    }
}

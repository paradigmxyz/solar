//! Parser for the textual MIR format produced by [`Function::to_text`] and [`Module::to_text`].
//!
//! # Format
//!
//! ```text
//! @module Counter
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
//! [`Module::parse`] interns function and module names via [`Symbol::intern`], which requires an
//! active [`solar_interface::Session`]. Wrap calls in `sess.enter(|| ...)`.
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
    BlockId, EffectKind, Function, FunctionId, InstId, InstKind, Instruction, InstructionMetadata,
    MemoryRegion, Module, StorageAlias, Terminator, Value, ValueId,
};
use crate::mir::{Immediate, MirType};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_ast::{
    Arena,
    token::{BinOpToken, Delimiter, TokenKind, TokenLitKind},
};
use solar_data_structures::map::FxHashMap;
use solar_interface::{
    BytePos, Ident, Result, Session, Span, Symbol, kw, source_map::SourceFile, sym,
};
use solar_parse::{PErr, PResult};
use solar_sema::hir;

// =============================================================================
// Public API
// =============================================================================

pub(super) fn parse(sess: &Session, source: &SourceFile) -> Result<Module> {
    let arena = Arena::new();
    let mut parser = Parser::new(sess, &arena, source);
    parser.parse_module().map_err(PErr::emit)
}

#[cfg(test)]
pub(super) fn parse_module(sess: &Session, input: &str) -> Result<Module> {
    let id = input.bytes().fold(0u64, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte.into()));
    let file = sess
        .source_map()
        .new_source_file(
            solar_interface::source_map::FileName::Custom(format!("mir-test-{id}")),
            input,
        )
        .unwrap();
    Module::parse(sess, &file)
}

#[cfg(test)]
fn parse_function(sess: &Session, input: &str) -> Result<Function> {
    let id = input.bytes().fold(0u64, |hash, byte| hash.wrapping_mul(31).wrapping_add(byte.into()));
    let source = sess
        .source_map()
        .new_source_file(
            solar_interface::source_map::FileName::Custom(format!("mir-function-test-{id}")),
            input,
        )
        .unwrap();
    let arena = Arena::new();
    let mut p = Parser::new(sess, &arena, &source);
    let func = p.parse_function().map_err(PErr::emit)?;
    if !p.parser.is_eof() {
        return Err(p.parser.error("trailing input after function").emit());
    }
    Ok(func)
}

// =============================================================================
// Parser implementation
// =============================================================================

struct Parser<'sess, 'ast> {
    parser: crate::ir_parse::Parser<'sess, 'ast>,
    pending_function_ref: Option<(Symbol, Span)>,
    function_refs: Vec<PendingFunctionRef>,
    arg_values: Vec<ValueId>,
    block_labels: FxHashMap<u32, BlockLabel>,
    block_order: Vec<BlockId>,
    value_labels: FxHashMap<u32, ValueId>,
}

struct PendingFunctionRef {
    name: Symbol,
    span: Span,
    target: FunctionRefTarget,
}

enum FunctionRefTarget {
    Instruction(InstId),
    Terminator(BlockId),
}

#[derive(Clone, Copy)]
struct BlockLabel {
    id: BlockId,
    defined: bool,
    reference_span: Option<Span>,
}

impl<'sess, 'ast> Parser<'sess, 'ast> {
    fn new(sess: &'sess Session, arena: &'ast Arena, source: &SourceFile) -> Self {
        Self {
            parser: crate::ir_parse::Parser::new(sess, arena, source),
            pending_function_ref: None,
            function_refs: Vec::new(),
            arg_values: Vec::new(),
            block_labels: FxHashMap::default(),
            block_order: Vec::new(),
            value_labels: FxHashMap::default(),
        }
    }

    /// Parses a phase name such as `evm-shaped`. Unlike an identifier, a phase
    /// name may contain internal hyphens.
    fn parse_phase_name(&mut self) -> PResult<'sess, Symbol> {
        let mut name = self.parser.parse_ident()?.to_string();
        while self.parser.eat(TokenKind::BinOp(BinOpToken::Minus)) {
            name.push('-');
            name.push_str(self.parser.parse_ident()?.as_str());
        }
        Ok(Symbol::intern(&name))
    }

    /// Parses a function name: an identifier, optionally with `.`-joined
    /// segments (`f.body`), as minted by the ABI lowering.
    fn parse_function_name(&mut self) -> PResult<'sess, String> {
        let mut name = self.parser.parse_ident()?.to_string();
        while self.parser.eat(TokenKind::Dot) {
            name.push('.');
            name.push_str(self.parser.parse_ident()?.as_str());
        }
        Ok(name)
    }

    // ----- module / function parsing -----

    fn parse_module(&mut self) -> PResult<'sess, Module> {
        let mut phase = super::MirPhase::default();
        self.parser.expect(TokenKind::At)?;
        self.parser.expect_keyword(sym::module)?;
        let module_name = self.parser.parse_ident()?.to_string();
        while self.parser.eat(TokenKind::At) {
            let attr = self.parser.parse_ident()?;
            match attr {
                sym::phase => {
                    let phase_span = self.parser.token().span;
                    let phase_name = self.parse_phase_name()?;
                    phase = super::MirPhase::by_name(phase_name).ok_or_else(|| {
                        self.parser
                            .error_at(phase_span, format!("unknown MIR phase `{phase_name}`"))
                    })?;
                }
                _ => return Err(self.parser.error(format!("unknown module attribute `@{attr}`"))),
            }
        }

        let module_ident = Ident::with_dummy_span(Symbol::intern(&module_name));
        let mut module = Module::new(module_ident);
        module.phase = phase;
        let mut function_refs = Vec::new();

        while !self.parser.is_eof() {
            let func = self.parse_function()?;
            let function = module.add_function(func);
            function_refs
                .extend(self.function_refs.drain(..).map(|reference| (function, reference)));
        }
        self.resolve_function_refs(&mut module, function_refs)?;

        Ok(module)
    }

    fn resolve_function_refs(
        &self,
        module: &mut Module,
        function_refs: Vec<(FunctionId, PendingFunctionRef)>,
    ) -> PResult<'sess, ()> {
        let mut functions = FxHashMap::<Symbol, Vec<FunctionId>>::default();
        for (id, function) in module.functions.iter_enumerated() {
            functions.entry(function.name.name).or_default().push(id);
        }
        for (owner, reference) in function_refs {
            let Some(matches) = functions.get(&reference.name) else {
                return Err(self
                    .parser
                    .error_at(reference.span, format!("unknown function `@{}`", reference.name)));
            };
            let [function] = matches.as_slice() else {
                return Err(self.parser.error_at(
                    reference.span,
                    format!(
                        "function name `@{}` is ambiguous; use the positional `fnN` form",
                        reference.name
                    ),
                ));
            };
            match reference.target {
                FunctionRefTarget::Instruction(inst) => {
                    let InstKind::InternalCall { function: target, .. } =
                        &mut module.functions[owner].instructions[inst].kind
                    else {
                        unreachable!()
                    };
                    *target = *function;
                }
                FunctionRefTarget::Terminator(block) => {
                    let Some(Terminator::TailCall { function: target, .. }) =
                        &mut module.functions[owner].blocks[block].terminator
                    else {
                        unreachable!()
                    };
                    *target = *function;
                }
            }
        }
        Ok(())
    }

    fn parse_function(&mut self) -> PResult<'sess, Function> {
        self.arg_values.clear();
        self.block_labels.clear();
        self.block_order.clear();
        self.value_labels.clear();

        self.parser.expect_keyword(sym::fn_)?;
        self.parser.expect(TokenKind::At)?;
        let name = self.parse_function_name()?;
        let func_ident = Ident::with_dummy_span(Symbol::intern(&name));
        let mut func = Function::new(func_ident);

        // Parse parameters: `(arg0: ty, arg1: ty, ...)` or `()`
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            loop {
                let arg_name = self.parser.parse_ident()?;
                let arg_name_str = arg_name.as_str();
                if !arg_name_str.starts_with("arg") {
                    return Err(self.parser.error(format!("expected `argN`, got `{arg_name}`")));
                }
                arg_name_str[3..]
                    .parse::<u32>()
                    .map_err(|_| self.parser.error(format!("invalid arg index in `{arg_name}`")))?;
                self.parser.expect(TokenKind::Colon)?;
                let ty = self.parse_type()?;
                let index = func.params.len() as u32;
                func.params.push(ty);
                let val = func.alloc_value(Value::Arg { index, ty });
                self.arg_values.push(val);
                if self.parser.eat(TokenKind::Comma) {
                    continue;
                }
                self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
                break;
            }
        }

        // Optional return type: `-> ty` or `-> (ty, ty, ...)`
        if self.parser.eat(TokenKind::Arrow) {
            if self.parser.eat(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
                if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                    loop {
                        let ty = self.parse_type()?;
                        func.returns.push(ty);
                        if self.parser.eat(TokenKind::Comma) {
                            continue;
                        }
                        self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
                        break;
                    }
                }
            } else {
                let ty = self.parse_type()?;
                func.returns.push(ty);
            }
        }

        self.parse_function_attributes(&mut func)?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Brace))?;

        let mut current_block = None;

        loop {
            if self.parser.is_eof() {
                return Err(self.parser.error("unterminated function body"));
            }
            if self.parser.eat(TokenKind::CloseDelim(Delimiter::Brace)) {
                break;
            }

            if let Some(idx) = self.try_parse_block_header()? {
                let bid = self.define_block(&mut func, idx)?;
                current_block = Some(bid);
                continue;
            }

            // Not a block header — must be an instruction or terminator.
            let block = current_block
                .ok_or_else(|| self.parser.error("instruction outside of any block"))?;
            self.parse_instruction_or_terminator(&mut func, block)?;
        }

        self.reject_unresolved_block_labels()?;
        self.reject_unresolved_value_labels(&func)?;
        let block_remap = crate::mir::utils::remap_block_order(&mut func, &self.block_order);
        for reference in &mut self.function_refs {
            if let FunctionRefTarget::Terminator(block) = &mut reference.target {
                *block = block_remap[block.index()];
            }
        }

        Ok(func)
    }

    fn try_parse_block_header(&mut self) -> PResult<'sess, Option<u32>> {
        let TokenKind::Ident(label) = self.parser.token().kind else { return Ok(None) };
        let Some(index) = label.as_str().strip_prefix("bb").filter(|s| !s.is_empty()) else {
            return Ok(None);
        };
        let Ok(index) = index.parse() else {
            return Ok(None);
        };
        if !matches!(
            self.parser.look_ahead(1).kind,
            TokenKind::Colon | TokenKind::OpenDelim(Delimiter::Parenthesis)
        ) {
            return Ok(None);
        }
        self.parser.bump();
        if self.parser.eat(TokenKind::OpenDelim(Delimiter::Parenthesis)) {
            self.parser.expect_keyword(sym::entry)?;
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        }
        self.parser.expect(TokenKind::Colon)?;
        Ok(Some(index))
    }

    fn define_block(&mut self, func: &mut Function, index: u32) -> PResult<'sess, BlockId> {
        if let Some(label) = self.block_labels.get_mut(&index) {
            if label.defined {
                return Err(self.parser.error(format!("duplicate block `bb{index}`")));
            }
            label.defined = true;
            self.block_order.push(label.id);
            return Ok(label.id);
        }
        let id = if self.block_labels.is_empty() { func.entry_block } else { func.alloc_block() };
        self.block_labels.insert(index, BlockLabel { id, defined: true, reference_span: None });
        self.block_order.push(id);
        Ok(id)
    }

    fn parse_function_attributes(&mut self, func: &mut Function) -> PResult<'sess, ()> {
        if !self.parser.eat(TokenKind::OpenDelim(Delimiter::Bracket)) {
            return Ok(());
        }

        loop {
            let key = self.parser.parse_ident()?;
            match key {
                sym::selector => {
                    self.parser.expect(TokenKind::Eq)?;
                    let selector = self.parser.parse_uint()?;
                    let selector = self.u256_to_u32(selector)?;
                    func.selector = Some(selector.to_be_bytes());
                }
                kw::Receive => func.attributes.is_receive = true,
                kw::Fallback => func.attributes.is_fallback = true,
                kw::Payable => {
                    func.attributes.state_mutability = hir::StateMutability::Payable;
                }
                _ => return Err(self.parser.error(format!("unknown function attribute `{key}`"))),
            }

            if self.parser.eat(TokenKind::Comma) {
                continue;
            }
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
            break;
        }

        Ok(())
    }

    fn parse_type(&mut self) -> PResult<'sess, MirType> {
        let id = self.parser.parse_ident()?;
        let id_str = id.as_str();
        // u8..u256, i8..i256, bytes1..bytes32 — split into prefix + number.
        let ty = if let Some(rest) = id_str.strip_prefix('u') {
            let bits: u16 =
                rest.parse().map_err(|_| self.parser.error(format!("invalid u-type `{id}`")))?;
            MirType::UInt(bits)
        } else if let Some(rest) = id_str.strip_prefix('i') {
            let bits: u16 =
                rest.parse().map_err(|_| self.parser.error(format!("invalid i-type `{id}`")))?;
            MirType::Int(bits)
        } else if let Some(rest) = id_str.strip_prefix("bytes") {
            let n: u8 = rest
                .parse()
                .map_err(|_| self.parser.error(format!("invalid bytes type `{id}`")))?;
            MirType::FixedBytes(n)
        } else {
            match id {
                kw::Bool => MirType::Bool,
                kw::Address => MirType::Address,
                sym::memptr => MirType::MemPtr,
                sym::storageptr => MirType::StoragePtr,
                sym::calldataptr => MirType::CalldataPtr,
                kw::Function => MirType::Function,
                sym::void => MirType::Void,
                _ => return Err(self.parser.error(format!("unknown type `{id}`"))),
            }
        };
        Ok(ty)
    }

    /// Parses a single value reference: `argN`, `vN`, integer literal, or `true`/`false`.
    /// Allocates a fresh `Immediate` for literals.
    fn parse_value(&mut self, func: &mut Function) -> PResult<'sess, ValueId> {
        // Integer literal? (decimal or 0x…)
        if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
            let v = self.parser.parse_uint()?;
            return Ok(func.alloc_value(Value::Immediate(Immediate::uint256(v))));
        }
        // Identifier-like — could be argN, vN, true, false.
        let ident = self.parser.parse_ident()?;
        if ident == kw::True {
            return Ok(func.alloc_value(Value::Immediate(Immediate::bool(true))));
        }
        if ident == kw::False {
            return Ok(func.alloc_value(Value::Immediate(Immediate::bool(false))));
        }
        if ident == sym::err {
            // Reconstructing an already-reported error state from text: there
            // is no live diagnostic to propagate here.
            let guar = solar_interface::diagnostics::ErrorGuaranteed::new_unchecked();
            return Ok(func.alloc_value(Value::Error(guar)));
        }
        if let Some(rest) = ident.as_str().strip_prefix("arg") {
            let idx: usize =
                rest.parse().map_err(|_| self.parser.error(format!("invalid arg `{ident}`")))?;
            // ABI wrappers reference `argN` with an empty parameter list:
            // those denote calldata head words. Allocate them on demand so
            // printed `abi`-phase modules round-trip. A function that does
            // declare parameters keeps strict bounds checking.
            if idx >= self.arg_values.len() && func.params.is_empty() {
                for index in self.arg_values.len()..=idx {
                    let val = func
                        .alloc_value(Value::Arg { index: index as u32, ty: MirType::uint256() });
                    self.arg_values.push(val);
                }
            }
            return self
                .arg_values
                .get(idx)
                .copied()
                .ok_or_else(|| self.parser.error(format!("arg{idx} out of range")));
        }
        if let Some(rest) = ident.as_str().strip_prefix('v') {
            let idx: u32 = rest
                .parse()
                .map_err(|_| self.parser.error(format!("invalid value reference `{ident}`")))?;
            if let Some(value) = self.value_labels.get(&idx).copied() {
                return Ok(value);
            }
            let value = func.alloc_value(Value::Undef(MirType::uint256()));
            self.value_labels.insert(idx, value);
            return Ok(value);
        }
        Err(self.parser.error(format!("expected value reference, got `{ident}`")))
    }

    fn resolve_result_label(
        &mut self,
        func: &mut Function,
        label: u32,
        inst_id: super::InstId,
    ) -> PResult<'sess, ()> {
        if let Some(value) = self.value_labels.get(&label).copied() {
            if matches!(func.values[value], Value::Undef(_)) {
                func.values[value] = Value::Inst(inst_id);
                return Ok(());
            }
            return Err(self.parser.error(format!("duplicate value `v{label}`")));
        }

        let value = func.alloc_value(Value::Inst(inst_id));
        self.value_labels.insert(label, value);
        Ok(())
    }

    fn reject_unresolved_value_labels(&self, func: &Function) -> PResult<'sess, ()> {
        let mut unresolved: Vec<_> = self
            .value_labels
            .iter()
            .filter_map(|(&label, &value)| {
                matches!(func.values[value], Value::Undef(_)).then_some(label)
            })
            .collect();
        unresolved.sort_unstable();
        if let Some(label) = unresolved.first() {
            return Err(self.parser.error(format!("undefined value `v{label}`")));
        }
        Ok(())
    }

    fn reject_unresolved_block_labels(&self) -> PResult<'sess, ()> {
        let mut unresolved = self
            .block_labels
            .iter()
            .filter_map(|(&index, label)| (!label.defined).then_some((index, label.reference_span)))
            .collect::<Vec<_>>();
        unresolved.sort_unstable_by_key(|&(index, _)| index);
        if let Some(&(index, span)) = unresolved.first() {
            let message = format!("unknown block `bb{index}`");
            return Err(match span {
                Some(span) => self.parser.error_at(span, message),
                None => self.parser.error(message),
            });
        }
        Ok(())
    }

    fn parse_block_id(&mut self, func: &mut Function) -> PResult<'sess, BlockId> {
        let span = self.parser.token().span;
        let id = self.parser.parse_ident()?;
        let rest = id
            .as_str()
            .strip_prefix("bb")
            .ok_or_else(|| self.parser.error(format!("expected `bbN`, got `{id}`")))?;
        let idx: u32 =
            rest.parse().map_err(|_| self.parser.error(format!("invalid block index `{id}`")))?;
        if let Some(label) = self.block_labels.get(&idx) {
            return Ok(label.id);
        }
        let block =
            if self.block_labels.is_empty() { func.entry_block } else { func.alloc_block() };
        self.block_labels
            .insert(idx, BlockLabel { id: block, defined: false, reference_span: Some(span) });
        Ok(block)
    }

    fn parse_function_id(&mut self) -> PResult<'sess, FunctionId> {
        if self.parser.eat(TokenKind::At) {
            let span = self.parser.token().span;
            let name = self.parse_function_name()?;
            self.pending_function_ref = Some((Symbol::intern(&name), span));
            return Ok(FunctionId::from_usize(0));
        }
        let id = self.parser.parse_ident()?;
        let rest = id
            .as_str()
            .strip_prefix("fn")
            .ok_or_else(|| self.parser.error(format!("expected `@name` or `fnN`, got `{id}`")))?;
        let idx: usize = rest
            .parse()
            .map_err(|_| self.parser.error(format!("invalid function index `{id}`")))?;
        Ok(FunctionId::from_usize(idx))
    }

    fn finish_function_ref(&mut self, target: FunctionRefTarget) {
        if let Some((name, span)) = self.pending_function_ref.take() {
            self.function_refs.push(PendingFunctionRef { name, span, target });
        }
    }

    /// Parses one instruction line (with optional `vN =` result) or a terminator.
    fn parse_instruction_or_terminator(
        &mut self,
        func: &mut Function,
        block: BlockId,
    ) -> PResult<'sess, ()> {
        // Optional result: `vN = ...`
        let result_label = if let TokenKind::Ident(label) = self.parser.token().kind
            && let Some(index) = label.as_str().strip_prefix('v').and_then(|s| s.parse().ok())
            && self.parser.look_ahead(1).kind == TokenKind::Eq
        {
            self.parser.bump();
            self.parser.bump();
            Some(index)
        } else {
            None
        };

        let mnemonic_span = self.parser.token().span;
        let mnemonic = self.parser.parse_ident()?;

        // Terminators (no result).
        match mnemonic {
            sym::jump => {
                let target = self.parse_block_id(func)?;
                self.set_terminator(func, block, Terminator::Jump(target));
                return Ok(());
            }
            sym::br => {
                let condition = self.parse_value(func)?;
                self.parser.expect(TokenKind::Comma)?;
                let then_block = self.parse_block_id(func)?;
                self.parser.expect(TokenKind::Comma)?;
                let else_block = self.parse_block_id(func)?;
                self.set_terminator(
                    func,
                    block,
                    Terminator::Branch { condition, then_block, else_block },
                );
                return Ok(());
            }
            kw::Switch => {
                let value = self.parse_value(func)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect_keyword(kw::Default)?;
                let default = self.parse_block_id(func)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
                let mut cases = Vec::new();
                if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                    loop {
                        let val = self.parse_value(func)?;
                        self.parser.expect(TokenKind::FatArrow)?;
                        let bid = self.parse_block_id(func)?;
                        cases.push((val, bid));
                        if self.parser.eat(TokenKind::Comma) {
                            continue;
                        }
                        self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
                        break;
                    }
                }
                self.set_terminator(func, block, Terminator::Switch { value, default, cases });
                return Ok(());
            }
            sym::ret => {
                let mut values: SmallVec<[ValueId; 2]> = SmallVec::new();
                if self.value_starts_here() {
                    loop {
                        values.push(self.parse_value(func)?);
                        if !self.parser.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                }
                self.set_terminator(func, block, Terminator::Return { values });
                return Ok(());
            }
            kw::Revert => {
                let offset = self.parse_value(func)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_value(func)?;
                self.set_terminator(func, block, Terminator::Revert { offset, size });
                return Ok(());
            }
            sym::returndata => {
                let offset = self.parse_value(func)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_value(func)?;
                self.set_terminator(func, block, Terminator::ReturnData { offset, size });
                return Ok(());
            }
            kw::Stop => {
                self.set_terminator(func, block, Terminator::Stop);
                return Ok(());
            }
            kw::Selfdestruct => {
                let recipient = self.parse_value(func)?;
                self.set_terminator(func, block, Terminator::SelfDestruct { recipient });
                return Ok(());
            }
            kw::Invalid => {
                self.set_terminator(func, block, Terminator::Invalid);
                return Ok(());
            }
            sym::tail_call => {
                let function = self.parse_function_id()?;
                let mut args = smallvec::SmallVec::new();
                while self.parser.eat(TokenKind::Comma) {
                    args.push(self.parse_value(func)?);
                }
                self.set_terminator(func, block, Terminator::TailCall { function, args });
                self.finish_function_ref(FunctionRefTarget::Terminator(block));
                return Ok(());
            }
            _ => {}
        }

        // Otherwise — instruction.
        let (kind, result_ty) = self.parse_inst_kind(mnemonic, mnemonic_span, func)?;

        let metadata = self.parse_metadata(func)?;
        let mut inst = Instruction::new(kind, result_ty);
        inst.metadata = metadata;
        let inst_id = func.alloc_inst(inst);
        func.blocks[block].instructions.push(inst_id);
        self.finish_function_ref(FunctionRefTarget::Instruction(inst_id));
        if let Some(label) = result_label {
            self.resolve_result_label(func, label, inst_id)?;
        }
        Ok(())
    }

    fn value_starts_here(&self) -> bool {
        match self.parser.token().kind {
            TokenKind::Literal(TokenLitKind::Integer, _) => true,
            TokenKind::Ident(symbol) if self.parser.look_ahead(1).kind != TokenKind::Eq => {
                symbol == kw::True
                    || symbol == kw::False
                    || symbol == sym::err
                    || symbol
                        .as_str()
                        .strip_prefix("arg")
                        .or_else(|| symbol.as_str().strip_prefix('v'))
                        .is_some_and(|index| {
                            !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit())
                        })
            }
            _ => false,
        }
    }

    fn parse_metadata(&mut self, func: &mut Function) -> PResult<'sess, InstructionMetadata> {
        let mut metadata = InstructionMetadata::EMPTY;
        if !self.parser.eat(TokenKind::Not) {
            return Ok(metadata);
        }
        self.parser.expect_keyword(sym::metadata)?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        if self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
            return Ok(metadata);
        }

        loop {
            let key = self.parser.parse_ident()?;
            match key {
                kw::Unchecked => {
                    metadata.set_unchecked(true);
                }
                kw::Storage => {
                    self.parser.expect(TokenKind::Eq)?;
                    metadata.set_storage_alias(Some(self.parse_storage_alias(func)?));
                }
                kw::Memory => {
                    self.parser.expect(TokenKind::Eq)?;
                    let value = self.parser.parse_ident()?;
                    metadata.set_memory_region(Some(self.parse_memory_region(value)?));
                }
                sym::effect => {
                    self.parser.expect(TokenKind::Eq)?;
                    let value = self.parser.parse_ident()?;
                    metadata.set_effect(Some(self.parse_effect_kind(value)?));
                }
                sym::loop_depth => {
                    self.parser.expect(TokenKind::Eq)?;
                    let value = self.parser.parse_uint()?;
                    metadata.loop_depth = self.u256_to_u16(value)?;
                }
                sym::hir => {
                    self.parser.expect(TokenKind::Eq)?;
                    let value = self.parser.parse_uint()?;
                    metadata.set_hir_expr(Some(hir::ExprId::from_usize(
                        self.u256_to_u32(value)? as usize
                    )));
                }
                sym::span => {
                    self.parser.expect(TokenKind::Eq)?;
                    let (lo, hi) = self.parse_span_bounds()?;
                    metadata.set_source_span(Some(Span::new(BytePos(lo), BytePos(hi))));
                }
                _ => return Err(self.parser.error(format!("unknown metadata key `{key}`"))),
            }

            if self.parser.eat(TokenKind::Comma) {
                continue;
            }
            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
            break;
        }

        Ok(metadata)
    }

    fn parse_span_bounds(&mut self) -> PResult<'sess, (u32, u32)> {
        if let TokenKind::Literal(TokenLitKind::Rational, symbol) = self.parser.token().kind
            && let Some(lo) = symbol.as_str().strip_suffix('.')
        {
            let lo = lo.parse().map_err(|_| self.parser.error("invalid span start"))?;
            self.parser.bump();
            let TokenKind::Literal(TokenLitKind::Rational, symbol) = self.parser.token().kind
            else {
                return Err(self.parser.error("expected span end"));
            };
            let Some(hi) = symbol.as_str().strip_prefix('.') else {
                return Err(self.parser.error("expected span end"));
            };
            let hi = hi.parse().map_err(|_| self.parser.error("invalid span end"))?;
            self.parser.bump();
            return Ok((lo, hi));
        }

        let lo = self.parser.parse_uint()?;
        let lo = self.u256_to_u32(lo)?;
        self.parser.expect(TokenKind::Dot)?;
        if let TokenKind::Literal(TokenLitKind::Rational, symbol) = self.parser.token().kind
            && let Some(hi) = symbol.as_str().strip_prefix('.')
        {
            let hi = hi.parse().map_err(|_| self.parser.error("invalid span end"))?;
            self.parser.bump();
            return Ok((lo, hi));
        }
        self.parser.expect(TokenKind::Dot)?;
        let hi = self.parser.parse_uint()?;
        Ok((lo, self.u256_to_u32(hi)?))
    }

    fn parse_storage_alias(&mut self, func: &mut Function) -> PResult<'sess, StorageAlias> {
        let kind = self.parser.parse_ident()?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let alias = match kind {
            sym::slot => StorageAlias::Slot(self.parser.parse_uint()?),
            sym::symbolic => StorageAlias::Symbolic(self.parse_value(func)?),
            sym::offset => {
                let base = self.parse_value(func)?;
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parser.parse_uint()?;
                StorageAlias::Offset { base, offset }
            }
            _ => return Err(self.parser.error(format!("unknown storage metadata value `{kind}`"))),
        };
        self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
        Ok(alias)
    }

    fn parse_memory_region(&self, value: Symbol) -> PResult<'sess, MemoryRegion> {
        Ok(match value {
            sym::scratch => MemoryRegion::Scratch,
            sym::abi_return => MemoryRegion::AbiReturn,
            sym::heap => MemoryRegion::Heap,
            sym::internal_frame => MemoryRegion::InternalFrame,
            sym::unknown => MemoryRegion::Unknown,
            _ => return Err(self.parser.error(format!("unknown memory metadata value `{value}`"))),
        })
    }

    fn parse_effect_kind(&self, value: Symbol) -> PResult<'sess, EffectKind> {
        Ok(match value {
            kw::Pure => EffectKind::Pure,
            sym::memory_read => EffectKind::MemoryRead,
            sym::memory_write => EffectKind::MemoryWrite,
            sym::storage_read => EffectKind::StorageRead,
            sym::storage_write => EffectKind::StorageWrite,
            sym::transient_read => EffectKind::TransientRead,
            sym::transient_write => EffectKind::TransientWrite,
            sym::environment_read => EffectKind::EnvironmentRead,
            sym::external_call => EffectKind::ExternalCall,
            sym::internal_call => EffectKind::InternalCall,
            kw::Create => EffectKind::Create,
            sym::log => EffectKind::Log,
            _ => return Err(self.parser.error(format!("unknown effect metadata value `{value}`"))),
        })
    }

    fn u256_to_u32(&self, value: U256) -> PResult<'sess, u32> {
        value
            .try_into()
            .map_err(|_| self.parser.error(format!("integer `{value}` does not fit in u32")))
    }

    fn u256_to_u16(&self, value: U256) -> PResult<'sess, u16> {
        value
            .try_into()
            .map_err(|_| self.parser.error(format!("integer `{value}` does not fit in u16")))
    }

    fn set_terminator(&mut self, func: &mut Function, block: BlockId, term: Terminator) {
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
        mnemonic: Symbol,
        mnemonic_span: Span,
        func: &mut Function,
    ) -> PResult<'sess, (InstKind, Option<MirType>)> {
        // Operand parsing helpers.
        macro_rules! v {
            () => {
                self.parse_value(func)?
            };
        }
        macro_rules! comma {
            () => {
                self.parser.expect(TokenKind::Comma)?
            };
        }

        Ok(match mnemonic {
            // ----- arithmetic (all uint256 result) -----
            kw::Add => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Add(a, b), Some(MirType::uint256()))
            }
            kw::Sub => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Sub(a, b), Some(MirType::uint256()))
            }
            kw::Mul => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Mul(a, b), Some(MirType::uint256()))
            }
            kw::Div => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Div(a, b), Some(MirType::uint256()))
            }
            kw::Sdiv => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SDiv(a, b), Some(MirType::int256()))
            }
            kw::Mod => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Mod(a, b), Some(MirType::uint256()))
            }
            kw::Smod => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SMod(a, b), Some(MirType::int256()))
            }
            kw::Exp => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Exp(a, b), Some(MirType::uint256()))
            }
            kw::Addmod => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::AddMod(a, b, c), Some(MirType::uint256()))
            }
            kw::Mulmod => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::MulMod(a, b, c), Some(MirType::uint256()))
            }

            // ----- bitwise -----
            kw::And => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::And(a, b), Some(MirType::uint256()))
            }
            kw::Or => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Or(a, b), Some(MirType::uint256()))
            }
            kw::Xor => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Xor(a, b), Some(MirType::uint256()))
            }
            kw::Not => {
                let a = v!();
                (InstKind::Not(a), Some(MirType::uint256()))
            }
            kw::Shl => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Shl(a, b), Some(MirType::uint256()))
            }
            kw::Shr => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Shr(a, b), Some(MirType::uint256()))
            }
            kw::Sar => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Sar(a, b), Some(MirType::int256()))
            }
            kw::Byte => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Byte(a, b), Some(MirType::uint256()))
            }
            kw::Signextend => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SignExtend(a, b), Some(MirType::int256()))
            }

            // ----- comparison -----
            kw::Lt => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Lt(a, b), Some(MirType::Bool))
            }
            kw::Gt => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Gt(a, b), Some(MirType::Bool))
            }
            kw::Slt => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SLt(a, b), Some(MirType::Bool))
            }
            kw::Sgt => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SGt(a, b), Some(MirType::Bool))
            }
            kw::Eq => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Eq(a, b), Some(MirType::Bool))
            }
            kw::Iszero => {
                let a = v!();
                (InstKind::IsZero(a), Some(MirType::Bool))
            }

            // ----- memory -----
            kw::Mload => {
                let a = v!();
                (InstKind::MLoad(a), Some(MirType::uint256()))
            }
            kw::Mstore => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::MStore(a, b), None)
            }
            kw::Mstore8 => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::MStore8(a, b), None)
            }
            kw::Msize => (InstKind::MSize, Some(MirType::uint256())),
            kw::Mcopy => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::MCopy(a, b, c), None)
            }

            // ----- storage -----
            kw::Sload => {
                let a = v!();
                (InstKind::SLoad(a), Some(MirType::uint256()))
            }
            kw::Sstore => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::SStore(a, b), None)
            }
            kw::Tload => {
                let a = v!();
                (InstKind::TLoad(a), Some(MirType::uint256()))
            }
            kw::Tstore => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::TStore(a, b), None)
            }

            // ----- calldata -----
            kw::Calldataload => {
                let a = v!();
                (InstKind::CalldataLoad(a), Some(MirType::uint256()))
            }
            kw::Calldatasize => (InstKind::CalldataSize, Some(MirType::uint256())),
            kw::Calldatacopy => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::CalldataCopy(a, b, c), None)
            }

            // ----- code -----
            kw::Codesize => (InstKind::CodeSize, Some(MirType::uint256())),
            kw::Codecopy => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::CodeCopy(a, b, c), None)
            }
            kw::Loadimmutable => {
                let offset = self.parser.parse_uint()?;
                let offset = self.u256_to_u32(offset)?;
                (InstKind::LoadImmutable(offset), Some(MirType::uint256()))
            }
            kw::Extcodesize => {
                let a = v!();
                (InstKind::ExtCodeSize(a), Some(MirType::uint256()))
            }
            kw::Extcodecopy => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                (InstKind::ExtCodeCopy(a, b, c, d), None)
            }
            kw::Extcodehash => {
                let a = v!();
                (InstKind::ExtCodeHash(a), Some(MirType::uint256()))
            }

            // ----- return data -----
            kw::Returndatasize => (InstKind::ReturnDataSize, Some(MirType::uint256())),
            kw::Returndatacopy => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::ReturnDataCopy(a, b, c), None)
            }

            // ----- environment (nullary) -----
            kw::Caller => (InstKind::Caller, Some(MirType::Address)),
            kw::Callvalue => (InstKind::CallValue, Some(MirType::uint256())),
            kw::Origin => (InstKind::Origin, Some(MirType::Address)),
            kw::Gasprice => (InstKind::GasPrice, Some(MirType::uint256())),
            kw::Coinbase => (InstKind::Coinbase, Some(MirType::Address)),
            kw::Timestamp => (InstKind::Timestamp, Some(MirType::uint256())),
            kw::Number => (InstKind::BlockNumber, Some(MirType::uint256())),
            kw::Prevrandao => (InstKind::PrevRandao, Some(MirType::uint256())),
            kw::Gaslimit => (InstKind::GasLimit, Some(MirType::uint256())),
            kw::Chainid => (InstKind::ChainId, Some(MirType::uint256())),
            kw::Address => (InstKind::Address, Some(MirType::Address)),
            kw::Selfbalance => (InstKind::SelfBalance, Some(MirType::uint256())),
            kw::Gas => (InstKind::Gas, Some(MirType::uint256())),
            kw::Basefee => (InstKind::BaseFee, Some(MirType::uint256())),
            kw::Blobbasefee => (InstKind::BlobBaseFee, Some(MirType::uint256())),

            // ----- environment (unary) -----
            kw::Blockhash => {
                let a = v!();
                (InstKind::BlockHash(a), Some(MirType::FixedBytes(32)))
            }
            kw::Balance => {
                let a = v!();
                (InstKind::Balance(a), Some(MirType::uint256()))
            }
            kw::Blobhash => {
                let a = v!();
                (InstKind::BlobHash(a), Some(MirType::FixedBytes(32)))
            }

            // ----- hashing -----
            kw::Keccak256 => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Keccak256(a, b), Some(MirType::bytes32()))
            }
            sym::mapping_slot => {
                let key = v!();
                comma!();
                let slot = v!();
                (InstKind::MappingSlot(key, slot), Some(MirType::bytes32()))
            }
            sym::mapping_slot_memory => {
                let key = v!();
                comma!();
                let slot = v!();
                (InstKind::MappingSlotMemory(key, slot), Some(MirType::bytes32()))
            }
            sym::mapping_slot_calldata => {
                let key = v!();
                comma!();
                let slot = v!();
                (InstKind::MappingSlotCalldata(key, slot), Some(MirType::bytes32()))
            }

            // ----- calls -----
            kw::Call => {
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
            kw::Staticcall => {
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
            kw::Delegatecall => {
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
            sym::internal_call => {
                let function = self.parse_function_id()?;
                comma!();
                let returns = self.parser.parse_uint()?.to::<u32>();
                let mut args = Vec::new();
                while self.parser.eat(TokenKind::Comma) {
                    args.push(v!());
                }
                let result_ty = (returns > 0).then(MirType::uint256);
                (InstKind::InternalCall { function, args: args.into(), returns }, result_ty)
            }
            sym::internal_frame_addr => {
                let offset = self.parser.parse_uint()?.to::<u64>();
                (InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
            }

            // ----- create -----
            kw::Create => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::Create(a, b, c), Some(MirType::Address))
            }
            kw::Create2 => {
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
            kw::Log0 => {
                let a = v!();
                comma!();
                let b = v!();
                (InstKind::Log0(a, b), None)
            }
            kw::Log1 => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                (InstKind::Log1(a, b, c), None)
            }
            kw::Log2 => {
                let a = v!();
                comma!();
                let b = v!();
                comma!();
                let c = v!();
                comma!();
                let d = v!();
                (InstKind::Log2(a, b, c, d), None)
            }
            kw::Log3 => {
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
            kw::Log4 => {
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
            sym::select => {
                let cond = v!();
                comma!();
                let then_v = v!();
                comma!();
                let else_v = v!();
                (InstKind::Select(cond, then_v, else_v), Some(MirType::uint256()))
            }
            sym::phi => {
                // Format: `phi [bb0: v1], [bb1: v2]` — matches the printer in display.rs.
                // Each pair is `[blockId: valueId]` separated by commas.
                let mut incoming: Vec<(BlockId, ValueId)> = Vec::new();
                loop {
                    self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
                    let bid = self.parse_block_id(func)?;
                    self.parser.expect(TokenKind::Colon)?;
                    let val = v!();
                    self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
                    incoming.push((bid, val));
                    if !self.parser.eat(TokenKind::Comma) {
                        break;
                    }
                }
                (InstKind::Phi(incoming), Some(MirType::uint256()))
            }

            other => {
                return Err(self
                    .parser
                    .error_at(mnemonic_span, format!("unknown instruction `{other}`")));
            }
        })
    }
}

// =============================================================================
// Tests
// =============================================================================

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{assert_data_eq, str};
    use solar_interface::{ColorChoice, Session, source_map::FileName};

    fn with_session<F: FnOnce(&Session) + Send>(f: F) {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.set_flags(|flags| flags.track_diagnostics = false);
        sess.enter(|| f(&sess));
    }

    #[test]
    fn parse_module_phase_header() {
        with_session(|sess| {
            let src = "/// module docs\n@module Phased\n@phase optimized\nfn @f() {\n  bb0 (entry):\n    stop\n}\n";
            let module = parse_module(sess, src).unwrap();
            assert_eq!(module.phase, crate::mir::MirPhase::Optimized);
            // Round-trips through the printer.
            let printed = module.to_text().to_string();
            assert_data_eq!(
                &printed,
                str![[r#"
@module Phased
@phase optimized
fn @f() {
  bb0 (entry):
    stop
}

"#]]
            );
            let reparsed = parse_module(sess, &printed).unwrap();
            assert_eq!(reparsed.phase, crate::mir::MirPhase::Optimized);

            // The default phase is not printed, and parses back as built.
            let src = "@module Fresh\nfn @f() {\n  bb0 (entry):\n    stop\n}\n";
            let module = parse_module(sess, src).unwrap();
            assert_eq!(module.phase, crate::mir::MirPhase::Built);
            assert_data_eq!(
                module.to_text().to_string(),
                str![[r#"
@module Fresh
fn @f() {
  bb0 (entry):
    stop
}

"#]]
            );

            // Every phase name round-trips through parse and print.
            for phase in [
                crate::mir::MirPhase::Built,
                crate::mir::MirPhase::Optimized,
                crate::mir::MirPhase::Abi,
                crate::mir::MirPhase::Dispatch,
                crate::mir::MirPhase::EvmShaped,
            ] {
                let src = format!(
                    "@module P\n@phase {}\nfn @f() {{\n  bb0 (entry):\n    stop\n}}\n",
                    phase.name()
                );
                let module = parse_module(sess, &src).unwrap();
                assert_eq!(module.phase, phase, "parse `{}`", phase.name());
                let reparsed = parse_module(sess, &module.to_text().to_string()).unwrap();
                assert_eq!(reparsed.phase, phase, "round-trip `{}`", phase.name());
            }

            // Unknown phase names are rejected.
            let src = "@module Bogus\n@phase shiny\nfn @f() {\n  bb0 (entry):\n    stop\n}\n";
            assert!(parse_module(sess, src).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: unknown MIR phase `shiny`
  ╭▸ <mir-test-15579854765591958512>:2:8
  │
2 │ @phase shiny
  ╰╴       ━━━━━


"#]]
            );
        });
    }

    #[test]
    fn parser_does_not_treat_newlines_as_syntax() {
        with_session(|sess| {
            let src = "@module m fn @f() -> u256 { bb0 (entry): v0 = add 1, 2 ret v0 }";
            let module = parse_module(sess, src).unwrap();
            assert_data_eq!(
                module.to_text().to_string(),
                str![[r#"
@module m
fn @f() -> u256 {
  bb0 (entry):
    v0 = add 1, 2
    ret v0
}

"#]]
            );
        });
    }

    #[test]
    fn parse_linear_function() {
        with_session(|sess| {
            let src = "\
fn @add(arg0: u256, arg1: u256) -> u256 {
  bb0 (entry):
    v2 = add arg0, arg1
    ret v2
}
";
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.blocks.len(), 1);
            assert_eq!(func.params.len(), 2);
            assert_eq!(func.returns.len(), 1);
            // Round-trip: print and re-parse should not error.
            let printed = func.to_text().to_string();
            let _func2 = parse_function(sess, &printed).unwrap();
        });
    }

    #[test]
    fn parse_storage_ops() {
        with_session(|sess| {
            let src = "\
fn @increment() {
  bb0 (entry):
    v0 = sload 0
    v1 = add v0, 1
    sstore 0, v1
    stop
}
";
            let func = parse_function(sess, src).unwrap();
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
        with_session(|sess| {
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
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.blocks.len(), 3);
            // bb0 should have 2 successors.
            assert_eq!(func.blocks[func.entry_block].terminator().unwrap().successors().len(), 2);
        });
    }

    #[test]
    fn parse_loop_with_jump() {
        with_session(|sess| {
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
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.blocks.len(), 4);
        });
    }

    #[test]
    fn parse_call_instruction() {
        with_session(|sess| {
            let src = "\
fn @do_call(arg0: address, arg1: u256) -> u256 {
  bb0 (entry):
    v2 = call 100, arg0, arg1, 0, 0, 0, 0
    ret v2
}
";
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.instructions.len(), 1);
        });
    }

    #[test]
    fn parse_keccak_and_mload_mstore() {
        with_session(|sess| {
            let src = "\
fn @hash() -> u256 {
  bb0 (entry):
    mstore 0, 1
    mstore 32, 2
    v1 = keccak256 0, 64
    ret v1
}
";
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.instructions.len(), 3);
        });
    }

    #[test]
    fn parse_round_trip_module() {
        with_session(|sess| {
            let src = "\
@module Counter
fn @count() {
  bb0 (entry):
    tail_call @set, 1
}

fn @set(arg0: u256) {
  bb0 (entry):
    sstore 0, arg0
    stop
}
";
            let module = parse_module(sess, src).unwrap();
            assert_eq!(module.functions.len(), 2);
            let Some(Terminator::TailCall { function, .. }) =
                &module.functions[FunctionId::from_usize(0)].blocks[BlockId::from_usize(0)]
                    .terminator
            else {
                panic!("expected tail call")
            };
            assert_eq!(*function, FunctionId::from_usize(1));
            // Round-trip the printed form.
            let printed = module.to_text().to_string();
            let module2 = parse_module(sess, &printed).unwrap();
            assert_eq!(module2.functions.len(), 2);
        });
    }

    #[test]
    fn parse_unknown_instruction_errors() {
        with_session(|sess| {
            let src = "\
fn @bad() {
  bb0 (entry):
    v1 = bogus arg0
    stop
}
";
            assert!(parse_function(sess, src).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: unknown instruction `bogus`
  ╭▸ <mir-function-test-13016118735129543399>:3:10
  │
3 │     v1 = bogus arg0
  ╰╴         ━━━━━


"#]]
            );
        });
    }

    #[test]
    fn malformed_tentative_parses_emit_diagnostics() {
        with_session(|sess| {
            let input = "@module m\nfn @ {\n";
            let source = sess
                .source_map()
                .new_source_file(FileName::Custom("malformed-function.mir".into()), input)
                .unwrap();
            assert!(Module::parse(sess, &source).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: expected identifier
  ╭▸ <malformed-function.mir>:2:6
  │
2 │ fn @ {
  ╰╴     ━


"#]]
            );
        });

        with_session(|sess| {
            let input = "@module m\nfn @f() {\n  bb0 (entry):\n    %bad\n}\n";
            let source = sess
                .source_map()
                .new_source_file(FileName::Custom("malformed-result.mir".into()), input)
                .unwrap();
            assert!(Module::parse(sess, &source).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: expected identifier
  ╭▸ <malformed-result.mir>:4:5
  │
4 │     %bad
  ╰╴    ━


"#]]
            );
        });

        with_session(|sess| {
            let input = "@module m\nfn @f() {\n  bb0 (entry):\n    jump bb9\n}\n";
            let source = sess
                .source_map()
                .new_source_file(FileName::Custom("unknown-block.mir".into()), input)
                .unwrap();
            assert!(Module::parse(sess, &source).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: unknown block `bb9`
  ╭▸ <unknown-block.mir>:4:10
  │
4 │     jump bb9
  ╰╴         ━━━


"#]]
            );
        });
    }

    #[test]
    fn error_includes_source_snippet() {
        with_session(|sess| {
            let src = "\
fn @bad() {
  bb0 (entry):
    v1 = bogus arg0
    stop
}
";
            assert!(parse_function(sess, src).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: unknown instruction `bogus`
  ╭▸ <mir-function-test-13016118735129543399>:3:10
  │
3 │     v1 = bogus arg0
  ╰╴         ━━━━━


"#]]
            );
        });
    }

    #[test]
    fn error_snippet_format_is_clang_like() {
        // Verify the precise format users will see, end-to-end.
        with_session(|sess| {
            let src = "fn @x() -> notatype {\n  bb0 (entry):\n    stop\n}\n";
            assert!(parse_function(sess, src).is_err());
            assert_data_eq!(
                sess.emitted_diagnostics().unwrap().to_string(),
                str![[r#"
error: unknown type `notatype`
  ╭▸ <mir-function-test-2067061233293511805>:1:21
  │
1 │ fn @x() -> notatype {
  ╰╴                    ━


"#]]
            );
        });
    }

    #[test]
    fn parse_compact_and_spaced_spans() {
        with_session(|sess| {
            for span in ["1..5", "1 .. 5"] {
                let src = format!(
                    "@module m\nfn @f() {{\n  bb0 (entry):\n    v0 = sload 0 !metadata(span={span})\n    stop\n}}\n"
                );
                let module = parse_module(sess, &src).unwrap();
                assert_data_eq!(
                    module.to_text().to_string(),
                    str![[r#"
@module m
fn @f() {
  bb0 (entry):
    v0 = sload 0 !metadata(span=1..5)
    stop
}

"#]]
                );
            }
        });
    }

    #[test]
    fn parse_revert_terminator() {
        with_session(|sess| {
            let src = "\
fn @oops() {
  bb0 (entry):
    revert 0, 0
}
";
            let func = parse_function(sess, src).unwrap();
            assert!(matches!(
                func.blocks[func.entry_block].terminator,
                Some(Terminator::Revert { .. })
            ));
        });
    }

    #[test]
    fn parse_environment_nullary() {
        with_session(|sess| {
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
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.instructions.len(), 4);
        });
    }

    #[test]
    fn parse_select_and_logs() {
        with_session(|sess| {
            let src = "\
fn @sel(arg0: bool, arg1: u256, arg2: u256) -> u256 {
  bb0 (entry):
    v3 = select arg0, arg1, arg2
    log1 0, 32, v3
    ret v3
}
";
            let func = parse_function(sess, src).unwrap();
            assert_eq!(func.instructions.len(), 2);
        });
    }

    #[test]
    fn parse_phi_node() {
        with_session(|sess| {
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
            let func = parse_function(sess, src).unwrap();
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
        with_session(|sess| {
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
            let func = parse_function(sess, src).unwrap();
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
        with_session(|sess| {
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
            let func = parse_function(sess, src).unwrap();
            let printed = func.to_text().to_string();
            assert_data_eq!(
                &printed,
                str![[r#"
fn @diamond(arg0: bool) -> u256 {
  bb0 (entry):
    br arg0, bb1, bb2
  bb1:
    jump bb3
  bb2:
    jump bb3
  bb3:
    v0 = phi [bb1: 10], [bb2: 20]
    ret v0
}

"#]]
            );
            // Round-trip: re-parse the printer output, must succeed.
            let _func2 = parse_function(sess, &printed).unwrap();
        });
    }
}

//! Parser for the textual MIR format produced by [`Function`] and [`Module::to_text`].
//!
//! # Format
//!
//! ```text
//! @module Counter
//! fn @increment() {
//!   bb0:
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
    AbiLayout, AbiLayoutRef, AbiType, AllocationAlignment, AllocationFailure,
    AllocationInitialization, AllocationKind, AllocationSemantics, BlockId, EffectKind, Function,
    FunctionBuilder, FunctionId, InstId, InstKind, Instruction, InstructionMetadata,
    MemoryObjectKind, MemoryObjectLayout, MemoryRegion, Module, StorageAlias, StorageField,
    StorageLayout, StorageLayoutRef, Terminator, Value, ValueId,
};
use crate::mir::{MirType, SliceLocation};
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
    let name = format!("test{}.mir", sess.source_map().files().len());
    let file = sess
        .source_map()
        .new_source_file(solar_interface::source_map::FileName::Custom(name), input)
        .unwrap();
    Module::parse(sess, &file)
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
    /// ABI layouts interned while parsing instructions.
    abi_layouts: Vec<AbiLayoutRef>,
    /// Aggregate storage layouts interned while parsing instructions.
    storage_layouts: Vec<StorageLayoutRef>,
    /// Number of `>` closers still owed after splitting a `>>`/`>>>` token.
    pending_gt: u32,
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
            abi_layouts: Vec::new(),
            storage_layouts: Vec::new(),
            pending_gt: 0,
        }
    }

    /// Parses a phase name such as `evm-shaped`. Unlike an identifier, a phase
    /// name may contain internal hyphens.
    fn parse_phase_name(&mut self) -> PResult<'sess, Symbol> {
        let first = self.parser.parse_ident()?;
        if !self.parser.eat(TokenKind::BinOp(BinOpToken::Minus)) {
            return Ok(first);
        }
        let mut name = first.to_string();
        name.push('-');
        name.push_str(self.parser.parse_ident()?.as_str());
        while self.parser.eat(TokenKind::BinOp(BinOpToken::Minus)) {
            name.push('-');
            name.push_str(self.parser.parse_ident()?.as_str());
        }
        Ok(Symbol::intern(&name))
    }

    /// Parses a function name: an identifier, optionally with `.`-joined
    /// segments (`f.body`), as minted by the ABI lowering.
    fn parse_function_name(&mut self) -> PResult<'sess, Symbol> {
        let first = self.parser.parse_ident()?;
        if !self.parser.eat(TokenKind::Dot) {
            return Ok(first);
        }
        let mut name = first.to_string();
        name.push('.');
        name.push_str(self.parser.parse_ident()?.as_str());
        while self.parser.eat(TokenKind::Dot) {
            name.push('.');
            name.push_str(self.parser.parse_ident()?.as_str());
        }
        Ok(Symbol::intern(&name))
    }

    // ----- module / function parsing -----

    fn parse_module(&mut self) -> PResult<'sess, Module> {
        let mut phase = super::MirPhase::default();
        self.parser.expect(TokenKind::At)?;
        self.parser.expect_keyword(sym::module)?;
        let module_name = self.parser.parse_ident()?;
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

        let module_ident = Ident::with_dummy_span(module_name);
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

        module.abi_layouts = std::mem::take(&mut self.abi_layouts);
        module.aggregate_layouts = std::mem::take(&mut self.storage_layouts);

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
                        &mut module.functions[owner].inst_mut(inst).kind
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
        let func_ident = Ident::with_dummy_span(name);
        let mut func = Function::new(func_ident);
        let block_remap = {
            let mut builder = FunctionBuilder::new(&mut func);

            // Parse parameters: `(arg0: ty, arg1: ty, ...)` or `()`
            self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
            if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Parenthesis)) {
                loop {
                    let arg_name = self.parser.parse_ident()?;
                    let arg_name_str = arg_name.as_str();
                    if !arg_name_str.starts_with("arg") {
                        return Err(self
                            .parser
                            .error(format!("expected `argN`, got `{arg_name}`")));
                    }
                    let parsed_index = arg_name_str[3..].parse::<u32>().map_err(|_| {
                        self.parser.error(format!("invalid arg index in `{arg_name}`"))
                    })?;
                    let index = builder.func().params.len() as u32;
                    if parsed_index != index {
                        return Err(self
                            .parser
                            .error(format!("expected `arg{index}`, got `{arg_name}`")));
                    }
                    self.parser.expect(TokenKind::Colon)?;
                    let ty = self.parse_type()?;
                    self.arg_values.push(builder.add_param(ty));
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
                            builder.add_return(ty);
                            if self.parser.eat(TokenKind::Comma) {
                                continue;
                            }
                            self.parser.expect(TokenKind::CloseDelim(Delimiter::Parenthesis))?;
                            break;
                        }
                    }
                } else {
                    let ty = self.parse_type()?;
                    builder.add_return(ty);
                }
            }

            self.parse_function_attributes(&mut builder)?;
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
                    let bid = self.define_block(&mut builder, idx)?;
                    builder.switch_to_block(bid);
                    current_block = Some(bid);
                    continue;
                }

                // Not a block header — must be an instruction or terminator.
                current_block
                    .ok_or_else(|| self.parser.error("instruction outside of any block"))?;
                self.parse_instruction_or_terminator(&mut builder)?;
            }

            if self.block_order.is_empty() {
                return Err(self.parser.error("function must contain at least one block"));
            }
            self.reject_unresolved_block_labels()?;
            self.reject_unresolved_value_labels(builder.func())?;
            crate::mir::utils::remap_block_order(builder.func_mut(), &self.block_order)
        };
        for reference in &mut self.function_refs {
            if let FunctionRefTarget::Terminator(block) = &mut reference.target {
                *block = block_remap[*block];
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
        if !matches!(self.parser.look_ahead(1).kind, TokenKind::Colon) {
            return Ok(None);
        }
        self.parser.bump();
        self.parser.expect(TokenKind::Colon)?;
        Ok(Some(index))
    }

    fn define_block(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        index: u32,
    ) -> PResult<'sess, BlockId> {
        if let Some(label) = self.block_labels.get_mut(&index) {
            if label.defined {
                return Err(self.parser.error(format!("duplicate block `bb{index}`")));
            }
            label.defined = true;
            self.block_order.push(label.id);
            return Ok(label.id);
        }
        let id = if self.block_labels.is_empty() { BlockId::ENTRY } else { builder.create_block() };
        self.block_labels.insert(index, BlockLabel { id, defined: true, reference_span: None });
        self.block_order.push(id);
        Ok(id)
    }

    fn parse_function_attributes(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
    ) -> PResult<'sess, ()> {
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
                    builder.func_mut().selector = Some(selector.to_be_bytes());
                }
                kw::Receive => builder.func_mut().attributes.is_receive = true,
                kw::Fallback => builder.func_mut().attributes.is_fallback = true,
                kw::Payable => {
                    builder.func_mut().attributes.state_mutability = hir::StateMutability::Payable;
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
                sym::memorybytes => MirType::MemoryObject(MemoryObjectKind::Bytes),
                sym::memoryarray => MirType::MemoryObject(MemoryObjectKind::DynamicArray),
                sym::memoryfixedarray => MirType::MemoryObject(MemoryObjectKind::FixedArray),
                sym::memorystruct => MirType::MemoryObject(MemoryObjectKind::Struct),
                sym::storageptr => MirType::StoragePtr,
                sym::calldataptr => MirType::CalldataPtr,
                sym::memoryslice => MirType::Slice(SliceLocation::Memory),
                sym::calldataslice => MirType::Slice(SliceLocation::Calldata),
                sym::returndataslice => MirType::Slice(SliceLocation::Returndata),
                kw::Function => MirType::Function,
                sym::void => MirType::Void,
                _ => return Err(self.parser.error(format!("unknown type `{id}`"))),
            }
        };
        Ok(ty)
    }

    /// Parses a single value reference: `argN`, `vN`, integer literal, or `true`/`false`.
    /// Allocates a fresh `Immediate` for literals.
    fn parse_value(&mut self, builder: &mut FunctionBuilder<'_>) -> PResult<'sess, ValueId> {
        // Integer literal? (decimal or 0x…)
        if matches!(self.parser.token().kind, TokenKind::Literal(..)) {
            let v = self.parser.parse_uint()?;
            return Ok(builder.imm_u256(v));
        }
        // Identifier-like — could be argN, vN, true, false.
        let ident = self.parser.parse_ident()?;
        if ident == kw::True {
            return Ok(builder.imm_bool(true));
        }
        if ident == kw::False {
            return Ok(builder.imm_bool(false));
        }
        if ident == sym::err {
            // Reconstructing an already-reported error state from text: there
            // is no live diagnostic to propagate here.
            let guar = solar_interface::diagnostics::ErrorGuaranteed::new_unchecked();
            return Ok(builder.error_value(guar));
        }
        if let Some(rest) = ident.as_str().strip_prefix("arg") {
            let idx: usize =
                rest.parse().map_err(|_| self.parser.error(format!("invalid arg `{ident}`")))?;
            // ABI wrappers reference `argN` with an empty parameter list:
            // those denote calldata head words. Allocate them on demand so
            // printed `abi`-phase modules round-trip. A function that does
            // declare parameters keeps strict bounds checking.
            if idx >= self.arg_values.len() && builder.func().params.is_empty() {
                for index in self.arg_values.len()..=idx {
                    let val = builder
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
            let value = builder.undef(MirType::uint256());
            self.value_labels.insert(idx, value);
            return Ok(value);
        }
        Err(self.parser.error(format!("expected value reference, got `{ident}`")))
    }

    fn resolve_result_label(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
        label: u32,
        inst_id: InstId,
    ) -> PResult<'sess, ()> {
        if let Some(value) = self.value_labels.get(&label).copied() {
            if matches!(builder.func().values[value], Value::Undef(_)) {
                builder.set_value(value, Value::Inst(inst_id));
                return Ok(());
            }
            return Err(self.parser.error(format!("duplicate value `v{label}`")));
        }

        let value = builder.alloc_value(Value::Inst(inst_id));
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

    fn parse_block_id(&mut self, builder: &mut FunctionBuilder<'_>) -> PResult<'sess, BlockId> {
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
            if self.block_labels.is_empty() { BlockId::ENTRY } else { builder.create_block() };
        self.block_labels
            .insert(idx, BlockLabel { id: block, defined: false, reference_span: Some(span) });
        Ok(block)
    }

    /// Consumes one `>` closer, splitting `>>`/`>>>` shift tokens so nested
    /// `<...>` layout arguments close correctly.
    fn eat_gt(&mut self) -> bool {
        if self.pending_gt > 0 {
            self.pending_gt -= 1;
            return true;
        }
        if self.parser.eat(TokenKind::Gt) {
            return true;
        }
        if self.parser.eat(TokenKind::BinOp(BinOpToken::Shr)) {
            self.pending_gt += 1;
            return true;
        }
        if self.parser.eat(TokenKind::BinOp(BinOpToken::Sar)) {
            self.pending_gt += 2;
            return true;
        }
        false
    }

    fn expect_gt(&mut self) -> PResult<'sess, ()> {
        if self.eat_gt() {
            return Ok(());
        }
        self.parser.expect(TokenKind::Gt).map(drop)
    }

    /// Parses an ABI layout: `[type, type, ...]`. Structurally identical
    /// layouts are interned so repeated encodes share one allocation.
    fn parse_abi_layout(&mut self) -> PResult<'sess, AbiLayoutRef> {
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
        let mut types = Vec::new();
        if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
            loop {
                types.push(self.parse_abi_type()?);
                if self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                    break;
                }
                self.parser.expect(TokenKind::Comma)?;
            }
        }
        let layout = AbiLayout::new(types);
        if let Some(existing) = self.abi_layouts.iter().find(|item| item.as_ref() == &layout) {
            return Ok(std::sync::Arc::clone(existing));
        }
        let layout = std::sync::Arc::new(layout);
        self.abi_layouts.push(std::sync::Arc::clone(&layout));
        Ok(layout)
    }

    fn parse_abi_type(&mut self) -> PResult<'sess, AbiType> {
        let name = self.parser.parse_ident()?;
        Ok(match name {
            sym::word => AbiType::Word,
            sym::memory_bytes => AbiType::Bytes(SliceLocation::Memory),
            sym::calldata_bytes => AbiType::Bytes(SliceLocation::Calldata),
            sym::memory_array | sym::calldata_array => {
                self.parser.expect(TokenKind::Lt)?;
                let element = Box::new(self.parse_abi_type()?);
                self.expect_gt()?;
                let location = if name == sym::memory_array {
                    SliceLocation::Memory
                } else {
                    SliceLocation::Calldata
                };
                AbiType::DynamicArray { element, location }
            }
            sym::array => {
                self.parser.expect(TokenKind::Lt)?;
                let len = self.parser.parse_uint()?;
                let len = len
                    .try_into()
                    .map_err(|_| self.parser.error("ABI fixed-array length does not fit in u64"))?;
                self.parser.expect(TokenKind::Comma)?;
                let element = Box::new(self.parse_abi_type()?);
                self.expect_gt()?;
                AbiType::FixedArray { element, len }
            }
            sym::tuple => {
                self.parser.expect(TokenKind::Lt)?;
                let mut fields = Vec::new();
                if !self.eat_gt() {
                    loop {
                        fields.push(self.parse_abi_type()?);
                        if self.eat_gt() {
                            break;
                        }
                        self.parser.expect(TokenKind::Comma)?;
                    }
                }
                AbiType::Tuple(fields.into())
            }
            _ => return Err(self.parser.error(format!("unknown ABI type `{name}`"))),
        })
    }

    /// Parses a storage layout: `struct<field, ...>` or `array<len, field>`.
    /// Structurally identical layouts are interned.
    fn parse_storage_layout(&mut self) -> PResult<'sess, StorageLayoutRef> {
        let name = self.parser.parse_ident()?;
        let layout = match name {
            kw::Struct => {
                self.parser.expect(TokenKind::Lt)?;
                let mut fields = Vec::new();
                if !self.eat_gt() {
                    loop {
                        fields.push(self.parse_storage_field()?);
                        if self.eat_gt() {
                            break;
                        }
                        self.parser.expect(TokenKind::Comma)?;
                    }
                }
                StorageLayout::Struct(fields.into())
            }
            sym::array => {
                self.parser.expect(TokenKind::Lt)?;
                let len = self.parser.parse_uint()?;
                let len = len
                    .try_into()
                    .map_err(|_| self.parser.error("storage array length does not fit in u64"))?;
                self.parser.expect(TokenKind::Comma)?;
                let element = self.parse_storage_field()?;
                self.expect_gt()?;
                StorageLayout::Array { element, len }
            }
            _ => return Err(self.parser.error(format!("unknown storage layout `{name}`"))),
        };
        if let Some(existing) = self.storage_layouts.iter().find(|item| item.as_ref() == &layout) {
            return Ok(std::sync::Arc::clone(existing));
        }
        let layout = std::sync::Arc::new(layout);
        self.storage_layouts.push(std::sync::Arc::clone(&layout));
        Ok(layout)
    }

    fn parse_storage_field(&mut self) -> PResult<'sess, StorageField> {
        if self.parser.eat_keyword(sym::word) {
            return Ok(StorageField::Word);
        }
        Ok(StorageField::Aggregate(self.parse_storage_layout()?))
    }

    /// Parses a memory-object layout whose kind identifier `name` has already
    /// been consumed, with optional `<...>` layout arguments.
    fn parse_memory_object_layout(&mut self, name: Symbol) -> PResult<'sess, MemoryObjectLayout> {
        let layout = match name {
            sym::memorybytes => MemoryObjectLayout::Bytes,
            sym::memoryarray => {
                let element_words = if self.parser.eat(TokenKind::Lt) {
                    let value = self.parser.parse_uint()?;
                    let value = value
                        .try_into()
                        .map_err(|_| self.parser.error("memory-array stride does not fit"))?;
                    self.expect_gt()?;
                    value
                } else {
                    1
                };
                MemoryObjectLayout::DynamicArray { element_words }
            }
            sym::memoryfixedarray => {
                let (len, element_words) = if self.parser.eat(TokenKind::Lt) {
                    let len = self.parser.parse_uint()?;
                    let len = len
                        .try_into()
                        .map_err(|_| self.parser.error("memory fixed-array length does not fit"))?;
                    self.parser.expect(TokenKind::Comma)?;
                    let element_words = self.parser.parse_uint()?;
                    let element_words = element_words
                        .try_into()
                        .map_err(|_| self.parser.error("memory fixed-array stride does not fit"))?;
                    self.expect_gt()?;
                    (len, element_words)
                } else {
                    (0, 1)
                };
                MemoryObjectLayout::FixedArray { len, element_words }
            }
            sym::memorystruct => {
                let fields = if self.parser.eat(TokenKind::Lt) {
                    let value = self.parser.parse_uint()?;
                    let value = value
                        .try_into()
                        .map_err(|_| self.parser.error("memory struct field count does not fit"))?;
                    self.expect_gt()?;
                    value
                } else {
                    0
                };
                MemoryObjectLayout::Struct { fields }
            }
            other => {
                return Err(self.parser.error(format!("unknown memory-object layout `{other}`")));
            }
        };
        Ok(layout)
    }

    fn parse_function_id(&mut self) -> PResult<'sess, FunctionId> {
        if self.parser.eat(TokenKind::At) {
            let span = self.parser.token().span;
            let name = self.parse_function_name()?;
            self.pending_function_ref = Some((name, span));
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
        builder: &mut FunctionBuilder<'_>,
    ) -> PResult<'sess, ()> {
        let block = builder.current_block();
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
                let target = self.parse_block_id(builder)?;
                builder.set_terminator(Terminator::Jump(target));
                return Ok(());
            }
            sym::jumpi => {
                let condition = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let then_block = self.parse_block_id(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let else_block = self.parse_block_id(builder)?;
                builder.set_terminator(Terminator::Branch { condition, then_block, else_block });
                return Ok(());
            }
            kw::Switch => {
                let value = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect_keyword(kw::Default)?;
                let default = self.parse_block_id(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
                let mut cases = Vec::new();
                if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                    loop {
                        let val = self.parse_value(builder)?;
                        self.parser.expect(TokenKind::FatArrow)?;
                        let bid = self.parse_block_id(builder)?;
                        cases.push((val, bid));
                        if self.parser.eat(TokenKind::Comma) {
                            continue;
                        }
                        self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
                        break;
                    }
                }
                builder.set_terminator(Terminator::Switch { value, default, cases });
                return Ok(());
            }
            sym::ret => {
                let mut values: SmallVec<[ValueId; 2]> = SmallVec::new();
                if self.value_starts_here() {
                    loop {
                        values.push(self.parse_value(builder)?);
                        if !self.parser.eat(TokenKind::Comma) {
                            break;
                        }
                    }
                }
                builder.set_terminator(Terminator::Return { values });
                return Ok(());
            }
            kw::Revert => {
                let offset = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_value(builder)?;
                builder.set_terminator(Terminator::Revert { offset, size });
                return Ok(());
            }
            sym::returndata => {
                let offset = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_value(builder)?;
                builder.set_terminator(Terminator::ReturnData { offset, size });
                return Ok(());
            }
            kw::Stop => {
                builder.set_terminator(Terminator::Stop);
                return Ok(());
            }
            kw::Selfdestruct => {
                let recipient = self.parse_value(builder)?;
                builder.set_terminator(Terminator::SelfDestruct { recipient });
                return Ok(());
            }
            kw::Invalid => {
                builder.set_terminator(Terminator::Invalid);
                return Ok(());
            }
            sym::tail_call => {
                let function = self.parse_function_id()?;
                let mut args = smallvec::SmallVec::new();
                while self.parser.eat(TokenKind::Comma) {
                    args.push(self.parse_value(builder)?);
                }
                builder.set_terminator(Terminator::TailCall { function, args });
                self.finish_function_ref(FunctionRefTarget::Terminator(block));
                return Ok(());
            }
            _ => {}
        }

        // Otherwise — instruction.
        let (kind, result_ty) = self.parse_inst_kind(mnemonic, mnemonic_span, builder)?;

        let metadata = self.parse_metadata(builder)?;
        let mut inst = Instruction::new(kind, result_ty);
        inst.metadata = metadata;
        let inst_id = builder.append_instruction(inst);
        self.finish_function_ref(FunctionRefTarget::Instruction(inst_id));
        if let Some(label) = result_label {
            self.resolve_result_label(builder, label, inst_id)?;
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

    fn parse_metadata(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
    ) -> PResult<'sess, InstructionMetadata> {
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
                sym::deferred_alloc => {
                    metadata.set_deferred_alloc();
                }
                kw::Storage => {
                    self.parser.expect(TokenKind::Eq)?;
                    metadata.set_storage_alias(Some(self.parse_storage_alias(builder)?));
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

    fn parse_storage_alias(
        &mut self,
        builder: &mut FunctionBuilder<'_>,
    ) -> PResult<'sess, StorageAlias> {
        let kind = self.parser.parse_ident()?;
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Parenthesis))?;
        let alias = match kind {
            sym::slot => StorageAlias::Slot(self.parser.parse_uint()?),
            sym::symbolic => StorageAlias::Symbolic(self.parse_value(builder)?),
            sym::offset => {
                let base = self.parse_value(builder)?;
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

    /// Parses one instruction by mnemonic.
    fn parse_inst_kind(
        &mut self,
        mnemonic: Symbol,
        mnemonic_span: Span,
        builder: &mut FunctionBuilder<'_>,
    ) -> PResult<'sess, (InstKind, Option<MirType>)> {
        macro_rules! operands {
            () => {};
            ($first:ident $(, $rest:ident)*) => {
                let $first = self.parse_value(builder)?;
                $(
                    self.parser.expect(TokenKind::Comma)?;
                    let $rest = self.parse_value(builder)?;
                )*
            };
        }
        macro_rules! inst {
            ($kind:ident($($operand:ident),*) => $ty:expr) => {{
                operands!($($operand),*);
                (InstKind::$kind($($operand),*), Some($ty))
            }};
            ($kind:ident($($operand:ident),*)) => {{
                operands!($($operand),*);
                (InstKind::$kind($($operand),*), None)
            }};
        }
        macro_rules! unit {
            ($kind:ident => $ty:expr) => {
                (InstKind::$kind, Some($ty))
            };
        }
        macro_rules! struct_inst {
            ($kind:ident { $($operand:ident),* } => $ty:expr) => {{
                operands!($($operand),*);
                (InstKind::$kind { $($operand),* }, Some($ty))
            }};
        }

        let parsed = match mnemonic {
            // Arithmetic and bitwise operations.
            kw::Add => inst!(Add(a, b) => MirType::uint256()),
            kw::Sub => inst!(Sub(a, b) => MirType::uint256()),
            kw::Mul => inst!(Mul(a, b) => MirType::uint256()),
            kw::Div => inst!(Div(a, b) => MirType::uint256()),
            kw::Sdiv => inst!(SDiv(a, b) => MirType::int256()),
            kw::Mod => inst!(Mod(a, b) => MirType::uint256()),
            kw::Smod => inst!(SMod(a, b) => MirType::int256()),
            kw::Exp => inst!(Exp(a, b) => MirType::uint256()),
            kw::Addmod => inst!(AddMod(a, b, c) => MirType::uint256()),
            kw::Mulmod => inst!(MulMod(a, b, c) => MirType::uint256()),
            kw::And => inst!(And(a, b) => MirType::uint256()),
            kw::Or => inst!(Or(a, b) => MirType::uint256()),
            kw::Xor => inst!(Xor(a, b) => MirType::uint256()),
            kw::Not => inst!(Not(a) => MirType::uint256()),
            kw::Shl => inst!(Shl(a, b) => MirType::uint256()),
            kw::Shr => inst!(Shr(a, b) => MirType::uint256()),
            kw::Sar => inst!(Sar(a, b) => MirType::int256()),
            kw::Byte => inst!(Byte(a, b) => MirType::uint256()),
            kw::Signextend => inst!(SignExtend(a, b) => MirType::int256()),

            // Comparisons.
            kw::Lt => inst!(Lt(a, b) => MirType::Bool),
            kw::Gt => inst!(Gt(a, b) => MirType::Bool),
            kw::Slt => inst!(SLt(a, b) => MirType::Bool),
            kw::Sgt => inst!(SGt(a, b) => MirType::Bool),
            kw::Eq => inst!(Eq(a, b) => MirType::Bool),
            kw::Iszero => inst!(IsZero(a) => MirType::Bool),

            // Memory and storage.
            kw::Mload => inst!(MLoad(a) => MirType::uint256()),
            kw::Mstore => inst!(MStore(a, b)),
            kw::Mstore8 => inst!(MStore8(a, b)),
            kw::Msize => unit!(MSize => MirType::uint256()),
            kw::Mcopy => inst!(MCopy(a, b, c)),
            kw::Sload => inst!(SLoad(a) => MirType::uint256()),
            kw::Sstore => inst!(SStore(a, b)),
            kw::Tload => inst!(TLoad(a) => MirType::uint256()),
            kw::Tstore => inst!(TStore(a, b)),

            // Free-memory pointer and allocation.
            sym::fmp => unit!(Fmp => MirType::MemPtr),
            sym::set_fmp => inst!(SetFmp(a)),
            sym::alloc => {
                let name = self.parser.parse_ident()?;
                let kind = match name {
                    sym::raw => AllocationKind::Raw,
                    _ => AllocationKind::Object(self.parse_memory_object_layout(name)?),
                };
                self.parser.expect(TokenKind::Comma)?;
                let alignment = match self.parser.parse_ident()? {
                    sym::exact => AllocationAlignment::Exact,
                    sym::word => AllocationAlignment::Word,
                    other => {
                        return Err(self
                            .parser
                            .error(format!("unknown allocation alignment `{other}`")));
                    }
                };
                self.parser.expect(TokenKind::Comma)?;
                let initialization = match self.parser.parse_ident()? {
                    sym::uninitialized => AllocationInitialization::Uninitialized,
                    sym::zeroed => AllocationInitialization::Zeroed,
                    other => {
                        return Err(self
                            .parser
                            .error(format!("unknown allocation initialization `{other}`")));
                    }
                };
                self.parser.expect(TokenKind::Comma)?;
                let failure = match self.parser.parse_ident()? {
                    sym::infallible => AllocationFailure::Infallible,
                    sym::panic => AllocationFailure::Panic,
                    other => {
                        return Err(self
                            .parser
                            .error(format!("unknown allocation failure `{other}`")));
                    }
                };
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_value(builder)?;
                let semantics = AllocationSemantics { alignment, initialization, failure };
                (InstKind::Alloc { size, kind, semantics }, Some(kind.result_type()))
            }

            // Semantic memory-object accessors.
            sym::memory_object_len => {
                let name = self.parser.parse_ident()?;
                let kind = self.parse_memory_object_layout(name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                (InstKind::MemoryObjectLen(object, kind), Some(MirType::uint256()))
            }
            sym::set_memory_object_len => {
                let name = self.parser.parse_ident()?;
                let kind = self.parse_memory_object_layout(name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let len = self.parse_value(builder)?;
                (InstKind::SetMemoryObjectLen(object, len, kind), None)
            }
            sym::memory_object_data => {
                let name = self.parser.parse_ident()?;
                let kind = self.parse_memory_object_layout(name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                (InstKind::MemoryObjectData(object, kind), Some(MirType::MemPtr))
            }
            sym::memory_object_field_addr => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let field = self.parser.parse_uint()?;
                let field = field
                    .try_into()
                    .map_err(|_| self.parser.error("memory field index does not fit in u64"))?;
                (InstKind::MemoryObjectFieldAddr { object, layout, field }, Some(MirType::MemPtr))
            }
            sym::memory_object_element_addr => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let index = self.parse_value(builder)?;
                (InstKind::MemoryObjectElementAddr { object, layout, index }, Some(MirType::MemPtr))
            }

            // Semantic ABI encoding.
            sym::abi_encode => {
                let layout = self.parse_abi_layout()?;
                let mut selector = None;
                let mut args = Vec::new();
                while self.parser.eat(TokenKind::Comma) {
                    let group = self.parser.parse_ident()?;
                    match group {
                        sym::selector if selector.is_none() => {
                            selector = Some(self.parse_value(builder)?)
                        }
                        sym::args if args.is_empty() => {
                            args.push(self.parse_value(builder)?);
                            while self.parser.eat(TokenKind::Comma) {
                                args.push(self.parse_value(builder)?);
                            }
                            break;
                        }
                        _ => {
                            return Err(self
                                .parser
                                .error(format!("unexpected ABI encode operand group `{group}`")));
                        }
                    }
                }
                if args.len() != layout.types.len() {
                    return Err(self.parser.error(format!(
                        "ABI encode layout has {} types but {} arguments",
                        layout.types.len(),
                        args.len()
                    )));
                }
                (
                    InstKind::AbiEncode { selector, args: args.into(), layout },
                    Some(MirType::Slice(SliceLocation::Memory)),
                )
            }
            // Aggregate storage/memory copies with interned layouts.
            sym::storage_to_memory => {
                let layout = self.parse_storage_layout()?;
                self.parser.expect(TokenKind::Comma)?;
                let storage = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let memory = self.parse_value(builder)?;
                (InstKind::StorageToMemory { storage, memory, layout }, None)
            }
            sym::memory_to_storage => {
                let layout = self.parse_storage_layout()?;
                self.parser.expect(TokenKind::Comma)?;
                let memory = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let storage = self.parse_value(builder)?;
                (InstKind::MemoryToStorage { memory, storage, layout }, None)
            }
            sym::clear_storage => {
                let layout = self.parse_storage_layout()?;
                self.parser.expect(TokenKind::Comma)?;
                let storage = self.parse_value(builder)?;
                (InstKind::ClearStorage { storage, layout }, None)
            }

            // Calldata, code, and return data.
            kw::Calldataload => inst!(CalldataLoad(a) => MirType::uint256()),
            kw::Calldatasize => unit!(CalldataSize => MirType::uint256()),
            kw::Calldatacopy => inst!(CalldataCopy(a, b, c)),

            // Slices.
            sym::make_memory_slice | sym::make_calldata_slice | sym::make_returndata_slice => {
                let ptr = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let len = self.parse_value(builder)?;
                let location = if mnemonic == sym::make_memory_slice {
                    SliceLocation::Memory
                } else if mnemonic == sym::make_calldata_slice {
                    SliceLocation::Calldata
                } else {
                    SliceLocation::Returndata
                };
                (InstKind::MakeSlice { ptr, len, location }, Some(MirType::Slice(location)))
            }
            sym::slice_ptr => inst!(SlicePtr(a) => MirType::uint256()),
            sym::slice_len => inst!(SliceLen(a) => MirType::uint256()),

            kw::Codesize => unit!(CodeSize => MirType::uint256()),
            kw::Codecopy => inst!(CodeCopy(a, b, c)),
            kw::Loadimmutable => {
                let offset = self.parser.parse_uint()?;
                let offset = self.u256_to_u32(offset)?;
                (InstKind::LoadImmutable(offset), Some(MirType::uint256()))
            }
            kw::Extcodesize => inst!(ExtCodeSize(a) => MirType::uint256()),
            kw::Extcodecopy => inst!(ExtCodeCopy(a, b, c, d)),
            kw::Extcodehash => inst!(ExtCodeHash(a) => MirType::uint256()),
            kw::Returndatasize => unit!(ReturnDataSize => MirType::uint256()),
            kw::Returndatacopy => inst!(ReturnDataCopy(a, b, c)),

            // Environment.
            kw::Caller => unit!(Caller => MirType::Address),
            kw::Callvalue => unit!(CallValue => MirType::uint256()),
            kw::Origin => unit!(Origin => MirType::Address),
            kw::Gasprice => unit!(GasPrice => MirType::uint256()),
            kw::Coinbase => unit!(Coinbase => MirType::Address),
            kw::Timestamp => unit!(Timestamp => MirType::uint256()),
            kw::Number => unit!(BlockNumber => MirType::uint256()),
            kw::Prevrandao => unit!(PrevRandao => MirType::uint256()),
            kw::Gaslimit => unit!(GasLimit => MirType::uint256()),
            kw::Chainid => unit!(ChainId => MirType::uint256()),
            kw::Address => unit!(Address => MirType::Address),
            kw::Selfbalance => unit!(SelfBalance => MirType::uint256()),
            kw::Gas => unit!(Gas => MirType::uint256()),
            kw::Basefee => unit!(BaseFee => MirType::uint256()),
            kw::Blobbasefee => unit!(BlobBaseFee => MirType::uint256()),
            kw::Blockhash => inst!(BlockHash(a) => MirType::FixedBytes(32)),
            kw::Balance => inst!(Balance(a) => MirType::uint256()),
            kw::Blobhash => inst!(BlobHash(a) => MirType::FixedBytes(32)),

            // Hashing.
            kw::Keccak256 => inst!(Keccak256(a, b) => MirType::bytes32()),
            sym::keccak256_bytes => inst!(Keccak256Bytes(a) => MirType::bytes32()),
            sym::mapping_slot => inst!(MappingSlot(key, slot) => MirType::bytes32()),
            sym::mapping_slot_memory => {
                inst!(MappingSlotMemory(key, slot) => MirType::bytes32())
            }
            sym::mapping_slot_calldata => {
                inst!(MappingSlotCalldata(key, slot) => MirType::bytes32())
            }

            // Calls and creation.
            kw::Call => struct_inst!(Call {
                gas, addr, value, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            kw::Staticcall => struct_inst!(StaticCall {
                gas, addr, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            kw::Delegatecall => struct_inst!(DelegateCall {
                gas, addr, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            sym::internal_call => {
                let function = self.parse_function_id()?;
                self.parser.expect(TokenKind::Comma)?;
                let returns = self.parser.parse_uint()?.to::<u32>();
                let mut args = Vec::new();
                while self.parser.eat(TokenKind::Comma) {
                    args.push(self.parse_value(builder)?);
                }
                let result_ty = (returns > 0).then(MirType::uint256);
                (InstKind::InternalCall { function, args: args.into(), returns }, result_ty)
            }
            sym::internal_frame_addr => {
                let offset = self.parser.parse_uint()?.to::<u64>();
                (InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
            }
            kw::Create => inst!(Create(a, b, c) => MirType::Address),
            kw::Create2 => inst!(Create2(a, b, c, d) => MirType::Address),

            // Logs and SSA operations.
            kw::Log0 => inst!(Log0(a, b)),
            kw::Log1 => inst!(Log1(a, b, c)),
            kw::Log2 => inst!(Log2(a, b, c, d)),
            kw::Log3 => inst!(Log3(a, b, c, d, e)),
            kw::Log4 => inst!(Log4(a, b, c, d, e, f)),
            sym::select => inst!(Select(condition, then_value, else_value) => MirType::uint256()),
            sym::phi => {
                let mut incoming = Vec::new();
                loop {
                    self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
                    let block = self.parse_block_id(builder)?;
                    self.parser.expect(TokenKind::Colon)?;
                    let value = self.parse_value(builder)?;
                    self.parser.expect(TokenKind::CloseDelim(Delimiter::Bracket))?;
                    incoming.push((block, value));
                    if !self.parser.eat(TokenKind::Comma) {
                        break;
                    }
                }
                (InstKind::Phi(incoming), Some(MirType::uint256()))
            }

            _ => {
                return Err(self
                    .parser
                    .error_at(mnemonic_span, format!("unknown instruction `{mnemonic}`")));
            }
        };
        Ok(parsed)
    }
}

//! Parser for the textual MIR format produced by [`Function`] and [`Module::to_text`].
//!
//! # Format
//!
//! ```text
//! @module Counter
//! immutables:
//!   initial: u256
//!
//! fn @constructor() {
//!   bb0:
//!     v0 = loadimmutable initial
//!     v1 = add v0, 1
//!     storeimmutable initial, v1
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
    AbiEncodeMode, AbiLayout, AbiLayoutRef, AbiParamLayout, AbiParamLayoutRef, AbiParamType,
    AbiType, AllocationAlignment, AllocationFailure, AllocationInitialization, AllocationKind,
    AllocationSemantics, BlockId, DataId, DataRef, Disambiguator, EffectKind, FrameMode,
    FrameSlotKind, Function, FunctionBuilder, FunctionId, ImmutableId, InstId, InstKind,
    Instruction, InstructionMetadata, MangledSymbol, MemoryObjectKind, MemoryObjectLayout,
    MemoryRegion, Module, StorageAlias, StorageField, StorageLayout, StorageLayoutRef, Terminator,
    Value, ValueId,
};
use crate::mir::{AbiWordValidator, MirType, SliceLocation, TypeSize};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_ast::{
    Arena,
    token::{BinOpToken, Delimiter, TokenKind, TokenLitKind},
};
use solar_data_structures::map::{FxHashMap, StdEntry};
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
    pending_function_ref: Option<(MangledSymbol, Span)>,
    parsed_dispatch_entry: bool,
    function_refs: Vec<PendingFunctionRef>,
    arg_values: Vec<ValueId>,
    block_labels: FxHashMap<u32, BlockLabel>,
    block_order: Vec<BlockId>,
    value_labels: FxHashMap<u32, ValueId>,
    immutable_names: FxHashMap<Symbol, (ImmutableId, MirType)>,
    data_sizes: Vec<usize>,
    /// ABI layouts interned while parsing instructions.
    abi_layouts: Vec<AbiLayoutRef>,
    /// ABI input layouts interned while parsing instructions.
    abi_param_layouts: Vec<AbiParamLayoutRef>,
    /// Number of `>` closers still owed after splitting a `>>`/`>>>` token.
    pending_gt: u32,
}

struct PendingFunctionRef {
    name: MangledSymbol,
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
            parsed_dispatch_entry: false,
            function_refs: Vec::new(),
            arg_values: Vec::new(),
            block_labels: FxHashMap::default(),
            block_order: Vec::new(),
            value_labels: FxHashMap::default(),
            immutable_names: FxHashMap::default(),
            data_sizes: Vec::new(),
            abi_layouts: Vec::new(),
            abi_param_layouts: Vec::new(),
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
    fn parse_function_name(&mut self) -> PResult<'sess, MangledSymbol> {
        let first = self.parser.parse_ident()?;
        let mut name = first.to_string();
        while self.parser.eat(TokenKind::Dot) {
            name.push('.');
            name.push_str(self.parser.parse_ident()?.as_str());
        }
        let symbol = Symbol::intern(&name);
        let TokenKind::Literal(TokenLitKind::Rational, suffix) = self.parser.token().kind else {
            return Ok(MangledSymbol::new(symbol));
        };
        let Some(disambiguator) = suffix.as_str().strip_prefix('.') else {
            return Ok(MangledSymbol::new(symbol));
        };
        let disambiguator = disambiguator
            .parse::<u32>()
            .map_err(|_| self.parser.error("invalid function disambiguator"))?;
        if disambiguator == u32::MAX {
            return Err(self.parser.error("invalid function disambiguator"));
        }
        let disambiguator = Disambiguator::new(disambiguator as usize);
        self.parser.bump();
        Ok(MangledSymbol::disambiguated(symbol, disambiguator))
    }

    // ----- module / function parsing -----

    fn parse_module(&mut self) -> PResult<'sess, Module> {
        let mut phase = super::MirPhase::default();
        let mut is_library = false;
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
                kw::Library => is_library = true,
                _ => return Err(self.parser.error(format!("unknown module attribute `@{attr}`"))),
            }
        }

        let module_ident = Ident::with_dummy_span(module_name);
        let mut module = Module::new(module_ident);
        module.phase = phase;
        module.is_library = is_library;
        let mut function_refs = Vec::new();

        if self.parser.check_keyword(sym::data) {
            self.parse_data_declarations(&mut module)?;
        }
        if self.parser.check_keyword(sym::immutables) {
            self.parse_immutable_declarations(&mut module)?;
        }

        while !self.parser.is_eof() {
            let func = self.parse_function()?;
            let is_dispatch_entry = self.parsed_dispatch_entry;
            let function = module.add_function(func);
            if is_dispatch_entry {
                if module.dispatch_entry().is_some() {
                    return Err(self.parser.error("module has multiple `entry` routing functions"));
                }
                module.set_dispatch_entry(function);
            }
            function_refs
                .extend(self.function_refs.drain(..).map(|reference| (function, reference)));
        }
        self.resolve_function_refs(&mut module, function_refs)?;

        module.abi_layouts = std::mem::take(&mut self.abi_layouts);
        module.abi_param_layouts = std::mem::take(&mut self.abi_param_layouts);
        let tracks_debug_info = module.iter_functions().any(|(_, func)| {
            func.instructions().any(|inst| {
                let metadata = &func.inst(inst).metadata;
                metadata.source_span().is_some() || metadata.modifier_depth() != 0
            })
        });
        if tracks_debug_info {
            module.set_debug_info_tracked(true);
            for function_id in module.functions.indices() {
                let function = &mut module.functions[function_id];
                let instructions = function.instructions().collect::<Vec<_>>();
                for instruction in instructions {
                    let metadata = &mut function.inst_mut(instruction).metadata;
                    if !metadata.debug_info_is_handled() {
                        metadata.mark_debug_info_dropped();
                    }
                }
            }
        }
        Ok(module)
    }

    fn parse_data_declarations(&mut self, module: &mut Module) -> PResult<'sess, ()> {
        self.parser.expect_keyword(sym::data)?;
        self.parser.expect(TokenKind::Colon)?;
        while !self.parser.is_eof()
            && !self.parser.check_keyword(sym::immutables)
            && !(self.parser.check_keyword(sym::fn_)
                && self.parser.look_ahead(1).kind == TokenKind::At)
        {
            let (id, name) = self.parser.parse_data_id()?;
            let expected = U256::from(module.data_count());
            if id != expected {
                return Err(self.parser.error(format!("expected data ID {expected}, found {id}")));
            }
            self.parser.expect(TokenKind::Colon)?;
            let bytes = self.parser.parse_data_bytes()?;
            module.add_data(bytes, name);
        }
        self.data_sizes = module.iter_data().map(|(_, data)| data.len()).collect();
        Ok(())
    }

    fn parse_immutable_declarations(&mut self, module: &mut Module) -> PResult<'sess, ()> {
        self.parser.expect_keyword(sym::immutables)?;
        self.parser.expect(TokenKind::Colon)?;
        while !self.parser.is_eof()
            && !(self.parser.check_keyword(sym::fn_)
                && self.parser.look_ahead(1).kind == TokenKind::At)
        {
            let name_span = self.parser.token().span;
            let name = self.parser.parse_ident()?;
            self.parser.expect(TokenKind::Colon)?;
            let ty = self.parse_type()?;
            match self.immutable_names.entry(name) {
                StdEntry::Occupied(entry) => {
                    return Err(self.parser.error_at(
                        name_span,
                        format!("duplicate immutable declaration `{}`", entry.key()),
                    ));
                }
                StdEntry::Vacant(entry) => {
                    let id = module.add_immutable(Ident::new(name, name_span), ty, None);
                    entry.insert((id, ty));
                }
            }
        }
        Ok(())
    }

    fn resolve_function_refs(
        &self,
        module: &mut Module,
        function_refs: Vec<(FunctionId, PendingFunctionRef)>,
    ) -> PResult<'sess, ()> {
        let mut declarations = FxHashMap::<MangledSymbol, Vec<FunctionId>>::default();
        for (id, function) in module.functions.iter_enumerated() {
            declarations.entry(function.name).or_default().push(id);
        }
        for (owner, reference) in function_refs {
            let matches = declarations.get(&reference.name);
            let Some(matches) = matches else {
                return Err(self.parser.error_at(
                    reference.span,
                    format!("unknown function reference `{}`", reference.name),
                ));
            };
            let [function] = matches.as_slice() else {
                return Err(self.parser.error_at(
                    reference.span,
                    format!("function reference `{}` is ambiguous", reference.name),
                ));
            };
            match reference.target {
                FunctionRefTarget::Instruction(inst) => {
                    let result_ty = module.functions[*function].returns.first().copied();
                    let instruction = module.functions[owner].inst_mut(inst);
                    let InstKind::ICall { function: target, returns, .. } = &mut instruction.kind
                    else {
                        unreachable!()
                    };
                    *target = *function;
                    if *returns > 0 && result_ty.is_some() {
                        instruction.result_ty = result_ty;
                    }
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
        self.parsed_dispatch_entry = false;
        self.arg_values.clear();
        self.block_labels.clear();
        self.block_order.clear();
        self.value_labels.clear();

        self.parser.expect_keyword(sym::fn_)?;
        self.parser.expect(TokenKind::At)?;
        let name = self.parse_function_name()?;
        let func_ident = Ident::with_dummy_span(name.symbol);
        let mut func = Function::new(func_ident);
        func.name = name;
        func.attributes.is_constructor = name.symbol == kw::Constructor;
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
                sym::abi_returns => {
                    self.parser.expect(TokenKind::Eq)?;
                    builder.func_mut().abi_returns = Some(self.parse_abi_layout()?);
                }
                sym::abi_return_params => {
                    self.parser.expect(TokenKind::Eq)?;
                    builder.func_mut().abi_return_params = Some(self.parse_abi_param_layout()?);
                }
                sym::abi_params => {
                    self.parser.expect(TokenKind::Eq)?;
                    builder.func_mut().abi_params = Some(self.parse_abi_param_layout()?);
                }
                sym::entry => self.parsed_dispatch_entry = true,
                sym::may_return_memory => {
                    builder.func_mut().attributes.may_return_memory = true;
                }
                sym::function_pointer_dispatcher => {
                    builder.func_mut().attributes.is_function_pointer_dispatcher = true;
                }
                kw::Constructor => builder.func_mut().attributes.is_constructor = true,
                kw::Receive => builder.func_mut().attributes.is_receive = true,
                kw::Fallback => builder.func_mut().attributes.is_fallback = true,
                kw::Pure => {
                    builder.func_mut().attributes.state_mutability = hir::StateMutability::Pure;
                }
                kw::View => {
                    builder.func_mut().attributes.state_mutability = hir::StateMutability::View;
                }
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
        self.parse_type_from_ident(id)
    }

    fn parse_type_from_ident(&mut self, id: Symbol) -> PResult<'sess, MirType> {
        let id_str = id.as_str();
        // u8..u256, i8..i256, bytes1..bytes32 — split into prefix + number.
        let ty = if let Some(rest) = id_str.strip_prefix('u') {
            let bits: u16 =
                rest.parse().map_err(|_| self.parser.error(format!("invalid u-type `{id}`")))?;
            let size = TypeSize::try_new_int_bits(bits)
                .filter(|size| size.bits_raw() != 0)
                .ok_or_else(|| self.parser.error(format!("invalid u-type `{id}`")))?;
            MirType::UInt(size)
        } else if let Some(rest) = id_str.strip_prefix('i') {
            let bits: u16 =
                rest.parse().map_err(|_| self.parser.error(format!("invalid i-type `{id}`")))?;
            let size = TypeSize::try_new_int_bits(bits)
                .filter(|size| size.bits_raw() != 0)
                .ok_or_else(|| self.parser.error(format!("invalid i-type `{id}`")))?;
            MirType::Int(size)
        } else if let Some(rest) = id_str.strip_prefix("bytes") {
            let n: u8 = rest
                .parse()
                .map_err(|_| self.parser.error(format!("invalid bytes type `{id}`")))?;
            let size = TypeSize::try_new_fb_bytes(n)
                .ok_or_else(|| self.parser.error(format!("invalid bytes type `{id}`")))?;
            MirType::FixedBytes(size)
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
            return Ok(builder.imm(v));
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
                for _ in self.arg_values.len()..=idx {
                    let val = builder.func_mut().alloc_implicit_arg(MirType::uint256());
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
    ) -> PResult<'sess, Option<ValueId>> {
        if let Some(value) = self.value_labels.get(&label).copied() {
            if matches!(builder.func().value(value), Value::Undef(_)) {
                return Ok(Some(value));
            }
            return Err(self.parser.error(format!("duplicate value `v{label}`")));
        }

        Ok(None)
    }

    fn reject_unresolved_value_labels(&self, func: &Function) -> PResult<'sess, ()> {
        let mut unresolved: Vec<_> = self
            .value_labels
            .iter()
            .filter_map(|(&label, &value)| {
                matches!(func.value(value), Value::Undef(_)).then_some(label)
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

    fn intern_abi_param_layout(&mut self, layout: AbiParamLayout) -> AbiParamLayoutRef {
        if let Some(existing) = self.abi_param_layouts.iter().find(|item| item.as_ref() == &layout)
        {
            return std::sync::Arc::clone(existing);
        }
        let layout = std::sync::Arc::new(layout);
        self.abi_param_layouts.push(std::sync::Arc::clone(&layout));
        layout
    }

    fn parse_abi_type(&mut self) -> PResult<'sess, AbiType> {
        let name = self.parser.parse_ident()?;
        Ok(match name {
            sym::word => {
                if !self.parser.eat(TokenKind::Lt) {
                    return Ok(AbiType::Word(None));
                }
                let id = self.parser.parse_ident()?;
                let cleanup = if id == kw::Enum {
                    let variants = self.parser.parse_uint()?;
                    let variants = variants.try_into().map_err(|_| {
                        self.parser.error("ABI enum variant count does not fit in u64")
                    })?;
                    AbiWordValidator::EnumRange(variants)
                } else {
                    let ty = self.parse_type_from_ident(id)?;
                    AbiWordValidator::from_mir_type(ty).ok_or_else(|| {
                        self.parser.error(format!("ABI word type `{ty}` needs no cleanup"))
                    })?
                };
                self.expect_gt()?;
                AbiType::Word(Some(cleanup))
            }
            kw::Function => AbiType::Function,
            sym::memory_bytes => AbiType::Bytes(SliceLocation::Memory),
            sym::calldata_bytes => AbiType::Bytes(SliceLocation::Calldata),
            sym::returndata_bytes => AbiType::Bytes(SliceLocation::Returndata),
            sym::memory_array | sym::calldata_array | sym::returndata_array => {
                self.parser.expect(TokenKind::Lt)?;
                let element = Box::new(self.parse_abi_type()?);
                self.expect_gt()?;
                let location = match name {
                    sym::memory_array => SliceLocation::Memory,
                    sym::calldata_array => SliceLocation::Calldata,
                    sym::returndata_array => SliceLocation::Returndata,
                    _ => unreachable!(),
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

    fn parse_abi_param_layout(&mut self) -> PResult<'sess, AbiParamLayout> {
        self.parser.expect(TokenKind::OpenDelim(Delimiter::Bracket))?;
        let mut types = Vec::new();
        if !self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
            loop {
                types.push(self.parse_abi_param_type()?);
                if self.parser.eat(TokenKind::CloseDelim(Delimiter::Bracket)) {
                    break;
                }
                self.parser.expect(TokenKind::Comma)?;
            }
        }
        Ok(AbiParamLayout::new(types))
    }

    fn parse_abi_param_type(&mut self) -> PResult<'sess, AbiParamType> {
        let name = self.parser.parse_ident()?;
        Ok(match name {
            kw::Bytes => AbiParamType::Bytes,
            kw::Enum => {
                self.parser.expect(TokenKind::Lt)?;
                let variants =
                    self.parser.parse_uint()?.try_into().map_err(|_| {
                        self.parser.error("ABI enum variant count does not fit in u64")
                    })?;
                self.parser.expect(TokenKind::Comma)?;
                let ty_name = self.parser.parse_ident()?;
                let ty = self.parse_type_from_ident(ty_name)?;
                self.expect_gt()?;
                AbiParamType::Enum { ty, variants }
            }
            sym::array => {
                self.parser.expect(TokenKind::Lt)?;
                let len = if self.parser.eat(TokenKind::Ident(sym::underscore)) {
                    None
                } else {
                    Some(self.parser.parse_uint()?.try_into().map_err(|_| {
                        self.parser.error("ABI fixed-array length does not fit in u64")
                    })?)
                };
                self.parser.expect(TokenKind::Comma)?;
                let element = Box::new(self.parse_abi_param_type()?);
                self.expect_gt()?;
                match len {
                    Some(len) => AbiParamType::FixedArray { element, len },
                    None => AbiParamType::DynamicArray(element),
                }
            }
            sym::tuple => {
                self.parser.expect(TokenKind::Lt)?;
                let mut fields = Vec::new();
                if !self.eat_gt() {
                    loop {
                        fields.push(self.parse_abi_param_type()?);
                        if self.eat_gt() {
                            break;
                        }
                        self.parser.expect(TokenKind::Comma)?;
                    }
                }
                AbiParamType::Tuple(fields.into())
            }
            _ => AbiParamType::Scalar(self.parse_type_from_ident(name)?),
        })
    }

    /// Parses a storage layout: `struct<field, ...>` or `array<len, field>`.
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
        Ok(std::sync::Arc::new(layout))
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
        let span = self.parser.token().span;
        let name = self.parser.parse_ident()?;
        if let Some(index) = name.as_str().strip_prefix("fn").and_then(|s| s.parse().ok()) {
            return Ok(FunctionId::from_usize(index));
        }
        Err(self.parser.error_at(span, format!("invalid function reference `{name}`")))
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
            sym::revert_returndata => {
                builder.set_terminator(Terminator::RevertReturndata);
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
        if result_label.is_some() && result_ty.is_none() {
            return Err(self
                .parser
                .error_at(mnemonic_span, "instruction does not produce a result value"));
        }
        let existing_result = match result_label {
            Some(label) => self.resolve_result_label(builder, label)?,
            None => None,
        };
        let (inst_id, result) = if let Some(result) = existing_result {
            (builder.append_instruction_with_result(inst, result), Some(result))
        } else {
            builder.append_instruction(inst)
        };
        self.finish_function_ref(FunctionRefTarget::Instruction(inst_id));
        if let Some(label) = result_label
            && existing_result.is_none()
        {
            let result = result.ok_or_else(|| {
                self.parser.error_at(mnemonic_span, "instruction does not produce a result value")
            })?;
            self.value_labels.insert(label, result);
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
                    let (lo, hi) = self.parser.parse_span_bounds()?;
                    metadata.set_source_span(Some(Span::new(BytePos(lo), BytePos(hi))));
                }
                sym::modifier_depth => {
                    self.parser.expect(TokenKind::Eq)?;
                    let value = self.parser.parse_uint()?;
                    metadata.set_modifier_depth(self.u256_to_u32(value)?);
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

    fn parse_frame_mode(&mut self) -> PResult<'sess, FrameMode> {
        let mode = self.parser.parse_ident()?;
        Ok(match mode {
            sym::scratch => FrameMode::External,
            sym::internal_frame => FrameMode::Internal,
            sym::multi_return => FrameMode::MultiReturn,
            _ => return Err(self.parser.error(format!("unknown frame mode `{mode}`"))),
        })
    }

    fn parse_frame_slot_kind(&mut self) -> PResult<'sess, FrameSlotKind> {
        let kind = self.parser.parse_ident()?;
        Ok(match kind {
            sym::word => FrameSlotKind::Word,
            kw::Memory => FrameSlotKind::Slice(SliceLocation::Memory),
            kw::Calldata => FrameSlotKind::Slice(SliceLocation::Calldata),
            sym::returndata => FrameSlotKind::Slice(SliceLocation::Returndata),
            _ => return Err(self.parser.error(format!("unknown frame slot kind `{kind}`"))),
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
            sym::icall => EffectKind::ICall,
            kw::Create => EffectKind::Create,
            sym::log => EffectKind::Log,
            sym::immutable_read => EffectKind::ImmutableRead,
            sym::immutable_write => EffectKind::ImmutableWrite,
            _ => return Err(self.parser.error(format!("unknown effect metadata value `{value}`"))),
        })
    }

    fn u256_to_u32(&self, value: U256) -> PResult<'sess, u32> {
        value
            .try_into()
            .map_err(|_| self.parser.error(format!("integer `{value}` does not fit in u32")))
    }

    fn parse_immutable_ref(&mut self) -> PResult<'sess, (ImmutableId, MirType)> {
        let span = self.parser.token().span;
        let name = self.parser.parse_ident()?;
        self.immutable_names.get(&name).copied().ok_or_else(|| {
            self.parser.error_at(span, format!("unknown immutable declaration `{name}`"))
        })
    }

    fn parse_data_ref(&mut self) -> PResult<'sess, DataRef> {
        let span = self.parser.token().span;
        let (value, offset, offset_span) = self.parser.parse_data_ref()?;
        let Ok(index) = usize::try_from(value) else {
            return Err(self.parser.error_at(span, "data ID exceeds the index limit"));
        };
        let Some(&size) = self.data_sizes.get(index) else {
            return Err(self.parser.error_at(span, format!("unknown data ID `{index}`")));
        };
        if offset as usize > size {
            return Err(self
                .parser
                .error_at(offset_span, format!("data offset {offset} exceeds data size {size}")));
        }
        Ok(DataRef::new(DataId::from_usize(index), offset))
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
            kw::Clz => inst!(Clz(a) => MirType::uint256()),
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
            sym::memory_zero => inst!(MemoryZero(a, b)),
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
            sym::memory_object_load_field => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let field = self
                    .parser
                    .parse_uint()?
                    .try_into()
                    .map_err(|_| self.parser.error("memory field index does not fit in u64"))?;
                (
                    InstKind::MemoryObjectLoadField { object, layout, field },
                    Some(MirType::uint256()),
                )
            }
            sym::memory_object_store_field => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let field = self
                    .parser
                    .parse_uint()?
                    .try_into()
                    .map_err(|_| self.parser.error("memory field index does not fit in u64"))?;
                self.parser.expect(TokenKind::Comma)?;
                let value = self.parse_value(builder)?;
                (InstKind::MemoryObjectStoreField { object, layout, field, value }, None)
            }
            sym::memory_object_load_element => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let index = self.parse_value(builder)?;
                (
                    InstKind::MemoryObjectLoadElement { object, layout, index },
                    Some(MirType::uint256()),
                )
            }
            sym::memory_object_load_byte => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                if layout != crate::mir::MemoryObjectLayout::Bytes {
                    return Err(self.parser.error("memory byte load requires a bytes object"));
                }
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let index = self.parse_value(builder)?;
                (InstKind::MemoryObjectLoadByte { object, index }, Some(MirType::uint256()))
            }
            sym::memory_object_store_element => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let index = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let value = self.parse_value(builder)?;
                (InstKind::MemoryObjectStoreElement { object, layout, index, value }, None)
            }
            sym::memory_object_store_byte => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                if layout != crate::mir::MemoryObjectLayout::Bytes {
                    return Err(self.parser.error("memory byte store requires a bytes object"));
                }
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let index = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let value = self.parse_value(builder)?;
                (InstKind::MemoryObjectStoreByte { object, index, value }, None)
            }
            sym::memory_object_store_word => {
                let name = self.parser.parse_ident()?;
                let layout = self.parse_memory_object_layout(name)?;
                if layout != crate::mir::MemoryObjectLayout::Bytes {
                    return Err(self.parser.error("memory word store requires a bytes object"));
                }
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let value = self.parse_value(builder)?;
                (InstKind::MemoryObjectStoreWord { object, offset, value }, None)
            }
            sym::memory_slice_load_word => {
                self.parser.expect(TokenKind::Ident(kw::Memory))?;
                self.parser.expect(TokenKind::Comma)?;
                let slice = self.parse_value(builder)?;
                if builder.func().value_ty(slice) != Some(MirType::Slice(SliceLocation::Memory)) {
                    return Err(self.parser.error("memory slice load requires a memory slice"));
                }
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parse_value(builder)?;
                (InstKind::MemorySliceLoadWord { slice, offset }, Some(MirType::uint256()))
            }
            sym::calldata_slice_load_word => {
                self.parser.expect(TokenKind::Ident(kw::Calldata))?;
                self.parser.expect(TokenKind::Comma)?;
                let slice = self.parse_value(builder)?;
                if builder.func().value_ty(slice) != Some(MirType::Slice(SliceLocation::Calldata)) {
                    return Err(self.parser.error("calldata slice load requires a calldata slice"));
                }
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parse_value(builder)?;
                (InstKind::CalldataSliceLoadWord { slice, offset }, Some(MirType::uint256()))
            }
            sym::memory_object_copy_from_slice => {
                let name = self.parser.parse_ident()?;
                let kind = self.parse_memory_object_layout(name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let source = self.parse_value(builder)?;
                (InstKind::MemoryObjectCopyFromSlice { object, kind, source }, None)
            }
            sym::memory_object_copy_from_slice_at => {
                let name = self.parser.parse_ident()?;
                let kind = self.parse_memory_object_layout(name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let object = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let source = self.parse_value(builder)?;
                (InstKind::MemoryObjectCopyFromSliceAt { object, kind, offset, source }, None)
            }
            sym::memory_object_copy => {
                let destination_name = self.parser.parse_ident()?;
                let destination_kind = self.parse_memory_object_layout(destination_name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let destination = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let source_name = self.parser.parse_ident()?;
                let source_kind = self.parse_memory_object_layout(source_name)?.kind();
                self.parser.expect(TokenKind::Comma)?;
                let source = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let length = self.parse_value(builder)?;
                (
                    InstKind::MemoryObjectCopy {
                        destination,
                        destination_kind,
                        source,
                        source_kind,
                        length,
                    },
                    None,
                )
            }

            // Semantic ABI encoding.
            sym::abi_encode => {
                let layout = self.parse_abi_layout()?;
                let mut mode = AbiEncodeMode::Slice;
                let mut selector = None;
                let mut args = Vec::new();
                while self.parser.eat(TokenKind::Comma) {
                    let group = self.parser.parse_ident()?;
                    match group {
                        sym::object if mode == AbiEncodeMode::Slice => mode = AbiEncodeMode::Bytes,
                        sym::scratch if mode == AbiEncodeMode::Slice => {
                            mode = AbiEncodeMode::Scratch
                        }
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
                    InstKind::AbiEncode { mode, selector, args: args.into(), layout },
                    Some(mode.result_type()),
                )
            }
            sym::abi_decode => {
                let layout = self.parse_abi_param_layout()?;
                self.parser.expect(TokenKind::Comma)?;
                let data = self.parse_value(builder)?;
                let data_ty = builder.func().value_ty(data);
                let pending_call = matches!(
                    builder.func().value(data),
                    Value::Inst(inst)
                        if matches!(builder.func().inst(*inst).kind, InstKind::ICall { .. })
                );
                if !matches!(data_ty, Some(MirType::MemoryObject(MemoryObjectKind::Bytes)))
                    && !(data_ty == Some(MirType::MemPtr)
                        && !layout.types.iter().any(AbiParamType::has_dynamic_child))
                    && !pending_call
                {
                    return Err(self
                        .parser
                        .error("ABI decode requires bytes or a static memory pointer"));
                }
                let result_ty = layout
                    .types
                    .first()
                    .map(AbiParamType::mir_type)
                    .ok_or_else(|| self.parser.error("ABI decode requires a result type"))?;
                let layout = self.intern_abi_param_layout(layout);
                (InstKind::AbiDecode { data, layout }, Some(result_ty))
            }
            // Aggregate storage/memory copies with recursive layouts.
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
            sym::constructor_args_base => unit!(ConstructorArgsBase => MirType::uint256()),
            sym::constructor_args_end => unit!(ConstructorArgsEnd => MirType::uint256()),

            sym::data_copy => {
                let data = self.parse_data_ref()?;
                self.parser.expect(TokenKind::Comma)?;
                let dest = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let size = self.parse_value(builder)?;
                (InstKind::DataCopy(data, dest, size), None)
            }
            kw::Codesize => unit!(CodeSize => MirType::uint256()),
            kw::Codecopy => inst!(CodeCopy(a, b, c)),
            sym::storeimmutable => {
                let (id, _) = self.parse_immutable_ref()?;
                self.parser.expect(TokenKind::Comma)?;
                let value = self.parse_value(builder)?;
                (InstKind::StoreImmutable(id, value), None)
            }
            kw::Loadimmutable => {
                let (id, ty) = self.parse_immutable_ref()?;
                (InstKind::LoadImmutable(id), Some(ty))
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
            kw::Slotnum => unit!(SlotNum => MirType::uint256()),
            kw::Chainid => unit!(ChainId => MirType::uint256()),
            kw::Address => unit!(Address => MirType::Address),
            kw::Selfbalance => unit!(SelfBalance => MirType::uint256()),
            kw::Gas => unit!(Gas => MirType::uint256()),
            kw::Basefee => unit!(BaseFee => MirType::uint256()),
            kw::Blobbasefee => unit!(BlobBaseFee => MirType::uint256()),
            kw::Blockhash => inst!(BlockHash(a) => MirType::bytes32()),
            kw::Balance => inst!(Balance(a) => MirType::uint256()),
            kw::Blobhash => inst!(BlobHash(a) => MirType::bytes32()),

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
            sym::storage_array_data_slot => {
                inst!(StorageArrayDataSlot(slot) => MirType::bytes32())
            }
            sym::storage_array_element_slot => {
                let slot = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let index = self.parse_value(builder)?;
                self.parser.expect(TokenKind::Comma)?;
                let element_slots = self.parser.parse_uint()?.try_into().map_err(|_| {
                    self.parser.error("storage array element stride does not fit in u64")
                })?;
                (
                    InstKind::StorageArrayElementSlot { slot, index, element_slots },
                    Some(MirType::bytes32()),
                )
            }

            // Calls and creation.
            kw::Call => struct_inst!(Call {
                gas, addr, value, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            kw::Callcode => struct_inst!(CallCode {
                gas, addr, value, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            kw::Staticcall => struct_inst!(StaticCall {
                gas, addr, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            kw::Delegatecall => struct_inst!(DelegateCall {
                gas, addr, args_offset, args_size, ret_offset, ret_size
            } => MirType::uint256()),
            kw::Extcall => struct_inst!(ExtCall { addr, args_offset, args_size, value }
                => MirType::uint256()),
            kw::Extdelegatecall => struct_inst!(ExtDelegateCall { addr, args_offset, args_size }
                => MirType::uint256()),
            kw::Extstaticcall => struct_inst!(ExtStaticCall { addr, args_offset, args_size }
                => MirType::uint256()),
            sym::icall => {
                let function = self.parse_function_id()?;
                self.parser.expect(TokenKind::Comma)?;
                let returns = self.parser.parse_uint()?.to::<u32>();
                let mut args = Vec::new();
                while self.parser.eat(TokenKind::Comma) {
                    args.push(self.parse_value(builder)?);
                }
                let result_ty = (returns > 0).then(MirType::uint256);
                (InstKind::ICall { function, args: args.into(), returns }, result_ty)
            }
            sym::internal_frame_addr => {
                let offset = self.parser.parse_uint()?.to::<u64>();
                (InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
            }
            sym::frame_load => {
                let mode = self.parse_frame_mode()?;
                self.parser.expect(TokenKind::Comma)?;
                let kind = self.parse_frame_slot_kind()?;
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parser.parse_uint()?.to::<u64>();
                (InstKind::FrameLoad { offset, mode, kind }, Some(kind.result_type()))
            }
            sym::frame_store => {
                let mode = self.parse_frame_mode()?;
                self.parser.expect(TokenKind::Comma)?;
                let kind = self.parse_frame_slot_kind()?;
                self.parser.expect(TokenKind::Comma)?;
                let offset = self.parser.parse_uint()?.to::<u64>();
                self.parser.expect(TokenKind::Comma)?;
                let value = self.parse_value(builder)?;
                (InstKind::FrameStore { offset, mode, kind, value }, None)
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
                let ty = incoming
                    .iter()
                    .filter(|(_, value)| !matches!(builder.func().value(*value), Value::Undef(_)))
                    .find_map(|(_, value)| builder.func().value_ty(*value))
                    .unwrap_or(MirType::uint256());
                (InstKind::Phi(incoming), Some(ty))
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

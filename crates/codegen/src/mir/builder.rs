//! MIR function builder.

use super::{
    AbiEncodeMode, AddressCallKind, AllocationFailure, AllocationSemantics, BlockId, CalldataTail,
    FrameMode, FrameSlotKind, Function, FunctionId, Immediate, ImmutableId, InstId, InstKind,
    Instruction, InstructionMetadata, LogTopics, MemoryObjectLayout, MemoryRegion, MirType,
    PanicCode, PtrLocation, RevertKind, RevertPayload, RevertReason, StorageAlias, StructId,
    Terminator, Value, ValueId,
};
use crate::{
    link::LibraryId,
    mir::{Callee, memory::EvmMemoryLayout},
    target::{ConditionUse, Target},
};
use alloy_primitives::{Bytes, U256};
use smallvec::SmallVec;
use solar_config::RevertStrings;
use solar_data_structures::map::FxHashMap;
use solar_interface::Span;

pub(crate) trait ToUint {
    fn to_uint(self) -> U256;
}

impl ToUint for U256 {
    fn to_uint(self) -> U256 {
        self
    }
}

macro_rules! impl_to_uint {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ToUint for $ty {
                fn to_uint(self) -> U256 {
                    U256::from(self)
                }
            }
        )*
    };
}

impl_to_uint!(u8, u16, u32, u64, u128, usize);

macro_rules! impl_signed_to_uint {
    ($($ty:ty),* $(,)?) => {
        $(
            impl ToUint for $ty {
                fn to_uint(self) -> U256 {
                    let value = U256::from(self.unsigned_abs());
                    if self < 0 {
                        value.wrapping_neg()
                    } else {
                        value
                    }
                }
            }
        )*
    };
}

impl_signed_to_uint!(i8, i16, i32, i64, i128, isize);

/// The bit width of the longest dynamic word array that a checked allocation can hold: `2^59`
/// words are `2^64` bytes, past [`EvmMemoryLayout::MAX_ALLOCATION_END`].
const ALLOCATION_LENGTH_BITS: u32 =
    EvmMemoryLayout::MAX_ALLOCATION_END.count_ones() - EvmMemoryLayout::WORD_SIZE.trailing_zeros();

/// The Error(string) selector, `keccak256("Error(string)")[..4]`, left-aligned in a word.
pub(crate) const ERROR_SELECTOR: U256 = U256::from_limbs([0, 0, 0, 0x08c3_79a0_u64 << 32]);

/// The Panic(uint256) selector, `keccak256("Panic(uint256)")[..4]`, left-aligned in a word.
const PANIC_SELECTOR: U256 = U256::from_limbs([0, 0, 0, 0x4e48_7b71_u64 << 32]);

/// Revert blocks shared while constructing one MIR function.
#[derive(Default)]
struct RevertBlocks(FxHashMap<RevertKind, BlockId>);

/// A builder for constructing MIR functions.
pub(crate) struct FunctionBuilder<'a> {
    /// The function being built.
    func: &'a mut Function,
    /// The current block.
    current_block: BlockId,
    /// Revert blocks shared within this function.
    revert_blocks: RevertBlocks,
    /// Source context attached to instructions emitted in the current lowering scope.
    current_debug_context: InstructionMetadata,
    /// How compiler-generated reverts with a [`RevertReason`] are encoded.
    revert_strings: RevertStrings,
    semantic: bool,
    /// The cost model that prices equivalent shapes of a test, such as
    /// [`Self::fits_in_bits`]. Without one, tests take the spelling of a comparison.
    target: Option<Target>,
}

/// A counted loop whose body is the builder's current block.
pub(crate) struct CountedLoop {
    header: BlockId,
    exit: BlockId,
    index: ValueId,
}

impl CountedLoop {
    /// Returns the loop index.
    pub(crate) const fn index(&self) -> ValueId {
        self.index
    }
}

impl<'a> FunctionBuilder<'a> {
    /// Creates a new function builder.
    pub(crate) fn new(func: &'a mut Function) -> Self {
        Self {
            func,
            current_block: BlockId::ENTRY,
            revert_blocks: RevertBlocks::default(),
            current_debug_context: InstructionMetadata::EMPTY,
            revert_strings: RevertStrings::Default,
            semantic: false,
            target: None,
        }
    }

    /// Builds semantic MIR without expanding checks into control flow.
    pub(crate) fn new_semantic(func: &'a mut Function) -> Self {
        Self { semantic: true, ..Self::new(func) }
    }

    /// Selects how compiler-generated reverts with a [`RevertReason`] are encoded.
    ///
    /// Only `debug` changes the output: reasons then revert with an `Error(string)` payload
    /// instead of empty data.
    pub(crate) fn with_revert_strings(mut self, revert_strings: RevertStrings) -> Self {
        self.revert_strings = revert_strings;
        self
    }

    /// Prices the shapes of a test by `target`.
    pub(crate) fn with_target(mut self, target: Target) -> Self {
        self.target = Some(target);
        self
    }

    /// Returns `true` if reasons revert with an `Error(string)` payload.
    ///
    /// Lowering can use this to split a fused check into per-reason checks only when the
    /// reasons are observable, keeping the default output unchanged.
    pub(crate) fn encodes_revert_reasons(&self) -> bool {
        self.revert_strings.is_debug()
    }

    /// Replaces the source span attached to newly emitted instructions.
    pub(crate) fn replace_source_span(&mut self, span: Span) -> Span {
        let previous = self.current_debug_context.source_span().unwrap_or(Span::DUMMY);
        self.current_debug_context.set_debug_source_span(Some(span));
        previous
    }

    /// Replaces the modifier nesting depth attached to newly emitted instructions.
    pub(crate) fn replace_modifier_depth(&mut self, depth: u32) -> u32 {
        let previous = self.current_debug_context.modifier_depth();
        self.current_debug_context.set_modifier_depth(depth);
        previous
    }

    /// Inherits source context while replacing an instruction or control transfer.
    pub(crate) fn set_debug_context(&mut self, metadata: &InstructionMetadata) {
        self.current_debug_context.copy_debug_context(metadata);
    }

    /// Inherits the source context of a terminator being lowered in this block.
    pub(crate) fn inherit_terminator_debug_context(&mut self, block: BlockId) {
        let metadata = self.func.blocks[block].terminator_metadata.clone();
        self.set_debug_context(&metadata);
    }

    fn set_current_debug_context(&self, metadata: &mut InstructionMetadata) {
        metadata.copy_debug_context(&self.current_debug_context);
    }

    /// Returns the current block.
    #[must_use]
    pub(crate) const fn current_block(&self) -> BlockId {
        self.current_block
    }

    /// Switches blocks without changing the enclosing source-lowering scope.
    ///
    /// Rewrites using a fresh builder must explicitly inherit the debug context
    /// of the instruction or terminator they replace after switching blocks.
    pub(crate) fn switch_to_block(&mut self, block: BlockId) {
        self.current_block = block;
    }

    /// Creates a new basic block.
    pub(crate) fn create_block(&mut self) -> BlockId {
        self.func.alloc_block()
    }

    /// Starts `for index in 0..length` and switches to its body.
    pub(crate) fn begin_counted_loop(&mut self, length: ValueId) -> CountedLoop {
        let preheader = self.current_block();
        let header = self.create_block();
        let body = self.create_block();
        let exit = self.create_block();
        self.jump(header);

        self.switch_to_block(header);
        let zero = self.imm(0);
        let index = self.phi(vec![(preheader, zero)]);
        let more = self.lt(index, length);
        self.branch(more, body, exit);
        self.switch_to_block(body);
        CountedLoop { header, exit, index }
    }

    /// Finishes a counted loop and switches to its exit.
    pub(crate) fn finish_counted_loop(&mut self, loop_: CountedLoop) {
        let next = self.add_u64_offset(loop_.index, 1);
        let backedge = self.current_block();
        self.jump(loop_.header);
        self.add_phi_incoming(loop_.index, backedge, next);
        self.switch_to_block(loop_.exit);
    }

    /// Emits `for index in 0..length { body(index) }`.
    pub(crate) fn counted_loop(&mut self, length: ValueId, body: impl FnOnce(&mut Self, ValueId)) {
        let loop_ = self.begin_counted_loop(length);
        body(self, loop_.index());
        self.finish_counted_loop(loop_);
    }

    /// Adds an argument to the function.
    pub(crate) fn add_param(&mut self, ty: MirType) -> ValueId {
        self.func.alloc_param(ty)
    }

    /// Sets the function's single return type.
    pub(crate) fn set_return_type(&mut self, ty: MirType) {
        self.func.set_return_type(ty);
    }

    /// Creates an i256 immediate value.
    pub(crate) fn imm(&mut self, value: impl ToUint) -> ValueId {
        self.alloc_value(Value::Immediate(Immediate::I256(value.to_uint())))
    }

    /// Creates a boolean immediate.
    pub(crate) fn imm_bool(&mut self, value: bool) -> ValueId {
        self.alloc_value(Value::Immediate(Immediate::I1(value)))
    }

    /// Adds a constant byte offset, folding zero offsets.
    pub(crate) fn add_u64_offset(&mut self, base: ValueId, offset: u64) -> ValueId {
        if offset == 0 {
            base
        } else {
            let offset = self.imm(offset);
            self.add(base, offset)
        }
    }

    /// Reverts with Solidity's `Panic(uint256)` payload.
    pub(crate) fn panic(&mut self, code: PanicCode) {
        if self.semantic {
            // panic_if true, code; unreachable
            let condition = self.imm_bool(true);
            self.panic_if(condition, code);
            self.invalid();
            return;
        }
        let code = self.imm(code.as_u64());
        self.revert_scratch(PANIC_SELECTOR, &[code]);
    }

    /// Reverts with the left-aligned `selector` and its ABI `words` staged in the reserved low
    /// memory: `mstore(32 * (i + 1), word_i)`, `mstore(0, selector >> 224)`, then
    /// `revert(28, 4 + 32 * words.len())`.
    ///
    /// The selector goes last so every payload ends in the same selector store and revert, and
    /// tail merging reduces a panic site to its code and a jump.
    pub(crate) fn revert_scratch(&mut self, selector: U256, words: &[ValueId]) {
        // mstore(32 * (i + 1), word_i)
        for (index, &word) in words.iter().enumerate() {
            let offset = self.imm(32 * (index as u64 + 1));
            self.store(offset, word);
        }
        // mstore(0, selector >> 224)
        let selector = self.imm(selector >> 224);
        let zero = self.imm(U256::ZERO);
        self.store(zero, selector);
        // revert(28, 4 + 32 * words.len())
        let offset = self.imm(28);
        let size = self.imm(4 + 32 * words.len() as u64);
        self.revert(offset, size);
    }

    /// Retains evaluated error arguments until conditional payload encoding.
    pub(crate) fn require(&mut self, condition: ValueId, payload: RevertPayload) {
        // icall require, condition, payload
        self.emit_void_inst(InstKind::require(condition, payload));
    }

    /// Reverts with Solidity's `Panic(uint256)` payload when `condition` is true.
    pub(crate) fn panic_if(&mut self, condition: ValueId, code: PanicCode) {
        self.branch_to_revert(condition, false, RevertKind::Panic(code));
    }

    /// Reverts with Solidity's `Panic(uint256)` payload when `condition` is zero.
    pub(crate) fn panic_if_zero(&mut self, condition: ValueId, code: PanicCode) {
        self.branch_to_revert(condition, true, RevertKind::Panic(code));
    }

    /// Reverts for `reason` when `condition` is true.
    ///
    /// The data is empty unless the builder encodes revert reasons; see
    /// [`Self::with_revert_strings`].
    pub(crate) fn revert_if(&mut self, condition: ValueId, reason: RevertReason) -> BlockId {
        self.branch_to_revert(condition, false, RevertKind::Reason(reason))
    }

    /// Reverts for `reason` when `condition` is zero.
    pub(crate) fn revert_if_zero(&mut self, condition: ValueId, reason: RevertReason) -> BlockId {
        self.branch_to_revert(condition, true, RevertKind::Reason(reason))
    }

    /// Reverts unless the offset of a nested calldata value from `base` leaves `tail_size` bytes
    /// before `calldata_end` by a signed compare.
    ///
    /// A negative offset may therefore wrap `base + offset` to a valid position, and later reads
    /// past `calldata_end` see zeros. `tail_size` is the value's length word, or the whole head of
    /// a statically sized aggregate. These are the checks of solc's `access_calldata_tail` and
    /// the ABI encoder's `calldata_access`; `tail` only picks the debug messages.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/codegen/YulUtilFunctions.cpp#L2542-L2569>
    /// and <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/codegen/ABIFunctions.cpp#L1435-L1475>
    pub(crate) fn check_calldata_tail_offset(
        &mut self,
        tail: CalldataTail,
        base: ValueId,
        offset: ValueId,
        calldata_end: ValueId,
        tail_size: u64,
    ) {
        // if !(offset <s calldata_end - base - (tail_size - 1)) { revert }
        let available = self.sub(calldata_end, base);
        let slack = self.imm(tail_size - 1);
        let bound = self.sub(available, slack);
        let valid = self.slt(offset, bound);
        let invalid = self.eq_zero(valid);
        self.revert_if(invalid, tail.offset_reason());
    }

    /// Returns the data address `start + data_offset` of a dynamically sized calldata value,
    /// reverting unless `length` fits in 64 bits and `length` elements of `stride` bytes from
    /// there end before `calldata_end`, with the signed bound of
    /// [`Self::check_calldata_tail_offset`].
    pub(crate) fn calldata_tail_data(
        &mut self,
        tail: CalldataTail,
        start: ValueId,
        data_offset: u64,
        length: ValueId,
        calldata_end: ValueId,
        stride: u64,
    ) -> ValueId {
        // if length > 0xffffffffffffffff { revert }
        // data = start + data_offset
        // if data >s calldata_end - length * stride { revert }
        let too_long = self.exceeds_bits(length, 64, ConditionUse::BRANCH);
        self.revert_if(too_long, tail.length_reason());
        let size = if stride == 1 {
            length
        } else {
            let stride = self.imm(stride);
            self.mul(length, stride)
        };
        let data = self.add_u64_offset(start, data_offset);
        let limit = self.sub(calldata_end, size);
        let short = self.sgt(data, limit);
        self.revert_if(short, tail.stride_reason());
        data
    }

    /// Terminates the current block by reverting for `reason`.
    pub(crate) fn revert_with(&mut self, reason: RevertReason) {
        if self.semantic {
            // revert_if true, reason; unreachable
            let condition = self.imm_bool(true);
            self.revert_if(condition, reason);
            self.invalid();
            return;
        }
        match reason.message() {
            Some(message) if self.encodes_revert_reasons() => self.revert_error_string(message),
            _ => {
                // revert(0, 0)
                let zero = self.imm(0);
                self.revert(zero, zero);
            }
        }
    }

    /// Reverts with `abi_encode(Error(string), message)` for a constant `message`.
    fn revert_error_string(&mut self, message: &str) {
        // words = [32, len, chunk_0, ..., chunk_n]
        let mut words = vec![self.imm(32), self.imm(message.len() as u64)];
        for chunk in message.as_bytes().chunks(32) {
            let mut word = [0u8; 32];
            word[..chunk.len()].copy_from_slice(chunk);
            words.push(self.imm(U256::from_be_bytes(word)));
        }
        self.revert_scratch(ERROR_SELECTOR, &words);
    }

    pub(crate) fn branch_to_revert(
        &mut self,
        condition: ValueId,
        condition_is_zero: bool,
        kind: RevertKind,
    ) -> BlockId {
        if self.semantic {
            // icall check<polarity, failure>, condition
            self.emit_void_inst(InstKind::builtin(
                super::Builtin::Check { is_zero: condition_is_zero, failure: kind },
                [condition],
            ));
            return self.current_block();
        }
        // branch condition, failure, continuation
        // failure: revert payload
        let (revert, new_revert) = self.revert_block(kind);
        let continue_block = self.create_block();
        if condition_is_zero {
            self.branch(condition, continue_block, revert);
        } else {
            self.branch(condition, revert, continue_block);
        }
        if new_revert {
            self.switch_to_block(revert);
            match kind {
                RevertKind::Panic(code) => self.panic(code),
                RevertKind::Reason(reason) => self.revert_with(reason),
            }
        }
        self.switch_to_block(continue_block);
        continue_block
    }

    fn revert_block(&mut self, kind: RevertKind) -> (BlockId, bool) {
        // Without encoded reasons every reason reverts with empty data, so share one block.
        let kind = match kind {
            RevertKind::Reason(reason)
                if !self.encodes_revert_reasons() || reason.message().is_none() =>
            {
                RevertKind::Reason(RevertReason::Empty)
            }
            kind => kind,
        };
        if let Some(&block) = self.revert_blocks.0.get(&kind) {
            // Shared revert body !metadata(union of all failed-check origins)
            for index in 0..self.func.blocks[block].instructions.len() {
                let inst = self.func.blocks[block].instructions[index];
                self.func.inst_mut(inst).metadata.merge_debug_context(&self.current_debug_context);
            }
            self.func.blocks[block]
                .terminator_metadata
                .merge_debug_context(&self.current_debug_context);
            return (block, false);
        }
        let block = self.create_block();
        self.revert_blocks.0.insert(kind, block);
        (block, true)
    }

    /// Reverts with `PanicCode::EnumConversion` when `value` is not a valid variant index.
    pub(crate) fn validate_enum_value(&mut self, variants: u64, value: ValueId) {
        let limit = self.imm(variants);
        let valid = self.lt(value, limit);
        let invalid = self.eq_zero(valid);
        self.panic_if(invalid, PanicCode::EnumConversion);
    }

    /// Reverts with `PanicCode::ArrayOutOfBounds` when `index` is not below `length`.
    pub(crate) fn bounds_check(&mut self, index: ValueId, length: ValueId) {
        let in_range = self.lt(index, length);
        let invalid = self.eq_zero(in_range);
        self.panic_if(invalid, PanicCode::ArrayOutOfBounds);
    }

    /// Adds two words and reverts when the result overflows.
    pub(crate) fn checked_add(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        if let (Some(lhs), Some(rhs)) = (self.func.value_u256(lhs), self.func.value_u256(rhs)) {
            if let Some(result) = lhs.checked_add(rhs) {
                return self.imm(result);
            }

            // branch true, panic 0x41
            let overflow = self.imm_bool(true);
            self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
            return self.imm(lhs.wrapping_add(rhs));
        }
        let result = self.add(lhs, rhs);
        let overflow = self.lt(result, lhs);
        self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        result
    }

    /// Multiplies two words and reverts when the result overflows.
    pub(crate) fn checked_mul(&mut self, lhs: ValueId, rhs: ValueId) -> ValueId {
        if let (Some(lhs), Some(rhs)) = (self.func.value_u256(lhs), self.func.value_u256(rhs)) {
            if let Some(result) = lhs.checked_mul(rhs) {
                return self.imm(result);
            }

            // branch true, panic 0x41
            let overflow = self.imm_bool(true);
            self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
            return self.imm(lhs.wrapping_mul(rhs));
        }
        let result = self.mul(lhs, rhs);
        let rhs_zero = self.eq_zero(rhs);
        let quotient = self.div(result, rhs);
        let exact = self.eq(quotient, lhs);
        let valid = self.or(rhs_zero, exact);
        let overflow = self.eq_zero(valid);
        self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        result
    }

    /// Returns the word-aligned allocation size for a bytes-like object.
    pub(crate) fn checked_padded_size(&mut self, length: ValueId) -> ValueId {
        let padding = self.imm(63);
        let rounded = self.checked_add(length, padding);
        self.mask_padded_size(rounded)
    }

    /// Rounds `rounded` down to a multiple of 32.
    pub(crate) fn mask_padded_size(&mut self, rounded: ValueId) -> ValueId {
        let mask = self.imm(31);
        let mask = self.not(mask);
        self.and(rounded, mask)
    }

    /// Creates an undefined value.
    pub(crate) fn undef(&mut self, ty: MirType) -> ValueId {
        self.alloc_value(Value::Undef(ty))
    }

    /// Creates an error sentinel value for an already-reported lowering error.
    pub(crate) fn error_value(
        &mut self,
        guar: solar_interface::diagnostics::ErrorGuaranteed,
    ) -> ValueId {
        self.alloc_value(Value::Error(guar))
    }

    /// Allocates a fully constructed value.
    pub(crate) fn alloc_value(&mut self, value: Value) -> ValueId {
        self.func.alloc_value(value)
    }

    fn make_inst(&self, kind: InstKind, result_ty: Option<MirType>) -> Instruction {
        let mut inst = Instruction::new(kind, result_ty);
        inst.metadata.set_effect(Some(inst.kind.effect_kind()));
        inst.metadata.set_memory_region(self.memory_region_for_inst(&inst.kind));
        inst.metadata.set_storage_alias(self.storage_alias_for_inst(&inst.kind));
        self.set_current_debug_context(&mut inst.metadata);
        inst
    }

    /// Appends a fully constructed instruction to the current block.
    pub(crate) fn append_instruction(&mut self, inst: Instruction) -> (InstId, Option<ValueId>) {
        let (inst_id, result) = if inst.result_ty.is_some() {
            let (inst_id, result) = self.func.alloc_value_inst(inst);
            (inst_id, Some(result))
        } else {
            (self.func.alloc_inst(inst), None)
        };
        self.func.blocks[self.current_block].instructions.push(inst_id);
        (inst_id, result)
    }

    /// Appends a fully constructed instruction for a preallocated undefined result value.
    pub(crate) fn append_instruction_with_result(
        &mut self,
        inst: Instruction,
        result: ValueId,
    ) -> InstId {
        let inst_id = self.func.alloc_inst_with_result(inst, result);
        self.func.blocks[self.current_block].instructions.push(inst_id);
        inst_id
    }

    fn cast_operands(&mut self, kind: &mut InstKind) {
        if matches!(
            kind,
            InstKind::Eq(..)
                | InstKind::Ne(..)
                | InstKind::Lt(..)
                | InstKind::Gt(..)
                | InstKind::SLt(..)
                | InstKind::SGt(..)
        ) && kind
            .operands()
            .windows(2)
            .any(|pair| self.func.value_ty(pair[0]) != self.func.value_ty(pair[1]))
        {
            let operands = kind.operands();
            let ty = if matches!(
                kind,
                InstKind::Eq(..) | InstKind::Ne(..) | InstKind::Lt(..) | InstKind::Gt(..)
            ) {
                operands
                    .iter()
                    .find_map(|&value| {
                        self.func
                            .value_ty(value)
                            .filter(|ty| ty.integer_bits().is_some_and(|bits| bits < 256))
                    })
                    .filter(|&ty| {
                        operands.iter().all(|&value| {
                            self.func.value_ty(value) == Some(ty)
                                || self.func.value_u256(value).is_some_and(|word| {
                                    word.bit_len() <= ty.integer_bits().unwrap() as usize
                                })
                        })
                    })
                    .unwrap_or(MirType::I256)
            } else {
                MirType::I256
            };
            kind.visit_operands_mut(|value| *value = self.cast(*value, ty));
        }
        let Some(types) = kind.operand_types(self.func) else { return };
        let mut types = types.into_iter();
        // operand = cast operand to its declared parameter type
        kind.visit_operands_mut(|value| {
            if let Some(ty) = types.next() {
                *value = self.cast(*value, ty);
            }
        });
    }

    /// Emits a typed value-producing instruction with the current source and effect metadata.
    pub(crate) fn emit_inst(&mut self, mut kind: InstKind, result_ty: Option<MirType>) -> ValueId {
        debug_assert!(result_ty.is_some(), "value-producing instructions must have a result type");
        let requested = result_ty.unwrap();
        let produced = if kind.op_def().result == super::ResultKind::Integer {
            let boolean_bitwise =
                matches!(kind, InstKind::And(..) | InstKind::Or(..) | InstKind::Xor(..))
                    && kind
                        .operands()
                        .iter()
                        .all(|&value| self.func.value_ty(value) == Some(MirType::I1));
            let ty = if boolean_bitwise {
                MirType::I1
            } else if requested.integer_bits().is_some() {
                requested
            } else {
                MirType::I256
            };
            kind.visit_operands_mut(|value| {
                *value = if ty == MirType::I1 && self.func.value_ty(*value) != Some(MirType::I1) {
                    let word = self.cast_word(*value);
                    self.emit_inst(InstKind::Trunc(word, 1), Some(MirType::I1))
                } else {
                    self.cast(*value, ty)
                };
            });
            ty
        } else {
            self.cast_operands(&mut kind);
            let result = kind.op_def().result;
            if result.admits_type(requested) {
                requested
            } else {
                kind.inferred_result_type(self.func).unwrap_or(requested)
            }
        };
        if let InstKind::Phi(incoming) = &mut kind {
            let current = self.current_block;
            for (predecessor, value) in incoming {
                self.switch_to_block(*predecessor);
                // incoming = cast value to the phi type
                *value = self.cast(*value, requested);
            }
            self.switch_to_block(current);
        }

        // result = op operands
        // requested = cast result
        let inst = self.make_inst(kind, Some(produced));
        let result = self
            .append_instruction(inst)
            .1
            .expect("value-producing instruction must have a result");
        if produced == requested { result } else { self.cast(result, requested) }
    }

    /// Emits an explicit conversion between scalar carriers and memory references.
    pub(crate) fn cast(&mut self, value: ValueId, ty: MirType) -> ValueId {
        if self.func.value_ty(value) == Some(ty) {
            return value;
        }
        let Some(from) = self.func.value_ty(value) else {
            assert!(matches!(self.func.value(value), Value::Error(_)), "cast operand has no type");
            return value;
        };
        if let Some(word) = self.func.value_u256(value) {
            let word = match ty {
                MirType::I1 => Some(U256::from(!word.is_zero())),
                MirType::Int(bits) if bits.get() <= 256 => {
                    Some(word & (U256::MAX >> (256 - bits.get())))
                }
                ty if ty.is_pointer() => Some(word),
                _ => None,
            };
            if let Some(word) = word {
                return self
                    .func
                    .alloc_value(Value::Immediate(Immediate::for_type(Some(ty), word)));
            }
        }
        if let Value::Inst(id) = self.func.value(value) {
            let original = match self.func.inst(*id).kind {
                InstKind::Zext(inner) => Some(inner),
                InstKind::PtrToInt(inner, 256) if ty.is_pointer() => Some(inner),
                InstKind::IntToPtr(inner) if ty == MirType::I256 => Some(inner),
                _ => None,
            };
            if let Some(original) = original
                && self.func.value_ty(original) == Some(ty)
            {
                return original;
            }
        }
        let kind = match (from, ty) {
            // boolean = ne word, 0
            (_, MirType::I1) => {
                let value = if from.integer_bits().is_some() {
                    value
                } else {
                    self.cast(value, MirType::I256)
                };
                let zero = self.func.alloc_value(Value::Immediate(Immediate::for_type(
                    self.func.value_ty(value),
                    U256::ZERO,
                )));
                InstKind::Ne(value, zero)
            }
            // narrow = trunc integer to destination
            (MirType::Int(from), MirType::Int(to)) if from > to => InstKind::Trunc(value, to.get()),
            // wide = zext integer to destination
            (MirType::Int(_), MirType::Int(_)) => InstKind::Zext(value),
            // integer = ptrtoint pointer to destination
            (from, MirType::Int(to)) if from.is_pointer() => InstKind::PtrToInt(value, to.get()),
            // pointer = inttoptr integer to destination
            (MirType::Int(_), to) if to.is_pointer() => InstKind::IntToPtr(value),
            _ => panic!("cannot cast MIR value from `{from}` to `{ty}`"),
        };
        let inst = self.make_inst(kind, Some(ty));
        self.append_instruction(inst).1.unwrap()
    }

    /// Emits an instruction that produces no value, such as a store or a log.
    ///
    /// No result [`Value`] is allocated: only value-producing instructions get
    /// an entry in the function's value table.
    pub(in crate::mir) fn emit_void_inst(&mut self, mut kind: InstKind) {
        self.cast_operands(&mut kind);
        let inst = self.make_inst(kind, None);
        self.append_instruction(inst);
    }

    /// Emits a void memory instruction with a proven destination region.
    fn emit_void_inst_in_region(&mut self, mut kind: InstKind, region: MemoryRegion) -> InstId {
        self.cast_operands(&mut kind);
        let mut inst = self.make_inst(kind, None);
        inst.metadata.set_memory_region(Some(region));
        self.append_instruction(inst).0
    }

    fn memory_region_for_inst(&self, kind: &InstKind) -> Option<MemoryRegion> {
        if let InstKind::FrameLoad { mode, .. } | InstKind::FrameStore { mode, .. } = *kind {
            return Some(match mode {
                FrameMode::External => MemoryRegion::Scratch,
                FrameMode::Internal => MemoryRegion::InternalFrame,
                FrameMode::MultiReturn => MemoryRegion::Scratch,
            });
        }
        let addr = match *kind {
            InstKind::Load(addr, PtrLocation::Memory)
            | InstKind::Store(addr, _, PtrLocation::Memory)
            | InstKind::Store8(addr, _)
            | InstKind::Keccak256(addr, _) => addr,
            InstKind::Copy(dest, ..)
            | InstKind::CodeCopy(dest, _, _)
            | InstKind::ExtCodeCopy(_, dest, _, _) => dest,
            _ => return None,
        };
        Some(self.memory_region_for_addr(addr))
    }

    fn memory_region_for_addr(&self, addr: ValueId) -> MemoryRegion {
        match self.func.value(addr) {
            Value::Immediate(imm)
                if imm
                    .as_u256()
                    .is_some_and(|value| value < U256::from(EvmMemoryLayout::HEAP_START)) =>
            {
                MemoryRegion::Scratch
            }
            Value::Inst(inst_id) => match self.func.inst(*inst_id).kind {
                InstKind::InternalFrameAddr(_) => MemoryRegion::InternalFrame,
                InstKind::Add(lhs, rhs) if self.is_internal_frame_add(lhs, rhs) => {
                    MemoryRegion::InternalFrame
                }
                InstKind::Sub(lhs, rhs)
                    if self.is_internal_frame_addr(lhs) && self.is_immediate(rhs) =>
                {
                    MemoryRegion::InternalFrame
                }
                _ => MemoryRegion::Unknown,
            },
            Value::Arg(_) | Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => {
                MemoryRegion::Unknown
            }
        }
    }

    fn is_internal_frame_add(&self, lhs: ValueId, rhs: ValueId) -> bool {
        (self.is_internal_frame_addr(lhs) && self.is_immediate(rhs))
            || (self.is_internal_frame_addr(rhs) && self.is_immediate(lhs))
    }

    fn is_internal_frame_addr(&self, value: ValueId) -> bool {
        matches!(
            self.func.value(value),
            Value::Inst(inst_id)
                if matches!(self.func.inst(*inst_id).kind, InstKind::InternalFrameAddr(_))
        )
    }

    fn is_immediate(&self, value: ValueId) -> bool {
        matches!(self.func.value(value), Value::Immediate(_))
    }

    fn storage_alias_for_inst(&self, kind: &InstKind) -> Option<StorageAlias> {
        match *kind {
            InstKind::Load(slot, PtrLocation::Storage)
            | InstKind::Store(slot, _, PtrLocation::Storage) => Some(self.storage_alias(slot)),
            _ => None,
        }
    }

    fn storage_alias(&self, slot: ValueId) -> StorageAlias {
        StorageAlias::for_value(self.func, slot)
    }

    /// Reserves memory under an explicit semantic policy.
    pub(crate) fn alloc(&mut self, size: ValueId, semantics: AllocationSemantics) -> ValueId {
        self.emit_inst(InstKind::Alloc { size, semantics }, Some(MirType::MEMPTR))
    }

    /// Allocates a padded bytes object and records its logical length.
    pub(crate) fn alloc_bytes_object(
        &mut self,
        length: ValueId,
        semantics: AllocationSemantics,
    ) -> ValueId {
        let size = self.checked_padded_size(length);
        let object = self.alloc(size, semantics);
        self.set_memory_len(object, length);
        object
    }

    /// Allocates a bytes object whose `length` is known to be far below `2^256 - 63`, such as
    /// the length of calldata, so its padded size needs no overflow check.
    pub(crate) fn alloc_bounded_bytes_object(
        &mut self,
        length: ValueId,
        semantics: AllocationSemantics,
    ) -> ValueId {
        // size = (length + 63) & ~31
        let padding = self.imm(63);
        let rounded = self.add(length, padding);
        let size = self.mask_padded_size(rounded);
        let object = self.alloc(size, semantics);
        self.set_memory_len(object, length);
        object
    }

    /// Allocates a fixed array whose elements each occupy one memory word.
    pub(crate) fn alloc_word_array(
        &mut self,
        len: u64,
        semantics: AllocationSemantics,
    ) -> (ValueId, MemoryObjectLayout) {
        let size = self.imm(len.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        let layout = MemoryObjectLayout::word_fixed_array(len);
        let object = self.alloc(size, semantics);
        (object, layout)
    }

    /// Allocates a struct whose fields each occupy one memory word.
    pub(crate) fn alloc_word_struct(
        &mut self,
        fields: u64,
        semantics: AllocationSemantics,
    ) -> ValueId {
        let size = self.imm(fields.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        self.alloc(size, semantics)
    }

    /// Allocates a dynamic array whose elements each occupy one memory word.
    pub(crate) fn alloc_dynamic_word_array(
        &mut self,
        length: ValueId,
        semantics: AllocationSemantics,
    ) -> (ValueId, MemoryObjectLayout) {
        let one = self.imm(1);
        let word_size = self.imm(EvmMemoryLayout::WORD_SIZE);
        let size = if semantics.failure == AllocationFailure::Checked {
            // panic_if length >> 59 != 0, 0x41
            // size = (length + 1) * WORD_SIZE
            //
            // A length of `2^59 - 1` passes, but its size `2^64` ends past the allocation limit,
            // which the checked allocation panics on with the same code.
            let invalid = self.exceeds_bits(length, ALLOCATION_LENGTH_BITS, ConditionUse::BRANCH);
            self.panic_if(invalid, PanicCode::MemoryAllocationOverflow);
            let words = self.add(length, one);
            self.mul(words, word_size)
        } else {
            let words = self.checked_add(length, one);
            self.checked_mul(words, word_size)
        };
        let layout = MemoryObjectLayout::WORD_ARRAY;
        let object = self.alloc(size, semantics);
        self.set_memory_len(object, length);
        (object, layout)
    }

    /// Builds a memory slice of a dynamic memory object's payload, reading its length word.
    pub(crate) fn memory_slice(&mut self, object: ValueId) -> ValueId {
        // slice = make_memory_slice (memory_data object), mload object
        let data = self.memory_data(object);
        let len = self.load(object, PtrLocation::Memory);
        self.make_slice(data, len, PtrLocation::Memory)
    }

    /// Returns a slice of a dynamic value: slices as they are, memory objects through their
    /// header.
    pub(crate) fn memory_view(&mut self, value: ValueId) -> ValueId {
        if matches!(self.func.value_ty(value), Some(MirType::Slice(_))) {
            value
        } else {
            self.memory_slice(value)
        }
    }

    /// Reads the length of a dynamic memory object or slice.
    pub(crate) fn memory_len(&mut self, value: ValueId) -> ValueId {
        if matches!(self.func.value_ty(value), Some(MirType::Slice(_))) {
            return self.slice_len(value);
        }
        // len = mload object
        self.load(value, PtrLocation::Memory)
    }

    /// Writes the length word of a fresh dynamic memory object.
    pub(crate) fn set_memory_len(&mut self, object: ValueId, len: ValueId) {
        // mstore object, len
        self.store(object, len);
    }

    /// Returns the payload address of a dynamic memory object or memory slice.
    pub(crate) fn memory_data(&mut self, value: ValueId) -> ValueId {
        self.memory_data_at(value, 0)
    }

    /// Returns the address `offset` bytes into the payload of a dynamic memory object or memory
    /// slice.
    pub(crate) fn memory_data_at(&mut self, value: ValueId, offset: u64) -> ValueId {
        if matches!(self.func.value_ty(value), Some(MirType::Slice(_))) {
            // data = ptradd (extract_value value, 0), offset
            let data = self.slice_ptr(value);
            return self.address_value(data, offset);
        }
        // data = ptradd value, DYNAMIC_HEADER_SIZE + offset
        self.address_value(value, EvmMemoryLayout::DYNAMIC_HEADER_SIZE.saturating_add(offset))
    }

    /// Hashes the bytes of a dynamic memory object or slice. Calldata and returndata bytes are
    /// first copied to free memory.
    pub(crate) fn keccak256_bytes(&mut self, value: ValueId) -> ValueId {
        let (data, len) = match self.func.value_slice_location(value) {
            Some(location @ (PtrLocation::Calldata | PtrLocation::Returndata)) => {
                // data = fmp
                // mcopy|calldatacopy|returndatacopy data, (extract_value value, 0), (extract_value
                // value, 1)
                let source = self.slice_ptr(value);
                let len = self.slice_len(value);
                let data = self.fmp();
                self.copy_heap(data, source, len, location);
                (data, len)
            }
            _ => (self.memory_data(value), self.memory_len(value)),
        };
        // hash = keccak256 data, len
        self.keccak256(data, len)
    }

    /// Loads a direct struct field.
    pub(crate) fn memory_load_field(&mut self, object: ValueId, field: u64) -> ValueId {
        self.memory_load_field_as(object, field, MirType::I256)
    }

    /// Loads a struct field as a `ptr<memory>` when `ty` is one, and as a word otherwise.
    pub(crate) fn memory_load_field_as(
        &mut self,
        object: ValueId,
        field: u64,
        ty: MirType,
    ) -> ValueId {
        let offset = self.imm(field.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        let (address, location) = self.object_address(object, 0, offset);
        self.load_as(address, location, ty)
    }

    /// Stores a direct struct field.
    pub(crate) fn memory_store_field(&mut self, object: ValueId, field: u64, value: ValueId) {
        let offset = self.imm(field.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        let (address, _) = self.object_address(object, 0, offset);
        self.store(address, value);
    }

    /// Loads an array element.
    pub(crate) fn memory_load_element(
        &mut self,
        object: ValueId,
        layout: MemoryObjectLayout,
        index: ValueId,
    ) -> ValueId {
        self.memory_load_element_as(object, layout, index, MirType::I256)
    }

    /// Loads an array element as a `ptr<memory>` when `ty` is one, and as a word otherwise.
    pub(crate) fn memory_load_element_as(
        &mut self,
        object: ValueId,
        layout: MemoryObjectLayout,
        index: ValueId,
        ty: MirType,
    ) -> ValueId {
        let offset = self.word_offset(index);
        let (address, location) = self.object_address(object, Self::header_size(layout), offset);
        self.load_as(address, location, ty)
    }

    /// Loads one byte from a bytes object or slice.
    pub(crate) fn memory_load_byte(&mut self, value: ValueId, index: ValueId) -> ValueId {
        // byte = byte 0, (mload|calldataload|returndataload (data + index))
        let (address, location) =
            self.object_address(value, EvmMemoryLayout::DYNAMIC_HEADER_SIZE, index);
        let word = self.load(address, location);
        let zero = self.imm(0);
        self.byte(zero, word)
    }

    /// Stores an array element.
    pub(crate) fn memory_store_element(
        &mut self,
        object: ValueId,
        layout: MemoryObjectLayout,
        index: ValueId,
        value: ValueId,
    ) {
        let offset = self.word_offset(index);
        let (address, _) = self.object_address(object, Self::header_size(layout), offset);
        self.store(address, value);
    }

    /// Returns the byte offset of word `index`, folded like [`Self::cast`] for an immediate.
    fn word_offset(&mut self, index: ValueId) -> ValueId {
        if let Some(index) = self.func.value_u64(index) {
            return self.imm(index.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        }
        // offset = shl index, 5
        let shift = self.imm(u64::from(EvmMemoryLayout::WORD_SIZE.trailing_zeros()));
        self.shl(index, shift)
    }

    /// Returns the header bytes before a memory object's words: none for a fixed array or
    /// struct, the length word for a dynamic object.
    fn header_size(layout: MemoryObjectLayout) -> u64 {
        match layout {
            MemoryObjectLayout::Bytes | MemoryObjectLayout::DynamicArray { .. } => {
                EvmMemoryLayout::DYNAMIC_HEADER_SIZE
            }
            MemoryObjectLayout::FixedArray { .. } | MemoryObjectLayout::Struct { .. } => 0,
        }
    }

    /// Returns the address `offset` bytes past `header` in a memory object, or `offset` bytes into
    /// a slice's data, with the location it reads. An object address stays a pointer, folding a
    /// constant offset into the header.
    fn object_address(
        &mut self,
        value: ValueId,
        header: u64,
        offset: ValueId,
    ) -> (ValueId, PtrLocation) {
        if let Some(location) = self.func.value_slice_location(value) {
            let data = self.slice_ptr(value);
            if let Some(offset) = self.func.value_u64(offset) {
                return (self.address_value(data, offset), location);
            }
            // address = add (extract_value value, 0), offset
            return (self.add(data, offset), location);
        }
        if let Some(offset) = self.func.value_u64(offset) {
            return (self.address_value(value, header.saturating_add(offset)), PtrLocation::Memory);
        }
        // address = ptradd (ptradd object, offset), header
        let element = self.ptradd(value, offset);
        (self.address_value(element, header), PtrLocation::Memory)
    }

    /// Returns `base + offset` as one address value: a `ptradd` of a pointer, an `add` of a word.
    pub(crate) fn address_value(&mut self, base: ValueId, offset: u64) -> ValueId {
        if offset == 0 || !self.func.value_ty(base).is_some_and(MirType::is_pointer) {
            // address = add base, offset
            return self.add_u64_offset(base, offset);
        }
        // address = ptradd base, offset
        let offset = self.imm(offset);
        self.ptradd(base, offset)
    }

    /// Loads a word as a pointer when `ty` is one, and as a word otherwise.
    pub(crate) fn load_as(
        &mut self,
        address: ValueId,
        location: PtrLocation,
        ty: MirType,
    ) -> ValueId {
        let result = if ty == MirType::MEMPTR { ty } else { MirType::I256 };
        let address = self.access_address(address);
        // result = mload|calldataload|returndataload address
        self.emit_inst(InstKind::Load(address, location), Some(result))
    }

    /// Returns the pointer an access address casts to a word, so the access takes the pointer.
    fn access_address(&self, address: ValueId) -> ValueId {
        // mload (ptrtoint pointer to i256) -> mload pointer
        if let Value::Inst(inst) = self.func.value(address)
            && let InstKind::PtrToInt(pointer, 256) = self.func.inst(*inst).kind
        {
            return pointer;
        }
        address
    }

    /// Offsets a pointer by `offset` bytes; a raw word offsets as a `ptr<memory>`.
    pub(crate) fn ptradd(&mut self, ptr: ValueId, offset: ValueId) -> ValueId {
        // result = ptradd ptr, offset
        let ty = crate::mir::typing::pointer(self.func, ptr);
        self.emit_inst(InstKind::PtrAdd(ptr, offset), Some(ty))
    }

    /// Stores one byte in a bytes object.
    pub(crate) fn memory_store_byte(&mut self, object: ValueId, index: ValueId, value: ValueId) {
        let (address, _) = self.object_address(object, EvmMemoryLayout::DYNAMIC_HEADER_SIZE, index);
        self.store8(address, value);
    }

    /// Stores one word at a byte offset in a bytes object.
    pub(crate) fn memory_store_word(&mut self, object: ValueId, offset: ValueId, value: ValueId) {
        let (address, _) =
            self.object_address(object, EvmMemoryLayout::DYNAMIC_HEADER_SIZE, offset);
        self.store(address, value);
    }

    /// Loads one word at a byte offset from a memory or calldata slice.
    pub(crate) fn slice_load_word(&mut self, slice: ValueId, offset: ValueId) -> ValueId {
        let (address, location) =
            self.object_address(slice, EvmMemoryLayout::DYNAMIC_HEADER_SIZE, offset);
        self.load(address, location)
    }

    /// Copies a slice into a dynamic memory object's payload.
    pub(crate) fn memory_copy_from_slice(&mut self, object: ValueId, source: ValueId) {
        let offset = self.imm(0);
        self.memory_copy_from_slice_at(object, offset, source);
    }

    /// Copies a slice into a fixed-size memory object, which has no length word.
    pub(crate) fn memory_copy_to_object(&mut self, object: ValueId, source: ValueId) {
        self.copy_slice(object, source);
    }

    /// Copies a slice into a byte offset in a dynamic memory object's payload.
    pub(crate) fn memory_copy_from_slice_at(
        &mut self,
        object: ValueId,
        offset: ValueId,
        source: ValueId,
    ) {
        let (destination, _) =
            self.object_address(object, EvmMemoryLayout::DYNAMIC_HEADER_SIZE, offset);
        self.copy_slice(destination, source);
    }

    /// Copies a slice's bytes to `destination` in memory.
    fn copy_slice(&mut self, destination: ValueId, source: ValueId) {
        // mcopy|calldatacopy|returndatacopy destination, (extract_value source, 0), (extract_value
        // source, 1)
        let location = self.func.value_slice_location(source).unwrap_or(PtrLocation::Memory);
        let source_ptr = self.slice_ptr(source);
        let len = self.slice_len(source);
        self.copy(destination, source_ptr, len, location);
    }

    /// ABI-encodes `args` into a freshly allocated memory slice.
    pub(crate) fn abi_encode(
        &mut self,
        layout: crate::mir::AbiLayoutRef,
        selector: Option<ValueId>,
        args: impl Into<Box<[ValueId]>>,
    ) -> ValueId {
        self.emit_abi_encode(layout, selector, args, AbiEncodeMode::Slice)
    }

    /// ABI-encodes `args` into a freshly allocated bytes object.
    pub(crate) fn abi_encode_bytes(
        &mut self,
        layout: crate::mir::AbiLayoutRef,
        selector: Option<ValueId>,
        args: impl Into<Box<[ValueId]>>,
    ) -> ValueId {
        self.emit_abi_encode(layout, selector, args, AbiEncodeMode::Bytes)
    }

    /// ABI-encodes `args` at the free-memory pointer without reserving the result.
    pub(crate) fn abi_encode_scratch(
        &mut self,
        layout: crate::mir::AbiLayoutRef,
        selector: Option<ValueId>,
        args: impl Into<Box<[ValueId]>>,
    ) -> ValueId {
        self.emit_abi_encode(layout, selector, args, AbiEncodeMode::Scratch)
    }

    fn emit_abi_encode(
        &mut self,
        layout: crate::mir::AbiLayoutRef,
        selector: Option<ValueId>,
        args: impl Into<Box<[ValueId]>>,
        mode: AbiEncodeMode,
    ) -> ValueId {
        self.emit_inst(
            InstKind::AbiEncode { mode, selector, args: args.into(), layout },
            Some(mode.result_type()),
        )
    }

    /// Replaces a field of a struct value.
    pub(crate) fn insert_value(
        &mut self,
        ty: StructId,
        aggregate: ValueId,
        index: u32,
        value: ValueId,
    ) -> ValueId {
        // result = insert_value aggregate, index, value
        let ty = MirType::Struct(ty);
        self.emit_inst(InstKind::InsertValue { ty, aggregate, index, value }, Some(ty))
    }

    /// Projects a field with its declared type from a struct value.
    pub(crate) fn extract_value(
        &mut self,
        ty: StructId,
        aggregate: ValueId,
        index: u32,
        field: MirType,
    ) -> ValueId {
        // result = extract_value aggregate, index
        let ty = MirType::Struct(ty);
        self.emit_inst(InstKind::ExtractValue { ty, aggregate, index }, Some(field))
    }

    /// Projects the pointer from a slice.
    pub(crate) fn slice_ptr(&mut self, slice: ValueId) -> ValueId {
        self.slice_field(slice, MirType::SLICE_PTR_FIELD)
    }

    /// Projects the length word from a slice.
    pub(crate) fn slice_len(&mut self, slice: ValueId) -> ValueId {
        self.slice_field(slice, MirType::SLICE_LEN_FIELD)
    }

    fn slice_field(&mut self, slice: ValueId, index: u32) -> ValueId {
        // result = extract_value slice_ty, slice, index
        let (kind, ty) = InstKind::slice_field(self.func, slice, index);
        self.emit_inst(kind, Some(ty))
    }

    /// Emits the test that `word` fits in `bits` bits, as a shift against zero or as a comparison
    /// with the limit, whichever [`Target::shifts_bits_test`] prices lower for how `read` uses it.
    pub(crate) fn fits_in_bits(&mut self, word: ValueId, bits: u32, read: ConditionUse) -> ValueId {
        self.bits_test(word, bits, true, read)
    }

    /// Emits the test that `word` has a bit at or above `bits`, the negation of
    /// [`Self::fits_in_bits`]. A branch or check reads it for free as a shifted word, so abort on
    /// this test rather than on the negation of a fit.
    pub(crate) fn exceeds_bits(&mut self, word: ValueId, bits: u32, read: ConditionUse) -> ValueId {
        self.bits_test(word, bits, false, read)
    }

    /// Emits `word < limit`, as [`Self::fits_in_bits`] when `limit` is a power of two.
    pub(crate) fn lt_literal(
        &mut self,
        word: ValueId,
        limit: impl ToUint,
        read: ConditionUse,
    ) -> ValueId {
        let limit = limit.to_uint();
        if limit > U256::ONE && limit.is_power_of_two() {
            return self.fits_in_bits(word, limit.trailing_zeros() as u32, read);
        }
        let limit = self.imm(limit);
        self.lt(word, limit)
    }

    /// Emits `word > limit`, as [`Self::exceeds_bits`] when `limit` is `2^k - 1`.
    pub(crate) fn gt_literal(
        &mut self,
        word: ValueId,
        limit: impl ToUint,
        read: ConditionUse,
    ) -> ValueId {
        let limit = limit.to_uint();
        if let Some(power) = limit.checked_add(U256::ONE)
            && power > U256::ONE
            && power.is_power_of_two()
        {
            return self.exceeds_bits(word, power.trailing_zeros() as u32, read);
        }
        let limit = self.imm(limit);
        self.gt(word, limit)
    }

    fn bits_test(&mut self, word: ValueId, bits: u32, fits: bool, read: ConditionUse) -> ValueId {
        debug_assert!((1..256).contains(&bits), "a width of {bits} bits is no test");
        if self.target.is_some_and(|target| target.shifts_bits_test(bits, fits, read)) {
            // high = shr word, bits
            // result = eq high, 0 | ne high, 0
            let word = if self.func.value_ty(word) == Some(MirType::I256) {
                word
            } else {
                self.cast_word(word)
            };
            let count = self.imm(bits);
            let high = self.shr(word, count);
            return if fits { self.eq_zero(high) } else { self.ne_zero(high) };
        }
        // result = lt word, 2^bits | gt word, 2^bits - 1
        if fits {
            let limit = self.imm(U256::ONE << bits);
            self.lt(word, limit)
        } else {
            let limit = self.imm((U256::ONE << bits) - U256::ONE);
            self.gt(word, limit)
        }
    }

    /// Compares a value with zero.
    pub(crate) fn eq_zero(&mut self, value: ValueId) -> ValueId {
        let zero = self.func.alloc_value(Value::Immediate(Immediate::for_type(
            self.func.value_ty(value),
            U256::ZERO,
        )));
        // result = eq value, 0
        self.eq(value, zero)
    }

    /// Tests a value for being nonzero.
    pub(crate) fn ne_zero(&mut self, value: ValueId) -> ValueId {
        let zero = self.func.alloc_value(Value::Immediate(Immediate::for_type(
            self.func.value_ty(value),
            U256::ZERO,
        )));
        // result = ne value, 0
        self.ne(value, zero)
    }

    /// Preserves all bits while forgetting a value's nominal one-word type.
    pub(crate) fn cast_word(&mut self, value: ValueId) -> ValueId {
        // word = zext integer or ptrtoint pointer to i256
        self.cast(value, MirType::I256)
    }

    /// Preserve a source ABI validation obligation until interface lowering discharges it.
    pub(crate) fn validate_abi(&mut self, value: ValueId) {
        // validate_abi value
        self.emit_void_inst(InstKind::ValidateAbi(value));
    }

    /// Stores a bytes object in storage, clearing any unused old data words.
    pub(crate) fn store_storage_bytes(&mut self, slot: ValueId, object: ValueId) {
        // store_storage_bytes slot, object
        self.emit_void_inst(InstKind::StorageBytesStore(slot, object));
    }

    /// Stores literal bytes in storage, clearing any unused old data words.
    pub(crate) fn store_storage_bytes_literal(&mut self, slot: ValueId, bytes: &[u8]) {
        // store_storage_bytes_literal slot, bytes
        self.emit_void_inst(InstKind::StorageBytesStoreLiteral {
            slot,
            bytes: Bytes::copy_from_slice(bytes),
        });
    }

    /// Clears the hashed data words of a storage array in `first..end`.
    pub(crate) fn clear_storage_words(&mut self, slot: ValueId, first: ValueId, end: ValueId) {
        // clear_storage_words slot, first, end
        self.emit_void_inst(InstKind::StorageClearWords(slot, first, end));
    }

    /// Validate a loaded Solidity storage bytes header.
    pub(crate) fn validate_storage_bytes(&mut self, header: ValueId) {
        // validate_storage_bytes header
        self.emit_void_inst(InstKind::ValidateStorageBytes(header));
    }

    /// Decodes the long-encoding flag and length without validating the header.
    pub(crate) fn storage_bytes_header_parts(&mut self, header: ValueId) -> (ValueId, ValueId) {
        // flag = header & 1
        // is_long = flag == 1
        // mask = 0x7f | (0 - flag)
        // length = (header >> 1) & mask
        let one = self.imm(1);
        let flag = self.and(header, one);
        let is_long = self.eq(flag, one);
        let zero = self.imm(0);
        let long_mask = self.sub(zero, flag);
        let short_mask = self.imm(0x7f);
        let mask = self.or(short_mask, long_mask);
        let half = self.shr(header, one);
        let length = self.and(half, mask);
        (is_long, length)
    }

    /// Keeps only the first `length` bytes of a storage `bytes` header word.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/codegen/YulUtilFunctions.cpp#L649>
    pub(crate) fn mask_storage_bytes_data(&mut self, data: ValueId, length: ValueId) -> ValueId {
        // mask = not((1 << (8 * (32 - length))) - 1)
        // masked = data & mask
        let word_size = self.imm(32);
        let unused_bytes = self.sub(word_size, length);
        let bits = self.imm(8);
        let shift = self.mul(unused_bytes, bits);
        let one = self.imm(1);
        let high_bit = self.shl(one, shift);
        let low_mask = self.sub(high_bit, one);
        let data_mask = self.not(low_mask);
        self.and(data, data_mask)
    }

    /// Builds the header word of a short storage `bytes` value of `length` bytes.
    ///
    /// Reference: <https://github.com/argotorg/solidity/blob/f401782df49be312ea4ef52a2d467cf5183b5906/libsolidity/codegen/YulUtilFunctions.cpp#L1620>
    pub(crate) fn short_storage_bytes_header(&mut self, data: ValueId, length: ValueId) -> ValueId {
        // header = mask(data, length) | length * 2
        let masked = self.mask_storage_bytes_data(data, length);
        let two = self.imm(2);
        let tag = self.mul(length, two);
        self.or(masked, tag)
    }

    /// Builds the header word of a long storage `bytes` value of `length` bytes.
    pub(crate) fn long_storage_bytes_header(&mut self, length: ValueId) -> ValueId {
        // header = length << 1 | 1
        let one = self.imm(1);
        let shifted = self.shl(length, one);
        self.or(shifted, one)
    }

    /// Builds a struct from its ordered field values.
    pub(crate) fn make_struct(
        &mut self,
        ty: StructId,
        values: impl IntoIterator<Item = ValueId>,
    ) -> ValueId {
        // result = undef struct
        // result = insert_value result, index, field
        let mut result = self.func.alloc_value(Value::Undef(MirType::Struct(ty)));
        for (index, value) in values.into_iter().enumerate() {
            result = self.insert_value(ty, result, index as u32, value);
        }
        result
    }

    /// Decodes an ABI tuple from a bytes object or a memory slice into semantic values.
    pub(crate) fn abi_decode(
        &mut self,
        layout: crate::mir::AbiParamLayoutRef,
        data: ValueId,
        result_ty: MirType,
    ) -> ValueId {
        // result = abi_decode data
        self.emit_inst(InstKind::AbiDecode { data, layout }, Some(result_ty))
    }

    /// Checks the length of the data for an empty ABI tuple, which decodes to no values.
    pub(crate) fn abi_decode_void(&mut self, layout: crate::mir::AbiParamLayoutRef, data: ValueId) {
        // abi_decode data
        self.emit_void_inst(InstKind::AbiDecode { data, layout });
    }

    /// Emits a word load from `location`.
    pub(crate) fn load(&mut self, ptr: ValueId, location: PtrLocation) -> ValueId {
        self.load_as(ptr, location, MirType::I256)
    }

    /// Emits a word store to memory.
    pub(crate) fn store(&mut self, ptr: ValueId, value: ValueId) {
        let ptr = self.access_address(ptr);
        // mstore ptr, value
        self.emit_void_inst(InstKind::Store(ptr, value, PtrLocation::Memory));
    }

    /// Emits a storage load.
    pub(crate) fn sload(&mut self, slot: ValueId) -> ValueId {
        self.load(slot, PtrLocation::Storage)
    }

    /// Emits a storage store.
    pub(crate) fn sstore(&mut self, slot: ValueId, value: ValueId) {
        // sstore slot, value
        self.emit_void_inst(InstKind::Store(slot, value, PtrLocation::Storage));
    }

    /// Emits a transient storage load.
    pub(crate) fn tload(&mut self, slot: ValueId) -> ValueId {
        self.load(slot, PtrLocation::Transient)
    }

    /// Emits a transient storage store.
    pub(crate) fn tstore(&mut self, slot: ValueId, value: ValueId) {
        // tstore slot, value
        self.emit_void_inst(InstKind::Store(slot, value, PtrLocation::Transient));
    }

    /// Emits a byte store to memory.
    pub(crate) fn store8(&mut self, ptr: ValueId, value: ValueId) {
        let ptr = self.access_address(ptr);
        // mstore8 ptr, value
        self.emit_void_inst(InstKind::Store8(ptr, value));
    }

    /// Emits a copy into memory.
    pub(crate) fn copy(
        &mut self,
        dest: ValueId,
        src: ValueId,
        len: ValueId,
        location: PtrLocation,
    ) {
        let (dest, src) = (self.access_address(dest), self.access_address(src));
        // mcopy|calldatacopy|returndatacopy dest, src, len
        self.emit_void_inst(InstKind::Copy(dest, src, len, location))
    }

    /// Emits a copy whose destination is proven to be in the heap.
    pub(crate) fn copy_heap(
        &mut self,
        dest: ValueId,
        src: ValueId,
        len: ValueId,
        location: PtrLocation,
    ) -> InstId {
        let (dest, src) = (self.access_address(dest), self.access_address(src));
        self.emit_void_inst_in_region(InstKind::Copy(dest, src, len, location), MemoryRegion::Heap)
    }

    /// Constructs a logical `(pointer, length, location)` slice.
    pub(crate) fn make_slice(
        &mut self,
        ptr: ValueId,
        len: ValueId,
        location: PtrLocation,
    ) -> ValueId {
        // slice = make_slice (inttoptr address to ptr<location>), length
        self.emit_inst(InstKind::MakeSlice { ptr, len, location }, Some(MirType::Slice(location)))
    }

    /// Emits a constant-data copy.
    pub(crate) fn data_copy(&mut self, data: crate::mir::DataRef, dest: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::DataCopy(data, dest, size))
    }

    /// Emits the byte length of deferred module data plus `addend`, rounded down to a
    /// multiple of 32 when `aligned` is set.
    pub(crate) fn data_size(
        &mut self,
        data: crate::mir::DataId,
        addend: u64,
        aligned: bool,
    ) -> ValueId {
        // result = datasize data, addend[, aligned]
        let size = crate::mir::DataSize { data, addend, aligned };
        self.emit_inst(InstKind::DataSize(size), Some(MirType::I256))
    }

    /// Emits an opaque library address supplied by the linker.
    pub(crate) fn library_address(&mut self, library: LibraryId) -> ValueId {
        // result = library_address source:library
        self.emit_inst(InstKind::LibraryAddress(library), Some(MirType::I160))
    }

    /// Emits a loadimmutable instruction.
    pub(crate) fn load_immutable(&mut self, id: ImmutableId, ty: MirType) -> ValueId {
        self.emit_inst(InstKind::LoadImmutable(id), Some(ty))
    }

    /// Emits a storeimmutable instruction.
    pub(crate) fn store_immutable(&mut self, id: ImmutableId, value: ValueId) {
        self.emit_void_inst(InstKind::StoreImmutable(id, value))
    }

    /// Emits an extcodecopy instruction.
    pub(crate) fn extcodecopy(
        &mut self,
        addr: ValueId,
        dest: ValueId,
        offset: ValueId,
        size: ValueId,
    ) {
        self.emit_void_inst(InstKind::ExtCodeCopy(addr, dest, offset, size))
    }

    /// Emits an extcodecopy whose destination is proven to be in the heap.
    pub(crate) fn extcodecopy_heap(
        &mut self,
        addr: ValueId,
        dest: ValueId,
        offset: ValueId,
        size: ValueId,
    ) {
        self.emit_void_inst_in_region(
            InstKind::ExtCodeCopy(addr, dest, offset, size),
            MemoryRegion::Heap,
        );
    }

    /// Emits a returndata copy that feeds an external return or revert.
    pub(crate) fn returndatacopy_abi_return(
        &mut self,
        dest: ValueId,
        offset: ValueId,
        size: ValueId,
    ) {
        self.emit_void_inst_in_region(
            InstKind::Copy(dest, offset, size, PtrLocation::Returndata),
            MemoryRegion::AbiReturn,
        );
    }

    /// Emits an internal function call.
    pub(crate) fn icall(
        &mut self,
        function: FunctionId,
        args: Vec<ValueId>,
        result_ty: MirType,
    ) -> ValueId {
        self.emit_inst(
            InstKind::ICall { function: Callee::Function(function), args: args.into() },
            Some(result_ty),
        )
    }

    /// Emits an internal function call whose result, if any, is not used as a value.
    pub(crate) fn icall_void(&mut self, function: FunctionId, args: Vec<ValueId>) {
        self.emit_void_inst(InstKind::ICall {
            function: Callee::Function(function),
            args: args.into(),
        });
    }

    /// Emits an address inside the current internal-call frame.
    pub(crate) fn internal_frame_addr(&mut self, offset: u64) -> ValueId {
        self.emit_inst(InstKind::InternalFrameAddr(offset), Some(MirType::MEMPTR))
    }

    /// Loads a mutable local through its logical frame slot.
    pub(crate) fn frame_load(
        &mut self,
        offset: u64,
        mode: FrameMode,
        kind: FrameSlotKind,
    ) -> ValueId {
        self.emit_inst(InstKind::FrameLoad { offset, mode, kind }, Some(kind.result_type()))
    }

    /// Stores a mutable local through its logical frame slot.
    pub(crate) fn frame_store(
        &mut self,
        offset: u64,
        mode: FrameMode,
        kind: FrameSlotKind,
        value: ValueId,
    ) {
        self.emit_void_inst(InstKind::FrameStore { offset, mode, kind, value });
    }

    pub(crate) fn slotnum(&mut self) -> ValueId {
        self.emit_inst(InstKind::SlotNum, Some(MirType::I256))
    }

    /// Emits a low-level address call over a bytes object.
    pub(crate) fn address_call(
        &mut self,
        kind: AddressCallKind,
        address: ValueId,
        input: ValueId,
        gas: Option<ValueId>,
        value: Option<ValueId>,
    ) -> ValueId {
        // success = address_call(address, input, gas?, value?)
        self.emit_inst(
            InstKind::AddressCall { kind, address, input, gas, value },
            Some(MirType::I1),
        )
    }

    /// Copies the current returndata into a fresh bytes object.
    pub(crate) fn returndata_bytes(&mut self) -> ValueId {
        // object = returndata_bytes
        self.emit_inst(
            InstKind::builtin(crate::mir::Builtin::ReturndataBytes, []),
            Some(MirType::MEMPTR),
        )
    }

    /// Sends value with the Solidity stipend, returning success.
    pub(crate) fn send(&mut self, address: ValueId, amount: ValueId) -> ValueId {
        // success = send address, amount
        self.emit_inst(
            InstKind::builtin(crate::mir::Builtin::Send, [address, amount]),
            Some(MirType::I256),
        )
    }

    /// Transfers value with the Solidity stipend and propagates failure returndata.
    pub(crate) fn transfer(&mut self, address: ValueId, amount: ValueId) {
        // transfer address, amount
        self.emit_void_inst(InstKind::builtin(crate::mir::Builtin::Transfer, [address, amount]));
    }

    /// Emits a call instruction (external call).
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn call(
        &mut self,
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    ) -> ValueId {
        self.emit_inst(
            InstKind::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size },
            Some(MirType::I1),
        )
    }

    /// Emits a callcode instruction.
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn callcode(
        &mut self,
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    ) -> ValueId {
        self.emit_inst(
            InstKind::CallCode { gas, addr, value, args_offset, args_size, ret_offset, ret_size },
            Some(MirType::I1),
        )
    }

    /// Emits a staticcall instruction (read-only external call).
    pub(crate) fn staticcall(
        &mut self,
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    ) -> ValueId {
        self.emit_inst(
            InstKind::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size },
            Some(MirType::I1),
        )
    }

    /// Emits a delegatecall instruction (call with caller's context).
    pub(crate) fn delegatecall(
        &mut self,
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    ) -> ValueId {
        self.emit_inst(
            InstKind::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size },
            Some(MirType::I1),
        )
    }

    /// Emits a create2 instruction (deploy a contract with salt).
    pub(crate) fn create2(
        &mut self,
        value: ValueId,
        offset: ValueId,
        size: ValueId,
        salt: ValueId,
    ) -> ValueId {
        self.emit_inst(InstKind::Create2(value, offset, size, salt), Some(MirType::I256))
    }

    /// Emits a log with at most four topics.
    pub(crate) fn log(&mut self, offset: ValueId, size: ValueId, topics: &[ValueId]) {
        let topics = LogTopics::new(topics).expect("a log has at most four topics");
        self.emit_void_inst(InstKind::Log { offset, size, topics });
    }

    /// Emits a select instruction.
    pub(crate) fn select(
        &mut self,
        cond: ValueId,
        then_val: ValueId,
        else_val: ValueId,
    ) -> ValueId {
        let cond = self.cast(cond, MirType::I1);
        let mut ty = self.func.value_ty(then_val).unwrap();
        if matches!(ty, MirType::Int(_)) && self.func.value_ty(else_val) != Some(ty) {
            ty = MirType::I256;
        }
        // then_val = cast then_val to the select type
        // else_val = cast else_val to the select type
        // result = select cond, then_val, else_val
        let then_val = self.cast(then_val, ty);
        let else_val = self.cast(else_val, ty);
        self.emit_inst(InstKind::Select(cond, then_val, else_val), Some(ty))
    }

    /// Emits a phi instruction. `incoming` pairs each predecessor block of the
    /// current block with the value the phi takes when control arrives from
    /// that block. Emit phis before any other instruction in their block.
    pub(crate) fn phi(&mut self, incoming: Vec<(BlockId, ValueId)>) -> ValueId {
        let mut ty = incoming
            .first()
            .and_then(|(_, value)| self.func.value_ty(*value))
            .unwrap_or(MirType::I256);
        if matches!(ty, MirType::Int(_))
            && incoming.iter().any(|(_, value)| self.func.value_ty(*value) != Some(ty))
        {
            ty = MirType::I256;
        }
        // result = phi [predecessor: cast value to the phi type, ...]
        self.emit_inst(InstKind::Phi(incoming), Some(ty))
    }

    /// Adds an incoming `(block, value)` edge to an existing phi. This is used
    /// to patch loop-carried phis whose back-edge values are only known after
    /// the loop body has been built.
    ///
    /// # Panics
    ///
    /// Panics if `phi` does not refer to a phi instruction result.
    pub(crate) fn add_phi_incoming(&mut self, phi: ValueId, block: BlockId, value: ValueId) {
        let Value::Inst(inst_id) = *self.func.value(phi) else {
            panic!("add_phi_incoming: value is not an instruction result");
        };
        let current = self.current_block;
        self.switch_to_block(block);
        // incoming = cast value to the phi type
        let value = self.cast(value, self.func.inst(inst_id).result_ty.unwrap());
        self.switch_to_block(current);
        let InstKind::Phi(incoming) = &mut self.func.inst_mut(inst_id).kind else {
            panic!("add_phi_incoming: instruction is not a phi");
        };
        incoming.push((block, value));
    }

    /// Sets a jump terminator.
    pub(crate) fn jump(&mut self, target: BlockId) {
        self.set_terminator(Terminator::Jump(target));
    }

    /// Sets a branch terminator.
    pub(crate) fn branch(&mut self, condition: ValueId, then_block: BlockId, else_block: BlockId) {
        let condition = self.cast(condition, MirType::I1);
        // branch condition, then_block, else_block
        self.set_terminator(Terminator::Branch { condition, then_block, else_block });
    }

    /// Sets a switch terminator.
    pub(crate) fn switch(
        &mut self,
        value: ValueId,
        default: BlockId,
        cases: Vec<(ValueId, BlockId)>,
    ) {
        self.set_terminator(Terminator::Switch { value, default, cases });
    }

    /// Sets a return terminator.
    pub(crate) fn ret(&mut self, values: impl IntoIterator<Item = ValueId>) {
        let mut values = values.into_iter().collect::<SmallVec<[ValueId; 2]>>();
        if let [MirType::Struct(ty)] = self.func.return_components()
            && !(values.len() == 1 && self.func.value_ty(values[0]) == Some(MirType::Struct(*ty)))
        {
            // result = insert_value(undef, field0), ...
            // ret result
            let result = self.make_struct(*ty, values);
            values = smallvec::smallvec![result];
        }
        for (value, ty) in values.iter_mut().zip(self.func.return_components().to_vec()) {
            // value = cast value to the declared return type
            *value = self.cast(*value, ty);
        }
        self.set_terminator(Terminator::Return { values });
    }

    /// Sets a revert terminator.
    pub(crate) fn revert(&mut self, offset: ValueId, size: ValueId) {
        // offset = zext offset to i256, unless it is a ptr<memory>
        // size = zext size to i256
        let offset = self.memory_address(offset);
        let size = self.cast(size, MirType::I256);
        self.set_terminator(Terminator::Revert { offset, size });
    }

    /// Returns a memory address operand: a `ptr<memory>` as is, anything else as a word.
    fn memory_address(&mut self, address: ValueId) -> ValueId {
        let ty = super::typing::address(self.func, address, PtrLocation::Memory);
        self.cast(address, ty)
    }

    /// Sets a returndata-bubbling revert terminator.
    pub(crate) fn revert_returndata(&mut self) {
        self.set_terminator(Terminator::RevertReturndata);
    }

    /// Reverts with the return data of a failed call unless `success`, then continues in a new
    /// block.
    pub(crate) fn revert_returndata_unless(&mut self, success: ValueId) {
        // branch success, done, failed
        // failed: revert_returndata
        let failed = self.create_block();
        let done = self.create_block();
        self.branch(success, done, failed);
        self.switch_to_block(failed);
        self.revert_returndata();
        self.switch_to_block(done);
    }

    /// Sets a return-data terminator: `RETURN(offset, size)`.
    pub(crate) fn ret_data(&mut self, offset: ValueId, size: ValueId) {
        // offset = zext offset to i256, unless it is a ptr<memory>
        // size = zext size to i256
        let offset = self.memory_address(offset);
        let size = self.cast(size, MirType::I256);
        self.set_terminator(Terminator::ReturnData { offset, size });
    }

    /// Sets a stop terminator.
    pub(crate) fn stop(&mut self) {
        self.set_terminator(Terminator::Stop);
    }

    /// Sets a tail-call terminator: transfer control to `function` without
    /// returning to this function.
    pub(crate) fn tail_call(&mut self, function: FunctionId, args: Vec<ValueId>) {
        self.set_terminator(Terminator::TailCall { function, args: args.into_iter().collect() });
    }

    /// Sets an invalid terminator.
    pub(crate) fn invalid(&mut self) {
        self.set_terminator(Terminator::Invalid);
    }

    /// Sets a selfdestruct terminator.
    pub(crate) fn selfdestruct(&mut self, recipient: ValueId) {
        // recipient = zext recipient to i256
        let recipient = self.cast(recipient, MirType::I256);
        self.set_terminator(Terminator::SelfDestruct { recipient });
    }

    /// Sets a fully constructed terminator on the current block.
    pub(crate) fn set_terminator(&mut self, terminator: Terminator) {
        let current = self.current_block;
        for successor in terminator.successors() {
            let predecessors = &mut self.func.blocks[successor].predecessors;
            if !predecessors.contains(&current) {
                predecessors.push(current);
            }
        }
        let mut metadata = InstructionMetadata::EMPTY;
        self.set_current_debug_context(&mut metadata);
        self.func.blocks[current].set_terminator(terminator, metadata);
    }

    /// Returns a reference to the function.
    #[must_use]
    pub(crate) fn func(&self) -> &Function {
        self.func
    }

    /// Returns a mutable reference to the function.
    pub(crate) fn func_mut(&mut self) -> &mut Function {
        self.func
    }

    /// Emits `value << shift`.
    pub(crate) fn shl(&mut self, value: ValueId, shift: ValueId) -> ValueId {
        // result = shl value, shift
        self.emit_inst(InstKind::Shl(shift, value), Some(MirType::I256))
    }

    /// Emits `value >> shift`.
    pub(crate) fn shr(&mut self, value: ValueId, shift: ValueId) -> ValueId {
        // result = shr value, shift
        self.emit_inst(InstKind::Shr(shift, value), Some(MirType::I256))
    }

    /// Emits the arithmetic shift `value >> shift`.
    pub(crate) fn sar(&mut self, value: ValueId, shift: ValueId) -> ValueId {
        // result = sar value, shift
        self.emit_inst(InstKind::Sar(shift, value), Some(MirType::I256))
    }
}

//! MIR function builder.

use super::{
    AbiEncodeMode, AllocationSemantics, BlockId, FrameMode, FrameSlotKind, Function, FunctionId,
    Immediate, ImmutableId, InstId, InstKind, Instruction, InstructionMetadata, MemoryObjectKind,
    MemoryObjectLayout, MemoryRegion, MirType, SliceLocation, StorageAlias, Terminator, Value,
    ValueId,
};
use crate::memory::EvmMemoryLayout;
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_config::RevertStrings;
use solar_data_structures::map::FxHashMap;
use solar_interface::Span;

/// Solidity's built-in `Panic(uint256)` error codes.
#[repr(u8)]
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum PanicCode {
    Assert = 0x01,
    ArithmeticOverflowUnderflow = 0x11,
    DivisionByZero = 0x12,
    EnumConversion = 0x21,
    StorageEncoding = 0x22,
    EmptyArrayPop = 0x31,
    ArrayOutOfBounds = 0x32,
    MemoryAllocationOverflow = 0x41,
    InvalidInternalFunction = 0x51,
}

impl PanicCode {
    const fn as_u64(self) -> u64 {
        self as u64
    }
}

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

/// The Error(string) selector, `keccak256("Error(string)")[..4]`, left-aligned in a word.
pub(crate) const ERROR_SELECTOR: U256 = U256::from_limbs([0, 0, 0, 0x08c3_79a0_u64 << 32]);

/// Why a revert with no user-supplied payload fires.
///
/// These reverts carry no data by default. With `--revert-strings debug`, each reason other than
/// [`RevertReason::Empty`] is encoded as an `Error(string)` payload with the same message solc
/// attaches to the corresponding check, so a failing transaction explains which internal check
/// rejected it.
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum RevertReason {
    /// Empty data in every mode: `require` and `revert()` without a message, stripped messages,
    /// and checks solc never attaches a message to, such as decoded ABI word validators.
    Empty,
    /// A non-payable external entry point received Ether.
    EtherSentToNonPayable,
    /// The selector did not match any external function and no fallback exists, but the
    /// contract has a `receive` function.
    UnknownSelector,
    /// The call matched nothing and the contract has neither a fallback nor a `receive`.
    NoFallbackNorReceive,
    /// ABI-encoded input ends before the static head of a tuple.
    TupleDataTooShort,
    /// A tuple element offset points outside the encoded input.
    InvalidTupleOffset,
    /// A dynamic array or `bytes` head offset points outside the encoded input.
    InvalidCalldataArrayOffset,
    /// A dynamic array or `bytes calldata` length exceeds the encodable range.
    InvalidCalldataArrayLength,
    /// A dynamic array's element data does not fit the encoded input.
    InvalidCalldataArrayStride,
    /// A `bytes` or `string` decoded to memory does not fit the encoded input.
    InvalidByteArrayLength,
    /// A struct member offset exceeds the encodable range.
    InvalidStructOffset,
    /// Calldata ends before the static head of a struct.
    StructCalldataTooShort,
    /// ABI-encoded memory data ends before the static head of a struct.
    StructDataTooShort,
    /// A calldata array element or struct member offset is out of range while re-encoding.
    InvalidCalldataAccessOffset,
    /// A calldata array element length exceeds the encodable range while re-encoding.
    InvalidCalldataAccessLength,
    /// A calldata array element's data does not fit in calldata while re-encoding.
    InvalidCalldataAccessStride,
    /// A calldata tail element offset is out of range.
    InvalidCalldataTailOffset,
    /// A calldata tail element length exceeds the encodable range.
    InvalidCalldataTailLength,
    /// A calldata tail element's data does not fit in calldata.
    CalldataTailTooShort,
    /// A slice end exceeds the sliced value's length.
    SliceGreaterThanLength,
    /// A slice starts after its end.
    SliceStartsAfterEnd,
    /// An external call target has no code.
    TargetContractHasNoCode,
    /// A non-view library function was called directly instead of through `DELEGATECALL`.
    LibraryCalledWithoutDelegatecall,
}

impl RevertReason {
    /// The message solc attaches to this check with `--revert-strings debug`, if any.
    pub(crate) const fn message(self) -> Option<&'static str> {
        Some(match self {
            Self::Empty => return None,
            Self::EtherSentToNonPayable => "Ether sent to non-payable function",
            Self::UnknownSelector => "Unknown signature and no fallback defined",
            Self::NoFallbackNorReceive => "Contract does not have fallback nor receive functions",
            Self::TupleDataTooShort => "ABI decoding: tuple data too short",
            Self::InvalidTupleOffset => "ABI decoding: invalid tuple offset",
            Self::InvalidCalldataArrayOffset => "ABI decoding: invalid calldata array offset",
            Self::InvalidCalldataArrayLength => "ABI decoding: invalid calldata array length",
            Self::InvalidCalldataArrayStride => "ABI decoding: invalid calldata array stride",
            Self::InvalidByteArrayLength => "ABI decoding: invalid byte array length",
            Self::InvalidStructOffset => "ABI decoding: invalid struct offset",
            Self::StructCalldataTooShort => "ABI decoding: struct calldata too short",
            Self::StructDataTooShort => "ABI decoding: struct data too short",
            Self::InvalidCalldataAccessOffset => "Invalid calldata access offset",
            Self::InvalidCalldataAccessLength => "Invalid calldata access length",
            Self::InvalidCalldataAccessStride => "Invalid calldata access stride",
            Self::InvalidCalldataTailOffset => "Invalid calldata tail offset",
            Self::InvalidCalldataTailLength => "Invalid calldata tail length",
            Self::CalldataTailTooShort => "Calldata tail too short",
            Self::SliceGreaterThanLength => "Slice is greater than length",
            Self::SliceStartsAfterEnd => "Slice starts after end",
            Self::TargetContractHasNoCode => "Target contract does not contain code",
            Self::LibraryCalledWithoutDelegatecall => {
                "Non-view function of library called without DELEGATECALL"
            }
        })
    }
}

#[derive(Clone, Copy, Eq, Hash, PartialEq)]
enum RevertKind {
    Panic(PanicCode),
    Reason(RevertReason),
}

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
        }
    }

    /// Selects how compiler-generated reverts with a [`RevertReason`] are encoded.
    ///
    /// Only `debug` changes the output: reasons then revert with an `Error(string)` payload
    /// instead of empty data.
    pub(crate) fn with_revert_strings(mut self, revert_strings: RevertStrings) -> Self {
        self.revert_strings = revert_strings;
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

    /// Adds a return type to the function.
    pub(crate) fn add_return(&mut self, ty: MirType) {
        self.func.returns.push(ty);
    }

    /// Creates a uint256 immediate value.
    pub(crate) fn imm(&mut self, value: impl ToUint) -> ValueId {
        self.alloc_value(Value::Immediate(Immediate::uint256(value.to_uint())))
    }

    /// Creates a boolean immediate.
    pub(crate) fn imm_bool(&mut self, value: bool) -> ValueId {
        self.alloc_value(Value::Immediate(Immediate::bool(value)))
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
        let selector = self.imm(U256::from(0x4e48_7b71_u64) << 224);
        let code = self.imm(code.as_u64());
        let zero = self.imm(U256::ZERO);
        self.mstore(zero, selector);
        let four = self.imm(4);
        self.mstore(four, code);
        let size = self.imm(36);
        self.revert(zero, size);
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

    /// Terminates the current block by reverting for `reason`.
    pub(crate) fn revert_with(&mut self, reason: RevertReason) {
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
        // mstore(0, Error(string).selector)
        // mstore(4, 32)
        // mstore(36, len)
        // mstore(68 + 32 * i, word_i) for each 32-byte chunk of message
        // revert(0, 68 + ceil32(len))
        let selector = self.imm(ERROR_SELECTOR);
        let zero = self.imm(0);
        self.mstore(zero, selector);
        let offset = self.imm(4);
        let tuple_offset = self.imm(32);
        self.mstore(offset, tuple_offset);
        let length_offset = self.imm(36);
        let length = self.imm(message.len() as u64);
        self.mstore(length_offset, length);
        let mut data_offset = 68u64;
        for chunk in message.as_bytes().chunks(32) {
            let mut word = [0u8; 32];
            word[..chunk.len()].copy_from_slice(chunk);
            let offset = self.imm(data_offset);
            let word = self.imm(U256::from_be_bytes(word));
            self.mstore(offset, word);
            data_offset += 32;
        }
        let size = self.imm(data_offset);
        self.revert(zero, size);
    }

    fn branch_to_revert(
        &mut self,
        condition: ValueId,
        condition_is_zero: bool,
        kind: RevertKind,
    ) -> BlockId {
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
        let invalid = self.iszero(valid);
        self.panic_if(invalid, PanicCode::EnumConversion);
    }

    /// Reverts with `PanicCode::ArrayOutOfBounds` when `index` is not below `length`.
    pub(crate) fn bounds_check(&mut self, index: ValueId, length: ValueId) {
        let in_range = self.lt(index, length);
        let invalid = self.iszero(in_range);
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
        let rhs_zero = self.iszero(rhs);
        let quotient = self.div(result, rhs);
        let exact = self.eq(quotient, lhs);
        let valid = self.or(rhs_zero, exact);
        let overflow = self.iszero(valid);
        self.panic_if(overflow, PanicCode::MemoryAllocationOverflow);
        result
    }

    /// Returns the word-aligned allocation size for a bytes-like object.
    pub(crate) fn checked_padded_size(&mut self, length: ValueId) -> ValueId {
        let padding = self.imm(63);
        let rounded = self.checked_add(length, padding);
        self.mask_padded_size(rounded)
    }

    /// Returns the Solidity-compatible padded size for `bytes.concat`.
    pub(crate) fn padded_size(&mut self, length: ValueId) -> ValueId {
        let padding = self.imm(63);
        let rounded = self.add(length, padding);
        self.mask_padded_size(rounded)
    }

    fn mask_padded_size(&mut self, rounded: ValueId) -> ValueId {
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

    fn emit_inst(&mut self, kind: InstKind, result_ty: Option<MirType>) -> ValueId {
        debug_assert!(result_ty.is_some(), "value-producing instructions must have a result type");
        let inst = self.make_inst(kind, result_ty);
        self.append_instruction(inst).1.expect("value-producing instruction must have a result")
    }

    /// Emits an instruction that produces no value, such as a store or a log.
    ///
    /// No result [`Value`] is allocated: only value-producing instructions get
    /// an entry in the function's value table.
    fn emit_void_inst(&mut self, kind: InstKind) {
        let inst = self.make_inst(kind, None);
        self.append_instruction(inst);
    }

    /// Emits a void memory instruction with a proven destination region.
    fn emit_void_inst_in_region(&mut self, kind: InstKind, region: MemoryRegion) {
        let mut inst = self.make_inst(kind, None);
        inst.metadata.set_memory_region(Some(region));
        self.append_instruction(inst);
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
            InstKind::MLoad(addr)
            | InstKind::MStore(addr, _)
            | InstKind::MStore8(addr, _)
            | InstKind::Keccak256(addr, _) => addr,
            InstKind::MCopy(dest, _, _)
            | InstKind::CalldataCopy(dest, _, _)
            | InstKind::CodeCopy(dest, _, _)
            | InstKind::ReturnDataCopy(dest, _, _)
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
            InstKind::SLoad(slot) | InstKind::SStore(slot, _) => Some(self.storage_alias(slot)),
            _ => None,
        }
    }

    fn storage_alias(&self, slot: ValueId) -> StorageAlias {
        StorageAlias::for_value(self.func, slot)
    }

    /// Emits an add instruction.
    pub(crate) fn add(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Add(a, b), Some(MirType::uint256()))
    }

    /// Emits a sub instruction.
    pub(crate) fn sub(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Sub(a, b), Some(MirType::uint256()))
    }

    /// Emits a mul instruction.
    pub(crate) fn mul(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Mul(a, b), Some(MirType::uint256()))
    }

    /// Emits a div instruction.
    pub(crate) fn div(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Div(a, b), Some(MirType::uint256()))
    }

    /// Emits a sdiv instruction.
    pub(crate) fn sdiv(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SDiv(a, b), Some(MirType::int256()))
    }

    /// Emits a mod instruction.
    pub(crate) fn mod_(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Mod(a, b), Some(MirType::uint256()))
    }

    /// Emits an addmod instruction.
    pub(crate) fn addmod(&mut self, a: ValueId, b: ValueId, n: ValueId) -> ValueId {
        self.emit_inst(InstKind::AddMod(a, b, n), Some(MirType::uint256()))
    }

    /// Emits a mulmod instruction.
    pub(crate) fn mulmod(&mut self, a: ValueId, b: ValueId, n: ValueId) -> ValueId {
        self.emit_inst(InstKind::MulMod(a, b, n), Some(MirType::uint256()))
    }

    /// Emits a smod instruction.
    pub(crate) fn smod(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SMod(a, b), Some(MirType::int256()))
    }

    /// Emits an exp instruction.
    pub(crate) fn exp(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Exp(a, b), Some(MirType::uint256()))
    }

    /// Emits an and instruction.
    pub(crate) fn and(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::And(a, b), Some(MirType::uint256()))
    }

    /// Emits an or instruction.
    pub(crate) fn or(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Or(a, b), Some(MirType::uint256()))
    }

    /// Emits a xor instruction.
    pub(crate) fn xor(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Xor(a, b), Some(MirType::uint256()))
    }

    /// Emits a not instruction.
    pub(crate) fn not(&mut self, a: ValueId) -> ValueId {
        self.emit_inst(InstKind::Not(a), Some(MirType::uint256()))
    }

    /// Emits a clz instruction.
    pub(crate) fn clz(&mut self, a: ValueId) -> ValueId {
        self.emit_inst(InstKind::Clz(a), Some(MirType::uint256()))
    }

    /// Emits a shl instruction.
    pub(crate) fn shl(&mut self, shift: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Shl(shift, value), Some(MirType::uint256()))
    }

    /// Emits a shr instruction.
    pub(crate) fn shr(&mut self, shift: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Shr(shift, value), Some(MirType::uint256()))
    }

    /// Emits a sar instruction.
    pub(crate) fn sar(&mut self, shift: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Sar(shift, value), Some(MirType::int256()))
    }

    /// Emits a lt instruction.
    pub(crate) fn lt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Lt(a, b), Some(MirType::Bool))
    }

    /// Emits a gt instruction.
    pub(crate) fn gt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Gt(a, b), Some(MirType::Bool))
    }

    /// Emits a slt instruction.
    pub(crate) fn slt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SLt(a, b), Some(MirType::Bool))
    }

    /// Emits a sgt instruction.
    pub(crate) fn sgt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SGt(a, b), Some(MirType::Bool))
    }

    /// Emits an eq instruction.
    pub(crate) fn eq(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Eq(a, b), Some(MirType::Bool))
    }

    /// Emits an iszero instruction.
    pub(crate) fn iszero(&mut self, a: ValueId) -> ValueId {
        self.emit_inst(InstKind::IsZero(a), Some(MirType::Bool))
    }

    /// Emits a byte instruction.
    pub(crate) fn byte(&mut self, index: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Byte(index, value), Some(MirType::uint256()))
    }

    /// Emits a signextend instruction.
    pub(crate) fn signextend(&mut self, size: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::SignExtend(size, value), Some(MirType::int256()))
    }

    /// Emits an mload instruction.
    pub(crate) fn mload(&mut self, offset: ValueId) -> ValueId {
        self.emit_inst(InstKind::MLoad(offset), Some(MirType::uint256()))
    }

    /// Emits an mstore instruction.
    pub(crate) fn mstore(&mut self, offset: ValueId, value: ValueId) {
        self.emit_void_inst(InstKind::MStore(offset, value))
    }

    /// Emits an mstore8 instruction.
    pub(crate) fn mstore8(&mut self, offset: ValueId, value: ValueId) {
        self.emit_void_inst(InstKind::MStore8(offset, value))
    }

    /// Emits a memory-zero instruction.
    pub(crate) fn memory_zero(&mut self, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::MemoryZero(offset, size))
    }

    /// Emits an msize instruction.
    pub(crate) fn msize(&mut self) -> ValueId {
        self.emit_inst(InstKind::MSize, Some(MirType::uint256()))
    }

    /// Reads the free-memory pointer.
    pub(crate) fn fmp(&mut self) -> ValueId {
        self.emit_inst(InstKind::Fmp, Some(MirType::MemPtr))
    }

    /// Sets the free-memory pointer.
    #[cfg(test)]
    pub(crate) fn set_fmp(&mut self, ptr: ValueId) {
        self.emit_void_inst(InstKind::SetFmp(ptr))
    }

    /// Reserves untyped memory under an explicit semantic policy.
    pub(crate) fn alloc_raw(&mut self, size: ValueId, semantics: AllocationSemantics) -> ValueId {
        self.alloc_kind(size, crate::mir::AllocationKind::Raw, semantics)
    }

    /// Reserves memory under an explicit semantic policy.
    #[cfg(test)]
    pub(crate) fn alloc(&mut self, size: ValueId, semantics: AllocationSemantics) -> ValueId {
        self.alloc_raw(size, semantics)
    }

    /// Reserves memory for a semantically shaped object.
    pub(crate) fn alloc_object(
        &mut self,
        size: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        semantics: AllocationSemantics,
    ) -> ValueId {
        self.alloc_kind(size, crate::mir::AllocationKind::Object(layout), semantics)
    }

    /// Allocates a padded bytes object and records its logical length.
    pub(crate) fn alloc_bytes_object(
        &mut self,
        length: ValueId,
        semantics: AllocationSemantics,
    ) -> ValueId {
        let size = self.checked_padded_size(length);
        let object = self.alloc_object(size, MemoryObjectLayout::Bytes, semantics);
        self.set_memory_object_len(object, length, MemoryObjectKind::Bytes);
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
        let object = self.alloc_object(size, layout, semantics);
        (object, layout)
    }

    /// Allocates a struct whose fields each occupy one memory word.
    pub(crate) fn alloc_word_struct(
        &mut self,
        fields: u64,
        semantics: AllocationSemantics,
    ) -> (ValueId, MemoryObjectLayout) {
        let size = self.imm(fields.saturating_mul(EvmMemoryLayout::WORD_SIZE));
        let layout = MemoryObjectLayout::structure(fields);
        let object = self.alloc_object(size, layout, semantics);
        (object, layout)
    }

    /// Allocates a dynamic array whose elements each occupy one memory word.
    pub(crate) fn alloc_dynamic_word_array(
        &mut self,
        length: ValueId,
        semantics: AllocationSemantics,
    ) -> (ValueId, MemoryObjectLayout) {
        let one = self.imm(1);
        let words = self.checked_add(length, one);
        let word_size = self.imm(32);
        let size = self.checked_mul(words, word_size);
        let layout = MemoryObjectLayout::WORD_ARRAY;
        let object = self.alloc_object(size, layout, semantics);
        self.set_memory_object_len(object, length, layout.kind());
        (object, layout)
    }

    /// Reads the logical length of a dynamic memory object.
    pub(crate) fn memory_object_len(
        &mut self,
        object: ValueId,
        kind: crate::mir::MemoryObjectKind,
    ) -> ValueId {
        self.emit_inst(InstKind::MemoryObjectLen(object, kind), Some(MirType::uint256()))
    }

    /// Sets the logical length of a dynamic memory object.
    pub(crate) fn set_memory_object_len(
        &mut self,
        object: ValueId,
        len: ValueId,
        kind: crate::mir::MemoryObjectKind,
    ) {
        self.emit_void_inst(InstKind::SetMemoryObjectLen(object, len, kind))
    }

    /// Projects an object's data address.
    pub(crate) fn memory_object_data(
        &mut self,
        object: ValueId,
        kind: crate::mir::MemoryObjectKind,
    ) -> ValueId {
        self.emit_inst(InstKind::MemoryObjectData(object, kind), Some(MirType::MemPtr))
    }

    /// Loads a direct struct field through the semantic object layout.
    pub(crate) fn memory_object_load_field(
        &mut self,
        object: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        field: u64,
    ) -> ValueId {
        self.emit_inst(
            InstKind::MemoryObjectLoadField { object, layout, field },
            Some(MirType::uint256()),
        )
    }

    /// Stores a direct struct field through the semantic object layout.
    pub(crate) fn memory_object_store_field(
        &mut self,
        object: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        field: u64,
        value: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryObjectStoreField { object, layout, field, value });
    }

    /// Loads an array element through the semantic object layout.
    pub(crate) fn memory_object_load_element(
        &mut self,
        object: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        index: ValueId,
    ) -> ValueId {
        self.emit_inst(
            InstKind::MemoryObjectLoadElement { object, layout, index },
            Some(MirType::uint256()),
        )
    }

    /// Loads a memory-object pointer stored in a one-word array.
    pub(crate) fn memory_object_load_object(
        &mut self,
        object: ValueId,
        layout: MemoryObjectLayout,
        index: ValueId,
        kind: MemoryObjectKind,
    ) -> ValueId {
        self.emit_inst(
            InstKind::MemoryObjectLoadElement { object, layout, index },
            Some(MirType::MemoryObject(kind)),
        )
    }

    /// Loads one byte from a bytes object through its semantic layout.
    pub(crate) fn memory_object_load_byte(&mut self, object: ValueId, index: ValueId) -> ValueId {
        self.emit_inst(InstKind::MemoryObjectLoadByte { object, index }, Some(MirType::uint256()))
    }

    /// Stores an array element through the semantic object layout.
    pub(crate) fn memory_object_store_element(
        &mut self,
        object: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        index: ValueId,
        value: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryObjectStoreElement { object, layout, index, value });
    }

    /// Stores one byte in a bytes object through its semantic layout.
    pub(crate) fn memory_object_store_byte(
        &mut self,
        object: ValueId,
        index: ValueId,
        value: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryObjectStoreByte { object, index, value });
    }

    /// Stores one word at a byte offset in a bytes object through its semantic
    /// layout.
    pub(crate) fn memory_object_store_word(
        &mut self,
        object: ValueId,
        offset: ValueId,
        value: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryObjectStoreWord { object, offset, value });
    }

    /// Loads one word from a memory slice at a byte offset through its
    /// semantic representation.
    pub(crate) fn memory_slice_load_word(&mut self, slice: ValueId, offset: ValueId) -> ValueId {
        self.emit_inst(
            InstKind::MemorySliceLoadWord { slice, offset },
            Some(crate::mir::MirType::uint256()),
        )
    }

    /// Loads one word from a calldata slice at a byte offset through its
    /// semantic representation.
    pub(crate) fn calldata_slice_load_word(&mut self, slice: ValueId, offset: ValueId) -> ValueId {
        self.emit_inst(
            InstKind::CalldataSliceLoadWord { slice, offset },
            Some(crate::mir::MirType::uint256()),
        )
    }

    /// Copies a typed slice into a dynamic memory object's payload.
    pub(crate) fn memory_object_copy_from_slice(
        &mut self,
        object: ValueId,
        kind: crate::mir::MemoryObjectKind,
        source: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryObjectCopyFromSlice { object, kind, source });
    }

    /// Copies a typed slice into a byte offset in a dynamic memory object's payload.
    pub(crate) fn memory_object_copy_from_slice_at(
        &mut self,
        object: ValueId,
        kind: crate::mir::MemoryObjectKind,
        offset: ValueId,
        source: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryObjectCopyFromSliceAt { object, kind, offset, source });
    }

    fn alloc_kind(
        &mut self,
        size: ValueId,
        kind: crate::mir::AllocationKind,
        semantics: AllocationSemantics,
    ) -> ValueId {
        self.emit_inst(InstKind::Alloc { size, kind, semantics }, Some(kind.result_type()))
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

    /// Decodes a memory-backed ABI tuple into semantic values.
    pub(crate) fn abi_decode(
        &mut self,
        layout: crate::mir::AbiParamLayoutRef,
        data: ValueId,
    ) -> ValueId {
        let result_ty = layout
            .types
            .first()
            .map(crate::mir::AbiParamType::mir_type)
            .expect("ABI decode requires at least one result");
        self.emit_inst(InstKind::AbiDecode { data, layout }, Some(result_ty))
    }

    /// Emits an mcopy instruction.
    pub(crate) fn mcopy(&mut self, dest: ValueId, src: ValueId, len: ValueId) {
        self.emit_void_inst(InstKind::MCopy(dest, src, len))
    }

    /// Emits an mcopy whose destination is proven to be in the heap.
    pub(crate) fn mcopy_heap(&mut self, dest: ValueId, src: ValueId, len: ValueId) {
        self.emit_void_inst_in_region(InstKind::MCopy(dest, src, len), MemoryRegion::Heap)
    }

    /// Emits an sload instruction.
    pub(crate) fn sload(&mut self, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::SLoad(slot), Some(MirType::uint256()))
    }

    /// Emits an sstore instruction.
    pub(crate) fn sstore(&mut self, slot: ValueId, value: ValueId) {
        self.emit_void_inst(InstKind::SStore(slot, value))
    }

    /// Emits a tload instruction.
    pub(crate) fn tload(&mut self, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::TLoad(slot), Some(MirType::uint256()))
    }

    /// Emits a tstore instruction.
    pub(crate) fn tstore(&mut self, slot: ValueId, value: ValueId) {
        self.emit_void_inst(InstKind::TStore(slot, value))
    }

    /// Emits a calldataload instruction.
    pub(crate) fn calldataload(&mut self, offset: ValueId) -> ValueId {
        self.emit_inst(InstKind::CalldataLoad(offset), Some(MirType::uint256()))
    }

    /// Emits a calldatasize instruction.
    pub(crate) fn calldatasize(&mut self) -> ValueId {
        self.emit_inst(InstKind::CalldataSize, Some(MirType::uint256()))
    }

    /// Constructs a logical `(pointer, length, location)` slice.
    pub(crate) fn make_slice(
        &mut self,
        ptr: ValueId,
        len: ValueId,
        location: SliceLocation,
    ) -> ValueId {
        self.emit_inst(InstKind::MakeSlice { ptr, len, location }, Some(MirType::Slice(location)))
    }

    /// Projects the data pointer from a slice.
    pub(crate) fn slice_ptr(&mut self, slice: ValueId) -> ValueId {
        self.emit_inst(InstKind::SlicePtr(slice), Some(MirType::uint256()))
    }

    /// Projects the logical length from a slice.
    pub(crate) fn slice_len(&mut self, slice: ValueId) -> ValueId {
        self.emit_inst(InstKind::SliceLen(slice), Some(MirType::uint256()))
    }

    /// Emits the base address of the constructor ABI argument blob.
    pub(crate) fn constructor_args_base(&mut self) -> ValueId {
        self.emit_inst(InstKind::ConstructorArgsBase, Some(MirType::uint256()))
    }

    /// Emits the end address of the constructor ABI argument blob.
    pub(crate) fn constructor_args_end(&mut self) -> ValueId {
        self.emit_inst(InstKind::ConstructorArgsEnd, Some(MirType::uint256()))
    }

    /// Emits a calldatacopy instruction.
    pub(crate) fn calldatacopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::CalldataCopy(dest, offset, size))
    }

    /// Emits a constant-data copy.
    pub(crate) fn data_copy(&mut self, data: crate::mir::DataRef, dest: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::DataCopy(data, dest, size))
    }
    /// Emits a calldatacopy whose destination is proven to be in the heap.
    pub(crate) fn calldatacopy_heap(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst_in_region(
            InstKind::CalldataCopy(dest, offset, size),
            MemoryRegion::Heap,
        )
    }

    /// Emits a codesize instruction.
    pub(crate) fn codesize(&mut self) -> ValueId {
        self.emit_inst(InstKind::CodeSize, Some(MirType::uint256()))
    }

    /// Emits an extcodesize instruction.
    pub(crate) fn extcodesize(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::ExtCodeSize(addr), Some(MirType::uint256()))
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
        )
    }

    /// Emits an extcodehash instruction.
    pub(crate) fn extcodehash(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::ExtCodeHash(addr), Some(MirType::uint256()))
    }

    /// Emits a returndatasize instruction.
    ///
    /// Emits the raw volatile `returndatasize()` query.
    pub(crate) fn returndatasize(&mut self) -> ValueId {
        self.emit_inst(InstKind::ReturnDataSize, Some(MirType::uint256()))
    }

    /// Emits a returndatacopy instruction.
    pub(crate) fn returndatacopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::ReturnDataCopy(dest, offset, size))
    }

    /// Copies bytes from a logical slice's address space into memory.
    pub(crate) fn copy_slice_data(
        &mut self,
        location: SliceLocation,
        dest: ValueId,
        source: ValueId,
        size: ValueId,
    ) {
        match location {
            SliceLocation::Memory => self.mcopy_heap(dest, source, size),
            SliceLocation::Calldata => self.calldatacopy_heap(dest, source, size),
            SliceLocation::Returndata => self.returndatacopy_heap(dest, source, size),
        }
    }

    /// Emits a returndatacopy whose destination is proven to be in the heap.
    pub(crate) fn returndatacopy_heap(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst_in_region(
            InstKind::ReturnDataCopy(dest, offset, size),
            MemoryRegion::Heap,
        )
    }

    /// Emits a returndata copy that feeds an external return or revert.
    pub(crate) fn returndatacopy_abi_return(
        &mut self,
        dest: ValueId,
        offset: ValueId,
        size: ValueId,
    ) {
        self.emit_void_inst_in_region(
            InstKind::ReturnDataCopy(dest, offset, size),
            MemoryRegion::AbiReturn,
        )
    }

    /// Emits an internal function call.
    pub(crate) fn icall(
        &mut self,
        function: FunctionId,
        args: Vec<ValueId>,
        result_ty: MirType,
        returns: usize,
    ) -> ValueId {
        let returns = u32::try_from(returns).expect("too many internal call return values");
        self.emit_inst(InstKind::ICall { function, args: args.into(), returns }, Some(result_ty))
    }

    /// Emits an internal function call whose result, if any, is not used as a value.
    pub(crate) fn icall_void(&mut self, function: FunctionId, args: Vec<ValueId>, returns: usize) {
        let returns = u32::try_from(returns).expect("too many internal call return values");
        self.emit_void_inst(InstKind::ICall { function, args: args.into(), returns });
    }

    /// Emits an address inside the current internal-call frame.
    pub(crate) fn internal_frame_addr(&mut self, offset: u64) -> ValueId {
        self.emit_inst(InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
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

    /// Emits a caller instruction.
    pub(crate) fn caller(&mut self) -> ValueId {
        self.emit_inst(InstKind::Caller, Some(MirType::Address))
    }

    /// Emits a callvalue instruction.
    pub(crate) fn callvalue(&mut self) -> ValueId {
        self.emit_inst(InstKind::CallValue, Some(MirType::uint256()))
    }

    /// Emits an origin instruction.
    pub(crate) fn origin(&mut self) -> ValueId {
        self.emit_inst(InstKind::Origin, Some(MirType::Address))
    }

    /// Emits a gasprice instruction.
    pub(crate) fn gasprice(&mut self) -> ValueId {
        self.emit_inst(InstKind::GasPrice, Some(MirType::uint256()))
    }

    /// Emits a blockhash instruction.
    pub(crate) fn blockhash(&mut self, block_num: ValueId) -> ValueId {
        self.emit_inst(InstKind::BlockHash(block_num), Some(MirType::bytes32()))
    }

    /// Emits a coinbase instruction.
    pub(crate) fn coinbase(&mut self) -> ValueId {
        self.emit_inst(InstKind::Coinbase, Some(MirType::Address))
    }

    /// Emits a timestamp instruction.
    pub(crate) fn timestamp(&mut self) -> ValueId {
        self.emit_inst(InstKind::Timestamp, Some(MirType::uint256()))
    }

    /// Emits a number instruction.
    pub(crate) fn number(&mut self) -> ValueId {
        self.emit_inst(InstKind::BlockNumber, Some(MirType::uint256()))
    }

    /// Emits a prevrandao instruction.
    pub(crate) fn prevrandao(&mut self) -> ValueId {
        self.emit_inst(InstKind::PrevRandao, Some(MirType::uint256()))
    }

    /// Emits a gaslimit instruction.
    pub(crate) fn gaslimit(&mut self) -> ValueId {
        self.emit_inst(InstKind::GasLimit, Some(MirType::uint256()))
    }

    pub(crate) fn slotnum(&mut self) -> ValueId {
        self.emit_inst(InstKind::SlotNum, Some(MirType::uint256()))
    }

    /// Emits a chainid instruction.
    pub(crate) fn chainid(&mut self) -> ValueId {
        self.emit_inst(InstKind::ChainId, Some(MirType::uint256()))
    }

    /// Emits an address instruction.
    pub(crate) fn address(&mut self) -> ValueId {
        self.emit_inst(InstKind::Address, Some(MirType::Address))
    }

    /// Emits a balance instruction.
    pub(crate) fn balance(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::Balance(addr), Some(MirType::uint256()))
    }

    /// Emits a selfbalance instruction.
    pub(crate) fn selfbalance(&mut self) -> ValueId {
        self.emit_inst(InstKind::SelfBalance, Some(MirType::uint256()))
    }

    /// Emits a gas instruction.
    pub(crate) fn gas(&mut self) -> ValueId {
        self.emit_inst(InstKind::Gas, Some(MirType::uint256()))
    }

    /// Emits a keccak256 instruction.
    pub(crate) fn keccak256(&mut self, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::Keccak256(offset, size), Some(MirType::bytes32()))
    }

    /// Hashes a `memorybytes` object's contents. Expanded by
    /// `lower-memory-objects` into the physical length load, data pointer, and
    /// `keccak256`.
    pub(crate) fn keccak256_bytes(&mut self, object: ValueId) -> ValueId {
        self.emit_inst(InstKind::Keccak256Bytes(object), Some(MirType::bytes32()))
    }

    /// Emits a fixed-width mapping-slot hash builtin.
    pub(crate) fn mapping_slot(&mut self, key: ValueId, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::MappingSlot(key, slot), Some(MirType::bytes32()))
    }

    /// Emits a memory-backed dynamic mapping-slot hash builtin.
    pub(crate) fn mapping_slot_memory(&mut self, key: ValueId, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::MappingSlotMemory(key, slot), Some(MirType::bytes32()))
    }

    /// Emits a calldata-backed dynamic mapping-slot hash builtin.
    pub(crate) fn mapping_slot_calldata(&mut self, key: ValueId, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::MappingSlotCalldata(key, slot), Some(MirType::bytes32()))
    }

    /// Resolves the first data slot of a dynamic storage array.
    pub(crate) fn storage_array_data_slot(&mut self, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::StorageArrayDataSlot(slot), Some(MirType::bytes32()))
    }

    /// Emits a basefee instruction.
    pub(crate) fn basefee(&mut self) -> ValueId {
        self.emit_inst(InstKind::BaseFee, Some(MirType::uint256()))
    }

    /// Emits a blobbasefee instruction.
    pub(crate) fn blobbasefee(&mut self) -> ValueId {
        self.emit_inst(InstKind::BlobBaseFee, Some(MirType::uint256()))
    }

    /// Emits a blobhash instruction.
    pub(crate) fn blobhash(&mut self, index: ValueId) -> ValueId {
        self.emit_inst(InstKind::BlobHash(index), Some(MirType::bytes32()))
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
            Some(MirType::uint256()),
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
            Some(MirType::uint256()),
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
            Some(MirType::uint256()),
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
            Some(MirType::uint256()),
        )
    }

    /// Emits a create instruction (deploy a contract).
    pub(crate) fn create(&mut self, value: ValueId, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::Create(value, offset, size), Some(MirType::Address))
    }

    /// Emits a create2 instruction (deploy a contract with salt).
    pub(crate) fn create2(
        &mut self,
        value: ValueId,
        offset: ValueId,
        size: ValueId,
        salt: ValueId,
    ) -> ValueId {
        self.emit_inst(InstKind::Create2(value, offset, size, salt), Some(MirType::Address))
    }

    /// Emits a codecopy instruction.
    pub(crate) fn codecopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::CodeCopy(dest, offset, size))
    }

    /// Emits a log0 instruction (event with no topics).
    pub(crate) fn log0(&mut self, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::Log0(offset, size));
    }

    /// Emits a log1 instruction (event with 1 topic).
    pub(crate) fn log1(&mut self, offset: ValueId, size: ValueId, topic1: ValueId) {
        self.emit_void_inst(InstKind::Log1(offset, size, topic1));
    }

    /// Emits a log2 instruction (event with 2 topics).
    pub(crate) fn log2(
        &mut self,
        offset: ValueId,
        size: ValueId,
        topic1: ValueId,
        topic2: ValueId,
    ) {
        self.emit_void_inst(InstKind::Log2(offset, size, topic1, topic2));
    }

    /// Emits a log3 instruction (event with 3 topics).
    pub(crate) fn log3(
        &mut self,
        offset: ValueId,
        size: ValueId,
        topic1: ValueId,
        topic2: ValueId,
        topic3: ValueId,
    ) {
        self.emit_void_inst(InstKind::Log3(offset, size, topic1, topic2, topic3));
    }

    /// Emits a log4 instruction (event with 4 topics).
    pub(crate) fn log4(
        &mut self,
        offset: ValueId,
        size: ValueId,
        topic1: ValueId,
        topic2: ValueId,
        topic3: ValueId,
        topic4: ValueId,
    ) {
        self.emit_void_inst(InstKind::Log4(offset, size, topic1, topic2, topic3, topic4));
    }

    /// Emits a select instruction.
    pub(crate) fn select(
        &mut self,
        cond: ValueId,
        then_val: ValueId,
        else_val: ValueId,
    ) -> ValueId {
        self.emit_inst(InstKind::Select(cond, then_val, else_val), Some(MirType::uint256()))
    }

    /// Emits a phi instruction. `incoming` pairs each predecessor block of the
    /// current block with the value the phi takes when control arrives from
    /// that block. Emit phis before any other instruction in their block.
    pub(crate) fn phi(&mut self, incoming: Vec<(BlockId, ValueId)>) -> ValueId {
        let ty = incoming
            .first()
            .and_then(|(_, value)| self.func.value_ty(*value))
            .unwrap_or(MirType::uint256());
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
        let values: SmallVec<[ValueId; 2]> = values.into_iter().collect();
        self.set_terminator(Terminator::Return { values });
    }

    /// Sets a revert terminator.
    pub(crate) fn revert(&mut self, offset: ValueId, size: ValueId) {
        self.set_terminator(Terminator::Revert { offset, size });
    }

    /// Sets a returndata-bubbling revert terminator.
    pub(crate) fn revert_returndata(&mut self) {
        self.set_terminator(Terminator::RevertReturndata);
    }

    /// Sets a return-data terminator: `RETURN(offset, size)`.
    pub(crate) fn ret_data(&mut self, offset: ValueId, size: ValueId) {
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
        self.set_terminator(Terminator::SelfDestruct { recipient });
    }

    /// Sets a fully constructed terminator on the current block.
    pub(crate) fn set_terminator(&mut self, terminator: Terminator) {
        let current = self.current_block;
        for successor in terminator.successors() {
            self.func.blocks[successor].predecessors.push(current);
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
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn to_uint_accepts_primitives() {
        let _ = 1_u8.to_uint();
        let _ = 1_u16.to_uint();
        let _ = 1_u32.to_uint();
        let _ = 1_u64.to_uint();
        let _ = 1_u128.to_uint();
        let _ = 1_usize.to_uint();
        let _ = 1_i8.to_uint();
        let _ = 1_i16.to_uint();
        let _ = 1_i32.to_uint();
        let _ = 1_i64.to_uint();
        let _ = 1_i128.to_uint();
        let _ = 1_isize.to_uint();
        let _ = U256::from(1).to_uint();
        assert_eq!((-1_i8).to_uint(), U256::MAX);
    }
}

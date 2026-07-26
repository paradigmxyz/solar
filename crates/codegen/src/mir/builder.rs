//! MIR function builder.

use super::{
    AllocationSemantics, BlockId, Function, FunctionId, Immediate, InstId, InstKind, Instruction,
    MemoryRegion, MirType, SliceLocation, StorageAlias, Terminator, Value, ValueId,
};
use crate::memory::EvmMemoryLayout;
use alloy_primitives::U256;
use smallvec::SmallVec;

/// A builder for constructing MIR functions.
pub(crate) struct FunctionBuilder<'a> {
    /// The function being built.
    func: &'a mut Function,
    /// The current block.
    current_block: BlockId,
}

impl<'a> FunctionBuilder<'a> {
    /// Creates a new function builder.
    pub(crate) fn new(func: &'a mut Function) -> Self {
        Self { func, current_block: BlockId::ENTRY }
    }

    /// Returns the current block.
    #[must_use]
    pub(crate) const fn current_block(&self) -> BlockId {
        self.current_block
    }

    /// Switches to a different block.
    pub(crate) fn switch_to_block(&mut self, block: BlockId) {
        self.current_block = block;
    }

    /// Creates a new basic block.
    pub(crate) fn create_block(&mut self) -> BlockId {
        self.func.alloc_block()
    }

    /// Adds an argument to the function.
    pub(crate) fn add_param(&mut self, ty: MirType) -> ValueId {
        let index = self.func.params.len() as u32;
        self.func.params.push(ty);
        self.alloc_value(Value::Arg { index, ty })
    }

    /// Adds a return type to the function.
    pub(crate) fn add_return(&mut self, ty: MirType) {
        self.func.returns.push(ty);
    }

    /// Creates an immediate value.
    pub(crate) fn imm_u256(&mut self, value: U256) -> ValueId {
        self.alloc_value(Value::Immediate(Immediate::uint256(value)))
    }

    /// Creates a u64 immediate value.
    pub(crate) fn imm_u64(&mut self, value: u64) -> ValueId {
        self.imm_u256(U256::from(value))
    }

    /// Creates a boolean immediate.
    pub(crate) fn imm_bool(&mut self, value: bool) -> ValueId {
        self.alloc_value(Value::Immediate(Immediate::bool(value)))
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

    /// Replaces an allocated value.
    pub(crate) fn set_value(&mut self, id: ValueId, value: Value) {
        self.func.values[id] = value;
    }

    fn emit_inst_raw(&mut self, kind: InstKind, result_ty: Option<MirType>) -> InstId {
        let mut inst = Instruction::new(kind, result_ty);
        inst.metadata.set_effect(Some(inst.kind.effect_kind()));
        inst.metadata.set_memory_region(self.memory_region_for_inst(&inst.kind));
        inst.metadata.set_storage_alias(self.storage_alias_for_inst(&inst.kind));
        self.append_instruction(inst)
    }

    /// Appends a fully constructed instruction to the current block.
    pub(crate) fn append_instruction(&mut self, inst: Instruction) -> InstId {
        let inst_id = self.func.alloc_inst(inst);
        self.func.blocks[self.current_block].instructions.push(inst_id);
        inst_id
    }

    fn emit_inst(&mut self, kind: InstKind, result_ty: Option<MirType>) -> ValueId {
        debug_assert!(result_ty.is_some(), "value-producing instructions must have a result type");
        let inst_id = self.emit_inst_raw(kind, result_ty);
        self.alloc_value(Value::Inst(inst_id))
    }

    /// Emits an instruction that produces no value, such as a store or a log.
    ///
    /// No result [`Value`] is allocated: only value-producing instructions get
    /// an entry in the function's value table.
    fn emit_void_inst(&mut self, kind: InstKind) {
        self.emit_inst_raw(kind, None);
    }

    fn memory_region_for_inst(&self, kind: &InstKind) -> Option<MemoryRegion> {
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
            Value::Arg { .. } | Value::Immediate(_) | Value::Undef(_) | Value::Error(_) => {
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

    /// Emits an msize instruction.
    pub(crate) fn msize(&mut self) -> ValueId {
        self.emit_inst(InstKind::MSize, Some(MirType::uint256()))
    }

    /// Reads the free-memory pointer.
    pub(crate) fn fmp(&mut self) -> ValueId {
        self.emit_inst(InstKind::Fmp, Some(MirType::MemPtr))
    }

    /// Reads the free-memory pointer as the base of a semantic object being built in place.
    pub(crate) fn fmp_object(&mut self, layout: crate::mir::MemoryObjectLayout) -> ValueId {
        self.emit_inst(InstKind::Fmp, Some(MirType::MemoryObject(layout.kind())))
    }

    /// Sets the free-memory pointer.
    pub(crate) fn set_fmp(&mut self, ptr: ValueId) {
        self.emit_void_inst(InstKind::SetFmp(ptr))
    }

    /// Reserves memory under an explicit semantic policy.
    pub(crate) fn alloc(&mut self, size: ValueId, semantics: AllocationSemantics) -> ValueId {
        self.alloc_kind(size, crate::mir::AllocationKind::Raw, semantics)
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

    /// Addresses a direct struct field.
    pub(crate) fn memory_object_field_addr(
        &mut self,
        object: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        field: u64,
    ) -> ValueId {
        self.emit_inst(
            InstKind::MemoryObjectFieldAddr { object, layout, field },
            Some(MirType::MemPtr),
        )
    }

    /// Addresses an array element.
    pub(crate) fn memory_object_element_addr(
        &mut self,
        object: ValueId,
        layout: crate::mir::MemoryObjectLayout,
        index: ValueId,
    ) -> ValueId {
        self.emit_inst(
            InstKind::MemoryObjectElementAddr { object, layout, index },
            Some(MirType::MemPtr),
        )
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
        self.emit_inst(
            InstKind::AbiEncode { selector, args: args.into(), layout },
            Some(MirType::Slice(SliceLocation::Memory)),
        )
    }

    /// Copies a statically shaped aggregate from storage into memory.
    pub(crate) fn storage_to_memory(
        &mut self,
        layout: crate::mir::StorageLayoutRef,
        storage: ValueId,
        memory: ValueId,
    ) {
        self.emit_void_inst(InstKind::StorageToMemory { storage, memory, layout })
    }

    /// Copies a statically shaped aggregate from memory into storage.
    pub(crate) fn memory_to_storage(
        &mut self,
        layout: crate::mir::StorageLayoutRef,
        memory: ValueId,
        storage: ValueId,
    ) {
        self.emit_void_inst(InstKind::MemoryToStorage { memory, storage, layout })
    }

    /// Clears every storage slot occupied by a statically shaped aggregate.
    pub(crate) fn clear_storage(&mut self, layout: crate::mir::StorageLayoutRef, storage: ValueId) {
        self.emit_void_inst(InstKind::ClearStorage { storage, layout })
    }

    /// Emits an mcopy instruction.
    pub(crate) fn mcopy(&mut self, dest: ValueId, src: ValueId, len: ValueId) {
        self.emit_void_inst(InstKind::MCopy(dest, src, len))
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

    /// Emits a calldatacopy instruction.
    pub(crate) fn calldatacopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::CalldataCopy(dest, offset, size))
    }

    /// Emits a codesize instruction.
    pub(crate) fn codesize(&mut self) -> ValueId {
        self.emit_inst(InstKind::CodeSize, Some(MirType::uint256()))
    }

    /// Emits an extcodesize instruction.
    pub(crate) fn extcodesize(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::ExtCodeSize(addr), Some(MirType::uint256()))
    }

    /// Emits a loadimmutable instruction for the immutable at `offset`.
    pub(crate) fn load_immutable(&mut self, offset: u32) -> ValueId {
        self.emit_inst(InstKind::LoadImmutable(offset), Some(MirType::uint256()))
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

    /// Emits an extcodehash instruction.
    pub(crate) fn extcodehash(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::ExtCodeHash(addr), Some(MirType::uint256()))
    }

    /// Emits a returndatasize instruction.
    pub(crate) fn returndatasize(&mut self) -> ValueId {
        self.emit_inst(InstKind::ReturnDataSize, Some(MirType::uint256()))
    }

    /// Emits a returndatacopy instruction.
    pub(crate) fn returndatacopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) {
        self.emit_void_inst(InstKind::ReturnDataCopy(dest, offset, size))
    }

    /// Emits an internal function call.
    pub(crate) fn internal_call(
        &mut self,
        function: FunctionId,
        args: Vec<ValueId>,
        result_ty: MirType,
        returns: usize,
    ) -> ValueId {
        let returns = u32::try_from(returns).expect("too many internal call return values");
        self.emit_inst(
            InstKind::InternalCall { function, args: args.into(), returns },
            Some(result_ty),
        )
    }

    /// Emits an internal function call whose result, if any, is not used as a value.
    pub(crate) fn internal_call_void(
        &mut self,
        function: FunctionId,
        args: Vec<ValueId>,
        returns: usize,
    ) {
        let returns = u32::try_from(returns).expect("too many internal call return values");
        self.emit_void_inst(InstKind::InternalCall { function, args: args.into(), returns });
    }

    /// Emits an address inside the current internal-call frame.
    pub(crate) fn internal_frame_addr(&mut self, offset: u64) -> ValueId {
        self.emit_inst(InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
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
        self.emit_inst(InstKind::BlockHash(block_num), Some(MirType::FixedBytes(32)))
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
        self.emit_inst(InstKind::BlobHash(index), Some(MirType::FixedBytes(32)))
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
        self.emit_inst(InstKind::Phi(incoming), Some(MirType::uint256()))
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
        self.func.blocks[current].terminator = Some(terminator);
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

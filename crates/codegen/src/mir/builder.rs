//! MIR function builder.

use super::{
    BlockId, Function, FunctionId, Immediate, InstKind, Instruction, MirType, Terminator, Value,
    ValueId,
};
use alloy_primitives::U256;
use smallvec::SmallVec;

/// A builder for constructing MIR functions.
pub struct FunctionBuilder<'a> {
    /// The function being built.
    func: &'a mut Function,
    /// The current block.
    current_block: BlockId,
}

impl<'a> FunctionBuilder<'a> {
    /// Creates a new function builder.
    pub fn new(func: &'a mut Function) -> Self {
        let entry = func.entry_block;
        Self { func, current_block: entry }
    }

    /// Returns the current block.
    #[must_use]
    pub const fn current_block(&self) -> BlockId {
        self.current_block
    }

    /// Switches to a different block.
    pub fn switch_to_block(&mut self, block: BlockId) {
        self.current_block = block;
    }

    /// Creates a new basic block.
    pub fn create_block(&mut self) -> BlockId {
        self.func.alloc_block()
    }

    /// Adds an argument to the function.
    pub fn add_param(&mut self, ty: MirType) -> ValueId {
        let index = self.func.params.len() as u32;
        self.func.params.push(ty);
        self.func.alloc_value(Value::Arg { index, ty })
    }

    /// Adds a return type to the function.
    pub fn add_return(&mut self, ty: MirType) {
        self.func.returns.push(ty);
    }

    /// Creates an immediate value.
    pub fn imm_u256(&mut self, value: U256) -> ValueId {
        self.func.alloc_value(Value::Immediate(Immediate::uint256(value)))
    }

    /// Creates a u64 immediate value.
    pub fn imm_u64(&mut self, value: u64) -> ValueId {
        self.imm_u256(U256::from(value))
    }

    /// Creates a boolean immediate.
    pub fn imm_bool(&mut self, value: bool) -> ValueId {
        self.func.alloc_value(Value::Immediate(Immediate::bool(value)))
    }

    /// Creates an undefined value.
    pub fn undef(&mut self, ty: MirType) -> ValueId {
        self.func.alloc_value(Value::Undef(ty))
    }

    fn emit_inst(&mut self, kind: InstKind, result_ty: Option<MirType>) -> ValueId {
        let inst_id = self.func.alloc_inst(Instruction::new(kind, result_ty));
        self.func.blocks[self.current_block].instructions.push(inst_id);
        self.func.alloc_value(Value::Inst(inst_id))
    }

    /// Emits an add instruction.
    pub fn add(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Add(a, b), Some(MirType::uint256()))
    }

    /// Emits a sub instruction.
    pub fn sub(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Sub(a, b), Some(MirType::uint256()))
    }

    /// Emits a mul instruction.
    pub fn mul(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Mul(a, b), Some(MirType::uint256()))
    }

    /// Emits a div instruction.
    pub fn div(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Div(a, b), Some(MirType::uint256()))
    }

    /// Emits a sdiv instruction.
    pub fn sdiv(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SDiv(a, b), Some(MirType::int256()))
    }

    /// Emits a mod instruction.
    pub fn mod_(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Mod(a, b), Some(MirType::uint256()))
    }

    /// Emits an addmod instruction.
    pub fn addmod(&mut self, a: ValueId, b: ValueId, n: ValueId) -> ValueId {
        self.emit_inst(InstKind::AddMod(a, b, n), Some(MirType::uint256()))
    }

    /// Emits a mulmod instruction.
    pub fn mulmod(&mut self, a: ValueId, b: ValueId, n: ValueId) -> ValueId {
        self.emit_inst(InstKind::MulMod(a, b, n), Some(MirType::uint256()))
    }

    /// Emits a smod instruction.
    pub fn smod(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SMod(a, b), Some(MirType::int256()))
    }

    /// Emits an exp instruction.
    pub fn exp(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Exp(a, b), Some(MirType::uint256()))
    }

    /// Emits an and instruction.
    pub fn and(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::And(a, b), Some(MirType::uint256()))
    }

    /// Emits an or instruction.
    pub fn or(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Or(a, b), Some(MirType::uint256()))
    }

    /// Emits a xor instruction.
    pub fn xor(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Xor(a, b), Some(MirType::uint256()))
    }

    /// Emits a not instruction.
    pub fn not(&mut self, a: ValueId) -> ValueId {
        self.emit_inst(InstKind::Not(a), Some(MirType::uint256()))
    }

    /// Emits a shl instruction.
    pub fn shl(&mut self, shift: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Shl(shift, value), Some(MirType::uint256()))
    }

    /// Emits a shr instruction.
    pub fn shr(&mut self, shift: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Shr(shift, value), Some(MirType::uint256()))
    }

    /// Emits a sar instruction.
    pub fn sar(&mut self, shift: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Sar(shift, value), Some(MirType::int256()))
    }

    /// Emits a lt instruction.
    pub fn lt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Lt(a, b), Some(MirType::Bool))
    }

    /// Emits a gt instruction.
    pub fn gt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Gt(a, b), Some(MirType::Bool))
    }

    /// Emits a slt instruction.
    pub fn slt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SLt(a, b), Some(MirType::Bool))
    }

    /// Emits a sgt instruction.
    pub fn sgt(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::SGt(a, b), Some(MirType::Bool))
    }

    /// Emits an eq instruction.
    pub fn eq(&mut self, a: ValueId, b: ValueId) -> ValueId {
        self.emit_inst(InstKind::Eq(a, b), Some(MirType::Bool))
    }

    /// Emits an iszero instruction.
    pub fn iszero(&mut self, a: ValueId) -> ValueId {
        self.emit_inst(InstKind::IsZero(a), Some(MirType::Bool))
    }

    /// Emits a byte instruction.
    pub fn byte(&mut self, index: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::Byte(index, value), Some(MirType::uint256()))
    }

    /// Emits a signextend instruction.
    pub fn signextend(&mut self, size: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::SignExtend(size, value), Some(MirType::int256()))
    }

    /// Emits an mload instruction.
    pub fn mload(&mut self, offset: ValueId) -> ValueId {
        self.emit_inst(InstKind::MLoad(offset), Some(MirType::uint256()))
    }

    /// Emits an mstore instruction.
    pub fn mstore(&mut self, offset: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::MStore(offset, value), None)
    }

    /// Emits an mstore8 instruction.
    pub fn mstore8(&mut self, offset: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::MStore8(offset, value), None)
    }

    /// Emits an msize instruction.
    pub fn msize(&mut self) -> ValueId {
        self.emit_inst(InstKind::MSize, Some(MirType::uint256()))
    }

    /// Emits an mcopy instruction.
    pub fn mcopy(&mut self, dest: ValueId, src: ValueId, len: ValueId) -> ValueId {
        self.emit_inst(InstKind::MCopy(dest, src, len), None)
    }

    /// Emits an sload instruction.
    pub fn sload(&mut self, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::SLoad(slot), Some(MirType::uint256()))
    }

    /// Emits an sstore instruction.
    pub fn sstore(&mut self, slot: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::SStore(slot, value), None)
    }

    /// Emits a tload instruction.
    pub fn tload(&mut self, slot: ValueId) -> ValueId {
        self.emit_inst(InstKind::TLoad(slot), Some(MirType::uint256()))
    }

    /// Emits a tstore instruction.
    pub fn tstore(&mut self, slot: ValueId, value: ValueId) -> ValueId {
        self.emit_inst(InstKind::TStore(slot, value), None)
    }

    /// Emits a calldataload instruction.
    pub fn calldataload(&mut self, offset: ValueId) -> ValueId {
        self.emit_inst(InstKind::CalldataLoad(offset), Some(MirType::uint256()))
    }

    /// Emits a calldatasize instruction.
    pub fn calldatasize(&mut self) -> ValueId {
        self.emit_inst(InstKind::CalldataSize, Some(MirType::uint256()))
    }

    /// Emits a calldatacopy instruction.
    pub fn calldatacopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::CalldataCopy(dest, offset, size), None)
    }

    /// Emits a codesize instruction.
    pub fn codesize(&mut self) -> ValueId {
        self.emit_inst(InstKind::CodeSize, Some(MirType::uint256()))
    }

    /// Emits an extcodesize instruction.
    pub fn extcodesize(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::ExtCodeSize(addr), Some(MirType::uint256()))
    }

    /// Emits an extcodecopy instruction.
    pub fn extcodecopy(
        &mut self,
        addr: ValueId,
        dest: ValueId,
        offset: ValueId,
        size: ValueId,
    ) -> ValueId {
        self.emit_inst(InstKind::ExtCodeCopy(addr, dest, offset, size), None)
    }

    /// Emits an extcodehash instruction.
    pub fn extcodehash(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::ExtCodeHash(addr), Some(MirType::uint256()))
    }

    /// Emits a returndatasize instruction.
    pub fn returndatasize(&mut self) -> ValueId {
        self.emit_inst(InstKind::ReturnDataSize, Some(MirType::uint256()))
    }

    /// Emits a returndatacopy instruction.
    pub fn returndatacopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::ReturnDataCopy(dest, offset, size), None)
    }

    /// Emits an internal function call.
    pub fn internal_call(
        &mut self,
        function: FunctionId,
        args: Vec<ValueId>,
        result_ty: Option<MirType>,
        returns: usize,
    ) -> ValueId {
        self.emit_inst(InstKind::InternalCall { function, args, returns }, result_ty)
    }

    /// Emits an address inside the current internal-call frame.
    pub fn internal_frame_addr(&mut self, offset: u64) -> ValueId {
        self.emit_inst(InstKind::InternalFrameAddr(offset), Some(MirType::MemPtr))
    }

    /// Emits a caller instruction.
    pub fn caller(&mut self) -> ValueId {
        self.emit_inst(InstKind::Caller, Some(MirType::Address))
    }

    /// Emits a callvalue instruction.
    pub fn callvalue(&mut self) -> ValueId {
        self.emit_inst(InstKind::CallValue, Some(MirType::uint256()))
    }

    /// Emits an origin instruction.
    pub fn origin(&mut self) -> ValueId {
        self.emit_inst(InstKind::Origin, Some(MirType::Address))
    }

    /// Emits a gasprice instruction.
    pub fn gasprice(&mut self) -> ValueId {
        self.emit_inst(InstKind::GasPrice, Some(MirType::uint256()))
    }

    /// Emits a blockhash instruction.
    pub fn blockhash(&mut self, block_num: ValueId) -> ValueId {
        self.emit_inst(InstKind::BlockHash(block_num), Some(MirType::FixedBytes(32)))
    }

    /// Emits a coinbase instruction.
    pub fn coinbase(&mut self) -> ValueId {
        self.emit_inst(InstKind::Coinbase, Some(MirType::Address))
    }

    /// Emits a timestamp instruction.
    pub fn timestamp(&mut self) -> ValueId {
        self.emit_inst(InstKind::Timestamp, Some(MirType::uint256()))
    }

    /// Emits a number instruction.
    pub fn number(&mut self) -> ValueId {
        self.emit_inst(InstKind::BlockNumber, Some(MirType::uint256()))
    }

    /// Emits a prevrandao instruction.
    pub fn prevrandao(&mut self) -> ValueId {
        self.emit_inst(InstKind::PrevRandao, Some(MirType::uint256()))
    }

    /// Emits a gaslimit instruction.
    pub fn gaslimit(&mut self) -> ValueId {
        self.emit_inst(InstKind::GasLimit, Some(MirType::uint256()))
    }

    /// Emits a chainid instruction.
    pub fn chainid(&mut self) -> ValueId {
        self.emit_inst(InstKind::ChainId, Some(MirType::uint256()))
    }

    /// Emits an address instruction.
    pub fn address(&mut self) -> ValueId {
        self.emit_inst(InstKind::Address, Some(MirType::Address))
    }

    /// Emits a balance instruction.
    pub fn balance(&mut self, addr: ValueId) -> ValueId {
        self.emit_inst(InstKind::Balance(addr), Some(MirType::uint256()))
    }

    /// Emits a selfbalance instruction.
    pub fn selfbalance(&mut self) -> ValueId {
        self.emit_inst(InstKind::SelfBalance, Some(MirType::uint256()))
    }

    /// Emits a gas instruction.
    pub fn gas(&mut self) -> ValueId {
        self.emit_inst(InstKind::Gas, Some(MirType::uint256()))
    }

    /// Emits a keccak256 instruction.
    pub fn keccak256(&mut self, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::Keccak256(offset, size), Some(MirType::bytes32()))
    }

    /// Emits a basefee instruction.
    pub fn basefee(&mut self) -> ValueId {
        self.emit_inst(InstKind::BaseFee, Some(MirType::uint256()))
    }

    /// Emits a blobbasefee instruction.
    pub fn blobbasefee(&mut self) -> ValueId {
        self.emit_inst(InstKind::BlobBaseFee, Some(MirType::uint256()))
    }

    /// Emits a blobhash instruction.
    pub fn blobhash(&mut self, index: ValueId) -> ValueId {
        self.emit_inst(InstKind::BlobHash(index), Some(MirType::FixedBytes(32)))
    }

    /// Emits a call instruction (external call).
    #[allow(clippy::too_many_arguments)]
    pub fn call(
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
    pub fn staticcall(
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
    pub fn delegatecall(
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
    pub fn create(&mut self, value: ValueId, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::Create(value, offset, size), Some(MirType::Address))
    }

    /// Emits a create2 instruction (deploy a contract with salt).
    pub fn create2(
        &mut self,
        value: ValueId,
        offset: ValueId,
        size: ValueId,
        salt: ValueId,
    ) -> ValueId {
        self.emit_inst(InstKind::Create2(value, offset, size, salt), Some(MirType::Address))
    }

    /// Emits a codecopy instruction.
    pub fn codecopy(&mut self, dest: ValueId, offset: ValueId, size: ValueId) -> ValueId {
        self.emit_inst(InstKind::CodeCopy(dest, offset, size), None)
    }

    /// Emits a log0 instruction (event with no topics).
    pub fn log0(&mut self, offset: ValueId, size: ValueId) {
        self.emit_inst(InstKind::Log0(offset, size), None);
    }

    /// Emits a log1 instruction (event with 1 topic).
    pub fn log1(&mut self, offset: ValueId, size: ValueId, topic1: ValueId) {
        self.emit_inst(InstKind::Log1(offset, size, topic1), None);
    }

    /// Emits a log2 instruction (event with 2 topics).
    pub fn log2(&mut self, offset: ValueId, size: ValueId, topic1: ValueId, topic2: ValueId) {
        self.emit_inst(InstKind::Log2(offset, size, topic1, topic2), None);
    }

    /// Emits a log3 instruction (event with 3 topics).
    pub fn log3(
        &mut self,
        offset: ValueId,
        size: ValueId,
        topic1: ValueId,
        topic2: ValueId,
        topic3: ValueId,
    ) {
        self.emit_inst(InstKind::Log3(offset, size, topic1, topic2, topic3), None);
    }

    /// Emits a log4 instruction (event with 4 topics).
    pub fn log4(
        &mut self,
        offset: ValueId,
        size: ValueId,
        topic1: ValueId,
        topic2: ValueId,
        topic3: ValueId,
        topic4: ValueId,
    ) {
        self.emit_inst(InstKind::Log4(offset, size, topic1, topic2, topic3, topic4), None);
    }

    /// Emits a select instruction.
    pub fn select(&mut self, cond: ValueId, then_val: ValueId, else_val: ValueId) -> ValueId {
        self.emit_inst(InstKind::Select(cond, then_val, else_val), Some(MirType::uint256()))
    }

    /// Emits a phi instruction.
    pub fn phi(&mut self, ty: MirType, incoming: Vec<(BlockId, ValueId)>) -> ValueId {
        self.func.alloc_value(Value::Phi { ty, incoming })
    }

    /// Sets a jump terminator.
    pub fn jump(&mut self, target: BlockId) {
        let block = &mut self.func.blocks[self.current_block];
        block.terminator = Some(Terminator::Jump(target));
        block.successors.push(target);
        self.func.blocks[target].predecessors.push(self.current_block);
    }

    /// Sets a branch terminator.
    pub fn branch(&mut self, condition: ValueId, then_block: BlockId, else_block: BlockId) {
        let block = &mut self.func.blocks[self.current_block];
        block.terminator = Some(Terminator::Branch { condition, then_block, else_block });
        block.successors.push(then_block);
        block.successors.push(else_block);
        self.func.blocks[then_block].predecessors.push(self.current_block);
        self.func.blocks[else_block].predecessors.push(self.current_block);
    }

    /// Sets a switch terminator.
    pub fn switch(&mut self, value: ValueId, default: BlockId, cases: Vec<(ValueId, BlockId)>) {
        let current = self.current_block;
        self.func.blocks[current].terminator =
            Some(Terminator::Switch { value, default, cases: cases.clone() });
        self.func.blocks[current].successors.push(default);
        self.func.blocks[default].predecessors.push(current);
        for (_, case_block) in cases {
            self.func.blocks[current].successors.push(case_block);
            self.func.blocks[case_block].predecessors.push(current);
        }
    }

    /// Sets a return terminator.
    pub fn ret(&mut self, values: impl IntoIterator<Item = ValueId>) {
        let values: SmallVec<[ValueId; 2]> = values.into_iter().collect();
        self.func.blocks[self.current_block].terminator = Some(Terminator::Return { values });
    }

    /// Sets a revert terminator.
    pub fn revert(&mut self, offset: ValueId, size: ValueId) {
        self.func.blocks[self.current_block].terminator = Some(Terminator::Revert { offset, size });
    }

    /// Sets a return-data terminator: `RETURN(offset, size)`.
    pub fn ret_data(&mut self, offset: ValueId, size: ValueId) {
        self.func.blocks[self.current_block].terminator =
            Some(Terminator::ReturnData { offset, size });
    }

    /// Sets a stop terminator.
    pub fn stop(&mut self) {
        self.func.blocks[self.current_block].terminator = Some(Terminator::Stop);
    }

    /// Sets an invalid terminator.
    pub fn invalid(&mut self) {
        self.func.blocks[self.current_block].terminator = Some(Terminator::Invalid);
    }

    /// Sets a selfdestruct terminator.
    pub fn selfdestruct(&mut self, recipient: ValueId) {
        self.func.blocks[self.current_block].terminator =
            Some(Terminator::SelfDestruct { recipient });
    }

    /// Returns a reference to the function.
    #[must_use]
    pub fn func(&self) -> &Function {
        self.func
    }

    /// Returns a mutable reference to the function.
    pub fn func_mut(&mut self) -> &mut Function {
        self.func
    }
}

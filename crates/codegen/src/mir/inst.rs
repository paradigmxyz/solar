//! MIR instructions.

use super::{BlockId, Function, FunctionId, MirType, Value, ValueId};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_interface::Span;
use std::fmt;

/// Extra information attached to a MIR instruction by lowering or analysis passes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstructionMetadata {
    /// Proven storage alias key for `sload`/`sstore` instructions.
    pub storage_alias: Option<StorageAlias>,
    /// Proven memory region for memory instructions.
    pub memory_region: Option<MemoryRegion>,
    /// HIR expression that produced this instruction, when the lowerer can preserve it.
    pub hir_expr: Option<u32>,
    /// Source span that produced this instruction, when the lowerer can preserve it.
    pub source_span: Option<Span>,
    /// Whether this instruction was lowered from an unchecked arithmetic context.
    pub unchecked: bool,
    /// Loop nesting depth attached by loop-aware analyses.
    pub loop_depth: u16,
    /// Conservative effect classification attached by lowering or analysis.
    pub effect: Option<EffectKind>,
}

impl InstructionMetadata {
    /// Empty instruction metadata.
    pub const EMPTY: Self = Self {
        storage_alias: None,
        memory_region: None,
        hir_expr: None,
        source_span: None,
        unchecked: false,
        loop_depth: 0,
        effect: None,
    };
}

/// A conservative storage alias key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StorageAlias {
    /// A known absolute storage slot.
    Slot(U256),
    /// A loop-invariant symbolic slot value.
    Symbolic(ValueId),
    /// A loop-invariant symbolic base plus a known constant offset.
    Offset {
        /// Symbolic base slot.
        base: ValueId,
        /// Constant offset added to the base.
        offset: U256,
    },
}

impl StorageAlias {
    /// Computes a conservative exact storage alias key for `value`.
    #[must_use]
    pub fn for_value(func: &Function, value: ValueId) -> Self {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256().map_or(Self::Symbolic(value), Self::Slot),
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind {
                InstKind::Add(lhs, rhs) => {
                    if let Some(offset) = Self::immediate_u256(func, rhs) {
                        Self::add_offset(func, lhs, offset)
                    } else if let Some(offset) = Self::immediate_u256(func, lhs) {
                        Self::add_offset(func, rhs, offset)
                    } else {
                        Self::Symbolic(value)
                    }
                }
                InstKind::Sub(lhs, rhs) => {
                    if let Some(offset) = Self::immediate_u256(func, rhs) {
                        Self::add_offset(func, lhs, U256::ZERO.wrapping_sub(offset))
                    } else {
                        Self::Symbolic(value)
                    }
                }
                _ => Self::Symbolic(value),
            },
            Value::Arg { .. } | Value::Phi { .. } | Value::Undef(_) => Self::Symbolic(value),
        }
    }

    /// Returns true if two alias keys may refer to the same storage slot.
    #[must_use]
    pub fn may_alias(self, other: Self) -> bool {
        match (self, other) {
            (Self::Slot(a), Self::Slot(b)) => a == b,
            (
                Self::Offset { base: a, offset: a_offset },
                Self::Offset { base: b, offset: b_offset },
            ) if a == b => a_offset == b_offset,
            (Self::Symbolic(_), Self::Symbolic(_)) => true,
            (Self::Symbolic(a), Self::Offset { base, offset })
            | (Self::Offset { base, offset }, Self::Symbolic(a))
                if a == base =>
            {
                offset.is_zero()
            }
            _ => true,
        }
    }

    /// Returns the symbolic base value, if this alias has one.
    #[must_use]
    pub const fn symbolic_base(self) -> Option<ValueId> {
        match self {
            Self::Symbolic(value) | Self::Offset { base: value, .. } => Some(value),
            Self::Slot(_) => None,
        }
    }

    fn add_offset(func: &Function, value: ValueId, offset: U256) -> Self {
        match Self::for_value(func, value) {
            Self::Slot(slot) => Self::Slot(slot.wrapping_add(offset)),
            Self::Symbolic(base) if offset.is_zero() => Self::Symbolic(base),
            Self::Symbolic(base) => Self::Offset { base, offset },
            Self::Offset { base, offset: existing } => {
                let offset = existing.wrapping_add(offset);
                if offset.is_zero() { Self::Symbolic(base) } else { Self::Offset { base, offset } }
            }
        }
    }

    fn immediate_u256(func: &Function, value: ValueId) -> Option<U256> {
        match func.value(value) {
            Value::Immediate(imm) => imm.as_u256(),
            _ => None,
        }
    }
}

/// A coarse memory region understood by MIR analyses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum MemoryRegion {
    /// Compiler-owned low-memory scratch space.
    Scratch,
    /// External ABI return buffer.
    AbiReturn,
    /// Solidity free-memory heap.
    Heap,
    /// Internal-call frame memory.
    InternalFrame,
    /// Region is known to be memory, but not classified more precisely.
    Unknown,
}

/// Conservative side-effect class for an instruction.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EffectKind {
    /// Pure computation.
    Pure,
    /// Memory read.
    MemoryRead,
    /// Memory write.
    MemoryWrite,
    /// Persistent storage read.
    StorageRead,
    /// Persistent storage write.
    StorageWrite,
    /// Transient storage read.
    TransientRead,
    /// Transient storage write.
    TransientWrite,
    /// Read from calldata, code, return data, or block/account environment.
    EnvironmentRead,
    /// External call.
    ExternalCall,
    /// Internal MIR call.
    InternalCall,
    /// Contract creation.
    Create,
    /// Event emission.
    Log,
}

/// An instruction in the MIR.
#[derive(Clone, Debug)]
pub struct Instruction {
    /// The kind of instruction.
    pub kind: InstKind,
    /// The result type (if any).
    pub result_ty: Option<MirType>,
    /// Metadata produced by lowering or analysis.
    pub metadata: InstructionMetadata,
}

impl Instruction {
    /// Creates a new instruction.
    #[must_use]
    pub const fn new(kind: InstKind, result_ty: Option<MirType>) -> Self {
        Self { kind, result_ty, metadata: InstructionMetadata::EMPTY }
    }

    /// Returns the operands of this instruction.
    #[must_use]
    pub fn operands(&self) -> SmallVec<[ValueId; 8]> {
        self.kind.operands()
    }

    /// Returns the metadata effect, or derives a conservative one from the instruction kind.
    #[must_use]
    pub fn effect_kind(&self) -> EffectKind {
        self.metadata.effect.unwrap_or_else(|| self.kind.effect_kind())
    }
}

/// The kind of an instruction.
///
/// TODO(codegen): Consider separating opcode and operands once the MIR shape stabilizes, e.g.
/// `Instruction { opcode: Opcode, operands: SmallVec<[ValueId; 4]>, ... }`. That would make generic
/// operand visitors and rewrites less variant-heavy.
#[derive(Clone, Debug)]
pub enum InstKind {
    // Arithmetic operations
    /// Addition: `a + b`
    Add(ValueId, ValueId),
    /// Subtraction: `a - b`
    Sub(ValueId, ValueId),
    /// Multiplication: `a * b`
    Mul(ValueId, ValueId),
    /// Unsigned division: `a / b`
    Div(ValueId, ValueId),
    /// Signed division: `a / b`
    SDiv(ValueId, ValueId),
    /// Unsigned modulo: `a % b`
    Mod(ValueId, ValueId),
    /// Signed modulo: `a % b`
    SMod(ValueId, ValueId),
    /// Exponentiation: `a ** b`
    Exp(ValueId, ValueId),
    /// Add modulo: `(a + b) % n`
    AddMod(ValueId, ValueId, ValueId),
    /// Multiply modulo: `(a * b) % n`
    MulMod(ValueId, ValueId, ValueId),

    // Bitwise operations
    /// Bitwise AND: `a & b`
    And(ValueId, ValueId),
    /// Bitwise OR: `a | b`
    Or(ValueId, ValueId),
    /// Bitwise XOR: `a ^ b`
    Xor(ValueId, ValueId),
    /// Bitwise NOT: `~a`
    Not(ValueId),
    /// Left shift: `a << b`
    Shl(ValueId, ValueId),
    /// Logical right shift: `a >> b`
    Shr(ValueId, ValueId),
    /// Arithmetic right shift: `a >> b` (signed)
    Sar(ValueId, ValueId),
    /// Extract a byte: `byte(i, x)`
    Byte(ValueId, ValueId),

    // Comparison operations
    /// Less than (unsigned): `a < b`
    Lt(ValueId, ValueId),
    /// Greater than (unsigned): `a > b`
    Gt(ValueId, ValueId),
    /// Less than (signed): `a < b`
    SLt(ValueId, ValueId),
    /// Greater than (signed): `a > b`
    SGt(ValueId, ValueId),
    /// Equality: `a == b`
    Eq(ValueId, ValueId),
    /// Check if zero: `a == 0`
    IsZero(ValueId),

    // Memory operations
    /// Load from memory: `mload(offset)`
    MLoad(ValueId),
    /// Store to memory: `mstore(offset, value)`
    MStore(ValueId, ValueId),
    /// Store a single byte: `mstore8(offset, value)`
    MStore8(ValueId, ValueId),
    /// Get memory size: `msize()`
    MSize,
    /// Copy memory: `mcopy(dest, src, len)`
    MCopy(ValueId, ValueId, ValueId),

    // Storage operations
    /// Load from storage: `sload(slot)`
    SLoad(ValueId),
    /// Store to storage: `sstore(slot, value)`
    SStore(ValueId, ValueId),
    /// Transient load: `tload(slot)`
    TLoad(ValueId),
    /// Transient store: `tstore(slot, value)`
    TStore(ValueId, ValueId),

    // Calldata operations
    /// Load from calldata: `calldataload(offset)`
    CalldataLoad(ValueId),
    /// Copy calldata to memory: `calldatacopy(destOffset, offset, size)`
    CalldataCopy(ValueId, ValueId, ValueId),
    /// Get calldata size: `calldatasize()`
    CalldataSize,
    /// Address inside the current internal-call frame.
    InternalFrameAddr(u64),

    // Code operations
    /// Get code size: `codesize()`
    CodeSize,
    /// Copy code to memory: `codecopy(destOffset, offset, size)`
    CodeCopy(ValueId, ValueId, ValueId),
    /// Get external code size: `extcodesize(addr)`
    ExtCodeSize(ValueId),
    /// Copy external code to memory: `extcodecopy(addr, destOffset, offset, size)`
    ExtCodeCopy(ValueId, ValueId, ValueId, ValueId),
    /// Get external code hash: `extcodehash(addr)`
    ExtCodeHash(ValueId),

    // Return data operations
    /// Get return data size: `returndatasize()`
    ReturnDataSize,
    /// Copy return data to memory: `returndatacopy(destOffset, offset, size)`
    ReturnDataCopy(ValueId, ValueId, ValueId),

    // Environment operations
    /// Get caller address: `caller()`
    Caller,
    /// Get call value: `callvalue()`
    CallValue,
    /// Get origin address: `origin()`
    Origin,
    /// Get gas price: `gasprice()`
    GasPrice,
    /// Get block hash: `blockhash(blockNum)`
    BlockHash(ValueId),
    /// Get coinbase address: `coinbase()`
    Coinbase,
    /// Get block timestamp: `timestamp()`
    Timestamp,
    /// Get block number: `number()`
    BlockNumber,
    /// Get previous randao: `prevrandao()`
    PrevRandao,
    /// Get gas limit: `gaslimit()`
    GasLimit,
    /// Get chain ID: `chainid()`
    ChainId,
    /// Get this contract's address: `address()`
    Address,
    /// Get balance: `balance(addr)`
    Balance(ValueId),
    /// Get self balance: `selfbalance()`
    SelfBalance,
    /// Get remaining gas: `gas()`
    Gas,
    /// Get base fee: `basefee()`
    BaseFee,
    /// Get blob base fee: `blobbasefee()`
    BlobBaseFee,
    /// Get blob hash: `blobhash(index)`
    BlobHash(ValueId),

    // Hashing
    /// Keccak256 hash: `keccak256(offset, size)`
    Keccak256(ValueId, ValueId),

    // Call operations
    // TODO(codegen): Consider unifying external calls as one instruction with a call-kind enum
    // and shared operands once the MIR shape stabilizes.
    /// External call: `call(gas, addr, value, argsOffset, argsSize, retOffset, retSize)`
    Call {
        gas: ValueId,
        addr: ValueId,
        value: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Static call: `staticcall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    StaticCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Delegate call: `delegatecall(gas, addr, argsOffset, argsSize, retOffset, retSize)`
    DelegateCall {
        gas: ValueId,
        addr: ValueId,
        args_offset: ValueId,
        args_size: ValueId,
        ret_offset: ValueId,
        ret_size: ValueId,
    },
    /// Internal function call lowered to a direct jump.
    InternalCall { function: FunctionId, args: Vec<ValueId>, returns: usize },

    // Contract creation
    /// Create contract: `create(value, offset, size)`
    Create(ValueId, ValueId, ValueId),
    /// Create2 contract: `create2(value, offset, size, salt)`
    Create2(ValueId, ValueId, ValueId, ValueId),

    // Log operations
    // TODO(codegen): Consider unifying log0..log4 as one instruction with a topic list.
    /// Log with no topics: `log0(offset, size)`
    Log0(ValueId, ValueId),
    /// Log with 1 topic: `log1(offset, size, topic1)`
    Log1(ValueId, ValueId, ValueId),
    /// Log with 2 topics: `log2(offset, size, topic1, topic2)`
    Log2(ValueId, ValueId, ValueId, ValueId),
    /// Log with 3 topics: `log3(offset, size, topic1, topic2, topic3)`
    Log3(ValueId, ValueId, ValueId, ValueId, ValueId),
    /// Log with 4 topics: `log4(offset, size, topic1, topic2, topic3, topic4)`
    Log4(ValueId, ValueId, ValueId, ValueId, ValueId, ValueId),

    // SSA operations
    /// Phi node: merge values from different predecessors.
    Phi(Vec<(BlockId, ValueId)>),
    /// Select: `select(cond, true_val, false_val)`
    Select(ValueId, ValueId, ValueId),

    // Sign extension
    /// Sign extend: `signextend(b, x)` - extends the sign bit from byte position b
    SignExtend(ValueId, ValueId),
}

impl InstKind {
    /// Collects all operands of this instruction into the provided vector.
    /// This is the canonical way to get all operands for liveness analysis.
    pub fn collect_operands(&self, out: &mut SmallVec<[ValueId; 8]>) {
        match self {
            // Binary operations
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::SDiv(a, b)
            | Self::Mod(a, b)
            | Self::SMod(a, b)
            | Self::Exp(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b)
            | Self::Shl(a, b)
            | Self::Shr(a, b)
            | Self::Sar(a, b)
            | Self::Byte(a, b)
            | Self::Lt(a, b)
            | Self::Gt(a, b)
            | Self::SLt(a, b)
            | Self::SGt(a, b)
            | Self::Eq(a, b)
            | Self::MStore(a, b)
            | Self::MStore8(a, b)
            | Self::SStore(a, b)
            | Self::TStore(a, b)
            | Self::Keccak256(a, b)
            | Self::Log0(a, b)
            | Self::SignExtend(a, b) => {
                out.push(*a);
                out.push(*b);
            }

            // Unary operations
            Self::Not(a)
            | Self::IsZero(a)
            | Self::MLoad(a)
            | Self::SLoad(a)
            | Self::TLoad(a)
            | Self::CalldataLoad(a)
            | Self::ExtCodeSize(a)
            | Self::ExtCodeHash(a)
            | Self::Balance(a)
            | Self::BlockHash(a)
            | Self::BlobHash(a) => {
                out.push(*a);
            }

            // Ternary operations
            Self::MCopy(a, b, c)
            | Self::CalldataCopy(a, b, c)
            | Self::CodeCopy(a, b, c)
            | Self::ReturnDataCopy(a, b, c)
            | Self::AddMod(a, b, c)
            | Self::MulMod(a, b, c)
            | Self::Create(a, b, c)
            | Self::Log1(a, b, c)
            | Self::Select(a, b, c) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
            }

            // 4-operand operations
            Self::ExtCodeCopy(a, b, c, d) | Self::Create2(a, b, c, d) | Self::Log2(a, b, c, d) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
                out.push(*d);
            }

            // 5-operand operations
            Self::Log3(a, b, c, d, e) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
                out.push(*d);
                out.push(*e);
            }

            // 6-operand operations
            Self::Log4(a, b, c, d, e, f) => {
                out.push(*a);
                out.push(*b);
                out.push(*c);
                out.push(*d);
                out.push(*e);
                out.push(*f);
            }

            // Call operations
            Self::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                out.push(*gas);
                out.push(*addr);
                out.push(*value);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*ret_offset);
                out.push(*ret_size);
            }
            Self::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                out.push(*gas);
                out.push(*addr);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*ret_offset);
                out.push(*ret_size);
            }
            Self::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                out.push(*gas);
                out.push(*addr);
                out.push(*args_offset);
                out.push(*args_size);
                out.push(*ret_offset);
                out.push(*ret_size);
            }
            Self::InternalCall { args, .. } => {
                out.extend(args.iter().copied());
            }

            // Phi node - operands are the incoming values
            Self::Phi(incoming) => {
                for (_, val) in incoming {
                    out.push(*val);
                }
            }

            // Nullary operations - no operands
            Self::MSize
            | Self::CalldataSize
            | Self::InternalFrameAddr(_)
            | Self::CodeSize
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::ChainId
            | Self::Address
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee => {}
        }
    }

    /// Returns the operands of this instruction.
    #[must_use]
    pub fn operands(&self) -> SmallVec<[ValueId; 8]> {
        let mut out = SmallVec::new();
        self.collect_operands(&mut out);
        out
    }

    /// Visits every operand mutably.
    pub fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
        match self {
            Self::Add(a, b)
            | Self::Sub(a, b)
            | Self::Mul(a, b)
            | Self::Div(a, b)
            | Self::SDiv(a, b)
            | Self::Mod(a, b)
            | Self::SMod(a, b)
            | Self::Exp(a, b)
            | Self::And(a, b)
            | Self::Or(a, b)
            | Self::Xor(a, b)
            | Self::Shl(a, b)
            | Self::Shr(a, b)
            | Self::Sar(a, b)
            | Self::Byte(a, b)
            | Self::Lt(a, b)
            | Self::Gt(a, b)
            | Self::SLt(a, b)
            | Self::SGt(a, b)
            | Self::Eq(a, b)
            | Self::MStore(a, b)
            | Self::MStore8(a, b)
            | Self::SStore(a, b)
            | Self::TStore(a, b)
            | Self::Keccak256(a, b)
            | Self::Log0(a, b)
            | Self::SignExtend(a, b) => {
                f(a);
                f(b);
            }

            Self::Not(a)
            | Self::IsZero(a)
            | Self::MLoad(a)
            | Self::SLoad(a)
            | Self::TLoad(a)
            | Self::CalldataLoad(a)
            | Self::ExtCodeSize(a)
            | Self::ExtCodeHash(a)
            | Self::Balance(a)
            | Self::BlockHash(a)
            | Self::BlobHash(a) => f(a),

            Self::MCopy(a, b, c)
            | Self::CalldataCopy(a, b, c)
            | Self::CodeCopy(a, b, c)
            | Self::ReturnDataCopy(a, b, c)
            | Self::AddMod(a, b, c)
            | Self::MulMod(a, b, c)
            | Self::Create(a, b, c)
            | Self::Log1(a, b, c)
            | Self::Select(a, b, c) => {
                f(a);
                f(b);
                f(c);
            }

            Self::ExtCodeCopy(a, b, c, d) | Self::Create2(a, b, c, d) | Self::Log2(a, b, c, d) => {
                f(a);
                f(b);
                f(c);
                f(d);
            }

            Self::Log3(a, b, c, d, e) => {
                f(a);
                f(b);
                f(c);
                f(d);
                f(e);
            }

            Self::Log4(a, b, c, d, e, g) => {
                f(a);
                f(b);
                f(c);
                f(d);
                f(e);
                f(g);
            }

            Self::Call { gas, addr, value, args_offset, args_size, ret_offset, ret_size } => {
                f(gas);
                f(addr);
                f(value);
                f(args_offset);
                f(args_size);
                f(ret_offset);
                f(ret_size);
            }
            Self::StaticCall { gas, addr, args_offset, args_size, ret_offset, ret_size }
            | Self::DelegateCall { gas, addr, args_offset, args_size, ret_offset, ret_size } => {
                f(gas);
                f(addr);
                f(args_offset);
                f(args_size);
                f(ret_offset);
                f(ret_size);
            }
            Self::InternalCall { args, .. } => {
                for arg in args {
                    f(arg);
                }
            }

            Self::Phi(incoming) => {
                for (_, value) in incoming {
                    f(value);
                }
            }

            Self::MSize
            | Self::CalldataSize
            | Self::InternalFrameAddr(_)
            | Self::CodeSize
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::ChainId
            | Self::Address
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee => {}
        }
    }

    /// Returns true if this instruction may mutate persistent storage.
    #[must_use]
    pub const fn may_mutate_storage(&self) -> bool {
        matches!(
            self,
            Self::SStore(_, _)
                | Self::Call { .. }
                | Self::DelegateCall { .. }
                | Self::InternalCall { .. }
                | Self::Create(_, _, _)
                | Self::Create2(_, _, _, _)
        )
    }

    /// Returns true if this instruction may mutate transient storage.
    #[must_use]
    pub const fn may_mutate_transient_storage(&self) -> bool {
        matches!(
            self,
            Self::TStore(_, _)
                | Self::Call { .. }
                | Self::DelegateCall { .. }
                | Self::InternalCall { .. }
                | Self::Create(_, _, _)
                | Self::Create2(_, _, _, _)
        )
    }

    /// Returns true if this instruction writes or may write memory.
    #[must_use]
    pub const fn may_mutate_memory(&self) -> bool {
        matches!(
            self,
            Self::MStore(_, _)
                | Self::MStore8(_, _)
                | Self::MCopy(_, _, _)
                | Self::CalldataCopy(_, _, _)
                | Self::CodeCopy(_, _, _)
                | Self::ReturnDataCopy(_, _, _)
                | Self::ExtCodeCopy(_, _, _, _)
                | Self::Call { .. }
                | Self::StaticCall { .. }
                | Self::DelegateCall { .. }
                | Self::InternalCall { .. }
                | Self::Create(_, _, _)
                | Self::Create2(_, _, _, _)
        )
    }

    /// Returns the mnemonic for this instruction.
    #[must_use]
    pub const fn mnemonic(&self) -> &'static str {
        match self {
            Self::Add(_, _) => "add",
            Self::Sub(_, _) => "sub",
            Self::Mul(_, _) => "mul",
            Self::Div(_, _) => "div",
            Self::SDiv(_, _) => "sdiv",
            Self::Mod(_, _) => "mod",
            Self::SMod(_, _) => "smod",
            Self::Exp(_, _) => "exp",
            Self::AddMod(_, _, _) => "addmod",
            Self::MulMod(_, _, _) => "mulmod",
            Self::And(_, _) => "and",
            Self::Or(_, _) => "or",
            Self::Xor(_, _) => "xor",
            Self::Not(_) => "not",
            Self::Shl(_, _) => "shl",
            Self::Shr(_, _) => "shr",
            Self::Sar(_, _) => "sar",
            Self::Byte(_, _) => "byte",
            Self::Lt(_, _) => "lt",
            Self::Gt(_, _) => "gt",
            Self::SLt(_, _) => "slt",
            Self::SGt(_, _) => "sgt",
            Self::Eq(_, _) => "eq",
            Self::IsZero(_) => "iszero",
            Self::MLoad(_) => "mload",
            Self::MStore(_, _) => "mstore",
            Self::MStore8(_, _) => "mstore8",
            Self::MSize => "msize",
            Self::MCopy(_, _, _) => "mcopy",
            Self::SLoad(_) => "sload",
            Self::SStore(_, _) => "sstore",
            Self::TLoad(_) => "tload",
            Self::TStore(_, _) => "tstore",
            Self::CalldataLoad(_) => "calldataload",
            Self::CalldataCopy(_, _, _) => "calldatacopy",
            Self::CalldataSize => "calldatasize",
            Self::CodeSize => "codesize",
            Self::CodeCopy(_, _, _) => "codecopy",
            Self::ExtCodeSize(_) => "extcodesize",
            Self::ExtCodeCopy(_, _, _, _) => "extcodecopy",
            Self::ExtCodeHash(_) => "extcodehash",
            Self::ReturnDataSize => "returndatasize",
            Self::ReturnDataCopy(_, _, _) => "returndatacopy",
            Self::InternalFrameAddr(_) => "internal_frame_addr",
            Self::Caller => "caller",
            Self::CallValue => "callvalue",
            Self::Origin => "origin",
            Self::GasPrice => "gasprice",
            Self::BlockHash(_) => "blockhash",
            Self::Coinbase => "coinbase",
            Self::Timestamp => "timestamp",
            Self::BlockNumber => "number",
            Self::PrevRandao => "prevrandao",
            Self::GasLimit => "gaslimit",
            Self::ChainId => "chainid",
            Self::Address => "address",
            Self::Balance(_) => "balance",
            Self::SelfBalance => "selfbalance",
            Self::Gas => "gas",
            Self::BaseFee => "basefee",
            Self::BlobBaseFee => "blobbasefee",
            Self::BlobHash(_) => "blobhash",
            Self::Keccak256(_, _) => "keccak256",
            Self::Call { .. } => "call",
            Self::StaticCall { .. } => "staticcall",
            Self::DelegateCall { .. } => "delegatecall",
            Self::InternalCall { .. } => "internal_call",
            Self::Create(_, _, _) => "create",
            Self::Create2(_, _, _, _) => "create2",
            Self::Log0(_, _) => "log0",
            Self::Log1(_, _, _) => "log1",
            Self::Log2(_, _, _, _) => "log2",
            Self::Log3(_, _, _, _, _) => "log3",
            Self::Log4(_, _, _, _, _, _) => "log4",
            Self::Phi(_) => "phi",
            Self::Select(_, _, _) => "select",
            Self::SignExtend(_, _) => "signextend",
        }
    }

    /// Returns true if this instruction has side effects.
    /// Side-effect instructions must not be eliminated by DCE.
    #[must_use]
    pub const fn has_side_effects(&self) -> bool {
        matches!(
            self,
            // Storage writes
            Self::SStore(_, _)
            | Self::TStore(_, _)
            // Memory writes (may affect external calls)
            | Self::MStore(_, _)
            | Self::MStore8(_, _)
            | Self::MCopy(_, _, _)
            // External calls
            | Self::Call { .. }
            | Self::StaticCall { .. }
            | Self::DelegateCall { .. }
            | Self::InternalCall { .. }
            // Contract creation
            | Self::Create(_, _, _)
            | Self::Create2(_, _, _, _)
            // Event emission
            | Self::Log0(_, _)
            | Self::Log1(_, _, _)
            | Self::Log2(_, _, _, _)
            | Self::Log3(_, _, _, _, _)
            | Self::Log4(_, _, _, _, _, _)
            // Data copy operations (write to memory)
            | Self::CalldataCopy(_, _, _)
            | Self::CodeCopy(_, _, _)
            | Self::ExtCodeCopy(_, _, _, _)
            | Self::ReturnDataCopy(_, _, _)
        )
    }

    /// Returns a conservative effect classification for this instruction.
    #[must_use]
    pub const fn effect_kind(&self) -> EffectKind {
        match self {
            Self::MStore(_, _)
            | Self::MStore8(_, _)
            | Self::MCopy(_, _, _)
            | Self::CalldataCopy(_, _, _)
            | Self::CodeCopy(_, _, _)
            | Self::ExtCodeCopy(_, _, _, _)
            | Self::ReturnDataCopy(_, _, _) => EffectKind::MemoryWrite,
            Self::MLoad(_) | Self::MSize | Self::Keccak256(_, _) => EffectKind::MemoryRead,
            Self::SLoad(_) => EffectKind::StorageRead,
            Self::SStore(_, _) => EffectKind::StorageWrite,
            Self::TLoad(_) => EffectKind::TransientRead,
            Self::TStore(_, _) => EffectKind::TransientWrite,
            Self::Call { .. } | Self::StaticCall { .. } | Self::DelegateCall { .. } => {
                EffectKind::ExternalCall
            }
            Self::InternalCall { .. } => EffectKind::InternalCall,
            Self::Create(_, _, _) | Self::Create2(_, _, _, _) => EffectKind::Create,
            Self::Log0(_, _)
            | Self::Log1(_, _, _)
            | Self::Log2(_, _, _, _)
            | Self::Log3(_, _, _, _, _)
            | Self::Log4(_, _, _, _, _, _) => EffectKind::Log,
            Self::CalldataLoad(_)
            | Self::CalldataSize
            | Self::CodeSize
            | Self::ExtCodeSize(_)
            | Self::ExtCodeHash(_)
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::BlockHash(_)
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::ChainId
            | Self::Address
            | Self::Balance(_)
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee
            | Self::BlobHash(_) => EffectKind::EnvironmentRead,
            Self::Add(_, _)
            | Self::Sub(_, _)
            | Self::Mul(_, _)
            | Self::Div(_, _)
            | Self::SDiv(_, _)
            | Self::Mod(_, _)
            | Self::SMod(_, _)
            | Self::Exp(_, _)
            | Self::AddMod(_, _, _)
            | Self::MulMod(_, _, _)
            | Self::And(_, _)
            | Self::Or(_, _)
            | Self::Xor(_, _)
            | Self::Not(_)
            | Self::Shl(_, _)
            | Self::Shr(_, _)
            | Self::Sar(_, _)
            | Self::Byte(_, _)
            | Self::Lt(_, _)
            | Self::Gt(_, _)
            | Self::SLt(_, _)
            | Self::SGt(_, _)
            | Self::Eq(_, _)
            | Self::IsZero(_)
            | Self::InternalFrameAddr(_)
            | Self::Phi(_)
            | Self::Select(_, _, _)
            | Self::SignExtend(_, _) => EffectKind::Pure,
        }
    }
}

impl fmt::Display for InstKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.mnemonic())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mir::{Function, Immediate, Value};
    use alloy_primitives::U256;
    use solar_interface::Ident;

    #[test]
    fn phi_operands_include_incoming_values() {
        let mut func = Function::new(Ident::DUMMY);
        let pred_a = func.entry_block;
        let pred_b = func.alloc_block();
        let a = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(1))));
        let b = func.alloc_value(Value::Immediate(Immediate::uint256(U256::from(2))));

        let phi = InstKind::Phi(vec![(pred_a, a), (pred_b, b)]);

        assert_eq!(phi.operands().as_slice(), &[a, b]);
    }
}

//! MIR instructions.

use super::{BlockId, Function, FunctionId, MirType, Value, ValueId};
use alloy_primitives::U256;
use smallvec::SmallVec;
use solar_interface::Span;
use solar_sema::hir;
use std::fmt;

/// Extra information attached to a MIR instruction by lowering or analysis passes.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct InstructionMetadata {
    /// Proven storage alias key for `sload`/`sstore` instructions.
    storage_alias: Option<Box<StorageAlias>>,
    /// Source span that produced this instruction, when the lowerer can preserve it.
    source_span: Span,
    /// HIR expression that produced this instruction, when the lowerer can preserve it.
    hir_expr: Option<hir::ExprId>,
    /// Loop nesting depth attached by loop-aware analyses.
    pub loop_depth: u16,
    /// Packed optional memory region, effect kind, and unchecked flag.
    flags: MetadataFlags,
}

impl InstructionMetadata {
    /// Empty instruction metadata.
    pub const EMPTY: Self = Self {
        storage_alias: None,
        hir_expr: None,
        source_span: Span::DUMMY,
        loop_depth: 0,
        flags: MetadataFlags::EMPTY,
    };

    /// Returns the proven storage alias key.
    #[must_use]
    pub fn storage_alias(&self) -> Option<StorageAlias> {
        self.storage_alias.as_deref().copied()
    }

    /// Sets the proven storage alias key.
    pub fn set_storage_alias(&mut self, alias: Option<StorageAlias>) {
        self.storage_alias = alias.map(Box::new);
    }

    /// Returns the HIR expression that produced this instruction.
    #[must_use]
    pub fn hir_expr(&self) -> Option<hir::ExprId> {
        self.hir_expr
    }

    /// Sets the HIR expression that produced this instruction.
    pub fn set_hir_expr(&mut self, expr: Option<hir::ExprId>) {
        self.hir_expr = expr;
    }

    /// Returns the source span that produced this instruction.
    #[must_use]
    pub fn source_span(&self) -> Option<Span> {
        (!self.source_span.is_dummy()).then_some(self.source_span)
    }

    /// Sets the source span that produced this instruction.
    pub fn set_source_span(&mut self, span: Option<Span>) {
        self.source_span = span.unwrap_or(Span::DUMMY);
    }

    /// Returns the proven memory region.
    #[must_use]
    pub fn memory_region(&self) -> Option<MemoryRegion> {
        self.flags.memory_region()
    }

    /// Sets the proven memory region.
    pub fn set_memory_region(&mut self, region: Option<MemoryRegion>) {
        self.flags.set_memory_region(region);
    }

    /// Returns whether this instruction was lowered from an unchecked arithmetic context.
    #[must_use]
    pub fn unchecked(&self) -> bool {
        self.flags.unchecked()
    }

    /// Sets whether this instruction was lowered from an unchecked arithmetic context.
    pub fn set_unchecked(&mut self, unchecked: bool) {
        self.flags.set_unchecked(unchecked);
    }

    /// Returns the conservative effect classification attached by lowering or analysis.
    #[must_use]
    pub fn effect(&self) -> Option<EffectKind> {
        self.flags.effect()
    }

    /// Sets the conservative effect classification attached by lowering or analysis.
    pub fn set_effect(&mut self, effect: Option<EffectKind>) {
        self.flags.set_effect(effect);
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct MetadataFlags(u8);

impl MetadataFlags {
    const EMPTY: Self = Self(0);
    const MEMORY_MASK: u8 = 0b0000_0111;
    const EFFECT_MASK: u8 = 0b0111_1000;
    const EFFECT_SHIFT: u8 = 3;
    const UNCHECKED: u8 = 0b1000_0000;

    fn memory_region(self) -> Option<MemoryRegion> {
        match self.0 & Self::MEMORY_MASK {
            0 => None,
            1 => Some(MemoryRegion::Scratch),
            2 => Some(MemoryRegion::AbiReturn),
            3 => Some(MemoryRegion::Heap),
            4 => Some(MemoryRegion::InternalFrame),
            5 => Some(MemoryRegion::Unknown),
            _ => unreachable!("invalid packed memory region"),
        }
    }

    fn set_memory_region(&mut self, region: Option<MemoryRegion>) {
        let bits = match region {
            None => 0,
            Some(MemoryRegion::Scratch) => 1,
            Some(MemoryRegion::AbiReturn) => 2,
            Some(MemoryRegion::Heap) => 3,
            Some(MemoryRegion::InternalFrame) => 4,
            Some(MemoryRegion::Unknown) => 5,
        };
        self.0 = (self.0 & !Self::MEMORY_MASK) | bits;
    }

    fn unchecked(self) -> bool {
        self.0 & Self::UNCHECKED != 0
    }

    fn set_unchecked(&mut self, unchecked: bool) {
        if unchecked {
            self.0 |= Self::UNCHECKED;
        } else {
            self.0 &= !Self::UNCHECKED;
        }
    }

    fn effect(self) -> Option<EffectKind> {
        match (self.0 & Self::EFFECT_MASK) >> Self::EFFECT_SHIFT {
            0 => None,
            1 => Some(EffectKind::Pure),
            2 => Some(EffectKind::MemoryRead),
            3 => Some(EffectKind::MemoryWrite),
            4 => Some(EffectKind::StorageRead),
            5 => Some(EffectKind::StorageWrite),
            6 => Some(EffectKind::TransientRead),
            7 => Some(EffectKind::TransientWrite),
            8 => Some(EffectKind::EnvironmentRead),
            9 => Some(EffectKind::ExternalCall),
            10 => Some(EffectKind::InternalCall),
            11 => Some(EffectKind::Create),
            12 => Some(EffectKind::Log),
            _ => unreachable!("invalid packed effect kind"),
        }
    }

    fn set_effect(&mut self, effect: Option<EffectKind>) {
        let bits = match effect {
            None => 0,
            Some(EffectKind::Pure) => 1,
            Some(EffectKind::MemoryRead) => 2,
            Some(EffectKind::MemoryWrite) => 3,
            Some(EffectKind::StorageRead) => 4,
            Some(EffectKind::StorageWrite) => 5,
            Some(EffectKind::TransientRead) => 6,
            Some(EffectKind::TransientWrite) => 7,
            Some(EffectKind::EnvironmentRead) => 8,
            Some(EffectKind::ExternalCall) => 9,
            Some(EffectKind::InternalCall) => 10,
            Some(EffectKind::Create) => 11,
            Some(EffectKind::Log) => 12,
        } << Self::EFFECT_SHIFT;
        self.0 = (self.0 & !Self::EFFECT_MASK) | bits;
    }
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
            Value::Inst(inst_id) => match func.instructions[*inst_id].kind() {
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
            Value::Arg { .. } | Value::Undef(_) => Self::Symbolic(value),
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

impl MemoryRegion {
    /// Returns the stable textual name used in MIR metadata.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Scratch => "scratch",
            Self::AbiReturn => "abi_return",
            Self::Heap => "heap",
            Self::InternalFrame => "internal_frame",
            Self::Unknown => "unknown",
        }
    }
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

impl EffectKind {
    /// Returns the stable textual name used in MIR metadata.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::Pure => "pure",
            Self::MemoryRead => "memory_read",
            Self::MemoryWrite => "memory_write",
            Self::StorageRead => "storage_read",
            Self::StorageWrite => "storage_write",
            Self::TransientRead => "transient_read",
            Self::TransientWrite => "transient_write",
            Self::EnvironmentRead => "environment_read",
            Self::ExternalCall => "external_call",
            Self::InternalCall => "internal_call",
            Self::Create => "create",
            Self::Log => "log",
        }
    }
}

/// Non-`ValueId` instruction payload data.
#[derive(Clone, Debug, Default)]
enum InstData {
    /// No extra payload.
    #[default]
    None,
    /// Immediate payload for `internal_frame_addr`.
    InternalFrameAddr(u64),
    /// Immediate payload for `loadimmutable`.
    LoadImmutable(u32),
    /// Non-operand payload for `internal_call`.
    InternalCall { function: FunctionId, returns: u32 },
    /// Predecessor blocks for phi operands.
    Phi { blocks: Vec<BlockId> },
}

/// An instruction in the MIR.
#[derive(Clone, Debug)]
pub struct Instruction {
    /// The tag-only kind of instruction.
    pub kind: InstTag,
    /// SSA operands for generic operand walks.
    operands: SmallVec<[ValueId; 8]>,
    /// Non-`ValueId` instruction payload data.
    data: InstData,
    /// The result type (if any).
    pub result_ty: Option<MirType>,
    /// Metadata produced by lowering or analysis.
    pub metadata: InstructionMetadata,
}

impl Instruction {
    /// Creates a new instruction.
    #[must_use]
    pub fn new(kind: InstKind, result_ty: Option<MirType>) -> Self {
        let (kind, operands, data) = Self::lower_kind(kind);
        Self { kind, operands, data, result_ty, metadata: InstructionMetadata::EMPTY }
    }

    fn lower_kind(kind: InstKind) -> (InstTag, SmallVec<[ValueId; 8]>, InstData) {
        let tag = kind.tag();
        let mut operands = SmallVec::new();
        let data = match kind {
            InstKind::InternalFrameAddr(offset) => InstData::InternalFrameAddr(offset),
            InstKind::LoadImmutable(offset) => InstData::LoadImmutable(offset),
            InstKind::InternalCall { function, args, returns } => {
                operands.extend(args.iter().copied());
                InstData::InternalCall { function, returns }
            }
            InstKind::Phi(incoming) => {
                let mut blocks = Vec::with_capacity(incoming.len());
                for (block, value) in incoming {
                    blocks.push(block);
                    operands.push(value);
                }
                InstData::Phi { blocks }
            }
            kind => {
                kind.collect_operands(&mut operands);
                InstData::None
            }
        };
        (tag, operands, data)
    }

    /// Returns the matching form of this instruction.
    #[must_use]
    pub fn kind(&self) -> InstKind {
        let op = |index| self.operands[index];
        match self.kind {
            InstTag::Add => InstKind::Add(op(0), op(1)),
            InstTag::Sub => InstKind::Sub(op(0), op(1)),
            InstTag::Mul => InstKind::Mul(op(0), op(1)),
            InstTag::Div => InstKind::Div(op(0), op(1)),
            InstTag::SDiv => InstKind::SDiv(op(0), op(1)),
            InstTag::Mod => InstKind::Mod(op(0), op(1)),
            InstTag::SMod => InstKind::SMod(op(0), op(1)),
            InstTag::Exp => InstKind::Exp(op(0), op(1)),
            InstTag::AddMod => InstKind::AddMod(op(0), op(1), op(2)),
            InstTag::MulMod => InstKind::MulMod(op(0), op(1), op(2)),
            InstTag::And => InstKind::And(op(0), op(1)),
            InstTag::Or => InstKind::Or(op(0), op(1)),
            InstTag::Xor => InstKind::Xor(op(0), op(1)),
            InstTag::Not => InstKind::Not(op(0)),
            InstTag::Shl => InstKind::Shl(op(0), op(1)),
            InstTag::Shr => InstKind::Shr(op(0), op(1)),
            InstTag::Sar => InstKind::Sar(op(0), op(1)),
            InstTag::Byte => InstKind::Byte(op(0), op(1)),
            InstTag::Lt => InstKind::Lt(op(0), op(1)),
            InstTag::Gt => InstKind::Gt(op(0), op(1)),
            InstTag::SLt => InstKind::SLt(op(0), op(1)),
            InstTag::SGt => InstKind::SGt(op(0), op(1)),
            InstTag::Eq => InstKind::Eq(op(0), op(1)),
            InstTag::IsZero => InstKind::IsZero(op(0)),
            InstTag::MLoad => InstKind::MLoad(op(0)),
            InstTag::MStore => InstKind::MStore(op(0), op(1)),
            InstTag::MStore8 => InstKind::MStore8(op(0), op(1)),
            InstTag::MSize => InstKind::MSize,
            InstTag::MCopy => InstKind::MCopy(op(0), op(1), op(2)),
            InstTag::SLoad => InstKind::SLoad(op(0)),
            InstTag::SStore => InstKind::SStore(op(0), op(1)),
            InstTag::TLoad => InstKind::TLoad(op(0)),
            InstTag::TStore => InstKind::TStore(op(0), op(1)),
            InstTag::CalldataLoad => InstKind::CalldataLoad(op(0)),
            InstTag::CalldataCopy => InstKind::CalldataCopy(op(0), op(1), op(2)),
            InstTag::CalldataSize => InstKind::CalldataSize,
            InstTag::InternalFrameAddr => {
                let InstData::InternalFrameAddr(offset) = self.data else {
                    unreachable!("internal_frame_addr missing payload")
                };
                InstKind::InternalFrameAddr(offset)
            }
            InstTag::CodeSize => InstKind::CodeSize,
            InstTag::CodeCopy => InstKind::CodeCopy(op(0), op(1), op(2)),
            InstTag::ExtCodeSize => InstKind::ExtCodeSize(op(0)),
            InstTag::ExtCodeCopy => InstKind::ExtCodeCopy(op(0), op(1), op(2), op(3)),
            InstTag::ExtCodeHash => InstKind::ExtCodeHash(op(0)),
            InstTag::LoadImmutable => {
                let InstData::LoadImmutable(offset) = self.data else {
                    unreachable!("loadimmutable missing payload")
                };
                InstKind::LoadImmutable(offset)
            }
            InstTag::ReturnDataSize => InstKind::ReturnDataSize,
            InstTag::ReturnDataCopy => InstKind::ReturnDataCopy(op(0), op(1), op(2)),
            InstTag::Caller => InstKind::Caller,
            InstTag::CallValue => InstKind::CallValue,
            InstTag::Origin => InstKind::Origin,
            InstTag::GasPrice => InstKind::GasPrice,
            InstTag::BlockHash => InstKind::BlockHash(op(0)),
            InstTag::Coinbase => InstKind::Coinbase,
            InstTag::Timestamp => InstKind::Timestamp,
            InstTag::BlockNumber => InstKind::BlockNumber,
            InstTag::PrevRandao => InstKind::PrevRandao,
            InstTag::GasLimit => InstKind::GasLimit,
            InstTag::ChainId => InstKind::ChainId,
            InstTag::Address => InstKind::Address,
            InstTag::Balance => InstKind::Balance(op(0)),
            InstTag::SelfBalance => InstKind::SelfBalance,
            InstTag::Gas => InstKind::Gas,
            InstTag::BaseFee => InstKind::BaseFee,
            InstTag::BlobBaseFee => InstKind::BlobBaseFee,
            InstTag::BlobHash => InstKind::BlobHash(op(0)),
            InstTag::Keccak256 => InstKind::Keccak256(op(0), op(1)),
            InstTag::Call => InstKind::Call {
                gas: op(0),
                addr: op(1),
                value: op(2),
                args_offset: op(3),
                args_size: op(4),
                ret_offset: op(5),
                ret_size: op(6),
            },
            InstTag::StaticCall => InstKind::StaticCall {
                gas: op(0),
                addr: op(1),
                args_offset: op(2),
                args_size: op(3),
                ret_offset: op(4),
                ret_size: op(5),
            },
            InstTag::DelegateCall => InstKind::DelegateCall {
                gas: op(0),
                addr: op(1),
                args_offset: op(2),
                args_size: op(3),
                ret_offset: op(4),
                ret_size: op(5),
            },
            InstTag::InternalCall => {
                let InstData::InternalCall { function, returns } = self.data else {
                    unreachable!("internal_call missing payload")
                };
                InstKind::InternalCall {
                    function,
                    args: self.operands.iter().copied().collect(),
                    returns,
                }
            }
            InstTag::Create => InstKind::Create(op(0), op(1), op(2)),
            InstTag::Create2 => InstKind::Create2(op(0), op(1), op(2), op(3)),
            InstTag::Log0 => InstKind::Log0(op(0), op(1)),
            InstTag::Log1 => InstKind::Log1(op(0), op(1), op(2)),
            InstTag::Log2 => InstKind::Log2(op(0), op(1), op(2), op(3)),
            InstTag::Log3 => InstKind::Log3(op(0), op(1), op(2), op(3), op(4)),
            InstTag::Log4 => InstKind::Log4(op(0), op(1), op(2), op(3), op(4), op(5)),
            InstTag::Phi => InstKind::Phi(self.phi_incoming().unwrap_or_default()),
            InstTag::Select => InstKind::Select(op(0), op(1), op(2)),
            InstTag::SignExtend => InstKind::SignExtend(op(0), op(1)),
        }
    }

    /// Returns the operands of this instruction.
    #[must_use]
    pub fn operands(&self) -> SmallVec<[ValueId; 8]> {
        self.operands.clone()
    }

    /// Collects all operands of this instruction into the provided vector.
    pub fn collect_operands(&self, out: &mut SmallVec<[ValueId; 8]>) {
        out.extend_from_slice(&self.operands);
    }

    /// Visits every operand mutably.
    pub fn visit_operands_mut(&mut self, mut f: impl FnMut(&mut ValueId)) {
        for operand in &mut self.operands {
            f(operand);
        }
    }

    /// Replaces the instruction kind.
    pub fn set_kind(&mut self, kind: InstKind) {
        let (kind, operands, data) = Self::lower_kind(kind);
        self.kind = kind;
        self.operands = operands;
        self.data = data;
    }

    /// Returns this phi's incoming edges.
    #[must_use]
    pub fn phi_incoming(&self) -> Option<Vec<(BlockId, ValueId)>> {
        let InstData::Phi { blocks } = &self.data else { return None };
        Some(blocks.iter().copied().zip(self.operands.iter().copied()).collect())
    }

    /// Replaces this phi's incoming edges.
    pub fn set_phi_incoming(&mut self, incoming: Vec<(BlockId, ValueId)>) {
        self.kind = InstTag::Phi;
        self.operands.clear();
        let mut blocks = Vec::with_capacity(incoming.len());
        for (block, value) in incoming {
            blocks.push(block);
            self.operands.push(value);
        }
        self.data = InstData::Phi { blocks };
    }

    /// Mutates this phi's incoming edges.
    pub fn update_phi_incoming(&mut self, f: impl FnOnce(&mut Vec<(BlockId, ValueId)>)) {
        let mut incoming = self.phi_incoming().expect("instruction is not a phi");
        f(&mut incoming);
        self.set_phi_incoming(incoming);
    }

    /// Returns the metadata effect, or derives a conservative one from the instruction kind.
    #[must_use]
    pub fn effect_kind(&self) -> EffectKind {
        self.metadata.effect().unwrap_or_else(|| self.kind.effect_kind())
    }
}

macro_rules! define_mir_insts {
    ($(
        $(#[$attr:meta])*
        $variant:ident
        $(($($tuple:ty),* $(,)?))?
        $({ $($fields:tt)* })?
    ,)*) => {
        /// The tag-only kind of a MIR instruction.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        #[repr(u8)]
        pub enum InstTag {
            $(
                $(#[$attr])*
                $variant,
            )*
        }

        /// The matching form of a MIR instruction, including immediate payloads.
        ///
        /// `Instruction` stores generic SSA operands separately, so hot operand walks do not need
        /// to match this enum.
        #[derive(Clone, Debug)]
        #[repr(u8)]
        pub enum InstKind {
            $(
                $(#[$attr])*
                $variant $(($($tuple),*))? $({ $($fields)* })?,
            )*
        }
    };
}

define_mir_insts! {
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
    /// Read an immutable word identified by its byte offset: `loadimmutable <offset>`
    ///
    /// In runtime code this assembles to a `PUSH32` placeholder that the
    /// constructor patches with the staged value before returning the runtime
    /// code. In constructor code it reads the staged scratch word instead.
    LoadImmutable(u32),

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
    InternalCall { function: FunctionId, args: Box<[ValueId]>, returns: u32 },

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

impl InstTag {
    /// Returns true if this instruction may mutate persistent storage.
    #[must_use]
    pub const fn may_mutate_storage(&self) -> bool {
        matches!(
            self,
            Self::SStore
                | Self::Call
                | Self::DelegateCall
                | Self::InternalCall
                | Self::Create
                | Self::Create2
        )
    }

    /// Returns true if this instruction may mutate transient storage.
    #[must_use]
    pub const fn may_mutate_transient_storage(&self) -> bool {
        matches!(
            self,
            Self::TStore
                | Self::Call
                | Self::DelegateCall
                | Self::InternalCall
                | Self::Create
                | Self::Create2
        )
    }

    /// Returns true if this instruction writes or may write memory.
    #[must_use]
    pub const fn may_mutate_memory(&self) -> bool {
        matches!(
            self,
            Self::MStore
                | Self::MStore8
                | Self::MCopy
                | Self::CalldataCopy
                | Self::CodeCopy
                | Self::ReturnDataCopy
                | Self::ExtCodeCopy
                | Self::Call
                | Self::StaticCall
                | Self::DelegateCall
                | Self::InternalCall
                | Self::Create
                | Self::Create2
        )
    }

    /// Returns the mnemonic for this instruction.
    #[must_use]
    pub const fn mnemonic(&self) -> &'static str {
        match self {
            Self::Add => "add",
            Self::Sub => "sub",
            Self::Mul => "mul",
            Self::Div => "div",
            Self::SDiv => "sdiv",
            Self::Mod => "mod",
            Self::SMod => "smod",
            Self::Exp => "exp",
            Self::AddMod => "addmod",
            Self::MulMod => "mulmod",
            Self::And => "and",
            Self::Or => "or",
            Self::Xor => "xor",
            Self::Not => "not",
            Self::Shl => "shl",
            Self::Shr => "shr",
            Self::Sar => "sar",
            Self::Byte => "byte",
            Self::Lt => "lt",
            Self::Gt => "gt",
            Self::SLt => "slt",
            Self::SGt => "sgt",
            Self::Eq => "eq",
            Self::IsZero => "iszero",
            Self::MLoad => "mload",
            Self::MStore => "mstore",
            Self::MStore8 => "mstore8",
            Self::MSize => "msize",
            Self::MCopy => "mcopy",
            Self::SLoad => "sload",
            Self::SStore => "sstore",
            Self::TLoad => "tload",
            Self::TStore => "tstore",
            Self::CalldataLoad => "calldataload",
            Self::CalldataCopy => "calldatacopy",
            Self::CalldataSize => "calldatasize",
            Self::CodeSize => "codesize",
            Self::CodeCopy => "codecopy",
            Self::LoadImmutable => "loadimmutable",
            Self::ExtCodeSize => "extcodesize",
            Self::ExtCodeCopy => "extcodecopy",
            Self::ExtCodeHash => "extcodehash",
            Self::ReturnDataSize => "returndatasize",
            Self::ReturnDataCopy => "returndatacopy",
            Self::InternalFrameAddr => "internal_frame_addr",
            Self::Caller => "caller",
            Self::CallValue => "callvalue",
            Self::Origin => "origin",
            Self::GasPrice => "gasprice",
            Self::BlockHash => "blockhash",
            Self::Coinbase => "coinbase",
            Self::Timestamp => "timestamp",
            Self::BlockNumber => "number",
            Self::PrevRandao => "prevrandao",
            Self::GasLimit => "gaslimit",
            Self::ChainId => "chainid",
            Self::Address => "address",
            Self::Balance => "balance",
            Self::SelfBalance => "selfbalance",
            Self::Gas => "gas",
            Self::BaseFee => "basefee",
            Self::BlobBaseFee => "blobbasefee",
            Self::BlobHash => "blobhash",
            Self::Keccak256 => "keccak256",
            Self::Call => "call",
            Self::StaticCall => "staticcall",
            Self::DelegateCall => "delegatecall",
            Self::InternalCall => "internal_call",
            Self::Create => "create",
            Self::Create2 => "create2",
            Self::Log0 => "log0",
            Self::Log1 => "log1",
            Self::Log2 => "log2",
            Self::Log3 => "log3",
            Self::Log4 => "log4",
            Self::Phi => "phi",
            Self::Select => "select",
            Self::SignExtend => "signextend",
        }
    }

    /// Returns true if this instruction has side effects.
    #[must_use]
    pub const fn has_side_effects(&self) -> bool {
        matches!(
            self,
            Self::SStore
                | Self::TStore
                | Self::MStore
                | Self::MStore8
                | Self::MCopy
                | Self::Call
                | Self::StaticCall
                | Self::DelegateCall
                | Self::InternalCall
                | Self::Create
                | Self::Create2
                | Self::Log0
                | Self::Log1
                | Self::Log2
                | Self::Log3
                | Self::Log4
                | Self::CalldataCopy
                | Self::CodeCopy
                | Self::ExtCodeCopy
                | Self::ReturnDataCopy
        )
    }

    /// Returns a conservative effect classification for this instruction.
    #[must_use]
    pub const fn effect_kind(&self) -> EffectKind {
        match self {
            Self::MStore
            | Self::MStore8
            | Self::MCopy
            | Self::CalldataCopy
            | Self::CodeCopy
            | Self::ExtCodeCopy
            | Self::ReturnDataCopy => EffectKind::MemoryWrite,
            Self::MLoad | Self::MSize | Self::Keccak256 => EffectKind::MemoryRead,
            Self::SLoad => EffectKind::StorageRead,
            Self::SStore => EffectKind::StorageWrite,
            Self::TLoad => EffectKind::TransientRead,
            Self::TStore => EffectKind::TransientWrite,
            Self::Call | Self::StaticCall | Self::DelegateCall => EffectKind::ExternalCall,
            Self::InternalCall => EffectKind::InternalCall,
            Self::Create | Self::Create2 => EffectKind::Create,
            Self::Log0 | Self::Log1 | Self::Log2 | Self::Log3 | Self::Log4 => EffectKind::Log,
            Self::CalldataLoad
            | Self::CalldataSize
            | Self::CodeSize
            | Self::LoadImmutable
            | Self::ExtCodeSize
            | Self::ExtCodeHash
            | Self::ReturnDataSize
            | Self::Caller
            | Self::CallValue
            | Self::Origin
            | Self::GasPrice
            | Self::BlockHash
            | Self::Coinbase
            | Self::Timestamp
            | Self::BlockNumber
            | Self::PrevRandao
            | Self::GasLimit
            | Self::ChainId
            | Self::Address
            | Self::Balance
            | Self::SelfBalance
            | Self::Gas
            | Self::BaseFee
            | Self::BlobBaseFee
            | Self::BlobHash => EffectKind::EnvironmentRead,
            Self::Add
            | Self::Sub
            | Self::Mul
            | Self::Div
            | Self::SDiv
            | Self::Mod
            | Self::SMod
            | Self::Exp
            | Self::AddMod
            | Self::MulMod
            | Self::And
            | Self::Or
            | Self::Xor
            | Self::Not
            | Self::Shl
            | Self::Shr
            | Self::Sar
            | Self::Byte
            | Self::Lt
            | Self::Gt
            | Self::SLt
            | Self::SGt
            | Self::Eq
            | Self::IsZero
            | Self::InternalFrameAddr
            | Self::Phi
            | Self::Select
            | Self::SignExtend => EffectKind::Pure,
        }
    }
}

impl InstKind {
    /// Returns the tag-only kind for this instruction.
    #[must_use]
    pub fn tag(&self) -> InstTag {
        // SAFETY: `define_mir_insts!` emits `InstTag` and `InstKind` with the same variants in the
        // same order, and both enums are `repr(u8)`.
        let tag = unsafe { *std::ptr::from_ref(self).cast::<u8>() };
        // SAFETY: every live `InstKind` has a discriminant emitted by the same macro as `InstTag`.
        unsafe { std::mem::transmute::<u8, InstTag>(tag) }
    }

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
            | Self::LoadImmutable(_)
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
            | Self::LoadImmutable(_)
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
            Self::LoadImmutable(_) => "loadimmutable",
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
            | Self::LoadImmutable(_)
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

    #[test]
    fn instruction_cached_operands_track_rewrites() {
        let a = ValueId::from_usize(1);
        let b = ValueId::from_usize(2);
        let c = ValueId::from_usize(3);
        let mut inst = Instruction::new(InstKind::Add(a, b), Some(MirType::uint256()));

        inst.visit_operands_mut(|value| {
            if *value == b {
                *value = c;
            }
        });

        assert_eq!(inst.operands().as_slice(), &[a, c]);
        assert!(matches!(inst.kind(), InstKind::Add(x, y) if x == a && y == c));

        inst.set_kind(InstKind::IsZero(c));
        assert_eq!(inst.operands().as_slice(), &[c]);
    }

    #[test]
    fn inst_tag_matches_matching_enum_discriminant() {
        let a = ValueId::from_usize(1);
        let b = ValueId::from_usize(2);

        let add = InstKind::Add(a, b);
        let msize = InstKind::MSize;
        let call = InstKind::Call {
            gas: a,
            addr: b,
            value: a,
            args_offset: b,
            args_size: a,
            ret_offset: b,
            ret_size: a,
        };

        assert_eq!(add.tag(), InstTag::Add);
        assert_eq!(add.tag().mnemonic(), add.mnemonic());
        assert_eq!(msize.tag(), InstTag::MSize);
        assert_eq!(msize.tag().effect_kind(), msize.effect_kind());
        assert_eq!(call.tag(), InstTag::Call);
        assert_eq!(call.tag().has_side_effects(), call.has_side_effects());
    }

    #[test]
    #[cfg_attr(not(target_pointer_width = "64"), ignore = "64-bit only")]
    #[cfg_attr(feature = "nightly", ignore = "stable only")]
    fn instruction_layout_sizes() {
        use snapbox::{assert_data_eq, str};

        #[track_caller]
        fn assert_size<T>(size: impl snapbox::IntoData) {
            assert_size_(std::mem::size_of::<T>(), size.into_data());
        }

        #[track_caller]
        fn assert_size_(actual: usize, expected: snapbox::Data) {
            assert_data_eq!(actual.to_string(), expected);
        }

        assert_size::<InstKind>(str!["32"]);
        assert_size::<InstructionMetadata>(str!["24"]);
        assert_size::<Instruction>(str!["96"]);
    }
}

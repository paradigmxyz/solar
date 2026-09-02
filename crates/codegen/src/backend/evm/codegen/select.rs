//! Single-opcode selection for MIR operations.
//!
//! Most MIR operations lower to exactly one EVM opcode whose stack contract
//! follows the operation's operand list. This table records that mapping so
//! the emitter only hand-writes operations with non-trivial lowering.

use crate::{backend::evm::op, mir::Op};

/// Stack shape of a MIR operation that lowers to one EVM opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OpcodeLowering {
    /// No operands; the opcode pushes the result.
    Nullary(u8),
    /// One operand; the opcode pushes the result.
    Unary(u8),
    /// Two operands in operand order; the opcode pushes the result.
    Binary(u8),
    /// An address and a value; the opcode pushes nothing.
    Store(u8),
    /// Every operand pushed last to first; the opcode pushes the result.
    Nary(u8),
    /// Every operand pushed last to first; the opcode copies into memory.
    MemoryCopy(u8),
    /// Every operand pushed last to first; the opcode emits a log.
    Log(u8),
}

impl OpcodeLowering {
    /// The selected opcode.
    pub(crate) fn opcode(self) -> u8 {
        match self {
            Self::Nullary(opcode)
            | Self::Unary(opcode)
            | Self::Binary(opcode)
            | Self::Store(opcode)
            | Self::Nary(opcode)
            | Self::MemoryCopy(opcode)
            | Self::Log(opcode) => opcode,
        }
    }
}

/// Returns the single-opcode lowering of `op`, when it has one.
pub(crate) fn opcode_lowering(op: &Op) -> Option<OpcodeLowering> {
    Some(match op {
        Op::Add { .. } => OpcodeLowering::Binary(op::ADD),
        Op::Sub { .. } => OpcodeLowering::Binary(op::SUB),
        Op::Mul { .. } => OpcodeLowering::Binary(op::MUL),
        Op::Div { .. } => OpcodeLowering::Binary(op::DIV),
        Op::SDiv { .. } => OpcodeLowering::Binary(op::SDIV),
        Op::Mod { .. } => OpcodeLowering::Binary(op::MOD),
        Op::SMod { .. } => OpcodeLowering::Binary(op::SMOD),
        Op::Exp { .. } => OpcodeLowering::Binary(op::EXP),
        Op::AddMod { .. } => OpcodeLowering::Nary(op::ADDMOD),
        Op::MulMod { .. } => OpcodeLowering::Nary(op::MULMOD),
        Op::And { .. } => OpcodeLowering::Binary(op::AND),
        Op::Or { .. } => OpcodeLowering::Binary(op::OR),
        Op::Xor { .. } => OpcodeLowering::Binary(op::XOR),
        Op::Not { .. } => OpcodeLowering::Unary(op::NOT),
        Op::Clz { .. } => OpcodeLowering::Unary(op::CLZ),
        Op::Shl { .. } => OpcodeLowering::Binary(op::SHL),
        Op::Shr { .. } => OpcodeLowering::Binary(op::SHR),
        Op::Sar { .. } => OpcodeLowering::Binary(op::SAR),
        Op::Byte { .. } => OpcodeLowering::Binary(op::BYTE),
        Op::Lt { .. } => OpcodeLowering::Binary(op::LT),
        Op::Gt { .. } => OpcodeLowering::Binary(op::GT),
        Op::SLt { .. } => OpcodeLowering::Binary(op::SLT),
        Op::SGt { .. } => OpcodeLowering::Binary(op::SGT),
        Op::Eq { .. } => OpcodeLowering::Binary(op::EQ),
        Op::IsZero { .. } => OpcodeLowering::Unary(op::ISZERO),
        Op::MLoad { .. } => OpcodeLowering::Unary(op::MLOAD),
        Op::MStore { .. } => OpcodeLowering::Store(op::MSTORE),
        Op::MStore8 { .. } => OpcodeLowering::Store(op::MSTORE8),
        Op::MSize => OpcodeLowering::Nullary(op::MSIZE),
        Op::SLoad { .. } => OpcodeLowering::Unary(op::SLOAD),
        Op::SStore { .. } => OpcodeLowering::Store(op::SSTORE),
        Op::TLoad { .. } => OpcodeLowering::Unary(op::TLOAD),
        Op::TStore { .. } => OpcodeLowering::Store(op::TSTORE),
        Op::CalldataLoad { .. } => OpcodeLowering::Unary(op::CALLDATALOAD),
        Op::CalldataSize => OpcodeLowering::Nullary(op::CALLDATASIZE),
        Op::Keccak256 { .. } => OpcodeLowering::Binary(op::KECCAK256),
        Op::Caller => OpcodeLowering::Nullary(op::CALLER),
        Op::CallValue => OpcodeLowering::Nullary(op::CALLVALUE),
        Op::Address => OpcodeLowering::Nullary(op::ADDRESS),
        Op::Origin => OpcodeLowering::Nullary(op::ORIGIN),
        Op::GasPrice => OpcodeLowering::Nullary(op::GASPRICE),
        Op::Gas => OpcodeLowering::Nullary(op::GAS),
        Op::Timestamp => OpcodeLowering::Nullary(op::TIMESTAMP),
        Op::BlockNumber => OpcodeLowering::Nullary(op::NUMBER),
        Op::Coinbase => OpcodeLowering::Nullary(op::COINBASE),
        Op::ChainId => OpcodeLowering::Nullary(op::CHAINID),
        Op::SelfBalance => OpcodeLowering::Nullary(op::SELFBALANCE),
        Op::BaseFee => OpcodeLowering::Nullary(op::BASEFEE),
        Op::BlobBaseFee => OpcodeLowering::Nullary(op::BLOBBASEFEE),
        Op::GasLimit => OpcodeLowering::Nullary(op::GASLIMIT),
        Op::SlotNum => OpcodeLowering::Nullary(op::SLOTNUM),
        Op::PrevRandao => OpcodeLowering::Nullary(op::PREVRANDAO),
        Op::Balance { .. } => OpcodeLowering::Unary(op::BALANCE),
        Op::BlockHash { .. } => OpcodeLowering::Unary(op::BLOCKHASH),
        Op::BlobHash { .. } => OpcodeLowering::Unary(op::BLOBHASH),
        Op::ExtCodeSize { .. } => OpcodeLowering::Unary(op::EXTCODESIZE),
        Op::ExtCodeHash { .. } => OpcodeLowering::Unary(op::EXTCODEHASH),
        Op::CodeSize => OpcodeLowering::Nullary(op::CODESIZE),
        Op::ReturnDataSize => OpcodeLowering::Nullary(op::RETURNDATASIZE),
        Op::SignExtend { .. } => OpcodeLowering::Binary(op::SIGNEXTEND),
        Op::Create { .. } => OpcodeLowering::Nary(op::CREATE),
        Op::Create2 { .. } => OpcodeLowering::Nary(op::CREATE2),
        Op::Log0 { .. } => OpcodeLowering::Log(op::LOG0),
        Op::Log1 { .. } => OpcodeLowering::Log(op::LOG1),
        Op::Log2 { .. } => OpcodeLowering::Log(op::LOG2),
        Op::Log3 { .. } => OpcodeLowering::Log(op::LOG3),
        Op::Log4 { .. } => OpcodeLowering::Log(op::LOG4),
        Op::CalldataCopy { .. } => OpcodeLowering::MemoryCopy(op::CALLDATACOPY),
        Op::CodeCopy { .. } => OpcodeLowering::MemoryCopy(op::CODECOPY),
        Op::ReturnDataCopy { .. } => OpcodeLowering::MemoryCopy(op::RETURNDATACOPY),
        Op::MCopy { .. } => OpcodeLowering::MemoryCopy(op::MCOPY),
        Op::ExtCodeCopy { .. } => OpcodeLowering::MemoryCopy(op::EXTCODECOPY),
        _ => return None,
    })
}

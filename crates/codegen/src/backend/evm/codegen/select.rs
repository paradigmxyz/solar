//! Single-opcode selection for MIR operations.
//!
//! Most MIR operations lower to exactly one EVM opcode whose stack contract
//! follows the operation's operand list. This table records that mapping so
//! the emitter only hand-writes operations with non-trivial lowering.

use crate::{backend::evm::op, mir::InstKind};

/// Stack shape of a MIR operation that lowers to one EVM opcode.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum OpcodeLowering {
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

/// Returns the single-opcode lowering of `kind`, when it has one.
pub(super) fn opcode_lowering(kind: &InstKind) -> Option<OpcodeLowering> {
    Some(match kind {
        InstKind::Add { .. } => OpcodeLowering::Binary(op::ADD),
        InstKind::Sub { .. } => OpcodeLowering::Binary(op::SUB),
        InstKind::Mul { .. } => OpcodeLowering::Binary(op::MUL),
        InstKind::Div { .. } => OpcodeLowering::Binary(op::DIV),
        InstKind::SDiv { .. } => OpcodeLowering::Binary(op::SDIV),
        InstKind::Mod { .. } => OpcodeLowering::Binary(op::MOD),
        InstKind::SMod { .. } => OpcodeLowering::Binary(op::SMOD),
        InstKind::Exp { .. } => OpcodeLowering::Binary(op::EXP),
        InstKind::AddMod { .. } => OpcodeLowering::Nary(op::ADDMOD),
        InstKind::MulMod { .. } => OpcodeLowering::Nary(op::MULMOD),
        InstKind::And { .. } => OpcodeLowering::Binary(op::AND),
        InstKind::Or { .. } => OpcodeLowering::Binary(op::OR),
        InstKind::Xor { .. } => OpcodeLowering::Binary(op::XOR),
        InstKind::Not { .. } => OpcodeLowering::Unary(op::NOT),
        InstKind::Clz { .. } => OpcodeLowering::Unary(op::CLZ),
        InstKind::Shl { .. } => OpcodeLowering::Binary(op::SHL),
        InstKind::Shr { .. } => OpcodeLowering::Binary(op::SHR),
        InstKind::Sar { .. } => OpcodeLowering::Binary(op::SAR),
        InstKind::Byte { .. } => OpcodeLowering::Binary(op::BYTE),
        InstKind::Lt { .. } => OpcodeLowering::Binary(op::LT),
        InstKind::Gt { .. } => OpcodeLowering::Binary(op::GT),
        InstKind::SLt { .. } => OpcodeLowering::Binary(op::SLT),
        InstKind::SGt { .. } => OpcodeLowering::Binary(op::SGT),
        InstKind::Eq { .. } => OpcodeLowering::Binary(op::EQ),
        InstKind::IsZero { .. } => OpcodeLowering::Unary(op::ISZERO),
        InstKind::MLoad { .. } => OpcodeLowering::Unary(op::MLOAD),
        InstKind::MStore { .. } => OpcodeLowering::Store(op::MSTORE),
        InstKind::MStore8 { .. } => OpcodeLowering::Store(op::MSTORE8),
        InstKind::MSize => OpcodeLowering::Nullary(op::MSIZE),
        InstKind::SLoad { .. } => OpcodeLowering::Unary(op::SLOAD),
        InstKind::SStore { .. } => OpcodeLowering::Store(op::SSTORE),
        InstKind::TLoad { .. } => OpcodeLowering::Unary(op::TLOAD),
        InstKind::TStore { .. } => OpcodeLowering::Store(op::TSTORE),
        InstKind::CalldataLoad { .. } => OpcodeLowering::Unary(op::CALLDATALOAD),
        InstKind::CalldataSize => OpcodeLowering::Nullary(op::CALLDATASIZE),
        InstKind::Keccak256 { .. } => OpcodeLowering::Binary(op::KECCAK256),
        InstKind::Caller => OpcodeLowering::Nullary(op::CALLER),
        InstKind::CallValue => OpcodeLowering::Nullary(op::CALLVALUE),
        InstKind::Address => OpcodeLowering::Nullary(op::ADDRESS),
        InstKind::Origin => OpcodeLowering::Nullary(op::ORIGIN),
        InstKind::GasPrice => OpcodeLowering::Nullary(op::GASPRICE),
        InstKind::Gas => OpcodeLowering::Nullary(op::GAS),
        InstKind::Timestamp => OpcodeLowering::Nullary(op::TIMESTAMP),
        InstKind::BlockNumber => OpcodeLowering::Nullary(op::NUMBER),
        InstKind::Coinbase => OpcodeLowering::Nullary(op::COINBASE),
        InstKind::ChainId => OpcodeLowering::Nullary(op::CHAINID),
        InstKind::SelfBalance => OpcodeLowering::Nullary(op::SELFBALANCE),
        InstKind::BaseFee => OpcodeLowering::Nullary(op::BASEFEE),
        InstKind::BlobBaseFee => OpcodeLowering::Nullary(op::BLOBBASEFEE),
        InstKind::GasLimit => OpcodeLowering::Nullary(op::GASLIMIT),
        InstKind::SlotNum => OpcodeLowering::Nullary(op::SLOTNUM),
        InstKind::PrevRandao => OpcodeLowering::Nullary(op::PREVRANDAO),
        InstKind::Balance { .. } => OpcodeLowering::Unary(op::BALANCE),
        InstKind::BlockHash { .. } => OpcodeLowering::Unary(op::BLOCKHASH),
        InstKind::BlobHash { .. } => OpcodeLowering::Unary(op::BLOBHASH),
        InstKind::ExtCodeSize { .. } => OpcodeLowering::Unary(op::EXTCODESIZE),
        InstKind::ExtCodeHash { .. } => OpcodeLowering::Unary(op::EXTCODEHASH),
        InstKind::CodeSize => OpcodeLowering::Nullary(op::CODESIZE),
        InstKind::ReturnDataSize => OpcodeLowering::Nullary(op::RETURNDATASIZE),
        InstKind::SignExtend { .. } => OpcodeLowering::Binary(op::SIGNEXTEND),
        InstKind::Create { .. } => OpcodeLowering::Nary(op::CREATE),
        InstKind::Create2 { .. } => OpcodeLowering::Nary(op::CREATE2),
        InstKind::Log0 { .. } => OpcodeLowering::Log(op::LOG0),
        InstKind::Log1 { .. } => OpcodeLowering::Log(op::LOG1),
        InstKind::Log2 { .. } => OpcodeLowering::Log(op::LOG2),
        InstKind::Log3 { .. } => OpcodeLowering::Log(op::LOG3),
        InstKind::Log4 { .. } => OpcodeLowering::Log(op::LOG4),
        InstKind::CalldataCopy { .. } => OpcodeLowering::MemoryCopy(op::CALLDATACOPY),
        InstKind::CodeCopy { .. } => OpcodeLowering::MemoryCopy(op::CODECOPY),
        InstKind::ReturnDataCopy { .. } => OpcodeLowering::MemoryCopy(op::RETURNDATACOPY),
        InstKind::MCopy { .. } => OpcodeLowering::MemoryCopy(op::MCOPY),
        InstKind::ExtCodeCopy { .. } => OpcodeLowering::MemoryCopy(op::EXTCODECOPY),
        _ => return None,
    })
}

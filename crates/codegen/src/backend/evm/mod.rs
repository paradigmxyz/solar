//! The EVM backend: the reference [`Backend`](crate::backend::Backend)
//! implementation, lowering MIR to EVM bytecode.
//!
//! This module contains:
//! - `EvmCodegen`: The main EVM code generator
//! - `ir`: Machine-level EVM instructions and block metadata
//! - `Assembler`: Final relocation and byte encoding
//! - `stack`: MIR-to-EVM stack scheduling for DUP/SWAP generation

use crate::mir::InstKind;
use alloy_primitives::U256;
use solar_config::EvmVersion;

/// Number of bytes in an EVM word.
pub(super) const EVM_WORD_BYTES: usize = 32;

/// Returns the encoded length of a minimally sized PUSH for an EVM version.
pub(super) fn push_len(evm_version: EvmVersion, value: U256) -> usize {
    if value.is_zero() && evm_version.has_push0() { 1 } else { value.byte_len().max(1) + 1 }
}

/// Returns the EVM opcode that directly implements a MIR instruction.
pub(super) const fn mir_opcode(kind: &InstKind) -> Option<u8> {
    Some(match kind {
        InstKind::Add(..) => op::ADD,
        InstKind::Sub(..) => op::SUB,
        InstKind::Mul(..) => op::MUL,
        InstKind::Div(..) => op::DIV,
        InstKind::SDiv(..) => op::SDIV,
        InstKind::Mod(..) => op::MOD,
        InstKind::SMod(..) => op::SMOD,
        InstKind::Exp(..) => op::EXP,
        InstKind::AddMod(..) => op::ADDMOD,
        InstKind::MulMod(..) => op::MULMOD,
        InstKind::And(..) => op::AND,
        InstKind::Or(..) => op::OR,
        InstKind::Xor(..) => op::XOR,
        InstKind::Not(..) => op::NOT,
        InstKind::Clz(..) => op::CLZ,
        InstKind::Shl(..) => op::SHL,
        InstKind::Shr(..) => op::SHR,
        InstKind::Sar(..) => op::SAR,
        InstKind::Byte(..) => op::BYTE,
        InstKind::Lt(..) => op::LT,
        InstKind::Gt(..) => op::GT,
        InstKind::SLt(..) => op::SLT,
        InstKind::SGt(..) => op::SGT,
        InstKind::Eq(..) => op::EQ,
        InstKind::IsZero(..) => op::ISZERO,
        InstKind::MLoad(..) => op::MLOAD,
        InstKind::MStore(..) => op::MSTORE,
        InstKind::MStore8(..) => op::MSTORE8,
        InstKind::MSize => op::MSIZE,
        InstKind::SLoad(..) => op::SLOAD,
        InstKind::SStore(..) => op::SSTORE,
        InstKind::TLoad(..) => op::TLOAD,
        InstKind::TStore(..) => op::TSTORE,
        InstKind::CalldataLoad(..) => op::CALLDATALOAD,
        InstKind::CalldataSize => op::CALLDATASIZE,
        InstKind::Keccak256(..) => op::KECCAK256,
        InstKind::Caller => op::CALLER,
        InstKind::CallValue => op::CALLVALUE,
        InstKind::Address => op::ADDRESS,
        InstKind::Origin => op::ORIGIN,
        InstKind::GasPrice => op::GASPRICE,
        InstKind::Gas => op::GAS,
        InstKind::Timestamp => op::TIMESTAMP,
        InstKind::BlockNumber => op::NUMBER,
        InstKind::Coinbase => op::COINBASE,
        InstKind::ChainId => op::CHAINID,
        InstKind::SelfBalance => op::SELFBALANCE,
        InstKind::BaseFee => op::BASEFEE,
        InstKind::BlobBaseFee => op::BLOBBASEFEE,
        InstKind::GasLimit => op::GASLIMIT,
        InstKind::SlotNum => op::SLOTNUM,
        InstKind::PrevRandao => op::PREVRANDAO,
        InstKind::Balance(..) => op::BALANCE,
        InstKind::BlockHash(..) => op::BLOCKHASH,
        InstKind::BlobHash(..) => op::BLOBHASH,
        InstKind::ExtCodeSize(..) => op::EXTCODESIZE,
        InstKind::ExtCodeHash(..) => op::EXTCODEHASH,
        InstKind::CodeSize => op::CODESIZE,
        InstKind::ReturnDataSize => op::RETURNDATASIZE,
        InstKind::SignExtend(..) => op::SIGNEXTEND,
        InstKind::Create(..) => op::CREATE,
        InstKind::Create2(..) => op::CREATE2,
        _ => return None,
    })
}

mod codegen;
pub use codegen::{EvmArtifact, EvmCodegen};

mod disasm;
pub use disasm::{disassemble, disassemble_standard_json};

mod layout;

mod materialize;

pub mod ir;

pub(crate) mod op;

pub(crate) mod assembler;

pub(crate) mod stack;

/// Generates bytecode from finalized EVM IR through the backend pipeline.
pub fn generate_evm_ir_bytecode(
    gcx: solar_sema::Gcx<'_>,
    module: ir::Module,
) -> solar_interface::Result<Vec<u8>> {
    ir::validate_for_evm_version(gcx.dcx(), &module, gcx.sess.opts.evm_version);
    gcx.dcx().has_errors()?;
    ir::validate_evm_version_before_legalization(gcx.dcx(), &module, gcx.sess.opts.evm_version);
    gcx.dcx().has_errors()?;
    let mut assembler = assembler::Assembler::from_evm_ir(gcx, module)?;
    let result = assembler.assemble_with_evm_ir(true);
    ir::validate(gcx.dcx(), result.evm_ir.as_ref().expect("requested EVM IR should be captured"));
    gcx.dcx().has_errors()?;
    Ok(result.bytecode)
}

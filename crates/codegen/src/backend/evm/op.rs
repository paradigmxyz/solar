//! Opcode identities, stack effects, fork availability and literal encoding facts.
//!
//! MIR mappings describe single physical instructions in pop order. Compound and
//! semantic operations remain the machine lowerer's responsibility. Extended stack
//! immediates follow EIP-8024 and preserve legacy jump-destination analysis.
//! See <https://eips.ethereum.org/EIPS/eip-8024>.

use crate::mir::InstKind;
use alloy_primitives::U256;
use solar_config::EvmVersion;

pub(crate) const WORD_BYTES: usize = 32;

macro_rules! opcodes {
    ($($name:ident = $byte:literal, $inputs:literal => $outputs:literal;)*) => {
        $(pub(crate) const $name: u8 = $byte;)*

        /// Returns a canonical uppercase mnemonic, independently of fork availability.
        pub(crate) const fn name(opcode: u8) -> Option<&'static str> {
            match opcode {
                $($name => Some(stringify!($name)),)*
                PUSH1..=PUSH32 => Some(PUSH_NAMES[(opcode - PUSH1) as usize]),
                DUP1..=DUP16 => Some(DUP_NAMES[(opcode - DUP1) as usize]),
                SWAP1..=SWAP16 => Some(SWAP_NAMES[(opcode - SWAP1) as usize]),
                CALLF => Some("CALLF"),
                RETF => Some("RETF"),
                JUMPF => Some("JUMPF"),
                DUPN => Some("DUPN"),
                SWAPN => Some("SWAPN"),
                EXCHANGE => Some("EXCHANGE"),
                _ => None,
            }
        }

        /// Returns required and resulting words; immediate-dependent operations return `None`.
        pub(crate) const fn stack_io(opcode: u8) -> Option<(u8, u8)> {
            match opcode {
                $($name => Some(($inputs, $outputs)),)*
                PUSH1..=PUSH32 => Some((0, 1)),
                DUP1..=DUP16 => Some((opcode - DUP1 + 1, opcode - DUP1 + 2)),
                SWAP1..=SWAP16 => Some((opcode - SWAP1 + 2, opcode - SWAP1 + 2)),
                _ => None,
            }
        }
    };
}

opcodes! {
    STOP = 0x00, 0 => 0;
    ADD = 0x01, 2 => 1;
    MUL = 0x02, 2 => 1;
    SUB = 0x03, 2 => 1;
    DIV = 0x04, 2 => 1;
    SDIV = 0x05, 2 => 1;
    MOD = 0x06, 2 => 1;
    SMOD = 0x07, 2 => 1;
    ADDMOD = 0x08, 3 => 1;
    MULMOD = 0x09, 3 => 1;
    EXP = 0x0a, 2 => 1;
    SIGNEXTEND = 0x0b, 2 => 1;
    LT = 0x10, 2 => 1;
    GT = 0x11, 2 => 1;
    SLT = 0x12, 2 => 1;
    SGT = 0x13, 2 => 1;
    EQ = 0x14, 2 => 1;
    ISZERO = 0x15, 1 => 1;
    AND = 0x16, 2 => 1;
    OR = 0x17, 2 => 1;
    XOR = 0x18, 2 => 1;
    NOT = 0x19, 1 => 1;
    BYTE = 0x1a, 2 => 1;
    SHL = 0x1b, 2 => 1;
    SHR = 0x1c, 2 => 1;
    SAR = 0x1d, 2 => 1;
    CLZ = 0x1e, 1 => 1;
    KECCAK256 = 0x20, 2 => 1;
    ADDRESS = 0x30, 0 => 1;
    BALANCE = 0x31, 1 => 1;
    ORIGIN = 0x32, 0 => 1;
    CALLER = 0x33, 0 => 1;
    CALLVALUE = 0x34, 0 => 1;
    CALLDATALOAD = 0x35, 1 => 1;
    CALLDATASIZE = 0x36, 0 => 1;
    CALLDATACOPY = 0x37, 3 => 0;
    CODESIZE = 0x38, 0 => 1;
    CODECOPY = 0x39, 3 => 0;
    GASPRICE = 0x3a, 0 => 1;
    EXTCODESIZE = 0x3b, 1 => 1;
    EXTCODECOPY = 0x3c, 4 => 0;
    RETURNDATASIZE = 0x3d, 0 => 1;
    RETURNDATACOPY = 0x3e, 3 => 0;
    EXTCODEHASH = 0x3f, 1 => 1;
    BLOCKHASH = 0x40, 1 => 1;
    COINBASE = 0x41, 0 => 1;
    TIMESTAMP = 0x42, 0 => 1;
    NUMBER = 0x43, 0 => 1;
    PREVRANDAO = 0x44, 0 => 1;
    GASLIMIT = 0x45, 0 => 1;
    CHAINID = 0x46, 0 => 1;
    SELFBALANCE = 0x47, 0 => 1;
    BASEFEE = 0x48, 0 => 1;
    BLOBHASH = 0x49, 1 => 1;
    BLOBBASEFEE = 0x4a, 0 => 1;
    SLOTNUM = 0x4b, 0 => 1;
    POP = 0x50, 1 => 0;
    MLOAD = 0x51, 1 => 1;
    MSTORE = 0x52, 2 => 0;
    MSTORE8 = 0x53, 2 => 0;
    SLOAD = 0x54, 1 => 1;
    SSTORE = 0x55, 2 => 0;
    JUMP = 0x56, 1 => 0;
    JUMPI = 0x57, 2 => 0;
    PC = 0x58, 0 => 1;
    MSIZE = 0x59, 0 => 1;
    GAS = 0x5a, 0 => 1;
    JUMPDEST = 0x5b, 0 => 0;
    TLOAD = 0x5c, 1 => 1;
    TSTORE = 0x5d, 2 => 0;
    MCOPY = 0x5e, 3 => 0;
    PUSH0 = 0x5f, 0 => 1;
    LOG0 = 0xa0, 2 => 0;
    LOG1 = 0xa1, 3 => 0;
    LOG2 = 0xa2, 4 => 0;
    LOG3 = 0xa3, 5 => 0;
    LOG4 = 0xa4, 6 => 0;
    DATALOAD = 0xd0, 1 => 1;
    DATALOADN = 0xd1, 0 => 1;
    DATASIZE = 0xd2, 0 => 1;
    DATACOPY = 0xd3, 3 => 0;
    EOFCREATE = 0xec, 4 => 1;
    RETURNCONTRACT = 0xee, 2 => 0;
    RETURNDATALOAD = 0xf7, 1 => 1;
    CREATE = 0xf0, 3 => 1;
    CALL = 0xf1, 7 => 1;
    CALLCODE = 0xf2, 7 => 1;
    RETURN = 0xf3, 2 => 0;
    DELEGATECALL = 0xf4, 6 => 1;
    CREATE2 = 0xf5, 4 => 1;
    EXTCALL = 0xf8, 4 => 1;
    EXTDELEGATECALL = 0xf9, 3 => 1;
    STATICCALL = 0xfa, 6 => 1;
    EXTSTATICCALL = 0xfb, 3 => 1;
    REVERT = 0xfd, 2 => 0;
    INVALID = 0xfe, 0 => 0;
    SELFDESTRUCT = 0xff, 1 => 0;
}

pub(crate) const PUSH1: u8 = 0x60;
pub(crate) const PUSH20: u8 = 0x73;
pub(crate) const PUSH32: u8 = 0x7f;
pub(crate) const DUP1: u8 = 0x80;
pub(crate) const DUP16: u8 = 0x8f;
pub(crate) const SWAP1: u8 = 0x90;
pub(crate) const SWAP16: u8 = 0x9f;
pub(crate) const CALLF: u8 = 0xe3;
pub(crate) const RETF: u8 = 0xe4;
pub(crate) const JUMPF: u8 = 0xe5;
pub(crate) const DUPN: u8 = 0xe6;
pub(crate) const SWAPN: u8 = 0xe7;
pub(crate) const EXCHANGE: u8 = 0xe8;

const PUSH_NAMES: [&str; 32] = [
    "PUSH1", "PUSH2", "PUSH3", "PUSH4", "PUSH5", "PUSH6", "PUSH7", "PUSH8", "PUSH9", "PUSH10",
    "PUSH11", "PUSH12", "PUSH13", "PUSH14", "PUSH15", "PUSH16", "PUSH17", "PUSH18", "PUSH19",
    "PUSH20", "PUSH21", "PUSH22", "PUSH23", "PUSH24", "PUSH25", "PUSH26", "PUSH27", "PUSH28",
    "PUSH29", "PUSH30", "PUSH31", "PUSH32",
];

const DUP_NAMES: [&str; 16] = [
    "DUP1", "DUP2", "DUP3", "DUP4", "DUP5", "DUP6", "DUP7", "DUP8", "DUP9", "DUP10", "DUP11",
    "DUP12", "DUP13", "DUP14", "DUP15", "DUP16",
];

const SWAP_NAMES: [&str; 16] = [
    "SWAP1", "SWAP2", "SWAP3", "SWAP4", "SWAP5", "SWAP6", "SWAP7", "SWAP8", "SWAP9", "SWAP10",
    "SWAP11", "SWAP12", "SWAP13", "SWAP14", "SWAP15", "SWAP16",
];

/// Parses instruction mnemonics without allocating or depending on a target fork.
pub(crate) fn parse_name(text: &str) -> Option<u8> {
    if text.eq_ignore_ascii_case("difficulty") {
        return Some(PREVRANDAO);
    }
    if text.eq_ignore_ascii_case("sha3") {
        return Some(KECCAK256);
    }
    (0..=u8::MAX).find(|&opcode| name(opcode).is_some_and(|name| name.eq_ignore_ascii_case(text)))
}

/// Checks physical instruction support using the retained target feature policy.
pub(crate) fn available(opcode: u8, evm_version: EvmVersion) -> bool {
    match opcode {
        SHL | SHR | SAR => evm_version.has_bitwise_shifting(),
        CLZ => evm_version.has_clz(),
        RETURNDATASIZE | RETURNDATACOPY | REVERT => evm_version.supports_returndata(),
        STATICCALL => evm_version.has_static_call(),
        CREATE2 => evm_version.has_create2(),
        EXTCODEHASH => evm_version.has_ext_code_hash(),
        CHAINID => evm_version.has_chain_id(),
        SELFBALANCE => evm_version.has_self_balance(),
        BASEFEE => evm_version.has_base_fee(),
        BLOBHASH | BLOBBASEFEE | TLOAD | TSTORE | MCOPY => evm_version.has_mcopy(),
        SLOTNUM => evm_version.has_slot_num(),
        PUSH0 => evm_version.has_push0(),
        EXTCALL | EXTDELEGATECALL | EXTSTATICCALL | RETURNDATALOAD | CALLF | RETF | JUMPF
        | DATALOAD | DATALOADN | DATASIZE | DATACOPY | EOFCREATE | RETURNCONTRACT => {
            evm_version.has_ext_call()
        }
        DUPN | SWAPN | EXCHANGE => evm_version.has_extended_stack_ops(),
        _ => name(opcode).is_some(),
    }
}

/// Returns the encoded size of the smallest legal literal PUSH.
pub(crate) fn push_len(evm_version: EvmVersion, value: U256) -> usize {
    let width = value.bit_len().div_ceil(8);
    1 + if evm_version.has_push0() { width } else { width.max(1) }
}

/// Encodes a DUPN/SWAPN depth, excluding depths covered by legacy opcodes.
pub(crate) fn encode_depth(depth: u16) -> Option<u8> {
    (17..=235).contains(&depth).then(|| (depth as u8).wrapping_add(111))
}

/// Decodes a valid DUPN/SWAPN immediate.
pub(crate) const fn decode_depth(byte: u8) -> Option<u16> {
    if byte > 90 && byte < 128 { None } else { Some(byte.wrapping_add(145) as u16) }
}

/// Encodes non-top stack indices in increasing order, as specified by EIP-8024.
pub(crate) fn encode_exchange(a: u16, b: u16) -> Option<u8> {
    if a == 0 || a >= b || b > 29 || a + b > 30 {
        return None;
    }
    let (high, low) = if b <= 16 { (a - 1, b - 1) } else { (29 - b, a - 1) };
    Some(((high * 16 + low) as u8) ^ 143)
}

/// Decodes an EXCHANGE immediate into its two non-top stack indices.
pub(crate) const fn decode_exchange(byte: u8) -> Option<(u16, u16)> {
    if byte > 81 && byte < 128 {
        return None;
    }
    let pair = byte ^ 143;
    let (high, low) = ((pair / 16) as u16, (pair % 16) as u16);
    Some(if high < low { (high + 1, low + 1) } else { (low + 1, 29 - high) })
}

impl InstKind {
    /// Returns the physical opcode when the MIR operation maps directly to one instruction.
    pub(crate) const fn evm_opcode(&self) -> Option<u8> {
        Some(match self {
            Self::Add(..) => ADD,
            Self::Sub(..) => SUB,
            Self::Mul(..) => MUL,
            Self::Div(..) => DIV,
            Self::SDiv(..) => SDIV,
            Self::Mod(..) => MOD,
            Self::SMod(..) => SMOD,
            Self::Exp(..) => EXP,
            Self::AddMod(..) => ADDMOD,
            Self::MulMod(..) => MULMOD,
            Self::And(..) => AND,
            Self::Or(..) => OR,
            Self::Xor(..) => XOR,
            Self::Not(..) => NOT,
            Self::Clz(..) => CLZ,
            Self::Shl(..) => SHL,
            Self::Shr(..) => SHR,
            Self::Sar(..) => SAR,
            Self::Byte(..) => BYTE,
            Self::Lt(..) => LT,
            Self::Gt(..) => GT,
            Self::SLt(..) => SLT,
            Self::SGt(..) => SGT,
            Self::Eq(..) => EQ,
            Self::IsZero(..) => ISZERO,
            Self::MLoad(..) => MLOAD,
            Self::MStore(..) => MSTORE,
            Self::MStore8(..) => MSTORE8,
            Self::MSize => MSIZE,
            Self::MCopy(..) => MCOPY,
            Self::SLoad(..) => SLOAD,
            Self::SStore(..) => SSTORE,
            Self::TLoad(..) => TLOAD,
            Self::TStore(..) => TSTORE,
            Self::CalldataLoad(..) => CALLDATALOAD,
            Self::CalldataCopy(..) => CALLDATACOPY,
            Self::CalldataSize => CALLDATASIZE,
            Self::CodeSize => CODESIZE,
            Self::CodeCopy(..) => CODECOPY,
            Self::ExtCodeSize(..) => EXTCODESIZE,
            Self::ExtCodeCopy(..) => EXTCODECOPY,
            Self::ExtCodeHash(..) => EXTCODEHASH,
            Self::ReturnDataSize => RETURNDATASIZE,
            Self::ReturnDataCopy(..) => RETURNDATACOPY,
            Self::Caller => CALLER,
            Self::CallValue => CALLVALUE,
            Self::Origin => ORIGIN,
            Self::GasPrice => GASPRICE,
            Self::BlockHash(..) => BLOCKHASH,
            Self::Coinbase => COINBASE,
            Self::Timestamp => TIMESTAMP,
            Self::BlockNumber => NUMBER,
            Self::PrevRandao => PREVRANDAO,
            Self::GasLimit => GASLIMIT,
            Self::SlotNum => SLOTNUM,
            Self::ChainId => CHAINID,
            Self::Address => ADDRESS,
            Self::Balance(..) => BALANCE,
            Self::SelfBalance => SELFBALANCE,
            Self::Gas => GAS,
            Self::BaseFee => BASEFEE,
            Self::BlobBaseFee => BLOBBASEFEE,
            Self::BlobHash(..) => BLOBHASH,
            Self::Keccak256(..) => KECCAK256,
            Self::Call { .. } => CALL,
            Self::CallCode { .. } => CALLCODE,
            Self::StaticCall { .. } => STATICCALL,
            Self::DelegateCall { .. } => DELEGATECALL,
            Self::ExtCall { .. } => EXTCALL,
            Self::ExtDelegateCall { .. } => EXTDELEGATECALL,
            Self::ExtStaticCall { .. } => EXTSTATICCALL,
            Self::Create(..) => CREATE,
            Self::Create2(..) => CREATE2,
            Self::Log0(..) => LOG0,
            Self::Log1(..) => LOG1,
            Self::Log2(..) => LOG2,
            Self::Log3(..) => LOG3,
            Self::Log4(..) => LOG4,
            Self::SignExtend(..) => SIGNEXTEND,
            _ => return None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extended_encoding_roundtrips() {
        for depth in 0..=1024 {
            assert_eq!(
                encode_depth(depth).and_then(decode_depth),
                (17..=235).contains(&depth).then_some(depth)
            );
        }
        for a in 0..=31 {
            for b in 0..=31 {
                assert_eq!(
                    encode_exchange(a, b).and_then(decode_exchange),
                    (a > 0 && a < b && a + b <= 30).then_some((a, b))
                );
            }
        }
        assert_eq!(encode_depth(17), Some(0x80));
        assert_eq!(encode_exchange(1, 16), Some(0x80));
        assert_eq!(encode_exchange(14, 15), Some(0x51));
    }
}

//! The gas schedule of EVM opcodes across EVM versions.
//!
//! Every opcode belongs to a [`GasTier`], whose static gas, and the gas of each unit of dynamic
//! work, depend on the EVM version. Account and storage accesses are priced cold unless a query
//! asks for the warm price of a slot or account the transaction already touched.

use alloy_primitives::U256;
use solar_config::EvmVersion;

/// The gas class of an opcode in the fork schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum GasTier {
    /// Halting instructions.
    Zero,
    /// `JUMPDEST`.
    Jumpdest,
    /// Environment reads, `POP`, `PUSH0`.
    Base,
    /// Arithmetic, comparisons, memory words, pushes, `DUP`, `SWAP`.
    VeryLow,
    /// Multiplication, division, `SIGNEXTEND`, `SELFBALANCE`, `CLZ`.
    Low,
    /// `ADDMOD`, `MULMOD`, `JUMP`.
    Mid,
    /// `JUMPI`.
    High,
    /// `EXP`, plus a per-byte exponent charge.
    Exp,
    /// `KECCAK256`, plus a per-word charge.
    Keccak,
    /// Memory copies, plus a per-word charge.
    Copy,
    /// `BLOCKHASH`.
    BlockHash,
    /// `BALANCE`, repriced by EIP-150, EIP-1884, and EIP-2929.
    Balance,
    /// `EXTCODESIZE` and `EXTCODECOPY`, repriced by EIP-150 and EIP-2929.
    ExtCode,
    /// `EXTCODEHASH`, repriced by EIP-1884 and EIP-2929.
    ExtCodeHash,
    /// `SLOAD`, repriced by EIP-150, EIP-1884, and EIP-2929.
    SLoad,
    /// `SSTORE` of a cold slot that changes its value.
    SStore,
    /// `TLOAD` and `TSTORE`.
    Transient,
    /// `LOGn`, plus a per-byte data charge.
    Log(u8),
    /// Contract creation.
    Create,
    /// Message calls to a cold account.
    Call,
    /// `SELFDESTRUCT`, priced by EIP-150.
    SelfDestruct,
}

/// Whether an account or storage slot was already touched in the transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warmth {
    /// The first access in the transaction, at the full price.
    Cold,
    /// An access after an earlier one, at the reduced price EIP-2929 charges;
    /// earlier schedules do not distinguish the two.
    Warm,
}

impl GasTier {
    /// Gas charged by one copy opcode per copied word.
    pub const COPY_WORD_GAS: u32 = 3;
    /// Gas of a warm account or storage access under EIP-2929.
    pub const WARM_ACCESS_GAS: u32 = 100;
    /// The surcharge EIP-2929 adds to a storage write of a cold slot.
    const COLD_SLOAD_GAS: u32 = 2100;

    /// Gas charged on `evm_version` before dynamic components for an access of
    /// the given warmth. Warmth only reprices account and storage accesses,
    /// and only since Berlin.
    pub const fn gas_at(self, evm_version: EvmVersion, warmth: Warmth) -> u32 {
        if !matches!(warmth, Warmth::Warm) || !since(evm_version, EvmVersion::Berlin) {
            return self.gas(evm_version);
        }
        match self {
            Self::Balance | Self::ExtCode | Self::ExtCodeHash | Self::SLoad | Self::Call => {
                Self::WARM_ACCESS_GAS
            }
            Self::SStore => self.gas(evm_version) - Self::COLD_SLOAD_GAS,
            _ => self.gas(evm_version),
        }
    }

    /// Gas charged on `evm_version` before dynamic components.
    pub const fn gas(self, evm_version: EvmVersion) -> u32 {
        match self {
            Self::Zero => 0,
            Self::Jumpdest => 1,
            Self::Base => 2,
            Self::VeryLow | Self::Copy => 3,
            Self::Low => 5,
            Self::Mid => 8,
            Self::High | Self::Exp => 10,
            Self::Keccak => 30,
            Self::BlockHash => 20,
            Self::Balance => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::Istanbul) {
                    700
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    400
                } else {
                    20
                }
            }
            Self::ExtCode => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    700
                } else {
                    20
                }
            }
            Self::ExtCodeHash => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::Istanbul) {
                    700
                } else {
                    400
                }
            }
            Self::SLoad => {
                if since(evm_version, EvmVersion::Berlin) {
                    2100
                } else if since(evm_version, EvmVersion::Istanbul) {
                    800
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    200
                } else {
                    50
                }
            }
            Self::SStore => 5000,
            Self::Transient => 100,
            Self::Log(topics) => 375 + 375 * topics as u32,
            Self::Create => 32000,
            Self::Call => {
                if since(evm_version, EvmVersion::Berlin) {
                    2600
                } else if since(evm_version, EvmVersion::TangerineWhistle) {
                    700
                } else {
                    40
                }
            }
            Self::SelfDestruct => {
                if since(evm_version, EvmVersion::TangerineWhistle) {
                    5000
                } else {
                    0
                }
            }
        }
    }

    /// Gas charged per unit of runtime-sized work: exponent bytes, hashed or
    /// copied words, and logged bytes.
    pub const fn dynamic_gas(self, evm_version: EvmVersion) -> u32 {
        match self {
            Self::Exp => {
                if since(evm_version, EvmVersion::SpuriousDragon) {
                    50
                } else {
                    10
                }
            }
            Self::Keccak => 6,
            Self::Copy => Self::COPY_WORD_GAS,
            Self::Log(_) => 8,
            _ => 0,
        }
    }

    /// The stack argument that sizes the dynamic work: the exponent of
    /// `EXP`, the byte length of a hash or a log, the last argument of a copy.
    pub const fn sizing_argument(self, arguments: usize) -> Option<usize> {
        match self {
            Self::Exp | Self::Keccak | Self::Log(_) => Some(1),
            Self::Copy => arguments.checked_sub(1),
            _ => None,
        }
    }

    /// Units of dynamic work for `arguments`, in stack order with immediates
    /// resolved: exponent bytes, hashed or copied words, or logged bytes. One
    /// unit when the size is not known statically.
    pub fn dynamic_units(self, arguments: &[Option<U256>]) -> u32 {
        let Some(size) = self.sizing_argument(arguments.len()).and_then(|index| arguments[index])
        else {
            return u32::from(self.sizing_argument(arguments.len()).is_some());
        };
        let units = match self {
            Self::Exp => U256::from(size.bit_len().div_ceil(8)),
            Self::Keccak | Self::Copy => size.div_ceil(U256::from(32)),
            Self::Log(_) => size,
            _ => U256::ZERO,
        };
        u32::try_from(units).unwrap_or(u32::MAX)
    }

    /// Gas of a tier priced the same on every EVM version, usable in constants.
    pub const fn fixed_gas(self) -> u32 {
        match self {
            Self::Balance
            | Self::ExtCode
            | Self::ExtCodeHash
            | Self::SLoad
            | Self::Call
            | Self::SelfDestruct => panic!("tier is priced per EVM version"),
            _ => self.gas(EvmVersion::Homestead),
        }
    }
}

/// Returns whether `evm_version` includes the `fork` schedule.
pub const fn since(evm_version: EvmVersion, fork: EvmVersion) -> bool {
    evm_version as u8 >= fork as u8
}

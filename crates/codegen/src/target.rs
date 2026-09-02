//! Target cost model.
//!
//! One place answers what code costs on the selected EVM version: the static
//! gas of every opcode across the fork schedule, the encoded size of pushes
//! and stack operations, the price of deposited bytes, and how the active
//! optimization objective ranks gas against bytes. Every pass that chooses
//! between equivalent code shapes consults [`Target`] instead of carrying its
//! own constants, so a schedule change or a new EVM version lands here once.
//!
//! Gas is the amount charged before dynamic components such as memory
//! expansion, copied words, or account warmth; account and storage accesses
//! are priced cold. The model ranks alternatives, it never bills an execution.

use crate::{
    backend::evm::{ir::compact_pushes, op, select},
    mir::Op,
};
use alloy_primitives::U256;
use solar_config::{EvmVersion, OptimizationMode};
use solar_sema::Gcx;
use std::{cmp::Ordering, ops};

/// The gas class of an opcode in the fork schedule.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum GasTier {
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
    /// A fixed price outside the tiered schedule.
    Fixed(u32),
}

impl GasTier {
    /// Gas charged by one copy opcode per copied word.
    pub(crate) const COPY_WORD_GAS: u32 = 3;
    /// Gas of a warm account or storage access under EIP-2929.
    pub(crate) const WARM_ACCESS_GAS: u32 = 100;

    /// Gas charged on `evm_version` before dynamic components.
    pub(crate) const fn gas(self, evm_version: EvmVersion) -> u32 {
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
            Self::Fixed(gas) => gas,
        }
    }

    /// Gas charged per unit of runtime-sized work: exponent bytes, hashed or
    /// copied words, and logged bytes.
    pub(crate) const fn dynamic_gas(self, evm_version: EvmVersion) -> u32 {
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

    /// Dynamic gas of a tier priced the same on every EVM version, usable in
    /// constants.
    pub(crate) const fn fixed_dynamic_gas(self) -> u32 {
        match self {
            Self::Exp => panic!("tier is priced per EVM version"),
            _ => self.dynamic_gas(EvmVersion::Homestead),
        }
    }

    /// Gas of a tier priced the same on every EVM version, usable in constants.
    pub(crate) const fn fixed_gas(self) -> u32 {
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
const fn since(evm_version: EvmVersion, fork: EvmVersion) -> bool {
    evm_version as u8 >= fork as u8
}

/// Static gas and encoded size of a code sequence.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub(crate) struct Cost {
    /// Static gas of one execution.
    pub(crate) gas: u32,
    /// Encoded bytes.
    pub(crate) bytes: u32,
}

impl Cost {
    /// Nothing emitted.
    pub(crate) const ZERO: Self = Self::new(0, 0);

    /// Creates a cost.
    pub(crate) const fn new(gas: u32, bytes: u32) -> Self {
        Self { gas, bytes }
    }

    /// Sums two costs, saturating.
    pub(crate) const fn plus(self, other: Self) -> Self {
        Self::new(self.gas.saturating_add(other.gas), self.bytes.saturating_add(other.bytes))
    }

    /// Scales a cost, saturating.
    pub(crate) const fn times(self, count: u32) -> Self {
        Self::new(self.gas.saturating_mul(count), self.bytes.saturating_mul(count))
    }
}

impl ops::Add for Cost {
    type Output = Self;

    fn add(self, other: Self) -> Self {
        self.plus(other)
    }
}

impl ops::AddAssign for Cost {
    fn add_assign(&mut self, other: Self) {
        *self = self.plus(other);
    }
}

impl std::iter::Sum for Cost {
    fn sum<I: Iterator<Item = Self>>(iter: I) -> Self {
        iter.fold(Self::ZERO, Self::plus)
    }
}

/// The cost model of the selected EVM version under the active objective.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Target {
    evm_version: EvmVersion,
    optimization: OptimizationMode,
    expected_executions: u64,
}

impl Target {
    /// Gas charged per deposited byte of runtime code (EIP-170 `CODEDEPOSIT`).
    pub(crate) const CODE_DEPOSIT_GAS_PER_BYTE: u32 = 200;
    /// Expected executions per deployment when the optimizer runs are not set;
    /// solc's convention.
    pub(crate) const DEFAULT_EXPECTED_EXECUTIONS: u64 = 200;

    /// The model of the session's EVM version, objective, and optimizer runs.
    pub(crate) fn new(gcx: Gcx<'_>) -> Self {
        let opts = &gcx.sess.opts;
        Self::with(
            opts.evm_version,
            opts.optimization,
            opts.optimizer_runs.unwrap_or(Self::DEFAULT_EXPECTED_EXECUTIONS),
        )
    }

    /// A model with explicit parameters.
    pub(crate) const fn with(
        evm_version: EvmVersion,
        optimization: OptimizationMode,
        expected_executions: u64,
    ) -> Self {
        Self { evm_version, optimization, expected_executions }
    }

    pub(crate) fn evm_version(self) -> EvmVersion {
        self.evm_version
    }

    pub(crate) fn optimization(self) -> OptimizationMode {
        self.optimization
    }

    pub(crate) fn expected_executions(self) -> u64 {
        self.expected_executions
    }

    /// Static gas of one opcode; unknown opcodes are free.
    pub(crate) fn opcode_gas(self, opcode: u8) -> u32 {
        op::definition(opcode).map_or(0, |def| def.gas.gas(self.evm_version))
    }

    /// Cost of one opcode with its immediate, before dynamic components.
    pub(crate) fn opcode(self, opcode: u8) -> Cost {
        let immediate = match opcode {
            op::PUSH1..=op::PUSH32 => u32::from(opcode - op::PUSH1 + 1),
            _ => 0,
        };
        Cost::new(self.opcode_gas(opcode), 1 + immediate)
    }

    /// Cost of one `DUP`.
    pub(crate) fn dup(self) -> Cost {
        self.opcode(op::DUP1)
    }

    /// Cost of pushing `value` through its cheapest materialization.
    pub(crate) fn push(self, value: U256) -> Cost {
        let (bytes, gas) = compact_pushes::immediate_materialization_cost(self.evm_version, value);
        Cost::new(gas as u32, bytes as u32)
    }

    /// Expected cost of one MIR operation: its opcode, or a very-low instruction
    /// for operations the backend expands by hand, plus one unit of dynamic work.
    pub(crate) fn op(self, op: &Op) -> Cost {
        let Some(lowering) = select::opcode_lowering(op) else {
            return Cost::new(GasTier::VeryLow.gas(self.evm_version), 1);
        };
        let tier = op::definition(lowering.opcode()).map_or(GasTier::VeryLow, |def| def.gas);
        Cost::new(tier.gas(self.evm_version) + tier.dynamic_gas(self.evm_version), 1)
    }

    /// Gas of copying one more word with a copy opcode.
    pub(crate) fn copy_word_gas(self) -> u32 {
        GasTier::Copy.dynamic_gas(self.evm_version)
    }

    /// Gas of one program-data copy of `size` bytes: three pushes, the copy,
    /// and its words.
    pub(crate) fn data_copy_gas(self, size: usize) -> u32 {
        let words = u32::try_from(size.div_ceil(32)).unwrap_or(u32::MAX);
        GasTier::VeryLow.gas(self.evm_version) * 3
            + self.opcode_gas(op::CODECOPY)
            + self.copy_word_gas().saturating_mul(words)
    }

    /// Deployment-lifetime gas of `cost`: expected executions of its runtime
    /// gas plus the deposit of its bytes.
    pub(crate) fn lifetime_gas(self, cost: Cost) -> u128 {
        u128::from(cost.gas) * u128::from(self.expected_executions)
            + u128::from(cost.bytes) * u128::from(Self::CODE_DEPOSIT_GAS_PER_BYTE)
    }

    /// The objective order of a cost: the dimension optimized first, then
    /// the other one. Gas leads when optimizing for gas, bytes when
    /// optimizing for size.
    pub(crate) fn objective_key(self, cost: Cost) -> [u32; 2] {
        match self.optimization {
            OptimizationMode::Size => [cost.bytes, cost.gas],
            _ => [cost.gas, cost.bytes],
        }
    }

    /// Ranks two costs under the objective.
    pub(crate) fn cmp(self, a: Cost, b: Cost) -> Ordering {
        self.objective_key(a).cmp(&self.objective_key(b))
    }

    /// Whether a change that saves `gas_saving` gas and `byte_saving` bytes
    /// per site improves the objective; negative savings are growth.
    pub(crate) fn improves(self, gas_saving: i128, byte_saving: i128) -> bool {
        if self.optimization.is_gas() {
            gas_saving > 0 && byte_saving >= 0
        } else {
            byte_saving > 0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn schedule_is_monotonic_and_fixed_tiers_agree() {
        const VERSIONS: [EvmVersion; 4] = [
            EvmVersion::Homestead,
            EvmVersion::Istanbul,
            EvmVersion::Berlin,
            EvmVersion::Amsterdam,
        ];
        for tier in [GasTier::Balance, GasTier::ExtCode, GasTier::SLoad, GasTier::Call] {
            let mut previous = 0;
            for version in VERSIONS {
                assert!(tier.gas(version) >= previous, "{tier:?} regresses at {version}");
                previous = tier.gas(version);
            }
        }
        for tier in [GasTier::Base, GasTier::VeryLow, GasTier::Mid, GasTier::High] {
            assert_eq!(tier.fixed_gas(), tier.gas(EvmVersion::Amsterdam));
        }
    }

    #[test]
    fn objectives_rank_costs() {
        let gas = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        let size = Target::with(EvmVersion::Osaka, OptimizationMode::Size, 200);
        let cheap_gas = Cost::new(3, 4);
        let cheap_bytes = Cost::new(5, 3);
        assert_eq!(gas.cmp(cheap_gas, cheap_bytes), Ordering::Less);
        assert_eq!(size.cmp(cheap_gas, cheap_bytes), Ordering::Greater);
        assert_eq!(gas.lifetime_gas(Cost::new(1, 1)), 400);
        assert!(gas.improves(1, 0));
        assert!(!gas.improves(1, -1));
        assert!(size.improves(-5, 1));
    }

    #[test]
    fn pushes_and_stack_ops() {
        let target = Target::with(EvmVersion::Osaka, OptimizationMode::Gas, 200);
        assert_eq!(target.dup(), Cost::new(3, 1));
        assert_eq!(target.opcode(op::POP), Cost::new(2, 1));
        assert_eq!(target.opcode(op::PUSH2), Cost::new(3, 3));
        assert_eq!(target.push(U256::ZERO), Cost::new(2, 1));
        assert_eq!(target.push(U256::from(0x1234)), Cost::new(3, 3));
        assert_eq!(target.data_copy_gas(64), 18);
        let legacy = Target::with(EvmVersion::Paris, OptimizationMode::Gas, 200);
        assert_eq!(legacy.push(U256::ZERO), Cost::new(3, 2));
    }
}

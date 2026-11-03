//! The gas schedule of EVM opcodes across EVM versions.
//!
//! Every opcode belongs to a [`GasTier`], whose static gas, and the gas of each unit of dynamic
//! work, depend on the EVM version. Account and storage accesses are priced cold unless a query
//! asks for the warm price of a slot or account the transaction already touched.

use alloy_primitives::U256;
use solar_config::EvmVersion;

/// Whether an account or storage slot was already touched in the transaction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Warmth {
    /// The first access in the transaction, at the full price.
    Cold,
    /// An access after an earlier one, at the reduced price EIP-2929 charges;
    /// earlier schedules do not distinguish the two.
    Warm,
}

include!(concat!(env!("OUT_DIR"), "/evm_gas.rs"));

/// Returns whether `evm_version` includes the `fork` schedule.
pub const fn since(evm_version: EvmVersion, fork: EvmVersion) -> bool {
    evm_version as u8 >= fork as u8
}

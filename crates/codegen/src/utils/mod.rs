//! Shared helper APIs used by MIR analyses, transformations, and backends.

use crate::mir::{FunctionBuilder, ValueId};
use solar_config::EvmVersion;

pub(crate) mod eval;

/// Covers Homestead's call and new-account costs plus setup emitted after `GAS`.
const PRE_TANGERINE_PRECOMPILE_GAS_RESERVE: u64 = 25_100;

/// Returns the gas operand for a precompile call while leaving enough gas for
/// the caller to handle failure on pre-Tangerine targets.
pub(crate) fn precompile_gas(
    builder: &mut FunctionBuilder<'_>,
    evm_version: EvmVersion,
) -> ValueId {
    let gas = builder.gas();
    if evm_version.can_overcharge_gas_for_call() {
        gas
    } else {
        let reserved = builder.imm_u64(PRE_TANGERINE_PRECOMPILE_GAS_RESERVE);
        builder.sub(gas, reserved)
    }
}

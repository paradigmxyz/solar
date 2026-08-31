//! Shared helper APIs used by MIR analyses, transformations, and backends.

use crate::mir::{FunctionBuilder, ValueId};
use solar_config::EvmVersion;
use solar_interface::Symbol;
use std::fmt;

pub(crate) mod eval;

/// Covers Homestead's call and new-account costs plus setup emitted after `GAS`.
const PRE_TANGERINE_PRECOMPILE_GAS_RESERVE: u64 = 25_100;

pub(crate) fn display_data_name(name: Symbol, index: usize) -> impl fmt::Display {
    fmt::from_fn(move |f| write!(f, "{name}_{index}"))
}

pub(crate) fn display_data_ref(
    name: Option<Symbol>,
    index: usize,
    offset: u32,
) -> impl fmt::Display {
    fmt::from_fn(move |f| {
        if let Some(name) = name {
            write!(f, "{}", display_data_name(name, index))?;
        } else {
            write!(f, "{index}")?;
        }
        if offset != 0 {
            write!(f, "+{offset}")?;
        }
        Ok(())
    })
}

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

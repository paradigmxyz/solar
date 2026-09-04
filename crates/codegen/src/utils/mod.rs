//! Shared helper APIs used by MIR analyses, transformations, and backends.

use crate::mir::{FunctionBuilder, ValueId};
use solar_config::EvmVersion;
use solar_interface::Symbol;
use std::fmt;

pub(crate) mod eval;

/// Homestead's `CALL` base cost plus the `SUB` and the call setup emitted after `GAS`.
///
/// Mirrors solc's `GasCosts::callGas(homestead) + 10`: 40 for the call itself and 10 for the
/// three-gas `SUB` and the pushes between the `GAS` and the `CALL`. That leaves seven gas of the
/// reserve for the call's own memory expansion, at most two words, so the output area has to be
/// touched before the reserve is computed; see
/// [`FunctionLowerer::touch_call_output_area`](crate::lower::FunctionLowerer).
const PRE_TANGERINE_CALL_GAS_RESERVE: u64 = 50;

/// Extra gas a pre-Tangerine call is charged for transferring value.
///
/// Mirrors solc's `GasCosts::callValueTransferGas`.
const PRE_TANGERINE_VALUE_TRANSFER_GAS_RESERVE: u64 = 9_000;

/// Extra gas a pre-Tangerine call is charged for creating the callee's account.
///
/// Mirrors solc's `GasCosts::callNewAccountGas`.
const PRE_TANGERINE_NEW_ACCOUNT_GAS_RESERVE: u64 = 25_000;

/// The reserve of a pre-Tangerine precompile call.
///
/// A precompile call sends no value and is not guarded by `extcodesize`, so it reserves the base
/// cost and the account-creation cost, like solc's `gasNeededByCaller` for `ECRecover`, `SHA256`
/// and `RIPEMD160`.
const PRE_TANGERINE_PRECOMPILE_GAS_RESERVE: u64 =
    PRE_TANGERINE_CALL_GAS_RESERVE + PRE_TANGERINE_NEW_ACCOUNT_GAS_RESERVE;

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
        let reserved = builder.imm(PRE_TANGERINE_PRECOMPILE_GAS_RESERVE);
        builder.sub(gas, reserved)
    }
}

/// Returns the `gas` operand of an external call that forwards the gas left, on a target that
/// predates EIP-150.
///
/// There a gas argument larger than the gas left aborts the call instead of being capped at all
/// but a 64th of it, so the operand withholds everything the `CALL` itself is charged, like
/// solc's `gasNeededByCaller`: the call base cost, the value-transfer cost when the call sends
/// value, and the account-creation cost when the callee is not already known to exist.
///
/// Everything emitted between the `GAS` and the call runs on the withheld gas, so the caller must
/// materialize this immediately before the call, with the argument encoding and the memory the
/// call needs already done.
pub(crate) fn pre_tangerine_call_gas(
    builder: &mut FunctionBuilder<'_>,
    sends_value: bool,
    may_create_account: bool,
) -> ValueId {
    // gas = sub(gas(), reserve)
    let mut reserve = PRE_TANGERINE_CALL_GAS_RESERVE;
    if sends_value {
        reserve += PRE_TANGERINE_VALUE_TRANSFER_GAS_RESERVE;
    }
    if may_create_account {
        reserve += PRE_TANGERINE_NEW_ACCOUNT_GAS_RESERVE;
    }
    let gas = builder.gas();
    let reserve = builder.imm(reserve);
    builder.sub(gas, reserve)
}

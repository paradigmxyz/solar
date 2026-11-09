//! The expansion of a call: gas, opcode, output, and failure.
//!
//! Every call that sends bytes and reads at most one word back goes through [`emit_call`]: the
//! `extcall` operation, which `lower-builtins` expands, and the external calls the frontend
//! emits itself. The caller states the differences between them as a [`CallPolicy`].

use crate::mir::{
    CallFailure, CallOutput, CallPolicy, ExtCallKind, FunctionBuilder, PtrLocation, ValueId,
    memory::EvmMemoryLayout,
};
use solar_config::EvmVersion;

/// Emits the opcode of a call and what follows it, and returns the success flag, the output word,
/// or nothing, as `policy.result_type()` says.
///
/// `input` and `output` are `(offset, size)` ranges, empty when absent. A scratch word output
/// overrides `output`.
///
/// `gas` is the explicit operand. Without it the call forwards `gas()`, less the pre-EIP-150
/// reserve where the target needs it. A `STATICCALL` becomes a `CALL` of no value where the target
/// has none.
#[allow(clippy::too_many_arguments)]
pub(crate) fn emit_call(
    builder: &mut FunctionBuilder<'_>,
    evm: EvmVersion,
    policy: CallPolicy,
    address: ValueId,
    input: Option<(ValueId, ValueId)>,
    gas: Option<ValueId>,
    value: Option<ValueId>,
    output: Option<(ValueId, ValueId)>,
) -> Option<ValueId> {
    let zero = builder.imm(0);
    let scratch = builder.imm(EvmMemoryLayout::SCRATCH_SLOT);
    let output = match policy.output {
        CallOutput::None => output,
        CallOutput::ScratchWord { clear } => {
            if clear {
                // mstore(scratch, 0)
                builder.store(scratch, zero);
            }
            Some((scratch, builder.imm(EvmMemoryLayout::WORD_SIZE)))
        }
    };
    // gas = explicit | gas() | sub(gas(), pre_tangerine_reserve)
    let gas = gas.unwrap_or_else(|| {
        if evm.can_overcharge_gas_for_call() {
            builder.gas()
        } else {
            super::pre_tangerine_call_gas(builder, value.is_some(), policy.may_create_account)
        }
    });
    let (input_offset, input_size) = input.unwrap_or((zero, zero));
    let (output_offset, output_size) = output.unwrap_or((zero, zero));
    // success = call(gas, address, value, input, output)
    //         | staticcall(gas, address, input, output)
    //         | delegatecall(gas, address, input, output)
    let success = match policy.kind {
        ExtCallKind::Call => builder.call(
            gas,
            address,
            value.unwrap_or(zero),
            input_offset,
            input_size,
            output_offset,
            output_size,
        ),
        ExtCallKind::Static if evm.has_static_call() => {
            builder.staticcall(gas, address, input_offset, input_size, output_offset, output_size)
        }
        ExtCallKind::Static => {
            builder.call(gas, address, zero, input_offset, input_size, output_offset, output_size)
        }
        ExtCallKind::Delegate => {
            builder.delegatecall(gas, address, input_offset, input_size, output_offset, output_size)
        }
    };
    match (policy.on_failure, policy.output) {
        (CallFailure::Return, _) => Some(success),
        (CallFailure::Revert, CallOutput::None) => {
            // branch success, done, failed
            // failed: revert_returndata
            builder.revert_returndata_unless(success);
            None
        }
        (CallFailure::Revert, CallOutput::ScratchWord { .. }) => {
            // branch success, done, failed
            // failed: revert_returndata
            // done: result = mload(scratch)
            builder.revert_returndata_unless(success);
            Some(builder.load(scratch, PtrLocation::Memory))
        }
    }
}

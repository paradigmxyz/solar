//! The expansion of a call: gas, opcode, and output.
//!
//! Every call that sends bytes and reads at most one word back goes through [`emit_call`]: the
//! `extcall` operation, which `lower-builtins` expands, and the external calls the frontend
//! emits itself. The caller states the differences between them as a [`CallPolicy`].

use crate::mir::{
    CallOutput, CallPolicy, ExtCallKind, FunctionBuilder, ValueId, memory::EvmMemoryLayout,
};
use solar_config::EvmVersion;

/// Emits the opcode of a call and returns its success flag.
///
/// `input` and `output` are `(offset, size)` ranges, empty when absent. A word output overrides
/// `output`, and is zeroed first unless the callee is a precompile that always writes it whole.
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
) -> ValueId {
    let mut zero = None;
    let output = match policy.output {
        CallOutput::None => output,
        CallOutput::Word => {
            let slot = builder.imm(EvmMemoryLayout::SCRATCH_SLOT);
            if builder.func().extcall_clears_word(address) {
                // mstore(scratch, 0)
                let zero = *zero.get_or_insert_with(|| builder.imm(0));
                builder.store(slot, zero);
            }
            Some((slot, builder.imm(EvmMemoryLayout::WORD_SIZE)))
        }
    };
    // gas = explicit | gas() | sub(gas(), pre_tangerine_reserve)
    let gas = gas.unwrap_or_else(|| {
        if evm.can_overcharge_gas_for_call() {
            builder.gas()
        } else {
            super::pre_tangerine_call_gas(builder, value.is_some(), true)
        }
    });
    let (input_offset, input_size) = input.unwrap_or_else(|| {
        let zero = *zero.get_or_insert_with(|| builder.imm(0));
        (zero, zero)
    });
    let (output_offset, output_size) = output.unwrap_or_else(|| {
        let zero = *zero.get_or_insert_with(|| builder.imm(0));
        (zero, zero)
    });
    // success = call(gas, address, value, input, output)
    //         | staticcall(gas, address, input, output)
    //         | delegatecall(gas, address, input, output)
    match policy.kind {
        ExtCallKind::Call => {
            let value = value.unwrap_or_else(|| *zero.get_or_insert_with(|| builder.imm(0)));
            builder.call(gas, address, value, input_offset, input_size, output_offset, output_size)
        }
        ExtCallKind::Static if evm.has_static_call() => {
            builder.staticcall(gas, address, input_offset, input_size, output_offset, output_size)
        }
        ExtCallKind::Static => {
            let zero = *zero.get_or_insert_with(|| builder.imm(0));
            builder.call(gas, address, zero, input_offset, input_size, output_offset, output_size)
        }
        ExtCallKind::Delegate => {
            builder.delegatecall(gas, address, input_offset, input_size, output_offset, output_size)
        }
    }
}

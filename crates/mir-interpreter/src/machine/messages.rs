//! Typed message adapters, independent of builtin lowering.
//!
//! External-call input is empty, a memory object (whose length word is read), or
//! a logical memory slice. Output is discarded or written to a scratch word,
//! zeroed first except for constant SHA-256 and RIPEMD-160 callees. Returndata
//! is replaced. Implicit gas requires a host that prices the caller's boundary
//! and applies the EIP-150 cap. Returndata objects reserve
//! uninitialized memory, write their length and copy exactly the returned bytes.
//!
//! `lower_builtins::lower_extcall` samples GAS before CALL on EIP-150
//! targets. That request is at least the gas left after CALL's upfront charges,
//! so its clipped budget is the cap. Before EIP-150, the compiler's reserve
//! depends on instruction pricing; that case remains unsupported here.

use super::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_evmir::op;
use solar_mir::{AllocationSemantics, CallOutput};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn semantic_extcall(
        &mut self,
        mut opcode: u8,
        output: CallOutput,
        clear_word: bool,
        address: U256,
        input: Option<RuntimeValue>,
        gas: Option<U256>,
        value: Option<U256>,
    ) -> ControlFlow<Outcome, U256> {
        if opcode == op::STATICCALL && self.lacks(opcode) {
            if !(U256::from(1)..=U256::from(4)).contains(&address) {
                return ControlFlow::Break(Outcome::Unsupported(
                    "pre-Byzantium static extcall requires a precompile",
                ));
            }
            opcode = op::CALL;
        }
        if self.lacks(opcode) {
            return ControlFlow::Break(Outcome::Invalid);
        }
        if self.world.is_none() {
            return ControlFlow::Break(Outcome::Unsupported("address call needs a host"));
        }
        if gas.is_none() && self.semantic.is_some_and(|evm| !evm.can_overcharge_gas_for_call()) {
            return ControlFlow::Break(Outcome::Unsupported(
                "pre-EIP-150 implicit address-call gas requires instruction pricing",
            ));
        }
        let (offset, length) = match input {
            None => (U256::ZERO, U256::ZERO),
            Some(RuntimeValue::Word(object)) => {
                let length = limit(self.memory.load(object))?;
                (object.wrapping_add(U256::from(32)), length)
            }
            Some(RuntimeValue::Aggregate(fields)) => {
                let [RuntimeValue::Word(pointer), RuntimeValue::Word(length)] = fields.as_slice()
                else {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "undefined address-call input",
                    ));
                };
                (*pointer, *length)
            }
            _ => return ControlFlow::Break(Outcome::Unsupported("undefined address-call input")),
        };
        if output == CallOutput::Word && clear_word {
            limit(self.memory.store(U256::ZERO, U256::ZERO))?;
        }
        let mut args = vec![address];
        if opcode == op::CALL {
            args.push(value.unwrap_or_default());
        } else if value.is_some() {
            return ControlFlow::Break(Outcome::Unsupported("value on non-value address call"));
        }
        let output_length = if output == CallOutput::Word { 32 } else { 0 };
        args.extend([offset, length, U256::ZERO, U256::from(output_length)]);
        self.external_message(opcode, &args, gas)
    }

    pub(super) fn semantic_returndata_bytes(&mut self) -> ControlFlow<Outcome, U256> {
        let Some(world) = &self.world else {
            return ControlFlow::Break(Outcome::Unsupported("returndata object needs a host"));
        };
        let data = world.effects.return_data.clone();
        let length = U256::from(data.len());
        let size = (length + U256::from(63)) & !U256::from(31);
        let pointer = self.semantic_allocate(size, AllocationSemantics::INTERNAL)?;
        limit(self.memory.store(pointer, length))?;
        let start = pointer.wrapping_add(U256::from(32));
        self.burn_words(length)?;
        limit(self.memory.write(start, &data))?;
        ControlFlow::Continue(pointer)
    }

    pub(super) fn semantic_destroy(&mut self, beneficiary: U256) -> ControlFlow<Outcome> {
        if self.semantic.is_some_and(|version| version >= EvmVersion::Amsterdam) {
            return ControlFlow::Break(Outcome::Unsupported(
                "Amsterdam MIR selfdestruct semantics",
            ));
        }
        let Some(world) = &mut self.world else {
            return ControlFlow::Break(Outcome::Unsupported("selfdestruct needs a host"));
        };
        if world.host.is_static() {
            return ControlFlow::Break(Outcome::Invalid);
        }
        let storage = world.effects.storage.iter().map(|(&k, &v)| (k, v)).collect::<Vec<_>>();
        let transient = world.effects.transient.iter().map(|(&k, &v)| (k, v)).collect::<Vec<_>>();
        let pending = super::PendingChanges {
            storage: &storage,
            transient: &transient,
            logs: &world.effects.logs,
        };
        if let Err(reason) = world.host.destroy(beneficiary, pending) {
            return ControlFlow::Break(Outcome::Unsupported(reason));
        }
        world.effects.storage.clear();
        world.effects.transient.clear();
        world.effects.logs.clear();
        ControlFlow::Continue(())
    }
}

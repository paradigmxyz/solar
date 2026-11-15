//! Typed message adapters, independent of builtin lowering.
//!
//! Address-call input is either a memory object (whose length word is read) or
//! a logical memory slice. Calls have no output buffer and replace volatile
//! returndata. Implicit gas requires a host that prices the caller's boundary
//! and applies the EIP-150 cap.
//!
//! Extcall input is absent, a memory object or a logical memory slice. Its output
//! is nothing or the scratch word. Explicit extcall gas uses the existing
//! sufficient-caller-gas boundary; implicit extcall gas is an ordered builtin_gas
//! boundary input. Returndata objects reserve
//! uninitialized memory, write their length and copy exactly the returned bytes.
//!
//! `lower_builtins::lower_address_call` samples GAS before CALL on EIP-150
//! targets. That request is at least the gas left after CALL's upfront charges,
//! so its clipped budget is the cap. Before EIP-150, the compiler's reserve
//! depends on instruction pricing; that case remains unsupported here.

use super::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_config::EvmVersion;
use solar_evmir::op;
use solar_mir::AllocationSemantics;
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn semantic_address_call(
        &mut self,
        opcode: u8,
        address: U256,
        input: RuntimeValue,
        gas: Option<U256>,
        value: Option<U256>,
    ) -> ControlFlow<Outcome, U256> {
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
            RuntimeValue::Word(object) => {
                let length = limit(self.memory.load(object))?;
                (object.wrapping_add(U256::from(32)), length)
            }
            RuntimeValue::Aggregate(fields) => {
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
        let mut args = vec![address];
        if opcode == op::CALL {
            args.push(value.unwrap_or_default());
        } else if value.is_some() {
            return ControlFlow::Break(Outcome::Unsupported("value on non-value address call"));
        }
        args.extend([offset, length, U256::ZERO, U256::ZERO]);
        self.external_message(opcode, &args, gas)
    }

    /// One `extcall`. A missing input is an empty range. A word output zeroes scratch first
    /// when `clears` (every callee but the constant SHA-256 and RIPEMD-160 precompiles, which
    /// write it whole) and receives the first 32 bytes of returndata. Explicit gas is the
    /// sufficient-caller-gas boundary; implicit gas takes the next `builtin_gas` budget, an
    /// explicit boundary input, instead of pricing the caller. A static call on a target
    /// without `STATICCALL` is a zero-value `CALL`, as lowering emits for precompiles.
    pub(super) fn semantic_extcall(
        &mut self,
        opcode: u8,
        address: U256,
        input: Option<RuntimeValue>,
        gas: Option<U256>,
        value: Option<U256>,
        word: Option<bool>,
    ) -> ControlFlow<Outcome, U256> {
        let opcode =
            if opcode == op::STATICCALL && self.lacks(op::STATICCALL) { op::CALL } else { opcode };
        if self.lacks(opcode) {
            return ControlFlow::Break(Outcome::Invalid);
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
                    return ControlFlow::Break(Outcome::Unsupported("undefined extcall input"));
                };
                (*pointer, *length)
            }
            Some(_) => return ControlFlow::Break(Outcome::Unsupported("undefined extcall input")),
        };
        let gas = match gas {
            Some(gas) => gas,
            None => {
                let Some(world) = &mut self.world else {
                    return ControlFlow::Break(Outcome::Unsupported("extcall needs a host"));
                };
                let Some(gas) = world.host.builtin_gas() else {
                    return ControlFlow::Break(Outcome::Unsupported(
                        "missing MIR builtin gas budget",
                    ));
                };
                gas
            }
        };
        let mut args = vec![gas, address];
        if opcode == op::CALL {
            args.push(value.unwrap_or_default());
        } else if value.is_some() {
            return ControlFlow::Break(Outcome::Unsupported("value on non-value extcall"));
        }
        let output = match word {
            None => U256::ZERO,
            Some(clears) => {
                if clears {
                    limit(self.memory.store(U256::ZERO, U256::ZERO))?;
                }
                U256::from(32)
            }
        };
        args.extend([offset, length, U256::ZERO, output]);
        let Some(result) = self.world_opcode(opcode, &args)? else {
            return ControlFlow::Break(Outcome::Unsupported("missing extcall result"));
        };
        ControlFlow::Continue(result)
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

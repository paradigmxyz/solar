//! Typed message adapters, independent of builtin lowering.
//!
//! Address-call input is either a memory object (whose length word is read) or
//! a logical memory slice. Calls have no output buffer and replace volatile
//! returndata. Explicit gas uses the existing sufficient-caller-gas boundary;
//! implicit gas requires pricing and is unsupported. Returndata objects reserve
//! uninitialized memory, write their length and copy exactly the returned bytes.

use super::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_evmir::op;
use solar_mir::AllocationSemantics;
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn semantic_address_call(
        &mut self,
        opcode: u8,
        address: U256,
        input: RuntimeValue,
        gas: U256,
        value: Option<U256>,
    ) -> ControlFlow<Outcome, U256> {
        if self.lacks(opcode) {
            return ControlFlow::Break(Outcome::Invalid);
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
        let mut args = vec![gas, address];
        if opcode == op::CALL {
            args.push(value.unwrap_or_default());
        } else if value.is_some() {
            return ControlFlow::Break(Outcome::Unsupported("value on non-value address call"));
        }
        args.extend([offset, length, U256::ZERO, U256::ZERO]);
        let Some(result) = self.world_opcode(opcode, &args)? else {
            return ControlFlow::Break(Outcome::Unsupported("missing address-call result"));
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
}

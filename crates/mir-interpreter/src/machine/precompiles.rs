//! Direct precompile-backed builtins with explicit child gas budgets. SHA-256
//! and RIPEMD-160 consume an object or slice synchronously; recovery stages four
//! words at FMP without reserving it. Results, failure, scratch and returndata
//! come from real host execution. No caller gas or backend layout is invented.

use super::{Outcome, Run, RuntimeValue, limit};
use alloy_primitives::U256;
use solar_evmir::op;
use solar_mir::Builtin;
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn semantic_precompile(
        &mut self,
        builtin: &Builtin,
        values: &[RuntimeValue],
    ) -> ControlFlow<Outcome, U256> {
        let (address, offset, length) = match (builtin, values) {
            (Builtin::Sha256 | Builtin::Ripemd160, [input]) => {
                let (offset, length) = match input {
                    RuntimeValue::Word(object) => {
                        let length = limit(self.memory.load(*object))?;
                        (object.wrapping_add(U256::from(32)), length)
                    }
                    RuntimeValue::Aggregate(fields) => {
                        let [RuntimeValue::Word(pointer), RuntimeValue::Word(length)] =
                            fields.as_slice()
                        else {
                            return ControlFlow::Break(Outcome::Unsupported("hash input slice"));
                        };
                        (*pointer, *length)
                    }
                    _ => {
                        return ControlFlow::Break(Outcome::Unsupported(
                            "hash input representation",
                        ));
                    }
                };
                (if matches!(builtin, Builtin::Sha256) { 2u64 } else { 3 }, offset, length)
            }
            (
                Builtin::EcRecover,
                [
                    RuntimeValue::Word(hash),
                    RuntimeValue::Word(v),
                    RuntimeValue::Word(r),
                    RuntimeValue::Word(s),
                ],
            ) => {
                let input = limit(self.memory.load(U256::from(64)))?;
                for (index, word) in [*hash, *v, *r, *s].into_iter().enumerate() {
                    limit(self.memory.store(input.wrapping_add(U256::from(index * 32)), word))?;
                }
                limit(self.memory.store(U256::ZERO, U256::ZERO))?;
                (1, input, U256::from(128))
            }
            _ => return ControlFlow::Break(Outcome::Unsupported("precompile builtin arity")),
        };
        let Some(world) = &mut self.world else {
            return ControlFlow::Break(Outcome::Unsupported("precompile builtin needs a host"));
        };
        let Some(gas) = world.host.builtin_gas() else {
            return ControlFlow::Break(Outcome::Unsupported("missing MIR builtin gas budget"));
        };
        let mut args = vec![gas, U256::from(address)];
        let opcode = if self.lacks(op::STATICCALL) {
            args.push(U256::ZERO);
            op::CALL
        } else {
            op::STATICCALL
        };
        args.extend([offset, length, U256::ZERO, U256::from(32)]);
        let Some(success) = self.world_opcode(opcode, &args)? else {
            return ControlFlow::Break(Outcome::Unsupported("missing precompile result"));
        };
        if success.is_zero() {
            let data = if self.lacks(op::RETURNDATASIZE) {
                Vec::new()
            } else {
                self.world.as_ref().expect("precompile has a host").effects.return_data.clone()
            };
            return ControlFlow::Break(Outcome::Revert(data));
        }
        let digest = limit(self.memory.load(U256::ZERO))?;
        ControlFlow::Continue(if matches!(builtin, Builtin::Ripemd160) {
            digest << 96
        } else {
            digest
        })
    }
}

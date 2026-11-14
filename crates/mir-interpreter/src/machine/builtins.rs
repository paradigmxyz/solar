//! Direct namespace semantics. Namespace derivation reads the byte object
//! before overwriting scratch. Value transfers are extcalls (see messages).
//! Caller-side gas pricing follows the semantic replay boundary; no backend
//! lowering or synthesized child results are involved.

use super::{Outcome, Run, limit, panic_data};
use alloy_primitives::{U256, keccak256};
use solar_mir::{Builtin, RevertKind};
use std::ops::ControlFlow;

impl Run<'_, '_, '_> {
    pub(super) fn semantic_builtin_words(
        &mut self,
        builtin: &Builtin,
        args: &[U256],
    ) -> ControlFlow<Outcome, Option<U256>> {
        let word = match (builtin, args) {
            (Builtin::CheckedAddMod | Builtin::CheckedMulMod, [_, _, modulus]) => {
                if modulus.is_zero() {
                    return ControlFlow::Break(Outcome::Revert(panic_data(U256::from(0x12))));
                }
                let name =
                    if matches!(builtin, Builtin::CheckedAddMod) { "addmod" } else { "mulmod" };
                return self.opcode(name, name, args);
            }
            (Builtin::Erc7201, &[object]) => self.semantic_namespace(object)?,
            (Builtin::Check { is_zero, failure }, &[condition]) => {
                if condition.is_zero() == *is_zero {
                    return ControlFlow::Break(Outcome::Revert(match failure {
                        RevertKind::Panic(code) => panic_data(U256::from(*code as u8)),
                        RevertKind::Reason(reason) => {
                            if self.world.as_ref().is_some_and(|world| world.host.debug_reverts()) {
                                reason.message().map_or_else(Vec::new, |message| {
                                    super::abi::literal_error(message.as_bytes())
                                })
                            } else {
                                Vec::new()
                            }
                        }
                    }));
                }
                return ControlFlow::Continue(None);
            }
            _ => return ControlFlow::Break(Outcome::Unsupported("unmodeled builtin or arity")),
        };
        ControlFlow::Continue(Some(word))
    }

    pub(super) fn semantic_namespace(&mut self, object: U256) -> ControlFlow<Outcome, U256> {
        let length = limit(self.memory.load(object))?;
        self.burn_words(length)?;
        let bytes = limit(self.memory.read(object.wrapping_add(U256::from(32)), length))?;
        let inner = U256::from_be_bytes(keccak256(bytes).0).wrapping_sub(U256::from(1));
        let outer = self.fixed_slot(&[inner])?;
        ControlFlow::Continue(outer & !U256::from(255))
    }
}

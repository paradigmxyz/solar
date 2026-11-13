//! Direct namespace and value-transfer semantics. Namespace derivation reads
//! the byte object before overwriting scratch. Value transfers forward exactly
//! the Solidity stipend and retain real child failure and returndata effects.
//! Caller-side gas pricing follows the semantic replay boundary; no backend
//! lowering or synthesized child results are involved.

use super::{Outcome, Run, limit, panic_data};
use alloy_primitives::{U256, keccak256};
use solar_evmir::op;
use solar_mir::{Builtin, RevertKind, RevertReason};
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
            (Builtin::Send | Builtin::Transfer, &[address, amount]) => {
                let success = self.semantic_send(address, amount)?;
                if matches!(builtin, Builtin::Transfer) {
                    if success.is_zero() {
                        let data = self
                            .world
                            .as_ref()
                            .expect("send has a host")
                            .effects
                            .return_data
                            .clone();
                        return ControlFlow::Break(Outcome::Revert(data));
                    }
                    return ControlFlow::Continue(None);
                }
                success
            }
            (Builtin::Check { is_zero, failure }, &[condition]) => {
                if condition.is_zero() == *is_zero {
                    return ControlFlow::Break(Outcome::Revert(match failure {
                        RevertKind::Panic(code) => panic_data(U256::from(*code as u8)),
                        RevertKind::Reason(RevertReason::Empty) => Vec::new(),
                        _ => {
                            return ControlFlow::Break(Outcome::Unsupported("check revert reason"));
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

    pub(super) fn semantic_send(
        &mut self,
        address: U256,
        amount: U256,
    ) -> ControlFlow<Outcome, U256> {
        // The CALL stipend is added only for nonzero value; request it otherwise.
        let gas = if amount.is_zero() { U256::from(2300) } else { U256::ZERO };
        let Some(result) = self.world_opcode(
            op::CALL,
            &[gas, address, amount, U256::ZERO, U256::ZERO, U256::ZERO, U256::ZERO],
        )?
        else {
            return ControlFlow::Break(Outcome::Unsupported("missing send result"));
        };
        ControlFlow::Continue(result)
    }
}

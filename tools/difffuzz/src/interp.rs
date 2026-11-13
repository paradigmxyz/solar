//! Runs a call on Solar's MIR and EVM IR interpreters, with the storage the EVM held before it.

use crate::obs::{Obs, Status};
use alloy_primitives::{Address, U256};
use solar_config::EvmVersion;
use solar_evmir::op;
use std::collections::BTreeMap;

/// The call's context, as the EVM saw it.
pub(crate) struct Ctx<'a> {
    pub(crate) storage: &'a BTreeMap<U256, U256>,
    pub(crate) address: Address,
    pub(crate) caller: Address,
    pub(crate) heap_start: Option<U256>,
    pub(crate) code_len: usize,
}

impl Ctx<'_> {
    fn read(&self, opcode: u8, _operands: &[U256]) -> Option<U256> {
        Some(match opcode {
            op::ADDRESS => self.address.into_word().into(),
            op::CALLER | op::ORIGIN => self.caller.into_word().into(),
            op::CALLVALUE | op::GASPRICE | op::SELFBALANCE => U256::ZERO,
            op::CHAINID => U256::from(1),
            op::CODESIZE => U256::from(self.code_len),
            _ => return None,
        })
    }

    fn slot(&self, slot: U256) -> U256 {
        self.storage.get(&slot).copied().unwrap_or_default()
    }
}

struct MirHost<'a>(&'a Ctx<'a>);

impl solar_mir_interpreter::Host for MirHost<'_> {
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        self.0.read(opcode, operands)
    }

    fn storage(&mut self, slot: U256) -> U256 {
        self.0.slot(slot)
    }

    fn free_memory_start(&mut self) -> U256 {
        self.0.heap_start.unwrap_or(U256::from(0x80))
    }
}

struct EvmIrHost<'a>(&'a Ctx<'a>);

impl solar_evmir_interpreter::Host for EvmIrHost<'_> {
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        self.0.read(opcode, operands)
    }

    fn storage(&mut self, slot: U256) -> U256 {
        self.0.slot(slot)
    }
}

fn logs<L>(logs: &[L], f: impl Fn(&L) -> (Vec<U256>, Vec<u8>)) -> Vec<(Vec<U256>, Vec<u8>)> {
    logs.iter().map(f).collect()
}

/// Runs the call on the final lowered MIR.
pub(crate) fn mir(module: &str, calldata: &[u8], ctx: &Ctx<'_>) -> Obs {
    use solar_mir_interpreter::{Options, Outcome};
    let options = Options {
        evm_version: EvmVersion::Osaka,
        check_integer_widths: true,
        ..Options::default()
    };
    let ex = match solar_mir_interpreter::transact(module, calldata, &mut MirHost(ctx), options) {
        Ok(ex) => ex,
        Err(e) => {
            return Obs { status: Status::Halt(format!("invalid MIR: {e}")), ..Obs::default() };
        }
    };
    let (status, output) = match ex.outcome {
        Outcome::Success(d) => (Status::Success, d),
        Outcome::Revert(d) => (Status::Revert, d),
        Outcome::Halt => (Status::Halt("halt".into()), vec![]),
        Outcome::ReturndataOutOfBounds => (Status::Halt("returndata out of bounds".into()), vec![]),
        Outcome::Unsupported(r) => return Obs::unsupported(r),
        Outcome::WidthViolated(w) => (Status::Halt(format!("MIR integer invariant: {w}")), vec![]),
    };
    let logs = logs(&ex.logs, |l| (l.topics.clone(), l.data.clone()));
    Obs { status, output, logs, writes: ex.storage.into_iter().collect(), ..Obs::default() }
}

/// Runs the call on an EVM IR module.
pub(crate) fn evmir(module: &str, calldata: &[u8], ctx: &Ctx<'_>) -> Obs {
    use solar_evmir_interpreter::{Options, Outcome};
    let options = Options { evm_version: EvmVersion::Osaka, ..Options::default() };
    let ex = match solar_evmir_interpreter::transact(module, calldata, &mut EvmIrHost(ctx), options)
    {
        Ok(ex) => ex,
        Err(e) => {
            return Obs { status: Status::Halt(format!("invalid EVM IR: {e}")), ..Obs::default() };
        }
    };
    let (status, output) = match ex.outcome {
        Outcome::Success(d) => (Status::Success, d),
        Outcome::Revert(d) => (Status::Revert, d),
        Outcome::Halt(h) => (Status::Halt(format!("{h:?}")), vec![]),
        Outcome::Unsupported(r) => return Obs::unsupported(r),
        #[allow(unreachable_patterns)]
        other => return Obs::unsupported(format!("{other:?}")),
    };
    let logs = logs(&ex.logs, |l| (l.topics.clone(), l.data.clone()));
    Obs { status, output, logs, writes: ex.storage.into_iter().collect(), ..Obs::default() }
}

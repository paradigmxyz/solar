//! Checks each `run-call` of a test written in lowered MIR against the MIR interpreter.
//!
//! The test runner executes a call in an EVM as usual, then compiles the test again with
//! `-Zdump=mir-final` and runs the same call on the called contract's dumped MIR through
//! `solar_mir_interpreter`, with the storage, balances, and code the EVM held just before the
//! call. Both runs execute the same compiled program, so they must end the same way, return the
//! same data, emit the same logs, and write the same storage. A test written in MIR exists to run
//! its calls both ways, so a call the interpreter cannot run, or one the EVM ends by running out
//! of gas or stack, which the interpreter does not model, fails the test.

use super::{
    dump::{self, Kind},
    trace::{Context, Trace},
};
use alloy_primitives::{U256, hex};
use evm2::interpreter::InstrStop;
use solar_config::EvmVersion;
use solar_evmir::op;
use solar_mir_interpreter::{self as interpret, Host, Outcome};
use ui_test::{build_manager::BuildManager, per_test_config::TestConfig};

/// The final MIR modules the backend compiled.
const KIND: Kind = Kind {
    dump: "mir-final",
    name: "MIR",
    modules: |text| {
        interpret::parse_dump(text).into_iter().map(|module| (module.name, module.mir)).collect()
    },
};

/// A call to check.
pub(super) struct Call<'a> {
    /// The called contract, as the compiler names it.
    pub(super) contract: &'a str,
    /// The call as the directive shows it.
    pub(super) name: &'a str,
    pub(super) input: &'a [u8],
    pub(super) value: U256,
    pub(super) evm_version: EvmVersion,
}

/// Returns whether the check runs for the test: whether the test is written in MIR.
pub(super) fn applies(config: &TestConfig) -> bool {
    config.status.path().extension().is_some_and(|extension| extension == "mir")
}

/// Answers the MIR interpreter's reads from the captured call context and the heap start the EVM
/// stored.
struct EvmHost<'a> {
    context: Context<'a>,
    heap_start: Option<U256>,
}

impl Host for EvmHost<'_> {
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        self.context.read(opcode, operands)
    }

    fn storage(&mut self, slot: U256) -> U256 {
        self.context.storage(slot)
    }

    fn free_memory_start(&mut self) -> U256 {
        self.heap_start.unwrap_or(U256::from(0x80))
    }
}

/// Runs `call` on its contract's MIR and checks that it agrees with `trace`, the EVM's run.
pub(super) fn check(
    config: &TestConfig,
    build_manager: &BuildManager,
    call: &Call<'_>,
    trace: &Trace,
) -> Result<(), String> {
    let unsupported =
        |reason: &str| Err(format!("the MIR interpreter cannot check `{}`: {reason}", call.name));
    let dumps = dump::dumps(config, build_manager, &KIND);
    let dump = match dumps.as_ref() {
        Ok(dump) => dump,
        Err(error) => {
            return unsupported(error);
        }
    };
    let Some(module) = dump.modules.get(call.contract) else {
        return unsupported("the MIR dump has no module for the contract");
    };
    // Every contract a call deploys has a runtime: a dump without it would fail every call.
    let Some(runtime) = dump.runtimes.get(call.contract) else {
        return Err(format!("the MIR dump has no runtime for `{}`", call.contract));
    };
    // A constructor may deploy other code, and deployment patches immutables into the runtime.
    if trace.before.deployed_code() != Some(&runtime[..]) {
        return unsupported("the deployed code is not the compiled runtime");
    }
    let expected = match trace.stop {
        stop if stop.is_success() => Outcome::Success(trace.output.clone()),
        InstrStop::Revert => Outcome::Revert(trace.output.clone()),
        InstrStop::OutOfOffset if trace.last_opcode == Some(op::RETURNDATACOPY) => {
            Outcome::ReturndataOutOfBounds
        }
        InstrStop::OutOfGas
        | InstrStop::MemoryOOG
        | InstrStop::MemoryLimitOOG
        | InstrStop::PrecompileOOG
        | InstrStop::InvalidOperandOOG
        | InstrStop::StackOverflow
        | InstrStop::CallTooDeep => {
            return unsupported(&format!("the EVM ended with {:?}", trace.stop));
        }
        stop if expected_halt(stop, trace.last_opcode, call.evm_version) => Outcome::Halt,
        _ => {
            return Err(format!(
                "unexpected EVM halt: {:?} at opcode {:?}",
                trace.stop, trace.last_opcode
            ));
        }
    };
    let context = Context::new(&trace.before, call.value);
    let mut host = EvmHost { context, heap_start: trace.heap_start };
    let options = interpret::Options { evm_version: call.evm_version, ..Default::default() };
    let execution = match interpret::transact(module, call.input, &mut host, options) {
        Ok(execution) => execution,
        Err(reason) => return Err(format!("invalid final MIR for `{}`: {reason}", call.contract)),
    };
    if let Outcome::Unsupported(reason) = &execution.outcome {
        return unsupported(reason);
    }

    let mut differences = Vec::new();
    if execution.outcome != expected {
        differences.push(format!(
            "the EVM {}, the MIR {}",
            describe(&expected, Some(trace.stop)),
            describe(&execution.outcome, None)
        ));
    } else if matches!(expected, Outcome::Success(_)) {
        let logs = execution.logs.iter().map(|log| (log.topics.clone(), log.data.clone()));
        differences.extend(trace.effect_differences(logs.collect(), &execution.storage, "MIR"));
    }
    if differences.is_empty() {
        return Ok(());
    }
    Err(format!("`{}` disagrees with its MIR: {}", call.name, differences.join("; ")))
}

/// Describes how a run ended, with the EVM's reason for halting if it has one.
fn describe(outcome: &Outcome, stop: Option<InstrStop>) -> String {
    match (outcome, stop) {
        (Outcome::Success(data), _) => format!("returned 0x{}", hex::encode(data)),
        (Outcome::Revert(data), _) => format!("reverted with 0x{}", hex::encode(data)),
        (Outcome::Halt, Some(stop)) => format!("halted ({stop:?})"),
        (Outcome::Halt, None) => "halted".into(),
        (Outcome::ReturndataOutOfBounds, _) => "halted on an out-of-bounds returndata copy".into(),
        (Outcome::Unsupported(reason), _) => format!("could not run: {reason}"),
    }
}

/// Accept only the explicit invalid instruction or REVERT before its activation.
fn expected_halt(stop: InstrStop, opcode: Option<u8>, version: EvmVersion) -> bool {
    (stop == InstrStop::InvalidOpcode && opcode == Some(op::INVALID))
        || (matches!(stop, InstrStop::NotActivated | InstrStop::InvalidOpcode)
            && opcode == Some(op::REVERT)
            && version < EvmVersion::Byzantium)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_unexpected_halts() {
        assert!(expected_halt(InstrStop::InvalidOpcode, Some(op::INVALID), EvmVersion::Osaka));
        assert!(expected_halt(InstrStop::NotActivated, Some(op::REVERT), EvmVersion::Homestead));
        for stop in [
            InstrStop::InvalidJump,
            InstrStop::StackUnderflow,
            InstrStop::InvalidOpcode,
            InstrStop::NotActivated,
        ] {
            assert!(!expected_halt(stop, Some(op::JUMP), EvmVersion::Osaka));
            assert!(!expected_halt(stop, Some(op::REVERT), EvmVersion::Osaka));
        }
    }
}

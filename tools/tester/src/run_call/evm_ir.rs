//! Checks each `run-call` against the EVM IR interpreter when `SOLAR_RUN_CALL_EVM_IR` is set, and
//! always in a test written in EVM IR.
//!
//! The test runner executes a call in an EVM as usual, then runs the same call through
//! `solar_evmir_interpreter`, with the storage, balances, and code the EVM held just
//! before the call. A Solidity test is compiled again with `-Zdump=evm-ir-runtime-scheduled`, and
//! the call runs on the called contract's runtime EVM IR as stack scheduling emitted it, before the
//! EVM IR pipeline. A test written in EVM IR runs its calls on its own text. Either way the EVM ran
//! that program after the EVM IR pipeline and assembly, so both runs must end the same way, return
//! the same data, emit the same logs, and write the same storage. A call the interpreter cannot run
//! is skipped, and so is one the EVM ends by running out of gas or stack, which the interpreter
//! does not model, or the interpreter ends by overflowing the stack, whose depth the pipeline may
//! change.
//!
//! With `SOLAR_RUN_CALL_EVM_IR=1`, only a disagreement or a harness fault, such as a failed dump,
//! is reported, as a test failure. Any other value names a file that also receives one line per
//! call: `checked`, `skipped` with the reason, `mismatch`, or `failed`.
//!
//! A test written in EVM IR exists to run its calls both ways, so it is checked without the
//! variable, and a call the interpreter cannot run fails it instead of being skipped.

use super::{
    dump::{self, Kind},
    trace::{Context, Trace},
};
use alloy_primitives::{U256, hex};
use evm2::interpreter::InstrStop;
use solar_config::EvmVersion;
use solar_evmir_interpreter::{self as interpret, Halt, Host, Outcome};
use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    sync::Mutex,
};
use ui_test::{build_manager::BuildManager, per_test_config::TestConfig};

/// The environment variable that enables the check.
const VARIABLE: &str = "SOLAR_RUN_CALL_EVM_IR";

/// The program the dump names for the runtime EVM IR of a contract before the EVM IR pipeline.
const SCHEDULED_RUNTIME: &str = "scheduled runtime";

/// The scheduled runtime EVM IR of each contract.
const KIND: Kind = Kind {
    dump: "evm-ir-runtime-scheduled",
    name: "EVM IR",
    modules: |text| {
        interpret::parse_dump(text)
            .into_iter()
            .filter(|module| module.program.as_deref() == Some(SCHEDULED_RUNTIME))
            .map(|module| (module.name, module.evm_ir))
            .collect()
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

/// Returns whether the check is enabled.
fn enabled() -> bool {
    std::env::var_os(VARIABLE).is_some_and(|value| !value.is_empty())
}

/// Returns whether the check runs for the test: always for a test written in EVM IR, and for every
/// other test when the check is enabled.
pub(super) fn applies(config: &TestConfig) -> bool {
    enabled() || is_evm_ir(config)
}

/// Returns whether the test is written in EVM IR.
pub(super) fn is_evm_ir(config: &TestConfig) -> bool {
    config.status.path().extension().is_some_and(|extension| extension == "evmir")
}

impl Host for Context<'_> {
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        Context::read(self, opcode, operands)
    }

    fn storage(&mut self, slot: U256) -> U256 {
        Context::storage(self, slot)
    }
}

/// Runs `call` on its contract's EVM IR and checks that it agrees with `trace`, the EVM's run.
pub(super) fn check(
    config: &TestConfig,
    build_manager: &BuildManager,
    call: &Call<'_>,
    trace: &Trace,
) -> Result<(), String> {
    let report = |status: &str, detail: &str| record(config, call.name, status, detail);
    let skip = |reason: &str| {
        report("skipped", reason);
        if is_evm_ir(config) {
            return Err(format!("the EVM IR interpreter cannot check `{}`: {reason}", call.name));
        }
        Ok(())
    };
    // A fault in the harness, not a program the interpreter cannot run: a skip would turn the
    // check off without a trace.
    let broken = |reason: &str| {
        report("failed", reason);
        Err(format!("the EVM IR check of `{}` cannot run: {reason}", call.name))
    };
    let expected = match trace.stop {
        stop if stop.is_success() => Some(Outcome::Success(trace.output.clone())),
        InstrStop::Revert => Some(Outcome::Revert(trace.output.clone())),
        InstrStop::OutOfGas
        | InstrStop::MemoryOOG
        | InstrStop::MemoryLimitOOG
        | InstrStop::PrecompileOOG
        | InstrStop::InvalidOperandOOG
        | InstrStop::StackOverflow
        | InstrStop::ReentrancySentryOOG
        | InstrStop::CallTooDeep => {
            return skip(&format!("the EVM ended with {:?}", trace.stop));
        }
        // Any other exceptional halt: halts compare equal whatever their reason.
        _ => None,
    };
    let program = if is_evm_ir(config) {
        match read(config.status.path()) {
            Ok(program) => program,
            Err(error) => return skip(&format!("cannot read the test: {error}")),
        }
    } else {
        let dumps = dump::dumps(config, build_manager, &KIND);
        let dump = match dumps.as_ref() {
            Ok(dump) => dump,
            Err(error) => return broken(error),
        };
        let Some(module) = dump.modules.get(call.contract) else {
            return broken("the EVM IR dump has no scheduled runtime for the contract");
        };
        // Every contract a call deploys has a runtime: a dump without it would skip every call.
        let Some(runtime) = dump.runtimes.get(call.contract) else {
            return Err(format!("the EVM IR dump has no runtime for `{}`", call.contract));
        };
        // A constructor may deploy other code, and deployment patches immutables into the
        // runtime.
        if trace.before.deployed_code() != Some(&runtime[..]) {
            return skip("the deployed code is not the compiled runtime");
        }
        module.clone()
    };
    let mut host = Context::new(&trace.before, call.value);
    let options = interpret::Options { evm_version: call.evm_version, ..Default::default() };
    let execution = match interpret::transact(&program, call.input, &mut host, options) {
        Ok(execution) => execution,
        Err(reason) if is_evm_ir(config) => return skip(&reason),
        Err(reason) => return broken(&reason),
    };
    match &execution.outcome {
        Outcome::Unsupported(reason) => return skip(reason),
        Outcome::Halt(Halt::StackOverflow) => {
            return skip("the EVM IR overflowed the stack, whose depth the pipeline may change");
        }
        _ => {}
    }

    let mut differences = Vec::new();
    let agrees = match &expected {
        Some(expected) => execution.outcome == *expected,
        None => matches!(execution.outcome, Outcome::Halt(_)),
    };
    if !agrees {
        differences.push(format!(
            "the EVM {}, the EVM IR {}",
            describe_evm(expected.as_ref(), trace.stop),
            describe(&execution.outcome)
        ));
    } else if matches!(expected, Some(Outcome::Success(_))) {
        let logs = execution.logs.iter().map(|log| (log.topics.clone(), log.data.clone()));
        differences.extend(trace.effect_differences(logs.collect(), &execution.storage, "EVM IR"));
    }
    if differences.is_empty() {
        report("checked", "");
        return Ok(());
    }
    let message = format!("`{}` disagrees with its EVM IR: {}", call.name, differences.join("; "));
    report("mismatch", &message);
    Err(message)
}

/// Reads the text of the file at `path`.
fn read(path: &Path) -> std::io::Result<String> {
    let mut text = String::new();
    File::open(path)?.read_to_string(&mut text)?;
    Ok(text)
}

/// Describes how the EVM's run ended: `expected` when it did not halt, and why it halted
/// otherwise.
fn describe_evm(expected: Option<&Outcome>, stop: InstrStop) -> String {
    match expected {
        Some(outcome) => describe(outcome),
        None => format!("halted ({stop:?})"),
    }
}

/// Describes how a run ended.
fn describe(outcome: &Outcome) -> String {
    match outcome {
        Outcome::Success(data) => format!("returned 0x{}", hex::encode(data)),
        Outcome::Revert(data) => format!("reverted with 0x{}", hex::encode(data)),
        Outcome::Halt(halt) => format!("halted ({halt})"),
        Outcome::Unsupported(reason) => format!("could not run: {reason}"),
    }
}

/// Appends one line about `call` to the log file the environment variable names, if any.
fn record(config: &TestConfig, call: &str, status: &str, detail: &str) {
    static FILE: Mutex<()> = Mutex::new(());
    let Some(path) = std::env::var_os(VARIABLE).filter(|value| value != "1") else { return };
    let _guard = FILE.lock().unwrap_or_else(|poison| poison.into_inner());
    let revision = config.status.revision();
    let test = config.status.path().display();
    let line = format!("{status}\t{test} [{revision}]\t{call}\t{}\n", detail.replace('\n', " "));
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
        let _ = file.write_all(line.as_bytes());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_follow_the_json() {
        let stdout = br#"{"contracts":{"a.sol:A":{"bin-runtime":"00"}},"version":"0.2.0"}
// === a.sol:A (scheduled runtime) ===
@module A_runtime
bb0:
  stop
"#;
        let dump = dump::parse_dump(stdout, KIND.modules).unwrap();
        assert_eq!(dump.runtimes["a.sol:A"], [0]);
        assert_eq!(dump.modules["a.sol:A"], "@module A_runtime\nbb0:\n  stop\n");
    }
}

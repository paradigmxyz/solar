//! Checks each `run-call` of a test written in lowered MIR against the MIR interpreter.
//!
//! The test runner executes a call in an EVM as usual, then compiles the test again with
//! `-Zdump=mir-final` and runs the same call on the called contract's dumped MIR through
//! `solar_codegen::interpret`, with the storage, balances, and code the EVM held just before the
//! call. Both runs execute the same compiled program, so they must end the same way, return the
//! same data, emit the same logs, and write the same storage. A test written in MIR exists to run
//! its calls both ways, so a call the interpreter cannot run, or one the EVM ends by running out
//! of gas or stack, which the interpreter does not model, fails the test.

use super::trace::{Context, Trace};
use alloy_primitives::{U256, hex};
use evm2::interpreter::InstrStop;
use solar_codegen::interpret::{self, Host, Outcome};
use solar_config::EvmVersion;
use solar_evmir::op;
use std::{
    collections::HashMap,
    ffi::OsString,
    process::Command,
    sync::{Arc, Mutex},
};
use ui_test::{build_manager::BuildManager, per_test_config::TestConfig};

/// What a compiler command dumped, or why it could not.
type Dumps = Arc<Result<Dump, String>>;

/// The final MIR modules and runtime bytecode of each contract a compiler command built.
#[derive(Default)]
struct Dump {
    modules: HashMap<String, String>,
    runtimes: HashMap<String, Vec<u8>>,
}

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
    let dumps = dumps(config, build_manager);
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

/// Returns what the test's compiler command dumps, compiling once per command.
fn dumps(config: &TestConfig, build_manager: &BuildManager) -> Dumps {
    static CACHE: Mutex<Option<HashMap<String, Dumps>>> = Mutex::new(None);
    let command = match config.build_command(build_manager) {
        Ok(command) => dump_command(&command),
        Err(error) => return Arc::new(Err(format!("cannot rebuild the command: {error:?}"))),
    };
    let key = format!("{command:?}");
    let cache = || CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    if let Some(dumps) = cache().get_or_insert_with(HashMap::new).get(&key) {
        return dumps.clone();
    }
    // Compile without the lock, so tests on other threads dump in parallel.
    let dumps = Arc::new(run_dump(command));
    cache().get_or_insert_with(HashMap::new).entry(key).or_insert(dumps).clone()
}

/// Returns `command` with its dumps replaced by a dump of the MIR the backend compiles, also
/// emitting the runtime bytecode.
fn dump_command(command: &Command) -> Command {
    let mut dump = Command::new(command.get_program());
    // `--emit X` is `--emit=X`.
    let mut original = command.get_args().peekable();
    let mut normalized = Vec::<OsString>::new();
    while let Some(arg) = original.next() {
        match original.peek() {
            Some(outputs) if arg == "--emit" => {
                normalized.push(format!("--emit={}", outputs.to_string_lossy()).into());
                original.next();
            }
            _ => normalized.push(arg.to_owned()),
        }
    }
    // The compiler rejects an output requested twice.
    let mut emits_runtime = normalized.iter().any(|arg| {
        let text = arg.to_string_lossy();
        text.strip_prefix("--emit=")
            .is_some_and(|outputs| outputs.split(',').any(|output| output == "bin-runtime"))
    });
    let mut args = normalized.iter().peekable();
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        // The JSON stays compact, one line among the dump's.
        if text.starts_with("-Zdump=") || text == "--pretty-json" {
            continue;
        }
        if text == "-Z"
            && args.peek().is_some_and(|next| next.to_string_lossy().starts_with("dump="))
        {
            args.next();
            continue;
        }
        if let Some(outputs) = text.strip_prefix("--emit=")
            && !emits_runtime
        {
            dump.arg(format!("--emit={outputs},bin-runtime"));
            emits_runtime = true;
            continue;
        }
        dump.arg(arg);
    }
    if !emits_runtime {
        dump.arg("--emit=bin-runtime");
    }
    dump.args(["-Zdump=mir-final", "--color=never"]);
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => dump.env(key, value),
            None => dump.env_remove(key),
        };
    }
    if let Some(directory) = command.get_current_dir() {
        dump.current_dir(directory);
    }
    dump
}

/// Runs a dump command and splits its output into modules and runtime bytecode by contract.
fn run_dump(mut command: Command) -> Result<Dump, String> {
    let output = command.output().map_err(|error| format!("cannot run the compiler: {error}"))?;
    if !output.status.success() {
        return Err("the MIR dump failed".into());
    }
    parse_dump(&output.stdout)
}

/// Splits what a dump command printed into modules and runtime bytecode by contract.
fn parse_dump(stdout: &[u8]) -> Result<Dump, String> {
    let mut dump = Dump::default();
    for module in interpret::parse_dump(&String::from_utf8_lossy(stdout)) {
        dump.modules.insert(module.name, module.mir);
    }
    let json = super::compiler_json(stdout)?;
    let contracts = json.get("contracts").and_then(serde_json::Value::as_object);
    for (name, contract) in contracts.into_iter().flatten() {
        let runtime = contract.get("bin-runtime").and_then(serde_json::Value::as_str);
        if let Some(runtime) = runtime.and_then(|runtime| hex::decode(runtime).ok()) {
            dump.runtimes.insert(name.clone(), runtime);
        }
    }
    Ok(dump)
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
    fn dumps_keep_compact_json() {
        let mut command = Command::new("solar");
        command.args(["--pretty-json", "--emit=abi", "-Zdump=evm-ir", "test.sol"]);
        let args = dump_command(&command)
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            ["--emit=abi,bin-runtime", "test.sol", "-Zdump=mir-final", "--color=never"]
        );
    }

    #[test]
    fn dumps_keep_a_separate_emit_value() {
        let mut command = Command::new("solar");
        command.args(["--emit", "abi,bin,bin-runtime", "test.sol"]);
        let args = dump_command(&command)
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            ["--emit=abi,bin,bin-runtime", "test.sol", "-Zdump=mir-final", "--color=never"]
        );
    }

    #[test]
    fn dumps_read_pretty_json() {
        let stdout =
            b"{\n  \"contracts\": {\n    \"a.sol:A\": {\"bin-runtime\": \"6001\"}\n  }\n}\n";
        let dump = parse_dump(stdout).unwrap();
        assert_eq!(dump.runtimes["a.sol:A"], [0x60, 0x01]);
    }

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

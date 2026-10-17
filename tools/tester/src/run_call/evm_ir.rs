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
//! With `SOLAR_RUN_CALL_EVM_IR=1`, only a disagreement is reported, as a test failure. Any other
//! value names a file that also receives one line per call: `checked`, `skipped` with the reason,
//! or `mismatch`.
//!
//! A test written in EVM IR exists to run its calls both ways, so it is checked without the
//! variable, and a call the interpreter cannot run fails it instead of being skipped.

use super::CALLER;
use alloy_primitives::{Address, B256, Log, U256, hex};
use evm2::{
    BaseEvmTypes, Evm,
    env::{BlockEnv, TxEnv},
    evm::AccountInfo,
    interpreter::InstrStop,
};
use solar_config::EvmVersion;
use solar_evmir::op;
use solar_evmir_interpreter::{self as interpret, Halt, Host, Outcome};
use std::{
    collections::HashMap,
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::Path,
    process::Command,
    sync::{Arc, Mutex},
};
use ui_test::{build_manager::BuildManager, per_test_config::TestConfig};

/// The environment variable that enables the check.
const VARIABLE: &str = "SOLAR_RUN_CALL_EVM_IR";

/// The program the dump names for the runtime EVM IR of a contract before the EVM IR pipeline.
const SCHEDULED_RUNTIME: &str = "scheduled runtime";

/// What a compiler command dumped, or why it could not.
type Dumps = Arc<Result<Dump, String>>;

/// The scheduled runtime EVM IR and the runtime bytecode of each contract a compiler command
/// built.
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

/// An account as the called contract sees it.
struct Account {
    balance: U256,
    code: Vec<u8>,
    code_hash: B256,
}

/// The chain state the called contract saw, captured from the EVM just before the call.
pub(super) struct Chain {
    contract: Address,
    storage: HashMap<U256, U256>,
    accounts: HashMap<Address, Account>,
}

impl Chain {
    /// Captures the state of `evm`, whose next transaction calls `contract`.
    ///
    /// The accepted-state overlay holds every account the deployment and `setUp()` touched,
    /// which includes every account the call can observe.
    pub(super) fn capture(evm: &Evm<'_, BaseEvmTypes>, contract: Address) -> Self {
        let cache = &evm.overlay_db().cache;
        let accounts = cache
            .accounts
            .iter()
            .filter_map(|(address, info)| Some((*address, info.as_ref()?)))
            .map(|(address, info): (Address, &AccountInfo)| {
                let code = info.code.as_ref().or_else(|| cache.contracts.get(&info.code_hash));
                let code = code.map(|code| code.original_byte_slice().to_vec()).unwrap_or_default();
                (address, Account { balance: info.balance, code, code_hash: info.code_hash })
            })
            .collect();
        Self { contract, storage: storage(evm, contract), accounts }
    }
}

/// Returns the persistent storage of `contract` in `evm`.
fn storage(evm: &Evm<'_, BaseEvmTypes>, contract: Address) -> HashMap<U256, U256> {
    let storage = evm.overlay_db().cache.storage.get(&contract);
    storage.map_or_else(HashMap::new, |storage| {
        storage.slots.iter().map(|(&slot, &value)| (slot, value)).collect()
    })
}

/// What a call did in the EVM.
pub(super) struct Trace {
    before: Chain,
    stop: InstrStop,
    output: Vec<u8>,
    logs: Vec<Log>,
    storage: HashMap<U256, U256>,
}

impl Trace {
    /// Records the call that took `evm` from `before` to its current state.
    pub(super) fn new(
        before: Chain,
        evm: &Evm<'_, BaseEvmTypes>,
        stop: InstrStop,
        output: &[u8],
        logs: &[Log],
    ) -> Self {
        let storage = storage(evm, before.contract);
        Self { before, stop, output: output.to_vec(), logs: logs.to_vec(), storage }
    }
}

/// Answers the interpreter's context reads from the captured chain state and the values the EVM
/// was configured with.
struct EvmHost<'a> {
    chain: &'a Chain,
    value: U256,
    block: BlockEnv<BaseEvmTypes>,
}

impl EvmHost<'_> {
    /// Returns an account's balance during the call, after its value moved to the contract.
    fn balance(&self, address: Address) -> U256 {
        let before =
            self.chain.accounts.get(&address).map_or(U256::ZERO, |account| account.balance);
        if address == self.chain.contract {
            before + self.value
        } else if address == CALLER {
            before - self.value
        } else {
            before
        }
    }
}

impl Host for EvmHost<'_> {
    fn read(&mut self, opcode: u8, operands: &[U256]) -> Option<U256> {
        let address = || operands.first().map(|word| Address::from_word(word.to_be_bytes().into()));
        let account = |address: Address| self.chain.accounts.get(&address);
        Some(match opcode {
            op::ADDRESS => self.chain.contract.into_word().into(),
            op::ORIGIN | op::CALLER => CALLER.into_word().into(),
            op::CALLVALUE => self.value,
            // The call is a legacy transaction with a zero gas price and no blobs, in the first
            // block, which has no ancestors to hash.
            op::GASPRICE | op::BLOCKHASH | op::BLOBHASH => U256::ZERO,
            op::CHAINID => TxEnv::<BaseEvmTypes>::default().chain_id,
            op::COINBASE => self.block.beneficiary.into_word().into(),
            op::TIMESTAMP => self.block.timestamp,
            op::NUMBER => self.block.number,
            op::PREVRANDAO => self.block.prevrandao,
            op::GASLIMIT => self.block.gas_limit,
            op::BASEFEE => self.block.basefee,
            op::BLOBBASEFEE => self.block.blob_basefee,
            op::SLOTNUM => self.block.slot_num,
            op::SELFBALANCE => self.balance(self.chain.contract),
            op::BALANCE => self.balance(address()?),
            op::EXTCODESIZE => {
                U256::from(account(address()?).map_or(0, |account| account.code.len()))
            }
            op::EXTCODEHASH => {
                account(address()?).map_or(U256::ZERO, |account| account.code_hash.into())
            }
            _ => return None,
        })
    }

    fn storage(&mut self, slot: U256) -> U256 {
        self.chain.storage.get(&slot).copied().unwrap_or_default()
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
    let expected = match trace.stop {
        stop if stop.is_success() => Some(Outcome::Success(trace.output.clone())),
        InstrStop::Revert => Some(Outcome::Revert(trace.output.clone())),
        InstrStop::OutOfGas
        | InstrStop::MemoryOOG
        | InstrStop::MemoryLimitOOG
        | InstrStop::PrecompileOOG
        | InstrStop::InvalidOperandOOG
        | InstrStop::StackOverflow
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
        let dumps = dumps(config, build_manager);
        let dump = match dumps.as_ref() {
            Ok(dump) => dump,
            Err(error) => return skip(error),
        };
        let Some(module) = dump.modules.get(call.contract) else {
            return skip("the EVM IR dump has no scheduled runtime for the contract");
        };
        // Every contract a call deploys has a runtime: a dump without it would skip every call.
        let Some(runtime) = dump.runtimes.get(call.contract) else {
            return Err(format!("the EVM IR dump has no runtime for `{}`", call.contract));
        };
        // A constructor may deploy other code, and deployment patches immutables into the
        // runtime.
        let deployed =
            trace.before.accounts.get(&trace.before.contract).map(|account| &account.code);
        if deployed != Some(runtime) {
            return skip("the deployed code is not the compiled runtime");
        }
        module.clone()
    };
    let block = BlockEnv::<BaseEvmTypes>::default();
    let mut host = EvmHost { chain: &trace.before, value: call.value, block };
    let options = interpret::Options { evm_version: call.evm_version, ..Default::default() };
    let execution = match interpret::transact(&program, call.input, &mut host, options) {
        Ok(execution) => execution,
        Err(reason) => return skip(&reason),
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
        let logs = trace
            .logs
            .iter()
            .map(|log| {
                let topics = log.data.topics().iter().map(|topic| U256::from_be_bytes(topic.0));
                (topics.collect::<Vec<_>>(), log.data.data.to_vec())
            })
            .collect::<Vec<_>>();
        let interpreted_logs = execution
            .logs
            .iter()
            .map(|log| (log.topics.clone(), log.data.clone()))
            .collect::<Vec<_>>();
        if logs != interpreted_logs {
            differences.push(format!(
                "the EVM logged {}, the EVM IR {}",
                describe_logs(&logs),
                describe_logs(&interpreted_logs)
            ));
        }
        let written = execution.storage.iter().copied().collect::<HashMap<_, _>>();
        for (&slot, &value) in &written {
            let evm = trace.storage.get(&slot).copied().unwrap_or_default();
            if evm != value {
                differences.push(format!(
                    "slot {slot:#x} holds {evm:#x} in the EVM, {value:#x} in the EVM IR"
                ));
            }
        }
        for (&slot, &value) in &trace.storage {
            let before = trace.before.storage.get(&slot).copied().unwrap_or_default();
            if value != before && !written.contains_key(&slot) {
                differences.push(format!(
                    "the EVM wrote {value:#x} to slot {slot:#x}, the EVM IR did not"
                ));
            }
        }
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

/// Describes events as their topics and data, in hex.
fn describe_logs(logs: &[(Vec<U256>, Vec<u8>)]) -> String {
    let logs = logs.iter().map(|(topics, data)| {
        let topics = topics.iter().map(|topic| format!("{topic:#x}")).collect::<Vec<_>>();
        format!("topics [{}] data 0x{}", topics.join(", "), hex::encode(data))
    });
    format!("[{}]", logs.collect::<Vec<_>>().join("; "))
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

/// Returns what the test's compiler command dumps, compiling once per command.
fn dumps(config: &TestConfig, build_manager: &BuildManager) -> Dumps {
    static CACHE: Mutex<Option<HashMap<String, Dumps>>> = Mutex::new(None);
    let command = match config.build_command(build_manager) {
        Ok(command) => dump_command(&command),
        Err(error) => return Arc::new(Err(format!("cannot rebuild the command: {error:?}"))),
    };
    let key = format!("{command:?}");
    let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
    let cache = cache.get_or_insert_with(HashMap::new);
    if let Some(dumps) = cache.get(&key) {
        return dumps.clone();
    }
    let dumps = Arc::new(run_dump(command));
    cache.insert(key, dumps.clone());
    dumps
}

/// Returns `command` with its dumps replaced by a dump of the scheduled runtime EVM IR, also
/// emitting the runtime bytecode.
fn dump_command(command: &Command) -> Command {
    let mut dump = Command::new(command.get_program());
    // The compiler rejects an output requested twice.
    let mut emits_runtime = command.get_args().any(|arg| {
        let text = arg.to_string_lossy();
        text.strip_prefix("--emit=")
            .is_some_and(|outputs| outputs.split(',').any(|output| output == "bin-runtime"))
    });
    let mut args = command.get_args().peekable();
    while let Some(arg) = args.next() {
        let text = arg.to_string_lossy();
        // The JSON stays compact, one line before the dumps.
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
    dump.args(["-Zdump=evm-ir-runtime-scheduled", "--color=never"]);
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
        return Err("the EVM IR dump failed".into());
    }
    parse_dump(&output.stdout)
}

/// Splits what a dump command printed into modules and runtime bytecode by contract.
fn parse_dump(stdout: &[u8]) -> Result<Dump, String> {
    let mut dump = Dump::default();
    for module in interpret::parse_dump(&String::from_utf8_lossy(stdout)) {
        if module.program.as_deref() == Some(SCHEDULED_RUNTIME) {
            dump.modules.insert(module.name, module.evm_ir);
        }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dumps_keep_compact_json() {
        let mut command = Command::new("solar");
        command.args(["--pretty-json", "--emit=abi", "-Zdump=mir", "test.sol"]);
        let args = dump_command(&command)
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect::<Vec<_>>();
        assert_eq!(
            args,
            [
                "--emit=abi,bin-runtime",
                "test.sol",
                "-Zdump=evm-ir-runtime-scheduled",
                "--color=never"
            ]
        );
    }

    #[test]
    fn dumps_follow_the_json() {
        let stdout = br#"{"contracts":{"a.sol:A":{"bin-runtime":"00"}},"version":"0.2.0"}
// === a.sol:A (scheduled runtime) ===
@module A_runtime
bb0:
  stop
"#;
        let dump = parse_dump(stdout).unwrap();
        assert_eq!(dump.runtimes["a.sol:A"], [0]);
        assert_eq!(dump.modules["a.sol:A"], "@module A_runtime\nbb0:\n  stop\n");
    }
}

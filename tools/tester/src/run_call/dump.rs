//! Compiles a test again to dump the program an interpreter check runs, together with the runtime
//! bytecode of each contract.

use alloy_primitives::hex;
use std::{
    collections::HashMap,
    ffi::OsString,
    process::Command,
    sync::{Arc, Mutex, OnceLock},
};
use ui_test::{build_manager::BuildManager, per_test_config::TestConfig};

/// What a compiler command dumped, or why it could not.
pub(super) type Dumps = Arc<Result<Dump, String>>;

/// The dumped program and the runtime bytecode of each contract a compiler command built.
#[derive(Default)]
pub(super) struct Dump {
    pub(super) modules: HashMap<String, String>,
    pub(super) runtimes: HashMap<String, Vec<u8>>,
}

/// What to dump and how to read it.
pub(super) struct Kind {
    /// The `-Zdump` value.
    pub(super) dump: &'static str,
    /// What the dump is called in errors.
    pub(super) name: &'static str,
    /// Picks the program of each contract out of the dump's text.
    pub(super) modules: fn(&str) -> HashMap<String, String>,
}

/// Returns what the test's compiler command dumps for `kind`, compiling once per command.
pub(super) fn dumps(config: &TestConfig, build_manager: &BuildManager, kind: &Kind) -> Dumps {
    static CACHE: Mutex<Option<HashMap<String, Arc<OnceLock<Dumps>>>>> = Mutex::new(None);
    let command = match config.build_command(build_manager) {
        Ok(command) => dump_command(&command, kind.dump),
        Err(error) => return Arc::new(Err(format!("cannot rebuild the command: {error:?}"))),
    };
    let key = format!("{command:?}");
    // The lock covers the lookup only: the compiles of different commands run in parallel.
    let once = {
        let mut cache = CACHE.lock().unwrap_or_else(|poison| poison.into_inner());
        cache.get_or_insert_with(HashMap::new).entry(key).or_default().clone()
    };
    once.get_or_init(|| Arc::new(run_dump(command, kind))).clone()
}

/// Returns `command` with its dumps replaced by `-Zdump=<dump>`, also emitting the runtime
/// bytecode.
fn dump_command(command: &Command, dump: &str) -> Command {
    let mut out = Command::new(command.get_program());
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
            out.arg(format!("--emit={outputs},bin-runtime"));
            emits_runtime = true;
            continue;
        }
        out.arg(arg);
    }
    if !emits_runtime {
        out.arg("--emit=bin-runtime");
    }
    out.args([format!("-Zdump={dump}").as_str(), "--color=never"]);
    for (key, value) in command.get_envs() {
        match value {
            Some(value) => out.env(key, value),
            None => out.env_remove(key),
        };
    }
    if let Some(directory) = command.get_current_dir() {
        out.current_dir(directory);
    }
    out
}

/// Runs a dump command and splits its output into programs and runtime bytecode by contract.
fn run_dump(mut command: Command, kind: &Kind) -> Result<Dump, String> {
    let output = command.output().map_err(|error| format!("cannot run the compiler: {error}"))?;
    if !output.status.success() {
        return Err(format!("the {} dump failed", kind.name));
    }
    parse_dump(&output.stdout, kind.modules)
}

/// Splits what a dump command printed into programs and runtime bytecode by contract.
pub(super) fn parse_dump(
    stdout: &[u8],
    modules: fn(&str) -> HashMap<String, String>,
) -> Result<Dump, String> {
    let mut dump = Dump { modules: modules(&String::from_utf8_lossy(stdout)), ..Dump::default() };
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

    fn args(command: &[&str]) -> Vec<String> {
        let mut original = Command::new("solar");
        original.args(command);
        dump_command(&original, "mir-final")
            .get_args()
            .map(|arg| arg.to_string_lossy().into_owned())
            .collect()
    }

    #[test]
    fn dumps_keep_compact_json() {
        assert_eq!(
            args(&["--pretty-json", "--emit=abi", "-Zdump=evm-ir", "test.sol"]),
            ["--emit=abi,bin-runtime", "test.sol", "-Zdump=mir-final", "--color=never"]
        );
    }

    #[test]
    fn dumps_keep_a_separate_emit_value() {
        assert_eq!(
            args(&["--emit", "abi,bin,bin-runtime", "test.sol"]),
            ["--emit=abi,bin,bin-runtime", "test.sol", "-Zdump=mir-final", "--color=never"]
        );
    }

    #[test]
    fn dumps_read_pretty_json() {
        let stdout =
            b"{\n  \"contracts\": {\n    \"a.sol:A\": {\"bin-runtime\": \"6001\"}\n  }\n}\n";
        let dump = parse_dump(stdout, |_| HashMap::new()).unwrap();
        assert_eq!(dump.runtimes["a.sol:A"], [0x60, 0x01]);
    }
}

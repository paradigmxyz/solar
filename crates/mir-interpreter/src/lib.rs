#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

use alloy_primitives::U256;
use machine::{Effects, Limit, Limits, Machine, Outcome as RunOutcome};
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use solar_interface::{ColorChoice, Session, source_map::FileName};
use solar_mir::{MirPhase, Module};

mod machine;
pub use machine::{Host, Log};

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It returned this data with `RETURN`, or stopped with none.
    Success(Vec<u8>),
    /// It reverted with this data.
    Revert(Vec<u8>),
    /// It reached `INVALID` or an opcode the EVM version lacks, such as `REVERT` before
    /// Byzantium.
    Halt,
    /// A returndata copy exceeded the available bytes.
    ReturndataOutOfBounds,
    /// The interpreter cannot run it, for this reason.
    Unsupported(String),
}

/// What a run did.
#[derive(Clone, Debug)]
pub struct Execution {
    /// How it ended.
    pub outcome: Outcome,
    /// The events it logged, in order.
    pub logs: Vec<Log>,
    /// The persistent storage slots it wrote, with their final values, ordered by slot.
    pub storage: Vec<(U256, U256)>,
    /// The transient storage slots it wrote, with their final values, ordered by slot.
    pub transient: Vec<(U256, U256)>,
}

/// How a run executes.
pub struct Options {
    /// The EVM version the module targets, which decides which opcodes a transaction can run:
    /// one the version lacks, such as `REVERT` before Byzantium, halts.
    pub evm_version: EvmVersion,
    /// Units of work before the run stops: one per operation and per copied, hashed, or returned
    /// word.
    pub fuel: u64,
    /// Calls that may be live at once, the first included.
    pub depth: usize,
}

impl Default for Options {
    fn default() -> Self {
        Self { evm_version: EvmVersion::default(), fuel: 10_000_000, depth: 1024 }
    }
}

/// One module of a `-Zdump=mir-final` dump.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DumpedModule {
    /// The contract the module compiles, as the dump's header names it, or the module's name.
    pub name: String,
    /// The module's text.
    pub mir: String,
}

/// Runs a transaction with `calldata` on the contract whose lowered MIR module `mir` holds, asking
/// `host` for its context. Fails when `mir` is not a valid lowered module.
///
/// Final dumps carry the backend's resolved allocation and multi-return buffer addresses.
/// Layout-dependent operations without those records are unsupported; guessing another address
/// cannot reliably detect pointer comparisons or aliasing.
pub fn transact(
    mir: &str,
    calldata: &[u8],
    host: &mut dyn Host,
    options: Options,
) -> Result<Execution, String> {
    with_module(mir, |module| {
        let limits = Limits { fuel: options.fuel, depth: options.depth };
        let machine = Machine::new(module);
        let execution = machine.transact(calldata, host, options.evm_version, limits);
        Ok(execution_from(execution.outcome, execution.effects))
    })
}

/// Splits the output of `solar -Zdump=mir-final` into its modules, each after a
/// `// === NAME ===` header. Text without headers is one module, named by its `@module` line. The
/// compiler's JSON output, which follows the dumps, ends the last module.
pub fn parse_dump(text: &str) -> Vec<DumpedModule> {
    let headed = text.lines().any(|line| header(line).is_some());
    let mut modules = Vec::new();
    let mut current = (!headed).then(DumpedModule::default);
    for line in text.lines() {
        if let Some(name) = header(line) {
            modules.extend(current.take());
            current = Some(DumpedModule { name: name.to_owned(), ..Default::default() });
            continue;
        }
        if line.starts_with('{') {
            break;
        }
        let Some(module) = &mut current else { continue };
        if module.name.is_empty()
            && let Some(name) = line.strip_prefix("@module ")
        {
            module.name = name.trim().to_owned();
        }
        module.mir.push_str(line);
        module.mir.push('\n');
    }
    modules.extend(current);
    modules.retain(|module| !module.mir.trim().is_empty());
    modules
}

/// Returns the contract a dump's module header names.
fn header(line: &str) -> Option<&str> {
    line.strip_prefix("// === ").and_then(|rest| rest.strip_suffix(" ==="))
}

/// Parses `mir` as one lowered module and runs `f` on it within a session of its own.
fn with_module(
    mir: &str,
    f: impl FnOnce(&Module) -> Result<Execution, String>,
) -> Result<Execution, String> {
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let name = FileName::Custom("interpreted.mir".into());
        let file = sess
            .source_map()
            .new_source_file(name, mir)
            .map_err(|error| format!("unreadable MIR: {error}"))?;
        let Ok(module) = Module::parse(&sess, &file) else {
            let diagnostics = sess.dcx.emitted_diagnostics().map(|d| d.to_string());
            return Err(format!("unparsable MIR: {}", diagnostics.unwrap_or_default()));
        };
        if module.phase() != MirPhase::Lowered {
            return Err(
                "the MIR was dumped before lowering, as `-O none -Zdump=mir` dumps it".into()
            );
        }
        // Run only what the backend would compile.
        if module.as_lowered(&sess, true).is_err() {
            let diagnostics = sess.dcx.emitted_diagnostics().map(|d| d.to_string());
            return Err(format!("invalid lowered MIR: {}", diagnostics.unwrap_or_default()));
        }
        f(&module)
    })
}

/// Converts what a run did into what the interpreter reports.
fn execution_from(outcome: RunOutcome, effects: Effects) -> Execution {
    let outcome = match outcome {
        RunOutcome::Return(_) => unreachable!("a transaction ends without returning words"),
        RunOutcome::ReturnData(data) => Outcome::Success(data),
        RunOutcome::Stop => Outcome::Success(Vec::new()),
        RunOutcome::Revert(data) => Outcome::Revert(data),
        RunOutcome::Invalid => Outcome::Halt,
        RunOutcome::ReturndataOutOfBounds => Outcome::ReturndataOutOfBounds,
        RunOutcome::Limit(Limit::Fuel) => Outcome::Unsupported("runs too long".into()),
        RunOutcome::Limit(Limit::Depth) => Outcome::Unsupported("nests calls too deeply".into()),
        RunOutcome::Limit(Limit::Memory) => {
            Outcome::Unsupported("reaches past 16 MiB of memory".into())
        }
        RunOutcome::Unsupported(what) => Outcome::Unsupported(format!("reaches `{what}`")),
    };
    let sorted = |slots: FxHashMap<U256, U256>| {
        let mut slots = slots.into_iter().collect::<Vec<_>>();
        slots.sort_unstable();
        slots
    };
    Execution {
        outcome,
        logs: effects.logs,
        storage: sorted(effects.storage),
        transient: sorted(effects.transient),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODULE: &str = "@module Interpret
@phase lowered
fn @double(arg0: i256) -> i256 {
  bb0:
    v0 = add arg0, arg0
    ret v0
}

fn @twice(arg0: i256) -> i256 {
  bb0:
    v0 = icall @double, arg0
    v1 = icall @double, v0
    sstore 1, v1
    ret v1
}
";

    /// Answers every read with zero and places the heap at `0x80`.
    struct Zero;

    impl Host for Zero {
        fn read(&mut self, _: u8, _: &[U256]) -> Option<U256> {
            Some(U256::ZERO)
        }

        fn storage(&mut self, _: U256) -> U256 {
            U256::ZERO
        }
    }

    #[test]
    fn transactions() {
        let transact = |body: &str| {
            let module = format!("@module Tx\n@phase lowered\nfn @entry() [entry] {{\n{body}}}\n");
            transact(&module, &[], &mut Zero, Options::default()).unwrap()
        };
        // A revert discards the transaction's writes and logs.
        let execution = transact(
            "  bb0:
    sstore 1, 2
    tstore 3, 4
    log 0, 0
    revert 0, 0
",
        );
        assert_eq!(execution.outcome, Outcome::Revert(Vec::new()));
        assert_eq!((execution.storage, execution.transient, execution.logs), Default::default());

        // An allocation without backend placement cannot be executed faithfully.
        let alloc = "  bb0:
    v0 = alloc 32 !metadata(deferred_alloc)
    v1 = ptrtoint ptr<memory> v0 to i256
    mstore v1, 7
";
        let execution = transact(&format!(
            "{alloc}    v2 = mload v1
    mstore 0, v2
    returndata 0, 32
"
        ));
        assert_eq!(
            execution.outcome,
            Outcome::Unsupported("reaches `allocation without backend placement`".into())
        );
        let execution = transact(&format!(
            "{alloc}    mstore 0, v1
    returndata 0, 32
"
        ));
        let unsupported = "reaches `allocation without backend placement`".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(unsupported));
    }

    #[test]
    fn dumps() {
        let dump = "// === a.sol:A ===
@module A
@phase lowered
// === a.sol:B ===
@module B
{\"contracts\":{}}
";
        let modules = parse_dump(dump);
        assert_eq!(
            modules.iter().map(|module| &module.name[..]).collect::<Vec<_>>(),
            ["a.sol:A", "a.sol:B"]
        );
        assert_eq!(modules[1].mir, "@module B\n");
        // A file without headers is one module, named by its `@module` line.
        let modules = parse_dump(MODULE);
        assert_eq!(modules.len(), 1);
        assert_eq!(modules[0].name, "Interpret");
        assert_eq!(modules[0].mir, MODULE);
    }

    #[test]
    fn memory_size_below_backend_frames() {
        let run = |records: &str, body: &str| {
            let module = format!(
                "@module Tx\n@phase lowered\n{records}fn @entry() [entry] {{\n  bb0:\n{body}    v0 = msize
    mstore 0, v0
    returndata 0, 32
}}\n"
            );
            transact(&module, &[], &mut Zero, Options::default()).unwrap().outcome
        };
        let unsupported = |reason: &str| Outcome::Unsupported(format!("reaches `{reason}`"));
        assert_eq!(run("", ""), unsupported("msize without backend layout"));
        // Spills below the frame end may have grown EVM memory past what MIR shows.
        let frames = "// backend-memory-v1\n// backend-frames 0 0x100\n";
        assert_eq!(run(frames, ""), unsupported("msize below backend frames"));
        assert_eq!(
            run(frames, "    mstore 0xe0, 1\n"),
            Outcome::Success(U256::from(0x100).to_be_bytes_vec())
        );
    }
}

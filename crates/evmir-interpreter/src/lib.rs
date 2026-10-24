#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]
#![cfg_attr(test, allow(unused_crate_dependencies))]

use alloy_primitives::U256;
use machine::{Effects, HOST_OPCODES, Layout, Limit, Machine};
use solar_config::EvmVersion;
use solar_data_structures::map::FxHashMap;
use solar_evmir::ir::Module;
use solar_interface::{ColorChoice, Session, source_map::FileName};

mod machine;
pub use machine::{Halt, Host, Log};

/// The opcodes whose values a run asks its [`Host`] for, such as `CALLER` and `TIMESTAMP`.
pub const HOST_READS: &[u8] = &HOST_OPCODES;

/// How a run ended.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// It returned this data with `RETURN`, or stopped with none.
    Success(Vec<u8>),
    /// It reverted with this data.
    Revert(Vec<u8>),
    /// It halted exceptionally, for this reason.
    Halt(Halt),
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
pub struct Options<'a> {
    /// The EVM version the program targets, which decides the opcodes it defines.
    pub evm_version: EvmVersion,
    /// Units of work before the run stops: one per operation and per copied, hashed, logged, or
    /// returned word.
    pub fuel: u64,
    /// Receives a line for every operation the run executes, with the words it read and the word
    /// it pushed.
    pub trace: Option<&'a mut dyn FnMut(&str)>,
}

impl Default for Options<'_> {
    fn default() -> Self {
        Self { evm_version: EvmVersion::default(), fuel: 10_000_000, trace: None }
    }
}

/// One module of an EVM IR dump.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct DumpedModule {
    /// The contract the module belongs to, as the dump's header names it, or the module's name.
    pub name: String,
    /// The program the header names in parentheses, such as `runtime` or `scheduled runtime`.
    pub program: Option<String>,
    /// The module's text.
    pub evm_ir: String,
}

/// Runs a transaction with `calldata` on the program of the EVM IR module `evm_ir`, asking `host`
/// for its context. Fails when `evm_ir` is not an EVM IR module.
///
/// The assembler decides the addresses of blocks and data, so the interpreter picks its own. The
/// transaction runs again with those moved, and a result that changes with them is unsupported,
/// since the compiled code would see other addresses.
///
/// `CODESIZE` is unknown before assembly. Its value may be discarded or used unmodified as the
/// source offset of `CODECOPY`, which reads zeros there. Other uses are unsupported even when
/// the two layouts agree: agreement cannot rule out equality with the assembled size.
pub fn transact(
    evm_ir: &str,
    calldata: &[u8],
    host: &mut dyn Host,
    options: Options<'_>,
) -> Result<Execution, String> {
    with_module(evm_ir, |module| {
        let machine = Machine::new(module, options.evm_version);
        let execution =
            machine.transact(calldata, host, Layout::COMPACT, options.fuel, options.trace);
        let moved = machine.transact(calldata, host, Layout::MOVED, options.fuel, None);
        if moved != execution {
            let reason = "a result that depends on the code layout".into();
            return execution_from(machine::Outcome::Unsupported(reason), Effects::default());
        }
        execution_from(execution.outcome, execution.effects)
    })
}

/// Splits an EVM IR dump of the compiler into its modules, each after a `// === NAME ===` or
/// `// === NAME (PROGRAM) ===` header. Text without headers is one module, named by its `@module`
/// line. The compiler's JSON output, which the compiler prints before its EVM IR dumps, is no
/// module's text.
pub fn parse_dump(text: &str) -> Vec<DumpedModule> {
    let headed = text.lines().any(|line| header(line).is_some());
    let mut modules = Vec::new();
    let mut current = (!headed).then(DumpedModule::default);
    for line in text.lines() {
        if let Some((name, program)) = header(line) {
            modules.extend(current.take());
            let (name, program) = (name.to_owned(), program.map(str::to_owned));
            current = Some(DumpedModule { name, program, evm_ir: String::new() });
            continue;
        }
        if line.starts_with('{') {
            modules.extend(current.take());
            continue;
        }
        let Some(module) = &mut current else { continue };
        if module.name.is_empty()
            && let Some(name) = line.strip_prefix("@module ")
        {
            module.name = name.trim().to_owned();
        }
        module.evm_ir.push_str(line);
        module.evm_ir.push('\n');
    }
    modules.extend(current);
    modules.retain(|module| !module.evm_ir.trim().is_empty());
    modules
}

/// Returns the contract a dump's module header names, and the program it names in parentheses.
fn header(line: &str) -> Option<(&str, Option<&str>)> {
    let name = line.strip_prefix("// === ")?.strip_suffix(" ===")?;
    Some(match name.strip_suffix(')').and_then(|name| name.rsplit_once(" (")) {
        Some((name, program)) => (name, Some(program)),
        None => (name, None),
    })
}

/// Parses `evm_ir` as one EVM IR module and runs `f` on it within a session of its own.
fn with_module(
    evm_ir: &str,
    f: impl FnOnce(&Module) -> Result<Execution, String>,
) -> Result<Execution, String> {
    let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
    sess.enter_sequential(|| {
        let name = FileName::Custom("interpreted.evmir".into());
        let file = sess
            .source_map()
            .new_source_file(name, evm_ir)
            .map_err(|error| format!("unreadable EVM IR: {error}"))?;
        let Ok(module) = Module::parse(&sess, &file) else {
            let diagnostics = sess.dcx.emitted_diagnostics().map(|d| d.to_string());
            return Err(format!("unparsable EVM IR: {}", diagnostics.unwrap_or_default()));
        };
        f(&module)
    })
}

/// Converts what a run did into what the interpreter reports.
fn execution_from(outcome: machine::Outcome, effects: Effects) -> Result<Execution, String> {
    let outcome = match outcome {
        machine::Outcome::Return(data) => Outcome::Success(data),
        machine::Outcome::Stop => Outcome::Success(Vec::new()),
        machine::Outcome::Revert(data) => Outcome::Revert(data),
        machine::Outcome::Halt(halt) => Outcome::Halt(halt),
        machine::Outcome::Limit(Limit::Fuel) => Outcome::Unsupported("runs too long".into()),
        machine::Outcome::Limit(Limit::Memory) => {
            Outcome::Unsupported("reaches past 16 MiB of memory".into())
        }
        machine::Outcome::Unsupported(what) => Outcome::Unsupported(what.into_owned()),
    };
    let sorted = |slots: FxHashMap<U256, U256>| {
        let mut slots = slots.into_iter().collect::<Vec<_>>();
        slots.sort_unstable();
        slots
    };
    Ok(Execution {
        outcome,
        logs: effects.logs,
        storage: sorted(effects.storage),
        transient: sorted(effects.transient),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Answers every read with zero.
    struct Zero;

    impl Host for Zero {
        fn read(&mut self, _: u8, _: &[U256]) -> Option<U256> {
            Some(U256::ZERO)
        }

        fn storage(&mut self, _: U256) -> U256 {
            U256::ZERO
        }
    }

    /// Runs the blocks `body` of a module on `calldata`.
    fn run(body: &str, calldata: &[u8]) -> Execution {
        let module = format!("@module test\n{body}");
        transact(&module, calldata, &mut Zero, Options::default()).unwrap()
    }

    fn word(value: u64) -> Vec<u8> {
        U256::from(value).to_be_bytes::<32>().to_vec()
    }

    #[test]
    fn traces() {
        let module = "@module test
bb0:
  push 2
  push 3
  add
  dup 1
  push 0
  mstore
  push bb1
  jump
bb1:
  push 32
  push 0
  return
";
        let mut lines = Vec::new();
        let mut trace = |line: &str| lines.push(line.to_owned());
        let options = Options { trace: Some(&mut trace), ..Options::default() };
        let execution = transact(module, &[], &mut Zero, options).unwrap();
        assert_eq!(execution.outcome, Outcome::Success(word(5)));
        snapbox::assert_data_eq!(
            lines.join("\n"),
            snapbox::str![[r#"
bb0: push 2 -> 0x2
bb0: push 3 -> 0x3
bb0: add  [0x3, 0x2] -> 0x5
bb0: dup 1 -> 0x5
bb0: push 0 -> 0x0
bb0: mstore  [0x0, 0x5]
bb0: push bb1 -> 0x1
bb0: jump  [0x1]
bb1: push 32 -> 0x20
bb1: push 0 -> 0x0
bb1: return  [0x0, 0x20]
"#]]
        );
    }

    #[test]
    fn effects_of_reverts_and_halts_are_discarded() {
        let execution = run(
            "bb0:
  push 2
  push 1
  sstore
  push 4
  push 3
  tstore
  push 0
  push 0
  log0
  push 0
  push 0
  revert
",
            &[],
        );
        assert_eq!(execution.outcome, Outcome::Revert(Vec::new()));
        assert!(execution.storage.is_empty() && execution.transient.is_empty());
        assert!(execution.logs.is_empty());

        let execution = run("bb0:\n  push 1\n  push 1\n  sstore\n  invalid\n", &[]);
        assert_eq!(execution.outcome, Outcome::Halt(Halt::Invalid));
        assert!(execution.storage.is_empty());

        let execution = run("bb0:\n  push 1\n  push 1\n  sstore\n", &[]);
        assert_eq!(execution.outcome, Outcome::Success(Vec::new()));
        assert_eq!(execution.storage, [(U256::ONE, U256::ONE)]);
    }

    #[test]
    fn stack_limits_halt() {
        let execution = run("bb0:\n  add\n  stop\n", &[]);
        assert_eq!(execution.outcome, Outcome::Halt(Halt::StackUnderflow));
        let execution = run("bb0:\n  push 1\n  jump bb0\n", &[]);
        assert_eq!(execution.outcome, Outcome::Halt(Halt::StackOverflow));
    }

    #[test]
    fn layout_dependent_results_are_unsupported() {
        // The address of a block reaches the returned data.
        let execution = run(
            "bb0:
  push bb1
  push 0
  mstore
  push 32
  push 0
  return
bb1:
  stop
",
            &[],
        );
        let reason = "a result that depends on the code layout".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(reason));

        // Comparing two addresses depends only on their order, which every layout keeps.
        let execution = run(
            "bb0:
  push bb2
  push bb1
  lt
  push 0
  mstore
  push 32
  push 0
  return
bb1:
  stop
bb2:
  stop
",
            &[],
        );
        assert_eq!(execution.outcome, Outcome::Success(word(1)));
    }

    #[test]
    fn data_and_code() {
        let module = |copy: &str| {
            format!(
                "@data
  0: hex\"1122\"
  1: hex\"3344\"
bb0:
{copy}  push 32
  push 0
  return
"
            )
        };
        // Data reads see its bytes.
        let execution = run(&module("  push 2\n  push_data 1\n  push 0\n  codecopy\n"), &[]);
        let mut expected = vec![0; 32];
        expected[..2].copy_from_slice(&[0x33, 0x44]);
        assert_eq!(execution.outcome, Outcome::Success(expected));

        // Reads past the end of the code are zero.
        let execution = run(&module("  push 32\n  codesize\n  push 0\n  codecopy\n"), &[]);
        assert_eq!(execution.outcome, Outcome::Success(vec![0; 32]));

        // Reading past a data entry reads whatever the layout puts after it.
        let execution = run(&module("  push 4\n  push_data 0\n  push 0\n  codecopy\n"), &[]);
        let reason = "a result that depends on the code layout".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(reason));

        // Code bytes are what assembly makes them.
        let execution = run(&module("  push 1\n  push 0\n  push 0\n  codecopy\n"), &[]);
        let reason = "copies code bytes, which only assembly decides".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(reason));
    }

    #[test]
    fn library_addresses_in_data_are_unknown() {
        let module = |len: u32| {
            format!(
                "@module test
@libraries
  L_0: \"a.sol:L\"
@data
  0: hex\"110000000000000000000000000000000000000000\" library_relocations [1: L_0]
bb0:
  push {len}
  push_data 0
  push 0
  codecopy
  push 32
  push 0
  return
"
            )
        };
        // The byte before the address is final.
        let execution = transact(&module(1), &[], &mut Zero, Options::default()).unwrap();
        let mut expected = vec![0; 32];
        expected[0] = 0x11;
        assert_eq!(execution.outcome, Outcome::Success(expected));
        // Linking writes the address.
        let execution = transact(&module(2), &[], &mut Zero, Options::default()).unwrap();
        let reason = "copies a library address, which linking sets".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(reason));
    }

    #[test]
    fn unsupported_operations() {
        let unsupported = |body: &str| match run(body, &[]).outcome {
            Outcome::Unsupported(reason) => reason,
            outcome => panic!("expected an unsupported run, not {outcome:?}"),
        };
        assert_eq!(unsupported("bb0:\n  gas\n  stop\n"), "reaches `gas`");
        assert_eq!(
            unsupported("bb0:\n  push 7\n  jump\n"),
            "jumps to an address that no pushed block has"
        );
        assert_eq!(
            unsupported("bb0:\n  push 3\n  indexed_jump bb1, bb2\nbb1:\n  stop\nbb2:\n  stop\n"),
            "an indexed jump outside its table"
        );
        assert_eq!(
            unsupported("bb0:\n  push_immutable 0, 32\n  stop\n"),
            "reads an immutable, which deployment sets"
        );
    }

    #[test]
    fn targets() {
        let options = |evm_version| Options { evm_version, ..Options::default() };
        let module = "@module test\nbb0:\n  push 0\n  push 0\n  revert\n";
        let execution = transact(module, &[], &mut Zero, options(EvmVersion::Homestead)).unwrap();
        assert_eq!(execution.outcome, Outcome::Halt(Halt::UndefinedOpcode(0xfd)));
        // Legalization expands shifts for targets without them.
        let module = "@module test
bb0:
  push 1
  push 4
  shl
  push 0
  mstore
  push 32
  push 0
  return
";
        let execution = transact(module, &[], &mut Zero, options(EvmVersion::Homestead)).unwrap();
        assert_eq!(execution.outcome, Outcome::Success(word(16)));
        // Other opcodes the target lacks are undefined there.
        let module = "@module test\nbb0:\n  push 0\n  tload\n  stop\n";
        let execution = transact(module, &[], &mut Zero, options(EvmVersion::Shanghai)).unwrap();
        assert_eq!(execution.outcome, Outcome::Halt(Halt::UndefinedOpcode(0x5c)));
        // So are the extended stack opcodes a stack operation past the 16th word needs.
        let module = format!(
            "@module test\nbb0:\n{}  dup 17\n  push 0\n  sstore\n  stop\n",
            "  push 1\n".repeat(17)
        );
        let execution = transact(&module, &[], &mut Zero, options(EvmVersion::Shanghai)).unwrap();
        assert_eq!(execution.outcome, Outcome::Halt(Halt::UndefinedOpcode(0xe6)));
        let execution = transact(&module, &[], &mut Zero, options(EvmVersion::Amsterdam)).unwrap();
        assert_eq!(execution.outcome, Outcome::Success(Vec::new()));
        assert_eq!(execution.storage, [(U256::ZERO, U256::ONE)]);
    }

    #[test]
    fn code_size_comparisons_are_unsupported() {
        // The assembled code is longer than 700 bytes, which neither layout may assume.
        let module = format!(
            "@module test\nbb0:\n{}  push 700\n  codesize\n  gt\n  push 0\n  mstore\n  push 32\n  push 0\n  return\n",
            "  push 0x1111111111111111111111111111111111111111111111111111111111111111\n  pop\n"
                .repeat(22)
        );
        let execution = transact(&module, &[], &mut Zero, Options::default()).unwrap();
        let reason = "observes the code size, which only assembly decides".to_owned();
        assert_eq!(execution.outcome, Outcome::Unsupported(reason.clone()));

        // Empty blocks assemble to no code, so the code can be shorter than the block count.
        let blocks = (0..700).map(|block| format!("bb{block}:\n  jump bb{}\n", block + 1));
        let module = format!(
            "@module test\n{}bb700:\n  push 700\n  codesize\n  lt\n  push 0\n  mstore\n  push 32\n  push 0\n  return\n",
            blocks.collect::<String>()
        );
        let execution = transact(&module, &[], &mut Zero, Options::default()).unwrap();
        assert_eq!(execution.outcome, Outcome::Unsupported(reason));
    }

    #[test]
    fn dumps() {
        let dump = "{\"contracts\":{}}
// === a.sol:A (scheduled runtime) ===
@module A_runtime
bb0:
  stop
// === a.sol:B (scheduled runtime) ===
@module B_runtime
bb0:
  stop
";
        let modules = parse_dump(dump);
        assert_eq!(
            modules
                .iter()
                .map(|module| (&module.name[..], module.program.as_deref()))
                .collect::<Vec<_>>(),
            [("a.sol:A", Some("scheduled runtime")), ("a.sol:B", Some("scheduled runtime"))]
        );
        assert!(modules.iter().all(|module| !module.evm_ir.contains("contracts")));
        // A file without headers is one module, named by its `@module` line.
        let modules = parse_dump("@module single\nbb0:\n  stop\n");
        assert_eq!(modules.len(), 1);
        assert_eq!((&modules[0].name[..], modules[0].program.as_deref()), ("single", None));
        let error = transact("@module", &[], &mut Zero, Options::default()).unwrap_err();
        assert!(error.starts_with("unparsable EVM IR: error: "), "{error}");
    }
}

//! Runs `solar-evmir-interp` on the fixtures next to this file.

#![allow(unused_crate_dependencies)]

use snapbox::{assert_data_eq, str};
use std::{
    io::Write,
    process::{Command, Stdio},
};

/// The caller the tests set.
const CALLER: &str = "0x70997970c51812dc3a010c7d01b50e0d17dc79c8";

/// What a run printed, and its exit code.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

/// Runs the tool on the fixture `input`, or on `stdin` when `input` is `-`.
fn run_with(input: &str, stdin: &str, args: &[&str]) -> Run {
    let path = if input == "-" {
        input.to_owned()
    } else {
        format!("{}/tests/fixtures/{input}", env!("CARGO_MANIFEST_DIR"))
    };
    let mut child = Command::new(env!("CARGO_BIN_EXE_solar-evmir-interp"))
        .arg(path)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the tool runs");
    child.stdin.take().expect("stdin is piped").write_all(stdin.as_bytes()).unwrap();
    let output = child.wait_with_output().expect("the tool exits");
    Run {
        code: output.status.code().expect("the tool exits"),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

fn run(fixture: &str, args: &[&str]) -> Run {
    run_with(fixture, "", args)
}

#[test]
fn transactions() {
    let caller = format!("caller={CALLER}");
    let run =
        run("counter.evmir", &["--call", "increment()", "--storage", "0=41", "--context", &caller]);
    assert_eq!((run.code, run.stderr.as_str()), (0, ""));
    assert_data_eq!(
        run.stdout,
        str![[r#"
stopped
events:
  0: topics [0xf57bb28dda50be2d99bc97dce404fae0ecca665bba60759ec491a82e893e4fd5, 0x70997970c51812dc3a010c7d01b50e0d17dc79c8], data 0x000000000000000000000000000000000000000000000000000000000000002a
storage:
  0x0 = 0x2a

"#]]
    );

    // A signature that says what the function returns decodes it.
    let run =
        self::run("counter.evmir", &["--call", "count() returns (uint256)", "--storage", "0=7"]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
returned 0x0000000000000000000000000000000000000000000000000000000000000007
decoded: 7

"#]]
    );

    // Standard reverts are decoded too: the increment overflows.
    let max = format!("0=0x{}", "f".repeat(64));
    let run = self::run("counter.evmir", &["--calldata", "0xd09de08a", "--storage", &max]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
reverted with 0x4e487b710000000000000000000000000000000000000000000000000000000000000011 (Panic(17))

"#]]
    );
}

#[test]
fn traces() {
    let run = run("counter.evmir", &["--call", "count()", "--storage", "0=7", "--trace"]);
    assert_eq!(run.code, 0);
    assert_data_eq!(
        run.stderr,
        str![[r#"
bb0: push 0 -> 0x0
bb0: calldataload  [0x0] -> 0x6661abd00000000000000000000000000000000000000000000000000000000
bb0: push 224 -> 0xe0
bb0: shr  [0xe0, 0x6661abd00000000000000000000000000000000000000000000000000000000] -> 0x6661abd
bb0: dup 1 -> 0x6661abd
bb0: push 0xd09de08a -> 0xd09de08a
bb0: eq  [0xd09de08a, 0x6661abd] -> 0x0
bb0: push bb2 -> 0x2
bb0: jumpi  [0x2, 0x0]
bb0: dup 1 -> 0x6661abd
bb0: push 0x6661abd -> 0x6661abd
bb0: eq  [0x6661abd, 0x6661abd] -> 0x1
bb0: push bb4 -> 0x4
bb0: jumpi  [0x4, 0x1]
bb4: push 0 -> 0x0
bb4: sload  [0x0] -> 0x7
bb4: push 0 -> 0x0
bb4: mstore  [0x0, 0x7]
bb4: push 32 -> 0x20
bb4: push 0 -> 0x0
bb4: return  [0x0, 0x20]

"#]]
    );
}

#[test]
fn json() {
    let run = run(
        "counter.evmir",
        &["--call", "count() returns (uint256)", "--storage", "0=7", "--json"],
    );
    assert_data_eq!(
        run.stdout,
        str![[r#"
{
  "data": "0x0000000000000000000000000000000000000000000000000000000000000007",
  "decoded": [
    "7"
  ],
  "logs": [],
  "outcome": "success",
  "storage": {},
  "transient": {}
}

"#]]
    );
}

#[test]
fn dumps() {
    let run = run("contracts.dump", &["--contract", "Probe"]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
returned 0x0000000000000000000000000000000000000000000000000000000000000007

"#]]
    );
    let run =
        self::run("contracts.dump", &["--contract", "src/Other.sol:Other (scheduled runtime)"]);
    assert_eq!(run.stdout, "stopped\n");

    let run = self::run("contracts.dump", &[]);
    assert_eq!(run.code, 1);
    assert_data_eq!(
        run.stderr,
        str![[r#"
error: the input holds several modules, so choose one of src/Probe.sol:Probe (scheduled runtime), src/Other.sol:Other (scheduled runtime) with `--contract`

"#]]
    );
    let run = self::run("contracts.dump", &["--contract", "Missing"]);
    assert_eq!(run.code, 1);
    assert_data_eq!(
        run.stderr,
        str![[r#"
error: no module is named `Missing`; the input holds src/Probe.sol:Probe (scheduled runtime), src/Other.sol:Other (scheduled runtime)

"#]]
    );
}

#[test]
fn unsupported_and_halts() {
    let run = run_with("-", "@module gas\nbb0:\n  gas\n  stop\n", &[]);
    assert_eq!(run.code, 2);
    assert_eq!(run.stdout, "could not run: reaches `gas`\n");

    let run = run_with("-", "@module halt\nbb0:\n  pop\n  stop\n", &["--json"]);
    assert_eq!(run.code, 0);
    assert_data_eq!(
        run.stdout,
        str![[r#"
{
  "logs": [],
  "outcome": "halt",
  "reason": "stack underflow",
  "storage": {},
  "transient": {}
}

"#]]
    );
}

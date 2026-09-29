//! Runs `solar-mir-interp` on the fixtures next to this file.

#![allow(unused_crate_dependencies)]

use snapbox::{assert_data_eq, str};
use std::process::Command;

/// The caller the tests set.
const CALLER: &str = "0x70997970c51812dc3a010c7d01b50e0d17dc79c8";

/// What a run printed, and its exit code.
struct Run {
    code: i32,
    stdout: String,
    stderr: String,
}

fn run(fixture: &str, args: &[&str]) -> Run {
    let path = format!("{}/tests/fixtures/{fixture}", env!("CARGO_MANIFEST_DIR"));
    let output = Command::new(env!("CARGO_BIN_EXE_solar-mir-interp"))
        .arg(path)
        .args(args)
        .output()
        .expect("the tool runs");
    Run {
        code: output.status.code().expect("the tool exits"),
        stdout: String::from_utf8(output.stdout).unwrap(),
        stderr: String::from_utf8(output.stderr).unwrap(),
    }
}

#[test]
fn transactions() {
    let caller = format!("caller={CALLER}");
    let run =
        run("counter.mir", &["--call", "increment()", "--storage", "0=41", "--context", &caller]);
    assert_eq!((run.code, run.stderr.as_str()), (0, ""));
    assert_data_eq!(
        run.stdout,
        str![[r#"
stopped
events:
  0: topics [0x7a2e4c9bdfb0c1ee2fb7e6d8d0e21e4d2e1c73f3a9a7ff1d6d8b1e8c1b0a9f01, 0x70997970c51812dc3a010c7d01b50e0d17dc79c8], data 0x000000000000000000000000000000000000000000000000000000000000002a
storage:
  0x0 = 0x2a

"#]]
    );

    // A signature that says what the function returns decodes it.
    let run =
        self::run("counter.mir", &["--call", "count() returns (uint256)", "--storage", "0=7"]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
returned 0x0000000000000000000000000000000000000000000000000000000000000007
decoded: 7

"#]]
    );

    // Standard reverts are decoded too: the checked add overflows.
    let max = format!("0=0x{}", "f".repeat(64));
    let run = self::run("counter.mir", &["--calldata", "0xd09de08a", "--storage", &max]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
reverted with 0x4e487b710000000000000000000000000000000000000000000000000000000000000011 (Panic(17))

"#]]
    );
}

#[test]
fn functions_and_traces() {
    let run = run("counter.mir", &["--function", "add", "--arg", "2", "--arg", "0x3", "--trace"]);
    assert_eq!(run.code, 0);
    assert_data_eq!(
        run.stdout,
        str![[r#"
returned 0x5

"#]]
    );
    assert_data_eq!(
        run.stderr,
        str![[r#"
@add bb0: v0 = add arg0, arg1  [arg0 = 0x2, arg1 = 0x3] -> 0x5
@add bb0: v1 = lt v0, arg0  [v0 = 0x5, arg0 = 0x2] -> 0x0
@add bb0: jumpi v1, bb1, bb2  [v1 = 0x0]
@add bb2: ret v0  [v0 = 0x5]

"#]]
    );
}

#[test]
fn json() {
    let run =
        run("counter.mir", &["--call", "count() returns (uint256)", "--storage", "0=7", "--json"]);
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
    // Calls take the heap frames the dump reports: `@probe` sees the free memory pointer past its
    // frame, which it keeps.
    let run = run("frames.dump", &["--contract", "Frames"]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
returned 0x00000000000000000000000000000000000000000000000000000000000000c000000000000000000000000000000000000000000000000000000000000000c0

"#]]
    );
    let run = self::run("frames.dump", &["--contract", "src/Other.sol:Other"]);
    assert_data_eq!(
        run.stdout,
        str![[r#"
stopped

"#]]
    );
    let run = self::run("frames.dump", &[]);
    assert_eq!(run.code, 1);
    assert_data_eq!(
        run.stderr,
        str![[r#"
error: the input holds several modules, so choose one of src/Frames.sol:Frames, src/Other.sol:Other with `--contract`

"#]]
    );
}

#[test]
fn errors() {
    let run = run("counter.mir", &["--function", "missing"]);
    assert_eq!(run.code, 1);
    assert_data_eq!(
        run.stderr,
        str![[r#"
error: the module has no function `@missing`

"#]]
    );
    let run = self::run("counter.mir", &["--function", "add", "--arg", "1"]);
    assert_eq!(run.code, 1);
    assert_data_eq!(
        run.stderr,
        str![[r#"
error: `@add` takes 2 words, not 1 word

"#]]
    );
    let run = self::run("counter.mir", &["--calldata", "0x", "--context", "sender=1"]);
    assert_eq!(run.code, 1);
    assert_data_eq!(
        run.stderr,
        str![[r#"
error: no context read is named `sender`; the reads are address, balance, origin, caller, callvalue, codesize, gasprice, extcodesize, extcodehash, blockhash, coinbase, timestamp, number, prevrandao, gaslimit, chainid, selfbalance, basefee, blobhash, blobbasefee, slotnum

"#]]
    );
    // The interpreter cannot run `gas`, and says so with its own exit code.
    let run = self::run("counter.mir", &["--function", "remaining"]);
    assert_eq!(run.code, 2);
    assert_data_eq!(
        run.stdout,
        str![[r#"
could not run: reaches `gas`

"#]]
    );
}

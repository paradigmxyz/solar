//! Builder validation is output-independent, bounded, and never edits executable MIR.

use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    path::Path,
    process::{Command, Stdio},
};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

fn compile(dir: &Path, source: &str, mode: &str, output: &str) -> std::process::Output {
    fs::write(dir.join("test.sol"), source).unwrap();
    Command::new(SOLAR)
        .current_dir(dir)
        .args(["test.sol", "--allow=2264", "--threads=1", "-O", mode, output])
        .output()
        .unwrap()
}

#[test]
fn standard_json_abi_checks_builder_lifetimes() {
    let source = r#"import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";
contract C { function f() public pure { ByteBuilder memory b = Buffers.create(2);
Buffers.finish(b); Buffers.appendByte(b, 0x01); } }"#;
    let input = json!({"language":"Solidity", "sources":{"test.sol":{"content":source}},
        "settings":{"outputSelection":{"*":{"*":["abi"]}}}});
    let mut child = Command::new(SOLAR)
        .arg("--standard-json")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(input.to_string().as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    let output: Value = serde_json::from_slice(&output.stdout).unwrap();
    let messages = output["errors"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["severity"] == "error")
        .map(|e| e["message"].as_str().unwrap())
        .collect::<Vec<_>>();
    snapbox::assert_data_eq!(
        messages.join("\n"),
        snapbox::str!["this uses a builder after `finish` emptied it"]
    );
}

#[test]
fn lifetime_analysis_keeps_bytecode() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("Buffers.sol"), include_str!("../../../std/solidity/Buffers.sol"))
        .unwrap();
    let source = r#"import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";
contract C {
function done(ByteBuilder memory b, bool abort) internal pure returns (bytes memory) {
    if (abort) { Buffers.finish(b); revert(); }
    Buffers.appendByte(b, 0x01); return Buffers.finish(b);
}
function f() public pure returns (bytes memory) { return done(Buffers.create(2), false); }
}"#;
    for mode in ["none", "gas", "size"] {
        let checked = compile(dir.path(), source, mode, "--emit=bin,bin-runtime");
        let portable = compile(
            dir.path(),
            &source.replace("solar:core/Buffers.sol", "./Buffers.sol"),
            mode,
            "--emit=bin,bin-runtime",
        );
        assert!(checked.status.success(), "{}", String::from_utf8_lossy(&checked.stderr));
        assert!(portable.status.success(), "{}", String::from_utf8_lossy(&portable.stderr));
        let checked: Value = serde_json::from_slice(&checked.stdout).unwrap();
        let portable: Value = serde_json::from_slice(&portable.stdout).unwrap();
        assert_eq!(
            checked["contracts"]["test.sol:C"], portable["contracts"]["test.sol:C"],
            "{mode}"
        );
    }
}

#[test]
fn builder_analysis_handles_sequential_branches_and_helper_chains() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from(
        r#"import {Buffers, ByteBuilder} from "solar:core/Buffers.sol";
contract C { function f(uint256 n) public pure returns (bytes memory) {
ByteBuilder memory b = Buffers.create(1); uint256 sum;"#,
    );
    for i in 0..512 {
        source.push_str(&format!("if (n == {i}) sum += {i};\n"));
    }
    source.push_str("Buffers.appendByte(b, bytes1(uint8(sum))); return h0(b); }\n");
    for i in 0..128 {
        source.push_str(&format!("function h{i}(ByteBuilder memory b) internal pure returns (bytes memory) {{ return h{}(b); }}\n", i+1));
    }
    source.push_str("function h128(ByteBuilder memory b) internal pure returns (bytes memory) { return Buffers.finish(b); } }");
    let output = compile(dir.path(), &source, "none", "--emit=abi");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

#[test]
fn call_exits_handle_many_entries_with_a_shared_chain() {
    let dir = tempfile::tempdir().unwrap();
    let mut source = String::from("import {Return} from \"solar:core/Return.sol\"; contract C {\n");
    for i in 0..256 {
        source.push_str(&format!("function f{i}() public pure returns (uint256) {{ h0(); }}\n"));
    }
    for i in 0..128 {
        source.push_str(&format!("function h{i}() internal pure {{ h{}(); }}\n", i + 1));
    }
    source.push_str("function h128() internal pure { Return.abiEncoded(uint256(42)); } }");
    let output = compile(dir.path(), &source, "none", "--emit=abi");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
}

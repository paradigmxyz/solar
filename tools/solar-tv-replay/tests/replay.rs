//! Exercise independent observations using the Rust interpreter bridge.

#![allow(unused_crate_dependencies)]

use serde_json::{Value, json};
use snapbox::{assert_data_eq, str};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn replay(kind: &str, before: &str, after: &str, inputs: Value, memory: bool, gas: bool) -> Value {
    let mut child = Command::new(env!("CARGO_BIN_EXE_solar-tv-replay"))
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    let request = json!({"version":1,"kind":kind,"before":before,"after":after,"inputs":inputs,
        "memory":memory,"gas":gas,"evm_version":"cancun"});
    child.stdin.take().unwrap().write_all(request.to_string().as_bytes()).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(output.status.success());
    serde_json::from_slice(&output.stdout).unwrap()
}

#[test]
fn scalar_checked_and_typed_mir() {
    let source = |body| format!("@module Test\nfn @f(arg0: i256) -> i256 {{\nbb0:\n{body}\n}}\n");
    let actual = replay(
        "mir",
        &source("v0 = checked_add uint256, arg0, 1\nret v0"),
        &source("v0 = add arg0, 1\nret v0"),
        json!({"arg0": alloy_primitives::U256::MAX.to_string()}),
        false,
        false,
    );
    assert_data_eq!(actual["observations"].to_string(), str![r#"["revert Panic(17)","0"]"#]);

    let before =
        "@module Typed\nfn @f(arg0: i1) -> i256 {\nbb0:\nv0 = sext i1 arg0 to i256\nret v0\n}\n";
    let after = before.replace("sext", "zext");
    let actual = replay("mir", before, &after, json!({"arg0":"1"}), false, false);
    assert_data_eq!(
        actual["observations"].to_string(),
        str![[
            r#"["115792089237316195423570985008687907853269984665640564039457584007913129639935","1"]"#
        ]]
    );
}

#[test]
fn evm_windows_observe_live_ins_and_gas() {
    let before = "@module Test\nbb0 [stack=1]:\ncalldatasize\nreturndatasize\nadd\npush bb1\njump\nbb1:\nstop\n";
    let after = "@module Test\nbb0 [stack=1]:\npush 10\npush bb1\njump\nbb1:\nstop\n";
    let actual = replay(
        "evmir",
        before,
        after,
        json!({"stack0":"5","calldatasize":"3","returndatasize":"7"}),
        false,
        false,
    );
    assert_data_eq!(
        actual["observations"].to_string(),
        str![r#"["bb1, stack=[10, 5]","bb1, stack=[10, 5]"]"#]
    );

    let before = include_str!("../../../solar-tv/examples/evmir/gas-observer/before.evmir");
    let after = include_str!("../../../solar-tv/examples/evmir/gas-observer/broken.evmir");
    let actual = replay("evmir", before, after, json!({"gas":"18"}), false, true);
    assert_data_eq!(
        actual["observations"].to_string(),
        str![r#"["bb1, stack=[16], gas=0","bb1, stack=[11], gas=0"]"#]
    );
}

#[test]
fn memory_and_invalid_state_are_explicit() {
    let before = "@module Test\nfn @f() -> i256 {\nbb0:\nmstore 0, 1\nret 0\n}\n";
    let after = before.replace("mstore 0,", "mstore 32,");
    let actual = replay("mir", before, &after, json!({}), true, false);
    assert_data_eq!(
        actual["observations"].to_string(),
        str![[
            r#"["return=0, msize=32, memory=[1, 0, 0, 0, 0, 0, 0, 0]","return=0, msize=64, memory=[0, 1, 0, 0, 0, 0, 0, 0]"]"#
        ]]
    );

    let actual = replay("mir", before, before, json!({"memoryWords":"9999999999"}), true, false);
    assert_data_eq!(
        actual["reason"].as_str().unwrap(),
        str!["initial memory exceeds Rust replay's 16 MiB limit"]
    );
}

#[test]
fn mir_replay_respects_target_opcodes() {
    let text = "@module Target\nfn @f(arg0: i256) -> i256 {\nbb0:\nv0 = clz arg0\nret v0\n}\n";
    let actual = replay("mir", text, text, json!({"arg0":"1"}), false, false);
    assert_data_eq!(actual["observations"].to_string(), str![r#"["invalid","invalid"]"#]);
}

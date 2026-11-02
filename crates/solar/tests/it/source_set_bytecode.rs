//! A contract's bytecode does not depend on unrelated sources in the same compilation.

use serde_json::Value;
use std::{fs, process::Command};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

const POINTERS: &[(&str, &str)] = &[
    (
        "A.sol",
        "contract A {
            function() internal view returns (uint256) private _op;
            function f() internal pure returns (uint256) { return 1; }
            function g() internal pure returns (uint256) { return 2; }
            function set(bool b) external { _op = b ? f : g; }
            function run() external view returns (uint256) { return _op(); }
        }",
    ),
    (
        "0.sol",
        "contract B {
            function h1() internal pure {}
            function h2() internal pure {}
            function h3() internal pure {}
        }",
    ),
];

const ORDER: &[(&str, &str)] = &[
    (
        "C.sol",
        "import \"./L2.sol\";
        import \"./L1.sol\";
        contract C {
            uint256 x;
            function c0(uint256 v) external { x = L2.l2f0(v * 1) + L1.l1f0(v + x); }
            function c1(uint256 v) external { x = L2.l2f0(v * 2) + L1.l1f0(v + x); }
        }",
    ),
    (
        "L1.sol",
        "library L1 {
            function l1f0(uint256 v) internal pure returns (uint256) {
                v = v * 3 + (v >> 1) ^ 26050;
                v = v * 4 + (v >> 2) ^ 5231;
                v = v * 5 + (v >> 3) ^ 42039;
                v = v * 6 + (v >> 4) ^ 19753;
                if (v & 166 == 0) { v = v * 7; } else { v += 3; }
                return v;
            }
        }",
    ),
    (
        "L2.sol",
        "library L2 {
            function l2f0(uint256 v) internal pure returns (uint256) {
                v = v * 6 + (v >> 1) ^ 59122;
                if (v & 235 == 0) { v = v * 7; } else { v += 3; }
                return v;
            }
        }",
    ),
    (
        "0b.sol",
        "import \"./L1.sol\";
        contract D { function d(uint256 v) external pure returns (uint256) { return L1.l1f0(v); } }",
    ),
];

const DEFAULT_BINDINGS: &[(&str, &str)] = &[
    (
        "R.sol",
        "contract R {
            struct S { uint256 a; bytes b; }
            function f() external pure returns (S memory, uint256[2] memory, S[2] memory) {
                S memory s; uint256[2] memory d; S[2] memory p;
                return (s, d, p);
            }
        }",
    ),
    (
        "0z.sol",
        "contract Z {
            uint256 internal v0; uint256 internal v1; uint256 internal v2;
            uint256 internal v3; uint256 internal v4; uint256 internal v5;
        }",
    ),
];

fn compile(dir: &std::path::Path, mode: &str, files: &[&str]) -> Value {
    let output = Command::new(SOLAR)
        .current_dir(dir)
        .args(["-j1", "-O", mode, "--emit=bin,bin-runtime"])
        .args(files)
        .output()
        .expect("run compiler");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).expect("parse compiler JSON")
}

fn assert_independent(sources: &[(&str, &str)], contract: &str, with_extra: &[&str]) {
    let dir = tempfile::tempdir().unwrap();
    for (name, source) in sources {
        fs::write(dir.path().join(name), source).unwrap();
    }
    let alone = [sources[0].0];
    for mode in ["none", "gas", "size"] {
        let alone = &compile(dir.path(), mode, &alone)["contracts"][contract];
        let together = &compile(dir.path(), mode, with_extra)["contracts"][contract];
        for output in ["bin", "bin-runtime"] {
            assert!(alone[output].as_str().is_some_and(|code| !code.is_empty()));
            assert_eq!(alone[output], together[output], "{contract} {output} at -O {mode}");
        }
    }
}

#[test]
fn internal_function_pointers_ignore_unrelated_sources() {
    assert_independent(POINTERS, "A.sol:A", &["0.sol", "A.sol"]);
}

#[test]
fn function_order_ignores_unrelated_sources() {
    assert_independent(ORDER, "C.sol:C", &["0b.sol", "C.sol"]);
}

#[test]
fn default_bindings_ignore_unrelated_sources() {
    assert_independent(DEFAULT_BINDINGS, "R.sol:R", &["0z.sol", "R.sol"]);
}

//! `@custom:solar-optimize` compiles a contract as a build for its objective would.

use serde_json::Value;
use std::{fs, path::Path, process::Command};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");

/// A contract whose code depends on the objective: a loop, string building, storage, errors,
/// events, and a repeated constant that size-first code builds by division.
const SOURCE: &str = r#"contract C {
    error TooLarge(uint256 value);
    event Pushed(uint256 indexed value);

    uint256[] internal items;

    constructor(uint256 first) {
        items.push(first);
    }

    function push(uint256 value) external {
        if (value > 1000) revert TooLarge(value);
        items.push(value);
        emit Pushed(value);
    }

    function sum() external view returns (uint256 total) {
        for (uint256 i = 0; i < items.length; i++) {
            total += items[i];
        }
    }

    function describe(string calldata name) external pure returns (string memory) {
        return string.concat("hello, ", name, "!");
    }

    function pattern() external pure returns (bytes32) {
        return 0x0101010101010101010101010101010101010101010101010101010101010101;
    }
}
"#;

/// Compiles `source` with `-O <mode>` and returns the deployment and runtime bytecode of its
/// contracts.
fn compile(dir: &Path, source: &str, mode: &str) -> Value {
    fs::write(dir.join("c.sol"), source).expect("write source");
    let output = Command::new(SOLAR)
        .current_dir(dir)
        .args(["--allow", "2264", "--threads", "1", "-O", mode, "--emit=bin,bin-runtime", "c.sol"])
        .output()
        .expect("run compiler");
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    let mut output = serde_json::from_slice::<Value>(&output.stdout).expect("parse compiler JSON");
    output["contracts"].take()
}

#[test]
fn tag_selects_the_objective_of_optimized_builds() {
    let dir = tempfile::tempdir().expect("create directory");
    let dir = dir.path();
    let tagged = |objective: &str| format!("/// @custom:solar-optimize {objective}\n{SOURCE}");
    let plain = |mode| compile(dir, SOURCE, mode);
    assert_ne!(plain("gas"), plain("size"));
    assert_eq!(compile(dir, &tagged("size"), "gas"), plain("size"));
    assert_eq!(compile(dir, &tagged("gas"), "size"), plain("gas"));
    // The tag picks what an optimized build optimizes for; `-O none` still optimizes nothing.
    assert_eq!(compile(dir, &tagged("size"), "none"), plain("none"));
}

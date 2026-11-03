//! Source origins survive lowering and agree with the legacy map, without changing bytecode.

use serde_json::Value;
use snapbox::{assert_data_eq, str};
use solar::interface::source_map::{FileLoader, RealFileLoader};
use std::{path::Path, process::Command};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/foundry/source-mappings");

fn compile(mode: &str, outputs: &str) -> Value {
    compile_with(mode, outputs, &[])
}

fn compile_with(mode: &str, outputs: &str, flags: &[&str]) -> Value {
    let output = Command::new(SOLAR)
        .current_dir(FIXTURES)
        .args(["src/Base.sol", "src/Decode.sol", "--allow=2264", "-O", mode])
        .arg(format!("--emit={outputs}"))
        .args(flags)
        .output()
        .unwrap();
    assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
    serde_json::from_slice(&output.stdout).unwrap()
}

fn source_entries(text: &str) -> Vec<[i64; 3]> {
    let mut previous = [-1; 3];
    text.split(';')
        .map(|entry| {
            for (index, value) in entry.split(':').take(3).enumerate() {
                if !value.is_empty() {
                    previous[index] = value.parse().unwrap();
                }
            }
            previous
        })
        .collect()
}

fn in_statement(entry: [i64; 3], source: usize, start: usize, end: usize) -> bool {
    entry[2] == source as i64 && entry[0] >= start as i64 && entry[0] < end as i64
}

#[test]
fn abi_origins_and_conditional_thunks() {
    for mode in ["none", "gas", "size"] {
        let plain = compile(mode, "bin,bin-runtime");
        let debug = compile(mode, "bin,bin-runtime,srcmap-runtime,ethdebug-runtime");
        let audited = compile_with(mode, "bin,bin-runtime", &["-Zdebugify=report"]);
        for name in ["src/Base.sol:Harness", "src/Decode.sol:Decode"] {
            for code in ["bin", "bin-runtime"] {
                assert_eq!(
                    plain["contracts"][name][code], debug["contracts"][name][code],
                    "{mode}"
                );
                assert_eq!(
                    plain["contracts"][name][code], audited["contracts"][name][code],
                    "{mode}: debugify changed bytecode"
                );
            }
        }
        let source_id = |name: &str| {
            debug["sourceList"]
                .as_array()
                .unwrap()
                .iter()
                .position(|source| source == name)
                .unwrap()
        };
        let decode = &debug["contracts"]["src/Decode.sol:Decode"];
        let entries = source_entries(decode["srcmap-runtime"].as_str().unwrap());
        let source = RealFileLoader.load_file(&Path::new(FIXTURES).join("src/Decode.sol")).unwrap();
        let mut mapped = Vec::new();
        for statement in [
            "abi.decode(data, (Op, bytes))",
            "op == Op.A",
            "return (who, got)",
            "return (address(0), 0)",
            "abi.decode(rest, (address, uint256))",
        ] {
            let start = source.find(statement).unwrap();
            let end = start + statement.len();
            let legacy = entries
                .iter()
                .any(|&entry| in_statement(entry, source_id("src/Decode.sol"), start, end));
            let ethdebug =
                decode["ethdebug-runtime"]["instructions"].as_array().unwrap().iter().any(|inst| {
                    let code = &inst["context"]["code"];
                    code["source"]["id"].as_u64() == Some(source_id("src/Decode.sol") as u64)
                        && code["range"]["offset"]
                            .as_u64()
                            .is_some_and(|offset| (start as u64..end as u64).contains(&offset))
                });
            mapped.push((statement, legacy, ethdebug));
        }
        assert_data_eq!(
            format!("{mapped:?}"),
            str![[
                r#"[("abi.decode(data, (Op, bytes))", true, true), ("op == Op.A", true, true), ("return (who, got)", true, true), ("return (address(0), 0)", true, true), ("abi.decode(rest, (address, uint256))", true, true)]"#
            ]]
        );

        let base = &debug["contracts"]["src/Base.sol:Harness"];
        let entries = source_entries(base["srcmap-runtime"].as_str().unwrap());
        let instructions = base["ethdebug-runtime"]["instructions"].as_array().unwrap();
        let source = RealFileLoader.load_file(&Path::new(FIXTURES).join("src/Base.sol")).unwrap();
        let start = source.find("require(").unwrap();
        let end = start + "require(x != address(0), ZeroAddress())".len();
        assert!(
            instructions.windows(2).enumerate().any(|(index, pair)| {
                pair[0]["operation"]["mnemonic"].as_str().unwrap().starts_with("PUSH")
                    && pair[1]["operation"]["mnemonic"] == "JUMPI"
                    && in_statement(entries[index], source_id("src/Base.sol"), start, end)
            }),
            "{mode}: the require branch lost its conditional label's origin"
        );
        if mode == "none" {
            let start = source.find("_set(x)").unwrap();
            let end = start + "_set(x)".len();
            assert!(
                instructions.iter().enumerate().any(|(index, inst)| {
                    inst["operation"]["mnemonic"] == "JUMPDEST"
                        && in_statement(entries[index], source_id("src/Base.sol"), start, end)
                        && inst["context"]["code"]["range"]["offset"].as_u64() == Some(start as u64)
                }),
                "the removed tail jump must use its exclusive callee's existing label"
            );
        }
    }
}

/// Run with explicit FORGE and SOLC paths to compare the current tools, without downloading them.
#[test]
#[ignore = "requires current Foundry and solc; set FORGE and SOLC and run this test explicitly"]
fn foundry_decode_and_require_coverage_matches_solc() {
    let forge = std::env::var_os("FORGE").expect("set FORGE to the current forge binary");
    let solc = std::env::var_os("SOLC").expect("set SOLC to the reference solc binary");
    let temp = tempfile::tempdir().unwrap();
    let mut reports = Vec::new();
    for (name, compiler) in [("solc", solc), ("solar", SOLAR.into())] {
        let run = |kind: &str| {
            let report = temp.path().join(format!("{name}-{kind}"));
            let output = Command::new(&forge)
                .current_dir(FIXTURES)
                .env("SOLC_WRAPPER", "1")
                .args(["coverage", "--exclude-tests", "--report", kind, "--use"])
                .arg(&compiler)
                .arg("--out")
                .arg(temp.path().join(format!("out-{name}")))
                .arg("--cache-path")
                .arg(temp.path().join(format!("cache-{name}")))
                .arg("--report-file")
                .arg(&report)
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{name}: {}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            RealFileLoader.load_file(&report).unwrap()
        };
        let text = run("lcov");
        let decode = text
            .split("end_of_record")
            .find(|record| record.lines().any(|line| line == "SF:src/Decode.sol"))
            .expect("Decode must appear in the coverage report");
        let base = text
            .split("end_of_record")
            .find(|record| record.lines().any(|line| line == "SF:src/Base.sol"))
            .expect("Base must appear in the coverage report");
        // LCOV combines statements on the same line. Also compare per-test source items so a
        // covered `if` line cannot hide a missing condition-expression statement.
        let mut attribution: Value = serde_json::from_str(&run("attribution")).unwrap();
        let tests = attribution["tests"].as_array_mut().unwrap();
        for test in tests.iter_mut() {
            let covered = test["covered"].as_array_mut().unwrap();
            covered.retain(|item| {
                item["source"] == "src/Decode.sol" || item["source"] == "src/Base.sol"
            });
            covered.sort_by_key(Value::to_string);
        }
        tests.sort_by_key(|test| test["test"].to_string());
        reports.push((decode.trim().to_owned(), base.trim().to_owned(), attribution));
    }
    assert_eq!(reports[0], reports[1]);
}

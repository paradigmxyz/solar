//! Source origins survive lowering and agree with the legacy map, without changing bytecode.

use serde_json::{Value, json};
use snapbox::{assert_data_eq, str};
use solar::{
    cli::standard_json::compile_standard_json,
    config::CompileOpts,
    interface::source_map::{FileLoader, RealFileLoader},
};
use std::{path::Path, process::Command};

const SOLAR: &str = env!("CARGO_BIN_EXE_solar");
const FIXTURES: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/../../tests/foundry/source-mappings");

fn compile(mode: &str, outputs: &str) -> Value {
    compile_with(mode, outputs, &[])
}

fn compile_with(mode: &str, outputs: &str, flags: &[&str]) -> Value {
    let output = Command::new(SOLAR)
        .current_dir(FIXTURES)
        .args(["src/Base.sol", "src/Decode.sol", "src/StaticBytes.sol", "--allow=2264", "-O", mode])
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
        let static_bytes = text
            .split("end_of_record")
            .find(|record| record.lines().any(|line| line == "SF:src/StaticBytes.sol"))
            .expect("StaticBytes must appear in the coverage report");
        // LCOV combines statements on the same line. Also compare per-test source items so a
        // covered `if` line cannot hide a missing condition-expression statement.
        let mut attribution: Value = serde_json::from_str(&run("attribution")).unwrap();
        let tests = attribution["tests"].as_array_mut().unwrap();
        for test in tests.iter_mut() {
            let covered = test["covered"].as_array_mut().unwrap();
            covered.retain(|item| {
                item["source"] == "src/Decode.sol"
                    || item["source"] == "src/Base.sol"
                    || item["source"] == "src/StaticBytes.sol"
            });
            covered.sort_by_key(Value::to_string);
        }
        tests.sort_by_key(|test| test["test"].to_string());
        reports.push((
            decode.trim().to_owned(),
            base.trim().to_owned(),
            static_bytes.trim().to_owned(),
            attribution,
        ));
    }
    assert_eq!(reports[0], reports[1]);
}

#[test]
fn debugify_sidecars_preserve_output() {
    let temp = tempfile::tempdir().unwrap();
    for mode in ["none", "gas", "size"] {
        let directory = temp.path().join(mode);
        let flag = format!("-Zdebugify-output={}", directory.display());
        let plain = compile(mode, "bin,bin-runtime");
        let audited = compile_with(mode, "bin,bin-runtime", &[&flag]);
        assert_eq!(plain, audited, "{mode}: sidecar auditing changed compiler output");
        let files = std::fs::read_dir(&directory).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
        assert_eq!(files.len(), 1, "one serialized audit file per compiler invocation");
        let text = RealFileLoader.load_file(&files[0].path()).unwrap();
        let records = text
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(records[0]["kind"], "invocation");
        assert_eq!(records[0]["version"], 1);
        assert!(!records[0]["sources"].as_array().unwrap().is_empty());
        let layers = records
            .iter()
            .skip(1)
            .map(|record| {
                if record["kind"] == "gaps" {
                    let count = record["count"].as_u64().unwrap();
                    assert!(count > 0);
                    assert_eq!(
                        record["omitted"].as_u64().unwrap()
                            + record["sites"].as_array().unwrap().len() as u64,
                        count
                    );
                    return record["ir"].as_str().unwrap();
                }
                assert_eq!(record["kind"], "pass");
                assert_eq!(
                    record["before"].as_u64().unwrap(),
                    record["retained"].as_u64().unwrap()
                        + record["lost"].as_array().unwrap().len() as u64
                );
                record["ir"].as_str().unwrap()
            })
            .collect::<std::collections::BTreeSet<_>>();
        assert_data_eq!(format!("{layers:?}"), str![[r#"{"EVM IR", "MIR"}"#]]);
    }
}

#[test]
fn static_bytes_origins_survive_direct_returns() {
    let source =
        RealFileLoader.load_file(&Path::new(FIXTURES).join("src/StaticBytes.sol")).unwrap();
    for mode in ["none", "gas", "size"] {
        let plain = compile(mode, "bin,bin-runtime");
        let debug = compile(mode, "bin,bin-runtime,srcmap-runtime,ethdebug-runtime");
        let name = "src/StaticBytes.sol:StaticBytes";
        for code in ["bin", "bin-runtime"] {
            assert_eq!(plain["contracts"][name][code], debug["contracts"][name][code], "{mode}");
        }
        let id = debug["sourceList"]
            .as_array()
            .unwrap()
            .iter()
            .position(|s| s == "src/StaticBytes.sol")
            .unwrap();
        let contract = &debug["contracts"][name];
        let entries = source_entries(contract["srcmap-runtime"].as_str().unwrap());
        for statement in
            ["abi.encode(uint256(7), uint256(11))", "abi.encode()", "return hex\"0102\""]
        {
            let start = source.find(statement).unwrap();
            let end = start + statement.len();
            assert!(
                entries.iter().any(|&entry| in_statement(entry, id, start, end)),
                "{mode}: {statement} missing from source map"
            );
            assert!(
                contract["ethdebug-runtime"]["instructions"].as_array().unwrap().iter().any(
                    |inst| {
                        let code = &inst["context"]["code"];
                        code["source"]["id"].as_u64() == Some(id as u64)
                            && code["range"]["offset"]
                                .as_u64()
                                .is_some_and(|offset| (start as u64..end as u64).contains(&offset))
                    }
                ),
                "{mode}: {statement} missing from ethdebug"
            );
        }
    }
}

#[test]
fn tail_call_thunk_chain_keeps_call_site() {
    let path =
        Path::new(FIXTURES).join("../../ui/codegen/lowering/run-call/internal_stop_runtime.sol");
    let source = RealFileLoader.load_file(&path).unwrap();
    let start = source.find("halt();").unwrap();
    let end = start + "halt()".len();
    let compile = |outputs: &str| {
        let output = Command::new(SOLAR)
            .arg(&path)
            .args(["-Onone", "--allow=2264"])
            .arg(format!("--emit={outputs}"))
            .output()
            .unwrap();
        assert!(output.status.success(), "{}", String::from_utf8_lossy(&output.stderr));
        serde_json::from_slice::<Value>(&output.stdout).unwrap()
    };
    let plain = compile("bin,bin-runtime");
    let debug = compile("bin,bin-runtime,srcmap-runtime,ethdebug-runtime");
    let contract = debug["contracts"].as_object().unwrap().values().next().unwrap();
    let baseline = plain["contracts"].as_object().unwrap().values().next().unwrap();
    for code in ["bin", "bin-runtime"] {
        assert_eq!(contract[code], baseline[code]);
    }
    let entries = source_entries(contract["srcmap-runtime"].as_str().unwrap());
    assert!(entries.iter().any(|&entry| in_statement(entry, 0, start, end)));
    assert!(contract["ethdebug-runtime"]["instructions"].as_array().unwrap().iter().any(|inst| {
        inst["operation"]["mnemonic"] == "JUMPDEST"
            && inst["context"]["code"]["range"]["offset"].as_u64() == Some(start as u64)
    }));
}

#[test]
fn debugify_sidecars_separate_compilations() {
    let temp = tempfile::tempdir().unwrap();
    let cases = [
        (
            "First.sol",
            "First",
            "contract First { function f(uint256 x) external pure returns (uint256) { unchecked { uint256 dead = x * 3; return x + 1; } } } contract FirstExtra {}",
        ),
        (
            "Second.sol",
            "Second",
            "// A different source table — its byte ranges also start at zero.\ncontract Second { function f(uint256 x) external pure returns (uint256) { unchecked { uint256 dead = x * 7; return x + 2; } } } contract SecondExtra {}",
        ),
    ];
    for (name, contract, source) in cases {
        let input = json!({
            "language": "Solidity",
            "sources": {name: {"content": source}},
            "settings": {
                "optimizer": {"enabled": true},
                "metadata": {"appendCBOR": false, "bytecodeHash": "none"},
                "outputSelection": {"*": {"*": ["evm.bytecode.object", "evm.deployedBytecode.object"]}}
            }
        }).to_string();
        let compile = |audit| {
            let mut opts = CompileOpts { threads: 2.into(), ..Default::default() };
            if audit {
                opts.unstable.debugify_output = Some(temp.path().to_owned());
            }
            let mut output = Vec::new();
            compile_standard_json(&input, opts, None, &mut output).unwrap();
            serde_json::from_slice::<Value>(&output).unwrap()
        };
        let plain = compile(false);
        assert!(
            plain["contracts"][name][contract]["evm"]["bytecode"]["object"]
                .as_str()
                .is_some_and(|code| !code.is_empty())
        );
        assert_eq!(compile(true), plain, "{name}: auditing changed compiler output");
    }
    let files = std::fs::read_dir(temp.path()).unwrap().collect::<Result<Vec<_>, _>>().unwrap();
    assert_eq!(files.len(), 2, "each compilation needs its own source header");
    let mut observed = Vec::new();
    for file in files {
        let text = RealFileLoader.load_file(&file.path()).unwrap();
        let records = text
            .lines()
            .map(|line| serde_json::from_str::<Value>(line).unwrap())
            .collect::<Vec<_>>();
        let header = &records[0];
        assert_eq!(header["kind"], "invocation");
        let sources = header["sources"].as_array().unwrap();
        assert_eq!(sources.len(), 1);
        let name = sources[0]["name"].as_str().unwrap();
        let (_, contract, source) = cases.iter().find(|(path, _, _)| *path == name).unwrap();
        assert_eq!(sources[0]["start"], 0);
        assert_eq!(sources[0]["end"], source.len());
        let mut lost_count = 0;
        for record in &records[1..] {
            assert!(
                record["module"].as_str().unwrap().starts_with(contract),
                "{name}: mixed compilation records"
            );
            if record["kind"] == "gaps" {
                continue;
            }
            assert_eq!(record["kind"], "pass");
            for span in record["lost"].as_array().unwrap() {
                let start = span[0].as_u64().unwrap();
                let end = span[1].as_u64().unwrap();
                assert!(
                    start <= end && end <= source.len() as u64,
                    "{name}: span outside its source table"
                );
                lost_count += 1;
            }
        }
        observed.push((name.to_owned(), records.len() > 1, lost_count > 0));
    }
    observed.sort();
    assert_data_eq!(
        format!("{observed:?}"),
        str![[r#"[("First.sol", true, true), ("Second.sol", true, true)]"#]]
    );
}

//! Runs the prover over `tests/cases`: every fact of a source in `proved/` must be proved, and
//! every source in `refuted/` must have a fact with a counterexample, so the encoding neither
//! loses facts it proves today nor starts proving false ones.
//!
//! The test needs Z3 and is skipped, with a message on stderr, when it is missing: set
//! `SOLAR_PROVE_ANALYSES_Z3` to the binary, or `SOLAR_PROVE_ANALYSES_REQUIRE_Z3=1` to fail
//! instead. It builds the compiler unless `SOLAR_BIN` names one.

#![allow(clippy::disallowed_methods)]

// The binary's dependencies, which the test does not use.
use clap as _;
use rayon as _;
use regex as _;
use ruint as _;
use serde_json::Value;
use sha2 as _;
use solar_smt as _;
use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn corpus() {
    let Some(z3) = z3() else { return };
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let out = tempfile::tempdir().unwrap();
    prove(&z3, out.path(), &cases);

    let results = std::fs::read_to_string(out.path().join("results.jsonl")).unwrap();
    let mut verdicts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in results.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        let field = |key: &str| record[key].as_str().unwrap_or_default().to_string();
        let source = field("source");
        let source = Path::new(&source).strip_prefix(&cases).unwrap().display().to_string();
        verdicts.entry(source).or_default().push(format!(
            "{}: {}",
            field("result"),
            field("claim")
        ));
    }
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(cases.join("proved")).unwrap() {
        let name = format!("proved/{}", entry.unwrap().file_name().to_string_lossy());
        match verdicts.get(&name) {
            None => failures.push(format!("{name}: no facts")),
            Some(facts) => failures.extend(
                facts
                    .iter()
                    .filter(|fact| !fact.starts_with("unsat:"))
                    .map(|f| format!("{name}: {f}")),
            ),
        }
    }
    for entry in std::fs::read_dir(cases.join("refuted")).unwrap() {
        let entry = entry.unwrap();
        let name = format!("refuted/{}", entry.file_name().to_string_lossy());
        // A counterexample through memory reads the caller's memory, which the queries leave
        // free, so such a case accepts one the runner marks opaque.
        let source = std::fs::read_to_string(entry.path()).unwrap();
        let opaque = source.contains("// counterexample: opaque");
        let refuted = verdicts.get(&name).is_some_and(|facts| {
            facts.iter().any(|f| f.starts_with("sat:") || (opaque && f.starts_with("sat-opaque:")))
        });
        if !refuted {
            failures.push(format!("{name}: no counterexample"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// A run into an `--out` directory an earlier run used solves this run's queries alone: an
/// abstract tier left there that proves anything does not prove a false fact.
#[test]
fn stale_out() {
    let Some(z3) = z3() else { return };
    let case = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases/refuted/false_range.mir");
    let out = tempfile::tempdir().unwrap();
    prove(&z3, out.path(), &case);
    let mut planted = 0;
    for dir in std::fs::read_dir(out.path()).unwrap() {
        let dir = dir.unwrap().path();
        if !dir.is_dir() {
            continue;
        }
        for query in std::fs::read_dir(&dir).unwrap() {
            let query = query.unwrap().path();
            let name = query.file_name().unwrap().to_string_lossy().into_owned();
            if let Some(stem) = name.strip_suffix(".smt2")
                && !stem.contains('.')
            {
                let stale = dir.join(format!("{stem}.abstract.smt2"));
                std::fs::write(
                    stale,
                    "(set-logic QF_AUFBV)
(assert false)
(check-sat)
",
                )
                .unwrap();
                planted += 1;
            }
        }
    }
    assert!(planted > 0);
    prove(&z3, out.path(), &case);
    let results = std::fs::read_to_string(out.path().join("results.jsonl")).unwrap();
    assert!(results.lines().any(|line| {
        let record: Value = serde_json::from_str(line).unwrap();
        record["result"] == "sat"
    }));
}

/// Runs the pass validator over `tests/validate`: every function a pass changed in a source in
/// `proved/` must be proved preserved, and a source in `refuted/`, a rewrite that changes its
/// function, must have a counterexample.
#[test]
fn validate() {
    let Some(z3) = z3() else { return };
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/validate");
    let out = tempfile::tempdir().unwrap();
    run(&z3, out.path(), &cases, &["--validate-passes"]);
    let results = std::fs::read_to_string(out.path().join("results.jsonl")).unwrap();
    let mut verdicts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in results.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        let field = |key: &str| record[key].as_str().unwrap_or_default().to_string();
        let source = field("source");
        let source = Path::new(&source).strip_prefix(&cases).unwrap().display().to_string();
        verdicts.entry(source).or_default().push(format!(
            "{}: {}",
            field("result"),
            field("claim")
        ));
    }
    let mut failures = Vec::new();
    for (kind, wanted) in [("proved", "unsat:"), ("refuted", "sat:")] {
        for entry in std::fs::read_dir(cases.join(kind)).unwrap() {
            let name = format!("{kind}/{}", entry.unwrap().file_name().to_string_lossy());
            let verdicts = verdicts.get(&name).map(Vec::as_slice).unwrap_or_default();
            let ok = if kind == "proved" {
                !verdicts.is_empty() && verdicts.iter().all(|v| v.starts_with(wanted))
            } else {
                verdicts.iter().any(|v| v.starts_with(wanted))
            };
            if !ok {
                failures.push(format!("{name}: {verdicts:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Runs the stack scheduling validator over `tests/validate-backend`: every block of a source in
/// `proved/` must be proved, and a source in `refuted/`, compiled with a fault planted in what the
/// validator reads, must have a counterexample.
#[test]
fn validate_backend() {
    let Some(z3) = z3() else { return };
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/validate-backend");
    let out = tempfile::tempdir().unwrap();
    run(&z3, out.path(), &cases, &["--validate-backend"]);
    let results = std::fs::read_to_string(out.path().join("results.jsonl")).unwrap();
    let mut verdicts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in results.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        let field = |key: &str| record[key].as_str().unwrap_or_default().to_string();
        let source = field("source");
        let source = Path::new(&source).strip_prefix(&cases).unwrap().display().to_string();
        verdicts.entry(source).or_default().push(format!(
            "{}: {}",
            field("result"),
            field("claim")
        ));
    }
    let mut failures = Vec::new();
    for kind in ["proved", "refuted"] {
        for entry in std::fs::read_dir(cases.join(kind)).unwrap() {
            let name = format!("{kind}/{}", entry.unwrap().file_name().to_string_lossy());
            let verdicts = verdicts.get(&name).map(Vec::as_slice).unwrap_or_default();
            let ok = if kind == "proved" {
                !verdicts.is_empty() && verdicts.iter().all(|v| v.starts_with("unsat:"))
            } else {
                verdicts.iter().any(|v| v.starts_with("sat:"))
            };
            if !ok {
                failures.push(format!("{name}: {verdicts:?}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Recording the schedule map and writing backend queries changes no bytecode: a few codegen
/// sources compile to the same deployment and runtime code with and without
/// `-Zvalidate-backend`, at `-Ogas` and `-Osize`.
#[test]
fn validate_backend_is_bytecode_neutral() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = [
        "tools/prove-analyses/tests/validate-backend/proved/calls.sol",
        "tests/ui/codegen/run-call/memory/byte_copy_loops.sol",
        "tests/ui/codegen/run-call/storage-arrays/array_copy_memory_storage.sol",
        "tests/ui/codegen/lowering/evm-ir/spills/decompression_loop_spills.sol",
        "tests/ui/codegen/lowering/evm-ir/stack-too-deep/deep_nesting.sol",
    ];
    let solar = solar();
    let out = tempfile::tempdir().unwrap();
    for source in sources {
        let source = root.join(source);
        for level in ["gas", "size"] {
            let compile = |flags: &[String]| {
                let output = Command::new(&solar)
                    .args(["-O", level, "--emit=bin,bin-runtime"])
                    .args(flags)
                    .arg(&source)
                    .output()
                    .unwrap();
                assert!(
                    output.status.success(),
                    "{}: {}",
                    source.display(),
                    String::from_utf8_lossy(&output.stderr)
                );
                output.stdout
            };
            let plain = compile(&[]);
            let validated = compile(&[format!("-Zvalidate-backend={}", out.path().display())]);
            assert_eq!(plain, validated, "{} at -O{level}", source.display());
        }
    }
}

/// Runs the EVM IR pass validator over `tests/validate-evm-ir`: every code region a pass
/// changed in each source must be proved preserved.
#[test]
fn validate_evm_ir() {
    let Some(z3) = z3() else { return };
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/validate-evm-ir");
    let out = tempfile::tempdir().unwrap();
    run(&z3, out.path(), &cases, &["--validate-evm-ir-passes"]);
    let results = std::fs::read_to_string(out.path().join("results.jsonl")).unwrap();
    let mut verdicts: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for line in results.lines() {
        let record: Value = serde_json::from_str(line).unwrap();
        let field = |key: &str| record[key].as_str().unwrap_or_default().to_string();
        let source = field("source");
        let source = Path::new(&source).strip_prefix(&cases).unwrap().display().to_string();
        verdicts.entry(source).or_default().push(format!(
            "{}: {}",
            field("result"),
            field("claim")
        ));
    }
    let mut failures = Vec::new();
    for entry in std::fs::read_dir(&cases).unwrap() {
        let name = entry.unwrap().file_name().to_string_lossy().into_owned();
        let verdicts = verdicts.get(&name).map(Vec::as_slice).unwrap_or_default();
        if verdicts.is_empty() || !verdicts.iter().all(|v| v.starts_with("unsat:")) {
            failures.push(format!("{name}: {verdicts:?}"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// Validating the EVM IR passes only reads the program: the compiler emits the same bytecode
/// with and without `-Zvalidate-evm-ir-passes`.
#[test]
fn evm_ir_validation_keeps_bytecode() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let sources = [
        "tools/prove-analyses/tests/validate-evm-ir/calls.sol",
        "tests/ui/codegen/evm-ir/copy-joins/loop_joins.sol",
        "tests/ui/codegen/evm-ir/peephole/constant_swaps_runtime.sol",
        "tests/ui/codegen/evm-ir/returndata-zero/returndata_zero.sol",
    ];
    let solar = solar();
    let out = tempfile::tempdir().unwrap();
    for source in sources {
        for level in ["gas", "size"] {
            let compile = |flags: &[String]| {
                let output = Command::new(&solar)
                    .args(["-O", level, "--emit=bin,bin-runtime"])
                    .args(flags)
                    .arg(root.join(source))
                    .output()
                    .unwrap();
                assert!(output.status.success(), "{source} at -O{level} failed");
                output.stdout
            };
            let flag = format!("-Zvalidate-evm-ir-passes={}", out.path().display());
            assert_eq!(compile(&[]), compile(&[flag]), "{source} at -O{level}");
        }
    }
    assert!(std::fs::read_dir(out.path()).unwrap().next().is_some(), "no queries written");
}

/// Z3, or `None` when it is missing and the test should be skipped.
fn z3() -> Option<PathBuf> {
    let z3 = env::var_os("SOLAR_PROVE_ANALYSES_Z3").map_or_else(|| "z3".into(), PathBuf::from);
    if Command::new(&z3).arg("--version").output().is_err() {
        assert!(
            env::var_os("SOLAR_PROVE_ANALYSES_REQUIRE_Z3").is_none(),
            "z3 not found at {}",
            z3.display()
        );
        eprintln!("skipping: z3 not found; set SOLAR_PROVE_ANALYSES_Z3 to run the prover corpus");
        return None;
    }
    Some(z3)
}

/// Runs the prover over `path`, writing its queries and `results.jsonl` to `out`.
fn prove(z3: &Path, out: &Path, path: &Path) {
    run(z3, out, path, &[]);
}

/// Runs the tool with `flags` over `path`, writing its queries and `results.jsonl` to `out`.
fn run(z3: &Path, out: &Path, path: &Path, flags: &[&str]) {
    let status = Command::new(env!("CARGO_BIN_EXE_solar-prove-analyses"))
        .args(flags)
        .arg("--solar")
        .arg(solar())
        .arg("--z3")
        .arg(z3)
        .args(["--levels", "gas,size", "--out"])
        .arg(out)
        .arg(path)
        .status()
        .unwrap();
    assert!(status.success());
}

/// The compiler: `SOLAR_BIN`, or a debug build of the workspace's.
fn solar() -> PathBuf {
    if let Some(solar) = env::var_os("SOLAR_BIN") {
        return solar.into();
    }
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let status = Command::new(env!("CARGO"))
        .args(["build", "-p", "solar-compiler", "--bin", "solar"])
        .current_dir(&root)
        .status()
        .unwrap();
    assert!(status.success(), "building the compiler failed");
    let target = env::var_os("CARGO_TARGET_DIR").map_or_else(|| root.join("target"), PathBuf::from);
    target.join("debug/solar")
}

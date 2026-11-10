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
use std::{
    collections::BTreeMap,
    env,
    path::{Path, PathBuf},
    process::Command,
};

#[test]
fn corpus() {
    let z3 = env::var_os("SOLAR_PROVE_ANALYSES_Z3").map_or_else(|| "z3".into(), PathBuf::from);
    if Command::new(&z3).arg("--version").output().is_err() {
        assert!(
            env::var_os("SOLAR_PROVE_ANALYSES_REQUIRE_Z3").is_none(),
            "z3 not found at {}",
            z3.display()
        );
        eprintln!("skipping: z3 not found; set SOLAR_PROVE_ANALYSES_Z3 to run the prover corpus");
        return;
    }
    let cases = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/cases");
    let out = tempfile::tempdir().unwrap();
    let status = Command::new(env!("CARGO_BIN_EXE_solar-prove-analyses"))
        .arg("--solar")
        .arg(solar())
        .arg("--z3")
        .arg(&z3)
        .args(["--levels", "gas,size", "--out"])
        .arg(out.path())
        .arg(&cases)
        .status()
        .unwrap();
    assert!(status.success());

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
        let name = format!("refuted/{}", entry.unwrap().file_name().to_string_lossy());
        let refuted =
            verdicts.get(&name).is_some_and(|facts| facts.iter().any(|f| f.starts_with("sat:")));
        if !refuted {
            failures.push(format!("{name}: no counterexample"));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
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

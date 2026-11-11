//! Tests of the `solar-verify` binary.
//!
//! The snapshot tests need no solver: they run `--dry-run`, `--instrument` or `mutate --list` and
//! pin the frontend's diagnostics, the SMT-LIB obligations of a function, the instrumented source
//! and the mutants drawn for a file.
//!
//! The other tests need z3 and are skipped, with a message on stderr, when it is missing (set
//! `SOLAR_VERIFY_Z3`, or `SOLAR_VERIFY_REQUIRE_Z3=1` to fail instead). In each case file every
//! function named `ok_*` must verify and every `bad_*` must fail with a definite verdict: a
//! counterexample or an unsupported construct, not an unknown from the solver. `// expect: text`
//! comments right above a function pin text in its diagnostics; `// expect: solver returned
//! unknown` is the one way to accept an unknown, for claims the solver cannot refute. A leading
//! `// args: ...` line passes more arguments, such as import remappings.
#![allow(unused_crate_dependencies)]
#![allow(clippy::disallowed_methods)] // The tests read their Solidity sources directly.

use snapbox::{assert_data_eq, file};
use std::{collections::BTreeMap, path::Path, process::Command};

/// Per-query solver timeout in milliseconds.
const TIMEOUT: &str = "5000";

struct Run {
    stdout: String,
    stderr: String,
    code: Option<i32>,
}

fn run_verifier(args: &[&str]) -> Run {
    let out = Command::new(env!("CARGO_BIN_EXE_solar-verify"))
        .args(args)
        .env("NO_COLOR", "1")
        .output()
        .expect("run solar-verify");
    Run {
        stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
        stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        code: out.status.code(),
    }
}

/// `s` ending in exactly one newline, as snapshot files do.
fn tidy(s: &str) -> String {
    format!("{}\n", s.trim_end())
}

/// The diagnostics (errors, warnings, notes) that name function `name`.
fn blocks_of(stderr: &str, name: &str) -> String {
    let mut blocks: Vec<String> = vec![];
    for l in stderr.lines() {
        if l.starts_with("error") || l.starts_with("warning") || l.starts_with("note") {
            blocks.push(String::new());
        }
        if let Some(b) = blocks.last_mut() {
            b.push_str(l);
            b.push('\n');
        }
    }
    let note = format!(".{name}");
    let header = format!(".{name}: ");
    blocks
        .into_iter()
        .filter(|b| {
            b.lines().next().is_some_and(|h| h.contains(&header))
                || b.lines().any(|l| {
                    l.split_once("note: in ").is_some_and(|(_, f)| f.trim_end().ends_with(&note))
                })
        })
        .collect()
}

/// `fn -> status` from `--json` output.
fn statuses(stdout: &str) -> BTreeMap<String, Vec<String>> {
    let json = stdout.lines().find_map(|l| l.strip_prefix("JSON ")).expect("no JSON line");
    let rows: Vec<serde_json::Value> = serde_json::from_str(json).expect("bad JSON line");
    let mut out = BTreeMap::<String, Vec<String>>::new();
    for row in rows {
        let name = row["fn"].as_str().unwrap().rsplit('.').next().unwrap().to_string();
        out.entry(name).or_default().push(row["status"].as_str().unwrap().to_string());
    }
    out
}

fn check(file: &str) {
    if z3_missing() {
        return;
    }
    let src = std::fs::read_to_string(file).unwrap();
    let extra: Vec<&str> = src
        .lines()
        .next()
        .and_then(|line| line.strip_prefix("// args:"))
        .map(|args| args.split_whitespace().collect())
        .unwrap_or_default();
    let run = |flags: &[&str]| {
        let mut args = vec!["--json", "--timeout", TIMEOUT];
        args.extend(flags);
        args.extend(&extra);
        args.push(file);
        run_verifier(&args)
    };
    // Every proof here takes well under a second; the timeout bounds the queries a case expects
    // the solver to give up on.
    let plain = run(&[]);
    let strict = src.contains("strict_only").then(|| run(&["--strict"]));
    let mut problems = vec![];
    let lines: Vec<&str> = src.lines().collect();
    for (ln, line) in lines.iter().enumerate() {
        let Some(pos) = line.find("function ") else { continue };
        let name: String =
            line[pos + 9..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let mut expects = vec![];
        let mut k = ln;
        while k > 0 && lines[k - 1].trim_start().starts_with("//") {
            k -= 1;
            if let Some(want) = lines[k].trim_start().strip_prefix("// expect:") {
                expects.push(want.trim());
            }
        }
        let want_ok = name.starts_with("ok_");
        // A function with `// expect:` lines must fail, whatever its name (an implementation
        // keeps its interface's name).
        if !want_ok && !name.starts_with("bad_") && expects.is_empty() {
            continue;
        }
        let run = if name.contains("strict_only") { strict.as_ref().unwrap() } else { &plain };
        let st = statuses(&run.stdout).remove(&name).unwrap_or_default();
        let ok = if want_ok {
            !st.is_empty() && st.iter().all(|s| s == "verified")
        } else {
            // Unknown counts only where the test says so, by expecting it.
            let unknown_ok = expects.iter().any(|e| e.contains("solver returned unknown"));
            st.iter().any(|s| s == "failed" || s == "unsupported" || unknown_ok && s == "unknown")
        };
        if !ok {
            let want = if want_ok { "verified" } else { "a definite failure" };
            problems.push(format!("{name}: expected {want}, got {st:?}"));
        }
        let blocks = blocks_of(&run.stderr, &name);
        for want in expects {
            if !blocks.contains(want) {
                problems.push(format!("{name}: output does not say `{want}`"));
            }
        }
    }
    assert!(
        problems.is_empty(),
        "{file}:\n{}\n--- stdout ---\n{}\n--- stderr ---\n{}",
        problems.join("\n"),
        plain.stdout,
        plain.stderr
    );
}

/// Whether z3 is missing, in which case the calling test is skipped.
fn z3_missing() -> bool {
    let z3 = std::env::var("SOLAR_VERIFY_Z3").unwrap_or_else(|_| "z3".into());
    if Command::new(&z3).arg("-version").output().is_ok() {
        return false;
    }
    assert!(
        std::env::var_os("SOLAR_VERIFY_REQUIRE_Z3").is_none(),
        "z3 not found (`{z3}`) and SOLAR_VERIFY_REQUIRE_Z3 is set"
    );
    eprintln!("skipped: z3 not found (`{z3}`); set SOLAR_VERIFY_Z3 to run this test");
    true
}

#[test]
fn spec_errors() {
    let run = run_verifier(&["--dry-run", "tests/snapshots/spec_errors.sol"]);
    assert_data_eq!(tidy(&run.stdout), file!["snapshots/spec_errors.stdout"]);
    assert_data_eq!(tidy(&run.stderr), file!["snapshots/spec_errors.stderr"]);
}

/// Ghost code that is not erasable: one error per broken non-interference rule, and nothing is
/// verified.
#[test]
fn ghost_errors() {
    let run = run_verifier(&["--dry-run", "tests/snapshots/ghost_errors.sol"]);
    assert_data_eq!(tidy(&run.stdout), file!["snapshots/ghost_errors.stdout"]);
    assert_data_eq!(tidy(&run.stderr), file!["snapshots/ghost_errors.stderr"]);
}

#[test]
fn smt_obligations() {
    let out = std::env::temp_dir()
        .join(format!("solar-verify-counter-increment-{}.smt2", std::process::id()));
    let out_s = out.to_str().unwrap();
    let run = run_verifier(&[
        "--dry-run",
        "--only",
        "Counter.increment",
        "--dump-smt",
        out_s,
        "examples/Counter.sol",
    ]);
    let smt = std::fs::read_to_string(&out).unwrap();
    let _ = std::fs::remove_file(&out);
    assert_data_eq!(tidy(&smt), file!["snapshots/counter_increment.smt2"]);
    assert_data_eq!(tidy(&run.stderr), file!["snapshots/counter_increment.stderr"]);
}

#[test]
fn instrument() {
    let out =
        std::env::temp_dir().join(format!("solar-verify-instrument-{}.sol", std::process::id()));
    let run =
        run_verifier(&["--instrument", out.to_str().unwrap(), "tests/snapshots/instrument.sol"]);
    let src = std::fs::read_to_string(&out).unwrap();
    assert!(run.stderr.contains("(4 checks as require)"), "{}", run.stderr);
    assert_data_eq!(tidy(&src), file!["snapshots/instrument.instrumented.sol"]);
    // The instrumented file verifies alike.
    let in_tests = Path::new("tests/snapshots/instrument.instrumented.sol");
    check(in_tests.to_str().unwrap());
    let _ = std::fs::remove_file(&out);
}

#[test]
fn mutate_list() {
    let run = run_verifier(&["mutate", "--list", "--per-fn", "100", "tests/snapshots/mutants.sol"]);
    assert_data_eq!(tidy(&run.stdout), file!["snapshots/mutants.list"]);
}

/// Counter's specs kill every drawn mutant but one: `>=` -> `>` in `max`, which no spec
/// constrains.
#[test]
fn mutate_counter() {
    if z3_missing() {
        return;
    }
    let run = run_verifier(&["mutate", "--per-fn", "3", "--no-tce", "examples/Counter.sol"]);
    assert_data_eq!(tidy(&run.stdout), file!["snapshots/counter.mutate.stdout"]);
}

/// `--spec-check`: every tested target named `ok_*` holds and every `bad_*` is violated, with
/// a counterexample; a `Math.mulDiv` that rounds up violates the built-in summary, and
/// OpenZeppelin's satisfies it.
#[test]
fn spec_check() {
    if z3_missing() {
        return;
    }
    let run = run_verifier(&["--spec-check", "tests/cases/speccheck.sol"]);
    let ctx = format!("--- stdout ---\n{}\n--- stderr ---\n{}", run.stdout, run.stderr);
    assert_eq!(run.code, Some(1), "{ctx}");
    let verdicts: BTreeMap<String, bool> = run
        .stdout
        .lines()
        .filter_map(|l| {
            let held = l.starts_with('✔');
            if !held && !l.starts_with('✘') {
                return None;
            }
            let name = l.split_whitespace().nth(1)?.rsplit('.').next()?.to_string();
            Some((name, held))
        })
        .collect();
    let want = [
        ("bad_inc", false),
        ("ok_max", true),
        ("ok_burnLocal", true),
        ("bad_withdraw", false),
        ("ok_vacuous", true),
        ("mulDiv", false),
        ("bad_quote", false),
        ("ok_half", true),
    ];
    for (name, held) in want {
        assert_eq!(verdicts.get(name), Some(&held), "{name}\n{ctx}");
    }
    assert_eq!(verdicts.len(), want.len(), "{ctx}");
    for cex in [
        "counterexample: x = 7; returned result = 7",
        "call reverts where @custom:succeeds-if says it returns",
        "failing spec: result <= amount",
        "Vault.ok_vacuous (@custom:trusted): no sample satisfied the requires",
    ] {
        assert!(run.stderr.contains(cex), "missing `{cex}`\n{ctx}");
    }
    assert!(run.stdout.contains("spec-check results:: 4 held, 4 violated, 0 not tested"), "{ctx}");

    let run = run_verifier(&["--spec-check", "tests/cases/speccheck_oz.sol"]);
    assert_eq!(run.code, Some(0), "{}\n{}", run.stdout, run.stderr);
    assert!(run.stdout.contains("spec-check results:: 2 held, 0 violated"), "{}", run.stdout);
}

#[test]
fn examples() {
    if z3_missing() {
        return;
    }
    let mut out = String::new();
    for f in [
        "examples/ArrayMax.sol",
        "examples/Bugs.sol",
        "examples/Counter.sol",
        "examples/Token.sol",
        "examples/Vault.sol",
        "examples/struct_slot.sol",
    ] {
        out.push_str(&format!("$ solar-verify {f}\n{}\n", run_verifier(&[f]).stdout));
    }
    assert_data_eq!(tidy(&out), file!["snapshots/examples.stdout"]);
}

/// Errors that stop a file with imports from being verified, which need no solver.
#[test]
fn import_errors() {
    let run = run_verifier(&["--dry-run", "tests/snapshots/imports/errors.sol"]);
    assert_data_eq!(tidy(&run.stderr), file!["snapshots/imports/errors.stderr"]);
}

/// `--instrument` rewrites only the verified file, so it refuses a check in an imported one.
#[test]
fn instrument_imported_check() {
    let out = std::env::temp_dir()
        .join(format!("solar-verify-instrument-imported-{}.sol", std::process::id()));
    let run = Command::new(env!("CARGO_BIN_EXE_solar-verify"))
        .args(["--instrument", out.to_str().unwrap(), "tests/snapshots/imports/instrument.sol"])
        .env("NO_COLOR", "1")
        .output()
        .expect("run solar-verify");
    assert_eq!(run.status.code(), Some(1));
    assert!(!out.exists(), "wrote {}", out.display());
    assert_data_eq!(
        tidy(&String::from_utf8_lossy(&run.stderr)),
        file!["snapshots/imports/instrument.stderr"]
    );
}

/// Imported contracts are not verified themselves, though the functions a verified contract
/// inherits from them are, and mutation testing reads the imports too.
#[test]
fn imports_scope() {
    let remap = "shapes/=tests/cases/auxiliary/imports/shapes/";
    let run = run_verifier(&["--dry-run", "--json", remap, "tests/cases/imports.sol"]);
    let contracts: std::collections::BTreeSet<String> = run
        .stdout
        .lines()
        .find_map(|l| l.strip_prefix("JSON "))
        .map(|json| serde_json::from_str::<Vec<serde_json::Value>>(json).unwrap())
        .unwrap()
        .iter()
        .map(|row| row["fn"].as_str().unwrap().split('.').next().unwrap().to_string())
        .collect();
    assert_eq!(contracts.into_iter().collect::<Vec<_>>(), ["Imports"], "{}", run.stderr);
    let list =
        run_verifier(&["mutate", "--list", "--per-fn", "1", remap, "tests/cases/imports.sol"]);
    assert!(list.stdout.contains("mutants in"), "{}{}", list.stdout, list.stderr);
    assert!(!list.stdout.contains("Base."), "{}", list.stdout);
}

#[test]
fn arith() {
    check("tests/cases/arith.sol")
}

#[test]
fn asm() {
    check("tests/cases/asm.sol")
}

#[test]
fn broadcast() {
    check("tests/cases/broadcast.sol")
}

#[test]
fn control() {
    check("tests/cases/control.sol")
}

#[test]
fn eth() {
    check("tests/cases/eth.sol")
}

#[test]
fn features() {
    check("tests/cases/features.sol")
}

#[test]
fn fixedarrays() {
    check("tests/cases/fixedarrays.sol")
}

#[test]
fn imports() {
    check("tests/cases/imports.sol");
}

#[test]
fn ghost() {
    check("tests/cases/ghost.sol")
}

#[test]
fn liveness() {
    check("tests/cases/liveness.sol")
}

#[test]
fn lockinternal() {
    check("tests/cases/lockinternal.sol")
}

#[test]
fn reentrancy() {
    check("tests/cases/reentrancy.sol")
}

#[test]
fn rewrites() {
    check("tests/cases/rewrites.sol")
}

#[test]
fn speclang() {
    check("tests/cases/speclang.sol")
}

#[test]
fn storage() {
    check("tests/cases/storage.sol")
}

#[test]
fn struct_invariant() {
    check("tests/cases/struct_invariant.sol")
}

#[test]
fn structs() {
    check("tests/cases/structs.sol")
}

#[test]
fn vox() {
    check("tests/cases/vox.sol")
}

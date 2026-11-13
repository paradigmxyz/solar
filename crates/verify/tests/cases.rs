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
fn asm_memory_paths() {
    check("tests/cases/asm_memory_paths.sol")
}

#[test]
fn memory_alias() {
    check("tests/cases/memory_alias.sol")
}

#[test]
fn mem_call_loop() {
    check("tests/cases/mem_call_loop.sol")
}

#[test]
fn boolkeys() {
    check("tests/cases/boolkeys.sol")
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
fn created() {
    check("tests/cases/created.sol")
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
fn fnptr() {
    check("tests/cases/fnptr.sol")
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
fn slots() {
    check("tests/cases/slots.sol")
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

#[test]
fn horn() {
    check("tests/cases/horn.sol")
}

/// The loops of the examples carry no invariants: inference finds them, and without it the
/// postconditions that need them fail.
#[test]
fn inferred() {
    if z3_missing() {
        return;
    }
    let mut out = String::new();
    for file in ["examples/ArrayMax.sol", "examples/Counter.sol"] {
        let run = run_verifier(&[
            "--show-inferred",
            "--only",
            if file.contains("Array") { "ArrayMax.max" } else { "Counter.bump" },
            file,
        ]);
        out.push_str(&format!(
            "$ solar-verify --show-inferred {file}
{}
",
            run.stdout
        ));
        let off = run_verifier(&["--no-infer", file]);
        out.push_str(&format!(
            "$ solar-verify --no-infer {file}
{}
",
            off.stdout
        ));
    }
    assert_data_eq!(tidy(&out), file!["snapshots/inferred.stdout"]);
}

/// A scratch directory for one test's output, empty.
fn scratch(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("solar-verify-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

/// `--emit-lean` without a solver: the module of a function is stable (format version 2), and
/// the manifest binds it to the source and names the spec clause each obligation discharges.
#[test]
fn emit_lean() {
    let dir = scratch("emit-lean");
    let out = dir.to_str().unwrap();
    // Without a solver every obligation is unknown, so the run fails; the export does not.
    let run = run_verifier(&["--dry-run", "--emit-lean", out, "examples/Counter.sol"]);
    assert!(!run.stderr.contains("--emit-lean"), "{}", run.stderr);
    let module = std::fs::read_to_string(dir.join("Counter/increment.lean")).unwrap();
    assert_data_eq!(module, file!["snapshots/emit_lean/Counter/increment.lean"]);
    let manifest = read_manifest(&dir);
    assert_eq!(manifest["format"], "solar-verify-lean");
    assert_eq!(manifest["version"], 2);
    assert_eq!(manifest["errors"], serde_json::json!([]));
    assert_eq!(
        specs(&manifest, "Counter.increment"),
        [
            ("postcondition not satisfied", "count == old(count) + 1"),
            ("contract invariant not preserved", "count <= limit"),
        ]
    );
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--emit-lean` on the other examples: mappings (Token), a reentrancy lock and `assert`
/// (Vault), and a loop with an invariant and a `decreases` clause over an array (ArrayMax), one
/// run per file.
#[test]
fn emit_lean_examples() {
    let dir = scratch("emit-lean-examples");
    for example in ["Token", "Vault", "ArrayMax"] {
        let src = format!("examples/{example}.sol");
        let out = dir.join(example);
        let run = run_verifier(&["--dry-run", "--emit-lean", out.to_str().unwrap(), &src]);
        assert!(!run.stderr.contains("--emit-lean"), "{}", run.stderr);
        assert_eq!(read_manifest(&out)["errors"], serde_json::json!([]), "{example}");
    }
    let read = |m: &str| std::fs::read_to_string(dir.join(m)).unwrap();
    assert_data_eq!(
        read("Token/Token/transfer.lean"),
        file!["snapshots/emit_lean/Token/transfer.lean"]
    );
    assert_data_eq!(
        read("Vault/Vault/withdraw.lean"),
        file!["snapshots/emit_lean/Vault/withdraw.lean"]
    );
    assert_data_eq!(
        read("ArrayMax/ArrayMax/max.lean"),
        file!["snapshots/emit_lean/ArrayMax/max.lean"]
    );
    let manifests: Vec<_> =
        ["Token", "Vault", "ArrayMax"].iter().map(|e| read_manifest(&dir.join(e))).collect();
    let units: Vec<_> = manifests
        .iter()
        .flat_map(|m| m["units"].as_array().unwrap())
        .map(|u| u["name"].as_str().unwrap())
        .collect();
    for name in [
        "Token.constructor",
        "Token.mint",
        "Token.transfer",
        "Token.burn",
        "Vault.constructor",
        "Vault.deposit",
        "Vault.withdraw",
        "ArrayMax.max",
        "ArrayMax.clamp",
    ] {
        assert!(units.contains(&name), "{name} missing from {units:?}");
    }
    // Without a solver inference keeps no candidate: the loop has only the postconditions.
    let kinds: Vec<_> = specs(&manifests[2], "ArrayMax.max").into_iter().map(|(k, _)| k).collect();
    assert_eq!(kinds, ["postcondition not satisfied", "postcondition not satisfied"]);
    let kinds: Vec<_> =
        specs(&manifests[1], "Vault.withdraw").into_iter().map(|(k, _)| k).collect();
    assert!(kinds.contains(&"assertion failed"), "{kinds:?}");
    assert!(
        kinds.iter().any(|k| k.contains("reentrancy guard `locked`")),
        "Vault.withdraw: no lock obligation in {kinds:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

fn read_manifest(dir: &std::path::Path) -> serde_json::Value {
    serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap()
}

/// The kind and spec text of a unit's obligations, in order.
fn specs<'a>(manifest: &'a serde_json::Value, unit: &str) -> Vec<(&'a str, &'a str)> {
    manifest["units"]
        .as_array()
        .unwrap()
        .iter()
        .find(|u| u["name"] == unit)
        .unwrap_or_else(|| panic!("no unit {unit}"))["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["role"] == "obligation")
        .map(|o| (o["kind"].as_str().unwrap(), o["spec"]["text"].as_str().unwrap_or("")))
        .collect()
}

/// The exported terms agree with the SMT-LIB solar-verify sends: on random models, z3 and the
/// Lean kernel evaluate them alike (`scripts/roundtrip.py`). Needs z3 and a solidity-lean checkout
/// in `SOLIDITY_LEAN`; skipped without one unless `SOLAR_VERIFY_REQUIRE_LEAN=1`.
#[test]
fn emit_lean_round_trip() {
    if z3_missing() {
        return;
    }
    let Some(lean_root) = solidity_lean() else { return };
    let lean_root = lean_root.as_path();
    let build = Command::new("lake")
        .args(["build", "Verify.Obligation"])
        .current_dir(lean_root)
        .output()
        .unwrap();
    assert!(build.status.success(), "{}", String::from_utf8_lossy(&build.stdout));
    for example in ["Counter", "Token", "Vault", "ArrayMax"] {
        let dir = scratch(&format!("round-trip-{example}"));
        let out = dir.to_str().unwrap();
        let src = format!("examples/{example}.sol");
        let run = run_verifier(&["--timeout", TIMEOUT, "--emit-lean", out, &src]);
        assert_eq!(run.code, Some(0), "{}\n{}", run.stdout, run.stderr);
        let rt = Command::new("python3")
            .arg("scripts/roundtrip.py")
            .arg(out)
            .env("SOLIDITY_LEAN", lean_root)
            .output()
            .expect("run python3");
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&rt.stdout),
            String::from_utf8_lossy(&rt.stderr)
        );
        assert!(rt.status.success(), "{example}:\n{text}");
        assert!(text.contains(" 0 failures"), "{example}:\n{text}");
        // Every exported module elaborates against the library.
        for unit in std::fs::read_dir(&dir).unwrap() {
            let unit = unit.unwrap().path();
            if !unit.is_dir() {
                continue;
            }
            for module in std::fs::read_dir(&unit).unwrap() {
                let module = module.unwrap().path();
                if module.extension().is_none_or(|e| e != "lean") {
                    continue;
                }
                let lean = Command::new("lake")
                    .args(["env", "lean"])
                    .arg(&module)
                    .env("LEAN_NUM_THREADS", "1")
                    .current_dir(lean_root)
                    .output()
                    .unwrap();
                assert!(
                    lean.status.success(),
                    "{}:\n{}{}",
                    module.display(),
                    String::from_utf8_lossy(&lean.stdout),
                    String::from_utf8_lossy(&lean.stderr)
                );
            }
        }
        let _ = std::fs::remove_dir_all(&dir);
    }
}

/// Several input files in one run verify as they do one at a time: each file's specs are its own
/// (spec source names were once reused across files, which parsed the first file's specs in
/// place of the later ones').
#[test]
fn several_files_in_one_run() {
    let files =
        ["examples/Token.sol", "examples/Vault.sol", "examples/ArrayMax.sol", "examples/Bugs.sol"];
    let marks = |out: &str| -> Vec<String> {
        out.lines()
            .filter(|l| l.starts_with('\u{2714}') || l.starts_with('\u{2718}'))
            .map(str::to_string)
            .collect()
    };
    let mut alone = vec![];
    for file in files {
        alone.extend(marks(&run_verifier(&["--dry-run", file]).stdout));
    }
    let mut args = vec!["--dry-run"];
    args.extend(files);
    let together = run_verifier(&args);
    assert!(!together.stderr.contains("not erasable"), "{}", together.stderr);
    assert_eq!(marks(&together.stdout), alone);
}

/// The solidity-lean checkout in `SOLIDITY_LEAN` (the Lean library exported modules import), or
/// `None` to skip a test that needs it; with `SOLAR_VERIFY_REQUIRE_LEAN` set, a missing one fails.
fn solidity_lean() -> Option<std::path::PathBuf> {
    let root = std::env::var_os("SOLIDITY_LEAN").map(std::path::PathBuf::from);
    let lake = root.as_ref().is_some_and(|r| {
        Command::new("lake")
            .arg("--version")
            .current_dir(r)
            .output()
            .is_ok_and(|o| o.status.success())
    });
    if !lake {
        assert!(
            std::env::var_os("SOLAR_VERIFY_REQUIRE_LEAN").is_none(),
            "SOLIDITY_LEAN or lake not found and SOLAR_VERIFY_REQUIRE_LEAN is set"
        );
        eprintln!("skipping: set SOLIDITY_LEAN to a solidity-lean checkout with lake installed");
        return None;
    }
    root
}

/// The library hashes `--emit-lean` records are those of the solidity-lean checkout in
/// `SOLIDITY_LEAN`: an exporter and a Lean library that disagree on the format fail here.
#[test]
fn emit_lean_library_pins() {
    let Some(root) = solidity_lean() else { return };
    let dir = scratch("emit-lean-pins");
    let out = dir.to_str().unwrap();
    run_verifier(&["--dry-run", "--emit-lean", out, "examples/Counter.sol"]);
    let manifest: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(dir.join("manifest.json")).unwrap()).unwrap();
    for (key, file) in [("smt_sha256", "Smt.lean"), ("vc_sha256", "Vc.lean")] {
        let path = root.join("Verify/Obligation").join(file);
        let sum = Command::new("sha256sum").arg(&path).output().expect("run sha256sum");
        let text = String::from_utf8_lossy(&sum.stdout);
        let got = text.split_whitespace().next().unwrap_or("");
        assert_eq!(manifest["lean"][key], got, "{}", path.display());
    }
    assert_eq!(manifest["lean"]["library"], "Verify.Obligation");
    let _ = std::fs::remove_dir_all(&dir);
}

/// `--emit-lean-spec` writes every form it translates (storage places, packed fields, mappings,
/// dynamic and packed arrays, struct fields and struct invariants, quantifiers, spec functions
/// plain and opaque, ghost state, `old`, named returns, `__time` and the call's environment) as a
/// Lean module over solidity-lean's `SolidCore.Spec`.
#[test]
fn emit_lean_spec() {
    let out = std::env::temp_dir().join(format!("solar-verify-lean-{}.lean", std::process::id()));
    let run = run_verifier(&[
        "--emit-lean-spec",
        out.to_str().unwrap(),
        "--lean-spec-module",
        "SolidCore.Spec.Export.Test",
        "tests/snapshots/lean_export.sol",
    ]);
    assert!(run.stderr.is_empty(), "{}", run.stderr);
    let lean = std::fs::read_to_string(&out).unwrap();
    assert_data_eq!(tidy(&lean), file!["snapshots/lean_export.lean"]);
    let _ = std::fs::remove_file(&out);
}

/// Every construct `--emit-lean-spec` does not translate is an error that names it, and nothing is
/// written: each file of `snapshots/lean_unsupported` pins the message in its `// expect:` line.
#[test]
fn emit_lean_spec_unsupported() {
    let mut files: Vec<_> = std::fs::read_dir("tests/snapshots/lean_unsupported")
        .unwrap()
        .map(|e| e.unwrap().path())
        .collect();
    files.sort();
    assert!(!files.is_empty());
    for f in files {
        let src = std::fs::read_to_string(&f).unwrap();
        let expect = src
            .lines()
            .find_map(|l| l.trim().strip_prefix("// expect: "))
            .unwrap_or_else(|| panic!("{}: no expect line", f.display()))
            .to_string();
        let out = std::env::temp_dir()
            .join(format!("solar-verify-lean-unsupported-{}.lean", std::process::id()));
        let _ = std::fs::remove_file(&out);
        let o = Command::new(env!("CARGO_BIN_EXE_solar-verify"))
            .args(["--emit-lean-spec", out.to_str().unwrap(), f.to_str().unwrap()])
            .env("NO_COLOR", "1")
            .output()
            .expect("run solar-verify");
        let stderr = String::from_utf8_lossy(&o.stderr);
        assert!(!o.status.success(), "{}: export succeeded", f.display());
        assert!(stderr.contains(&expect), "{}: expected {expect:?} in\n{stderr}", f.display());
        assert!(!out.exists(), "{}: a file was written", f.display());
    }
}

/// Loops whose facts the abstract interpreter finds (`--infer-absint`), and writes, calls and
/// wraparound that must keep it from claiming them.
#[test]
fn absint() {
    check("tests/cases/absint.sol")
}

/// The invariants the abstract interpreter proposes per loop, before any solver: after a write
/// to an array, a call that replaces it or a wrapping increment, the facts they break are gone.
#[test]
fn absint_candidates() {
    let out = Command::new(env!("CARGO_BIN_EXE_solar-verify"))
        .args(["--dry-run", "--infer-absint", "tests/snapshots/absint_candidates.sol"])
        .env("NO_COLOR", "1")
        .env("SOLAR_VERIFY_DEBUG", "1")
        .output()
        .expect("run solar-verify");
    let stderr = String::from_utf8_lossy(&out.stderr);
    let mut keep = false;
    let mut lines = vec![];
    for l in stderr.lines() {
        if l.starts_with("absint: ") {
            keep = true;
        }
        if keep {
            lines.push(l);
        }
        if l == "]" {
            keep = false;
        }
    }
    assert_data_eq!(tidy(&lines.join("\n")), file!["snapshots/absint_candidates.stderr"]);
}

/// The manifest names sources relative to the run's directory and leaves the command line to
/// `run.json`: the same example exported from two checkouts gives the same `manifest.json`.
#[test]
fn emit_lean_manifest_checkout_independent() {
    let root = scratch("emit-lean-checkouts");
    let mut manifests = vec![];
    for checkout in ["a", "b/nested"] {
        let dir = root.join(checkout);
        std::fs::create_dir_all(dir.join("examples")).unwrap();
        std::fs::copy("examples/Counter.sol", dir.join("examples/Counter.sol")).unwrap();
        let out = dir.join("out");
        let run = Command::new(env!("CARGO_BIN_EXE_solar-verify"))
            .args(["--dry-run", "--emit-lean", out.to_str().unwrap(), "examples/Counter.sol"])
            .current_dir(&dir)
            .env("NO_COLOR", "1")
            .output()
            .expect("run solar-verify");
        let stderr = String::from_utf8_lossy(&run.stderr);
        assert!(!stderr.contains("--emit-lean"), "{stderr}");
        let manifest = std::fs::read_to_string(out.join("manifest.json")).unwrap();
        let info: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(out.join("run.json")).unwrap()).unwrap();
        assert!(info["arguments"].as_array().is_some_and(|a| !a.is_empty()));
        assert!(info["cwd"].as_str().is_some_and(|c| c.ends_with(checkout)));
        manifests.push(manifest);
    }
    assert!(manifests[0].contains(r#""path": "examples/Counter.sol""#), "{}", manifests[0]);
    assert!(!manifests[0].contains(r#""arguments""#));
    assert_eq!(manifests[0], manifests[1]);
    let _ = std::fs::remove_dir_all(&root);
}

/// A source outside the run's directory has no checkout-independent name: the export refuses it.
#[test]
fn emit_lean_source_outside_cwd() {
    let root = scratch("emit-lean-outside");
    let run_dir = root.join("run");
    std::fs::create_dir_all(&run_dir).unwrap();
    let src = std::fs::canonicalize("examples/Counter.sol").unwrap();
    let out = run_dir.join("out");
    let run = Command::new(env!("CARGO_BIN_EXE_solar-verify"))
        .args(["--dry-run", "--emit-lean", out.to_str().unwrap(), src.to_str().unwrap()])
        .current_dir(&run_dir)
        .env("NO_COLOR", "1")
        .output()
        .expect("run solar-verify");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success());
    assert!(stderr.contains("source outside the run's directory"), "{stderr}");
    let manifest = read_manifest(&out);
    assert_eq!(manifest["sources"], serde_json::json!([]));
    assert!(manifest["errors"].to_string().contains("source outside the run's directory"));
    let _ = std::fs::remove_dir_all(&root);
}

/// With a solver, the invariants and the measure inference finds for ArrayMax's loop are exported
/// as obligations like any other: initiation, preservation and decrease.
#[test]
fn emit_lean_inferred_loop() {
    if z3_missing() {
        return;
    }
    let dir = scratch("emit-lean-inferred");
    let out = dir.to_str().unwrap();
    let run = run_verifier(&[
        "--timeout",
        TIMEOUT,
        "--only",
        "ArrayMax.max",
        "--emit-lean",
        out,
        "examples/ArrayMax.sol",
    ]);
    assert_eq!(run.code, Some(0), "{}\n{}", run.stdout, run.stderr);
    let manifest = read_manifest(&dir);
    let unit =
        manifest["units"].as_array().unwrap().iter().find(|u| u["name"] == "ArrayMax.max").unwrap();
    let kinds: Vec<&str> = unit["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|o| o["role"] == "obligation")
        .filter_map(|o| o["kind"].as_str())
        .collect();
    for prefix in [
        "inferred loop invariant not satisfied on entry: ",
        "inferred loop invariant not preserved by the loop body: ",
        "inferred termination measure not decreased: ",
    ] {
        assert!(kinds.iter().any(|k| k.starts_with(prefix)), "no {prefix:?} in {kinds:?}");
    }
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn memalias_calls() {
    check("tests/cases/memalias_calls.sol");
}

#[test]
fn memalias() {
    check("tests/cases/memalias.sol")
}

#[test]
fn fixedbytes() {
    check("tests/cases/fixedbytes.sol")
}

#[test]
fn hashing() {
    check("tests/cases/hashing.sol")
}

#[test]
fn rawstorage() {
    check("tests/cases/rawstorage.sol")
}

#[test]
fn storageptr() {
    check("tests/cases/storageptr.sol")
}

#[test]
fn infer_newton() {
    check("tests/cases/infer_newton.sol");
}

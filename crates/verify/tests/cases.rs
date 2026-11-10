#![allow(unused_crate_dependencies)]
#![allow(clippy::disallowed_methods)] // The tests read their Solidity sources directly.
//! Every function named ok_* must verify; every bad_* must fail.
use std::process::Command;

fn run(file: &str, strict: bool) -> String {
    let mut c = Command::new(env!("CARGO_BIN_EXE_solverus"));
    if strict {
        c.arg("--strict");
    }
    let out = c.arg(file).output().expect("run solverus");
    let s =
        String::from_utf8_lossy(&out.stdout).to_string() + &String::from_utf8_lossy(&out.stderr);
    // strip ANSI colour codes
    let mut o = String::new();
    let mut it = s.chars().peekable();
    while let Some(c) = it.next() {
        if c == '\x1b' {
            for d in it.by_ref() {
                if d == 'm' {
                    break;
                }
            }
        } else {
            o.push(c);
        }
    }
    o
}

/// The diagnostics (errors, warnings, notes) that name function `name`.
fn blocks_of(out: &str, name: &str) -> String {
    let mut blocks: Vec<String> = vec![];
    for l in out.lines() {
        let start = l.starts_with("error") || l.starts_with("warning") || l.starts_with("note");
        let end = l.starts_with('\u{2714}')
            || l.starts_with('\u{2718}')
            || l.starts_with("verification results");
        if start {
            blocks.push(String::new());
        } else if end {
            blocks.push(String::new());
            continue;
        }
        if let Some(b) = blocks.last_mut() {
            b.push_str(l);
            b.push('\n');
        }
    }
    let tag_in = "in ".to_string();
    blocks
        .into_iter()
        .filter(|b| {
            b.lines().any(|l| {
                let l = l.trim_start().trim_start_matches('=').trim();
                (l.starts_with(&tag_in) && l.ends_with(&format!(".{name}")))
                    || l.contains(&format!(".{name}: "))
            })
        })
        .collect::<Vec<_>>()
        .join("")
}

fn check(file: &str) {
    let src = std::fs::read_to_string(file).unwrap();
    let out = run(file, false);
    let mut problems = vec![];
    let lines: Vec<&str> = src.lines().collect();
    for (ln, line) in lines.iter().enumerate() {
        let Some(pos) = line.find("function ") else {
            continue;
        };
        let name: String =
            line[pos + 9..].chars().take_while(|c| c.is_alphanumeric() || *c == '_').collect();
        let want_ok = name.starts_with("ok_");
        // a function with `// expect:` lines above it must fail, whatever its name (an
        // implementation keeps its interface's name)
        let mut k0 = ln;
        let mut has_expect = false;
        while k0 > 0 && lines[k0 - 1].trim_start().starts_with("//") {
            k0 -= 1;
            has_expect |= lines[k0].trim_start().starts_with("// expect:");
        }
        if !want_ok && !name.starts_with("bad_") && !has_expect {
            continue;
        }
        let strict_only = name.contains("strict_only");
        let o = if strict_only { run(file, true) } else { out.clone() };
        let pass =
            o.lines().any(|l| l.starts_with('\u{2714}') && l.contains(&format!(".{name} (")));
        let fail =
            o.lines().any(|l| l.starts_with('\u{2718}') && l.contains(&format!(".{name} (")))
                || blocks_of(&o, &name).contains("unsupported:");
        // `// expect: text` comments right above the function pin what its output says (its
        // counterexample, the failing obligation, a note)
        let mut k = ln;
        while k > 0 && lines[k - 1].trim_start().starts_with("//") {
            k -= 1;
            if let Some(want) = lines[k].trim_start().strip_prefix("// expect:") {
                let want = want.trim();
                if !blocks_of(&o, &name).contains(want) {
                    problems.push(format!("{name}: output does not say `{want}`"));
                }
            }
        }
        if want_ok && !pass || !want_ok && !fail {
            problems
                .push(format!("{name}: expected {}", if want_ok { "verified" } else { "failure" }));
        }
    }
    assert!(problems.is_empty(), "{file}:\n{}\n--- output ---\n{out}", problems.join("\n"));
}

#[test]
fn arith() {
    if z3_missing() {
        return;
    }
    check("tests/cases/arith.sol")
}
#[test]
fn control() {
    if z3_missing() {
        return;
    }
    check("tests/cases/control.sol")
}
#[test]
fn storage() {
    if z3_missing() {
        return;
    }
    check("tests/cases/storage.sol")
}
#[test]
fn structs() {
    if z3_missing() {
        return;
    }
    check("tests/cases/structs.sol")
}
#[test]
fn reentrancy() {
    if z3_missing() {
        return;
    }
    check("tests/cases/reentrancy.sol")
}
#[test]
fn asm() {
    if z3_missing() {
        return;
    }
    check("tests/cases/asm.sol")
}

#[test]
fn examples() {
    if z3_missing() {
        return;
    }
    for (f, ok) in [
        ("examples/Counter.sol", true),
        ("examples/Token.sol", true),
        ("examples/ArrayMax.sol", true),
        ("examples/Vault.sol", true),
        ("examples/Bugs.sol", false),
    ] {
        let out = run(f, false);
        let summary =
            out.lines().find(|l| l.starts_with("verification results")).unwrap_or("").to_string();
        assert_eq!(summary.ends_with(" 0 errors"), ok, "{f}: {out}");
    }
}
#[test]
fn eth() {
    if z3_missing() {
        return;
    }
    check("tests/cases/eth.sol")
}
#[test]
fn features() {
    if z3_missing() {
        return;
    }
    check("tests/cases/features.sol")
}
#[test]
fn liveness() {
    if z3_missing() {
        return;
    }
    check("tests/cases/liveness.sol")
}
#[test]
fn speclang() {
    if z3_missing() {
        return;
    }
    check("tests/cases/speclang.sol")
}
#[test]
fn fixedarrays() {
    if z3_missing() {
        return;
    }
    check("tests/cases/fixedarrays.sol")
}
#[test]
fn lockinternal() {
    if z3_missing() {
        return;
    }
    check("tests/cases/lockinternal.sol")
}
#[test]
fn rewrites() {
    if z3_missing() {
        return;
    }
    check("tests/cases/rewrites.sol")
}
#[test]
fn vox() {
    if z3_missing() {
        return;
    }
    check("tests/cases/vox.sol")
}
/// --instrument writes every @custom:check as a require, and the result verifies alike.
#[test]
fn instrument() {
    if z3_missing() {
        return;
    }
    let out =
        std::env::temp_dir().join(format!("solverus-instrumented-{}.sol", std::process::id()));
    let st = Command::new(env!("CARGO_BIN_EXE_solverus"))
        .arg("--instrument")
        .arg(&out)
        .arg("tests/cases/vox.sol")
        .status()
        .unwrap();
    let _ = st;
    let src = std::fs::read_to_string(&out).unwrap();
    for want in [
        "{ require(amount <= 1000, \"solverus check failed: amount <= 1000\");",
        "require(x <= 1000, \"solverus check failed: x <= 1000\"); uint256 y = x;",
        "{ require(x > 3, \"solverus check failed: x > 3\"); return x; }",
    ] {
        assert!(src.contains(want), "instrumented source lacks {want}:\n{src}");
    }
    check(out.to_str().unwrap());
}

/// The verifier needs z3; without it (as in CI) the tests are skipped.
fn z3_missing() -> bool {
    let z3 = std::env::var("SOLVERUS_Z3").unwrap_or_else(|_| "z3".into());
    let missing = Command::new(z3).arg("-version").output().is_err();
    if missing {
        eprintln!("z3 not found; skipping (set SOLVERUS_Z3)");
    }
    missing
}

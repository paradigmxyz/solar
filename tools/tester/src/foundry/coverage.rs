//! Foundry coverage parity mode.
//!
//! Runs `forge coverage --report debug` for each project across a matrix of compiler legs and
//! compares the coverage items of the legs. Source mappings that the compiler drops or
//! misplaces show up as items that a leg does not anchor, or anchors but never hits. The mode
//! is local-only, like the external suite, and never runs in CI.
//!
//! Run it with `cargo tq foundry-coverage [project]`. The project is an in-repo project under
//! `tests/foundry` or a curated external project (see `external.rs`); without one, every
//! in-repo project runs, plus every external one when `SOLAR_FOUNDRY_COVERAGE_EXTERNAL=1`.
//!
//! Forge disables the optimizer and `viaIR` for coverage unless `--ir-minimum` is set. The
//! optimized legs need `forge coverage --keep-optimizer` and are skipped on a forge without it.
//!
//! Coverage differences never fail the run. A leg fails only when forge fails without a report,
//! except for a solc leg that hits solc's stack-too-deep limit, which is skipped.

use super::{
    ForgeCompiler, SOLAR_BINARY, TestConfig, combine_skips, discover_projects, external,
    forge_available, forge_version, foundry_root, foundry_solc, prebuild_profiles, report_dir,
    report_file_name, select_solar_binary, set_forge_compiler, workspace_root,
};
use regex::Regex;
use std::{
    collections::{BTreeMap, BTreeSet, HashMap},
    fs,
    panic::{AssertUnwindSafe, catch_unwind, resume_unwind},
    path::Path,
    process::Command,
    sync::OnceLock,
    thread,
    time::{Duration, Instant},
};

/// Fuzz and invariant limits, equal across legs. They stay low because coverage runs seven
/// legs per project.
const FUZZ_RUNS: u32 = 16;
const INVARIANT_RUNS: u32 = 4;
const INVARIANT_DEPTH: u32 = 16;

/// The settings a leg builds with.
#[derive(Clone, Copy)]
enum Settings {
    /// Forge's coverage default: no optimizer and no `viaIR`.
    Disabled,
    /// `--ir-minimum`: `viaIR` with minimum optimization.
    IrMinimum,
    /// `--keep-optimizer` with the optimizer on.
    Optimized { runs: u32, via_ir: bool },
}

/// One column of the matrix.
struct Leg {
    name: &'static str,
    compiler: ForgeCompiler,
    settings: Settings,
    /// Legs of the other compiler with comparable settings, whose coverage this leg should match.
    peers: &'static [&'static str],
}

/// The oracle: every leg is compared against it.
const BASELINE: &str = "solc-legacy";

const LEGS: [Leg; 7] = [
    Leg { name: BASELINE, compiler: ForgeCompiler::Solc, settings: Settings::Disabled, peers: &[] },
    Leg {
        name: "solc-legacy-opt",
        compiler: ForgeCompiler::Solc,
        settings: Settings::Optimized { runs: 200, via_ir: false },
        peers: &[],
    },
    Leg {
        name: "solc-ir-min",
        compiler: ForgeCompiler::Solc,
        settings: Settings::IrMinimum,
        peers: &[],
    },
    Leg {
        name: "solc-ir-opt",
        compiler: ForgeCompiler::Solc,
        settings: Settings::Optimized { runs: 200, via_ir: true },
        peers: &[],
    },
    Leg {
        name: "solar-none",
        compiler: ForgeCompiler::Solar,
        settings: Settings::Disabled,
        peers: &["solc-ir-min"],
    },
    Leg {
        name: "solar-gas",
        compiler: ForgeCompiler::Solar,
        settings: Settings::Optimized { runs: 200, via_ir: false },
        peers: &["solc-legacy-opt", "solc-ir-opt"],
    },
    Leg {
        name: "solar-size",
        compiler: ForgeCompiler::Solar,
        settings: Settings::Optimized { runs: 1, via_ir: false },
        peers: &["solc-legacy-opt", "solc-ir-opt"],
    },
];

// ============================================================================
// Debug report
// ============================================================================

/// An item's identity across legs: its source file, kind (with branch and path IDs, or the
/// function name), and byte range.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
struct ItemKey {
    file: String,
    kind: String,
    bytes: (usize, usize),
}

#[derive(Debug, Default, PartialEq, Eq)]
struct ItemData {
    lines: (usize, usize),
    /// Hits, summed over the builds that report the same item.
    hits: u64,
    /// Whether any contract's anchors reference the item.
    anchored: bool,
}

/// The items of one `forge coverage --report debug` run.
#[derive(Debug, Default)]
struct DebugReport {
    items: BTreeMap<ItemKey, ItemData>,
    /// Anchors whose item is not listed, or whose source ID names different files in different
    /// builds.
    unresolved_anchors: usize,
    /// Lines that start like a listed item but do not parse, which means the report format
    /// changed.
    unparsed_items: usize,
}

struct ParsedItem {
    source_id: u32,
    kind: String,
    lines: (usize, usize),
    bytes: (usize, usize),
    hits: u64,
}

fn item_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(concat!(
            r#"^(?<kind>Line|Statement|Branch \(branch: (?<branch>\d+), path: (?<path>\d+)\)"#,
            r#"|Function "(?<function>[^"]*)")"#,
            r" \(location: \(source ID: (?<source>\d+), lines: (?<l0>\d+)\.\.(?<l1>\d+),",
            r" bytes: (?<b0>\d+)\.\.(?<b1>\d+)\), hits: (?<hits>\d+)\)",
        ))
        .unwrap()
    })
}

fn anchor_regex() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| Regex::new(r"^- +(?:creation|runtime) IC \d+ -> Item \d+: (.*)$").unwrap())
}

/// Parses an item as the debug report prints it, without the `- ` prefix.
fn parse_item(text: &str) -> Option<ParsedItem> {
    let caps = item_regex().captures(text)?;
    let number = |name: &str| caps[name].parse::<usize>().ok();
    let kind = if let Some(function) = caps.name("function") {
        format!("Function {}", function.as_str())
    } else if let Some(branch) = caps.name("branch") {
        format!("Branch {}/{}", branch.as_str(), &caps["path"])
    } else {
        caps["kind"].to_string()
    };
    Some(ParsedItem {
        source_id: caps["source"].parse().ok()?,
        kind,
        lines: (number("l0")?, number("l1")?),
        bytes: (number("b0")?, number("b1")?),
        hits: caps["hits"].parse().ok()?,
    })
}

/// Parses the stdout of `forge coverage --report debug`.
///
/// The report lists every item under its file's path, then an `Anchors for Contract` section
/// per contract. Anchor lines carry the item and its source ID but not its file, so the file
/// comes from the listed item with the same source ID, kind, and byte range.
fn parse_debug_report(stdout: &str) -> DebugReport {
    let mut report = DebugReport::default();
    let mut file = None;
    let mut files_by_location = HashMap::<_, BTreeSet<&str>>::new();
    let mut in_anchors = false;
    let mut anchors = Vec::new();

    for line in stdout.lines() {
        if line.starts_with("Anchors for ") {
            in_anchors = true;
        } else if in_anchors {
            if let Some(caps) = anchor_regex().captures(line)
                && let Some(item) = parse_item(caps.get(1).unwrap().as_str())
            {
                anchors.push((item.source_id, item.kind, item.bytes));
            }
        } else if let Some(item) = line.strip_prefix("- ").and_then(parse_item) {
            let Some(file) = file else { continue };
            files_by_location
                .entry((item.source_id, item.kind.clone(), item.bytes))
                .or_default()
                .insert(file);
            let data = report
                .items
                .entry(ItemKey { file: file.to_string(), kind: item.kind, bytes: item.bytes })
                .or_default();
            if data.lines == (0, 0) {
                data.lines = item.lines;
            }
            data.hits += item.hits;
        } else if ["- Line ", "- Statement ", "- Branch ", "- Function "]
            .iter()
            .any(|prefix| line.starts_with(prefix))
        {
            report.unparsed_items += 1;
        } else if let Some(header) = line.strip_suffix(':')
            && !line.starts_with(['-', ' '])
        {
            file = Some(header);
        }
    }

    for (source_id, kind, bytes) in anchors {
        let location = (source_id, kind, bytes);
        let mut files = files_by_location.get(&location).into_iter().flatten();
        let (Some(file), None) = (files.next(), files.next()) else {
            report.unresolved_anchors += 1;
            continue;
        };
        let key = ItemKey { file: file.to_string(), kind: location.1, bytes };
        match report.items.get_mut(&key) {
            Some(data) => data.anchored = true,
            None => report.unresolved_anchors += 1,
        }
    }
    report
}

// ============================================================================
// Comparison
// ============================================================================

#[derive(Debug, Default, PartialEq, Eq)]
struct Totals {
    items: usize,
    anchored: usize,
    hit: usize,
}

/// How a leg's coverage differs from a reference leg, counted over the reference's items.
#[derive(Debug, Default, PartialEq, Eq)]
struct Diff {
    /// Items the reference anchors that the leg does not.
    unanchored: usize,
    /// Items the reference hits that the leg does not.
    unhit: usize,
    /// Items both hit, with different hit counts.
    mismatched: usize,
}

impl DebugReport {
    fn totals(&self) -> Totals {
        Totals {
            items: self.items.len(),
            anchored: self.items.values().filter(|data| data.anchored).count(),
            hit: self.items.values().filter(|data| data.hits > 0).count(),
        }
    }

    fn diff_from(&self, reference: &Self) -> Diff {
        let mut diff = Diff::default();
        for (key, base) in &reference.items {
            let leg = self.items.get(key);
            diff.unanchored += (base.anchored && !leg.is_some_and(|leg| leg.anchored)) as usize;
            let hits = leg.map_or(0, |leg| leg.hits);
            diff.unhit += (base.hits > 0 && hits == 0) as usize;
            diff.mismatched += (base.hits > 0 && hits > 0 && hits != base.hits) as usize;
        }
        diff
    }
}

// ============================================================================
// Running
// ============================================================================

enum Outcome {
    /// Forge produced a report.
    Reported {
        tests_failed: bool,
        report: DebugReport,
    },
    Skipped(String),
    Failed(String),
}

struct LegRun {
    leg: &'static Leg,
    outcome: Outcome,
    time: Duration,
}

impl LegRun {
    fn report(&self) -> Option<&DebugReport> {
        match &self.outcome {
            Outcome::Reported { report, .. } => Some(report),
            _ => None,
        }
    }

    fn status(&self) -> &'static str {
        match &self.outcome {
            Outcome::Reported { tests_failed: false, .. } => "ok",
            Outcome::Reported { tests_failed: true, .. } => "tests-failed",
            Outcome::Skipped(_) => "skipped",
            Outcome::Failed(_) => "failed",
        }
    }
}

/// Returns whether this forge accepts `forge coverage --keep-optimizer`.
fn forge_supports_keep_optimizer() -> bool {
    static SUPPORTED: OnceLock<bool> = OnceLock::new();
    *SUPPORTED.get_or_init(|| {
        Command::new("forge").args(["coverage", "--help"]).output().is_ok_and(|output| {
            String::from_utf8_lossy(&output.stdout).contains("--keep-optimizer")
        })
    })
}

/// Runs `forge coverage --report debug` for one leg of a project.
fn run_leg(config: &TestConfig, leg: &'static Leg) -> LegRun {
    let label = format!("{}/{}", config.name, leg.name);
    let start = Instant::now();
    let finish = |outcome| LegRun { leg, outcome, time: start.elapsed() };

    if matches!(leg.settings, Settings::Optimized { .. }) && !forge_supports_keep_optimizer() {
        let reason = "this forge lacks `forge coverage --keep-optimizer`".to_string();
        eprintln!("[{label}] SKIPPED: {reason}");
        return finish(Outcome::Skipped(reason));
    }

    let cache_dir = tempfile::Builder::new()
        .prefix(leg.compiler.cache_prefix())
        .tempdir()
        .expect("failed to create Foundry cache directory");
    let out_dir = tempfile::Builder::new()
        .prefix(leg.compiler.out_prefix())
        .tempdir()
        .expect("failed to create Foundry output directory");
    let foundry_solc = match leg.compiler {
        ForgeCompiler::Solar => Some(foundry_solc()),
        ForgeCompiler::Solc => None,
    };

    // Profiles the tests deploy from keep the project's own settings in every leg.
    if !prebuild_profiles(&config.path, &label, config, leg.compiler, foundry_solc.as_ref(), None) {
        return finish(Outcome::Failed("a prebuilt profile failed to build".to_string()));
    }

    let mut cmd = Command::new("forge");
    cmd.current_dir(&config.path)
        .args(["coverage", "--force", "--report", "debug"])
        .arg("--out")
        .arg(out_dir.path())
        .arg("--cache-path")
        .arg(cache_dir.path());
    if let Some(profile) = &config.foundry_profile {
        cmd.env("FOUNDRY_PROFILE", profile);
    }
    set_forge_compiler(&mut cmd, config, foundry_solc.as_ref(), None);
    cmd.env("FOUNDRY_FUZZ_RUNS", FUZZ_RUNS.to_string())
        .env("FOUNDRY_INVARIANT_RUNS", INVARIANT_RUNS.to_string())
        .env("FOUNDRY_INVARIANT_DEPTH", INVARIANT_DEPTH.to_string());
    match leg.settings {
        Settings::Disabled => {}
        Settings::IrMinimum => {
            cmd.arg("--ir-minimum");
        }
        Settings::Optimized { runs, via_ir } => {
            cmd.arg("--keep-optimizer")
                .env("FOUNDRY_OPTIMIZER", "true")
                .env("FOUNDRY_OPTIMIZER_RUNS", runs.to_string())
                .env("FOUNDRY_VIA_IR", via_ir.to_string());
        }
    }
    if let Some(pattern) = combine_skips(&config.skip_tests) {
        cmd.arg("--no-match-test").arg(pattern);
    }
    if let Some(pattern) = combine_skips(&config.skip_contracts) {
        cmd.arg("--no-match-contract").arg(pattern);
    }
    if let Some(seed) = config.fuzz_seed {
        cmd.arg("--fuzz-seed").arg(seed.to_string());
    }

    let output = match cmd.output() {
        Ok(output) => output,
        Err(error) => return finish(Outcome::Failed(format!("failed to run forge: {error}"))),
    };
    let report = parse_debug_report(&String::from_utf8_lossy(&output.stdout));
    if report.items.is_empty() && !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Solc cannot compile every project with every setting; that is its limit, not a leg
        // that failed to run. The compiler under test never gets this allowance.
        if matches!(leg.compiler, ForgeCompiler::Solc)
            && (stderr.contains("Stack too deep") || stderr.contains("too deep in the stack"))
        {
            let reason = "solc cannot compile this project with these settings: stack too deep";
            eprintln!("[{label}] SKIPPED: {reason}");
            return finish(Outcome::Skipped(reason.to_string()));
        }
        let mut tail = stderr.lines().rev().take(20).collect::<Vec<_>>();
        tail.reverse();
        return finish(Outcome::Failed(format!(
            "forge coverage reported no items ({}):\n{}",
            output.status,
            tail.join("\n")
        )));
    }
    if report.items.is_empty() && report.unparsed_items > 0 {
        return finish(Outcome::Failed(format!(
            "forge coverage printed {} item lines that did not parse; the debug report format \
             may have changed",
            report.unparsed_items
        )));
    }
    let tests_failed = !output.status.success();
    if tests_failed {
        eprintln!("[{label}] forge coverage exited with {}; using its report", output.status);
    }
    if report.unresolved_anchors > 0 {
        eprintln!(
            "[{label}] {} anchors name no listed item or a source ID that maps to several files",
            report.unresolved_anchors
        );
    }
    finish(Outcome::Reported { tests_failed, report })
}

/// Runs every leg of a project. Legs run in parallel unless the project prebuilds profiles into
/// its own directory, which the legs would overwrite for each other.
fn run_legs(config: &TestConfig) -> Vec<LegRun> {
    if !config.prebuild_profiles.is_empty() {
        return LEGS.iter().map(|leg| run_leg(config, leg)).collect();
    }
    thread::scope(|scope| {
        let handles =
            LEGS.iter().map(|leg| scope.spawn(move || run_leg(config, leg))).collect::<Vec<_>>();
        handles
            .into_iter()
            .map(|handle| handle.join().unwrap_or_else(|panic| resume_unwind(panic)))
            .collect()
    })
}

// ============================================================================
// Reporting
// ============================================================================

fn find<'a>(runs: &'a [LegRun], name: &str) -> Option<&'a DebugReport> {
    runs.iter().find(|run| run.leg.name == name)?.report()
}

fn print_summary(config: &TestConfig, runs: &[LegRun]) {
    println!("\n{}", "=".repeat(100));
    println!(" COVERAGE: {}", config.name);
    println!("{}", "=".repeat(100));
    println!(
        "{:<16} {:<12} {:>6} {:>8} {:>6} | vs {BASELINE}: {:>10} {:>6} {:>10}",
        "leg", "status", "items", "anchored", "hit", "unanchored", "unhit", "mismatched"
    );
    let baseline = find(runs, BASELINE);
    for run in runs {
        let Some(report) = run.report() else {
            println!("{:<16} {:<12}", run.leg.name, run.status());
            continue;
        };
        let totals = report.totals();
        print!(
            "{:<16} {:<12} {:>6} {:>8} {:>6} |",
            run.leg.name,
            run.status(),
            totals.items,
            totals.anchored,
            totals.hit
        );
        match baseline {
            Some(baseline) if run.leg.name != BASELINE => {
                let diff = report.diff_from(baseline);
                println!("{:>23} {:>6} {:>10}", diff.unanchored, diff.unhit, diff.mismatched);
            }
            Some(_) => println!(),
            None => println!("{:>23}", format!("no {BASELINE} report")),
        }
    }

    for run in runs {
        let Some(report) = run.report() else { continue };
        for &peer in run.leg.peers {
            match find(runs, peer) {
                Some(peer_report) => {
                    let diff = report.diff_from(peer_report);
                    println!(
                        "{} vs {peer}: unanchored {}, unhit {}, mismatched {}",
                        run.leg.name, diff.unanchored, diff.unhit, diff.mismatched
                    );
                }
                None => println!("{} vs {peer}: no {peer} report", run.leg.name),
            }
        }
    }
}

fn leg_json(run: &LegRun, runs: &[LegRun]) -> serde_json::Value {
    let mut json = serde_json::json!({ "status": run.status(), "time_ms": run.time.as_millis() });
    match &run.outcome {
        Outcome::Reported { report, .. } => {
            let totals = report.totals();
            json["items"] = totals.items.into();
            json["anchored"] = totals.anchored.into();
            json["hit"] = totals.hit.into();
            json["unresolved_anchors"] = report.unresolved_anchors.into();
            let references = std::iter::once(BASELINE).chain(run.leg.peers.iter().copied());
            let diffs = references
                .filter(|&reference| reference != run.leg.name)
                .filter_map(|reference| {
                    let diff = report.diff_from(find(runs, reference)?);
                    Some((
                        reference.to_string(),
                        serde_json::json!({
                            "unanchored": diff.unanchored,
                            "unhit": diff.unhit,
                            "mismatched": diff.mismatched,
                        }),
                    ))
                })
                .collect::<serde_json::Map<_, _>>();
            json["diff_from"] = diffs.into();
        }
        Outcome::Skipped(reason) | Outcome::Failed(reason) => {
            json["reason"] = reason.clone().into()
        }
    }
    json
}

/// Writes the project's JSON report: per-leg totals and differences, then every item with its
/// anchored flag and hits in each leg that reported. An item a leg does not list is `null` there.
fn write_report(config: &TestConfig, runs: &[LegRun]) {
    let dir = report_dir().unwrap_or_else(|| workspace_root().join("target/foundry-coverage"));
    fs::create_dir_all(&dir).expect("failed to create Foundry report directory");

    let reported =
        runs.iter().filter_map(|run| Some((run.leg.name, run.report()?))).collect::<Vec<_>>();
    let keys = reported.iter().flat_map(|(_, report)| report.items.keys()).collect::<BTreeSet<_>>();
    let items = keys
        .into_iter()
        .map(|key| {
            let lines = reported
                .iter()
                .find_map(|(_, report)| report.items.get(key))
                .map_or((0, 0), |data| data.lines);
            let legs = reported
                .iter()
                .map(|(name, report)| {
                    let data = report.items.get(key).map(
                        |data| serde_json::json!({ "anchored": data.anchored, "hits": data.hits }),
                    );
                    (name.to_string(), data.unwrap_or(serde_json::Value::Null))
                })
                .collect::<serde_json::Map<_, _>>();
            serde_json::json!({
                "file": key.file,
                "kind": key.kind,
                "bytes": [key.bytes.0, key.bytes.1],
                "lines": [lines.0, lines.1],
                "legs": legs,
            })
        })
        .collect::<Vec<_>>();

    let report = serde_json::json!({
        "forge_version": forge_version(),
        "project": { "name": config.name, "path": config.path.display().to_string() },
        "rerun": format!("cargo tq foundry-coverage {}", config.name),
        "baseline": BASELINE,
        "fuzz_runs": FUZZ_RUNS,
        "invariant_runs": INVARIANT_RUNS,
        "invariant_depth": INVARIANT_DEPTH,
        "legs": runs
            .iter()
            .map(|run| (run.leg.name.to_string(), leg_json(run, runs)))
            .collect::<serde_json::Map<_, _>>(),
        "items": items,
    });
    let path = dir.join(report_file_name(&format!("coverage-{}", config.name)));
    let json = serde_json::to_string_pretty(&report).expect("failed to serialize report");
    fs::write(&path, json).expect("failed to write the coverage report");
    println!("\nWrote {}", path.display());
}

// ============================================================================
// Suite
// ============================================================================

/// Runs the coverage matrix on the selected projects.
///
/// Panics when a leg cannot run; coverage differences only print.
pub fn run_coverage_suite(solar: &Path) {
    let _ = SOLAR_BINARY.set(select_solar_binary(solar));
    assert!(forge_available(), "forge not found in PATH");
    let selected = std::env::var("SOLAR_FOUNDRY_PROJECT").ok();
    let include_external =
        std::env::var_os("SOLAR_FOUNDRY_COVERAGE_EXTERNAL").is_some_and(|value| value != "0");
    // A selected name is an in-repo project, else an external one.
    let mut configs = discover_projects(&foundry_root());
    match (&selected, include_external) {
        (Some(_), _) if !configs.is_empty() => {}
        (Some(name), _) => configs = external::coverage_projects(Some(name)),
        (None, true) => configs.extend(external::coverage_projects(None)),
        (None, false) => {}
    }
    assert!(!configs.is_empty(), "no Foundry projects to measure");

    let mut failures = Vec::new();
    for config in &configs {
        let runs = catch_unwind(AssertUnwindSafe(|| {
            let runs = run_legs(config);
            print_summary(config, &runs);
            write_report(config, &runs);
            runs
        }));
        let Ok(runs) = runs else {
            failures.push(config.name.clone());
            continue;
        };
        for run in &runs {
            if let Outcome::Failed(reason) = &run.outcome {
                eprintln!("[{}/{}] FAILED: {reason}", config.name, run.leg.name);
                failures.push(format!("{}/{}", config.name, run.leg.name));
            }
        }
    }
    assert!(failures.is_empty(), "coverage legs failed: {}", failures.join(", "));
}

#[cfg(test)]
mod tests {
    use super::*;

    const REPORT: &str = r#"Ran 1 test suite in 1.2ms (0.5ms CPU time): 1 tests passed, 0 failed, 0 skipped (1 total tests)
src/A.sol:
- Line (location: (source ID: 1, lines: 10..16, bytes: 221..359), hits: 4) -> "function f() {}"
- Function "f" (location: (source ID: 1, lines: 10..16, bytes: 221..359), hits: 4) -> "function f() {}"
- Branch (branch: 11, path: 1) (location: (source ID: 1, lines: 11..14, bytes: 290..335), hits: 0) -> "if (x) {}"
- Statement (location: (source ID: 1, lines: 12..13, bytes: 316..324), hits: 2) -> "return 1"
- Statement (location: (source ID: 1, lines: 12..13, bytes: 316..324), hits: 3) -> "return 1"
src/B.sol:
- Statement (location: (source ID: 2, lines: 5..6, bytes: 7..9), hits: 1) -> "x"

Anchors for Contract "A" (solc 0.8.38, source ID 1):
- creation IC 12 -> Item 0: Line (location: (source ID: 1, lines: 10..16, bytes: 221..359), hits: 4)
-  runtime IC 550 -> Item 1: Function "f" (location: (source ID: 1, lines: 10..16, bytes: 221..359), hits: 4)
-  runtime IC 560 -> Item 2: Branch (branch: 11, path: 1) (location: (source ID: 1, lines: 11..14, bytes: 290..335), hits: 0)
-  runtime IC 570 -> Item 9: Statement (location: (source ID: 9, lines: 1..2, bytes: 1..2), hits: 0)

"#;

    fn key(file: &str, kind: &str, bytes: (usize, usize)) -> ItemKey {
        ItemKey { file: file.to_string(), kind: kind.to_string(), bytes }
    }

    #[test]
    fn parses_items_and_anchors() {
        let report = parse_debug_report(REPORT);
        let expected = [
            (key("src/A.sol", "Line", (221, 359)), (10, 16), 4, true),
            (key("src/A.sol", "Function f", (221, 359)), (10, 16), 4, true),
            (key("src/A.sol", "Branch 11/1", (290, 335)), (11, 14), 0, true),
            // Duplicates of one location sum their hits.
            (key("src/A.sol", "Statement", (316, 324)), (12, 13), 5, false),
            (key("src/B.sol", "Statement", (7, 9)), (5, 6), 1, false),
        ];
        assert_eq!(report.items.len(), expected.len());
        for (key, lines, hits, anchored) in expected {
            assert_eq!(report.items[&key], ItemData { lines, hits, anchored }, "{key:?}");
        }
        // The last anchor names a source ID that no listed item has.
        assert_eq!(report.unresolved_anchors, 1);
        assert_eq!(report.totals(), Totals { items: 5, anchored: 3, hit: 4 });
    }

    #[test]
    fn counts_differences_over_the_reference_items() {
        let reference = parse_debug_report(REPORT);
        let other = parse_debug_report(
            r#"src/A.sol:
- Line (location: (source ID: 1, lines: 10..16, bytes: 221..359), hits: 4) -> ""
- Statement (location: (source ID: 1, lines: 12..13, bytes: 316..324), hits: 0) -> ""
- Branch (branch: 11, path: 1) (location: (source ID: 1, lines: 11..14, bytes: 290..335), hits: 0) -> ""
src/B.sol:
- Statement (location: (source ID: 2, lines: 5..6, bytes: 7..9), hits: 2) -> ""

Anchors for Contract "A" (solc 0.8.38, source ID 1):
- creation IC 12 -> Item 0: Line (location: (source ID: 1, lines: 10..16, bytes: 221..359), hits: 4)
"#,
        );
        // The function is missing (unanchored and unhit), the branch lost its anchor, the
        // statement is never hit, and B's statement has another count.
        assert_eq!(other.diff_from(&reference), Diff { unanchored: 2, unhit: 2, mismatched: 1 });
    }
}

//! External Foundry project suite.
//!
//! Curated real-world Foundry projects, fetched at pinned revisions and run
//! with `forge test` under both compilers so that each project's own test
//! suite acts as a differential oracle against solc. The suite is local-only:
//! it fetches from the network, takes long, and is never run in CI.
//!
//! Run it with `cargo tq foundry-external [name]`. Checkouts live under
//! `target/foundry-external/checkouts` and are reused offline once fetched.
//! `SOLAR_FOUNDRY_EXTERNAL_MANIFEST` replaces the curated list with a TOML
//! manifest of out-of-repo projects.

use super::{
    AssertionPolicy, SOLAR_BINARY, SkipEntry, TestConfig, forge_available, workspace_root,
};
use std::{
    collections::HashSet,
    fs,
    panic::{AssertUnwindSafe, catch_unwind},
    path::{Path, PathBuf},
    process::Command,
};

/// Fixed fuzz seed so both compiler legs run identical fuzz and invariant
/// inputs; forge randomizes the seed per run otherwise, making differential
/// results flaky.
const EXTERNAL_FUZZ_SEED: u64 = 100;

/// How an external project is exercised.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
enum ExternalMode {
    /// Compile with `forge build` and audit artifacts only.
    Build,
    /// Run the project's own test suite differentially.
    Test,
}

/// A skipped test or contract pattern; the reason is mandatory.
struct Skip {
    pattern: &'static str,
    reason: &'static str,
}

/// A curated external project pinned to a full commit hash.
struct ExternalProject {
    name: &'static str,
    repo: &'static str,
    rev: &'static str,
    mode: ExternalMode,
    /// Solc version to emulate on the Solar leg, for projects whose sources
    /// pin an exact solc version that forge's resolver checks pragmas against.
    solc_version: Option<&'static str>,
    skip_tests: &'static [Skip],
    skip_contracts: &'static [Skip],
    notes: &'static str,
}

/// The curated corpus. Dependencies must come from the projects' own git
/// submodules: `forge install` or npm/soldeer dependency managers would fetch
/// unpinned revisions and break reproducibility, so projects that need them
/// stay out of this list (use the TOML manifest for those).
const EXTERNAL_PROJECTS: &[ExternalProject] = &[
    ExternalProject {
        name: "morpho-blue",
        repo: "https://github.com/morpho-org/morpho-blue",
        rev: "d09dd1c4b9c7d9d05f976faa7ebfdc424dae5e8c",
        mode: ExternalMode::Test,
        solc_version: Some("0.8.19"),
        skip_tests: &[],
        skip_contracts: &[],
        notes: "lending core: exact 0.8.19 pragma, invariant suite, evm paris",
    },
    ExternalProject {
        name: "solmate",
        repo: "https://github.com/transmissions11/solmate",
        rev: "89365b880c4f3c786bdd453d4b8e8fe410344a69",
        mode: ExternalMode::Test,
        // The test files pin `pragma solidity 0.8.15` exactly.
        solc_version: Some("0.8.15"),
        skip_tests: &[],
        skip_contracts: &[],
        notes: "token/utility library: heavy fuzz coverage of arithmetic edge cases",
    },
    ExternalProject {
        name: "solady",
        repo: "https://github.com/Vectorized/solady",
        rev: "cedd7936a11807acd819c9f6acf48fdcefee3f73",
        mode: ExternalMode::Test,
        solc_version: None,
        skip_tests: &[],
        skip_contracts: &[],
        notes: "assembly-heavy library: the widest inline-assembly coverage available",
    },
    // prb-math was considered but is excluded: its forge-std comes from
    // npm/bun (`devDependencies`), not a git submodule. Run it through
    // `SOLAR_FOUNDRY_EXTERNAL_MANIFEST` with `path=` after `bun install`.
    ExternalProject {
        name: "seaport",
        repo: "https://github.com/ProjectOpenSea/seaport",
        rev: "080133906585660f6a76b82984f3fb690ff4b2a9",
        mode: ExternalMode::Build,
        // `contracts/Seaport.sol` pins `pragma solidity =0.8.24`.
        solc_version: Some("0.8.24"),
        skip_tests: &[],
        skip_contracts: &[],
        notes: "build-only: whole-project codegen, artifact parity and EIP-170 tracker",
    },
    ExternalProject {
        name: "openzeppelin-contracts",
        repo: "https://github.com/OpenZeppelin/openzeppelin-contracts",
        rev: "f646874fdc9b151631e3c96a68defbdbe736cd53",
        mode: ExternalMode::Test,
        solc_version: None,
        skip_tests: &[],
        skip_contracts: &[],
        notes: "divergence tracker: broadest idiomatic Solidity surface; needs a forge that knows evm osaka",
    },
    ExternalProject {
        name: "uniswap-v4-core",
        repo: "https://github.com/Uniswap/v4-core",
        rev: "46c6834698c48bc4a463a86d8420f4eb1d7f3b75",
        mode: ExternalMode::Test,
        // `src/PoolManager.sol` pins `pragma solidity =0.8.26`.
        solc_version: Some("0.8.26"),
        skip_tests: &[],
        skip_contracts: &[],
        notes: "divergence tracker: transient storage, via-ir profile, ffi gas snapshots",
    },
];

// ============================================================================
// Manifest resolution
// ============================================================================

/// Where a project's sources come from.
#[derive(Debug)]
enum ProjectSource {
    /// Fetched from `repo` at the pinned `rev` into the checkouts directory.
    Fetch { repo: String, rev: String },
    /// An existing local directory; never fetched.
    Local(PathBuf),
}

/// A project resolved to owned data, from the curated list or a TOML manifest.
struct ResolvedProject {
    name: String,
    source: ProjectSource,
    mode: ExternalMode,
    solc_version: Option<String>,
    skip_tests: Vec<SkipEntry>,
    skip_contracts: Vec<SkipEntry>,
    notes: String,
}

/// Root of a `SOLAR_FOUNDRY_EXTERNAL_MANIFEST` TOML file.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestFile {
    #[serde(default)]
    project: Vec<ManifestProject>,
}

/// One `[[project]]` manifest entry: either `repo` + `rev` (fetched) or
/// `path` (local directory, resolved relative to the manifest file).
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestProject {
    name: String,
    repo: Option<String>,
    rev: Option<String>,
    path: Option<PathBuf>,
    #[serde(default = "default_mode")]
    mode: ExternalMode,
    solc_version: Option<String>,
    #[serde(default)]
    skip_tests: Vec<ManifestSkip>,
    #[serde(default)]
    skip_contracts: Vec<ManifestSkip>,
    #[serde(default)]
    notes: String,
}

fn default_mode() -> ExternalMode {
    ExternalMode::Test
}

/// A skip entry in a TOML manifest; the reason is mandatory there as well.
#[derive(Debug, serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct ManifestSkip {
    pattern: String,
    reason: String,
}

impl ResolvedProject {
    fn from_curated(project: &ExternalProject) -> Self {
        assert_full_sha(project.name, project.rev);
        Self {
            name: project.name.to_string(),
            source: ProjectSource::Fetch {
                repo: project.repo.to_string(),
                rev: project.rev.to_string(),
            },
            mode: project.mode,
            solc_version: project.solc_version.map(str::to_string),
            skip_tests: skip_entries(project.skip_tests),
            skip_contracts: skip_entries(project.skip_contracts),
            notes: project.notes.to_string(),
        }
    }

    fn from_manifest(project: ManifestProject, manifest_dir: &Path) -> Self {
        let source = match (project.repo, project.rev, project.path) {
            (Some(repo), Some(rev), None) => {
                assert_full_sha(&project.name, &rev);
                ProjectSource::Fetch { repo, rev }
            }
            (None, None, Some(path)) => {
                let path = if path.is_absolute() { path } else { manifest_dir.join(path) };
                ProjectSource::Local(path)
            }
            _ => panic!(
                "[{}] external manifest entries need either `repo` + `rev` or `path`",
                project.name
            ),
        };
        Self {
            name: project.name,
            source,
            mode: project.mode,
            solc_version: project.solc_version,
            skip_tests: manifest_skips(project.skip_tests),
            skip_contracts: manifest_skips(project.skip_contracts),
            notes: project.notes,
        }
    }

    fn test_config(&self, path: PathBuf) -> TestConfig {
        TestConfig {
            name: self.name.clone(),
            path,
            test_filter: None,
            contract_filter: None,
            solar_only: false,
            ignored: false,
            skip_tests: self.skip_tests.clone(),
            skip_contracts: self.skip_contracts.clone(),
            policy: AssertionPolicy::ExternalDifferential,
            build_only: self.mode == ExternalMode::Build,
            fuzz_seed: Some(EXTERNAL_FUZZ_SEED),
            solc_wrapper_version: self.solc_version.clone(),
            traces: false,
            rerun_command: format!("cargo tq foundry-external {}", self.name),
        }
    }
}

fn skip_entries(skips: &[Skip]) -> Vec<SkipEntry> {
    skips
        .iter()
        .map(|skip| SkipEntry {
            pattern: skip.pattern.to_string(),
            reason: skip.reason.to_string(),
        })
        .collect()
}

fn manifest_skips(skips: Vec<ManifestSkip>) -> Vec<SkipEntry> {
    skips.into_iter().map(|skip| SkipEntry { pattern: skip.pattern, reason: skip.reason }).collect()
}

fn assert_full_sha(name: &str, rev: &str) {
    assert!(
        rev.len() == 40 && rev.bytes().all(|b| b.is_ascii_hexdigit()),
        "[{name}] external project revs must be full 40-character commit hashes, got `{rev}`"
    );
}

fn resolve_projects() -> Vec<ResolvedProject> {
    let projects = match std::env::var_os("SOLAR_FOUNDRY_EXTERNAL_MANIFEST") {
        Some(path) => load_manifest(Path::new(&path)),
        None => EXTERNAL_PROJECTS.iter().map(ResolvedProject::from_curated).collect(),
    };
    let mut seen = HashSet::new();
    for project in &projects {
        assert!(
            seen.insert(project.name.as_str()),
            "duplicate external project name `{}`",
            project.name
        );
    }
    projects
}

fn load_manifest(path: &Path) -> Vec<ResolvedProject> {
    let text = fs::read_to_string(path).unwrap_or_else(|error| {
        panic!("failed to read external manifest {}: {error}", path.display())
    });
    let manifest: ManifestFile = toml_edit::de::from_str(&text).unwrap_or_else(|error| {
        panic!("failed to parse external manifest {}: {error}", path.display())
    });
    let manifest_dir = path.parent().unwrap_or(Path::new("."));
    manifest
        .project
        .into_iter()
        .map(|project| ResolvedProject::from_manifest(project, manifest_dir))
        .collect()
}

// ============================================================================
// Fetching
// ============================================================================

/// Marker file recording the fetched revision; written only after a fully
/// successful fetch, including submodules.
const CHECKOUT_OK_FILE: &str = ".solar-checkout-ok";

fn checkouts_root() -> PathBuf {
    workspace_root().join("target/foundry-external/checkouts")
}

/// Ensures `name` is checked out at `rev`, fetching it if needed.
///
/// Uses zero network when the previous fetch completed and HEAD still matches.
/// Fetches by sha (`git fetch --depth 1 origin <rev>`) instead of cloning:
/// clones cannot target arbitrary pinned revisions. Dependencies come from the
/// project's own submodules.
fn ensure_checkout(name: &str, repo: &str, rev: &str) -> Result<PathBuf, String> {
    let dir = checkouts_root().join(name);
    let marker = dir.join(CHECKOUT_OK_FILE);
    if let Ok(previous) = fs::read_to_string(&marker)
        && previous.trim() == rev
        && git_output(&dir, &["rev-parse", "HEAD"]).is_ok_and(|head| head.trim() == rev)
    {
        println!("[{name}] using cached checkout at {rev}");
        return Ok(dir);
    }

    // Stale, partial, or absent: refetch from scratch.
    if dir.exists() {
        fs::remove_dir_all(&dir)
            .map_err(|error| format!("failed to remove stale checkout: {error}"))?;
    }
    fs::create_dir_all(&dir)
        .map_err(|error| format!("failed to create checkout directory: {error}"))?;

    println!("[{name}] fetching {repo} @ {rev}");
    git(&dir, &["init", "--quiet"])?;
    git(&dir, &["remote", "add", "origin", repo])?;
    git(&dir, &["fetch", "--quiet", "--depth", "1", "origin", rev])?;
    git(&dir, &["checkout", "--quiet", "--detach", "FETCH_HEAD"])?;
    // Shallow submodules save most of the transfer, but some servers refuse
    // shallow fetches of pinned submodule commits; retry without the cap.
    if git(&dir, &["submodule", "update", "--init", "--recursive", "--depth", "1"]).is_err() {
        git(&dir, &["submodule", "update", "--init", "--recursive"])?;
    }
    fs::write(&marker, rev).map_err(|error| format!("failed to write checkout marker: {error}"))?;
    Ok(dir)
}

fn git(dir: &Path, args: &[&str]) -> Result<(), String> {
    git_output(dir, args).map(drop)
}

fn git_output(dir: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .current_dir(dir)
        .args(args)
        // Fail fast instead of prompting for credentials on bad URLs.
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map_err(|error| format!("failed to spawn git {}: {error}", args.join(" ")))?;
    if !output.status.success() {
        return Err(format!(
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

// ============================================================================
// Suite runner
// ============================================================================

/// Runs the external Foundry suite.
///
/// Fetch failures skip the affected project so offline runs degrade instead of
/// failing; every other divergence is judged by [`AssertionPolicy::ExternalDifferential`].
pub(super) fn run_external_suite(solar: &Path) {
    let _ = SOLAR_BINARY.set(solar.to_path_buf());

    if !forge_available() {
        eprintln!("Skipping external Foundry suite: forge not found in PATH");
        return;
    }

    let projects = resolve_projects();
    assert!(!projects.is_empty(), "No external Foundry projects configured");

    let selected = std::env::var("SOLAR_FOUNDRY_PROJECT").ok();
    if let Some(selected) = &selected
        && !projects.iter().any(|project| project.name == *selected)
    {
        panic!(
            "unknown external project `{selected}`; known projects: {}",
            projects.iter().map(|project| project.name.as_str()).collect::<Vec<_>>().join(", ")
        );
    }

    let mut failures = Vec::new();
    for project in &projects {
        if selected.as_deref().is_some_and(|selected| selected != project.name) {
            continue;
        }
        println!("\n### [{}] {}", project.name, project.notes);
        let dir = match &project.source {
            ProjectSource::Local(path) => {
                assert!(
                    path.is_dir(),
                    "[{}] local manifest path does not exist: {}",
                    project.name,
                    path.display()
                );
                path.clone()
            }
            ProjectSource::Fetch { repo, rev } => match ensure_checkout(&project.name, repo, rev) {
                Ok(dir) => dir,
                Err(reason) => {
                    eprintln!("[{}] SKIPPED (fetch): {reason}", project.name);
                    continue;
                }
            },
        };
        let config = project.test_config(dir);
        if catch_unwind(AssertUnwindSafe(|| config.run())).is_err() {
            failures.push(project.name.clone());
        }
    }
    assert!(failures.is_empty(), "External Foundry projects failed: {}", failures.join(", "));
}

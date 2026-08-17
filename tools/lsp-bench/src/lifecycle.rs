//! Preparation and reproducibility checks for benchmark inputs.

use crate::{
    config::{
        ArtifactSpec, CompilerSpec, Config, FixtureSpec, InstallSpec, ServerSpec, SourceSpec,
    },
    fixture::FixtureSource,
    process::{cgroup_v2_process_tree_available, network_isolation_available},
};
use anyhow::{Context, Result, bail};
use serde::Serialize;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeSet,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const REQUIRED_FIXTURES: [&str; 4] = ["synthetic", "v4-core", "aave-v3-origin", "optimism-bedrock"];
pub(crate) const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);
const PIP_CLOSURE_DOWNLOAD_ARGS: [&str; 5] =
    ["download", "--no-cache-dir", "--no-deps", "--require-hashes", "--dest"];

pub(crate) struct PrepareOptions {
    pub(crate) config: PathBuf,
    pub(crate) servers: BTreeSet<String>,
    pub(crate) fixtures: BTreeSet<String>,
    pub(crate) prepare_servers: bool,
}

pub(crate) struct DoctorOptions {
    pub(crate) config: PathBuf,
    pub(crate) publish: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum CheckStatus {
    Pass,
    Unavailable,
    Mismatch,
    Unpinned,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct Check {
    pub(crate) kind: String,
    pub(crate) id: String,
    pub(crate) status: CheckStatus,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct DoctorReport {
    pub(crate) publish: bool,
    pub(crate) checks: Vec<Check>,
}

impl DoctorReport {
    fn is_publishable(&self) -> bool {
        self.checks.iter().all(|check| matches!(check.status, CheckStatus::Pass))
    }
}

pub(crate) fn prepare(options: PrepareOptions) -> Result<DoctorReport> {
    let config = Config::load(&options.config)?;
    let manifest_dir = absolute_manifest_dir(&options.config)?;
    for fixture in config
        .fixtures
        .iter()
        .filter(|fixture| options.fixtures.is_empty() || options.fixtures.contains(&fixture.id))
    {
        if !fixture.root.exists() {
            let source = fixture.source.as_ref().with_context(|| {
                format!("fixture `{}` is missing and has no source checkout", fixture.id)
            })?;
            prepare_checkout(source, &fixture.root)?;
        }
        prepare_submodules(&fixture.root)?;
        prepare_fixture_artifacts(fixture)?;
    }
    for server in config.servers.iter().filter(|server| {
        options.prepare_servers
            && (options.servers.is_empty() || options.servers.contains(&server.id))
    }) {
        prepare_server(server, &manifest_dir)?;
    }
    doctor(DoctorOptions { config: options.config, publish: false })
}

pub(crate) fn doctor(options: DoctorOptions) -> Result<DoctorReport> {
    let config = Config::load(&options.config)?;
    let manifest_dir = absolute_manifest_dir(&options.config)?;
    let mut checks = Vec::new();
    validate_inventory(&config, &mut checks);
    for server in &config.servers {
        if let Some(source) = &server.source {
            checks.push(check_source_checkout(
                &server.id,
                source,
                &server_source_root(&manifest_dir, &server.id),
            ));
        }
        if let Some(check) = check_install_manifest(server) {
            checks.push(check);
        }
        if let Some(check) = check_installed_closure(server, &manifest_dir) {
            checks.push(check);
        }
        checks.extend(check_server(server));
    }
    for fixture in &config.fixtures {
        checks.push(check_fixture(fixture));
        if let Some(solc) = &fixture.solc {
            checks.extend(check_compiler("solc", &fixture.id, solc));
        }
        if let Some(foundry) = &fixture.foundry {
            checks.extend(check_compiler("foundry", &fixture.id, foundry));
        }
    }
    if options.publish {
        checks.extend(publish_environment_checks(&options.config));
    }
    let report = DoctorReport { publish: options.publish, checks };
    if options.publish && !report.is_publishable() {
        bail!("benchmark environment is not publishable")
    }
    Ok(report)
}

pub(crate) fn render_doctor(report: &DoctorReport) -> String {
    let mut output = String::from("Kind\tID\tStatus\tDetail\n");
    for check in &report.checks {
        output.push_str(&format!(
            "{}\t{}\t{}\t{}\n",
            check.kind,
            check.id,
            match check.status {
                CheckStatus::Pass => "pass",
                CheckStatus::Unavailable => "unavailable",
                CheckStatus::Mismatch => "mismatch",
                CheckStatus::Unpinned => "unpinned",
            },
            check.detail
        ));
    }
    output
}

fn validate_inventory(config: &Config, checks: &mut Vec<Check>) {
    let fixtures =
        config.fixtures.iter().map(|fixture| fixture.id.as_str()).collect::<BTreeSet<_>>();
    if config.servers.is_empty() {
        checks.push(inventory_check("server-inventory", "manifest", false));
    } else {
        for server in &config.servers {
            checks.push(inventory_check("server-inventory", &server.id, true));
        }
    }
    for id in REQUIRED_FIXTURES {
        checks.push(inventory_check("fixture-inventory", id, fixtures.contains(id)));
    }
}

fn inventory_check(kind: &str, id: &str, present: bool) -> Check {
    Check {
        kind: kind.into(),
        id: id.into(),
        status: if present { CheckStatus::Pass } else { CheckStatus::Mismatch },
        detail: if present { "declared".into() } else { "required entry is missing".into() },
    }
}

fn check_source_checkout(id: &str, source: &SourceSpec, root: &Path) -> Check {
    if !root.is_dir() {
        return Check {
            kind: "server-source".into(),
            id: id.into(),
            status: CheckStatus::Unavailable,
            detail: format!("source checkout `{}` was not found", root.display()),
        };
    }
    let actual_revision = match git_output(root, &["rev-parse", "HEAD"]) {
        Ok(revision) => revision,
        Err(error) => {
            return Check {
                kind: "server-source".into(),
                id: id.into(),
                status: CheckStatus::Unavailable,
                detail: format!("{error:#}"),
            };
        }
    };
    if actual_revision != source.revision {
        return Check {
            kind: "server-source".into(),
            id: id.into(),
            status: CheckStatus::Mismatch,
            detail: format!("expected revision {}, found {actual_revision}", source.revision),
        };
    }
    let actual_url = git_output(root, &["remote", "get-url", "origin"]);
    if actual_url.as_ref().ok().map(String::as_str) != Some(source.url.as_str()) {
        return Check {
            kind: "server-source".into(),
            id: id.into(),
            status: CheckStatus::Mismatch,
            detail: format!("expected origin `{}`, found {actual_url:?}", source.url),
        };
    }
    let dirty = git_output(root, &["status", "--porcelain", "--untracked-files=normal"])
        .map_or(true, |status| !status.is_empty());
    Check {
        kind: "server-source".into(),
        id: id.into(),
        status: if dirty { CheckStatus::Mismatch } else { CheckStatus::Pass },
        detail: if dirty { "source checkout is dirty".into() } else { actual_revision },
    }
}

fn check_server(server: &ServerSpec) -> Vec<Check> {
    if !server.enabled {
        return vec![Check {
            kind: "server".into(),
            id: server.id.clone(),
            status: if server.required { CheckStatus::Mismatch } else { CheckStatus::Pass },
            detail: if server.required {
                "required server is disabled".into()
            } else {
                "optional server is disabled".into()
            },
        }];
    }
    let executable = resolve_executable(&server.command);
    if !executable.is_file() {
        return vec![Check {
            kind: "server".into(),
            id: server.id.clone(),
            status: CheckStatus::Unavailable,
            detail: format!("executable `{}` was not found", server.command.display()),
        }];
    }
    let Some(artifact) = &server.artifact else {
        return vec![Check {
            kind: "server".into(),
            id: server.id.clone(),
            status: CheckStatus::Unpinned,
            detail: "artifact digest is not declared".into(),
        }];
    };
    let artifact_check = if artifact.sha256.is_none()
        && server.source.as_ref().is_some_and(|source| is_full_git_revision(&source.revision))
        && artifact.path.exists()
    {
        match sha256_path(&artifact.path) {
            Ok(actual) => Check {
                kind: "server-source-build".into(),
                id: server.id.clone(),
                status: CheckStatus::Pass,
                detail: format!("executed artifact {actual}"),
            },
            Err(error) => Check {
                kind: "server-source-build".into(),
                id: server.id.clone(),
                status: CheckStatus::Unavailable,
                detail: format!("{error:#}"),
            },
        }
    } else {
        check_artifact("server-artifact", &server.id, artifact)
    };
    if !matches!(artifact_check.status, CheckStatus::Pass) {
        return vec![artifact_check];
    }
    let executable_check = match sha256_path(&executable) {
        Ok(actual) => Check {
            kind: "server-executable".into(),
            id: server.id.clone(),
            status: CheckStatus::Pass,
            detail: actual,
        },
        Err(error) => Check {
            kind: "server-executable".into(),
            id: server.id.clone(),
            status: CheckStatus::Unavailable,
            detail: format!("{error:#}"),
        },
    };
    let version = inspect_version(&executable, server, VERSION_PROBE_TIMEOUT).and_then(|version| {
        verify_server_version_output(server, &version)?;
        Ok(version)
    });
    let version = match version {
        Ok(version) => version,
        Err(error) => {
            return vec![
                artifact_check,
                executable_check,
                Check {
                    kind: "server-version".into(),
                    id: server.id.clone(),
                    status: CheckStatus::Mismatch,
                    detail: format!("{error:#}"),
                },
            ];
        }
    };
    vec![
        artifact_check,
        executable_check,
        Check {
            kind: "server-version".into(),
            id: server.id.clone(),
            status: CheckStatus::Pass,
            detail: version,
        },
    ]
}

fn check_install_manifest(server: &ServerSpec) -> Option<Check> {
    let install = server.install.as_ref()?;
    let manifest = server_install_manifest_path(install)?;
    Some(check_file_digest(
        "server-install-manifest",
        &server.id,
        &manifest,
        install.manifest_sha256.as_deref(),
    ))
}

fn check_installed_closure(server: &ServerSpec, manifest_dir: &Path) -> Option<Check> {
    let root = server_installed_closure_root(server, manifest_dir)?;
    let receipt = server_installed_closure_receipt(manifest_dir, &server.id);
    let expected = match fs::read_to_string(&receipt) {
        Ok(expected) => expected.trim().to_owned(),
        Err(error) => {
            return Some(Check {
                kind: "server-install-closure".into(),
                id: server.id.clone(),
                status: CheckStatus::Unavailable,
                detail: format!("failed to read `{}`: {error}", receipt.display()),
            });
        }
    };
    let actual = match sha256_installed_closure(&root) {
        Ok(actual) => actual,
        Err(error) => {
            return Some(Check {
                kind: "server-install-closure".into(),
                id: server.id.clone(),
                status: CheckStatus::Unavailable,
                detail: format!("{error:#}"),
            });
        }
    };
    Some(Check {
        kind: "server-install-closure".into(),
        id: server.id.clone(),
        status: if expected.len() == 64
            && expected.bytes().all(|byte| byte.is_ascii_hexdigit())
            && expected.eq_ignore_ascii_case(&actual)
        {
            CheckStatus::Pass
        } else {
            CheckStatus::Mismatch
        },
        detail: if expected.eq_ignore_ascii_case(&actual) {
            actual
        } else {
            format!("expected {expected}, found {actual}")
        },
    })
}

pub(crate) fn server_install_manifest_path(install: &InstallSpec) -> Option<PathBuf> {
    let manifest = install.manifest.as_deref()?;
    match install.kind.as_str() {
        "npm" => Some(manifest.join("package-lock.json")),
        "pip" => Some(manifest.to_owned()),
        _ => None,
    }
}

/// Revalidate immutable server inputs immediately before benchmark execution.
///
/// `prepare` and `doctor` provide the complete audit trail, but a later mutation must not let a
/// direct `run` execute a different source checkout or package closure.
pub(crate) fn verify_server_runtime_inputs(server: &ServerSpec, manifest_dir: &Path) -> Result<()> {
    if let Some(source) = &server.source {
        require_passing_check(check_source_checkout(
            &server.id,
            source,
            &server_source_root(manifest_dir, &server.id),
        ))?;
    }
    if let Some(check) = check_install_manifest(server) {
        require_passing_check(check)?;
    }
    if let Some(check) = check_installed_closure(server, manifest_dir) {
        require_passing_check(check)?;
    }
    Ok(())
}

fn require_passing_check(check: Check) -> Result<()> {
    if matches!(check.status, CheckStatus::Pass) {
        return Ok(());
    }
    bail!("{} `{}` failed: {}", check.kind, check.id, check.detail)
}

fn is_full_git_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn check_artifact(kind: &str, id: &str, artifact: &ArtifactSpec) -> Check {
    if !artifact.path.exists() {
        return Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Unavailable,
            detail: format!("artifact `{}` was not found", artifact.path.display()),
        };
    }
    let Some(expected) = artifact.sha256.as_deref() else {
        return Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Unpinned,
            detail: "artifact SHA-256 is not declared".into(),
        };
    };
    match sha256_path(&artifact.path) {
        Ok(actual) if actual == expected => {
            Check { kind: kind.into(), id: id.into(), status: CheckStatus::Pass, detail: actual }
        }
        Ok(actual) => Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Mismatch,
            detail: format!("expected {expected}, found {actual}"),
        },
        Err(error) => Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Unavailable,
            detail: format!("{error:#}"),
        },
    }
}

fn check_compiler(kind: &str, fixture: &str, compiler: &CompilerSpec) -> Vec<Check> {
    let mut checks = Vec::new();
    if let Some(path) = &compiler.native {
        let artifact = check_file_digest(
            &format!("{kind}-native"),
            fixture,
            path,
            compiler.native_sha256.as_deref(),
        );
        let verified = matches!(artifact.status, CheckStatus::Pass);
        checks.push(artifact);
        if verified {
            checks.push(check_compiler_version(kind, fixture, compiler));
        }
    }
    if let Some(path) = &compiler.soljson {
        checks.push(check_file_digest(
            &format!("{kind}-soljson"),
            fixture,
            path,
            compiler.soljson_sha256.as_deref(),
        ));
    }
    if compiler.archive_url.is_some() {
        let archive = compiler_archive_path(compiler);
        checks.push(check_file_digest(
            &format!("{kind}-archive"),
            fixture,
            &archive,
            compiler.archive_sha256.as_deref(),
        ));
    }
    if checks.is_empty() {
        checks.push(Check {
            kind: kind.into(),
            id: fixture.into(),
            status: CheckStatus::Unpinned,
            detail: format!("{} has no declared artifact paths", compiler.version),
        });
    }
    checks
}

fn check_compiler_version(kind: &str, fixture: &str, compiler: &CompilerSpec) -> Check {
    match inspect_compiler_version(kind, compiler, VERSION_PROBE_TIMEOUT) {
        Ok(Some(version)) => Check {
            kind: format!("{kind}-version"),
            id: fixture.into(),
            status: CheckStatus::Pass,
            detail: version,
        },
        Ok(None) => Check {
            kind: format!("{kind}-version"),
            id: fixture.into(),
            status: CheckStatus::Unpinned,
            detail: "native compiler artifact is not declared".into(),
        },
        Err(error) => Check {
            kind: format!("{kind}-version"),
            id: fixture.into(),
            status: CheckStatus::Mismatch,
            detail: format!("{error:#}"),
        },
    }
}

fn check_file_digest(kind: &str, id: &str, path: &Path, expected: Option<&str>) -> Check {
    if !path.exists() {
        return Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Unavailable,
            detail: format!("artifact `{}` was not found", path.display()),
        };
    }
    let Some(expected) = expected else {
        return Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Unpinned,
            detail: format!("artifact `{}` has no SHA-256", path.display()),
        };
    };
    match sha256_path(path) {
        Ok(actual) if actual == expected => {
            Check { kind: kind.into(), id: id.into(), status: CheckStatus::Pass, detail: actual }
        }
        Ok(actual) => Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Mismatch,
            detail: format!("expected {expected}, found {actual}"),
        },
        Err(error) => Check {
            kind: kind.into(),
            id: id.into(),
            status: CheckStatus::Unavailable,
            detail: format!("{error:#}"),
        },
    }
}

fn compiler_archive_path(compiler: &CompilerSpec) -> PathBuf {
    compiler
        .native
        .as_deref()
        .and_then(Path::parent)
        .unwrap_or_else(|| Path::new("."))
        .join("archive.tar.gz")
}

fn prepare_fixture_artifacts(fixture: &FixtureSpec) -> Result<()> {
    if let Some(solc) = &fixture.solc {
        prepare_compiler("solc", &fixture.id, solc)?;
    }
    if let Some(foundry) = &fixture.foundry {
        prepare_compiler("foundry", &fixture.id, foundry)?;
    }
    Ok(())
}

fn prepare_compiler(kind: &str, fixture: &str, compiler: &CompilerSpec) -> Result<()> {
    if let Some(url) = &compiler.archive_url {
        let archive = compiler_archive_path(compiler);
        download_verified(url, &archive, compiler.archive_sha256.as_deref())?;
        let parent = archive.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent)?;
        let status = Command::new("tar")
            .args(["-xzf"])
            .arg(&archive)
            .args(["--no-same-owner", "--no-same-permissions", "-C"])
            .arg(parent)
            .status()
            .with_context(|| format!("failed to extract {kind} archive `{}`", archive.display()))?;
        if !status.success() {
            bail!("failed to extract {kind} archive `{}`", archive.display())
        }
        let native = compiler.native.as_deref().context("archive has no native artifact path")?;
        let expected = compiler
            .native_sha256
            .as_deref()
            .context("archive has no extracted native artifact SHA-256")?;
        let actual = sha256_path(native).with_context(|| {
            format!("failed to hash extracted {kind} artifact `{}`", native.display())
        })?;
        if actual != expected {
            bail!(
                "extracted {kind} artifact `{}` has SHA-256 {actual}, expected {expected}",
                native.display()
            )
        }
    }
    if let Some(path) = &compiler.native {
        if let Some(url) = &compiler.native_url {
            download_verified(url, path, compiler.native_sha256.as_deref())?;
        } else if !path.exists() {
            bail!("{kind} compiler `{}` for fixture `{fixture}` is missing", path.display())
        }
        make_executable(path)?;
    }
    if let Some(path) = &compiler.soljson {
        if let Some(url) = &compiler.soljson_url {
            download_verified(url, path, compiler.soljson_sha256.as_deref())?;
        } else if !path.exists() {
            bail!("{kind} soljson `{}` for fixture `{fixture}` is missing", path.display())
        }
    }
    Ok(())
}

fn download_verified(url: &str, destination: &Path, expected_sha256: Option<&str>) -> Result<()> {
    if destination.exists() {
        if let Some(expected) = expected_sha256 {
            if sha256_path(destination).is_ok_and(|actual| actual == expected) {
                return Ok(());
            }
            bail!("existing artifact `{}` does not match declared SHA-256", destination.display())
        }
        return Ok(());
    }
    if expected_sha256.is_none() {
        bail!("download `{url}` has no SHA-256 pin")
    }
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    let temporary = destination.with_extension("download.tmp");
    let status = Command::new("curl")
        .args(["--fail", "--location", "--retry", "3", "--silent", "--show-error"])
        .arg(url)
        .args(["--output"])
        .arg(&temporary)
        .status()
        .with_context(|| format!("failed to download `{url}`"))?;
    if !status.success() {
        let _ = fs::remove_file(&temporary);
        bail!("download `{url}` exited with {status}")
    }
    let actual = sha256_path(&temporary)?;
    if Some(actual.as_str()) != expected_sha256 {
        let _ = fs::remove_file(&temporary);
        bail!("download `{url}` has SHA-256 {actual}, expected {expected_sha256:?}")
    }
    fs::rename(&temporary, destination)?;
    Ok(())
}

#[cfg(unix)]
fn make_executable(path: &Path) -> Result<()> {
    let mut permissions = fs::metadata(path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn make_executable(_: &Path) -> Result<()> {
    Ok(())
}

pub(crate) fn resolve_executable(path: &Path) -> PathBuf {
    if path.is_absolute() || path.components().count() > 1 {
        return path.to_path_buf();
    }
    let Some(paths) = std::env::var_os("PATH") else { return path.to_path_buf() };
    std::env::split_paths(&paths)
        .map(|directory| directory.join(path))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| path.to_path_buf())
}

pub(crate) fn inspect_version(
    command: &Path,
    spec: &ServerSpec,
    timeout: Duration,
) -> Result<String> {
    if spec.version_args.is_empty() {
        let package_json = spec
            .artifact
            .as_ref()
            .map(|artifact| artifact.path.join("package.json"))
            .filter(|path| path.is_file())
            .with_context(|| {
                format!(
                    "server `{}` has no version command or observable installed package metadata",
                    spec.id
                )
            })?;
        let value: serde_json::Value = serde_json::from_slice(&fs::read(&package_json)?)
            .with_context(|| format!("failed to parse `{}`", package_json.display()))?;
        return value
            .get("version")
            .and_then(serde_json::Value::as_str)
            .map(str::to_owned)
            .with_context(|| format!("`{}` has no package version", package_json.display()));
    }
    let mut process = Command::new(command);
    process
        .args(&spec.version_args)
        .env_remove("RUST_LOG")
        .env_remove("SOLAR_PROFILE")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    for (key, value) in &spec.env {
        process.env(key, value);
    }
    inspect_version_command(process, command, timeout)
}

/// Run a compiler version probe and require the configured version in its output.
///
/// The native executable is the artifact linked into each isolated server environment, so this
/// check binds the configured compiler version to the digest checked before the probe.
pub(crate) fn inspect_compiler_version(
    kind: &str,
    compiler: &CompilerSpec,
    timeout: Duration,
) -> Result<Option<String>> {
    let Some(command) = compiler.native.as_deref() else { return Ok(None) };
    let mut process = Command::new(command);
    process
        .arg("--version")
        .env_remove("RUST_LOG")
        .env_remove("SOLAR_PROFILE")
        .env("NO_COLOR", "1")
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let actual = inspect_version_command(process, command, timeout)
        .with_context(|| format!("failed to inspect {kind} compiler `{}`", command.display()))?;
    if !version_output_contains(&compiler.version, &actual) {
        bail!("{kind} compiler version mismatch: expected `{}`, found `{actual}`", compiler.version)
    }
    Ok(Some(actual))
}

fn inspect_version_command(
    mut process: Command,
    command: &Path,
    timeout: Duration,
) -> Result<String> {
    let mut child =
        process.spawn().with_context(|| format!("failed to run `{}`", command.display()))?;
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            let output = child.wait_with_output()?;
            if !status.success() {
                bail!("version command exited with {status}")
            }
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            let value = if stdout.trim().is_empty() { stderr.trim() } else { stdout.trim() };
            if value.is_empty() {
                bail!("version command produced no output")
            }
            return Ok(value.to_owned());
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!("timed out waiting for version command")
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub(crate) fn verify_server_version_output(server: &ServerSpec, actual: &str) -> Result<()> {
    if let Some(expected) = &server.expected_version
        && actual.trim() != expected.trim()
    {
        bail!("version mismatch: expected `{expected}`, found `{actual}`")
    }
    if let Some(expected) = &server.locked_version
        && !version_output_contains(expected, actual)
    {
        bail!("locked version `{expected}` was not found in `{actual}`")
    }
    Ok(())
}

fn version_output_contains(expected: &str, actual: &str) -> bool {
    actual
        .split(|character: char| {
            !character.is_ascii_alphanumeric() && character != '.' && character != '-'
        })
        .any(|token| {
            token == expected
                || token.strip_prefix('v') == Some(expected)
                || token.starts_with(&format!("{expected}+"))
                || token.starts_with(&format!("{expected}-"))
        })
}

fn check_fixture(fixture: &FixtureSpec) -> Check {
    if !fixture.enabled {
        return Check {
            kind: "fixture".into(),
            id: fixture.id.clone(),
            status: if fixture.required { CheckStatus::Mismatch } else { CheckStatus::Pass },
            detail: if fixture.required {
                "required fixture is disabled".into()
            } else {
                "optional fixture is disabled".into()
            },
        };
    }
    match FixtureSource::open(fixture) {
        Ok(source) => Check {
            kind: "fixture".into(),
            id: fixture.id.clone(),
            status: CheckStatus::Pass,
            detail: format!("{} Solidity files", source.metadata().source_file_count),
        },
        Err(error) => Check {
            kind: "fixture".into(),
            id: fixture.id.clone(),
            status: CheckStatus::Unavailable,
            detail: format!(
                "{} fixture: {error:#}",
                if fixture.required { "required" } else { "optional" }
            ),
        },
    }
}

fn publish_environment_checks(config_path: &Path) -> Vec<Check> {
    let mut checks = vec![Check {
        kind: "environment".into(),
        id: "platform".into(),
        status: if cfg!(all(target_os = "linux", target_arch = "x86_64")) {
            CheckStatus::Pass
        } else {
            CheckStatus::Mismatch
        },
        detail: format!("{}-{}", std::env::consts::ARCH, std::env::consts::OS),
    }];
    checks.push(match cgroup_v2_process_tree_available() {
        Ok(path) => Check {
            kind: "environment".into(),
            id: "cgroup-v2-process-tree".into(),
            status: CheckStatus::Pass,
            detail: format!("delegated under `{}`", path.parent().unwrap_or(&path).display()),
        },
        Err(error) => Check {
            kind: "environment".into(),
            id: "cgroup-v2-process-tree".into(),
            status: CheckStatus::Unavailable,
            detail: format!("{error:#}"),
        },
    });
    checks.push(match network_isolation_available() {
        Ok(()) => Check {
            kind: "environment".into(),
            id: "network-namespace".into(),
            status: CheckStatus::Pass,
            detail: "unprivileged network namespace available".into(),
        },
        Err(error) => Check {
            kind: "environment".into(),
            id: "network-namespace".into(),
            status: CheckStatus::Unavailable,
            detail: format!("{error:#}"),
        },
    });
    let root = repository_root(config_path).unwrap_or_else(|| PathBuf::from("."));
    let clean = git_output(&root, &["status", "--porcelain", "--untracked-files=normal"])
        .is_ok_and(|status| status.is_empty());
    checks.push(Check {
        kind: "environment".into(),
        id: "git-clean".into(),
        status: if clean { CheckStatus::Pass } else { CheckStatus::Mismatch },
        detail: root.display().to_string(),
    });
    checks
}

fn prepare_server(server: &ServerSpec, manifest_dir: &Path) -> Result<()> {
    let source_root = server_source_root(manifest_dir, &server.id);
    if let Some(source) = &server.source {
        prepare_checkout(source, &source_root)?;
    }
    let Some(install) = &server.install else { return Ok(()) };
    if install.kind == "none" {
        return Ok(());
    }
    let artifact_root = server_artifact_root(manifest_dir, &server.id);
    fs::create_dir_all(&artifact_root)?;
    match install.kind.as_str() {
        "npm" => {
            let manifest = install
                .manifest
                .as_deref()
                .with_context(|| format!("server `{}` npm manifest is missing", server.id))?;
            let expected_sha256 = install.manifest_sha256.as_deref().with_context(|| {
                format!("server `{}` npm manifest SHA-256 is missing", server.id)
            })?;
            verify_file_sha256(
                &manifest.join("package-lock.json"),
                expected_sha256,
                "npm package lock",
            )?;
            for name in ["package.json", "package-lock.json"] {
                let source = manifest.join(name);
                if !source.is_file() {
                    bail!("server `{}` npm manifest is missing `{}`", server.id, source.display())
                }
                fs::copy(&source, artifact_root.join(name)).with_context(|| {
                    format!("failed to stage server `{}` npm `{name}`", server.id)
                })?;
            }
            let status = Command::new("npm")
                .args(["ci", "--prefix"])
                .arg(&artifact_root)
                .args(["--ignore-scripts", "--no-audit", "--no-fund"])
                .current_dir(manifest)
                .stdin(Stdio::null())
                .status()
                .with_context(|| format!("failed to install server `{}`", server.id))?;
            if !status.success() {
                bail!("server `{}` install command exited with {status}", server.id)
            }
        }
        "pip" => prepare_pip_server(server, install, &artifact_root)?,
        _ => {
            let program = install
                .command
                .as_deref()
                .with_context(|| format!("server `{}` install command is missing", server.id))?;
            let args = install
                .args
                .iter()
                .map(|arg| {
                    arg.replace("{source}", &source_root.display().to_string())
                        .replace("{target}", &artifact_root.display().to_string())
                })
                .collect::<Vec<_>>();
            let status = Command::new(program)
                .args(args)
                .current_dir(if source_root.is_dir() { &source_root } else { manifest_dir })
                .stdin(Stdio::null())
                .status()
                .with_context(|| format!("failed to install server `{}`", server.id))?;
            if !status.success() {
                bail!("server `{}` install command exited with {status}", server.id)
            }
        }
    }
    if let Some(artifact) = &server.artifact
        && let Some(expected) = artifact.sha256.as_deref()
    {
        let actual = sha256_path(&artifact.path).with_context(|| {
            format!("failed to hash installed artifact `{}`", artifact.path.display())
        })?;
        if actual != expected {
            bail!(
                "installed artifact `{}` has SHA-256 {actual}, expected {expected}",
                artifact.path.display()
            )
        }
    }
    if server_installed_closure_root(server, manifest_dir).is_some() {
        let digest = sha256_installed_closure(&artifact_root).with_context(|| {
            format!("failed to hash server `{}` installed dependency closure", server.id)
        })?;
        let receipt = server_installed_closure_receipt(manifest_dir, &server.id);
        if let Some(parent) = receipt.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&receipt, format!("{digest}\n")).with_context(|| {
            format!("failed to write installed closure receipt `{}`", receipt.display())
        })?;
    }
    Ok(())
}

fn prepare_pip_server(
    server: &ServerSpec,
    install: &InstallSpec,
    artifact_root: &Path,
) -> Result<()> {
    let manifest = install
        .manifest
        .as_deref()
        .with_context(|| format!("server `{}` pip manifest is missing", server.id))?;
    let expected_sha256 = install
        .manifest_sha256
        .as_deref()
        .with_context(|| format!("server `{}` pip manifest SHA-256 is missing", server.id))?;
    verify_file_sha256(manifest, expected_sha256, "pip manifest")?;

    if std::env::consts::OS != "linux" || std::env::consts::ARCH != "x86_64" {
        bail!(
            "server `{}` pip lock targets x86_64 Linux, found {} {}",
            server.id,
            std::env::consts::OS,
            std::env::consts::ARCH
        )
    }
    let python = install
        .command
        .as_deref()
        .with_context(|| format!("server `{}` Python command is missing", server.id))?;
    let expected_python = install
        .python_version
        .as_deref()
        .with_context(|| format!("server `{}` Python version is missing", server.id))?;
    let actual_python = inspect_python_minor(python)?;
    if actual_python != expected_python {
        bail!(
            "server `{}` pip lock requires Python {expected_python}, found {actual_python}",
            server.id
        )
    }

    let wheelhouse = artifact_root.join("wheelhouse");
    reset_directory(&wheelhouse)?;
    // The lock includes hashed source distributions for packages without Python 3.12 wheels.
    let status = pip_command(python)
        .args(PIP_CLOSURE_DOWNLOAD_ARGS)
        .arg(&wheelhouse)
        .args(["--requirement"])
        .arg(manifest)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to download server `{}` Python closure", server.id))?;
    if !status.success() {
        bail!("server `{}` Python closure download exited with {status}", server.id)
    }

    let venv = artifact_root.join("venv");
    if venv.exists() {
        fs::remove_dir_all(&venv)
            .with_context(|| format!("failed to remove stale venv `{}`", venv.display()))?;
    }
    let status = Command::new(python)
        .args(["-m", "venv"])
        .arg(&venv)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to create server `{}` Python venv", server.id))?;
    if !status.success() {
        bail!("server `{}` Python venv creation exited with {status}", server.id)
    }

    let venv_python = venv.join("bin/python");
    let status = pip_command(&venv_python)
        .args([
            "install",
            "--no-cache-dir",
            "--no-index",
            "--no-deps",
            "--require-hashes",
            "--find-links",
        ])
        .arg(&wheelhouse)
        .args(["--requirement"])
        .arg(manifest)
        .env("PIP_NO_INDEX", "1")
        .env("PIP_FIND_LINKS", &wheelhouse)
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to install server `{}` Python closure", server.id))?;
    if !status.success() {
        bail!("server `{}` offline Python install exited with {status}", server.id)
    }
    let status = pip_command(&venv_python)
        .arg("check")
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to check server `{}` Python closure", server.id))?;
    if !status.success() {
        bail!("server `{}` Python dependency check exited with {status}", server.id)
    }
    Ok(())
}

fn pip_command(command: impl AsRef<Path>) -> Command {
    let mut process = Command::new(command.as_ref());
    process.args(["-m", "pip", "--disable-pip-version-check"]).env("PIP_CONFIG_FILE", "/dev/null");
    process
}

fn inspect_python_minor(command: &str) -> Result<String> {
    let output = Command::new(command)
        .args(["-c", "import sys; print(f'{sys.version_info.major}.{sys.version_info.minor}')"])
        .stdin(Stdio::null())
        .output()
        .with_context(|| format!("failed to inspect Python command `{command}`"))?;
    if !output.status.success() {
        bail!("Python version command `{command}` exited with {}", output.status)
    }
    let version = String::from_utf8(output.stdout).context("Python version is not UTF-8")?;
    let version = version.trim();
    if version.is_empty() {
        bail!("Python version command `{command}` produced no output")
    }
    Ok(version.to_owned())
}

fn verify_file_sha256(path: &Path, expected: &str, kind: &str) -> Result<()> {
    if !path.is_file() {
        bail!("{kind} `{}` was not found", path.display())
    }
    let actual = sha256_path(path)?;
    if actual != expected {
        bail!("{kind} `{}` has SHA-256 {actual}, expected {expected}", path.display())
    }
    Ok(())
}

fn reset_directory(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_dir_all(path)
            .with_context(|| format!("failed to remove stale directory `{}`", path.display()))?;
    }
    fs::create_dir_all(path)
        .with_context(|| format!("failed to create directory `{}`", path.display()))
}

fn server_source_root(manifest_dir: &Path, id: &str) -> PathBuf {
    manifest_dir.join("../../target/lsp-bench/sources/servers").join(id)
}

fn server_artifact_root(manifest_dir: &Path, id: &str) -> PathBuf {
    manifest_dir.join("../../target/lsp-bench/servers").join(id)
}

fn server_installed_closure_root(server: &ServerSpec, manifest_dir: &Path) -> Option<PathBuf> {
    server
        .install
        .as_ref()
        .is_some_and(|install| matches!(install.kind.as_str(), "npm" | "pip"))
        .then(|| server_artifact_root(manifest_dir, &server.id))
}

fn server_installed_closure_receipt(manifest_dir: &Path, id: &str) -> PathBuf {
    manifest_dir
        .join("../../target/lsp-bench/provenance/installed-closures")
        .join(format!("{id}.sha256"))
}

pub(crate) fn absolute_manifest_dir(config_path: &Path) -> Result<PathBuf> {
    let manifest_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    if manifest_dir.is_absolute() {
        Ok(manifest_dir.to_path_buf())
    } else {
        Ok(std::env::current_dir()?.join(manifest_dir))
    }
}

fn prepare_checkout(source: &SourceSpec, destination: &Path) -> Result<()> {
    if destination.join(".git").is_dir() {
        run_git(destination, &["fetch", "--depth=1", "origin", &source.revision])?;
    } else {
        if destination.exists() {
            bail!(
                "checkout destination `{}` exists but is not a Git repository",
                destination.display()
            )
        }
        if let Some(parent) = destination.parent() {
            fs::create_dir_all(parent)?;
        }
        let destination_arg = destination.to_string_lossy().into_owned();
        run_git(
            Path::new("."),
            &["clone", "--filter=blob:none", "--no-checkout", &source.url, &destination_arg],
        )?;
    }
    run_git(destination, &["checkout", "--detach", &source.revision])?;
    let actual = git_output(destination, &["rev-parse", "HEAD"])?;
    if actual != source.revision {
        bail!(
            "checkout `{}` resolved to `{actual}`, expected `{}`",
            destination.display(),
            source.revision
        )
    }
    Ok(())
}

fn prepare_submodules(root: &Path) -> Result<()> {
    if !root.join(".git").exists() {
        return Ok(());
    }
    run_git(root, &["submodule", "sync", "--recursive"])?;
    run_git(root, &["submodule", "update", "--init", "--recursive"])
}

fn run_git(root: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git").arg("-C").arg(root).args(args).status()?;
    if !status.success() {
        bail!("Git command failed in `{}` with {status}", root.display())
    }
    Ok(())
}

fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git").arg("-C").arg(root).args(args).output()?;
    if !output.status.success() {
        bail!("Git command failed in `{}`", root.display())
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn repository_root(path: &Path) -> Option<PathBuf> {
    let root = path.parent().unwrap_or_else(|| Path::new("."));
    git_output(root, &["rev-parse", "--show-toplevel"]).ok().map(PathBuf::from)
}

pub(crate) fn sha256_path(path: &Path) -> Result<String> {
    let mut hasher = Sha256::new();
    if path.is_file() {
        hash_file(path, &mut hasher)?;
    } else if path.is_dir() {
        let mut files = Vec::new();
        collect_files(path, path, &mut files)?;
        files.sort();
        for relative in files {
            hasher.update(relative.to_string_lossy().as_bytes());
            hasher.update([0]);
            hash_file(&path.join(relative), &mut hasher)?;
        }
    } else {
        bail!("artifact `{}` is not a file or directory", path.display())
    }
    Ok(format!("{:x}", hasher.finalize()))
}

enum InstalledClosureEntry {
    File(PathBuf),
    Symlink { path: PathBuf, target: PathBuf },
}

impl InstalledClosureEntry {
    fn path(&self) -> &Path {
        match self {
            Self::File(path) | Self::Symlink { path, .. } => path,
        }
    }
}

fn sha256_installed_closure(root: &Path) -> Result<String> {
    if !fs::symlink_metadata(root)?.file_type().is_dir() {
        bail!("installed closure `{}` is not a directory", root.display())
    }
    let mut entries = Vec::new();
    collect_installed_closure_entries(root, root, &mut entries)?;
    entries.sort_by(|left, right| left.path().cmp(right.path()));

    let mut hasher = Sha256::new();
    hasher.update(b"solar-lsp-bench-installed-closure-v1\0");
    for entry in entries {
        match entry {
            InstalledClosureEntry::File(path) => {
                hasher.update(b"file\0");
                hash_encoded_bytes(path.as_os_str().as_encoded_bytes(), &mut hasher);
                let path = root.join(path);
                hasher.update(fs::metadata(&path)?.len().to_le_bytes());
                hash_file(&path, &mut hasher)?;
            }
            InstalledClosureEntry::Symlink { path, target } => {
                hasher.update(b"symlink\0");
                hash_encoded_bytes(path.as_os_str().as_encoded_bytes(), &mut hasher);
                hash_encoded_bytes(target.as_os_str().as_encoded_bytes(), &mut hasher);
            }
        }
    }
    Ok(format!("{:x}", hasher.finalize()))
}

fn hash_encoded_bytes(bytes: &[u8], hasher: &mut Sha256) {
    hasher.update(bytes.len().to_le_bytes());
    hasher.update(bytes);
}

fn collect_installed_closure_entries(
    root: &Path,
    directory: &Path,
    entries: &mut Vec<InstalledClosureEntry>,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;
        if file_type.is_dir() {
            collect_installed_closure_entries(root, &path, entries)?;
        } else if file_type.is_file() {
            entries.push(InstalledClosureEntry::File(path.strip_prefix(root)?.to_path_buf()));
        } else if file_type.is_symlink() {
            entries.push(InstalledClosureEntry::Symlink {
                path: path.strip_prefix(root)?.to_path_buf(),
                target: fs::read_link(&path)?,
            });
        } else {
            bail!("installed closure contains unsupported entry `{}`", path.display())
        }
    }
    Ok(())
}

fn collect_files(root: &Path, directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_files(root, &path, files)?;
        } else if entry.file_type()?.is_file() {
            files.push(path.strip_prefix(root)?.to_path_buf());
        }
    }
    Ok(())
}

fn hash_file(path: &Path, hasher: &mut Sha256) -> Result<()> {
    let mut file = fs::File::open(path)?;
    let mut buffer = [0; 64 * 1024];
    loop {
        let read = file.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;
    use std::collections::BTreeMap;

    #[cfg(unix)]
    use std::os::unix::fs::symlink;

    #[test]
    fn server_inventory_is_derived_from_the_manifest() {
        let mut server = server_spec();
        server.id = "manifest-server".into();
        let mut config = Config {
            schema_version: crate::config::SCHEMA_VERSION,
            config_sha256: String::new(),
            profiles: BTreeMap::new(),
            scenarios: Vec::new(),
            servers_lock_sha256: None,
            fixtures_lock_sha256: None,
            servers: vec![server],
            fixtures: Vec::new(),
            workloads: Vec::new(),
        };
        let mut checks = Vec::new();
        validate_inventory(&config, &mut checks);
        let server_checks =
            checks.iter().filter(|check| check.kind == "server-inventory").collect::<Vec<_>>();
        assert_eq!(server_checks.len(), 1);
        assert_eq!(server_checks[0].id, "manifest-server");
        assert!(matches!(server_checks[0].status, CheckStatus::Pass));

        config.servers.clear();
        checks.clear();
        validate_inventory(&config, &mut checks);
        let missing = checks.iter().find(|check| check.kind == "server-inventory").unwrap();
        assert!(matches!(missing.status, CheckStatus::Mismatch));
    }

    #[test]
    fn installer_roots_are_absolute_before_changing_directory() {
        let manifest_dir =
            absolute_manifest_dir(Path::new("tools/lsp-bench/benchmark.yaml")).unwrap();
        assert!(manifest_dir.is_absolute());
        assert!(server_source_root(&manifest_dir, "server").is_absolute());
    }

    #[test]
    fn directory_digest_is_stable_and_path_sensitive() {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("a")).unwrap();
        fs::write(root.path().join("a/file"), "value").unwrap();
        let first = sha256_path(root.path()).unwrap();
        let second = sha256_path(root.path()).unwrap();
        assert_eq!(first, second);
        fs::rename(root.path().join("a/file"), root.path().join("file")).unwrap();
        assert_ne!(first, sha256_path(root.path()).unwrap());
    }

    #[test]
    fn installed_closure_receipt_rejects_dependency_mutation() {
        let root = tempfile::tempdir().unwrap();
        let manifest_dir = root.path().join("tools/lsp-bench");
        fs::create_dir_all(&manifest_dir).unwrap();
        let mut server = server_spec();
        server.install = Some(InstallSpec {
            kind: "npm".into(),
            command: None,
            args: Vec::new(),
            manifest: None,
            manifest_sha256: None,
            python_version: None,
            target: None,
        });
        let closure = server_artifact_root(&manifest_dir, &server.id);
        fs::create_dir_all(closure.join("node_modules/dependency")).unwrap();
        let dependency = closure.join("node_modules/dependency/index.js");
        fs::write(&dependency, "first").unwrap();
        let receipt = server_installed_closure_receipt(&manifest_dir, &server.id);
        fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        fs::write(&receipt, format!("{}\n", sha256_installed_closure(&closure).unwrap())).unwrap();

        assert!(matches!(
            check_installed_closure(&server, &manifest_dir).unwrap().status,
            CheckStatus::Pass
        ));
        fs::write(dependency, "changed").unwrap();
        assert!(matches!(
            check_installed_closure(&server, &manifest_dir).unwrap().status,
            CheckStatus::Mismatch
        ));
        let error = verify_server_runtime_inputs(&server, &manifest_dir).unwrap_err().to_string();
        assert!(error.contains("server-install-closure"), "{error}");
    }

    #[test]
    fn installed_closure_digest_frames_tree_entries() {
        let one_file = tempfile::tempdir().unwrap();
        fs::write(one_file.path().join("a"), b"xb\0file\0y").unwrap();

        let two_files = tempfile::tempdir().unwrap();
        fs::write(two_files.path().join("a"), b"x").unwrap();
        fs::write(two_files.path().join("b"), b"y").unwrap();

        assert_ne!(
            sha256_installed_closure(one_file.path()).unwrap(),
            sha256_installed_closure(two_files.path()).unwrap()
        );
    }

    #[cfg(unix)]
    #[test]
    fn installed_closure_receipt_rejects_symlink_retargeting() {
        let root = tempfile::tempdir().unwrap();
        let manifest_dir = root.path().join("tools/lsp-bench");
        fs::create_dir_all(&manifest_dir).unwrap();
        let mut server = server_spec();
        server.install = Some(InstallSpec {
            kind: "npm".into(),
            command: None,
            args: Vec::new(),
            manifest: None,
            manifest_sha256: None,
            python_version: None,
            target: None,
        });
        let closure = server_artifact_root(&manifest_dir, &server.id);
        fs::create_dir_all(closure.join("node_modules/.bin")).unwrap();
        for package in ["first", "second"] {
            let package = closure.join("node_modules").join(package);
            fs::create_dir_all(&package).unwrap();
            fs::write(package.join("server.js"), package.display().to_string()).unwrap();
        }
        let executable = closure.join("node_modules/.bin/server");
        symlink("../first/server.js", &executable).unwrap();
        let receipt = server_installed_closure_receipt(&manifest_dir, &server.id);
        fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        fs::write(&receipt, format!("{}\n", sha256_installed_closure(&closure).unwrap())).unwrap();

        assert!(matches!(
            check_installed_closure(&server, &manifest_dir).unwrap().status,
            CheckStatus::Pass
        ));
        fs::remove_file(&executable).unwrap();
        symlink("../second/server.js", &executable).unwrap();
        let error = verify_server_runtime_inputs(&server, &manifest_dir).unwrap_err().to_string();
        assert!(error.contains("server-install-closure"), "{error}");
    }

    #[test]
    fn locked_server_version_must_appear_in_probe_output() {
        let mut server = server_spec();
        server.locked_version = Some("0.8.36".into());
        assert!(
            verify_server_version_output(
                &server,
                "solc, the solidity compiler commandline interface\nVersion: 0.8.36+commit.8a079791"
            )
            .is_ok()
        );
        assert!(verify_server_version_output(&server, "Version: 0.8.35").is_err());
    }

    #[cfg(unix)]
    #[test]
    fn doctor_records_the_observed_server_version() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("server");
        fs::write(&executable, "#!/bin/sh\nprintf 'Wake 4.9.0 build abc\\n'\n").unwrap();
        make_executable(&executable).unwrap();
        let digest = sha256_path(&executable).unwrap();
        let mut server = server_spec();
        server.command = executable.clone();
        server.locked_version = Some("4.9.0".into());
        server.artifact = Some(ArtifactSpec { path: executable, sha256: Some(digest) });

        let checks = check_server(&server);
        let version = checks.iter().find(|check| check.kind == "server-version").unwrap();
        assert!(matches!(version.status, CheckStatus::Pass));
        assert_eq!(version.detail, "Wake 4.9.0 build abc");
    }

    #[test]
    fn versionless_npm_server_reads_installed_package_version() {
        let root = tempfile::tempdir().unwrap();
        fs::write(root.path().join("package.json"), r#"{"version":"0.8.24"}"#).unwrap();
        let mut server = server_spec();
        server.version_args.clear();
        server.locked_version = Some("0.8.25".into());
        server.artifact = Some(ArtifactSpec { path: root.path().into(), sha256: None });

        assert_eq!(
            inspect_version(Path::new("unused"), &server, VERSION_PROBE_TIMEOUT).unwrap(),
            "0.8.24"
        );
    }

    #[test]
    fn versionless_server_does_not_report_its_locked_version_as_observed() {
        let root = tempfile::tempdir().unwrap();
        let mut server = server_spec();
        server.version_args.clear();
        server.locked_version = Some("0.8.25".into());
        server.artifact = Some(ArtifactSpec { path: root.path().into(), sha256: None });

        let error = inspect_version(Path::new("unused"), &server, VERSION_PROBE_TIMEOUT)
            .unwrap_err()
            .to_string();
        assert!(error.contains("observable installed package metadata"), "{error}");
    }

    #[test]
    fn pip_manifest_is_verified_before_any_installer_command() {
        let root = tempfile::tempdir().unwrap();
        let manifest = root.path().join("requirements.txt");
        fs::write(&manifest, "eth-wake==4.9.0\n").unwrap();
        let install = InstallSpec {
            kind: "pip".into(),
            command: Some("missing-python-command".into()),
            args: Vec::new(),
            manifest: Some(manifest),
            manifest_sha256: Some("00".repeat(32)),
            python_version: Some("3.12".into()),
            target: Some("x86_64-unknown-linux-gnu".into()),
        };

        let error =
            prepare_pip_server(&server_spec(), &install, root.path()).unwrap_err().to_string();
        assert!(error.contains("pip manifest"), "{error}");
        assert!(error.contains("has SHA-256"), "{error}");
    }

    #[test]
    fn pip_closure_download_accepts_hashed_source_distributions() {
        assert!(PIP_CLOSURE_DOWNLOAD_ARGS.contains(&"--require-hashes"));
        assert!(!PIP_CLOSURE_DOWNLOAD_ARGS.iter().any(|arg| arg.starts_with("--only-binary")));
    }

    #[test]
    fn npm_manifest_is_verified_before_any_installer_command() {
        let root = tempfile::tempdir().unwrap();
        let manifest_dir = root.path().join("tools/lsp-bench");
        let manifest = root.path().join("npm");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::create_dir(&manifest).unwrap();
        fs::write(manifest.join("package.json"), "{}").unwrap();
        fs::write(manifest.join("package-lock.json"), "{}").unwrap();
        let mut server = server_spec();
        server.install = Some(InstallSpec {
            kind: "npm".into(),
            command: None,
            args: Vec::new(),
            manifest: Some(manifest),
            manifest_sha256: Some("00".repeat(32)),
            python_version: None,
            target: None,
        });

        let error = prepare_server(&server, &manifest_dir).unwrap_err().to_string();
        assert!(error.contains("npm package lock"), "{error}");
        assert!(error.contains("has SHA-256"), "{error}");
    }

    #[test]
    fn compiler_artifact_check_rejects_missing_and_mismatched_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("solc");
        assert!(matches!(
            check_file_digest("solc-native", "fixture", &path, Some("00".repeat(32).as_str()))
                .status,
            CheckStatus::Unavailable
        ));
        fs::write(&path, "compiler").unwrap();
        assert!(matches!(
            check_file_digest("solc-native", "fixture", &path, Some("00".repeat(32).as_str()))
                .status,
            CheckStatus::Mismatch
        ));
        let digest = sha256_path(&path).unwrap();
        assert!(matches!(
            check_file_digest("solc-native", "fixture", &path, Some(&digest)).status,
            CheckStatus::Pass
        ));
    }

    #[test]
    fn server_source_checkout_must_match_the_locked_revision_and_be_clean() {
        let root = tempfile::tempdir().unwrap();
        run_git(root.path(), &["init"]).unwrap();
        run_git(root.path(), &["remote", "add", "origin", "https://example.invalid/server.git"])
            .unwrap();
        fs::write(root.path().join("source"), "first").unwrap();
        run_git(root.path(), &["add", "source"]).unwrap();
        run_git(
            root.path(),
            &[
                "-c",
                "user.name=lsp-bench",
                "-c",
                "user.email=lsp-bench@example.invalid",
                "commit",
                "-m",
                "fixture",
            ],
        )
        .unwrap();
        let revision = git_output(root.path(), &["rev-parse", "HEAD"]).unwrap();
        let source = SourceSpec { url: "https://example.invalid/server.git".into(), revision };

        assert!(matches!(
            check_source_checkout("server", &source, root.path()).status,
            CheckStatus::Pass
        ));
        fs::write(root.path().join("source"), "changed").unwrap();
        assert!(matches!(
            check_source_checkout("server", &source, root.path()).status,
            CheckStatus::Mismatch
        ));
    }

    fn server_spec() -> ServerSpec {
        ServerSpec {
            id: "server".into(),
            command: "server".into(),
            args: Vec::new(),
            transport: crate::config::TransportSpec::Stdio,
            version_args: vec!["--version".into()],
            locked_version: None,
            expected_version: None,
            enabled: true,
            env: BTreeMap::new(),
            initialization_options: Value::Null,
            configuration: Value::Null,
            label: None,
            source: None,
            install: None,
            artifact: None,
            required: false,
        }
    }
}

//! Preparation and reproducibility checks for benchmark inputs.

use crate::{
    config::{
        ArtifactSpec, CompilerSpec, Config, FixtureSpec, InstallSpec, ServerSpec, SourceSpec,
    },
    fixture::FixtureSource,
    process::{
        cgroup_v2_process_tree_available, network_isolation_available, restricted_command,
        run_command_with_bounded_output,
    },
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
    time::Duration,
};

#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;

const REQUIRED_FIXTURES: [&str; 4] = ["synthetic", "v4-core", "aave-v3-origin", "optimism-bedrock"];
pub(crate) const VERSION_PROBE_TIMEOUT: Duration = Duration::from_secs(5);

pub(crate) struct PrepareOptions {
    pub(crate) config: PathBuf,
    pub(crate) servers: BTreeSet<String>,
    pub(crate) fixtures: BTreeSet<String>,
    pub(crate) prepare_servers: bool,
}

pub(crate) struct DoctorOptions {
    pub(crate) config: PathBuf,
    pub(crate) servers: BTreeSet<String>,
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
    let known_fixtures =
        config.fixtures.iter().map(|fixture| fixture.id.as_str()).collect::<BTreeSet<_>>();
    let missing_fixtures = options
        .fixtures
        .iter()
        .filter(|fixture| !known_fixtures.contains(fixture.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing_fixtures.is_empty() {
        bail!(
            "selected fixtures are not declared in the benchmark manifest: {}",
            missing_fixtures.join(", ")
        )
    }
    let manifest_dir = absolute_manifest_dir(&options.config)?;
    let servers = selected_servers(&config, &options.servers)?;
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
    if options.prepare_servers {
        for server in servers {
            prepare_server(server, &manifest_dir)?;
        }
    }
    doctor(DoctorOptions { config: options.config, servers: options.servers, publish: false })
}

pub(crate) fn doctor(options: DoctorOptions) -> Result<DoctorReport> {
    if options.publish && !options.servers.is_empty() {
        bail!("publication audit cannot use server filters")
    }
    let config = Config::load(&options.config)?;
    let manifest_dir = absolute_manifest_dir(&options.config)?;
    let servers = selected_servers(&config, &options.servers)?;
    let mut checks = Vec::new();
    validate_inventory(&servers, &config, &mut checks);
    for server in servers {
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

fn selected_servers<'a>(
    config: &'a Config,
    selected: &BTreeSet<String>,
) -> Result<Vec<&'a ServerSpec>> {
    let servers = config
        .servers
        .iter()
        .filter(|server| selected.is_empty() || selected.contains(&server.id))
        .collect::<Vec<_>>();
    if selected.is_empty() {
        return Ok(servers);
    }
    let matched = servers.iter().map(|server| server.id.as_str()).collect::<BTreeSet<_>>();
    let missing = selected
        .iter()
        .filter(|server| !matched.contains(server.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!("selected servers are not declared in the benchmark manifest: {}", missing.join(", "))
    }
    Ok(servers)
}

fn validate_inventory(servers: &[&ServerSpec], config: &Config, checks: &mut Vec<Check>) {
    let fixtures =
        config.fixtures.iter().map(|fixture| fixture.id.as_str()).collect::<BTreeSet<_>>();
    if servers.is_empty() {
        checks.push(inventory_check("server-inventory", "manifest", false));
    } else {
        for server in servers {
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
    let InstallSpec::Npm { manifest, manifest_sha256 } = server.install.as_ref()? else {
        return None;
    };
    Some(check_file_digest(
        "server-install-manifest",
        &server.id,
        &manifest.join("package-lock.json"),
        Some(manifest_sha256),
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
        let status = restricted_command("tar")
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
    let status = restricted_command("curl")
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
    let mut process = restricted_command(command);
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
    let mut process = restricted_command(command);
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

fn inspect_version_command(process: Command, command: &Path, timeout: Duration) -> Result<String> {
    let output = run_command_with_bounded_output(process, command, timeout)?;
    if output.timed_out {
        bail!("timed out waiting for version command")
    }
    if output.forced_kill {
        bail!("version command left descendants running")
    }
    if !output.status.success() {
        bail!("version command exited with {}", output.status)
    }
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let value = if stdout.trim().is_empty() { stderr.trim() } else { stdout.trim() };
    if value.is_empty() {
        bail!("version command produced no output")
    }
    Ok(value.to_owned())
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
    if let Some(source) = &server.source {
        prepare_checkout(source, &server_source_root(manifest_dir, &server.id))?;
    }
    let Some(install) = &server.install else { return Ok(()) };
    let artifact_root = server_artifact_root(manifest_dir, &server.id);
    fs::create_dir_all(&artifact_root)?;
    match install {
        InstallSpec::Npm { manifest, manifest_sha256 } => {
            verify_file_sha256(
                &manifest.join("package-lock.json"),
                manifest_sha256,
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
            let status = restricted_command("npm")
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
        InstallSpec::Archive { url } => prepare_archive_server(server, url, &artifact_root)?,
        InstallSpec::Binary { url } => prepare_binary_server(server, url)?,
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

fn prepare_archive_server(server: &ServerSpec, url: &str, artifact_root: &Path) -> Result<()> {
    let artifact = server
        .artifact
        .as_ref()
        .with_context(|| format!("server `{}` archive artifact is missing", server.id))?;
    let expected = artifact
        .sha256
        .as_deref()
        .with_context(|| format!("server `{}` archive artifact SHA-256 is missing", server.id))?;
    let artifact_relative = artifact.path.strip_prefix(artifact_root).with_context(|| {
        format!(
            "server `{}` archive artifact `{}` is outside `{}`",
            server.id,
            artifact.path.display(),
            artifact_root.display()
        )
    })?;
    let command_relative = server.command.strip_prefix(artifact_root).with_context(|| {
        format!(
            "server `{}` archive executable `{}` is outside `{}`",
            server.id,
            server.command.display(),
            artifact_root.display()
        )
    })?;
    for (kind, path) in [("artifact", artifact_relative), ("executable", command_relative)] {
        if path.as_os_str().is_empty()
            || path
                .components()
                .any(|component| !matches!(component, std::path::Component::Normal(_)))
        {
            bail!("server `{}` archive {kind} path is not safely relative", server.id)
        }
    }

    let parent = artifact_root
        .parent()
        .with_context(|| format!("server `{}` archive root has no parent", server.id))?;
    fs::create_dir_all(parent)?;
    let staging =
        tempfile::Builder::new().prefix(&format!(".{}-archive-", server.id)).tempdir_in(parent)?;
    let staged_artifact = staging.path().join(artifact_relative);
    if let Some(parent) = staged_artifact.parent() {
        fs::create_dir_all(parent)?;
    }
    download_verified(url, &staged_artifact, Some(expected))?;
    let status = restricted_command("tar")
        .arg("-xzf")
        .arg(&staged_artifact)
        .arg("-C")
        .arg(staging.path())
        .stdin(Stdio::null())
        .status()
        .with_context(|| format!("failed to extract server `{}` archive", server.id))?;
    if !status.success() {
        bail!("server `{}` archive extraction exited with {status}", server.id)
    }
    let staged_command = staging.path().join(command_relative);
    if !staged_command.is_file() {
        bail!(
            "server `{}` archive did not contain executable `{}`",
            server.id,
            command_relative.display()
        )
    }
    #[cfg(unix)]
    if staged_command.metadata()?.permissions().mode() & 0o111 == 0 {
        bail!(
            "server `{}` archive executable `{}` is not executable",
            server.id,
            command_relative.display()
        )
    }

    match fs::symlink_metadata(artifact_root) {
        Ok(metadata) if metadata.is_dir() && !metadata.file_type().is_symlink() => {
            fs::remove_dir_all(artifact_root)?;
        }
        Ok(_) => bail!(
            "server `{}` archive root `{}` is not a directory",
            server.id,
            artifact_root.display()
        ),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => return Err(error.into()),
    }
    fs::rename(staging.keep(), artifact_root).with_context(|| {
        format!("failed to publish server `{}` archive installation", server.id)
    })?;
    Ok(())
}

fn prepare_binary_server(server: &ServerSpec, url: &str) -> Result<()> {
    let artifact = server
        .artifact
        .as_ref()
        .with_context(|| format!("server `{}` binary artifact is missing", server.id))?;
    let expected = artifact
        .sha256
        .as_deref()
        .with_context(|| format!("server `{}` binary artifact SHA-256 is missing", server.id))?;
    if server.command != artifact.path {
        bail!("server `{}` binary command must be its pinned artifact", server.id)
    }
    download_verified(url, &artifact.path, Some(expected))?;
    #[cfg(unix)]
    {
        let mut permissions = artifact.path.metadata()?.permissions();
        permissions.set_mode(permissions.mode() | 0o111);
        fs::set_permissions(&artifact.path, permissions)?;
    }
    Ok(())
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
        .is_some_and(|install| {
            matches!(install, InstallSpec::Npm { .. } | InstallSpec::Archive { .. })
        })
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
    let status = restricted_command("git").arg("-C").arg(root).args(args).status()?;
    if !status.success() {
        bail!("Git command failed in `{}` with {status}", root.display())
    }
    Ok(())
}

pub(crate) fn git_output(root: &Path, args: &[&str]) -> Result<String> {
    let output = restricted_command("git")
        .arg("-C")
        .arg(root)
        .args(args)
        .output()
        .with_context(|| format!("failed to run Git in `{}`", root.display()))?;
    if !output.status.success() {
        bail!(
            "Git command failed in `{}`: {}",
            root.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        )
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
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn server_inventory_is_derived_from_the_manifest() {
        let mut server = server_spec();
        server.id = "manifest-server".into();
        let mut config = Config {
            schema_version: crate::config::SCHEMA_VERSION,
            config_sha256: String::new(),
            profiles: BTreeMap::new(),
            servers_lock_sha256: None,
            fixtures_lock_sha256: None,
            servers: vec![server],
            fixtures: Vec::new(),
            workloads: Vec::new(),
        };
        let mut checks = Vec::new();
        let servers = selected_servers(&config, &BTreeSet::new()).unwrap();
        validate_inventory(&servers, &config, &mut checks);
        let server_checks =
            checks.iter().filter(|check| check.kind == "server-inventory").collect::<Vec<_>>();
        assert_eq!(server_checks.len(), 1);
        assert_eq!(server_checks[0].id, "manifest-server");
        assert!(matches!(server_checks[0].status, CheckStatus::Pass));

        let selected =
            selected_servers(&config, &BTreeSet::from(["manifest-server".to_owned()])).unwrap();
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].id, "manifest-server");
        let error = selected_servers(&config, &BTreeSet::from(["missing".to_owned()]))
            .unwrap_err()
            .to_string();
        assert!(error.contains("not declared"), "{error}");

        config.servers.clear();
        checks.clear();
        let servers = selected_servers(&config, &BTreeSet::new()).unwrap();
        validate_inventory(&servers, &config, &mut checks);
        let missing = checks.iter().find(|check| check.kind == "server-inventory").unwrap();
        assert!(matches!(missing.status, CheckStatus::Mismatch));
    }

    #[test]
    fn prepare_rejects_unknown_fixture_selection() {
        let root = tempfile::tempdir().unwrap();
        let fixture = root.path().join("fixture");
        fs::create_dir(&fixture).unwrap();
        fs::write(fixture.join("Main.sol"), "contract Main {}\n").unwrap();
        let config = root.path().join("benchmark.yaml");
        fs::write(
            &config,
            format!(
                "version: 1\nservers: []\nfixtures:\n  - id: known\n    root: {}\nworkloads: []\n",
                fixture.display()
            ),
        )
        .unwrap();

        let error = prepare(PrepareOptions {
            config,
            servers: BTreeSet::new(),
            fixtures: BTreeSet::from(["missing".to_owned()]),
            prepare_servers: false,
        })
        .unwrap_err()
        .to_string();

        assert!(error.contains("selected fixtures are not declared"), "{error}");
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
        server.install =
            Some(InstallSpec::Archive { url: "https://example.invalid/server.tar.gz".into() });
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
    fn archive_closure_receipt_rejects_executable_mutation() {
        let root = tempfile::tempdir().unwrap();
        let manifest_dir = root.path().join("tools/lsp-bench");
        fs::create_dir_all(&manifest_dir).unwrap();
        let mut server = server_spec();
        server.install =
            Some(InstallSpec::Archive { url: "https://example.invalid/server.tar.gz".into() });
        let closure = server_artifact_root(&manifest_dir, &server.id);
        fs::create_dir_all(&closure).unwrap();
        server.command = closure.join("server");
        fs::write(&server.command, "first").unwrap();
        let receipt = server_installed_closure_receipt(&manifest_dir, &server.id);
        fs::create_dir_all(receipt.parent().unwrap()).unwrap();
        fs::write(&receipt, format!("{}\n", sha256_installed_closure(&closure).unwrap())).unwrap();

        verify_server_runtime_inputs(&server, &manifest_dir).unwrap();
        fs::write(&server.command, "changed").unwrap();
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
        server.install =
            Some(InstallSpec::Archive { url: "https://example.invalid/server.tar.gz".into() });
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

    #[cfg(unix)]
    #[test]
    fn version_probe_rejects_descendants_that_outlive_the_command() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("server");
        fs::write(&executable, "#!/bin/sh\nsleep 30 & printf 'Wake 4.9.0 build abc\\n'; exit 0\n")
            .unwrap();
        make_executable(&executable).unwrap();
        let mut server = server_spec();
        server.command = executable.clone();
        server.version_args = vec!["--version".into()];

        let error =
            inspect_version(&executable, &server, VERSION_PROBE_TIMEOUT).unwrap_err().to_string();

        assert!(error.contains("left descendants running"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn version_probe_reports_when_the_command_itself_times_out() {
        let root = tempfile::tempdir().unwrap();
        let executable = root.path().join("server");
        fs::write(&executable, "#!/bin/sh\nexec sleep 30\n").unwrap();
        make_executable(&executable).unwrap();
        let mut server = server_spec();
        server.command = executable.clone();
        server.version_args = vec!["--version".into()];

        let error = inspect_version(&executable, &server, Duration::from_millis(50))
            .unwrap_err()
            .to_string();

        assert!(error.contains("timed out waiting for version command"), "{error}");
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
    fn npm_manifest_is_verified_before_any_installer_command() {
        let root = tempfile::tempdir().unwrap();
        let manifest_dir = root.path().join("tools/lsp-bench");
        let manifest = root.path().join("npm");
        fs::create_dir_all(&manifest_dir).unwrap();
        fs::create_dir(&manifest).unwrap();
        fs::write(manifest.join("package.json"), "{}").unwrap();
        fs::write(manifest.join("package-lock.json"), "{}").unwrap();
        let mut server = server_spec();
        server.install = Some(InstallSpec::Npm { manifest, manifest_sha256: "00".repeat(32) });

        let error = prepare_server(&server, &manifest_dir).unwrap_err().to_string();
        assert!(error.contains("npm package lock"), "{error}");
        assert!(error.contains("has SHA-256"), "{error}");
    }

    #[cfg(unix)]
    #[test]
    fn archive_is_verified_before_extraction() {
        let root = tempfile::tempdir().unwrap();
        let payload = root.path().join("payload");
        fs::create_dir(&payload).unwrap();
        let executable = payload.join("server");
        fs::write(&executable, "#!/bin/sh\necho server 1\n").unwrap();
        let mut permissions = executable.metadata().unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&executable, permissions).unwrap();
        let archive = root.path().join("server.tar.gz");
        let status = Command::new("tar")
            .args(["-czf"])
            .arg(&archive)
            .arg("-C")
            .arg(&payload)
            .arg("server")
            .status()
            .unwrap();
        assert!(status.success());

        let manifest_dir = root.path().join("workspace/tools/lsp-bench");
        fs::create_dir_all(&manifest_dir).unwrap();
        let artifact_root = server_artifact_root(&manifest_dir, "server");
        let mut server = server_spec();
        server.command = artifact_root.join("server");
        server.install =
            Some(InstallSpec::Archive { url: format!("file://{}", archive.display()) });
        server.artifact = Some(ArtifactSpec {
            path: artifact_root.join("server.tar.gz"),
            sha256: Some(sha256_path(&archive).unwrap()),
        });

        prepare_server(&server, &manifest_dir).unwrap();
        assert_eq!(fs::read_to_string(&server.command).unwrap(), "#!/bin/sh\necho server 1\n");

        let bad_root = server_artifact_root(&manifest_dir, "bad-server");
        server.id = "bad-server".into();
        server.command = bad_root.join("server");
        server.artifact = Some(ArtifactSpec {
            path: bad_root.join("server.tar.gz"),
            sha256: Some("00".repeat(32)),
        });
        let error = prepare_server(&server, &manifest_dir).unwrap_err().to_string();
        assert!(error.contains("SHA-256"), "{error}");
        assert!(!server.command.exists());
    }

    #[cfg(unix)]
    #[test]
    fn binary_is_verified_before_becoming_executable() {
        let root = tempfile::tempdir().unwrap();
        let source = root.path().join("source-server");
        fs::write(&source, "#!/bin/sh\necho server 1\n").unwrap();
        let manifest_dir = root.path().join("workspace/tools/lsp-bench");
        fs::create_dir_all(&manifest_dir).unwrap();
        let artifact_root = server_artifact_root(&manifest_dir, "server");
        let mut server = server_spec();
        server.command = artifact_root.join("server");
        server.install = Some(InstallSpec::Binary { url: format!("file://{}", source.display()) });
        server.artifact = Some(ArtifactSpec {
            path: server.command.clone(),
            sha256: Some(sha256_path(&source).unwrap()),
        });

        prepare_server(&server, &manifest_dir).unwrap();
        assert_eq!(sha256_path(&server.command).unwrap(), sha256_path(&source).unwrap());
        assert_ne!(server.command.metadata().unwrap().permissions().mode() & 0o111, 0);

        let bad_root = server_artifact_root(&manifest_dir, "bad-server");
        server.id = "bad-server".into();
        server.command = bad_root.join("server");
        server.artifact =
            Some(ArtifactSpec { path: server.command.clone(), sha256: Some("00".repeat(32)) });
        let error = prepare_server(&server, &manifest_dir).unwrap_err().to_string();
        assert!(error.contains("SHA-256"), "{error}");
        assert!(!server.command.exists());
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

//! Machine-readable samples and summaries for cross-server runs.

use crate::{
    config::{Config, TransportSpec},
    process::{MemoryAccounting, Observations, ProcessAccounting, ProcessMetrics},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fmt::Write as _,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) const RESULT_SCHEMA_VERSION: u32 = 6;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Environment {
    pub(crate) os: String,
    pub(crate) architecture: String,
    pub(crate) logical_cpus: usize,
    pub(crate) accounting_backends: Vec<ProcessAccounting>,
    pub(crate) memory_accounting_backends: Vec<MemoryAccounting>,
    pub(crate) network_isolated: bool,
    pub(crate) authoritative: bool,
}

impl Environment {
    pub(crate) fn current(samples: &[RunSample]) -> Self {
        let accounting_backends = samples
            .iter()
            .flat_map(sample_processes)
            .map(|process| process.accounting)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let memory_accounting_backends = samples
            .iter()
            .flat_map(sample_processes)
            .map(|process| process.memory_accounting)
            .collect::<std::collections::BTreeSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let successful = samples.iter().filter(|sample| sample.succeeded()).collect::<Vec<_>>();
        let network_isolated = !successful.is_empty()
            && successful
                .iter()
                .flat_map(|sample| sample_processes(sample))
                .all(|process| process.network_isolated);
        let authoritative = cfg!(all(target_os = "linux", target_arch = "x86_64"))
            && network_isolated
            && samples_have_authoritative_metrics(&successful);
        Self {
            os: std::env::consts::OS.into(),
            architecture: std::env::consts::ARCH.into(),
            logical_cpus: std::thread::available_parallelism().map_or(1, usize::from),
            accounting_backends,
            memory_accounting_backends,
            network_isolated,
            authoritative,
        }
    }
}

fn samples_have_authoritative_metrics(samples: &[&RunSample]) -> bool {
    !samples.is_empty()
        && samples.iter().all(|sample| {
            sample
                .process
                .as_ref()
                .is_some_and(ProcessMetrics::has_authoritative_process_tree_metrics)
                && sample.observations.has_authoritative_process_tree_request_metrics()
                && sample.setup_phases.iter().all(|phase| {
                    phase.process.has_authoritative_process_tree_metrics()
                        && phase.observations.has_authoritative_process_tree_request_metrics()
                })
        })
}

fn sample_processes(sample: &RunSample) -> impl Iterator<Item = &ProcessMetrics> {
    sample.process.iter().chain(sample.setup_phases.iter().map(|phase| &phase.process))
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServerMetadata {
    pub(crate) id: String,
    pub(crate) label: Option<String>,
    pub(crate) command: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) transport: TransportSpec,
    pub(crate) version_args: Vec<String>,
    pub(crate) version: Option<String>,
    pub(crate) locked_version: Option<String>,
    pub(crate) expected_version: Option<String>,
    pub(crate) enabled: bool,
    pub(crate) env: BTreeMap<String, String>,
    pub(crate) initialization_options: Value,
    pub(crate) configuration: Value,
    pub(crate) source: Option<crate::config::SourceSpec>,
    /// Set when a workflow-built Solar executable replaced the manifest command.
    #[serde(default)]
    pub(crate) source_revision_override: Option<String>,
    pub(crate) executable_sha256: Option<String>,
    pub(crate) artifact_path: Option<PathBuf>,
    pub(crate) artifact_expected_sha256: Option<String>,
    pub(crate) artifact_sha256: Option<String>,
    pub(crate) required: bool,
    pub(crate) status: ServerStatus,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct NpmRuntimeMetadata {
    pub(crate) node_version: Option<String>,
    pub(crate) npm_version: Option<String>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ServerStatus {
    Available,
    Disabled,
    Incompatible,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkloadMetadata {
    pub(crate) id: String,
    pub(crate) fixture: String,
    pub(crate) methods: Vec<String>,
    pub(crate) step_count: usize,
    pub(crate) repetitions: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CorrectnessResult {
    pub(crate) probe: String,
    pub(crate) ok: bool,
    pub(crate) detail: String,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum RunStatus {
    Pass,
    Unsupported,
    Incorrect,
    Incompatible,
    Timeout,
    Crash,
    Unavailable,
    HarnessError,
}

impl RunStatus {
    pub(crate) const fn is_success(&self) -> bool {
        matches!(self, Self::Pass)
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RunSample {
    pub(crate) server: String,
    pub(crate) fixture: String,
    pub(crate) workload: String,
    pub(crate) repetition: usize,
    pub(crate) status: RunStatus,
    pub(crate) timings_ms: BTreeMap<String, f64>,
    pub(crate) process: Option<ProcessMetrics>,
    pub(crate) setup_phases: Vec<ProcessPhase>,
    pub(crate) observations: Observations,
    pub(crate) correctness: Vec<CorrectnessResult>,
    pub(crate) error: Option<String>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProcessPhase {
    pub(crate) name: String,
    pub(crate) process: ProcessMetrics,
    pub(crate) observations: Observations,
}

impl RunSample {
    pub(crate) fn succeeded(&self) -> bool {
        self.status.is_success()
    }
}

#[derive(Clone, Copy, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Stats {
    pub(crate) count: usize,
    pub(crate) mean: f64,
    pub(crate) p50: f64,
    pub(crate) p95: f64,
    pub(crate) p99: f64,
    pub(crate) max: f64,
}

impl Stats {
    pub(crate) fn new(values: &[f64]) -> Self {
        if values.is_empty() {
            return Self { count: 0, mean: 0.0, p50: 0.0, p95: 0.0, p99: 0.0, max: 0.0 };
        }
        let mut sorted = values.to_vec();
        sorted.sort_by(f64::total_cmp);
        Self {
            count: sorted.len(),
            mean: sorted.iter().sum::<f64>() / sorted.len() as f64,
            p50: percentile(&sorted, 0.50),
            p95: percentile(&sorted, 0.95),
            p99: percentile(&sorted, 0.99),
            max: *sorted.last().unwrap(),
        }
    }
}

fn percentile(sorted: &[f64], ratio: f64) -> f64 {
    let rank = (sorted.len() as f64 * ratio).ceil() as usize;
    sorted[rank.saturating_sub(1).min(sorted.len() - 1)]
}

#[derive(Clone, Debug, Deserialize, PartialEq, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SummaryGroup {
    pub(crate) server: String,
    pub(crate) fixture: String,
    pub(crate) workload: String,
    pub(crate) successful_runs: usize,
    pub(crate) status_counts: BTreeMap<String, usize>,
    pub(crate) status: SummaryStatus,
    pub(crate) metrics: BTreeMap<String, Stats>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum SummaryStatus {
    Pass,
    Partial,
    Unsupported,
    Unavailable,
    Failed,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SummaryReport {
    pub(crate) schema_version: u32,
    pub(crate) config_schema_version: u32,
    pub(crate) config_path: PathBuf,
    pub(crate) config_sha256: String,
    pub(crate) servers_lock_sha256: Option<String>,
    pub(crate) fixtures_lock_sha256: Option<String>,
    pub(crate) profile: String,
    pub(crate) harness_version: String,
    pub(crate) harness_contract_sha256: Option<String>,
    pub(crate) rustc_version: Option<String>,
    pub(crate) npm_runtime: Option<NpmRuntimeMetadata>,
    pub(crate) harness_git_revision: Option<String>,
    pub(crate) harness_git_dirty: Option<bool>,
    pub(crate) repeat_override: Option<usize>,
    pub(crate) timeout_ms: u64,
    pub(crate) environment: Environment,
    pub(crate) servers: Vec<ServerMetadata>,
    pub(crate) fixtures: Vec<crate::fixture::FixtureMetadata>,
    pub(crate) workloads: Vec<WorkloadMetadata>,
    pub(crate) summaries: Vec<SummaryGroup>,
}

#[derive(Serialize)]
struct SamplesReport<'a> {
    schema_version: u32,
    samples: &'a [RunSample],
}

pub(crate) struct SummaryInput<'a> {
    pub(crate) config_path: PathBuf,
    pub(crate) config: &'a Config,
    pub(crate) servers: Vec<ServerMetadata>,
    pub(crate) fixtures: Vec<crate::fixture::FixtureMetadata>,
    pub(crate) samples: &'a [RunSample],
    pub(crate) repeat_override: Option<usize>,
    pub(crate) workload_repetitions: &'a BTreeMap<String, usize>,
    pub(crate) timeout_ms: u64,
    pub(crate) profile: String,
}

pub(crate) fn summarize(input: SummaryInput<'_>) -> SummaryReport {
    let SummaryInput {
        config_path,
        config,
        servers,
        fixtures,
        samples,
        repeat_override,
        workload_repetitions,
        timeout_ms,
        profile,
    } = input;
    let warm_workloads = config
        .workloads
        .iter()
        .filter(|workload| {
            workload.steps.iter().any(|step| matches!(step, crate::config::StepSpec::Warm { .. }))
        })
        .map(|workload| workload.id.as_str())
        .collect::<std::collections::BTreeSet<_>>();
    let mut groups = BTreeMap::<(&str, &str, &str), Vec<&RunSample>>::new();
    for sample in samples {
        groups.entry((&sample.server, &sample.fixture, &sample.workload)).or_default().push(sample);
    }
    let summaries = groups
        .into_iter()
        .map(|((server, fixture, workload), runs)| {
            let mut status_counts = BTreeMap::new();
            let mut metric_values = BTreeMap::<String, Vec<f64>>::new();
            for run in &runs {
                *status_counts.entry(status_name(&run.status).to_owned()).or_insert(0) += 1;
                if run.succeeded() {
                    if !warm_workloads.contains(workload) {
                        for (name, value) in &run.timings_ms {
                            metric_values
                                .entry(summary_metric_name(name))
                                .or_default()
                                .push(*value);
                        }
                        if let Some(process) = &run.process {
                            if let (Some(user), Some(system)) =
                                (process.user_cpu_ms, process.system_cpu_ms)
                            {
                                metric_values
                                    .entry("session_cpu_ms".into())
                                    .or_default()
                                    .push(user + system);
                            }
                            if let Some((name, memory)) = process.peak_memory_metric() {
                                metric_values
                                    .entry(format!("session_{name}"))
                                    .or_default()
                                    .push(memory);
                            }
                            if let Some(rss) = process.peak_process_tree_rss_mib {
                                metric_values
                                    .entry("session_peak_process_tree_rss_mib".into())
                                    .or_default()
                                    .push(rss);
                            }
                            metric_values
                                .entry("session_wall_ms".into())
                                .or_default()
                                .push(process.wall_ms);
                        }
                    }
                    for request in &run.observations.requests {
                        metric_values
                            .entry(request.method.clone())
                            .or_default()
                            .push(request.elapsed_ms);
                        if let Some(cpu) = request.process_tree_cpu_ms {
                            metric_values
                                .entry(format!("{}_cpu_ms", request.method))
                                .or_default()
                                .push(cpu);
                        }
                    }
                }
            }
            let successful_runs = runs.iter().filter(|run| run.succeeded()).count();
            let status = summary_status(&status_counts, successful_runs);
            SummaryGroup {
                server: server.into(),
                fixture: fixture.into(),
                workload: workload.into(),
                successful_runs,
                status_counts,
                status,
                metrics: metric_values
                    .into_iter()
                    .map(|(name, values)| (name, Stats::new(&values)))
                    .collect(),
            }
        })
        .collect();

    let (harness_git_revision, harness_git_dirty) = harness_git_provenance();
    let npm_runtime = requires_npm_runtime(config, &servers).then(observe_npm_runtime);
    SummaryReport {
        schema_version: RESULT_SCHEMA_VERSION,
        config_schema_version: config.schema_version,
        config_sha256: config.config_sha256.clone(),
        servers_lock_sha256: config.servers_lock_sha256.clone(),
        fixtures_lock_sha256: config.fixtures_lock_sha256.clone(),
        config_path,
        profile,
        harness_version: env!("CARGO_PKG_VERSION").into(),
        harness_contract_sha256: harness_contract_sha256(),
        rustc_version: command_output("rustc", &["--version", "--verbose"])
            .map(|output| normalize_multiline_output(&output)),
        npm_runtime,
        harness_git_revision,
        harness_git_dirty,
        repeat_override,
        timeout_ms,
        environment: Environment::current(samples),
        servers,
        fixtures,
        workloads: config
            .workloads
            .iter()
            .filter_map(|workload| {
                Some(WorkloadMetadata {
                    id: workload.id.clone(),
                    fixture: workload.fixture.clone(),
                    methods: workload.methods.clone(),
                    step_count: workload.steps.len(),
                    repetitions: *workload_repetitions.get(&workload.id)?,
                })
            })
            .collect(),
        summaries,
    }
}

fn harness_git_provenance() -> (Option<String>, Option<bool>) {
    let revision = git_output(&["rev-parse", "HEAD"]).filter(|revision| {
        revision.len() == 40 && revision.bytes().all(|byte| byte.is_ascii_hexdigit())
    });
    let dirty = revision
        .as_ref()
        .and_then(|_| git_output(&["status", "--porcelain", "--untracked-files=normal"]))
        .map(|status| !status.is_empty());
    (revision, dirty)
}

fn harness_contract_sha256() -> Option<String> {
    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let mut files = vec![manifest_dir.join("Cargo.toml")];
    collect_contract_files(&manifest_dir.join("src"), &mut files).ok()?;
    files.sort();

    let mut hasher = Sha256::new();
    for path in files {
        let relative = path.strip_prefix(manifest_dir).ok()?;
        let contents = fs::read(&path).ok()?;
        hasher.update(relative.to_string_lossy().as_bytes());
        hasher.update([0]);
        hasher.update(contents.len().to_le_bytes());
        hasher.update(contents);
    }

    let dependency_tree = Command::new("cargo")
        .args(["tree", "--locked", "--offline", "--manifest-path"])
        .arg(manifest_dir.join("Cargo.toml"))
        .args([
            "-p",
            "solar-lsp-bench",
            "--edges",
            "normal,build,features",
            "--prefix",
            "none",
            "--no-dedupe",
        ])
        .env("CARGO_TERM_COLOR", "never")
        .output()
        .ok()?;
    if !dependency_tree.status.success() {
        return None;
    }
    let dependency_tree = String::from_utf8(dependency_tree.stdout).ok()?;
    hasher.update(dependency_tree.replace(&manifest_dir.to_string_lossy().into_owned(), "."));
    Some(format!("{:x}", hasher.finalize()))
}

fn collect_contract_files(directory: &Path, files: &mut Vec<PathBuf>) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let entry = entry?;
        let path = entry.path();
        if entry.file_type()?.is_dir() {
            collect_contract_files(&path, files)?;
        } else if path.extension().is_some_and(|extension| extension == "rs") {
            files.push(path);
        }
    }
    Ok(())
}

fn git_output(args: &[&str]) -> Option<String> {
    command_output("git", args)
}

fn command_output(command: &str, args: &[&str]) -> Option<String> {
    let output = Command::new(command).args(args).output().ok()?;
    output.status.success().then(|| String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn requires_npm_runtime(config: &Config, servers: &[ServerMetadata]) -> bool {
    config.servers.iter().any(|spec| {
        spec.install.as_ref().is_some_and(|install| install.kind == "npm")
            && servers.iter().any(|server| server.id == spec.id)
    })
}

fn observe_npm_runtime() -> NpmRuntimeMetadata {
    NpmRuntimeMetadata {
        node_version: command_output("node", &["--version"]),
        npm_version: command_output("npm", &["--version"]),
    }
}

fn normalize_multiline_output(output: &str) -> String {
    output.lines().map(str::trim).filter(|line| !line.is_empty()).collect::<Vec<_>>().join("; ")
}

fn summary_metric_name(name: &str) -> String {
    if let Some(warm) = name.strip_prefix("warm_").and_then(|name| name.strip_suffix("_ms"))
        && let Some((label, index)) = warm.rsplit_once('_')
        && index.parse::<usize>().is_ok()
    {
        return format!("warm_{label}_ms");
    }
    name.into()
}

pub(crate) fn write_reports(
    output: &Path,
    summary: &SummaryReport,
    samples: &[RunSample],
) -> Result<()> {
    fs::create_dir_all(output)?;
    let temporary = tempfile::tempdir_in(output)?;
    let summary_json = serde_json::to_vec_pretty(summary)?;
    let canonical_summary = serde_json::from_slice::<SummaryReport>(&summary_json)?;
    fs::write(temporary.path().join("summary.json"), summary_json)?;
    fs::write(
        temporary.path().join("samples.json"),
        serde_json::to_vec_pretty(&SamplesReport {
            schema_version: RESULT_SCHEMA_VERSION,
            samples,
        })?,
    )?;
    let mut jsonl = String::new();
    for sample in samples {
        jsonl.push_str(&serde_json::to_string(sample)?);
        jsonl.push('\n');
    }
    fs::write(temporary.path().join("samples.jsonl"), jsonl)?;
    fs::write(temporary.path().join("summary.md"), markdown(&canonical_summary))?;
    for name in ["summary.json", "samples.json", "samples.jsonl", "summary.md"] {
        let source = temporary.path().join(name);
        let destination = output.join(name);
        fs::rename(source, destination).with_context(|| format!("failed to publish `{name}`"))?;
    }
    Ok(())
}

pub(crate) fn regenerate_markdown(
    input: &Path,
    output: &Path,
    require_authoritative: bool,
) -> Result<()> {
    let summary = read_summary(input)?;
    if require_authoritative && !summary.environment.authoritative {
        anyhow::bail!("benchmark summary is not authoritative")
    }
    if let Some(parent) = output.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(output, markdown(&summary))?;
    Ok(())
}

/// Read a summary artifact from a regular file and validate its schema before rendering it.
///
/// The offline `report` command consumes artifacts produced by `run`; keeping this read strict
/// avoids silently accepting a directory, special file, or summary from an incompatible harness.
fn read_summary(path: &Path) -> Result<SummaryReport> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("failed to read summary `{}`", path.display()))?;
    if !metadata.file_type().is_file() {
        bail!("summary `{}` is not a regular file", path.display());
    }
    let bytes =
        fs::read(path).with_context(|| format!("failed to read summary `{}`", path.display()))?;
    let summary = serde_json::from_slice::<SummaryReport>(&bytes)
        .with_context(|| format!("failed to parse summary `{}`", path.display()))?;
    if summary.schema_version != RESULT_SCHEMA_VERSION {
        bail!(
            "unsupported result schema {}; expected {}",
            summary.schema_version,
            RESULT_SCHEMA_VERSION
        );
    }
    Ok(summary)
}

pub(crate) fn terminal(summary: &SummaryReport) -> String {
    let mut output = String::from(
        "Cross-server Solidity LSP benchmark\n\
         Latencies are milliseconds; failed and unsupported runs are excluded from metric stats\n\n",
    );
    output.push_str("Server / fixture / workload                  Runs  Statuses                         p50       p95       p99       max\n");
    output.push_str("--------------------------------------------  ----  ------------------------------  --------  --------  --------  --------\n");
    for group in &summary.summaries {
        let key = format!("{}/{}/{}", group.server, group.fixture, group.workload);
        let statuses = group
            .status_counts
            .iter()
            .map(|(status, count)| format!("{status}:{count}"))
            .collect::<Vec<_>>()
            .join(",");
        let stats = group.metrics.get("cold_ready_ms").or_else(|| group.metrics.values().next());
        let values = stats.map_or([0.0; 4], |stats| [stats.p50, stats.p95, stats.p99, stats.max]);
        let _ = writeln!(
            output,
            "{key:<44}  {:>4}  {statuses:<30}  {:>8.2}  {:>8.2}  {:>8.2}  {:>8.2}",
            group.successful_runs, values[0], values[1], values[2], values[3],
        );
    }
    output
}

fn markdown(summary: &SummaryReport) -> String {
    let mut output = String::from("# Cross-server Solidity LSP benchmark\n\n");
    if !summary.environment.authoritative {
        output.push_str(
            "> [!WARNING]\n> This run is not an authoritative performance measurement.\n\n",
        );
    }
    output.push_str("## Run metadata\n\n| Field | Value |\n|---|---|\n");
    let metadata = [
        ("Result schema", summary.schema_version.to_string()),
        ("Config schema", summary.config_schema_version.to_string()),
        ("Profile", summary.profile.clone()),
        ("Harness version", summary.harness_version.clone()),
        (
            "Harness contract SHA-256",
            summary.harness_contract_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        ("Rust compiler", summary.rustc_version.clone().unwrap_or_else(|| "unavailable".into())),
        (
            "Node.js runtime",
            summary.npm_runtime.as_ref().map_or_else(
                || "not required".into(),
                |runtime| runtime.node_version.clone().unwrap_or_else(|| "unavailable".into()),
            ),
        ),
        (
            "npm version",
            summary.npm_runtime.as_ref().map_or_else(
                || "not required".into(),
                |runtime| runtime.npm_version.clone().unwrap_or_else(|| "unavailable".into()),
            ),
        ),
        (
            "Harness revision",
            summary.harness_git_revision.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        ("Harness dirty", summary.harness_git_dirty.map(yes_no).unwrap_or("unavailable").into()),
        ("Platform", format!("{}-{}", summary.environment.architecture, summary.environment.os)),
        ("Logical CPUs", summary.environment.logical_cpus.to_string()),
        ("Network isolated", yes_no(summary.environment.network_isolated).into()),
        ("Authoritative", yes_no(summary.environment.authoritative).into()),
        ("Config SHA-256", summary.config_sha256.clone()),
        (
            "Servers lock SHA-256",
            summary.servers_lock_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        (
            "Fixtures lock SHA-256",
            summary.fixtures_lock_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
    ];
    for (name, value) in metadata {
        let _ = writeln!(output, "| {name} | {} |", markdown_cell(&value));
    }
    if !summary.servers.is_empty() {
        output.push_str(
            "\n## Servers\n\n| ID | Label | Status | Observed version | Locked version | Executable SHA-256 | Source revision | Artifact SHA-256 |\n|---|---|---|---|---|---|---|---|\n",
        );
        for server in &summary.servers {
            let values = [
                server.id.as_str(),
                server.label.as_deref().unwrap_or("unavailable"),
                server_status_name(server.status),
                server.version.as_deref().unwrap_or("unavailable"),
                server.locked_version.as_deref().unwrap_or("unavailable"),
                server.executable_sha256.as_deref().unwrap_or("unavailable"),
                server.source.as_ref().map_or("unavailable", |source| source.revision.as_str()),
                server.artifact_sha256.as_deref().unwrap_or("unavailable"),
            ];
            let values = values.map(markdown_cell);
            let _ = writeln!(output, "| {} |", values.join(" | "));
        }
    }
    if !summary.fixtures.is_empty() {
        output.push_str(
            "\n## Fixtures\n\n| ID | Corpus | Revision | Content SHA-256 | Solidity files | Lines | Bytes | Solc | Foundry |\n|---|---|---|---|---:|---:|---:|---|---|\n",
        );
        for fixture in &summary.fixtures {
            let solc = compiler_provenance(
                fixture.solc.as_ref(),
                fixture.solc_native_sha256.as_deref(),
                fixture.solc_soljson_sha256.as_deref(),
            );
            let foundry = compiler_provenance(
                fixture.foundry.as_ref(),
                fixture.foundry_native_sha256.as_deref(),
                None,
            );
            let values = [
                markdown_cell(&fixture.id),
                markdown_cell(fixture.corpus.as_deref().unwrap_or("unavailable")),
                markdown_cell(fixture.revision.as_deref().unwrap_or("unavailable")),
                markdown_cell(&fixture.content_sha256),
                fixture.source_file_count.to_string(),
                fixture.source_line_count.to_string(),
                fixture.source_byte_count.to_string(),
                markdown_cell(&solc),
                markdown_cell(&foundry),
            ];
            let _ = writeln!(output, "| {} |", values.join(" | "));
        }
    }
    let capabilities = summary
        .workloads
        .iter()
        .map(|workload| {
            ((workload.fixture.as_str(), workload.id.as_str()), workload.methods.join(", "))
        })
        .collect::<BTreeMap<_, _>>();
    output.push_str(
        "\n## Results\n\n| Server | Fixture | Workload | Capabilities | Successful | Statuses | Result | Metric | p50 | p95 | p99 | Max |\n|---|---|---|---|---:|---|---|---|---:|---:|---:|---:|\n",
    );
    for group in &summary.summaries {
        let capability = capabilities
            .get(&(group.fixture.as_str(), group.workload.as_str()))
            .map(String::as_str)
            .filter(|methods| !methods.is_empty())
            .unwrap_or("-");
        let capability = markdown_cell(capability);
        let statuses = group
            .status_counts
            .iter()
            .map(|(status, count)| format!("{status}:{count}"))
            .collect::<Vec<_>>()
            .join(", ");
        let result = markdown_result(group);
        if group.metrics.is_empty() {
            let _ = writeln!(
                output,
                "| {} | {} | {} | {} | {} | {} | {} | - | - | - | - | - |",
                group.server,
                group.fixture,
                group.workload,
                capability,
                group.successful_runs,
                statuses,
                result,
            );
        }
        for (name, stats) in &group.metrics {
            let _ = writeln!(
                output,
                "| {} | {} | {} | {} | {} | {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} |",
                group.server,
                group.fixture,
                group.workload,
                capability,
                group.successful_runs,
                statuses,
                result,
                name,
                stats.p50,
                stats.p95,
                stats.p99,
                stats.max,
            );
        }
    }
    output
}

const fn yes_no(value: bool) -> &'static str {
    if value { "yes" } else { "no" }
}

fn markdown_cell(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ").replace('|', "\\|")
}

fn server_status_name(status: ServerStatus) -> &'static str {
    match status {
        ServerStatus::Available => "available",
        ServerStatus::Disabled => "disabled",
        ServerStatus::Incompatible => "incompatible",
        ServerStatus::Unavailable => "unavailable",
    }
}

fn compiler_provenance(
    compiler: Option<&crate::config::CompilerSpec>,
    native_sha256: Option<&str>,
    soljson_sha256: Option<&str>,
) -> String {
    let Some(compiler) = compiler else { return "unavailable".into() };
    let mut value =
        format!("{}; native={}", compiler.version, native_sha256.unwrap_or("unavailable"));
    if compiler.soljson.is_some() {
        value.push_str("; soljson=");
        value.push_str(soljson_sha256.unwrap_or("unavailable"));
    }
    value
}

fn markdown_result(group: &SummaryGroup) -> &'static str {
    match group.status {
        SummaryStatus::Pass => ":green_circle: PASS",
        SummaryStatus::Partial => ":yellow_circle: **PARTIAL**",
        SummaryStatus::Unsupported => ":yellow_circle: **UNSUPPORTED**",
        SummaryStatus::Unavailable => ":red_circle: **UNAVAILABLE**",
        SummaryStatus::Failed => ":red_circle: **FAILED**",
    }
}

fn summary_status(
    status_counts: &BTreeMap<String, usize>,
    successful_runs: usize,
) -> SummaryStatus {
    let has = |status| status_counts.get(status).is_some_and(|count| *count != 0);
    if ["incorrect", "incompatible", "timeout", "crash", "harness-error"].into_iter().any(has) {
        SummaryStatus::Failed
    } else if has("unavailable") {
        SummaryStatus::Unavailable
    } else if has("unsupported") && successful_runs == 0 {
        SummaryStatus::Unsupported
    } else if has("unsupported") {
        SummaryStatus::Partial
    } else {
        SummaryStatus::Pass
    }
}

fn status_name(status: &RunStatus) -> &'static str {
    match status {
        RunStatus::Pass => "pass",
        RunStatus::Unsupported => "unsupported",
        RunStatus::Incorrect => "incorrect",
        RunStatus::Incompatible => "incompatible",
        RunStatus::Timeout => "timeout",
        RunStatus::Crash => "crash",
        RunStatus::Unavailable => "unavailable",
        RunStatus::HarnessError => "harness-error",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::process::{Direction, RequestMeasurement, TraceEvent};
    use snapbox::{assert_data_eq, str};

    fn process_metrics(
        accounting: ProcessAccounting,
        memory_accounting: MemoryAccounting,
        peak_memory_mib: Option<f64>,
    ) -> ProcessMetrics {
        ProcessMetrics {
            wall_ms: 1.0,
            user_cpu_ms: Some(2.0),
            system_cpu_ms: Some(3.0),
            peak_memory_mib,
            peak_process_tree_rss_mib: (accounting == ProcessAccounting::CgroupV2ProcessTree)
                .then_some(1.5),
            accounting,
            memory_accounting,
            process_tree: accounting == ProcessAccounting::CgroupV2ProcessTree,
            network_isolated: true,
            cgroup_path: None,
            exit_code: Some(0),
            forced_kill: false,
            stderr: String::new(),
        }
    }

    fn sample(process: ProcessMetrics) -> RunSample {
        RunSample {
            server: "server".into(),
            fixture: "fixture".into(),
            workload: "workload".into(),
            repetition: 0,
            status: RunStatus::Pass,
            timings_ms: BTreeMap::new(),
            process: Some(process),
            setup_phases: Vec::new(),
            observations: Observations::default(),
            correctness: Vec::new(),
            error: None,
        }
    }

    fn summary_with_groups(summaries: Vec<SummaryGroup>) -> SummaryReport {
        SummaryReport {
            schema_version: RESULT_SCHEMA_VERSION,
            config_schema_version: crate::config::SCHEMA_VERSION,
            config_path: "benchmark.yaml".into(),
            config_sha256: "c".repeat(64),
            servers_lock_sha256: Some("b".repeat(64)),
            fixtures_lock_sha256: Some("f".repeat(64)),
            profile: "test".into(),
            harness_version: env!("CARGO_PKG_VERSION").into(),
            harness_contract_sha256: None,
            rustc_version: None,
            npm_runtime: None,
            harness_git_revision: None,
            harness_git_dirty: None,
            repeat_override: None,
            timeout_ms: 1_000,
            environment: Environment {
                os: "linux".into(),
                architecture: "x86_64".into(),
                logical_cpus: 8,
                accounting_backends: Vec::new(),
                memory_accounting_backends: Vec::new(),
                network_isolated: true,
                authoritative: true,
            },
            servers: Vec::new(),
            fixtures: Vec::new(),
            workloads: Vec::new(),
            summaries,
        }
    }

    fn metric_stats(count: usize, mean: f64, p50: f64, p95: f64) -> Stats {
        Stats { count, mean, p50, p95, p99: p95, max: p95 }
    }

    fn add_unknown_field(mut value: Value, pointer: &str) -> Value {
        let target =
            if pointer.is_empty() { &mut value } else { value.pointer_mut(pointer).unwrap() };
        target.as_object_mut().unwrap().insert("unexpected".into(), Value::Bool(true));
        value
    }

    #[test]
    fn summary_artifact_rejects_unknown_fields() {
        let group = SummaryGroup {
            server: "server".into(),
            fixture: "fixture".into(),
            workload: "workload".into(),
            successful_runs: 1,
            status_counts: BTreeMap::from([("pass".into(), 1)]),
            status: SummaryStatus::Pass,
            metrics: BTreeMap::from([("hover".into(), metric_stats(1, 1.0, 1.0, 1.0))]),
        };
        let summary = serde_json::to_value(summary_with_groups(vec![group])).unwrap();
        for pointer in ["", "/environment", "/summaries/0", "/summaries/0/metrics/hover"] {
            assert!(
                serde_json::from_value::<SummaryReport>(add_unknown_field(
                    summary.clone(),
                    pointer
                ))
                .is_err()
            );
        }
    }

    #[test]
    fn stats_include_p99_nearest_rank() {
        let stats = Stats::new(&[1.0, 2.0, 3.0, 4.0, 100.0]);
        assert_eq!(stats.p50, 3.0);
        assert_eq!(stats.p95, 100.0);
        assert_eq!(stats.p99, 100.0);
    }

    #[test]
    fn rustc_provenance_is_single_line() {
        let verbose = "rustc 1.96.0\nbinary: rustc\r\ncommit-hash: abc\n";
        assert_eq!(
            normalize_multiline_output(verbose),
            "rustc 1.96.0; binary: rustc; commit-hash: abc"
        );
    }

    #[test]
    fn empty_stats_are_explicitly_zero() {
        assert_eq!(Stats::new(&[]).count, 0);
        assert_eq!(Stats::new(&[]).p99, 0.0);
    }

    #[test]
    fn cgroup_memory_is_serialized_as_total_memory_not_rss() {
        let metrics = process_metrics(
            ProcessAccounting::CgroupV2ProcessTree,
            MemoryAccounting::CgroupV2Total,
            Some(4.0),
        );
        assert_eq!(metrics.peak_memory_metric(), Some(("peak_cgroup_memory_mib", 4.0)));
        let value = serde_json::to_value(&metrics).unwrap();
        assert_eq!(value["memory_accounting"], "cgroup-v2-total");
        assert_eq!(value["peak_memory_mib"], 4.0);
        assert!(value.get("peak_rss_mib").is_none());
        let direct_child = process_metrics(
            ProcessAccounting::RusageDirectChild,
            MemoryAccounting::RusageMaxRssDirectChild,
            Some(2.0),
        );
        assert_eq!(direct_child.peak_memory_metric(), Some(("peak_direct_child_rss_mib", 2.0)));
    }

    #[test]
    fn authority_requires_complete_tree_metrics_in_all_phases() {
        let complete = process_metrics(
            ProcessAccounting::CgroupV2ProcessTree,
            MemoryAccounting::CgroupV2Total,
            Some(4.0),
        );
        let missing_memory = process_metrics(
            ProcessAccounting::CgroupV2ProcessTree,
            MemoryAccounting::Unavailable,
            None,
        );
        let missing = sample(missing_memory);
        assert!(!samples_have_authoritative_metrics(&[&missing]));
        assert!(!Environment::current(&[missing]).authoritative);
        let mut missing_request_cpu = sample(complete.clone());
        missing_request_cpu.observations.requests.push(RequestMeasurement {
            method: "textDocument/hover".into(),
            elapsed_ms: 1.0,
            process_tree_cpu_ms: None,
        });
        assert!(!samples_have_authoritative_metrics(&[&missing_request_cpu]));
        let complete_sample = sample(complete);
        assert!(samples_have_authoritative_metrics(&[&complete_sample]));
    }

    #[test]
    fn markdown_keeps_failed_groups_visible_without_metrics() {
        let group = SummaryGroup {
            server: "external".into(),
            fixture: "synthetic".into(),
            workload: "correctness".into(),
            successful_runs: 0,
            status_counts: BTreeMap::from([("incorrect".into(), 1)]),
            status: SummaryStatus::Failed,
            metrics: BTreeMap::new(),
        };
        let mut summary = summary_with_groups(vec![group]);
        summary.workloads.push(WorkloadMetadata {
            id: "correctness".into(),
            fixture: "synthetic".into(),
            methods: vec!["textDocument/didChange".into(), "textDocument/didSave".into()],
            step_count: 2,
            repetitions: 1,
        });
        let output = markdown(&summary);
        assert_data_eq!(
            output,
            str![[r#"
# Cross-server Solidity LSP benchmark
...
## Run metadata
...
## Results
...
| external | synthetic | correctness | textDocument/didChange, textDocument/didSave | 0 | incorrect:1 | :red_circle: **FAILED** | - | - | - | - | - |
...
"#]],
        );
    }

    #[test]
    fn markdown_includes_server_and_fixture_provenance() {
        let mut summary = summary_with_groups(Vec::new());
        summary.servers.push(ServerMetadata {
            id: "server".into(),
            label: Some("Server 1".into()),
            command: "/bin/server".into(),
            args: vec!["--stdio".into()],
            transport: TransportSpec::Stdio,
            version_args: vec!["--version".into()],
            version: Some("server 1.0".into()),
            locked_version: Some("1.0".into()),
            expected_version: None,
            enabled: true,
            env: BTreeMap::new(),
            initialization_options: Value::Null,
            configuration: Value::Null,
            source: Some(crate::config::SourceSpec {
                url: "https://example.invalid/server.git".into(),
                revision: "1".repeat(40),
            }),
            source_revision_override: None,
            executable_sha256: Some("2".repeat(64)),
            artifact_path: None,
            artifact_expected_sha256: None,
            artifact_sha256: None,
            required: true,
            status: ServerStatus::Available,
            error: None,
        });
        summary.fixtures.push(crate::fixture::FixtureMetadata {
            id: "fixture".into(),
            root: "/fixture".into(),
            revision: Some("3".repeat(40)),
            source_file_count: 2,
            source_line_count: 20,
            source_byte_count: 200,
            content_sha256: "4".repeat(64),
            corpus: Some("Fixture corpus".into()),
            source: None,
            solc: None,
            solc_native_sha256: None,
            solc_soljson_sha256: None,
            solc_native_version: None,
            foundry: None,
            foundry_native_sha256: None,
            foundry_native_version: None,
            dependencies: BTreeMap::new(),
        });
        let output = markdown(&summary);
        assert_data_eq!(
            output,
            str![[r#"
# Cross-server Solidity LSP benchmark
...
## Servers
...
| server | Server 1 | available | server 1.0 | 1.0 | [..] | [..] | unavailable |
...
## Fixtures
...
| fixture | Fixture corpus | [..] | [..] | 2 | 20 | 200 | unavailable | unavailable |
...
"#]],
        );
    }

    #[test]
    fn warm_summaries_exclude_whole_session_resource_totals() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("benchmark.yaml");
        fs::write(&config_path, "version: 1\nservers:\n  - id: server\n    command: server\nfixtures:\n  - id: fixture\n    root: .\nscenarios:\n  - id: cold\n    fixture: fixture\n    steps:\n      - kind: probe\n        name: cold-ready\n        probe:\n          kind: document-symbol\n          path: Main.sol\n  - id: warm\n    fixture: fixture\n    steps:\n      - kind: warm\n        name: hover\n        probe:\n          kind: document-symbol\n          path: Main.sol\n").unwrap();
        let config = Config::load(&config_path).unwrap();
        let metrics = process_metrics(
            ProcessAccounting::CgroupV2ProcessTree,
            MemoryAccounting::CgroupV2Total,
            Some(4.0),
        );
        let mut cold = sample(metrics.clone());
        cold.workload = "cold".into();
        cold.timings_ms.insert("cold_ready_ms".into(), 1.0);
        let mut warm = sample(metrics);
        warm.workload = "warm".into();
        warm.timings_ms.insert("spawn_to_initialize_response_ms".into(), 1.0);
        warm.timings_ms.insert("ready_ms".into(), 1.5);
        warm.timings_ms.insert("warm_hover_0_ms".into(), 2.0);
        warm.timings_ms.insert("warm_hover_1_ms".into(), 2.1);
        warm.observations.requests.extend((0..2).map(|_| RequestMeasurement {
            method: "textDocument/documentSymbol".into(),
            elapsed_ms: 1.8,
            process_tree_cpu_ms: Some(0.4),
        }));
        let samples = [cold, warm];
        let repetitions = BTreeMap::from([("cold".into(), 1), ("warm".into(), 1)]);
        let summary = summarize(SummaryInput {
            config_path,
            config: &config,
            servers: Vec::new(),
            fixtures: Vec::new(),
            samples: &samples,
            repeat_override: None,
            workload_repetitions: &repetitions,
            timeout_ms: 1_000,
            profile: "test".into(),
        });
        let cold = summary.summaries.iter().find(|group| group.workload == "cold").unwrap();
        assert!(cold.metrics.contains_key("session_cpu_ms"));
        let warm = summary.summaries.iter().find(|group| group.workload == "warm").unwrap();
        assert_eq!(warm.metrics["textDocument/documentSymbol"].count, 2);
        assert_eq!(warm.metrics["textDocument/documentSymbol_cpu_ms"].count, 2);
        assert!(!warm.metrics.contains_key("warm_hover_ms"));
    }

    #[test]
    fn write_reports_publishes_all_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let summary = summary_with_groups(Vec::new());
        let samples = Vec::new();
        write_reports(directory.path(), &summary, &samples).unwrap();
        for name in ["summary.json", "samples.json", "samples.jsonl", "summary.md"] {
            assert!(directory.path().join(name).is_file(), "missing {name}");
        }
        let rendered = fs::read_to_string(directory.path().join("summary.md")).unwrap();
        assert!(rendered.starts_with("# Cross-server Solidity LSP benchmark"));
    }

    #[test]
    fn observations_retain_server_request_trace_shape() {
        let mut observations = Observations::default();
        observations.events.push(TraceEvent {
            elapsed_ms: 1.0,
            direction: Direction::Send,
            method: Some("initialize".into()),
            id: Some(Value::from(1)),
            message: serde_json::json!({"jsonrpc": "2.0"}),
        });
        observations.server_requests.push(crate::process::ServerRequest {
            method: "workspace/configuration".into(),
            handled: true,
            error_code: None,
        });
        let value = serde_json::to_value(observations).unwrap();
        assert_eq!(value["server_requests"][0]["handled"], true);
    }
}

//! Machine-readable samples and summaries for cross-server runs.

use crate::{
    config::{CompilerSpec, Config, TransportSpec},
    fixture::FixtureSource,
    process::{MemoryAccounting, Observations, ProcessAccounting, ProcessMetrics},
};
use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fmt::Write as _,
    fs,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
    process::Command,
};

pub(crate) const RESULT_SCHEMA_VERSION: u32 = 5;
pub(crate) const COMPARISON_SCHEMA_VERSION: u32 = 2;

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
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
    pub(crate) executable_sha256: Option<String>,
    pub(crate) artifact_path: Option<PathBuf>,
    pub(crate) artifact_expected_sha256: Option<String>,
    pub(crate) artifact_sha256: Option<String>,
    pub(crate) required: bool,
    pub(crate) status: ServerStatus,
    pub(crate) error: Option<String>,
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
pub(crate) struct WorkloadMetadata {
    pub(crate) id: String,
    pub(crate) fixture: String,
    pub(crate) methods: Vec<String>,
    pub(crate) step_count: usize,
    pub(crate) repetitions: usize,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
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

struct LoadedSummary {
    summary: SummaryReport,
    sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ComparisonVerdict {
    Regression,
    Improvement,
    Stable,
    Inconclusive,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ComparisonRow {
    pub(crate) server: String,
    pub(crate) fixture: String,
    pub(crate) workload: String,
    pub(crate) metric: String,
    pub(crate) baseline_status: Option<SummaryStatus>,
    pub(crate) candidate_status: Option<SummaryStatus>,
    pub(crate) expected_runs: Option<usize>,
    pub(crate) baseline_successful_runs: Option<usize>,
    pub(crate) candidate_successful_runs: Option<usize>,
    pub(crate) baseline_count: Option<usize>,
    pub(crate) candidate_count: Option<usize>,
    pub(crate) baseline_mean: Option<f64>,
    pub(crate) candidate_mean: Option<f64>,
    pub(crate) mean_delta_pct: Option<f64>,
    pub(crate) baseline_p50: Option<f64>,
    pub(crate) candidate_p50: Option<f64>,
    pub(crate) p50_delta_pct: Option<f64>,
    pub(crate) baseline_p95: Option<f64>,
    pub(crate) candidate_p95: Option<f64>,
    pub(crate) p95_delta_pct: Option<f64>,
    pub(crate) verdict: ComparisonVerdict,
    pub(crate) reason: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ComparisonSource {
    pub(crate) path: PathBuf,
    pub(crate) summary_sha256: Option<String>,
    pub(crate) source_url: Option<String>,
    pub(crate) revision: Option<String>,
    pub(crate) executable_sha256: Option<String>,
}

#[derive(Clone, Debug, Serialize)]
pub(crate) struct ComparisonReport {
    pub(crate) schema_version: u32,
    pub(crate) baseline: ComparisonSource,
    pub(crate) candidate: ComparisonSource,
    pub(crate) threshold_pct: f64,
    pub(crate) min_samples: usize,
    pub(crate) compatible: bool,
    pub(crate) blockers: Vec<String>,
    pub(crate) comparable_metrics: usize,
    pub(crate) regressions: usize,
    pub(crate) improvements: usize,
    pub(crate) stable: usize,
    pub(crate) inconclusive: usize,
    pub(crate) rows: Vec<ComparisonRow>,
}

impl ComparisonReport {
    pub(crate) fn has_regressions(&self) -> bool {
        self.regressions != 0
    }
}

#[derive(Serialize)]
struct SamplesReport<'a> {
    schema_version: u32,
    samples: &'a [RunSample],
}

#[derive(Deserialize)]
struct OwnedSamplesReport {
    schema_version: u32,
    samples: Vec<RunSample>,
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

#[cfg(test)]
pub(crate) fn validate_summary_manifest_contract(
    config: &Config,
    profile_name: &str,
    summary: &SummaryReport,
) -> Result<()> {
    validate_summary_manifest_contract_for_servers(config, profile_name, summary, &BTreeSet::new())
}

fn selected_server_specs<'a>(
    config: &'a Config,
    selected_servers: &BTreeSet<String>,
) -> Result<Vec<&'a crate::config::ServerSpec>> {
    let servers = config
        .servers
        .iter()
        .filter(|server| {
            server.enabled && (selected_servers.is_empty() || selected_servers.contains(&server.id))
        })
        .collect::<Vec<_>>();
    if selected_servers.is_empty() {
        return Ok(servers);
    }

    let matched = servers.iter().map(|server| server.id.as_str()).collect::<BTreeSet<_>>();
    let missing = selected_servers
        .iter()
        .filter(|server| !matched.contains(server.as_str()))
        .cloned()
        .collect::<Vec<_>>();
    if !missing.is_empty() {
        bail!(
            "selected servers are missing or disabled in the benchmark manifest: {}",
            missing.join(", ")
        )
    }
    Ok(servers)
}

fn validate_summary_manifest_contract_for_servers(
    config: &Config,
    profile_name: &str,
    summary: &SummaryReport,
    selected_servers: &BTreeSet<String>,
) -> Result<()> {
    let profile = config
        .profiles
        .get(profile_name)
        .with_context(|| format!("benchmark profile `{profile_name}` is not defined"))?;
    if summary.schema_version != RESULT_SCHEMA_VERSION {
        bail!(
            "unsupported result schema {}; expected {}",
            summary.schema_version,
            RESULT_SCHEMA_VERSION
        )
    }
    if summary.config_schema_version != config.schema_version {
        bail!("summary config schema does not match the loaded benchmark manifest")
    }
    if summary.profile != profile_name {
        bail!("summary profile does not match `{profile_name}`")
    }
    if summary.config_sha256 != config.config_sha256 {
        bail!("summary config digest does not match the loaded benchmark manifest")
    }
    if summary.servers_lock_sha256 != config.servers_lock_sha256 {
        bail!("summary server lock digest does not match the loaded benchmark manifest")
    }
    if summary.fixtures_lock_sha256 != config.fixtures_lock_sha256 {
        bail!("summary fixture lock digest does not match the loaded benchmark manifest")
    }
    if summary.repeat_override.is_some() {
        bail!("publication summary contains a repetition override")
    }
    if summary.timeout_ms != profile.timeout_ms {
        bail!("summary timeout does not match the publication profile")
    }
    if summary.harness_version != env!("CARGO_PKG_VERSION") {
        bail!("summary harness version does not match the validating harness")
    }
    let harness_contract_sha256 =
        harness_contract_sha256().context("validating harness contract digest is unavailable")?;
    if summary.harness_contract_sha256.as_deref() != Some(harness_contract_sha256.as_str()) {
        bail!("summary harness contract digest does not match the validating harness")
    }
    let rustc_version = command_output("rustc", &["--version", "--verbose"])
        .map(|output| normalize_multiline_output(&output))
        .context("validating Rust compiler evidence is unavailable")?;
    if summary.rustc_version.as_deref() != Some(rustc_version.as_str()) {
        bail!("summary Rust compiler evidence does not match the validating environment")
    }
    let (harness_git_revision, harness_git_dirty) = harness_git_provenance();
    let harness_git_revision =
        harness_git_revision.context("validating harness revision is unavailable")?;
    if summary.harness_git_revision.as_deref() != Some(harness_git_revision.as_str()) {
        bail!("summary harness revision does not match the current checkout")
    }
    if summary.harness_git_dirty != harness_git_dirty {
        bail!("summary harness dirty state does not match the current checkout")
    }

    let server_specs = selected_server_specs(config, selected_servers)?;
    let expected_servers =
        server_specs.iter().map(|server| server.id.as_str()).collect::<BTreeSet<_>>();
    let actual_servers =
        summary.servers.iter().map(|server| server.id.as_str()).collect::<Vec<_>>();
    if expected_servers.len() != actual_servers.len()
        || actual_servers.iter().copied().collect::<BTreeSet<_>>() != expected_servers
    {
        bail!("summary server selection does not match the publication manifest")
    }
    for spec in server_specs {
        let metadata = summary
            .servers
            .iter()
            .find(|server| server.id == spec.id)
            .expect("server selection was validated above");
        validate_server_evidence(spec, metadata)?;
    }

    let selected_workload_ids = if profile.scenarios.is_empty() {
        config.workloads.iter().map(|workload| workload.id.as_str()).collect::<BTreeSet<_>>()
    } else {
        profile.scenarios.iter().map(String::as_str).collect::<BTreeSet<_>>()
    };
    let expected_workloads = config
        .workloads
        .iter()
        .filter(|workload| selected_workload_ids.contains(workload.id.as_str()))
        .map(|workload| {
            let metadata = WorkloadMetadata {
                id: workload.id.clone(),
                fixture: workload.fixture.clone(),
                methods: workload.methods.clone(),
                step_count: workload.steps.len(),
                repetitions: profile.repetitions_for(workload),
            };
            (metadata.id.clone(), metadata)
        })
        .collect::<BTreeMap<_, _>>();
    let actual_workloads = summary
        .workloads
        .iter()
        .cloned()
        .map(|workload| (workload.id.clone(), workload))
        .collect::<BTreeMap<_, _>>();
    if expected_workloads.len() != summary.workloads.len() || expected_workloads != actual_workloads
    {
        bail!("summary workload selection does not match the publication profile")
    }

    let expected_fixtures = expected_workloads
        .values()
        .map(|workload| workload.fixture.as_str())
        .collect::<BTreeSet<_>>();
    let actual_fixtures =
        summary.fixtures.iter().map(|fixture| fixture.id.as_str()).collect::<Vec<_>>();
    if expected_fixtures.len() != actual_fixtures.len()
        || actual_fixtures.iter().copied().collect::<BTreeSet<_>>() != expected_fixtures
    {
        bail!("summary fixture selection does not match the publication profile")
    }
    for fixture_id in expected_fixtures {
        let spec = config
            .fixtures
            .iter()
            .find(|fixture| fixture.id == fixture_id)
            .expect("workload fixtures were validated when loading the manifest");
        let expected = FixtureSource::open(spec)
            .with_context(|| format!("failed to verify fixture evidence for `{fixture_id}`"))?;
        let actual = summary
            .fixtures
            .iter()
            .find(|fixture| fixture.id == fixture_id)
            .expect("fixture selection was validated above");
        if serde_json::to_value(expected.metadata())? != serde_json::to_value(actual)? {
            bail!("summary fixture evidence does not match manifest fixture `{fixture_id}`")
        }
    }

    let mut blockers = Vec::new();
    summary_integrity_blockers(&mut blockers, "publication", summary);
    if !blockers.is_empty() {
        bail!("invalid publication summary: {}", blockers.join("; "))
    }
    Ok(())
}

fn validate_server_evidence(
    spec: &crate::config::ServerSpec,
    metadata: &ServerMetadata,
) -> Result<()> {
    let command = crate::lifecycle::resolve_executable(&spec.command);
    let source_matches =
        serde_json::to_value(&metadata.source)? == serde_json::to_value(&spec.source)?;
    let artifact_path = spec.artifact.as_ref().map(|artifact| &artifact.path);
    let artifact_expected_sha256 =
        spec.artifact.as_ref().and_then(|artifact| artifact.sha256.as_ref());
    if metadata.label != spec.label
        || metadata.command != command
        || metadata.args != spec.args
        || metadata.transport != spec.transport
        || metadata.version_args != spec.version_args
        || metadata.locked_version != spec.locked_version
        || metadata.expected_version != spec.expected_version
        || metadata.enabled != spec.enabled
        || metadata.env != spec.env
        || metadata.initialization_options != spec.initialization_options
        || metadata.configuration != spec.configuration
        || !source_matches
        || metadata.artifact_path.as_ref() != artifact_path
        || metadata.artifact_expected_sha256.as_ref() != artifact_expected_sha256
        || metadata.required != spec.required
    {
        bail!("summary server evidence does not match manifest server `{}`", spec.id)
    }

    let executable_sha256 =
        command.is_file().then(|| crate::lifecycle::sha256_path(&command).ok()).flatten();
    let artifact_sha256 = spec
        .artifact
        .as_ref()
        .and_then(|artifact| crate::lifecycle::sha256_path(&artifact.path).ok());
    if metadata.executable_sha256 != executable_sha256
        || metadata.artifact_sha256 != artifact_sha256
    {
        bail!("summary server evidence does not match installed server `{}`", spec.id)
    }

    if metadata.status == ServerStatus::Available {
        let version = crate::lifecycle::inspect_version(
            &command,
            spec,
            crate::lifecycle::VERSION_PROBE_TIMEOUT,
        )
        .with_context(|| format!("failed to verify server evidence for `{}`", spec.id))?;
        crate::lifecycle::verify_server_version_output(spec, &version)
            .with_context(|| format!("failed to verify server evidence for `{}`", spec.id))?;
        if metadata.version.as_deref() != Some(version.as_str()) || metadata.error.is_some() {
            bail!("summary server evidence does not match observed server `{}`", spec.id)
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) fn validate_results_directory(
    config_path: &Path,
    input: &Path,
    profile_name: &str,
    require_authoritative: bool,
) -> Result<()> {
    validate_results_directory_for_servers(
        config_path,
        input,
        profile_name,
        require_authoritative,
        &BTreeSet::new(),
    )
}

pub(crate) fn validate_results_directory_for_servers(
    config_path: &Path,
    input: &Path,
    profile_name: &str,
    require_authoritative: bool,
    selected_servers: &BTreeSet<String>,
) -> Result<()> {
    let config = Config::load(config_path)?;
    let summary_path = input.join("summary.json");
    let summary = read_summary(&summary_path, "publication")?.summary;
    validate_summary_manifest_contract_for_servers(
        &config,
        profile_name,
        &summary,
        selected_servers,
    )?;
    if require_authoritative && !summary.environment.authoritative {
        bail!("benchmark summary is not authoritative")
    }

    let samples_path = input.join("samples.json");
    let samples_bytes = fs::read(&samples_path)
        .with_context(|| format!("failed to read raw samples `{}`", samples_path.display()))?;
    let samples_value = serde_json::from_slice::<Value>(&samples_bytes)
        .with_context(|| format!("failed to parse raw samples `{}`", samples_path.display()))?;
    let sample_values = samples_value
        .get("samples")
        .and_then(Value::as_array)
        .context("raw samples JSON has no `samples` array")?;
    let samples_report = serde_json::from_slice::<OwnedSamplesReport>(&samples_bytes)
        .with_context(|| format!("failed to decode raw samples `{}`", samples_path.display()))?;
    if samples_report.schema_version != RESULT_SCHEMA_VERSION {
        bail!(
            "unsupported raw sample schema {}; expected {}",
            samples_report.schema_version,
            RESULT_SCHEMA_VERSION
        )
    }

    let jsonl_path = input.join("samples.jsonl");
    let jsonl = BufReader::new(
        fs::File::open(&jsonl_path)
            .with_context(|| format!("failed to read raw JSONL `{}`", jsonl_path.display()))?,
    );
    let jsonl_values = jsonl
        .lines()
        .enumerate()
        .map(|(index, line)| {
            let line = line
                .with_context(|| format!("failed to read raw JSONL `{}`", jsonl_path.display()))?;
            serde_json::from_str::<Value>(&line).with_context(|| {
                format!("failed to parse raw JSONL `{}` line {}", jsonl_path.display(), index + 1)
            })
        })
        .collect::<Result<Vec<_>>>()?;
    if sample_values != jsonl_values.as_slice() {
        bail!("samples.jsonl rows do not exactly match samples.json")
    }

    validate_raw_sample_contract_for_servers(
        config_path,
        &config,
        profile_name,
        &summary,
        &samples_report.samples,
        selected_servers,
    )
}

#[cfg(test)]
fn validate_raw_sample_contract(
    config_path: &Path,
    config: &Config,
    profile_name: &str,
    summary: &SummaryReport,
    samples: &[RunSample],
) -> Result<()> {
    validate_raw_sample_contract_for_servers(
        config_path,
        config,
        profile_name,
        summary,
        samples,
        &BTreeSet::new(),
    )
}

fn validate_raw_sample_contract_for_servers(
    config_path: &Path,
    config: &Config,
    profile_name: &str,
    summary: &SummaryReport,
    samples: &[RunSample],
    selected_servers: &BTreeSet<String>,
) -> Result<()> {
    let profile = &config.profiles[profile_name];
    let selected_ids = if profile.scenarios.is_empty() {
        config.workloads.iter().map(|workload| workload.id.as_str()).collect::<BTreeSet<_>>()
    } else {
        profile.scenarios.iter().map(String::as_str).collect::<BTreeSet<_>>()
    };
    let workloads = config
        .workloads
        .iter()
        .filter(|workload| selected_ids.contains(workload.id.as_str()))
        .map(|workload| (workload.id.as_str(), workload))
        .collect::<BTreeMap<_, _>>();
    let servers = selected_server_specs(config, selected_servers)?;
    let expected_keys = servers
        .iter()
        .flat_map(|server| {
            workloads.values().flat_map(move |workload| {
                (0..profile.repetitions_for(workload)).map(move |repetition| {
                    (
                        server.id.as_str(),
                        workload.fixture.as_str(),
                        workload.id.as_str(),
                        repetition,
                    )
                })
            })
        })
        .collect::<BTreeSet<_>>();
    let mut actual_keys = BTreeSet::new();
    for sample in samples {
        let key = (
            sample.server.as_str(),
            sample.fixture.as_str(),
            sample.workload.as_str(),
            sample.repetition,
        );
        if !actual_keys.insert(key) {
            bail!(
                "raw samples contain duplicate key `{}/{}/{}/{}`",
                sample.server,
                sample.fixture,
                sample.workload,
                sample.repetition
            )
        }
        let workload = workloads.get(sample.workload.as_str()).with_context(|| {
            format!("raw sample refers to unknown workload `{}`", sample.workload)
        })?;
        if sample.fixture != workload.fixture {
            bail!("raw sample fixture does not match workload `{}`", sample.workload)
        }
        if let Some(process) = &sample.process {
            validate_process_metrics(process, "raw sample process")?;
        }
        validate_observation_metrics(&sample.observations, "raw sample request")?;
        for phase in &sample.setup_phases {
            validate_process_metrics(&phase.process, "raw setup process")?;
            validate_observation_metrics(&phase.observations, "raw setup request")?;
        }
        for (name, value) in &sample.timings_ms {
            if !value.is_finite() || *value < 0.0 {
                bail!("raw sample metric `{name}` is not finite and nonnegative")
            }
        }
        if sample.succeeded() {
            let server =
                summary.servers.iter().find(|server| server.id == sample.server).with_context(
                    || format!("passing raw sample refers to unknown server `{}`", sample.server),
                )?;
            if server.status != ServerStatus::Available {
                bail!("passing raw sample has an unavailable server `{}`", sample.server)
            }
            let process =
                sample.process.as_ref().context("passing raw sample has no process data")?;
            if process.exit_code != Some(0) || process.forced_kill {
                bail!("passing raw sample has an unsuccessful process exit")
            }
            if sample.error.is_some() {
                bail!("passing raw sample contains an error")
            }
            if sample.correctness.iter().any(|result| !result.ok) {
                bail!("passing raw sample contains a failed correctness result")
            }
            validate_required_correctness(sample, workload)?;
            let has_restart = workload
                .steps
                .iter()
                .any(|step| matches!(step, crate::config::StepSpec::Restart { .. }));
            if has_restart
                && (sample.setup_phases.len() != 1
                    || sample.setup_phases[0].name != "cache-population")
                || !has_restart && !sample.setup_phases.is_empty()
            {
                bail!("passing raw sample setup phases do not match the manifest")
            }
            if sample
                .setup_phases
                .iter()
                .any(|phase| phase.process.exit_code != Some(0) || phase.process.forced_kill)
            {
                bail!("passing raw sample has an unsuccessful setup process exit")
            }
            let warm_request = warm_request_contract(workload, profile);
            if let Some((method, expected)) = warm_request {
                if workload.methods.len() != 1
                    || workload.methods.first().map(String::as_str) != Some(method)
                {
                    bail!("warm workload methods do not match its manifest probe")
                }
                if sample.observations.requests.len() != expected {
                    bail!(
                        "passing warm raw sample contains {} measured requests; expected {expected}",
                        sample.observations.requests.len()
                    )
                }
                if sample.observations.requests.iter().any(|request| request.method != method) {
                    bail!("passing warm raw sample request methods do not match the manifest")
                }
                if summary.environment.authoritative
                    && !sample.observations.has_authoritative_process_tree_request_metrics()
                {
                    bail!("authoritative warm raw sample has incomplete request CPU evidence")
                }
            } else if sample.timings_ms.is_empty() {
                bail!("passing non-warm raw sample contains no timing metrics")
            }
        } else if sample.error.is_none() {
            bail!("failing raw sample has no error")
        }
    }
    if actual_keys != expected_keys {
        bail!("raw sample matrix does not match the publication manifest")
    }

    let successful = samples.iter().filter(|sample| sample.succeeded()).collect::<Vec<_>>();
    let derived_environment = Environment::current(samples);
    if summary.environment != derived_environment {
        bail!("summary environment accounting does not match raw process evidence")
    }
    if summary.environment.authoritative && !samples_have_authoritative_metrics(&successful) {
        bail!("raw samples do not contain authoritative process evidence")
    }

    let repetitions = workloads
        .values()
        .map(|workload| (workload.id.clone(), profile.repetitions_for(workload)))
        .collect::<BTreeMap<_, _>>();
    let recomputed = summarize(SummaryInput {
        config_path: config_path.to_path_buf(),
        config,
        servers: summary.servers.clone(),
        fixtures: summary.fixtures.clone(),
        samples,
        repeat_override: None,
        workload_repetitions: &repetitions,
        timeout_ms: profile.timeout_ms,
        profile: profile_name.into(),
    });
    validate_recomputed_summaries(&summary.summaries, &recomputed.summaries)?;
    for group in &summary.summaries {
        if group.status != SummaryStatus::Pass {
            continue;
        }
        if group.metrics.is_empty() {
            bail!("passing summary group `{}` contains no performance metrics", group.workload)
        }
        let workload = workloads[&group.workload.as_str()];
        let repetitions = profile.repetitions_for(workload);
        if let Some((method, samples)) = warm_request_contract(workload, profile) {
            let expected_count = repetitions * samples;
            let cpu_metric = format!("{method}_cpu_ms");
            if group.metrics.get(method).is_none_or(|stats| stats.count != expected_count)
                || summary.environment.authoritative
                    && group
                        .metrics
                        .get(&cpu_metric)
                        .is_none_or(|stats| stats.count != expected_count)
                || summary.environment.authoritative
                    && group.metrics.keys().any(|metric| metric != method && metric != &cpu_metric)
            {
                bail!(
                    "passing summary group `{}` metrics do not match manifest warm observations",
                    group.workload
                )
            }
        } else if group.metrics.values().any(|stats| stats.count != repetitions) {
            bail!(
                "passing summary group `{}` metric counts do not match expected observations",
                group.workload
            )
        }
    }
    Ok(())
}

fn validate_recomputed_summaries(
    published: &[SummaryGroup],
    recomputed: &[SummaryGroup],
) -> Result<()> {
    if published.len() != recomputed.len() {
        bail!("summary group count does not match the raw samples")
    }
    for (published, recomputed) in published.iter().zip(recomputed) {
        let key = format!("{}/{}/{}", published.server, published.fixture, published.workload);
        if published.server != recomputed.server
            || published.fixture != recomputed.fixture
            || published.workload != recomputed.workload
        {
            bail!("summary group identity does not match the raw samples at `{key}`")
        }
        if published.successful_runs != recomputed.successful_runs
            || published.status_counts != recomputed.status_counts
            || published.status != recomputed.status
        {
            bail!("summary group status does not match the raw samples for `{key}`")
        }
        if published.metrics.len() != recomputed.metrics.len()
            || published.metrics.keys().ne(recomputed.metrics.keys())
        {
            bail!("summary metric selection does not match the raw samples for `{key}`")
        }
        for (name, published) in &published.metrics {
            let recomputed = &recomputed.metrics[name];
            if published.count != recomputed.count {
                bail!("summary metric count does not match the raw samples for `{key}/{name}`")
            }
            for (field, published, recomputed) in [
                ("mean", published.mean, recomputed.mean),
                ("p50", published.p50, recomputed.p50),
                ("p95", published.p95, recomputed.p95),
                ("p99", published.p99, recomputed.p99),
                ("max", published.max, recomputed.max),
            ] {
                if !equivalent_metric_value(published, recomputed) {
                    bail!(
                        "summary metric `{key}/{name}` {field} does not match the raw samples: published {published}, recomputed {recomputed}"
                    )
                }
            }
        }
    }
    Ok(())
}

// Summary and raw samples are serialized independently, so their parsed aggregates can differ by
// a few rounding bits.
fn equivalent_metric_value(left: f64, right: f64) -> bool {
    left == right
        || (left - right).abs() <= f64::EPSILON * 4.0 * left.abs().max(right.abs()).max(1.0)
}

fn warm_request_contract<'a>(
    workload: &'a crate::config::WorkloadSpec,
    profile: &crate::config::ProfileSpec,
) -> Option<(&'a str, usize)> {
    workload.steps.iter().find_map(|step| {
        let crate::config::StepSpec::Warm { probe, samples, .. } = step else { return None };
        let method = match probe {
            crate::config::ProbeSpec::Definition { .. } => "textDocument/definition",
            crate::config::ProbeSpec::Completion { .. } => "textDocument/completion",
            crate::config::ProbeSpec::Hover { .. } => "textDocument/hover",
            crate::config::ProbeSpec::References { .. } => "textDocument/references",
            crate::config::ProbeSpec::DocumentSymbol { .. } => "textDocument/documentSymbol",
            crate::config::ProbeSpec::WorkspaceSymbol { .. } => "workspace/symbol",
        };
        Some((method, samples.unwrap_or(profile.samples)))
    })
}

fn validate_process_metrics(process: &ProcessMetrics, role: &str) -> Result<()> {
    let values = [
        Some(process.wall_ms),
        process.user_cpu_ms,
        process.system_cpu_ms,
        process.peak_memory_mib,
        process.peak_process_tree_rss_mib,
    ];
    if values.into_iter().flatten().any(|value| !value.is_finite() || value < 0.0) {
        bail!("{role} contains a non-finite or negative metric")
    }
    if process
        .user_cpu_ms
        .zip(process.system_cpu_ms)
        .is_some_and(|(user, system)| !(user + system).is_finite())
    {
        bail!("{role} contains an overflowing CPU metric")
    }
    Ok(())
}

fn validate_observation_metrics(observations: &Observations, role: &str) -> Result<()> {
    for request in &observations.requests {
        if !request.elapsed_ms.is_finite() || request.elapsed_ms < 0.0 {
            bail!("{role} latency is not finite and nonnegative")
        }
        if request.process_tree_cpu_ms.is_some_and(|cpu| !cpu.is_finite() || cpu < 0.0) {
            bail!("{role} CPU metric is not finite and nonnegative")
        }
    }
    Ok(())
}

fn validate_required_correctness(
    sample: &RunSample,
    workload: &crate::config::WorkloadSpec,
) -> Result<()> {
    let restart = workload
        .steps
        .iter()
        .position(|step| matches!(step, crate::config::StepSpec::Restart { .. }));
    let mut required = BTreeMap::<String, usize>::new();
    for (index, step) in workload.steps.iter().enumerate() {
        let prefix =
            if restart.is_some_and(|restart| index < restart) { "cache-setup/" } else { "" };
        let probe = match step {
            crate::config::StepSpec::Probe { name, .. } => Some(name.as_str()),
            crate::config::StepSpec::Replace { probe: Some(_), .. } => Some("edit-ready"),
            crate::config::StepSpec::Save { probe: Some(_), .. } => Some("save-ready"),
            crate::config::StepSpec::Rename { probe: Some(_), .. } => Some("rename-ready"),
            crate::config::StepSpec::CreateFile { probe: Some(_), .. } => Some("create-file-ready"),
            crate::config::StepSpec::RenameFile { probe: Some(_), .. } => Some("rename-file-ready"),
            crate::config::StepSpec::DeleteFile { probe: Some(_), .. } => Some("delete-file-ready"),
            _ => None,
        };
        let Some(probe) = probe else { continue };
        *required.entry(format!("{prefix}{probe}")).or_default() += 1;
        if probe == "cold-ready" {
            *required.entry(format!("{prefix}workspace-indexed")).or_default() += 1;
        }
    }

    let mut observed = BTreeMap::<&str, usize>::new();
    for result in &sample.correctness {
        *observed.entry(result.probe.as_str()).or_default() += 1;
    }
    if required
        .iter()
        .any(|(probe, count)| observed.get(probe.as_str()).copied().unwrap_or(0) < *count)
    {
        bail!("passing raw sample is missing required correctness evidence")
    }
    Ok(())
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
    let bytes =
        fs::read(input).with_context(|| format!("failed to read summary `{}`", input.display()))?;
    let summary = serde_json::from_slice::<SummaryReport>(&bytes)
        .with_context(|| format!("failed to parse summary `{}`", input.display()))?;
    if summary.schema_version != RESULT_SCHEMA_VERSION {
        anyhow::bail!(
            "unsupported result schema {}; expected {}",
            summary.schema_version,
            RESULT_SCHEMA_VERSION
        )
    }
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

pub(crate) fn compare_files(
    baseline_path: &Path,
    candidate_path: &Path,
    threshold_pct: f64,
    min_samples: usize,
) -> Result<ComparisonReport> {
    if !threshold_pct.is_finite() || threshold_pct <= 0.0 {
        bail!("comparison threshold must be a positive finite percentage")
    }
    if min_samples == 0 {
        bail!("comparison minimum sample count must be greater than zero")
    }

    let candidate = read_summary(candidate_path, "candidate")?;
    let baseline = match read_summary(baseline_path, "baseline") {
        Ok(summary) => summary,
        Err(error) => {
            return Ok(inconclusive_comparison(
                missing_comparison_source(baseline_path),
                comparison_source(candidate_path, &candidate.summary, &candidate.sha256),
                threshold_pct,
                min_samples,
                format!("baseline summary is unavailable or invalid: {error:#}"),
            ));
        }
    };
    Ok(compare_summaries_with_sources(
        baseline_path,
        &baseline.summary,
        &baseline.sha256,
        candidate_path,
        &candidate.summary,
        &candidate.sha256,
        threshold_pct,
        min_samples,
    ))
}

pub(crate) fn write_comparison(
    report: &ComparisonReport,
    markdown_output: &Path,
    json_output: &Path,
) -> Result<()> {
    for output in [markdown_output, json_output] {
        if let Some(parent) = output.parent()
            && !parent.as_os_str().is_empty()
        {
            fs::create_dir_all(parent)?;
        }
    }
    fs::write(markdown_output, comparison_markdown(report))?;
    fs::write(json_output, serde_json::to_vec_pretty(report)?)?;
    Ok(())
}

fn read_summary(path: &Path, role: &str) -> Result<LoadedSummary> {
    let bytes = fs::read(path)
        .with_context(|| format!("failed to read {role} summary `{}`", path.display()))?;
    // Parse and hash this exact byte buffer. Do not reread the path after parsing: the
    // comparison provenance must describe the bytes that produced the decoded summary.
    let sha256 = sha256_bytes(&bytes);
    let summary = serde_json::from_slice::<SummaryReport>(&bytes)
        .with_context(|| format!("failed to parse {role} summary `{}`", path.display()))?;
    if summary.schema_version != RESULT_SCHEMA_VERSION {
        bail!(
            "unsupported {role} result schema {}; expected {}",
            summary.schema_version,
            RESULT_SCHEMA_VERSION
        )
    }
    Ok(LoadedSummary { summary, sha256 })
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn missing_comparison_source(path: &Path) -> ComparisonSource {
    ComparisonSource {
        path: path.into(),
        summary_sha256: None,
        source_url: None,
        revision: None,
        executable_sha256: None,
    }
}

fn comparison_source(
    path: &Path,
    summary: &SummaryReport,
    summary_sha256: &str,
) -> ComparisonSource {
    let mut solar_servers = summary.servers.iter().filter(|server| server.id == "solar");
    let solar = solar_servers.next().filter(|_| solar_servers.next().is_none());
    ComparisonSource {
        path: path.into(),
        summary_sha256: Some(summary_sha256.into()),
        source_url: solar
            .and_then(|server| server.source.as_ref().map(|source| source.url.clone())),
        revision: solar
            .and_then(|server| server.source.as_ref().map(|source| source.revision.clone())),
        executable_sha256: solar.and_then(|server| server.executable_sha256.clone()),
    }
}

fn require_comparison_source(blockers: &mut Vec<String>, role: &str, source: &ComparisonSource) {
    let mut missing = Vec::new();
    if !source.summary_sha256.as_deref().is_some_and(is_sha256_digest) {
        missing.push("summary digest");
    }
    if !source.source_url.as_deref().is_some_and(|url| !url.trim().is_empty()) {
        missing.push("Solar source URL");
    }
    if !source.revision.as_deref().is_some_and(is_git_revision) {
        missing.push("Solar source revision");
    }
    if !source.executable_sha256.as_deref().is_some_and(is_sha256_digest) {
        missing.push("Solar executable digest");
    }
    if !missing.is_empty() {
        blockers.push(format!(
            "{role} summary, Solar source, or executable provenance is unavailable ({})",
            missing.join(", ")
        ));
    }
}

fn is_git_revision(value: &str) -> bool {
    value.len() == 40 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn is_sha256_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn inconclusive_comparison(
    baseline: ComparisonSource,
    candidate: ComparisonSource,
    threshold_pct: f64,
    min_samples: usize,
    blocker: String,
) -> ComparisonReport {
    ComparisonReport {
        schema_version: COMPARISON_SCHEMA_VERSION,
        baseline,
        candidate,
        threshold_pct,
        min_samples,
        compatible: false,
        blockers: vec![blocker],
        comparable_metrics: 0,
        regressions: 0,
        improvements: 0,
        stable: 0,
        inconclusive: 0,
        rows: Vec::new(),
    }
}

#[allow(clippy::too_many_arguments)]
fn compare_summaries_with_sources(
    baseline_path: &Path,
    baseline: &SummaryReport,
    baseline_summary_sha256: &str,
    candidate_path: &Path,
    candidate: &SummaryReport,
    candidate_summary_sha256: &str,
    threshold_pct: f64,
    min_samples: usize,
) -> ComparisonReport {
    let baseline_source = comparison_source(baseline_path, baseline, baseline_summary_sha256);
    let candidate_source = comparison_source(candidate_path, candidate, candidate_summary_sha256);
    let mut blockers = compatibility_blockers(baseline, candidate);
    // The bounded PR profile has a single Solar executable and must carry its provenance. The
    // general cross-server profiles may intentionally compare servers without a `solar` entry.
    if baseline.profile == "pr" || candidate.profile == "pr" {
        require_comparison_source(&mut blockers, "baseline", &baseline_source);
        require_comparison_source(&mut blockers, "candidate", &candidate_source);
    }
    let compatible = blockers.is_empty();
    // Duplicate keys are invalid input. `groups_by_key` deliberately returns `None` rather
    // than silently selecting whichever duplicate happened to be visited last.
    let (Some(baseline_groups), Some(candidate_groups)) =
        (groups_by_key(baseline), groups_by_key(candidate))
    else {
        return ComparisonReport {
            schema_version: COMPARISON_SCHEMA_VERSION,
            baseline: baseline_source,
            candidate: candidate_source,
            threshold_pct,
            min_samples,
            compatible: false,
            blockers,
            comparable_metrics: 0,
            regressions: 0,
            improvements: 0,
            stable: 0,
            inconclusive: 0,
            rows: Vec::new(),
        };
    };
    let keys =
        baseline_groups.keys().chain(candidate_groups.keys()).cloned().collect::<BTreeSet<_>>();
    let repetitions = candidate
        .workloads
        .iter()
        .map(|workload| (workload.id.as_str(), workload.repetitions))
        .collect::<BTreeMap<_, _>>();
    let mut rows = Vec::new();

    for key in keys {
        let baseline_group = baseline_groups.get(&key).copied();
        let candidate_group = candidate_groups.get(&key).copied();
        let mut metrics = baseline_group
            .into_iter()
            .flat_map(|group| group.metrics.keys())
            .chain(candidate_group.into_iter().flat_map(|group| group.metrics.keys()))
            .cloned()
            .collect::<BTreeSet<_>>();
        if metrics.is_empty() {
            metrics.insert("status".into());
        }
        let expected_runs = repetitions.get(key.2.as_str()).copied();
        for metric in metrics {
            rows.push(compare_metric(
                &key,
                &metric,
                baseline_group,
                candidate_group,
                expected_runs,
                compatible,
                threshold_pct,
                min_samples,
            ));
        }
    }

    let mut comparable_metrics = 0;
    let mut regressions = 0;
    let mut improvements = 0;
    let mut stable = 0;
    let mut inconclusive = 0;
    for row in &rows {
        match row.verdict {
            ComparisonVerdict::Regression => {
                comparable_metrics += 1;
                regressions += 1;
            }
            ComparisonVerdict::Improvement => {
                comparable_metrics += 1;
                improvements += 1;
            }
            ComparisonVerdict::Stable => {
                comparable_metrics += 1;
                stable += 1;
            }
            ComparisonVerdict::Inconclusive => inconclusive += 1,
        }
    }
    ComparisonReport {
        schema_version: COMPARISON_SCHEMA_VERSION,
        baseline: baseline_source,
        candidate: candidate_source,
        threshold_pct,
        min_samples,
        compatible,
        blockers,
        comparable_metrics,
        regressions,
        improvements,
        stable,
        inconclusive,
        rows,
    }
}

#[cfg(test)]
fn compare_summaries(
    baseline_path: &Path,
    baseline: &SummaryReport,
    candidate_path: &Path,
    candidate: &SummaryReport,
    threshold_pct: f64,
    min_samples: usize,
) -> ComparisonReport {
    let baseline_bytes = serde_json::to_vec(baseline).unwrap();
    let candidate_bytes = serde_json::to_vec(candidate).unwrap();
    compare_summaries_with_sources(
        baseline_path,
        baseline,
        &sha256_bytes(&baseline_bytes),
        candidate_path,
        candidate,
        &sha256_bytes(&candidate_bytes),
        threshold_pct,
        min_samples,
    )
}

type GroupKey = (String, String, String);

fn groups_by_key(summary: &SummaryReport) -> Option<BTreeMap<GroupKey, &SummaryGroup>> {
    let mut groups = BTreeMap::new();
    for group in &summary.summaries {
        let key = (group.server.clone(), group.fixture.clone(), group.workload.clone());
        if groups.insert(key, group).is_some() {
            return None;
        }
    }
    Some(groups)
}

#[allow(clippy::too_many_arguments)]
fn compare_metric(
    key: &GroupKey,
    metric: &str,
    baseline_group: Option<&SummaryGroup>,
    candidate_group: Option<&SummaryGroup>,
    expected_runs: Option<usize>,
    compatible: bool,
    threshold_pct: f64,
    min_samples: usize,
) -> ComparisonRow {
    let baseline_stats = baseline_group.and_then(|group| group.metrics.get(metric));
    let candidate_stats = candidate_group.and_then(|group| group.metrics.get(metric));
    let baseline_status = baseline_group.map(|group| group.status);
    let candidate_status = candidate_group.map(|group| group.status);
    let mut reason = None;

    if !compatible {
        reason = Some("run metadata is incompatible".into());
    } else if baseline_group.is_none() {
        reason = Some("metric group is missing from the baseline".into());
    } else if candidate_group.is_none() {
        reason = Some("metric group is missing from the candidate".into());
    } else if baseline_status != Some(SummaryStatus::Pass) {
        reason = Some("baseline group did not pass".into());
    } else if candidate_status != Some(SummaryStatus::Pass) {
        reason = Some("candidate group did not pass".into());
    } else if expected_runs.is_none() {
        reason = Some("workload repetition contract is missing".into());
    } else if baseline_group.is_some_and(|group| Some(group.successful_runs) != expected_runs) {
        reason = Some("baseline group did not complete every configured repetition".into());
    } else if candidate_group.is_some_and(|group| Some(group.successful_runs) != expected_runs) {
        reason = Some("candidate group did not complete every configured repetition".into());
    } else if baseline_stats.is_none() {
        reason = Some("metric is missing from the baseline".into());
    } else if candidate_stats.is_none() {
        reason = Some("metric is missing from the candidate".into());
    } else if baseline_stats.is_some_and(|stats| stats.count < min_samples)
        || candidate_stats.is_some_and(|stats| stats.count < min_samples)
    {
        reason = Some(format!("metric has fewer than {min_samples} samples"));
    } else if baseline_stats.map(|stats| stats.count) != candidate_stats.map(|stats| stats.count) {
        reason = Some("baseline and candidate sample counts differ".into());
    }

    let baseline_mean = baseline_stats.map(|stats| stats.mean);
    let candidate_mean = candidate_stats.map(|stats| stats.mean);
    let baseline_p50 = baseline_stats.map(|stats| stats.p50);
    let candidate_p50 = candidate_stats.map(|stats| stats.p50);
    let baseline_p95 = baseline_stats.map(|stats| stats.p95);
    let candidate_p95 = candidate_stats.map(|stats| stats.p95);
    let mean_delta_pct = percentage_delta(baseline_mean, candidate_mean);
    let p50_delta_pct = percentage_delta(baseline_p50, candidate_p50);
    let p95_delta_pct = percentage_delta(baseline_p95, candidate_p95);
    if reason.is_none()
        && (mean_delta_pct.is_none() || p50_delta_pct.is_none() || p95_delta_pct.is_none())
    {
        reason = Some("baseline metric contains a zero or non-finite value".into());
    }
    let verdict = if reason.is_some() {
        ComparisonVerdict::Inconclusive
    } else if p50_delta_pct.is_some_and(|delta| delta >= threshold_pct)
        && p95_delta_pct.is_some_and(|delta| delta >= threshold_pct)
    {
        ComparisonVerdict::Regression
    } else if p50_delta_pct.is_some_and(|delta| delta <= -threshold_pct)
        && p95_delta_pct.is_some_and(|delta| delta <= -threshold_pct)
    {
        ComparisonVerdict::Improvement
    } else {
        ComparisonVerdict::Stable
    };

    ComparisonRow {
        server: key.0.clone(),
        fixture: key.1.clone(),
        workload: key.2.clone(),
        metric: metric.into(),
        baseline_status,
        candidate_status,
        expected_runs,
        baseline_successful_runs: baseline_group.map(|group| group.successful_runs),
        candidate_successful_runs: candidate_group.map(|group| group.successful_runs),
        baseline_count: baseline_stats.map(|stats| stats.count),
        candidate_count: candidate_stats.map(|stats| stats.count),
        baseline_mean,
        candidate_mean,
        mean_delta_pct,
        baseline_p50,
        candidate_p50,
        p50_delta_pct,
        baseline_p95,
        candidate_p95,
        p95_delta_pct,
        verdict,
        reason,
    }
}

fn percentage_delta(baseline: Option<f64>, candidate: Option<f64>) -> Option<f64> {
    let (baseline, candidate) = (baseline?, candidate?);
    if !baseline.is_finite() || !candidate.is_finite() || baseline == 0.0 {
        return None;
    }
    let delta = (candidate - baseline) / baseline.abs() * 100.0;
    delta.is_finite().then_some(delta)
}

fn compatibility_blockers(baseline: &SummaryReport, candidate: &SummaryReport) -> Vec<String> {
    let mut blockers = Vec::new();
    for (role, summary) in [("baseline", baseline), ("candidate", candidate)] {
        if has_duplicate_group_keys(summary) {
            blockers.push(format!("{role} contains duplicate summary group keys"));
        }
        summary_integrity_blockers(&mut blockers, role, summary);
        pr_solar_provenance_blockers(&mut blockers, role, summary);
    }
    compare_contract(
        &mut blockers,
        "config schema",
        baseline.config_schema_version,
        candidate.config_schema_version,
    );
    compare_contract(&mut blockers, "benchmark profile", &baseline.profile, &candidate.profile);
    compare_contract(
        &mut blockers,
        "benchmark config",
        &baseline.config_sha256,
        &candidate.config_sha256,
    );
    if baseline.profile == "pr" || candidate.profile == "pr" {
        for (role, summary) in [("baseline", baseline), ("candidate", candidate)] {
            if !summary.servers_lock_sha256.as_deref().is_some_and(is_sha256_digest) {
                blockers.push(format!("{role} server lock digest is unavailable or invalid"));
            }
        }
    } else {
        require_digest_contract(
            &mut blockers,
            "server lock",
            baseline.servers_lock_sha256.as_deref(),
            candidate.servers_lock_sha256.as_deref(),
            true,
        );
    }
    require_digest_contract(
        &mut blockers,
        "fixture lock",
        baseline.fixtures_lock_sha256.as_deref(),
        candidate.fixtures_lock_sha256.as_deref(),
        baseline.profile == "pr" || candidate.profile == "pr",
    );
    for (role, summary) in [("baseline", baseline), ("candidate", candidate)] {
        if !is_sha256_digest(&summary.config_sha256) {
            blockers.push(format!("{role} benchmark config digest is unavailable or invalid"));
        }
    }
    compare_contract(&mut blockers, "timeout", baseline.timeout_ms, candidate.timeout_ms);
    compare_contract(
        &mut blockers,
        "repeat override",
        baseline.repeat_override,
        candidate.repeat_override,
    );
    compare_contract(
        &mut blockers,
        "harness version",
        &baseline.harness_version,
        &candidate.harness_version,
    );
    require_digest_contract(
        &mut blockers,
        "harness contract",
        baseline.harness_contract_sha256.as_deref(),
        candidate.harness_contract_sha256.as_deref(),
        true,
    );
    compare_required_contract(
        &mut blockers,
        "Rust compiler",
        baseline.rustc_version.as_deref(),
        candidate.rustc_version.as_deref(),
    );
    compare_contract(
        &mut blockers,
        "operating system",
        &baseline.environment.os,
        &candidate.environment.os,
    );
    compare_contract(
        &mut blockers,
        "architecture",
        &baseline.environment.architecture,
        &candidate.environment.architecture,
    );
    compare_contract(
        &mut blockers,
        "logical CPU count",
        baseline.environment.logical_cpus,
        candidate.environment.logical_cpus,
    );
    compare_contract(
        &mut blockers,
        "process accounting backends",
        &baseline.environment.accounting_backends,
        &candidate.environment.accounting_backends,
    );
    compare_contract(
        &mut blockers,
        "memory accounting backends",
        &baseline.environment.memory_accounting_backends,
        &candidate.environment.memory_accounting_backends,
    );
    compare_contract(
        &mut blockers,
        "network isolation",
        baseline.environment.network_isolated,
        candidate.environment.network_isolated,
    );
    compare_contract(
        &mut blockers,
        "server contract",
        server_contract(baseline),
        server_contract(candidate),
    );
    compare_contract(
        &mut blockers,
        "workload contract",
        workload_contract(baseline),
        workload_contract(candidate),
    );
    compare_contract(
        &mut blockers,
        "fixture contents",
        fixture_contract(baseline),
        fixture_contract(candidate),
    );
    fixture_artifact_blockers(&mut blockers, "baseline", baseline);
    fixture_artifact_blockers(&mut blockers, "candidate", candidate);
    blockers
}

fn summary_integrity_blockers(blockers: &mut Vec<String>, role: &str, summary: &SummaryReport) {
    let server_ids =
        summary.servers.iter().map(|server| server.id.as_str()).collect::<BTreeSet<_>>();
    if server_ids.is_empty() {
        blockers.push(format!("{role} contains no selected servers"));
    } else if server_ids.len() != summary.servers.len() {
        blockers.push(format!("{role} contains duplicate server ids"));
    }

    let mut workloads = BTreeMap::new();
    for workload in &summary.workloads {
        if workload.repetitions == 0 {
            blockers
                .push(format!("{role} workload `{}` has no configured repetitions", workload.id));
        }
        if workloads
            .insert(workload.id.as_str(), (workload.fixture.as_str(), workload.repetitions))
            .is_some()
        {
            blockers.push(format!("{role} contains duplicate workload ids"));
        }
    }
    if workloads.is_empty() {
        blockers.push(format!("{role} contains no selected workloads"));
    }

    let expected_groups = server_ids
        .iter()
        .flat_map(|server| {
            workloads.iter().map(move |(workload, (fixture, _))| (*server, *fixture, *workload))
        })
        .collect::<BTreeSet<_>>();
    let actual_groups = summary
        .summaries
        .iter()
        .map(|group| (group.server.as_str(), group.fixture.as_str(), group.workload.as_str()))
        .collect::<BTreeSet<_>>();
    if actual_groups != expected_groups {
        blockers.push(format!(
            "{role} summary groups do not match the selected server and workload contract"
        ));
    }

    for group in &summary.summaries {
        let Some((expected_fixture, expected_runs)) = workloads.get(group.workload.as_str()) else {
            continue;
        };
        if group.fixture != *expected_fixture {
            blockers
                .push(format!("{role} summary group `{}` has the wrong fixture", group.workload));
        }
        let valid_counts = group
            .status_counts
            .iter()
            .all(|(status, count)| is_run_status_name(status) && *count != 0);
        let total_runs =
            group.status_counts.values().try_fold(0usize, |total, count| total.checked_add(*count));
        if !valid_counts || total_runs != Some(*expected_runs) {
            blockers.push(format!(
                "{role} summary group `{}` has invalid status counts",
                group.workload
            ));
        }
        let passing_runs = group.status_counts.get("pass").copied().unwrap_or(0);
        if group.successful_runs != passing_runs {
            blockers.push(format!(
                "{role} summary group `{}` has inconsistent successful run counts",
                group.workload
            ));
        }
        if group.status != summary_status(&group.status_counts, group.successful_runs) {
            blockers.push(format!(
                "{role} summary group `{}` has an inconsistent aggregate status",
                group.workload
            ));
        }
        if group.metrics.values().any(|stats| !valid_stats(stats)) {
            blockers.push(format!(
                "{role} summary group `{}` has invalid metric statistics",
                group.workload
            ));
        }
    }
}

fn pr_solar_provenance_blockers(blockers: &mut Vec<String>, role: &str, summary: &SummaryReport) {
    if summary.profile != "pr" {
        return;
    }
    if summary.servers.len() != 1 || summary.servers[0].id != "solar" {
        blockers.push(format!("{role} PR summary must contain only the Solar server"));
        return;
    }
    let solar = &summary.servers[0];
    if solar.status != ServerStatus::Available {
        blockers.push(format!("{role} PR summary Solar server is not available"));
    }
    let digests = [
        solar.executable_sha256.as_deref(),
        solar.artifact_expected_sha256.as_deref(),
        solar.artifact_sha256.as_deref(),
    ];
    let valid = digests.iter().all(|digest| digest.is_some_and(is_sha256_digest));
    let matching = digests
        .iter()
        .copied()
        .flatten()
        .reduce(|left, right| if left.eq_ignore_ascii_case(right) { left } else { "" })
        .is_some_and(|digest| !digest.is_empty());
    if !valid || !matching {
        blockers.push(format!(
            "{role} PR summary Solar executable and artifact digests are unavailable or inconsistent"
        ));
    }
}

fn is_run_status_name(status: &str) -> bool {
    matches!(
        status,
        "pass"
            | "unsupported"
            | "incorrect"
            | "incompatible"
            | "timeout"
            | "crash"
            | "unavailable"
            | "harness-error"
    )
}

fn valid_stats(stats: &Stats) -> bool {
    let values = [stats.mean, stats.p50, stats.p95, stats.p99, stats.max];
    stats.count != 0
        && values.into_iter().all(|value| value.is_finite() && value >= 0.0)
        && stats.p50 <= stats.p95
        && stats.p95 <= stats.p99
        && stats.p99 <= stats.max
        && stats.mean <= stats.max
}

fn has_duplicate_group_keys(summary: &SummaryReport) -> bool {
    let mut keys = BTreeSet::new();
    summary.summaries.iter().any(|group| {
        !keys.insert((group.server.as_str(), group.fixture.as_str(), group.workload.as_str()))
    })
}

fn compare_contract<T: PartialEq>(
    blockers: &mut Vec<String>,
    name: &str,
    baseline: T,
    candidate: T,
) {
    if baseline != candidate {
        blockers.push(format!("{name} differs between baseline and candidate"));
    }
}

fn compare_required_contract<T: PartialEq>(
    blockers: &mut Vec<String>,
    name: &str,
    baseline: Option<T>,
    candidate: Option<T>,
) {
    if baseline.is_none() || candidate.is_none() {
        blockers.push(format!("{name} is unavailable"));
    } else {
        compare_contract(blockers, name, baseline, candidate);
    }
}

fn require_digest_contract(
    blockers: &mut Vec<String>,
    name: &str,
    baseline: Option<&str>,
    candidate: Option<&str>,
    required: bool,
) {
    match (baseline, candidate) {
        (None, None) if !required => {}
        (Some(baseline), Some(candidate))
            if is_sha256_digest(baseline) && is_sha256_digest(candidate) =>
        {
            if baseline != candidate {
                blockers.push(format!("{name} differs between baseline and candidate"));
            }
        }
        _ => blockers.push(format!("{name} digest is unavailable or invalid")),
    }
}

#[derive(PartialEq)]
struct ServerContract<'a> {
    id: &'a str,
    args: &'a [String],
    transport: TransportSpec,
    version_args: &'a [String],
    locked_version: Option<&'a str>,
    expected_version: Option<&'a str>,
    source_url: Option<&'a str>,
    source_revision: Option<&'a str>,
    artifact_expected_sha256: Option<&'a str>,
    enabled: bool,
    env: &'a BTreeMap<String, String>,
    initialization_options: &'a Value,
    configuration: &'a Value,
    required: bool,
}

fn server_contract(summary: &SummaryReport) -> Vec<ServerContract<'_>> {
    let mut servers = summary
        .servers
        .iter()
        .map(|server| {
            let role_specific_solar = summary.profile == "pr" && server.id == "solar";
            let source = if role_specific_solar { None } else { server.source.as_ref() };
            ServerContract {
                id: &server.id,
                args: &server.args,
                transport: server.transport,
                version_args: &server.version_args,
                locked_version: server.locked_version.as_deref(),
                expected_version: server.expected_version.as_deref(),
                source_url: source.map(|source| source.url.as_str()),
                source_revision: source.map(|source| source.revision.as_str()),
                artifact_expected_sha256: if role_specific_solar {
                    None
                } else {
                    server.artifact_expected_sha256.as_deref()
                },
                enabled: server.enabled,
                env: &server.env,
                initialization_options: &server.initialization_options,
                configuration: &server.configuration,
                required: server.required,
            }
        })
        .collect::<Vec<_>>();
    servers.sort_by_key(|server| server.id);
    servers
}

fn workload_contract(summary: &SummaryReport) -> Vec<(&str, &str, &[String], usize, usize)> {
    let mut workloads = summary
        .workloads
        .iter()
        .map(|workload| {
            (
                workload.id.as_str(),
                workload.fixture.as_str(),
                workload.methods.as_slice(),
                workload.step_count,
                workload.repetitions,
            )
        })
        .collect::<Vec<_>>();
    workloads.sort_by_key(|workload| workload.0);
    workloads
}

#[derive(PartialEq)]
struct FixtureContract<'a> {
    id: &'a str,
    content_sha256: &'a str,
    source_file_count: usize,
    source_line_count: usize,
    source_byte_count: usize,
    solc: Option<CompilerContract<'a>>,
    foundry: Option<CompilerContract<'a>>,
    dependencies: &'a BTreeMap<String, String>,
}

#[derive(PartialEq)]
struct CompilerContract<'a> {
    version: &'a str,
    native_url: Option<&'a str>,
    native_sha256: Option<&'a str>,
    native_actual_sha256: Option<&'a str>,
    soljson_url: Option<&'a str>,
    soljson_sha256: Option<&'a str>,
    soljson_actual_sha256: Option<&'a str>,
    archive_url: Option<&'a str>,
    archive_sha256: Option<&'a str>,
}

fn fixture_contract(summary: &SummaryReport) -> Vec<FixtureContract<'_>> {
    let selected =
        summary.workloads.iter().map(|workload| workload.fixture.as_str()).collect::<BTreeSet<_>>();
    let mut fixtures = summary
        .fixtures
        .iter()
        .filter(|fixture| selected.contains(fixture.id.as_str()))
        .map(|fixture| FixtureContract {
            id: fixture.id.as_str(),
            content_sha256: fixture.content_sha256.as_str(),
            source_file_count: fixture.source_file_count,
            source_line_count: fixture.source_line_count,
            source_byte_count: fixture.source_byte_count,
            solc: compiler_contract(
                fixture.solc.as_ref(),
                fixture.solc_native_sha256.as_deref(),
                fixture.solc_soljson_sha256.as_deref(),
            ),
            foundry: compiler_contract(
                fixture.foundry.as_ref(),
                fixture.foundry_native_sha256.as_deref(),
                None,
            ),
            dependencies: &fixture.dependencies,
        })
        .collect::<Vec<_>>();
    fixtures.sort_by_key(|fixture| fixture.id);
    fixtures
}

fn compiler_contract<'a>(
    compiler: Option<&'a CompilerSpec>,
    native_actual_sha256: Option<&'a str>,
    soljson_actual_sha256: Option<&'a str>,
) -> Option<CompilerContract<'a>> {
    compiler.map(|compiler| CompilerContract {
        version: compiler.version.as_str(),
        native_url: compiler.native_url.as_deref(),
        native_sha256: compiler.native_sha256.as_deref(),
        native_actual_sha256,
        soljson_url: compiler.soljson_url.as_deref(),
        soljson_sha256: compiler.soljson_sha256.as_deref(),
        soljson_actual_sha256,
        archive_url: compiler.archive_url.as_deref(),
        archive_sha256: compiler.archive_sha256.as_deref(),
    })
}

fn fixture_artifact_blockers(blockers: &mut Vec<String>, role: &str, summary: &SummaryReport) {
    let selected =
        summary.workloads.iter().map(|workload| workload.fixture.as_str()).collect::<BTreeSet<_>>();
    for fixture_id in selected {
        let matches =
            summary.fixtures.iter().filter(|fixture| fixture.id == fixture_id).collect::<Vec<_>>();
        if matches.len() != 1 {
            blockers
                .push(format!("{role} fixture `{fixture_id}` metadata is missing or duplicated"));
            continue;
        }
        let fixture = matches[0];
        if !is_sha256_digest(&fixture.content_sha256) {
            blockers.push(format!(
                "{role} fixture `{fixture_id}` content digest is unavailable or invalid"
            ));
        }
        check_compiler_artifact(
            blockers,
            role,
            fixture_id,
            "solc native",
            fixture.solc.as_ref().and_then(|compiler| compiler.native.as_ref()),
            fixture.solc.as_ref().and_then(|compiler| compiler.native_sha256.as_deref()),
            fixture.solc_native_sha256.as_deref(),
        );
        check_compiler_artifact(
            blockers,
            role,
            fixture_id,
            "solc soljson",
            fixture.solc.as_ref().and_then(|compiler| compiler.soljson.as_ref()),
            fixture.solc.as_ref().and_then(|compiler| compiler.soljson_sha256.as_deref()),
            fixture.solc_soljson_sha256.as_deref(),
        );
        check_compiler_artifact(
            blockers,
            role,
            fixture_id,
            "foundry native",
            fixture.foundry.as_ref().and_then(|compiler| compiler.native.as_ref()),
            fixture.foundry.as_ref().and_then(|compiler| compiler.native_sha256.as_deref()),
            fixture.foundry_native_sha256.as_deref(),
        );
    }
}

fn check_compiler_artifact(
    blockers: &mut Vec<String>,
    role: &str,
    fixture_id: &str,
    artifact: &str,
    path: Option<&PathBuf>,
    expected_sha256: Option<&str>,
    actual_sha256: Option<&str>,
) {
    let Some(_path) = path else { return };
    let Some(actual) = actual_sha256 else {
        blockers.push(format!("{role} fixture `{fixture_id}` {artifact} digest is unavailable"));
        return;
    };
    if !is_sha256_digest(actual) {
        blockers.push(format!("{role} fixture `{fixture_id}` {artifact} digest is invalid"));
    }
    if let Some(expected) = expected_sha256
        && !expected.eq_ignore_ascii_case(actual)
    {
        blockers.push(format!(
            "{role} fixture `{fixture_id}` {artifact} digest does not match its declared pin"
        ));
    }
}

fn comparison_markdown(report: &ComparisonReport) -> String {
    let mut output = String::from("# Solar LSP PR benchmark\n\n");
    let verdict = if report.regressions != 0 {
        "REGRESSION"
    } else if !report.compatible || report.inconclusive != 0 || report.comparable_metrics == 0 {
        "INCONCLUSIVE"
    } else if report.improvements != 0 {
        "NO REGRESSION (improvements detected)"
    } else {
        "STABLE"
    };
    let _ = writeln!(output, "**{verdict}**\n");
    output.push_str(
        "This is a portable same-runner-class signal, not an authoritative cross-server comparison.\n\n",
    );
    output.push_str("| Field | Value |\n|---|---|\n");
    let metadata = [
        ("Baseline", report.baseline.path.display().to_string()),
        (
            "Baseline revision",
            report.baseline.revision.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        (
            "Baseline executable",
            report.baseline.executable_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        (
            "Baseline summary",
            report.baseline.summary_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        ("Candidate", report.candidate.path.display().to_string()),
        (
            "Candidate revision",
            report.candidate.revision.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        (
            "Candidate executable",
            report.candidate.executable_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        (
            "Candidate summary",
            report.candidate.summary_sha256.clone().unwrap_or_else(|| "unavailable".into()),
        ),
        ("Noise threshold", format!("{:.2}%", report.threshold_pct)),
        ("Minimum samples", report.min_samples.to_string()),
        ("Compatible", yes_no(report.compatible).into()),
        ("Comparable metrics", report.comparable_metrics.to_string()),
        ("Regressions", report.regressions.to_string()),
        ("Improvements", report.improvements.to_string()),
        ("Stable", report.stable.to_string()),
        ("Inconclusive", report.inconclusive.to_string()),
    ];
    for (name, value) in metadata {
        let _ = writeln!(output, "| {name} | {} |", markdown_cell(&value));
    }
    if !report.blockers.is_empty() {
        output.push_str("\n## Compatibility blockers\n\n");
        for blocker in &report.blockers {
            let _ = writeln!(output, "- {}", markdown_cell(blocker));
        }
    }
    if report.rows.is_empty() {
        return output;
    }
    output.push_str(
        "\n## Metric deltas\n\nHigher values are worse. A regression or improvement requires both p50 and p95 to cross the noise threshold in the same direction.\n\n",
    );
    output.push_str(
        "| Workload | Metric | Samples | Baseline p50 | Candidate p50 | p50 delta | p95 delta | Verdict | Reason |\n|---|---|---:|---:|---:|---:|---:|---|---|\n",
    );
    for row in &report.rows {
        let workload = format!("{}/{}/{}", row.server, row.fixture, row.workload);
        let samples = match (row.baseline_count, row.candidate_count) {
            (Some(baseline), Some(candidate)) if baseline == candidate => baseline.to_string(),
            (Some(baseline), Some(candidate)) => format!("{baseline}/{candidate}"),
            (Some(baseline), None) => format!("{baseline}/-"),
            (None, Some(candidate)) => format!("-/{candidate}"),
            (None, None) => "-".into(),
        };
        let values = [
            markdown_cell(&workload),
            markdown_cell(&row.metric),
            samples,
            format_optional_number(row.baseline_p50),
            format_optional_number(row.candidate_p50),
            format_optional_percentage(row.p50_delta_pct),
            format_optional_percentage(row.p95_delta_pct),
            comparison_verdict_name(row.verdict).into(),
            markdown_cell(row.reason.as_deref().unwrap_or("")),
        ];
        let _ = writeln!(output, "| {} |", values.join(" | "));
    }
    output
}

fn format_optional_number(value: Option<f64>) -> String {
    value.map_or_else(|| "-".into(), |value| format!("{value:.2}"))
}

fn format_optional_percentage(value: Option<f64>) -> String {
    value.map_or_else(|| "-".into(), |value| format!("{value:+.2}%"))
}

fn comparison_verdict_name(verdict: ComparisonVerdict) -> &'static str {
    match verdict {
        ComparisonVerdict::Regression => "regression",
        ComparisonVerdict::Improvement => "improvement",
        ComparisonVerdict::Stable => "stable",
        ComparisonVerdict::Inconclusive => "inconclusive",
    }
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
            "> [!WARNING]\n> This run is not an authoritative performance comparison.\n\n",
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
    output.push_str(
        "\n## Results\n\n| Server | Fixture | Workload | Successful | Statuses | Result | Metric | p50 | p95 | p99 | Max |\n|---|---|---|---:|---|---|---|---:|---:|---:|---:|\n",
    );
    for group in &summary.summaries {
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
                "| {} | {} | {} | {} | {} | {} | - | - | - | - | - |",
                group.server,
                group.fixture,
                group.workload,
                group.successful_runs,
                statuses,
                result,
            );
        }
        for (name, stats) in &group.metrics {
            let _ = writeln!(
                output,
                "| {} | {} | {} | {} | {} | {} | {} | {:.2} | {:.2} | {:.2} | {:.2} |",
                group.server,
                group.fixture,
                group.workload,
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

    fn sample(process: ProcessMetrics, setup_phases: Vec<ProcessPhase>) -> RunSample {
        RunSample {
            server: "server".into(),
            fixture: "fixture".into(),
            workload: "workload".into(),
            repetition: 0,
            status: RunStatus::Pass,
            timings_ms: BTreeMap::new(),
            process: Some(process),
            setup_phases,
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
            profile: "publish".into(),
            harness_version: "0.2.0".into(),
            harness_contract_sha256: Some("a".repeat(64)),
            rustc_version: Some("rustc 1.96.0".into()),
            harness_git_revision: Some("0".repeat(40)),
            harness_git_dirty: Some(false),
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

    fn publication_config(directory: &Path, include_peer: bool) -> (PathBuf, Config) {
        let config_path = directory.join("benchmark.yaml");
        let fixture_root = directory.join("fixture");
        fs::create_dir(&fixture_root).unwrap();
        fs::write(fixture_root.join("Main.sol"), "contract Main {}\n").unwrap();
        let fixture_root = serde_json::to_string(&fixture_root).unwrap();
        let rustc = crate::lifecycle::resolve_executable(Path::new("rustc"));
        let rustc_path = serde_json::to_string(&rustc).unwrap();
        let rustc_sha256 = crate::lifecycle::sha256_path(&rustc).unwrap();
        let rustc_version = command_output("rustc", &["--version"])
            .unwrap()
            .split_whitespace()
            .nth(1)
            .unwrap()
            .to_owned();
        let source_revision = "1".repeat(40);
        let peer = if include_peer {
            format!(
                r#"  - id: peer
    command: {rustc_path}
    locked_version: {rustc_version}
    source:
      url: https://example.invalid/peer.git
      revision: {source_revision}
    artifact:
      path: {rustc_path}
      sha256: {rustc_sha256}
"#,
            )
        } else {
            String::new()
        };
        let contents = format!(
            r#"version: 1
profiles:
  publish:
    warmup: 1
    samples: 2
    cold_samples: 1
    lifecycle_samples: 1
    timeout_ms: 1000
    scenarios: [synthetic-warm-hover]
servers:
  - id: solar
    command: {rustc_path}
    locked_version: {rustc_version}
    source:
      url: https://example.invalid/solar.git
      revision: {source_revision}
    artifact:
      path: {rustc_path}
      sha256: {rustc_sha256}
{peer}fixtures:
  - id: synthetic
    root: {fixture_root}
    solc:
      version: {rustc_version}
      native: {rustc_path}
      native_sha256: {rustc_sha256}
    foundry:
      version: {rustc_version}
      native: {rustc_path}
      native_sha256: {rustc_sha256}
    anchors:
      hover:
        path: Main.sol
        needle: Main
scenarios:
  - id: synthetic-warm-hover
    fixture: synthetic
    methods: [textDocument/hover]
    steps:
      - kind: open
        path: Main.sol
      - kind: probe
        name: ready
        probe:
          kind: hover
          path: Main.sol
          anchor: hover
          expected_text: Main
      - kind: warm
        name: hover
        probe:
          kind: hover
          path: Main.sol
          anchor: hover
          expected_text: Main
"#
        );
        fs::write(&config_path, contents).unwrap();
        let config = Config::load(&config_path).unwrap();
        (config_path, config)
    }

    fn publication_artifacts(directory: &Path) -> (PathBuf, PathBuf) {
        publication_artifacts_for_servers(directory, &["solar", "peer"])
    }

    fn publication_artifacts_for_servers(
        directory: &Path,
        selected_servers: &[&str],
    ) -> (PathBuf, PathBuf) {
        let (config_path, config) = publication_config(directory, true);
        let servers = config
            .servers
            .iter()
            .filter(|server| selected_servers.contains(&server.id.as_str()))
            .map(publication_server_metadata)
            .collect();
        let fixtures = config
            .fixtures
            .iter()
            .map(|fixture| FixtureSource::open(fixture).unwrap().metadata().clone())
            .collect();
        let samples = selected_servers
            .iter()
            .copied()
            .map(|server| {
                let mut sample = sample(
                    process_metrics(
                        ProcessAccounting::CgroupV2ProcessTree,
                        MemoryAccounting::CgroupV2Total,
                        Some(2.0),
                    ),
                    Vec::new(),
                );
                sample.server = server.into();
                sample.fixture = "synthetic".into();
                sample.workload = "synthetic-warm-hover".into();
                sample.observations.requests = (0..2)
                    .map(|_| crate::process::RequestMeasurement {
                        method: "textDocument/hover".into(),
                        elapsed_ms: 1.0,
                        process_tree_cpu_ms: Some(0.5),
                    })
                    .collect();
                sample.correctness.push(CorrectnessResult {
                    probe: "ready".into(),
                    ok: true,
                    detail: "matched".into(),
                });
                sample
            })
            .collect::<Vec<_>>();
        let repetitions = BTreeMap::from([("synthetic-warm-hover".into(), 1)]);
        let summary = summarize(SummaryInput {
            config_path: config_path.clone(),
            config: &config,
            servers,
            fixtures,
            samples: &samples,
            repeat_override: None,
            workload_repetitions: &repetitions,
            timeout_ms: config.profiles["publish"].timeout_ms,
            profile: "publish".into(),
        });
        let output = directory.join("results");
        write_reports(&output, &summary, &samples).unwrap();
        (config_path, output)
    }

    fn publication_server_metadata(spec: &crate::config::ServerSpec) -> ServerMetadata {
        let command = crate::lifecycle::resolve_executable(&spec.command);
        ServerMetadata {
            id: spec.id.clone(),
            label: spec.label.clone(),
            command: command.clone(),
            args: spec.args.clone(),
            transport: spec.transport,
            version_args: spec.version_args.clone(),
            version: Some(
                crate::lifecycle::inspect_version(
                    &command,
                    spec,
                    crate::lifecycle::VERSION_PROBE_TIMEOUT,
                )
                .unwrap(),
            ),
            locked_version: spec.locked_version.clone(),
            expected_version: spec.expected_version.clone(),
            enabled: spec.enabled,
            env: spec.env.clone(),
            initialization_options: spec.initialization_options.clone(),
            configuration: spec.configuration.clone(),
            source: spec.source.clone(),
            executable_sha256: Some(crate::lifecycle::sha256_path(&command).unwrap()),
            artifact_path: spec.artifact.as_ref().map(|artifact| artifact.path.clone()),
            artifact_expected_sha256: spec
                .artifact
                .as_ref()
                .and_then(|artifact| artifact.sha256.clone()),
            artifact_sha256: spec
                .artifact
                .as_ref()
                .map(|artifact| crate::lifecycle::sha256_path(&artifact.path).unwrap()),
            required: spec.required,
            status: ServerStatus::Available,
            error: None,
        }
    }

    fn rewrite_raw_samples(output: &Path, mutate: impl FnOnce(&mut Vec<Value>)) {
        let samples_path = output.join("samples.json");
        let mut samples =
            serde_json::from_slice::<Value>(&fs::read(&samples_path).unwrap()).unwrap();
        let values = samples["samples"].as_array_mut().unwrap();
        mutate(values);
        let jsonl = values
            .iter()
            .map(|sample| serde_json::to_string(sample).unwrap())
            .collect::<Vec<_>>()
            .join("\n")
            + "\n";
        fs::write(&samples_path, serde_json::to_vec_pretty(&samples).unwrap()).unwrap();
        fs::write(output.join("samples.jsonl"), jsonl).unwrap();
    }

    fn rewrite_summary(output: &Path, mutate: impl FnOnce(&mut Value)) {
        let summary_path = output.join("summary.json");
        let mut summary =
            serde_json::from_slice::<Value>(&fs::read(&summary_path).unwrap()).unwrap();
        mutate(&mut summary);
        fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();
    }

    fn comparison_summary(metrics: BTreeMap<String, Stats>) -> SummaryReport {
        let group = SummaryGroup {
            server: "solar".into(),
            fixture: "synthetic".into(),
            workload: "synthetic-warm-hover".into(),
            successful_runs: 3,
            status_counts: BTreeMap::from([("pass".into(), 3)]),
            status: SummaryStatus::Pass,
            metrics,
        };
        let mut summary = summary_with_groups(vec![group]);
        summary.profile = "pr".into();
        summary.timeout_ms = 30_000;
        summary.environment.network_isolated = false;
        summary.environment.authoritative = false;
        summary.environment.accounting_backends = vec![ProcessAccounting::RusageDirectChild];
        summary.environment.memory_accounting_backends =
            vec![MemoryAccounting::RusageMaxRssDirectChild];
        summary.servers.push(ServerMetadata {
            id: "solar".into(),
            label: Some("Solar candidate".into()),
            command: "/tmp/solar".into(),
            args: vec!["lsp".into()],
            transport: TransportSpec::Stdio,
            version_args: vec!["--version".into()],
            version: Some("solar 0.2.0".into()),
            locked_version: None,
            expected_version: None,
            enabled: true,
            env: BTreeMap::new(),
            initialization_options: Value::Null,
            configuration: Value::Null,
            source: Some(crate::config::SourceSpec {
                url: "https://github.com/paradigmxyz/solar.git".into(),
                revision: "1".repeat(40),
            }),
            executable_sha256: Some("1".repeat(64)),
            artifact_path: Some("/tmp/solar".into()),
            artifact_expected_sha256: Some("1".repeat(64)),
            artifact_sha256: Some("1".repeat(64)),
            required: true,
            status: ServerStatus::Available,
            error: None,
        });
        summary.fixtures.push(crate::fixture::FixtureMetadata {
            id: "synthetic".into(),
            root: "/tmp/synthetic".into(),
            revision: None,
            source_file_count: 2,
            source_line_count: 20,
            source_byte_count: 200,
            content_sha256: "2".repeat(64),
            corpus: Some("synthetic".into()),
            source: None,
            solc: None,
            solc_native_sha256: Some("4".repeat(64)),
            solc_soljson_sha256: Some("5".repeat(64)),
            solc_native_version: None,
            foundry: None,
            foundry_native_sha256: Some("6".repeat(64)),
            foundry_native_version: None,
            dependencies: BTreeMap::new(),
        });
        summary.workloads.push(WorkloadMetadata {
            id: "synthetic-warm-hover".into(),
            fixture: "synthetic".into(),
            methods: vec!["textDocument/hover".into()],
            step_count: 3,
            repetitions: 3,
        });
        summary
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
        let direct_child = process_metrics(
            ProcessAccounting::RusageDirectChild,
            MemoryAccounting::RusageMaxRssDirectChild,
            Some(2.0),
        );
        let sample_with_missing_memory = sample(missing_memory, Vec::new());
        assert!(!samples_have_authoritative_metrics(&[&sample_with_missing_memory]));
        let environment = Environment::current(&[sample_with_missing_memory]);
        assert!(!environment.authoritative);
        assert_eq!(environment.accounting_backends, vec![ProcessAccounting::CgroupV2ProcessTree]);
        assert_eq!(environment.memory_accounting_backends, vec![MemoryAccounting::Unavailable]);

        let sample_with_fallback_setup = sample(
            complete.clone(),
            vec![ProcessPhase {
                name: "setup".into(),
                process: direct_child,
                observations: Observations::default(),
            }],
        );
        assert!(!samples_have_authoritative_metrics(&[&sample_with_fallback_setup]));
        let environment = Environment::current(&[sample_with_fallback_setup]);
        assert!(!environment.authoritative);
        assert_eq!(
            environment.accounting_backends,
            vec![ProcessAccounting::CgroupV2ProcessTree, ProcessAccounting::RusageDirectChild]
        );
        assert_eq!(
            environment.memory_accounting_backends,
            vec![MemoryAccounting::CgroupV2Total, MemoryAccounting::RusageMaxRssDirectChild]
        );

        let mut missing_request_cpu = sample(complete.clone(), Vec::new());
        missing_request_cpu.observations.requests.push(crate::process::RequestMeasurement {
            method: "textDocument/hover".into(),
            elapsed_ms: 1.0,
            process_tree_cpu_ms: None,
        });
        assert!(!samples_have_authoritative_metrics(&[&missing_request_cpu]));

        let mut setup_observations = Observations::default();
        setup_observations.requests.push(crate::process::RequestMeasurement {
            method: "textDocument/hover".into(),
            elapsed_ms: 1.0,
            process_tree_cpu_ms: None,
        });
        let missing_setup_request_cpu = sample(
            complete.clone(),
            vec![ProcessPhase {
                name: "setup".into(),
                process: complete.clone(),
                observations: setup_observations,
            }],
        );
        assert!(!samples_have_authoritative_metrics(&[&missing_setup_request_cpu]));

        let complete_sample = sample(complete, Vec::new());
        assert!(samples_have_authoritative_metrics(&[&complete_sample]));

        let mut forced = process_metrics(
            ProcessAccounting::CgroupV2ProcessTree,
            MemoryAccounting::CgroupV2Total,
            Some(4.0),
        );
        forced.forced_kill = true;
        assert!(!forced.has_authoritative_process_tree_metrics());
        assert!(!samples_have_authoritative_metrics(&[&sample(forced, Vec::new())]));
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
        let summary = summary_with_groups(vec![group.clone()]);

        let output = markdown(&summary);
        assert!(output.contains("## Run metadata"), "{output}");
        assert!(output.contains("| Authoritative | yes |"), "{output}");
        assert!(output.contains(&format!("| Harness revision | {} |", "0".repeat(40))), "{output}");
        assert!(output.contains(":red_circle: **FAILED**"), "{output}");
        assert!(
            output.contains("| external | synthetic | correctness | 0 | incorrect:1 |"),
            "{output}"
        );

        let value = serde_json::to_value(group).unwrap();
        assert_eq!(value["status"], "failed");
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
        assert!(output.contains("## Servers"), "{output}");
        assert!(
            output.contains(&format!(
                "| server | Server 1 | available | server 1.0 | 1.0 | {} | {} |",
                "2".repeat(64),
                "1".repeat(40)
            )),
            "{output}"
        );
        assert!(output.contains("## Fixtures"), "{output}");
        assert!(
            output.contains(&format!(
                "| fixture | Fixture corpus | {} | {} | 2 | 20 | 200 |",
                "3".repeat(40),
                "4".repeat(64)
            )),
            "{output}"
        );
    }

    #[test]
    fn warm_summaries_exclude_whole_session_resource_totals() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("benchmark.yaml");
        fs::write(
            &config_path,
            "version: 1\nservers:\n  - id: server\n    command: server\nfixtures:\n  - id: fixture\n    root: .\nscenarios:\n  - id: cold\n    fixture: fixture\n    steps:\n      - kind: probe\n        name: cold-ready\n        probe:\n          kind: document-symbol\n          path: Main.sol\n  - id: warm\n    fixture: fixture\n    steps:\n      - kind: warm\n        name: hover\n        probe:\n          kind: document-symbol\n          path: Main.sol\n",
        )
        .unwrap();
        let config = Config::load(&config_path).unwrap();
        let metrics = process_metrics(
            ProcessAccounting::CgroupV2ProcessTree,
            MemoryAccounting::CgroupV2Total,
            Some(4.0),
        );
        let mut cold = sample(metrics.clone(), Vec::new());
        cold.workload = "cold".into();
        cold.timings_ms.insert("cold_ready_ms".into(), 1.0);
        let mut warm = sample(metrics, Vec::new());
        warm.workload = "warm".into();
        warm.timings_ms.insert("spawn_to_initialize_response_ms".into(), 1.0);
        warm.timings_ms.insert("ready_ms".into(), 1.5);
        warm.timings_ms.insert("warm_hover_0_ms".into(), 2.0);
        warm.timings_ms.insert("warm_hover_1_ms".into(), 2.1);
        warm.observations.requests.push(crate::process::RequestMeasurement {
            method: "textDocument/documentSymbol".into(),
            elapsed_ms: 1.8,
            process_tree_cpu_ms: Some(0.4),
        });
        warm.observations.requests.push(crate::process::RequestMeasurement {
            method: "textDocument/documentSymbol".into(),
            elapsed_ms: 1.9,
            process_tree_cpu_ms: Some(0.5),
        });
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
        assert!(cold.metrics.contains_key("session_peak_cgroup_memory_mib"));
        assert!(cold.metrics.contains_key("session_wall_ms"));
        assert!(!cold.metrics.contains_key("process_cpu_ms"));
        let warm = summary.summaries.iter().find(|group| group.workload == "warm").unwrap();
        assert_eq!(
            warm.metrics.keys().map(String::as_str).collect::<Vec<_>>(),
            ["textDocument/documentSymbol", "textDocument/documentSymbol_cpu_ms"]
        );
        assert_eq!(warm.metrics["textDocument/documentSymbol"].count, 2);
        assert_eq!(warm.metrics["textDocument/documentSymbol_cpu_ms"].count, 2);
        assert!(!warm.metrics.contains_key("warm_hover_ms"));
    }

    #[test]
    fn summary_uses_digests_of_the_loaded_manifest_bytes() {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("benchmark.yaml");
        let servers_lock_path = directory.path().join("servers.lock.yaml");
        let fixtures_lock_path = directory.path().join("fixtures.lock.yaml");
        let config_bytes = b"version: 1\nservers_lock: servers.lock.yaml\nfixtures_lock: fixtures.lock.yaml\nservers: []\nfixtures: []\nworkloads: []\n";
        let servers_lock_bytes = b"version: 1\nservers: []\n";
        let fixtures_lock_bytes = b"version: 1\nfixtures: []\n";
        fs::write(&config_path, config_bytes).unwrap();
        fs::write(&servers_lock_path, servers_lock_bytes).unwrap();
        fs::write(&fixtures_lock_path, fixtures_lock_bytes).unwrap();

        let config = Config::load(&config_path).unwrap();
        fs::write(&config_path, b"changed config\n").unwrap();
        fs::write(&servers_lock_path, b"changed server lock\n").unwrap();
        fs::write(&fixtures_lock_path, b"changed fixture lock\n").unwrap();

        let summary = summarize(SummaryInput {
            config_path,
            config: &config,
            servers: Vec::new(),
            fixtures: Vec::new(),
            samples: &[],
            repeat_override: None,
            workload_repetitions: &BTreeMap::new(),
            timeout_ms: 1_000,
            profile: "test".into(),
        });
        assert_eq!(summary.config_sha256, sha256_bytes(config_bytes));
        assert_eq!(
            summary.servers_lock_sha256.as_deref(),
            Some(sha256_bytes(servers_lock_bytes).as_str())
        );
        assert_eq!(
            summary.fixtures_lock_sha256.as_deref(),
            Some(sha256_bytes(fixtures_lock_bytes).as_str())
        );
    }

    #[test]
    fn comparison_uses_a_two_quantile_noise_gate() {
        let baseline = comparison_summary(BTreeMap::from([
            ("regression".into(), metric_stats(3, 100.0, 100.0, 120.0)),
            ("improvement".into(), metric_stats(3, 100.0, 100.0, 120.0)),
            ("mixed".into(), metric_stats(3, 100.0, 100.0, 120.0)),
        ]));
        let candidate = comparison_summary(BTreeMap::from([
            ("regression".into(), metric_stats(3, 115.0, 115.0, 138.0)),
            ("improvement".into(), metric_stats(3, 85.0, 85.0, 100.0)),
            ("mixed".into(), metric_stats(3, 108.0, 115.0, 120.0)),
        ]));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert!(comparison.compatible, "{:?}", comparison.blockers);
        assert_eq!(comparison.regressions, 1);
        assert_eq!(comparison.improvements, 1);
        assert_eq!(comparison.stable, 1);
        assert_eq!(comparison.inconclusive, 0);
        assert_eq!(comparison.comparable_metrics, 3);
        assert_eq!(comparison.rows[0].verdict, ComparisonVerdict::Improvement);
        assert_eq!(comparison.rows[1].verdict, ComparisonVerdict::Stable);
        assert_eq!(comparison.rows[2].verdict, ComparisonVerdict::Regression);
    }

    #[test]
    fn comparison_preserves_role_specific_solar_provenance() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.servers[0].source.as_mut().unwrap().revision = "2".repeat(40);
        candidate.servers[0].executable_sha256 = Some("3".repeat(64));
        candidate.servers[0].artifact_expected_sha256 = Some("3".repeat(64));
        candidate.servers[0].artifact_sha256 = Some("3".repeat(64));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(comparison.compatible, "{:?}", comparison.blockers);
        assert_eq!(comparison.baseline.revision.as_deref(), Some("1".repeat(40).as_str()));
        assert_eq!(comparison.candidate.revision.as_deref(), Some("2".repeat(40).as_str()));
        assert_eq!(
            comparison.candidate.executable_sha256.as_deref(),
            Some("3".repeat(64).as_str())
        );
    }

    #[test]
    fn comparison_marks_incompatible_metadata_inconclusive() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.profile = "smoke".into();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert!(!comparison.compatible);
        assert_eq!(comparison.regressions, 0);
        assert_eq!(comparison.inconclusive, 1);
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("profile")));
        assert_eq!(comparison.rows[0].verdict, ComparisonVerdict::Inconclusive);
    }

    #[test]
    fn comparison_requires_the_same_harness_contract() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.harness_contract_sha256 = Some("changed-harness".into());

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert!(!comparison.compatible);
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("harness contract")));
    }

    #[test]
    fn comparison_requires_fixture_lock_and_harness_digests() {
        let mut baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        baseline.fixtures_lock_sha256 = None;
        baseline.harness_contract_sha256 = None;
        let candidate = baseline.clone();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("fixture lock")));
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("harness contract")));
    }

    #[test]
    fn comparison_requires_the_same_server_runtime_contract() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.servers[0].initialization_options =
            serde_json::json!({"projectIndex": {"fullProjectScan": true}});

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert!(!comparison.compatible);
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("server contract")));
    }

    #[test]
    fn comparison_allows_role_specific_server_source_urls() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.servers[0].source.as_mut().unwrap().url =
            "https://example.invalid/solar.git".into();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(comparison.compatible);
        assert_eq!(
            comparison.candidate.source_url.as_deref(),
            Some("https://example.invalid/solar.git")
        );
    }

    #[test]
    fn non_pr_comparison_requires_the_same_server_source() {
        let mut baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        baseline.profile = "default".into();
        let mut candidate = baseline.clone();
        candidate.servers[0].source.as_mut().unwrap().url =
            "https://example.invalid/solar.git".into();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("server contract")));
    }

    #[test]
    fn comparison_rejects_inconsistent_status_counts() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.summaries[0].status_counts = BTreeMap::from([("crash".into(), 3)]);

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert_eq!(comparison.comparable_metrics, 0);
        assert!(
            comparison
                .blockers
                .iter()
                .any(|blocker| blocker.contains("inconsistent aggregate status"))
        );
    }

    #[test]
    fn comparison_rejects_a_workload_missing_from_both_summaries() {
        let mut baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        baseline.summaries.clear();
        let candidate = baseline.clone();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert_eq!(comparison.comparable_metrics, 0);
        assert_eq!(comparison.stable, 0);
        assert!(comparison.blockers.iter().any(|blocker| blocker.contains("summary groups")));
    }

    #[test]
    fn comparison_requires_bound_pr_executable_provenance() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.servers[0].artifact_sha256 = Some("9".repeat(64));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(
            comparison
                .blockers
                .iter()
                .any(|blocker| blocker.contains("executable and artifact digests"))
        );
    }

    #[test]
    fn comparison_requires_a_solar_only_pr_summary() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        let mut extra = candidate.servers[0].clone();
        extra.id = "other".into();
        candidate.servers.push(extra);

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(
            comparison
                .blockers
                .iter()
                .any(|blocker| blocker.contains("must contain only the Solar server"))
        );
    }

    #[test]
    fn comparison_rejects_duplicate_summary_groups_without_overwriting_one() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.summaries.push(candidate.summaries[0].clone());
        candidate.summaries[1].metrics.insert("different".into(), metric_stats(3, 1.0, 1.0, 1.0));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(
            comparison
                .blockers
                .iter()
                .any(|blocker| blocker.contains("candidate contains duplicate summary group keys"))
        );
        assert!(comparison.rows.is_empty());
    }

    #[test]
    fn comparison_requires_fixture_compiler_artifact_digests() {
        let mut baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        let compiler = crate::config::CompilerSpec {
            version: "0.8.36".into(),
            native: Some("/tmp/solc".into()),
            soljson: None,
            native_url: None,
            native_sha256: None,
            soljson_url: None,
            soljson_sha256: None,
            archive_url: None,
            archive_sha256: None,
        };
        baseline.fixtures[0].solc = Some(compiler.clone());
        candidate.fixtures[0].solc = Some(compiler);
        baseline.fixtures[0].solc_native_sha256 = None;
        candidate.fixtures[0].solc_native_sha256 = None;

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(comparison.blockers.iter().any(|blocker| {
            blocker.contains("baseline fixture `synthetic` solc native digest is unavailable")
        }));
        assert!(comparison.blockers.iter().any(|blocker| {
            blocker.contains("candidate fixture `synthetic` solc native digest is unavailable")
        }));
    }

    #[test]
    fn comparison_rejects_unbound_summary_input_digests() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.config_sha256 = "not-a-digest".into();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(
            comparison
                .blockers
                .iter()
                .any(|blocker| blocker.contains("candidate benchmark config digest"))
        );
    }

    #[test]
    fn comparison_requires_unique_solar_provenance() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.servers[0].executable_sha256 = None;

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert!(!comparison.compatible);
        assert!(comparison.blockers.iter().any(|blocker| {
            blocker.contains("candidate summary, Solar source, or executable provenance")
        }));
    }

    #[test]
    fn comparison_uses_fixture_contents_instead_of_enclosing_revision() {
        let mut baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        baseline.fixtures[0].revision = Some("1".repeat(40));
        let mut candidate = baseline.clone();
        candidate.fixtures[0].revision = Some("2".repeat(40));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert!(comparison.compatible, "{:?}", comparison.blockers);
    }

    #[test]
    fn comparison_marks_an_incomplete_group_inconclusive() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        let mut candidate = baseline.clone();
        candidate.summaries[0].successful_runs = 2;
        candidate.summaries[0].status_counts =
            BTreeMap::from([("pass".into(), 2), ("unsupported".into(), 1)]);
        candidate.summaries[0].status = SummaryStatus::Partial;

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert_eq!(comparison.inconclusive, 1);
        assert_eq!(comparison.rows[0].reason.as_deref(), Some("candidate group did not pass"));
    }

    #[test]
    fn comparison_requires_enough_metric_samples() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(1, 100.0, 100.0, 120.0),
        )]));
        let candidate = baseline.clone();

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );

        assert_eq!(comparison.inconclusive, 1);
        assert_eq!(comparison.rows[0].reason.as_deref(), Some("metric has fewer than 2 samples"));
    }

    #[test]
    fn result_validation_rejects_a_server_missing_from_the_manifest_matrix() {
        let directory = tempfile::tempdir().unwrap();
        let (_, config) = publication_config(directory.path(), true);
        let mut summary = comparison_summary(BTreeMap::from([(
            "textDocument/hover".into(),
            metric_stats(2, 1.0, 1.0, 1.0),
        )]));
        summary.profile = "publish".into();
        summary.config_sha256 = config.config_sha256.clone();
        summary.servers_lock_sha256 = config.servers_lock_sha256.clone();
        summary.fixtures_lock_sha256 = config.fixtures_lock_sha256.clone();
        summary.timeout_ms = config.profiles["publish"].timeout_ms;
        summary.harness_version = env!("CARGO_PKG_VERSION").into();
        summary.harness_contract_sha256 = harness_contract_sha256();
        summary.rustc_version = command_output("rustc", &["--version", "--verbose"])
            .map(|output| normalize_multiline_output(&output));
        (summary.harness_git_revision, summary.harness_git_dirty) = harness_git_provenance();

        let error = validate_summary_manifest_contract(&config, "publish", &summary)
            .unwrap_err()
            .to_string();

        assert!(error.contains("server selection"), "{error}");
    }

    #[test]
    fn result_validation_rejects_a_missing_raw_sample() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples.pop();
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("raw sample matrix"), "{error}");
    }

    #[test]
    fn result_validation_accepts_matching_manifest_raw_and_summary_outputs() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());

        validate_results_directory(&config_path, &output, "publish", false).unwrap();
    }

    #[test]
    fn result_validation_accepts_serialization_rounding() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            let stats = &mut summary["summaries"][0]["metrics"]["textDocument/hover"];
            for field in ["mean", "p50", "p95", "p99", "max"] {
                let value = stats[field].as_f64().unwrap();
                stats[field] = Value::from(f64::from_bits(value.to_bits() - 1));
            }
        });

        validate_results_directory(&config_path, &output, "publish", false).unwrap();
    }

    #[test]
    fn result_validation_accepts_an_explicit_server_subset() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts_for_servers(directory.path(), &["solar"]);
        let servers = BTreeSet::from(["solar".to_owned()]);

        validate_results_directory_for_servers(&config_path, &output, "publish", false, &servers)
            .unwrap();
    }

    #[test]
    fn result_validation_rejects_an_unknown_server_selection() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let servers = BTreeSet::from(["missing".to_owned()]);

        let error = validate_results_directory_for_servers(
            &config_path,
            &output,
            "publish",
            false,
            &servers,
        )
        .unwrap_err()
        .to_string();

        assert!(error.contains("missing or disabled"), "{error}");
    }

    #[test]
    fn result_validation_binds_profile_and_manifest_digests() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let summary_path = output.join("summary.json");
        let original = fs::read(&summary_path).unwrap();
        for (field, value, expected) in [
            ("profile", Value::String("other".into()), "summary profile"),
            ("config_sha256", Value::String("9".repeat(64)), "config digest"),
            ("servers_lock_sha256", Value::String("9".repeat(64)), "server lock digest"),
            ("fixtures_lock_sha256", Value::String("9".repeat(64)), "fixture lock digest"),
        ] {
            let mut summary = serde_json::from_slice::<Value>(&original).unwrap();
            summary[field] = value;
            fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();
            let error = validate_results_directory(&config_path, &output, "publish", false)
                .unwrap_err()
                .to_string();
            assert!(error.contains(expected), "{field}: {error}");
        }
    }

    #[test]
    fn result_validation_binds_publication_repetitions() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            summary["repeat_override"] = Value::from(1);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("repetition override"), "{error}");
    }

    #[test]
    fn result_validation_rejects_changed_workload_repetitions() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            summary["workloads"][0]["repetitions"] = Value::from(2);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("workload selection"), "{error}");
    }

    #[test]
    fn result_validation_rejects_a_workload_missing_from_the_manifest_matrix() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let summary_path = output.join("summary.json");
        let mut summary =
            serde_json::from_slice::<Value>(&fs::read(&summary_path).unwrap()).unwrap();
        summary["workloads"].as_array_mut().unwrap().clear();
        summary["summaries"].as_array_mut().unwrap().clear();
        fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("workload selection"), "{error}");
    }

    #[test]
    fn result_validation_rejects_jsonl_content_divergence() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let jsonl_path = output.join("samples.jsonl");
        let mut rows =
            fs::read_to_string(&jsonl_path).unwrap().lines().map(str::to_owned).collect::<Vec<_>>();
        let mut first = serde_json::from_str::<Value>(&rows[0]).unwrap();
        first["repetition"] = Value::from(99);
        rows[0] = serde_json::to_string(&first).unwrap();
        fs::write(&jsonl_path, rows.join("\n") + "\n").unwrap();

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("exactly match"), "{error}");
    }

    #[test]
    fn result_validation_rejects_duplicate_raw_keys() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples.push(samples[0].clone());
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("duplicate key"), "{error}");
    }

    #[test]
    fn result_validation_rejects_duplicate_summary_groups() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            let duplicate = summary["summaries"][0].clone();
            summary["summaries"].as_array_mut().unwrap().push(duplicate);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("group count"), "{error}");
    }

    #[test]
    fn result_validation_rejects_raw_fixture_workload_mismatch() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples[0]["fixture"] = Value::String("other".into());
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("fixture does not match workload"), "{error}");
    }

    #[test]
    fn result_validation_rejects_warm_request_method_mismatch() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples[0]["observations"]["requests"][0]["method"] =
                Value::String("textDocument/definition".into());
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("request methods"), "{error}");
    }

    #[test]
    fn result_validation_rejects_authoritative_warm_sample_without_request_cpu() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            summary["environment"]["authoritative"] = Value::Bool(true);
        });
        rewrite_raw_samples(&output, |samples| {
            for request in samples[0]["observations"]["requests"].as_array_mut().unwrap() {
                request["process_tree_cpu_ms"] = Value::Null;
            }
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("request CPU evidence"), "{error}");
    }

    #[test]
    fn result_validation_rejects_missing_passing_metrics() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let summary_path = output.join("summary.json");
        let mut summary =
            serde_json::from_slice::<Value>(&fs::read(&summary_path).unwrap()).unwrap();
        for group in summary["summaries"].as_array_mut().unwrap() {
            group["metrics"] = serde_json::json!({});
        }
        fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("metric selection"), "{error}");
    }

    #[test]
    fn result_validation_recomputes_valid_looking_statistics() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            for group in summary["summaries"].as_array_mut().unwrap() {
                for stats in group["metrics"].as_object_mut().unwrap().values_mut() {
                    for field in ["mean", "p50", "p95", "p99", "max"] {
                        stats[field] = Value::from(2.0);
                    }
                }
            }
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("does not match the raw samples"), "{error}");
    }

    #[test]
    fn result_validation_rejects_negative_process_metrics() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples[0]["process"]["wall_ms"] = Value::from(-1.0);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("negative metric"), "{error}");
    }

    #[test]
    fn result_validation_rejects_setup_phase_without_restart() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            let process = samples[0]["process"].clone();
            samples[0]["setup_phases"] = serde_json::json!([{
                "name": "cache-population",
                "process": process,
                "observations": {
                    "diagnostic_publications": 0,
                    "requests": [],
                    "events": [],
                    "server_requests": [],
                    "trace_truncated": false
                }
            }]);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("setup phases"), "{error}");
    }

    #[test]
    fn result_validation_requires_setup_phase_for_restart() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let mut config = Config::load(&config_path).unwrap();
        config.workloads[0].steps.insert(2, crate::config::StepSpec::Restart { invalidate: None });
        let summary = read_summary(&output.join("summary.json"), "publication").unwrap().summary;
        let mut samples = serde_json::from_slice::<OwnedSamplesReport>(
            &fs::read(output.join("samples.json")).unwrap(),
        )
        .unwrap()
        .samples;
        for sample in &mut samples {
            sample.correctness[0].probe = "cache-setup/ready".into();
        }

        let error =
            validate_raw_sample_contract(&config_path, &config, "publish", &summary, &samples)
                .unwrap_err()
                .to_string();

        assert!(error.contains("setup phases"), "{error}");
    }

    #[test]
    fn result_validation_rejects_forged_fixture_content_evidence() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let summary_path = output.join("summary.json");
        let mut summary =
            serde_json::from_slice::<Value>(&fs::read(&summary_path).unwrap()).unwrap();
        summary["fixtures"][0]["content_sha256"] = Value::String("9".repeat(64));
        fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("fixture evidence"), "{error}");
    }

    #[test]
    fn result_validation_rejects_coordinated_compiler_evidence_tampering() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            summary["fixtures"][0]["solc"]["version"] = Value::String("forged".into());
            summary["fixtures"][0]["solc"]["native_sha256"] = Value::String("9".repeat(64));
            summary["fixtures"][0]["solc_native_sha256"] = Value::String("9".repeat(64));
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("fixture evidence"), "{error}");
    }

    #[test]
    fn result_validation_rejects_coordinated_server_provenance_tampering() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let summary_path = output.join("summary.json");
        let mut summary =
            serde_json::from_slice::<Value>(&fs::read(&summary_path).unwrap()).unwrap();
        summary["servers"][0]["source"]["revision"] = Value::String("2".repeat(40));
        summary["servers"][0]["artifact_expected_sha256"] = Value::String("9".repeat(64));
        summary["servers"][0]["artifact_sha256"] = Value::String("9".repeat(64));
        fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("server evidence"), "{error}");
    }

    #[test]
    fn result_validation_rejects_passing_samples_for_an_unavailable_server() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_summary(&output, |summary| {
            summary["servers"][0]["status"] = Value::String("unavailable".into());
            summary["servers"][0]["version"] = Value::Null;
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("passing raw sample has an unavailable server"), "{error}");
    }

    #[test]
    fn result_validation_rejects_forged_harness_revision() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        let summary_path = output.join("summary.json");
        let mut summary =
            serde_json::from_slice::<Value>(&fs::read(&summary_path).unwrap()).unwrap();
        let current = summary["harness_git_revision"].as_str().unwrap();
        summary["harness_git_revision"] =
            Value::String(if current == "1".repeat(40) { "2".repeat(40) } else { "1".repeat(40) });
        fs::write(&summary_path, serde_json::to_vec_pretty(&summary).unwrap()).unwrap();

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("harness revision"), "{error}");
    }

    #[test]
    fn result_validation_rejects_failed_correctness_in_a_passing_sample() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples[0]["correctness"] = serde_json::json!([{
                "probe": "tampered",
                "ok": false,
                "detail": "did not match"
            }]);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("failed correctness"), "{error}");
    }

    #[test]
    fn result_validation_rejects_missing_manifest_correctness() {
        let directory = tempfile::tempdir().unwrap();
        let (config_path, output) = publication_artifacts(directory.path());
        rewrite_raw_samples(&output, |samples| {
            samples[0]["correctness"] = serde_json::json!([]);
        });

        let error = validate_results_directory(&config_path, &output, "publish", false)
            .unwrap_err()
            .to_string();

        assert!(error.contains("required correctness"), "{error}");
    }

    #[test]
    fn missing_baseline_writes_an_inconclusive_comparison() {
        let directory = tempfile::tempdir().unwrap();
        let candidate_path = directory.path().join("candidate.json");
        let baseline_path = directory.path().join("missing-baseline.json");
        let candidate = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, 100.0, 100.0, 120.0),
        )]));
        fs::write(&candidate_path, serde_json::to_vec(&candidate).unwrap()).unwrap();

        let comparison = compare_files(&baseline_path, &candidate_path, 10.0, 2).unwrap();
        assert!(!comparison.compatible);
        assert_eq!(comparison.regressions, 0);
        assert!(comparison.rows.is_empty());
        assert!(comparison.blockers[0].contains("baseline summary is unavailable"));
        let markdown = comparison_markdown(&comparison);
        assert!(markdown.contains("**INCONCLUSIVE**"), "{markdown}");
    }

    #[test]
    fn zero_baseline_metric_is_inconclusive() {
        let baseline =
            comparison_summary(BTreeMap::from([("hover".into(), metric_stats(3, 0.0, 0.0, 0.0))]));
        let candidate =
            comparison_summary(BTreeMap::from([("hover".into(), metric_stats(3, 1.0, 1.0, 1.0))]));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert_eq!(comparison.inconclusive, 1);
        assert_eq!(
            comparison.rows[0].reason.as_deref(),
            Some("baseline metric contains a zero or non-finite value")
        );
    }

    #[test]
    fn overflowing_metric_delta_is_inconclusive_and_serializable() {
        let baseline = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, f64::MIN_POSITIVE, f64::MIN_POSITIVE, f64::MIN_POSITIVE),
        )]));
        let candidate = comparison_summary(BTreeMap::from([(
            "hover".into(),
            metric_stats(3, f64::MAX, f64::MAX, f64::MAX),
        )]));

        let comparison = compare_summaries(
            Path::new("baseline.json"),
            &baseline,
            Path::new("candidate.json"),
            &candidate,
            10.0,
            2,
        );
        assert_eq!(comparison.inconclusive, 1);
        assert_eq!(comparison.rows[0].p50_delta_pct, None);
        assert!(serde_json::to_vec(&comparison).is_ok());
    }
}

//! Manifest-driven benchmark execution.

use crate::{
    config::{Config, ProbeSpec, ProfileSpec, ServerSpec, StepSpec, WorkloadSpec},
    fixture::{
        Anchor, Fixture, FixtureMetadata, FixtureSource, PositionEncoding, file_uri,
        offset_at_position, position_at_with_encoding,
    },
    lifecycle::{
        VERSION_PROBE_TIMEOUT, absolute_manifest_dir, inspect_version, resolve_executable,
        verify_server_runtime_inputs, verify_server_version_output,
    },
    process::{
        FinishedProcess, LspProcess, Observations, ProcessEnvironment, RemoteError,
        WorkspaceEditNotifications, network_namespace_active,
    },
    report::{
        CorrectnessResult, ProcessPhase, RunSample, RunStatus, ServerMetadata, ServerStatus,
        SummaryInput, SummaryReport, summarize, write_reports,
    },
};
use anyhow::{Context, Result, anyhow, bail};
use lsp_types::{
    CompletionResponse, DocumentSymbol, DocumentSymbolResponse, GotoDefinitionResponse, Location,
    Range, Url,
};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
    thread,
    time::{Duration, Instant},
};

pub(crate) struct RunOptions {
    pub(crate) config: PathBuf,
    pub(crate) repeat: usize,
    pub(crate) timeout: Duration,
    pub(crate) profile: String,
    pub(crate) output: PathBuf,
    pub(crate) servers: BTreeSet<String>,
    pub(crate) workloads: BTreeSet<String>,
}

pub(crate) struct RunOutcome {
    pub(crate) summary: SummaryReport,
    pub(crate) failed_runs: usize,
    pub(crate) authority_failure: bool,
}

struct PreparedServer {
    spec: ServerSpec,
    metadata: ServerMetadata,
}

struct PreparedFixture {
    source: FixtureSource,
}

#[derive(Clone, Copy, Debug)]
enum FailureKind {
    Unsupported,
    Incorrect,
    Timeout,
    Crashed,
    HarnessError,
}

#[derive(Debug, thiserror::Error)]
#[error("{message}")]
struct WorkloadError {
    kind: FailureKind,
    message: String,
}

impl WorkloadError {
    fn new(kind: FailureKind, message: impl Into<String>) -> Self {
        Self { kind, message: message.into() }
    }
}

pub(crate) fn run(options: RunOptions) -> Result<RunOutcome> {
    let config = Config::load(&options.config)?;
    let manifest_dir = absolute_manifest_dir(&options.config)?;
    let profile = config
        .profiles
        .get(&options.profile)
        .with_context(|| format!("benchmark profile `{}` is not defined", options.profile))?;
    if profile.network_isolation && !network_namespace_active() {
        bail!("network-isolated profile must run inside the benchmark network namespace")
    }
    let repeat_override = (options.repeat != 0).then_some(options.repeat);
    let timeout = if options.timeout.is_zero() {
        Duration::from_millis(profile.timeout_ms)
    } else {
        options.timeout
    };
    if profile.require_authoritative
        && (!options.servers.is_empty()
            || !options.workloads.is_empty()
            || repeat_override.is_some()
            || !options.timeout.is_zero())
    {
        bail!("authoritative profiles cannot use server, workload, repeat, or timeout overrides");
    }
    if !options.servers.is_empty() {
        let known_servers =
            config.servers.iter().map(|server| server.id.as_str()).collect::<BTreeSet<_>>();
        let missing = options
            .servers
            .iter()
            .filter(|server| !known_servers.contains(server.as_str()))
            .cloned()
            .collect::<Vec<_>>();
        if !missing.is_empty() {
            bail!(
                "selected servers are not declared in the benchmark manifest: {}",
                missing.join(", ")
            )
        }
    }

    let disabled = options
        .servers
        .iter()
        .filter(|server| {
            config.servers.iter().find(|spec| spec.id == **server).is_some_and(|spec| !spec.enabled)
        })
        .cloned()
        .collect::<Vec<_>>();
    if !disabled.is_empty() {
        bail!(
            "selected servers are missing or disabled in the benchmark manifest: {}",
            disabled.join(", ")
        )
    }

    let servers = config
        .servers
        .iter()
        .filter(|server| {
            server.enabled && (options.servers.is_empty() || options.servers.contains(&server.id))
        })
        .map(|server| prepare_server_with_inputs(server, &manifest_dir))
        .collect::<Result<Vec<_>>>()?;
    if servers.is_empty() {
        bail!("the selected benchmark config contains no enabled servers")
    }

    let workloads = config
        .workloads
        .iter()
        .filter(|workload| {
            options.workloads.is_empty()
                && (profile.scenarios.is_empty() || profile.scenarios.contains(&workload.id))
                || options.workloads.contains(&workload.id)
        })
        .collect::<Vec<_>>();
    if workloads.is_empty() {
        bail!("the selected benchmark config contains no workloads")
    }
    let selected_fixture_ids =
        workloads.iter().map(|workload| workload.fixture.as_str()).collect::<BTreeSet<_>>();

    let mut fixtures = BTreeMap::new();
    let mut fixture_metadata = Vec::new();
    for spec in &config.fixtures {
        if !spec.enabled || !selected_fixture_ids.contains(spec.id.as_str()) {
            continue;
        }
        match FixtureSource::open(spec) {
            Ok(source) => {
                fixture_metadata.push(source.metadata().clone());
                fixtures.insert(spec.id.clone(), PreparedFixture { source });
            }
            Err(error) => {
                fixture_metadata.push(FixtureMetadata {
                    id: spec.id.clone(),
                    root: spec.root.clone(),
                    revision: spec.revision.clone(),
                    source_file_count: 0,
                    source_line_count: 0,
                    source_byte_count: 0,
                    content_sha256: "unavailable".into(),
                    corpus: spec.corpus.clone(),
                    source: spec.source.clone(),
                    solc: spec.solc.clone(),
                    solc_native_sha256: None,
                    solc_soljson_sha256: None,
                    solc_native_version: None,
                    foundry: spec.foundry.clone(),
                    foundry_native_sha256: None,
                    foundry_native_version: None,
                    dependencies: spec.dependencies.clone(),
                });
                eprintln!("fixture {} unavailable: {error:#}", spec.id);
            }
        }
    }

    let mut samples = Vec::new();
    let mut workload_repetitions = BTreeMap::new();
    for workload in workloads {
        let repeat = repeat_override.unwrap_or_else(|| profile.repetitions_for(workload));
        if profile.require_authoritative && !repeat.is_multiple_of(servers.len()) {
            bail!(
                "authoritative workload `{}` repetitions ({repeat}) must be divisible by the selected server count ({})",
                workload.id,
                servers.len()
            );
        }
        workload_repetitions.insert(workload.id.clone(), repeat);
        let Some(fixture) = fixtures.get(&workload.fixture) else {
            for server in &servers {
                for repetition in 0..repeat {
                    samples.push(unavailable_sample(
                        &server.metadata.id,
                        &workload.fixture,
                        &workload.id,
                        repetition,
                        RunStatus::Unavailable,
                        "fixture is unavailable",
                    ));
                }
            }
            continue;
        };
        for repetition in 0..repeat {
            for index in 0..servers.len() {
                let server = &servers[(index + repetition) % servers.len()];
                let sample = if server.metadata.status != ServerStatus::Available {
                    unavailable_sample(
                        &server.metadata.id,
                        &workload.fixture,
                        &workload.id,
                        repetition,
                        match server.metadata.status {
                            ServerStatus::Incompatible => RunStatus::Incompatible,
                            _ => RunStatus::Unavailable,
                        },
                        server.metadata.error.as_deref().unwrap_or("server is unavailable"),
                    )
                } else {
                    eprintln!(
                        "{} {}/{} repetition {}",
                        server.metadata.id,
                        workload.fixture,
                        workload.id,
                        repetition + 1
                    );
                    run_once(server, &fixture.source, workload, profile, repetition, timeout)
                };
                samples.push(sample);
            }
        }
    }

    let summary = summarize(SummaryInput {
        config_path: options.config.clone(),
        config: &config,
        servers: servers.into_iter().map(|server| server.metadata).collect(),
        fixtures: fixture_metadata,
        samples: &samples,
        repeat_override,
        workload_repetitions: &workload_repetitions,
        timeout_ms: timeout.as_millis().try_into().unwrap_or(u64::MAX),
        profile: options.profile.clone(),
    });
    write_reports(&options.output, &summary, &samples)?;
    let failed_runs = samples
        .iter()
        .filter(|sample| !matches!(sample.status, RunStatus::Pass | RunStatus::Unsupported))
        .count();
    let authority_failure = profile.require_authoritative && !summary.environment.authoritative;
    Ok(RunOutcome { summary, failed_runs, authority_failure })
}

#[cfg(all(test, unix))]
fn prepare_server(spec: &ServerSpec) -> Result<PreparedServer> {
    prepare_server_with_optional_inputs(spec, None)
}

fn prepare_server_with_inputs(spec: &ServerSpec, manifest_dir: &Path) -> Result<PreparedServer> {
    prepare_server_with_optional_inputs(spec, Some(manifest_dir))
}

fn prepare_server_with_optional_inputs(
    spec: &ServerSpec,
    manifest_dir: Option<&Path>,
) -> Result<PreparedServer> {
    let command = resolve_executable(&spec.command);
    let mut metadata = ServerMetadata {
        id: spec.id.clone(),
        label: spec.label.clone(),
        command: command.clone(),
        args: spec.args.clone(),
        transport: spec.transport,
        version_args: spec.version_args.clone(),
        version: None,
        locked_version: spec.locked_version.clone(),
        expected_version: spec.expected_version.clone(),
        enabled: spec.enabled,
        env: spec.env.clone(),
        initialization_options: spec.initialization_options.clone(),
        configuration: spec.configuration.clone(),
        source: spec.source.clone(),
        executable_sha256: command
            .is_file()
            .then(|| crate::lifecycle::sha256_path(&command).ok())
            .flatten(),
        artifact_path: spec.artifact.as_ref().map(|artifact| artifact.path.clone()),
        artifact_expected_sha256: spec
            .artifact
            .as_ref()
            .and_then(|artifact| artifact.sha256.clone()),
        artifact_sha256: spec
            .artifact
            .as_ref()
            .and_then(|artifact| crate::lifecycle::sha256_path(&artifact.path).ok()),
        required: spec.required,
        status: if spec.enabled { ServerStatus::Unavailable } else { ServerStatus::Disabled },
        error: None,
    };
    if !spec.enabled {
        return Ok(PreparedServer { spec: spec.clone(), metadata });
    }
    if let Some(manifest_dir) = manifest_dir
        && let Err(error) = verify_server_runtime_inputs(spec, manifest_dir)
    {
        metadata.status = ServerStatus::Incompatible;
        metadata.error = Some(format!("server input verification failed: {error:#}"));
        return Ok(PreparedServer { spec: spec.clone(), metadata });
    }
    if !command.is_absolute() && command.components().count() > 1
        || command.is_absolute() && !command.is_file()
    {
        metadata.error = Some(format!("server executable `{}` was not found", command.display()));
        return Ok(PreparedServer { spec: spec.clone(), metadata });
    }
    let artifact_error =
        match (metadata.artifact_expected_sha256.as_deref(), metadata.artifact_sha256.as_deref()) {
            (Some(expected), Some(actual)) if !expected.eq_ignore_ascii_case(actual) => Some((
                ServerStatus::Incompatible,
                format!("server artifact digest mismatch: expected {expected}, found {actual}"),
            )),
            (Some(_), None) => {
                Some((ServerStatus::Unavailable, "server artifact digest could not be read".into()))
            }
            _ => None,
        };
    if let Some((status, error)) = artifact_error {
        metadata.status = status;
        metadata.error = Some(error);
        return Ok(PreparedServer { spec: spec.clone(), metadata });
    }
    let version = inspect_version(&command, spec, VERSION_PROBE_TIMEOUT);
    match version {
        Ok(version) => {
            match verify_server_version_output(spec, &version) {
                Ok(()) => metadata.status = ServerStatus::Available,
                Err(error) => {
                    metadata.status = ServerStatus::Incompatible;
                    metadata.error = Some(format!("{error:#}"));
                }
            }
            metadata.version = Some(version);
        }
        Err(error) => metadata.error = Some(format!("version probe failed: {error:#}")),
    }
    Ok(PreparedServer { spec: spec.clone(), metadata })
}

fn run_once(
    server: &PreparedServer,
    fixture_source: &FixtureSource,
    workload: &WorkloadSpec,
    profile: &ProfileSpec,
    repetition: usize,
    timeout: Duration,
) -> RunSample {
    let mut sample = RunSample {
        server: server.metadata.id.clone(),
        fixture: workload.fixture.clone(),
        workload: workload.id.clone(),
        repetition,
        status: RunStatus::HarnessError,
        timings_ms: BTreeMap::new(),
        process: None,
        setup_phases: Vec::new(),
        observations: Observations::default(),
        correctness: Vec::new(),
        error: None,
    };
    let fixture = match fixture_source.materialize() {
        Ok(fixture) => fixture,
        Err(error) => return sample_with_error(sample, FailureKind::HarnessError, error),
    };
    let environment = match ProcessEnvironment::for_toolchains(
        fixture.metadata().solc.as_ref(),
        fixture.metadata().foundry.as_ref(),
        profile.network_isolation,
    ) {
        Ok(environment) => environment,
        Err(error) => return sample_with_error(sample, FailureKind::HarnessError, error),
    };
    let restart = workload.steps.iter().position(|step| matches!(step, StepSpec::Restart { .. }));
    let measured_steps = if let Some(index) = restart {
        let setup = run_phase(
            &server.spec,
            &fixture,
            &workload.steps[..index],
            profile,
            timeout,
            environment.clone(),
        );
        if !record_setup_phase(&mut sample, setup) {
            return sample;
        }
        if let StepSpec::Restart { invalidate: Some(invalidate) } = &workload.steps[index]
            && let Err(error) = invalidate_fixture(&fixture, invalidate)
        {
            return sample_with_error(sample, FailureKind::HarnessError, error);
        }
        &workload.steps[index + 1..]
    } else {
        &workload.steps
    };
    let measured = run_phase(&server.spec, &fixture, measured_steps, profile, timeout, environment);
    record_measured_phase(&mut sample, measured);
    sample
}

struct PhaseOutcome {
    result: Result<()>,
    process_result: Result<FinishedProcess>,
    timings: BTreeMap<String, f64>,
    correctness: Vec<CorrectnessResult>,
    fallback_observations: Observations,
}

fn run_phase(
    server: &ServerSpec,
    fixture: &Fixture,
    steps: &[StepSpec],
    profile: &ProfileSpec,
    timeout: Duration,
    environment: ProcessEnvironment,
) -> PhaseOutcome {
    let process =
        match LspProcess::spawn_with_environment(server, fixture.root(), timeout, environment) {
            Ok(process) => process,
            Err(error) => {
                return PhaseOutcome {
                    result: Err(error),
                    process_result: Err(anyhow!("server did not start")),
                    timings: BTreeMap::new(),
                    correctness: Vec::new(),
                    fallback_observations: Observations::default(),
                };
            }
        };
    let mut session = Session::new(process, fixture);
    let result = session.initialize().and_then(|()| session.execute(steps, profile));
    let graceful = result.is_ok()
        || result
            .as_ref()
            .err()
            .and_then(|error| error.downcast_ref::<WorkloadError>())
            .is_some_and(|error| {
                !matches!(error.kind, FailureKind::Timeout | FailureKind::Crashed)
            });
    let fallback_observations = session.process.observations().clone();
    let position_encoding = session.process.position_encoding().to_owned();
    let incremental = session.process.incremental_sync();
    let fixture = session.fixture;
    let documents = &mut session.documents;
    let mut handler = |edit: &Value| {
        let encoding = PositionEncoding::parse(&position_encoding)?;
        apply_workspace_edit_to(fixture, documents, encoding, incremental, edit)
            .map(|(_, notifications)| notifications)
    };
    let process_result = session.process.finish_with_handler(graceful, &mut handler);
    PhaseOutcome {
        result,
        process_result,
        timings: session.timings,
        correctness: session.correctness,
        fallback_observations,
    }
}

fn record_setup_phase(sample: &mut RunSample, outcome: PhaseOutcome) -> bool {
    let PhaseOutcome { result, process_result, timings, correctness, fallback_observations } =
        outcome;
    sample.correctness.extend(correctness.into_iter().map(|mut result| {
        result.probe = format!("cache-setup/{}", result.probe);
        result
    }));
    for (name, value) in timings {
        sample.timings_ms.insert(format!("cache_setup_{name}"), value);
    }
    match (result, process_result) {
        (Ok(()), Ok(FinishedProcess { metrics, observations }))
            if metrics.exit_code == Some(0) && !metrics.forced_kill =>
        {
            sample.timings_ms.insert("cache_population_process_ms".into(), metrics.wall_ms);
            sample.setup_phases.push(ProcessPhase {
                name: "cache-population".into(),
                process: metrics,
                observations,
            });
            true
        }
        (Err(error), Ok(FinishedProcess { metrics, observations })) => {
            let kind = error
                .downcast_ref::<WorkloadError>()
                .map_or(FailureKind::HarnessError, |error| error.kind);
            sample.status = status_for(kind);
            sample.error = Some(format!("cache setup failed: {error:#}"));
            sample.setup_phases.push(ProcessPhase {
                name: "cache-population".into(),
                process: metrics,
                observations,
            });
            false
        }
        (Ok(()), Ok(FinishedProcess { metrics, observations })) => {
            sample.status = RunStatus::Crash;
            sample.error = Some(format!("cache setup server exited with {:?}", metrics.exit_code));
            sample.setup_phases.push(ProcessPhase {
                name: "cache-population".into(),
                process: metrics,
                observations,
            });
            false
        }
        (result, Err(stop_error)) => {
            sample.status = RunStatus::HarnessError;
            sample.error = Some(match result {
                Ok(()) => format!("failed to stop cache setup server: {stop_error:#}"),
                Err(error) => {
                    format!("cache setup failed: {error:#}; failed to stop server: {stop_error:#}")
                }
            });
            sample.observations = fallback_observations;
            false
        }
    }
}

fn record_measured_phase(sample: &mut RunSample, outcome: PhaseOutcome) {
    let PhaseOutcome { result, process_result, timings, correctness, fallback_observations } =
        outcome;
    sample.correctness.extend(correctness);
    sample.timings_ms.extend(timings);
    match (result, process_result) {
        (Ok(()), Ok(FinishedProcess { metrics, observations }))
            if metrics.exit_code == Some(0) && !metrics.forced_kill =>
        {
            sample.status = RunStatus::Pass;
            sample.process = Some(metrics);
            sample.observations = observations;
        }
        (Err(error), Ok(FinishedProcess { metrics, observations })) => {
            let kind = error
                .downcast_ref::<WorkloadError>()
                .map_or(FailureKind::HarnessError, |error| error.kind);
            sample.status = status_for(kind);
            sample.error = Some(format!("{error:#}"));
            sample.process = Some(metrics);
            sample.observations = observations;
        }
        (Ok(()), Ok(FinishedProcess { metrics, observations })) => {
            sample.status = RunStatus::Crash;
            sample.error = Some(format!("server exited with {:?}", metrics.exit_code));
            sample.process = Some(metrics);
            sample.observations = observations;
        }
        (result, Err(stop_error)) => {
            sample.status = RunStatus::HarnessError;
            sample.error = Some(match result {
                Ok(()) => format!("failed to stop server: {stop_error:#}"),
                Err(error) => format!("{error:#}; failed to stop server: {stop_error:#}"),
            });
            sample.observations = fallback_observations;
        }
    }
}

fn invalidate_fixture(
    fixture: &Fixture,
    replacement: &crate::config::DiskReplacementSpec,
) -> Result<()> {
    let path = fixture.path(&replacement.path)?;
    let anchor = fixture.anchor(&replacement.anchor)?;
    if anchor.path != path {
        bail!(
            "restart invalidation anchor `{}` belongs to `{}`",
            replacement.anchor,
            anchor.path.display()
        )
    }
    let needle = fixture.anchor_needle(&replacement.anchor)?;
    let mut text = fs::read_to_string(&path)?;
    let start = text.find(&needle).with_context(|| {
        format!("restart invalidation anchor `{}` disappeared", replacement.anchor)
    })?;
    text.replace_range(start..start + needle.len(), &replacement.text);
    atomic_write(&path, &text, 0)
}

fn sample_with_error(mut sample: RunSample, kind: FailureKind, error: anyhow::Error) -> RunSample {
    sample.status = status_for(kind);
    sample.error = Some(format!("{error:#}"));
    sample
}

fn unavailable_sample(
    server: &str,
    fixture: &str,
    workload: &str,
    repetition: usize,
    status: RunStatus,
    error: &str,
) -> RunSample {
    RunSample {
        server: server.into(),
        fixture: fixture.into(),
        workload: workload.into(),
        repetition,
        status,
        timings_ms: BTreeMap::new(),
        process: None,
        setup_phases: Vec::new(),
        observations: Observations::default(),
        correctness: Vec::new(),
        error: Some(error.into()),
    }
}

fn status_for(kind: FailureKind) -> RunStatus {
    match kind {
        FailureKind::Unsupported => RunStatus::Unsupported,
        FailureKind::Incorrect => RunStatus::Incorrect,
        FailureKind::Timeout => RunStatus::Timeout,
        FailureKind::Crashed => RunStatus::Crash,
        FailureKind::HarnessError => RunStatus::HarnessError,
    }
}

struct Session<'a> {
    process: LspProcess,
    fixture: &'a Fixture,
    documents: BTreeMap<PathBuf, Document>,
    timings: BTreeMap<String, f64>,
    correctness: Vec<CorrectnessResult>,
    barriers: BTreeMap<String, Instant>,
    last_open_started: Option<Instant>,
    readiness_quiet: Duration,
}

#[derive(Clone)]
struct Document {
    text: String,
    version: i32,
}

enum WorkspaceEditOperation {
    Text { uri: Url, version: Option<i32>, edits: Vec<Value> },
    Create { uri: Url, overwrite: bool, ignore_if_exists: bool },
    Rename { old_uri: Url, new_uri: Url, overwrite: bool, ignore_if_exists: bool },
    Delete { uri: Url, recursive: bool, ignore_if_not_exists: bool },
}

impl<'a> Session<'a> {
    fn new(process: LspProcess, fixture: &'a Fixture) -> Self {
        Self {
            process,
            fixture,
            documents: BTreeMap::new(),
            timings: BTreeMap::new(),
            correctness: Vec::new(),
            barriers: BTreeMap::new(),
            last_open_started: None,
            readiness_quiet: Duration::from_millis(50),
        }
    }

    fn initialize(&mut self) -> Result<()> {
        let root_uri = file_uri(self.fixture.root())?;
        self.process.set_root(root_uri.as_str());
        let root_path = self.fixture.root().display().to_string();
        let result = self.setup_request(
            "initialize",
            json!({
                "processId": std::process::id(),
                "clientInfo": {"name": "solar-lsp-bench", "version": env!("CARGO_PKG_VERSION")},
                "rootUri": root_uri,
                "rootPath": root_path,
                "capabilities": {
                    "workspace": {
                        "workspaceFolders": true,
                        "configuration": true,
                        "applyEdit": true,
                        "workspaceEdit": {
                            "documentChanges": true,
                            "resourceOperations": ["create", "rename", "delete"],
                            "failureHandling": "transactional"
                        },
                        "fileOperations": {
                            "dynamicRegistration": true,
                            "didCreate": true,
                            "willCreate": true,
                            "didRename": true,
                            "willRename": true,
                            "didDelete": true,
                            "willDelete": true
                        }
                    },
                    "textDocument": {
                        "synchronization": {"dynamicRegistration": true, "didSave": true},
                        "definition": {"linkSupport": true},
                        "completion": {
                            "completionItem": {"snippetSupport": false},
                            "contextSupport": true
                        },
                        "hover": {"contentFormat": ["markdown", "plaintext"]},
                        "rename": {"prepareSupport": true}
                    },
                    "window": {"workDoneProgress": true},
                    "general": {"positionEncodings": ["utf-8", "utf-16", "utf-32"]}
                },
                "initializationOptions": self.process.initialization_options(),
                "workspaceFolders": [{"uri": root_uri, "name": "lsp-bench"}]
            }),
        )?;
        self.process.set_initialize_result(&result);
        self.timings.insert(
            "spawn_to_initialize_response_ms".into(),
            duration_ms(self.process.process_started_at().elapsed()),
        );
        PositionEncoding::parse(self.process.position_encoding())
            .map_err(|error| WorkloadError::new(FailureKind::HarnessError, format!("{error:#}")))?;
        self.process.notify("initialized", json!({}))?;
        self.wait_for_readiness()
    }

    fn execute(&mut self, steps: &[StepSpec], profile: &ProfileSpec) -> Result<()> {
        self.readiness_quiet = Duration::from_millis(profile.readiness_quiet_ms);
        for step in steps {
            match step {
                StepSpec::Open { path } => self.open(path)?,
                StepSpec::Probe { name, probe } => self.probe(name, probe)?,
                StepSpec::Replace { path, anchor, text, probe } => {
                    self.replace(path, anchor, text)?;
                    if let Some(probe) = probe {
                        self.probe("edit-ready", probe)?;
                    }
                }
                StepSpec::Save { path, probe } => {
                    self.save(path)?;
                    if let Some(probe) = probe {
                        self.probe("save-ready", probe)?;
                    }
                }
                StepSpec::Warm { name, probe, warmup, samples } => {
                    self.warm(
                        name,
                        probe,
                        warmup.unwrap_or(profile.warmup),
                        samples.unwrap_or(profile.samples),
                    )
                    .map_err(anyhow::Error::from)?;
                }
                StepSpec::Rename { path, anchor, new_name, expected_edits, probe } => {
                    self.rename_symbol(path, anchor, new_name, expected_edits, probe.as_ref())?;
                }
                StepSpec::CreateFile { path, text, probe } => {
                    self.create_file(path, text, probe.as_ref())?;
                }
                StepSpec::RenameFile { from, to, probe } => {
                    self.rename_file(from, to, probe.as_ref())?;
                }
                StepSpec::DeleteFile { path, probe } => {
                    self.delete_file(path, probe.as_ref())?;
                }
                StepSpec::Restart { .. } => bail!("restart step reached session execution"),
            }
        }
        Ok(())
    }

    fn open(&mut self, relative: &Path) -> Result<()> {
        let path = self.fixture.path(relative)?;
        if self.documents.contains_key(&path) {
            return Ok(());
        }
        let text = fs::read_to_string(&path)
            .with_context(|| format!("failed to read `{}`", relative.display()))?;
        let uri = file_uri(&path)?;
        let started = Instant::now();
        if self.process.supports_open() {
            self.process.notify(
                "textDocument/didOpen",
                json!({
                    "textDocument": {
                        "uri": uri,
                        "languageId": "solidity",
                        "version": 1,
                        "text": text
                    }
                }),
            )?;
        }
        self.last_open_started = Some(started);
        self.documents.insert(path, Document { text, version: 1 });
        Ok(())
    }

    fn require_open_for_probe(&self, relative: &Path) -> std::result::Result<(), WorkloadError> {
        let path = self.fixture.path(relative).map_err(harness_error)?;
        if self.documents.contains_key(&path) {
            Ok(())
        } else {
            Err(WorkloadError::new(
                FailureKind::HarnessError,
                format!(
                    "probe target `{}` is not open; add an explicit `open` step",
                    relative.display()
                ),
            ))
        }
    }

    fn replace(&mut self, relative: &Path, anchor_name: &str, replacement: &str) -> Result<()> {
        if !self.process.supports_change() {
            return Err(WorkloadError::new(
                FailureKind::Unsupported,
                "server does not advertise `textDocument/didChange`",
            )
            .into());
        }
        self.open(relative)?;
        let path = self.fixture.path(relative)?;
        let anchor = self.fixture.anchor(anchor_name)?;
        if anchor.path != path {
            return Err(WorkloadError::new(
                FailureKind::HarnessError,
                format!("anchor `{anchor_name}` belongs to `{}`", anchor.path.display()),
            )
            .into());
        }
        let (version, start, end, text) = {
            let document = self.documents.get_mut(&path).context("document is not open")?;
            let needle = self.fixture.anchor_needle(anchor_name)?;
            let offset = document
                .text
                .find(&needle)
                .context("edit anchor disappeared from open document")?;
            let end_offset = offset + needle.len();
            let encoding = PositionEncoding::parse(self.process.position_encoding())?;
            let start = position_at_with_encoding(&document.text, offset, encoding);
            let end = position_at_with_encoding(&document.text, end_offset, encoding);
            document.text.replace_range(offset..end_offset, replacement);
            document.version += 1;
            (document.version, start, end, document.text.clone())
        };
        let uri = file_uri(&path)?;
        self.barriers.insert("edit".into(), Instant::now());
        self.process.send_change(
            uri.as_str(),
            version,
            json!(start),
            json!(end),
            replacement,
            &text,
        )
    }

    fn save(&mut self, relative: &Path) -> Result<()> {
        if !self.process.supports_save() {
            return Err(WorkloadError::new(
                FailureKind::Unsupported,
                "server does not advertise `textDocument/didSave`",
            )
            .into());
        }
        let path = self.fixture.path(relative)?;
        let document = self.documents.get(&path).context("document is not open")?.clone();
        let uri = file_uri(&path)?;
        let started = Instant::now();
        self.barriers.insert("save".into(), started);
        atomic_write(&path, &document.text, document.version)?;
        let mut params = json!({
            "textDocument": {"uri": uri, "version": document.version}
        });
        if self.process.save_include_text() {
            params["text"] = Value::String(document.text);
        }
        self.process.notify("textDocument/didSave", params)?;
        self.timings
            .insert(format!("save_{}_ms", relative.display()), duration_ms(started.elapsed()));
        Ok(())
    }

    fn rename_symbol(
        &mut self,
        relative: &Path,
        anchor: &str,
        new_name: &str,
        expected_edits: &[crate::config::ExpectedLocationSpec],
        probe: Option<&ProbeSpec>,
    ) -> Result<()> {
        if !self.process.supports("textDocument/rename") {
            return Err(WorkloadError::new(
                FailureKind::Unsupported,
                "server does not advertise rename",
            )
            .into());
        }
        self.open(relative)?;
        let encoding = PositionEncoding::parse(self.process.position_encoding())?;
        let anchor = self.fixture.anchor_with_encoding(anchor, encoding)?;
        let uri = file_uri(&anchor.path)?;
        let expected = expected_edits
            .iter()
            .map(|expected| {
                let path = self.fixture.path(&expected.path)?;
                let anchor = self.fixture.anchor_with_encoding(&expected.anchor, encoding)?;
                if anchor.path != path {
                    bail!(
                        "rename expected anchor `{}` belongs to `{}`, not `{}`",
                        expected.anchor,
                        anchor.path.display(),
                        path.display()
                    )
                }
                let text = fs::read_to_string(&path)?;
                let range = solidity_identifier_range(&text, anchor.position, encoding)?;
                Ok((file_uri(&path)?, range))
            })
            .collect::<Result<Vec<_>>>()?;
        let started = Instant::now();
        self.barriers.insert("rename".into(), started);
        let edit = self.request(
            "textDocument/rename",
            json!({
                "textDocument": {"uri": uri},
                "position": anchor.position,
                "newName": new_name
            }),
            true,
        )?;
        if edit.is_null() {
            return Err(WorkloadError::new(FailureKind::Incorrect, "rename returned null").into());
        }
        let operations = parse_workspace_edit(self.fixture, &edit)?;
        validate_rename_operations(&operations, new_name, &expected)?;
        let applied = self.apply_workspace_edit(&edit)?;
        if applied == 0 {
            return Err(WorkloadError::new(
                FailureKind::Incorrect,
                "rename WorkspaceEdit changed no files",
            )
            .into());
        }
        if let Some(probe) = probe {
            self.probe_with_policy("rename-ready", probe, true)?;
        } else {
            self.timings.insert("rename_end_to_end_ms".into(), duration_ms(started.elapsed()));
            self.barriers.remove("rename");
        }
        Ok(())
    }

    fn create_file(
        &mut self,
        relative: &Path,
        text: &str,
        probe: Option<&ProbeSpec>,
    ) -> Result<()> {
        let path = self.fixture.path(relative)?;
        if path.exists() {
            bail!("lifecycle create target `{}` already exists", relative.display())
        }
        let started = Instant::now();
        self.barriers.insert("create-file".into(), started);
        let uri = file_uri(&path)?;
        self.require_file_operation("workspace/willCreateFiles", &uri, false)?;
        self.require_file_operation("workspace/didCreateFiles", &uri, false)?;
        self.prepare_file_operation("workspace/willCreateFiles", json!({"files": [{"uri": uri}]}))?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        atomic_write(&path, text, 1)?;
        let uri = file_uri(&path)?;
        self.process.notify("workspace/didCreateFiles", json!({"files": [{"uri": uri}]}))?;
        self.finish_lifecycle("create-file", started, probe)
    }

    fn rename_file(&mut self, from: &Path, to: &Path, probe: Option<&ProbeSpec>) -> Result<()> {
        let old_path = self.fixture.path(from)?;
        let new_path = self.fixture.path(to)?;
        if !old_path.is_file() {
            bail!("lifecycle rename source `{}` does not exist", from.display())
        }
        if new_path.exists() {
            bail!("lifecycle rename target `{}` already exists", to.display())
        }
        let started = Instant::now();
        self.barriers.insert("rename-file".into(), started);
        let old_uri = file_uri(&old_path)?;
        let new_uri = file_uri(&new_path)?;
        self.require_file_operation_pair("workspace/willRenameFiles", &old_uri, &new_uri)?;
        self.require_file_operation_pair("workspace/didRenameFiles", &old_uri, &new_uri)?;
        self.prepare_file_operation(
            "workspace/willRenameFiles",
            json!({"files": [{
                "oldUri": old_uri,
                "newUri": new_uri
            }]}),
        )?;
        if let Some(parent) = new_path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::rename(&old_path, &new_path)?;
        if let Some(document) = self.documents.remove(&old_path) {
            self.documents.insert(new_path, document);
        }
        self.process.notify(
            "workspace/didRenameFiles",
            json!({"files": [{"oldUri": old_uri, "newUri": new_uri}]}),
        )?;
        self.finish_lifecycle("rename-file", started, probe)
    }

    fn delete_file(&mut self, relative: &Path, probe: Option<&ProbeSpec>) -> Result<()> {
        let path = self.fixture.path(relative)?;
        let started = Instant::now();
        self.barriers.insert("delete-file".into(), started);
        let uri = file_uri(&path)?;
        self.require_file_operation("workspace/willDeleteFiles", &uri, false)?;
        self.require_file_operation("workspace/didDeleteFiles", &uri, false)?;
        self.prepare_file_operation("workspace/willDeleteFiles", json!({"files": [{"uri": uri}]}))?;
        fs::remove_file(&path)?;
        self.documents.remove(&path);
        self.process.notify("workspace/didDeleteFiles", json!({"files": [{"uri": uri}]}))?;
        self.finish_lifecycle("delete-file", started, probe)
    }

    fn require_file_operation(&self, method: &str, uri: &Url, is_directory: bool) -> Result<()> {
        if self.process.supports_file_operation(method, uri, is_directory) {
            return Ok(());
        }
        Err(WorkloadError::new(
            FailureKind::Unsupported,
            format!("server does not advertise applicable `{method}` for `{uri}`"),
        )
        .into())
    }

    fn require_file_operation_pair(
        &self,
        method: &str,
        old_uri: &Url,
        new_uri: &Url,
    ) -> Result<()> {
        self.require_file_operation(method, old_uri, false)?;
        self.require_file_operation(method, new_uri, false)
    }

    fn finish_lifecycle(
        &mut self,
        name: &str,
        started: Instant,
        probe: Option<&ProbeSpec>,
    ) -> Result<()> {
        if let Some(probe) = probe {
            self.probe_after_file_operation(&format!("{name}-ready"), probe)
        } else {
            self.timings.insert(format!("{name}_end_to_end_ms"), duration_ms(started.elapsed()));
            self.barriers.remove(name);
            Ok(())
        }
    }

    fn prepare_file_operation(&mut self, method: &str, params: Value) -> Result<()> {
        let edit = self.request(method, params, true).map_err(anyhow::Error::from)?;
        if !edit.is_null() && !edit.is_object() {
            return Err(WorkloadError::new(
                FailureKind::Incorrect,
                format!("`{method}` returned neither `null` nor a WorkspaceEdit object"),
            )
            .into());
        }
        if !edit.is_null() {
            self.apply_workspace_edit(&edit)?;
        }
        Ok(())
    }

    fn apply_workspace_edit(&mut self, edit: &Value) -> Result<usize> {
        let encoding = PositionEncoding::parse(self.process.position_encoding())?;
        let incremental = self.process.incremental_sync();
        let (applied, notifications) = apply_workspace_edit_to(
            self.fixture,
            &mut self.documents,
            encoding,
            incremental,
            edit,
        )?;
        self.process.notify_workspace_edit_notifications(notifications)?;
        Ok(applied)
    }

    fn probe(&mut self, name: &str, probe: &ProbeSpec) -> Result<()> {
        self.probe_with_policy(name, probe, false)
    }

    fn probe_after_file_operation(&mut self, name: &str, probe: &ProbeSpec) -> Result<()> {
        self.probe_with_policy(name, probe, true)
    }

    fn probe_with_policy(
        &mut self,
        name: &str,
        probe: &ProbeSpec,
        allow_unopened_target: bool,
    ) -> Result<()> {
        let started = Instant::now();
        let deadline = Instant::now() + self.process.timeout();
        loop {
            let attempt = self.probe_once(probe, false, allow_unopened_target).and_then(|()| {
                if name == "cold-ready" {
                    self.verify_full_workspace_readiness()?;
                }
                self.wait_for_readiness().map_err(classify_request_error)
            });
            let failure = match attempt {
                Ok(()) => {
                    let elapsed = duration_ms(started.elapsed());
                    self.timings.insert(format!("{name}_ms"), elapsed);
                    for (barrier, barrier_started) in std::mem::take(&mut self.barriers) {
                        self.timings.insert(
                            format!("{barrier}_to_{name}_ms"),
                            duration_ms(barrier_started.elapsed()),
                        );
                    }
                    if name == "cold-ready" {
                        self.timings.insert(
                            "cold_ready_ms".into(),
                            duration_ms(self.process.process_started_at().elapsed()),
                        );
                        if let Some(open_started) = self.last_open_started.take() {
                            self.timings.insert(
                                "did_open_to_semantic_ready_ms".into(),
                                duration_ms(open_started.elapsed()),
                            );
                        }
                    }
                    self.correctness.push(CorrectnessResult {
                        probe: name.into(),
                        ok: true,
                        detail: "matched".into(),
                    });
                    return Ok(());
                }
                Err(error) => error,
            };
            if matches!(failure.kind, FailureKind::Unsupported) {
                self.correctness.push(CorrectnessResult {
                    probe: name.into(),
                    ok: false,
                    detail: failure.message.clone(),
                });
                return Err(failure.into());
            }
            if Instant::now() >= deadline {
                let error = WorkloadError::new(
                    failure.kind,
                    format!("probe `{name}` did not become correct: {}", failure.message),
                );
                self.correctness.push(CorrectnessResult {
                    probe: name.into(),
                    ok: false,
                    detail: error.message.clone(),
                });
                return Err(error.into());
            }
            thread::sleep(Duration::from_millis(20));
        }
    }

    fn warm(
        &mut self,
        _name: &str,
        probe: &ProbeSpec,
        warmup: usize,
        samples: usize,
    ) -> std::result::Result<(), WorkloadError> {
        for _ in 0..warmup {
            self.probe_once(probe, false, false)?;
        }
        for _ in 0..samples {
            self.probe_once(probe, true, false)?;
        }
        Ok(())
    }

    fn probe_once(
        &mut self,
        probe: &ProbeSpec,
        measured: bool,
        allow_unopened_target: bool,
    ) -> std::result::Result<(), WorkloadError> {
        match probe {
            ProbeSpec::Definition { path, anchor, expected_path, expected_anchor } => {
                if !self.process.supports("textDocument/definition") {
                    return Err(WorkloadError::new(
                        FailureKind::Unsupported,
                        "server does not advertise definition",
                    ));
                }
                let encoding = self.position_encoding()?;
                if !allow_unopened_target {
                    self.require_open_for_probe(path)?;
                }
                let source_anchor =
                    self.fixture.anchor_with_encoding(anchor, encoding).map_err(harness_error)?;
                let expected = self
                    .fixture
                    .anchor_with_encoding(expected_anchor, encoding)
                    .map_err(harness_error)?;
                let uri = file_uri(&source_anchor.path).map_err(harness_error)?;
                let expected_uri =
                    file_uri(&self.fixture.path(expected_path).map_err(harness_error)?)
                        .map_err(harness_error)?;
                let value = self.request(
                    "textDocument/definition",
                    json!({"textDocument": {"uri": uri}, "position": source_anchor.position}),
                    measured,
                )?;
                validate_definition(value, &expected_uri, &expected)
            }
            ProbeSpec::Completion { path, anchor, expected_label } => {
                if !self.process.supports("textDocument/completion") {
                    return Err(WorkloadError::new(
                        FailureKind::Unsupported,
                        "server does not advertise completion",
                    ));
                }
                let encoding = self.position_encoding()?;
                if !allow_unopened_target {
                    self.require_open_for_probe(path)?;
                }
                let source_anchor =
                    self.fixture.anchor_with_encoding(anchor, encoding).map_err(harness_error)?;
                let uri = file_uri(&source_anchor.path).map_err(harness_error)?;
                let context = if self.process.completion_uses_trigger(".") {
                    json!({"triggerKind": 2, "triggerCharacter": "."})
                } else {
                    json!({"triggerKind": 1})
                };
                let value = self.request(
                    "textDocument/completion",
                    json!({
                        "textDocument": {"uri": uri},
                        "position": source_anchor.position,
                        "context": context
                    }),
                    measured,
                )?;
                validate_completion(value, expected_label)
            }
            ProbeSpec::Hover { path, anchor, expected_text } => {
                if !self.process.supports("textDocument/hover") {
                    return Err(WorkloadError::new(
                        FailureKind::Unsupported,
                        "server does not advertise hover",
                    ));
                }
                let encoding = self.position_encoding()?;
                if !allow_unopened_target {
                    self.require_open_for_probe(path)?;
                }
                let source_anchor =
                    self.fixture.anchor_with_encoding(anchor, encoding).map_err(harness_error)?;
                let uri = file_uri(&source_anchor.path).map_err(harness_error)?;
                let value = self.request(
                    "textDocument/hover",
                    json!({"textDocument": {"uri": uri}, "position": source_anchor.position}),
                    measured,
                )?;
                validate_hover(value, expected_text)
            }
            ProbeSpec::References { path, anchor, min_count, expected_locations } => {
                if !self.process.supports("textDocument/references") {
                    return Err(WorkloadError::new(
                        FailureKind::Unsupported,
                        "server does not advertise references",
                    ));
                }
                let encoding = self.position_encoding()?;
                if !allow_unopened_target {
                    self.require_open_for_probe(path)?;
                }
                let source_anchor =
                    self.fixture.anchor_with_encoding(anchor, encoding).map_err(harness_error)?;
                let uri = file_uri(&source_anchor.path).map_err(harness_error)?;
                let value = self.request(
                    "textDocument/references",
                    json!({
                        "textDocument": {"uri": uri},
                        "position": source_anchor.position,
                        "context": {"includeDeclaration": true}
                    }),
                    measured,
                )?;
                let expected = expected_locations
                    .iter()
                    .map(|location| {
                        let anchor = self
                            .fixture
                            .anchor_with_encoding(&location.anchor, encoding)
                            .map_err(harness_error)?;
                        let uri =
                            file_uri(&self.fixture.path(&location.path).map_err(harness_error)?)
                                .map_err(harness_error)?;
                        Ok((uri, anchor))
                    })
                    .collect::<std::result::Result<Vec<_>, WorkloadError>>()?;
                validate_references(value, *min_count, &expected)
            }
            ProbeSpec::DocumentSymbol { path, min_count, expected_name } => {
                if !self.process.supports("textDocument/documentSymbol") {
                    return Err(WorkloadError::new(
                        FailureKind::Unsupported,
                        "server does not advertise document symbols",
                    ));
                }
                if !allow_unopened_target {
                    self.require_open_for_probe(path)?;
                }
                let uri = file_uri(&self.fixture.path(path).map_err(harness_error)?)
                    .map_err(harness_error)?;
                let value = self.request(
                    "textDocument/documentSymbol",
                    json!({"textDocument": {"uri": uri}}),
                    measured,
                )?;
                validate_document_symbols(value, *min_count, expected_name.as_deref())
            }
            ProbeSpec::WorkspaceSymbol { query, expected_name, expected_path, present } => {
                if !self.process.supports("workspace/symbol") {
                    return Err(WorkloadError::new(
                        FailureKind::Unsupported,
                        "server does not advertise workspace symbols",
                    ));
                }
                let expected_uri =
                    file_uri(&self.fixture.path(expected_path).map_err(harness_error)?)
                        .map_err(harness_error)?;
                let value = self.request("workspace/symbol", json!({"query": query}), measured)?;
                validate_workspace_symbol(value, expected_name, &expected_uri, *present)
            }
        }
    }

    fn position_encoding(&self) -> std::result::Result<PositionEncoding, WorkloadError> {
        PositionEncoding::parse(self.process.position_encoding())
            .map_err(|error| WorkloadError::new(FailureKind::HarnessError, format!("{error:#}")))
    }

    fn wait_for_readiness(&mut self) -> Result<()> {
        let encoding = PositionEncoding::parse(self.process.position_encoding())?;
        let incremental = self.process.incremental_sync();
        let fixture = self.fixture;
        let documents = &mut self.documents;
        let mut handler = |edit: &Value| {
            apply_workspace_edit_to(fixture, documents, encoding, incremental, edit)
                .map(|(_, notifications)| notifications)
        };
        self.process.wait_for_readiness_with_handler(self.readiness_quiet, &mut handler)
    }

    fn verify_full_workspace_readiness(&mut self) -> std::result::Result<(), WorkloadError> {
        if !self.process.supports("textDocument/documentSymbol") {
            return Err(WorkloadError::new(
                FailureKind::Unsupported,
                "server does not advertise document symbols required for full workspace readiness",
            ));
        }
        let files = self.fixture.source_files().map_err(harness_error)?;
        for relative in &files {
            let uri = file_uri(&self.fixture.path(relative).map_err(harness_error)?)
                .map_err(harness_error)?;
            let value = self.request(
                "textDocument/documentSymbol",
                json!({"textDocument": {"uri": uri}}),
                false,
            )?;
            validate_document_symbols(value, 1, None)?;
        }
        self.correctness.push(CorrectnessResult {
            probe: "workspace-indexed".into(),
            ok: true,
            detail: format!("validated {} Solidity source files", files.len()),
        });
        Ok(())
    }

    fn setup_request(&mut self, method: &str, params: Value) -> Result<Value> {
        self.request_with_handler(method, params, false)
    }

    fn request_with_handler(
        &mut self,
        method: &str,
        params: Value,
        measured: bool,
    ) -> Result<Value> {
        let encoding = PositionEncoding::parse(self.process.position_encoding())?;
        let incremental = self.process.incremental_sync();
        let fixture = self.fixture;
        let documents = &mut self.documents;
        let mut handler = |edit: &Value| {
            apply_workspace_edit_to(fixture, documents, encoding, incremental, edit)
                .map(|(_, notifications)| notifications)
        };
        if measured {
            self.process.request_with_handler(method, params, &mut handler)
        } else {
            self.process.setup_request_with_handler(method, params, &mut handler)
        }
    }

    fn request(
        &mut self,
        method: &str,
        params: Value,
        measured: bool,
    ) -> std::result::Result<Value, WorkloadError> {
        let result = self.request_with_handler(method, params, measured);
        result.map_err(classify_request_error)
    }
}

enum WorkspaceUndo {
    Restore { backup: PathBuf, path: PathBuf },
    Remove { path: PathBuf },
    Rename { from: PathBuf, to: PathBuf },
    RemoveDirectory { path: PathBuf },
}

struct WorkspaceEditTransaction {
    backup: tempfile::TempDir,
    undo: Vec<WorkspaceUndo>,
}

impl WorkspaceEditTransaction {
    fn new(root: &Path) -> Result<Self> {
        let parent = root.parent().context("fixture root has no parent")?;
        let backup =
            tempfile::Builder::new().prefix(".lsp-bench-workspace-edit-").tempdir_in(parent)?;
        Ok(Self { backup, undo: Vec::new() })
    }

    fn write(&mut self, path: &Path, text: &str, version: i32) -> Result<()> {
        let existed = self.backup_existing(path)?;
        self.create_parent_directories(path)?;
        atomic_write(path, text, version)?;
        if !existed {
            self.undo.push(WorkspaceUndo::Remove { path: path.to_owned() });
        }
        Ok(())
    }

    fn rename(&mut self, from: &Path, to: &Path, overwrite: bool) -> Result<()> {
        if overwrite {
            self.backup_existing(to)?;
        }
        self.create_parent_directories(to)?;
        fs::rename(from, to)?;
        self.undo.push(WorkspaceUndo::Rename { from: to.to_owned(), to: from.to_owned() });
        Ok(())
    }

    fn delete(&mut self, path: &Path) -> Result<()> {
        if !self.backup_existing(path)? {
            bail!("transaction delete target `{}` does not exist", path.display())
        }
        Ok(())
    }

    fn backup_existing(&mut self, path: &Path) -> Result<bool> {
        if fs::symlink_metadata(path).is_err() {
            return Ok(false);
        }
        let backup = self.backup.path().join(self.undo.len().to_string());
        fs::rename(path, &backup)?;
        self.undo.push(WorkspaceUndo::Restore { backup, path: path.to_owned() });
        Ok(true)
    }

    fn create_parent_directories(&mut self, path: &Path) -> Result<()> {
        let Some(parent) = path.parent() else { return Ok(()) };
        let mut missing = Vec::new();
        let mut current = parent;
        while fs::symlink_metadata(current).is_err() {
            missing.push(current.to_owned());
            current = current.parent().context("workspace path has no existing ancestor")?;
        }
        fs::create_dir_all(parent)?;
        for directory in missing.into_iter().rev() {
            self.undo.push(WorkspaceUndo::RemoveDirectory { path: directory });
        }
        Ok(())
    }

    fn rollback(mut self) -> Result<()> {
        while let Some(undo) = self.undo.pop() {
            match undo {
                WorkspaceUndo::Restore { backup, path } => {
                    remove_workspace_path(&path)?;
                    if let Some(parent) = path.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(backup, path)?;
                }
                WorkspaceUndo::Remove { path } => remove_workspace_path(&path)?,
                WorkspaceUndo::Rename { from, to } => {
                    if let Some(parent) = to.parent() {
                        fs::create_dir_all(parent)?;
                    }
                    fs::rename(from, to)?;
                }
                WorkspaceUndo::RemoveDirectory { path } => match fs::remove_dir(path) {
                    Ok(()) => {}
                    Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                    Err(error) => return Err(error.into()),
                },
            }
        }
        Ok(())
    }
}

fn remove_workspace_path(path: &Path) -> Result<()> {
    let metadata = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(error) => return Err(error.into()),
    };
    if metadata.file_type().is_dir() {
        fs::remove_dir_all(path)?;
    } else {
        fs::remove_file(path)?;
    }
    Ok(())
}

fn apply_workspace_edit_to(
    fixture: &Fixture,
    documents: &mut BTreeMap<PathBuf, Document>,
    encoding: PositionEncoding,
    incremental: bool,
    edit: &Value,
) -> Result<(usize, WorkspaceEditNotifications)> {
    // Parse and validate every URI before mutating the fixture. Application then follows the
    // declared `documentChanges` order, which matters when a text edit precedes a rename or
    // delete in one WorkspaceEdit.
    let operations = parse_workspace_edit(fixture, edit)?;
    let original_documents = documents.clone();
    let mut transaction = WorkspaceEditTransaction::new(fixture.root())?;
    let result = (|| {
        let mut notifications = Vec::new();
        let mut applied = 0;
        for operation in operations {
            match operation {
                WorkspaceEditOperation::Text { uri, version, edits } => {
                    let path = workspace_path_for(fixture, &uri, "WorkspaceEdit")?;
                    let was_open = documents.contains_key(&path);
                    if let Some(expected_version) = version {
                        let actual_version = documents.get(&path).with_context(|| {
                            format!(
                                "versioned WorkspaceEdit targets unopened document `{}`",
                                path.display()
                            )
                        })?;
                        if actual_version.version != expected_version {
                            bail!(
                                "WorkspaceEdit targets version {expected_version} of `{}`, but the open document is version {}",
                                path.display(),
                                actual_version.version
                            )
                        }
                    }
                    let mut document = documents
                        .get(&path)
                        .cloned()
                        .unwrap_or(Document { text: fs::read_to_string(&path)?, version: 0 });
                    let incremental_changes =
                        apply_text_edits(&mut document.text, &edits, encoding)?;
                    document.version += 1;
                    transaction.write(&path, &document.text, document.version)?;
                    if was_open {
                        let content_changes = if incremental {
                            incremental_changes
                        } else {
                            vec![json!({"text": document.text.clone()})]
                        };
                        notifications.push((
                            "textDocument/didChange".into(),
                            json!({
                                "textDocument": {
                                    "uri": uri,
                                    "version": document.version
                                },
                                "contentChanges": content_changes
                            }),
                        ));
                        documents.insert(path, document);
                    }
                    applied += edits.len();
                }
                WorkspaceEditOperation::Create { uri, overwrite, ignore_if_exists } => {
                    let path = workspace_path_for(fixture, &uri, "WorkspaceEdit create")?;
                    if path.exists() && !overwrite {
                        if ignore_if_exists {
                            continue;
                        } else {
                            bail!("WorkspaceEdit create target `{}` already exists", path.display())
                        }
                    }
                    transaction.write(&path, "", 0)?;
                    notifications.push((
                        "workspace/didCreateFiles".into(),
                        json!({"files": [{"uri": uri}]}),
                    ));
                    applied += 1;
                }
                WorkspaceEditOperation::Rename {
                    old_uri,
                    new_uri,
                    overwrite,
                    ignore_if_exists,
                } => {
                    let old_path = workspace_path_for(fixture, &old_uri, "WorkspaceEdit rename")?;
                    let new_path = workspace_path_for(fixture, &new_uri, "WorkspaceEdit rename")?;
                    if !old_path.exists() {
                        bail!("WorkspaceEdit rename source `{}` does not exist", old_path.display())
                    }
                    if new_path.exists() {
                        if !overwrite {
                            if ignore_if_exists {
                                continue;
                            } else {
                                bail!(
                                    "WorkspaceEdit rename target `{}` already exists",
                                    new_path.display()
                                )
                            }
                        }
                        if new_path.is_dir() {
                            bail!(
                                "WorkspaceEdit cannot overwrite directory `{}`",
                                new_path.display()
                            )
                        }
                    }
                    transaction.rename(&old_path, &new_path, overwrite)?;
                    if let Some(document) = documents.remove(&old_path) {
                        let version = document.version;
                        let text = document.text.clone();
                        notifications.push((
                            "textDocument/didClose".into(),
                            json!({"textDocument": {"uri": old_uri}}),
                        ));
                        notifications.push((
                            "textDocument/didOpen".into(),
                            json!({
                                "textDocument": {
                                    "uri": new_uri,
                                    "languageId": "solidity",
                                    "version": version,
                                    "text": text
                                }
                            }),
                        ));
                        documents.insert(new_path, document);
                    }
                    notifications.push((
                        "workspace/didRenameFiles".into(),
                        json!({"files": [{"oldUri": old_uri, "newUri": new_uri}]}),
                    ));
                    applied += 1;
                }
                WorkspaceEditOperation::Delete { uri, recursive, ignore_if_not_exists } => {
                    let path = workspace_path_for(fixture, &uri, "WorkspaceEdit delete")?;
                    if !path.exists() {
                        if ignore_if_not_exists {
                            continue;
                        }
                        bail!("WorkspaceEdit delete target `{}` does not exist", path.display())
                    }
                    if path.is_dir() && !recursive {
                        bail!("WorkspaceEdit delete target `{}` is a directory", path.display())
                    }
                    transaction.delete(&path)?;
                    if documents.remove(&path).is_some() {
                        notifications.push((
                            "textDocument/didClose".into(),
                            json!({"textDocument": {"uri": uri}}),
                        ));
                    }
                    notifications.push((
                        "workspace/didDeleteFiles".into(),
                        json!({"files": [{"uri": uri}]}),
                    ));
                    applied += 1;
                }
            }
        }
        Ok((applied, notifications))
    })();
    match result {
        Ok(result) => Ok(result),
        Err(error) => {
            *documents = original_documents;
            transaction
                .rollback()
                .with_context(|| format!("failed to roll back WorkspaceEdit after: {error:#}"))?;
            Err(error)
        }
    }
}

fn parse_workspace_edit(fixture: &Fixture, edit: &Value) -> Result<Vec<WorkspaceEditOperation>> {
    let mut operations = Vec::new();
    if let Some(document_changes) = edit.get("documentChanges") {
        let document_changes = document_changes
            .as_array()
            .context("WorkspaceEdit `documentChanges` must be an array")?;
        for change in document_changes {
            if let Some(uri) = change.pointer("/textDocument/uri").and_then(Value::as_str) {
                let uri = uri.parse::<Url>()?;
                workspace_path_for(fixture, &uri, "WorkspaceEdit")?;
                let version = match change
                    .pointer("/textDocument/version")
                    .context("WorkspaceEdit text document change is missing `version`")?
                {
                    Value::Null => None,
                    value => {
                        let version = value.as_i64().context(
                            "WorkspaceEdit text document version must be an integer or null",
                        )?;
                        Some(i32::try_from(version).context(
                            "WorkspaceEdit text document version is outside the supported range",
                        )?)
                    }
                };
                let edits = change
                    .get("edits")
                    .and_then(Value::as_array)
                    .context("WorkspaceEdit text document change is missing `edits`")?
                    .clone();
                operations.push(WorkspaceEditOperation::Text { uri, version, edits });
                continue;
            }
            let kind = change
                .get("kind")
                .and_then(Value::as_str)
                .context("WorkspaceEdit document change has no text edit or resource kind")?;
            let options = change.get("options").and_then(Value::as_object);
            let option = |name: &str, default: bool| -> Result<bool> {
                options.and_then(|options| options.get(name)).map_or(Ok(default), |value| {
                    value.as_bool().with_context(|| {
                        format!("WorkspaceEdit resource option `{name}` must be boolean")
                    })
                })
            };
            match kind {
                "create" => {
                    let uri = change
                        .get("uri")
                        .and_then(Value::as_str)
                        .context("WorkspaceEdit create URI is missing")?
                        .parse::<Url>()?;
                    workspace_path_for(fixture, &uri, "WorkspaceEdit create")?;
                    operations.push(WorkspaceEditOperation::Create {
                        uri,
                        overwrite: option("overwrite", false)?,
                        ignore_if_exists: option("ignoreIfExists", false)?,
                    });
                }
                "rename" => {
                    let old_uri = change
                        .get("oldUri")
                        .and_then(Value::as_str)
                        .context("WorkspaceEdit rename old URI is missing")?
                        .parse::<Url>()?;
                    let new_uri = change
                        .get("newUri")
                        .and_then(Value::as_str)
                        .context("WorkspaceEdit rename new URI is missing")?
                        .parse::<Url>()?;
                    workspace_path_for(fixture, &old_uri, "WorkspaceEdit rename")?;
                    workspace_path_for(fixture, &new_uri, "WorkspaceEdit rename")?;
                    operations.push(WorkspaceEditOperation::Rename {
                        old_uri,
                        new_uri,
                        overwrite: option("overwrite", false)?,
                        ignore_if_exists: option("ignoreIfExists", false)?,
                    });
                }
                "delete" => {
                    let uri = change
                        .get("uri")
                        .and_then(Value::as_str)
                        .context("WorkspaceEdit delete URI is missing")?
                        .parse::<Url>()?;
                    workspace_path_for(fixture, &uri, "WorkspaceEdit delete")?;
                    operations.push(WorkspaceEditOperation::Delete {
                        uri,
                        recursive: option("recursive", false)?,
                        ignore_if_not_exists: option("ignoreIfNotExists", false)?,
                    });
                }
                _ => bail!("unsupported WorkspaceEdit resource operation `{kind}"),
            }
        }
    } else if let Some(changes) = edit.get("changes") {
        let changes = changes.as_object().context("WorkspaceEdit `changes` must be an object")?;
        for (uri, edits) in changes {
            let uri = uri.parse::<Url>()?;
            workspace_path_for(fixture, &uri, "WorkspaceEdit")?;
            let edits =
                edits.as_array().context("WorkspaceEdit change list must be an array")?.clone();
            operations.push(WorkspaceEditOperation::Text { uri, version: None, edits });
        }
    } else {
        bail!("WorkspaceEdit has neither `changes` nor `documentChanges`")
    }
    Ok(operations)
}

fn workspace_path_for(fixture: &Fixture, uri: &Url, operation: &str) -> Result<PathBuf> {
    let path = uri.to_file_path().map_err(|()| anyhow!("{operation} URI `{uri}` is not a file"))?;
    validate_workspace_path(fixture.root(), &path, operation)?;
    Ok(path)
}

fn validate_workspace_path(root: &Path, path: &Path, operation: &str) -> Result<()> {
    let canonical_root = fs::canonicalize(root)?;
    let relative = path
        .strip_prefix(root)
        .map_err(|_| anyhow!("{operation} path `{}` escapes the fixture", path.display()))?;
    if relative.components().any(|component| matches!(component, std::path::Component::ParentDir)) {
        bail!("{operation} path `{}` escapes the fixture", path.display())
    }

    // A lexical prefix is insufficient when a server points a path through a symlink. Check the
    // existing path, or its nearest existing parent for a newly-created resource, after resolving
    // symlinks so a WorkspaceEdit cannot mutate files outside the temporary fixture.
    let existing = if path.exists() {
        Some(path)
    } else {
        path.ancestors().find(|ancestor| ancestor.exists())
    };
    if let Some(existing) = existing {
        let canonical = fs::canonicalize(existing)?;
        if !canonical.starts_with(&canonical_root) {
            bail!("{operation} path `{}` escapes the fixture", path.display())
        }
    }
    Ok(())
}

fn atomic_write(path: &Path, text: &str, version: i32) -> Result<()> {
    let temporary = path.with_extension(format!("lsp-bench-{version}.tmp"));
    fs::write(&temporary, text)
        .with_context(|| format!("failed to write temporary file `{}`", temporary.display()))?;
    fs::rename(&temporary, path)
        .with_context(|| format!("failed to atomically replace `{}`", path.display()))?;
    Ok(())
}

fn apply_text_edits(
    text: &mut String,
    edits: &[Value],
    encoding: PositionEncoding,
) -> Result<Vec<Value>> {
    let original = text.clone();
    let mut replacements = Vec::with_capacity(edits.len());
    for edit in edits {
        let range = serde_json::from_value::<Range>(
            edit.get("range").cloned().context("WorkspaceEdit text edit is missing a range")?,
        )?;
        let replacement = edit
            .get("newText")
            .and_then(Value::as_str)
            .context("WorkspaceEdit text edit is missing `newText`")?;
        let start = offset_at_position(&original, range.start, encoding)?;
        let end = offset_at_position(&original, range.end, encoding)?;
        if start > end {
            bail!("WorkspaceEdit range starts after it ends")
        }
        replacements.push((start, end, range, replacement));
    }
    replacements.sort_by_key(|(start, end, _, _)| (*start, *end));
    for pair in replacements.windows(2) {
        if pair[0].1 > pair[1].0 {
            bail!("WorkspaceEdit contains overlapping text edits")
        }
    }
    let content_changes = replacements
        .iter()
        .rev()
        .map(|(_, _, range, replacement)| json!({"range": range, "text": replacement}))
        .collect();
    for (start, end, _, replacement) in replacements.into_iter().rev() {
        text.replace_range(start..end, replacement);
    }
    Ok(content_changes)
}

fn harness_error(error: anyhow::Error) -> WorkloadError {
    WorkloadError::new(FailureKind::HarnessError, format!("{error:#}"))
}

fn classify_request_error(error: anyhow::Error) -> WorkloadError {
    if let Some(remote) = error.downcast_ref::<RemoteError>() {
        let kind = match remote.code {
            Some(-32601) => FailureKind::Unsupported,
            Some(-32602) => FailureKind::HarnessError,
            _ => FailureKind::HarnessError,
        };
        return WorkloadError::new(kind, format!("{remote}"));
    }
    let message = format!("{error:#}");
    let kind = if message.contains("timed out") {
        FailureKind::Timeout
    } else if message.contains("LSP stdout closed unexpectedly") {
        FailureKind::Crashed
    } else {
        FailureKind::HarnessError
    };
    WorkloadError::new(kind, message)
}

fn validate_definition(
    value: Value,
    expected_uri: &Url,
    expected: &Anchor,
) -> std::result::Result<(), WorkloadError> {
    let locations = match serde_json::from_value::<GotoDefinitionResponse>(value.clone()) {
        Ok(GotoDefinitionResponse::Scalar(location)) => vec![location],
        Ok(GotoDefinitionResponse::Array(locations)) => locations,
        Ok(GotoDefinitionResponse::Link(links)) => links
            .into_iter()
            .map(|link| Location { uri: link.target_uri, range: link.target_selection_range })
            .collect(),
        Err(_) => {
            if let Some(array) = value.as_array() {
                array.iter().filter_map(location_from_value).collect()
            } else {
                Vec::new()
            }
        }
    };
    let matched = locations.iter().any(|location| {
        location.uri == *expected_uri
            && location.range.start <= expected.position
            && expected.position <= location.range.end
    });
    if matched {
        Ok(())
    } else {
        Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!(
                "definition did not target {} at {:?}: {value}",
                expected_uri, expected.position
            ),
        ))
    }
}

fn location_from_value(value: &Value) -> Option<Location> {
    if value.get("uri").is_some() {
        serde_json::from_value(value.clone()).ok()
    } else {
        let uri = value.get("targetUri")?.as_str()?.parse().ok()?;
        let range = serde_json::from_value(value.get("targetRange")?.clone()).ok()?;
        Some(Location { uri, range })
    }
}

fn validate_completion(
    value: Value,
    expected_label: &str,
) -> std::result::Result<(), WorkloadError> {
    let response = serde_json::from_value::<CompletionResponse>(value.clone()).ok();
    let labels = match response {
        Some(CompletionResponse::Array(items)) => {
            items.into_iter().map(|item| item.label).collect::<Vec<_>>()
        }
        Some(CompletionResponse::List(list)) => {
            list.items.into_iter().map(|item| item.label).collect::<Vec<_>>()
        }
        None => Vec::new(),
    };
    if labels.iter().any(|label| label == expected_label) {
        Ok(())
    } else {
        Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!("completion did not contain `{expected_label}`: {value}"),
        ))
    }
}

fn validate_hover(value: Value, expected_text: &str) -> std::result::Result<(), WorkloadError> {
    let Some(contents) = value.get("contents") else {
        return Err(WorkloadError::new(FailureKind::Incorrect, "hover returned null"));
    };
    if contents.to_string().contains(expected_text) {
        Ok(())
    } else {
        Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!("hover did not contain `{expected_text}`: {contents}"),
        ))
    }
}

fn validate_references(
    value: Value,
    min_count: usize,
    expected: &[(Url, Anchor)],
) -> std::result::Result<(), WorkloadError> {
    let Some(items) = value.as_array() else {
        return Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!("references returned a non-array result: {value}"),
        ));
    };
    let locations = items
        .iter()
        .map(|item| {
            serde_json::from_value::<Location>(item.clone()).map_err(|_| {
                WorkloadError::new(
                    FailureKind::Incorrect,
                    format!("references returned an invalid location: {item}"),
                )
            })
        })
        .collect::<std::result::Result<Vec<_>, _>>()?;
    let unique = locations
        .iter()
        .map(|location| format!("{}:{:?}", location.uri, location.range))
        .collect::<BTreeSet<_>>();
    if unique.len() < min_count {
        return Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!(
                "references returned {} unique locations; expected at least {min_count}: {value}",
                unique.len()
            ),
        ));
    }
    for (expected_uri, expected_anchor) in expected {
        if !locations.iter().any(|location| {
            location.uri == *expected_uri
                && location.range.start <= expected_anchor.position
                && expected_anchor.position <= location.range.end
        }) {
            return Err(WorkloadError::new(
                FailureKind::Incorrect,
                format!(
                    "references did not contain {expected_uri} at {:?}: {value}",
                    expected_anchor.position
                ),
            ));
        }
    }
    Ok(())
}

fn validate_rename_operations(
    operations: &[WorkspaceEditOperation],
    new_name: &str,
    expected: &[(Url, Range)],
) -> std::result::Result<(), WorkloadError> {
    for (expected_uri, expected_range) in expected {
        let matched = operations.iter().any(|operation| {
            let WorkspaceEditOperation::Text { uri, edits, .. } = operation else { return false };
            uri == expected_uri
                && edits.iter().any(|edit| {
                    edit.get("newText").and_then(Value::as_str) == Some(new_name)
                        && edit
                            .get("range")
                            .cloned()
                            .and_then(|range| serde_json::from_value::<Range>(range).ok())
                            .is_some_and(|range| range == *expected_range)
                })
        });
        if !matched {
            return Err(WorkloadError::new(
                FailureKind::Incorrect,
                format!(
                    "rename WorkspaceEdit did not change `{new_name}` at {expected_uri} {expected_range:?}"
                ),
            ));
        }
    }
    Ok(())
}

fn solidity_identifier_range(
    text: &str,
    position: lsp_types::Position,
    encoding: PositionEncoding,
) -> Result<Range> {
    let offset = offset_at_position(text, position, encoding)?;
    let bytes = text.as_bytes();
    if bytes.get(offset).is_none_or(|byte| !is_solidity_identifier_continue(*byte)) {
        bail!("rename expected anchor does not point at a Solidity identifier")
    }

    let mut start = offset;
    while start > 0 && is_solidity_identifier_continue(bytes[start - 1]) {
        start -= 1;
    }
    if !is_solidity_identifier_start(bytes[start]) {
        bail!("rename expected anchor does not point at a valid Solidity identifier")
    }
    let mut end = offset + 1;
    while end < bytes.len() && is_solidity_identifier_continue(bytes[end]) {
        end += 1;
    }
    Ok(Range {
        start: position_at_with_encoding(text, start, encoding),
        end: position_at_with_encoding(text, end, encoding),
    })
}

const fn is_solidity_identifier_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_' || byte == b'$'
}

const fn is_solidity_identifier_continue(byte: u8) -> bool {
    is_solidity_identifier_start(byte) || byte.is_ascii_digit()
}

fn validate_document_symbols(
    value: Value,
    min_count: usize,
    expected_name: Option<&str>,
) -> std::result::Result<(), WorkloadError> {
    if value.is_null() {
        return Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!("document symbols returned 0 items; expected at least {min_count}: {value}"),
        ));
    }
    let response =
        serde_json::from_value::<DocumentSymbolResponse>(value.clone()).map_err(|_| {
            WorkloadError::new(
                FailureKind::Incorrect,
                format!("document symbols returned an invalid result: {value}"),
            )
        })?;
    let count = document_symbol_count(&response);
    if count < min_count {
        return Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!(
                "document symbols returned {count} items; expected at least {min_count}: {value}"
            ),
        ));
    }
    if let Some(expected_name) = expected_name
        && !contains_symbol_name(&response, expected_name)
    {
        return Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!("document symbols did not contain `{expected_name}`: {value}"),
        ));
    }
    Ok(())
}

fn document_symbol_count(response: &DocumentSymbolResponse) -> usize {
    match response {
        DocumentSymbolResponse::Flat(symbols) => symbols.len(),
        DocumentSymbolResponse::Nested(symbols) => symbols.iter().map(nested_symbol_count).sum(),
    }
}

fn nested_symbol_count(symbol: &DocumentSymbol) -> usize {
    1 + symbol
        .children
        .as_deref()
        .map_or(0, |children| children.iter().map(nested_symbol_count).sum())
}

fn contains_symbol_name(response: &DocumentSymbolResponse, expected: &str) -> bool {
    match response {
        DocumentSymbolResponse::Flat(symbols) => {
            symbols.iter().any(|symbol| symbol.name == expected)
        }
        DocumentSymbolResponse::Nested(symbols) => {
            symbols.iter().any(|symbol| nested_symbol_has_name(symbol, expected))
        }
    }
}

fn nested_symbol_has_name(symbol: &DocumentSymbol, expected: &str) -> bool {
    symbol.name == expected
        || symbol.children.as_deref().is_some_and(|children| {
            children.iter().any(|child| nested_symbol_has_name(child, expected))
        })
}

fn validate_workspace_symbol(
    value: Value,
    expected_name: &str,
    expected_uri: &Url,
    present: bool,
) -> std::result::Result<(), WorkloadError> {
    let matched = value.as_array().is_some_and(|items| {
        items.iter().any(|item| {
            item.get("name").and_then(Value::as_str) == Some(expected_name)
                && item.pointer("/location/uri").and_then(Value::as_str)
                    == Some(expected_uri.as_str())
        })
    });
    if matched == present {
        Ok(())
    } else {
        Err(WorkloadError::new(
            FailureKind::Incorrect,
            format!(
                "workspace symbols {} `{expected_name}` at {expected_uri}: {value}",
                if present { "did not contain" } else { "still contained" }
            ),
        ))
    }
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use lsp_types::Position;
    use serde_json::json;
    #[cfg(unix)]
    use std::os::unix::fs::{PermissionsExt, symlink};

    #[test]
    fn definition_validator_requires_uri_and_target_range() {
        let expected_uri = Url::parse("file:///tmp/Math.sol").unwrap();
        let expected = Anchor {
            path: PathBuf::from("Math.sol"),
            position: Position { line: 2, character: 10 },
        };
        let value = json!({"uri":"file:///tmp/Math.sol", "range":{"start":{"line":2,"character":5},"end":{"line":2,"character":16}}});
        assert!(validate_definition(value, &expected_uri, &expected).is_ok());
    }

    #[test]
    fn reference_and_document_symbol_predicates_reject_incorrect_results() {
        let expected_uri = Url::parse("file:///tmp/Math.sol").unwrap();
        let expected = vec![(
            expected_uri.clone(),
            Anchor { path: "Math.sol".into(), position: Position { line: 2, character: 10 } },
        )];
        let wrong = json!([
            {"uri":"file:///tmp/Wrong.sol","range":{"start":{"line":2,"character":5},"end":{"line":2,"character":16}}},
            {"uri":"file:///tmp/Wrong.sol","range":{"start":{"line":2,"character":5},"end":{"line":2,"character":16}}}
        ]);
        assert!(validate_references(wrong, 2, &expected).is_err());
        let correct = json!([
            {"uri":expected_uri,"range":{"start":{"line":2,"character":5},"end":{"line":2,"character":16}}},
            {"uri":"file:///tmp/Use.sol","range":{"start":{"line":1,"character":0},"end":{"line":1,"character":4}}}
        ]);
        assert!(validate_references(correct, 2, &expected).is_ok());
        let range = json!({
            "start": {"line": 0, "character": 0},
            "end": {"line": 0, "character": 1}
        });
        let symbol = |name: &str, children: Vec<Value>| {
            json!({
                "name": name,
                "kind": 5,
                "range": range,
                "selectionRange": range,
                "children": children
            })
        };
        let symbols = json!([symbol(
            "Main",
            vec![
                symbol("stored", vec![]),
                symbol("calculate", vec![symbol("input", vec![])]),
                symbol("status", vec![]),
            ]
        )]);
        assert!(validate_document_symbols(symbols.clone(), 5, Some("input")).is_ok());
        assert!(validate_document_symbols(symbols.clone(), 6, None).is_err());
        assert!(validate_document_symbols(symbols, 1, Some("missing")).is_err());
        assert!(validate_document_symbols(json!([{}]), 1, None).is_err());
    }

    #[test]
    fn workspace_text_edits_apply_in_reverse_and_use_negotiated_encoding() {
        let mut text = "a😀bc".to_owned();
        let edits = json!([
            {"range":{"start":{"line":0,"character":1},"end":{"line":0,"character":5}},"newText":"X"},
            {"range":{"start":{"line":0,"character":6},"end":{"line":0,"character":7}},"newText":"Y"}
        ]);
        apply_text_edits(&mut text, edits.as_array().unwrap(), PositionEncoding::Utf8).unwrap();
        assert_eq!(text, "aXbY");
    }

    #[test]
    fn rename_edit_validator_requires_every_expected_anchor() {
        let main_uri = Url::parse("file:///tmp/Main.sol").unwrap();
        let math_uri = Url::parse("file:///tmp/Math.sol").unwrap();
        let expected = vec![
            (
                main_uri.clone(),
                Range {
                    start: Position { line: 0, character: 12 },
                    end: Position { line: 0, character: 18 },
                },
            ),
            (
                math_uri.clone(),
                Range {
                    start: Position { line: 0, character: 9 },
                    end: Position { line: 0, character: 15 },
                },
            ),
        ];
        let edit = |uri: Url, start| WorkspaceEditOperation::Text {
            uri,
            version: None,
            edits: vec![json!({
                "range": {
                    "start": {"line": 0, "character": start},
                    "end": {"line": 0, "character": start + 6}
                },
                "newText": "renamed"
            })],
        };
        let incomplete = vec![edit(main_uri.clone(), 12)];
        assert!(validate_rename_operations(&incomplete, "renamed", &expected).is_err());
        let insertion = vec![WorkspaceEditOperation::Text {
            uri: main_uri.clone(),
            version: None,
            edits: vec![json!({
                "range": {
                    "start": {"line": 0, "character": 12},
                    "end": {"line": 0, "character": 12}
                },
                "newText": "renamed"
            })],
        }];
        assert!(validate_rename_operations(&insertion, "renamed", &expected[..1]).is_err());
        let oversized = vec![WorkspaceEditOperation::Text {
            uri: main_uri.clone(),
            version: None,
            edits: vec![json!({
                "range": {
                    "start": {"line": 0, "character": 0},
                    "end": {"line": 0, "character": 18}
                },
                "newText": "renamed"
            })],
        }];
        assert!(validate_rename_operations(&oversized, "renamed", &expected[..1]).is_err());
        let complete = vec![edit(main_uri, 12), edit(math_uri, 9)];
        assert!(validate_rename_operations(&complete, "renamed", &expected).is_ok());
    }

    #[test]
    fn workspace_edit_paths_cannot_escape_fixture() {
        let fixture = tempfile::tempdir().unwrap();
        #[cfg(unix)]
        let outside = tempfile::tempdir().unwrap();
        let inside = fixture.path().join("Main.sol");
        fs::write(&inside, "contract Main {}\n").unwrap();

        assert!(validate_workspace_path(fixture.path(), &inside, "edit").is_ok());
        assert!(
            validate_workspace_path(
                fixture.path(),
                &fixture.path().join("..").join("outside.sol"),
                "edit"
            )
            .is_err()
        );

        #[cfg(unix)]
        {
            let link = fixture.path().join("link");
            symlink(outside.path(), &link).unwrap();
            assert!(
                validate_workspace_path(fixture.path(), &link.join("escape.sol"), "edit").is_err()
            );
        }
    }

    #[test]
    fn failed_workspace_edit_does_not_leave_partial_mutations() {
        let source_root = tempfile::tempdir().unwrap();
        fs::write(source_root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        let spec = crate::config::FixtureSpec {
            id: "fixture".into(),
            root: source_root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let fixture = FixtureSource::open(&spec).unwrap().materialize().unwrap();
        let main = fixture.path(Path::new("Main.sol")).unwrap();
        let main_uri = file_uri(&main).unwrap();
        let missing_uri = file_uri(&fixture.path(Path::new("Missing.sol")).unwrap()).unwrap();
        let mut documents = BTreeMap::new();
        documents.insert(main.clone(), Document { text: "contract Main {}\n".into(), version: 1 });
        let edit = json!({
            "documentChanges": [
                {
                    "textDocument": {"uri": main_uri, "version": 1},
                    "edits": [{
                        "range": {
                            "start": {"line": 0, "character": 9},
                            "end": {"line": 0, "character": 13}
                        },
                        "newText": "Changed"
                    }]
                },
                {"kind": "delete", "uri": missing_uri}
            ]
        });

        assert!(
            apply_workspace_edit_to(&fixture, &mut documents, PositionEncoding::Utf8, true, &edit,)
                .is_err()
        );
        assert_eq!(fs::read_to_string(&main).unwrap(), "contract Main {}\n");
        assert_eq!(documents[&main].text, "contract Main {}\n");
        assert_eq!(documents[&main].version, 1);
    }

    #[test]
    fn versioned_workspace_edit_rejects_a_stale_open_document() {
        let source_root = tempfile::tempdir().unwrap();
        fs::write(source_root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        let spec = crate::config::FixtureSpec {
            id: "fixture".into(),
            root: source_root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let fixture = FixtureSource::open(&spec).unwrap().materialize().unwrap();
        let main = fixture.path(Path::new("Main.sol")).unwrap();
        let main_uri = file_uri(&main).unwrap();
        let mut documents = BTreeMap::new();
        documents.insert(main.clone(), Document { text: "contract Main {}\n".into(), version: 2 });
        let edit = json!({
            "documentChanges": [{
                "textDocument": {"uri": main_uri, "version": 1},
                "edits": [{
                    "range": {
                        "start": {"line": 0, "character": 9},
                        "end": {"line": 0, "character": 13}
                    },
                    "newText": "Changed"
                }]
            }]
        });

        assert!(
            apply_workspace_edit_to(&fixture, &mut documents, PositionEncoding::Utf8, true, &edit,)
                .is_err()
        );
        assert_eq!(fs::read_to_string(&main).unwrap(), "contract Main {}\n");
        assert_eq!(documents[&main].text, "contract Main {}\n");
        assert_eq!(documents[&main].version, 2);
    }

    #[test]
    fn incremental_workspace_edit_notifications_reproduce_the_applied_text() {
        let source_root = tempfile::tempdir().unwrap();
        let original = "alpha beta\n";
        fs::write(source_root.path().join("Main.sol"), original).unwrap();
        let spec = crate::config::FixtureSpec {
            id: "fixture".into(),
            root: source_root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let fixture = FixtureSource::open(&spec).unwrap().materialize().unwrap();
        let main = fixture.path(Path::new("Main.sol")).unwrap();
        let main_uri = file_uri(&main).unwrap();
        let mut documents = BTreeMap::new();
        documents.insert(main.clone(), Document { text: original.into(), version: 1 });
        let edit = json!({
            "documentChanges": [{
                "textDocument": {"uri": main_uri, "version": 1},
                "edits": [
                    {
                        "range": {
                            "start": {"line": 0, "character": 0},
                            "end": {"line": 0, "character": 5}
                        },
                        "newText": "alphabet"
                    },
                    {
                        "range": {
                            "start": {"line": 0, "character": 6},
                            "end": {"line": 0, "character": 10}
                        },
                        "newText": "B"
                    }
                ]
            }]
        });

        let (_, notifications) =
            apply_workspace_edit_to(&fixture, &mut documents, PositionEncoding::Utf8, true, &edit)
                .unwrap();
        let content_changes = notifications
            .iter()
            .find_map(|(method, params)| {
                (method == "textDocument/didChange")
                    .then(|| params["contentChanges"].as_array().unwrap())
            })
            .unwrap();
        let mut synchronized = original.to_owned();
        for change in content_changes {
            let range = serde_json::from_value::<Range>(change["range"].clone()).unwrap();
            let start =
                offset_at_position(&synchronized, range.start, PositionEncoding::Utf8).unwrap();
            let end = offset_at_position(&synchronized, range.end, PositionEncoding::Utf8).unwrap();
            synchronized.replace_range(start..end, change["text"].as_str().unwrap());
        }

        assert_eq!(fs::read_to_string(&main).unwrap(), "alphabet B\n");
        assert_eq!(documents[&main].text, "alphabet B\n");
        assert_eq!(synchronized, documents[&main].text);
    }

    #[test]
    fn document_changes_apply_in_declared_order() {
        let source_root = tempfile::tempdir().unwrap();
        fs::write(source_root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        let spec = crate::config::FixtureSpec {
            id: "fixture".into(),
            root: source_root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let fixture = FixtureSource::open(&spec).unwrap().materialize().unwrap();
        let created = fixture.path(Path::new("Created.sol")).unwrap();
        let renamed = fixture.path(Path::new("Renamed.sol")).unwrap();
        let created_uri = file_uri(&created).unwrap();
        let renamed_uri = file_uri(&renamed).unwrap();
        let edit = json!({
            "documentChanges": [
                {"kind": "create", "uri": created_uri},
                {
                    "textDocument": {"uri": created_uri, "version": null},
                    "edits": [{
                        "range": {
                            "start": {"line": 0, "character": 0},
                            "end": {"line": 0, "character": 0}
                        },
                        "newText": "contract Created {}\n"
                    }]
                },
                {"kind": "rename", "oldUri": created_uri, "newUri": renamed_uri},
                {
                    "textDocument": {"uri": renamed_uri, "version": null},
                    "edits": [{
                        "range": {
                            "start": {"line": 0, "character": 9},
                            "end": {"line": 0, "character": 16}
                        },
                        "newText": "Renamed"
                    }]
                }
            ]
        });

        let (applied, notifications) = apply_workspace_edit_to(
            &fixture,
            &mut BTreeMap::new(),
            PositionEncoding::Utf8,
            true,
            &edit,
        )
        .unwrap();
        assert_eq!(applied, 4);
        assert!(!created.exists());
        assert_eq!(fs::read_to_string(renamed).unwrap(), "contract Renamed {}\n");
        assert_eq!(
            notifications.iter().map(|(method, _)| method.as_str()).collect::<Vec<_>>(),
            ["workspace/didCreateFiles", "workspace/didRenameFiles"]
        );
    }

    #[test]
    fn document_changes_take_precedence_over_plain_changes() {
        let source_root = tempfile::tempdir().unwrap();
        fs::write(source_root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        let spec = crate::config::FixtureSpec {
            id: "fixture".into(),
            root: source_root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let fixture = FixtureSource::open(&spec).unwrap().materialize().unwrap();
        let main = fixture.path(Path::new("Main.sol")).unwrap();
        let main_uri = file_uri(&main).unwrap();
        let missing_uri = file_uri(&fixture.path(Path::new("Missing.sol")).unwrap()).unwrap();
        let edit = json!({
            "changes": {
                missing_uri: [{
                    "range": {
                        "start": {"line": 0, "character": 0},
                        "end": {"line": 0, "character": 0}
                    },
                    "newText": "ignored"
                }]
            },
            "documentChanges": [{
                "textDocument": {"uri": main_uri, "version": null},
                "edits": [{
                    "range": {
                        "start": {"line": 0, "character": 9},
                        "end": {"line": 0, "character": 13}
                    },
                    "newText": "Updated"
                }]
            }]
        });

        let (applied, _) = apply_workspace_edit_to(
            &fixture,
            &mut BTreeMap::new(),
            PositionEncoding::Utf8,
            true,
            &edit,
        )
        .unwrap();
        assert_eq!(applied, 1);
        assert_eq!(fs::read_to_string(main).unwrap(), "contract Updated {}\n");
    }

    #[test]
    fn rename_workspace_edit_does_not_ignore_a_missing_source() {
        let source_root = tempfile::tempdir().unwrap();
        fs::write(source_root.path().join("Main.sol"), "contract Main {}\n").unwrap();
        let spec = crate::config::FixtureSpec {
            id: "fixture".into(),
            root: source_root.path().into(),
            revision: None,
            enabled: true,
            source_roots: vec![".".into()],
            anchors: BTreeMap::new(),
            required: false,
            corpus: None,
            solc: None,
            foundry: None,
            dependencies: BTreeMap::new(),
            source: None,
        };
        let fixture = FixtureSource::open(&spec).unwrap().materialize().unwrap();
        let missing_uri = file_uri(&fixture.path(Path::new("Missing.sol")).unwrap()).unwrap();
        let target_uri = file_uri(&fixture.path(Path::new("Target.sol")).unwrap()).unwrap();
        let edit = json!({
            "documentChanges": [{
                "kind": "rename",
                "oldUri": missing_uri,
                "newUri": target_uri,
                "options": {"ignoreIfExists": true}
            }]
        });

        assert!(
            apply_workspace_edit_to(
                &fixture,
                &mut BTreeMap::new(),
                PositionEncoding::Utf8,
                true,
                &edit,
            )
            .is_err()
        );
    }

    #[test]
    fn closed_server_transport_is_a_crash() {
        let error = classify_request_error(anyhow!("LSP stdout closed unexpectedly"));
        assert!(matches!(error.kind, FailureKind::Crashed));
    }

    #[test]
    fn cold_and_warm_workloads_use_cold_process_repetitions() {
        let profile = ProfileSpec {
            warmup: 1,
            samples: 2,
            cold_samples: 3,
            lifecycle_samples: 4,
            timeout_ms: 1_000,
            readiness_quiet_ms: 10,
            network_isolation: false,
            require_authoritative: false,
            scenarios: Vec::new(),
        };
        let workload = |steps| WorkloadSpec {
            id: "workload".into(),
            fixture: "fixture".into(),
            methods: Vec::new(),
            steps,
        };
        let symbols = || ProbeSpec::DocumentSymbol {
            path: "Main.sol".into(),
            min_count: 1,
            expected_name: None,
        };

        let cold = workload(vec![StepSpec::Probe { name: "cold-ready".into(), probe: symbols() }]);
        let warm = workload(vec![StepSpec::Warm {
            name: "symbols".into(),
            probe: symbols(),
            warmup: None,
            samples: None,
        }]);
        let lifecycle = workload(vec![StepSpec::Open { path: "Main.sol".into() }]);

        assert_eq!(profile.repetitions_for(&cold), 3);
        assert_eq!(profile.repetitions_for(&warm), 3);
        assert_eq!(profile.repetitions_for(&lifecycle), 4);
    }

    #[cfg(unix)]
    #[test]
    fn mismatched_declared_server_artifacts_are_not_run() {
        let directory = tempfile::tempdir().unwrap();
        let command = directory.path().join("server");
        fs::write(&command, "#!/bin/sh\nprintf 'server 1.0\\n'\n").unwrap();
        fs::set_permissions(&command, fs::Permissions::from_mode(0o755)).unwrap();
        let spec = ServerSpec {
            id: "server".into(),
            command,
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
            artifact: Some(crate::config::ArtifactSpec {
                path: directory.path().join("server"),
                sha256: Some("0".repeat(64)),
            }),
            required: false,
        };

        let prepared = prepare_server(&spec).unwrap();

        assert_eq!(prepared.metadata.status, ServerStatus::Incompatible);
        assert!(prepared.metadata.error.unwrap().contains("artifact digest"));
    }
}

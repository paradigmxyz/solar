//! Reproducible benchmark configuration.

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

pub(crate) const SCHEMA_VERSION: u32 = 1;

#[derive(Clone, Debug, Deserialize)]
pub(crate) struct Config {
    pub(crate) schema_version: u32,
    #[serde(skip)]
    pub(crate) config_sha256: String,
    #[serde(default)]
    pub(crate) profiles: BTreeMap<String, ProfileSpec>,
    #[serde(default)]
    pub(crate) scenarios: Vec<ScenarioSpec>,
    #[serde(skip)]
    pub(crate) servers_lock_sha256: Option<String>,
    #[serde(skip)]
    pub(crate) fixtures_lock_sha256: Option<String>,
    pub(crate) servers: Vec<ServerSpec>,
    pub(crate) fixtures: Vec<FixtureSpec>,
    pub(crate) workloads: Vec<WorkloadSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ProfileSpec {
    #[serde(default = "default_warmup")]
    pub(crate) warmup: usize,
    #[serde(default = "default_samples")]
    pub(crate) samples: usize,
    #[serde(default = "default_cold_samples")]
    pub(crate) cold_samples: usize,
    #[serde(default = "default_lifecycle_samples")]
    pub(crate) lifecycle_samples: usize,
    #[serde(default = "default_timeout_ms")]
    pub(crate) timeout_ms: u64,
    #[serde(default = "default_readiness_quiet_ms")]
    pub(crate) readiness_quiet_ms: u64,
    #[serde(default)]
    pub(crate) network_isolation: bool,
    #[serde(default)]
    pub(crate) require_authoritative: bool,
    #[serde(default)]
    pub(crate) scenarios: Vec<String>,
}

impl ProfileSpec {
    pub(crate) fn repetitions_for(&self, workload: &WorkloadSpec) -> usize {
        if workload.steps.iter().any(|step| match step {
            StepSpec::Warm { .. } => true,
            StepSpec::Probe { name, .. } => name == "cold-ready",
            _ => false,
        }) {
            self.cold_samples
        } else {
            self.lifecycle_samples
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ScenarioSpec {
    pub(crate) id: String,
    pub(crate) fixture: String,
    #[serde(default)]
    pub(crate) profile: Option<String>,
    #[serde(default)]
    pub(crate) steps: Vec<StepSpec>,
    #[serde(default)]
    pub(crate) methods: Vec<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ServerSpec {
    pub(crate) id: String,
    pub(crate) command: PathBuf,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) transport: TransportSpec,
    #[serde(default = "default_version_args")]
    pub(crate) version_args: Vec<String>,
    #[serde(default)]
    pub(crate) locked_version: Option<String>,
    #[serde(default)]
    pub(crate) expected_version: Option<String>,
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) env: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) initialization_options: Value,
    #[serde(default)]
    pub(crate) configuration: Value,
    #[serde(default)]
    pub(crate) label: Option<String>,
    #[serde(default)]
    pub(crate) source: Option<SourceSpec>,
    #[serde(default)]
    pub(crate) install: Option<InstallSpec>,
    #[serde(default)]
    pub(crate) artifact: Option<ArtifactSpec>,
    #[serde(default)]
    pub(crate) required: bool,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
#[serde(tag = "kind", rename_all = "kebab-case", deny_unknown_fields)]
pub(crate) enum TransportSpec {
    #[default]
    Stdio,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceSpec {
    pub(crate) url: String,
    pub(crate) revision: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct InstallSpec {
    pub(crate) kind: String,
    #[serde(default)]
    pub(crate) url: Option<String>,
    #[serde(default)]
    pub(crate) command: Option<String>,
    #[serde(default)]
    pub(crate) args: Vec<String>,
    #[serde(default)]
    pub(crate) manifest: Option<PathBuf>,
    #[serde(default)]
    pub(crate) manifest_sha256: Option<String>,
    #[serde(default)]
    pub(crate) python_version: Option<String>,
    #[serde(default)]
    pub(crate) target: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ArtifactSpec {
    pub(crate) path: PathBuf,
    #[serde(default)]
    pub(crate) sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct FixtureSpec {
    pub(crate) id: String,
    pub(crate) root: PathBuf,
    #[serde(default = "default_lsp_root")]
    pub(crate) lsp_root: PathBuf,
    #[serde(default)]
    pub(crate) revision: Option<String>,
    #[serde(default = "default_enabled")]
    pub(crate) enabled: bool,
    #[serde(default)]
    pub(crate) source_roots: Vec<PathBuf>,
    #[serde(default)]
    pub(crate) anchors: BTreeMap<String, AnchorSpec>,
    #[serde(default)]
    pub(crate) required: bool,
    #[serde(default)]
    pub(crate) corpus: Option<String>,
    #[serde(default)]
    pub(crate) solc: Option<CompilerSpec>,
    #[serde(default)]
    pub(crate) foundry: Option<CompilerSpec>,
    #[serde(default)]
    pub(crate) dependencies: BTreeMap<String, String>,
    #[serde(default)]
    pub(crate) source: Option<SourceSpec>,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CompilerSpec {
    pub(crate) version: String,
    #[serde(default)]
    pub(crate) native: Option<PathBuf>,
    #[serde(default)]
    pub(crate) soljson: Option<PathBuf>,
    #[serde(default)]
    pub(crate) native_url: Option<String>,
    #[serde(default)]
    pub(crate) native_sha256: Option<String>,
    #[serde(default)]
    pub(crate) soljson_url: Option<String>,
    #[serde(default)]
    pub(crate) soljson_sha256: Option<String>,
    #[serde(default)]
    pub(crate) archive_url: Option<String>,
    #[serde(default)]
    pub(crate) archive_sha256: Option<String>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct AnchorSpec {
    pub(crate) path: PathBuf,
    pub(crate) needle: String,
    #[serde(default)]
    pub(crate) offset: usize,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ExpectedLocationSpec {
    pub(crate) path: PathBuf,
    pub(crate) anchor: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct WorkloadSpec {
    pub(crate) id: String,
    pub(crate) fixture: String,
    #[serde(default)]
    pub(crate) methods: Vec<String>,
    pub(crate) steps: Vec<StepSpec>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct DiskReplacementSpec {
    pub(crate) path: PathBuf,
    pub(crate) anchor: String,
    pub(crate) text: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(crate) enum StepSpec {
    Open {
        path: PathBuf,
    },
    Probe {
        name: String,
        probe: ProbeSpec,
    },
    Replace {
        path: PathBuf,
        anchor: String,
        text: String,
        #[serde(default)]
        probe: Option<ProbeSpec>,
    },
    Save {
        path: PathBuf,
        #[serde(default)]
        probe: Option<ProbeSpec>,
    },
    Warm {
        name: String,
        probe: ProbeSpec,
        #[serde(default)]
        warmup: Option<usize>,
        #[serde(default)]
        samples: Option<usize>,
    },
    Rename {
        path: PathBuf,
        anchor: String,
        new_name: String,
        expected_edits: Vec<ExpectedLocationSpec>,
        #[serde(default)]
        probe: Option<ProbeSpec>,
    },
    CreateFile {
        path: PathBuf,
        text: String,
        #[serde(default)]
        probe: Option<ProbeSpec>,
    },
    RenameFile {
        from: PathBuf,
        to: PathBuf,
        #[serde(default)]
        probe: Option<ProbeSpec>,
    },
    DeleteFile {
        path: PathBuf,
        #[serde(default)]
        probe: Option<ProbeSpec>,
    },
    Restart {
        #[serde(default)]
        invalidate: Option<DiskReplacementSpec>,
    },
}

#[derive(Clone, Debug, Deserialize)]
#[serde(tag = "kind", rename_all = "kebab-case")]
#[serde(deny_unknown_fields)]
pub(crate) enum ProbeSpec {
    Definition {
        path: PathBuf,
        anchor: String,
        expected_path: PathBuf,
        expected_anchor: String,
    },
    Completion {
        path: PathBuf,
        anchor: String,
        expected_label: String,
    },
    Hover {
        path: PathBuf,
        anchor: String,
        expected_text: String,
    },
    References {
        path: PathBuf,
        anchor: String,
        #[serde(default = "default_min_count")]
        min_count: usize,
        expected_locations: Vec<ExpectedLocationSpec>,
    },
    DocumentSymbol {
        path: PathBuf,
        #[serde(default = "default_min_count")]
        min_count: usize,
        #[serde(default)]
        expected_name: Option<String>,
    },
    WorkspaceSymbol {
        query: String,
        expected_name: String,
        expected_path: PathBuf,
        #[serde(default = "default_enabled")]
        present: bool,
    },
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct BenchmarkDocument {
    #[serde(alias = "schema_version")]
    version: u32,
    #[serde(default)]
    profiles: BTreeMap<String, ProfileSpec>,
    #[serde(default)]
    scenarios: Vec<ScenarioSpec>,
    #[serde(default)]
    servers_lock: Option<PathBuf>,
    #[serde(default)]
    fixtures_lock: Option<PathBuf>,
    #[serde(default)]
    servers: Vec<ServerSpec>,
    #[serde(default)]
    fixtures: Vec<FixtureSpec>,
    #[serde(default)]
    workloads: Vec<WorkloadSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ServersLock {
    version: u32,
    servers: Vec<ServerSpec>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct FixturesLock {
    version: u32,
    fixtures: Vec<FixtureSpec>,
}

impl Config {
    pub(crate) fn load(path: &Path) -> Result<Self> {
        let bytes = fs::read(path)
            .with_context(|| format!("failed to read benchmark config `{}`", path.display()))?;
        let config_sha256 = sha256_bytes(&bytes);
        let document = serde_yaml::from_slice::<BenchmarkDocument>(&bytes)
            .with_context(|| format!("failed to parse benchmark config `{}`", path.display()))?;
        if document.version != SCHEMA_VERSION {
            bail!(
                "unsupported benchmark config schema {}; expected {}",
                document.version,
                SCHEMA_VERSION
            )
        }
        let manifest_dir = path.parent().unwrap_or_else(|| Path::new("."));
        let base = if manifest_dir.is_absolute() {
            manifest_dir.to_path_buf()
        } else {
            std::env::current_dir()?.join(manifest_dir)
        };

        let servers_lock = document
            .servers_lock
            .as_deref()
            .map(|lock| resolve_manifest_path(&base, lock))
            .transpose()?;
        let fixtures_lock = document
            .fixtures_lock
            .as_deref()
            .map(|lock| resolve_manifest_path(&base, lock))
            .transpose()?;
        let mut servers = document.servers;
        let mut servers_lock_sha256 = None;
        if let Some(lock) = &servers_lock {
            if !servers.is_empty() {
                bail!("benchmark config cannot define both inline servers and `servers_lock`");
            }
            let (document, sha256) = load_yaml::<ServersLock>(lock, "server lock")?;
            validate_schema(document.version, "server lock")?;
            servers = document.servers;
            servers_lock_sha256 = Some(sha256);
        }
        let mut fixtures = document.fixtures;
        let mut fixtures_lock_sha256 = None;
        if let Some(lock) = &fixtures_lock {
            if !fixtures.is_empty() {
                bail!("benchmark config cannot define both inline fixtures and `fixtures_lock`");
            }
            let (document, sha256) = load_yaml::<FixturesLock>(lock, "fixture lock")?;
            validate_schema(document.version, "fixture lock")?;
            fixtures = document.fixtures;
            fixtures_lock_sha256 = Some(sha256);
        }
        let mut workloads = document.workloads;
        if !document.scenarios.is_empty() {
            if !workloads.is_empty() {
                bail!("benchmark config cannot define both `scenarios` and legacy `workloads`");
            }
            workloads = document
                .scenarios
                .iter()
                .map(|scenario| WorkloadSpec {
                    id: scenario.id.clone(),
                    fixture: scenario.fixture.clone(),
                    methods: scenario.methods.clone(),
                    steps: scenario.steps.clone(),
                })
                .collect();
        }
        let mut config = Self {
            schema_version: document.version,
            config_sha256,
            profiles: document.profiles,
            scenarios: document.scenarios,
            servers_lock_sha256,
            fixtures_lock_sha256,
            servers,
            fixtures,
            workloads,
        };
        for server in &mut config.servers {
            if server.id.is_empty() {
                bail!("server ids cannot be empty")
            }
            if server.version_args.is_empty() && server.locked_version.is_none() {
                bail!(
                    "server `{}` must declare `locked_version` when it has no version probe",
                    server.id
                )
            }
            if server.locked_version.as_ref().is_some_and(String::is_empty)
                || server.expected_version.as_ref().is_some_and(String::is_empty)
            {
                bail!("server `{}` contains an empty version pin", server.id)
            }
            if let Some(source) = &server.source {
                validate_source(source, &format!("server `{}`", server.id))?;
            }
            if let Some(install) = &mut server.install {
                match install.kind.as_str() {
                    "none" => {
                        if install.url.is_some()
                            || install.command.is_some()
                            || !install.args.is_empty()
                            || install.manifest.is_some()
                            || install.manifest_sha256.is_some()
                            || install.python_version.is_some()
                            || install.target.is_some()
                        {
                            bail!("server `{}` `none` install cannot define options", server.id)
                        }
                    }
                    "npm" => {
                        if install.url.is_some()
                            || install.command.is_some()
                            || !install.args.is_empty()
                            || install.python_version.is_some()
                            || install.target.is_some()
                        {
                            bail!("server `{}` npm install must use its locked manifest", server.id)
                        }
                        let manifest = install.manifest.as_mut().with_context(|| {
                            format!("server `{}` npm install requires a locked manifest", server.id)
                        })?;
                        validate_relative_path(manifest, "npm install manifest")?;
                        *manifest = resolve_path(&base, manifest);
                        let digest = install.manifest_sha256.as_deref().with_context(|| {
                            format!(
                                "server `{}` npm install requires a manifest SHA-256",
                                server.id
                            )
                        })?;
                        validate_sha256(digest, &format!("server `{}` npm manifest", server.id))?;
                    }
                    "pip" => {
                        if install.url.is_some() || !install.args.is_empty() {
                            bail!(
                                "server `{}` pip install does not accept arbitrary arguments",
                                server.id
                            )
                        }
                        if install.command.as_ref().is_none_or(|command| command.trim().is_empty())
                        {
                            bail!("server `{}` pip install requires a Python command", server.id)
                        }
                        let manifest = install.manifest.as_mut().with_context(|| {
                            format!("server `{}` pip install requires a locked manifest", server.id)
                        })?;
                        validate_relative_path(manifest, "pip install manifest")?;
                        *manifest = resolve_path(&base, manifest);
                        let digest = install.manifest_sha256.as_deref().with_context(|| {
                            format!(
                                "server `{}` pip install requires a manifest SHA-256",
                                server.id
                            )
                        })?;
                        validate_sha256(digest, &format!("server `{}` pip manifest", server.id))?;
                        if install
                            .python_version
                            .as_ref()
                            .is_none_or(|version| !is_python_minor_version(version))
                        {
                            bail!(
                                "server `{}` pip install requires a `major.minor` Python version",
                                server.id
                            )
                        }
                        if install.target.as_deref() != Some("x86_64-unknown-linux-gnu") {
                            bail!(
                                "server `{}` pip install target must be `x86_64-unknown-linux-gnu`",
                                server.id
                            )
                        }
                    }
                    "archive" | "binary" => {
                        if install.command.is_some()
                            || !install.args.is_empty()
                            || install.manifest.is_some()
                            || install.manifest_sha256.is_some()
                            || install.python_version.is_some()
                            || install.target.is_some()
                        {
                            bail!(
                                "server `{}` {} install must use its structured URL",
                                server.id,
                                install.kind
                            )
                        }
                        if install.url.as_ref().is_none_or(|url| url.trim().is_empty()) {
                            bail!(
                                "server `{}` {} install requires a structured URL",
                                server.id,
                                install.kind
                            )
                        }
                    }
                    "cargo" => {
                        if install.command.is_none() {
                            bail!("server `{}` cargo install command is missing", server.id)
                        }
                        if install.url.is_some()
                            || install.manifest.is_some()
                            || install.manifest_sha256.is_some()
                            || install.python_version.is_some()
                            || install.target.is_some()
                        {
                            bail!(
                                "server `{}` cargo install cannot define a download URL or package manifest",
                                server.id
                            )
                        }
                    }
                    kind => bail!("server `{}` has unsupported install kind `{kind}`", server.id),
                }
            }
            if let Some(artifact) = &server.artifact
                && let Some(digest) = &artifact.sha256
            {
                validate_sha256(digest, &format!("server `{}` artifact", server.id))?;
            }
            if server
                .install
                .as_ref()
                .is_some_and(|install| matches!(install.kind.as_str(), "archive" | "binary"))
            {
                server
                    .artifact
                    .as_ref()
                    .and_then(|artifact| artifact.sha256.as_deref())
                    .with_context(|| {
                        format!(
                            "server `{}` download install requires a pinned artifact SHA-256",
                            server.id
                        )
                    })?;
            }
            server.command = resolve_command(&base, &server.command);
            if let Some(artifact) = &mut server.artifact {
                artifact.path = resolve_path(&base, &artifact.path);
            }
            if server.install.as_ref().is_some_and(|install| install.kind == "binary")
                && server.artifact.as_ref().is_none_or(|artifact| artifact.path != server.command)
            {
                bail!("server `{}` binary command must be its pinned artifact", server.id)
            }
        }
        for fixture in &mut config.fixtures {
            if fixture.id.is_empty() {
                bail!("fixture ids cannot be empty")
            }
            fixture.root = resolve_path(&base, &fixture.root);
            if let Some(solc) = &mut fixture.solc {
                validate_compiler(solc, &format!("fixture `{}` solc", fixture.id))?;
                resolve_compiler_paths(&base, solc);
            }
            if let Some(foundry) = &mut fixture.foundry {
                validate_compiler(foundry, &format!("fixture `{}` foundry", fixture.id))?;
                resolve_compiler_paths(&base, foundry);
            }
            if let Some(revision) = &fixture.revision {
                validate_git_revision(revision, &format!("fixture `{}`", fixture.id))?;
            }
            if let Some(source) = &fixture.source {
                validate_source(source, &format!("fixture `{}`", fixture.id))?;
                if fixture.revision.as_ref().is_some_and(|revision| revision != &source.revision) {
                    bail!("fixture `{}` source and checkout revisions differ", fixture.id)
                }
            }
            for (name, dependency) in &fixture.dependencies {
                let Some((url, revision)) = dependency.rsplit_once('@') else {
                    bail!(
                        "fixture `{}` dependency `{name}` is not pinned as URL@revision",
                        fixture.id
                    )
                };
                if url.is_empty() {
                    bail!("fixture `{}` dependency `{name}` has an empty URL", fixture.id)
                }
                validate_git_revision(
                    revision,
                    &format!("fixture `{}` dependency `{name}`", fixture.id),
                )?;
            }
            if fixture.source_roots.is_empty() {
                fixture.source_roots.push(PathBuf::from("."));
            }
            if fixture.lsp_root.as_os_str().is_empty() {
                fixture.lsp_root = default_lsp_root();
            }
            validate_relative_path(&fixture.lsp_root, "fixture LSP root")?;
            for source_root in &fixture.source_roots {
                validate_relative_path(source_root, "fixture source root")?;
            }
            if fixture.anchors.keys().any(|name| name.is_empty()) {
                bail!("fixture `{}` contains an empty anchor name", fixture.id)
            }
            for anchor in fixture.anchors.values() {
                validate_relative_path(&anchor.path, "fixture anchor path")?;
                if !anchor.needle.is_char_boundary(anchor.offset) {
                    bail!(
                        "fixture `{}` anchor offset {} is not a UTF-8 character boundary",
                        fixture.id,
                        anchor.offset
                    )
                }
            }
        }
        validate_unique_ids(config.servers.iter().map(|server| server.id.as_str()), "server")?;
        validate_unique_ids(config.fixtures.iter().map(|fixture| fixture.id.as_str()), "fixture")?;
        validate_unique_ids(
            config.workloads.iter().map(|workload| workload.id.as_str()),
            "workload",
        )?;
        let fixtures =
            config.fixtures.iter().map(|fixture| fixture.id.as_str()).collect::<BTreeSet<_>>();
        let workloads =
            config.workloads.iter().map(|workload| workload.id.as_str()).collect::<BTreeSet<_>>();
        let profiles = config.profiles.keys().map(String::as_str).collect::<BTreeSet<_>>();
        for scenario in &config.scenarios {
            if let Some(profile) = &scenario.profile
                && !profiles.contains(profile.as_str())
            {
                bail!("scenario `{}` refers to unknown profile `{profile}`", scenario.id)
            }
        }
        for workload in &config.workloads {
            if !fixtures.contains(workload.fixture.as_str()) {
                bail!("workload `{}` refers to unknown fixture `{}`", workload.id, workload.fixture)
            }
            if workload.steps.is_empty() {
                bail!("workload `{}` has no steps", workload.id)
            }
            let warm_requests =
                workload.steps.iter().filter(|step| matches!(step, StepSpec::Warm { .. })).count();
            if warm_requests > 1 {
                bail!("workload `{}` contains more than one warm request", workload.id)
            }
            if workload.steps.iter().any(|step| {
                matches!(
                    step,
                    StepSpec::Replace { probe: None, .. } | StepSpec::Save { probe: None, .. }
                )
            }) {
                bail!(
                    "workload `{}` document mutation steps must define a correctness probe",
                    workload.id
                )
            }
            if workload.steps.iter().any(|step| {
                matches!(
                    step,
                    StepSpec::CreateFile { probe: None, .. }
                        | StepSpec::RenameFile { probe: None, .. }
                        | StepSpec::DeleteFile { probe: None, .. }
                )
            }) {
                bail!(
                    "workload `{}` file lifecycle steps must define a correctness probe",
                    workload.id
                )
            }
            let restart_indices = workload
                .steps
                .iter()
                .enumerate()
                .filter_map(|(index, step)| {
                    matches!(step, StepSpec::Restart { .. }).then_some(index)
                })
                .collect::<Vec<_>>();
            if restart_indices.len() > 1 {
                bail!("workload `{}` contains more than one restart", workload.id)
            }
            if restart_indices
                .first()
                .is_some_and(|index| *index == 0 || *index + 1 == workload.steps.len())
            {
                bail!("workload `{}` restart must have steps before and after it", workload.id)
            }
            for step in &workload.steps {
                validate_step_paths(step)?;
                let fixture = config
                    .fixtures
                    .iter()
                    .find(|fixture| fixture.id == workload.fixture)
                    .expect("workload fixture was validated above");
                validate_step_anchors(step, fixture, &workload.id)?;
            }
        }
        for (name, profile) in &config.profiles {
            if name.is_empty() {
                bail!("profile names cannot be empty")
            }
            if profile.samples == 0 || profile.cold_samples == 0 || profile.lifecycle_samples == 0 {
                bail!("profile `{name}` sample counts must be greater than zero")
            }
            if profile.timeout_ms == 0 {
                bail!("profile `{name}` timeout must be greater than zero")
            }
            if profile.readiness_quiet_ms == 0 || profile.readiness_quiet_ms >= profile.timeout_ms {
                bail!(
                    "profile `{name}` readiness quiet period must be between zero and its timeout"
                )
            }
            let mut seen = BTreeSet::new();
            for scenario in &profile.scenarios {
                if !seen.insert(scenario) {
                    bail!("profile `{name}` contains duplicate scenario `{scenario}`")
                }
                if !workloads.contains(scenario.as_str()) {
                    bail!("profile `{name}` refers to unknown scenario `{scenario}`")
                }
            }
        }
        Ok(config)
    }
}

fn default_enabled() -> bool {
    true
}

fn default_lsp_root() -> PathBuf {
    PathBuf::from(".")
}

const fn default_warmup() -> usize {
    10
}

const fn default_samples() -> usize {
    100
}

const fn default_cold_samples() -> usize {
    5
}

const fn default_lifecycle_samples() -> usize {
    10
}

const fn default_timeout_ms() -> u64 {
    30_000
}

const fn default_readiness_quiet_ms() -> u64 {
    50
}

const fn default_min_count() -> usize {
    1
}

fn default_version_args() -> Vec<String> {
    vec!["--version".into()]
}

fn resolve_path(base: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { base.join(path) }
}

fn resolve_compiler_paths(base: &Path, compiler: &mut CompilerSpec) {
    if let Some(path) = &mut compiler.native {
        *path = resolve_path(base, path);
    }
    if let Some(path) = &mut compiler.soljson {
        *path = resolve_path(base, path);
    }
}

fn resolve_manifest_path(base: &Path, path: &Path) -> Result<PathBuf> {
    validate_relative_path(path, "manifest reference")?;
    Ok(base.join(path))
}

fn load_yaml<T: for<'de> Deserialize<'de>>(path: &Path, kind: &str) -> Result<(T, String)> {
    let bytes =
        fs::read(path).with_context(|| format!("failed to read {kind} `{}`", path.display()))?;
    let document = serde_yaml::from_slice(&bytes)
        .with_context(|| format!("failed to parse {kind} `{}`", path.display()))?;
    Ok((document, sha256_bytes(&bytes)))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn validate_schema(version: u32, kind: &str) -> Result<()> {
    if version != SCHEMA_VERSION {
        bail!("unsupported {kind} schema {version}; expected {SCHEMA_VERSION}")
    }
    Ok(())
}

fn validate_source(source: &SourceSpec, kind: &str) -> Result<()> {
    if source.url.trim().is_empty() {
        bail!("{kind} source URL cannot be empty")
    }
    validate_git_revision(&source.revision, kind)
}

fn validate_git_revision(revision: &str, kind: &str) -> Result<()> {
    if revision.len() != 40 || !revision.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{kind} revision must be a full 40-character Git commit")
    }
    Ok(())
}

fn validate_sha256(digest: &str, kind: &str) -> Result<()> {
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        bail!("{kind} SHA-256 must contain exactly 64 hexadecimal characters")
    }
    Ok(())
}

fn is_python_minor_version(version: &str) -> bool {
    let Some((major, minor)) = version.split_once('.') else { return false };
    !major.is_empty()
        && !minor.is_empty()
        && !minor.contains('.')
        && major.bytes().all(|byte| byte.is_ascii_digit())
        && minor.bytes().all(|byte| byte.is_ascii_digit())
}

fn validate_compiler(compiler: &CompilerSpec, kind: &str) -> Result<()> {
    if compiler.version.trim().is_empty() {
        bail!("{kind} version cannot be empty")
    }
    validate_download_pair(
        compiler.native.as_deref(),
        compiler.native_url.as_deref(),
        compiler.native_sha256.as_deref(),
        &format!("{kind} native artifact"),
    )?;
    validate_download_pair(
        compiler.soljson.as_deref(),
        compiler.soljson_url.as_deref(),
        compiler.soljson_sha256.as_deref(),
        &format!("{kind} soljson artifact"),
    )?;
    if compiler.archive_url.is_some() != compiler.archive_sha256.is_some() {
        bail!("{kind} archive URL and SHA-256 must be declared together")
    }
    if let Some(digest) = &compiler.archive_sha256 {
        validate_sha256(digest, &format!("{kind} archive"))?;
    }
    if compiler.archive_url.is_some() && compiler.native.is_none() {
        bail!("{kind} archive requires a native artifact destination")
    }
    if compiler.archive_url.is_some() && compiler.native_sha256.is_none() {
        bail!("{kind} archive requires an extracted native artifact SHA-256")
    }
    Ok(())
}

fn validate_download_pair(
    path: Option<&Path>,
    url: Option<&str>,
    digest: Option<&str>,
    kind: &str,
) -> Result<()> {
    if url.is_some() && digest.is_none() {
        bail!("{kind} URL requires a SHA-256")
    }
    if (url.is_some() || digest.is_some()) && path.is_none() {
        bail!("{kind} URL or SHA-256 requires an artifact path")
    }
    if let Some(digest) = digest {
        validate_sha256(digest, kind)?;
    }
    Ok(())
}

fn validate_relative_path(path: &Path, kind: &str) -> Result<()> {
    if path.as_os_str().is_empty()
        || path.is_absolute()
        || path.components().any(|component| {
            matches!(component, std::path::Component::ParentDir | std::path::Component::Prefix(_))
        })
    {
        bail!("{kind} `{}` must be relative and stay in its root", path.display())
    }
    Ok(())
}

fn validate_step_paths(step: &StepSpec) -> Result<()> {
    match step {
        StepSpec::Open { path } => validate_relative_path(path, "scenario path"),
        StepSpec::Save { path, probe } => {
            validate_relative_path(path, "scenario path")?;
            if let Some(probe) = probe {
                validate_probe_path(probe)?;
            }
            Ok(())
        }
        StepSpec::Replace { path, probe, .. } => {
            validate_relative_path(path, "scenario path")?;
            if let Some(probe) = probe {
                validate_probe_path(probe)?;
            }
            Ok(())
        }
        StepSpec::Probe { probe, .. } => match probe {
            ProbeSpec::Definition { path, expected_path, .. } => {
                validate_relative_path(path, "scenario path")?;
                validate_relative_path(expected_path, "scenario expected path")
            }
            ProbeSpec::Completion { path, .. } | ProbeSpec::Hover { path, .. } => {
                validate_relative_path(path, "scenario path")
            }
            ProbeSpec::References { path, expected_locations, .. } => {
                validate_relative_path(path, "scenario path")?;
                validate_expected_locations(expected_locations)
            }
            ProbeSpec::DocumentSymbol { path, .. } => validate_relative_path(path, "scenario path"),
            ProbeSpec::WorkspaceSymbol { expected_path, .. } => {
                validate_relative_path(expected_path, "scenario expected path")
            }
        },
        StepSpec::Warm { probe, .. } => validate_probe_path(probe),
        StepSpec::Rename { path, expected_edits, probe, .. } => {
            validate_relative_path(path, "scenario path")?;
            validate_rename_edits(expected_edits)?;
            if let Some(probe) = probe {
                validate_probe_path(probe)?;
            }
            Ok(())
        }
        StepSpec::CreateFile { path, probe, .. } | StepSpec::DeleteFile { path, probe } => {
            validate_relative_path(path, "scenario path")?;
            if let Some(probe) = probe {
                validate_probe_path(probe)?;
            }
            Ok(())
        }
        StepSpec::RenameFile { from, to, probe } => {
            validate_relative_path(from, "scenario path")?;
            validate_relative_path(to, "scenario path")?;
            if let Some(probe) = probe {
                validate_probe_path(probe)?;
            }
            Ok(())
        }
        StepSpec::Restart { invalidate } => {
            if let Some(invalidate) = invalidate {
                validate_relative_path(&invalidate.path, "restart invalidation path")?;
            }
            Ok(())
        }
    }
}

fn validate_probe_path(probe: &ProbeSpec) -> Result<()> {
    match probe {
        ProbeSpec::Definition { path, expected_path, .. } => {
            validate_relative_path(path, "scenario path")?;
            validate_relative_path(expected_path, "scenario expected path")
        }
        ProbeSpec::Completion { path, .. } | ProbeSpec::Hover { path, .. } => {
            validate_relative_path(path, "scenario path")
        }
        ProbeSpec::References { path, expected_locations, .. } => {
            validate_relative_path(path, "scenario path")?;
            validate_expected_locations(expected_locations)
        }
        ProbeSpec::DocumentSymbol { path, .. } => validate_relative_path(path, "scenario path"),
        ProbeSpec::WorkspaceSymbol { expected_path, .. } => {
            validate_relative_path(expected_path, "scenario expected path")
        }
    }
}

fn validate_step_anchors(step: &StepSpec, fixture: &FixtureSpec, workload: &str) -> Result<()> {
    let require = |name: &str| {
        if fixture.anchors.contains_key(name) {
            Ok(())
        } else {
            bail!(
                "workload `{workload}` refers to unknown fixture `{}` anchor `{name}`",
                fixture.id
            )
        }
    };
    match step {
        StepSpec::Replace { anchor, probe, .. } => {
            require(anchor)?;
            if let Some(probe) = probe {
                validate_probe_anchors(probe, &require)?;
            }
        }
        StepSpec::Rename { anchor, expected_edits, probe, .. } => {
            require(anchor)?;
            for expected in expected_edits {
                require(&expected.anchor)?;
            }
            if let Some(probe) = probe {
                validate_probe_anchors(probe, &require)?;
            }
        }
        StepSpec::Probe { probe, .. } | StepSpec::Warm { probe, .. } => {
            validate_probe_anchors(probe, &require)?;
        }
        StepSpec::Save { probe, .. }
        | StepSpec::CreateFile { probe, .. }
        | StepSpec::RenameFile { probe, .. }
        | StepSpec::DeleteFile { probe, .. } => {
            if let Some(probe) = probe {
                validate_probe_anchors(probe, &require)?;
            }
        }
        StepSpec::Restart { invalidate } => {
            if let Some(invalidate) = invalidate {
                require(&invalidate.anchor)?;
            }
        }
        StepSpec::Open { .. } => {}
    }
    Ok(())
}

fn validate_probe_anchors(probe: &ProbeSpec, require: &impl Fn(&str) -> Result<()>) -> Result<()> {
    match probe {
        ProbeSpec::Definition { anchor, expected_anchor, .. } => {
            require(anchor)?;
            require(expected_anchor)
        }
        ProbeSpec::Completion { anchor, .. } | ProbeSpec::Hover { anchor, .. } => require(anchor),
        ProbeSpec::References { anchor, expected_locations, .. } => {
            require(anchor)?;
            for expected in expected_locations {
                require(&expected.anchor)?;
            }
            Ok(())
        }
        ProbeSpec::DocumentSymbol { .. } | ProbeSpec::WorkspaceSymbol { .. } => Ok(()),
    }
}

fn validate_expected_locations(expected: &[ExpectedLocationSpec]) -> Result<()> {
    if expected.is_empty() {
        bail!("references probe must declare at least one expected location")
    }
    for location in expected {
        validate_relative_path(&location.path, "scenario expected path")?;
    }
    Ok(())
}

fn validate_rename_edits(expected: &[ExpectedLocationSpec]) -> Result<()> {
    if expected.is_empty() {
        bail!("rename step must declare at least one expected edit")
    }
    for location in expected {
        validate_relative_path(&location.path, "rename expected path")?;
    }
    Ok(())
}

fn resolve_command(base: &Path, command: &Path) -> PathBuf {
    if command.is_absolute() || command.components().count() == 1 {
        command.to_path_buf()
    } else {
        base.join(command)
    }
}

fn validate_unique_ids<'a>(ids: impl IntoIterator<Item = &'a str>, kind: &str) -> Result<()> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            bail!("duplicate {kind} id `{id}`")
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn loads_and_resolves_relative_commands_and_fixture_roots() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.json");
        let mut file = fs::File::create(&path).unwrap();
        write!(
            file,
            r#"{{
                "schema_version": 1,
                "servers": [{{"id":"s", "command":"bin/server"}}],
                "fixtures": [{{"id":"f", "root":"fixtures"}}],
                "workloads": [{{"id":"w", "fixture":"f", "steps":[{{"kind":"open", "path":"src/Main.sol"}}]}}]
            }}"#
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.servers[0].command, directory.path().join("bin/server"));
        assert_eq!(config.fixtures[0].root, directory.path().join("fixtures"));
        assert_eq!(config.servers[0].version_args, ["--version"]);
    }

    #[test]
    fn rejects_non_stdio_transport() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: tcp\n    command: server\n    transport:\n      kind: tcp\n      address: 127.0.0.1:12345\nfixtures: []\nscenarios: []\n",
        )
        .unwrap();

        assert!(Config::load(&path).is_err());
    }

    #[test]
    fn rejects_duplicate_ids_and_unknown_fixture() {
        let config = r#"{
            "schema_version": 1,
            "servers": [{"id":"s", "command":"s"}, {"id":"s", "command":"s"}],
            "fixtures": [{"id":"f", "root":"."}],
            "workloads": [{"id":"w", "fixture":"missing", "steps":[{"kind":"save", "path":"a.sol"}]}]
        }"#;
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.json");
        fs::write(&path, config).unwrap();
        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("duplicate server id"));
    }

    #[test]
    fn loads_versioned_yaml_profiles_and_lock_references() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers_lock: servers.lock.yaml\nfixtures_lock: fixtures.lock.yaml\nprofiles:\n  smoke:\n    warmup: 1\n    samples: 2\nscenarios:\n  - id: synthetic-smoke\n    fixture: synthetic\n    profile: smoke\n    steps:\n      - kind: open\n        path: Main.sol\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("servers.lock.yaml"),
            "version: 1\nservers:\n  - id: solar\n    command: solar-lsp\n    required: true\n",
        )
        .unwrap();
        fs::write(
            directory.path().join("fixtures.lock.yaml"),
            "version: 1\nfixtures:\n  - id: synthetic\n    root: fixtures/synthetic\n    required: true\n    solc:\n      version: 0.8.30\n      native: artifacts/solc\n      soljson: artifacts/soljson.js\n    foundry:\n      version: 1.0.0\n      native: artifacts/forge\n",
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        assert_eq!(config.profiles["smoke"].samples, 2);
        assert_eq!(config.servers[0].id, "solar");
        assert!(config.fixtures[0].required);
        assert_eq!(config.workloads[0].id, "synthetic-smoke");
        assert_eq!(
            config.fixtures[0].solc.as_ref().unwrap().native.as_deref(),
            Some(directory.path().join("artifacts/solc").as_path())
        );
        assert_eq!(
            config.fixtures[0].solc.as_ref().unwrap().soljson.as_deref(),
            Some(directory.path().join("artifacts/soljson.js").as_path())
        );
        assert_eq!(
            config.fixtures[0].foundry.as_ref().unwrap().native.as_deref(),
            Some(directory.path().join("artifacts/forge").as_path())
        );
    }

    #[test]
    fn rejects_unknown_fields_and_short_revisions() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(&path, "version: 1\nunknown: true\nservers: []\nfixtures: []\nworkloads: []\n")
            .unwrap();
        assert!(format!("{:#}", Config::load(&path).unwrap_err()).contains("unknown field"));

        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\n    source:\n      url: https://example.invalid/server.git\n      revision: deadbeef\nfixtures: []\nworkloads: []\n",
        )
        .unwrap();
        assert!(Config::load(&path).unwrap_err().to_string().contains("40-character"));
    }

    #[test]
    fn rejects_unknown_profile_scenarios() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nprofiles:\n  smoke:\n    scenarios: [missing]\nservers: []\nfixtures: []\nworkloads: []\n",
        )
        .unwrap();
        assert!(Config::load(&path).unwrap_err().to_string().contains("unknown scenario"));
    }

    #[test]
    fn archive_compilers_require_an_extracted_native_digest() {
        let compiler = CompilerSpec {
            version: "1.0.0".into(),
            native: Some("artifacts/compiler".into()),
            soljson: None,
            native_url: None,
            native_sha256: None,
            soljson_url: None,
            soljson_sha256: None,
            archive_url: Some("https://example.invalid/compiler.tar.gz".into()),
            archive_sha256: Some("1".repeat(64)),
        };

        let error = validate_compiler(&compiler, "fixture compiler").unwrap_err().to_string();
        assert!(error.contains("extracted native artifact SHA-256"), "{error}");
    }

    #[test]
    fn rejects_multiple_warm_requests_in_one_workload() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\nfixtures:\n  - id: fixture\n    root: .\nscenarios:\n  - id: mixed-warm\n    fixture: fixture\n    steps:\n      - kind: warm\n        name: first\n        probe:\n          kind: document-symbol\n          path: Main.sol\n      - kind: warm\n        name: second\n        probe:\n          kind: document-symbol\n          path: Main.sol\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("more than one warm request"), "{error}");
    }

    #[test]
    fn rejects_file_lifecycle_without_correctness_probe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\nfixtures:\n  - id: fixture\n    root: .\nscenarios:\n  - id: unchecked-lifecycle\n    fixture: fixture\n    steps:\n      - kind: create-file\n        path: Scratch.sol\n        text: contract Scratch {}\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("must define a correctness probe"), "{error}");
    }

    #[test]
    fn rejects_document_mutations_without_correctness_probe() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        for step in [
            "      - kind: replace\n        path: Main.sol\n        anchor: edit\n        text: Replacement\n",
            "      - kind: save\n        path: Main.sol\n",
        ] {
            fs::write(
                &path,
                format!(
                    "version: 1\nservers:\n  - id: server\n    command: server\nfixtures:\n  - id: fixture\n    root: .\n    anchors:\n      edit:\n        path: Main.sol\n        needle: Main\nscenarios:\n  - id: unchecked-mutation\n    fixture: fixture\n    steps:\n{step}"
                ),
            )
            .unwrap();

            let error = Config::load(&path).unwrap_err().to_string();
            assert!(error.contains("must define a correctness probe"), "{error}");
        }
    }

    #[test]
    fn save_probe_paths_stay_inside_fixture() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\nfixtures:\n  - id: fixture\n    root: .\nscenarios:\n  - id: checked-save\n    fixture: fixture\n    steps:\n      - kind: save\n        path: Main.sol\n        probe:\n          kind: document-symbol\n          path: ../escape.sol\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("must be relative and stay in its root"), "{error}");
    }

    #[test]
    fn npm_install_requires_a_locked_manifest() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: npm\n      command: npm\n      args: [install, package@1.0.0]\nfixtures: []\nscenarios: []\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("locked manifest"), "{error}");

        fs::create_dir(directory.path().join("npm")).unwrap();
        fs::write(directory.path().join("npm/package-lock.json"), "{}").unwrap();
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: npm\n      manifest: npm\nfixtures: []\nscenarios: []\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("manifest SHA-256"), "{error}");
    }

    #[test]
    fn download_installs_require_pinned_artifacts() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        for kind in ["archive", "binary"] {
            fs::write(
                &path,
                format!(
                    "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: {kind}\n      url: https://example.invalid/artifact\nfixtures: []\nscenarios: []\n"
                ),
            )
            .unwrap();

            let error = Config::load(&path).unwrap_err().to_string();
            assert!(error.contains("pinned artifact SHA-256"), "{kind}: {error}");
        }
    }

    #[test]
    fn download_installs_reject_arbitrary_commands() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        for kind in ["archive", "binary"] {
            fs::write(
                &path,
                format!(
                    "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: {kind}\n      command: sh\n      args: [-c, download-and-run]\n    artifact:\n      path: artifact\n      sha256: {}\nfixtures: []\nscenarios: []\n",
                    "a".repeat(64)
                ),
            )
            .unwrap();

            let error = Config::load(&path).unwrap_err().to_string();
            assert!(error.contains("structured URL"), "{kind}: {error}");
        }
    }

    #[test]
    fn pip_install_requires_a_hashed_platform_lock() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: pip\n      command: python3\n      manifest: requirements.txt\n      python_version: '3.12'\n      target: x86_64-unknown-linux-gnu\nfixtures: []\nscenarios: []\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("manifest SHA-256"), "{error}");

        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: pip\n      command: python3\n      manifest: requirements.txt\n      manifest_sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n      python_version: '3.12'\n      target: x86_64-unknown-linux-gnu\nfixtures: []\nscenarios: []\n",
        )
        .unwrap();

        let config = Config::load(&path).unwrap();
        let install = config.servers[0].install.as_ref().unwrap();
        assert_eq!(
            install.manifest.as_deref(),
            Some(directory.path().join("requirements.txt").as_path())
        );
    }

    #[test]
    fn pip_install_rejects_unstructured_commands() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("benchmark.yaml");
        fs::write(
            &path,
            "version: 1\nservers:\n  - id: server\n    command: server\n    install:\n      kind: pip\n      command: sh\n      args: [-c, pip-install]\n      manifest: requirements.txt\n      manifest_sha256: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa\n      python_version: '3.12'\n      target: x86_64-unknown-linux-gnu\nfixtures: []\nscenarios: []\n",
        )
        .unwrap();

        let error = Config::load(&path).unwrap_err().to_string();
        assert!(error.contains("does not accept arbitrary arguments"), "{error}");
    }
}

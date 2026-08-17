//! JSON-RPC transport and process accounting for external LSP servers.

use crate::{
    config::{CompilerSpec, ServerSpec, TransportSpec},
    protocol,
};
use anyhow::{Context, Result, anyhow, bail};
use glob::Pattern;
use lsp_types::Url;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::{
    collections::{BTreeMap, BTreeSet},
    ffi::OsString,
    io::{BufReader, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    process::{Child, Command, ExitStatus, Stdio},
    sync::{Arc, Mutex, mpsc},
    thread,
    time::{Duration, Instant},
};

#[cfg(target_os = "linux")]
const NETWORK_NAMESPACE_ENV: &str = "LSP_BENCH_NETWORK_NAMESPACE";

#[cfg(unix)]
use std::os::unix::process::ExitStatusExt;

#[cfg(target_os = "linux")]
use std::{
    fs::{File, OpenOptions},
    os::fd::AsRawFd,
    sync::atomic::{AtomicU64, Ordering},
};

const PUBLISH_DIAGNOSTICS: &str = "textDocument/publishDiagnostics";
const MAX_SERVER_MESSAGE_BYTES: usize = 64 * 1024 * 1024;
const MAX_STDERR_BYTES: usize = 256 * 1024;
const MAX_TRACE_BYTES: usize = 8 * 1024 * 1024;
const MAX_TRACE_EVENTS: usize = 4096;
const MAX_TRACE_MESSAGE_BYTES: usize = 256 * 1024;
const STDERR_TRUNCATION_MARKER: &[u8] = b"\n[stderr truncated]\n";

const FILE_OPERATION_CAPABILITIES: [(&str, &str); 6] = [
    ("workspace/willCreateFiles", "willCreate"),
    ("workspace/didCreateFiles", "didCreate"),
    ("workspace/willRenameFiles", "willRename"),
    ("workspace/didRenameFiles", "didRename"),
    ("workspace/willDeleteFiles", "willDelete"),
    ("workspace/didDeleteFiles", "didDelete"),
];

#[derive(Clone, Debug)]
struct FileOperationRegistration {
    id: Option<String>,
    filters: Option<Vec<FileOperationFilter>>,
}

#[derive(Clone, Debug)]
struct FileOperationFilter {
    scheme: Option<String>,
    authority: Option<String>,
    pattern: Pattern,
    matches: Option<FileOperationMatch>,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum FileOperationMatch {
    File,
    Folder,
}

/// Notifications that must be sent after a client-side workspace edit has been applied.
///
/// The process layer owns the JSON-RPC response, while the session owns the fixture and its
/// open-document state. Returning notifications from the session keeps those ownership
/// boundaries explicit and lets the response acknowledge only an already-applied edit.
pub(crate) type WorkspaceEditNotifications = Vec<(String, Value)>;

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum Direction {
    Send,
    Receive,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct TraceEvent {
    pub(crate) elapsed_ms: f64,
    pub(crate) direction: Direction,
    pub(crate) method: Option<String>,
    pub(crate) id: Option<Value>,
    pub(crate) message: Value,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct RequestMeasurement {
    pub(crate) method: String,
    pub(crate) elapsed_ms: f64,
    pub(crate) process_tree_cpu_ms: Option<f64>,
}

struct PendingResponse {
    message: Value,
    received_at: Instant,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub(crate) struct Observations {
    pub(crate) diagnostic_publications: usize,
    pub(crate) requests: Vec<RequestMeasurement>,
    pub(crate) events: Vec<TraceEvent>,
    pub(crate) server_requests: Vec<ServerRequest>,
    pub(crate) trace_truncated: bool,
}

impl Observations {
    pub(crate) fn has_authoritative_process_tree_request_metrics(&self) -> bool {
        self.requests
            .iter()
            .all(|request| request.process_tree_cpu_ms.is_some_and(is_finite_non_negative))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ServerRequest {
    pub(crate) method: String,
    pub(crate) handled: bool,
    pub(crate) error_code: Option<i64>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ProcessAccounting {
    CgroupV2ProcessTree,
    RusageDirectChild,
    Unavailable,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, Ord, PartialEq, PartialOrd, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum MemoryAccounting {
    /// Peak total cgroup memory, including anonymous, file, and kernel memory.
    CgroupV2Total,
    /// Peak resident set size reported for the direct child only.
    RusageMaxRssDirectChild,
    Unavailable,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(crate) struct ProcessMetrics {
    pub(crate) wall_ms: f64,
    pub(crate) user_cpu_ms: Option<f64>,
    pub(crate) system_cpu_ms: Option<f64>,
    pub(crate) peak_memory_mib: Option<f64>,
    /// Peak sampled resident set size summed across live cgroup members.
    pub(crate) peak_process_tree_rss_mib: Option<f64>,
    pub(crate) accounting: ProcessAccounting,
    pub(crate) memory_accounting: MemoryAccounting,
    pub(crate) process_tree: bool,
    pub(crate) network_isolated: bool,
    pub(crate) cgroup_path: Option<PathBuf>,
    pub(crate) exit_code: Option<i32>,
    pub(crate) forced_kill: bool,
    pub(crate) stderr: String,
}

impl ProcessMetrics {
    pub(crate) fn has_authoritative_process_tree_metrics(&self) -> bool {
        let cpu_metrics_are_valid =
            self.user_cpu_ms.zip(self.system_cpu_ms).is_some_and(|(user, system)| {
                is_finite_non_negative(user)
                    && is_finite_non_negative(system)
                    && is_finite_non_negative(user + system)
            });
        self.process_tree
            && self.accounting == ProcessAccounting::CgroupV2ProcessTree
            && is_finite_non_negative(self.wall_ms)
            && cpu_metrics_are_valid
            && self.peak_memory_mib.is_some_and(is_finite_non_negative)
            && self.peak_process_tree_rss_mib.is_some_and(is_finite_non_negative)
            && self.memory_accounting == MemoryAccounting::CgroupV2Total
            && self.network_isolated
            && !self.forced_kill
    }

    pub(crate) fn peak_memory_metric(&self) -> Option<(&'static str, f64)> {
        let name = match self.memory_accounting {
            MemoryAccounting::CgroupV2Total => "peak_cgroup_memory_mib",
            MemoryAccounting::RusageMaxRssDirectChild => "peak_direct_child_rss_mib",
            MemoryAccounting::Unavailable => return None,
        };
        self.peak_memory_mib.map(|value| (name, value))
    }
}

fn is_finite_non_negative(value: f64) -> bool {
    value.is_finite() && value >= 0.0
}

/// Enter one private network namespace for an entire benchmark run.
///
/// The caller must invoke this before preparing or spawning any measured
/// server.  Keeping the namespace at the run boundary is important for TCP
/// transports: the runner and a TCP server must see the same loopback device.
pub(crate) fn ensure_network_namespace(required: bool) -> Result<()> {
    if !required {
        return Ok(());
    }

    #[cfg(target_os = "linux")]
    {
        if network_namespace_active() {
            return Ok(());
        }

        let parent_namespace = std::fs::read_link("/proc/self/ns/net")
            .context("failed to inspect the current network namespace")?;
        let executable =
            std::env::current_exe().context("failed to locate benchmark executable")?;
        let arguments = std::env::args_os().skip(1).collect::<Vec<_>>();
        let mut command = Command::new("unshare");
        command
            .args(["--user", "--map-root-user", "--net", "--"])
            .arg("sh")
            .arg("-c")
            .arg("ip link set lo up || exit $?; exec \"$0\" \"$@\"")
            .arg(&executable)
            .args(arguments)
            .env(NETWORK_NAMESPACE_ENV, parent_namespace);

        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            let error = command.exec();
            Err(error).context("failed to re-exec benchmark in a network namespace")
        }

        #[cfg(not(unix))]
        {
            let status = command.status().context("failed to execute `unshare`")?;
            if status.success() {
                Ok(())
            } else {
                bail!("network namespace re-exec exited with {status}")
            }
        }
    }

    #[cfg(not(target_os = "linux"))]
    {
        bail!("network namespace isolation is only available on Linux")
    }
}

pub(crate) fn network_namespace_active() -> bool {
    #[cfg(target_os = "linux")]
    {
        let Some(parent) = std::env::var_os(NETWORK_NAMESPACE_ENV) else { return false };
        std::fs::read_link("/proc/self/ns/net").is_ok_and(|current| current.as_os_str() != parent)
            && isolated_network_interfaces(Path::new("/sys/class/net"))
    }

    #[cfg(not(target_os = "linux"))]
    false
}

#[cfg(any(target_os = "linux", test))]
fn isolated_network_interfaces(directory: &Path) -> bool {
    let Ok(entries) = std::fs::read_dir(directory) else { return false };
    let mut count = 0;
    for entry in entries {
        let Ok(entry) = entry else { return false };
        count += 1;
        if entry.file_name() != "lo" {
            return false;
        }
    }
    if count != 1 {
        return false;
    }

    let Ok(flags) = std::fs::read_to_string(directory.join("lo/flags")) else { return false };
    let Some(flags) = flags.trim().strip_prefix("0x") else { return false };
    u32::from_str_radix(flags, 16).is_ok_and(|flags| flags & 0x9 == 0x9)
}

pub(crate) struct FinishedProcess {
    pub(crate) metrics: ProcessMetrics,
    pub(crate) observations: Observations,
}

#[derive(Debug, thiserror::Error)]
#[error("LSP request `{method}` failed with code {code:?}: {message}")]
pub(crate) struct RemoteError {
    pub(crate) method: String,
    pub(crate) code: Option<i64>,
    pub(crate) message: String,
}

/// Isolated HOME, XDG directories, and package-manager caches for one logical
/// benchmark sequence.
#[derive(Clone)]
pub(crate) struct ProcessEnvironment {
    root: Arc<tempfile::TempDir>,
    variables: Arc<BTreeMap<OsString, OsString>>,
    network_isolation: bool,
}

impl ProcessEnvironment {
    pub(crate) fn for_toolchains(
        solc: Option<&CompilerSpec>,
        foundry: Option<&CompilerSpec>,
        network_isolation: bool,
    ) -> Result<Self> {
        let root =
            Arc::new(tempfile::tempdir().context("failed to create isolated server environment")?);
        for name in ["home", "cache", "config", "data", "bin"] {
            std::fs::create_dir_all(root.path().join(name))?;
        }
        let bin = root.path().join("bin");
        let mut variables = BTreeMap::from([
            (OsString::from("LSP_BENCH_OFFLINE"), OsString::from("1")),
            (OsString::from("CARGO_NET_OFFLINE"), OsString::from("true")),
            (OsString::from("npm_config_offline"), OsString::from("true")),
            (OsString::from("PIP_NO_INDEX"), OsString::from("1")),
            (OsString::from("UV_OFFLINE"), OsString::from("1")),
            (OsString::from("FOUNDRY_OFFLINE"), OsString::from("true")),
            (OsString::from("HARDHAT_DISABLE_TELEMETRY"), OsString::from("1")),
        ]);
        if let Some(compiler) = solc {
            if let Some(native) = &compiler.native {
                let alias = bin.join(executable_name("solc"));
                link_tool(native, &alias)?;
                variables.insert(OsString::from("SOLC"), alias.as_os_str().to_owned());
                variables.insert(OsString::from("SOLC_PATH"), alias.as_os_str().to_owned());
                variables.insert(OsString::from("FOUNDRY_SOLC"), alias.as_os_str().to_owned());
            }
            if let Some(soljson) = &compiler.soljson {
                if !soljson.is_file() {
                    bail!("pinned soljson `{}` was not prepared", soljson.display())
                }
                variables.insert(OsString::from("SOLJSON_PATH"), soljson.as_os_str().to_owned());
            }
            variables.insert(OsString::from("SOLC_VERSION"), OsString::from(&compiler.version));
        }
        if let Some(toolchain) = foundry {
            if let Some(native) = &toolchain.native {
                let alias = bin.join(executable_name("forge"));
                link_tool(native, &alias)?;
                variables.insert(OsString::from("FORGE_PATH"), alias.as_os_str().to_owned());
            }
            variables.insert(OsString::from("FOUNDRY_VERSION"), OsString::from(&toolchain.version));
        }
        let path = std::env::join_paths(
            std::iter::once(bin).chain(
                std::env::var_os("PATH")
                    .into_iter()
                    .flat_map(|paths| std::env::split_paths(&paths).collect::<Vec<_>>()),
            ),
        )?;
        variables.insert(OsString::from("PATH"), path);
        Ok(Self { root, variables: Arc::new(variables), network_isolation })
    }

    fn path(&self) -> &Path {
        self.root.path()
    }

    fn variables(&self) -> &BTreeMap<OsString, OsString> {
        &self.variables
    }
}

fn executable_name(name: &str) -> OsString {
    #[cfg(windows)]
    return OsString::from(format!("{name}.exe"));
    #[cfg(not(windows))]
    OsString::from(name)
}

fn link_tool(source: &Path, destination: &Path) -> Result<()> {
    if !source.is_file() {
        bail!("pinned tool `{}` was not prepared", source.display())
    }
    #[cfg(unix)]
    std::os::unix::fs::symlink(source, destination)?;
    #[cfg(windows)]
    std::fs::copy(source, destination)?;
    Ok(())
}

pub(crate) struct LspProcess {
    child: Option<Child>,
    writer: Option<Box<dyn Write + Send>>,
    messages: mpsc::Receiver<Result<Value>>,
    stdout_thread: Option<thread::JoinHandle<()>>,
    stderr_thread: Option<thread::JoinHandle<()>>,
    stderr: Arc<Mutex<Vec<u8>>>,
    trace_bytes: usize,
    next_id: i64,
    timeout: Duration,
    started_at: Instant,
    root_uri: String,
    initialization_options: Value,
    configuration: Value,
    observations: Observations,
    capabilities: Value,
    text_sync_kind: Option<u8>,
    text_open_close: bool,
    text_save: bool,
    text_save_include_text: bool,
    position_encoding: String,
    process_group: bool,
    pending_responses: BTreeMap<String, PendingResponse>,
    dynamic_capabilities: BTreeMap<String, BTreeMap<String, Value>>,
    completion_trigger_characters: BTreeSet<String>,
    file_operations: BTreeMap<String, Vec<FileOperationRegistration>>,
    active_progress: BTreeSet<String>,
    _environment: ProcessEnvironment,
    network_isolated: bool,
    #[cfg(target_os = "linux")]
    rss_sampler: Option<ProcessTreeRssSampler>,
    cgroup: Option<CgroupHandle>,
}

impl LspProcess {
    pub(crate) fn spawn_with_environment(
        spec: &ServerSpec,
        cwd: &Path,
        timeout: Duration,
        environment: ProcessEnvironment,
    ) -> Result<Self> {
        if let TransportSpec::Tcp { address } = spec.transport {
            TcpListener::bind(address).with_context(|| {
                format!("TCP LSP address `{address}` for server `{}` is already in use", spec.id)
            })?;
        }
        let started_at = Instant::now();
        let home = environment.path().join("home");
        let cache = environment.path().join("cache");
        let config = environment.path().join("config");
        let data = environment.path().join("data");
        #[cfg(target_os = "linux")]
        let (cgroup, cgroup_procs) = match CgroupHandle::create_linux() {
            Ok((cgroup, procs)) => (Some(cgroup), Some(procs)),
            Err(_) => (None, None),
        };
        #[cfg(not(target_os = "linux"))]
        let cgroup = None;
        let mut command = server_command(spec);
        for (key, value) in &spec.env {
            command.env(key, value);
        }
        command
            .current_dir(cwd)
            .env_remove("RUST_LOG")
            .env_remove("SOLAR_PROFILE")
            .env("NO_COLOR", "1")
            .env("LANG", "C")
            .env("HOME", &home)
            .env("XDG_CACHE_HOME", &cache)
            .env("XDG_CONFIG_HOME", &config)
            .env("XDG_DATA_HOME", &data)
            .env("npm_config_cache", cache.join("npm"))
            .env("PIP_CACHE_DIR", cache.join("pip"))
            .envs(environment.variables());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            command.process_group(0);
            #[cfg(target_os = "linux")]
            if let Some(cgroup_procs) = cgroup_procs {
                unsafe {
                    command.pre_exec(move || {
                        let bytes = b"0\n";
                        let written = libc::write(
                            cgroup_procs.as_raw_fd(),
                            bytes.as_ptr().cast(),
                            bytes.len(),
                        );
                        if written == bytes.len() as isize {
                            Ok(())
                        } else {
                            Err(std::io::Error::last_os_error())
                        }
                    });
                }
            }
        }
        let stdio_transport = matches!(spec.transport, TransportSpec::Stdio);
        let mut child = command
            .stdin(if stdio_transport { Stdio::piped() } else { Stdio::null() })
            .stdout(if stdio_transport { Stdio::piped() } else { Stdio::null() })
            .stderr(Stdio::piped())
            .spawn()
            .with_context(|| {
                format!(
                    "failed to start server `{}` with command `{}`",
                    spec.id,
                    display_command(spec)
                )
            })?;
        let stderr_pipe = child.stderr.take().context("LSP stderr is unavailable")?;
        let stderr = Arc::new(Mutex::new(Vec::new()));
        let stderr_buffer = stderr.clone();
        let stderr_thread = thread::spawn(move || {
            let mut reader = BufReader::new(stderr_pipe);
            capture_stderr(
                &mut reader,
                &mut stderr_buffer.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
            );
        });

        let (writer, reader): (Box<dyn Write + Send>, Box<dyn Read + Send>) = match spec.transport {
            TransportSpec::Stdio => (
                Box::new(child.stdin.take().context("LSP stdin is unavailable")?),
                Box::new(child.stdout.take().context("LSP stdout is unavailable")?),
            ),
            TransportSpec::Tcp { address } => {
                let stream = match connect_tcp(&mut child, address, timeout) {
                    Ok(stream) => stream,
                    Err(error) => {
                        terminate_child(child, true)?;
                        let _ = stderr_thread.join();
                        let stderr = String::from_utf8_lossy(
                            &stderr.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
                        )
                        .into_owned();
                        return Err(error.context(format!(
                            "failed to connect to TCP LSP server `{}`; stderr: {stderr}",
                            spec.id
                        )));
                    }
                };
                stream.set_nodelay(true)?;
                (Box::new(stream.try_clone()?), Box::new(stream))
            }
        };
        let (sender, messages) = mpsc::channel();
        let stdout_thread = thread::spawn(move || {
            let mut reader = BufReader::new(reader);
            loop {
                match protocol::read_message_limited(&mut reader, MAX_SERVER_MESSAGE_BYTES) {
                    Ok(Some(message)) => {
                        if sender.send(Ok(message)).is_err() {
                            return;
                        }
                    }
                    Ok(None) => return,
                    Err(error) => {
                        let _ = sender.send(Err(error));
                        return;
                    }
                }
            }
        });
        #[cfg(target_os = "linux")]
        let rss_sampler = cgroup.as_ref().map(ProcessTreeRssSampler::start);

        Ok(Self {
            child: Some(child),
            writer: Some(writer),
            messages,
            stdout_thread: Some(stdout_thread),
            stderr_thread: Some(stderr_thread),
            stderr,
            trace_bytes: 0,
            next_id: 1,
            timeout,
            started_at,
            root_uri: String::new(),
            initialization_options: spec.initialization_options.clone(),
            configuration: spec.configuration.clone(),
            observations: Observations::default(),
            capabilities: Value::Null,
            text_sync_kind: None,
            text_open_close: false,
            text_save: false,
            text_save_include_text: false,
            position_encoding: "utf-16".into(),
            process_group: true,
            pending_responses: BTreeMap::new(),
            dynamic_capabilities: BTreeMap::new(),
            completion_trigger_characters: BTreeSet::new(),
            file_operations: BTreeMap::new(),
            active_progress: BTreeSet::new(),
            network_isolated: environment.network_isolation && network_namespace_active(),
            #[cfg(target_os = "linux")]
            rss_sampler,
            _environment: environment,
            cgroup,
        })
    }

    pub(crate) fn notify(&mut self, method: &str, params: Value) -> Result<()> {
        self.send(json!({"jsonrpc": "2.0", "method": method, "params": params}))
    }

    pub(crate) fn notify_workspace_edit_notifications(
        &mut self,
        notifications: WorkspaceEditNotifications,
    ) -> Result<()> {
        for (method, params) in notifications {
            if method == "textDocument/didOpen" && !self.supports_open() {
                continue;
            }
            if method == "textDocument/didClose" && !self.supports_close() {
                continue;
            }
            if method == "textDocument/didChange" && !self.supports_change() {
                continue;
            }
            if is_file_operation_method(&method)
                && !self.file_operation_notification_matches(&method, &params)?
            {
                continue;
            }
            self.notify(&method, params)?;
        }
        Ok(())
    }

    pub(crate) fn request_with_handler<F>(
        &mut self,
        method: &str,
        params: Value,
        mut handler: F,
    ) -> Result<Value>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        self.request_inner_with_handler(method, Some(params), true, &mut handler)
    }

    pub(crate) fn setup_request_with_handler<F>(
        &mut self,
        method: &str,
        params: Value,
        mut handler: F,
    ) -> Result<Value>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        self.request_inner_with_handler(method, Some(params), false, &mut handler)
    }

    fn notify_without_params(&mut self, method: &str) -> Result<()> {
        self.send(json!({"jsonrpc": "2.0", "method": method}))
    }

    pub(crate) fn set_root(&mut self, uri: &str) {
        self.root_uri = uri.to_owned();
    }

    pub(crate) fn set_initialize_result(&mut self, result: &Value) {
        self.capabilities = result.get("capabilities").cloned().unwrap_or(Value::Null);
        self.text_sync_kind = text_sync_kind(&self.capabilities);
        let synchronization = self.capabilities.get("textDocumentSync");
        self.text_open_close = text_sync_open_close(&self.capabilities);
        self.text_save = synchronization
            .and_then(Value::as_object)
            .and_then(|value| value.get("save"))
            .is_some_and(|save| !save.is_null() && save != &Value::Bool(false));
        self.text_save_include_text = synchronization
            .and_then(Value::as_object)
            .and_then(|value| value.get("save"))
            .and_then(Value::as_object)
            .and_then(|value| value.get("includeText"))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.completion_trigger_characters = self
            .capabilities
            .pointer("/completionProvider/triggerCharacters")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .filter_map(Value::as_str)
            .map(str::to_owned)
            .collect();
        self.file_operations = file_operation_capabilities(&self.capabilities);
        self.position_encoding = self
            .capabilities
            .get("positionEncoding")
            .and_then(Value::as_str)
            .unwrap_or("utf-16")
            .to_owned();
    }

    pub(crate) fn supports(&self, method: &str) -> bool {
        if is_file_operation_method(method) {
            return self.file_operations.contains_key(method);
        }
        if self.has_dynamic_capability(method) {
            return true;
        }
        let key = match method {
            "textDocument/definition" => "definitionProvider",
            "textDocument/completion" => "completionProvider",
            "textDocument/hover" => "hoverProvider",
            "textDocument/references" => "referencesProvider",
            "textDocument/documentSymbol" => "documentSymbolProvider",
            "textDocument/rename" => "renameProvider",
            "workspace/symbol" => "workspaceSymbolProvider",
            _ => return true,
        };
        self.capabilities
            .get(key)
            .is_some_and(|value| !value.is_null() && value != &Value::Bool(false))
    }

    pub(crate) fn supports_file_operation(
        &self,
        method: &str,
        uri: &Url,
        is_directory: bool,
    ) -> bool {
        self.file_operations.get(method).is_some_and(|registrations| {
            registrations.iter().any(|registration| {
                registration.filters.as_ref().is_none_or(|filters| {
                    filters.iter().any(|filter| filter.matches(&self.root_uri, uri, is_directory))
                })
            })
        })
    }

    fn file_operation_notification_matches(&self, method: &str, params: &Value) -> Result<bool> {
        let files = params
            .get("files")
            .and_then(Value::as_array)
            .context("file operation notification is missing `files`")?;
        if files.is_empty() {
            return Ok(false);
        }
        for file in files {
            let uris = if method.ends_with("RenameFiles") {
                [
                    file.get("oldUri").and_then(Value::as_str),
                    file.get("newUri").and_then(Value::as_str),
                ]
            } else {
                [file.get("uri").and_then(Value::as_str), None]
            };
            for uri in uris.into_iter().flatten() {
                let uri = uri.parse::<Url>()?;
                let is_directory = uri.to_file_path().is_ok_and(|path| path.is_dir());
                if !self.supports_file_operation(method, &uri, is_directory) {
                    return Ok(false);
                }
            }
        }
        Ok(true)
    }

    pub(crate) fn incremental_sync(&self) -> bool {
        self.effective_text_sync_kind() == Some(2)
    }

    pub(crate) fn supports_change(&self) -> bool {
        self.effective_text_sync_kind().is_some_and(|kind| kind != 0)
    }

    pub(crate) fn supports_open(&self) -> bool {
        self.text_open_close || self.has_dynamic_capability("textDocument/didOpen")
    }

    pub(crate) fn supports_close(&self) -> bool {
        self.text_open_close || self.has_dynamic_capability("textDocument/didClose")
    }

    pub(crate) fn supports_save(&self) -> bool {
        self.has_dynamic_capability("textDocument/didSave") || self.text_save
    }

    pub(crate) fn save_include_text(&self) -> bool {
        self.text_save_include_text
            || self
                .dynamic_registration_options("textDocument/didSave")
                .any(|options| options.get("includeText").and_then(Value::as_bool).unwrap_or(false))
    }

    pub(crate) fn completion_uses_trigger(&self, trigger: &str) -> bool {
        self.completion_trigger_characters.contains(trigger)
            || self.dynamic_registration_options("textDocument/completion").any(|options| {
                options
                    .get("triggerCharacters")
                    .and_then(Value::as_array)
                    .into_iter()
                    .flatten()
                    .filter_map(Value::as_str)
                    .any(|character| character == trigger)
            })
    }

    fn has_dynamic_capability(&self, method: &str) -> bool {
        self.dynamic_capabilities.get(method).is_some_and(|registrations| !registrations.is_empty())
    }

    fn effective_text_sync_kind(&self) -> Option<u8> {
        self.dynamic_registration_options("textDocument/didChange")
            .find_map(|options| options.get("syncKind").and_then(Value::as_u64))
            .map(|kind| kind as u8)
            .or(self.text_sync_kind)
    }

    fn dynamic_registration_options(&self, method: &str) -> impl Iterator<Item = &Value> {
        self.dynamic_capabilities.get(method).into_iter().flat_map(BTreeMap::values)
    }

    pub(crate) fn position_encoding(&self) -> &str {
        &self.position_encoding
    }

    pub(crate) fn observations(&self) -> &Observations {
        &self.observations
    }

    pub(crate) fn process_started_at(&self) -> Instant {
        self.started_at
    }

    pub(crate) fn timeout(&self) -> Duration {
        self.timeout
    }

    pub(crate) fn wait_for_readiness_with_handler<F>(
        &mut self,
        quiet: Duration,
        mut handler: F,
    ) -> Result<()>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        let deadline = Instant::now() + self.timeout;
        let mut quiet_deadline = Instant::now() + quiet;
        loop {
            let now = Instant::now();
            if now >= deadline {
                bail!("timed out waiting for LSP indexing readiness")
            }
            if self.active_progress.is_empty() && now >= quiet_deadline {
                return Ok(());
            }
            let receive_deadline = if self.active_progress.is_empty() {
                quiet_deadline.min(deadline)
            } else {
                deadline
            };
            let remaining = receive_deadline.saturating_duration_since(now);
            match self.messages.recv_timeout(remaining) {
                Ok(message) => {
                    let message = message?;
                    let received_at = Instant::now();
                    self.record_event(Direction::Receive, &message);
                    self.dispatch_with_handler(message, received_at, &mut handler)?;
                    quiet_deadline = Instant::now() + quiet;
                }
                Err(mpsc::RecvTimeoutError::Timeout) if self.active_progress.is_empty() => {
                    return Ok(());
                }
                Err(mpsc::RecvTimeoutError::Timeout) => {
                    bail!("timed out waiting for LSP indexing progress to finish")
                }
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    bail!("LSP stdout closed while waiting for indexing readiness")
                }
            }
        }
    }

    pub(crate) fn initialization_options(&self) -> Value {
        self.initialization_options.clone()
    }

    fn request_inner_with_handler<F>(
        &mut self,
        method: &str,
        params: Option<Value>,
        measured: bool,
        handler: &mut F,
    ) -> Result<Value>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        let id = Value::from(self.next_id);
        self.next_id += 1;
        let key = id_key(&id)?;
        let mut message = json!({"jsonrpc": "2.0", "id": id, "method": method});
        if let Some(params) = params {
            message["params"] = params;
        }
        let process_tree_cpu_started_ms = measured.then(|| self.process_tree_cpu_ms()).flatten();
        let started_at = Instant::now();
        self.send(message)?;
        let deadline = Instant::now() + self.timeout;

        loop {
            if let Some(PendingResponse { message, received_at }) =
                self.pending_responses.remove(&key)
            {
                if let Some(error) = message.get("error") {
                    return Err(RemoteError {
                        method: method.to_owned(),
                        code: error.get("code").and_then(Value::as_i64),
                        message: error
                            .get("message")
                            .and_then(Value::as_str)
                            .unwrap_or("unknown remote error")
                            .to_owned(),
                    }
                    .into());
                }
                if measured {
                    let process_tree_cpu_ms = process_tree_cpu_started_ms
                        .zip(self.process_tree_cpu_ms())
                        .map(|(before, after)| (after - before).max(0.0));
                    self.observations.requests.push(RequestMeasurement {
                        method: method.to_owned(),
                        elapsed_ms: duration_ms(received_at.saturating_duration_since(started_at)),
                        process_tree_cpu_ms,
                    });
                }
                return Ok(message.get("result").cloned().unwrap_or(Value::Null));
            }
            let (message, received_at) = self.receive(deadline)?;
            self.dispatch_with_handler(message, received_at, handler)?;
        }
    }

    pub(crate) fn send_change(
        &mut self,
        uri: &str,
        version: i32,
        start: Value,
        end: Value,
        replacement: &str,
        full_text: &str,
    ) -> Result<()> {
        if !self.supports_change() {
            bail!("server does not advertise `textDocument/didChange`")
        }
        let change = if self.incremental_sync() {
            json!({"range": {"start": start, "end": end}, "text": replacement})
        } else {
            json!({"text": full_text})
        };
        self.notify(
            "textDocument/didChange",
            json!({
                "textDocument": {"uri": uri, "version": version},
                "contentChanges": [change]
            }),
        )
    }

    pub(crate) fn finish_with_handler<F>(
        mut self,
        graceful: bool,
        handler: &mut F,
    ) -> Result<FinishedProcess>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        let mut shutdown_error = None;
        if graceful {
            if let Err(error) = self.request_inner_with_handler("shutdown", None, false, handler) {
                shutdown_error = Some(error);
            } else if let Err(error) = self.notify_without_params("exit") {
                shutdown_error = Some(error);
            }
        }

        self.writer.take();

        let child = self.child.take().context("LSP child is unavailable")?;
        let (status, usage, mut forced_kill) =
            wait_with_usage(child, self.timeout, self.process_group)?;
        if let Some(cgroup) = &self.cgroup {
            forced_kill |= cgroup.kill_and_wait(self.timeout)?;
        }
        #[cfg(target_os = "linux")]
        let peak_process_tree_rss_mib =
            self.rss_sampler.take().and_then(ProcessTreeRssSampler::finish);
        #[cfg(not(target_os = "linux"))]
        let peak_process_tree_rss_mib = None;
        if let Some(thread) = self.stdout_thread.take() {
            let _ = thread.join();
        }
        if let Some(thread) = self.stderr_thread.take() {
            let _ = thread.join();
        }
        let stderr = String::from_utf8_lossy(
            &self.stderr.lock().unwrap_or_else(|poisoned| poisoned.into_inner()),
        )
        .into_owned();
        if let Some(error) = shutdown_error {
            return Err(error.context(format!("failed to stop LSP; stderr: {stderr}")));
        }
        let cgroup_path = self.cgroup.as_ref().map(CgroupHandle::path).cloned();
        let cgroup_metrics = self.cgroup.as_ref().and_then(CgroupHandle::read_metrics);
        let (
            user_cpu_ms,
            system_cpu_ms,
            peak_memory_mib,
            accounting,
            memory_accounting,
            process_tree,
        ) = if let Some(metrics) = cgroup_metrics {
            (
                metrics.user_cpu_ms,
                metrics.system_cpu_ms,
                metrics.peak_memory_mib,
                ProcessAccounting::CgroupV2ProcessTree,
                if metrics.peak_memory_mib.is_some() {
                    MemoryAccounting::CgroupV2Total
                } else {
                    MemoryAccounting::Unavailable
                },
                true,
            )
        } else {
            (
                usage.user_cpu_ms,
                usage.system_cpu_ms,
                usage.peak_rss_mib,
                usage.accounting,
                usage.memory_accounting,
                false,
            )
        };
        Ok(FinishedProcess {
            metrics: ProcessMetrics {
                wall_ms: duration_ms(self.started_at.elapsed()),
                user_cpu_ms,
                system_cpu_ms,
                peak_memory_mib,
                peak_process_tree_rss_mib,
                accounting,
                memory_accounting,
                process_tree,
                network_isolated: self.network_isolated,
                cgroup_path,
                exit_code: status.code(),
                forced_kill,
                stderr,
            },
            observations: self.observations.clone(),
        })
    }

    fn send(&mut self, message: Value) -> Result<()> {
        self.record_event(Direction::Send, &message);
        let writer = self.writer.as_mut().context("LSP transport is closed")?;
        protocol::write_message(writer, &message)
    }

    fn receive(&mut self, deadline: Instant) -> Result<(Value, Instant)> {
        let remaining = deadline.saturating_duration_since(Instant::now());
        if remaining.is_zero() {
            bail!("timed out waiting for LSP message")
        }
        let message = self.messages.recv_timeout(remaining).map_err(|error| match error {
            mpsc::RecvTimeoutError::Timeout => anyhow!("timed out waiting for LSP message"),
            mpsc::RecvTimeoutError::Disconnected => anyhow!("LSP stdout closed unexpectedly"),
        })??;
        let received_at = Instant::now();
        self.record_event(Direction::Receive, &message);
        Ok((message, received_at))
    }

    fn dispatch_with_handler<F>(
        &mut self,
        message: Value,
        received_at: Instant,
        handler: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        let method = message.get("method").and_then(Value::as_str).map(str::to_owned);
        let id = message.get("id").cloned();
        match (method, id) {
            (None, Some(id)) => {
                let key = id_key(&id)?;
                if self
                    .pending_responses
                    .insert(key.clone(), PendingResponse { message, received_at })
                    .is_some()
                {
                    bail!("received duplicate LSP response id `{key}`")
                }
                Ok(())
            }
            (Some(method), Some(id)) => {
                self.handle_server_request_with_handler(&method, id, &message, handler)
            }
            (Some(method), None) => {
                if method == PUBLISH_DIAGNOSTICS {
                    self.observations.diagnostic_publications += 1;
                }
                if method == "$/progress"
                    && let Some(token) = message.pointer("/params/token")
                    && let Some(kind) =
                        message.pointer("/params/value/kind").and_then(Value::as_str)
                {
                    let token = id_key(token)?;
                    match kind {
                        "begin" => {
                            self.active_progress.insert(token);
                        }
                        "end" => {
                            self.active_progress.remove(&token);
                        }
                        _ => {}
                    }
                }
                Ok(())
            }
            (None, None) => bail!("received invalid JSON-RPC message without `id` or `method`"),
        }
    }

    fn process_tree_cpu_ms(&self) -> Option<f64> {
        self.cgroup.as_ref()?.read_metrics()?.cpu_ms()
    }

    fn handle_server_request_with_handler<F>(
        &mut self,
        method: &str,
        id: Value,
        message: &Value,
        handler: &mut F,
    ) -> Result<()>
    where
        F: FnMut(&Value) -> Result<WorkspaceEditNotifications>,
    {
        let mut handler_error = None;
        let (result, handled) = match method {
            "workspace/configuration" => {
                let items = message
                    .pointer("/params/items")
                    .and_then(Value::as_array)
                    .cloned()
                    .unwrap_or_default();
                let values = items
                    .iter()
                    .map(|item| configuration_value(&self.configuration, item))
                    .collect::<Vec<_>>();
                (Value::Array(values), true)
            }
            "workspace/workspaceFolders" => {
                (json!([{"uri": self.root_uri, "name": "lsp-bench"}]), true)
            }
            "client/registerCapability" => {
                if let Some(registrations) =
                    message.pointer("/params/registrations").and_then(Value::as_array)
                {
                    for registration in registrations {
                        let Some(method) = registration.get("method").and_then(Value::as_str)
                        else {
                            continue;
                        };
                        if is_file_operation_method(method) {
                            let Some(parsed) =
                                file_operation_registration_from_dynamic(registration)
                            else {
                                continue;
                            };
                            let entries =
                                self.file_operations.entry(method.to_owned()).or_default();
                            if let Some(id) = parsed.id.as_deref() {
                                entries.retain(|entry| entry.id.as_deref() != Some(id));
                            }
                            entries.push(parsed);
                        } else if let Some(id) = registration.get("id").and_then(Value::as_str) {
                            let options =
                                registration.get("registerOptions").cloned().unwrap_or(Value::Null);
                            self.dynamic_capabilities
                                .entry(method.to_owned())
                                .or_default()
                                .insert(id.to_owned(), options);
                        }
                    }
                }
                (Value::Null, true)
            }
            "client/unregisterCapability" => {
                let registrations = message
                    .pointer("/params/unregisterations")
                    .or_else(|| message.pointer("/params/unregistrations"))
                    .and_then(Value::as_array);
                if let Some(registrations) = registrations {
                    for registration in registrations {
                        let Some(method) = registration.get("method").and_then(Value::as_str)
                        else {
                            continue;
                        };
                        if is_file_operation_method(method) {
                            let id = registration.get("id").and_then(Value::as_str);
                            if let Some(entries) = self.file_operations.get_mut(method) {
                                if let Some(id) = id {
                                    entries.retain(|entry| entry.id.as_deref() != Some(id));
                                } else {
                                    entries.retain(|entry| entry.id.is_none());
                                }
                                if entries.is_empty() {
                                    self.file_operations.remove(method);
                                }
                            }
                        } else if let Some(id) = registration.get("id").and_then(Value::as_str) {
                            let empty = self.dynamic_capabilities.get_mut(method).is_some_and(
                                |registrations| {
                                    registrations.remove(id);
                                    registrations.is_empty()
                                },
                            );
                            if empty {
                                self.dynamic_capabilities.remove(method);
                            }
                        }
                    }
                }
                (Value::Null, true)
            }
            "window/workDoneProgress/create"
            | "window/showMessageRequest"
            | "window/showDocument" => (Value::Null, true),
            "workspace/applyEdit" => {
                if let Some(edit) = message.pointer("/params/edit") {
                    match handler(edit) {
                        Ok(notifications) => {
                            self.notify_workspace_edit_notifications(notifications)?;
                            (json!({"applied": true}), true)
                        }
                        Err(error) => {
                            let reason = format!("{error:#}");
                            handler_error = Some(error);
                            (json!({"applied": false, "failureReason": reason}), true)
                        }
                    }
                } else {
                    handler_error = Some(anyhow!("workspace/applyEdit request has no edit"));
                    (json!({"applied": false, "failureReason": "request has no edit"}), true)
                }
            }
            _ => (Value::Null, false),
        };
        self.observations.server_requests.push(ServerRequest {
            method: method.to_owned(),
            handled,
            error_code: (!handled).then_some(-32601),
        });
        let response = if handled {
            json!({"jsonrpc": "2.0", "id": id, "result": result})
        } else {
            json!({
                "jsonrpc": "2.0",
                "id": id,
                "error": {"code": -32601, "message": format!("method not found: {method}")}
            })
        };
        self.send(response)?;
        if let Some(error) = handler_error {
            return Err(error.context("failed to apply server workspace edit"));
        }
        Ok(())
    }

    fn record_event(&mut self, direction: Direction, message: &Value) {
        let (message, message_bytes, truncated) = bounded_trace_message(message);
        if truncated {
            self.observations.trace_truncated = true;
        }
        if self.observations.events.len() >= MAX_TRACE_EVENTS
            || self.trace_bytes.saturating_add(message_bytes) > MAX_TRACE_BYTES
        {
            self.observations.trace_truncated = true;
            return;
        }
        self.trace_bytes += message_bytes;
        self.observations.events.push(TraceEvent {
            elapsed_ms: duration_ms(self.started_at.elapsed()),
            direction,
            method: message.get("method").and_then(Value::as_str).map(str::to_owned),
            id: message.get("id").cloned(),
            message,
        });
    }
}

impl Drop for LspProcess {
    fn drop(&mut self) {
        if let Some(child) = self.child.take() {
            let _ = terminate_child(child, self.process_group);
        }
        if let Some(cgroup) = &self.cgroup {
            let _ = cgroup.kill_and_wait(self.timeout);
        }
        #[cfg(target_os = "linux")]
        if let Some(sampler) = self.rss_sampler.take() {
            let _ = sampler.finish();
        }
    }
}

fn server_command(spec: &ServerSpec) -> Command {
    let mut command = Command::new(&spec.command);
    command.args(&spec.args);
    command
}

fn connect_tcp(
    child: &mut Child,
    address: std::net::SocketAddr,
    timeout: Duration,
) -> Result<TcpStream> {
    let deadline = Instant::now() + timeout;
    loop {
        match TcpStream::connect_timeout(&address, Duration::from_millis(50)) {
            Ok(stream) => return Ok(stream),
            Err(error) if Instant::now() >= deadline => {
                return Err(error).with_context(|| format!("timed out connecting to `{address}`"));
            }
            Err(_) => {}
        }
        if let Some(status) = child.try_wait()? {
            bail!("server exited with status {status} before listening on `{address}`")
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn network_isolation_available() -> Result<()> {
    let output = Command::new("unshare")
        .args(["--user", "--map-root-user", "--net", "--", "sh", "-c", "ip link set lo up"])
        .stdin(Stdio::null())
        .output()
        .context("failed to execute `unshare`")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        if stderr.is_empty() {
            bail!("unprivileged user/network namespaces or loopback setup are unavailable")
        }
        bail!("unprivileged user/network namespaces or loopback setup are unavailable: {stderr}")
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn network_isolation_available() -> Result<()> {
    bail!("network namespace isolation is only available on Linux")
}

fn display_command(spec: &ServerSpec) -> String {
    std::iter::once(spec.command.display().to_string())
        .chain(spec.args.iter().cloned())
        .collect::<Vec<_>>()
        .join(" ")
}

fn id_key(id: &Value) -> Result<String> {
    match id {
        Value::String(value) => Ok(format!("s:{value}")),
        Value::Number(value) => Ok(format!("n:{value}")),
        _ => bail!("invalid JSON-RPC id `{id}`"),
    }
}

fn configuration_value(configuration: &Value, item: &Value) -> Value {
    let Some(section) = item.get("section").and_then(Value::as_str) else {
        return configuration.clone();
    };
    let mut value = configuration;
    for component in section.split('.') {
        let Some(next) = value.get(component) else { return Value::Null };
        value = next;
    }
    value.clone()
}

fn is_file_operation_method(method: &str) -> bool {
    FILE_OPERATION_CAPABILITIES.iter().any(|(name, _)| *name == method)
}

fn file_operation_capabilities(
    capabilities: &Value,
) -> BTreeMap<String, Vec<FileOperationRegistration>> {
    let Some(file_operations) = capabilities.pointer("/workspace/fileOperations") else {
        return BTreeMap::new();
    };
    FILE_OPERATION_CAPABILITIES
        .iter()
        .filter_map(|(method, capability)| {
            let value = file_operations.get(*capability)?;
            let registration = file_operation_registration(value, None)?;
            Some(((*method).to_owned(), vec![registration]))
        })
        .collect()
}

fn file_operation_registration_from_dynamic(
    registration: &Value,
) -> Option<FileOperationRegistration> {
    let method = registration.get("method").and_then(Value::as_str)?;
    if !is_file_operation_method(method) {
        return None;
    }
    let id = registration.get("id").and_then(Value::as_str).map(str::to_owned);
    match registration.get("registerOptions") {
        Some(options) => file_operation_registration(options, id),
        None => Some(FileOperationRegistration { id, filters: None }),
    }
}

fn file_operation_registration(
    value: &Value,
    id: Option<String>,
) -> Option<FileOperationRegistration> {
    let filters = match value {
        Value::Bool(true) => None,
        Value::Bool(false) | Value::Null => return None,
        Value::Object(object) => {
            let Some(filters) = object.get("filters") else {
                return Some(FileOperationRegistration { id, filters: None });
            };
            let filters = filters.as_array()?;
            Some(filters.iter().filter_map(parse_file_operation_filter).collect())
        }
        _ => return None,
    };
    Some(FileOperationRegistration { id, filters })
}

fn parse_file_operation_filter(value: &Value) -> Option<FileOperationFilter> {
    let object = value.as_object()?;
    let scheme = object.get("scheme").and_then(Value::as_str).map(str::to_owned);
    let authority = object.get("authority").and_then(Value::as_str).map(str::to_owned);
    let pattern_object = object.get("pattern")?.as_object()?;
    let glob = pattern_object.get("glob").and_then(Value::as_str)?;
    let pattern = Pattern::new(glob).ok()?;
    let matches = match pattern_object.get("matches").and_then(Value::as_str) {
        None => None,
        Some("file") => Some(FileOperationMatch::File),
        Some("folder") => Some(FileOperationMatch::Folder),
        Some(_) => return None,
    };
    Some(FileOperationFilter { scheme, authority, pattern, matches })
}

impl FileOperationFilter {
    fn matches(&self, root_uri: &str, uri: &Url, is_directory: bool) -> bool {
        if self.scheme.as_deref().is_some_and(|scheme| scheme != uri.scheme()) {
            return false;
        }
        if self
            .authority
            .as_deref()
            .is_some_and(|authority| uri.host_str().unwrap_or_default() != authority)
        {
            return false;
        }
        if let Some(kind) = self.matches
            && (kind == FileOperationMatch::Folder) != is_directory
        {
            return false;
        }
        let path = relative_file_operation_path(root_uri, uri);
        self.pattern.matches(&path)
    }
}

fn relative_file_operation_path(root_uri: &str, uri: &Url) -> String {
    let path = uri.to_file_path().ok();
    let root = Url::parse(root_uri).ok().and_then(|root| root.to_file_path().ok());
    if let (Some(root), Some(path)) = (root, path)
        && let Ok(relative) = path.strip_prefix(root)
    {
        return relative.to_string_lossy().replace('\\', "/");
    }
    uri.path().trim_start_matches('/').to_owned()
}

fn text_sync_kind(capabilities: &Value) -> Option<u8> {
    let value = capabilities.get("textDocumentSync")?;
    value
        .as_u64()
        .map(|value| value as u8)
        .or_else(|| value.get("change").and_then(Value::as_u64).map(|value| value as u8))
}

fn text_sync_open_close(capabilities: &Value) -> bool {
    let Some(value) = capabilities.get("textDocumentSync") else { return false };
    value.as_u64().is_some_and(|kind| kind != 0)
        || value.get("openClose").and_then(Value::as_bool).unwrap_or(false)
}

fn capture_stderr(reader: &mut impl Read, buffer: &mut Vec<u8>) {
    let mut chunk = [0; 8192];
    let mut truncated = false;
    loop {
        match reader.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => {
                let available = MAX_STDERR_BYTES.saturating_sub(buffer.len());
                let keep = available.min(read);
                buffer.extend_from_slice(&chunk[..keep]);
                truncated |= keep < read;
            }
            Err(_) => break,
        }
    }
    if truncated && MAX_STDERR_BYTES >= STDERR_TRUNCATION_MARKER.len() {
        let content_limit = MAX_STDERR_BYTES - STDERR_TRUNCATION_MARKER.len();
        buffer.truncate(content_limit);
        buffer.extend_from_slice(STDERR_TRUNCATION_MARKER);
    }
}

fn bounded_trace_message(message: &Value) -> (Value, usize, bool) {
    let encoded_bytes = serde_json::to_vec(message)
        .map(|encoded| encoded.len())
        .unwrap_or(MAX_TRACE_MESSAGE_BYTES.saturating_add(1));
    if encoded_bytes <= MAX_TRACE_MESSAGE_BYTES {
        return (message.clone(), encoded_bytes, false);
    }
    let placeholder = json!({"truncated": true, "bytes": encoded_bytes});
    let placeholder_bytes = serde_json::to_vec(&placeholder).map_or(0, |encoded| encoded.len());
    (placeholder, placeholder_bytes, true)
}

#[cfg(target_os = "linux")]
static NEXT_CGROUP_ID: AtomicU64 = AtomicU64::new(0);

struct CgroupHandle {
    path: PathBuf,
}

#[cfg(target_os = "linux")]
struct ProcessTreeRssSampler {
    stop: mpsc::Sender<()>,
    thread: thread::JoinHandle<Option<f64>>,
}

#[cfg(target_os = "linux")]
impl ProcessTreeRssSampler {
    const INTERVAL: Duration = Duration::from_millis(10);

    fn start(cgroup: &CgroupHandle) -> Self {
        let path = cgroup.path.clone();
        let (stop, receiver) = mpsc::channel();
        let thread = thread::spawn(move || {
            let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
            if page_size <= 0 {
                return None;
            }
            let mut peak_bytes = None::<u64>;
            loop {
                if let Some(bytes) = process_tree_rss_bytes(&path, page_size as u64) {
                    peak_bytes = Some(peak_bytes.map_or(bytes, |peak| peak.max(bytes)));
                }
                match receiver.recv_timeout(Self::INTERVAL) {
                    Ok(()) | Err(mpsc::RecvTimeoutError::Disconnected) => break,
                    Err(mpsc::RecvTimeoutError::Timeout) => {}
                }
            }
            peak_bytes.map(|bytes| bytes as f64 / (1024.0 * 1024.0))
        });
        Self { stop, thread }
    }

    fn finish(self) -> Option<f64> {
        let _ = self.stop.send(());
        self.thread.join().ok().flatten()
    }
}

#[cfg(target_os = "linux")]
fn process_tree_rss_bytes(cgroup: &Path, page_size: u64) -> Option<u64> {
    let pids = std::fs::read_to_string(cgroup.join("cgroup.procs")).ok()?;
    let mut observed = false;
    let mut pages = 0u64;
    for pid in pids.lines() {
        let Ok(statm) = std::fs::read_to_string(format!("/proc/{pid}/statm")) else { continue };
        let Some(resident) = resident_pages(&statm) else { continue };
        observed = true;
        pages = pages.saturating_add(resident);
    }
    observed.then(|| pages.saturating_mul(page_size))
}

#[cfg(any(target_os = "linux", test))]
fn resident_pages(statm: &str) -> Option<u64> {
    statm.split_whitespace().nth(1)?.parse().ok()
}

#[derive(Clone, Copy)]
struct CgroupMetrics {
    user_cpu_ms: Option<f64>,
    system_cpu_ms: Option<f64>,
    peak_memory_mib: Option<f64>,
}

impl CgroupMetrics {
    #[cfg(any(target_os = "linux", test))]
    fn is_complete(&self) -> bool {
        self.user_cpu_ms.is_some() && self.system_cpu_ms.is_some() && self.peak_memory_mib.is_some()
    }

    fn cpu_ms(&self) -> Option<f64> {
        Some(self.user_cpu_ms? + self.system_cpu_ms?)
    }
}

#[cfg(target_os = "linux")]
pub(crate) fn cgroup_v2_process_tree_available() -> Result<PathBuf> {
    let (handle, procs) = CgroupHandle::create_linux()?;
    let path = handle.path.clone();
    drop(procs);
    drop(handle);
    Ok(path)
}

#[cfg(not(target_os = "linux"))]
pub(crate) fn cgroup_v2_process_tree_available() -> Result<PathBuf> {
    bail!("cgroup v2 process-tree accounting is only available on Linux")
}

impl CgroupHandle {
    #[cfg(target_os = "linux")]
    fn create_linux() -> Result<(Self, File)> {
        let membership = std::fs::read_to_string("/proc/self/cgroup")?;
        let relative = membership
            .lines()
            .find_map(|line| line.strip_prefix("0::"))
            .context("current process has no cgroup v2 membership")?;
        let parent = Path::new("/sys/fs/cgroup").join(relative.trim_start_matches('/'));
        let id = NEXT_CGROUP_ID.fetch_add(1, Ordering::Relaxed);
        let path = parent.join(format!("solar-lsp-bench-{}-{id}", std::process::id()));
        std::fs::create_dir(&path)?;
        let handle = Self { path };
        let complete_metrics = handle.read_metrics().is_some_and(|metrics| metrics.is_complete());
        if !complete_metrics || !handle.path.join("cgroup.events").is_file() {
            bail!("cgroup v2 process-tree CPU and peak-memory accounting is unavailable")
        }
        let procs = OpenOptions::new().write(true).open(handle.path.join("cgroup.procs"))?;
        Ok((handle, procs))
    }

    fn path(&self) -> &PathBuf {
        &self.path
    }

    #[cfg(target_os = "linux")]
    fn read_metrics(&self) -> Option<CgroupMetrics> {
        let cpu = std::fs::read_to_string(self.path.join("cpu.stat")).ok()?;
        let mut user_cpu_ms = None;
        let mut system_cpu_ms = None;
        for line in cpu.lines() {
            let (name, value) = line.split_once(' ')?;
            let value = value.parse::<f64>().ok()? / 1_000.0;
            match name {
                "user_usec" => user_cpu_ms = Some(value),
                "system_usec" => system_cpu_ms = Some(value),
                _ => {}
            }
        }
        let peak_memory_mib = std::fs::read_to_string(self.path.join("memory.peak"))
            .ok()
            .and_then(|value| value.trim().parse::<f64>().ok())
            .map(|bytes| bytes / (1024.0 * 1024.0));
        Some(CgroupMetrics { user_cpu_ms, system_cpu_ms, peak_memory_mib })
    }

    #[cfg(target_os = "linux")]
    fn kill_and_wait(&self, timeout: Duration) -> Result<bool> {
        if !self.is_populated()? {
            return Ok(false);
        }
        let kill = self.path.join("cgroup.kill");
        if kill.is_file() {
            std::fs::write(&kill, "1\n")?;
        } else {
            self.kill_members()?;
        }
        let deadline = Instant::now() + timeout;
        loop {
            if !self.is_populated()? {
                return Ok(true);
            }
            if Instant::now() >= deadline {
                bail!("timed out waiting for benchmark cgroup to become empty")
            }
            if !kill.is_file() {
                self.kill_members()?;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[cfg(target_os = "linux")]
    fn is_populated(&self) -> Result<bool> {
        let events = std::fs::read_to_string(self.path.join("cgroup.events"))?;
        events
            .lines()
            .find_map(|line| line.strip_prefix("populated "))
            .map(|value| value == "1")
            .context("benchmark cgroup has no `populated` event")
    }

    #[cfg(target_os = "linux")]
    fn kill_members(&self) -> Result<()> {
        for pid in std::fs::read_to_string(self.path.join("cgroup.procs"))?.lines() {
            if let Ok(pid) = pid.parse::<libc::pid_t>() {
                unsafe {
                    let _ = libc::kill(pid, libc::SIGKILL);
                }
            }
        }
        Ok(())
    }

    #[cfg(not(target_os = "linux"))]
    fn kill_and_wait(&self, _timeout: Duration) -> Result<bool> {
        Ok(false)
    }

    #[cfg(not(target_os = "linux"))]
    fn read_metrics(&self) -> Option<CgroupMetrics> {
        None
    }
}

impl Drop for CgroupHandle {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir(&self.path);
    }
}

struct ResourceUsage {
    user_cpu_ms: Option<f64>,
    system_cpu_ms: Option<f64>,
    peak_rss_mib: Option<f64>,
    accounting: ProcessAccounting,
    memory_accounting: MemoryAccounting,
}

#[cfg(unix)]
fn wait_with_usage(
    child: Child,
    timeout: Duration,
    process_group: bool,
) -> Result<(ExitStatus, ResourceUsage, bool)> {
    let pid = child.id() as libc::pid_t;
    let deadline = Instant::now() + timeout;
    let mut status = 0;
    let mut usage = unsafe { std::mem::zeroed::<libc::rusage>() };
    let mut forced_kill = false;
    loop {
        let result = unsafe { libc::wait4(pid, &mut status, libc::WNOHANG, &mut usage) };
        if result == pid {
            forced_kill |= kill_remaining_process_group(pid, process_group);
            drop(child);
            return Ok((ExitStatus::from_raw(status), resource_usage(usage), forced_kill));
        }
        if result < 0 {
            let error = std::io::Error::last_os_error();
            if error.raw_os_error() != Some(libc::EINTR) {
                return Err(error.into());
            }
        }
        if Instant::now() >= deadline {
            forced_kill = true;
            kill_process_group(pid, process_group);
            loop {
                let result = unsafe { libc::wait4(pid, &mut status, 0, &mut usage) };
                if result == pid {
                    drop(child);
                    return Ok((ExitStatus::from_raw(status), resource_usage(usage), forced_kill));
                }
                if result < 0 && std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR)
                {
                    return Err(std::io::Error::last_os_error().into());
                }
            }
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(not(unix))]
fn wait_with_usage(
    mut child: Child,
    timeout: Duration,
    _process_group: bool,
) -> Result<(ExitStatus, ResourceUsage, bool)> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok((status, unavailable_resource_usage(), false));
        }
        if Instant::now() >= deadline {
            child.kill()?;
            return Ok((child.wait()?, unavailable_resource_usage(), true));
        }
        thread::sleep(Duration::from_millis(5));
    }
}

#[cfg(unix)]
fn resource_usage(usage: libc::rusage) -> ResourceUsage {
    #[cfg(target_os = "macos")]
    let peak_rss_mib = usage.ru_maxrss as f64 / (1024.0 * 1024.0);
    #[cfg(not(target_os = "macos"))]
    let peak_rss_mib = usage.ru_maxrss as f64 / 1024.0;
    ResourceUsage {
        user_cpu_ms: Some(timeval_ms(usage.ru_utime)),
        system_cpu_ms: Some(timeval_ms(usage.ru_stime)),
        peak_rss_mib: Some(peak_rss_mib),
        accounting: ProcessAccounting::RusageDirectChild,
        memory_accounting: MemoryAccounting::RusageMaxRssDirectChild,
    }
}

#[cfg(not(unix))]
fn unavailable_resource_usage() -> ResourceUsage {
    ResourceUsage {
        user_cpu_ms: None,
        system_cpu_ms: None,
        peak_rss_mib: None,
        accounting: ProcessAccounting::Unavailable,
        memory_accounting: MemoryAccounting::Unavailable,
    }
}

#[cfg(unix)]
fn kill_process_group(pid: libc::pid_t, process_group: bool) {
    if process_group {
        unsafe {
            let _ = libc::kill(-pid, libc::SIGKILL);
        }
    }
    unsafe {
        let _ = libc::kill(pid, libc::SIGKILL);
    }
}

#[cfg(unix)]
fn kill_remaining_process_group(pid: libc::pid_t, process_group: bool) -> bool {
    process_group && unsafe { libc::kill(-pid, libc::SIGKILL) == 0 }
}

#[cfg(unix)]
fn terminate_child(mut child: Child, process_group: bool) -> Result<()> {
    let pid = child.id() as libc::pid_t;
    kill_process_group(pid, process_group);
    let _ = child.wait();
    Ok(())
}

#[cfg(not(unix))]
fn terminate_child(mut child: Child, _process_group: bool) -> Result<()> {
    child.kill().ok();
    child.wait().ok();
    Ok(())
}

#[cfg(unix)]
fn timeval_ms(value: libc::timeval) -> f64 {
    value.tv_sec as f64 * 1_000.0 + value.tv_usec as f64 / 1_000.0
}

fn duration_ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1_000.0
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ServerSpec;
    use std::{collections::BTreeMap, fs, io::Cursor};

    #[cfg(unix)]
    use std::os::unix::process::CommandExt;

    fn authoritative_process_metrics() -> ProcessMetrics {
        ProcessMetrics {
            wall_ms: 1.0,
            user_cpu_ms: Some(2.0),
            system_cpu_ms: Some(3.0),
            peak_memory_mib: Some(4.0),
            peak_process_tree_rss_mib: Some(1.5),
            accounting: ProcessAccounting::CgroupV2ProcessTree,
            memory_accounting: MemoryAccounting::CgroupV2Total,
            process_tree: true,
            network_isolated: true,
            cgroup_path: None,
            exit_code: Some(0),
            forced_kill: false,
            stderr: String::new(),
        }
    }

    #[test]
    fn authoritative_process_metrics_require_finite_non_negative_values() {
        #[derive(Clone, Copy)]
        enum Field {
            Wall,
            UserCpu,
            SystemCpu,
            PeakMemory,
            PeakProcessTreeRss,
        }

        let complete = authoritative_process_metrics();
        assert!(complete.has_authoritative_process_tree_metrics());

        for field in [
            Field::Wall,
            Field::UserCpu,
            Field::SystemCpu,
            Field::PeakMemory,
            Field::PeakProcessTreeRss,
        ] {
            for value in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -1.0] {
                let mut invalid = complete.clone();
                match field {
                    Field::Wall => invalid.wall_ms = value,
                    Field::UserCpu => invalid.user_cpu_ms = Some(value),
                    Field::SystemCpu => invalid.system_cpu_ms = Some(value),
                    Field::PeakMemory => invalid.peak_memory_mib = Some(value),
                    Field::PeakProcessTreeRss => invalid.peak_process_tree_rss_mib = Some(value),
                }
                assert!(!invalid.has_authoritative_process_tree_metrics());
            }
        }

        let mut overflowing_cpu = complete;
        overflowing_cpu.user_cpu_ms = Some(f64::MAX);
        overflowing_cpu.system_cpu_ms = Some(f64::MAX);
        assert!(!overflowing_cpu.has_authoritative_process_tree_metrics());
    }

    #[test]
    fn authoritative_request_metrics_require_finite_non_negative_process_tree_cpu() {
        let mut observations = Observations::default();
        assert!(observations.has_authoritative_process_tree_request_metrics());

        observations.requests.push(RequestMeasurement {
            method: "textDocument/hover".into(),
            elapsed_ms: 1.0,
            process_tree_cpu_ms: Some(0.5),
        });
        assert!(observations.has_authoritative_process_tree_request_metrics());

        for process_tree_cpu_ms in
            [None, Some(f64::NAN), Some(f64::INFINITY), Some(f64::NEG_INFINITY), Some(-1.0)]
        {
            observations.requests[0].process_tree_cpu_ms = process_tree_cpu_ms;
            assert!(!observations.has_authoritative_process_tree_request_metrics());
        }
    }

    #[test]
    fn text_sync_capability_supports_numeric_and_object_forms() {
        assert_eq!(text_sync_kind(&json!({"textDocumentSync": 1})), Some(1));
        assert_eq!(text_sync_kind(&json!({"textDocumentSync": {"change": 2}})), Some(2));
        assert_eq!(text_sync_kind(&json!({})), None);

        assert!(text_sync_open_close(&json!({"textDocumentSync": 1})));
        assert!(text_sync_open_close(&json!({"textDocumentSync": 2})));
        assert!(!text_sync_open_close(&json!({"textDocumentSync": 0})));
        assert!(text_sync_open_close(&json!({"textDocumentSync": {"openClose": true}})));
        assert!(!text_sync_open_close(&json!({"textDocumentSync": {"openClose": false}})));
    }

    #[test]
    fn file_operation_capabilities_parse_methods_and_filters() {
        let capabilities = json!({
            "workspace": {"fileOperations": {
                "didCreate": {"filters": [{
                    "scheme": "file",
                    "pattern": {"glob": "**/*.sol", "matches": "file"}
                }]},
                "willDelete": true
            }}
        });
        let parsed = file_operation_capabilities(&capabilities);
        assert!(parsed.contains_key("workspace/didCreateFiles"));
        assert!(parsed.contains_key("workspace/willDeleteFiles"));
        assert!(!parsed.contains_key("workspace/didRenameFiles"));

        let root = Url::from_file_path("/workspace").unwrap();
        let source = Url::from_file_path("/workspace/src/Main.sol").unwrap();
        let root_file = Url::from_file_path("/workspace/Main.sol").unwrap();
        let folder = Url::from_file_path("/workspace/src").unwrap();
        assert!(parsed["workspace/didCreateFiles"][0].filters.as_ref().unwrap()[0].matches(
            root.as_str(),
            &source,
            false
        ));
        assert!(parsed["workspace/didCreateFiles"][0].filters.as_ref().unwrap()[0].matches(
            root.as_str(),
            &root_file,
            false
        ));
        assert!(!parsed["workspace/didCreateFiles"][0].filters.as_ref().unwrap()[0].matches(
            root.as_str(),
            &folder,
            true
        ));
        assert!(parsed["workspace/willDeleteFiles"][0].filters.as_ref().is_none());
    }

    #[test]
    fn file_operation_registration_and_unregistration_keep_ids() {
        let registration = json!({
            "id": "create",
            "method": "workspace/didCreateFiles",
            "registerOptions": {"filters": [{"pattern": {"glob": "**/*.sol"}}]}
        });
        let parsed = file_operation_registration_from_dynamic(&registration).unwrap();
        assert_eq!(parsed.id.as_deref(), Some("create"));
        assert!(parsed.filters.is_some());
        assert!(
            file_operation_registration_from_dynamic(&json!({
                "id": "other",
                "method": "textDocument/hover"
            }))
            .is_none()
        );
    }

    #[test]
    fn file_operation_method_names_are_explicit() {
        assert!(is_file_operation_method("workspace/willCreateFiles"));
        assert!(is_file_operation_method("workspace/didDeleteFiles"));
        assert!(!is_file_operation_method("workspace/didChangeWatchedFiles"));
    }

    #[test]
    fn stderr_capture_drains_and_bounds_output() {
        let mut buffer = Vec::new();
        let input = vec![b'x'; MAX_STDERR_BYTES + 17];
        capture_stderr(&mut Cursor::new(input), &mut buffer);
        assert_eq!(buffer.len(), MAX_STDERR_BYTES);
        assert!(buffer.ends_with(STDERR_TRUNCATION_MARKER));
    }

    #[test]
    fn trace_messages_are_bounded_with_a_placeholder() {
        let message = json!({"payload": "x".repeat(MAX_TRACE_MESSAGE_BYTES) });
        let encoded_bytes = serde_json::to_vec(&message).unwrap().len();
        let (bounded, bytes, truncated) = bounded_trace_message(&message);
        assert!(truncated);
        assert!(bytes <= MAX_TRACE_MESSAGE_BYTES);
        assert_eq!(bounded["truncated"], true);
        assert_eq!(bounded["bytes"].as_u64(), Some(encoded_bytes as u64));
    }

    #[test]
    fn command_display_includes_arguments() {
        let spec = ServerSpec {
            id: "server".into(),
            command: "server".into(),
            args: vec!["--stdio".into()],
            transport: TransportSpec::Stdio,
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
        };
        assert_eq!(display_command(&spec), "server --stdio");
    }

    #[test]
    fn response_ids_preserve_string_and_number_domains() {
        assert_eq!(id_key(&json!(1)).unwrap(), "n:1");
        assert_eq!(id_key(&json!("1")).unwrap(), "s:1");
        assert!(id_key(&Value::Null).is_err());
    }

    #[test]
    fn workspace_configuration_resolves_dotted_sections() {
        let configuration = json!({"solidity": {"compiler": {"version": "0.8.30"}}});
        assert_eq!(
            configuration_value(&configuration, &json!({"section": "solidity.compiler"})),
            json!({"version": "0.8.30"})
        );
        assert_eq!(
            configuration_value(&configuration, &json!({"section": "missing"})),
            Value::Null
        );
    }

    #[test]
    fn server_command_does_not_wrap_tcp_or_stdio_transports() {
        for transport in [
            TransportSpec::Stdio,
            TransportSpec::Tcp { address: "127.0.0.1:12345".parse().unwrap() },
        ] {
            let spec = ServerSpec {
                id: "server".into(),
                command: "server".into(),
                args: vec!["--arg".into()],
                transport,
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
            };
            let command = server_command(&spec);
            assert_eq!(command.get_program(), "server");
            assert_eq!(command.get_args().collect::<Vec<_>>(), ["--arg"]);
        }
    }

    #[test]
    fn cgroup_metrics_require_cpu_breakdown_and_peak_memory() {
        let complete = CgroupMetrics {
            user_cpu_ms: Some(1.0),
            system_cpu_ms: Some(2.0),
            peak_memory_mib: Some(3.0),
        };
        assert!(complete.is_complete());
        assert!(!CgroupMetrics { user_cpu_ms: None, ..complete }.is_complete());
        assert!(!CgroupMetrics { system_cpu_ms: None, ..complete }.is_complete());
        assert!(!CgroupMetrics { peak_memory_mib: None, ..complete }.is_complete());
    }

    #[test]
    fn parses_resident_pages_from_proc_statm() {
        assert_eq!(resident_pages("100 42 7 3 0 9 0\n"), Some(42));
        assert_eq!(resident_pages("100\n"), None);
        assert_eq!(resident_pages("100 invalid\n"), None);
    }

    #[test]
    fn network_namespace_requires_only_an_enabled_loopback_interface() {
        let directory = tempfile::tempdir().unwrap();
        let network = directory.path();
        fs::create_dir(network.join("lo")).unwrap();
        fs::write(network.join("lo/flags"), "0x9\n").unwrap();
        assert!(isolated_network_interfaces(network));

        fs::create_dir(network.join("eth0")).unwrap();
        fs::write(network.join("eth0/flags"), "0x1003\n").unwrap();
        assert!(!isolated_network_interfaces(network));

        fs::remove_dir_all(network.join("eth0")).unwrap();
        fs::write(network.join("lo/flags"), "0x8\n").unwrap();
        assert!(!isolated_network_interfaces(network));
    }

    #[cfg(unix)]
    #[test]
    fn waiting_for_a_group_leader_kills_remaining_descendants() {
        let directory = tempfile::tempdir().unwrap();
        let pid_file = directory.path().join("descendant.pid");
        let mut command = Command::new("sh");
        command
            .args(["-c", "sleep 30 & echo $! > \"$1\"", "lsp-bench"])
            .arg(&pid_file)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .process_group(0);
        let child = command.spawn().unwrap();
        let deadline = Instant::now() + Duration::from_secs(2);
        while !pid_file.is_file() && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let descendant =
            fs::read_to_string(&pid_file).unwrap().trim().parse::<libc::pid_t>().unwrap();

        let (_, _, forced_kill) = wait_with_usage(child, Duration::from_secs(2), true).unwrap();
        assert!(forced_kill, "cleaning up a surviving descendant must be recorded");

        let deadline = Instant::now() + Duration::from_secs(2);
        while process_exists(descendant) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        if process_exists(descendant) {
            unsafe {
                libc::kill(descendant, libc::SIGKILL);
            }
            panic!("descendant process {descendant} survived its process-group leader");
        }
    }

    #[cfg(unix)]
    fn process_exists(pid: libc::pid_t) -> bool {
        let result = unsafe { libc::kill(pid, 0) };
        result == 0 || std::io::Error::last_os_error().raw_os_error() == Some(libc::EPERM)
    }
}

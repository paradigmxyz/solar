#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use crate::global_state::GlobalState;
use async_lsp::{
    ClientSocket, LspService, ResponseError, client_monitor::ClientProcessMonitorLayer,
    router::Router, tracing::TracingLayer,
};
#[cfg(test)]
use criterion as _;
use lsp_types::{notification as notif, request as req};
use normalize_path::NormalizePath;
use serde_json as _;
use solar_config::{EvmVersion, ImportRemapping, LspArgs};
use std::{
    ops::ControlFlow,
    path::{Path, PathBuf},
};
use tower::ServiceBuilder;

/// Configuration used to launch the language server before the client initializes it.
#[derive(Clone, Debug, Default)]
pub struct LaunchConfig {
    default_forge_path: Option<PathBuf>,
    /// Foundry profile selected by the embedding host, if any.
    selected_profile: Option<String>,
    /// Effective Foundry workspace configurations supplied by the embedding host.
    foundry_workspace_configs: Vec<FoundryWorkspaceConfig>,
}

/// Effective Foundry configuration for one workspace supplied by an embedding host.
///
/// `workspace_root` is the canonical absolute directory containing that workspace's
/// `foundry.toml`. The `PathBuf` fields must also be canonical absolute paths; root matching folds
/// redundant path components but does not resolve symlinks. Remapping target strings are final
/// [`ImportRemapping`] values and are passed through unchanged. These values are
/// already resolved by the host, including any inherited profiles and remappings. When this
/// configuration matches a workspace, the language server uses these values as-is: it does not
/// parse the local profile, read `remappings.txt`, or autodetect library remappings. The snapshot
/// is captured when [`LaunchConfig`] is built and is reused for rediscovery during that LSP
/// session; rebuild the launch configuration when Foundry configuration changes.
#[derive(Clone, Debug, Default)]
pub struct FoundryWorkspaceConfig {
    /// Canonical absolute directory containing the workspace manifest.
    pub workspace_root: PathBuf,
    /// Effective source roots used for workspace indexing.
    pub source_roots: Vec<PathBuf>,
    /// Effective source roots used for flycheck indexing (including tests and scripts).
    pub flycheck_source_roots: Vec<PathBuf>,
    /// Effective Foundry library/include paths.
    pub include_paths: Vec<PathBuf>,
    /// Final import remappings after all host-side resolution.
    pub import_remappings: Vec<ImportRemapping>,
    /// Effective EVM version, if the selected Foundry configuration supplied one.
    pub evm_version: Option<EvmVersion>,
}

impl FoundryWorkspaceConfig {
    /// Creates an empty resolved configuration for `workspace_root`.
    ///
    /// The root and all paths supplied to the builder must be canonical absolute paths. Hosts
    /// should provide every effective value they want the compiler to use.
    pub fn new(workspace_root: impl Into<PathBuf>) -> Self {
        Self { workspace_root: workspace_root.into(), ..Self::default() }
    }

    /// Sets the effective workspace source roots.
    pub fn with_source_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.source_roots = roots.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the effective source roots used by flycheck.
    pub fn with_flycheck_source_roots<I, P>(mut self, roots: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.flycheck_source_roots = roots.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the effective Foundry library/include paths.
    pub fn with_include_paths<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.include_paths = paths.into_iter().map(Into::into).collect();
        self
    }

    /// Sets the final host-resolved import remappings.
    pub fn with_import_remappings<I>(mut self, remappings: I) -> Self
    where
        I: IntoIterator<Item = ImportRemapping>,
    {
        self.import_remappings = remappings.into_iter().collect();
        self
    }

    /// Sets the effective EVM version.
    pub fn with_evm_version(mut self, evm_version: EvmVersion) -> Self {
        self.evm_version = Some(evm_version);
        self
    }

    /// Returns the workspace root used for exact manifest matching.
    pub fn workspace_root(&self) -> &Path {
        &self.workspace_root
    }

    /// Returns the effective source roots.
    pub fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }

    /// Returns the effective flycheck source roots.
    pub fn flycheck_source_roots(&self) -> &[PathBuf] {
        &self.flycheck_source_roots
    }

    /// Returns the effective include paths.
    pub fn include_paths(&self) -> &[PathBuf] {
        &self.include_paths
    }

    /// Returns the final host-resolved import remappings.
    pub fn import_remappings(&self) -> &[ImportRemapping] {
        &self.import_remappings
    }

    /// Returns the effective EVM version, if one was supplied.
    pub fn evm_version(&self) -> Option<EvmVersion> {
        self.evm_version
    }
}

impl From<LspArgs> for LaunchConfig {
    fn from(_: LspArgs) -> Self {
        Self::default()
    }
}

impl LaunchConfig {
    /// Sets the Forge executable to use when the client does not provide one.
    pub fn with_default_forge_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.default_forge_path = Some(path.into());
        self
    }

    /// Selects the Foundry profile used for workspace discovery and Forge flychecks.
    pub fn with_selected_profile(mut self, profile: impl Into<String>) -> Self {
        self.selected_profile = Some(profile.into());
        self
    }

    /// Supplies an already-resolved Foundry workspace configuration.
    ///
    /// The configuration is keyed by its exact canonical workspace root. Calling this method
    /// again for the same root replaces the earlier snapshot; configurations for other roots are
    /// retained. The snapshot is reused for workspace rediscovery until a new LSP launch.
    pub fn with_foundry_workspace_config(mut self, config: FoundryWorkspaceConfig) -> Self {
        let mut config = config;
        config.workspace_root = config.workspace_root.normalize();
        if let Some(existing) = self
            .foundry_workspace_configs
            .iter_mut()
            .find(|existing| existing.workspace_root.normalize() == config.workspace_root)
        {
            *existing = config;
        } else {
            self.foundry_workspace_configs.push(config);
        }
        self
    }

    /// Supplies already-resolved configurations for multiple Foundry workspaces.
    pub fn with_foundry_workspace_configs(
        mut self,
        configs: impl IntoIterator<Item = FoundryWorkspaceConfig>,
    ) -> Self {
        for config in configs {
            self = self.with_foundry_workspace_config(config);
        }
        self
    }

    pub(crate) fn default_forge_path(&self) -> Option<&Path> {
        self.default_forge_path.as_deref()
    }

    pub(crate) fn selected_profile(&self) -> Option<&str> {
        self.selected_profile.as_deref()
    }

    pub(crate) fn foundry_workspace_configs(&self) -> &[FoundryWorkspaceConfig] {
        &self.foundry_workspace_configs
    }
}

mod call_hierarchy;
mod code_actions;
mod code_lens;
mod commands;
mod config;
mod diagnostics;
mod document_links;
mod documentation;
mod file_operations;
mod flycheck;
mod folding_range;
mod formatter;
mod global_state;
mod handlers;
mod import_resolution;
mod inlay_hints;
mod lifecycle;
mod natspec_completion;
mod override_index;
mod progress;
#[cfg(any(test, feature = "bench"))]
#[cfg_attr(all(feature = "bench", not(test)), allow(dead_code))]
mod project_fixture;
mod proto;
mod protocol_trace;
mod rename;
mod request_cancellation;
mod selection_range;
mod serde;
mod signature_help;
mod symbols;
mod type_hierarchy;
mod utils;
mod vfs;
mod workspace;

/// Benchmark-only access to prepared LSP projects and opaque analysis snapshots.
#[cfg(feature = "bench")]
#[doc(hidden)]
pub use global_state::benchmark::{
    BenchmarkAnalysis, BenchmarkDocumentChange, BenchmarkDocumentUpdate, BenchmarkEdit,
    BenchmarkError, BenchmarkProject, BenchmarkRequest, BenchmarkResponse,
    BenchmarkWorkspaceDiscovery, BenchmarkWorkspacePathQueries, BenchmarkWorkspaceReports,
};

/// Runs the selection-range kernel for Criterion benchmarks.
#[cfg(feature = "bench")]
#[doc(hidden)]
pub fn benchmark_selection_ranges(
    source: String,
    positions: &[lsp_types::Position],
) -> Option<Vec<lsp_types::SelectionRange>> {
    selection_range::selection_ranges(source, positions)
}

#[cfg(test)]
mod test_support;

pub(crate) type NotifyResult = ControlFlow<async_lsp::Result<()>>;

fn new_router_with_state(mut this: GlobalState) -> Router<GlobalState> {
    this.enable_background_discovery();
    let mut router = Router::new(this);

    // Lifecycle
    router
        .request::<proto::Initialize, _>(|state, params| {
            let pull_diagnostic_data_support = params.pull_diagnostic_data_support();
            state.on_initialize_with_pull_diagnostic_data(
                params.into_inner(),
                pull_diagnostic_data_support,
            )
        })
        .notification::<notif::Initialized>(GlobalState::on_initialized)
        .event::<global_state::WorkspaceDiscoveryReady>(GlobalState::on_workspace_discovery_ready)
        .event::<global_state::DeferredSourceFileEventsReady>(
            GlobalState::on_deferred_source_file_events_ready,
        )
        .event::<global_state::WatchedFileRegistrationReady>(
            GlobalState::on_watched_file_registration_ready,
        )
        .request::<req::Shutdown, _>(|_, _| std::future::ready(Ok(())))
        .notification::<notif::Exit>(|_, _| ControlFlow::Break(Ok(())));

    // Requests
    router
        .request::<req::ExecuteCommand, _>(commands::execute_command)
        .request::<req::DocumentSymbolRequest, _>(handlers::document_symbol)
        .request::<req::DocumentLinkRequest, _>(handlers::document_links)
        .request::<req::CodeActionRequest, _>(handlers::code_actions)
        .request::<req::WorkspaceSymbolRequest, _>(handlers::workspace_symbol)
        .request::<req::GotoDefinition, _>(handlers::goto_definition)
        .request::<req::GotoTypeDefinition, _>(handlers::goto_type_definition)
        .request::<req::GotoDeclaration, _>(handlers::goto_declaration)
        .request::<req::GotoImplementation, _>(handlers::goto_implementation)
        .request::<req::References, _>(handlers::references)
        .request::<req::CodeLensRequest, _>(handlers::code_lens)
        .request::<req::CallHierarchyPrepare, _>(handlers::prepare_call_hierarchy)
        .request::<req::CallHierarchyIncomingCalls, _>(handlers::call_hierarchy_incoming)
        .request::<req::CallHierarchyOutgoingCalls, _>(handlers::call_hierarchy_outgoing)
        .request::<req::DocumentHighlightRequest, _>(handlers::document_highlight)
        .request::<req::HoverRequest, _>(handlers::hover)
        .request::<req::PrepareRenameRequest, _>(handlers::prepare_rename)
        .request::<req::Rename, _>(handlers::rename)
        .request::<req::SignatureHelpRequest, _>(handlers::signature_help)
        .request::<req::InlayHintRequest, _>(handlers::inlay_hints)
        .request::<req::FoldingRangeRequest, _>(handlers::folding_range)
        .request::<req::SelectionRangeRequest, _>(handlers::selection_range)
        .request::<req::TypeHierarchyPrepare, _>(handlers::prepare_type_hierarchy)
        .request::<req::TypeHierarchySupertypes, _>(handlers::type_hierarchy_supertypes)
        .request::<req::TypeHierarchySubtypes, _>(handlers::type_hierarchy_subtypes)
        .request::<req::Completion, _>(handlers::completion)
        .request::<req::ResolveCompletionItem, _>(handlers::resolve_completion_item)
        .request::<req::DocumentDiagnosticRequest, _>(handlers::document_diagnostic)
        .request::<req::WorkspaceDiagnosticRequest, _>(handlers::workspace_diagnostic)
        .request::<req::Formatting, _>(handlers::formatting);

    // Workspace management
    router
        .request::<req::WillCreateFiles, _>(handlers::will_create_files)
        .request::<req::WillRenameFiles, _>(handlers::will_rename_files)
        .request::<req::WillDeleteFiles, _>(handlers::will_delete_files)
        .notification::<notif::DidCreateFiles>(handlers::did_create_files)
        .notification::<notif::DidRenameFiles>(handlers::did_rename_files)
        .notification::<notif::DidDeleteFiles>(handlers::did_delete_files)
        .notification::<notif::DidChangeWorkspaceFolders>(handlers::did_change_workspace_folders)
        .notification::<notif::DidChangeWatchedFiles>(handlers::did_change_watched_files);

    // Notifications
    router
        .notification::<notif::DidOpenTextDocument>(handlers::did_open_text_document)
        .notification::<notif::DidCloseTextDocument>(handlers::did_close_text_document)
        .notification::<notif::DidChangeTextDocument>(handlers::did_change_text_document)
        .notification::<notif::WillSaveTextDocument>(handlers::will_save_text_document)
        .notification::<notif::DidSaveTextDocument>(handlers::did_save_text_document)
        .notification::<notif::DidChangeConfiguration>(handlers::did_change_configuration)
        .notification::<notif::SetTrace>(GlobalState::on_set_trace)
        .notification::<notif::WorkDoneProgressCancel>(GlobalState::on_work_done_progress_cancel);

    router
}

fn request_layer(client: ClientSocket) -> request_cancellation::RequestCancellationLayer {
    request_cancellation::RequestCancellationLayer::new(client)
}

fn new_server_service_with_router<S>(
    client: ClientSocket,
    launch_config: LaunchConfig,
    new_router: impl FnOnce(GlobalState) -> S,
) -> impl LspService<Response = serde_json::Value, Error = ResponseError, Future: Send + 'static> + Send
where
    S: LspService<Response = serde_json::Value, Error = ResponseError> + Send,
    S::Future: Send + 'static,
{
    let state = GlobalState::new(client.clone()).with_launch_config(launch_config);
    let protocol_trace = state.protocol_trace();
    ServiceBuilder::new()
        .layer(TracingLayer::default())
        .layer(lifecycle::LifecycleLayer)
        .layer(protocol_trace::ProtocolTraceLayer::new(protocol_trace))
        .layer(request_layer(client.clone()))
        .layer(ClientProcessMonitorLayer::new(client))
        .service(new_router(state))
}

fn new_server_service(
    client: ClientSocket,
    launch_config: LaunchConfig,
) -> impl LspService<Response = serde_json::Value, Error = ResponseError, Future: Send + 'static> + Send
{
    new_server_service_with_router(client, launch_config, new_router_with_state)
}

/// Start the LSP server over stdin/stdout.
///
/// The caller must poll this future inside a Tokio runtime and owns all process-global setup. Once
/// polled, the server owns stdin and stdout until the LSP session exits; stdout is reserved for
/// JSON-RPC frames. Transport and protocol failures are returned to the caller.
///
/// This future is long running and will not stop until the server exits.
pub async fn launch(config: LaunchConfig) -> async_lsp::Result<()> {
    // Prefer truly asynchronous piped stdin/stdout without blocking tasks.
    #[cfg(unix)]
    let (stdin, stdout) =
        (async_lsp::stdio::PipeStdin::lock_tokio()?, async_lsp::stdio::PipeStdout::lock_tokio()?);

    // Fallback to spawn blocking read/write otherwise.
    #[cfg(not(unix))]
    let (stdin, stdout) = (
        tokio_util::compat::TokioAsyncReadCompatExt::compat(tokio::io::stdin()),
        tokio_util::compat::TokioAsyncWriteCompatExt::compat_write(tokio::io::stdout()),
    );

    let (eloop, _) =
        async_lsp::MainLoop::new_server(move |client| new_server_service(client, config));

    eloop.run_buffered(stdin, stdout).await
}

#[cfg(test)]
#[path = "tests/router.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/flycheck.rs"]
mod flycheck_tests;

#[cfg(test)]
#[path = "tests/launch.rs"]
mod launch_tests;

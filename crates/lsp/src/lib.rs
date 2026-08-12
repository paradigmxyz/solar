#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(docsrs, feature(doc_cfg))]

use crate::global_state::GlobalState;
use async_lsp::{
    ClientSocket, LspService, ResponseError, client_monitor::ClientProcessMonitorLayer,
    router::Router, server::LifecycleLayer, tracing::TracingLayer,
};
#[cfg(test)]
use criterion as _;
use lsp_types::{notification as notif, request as req};
use serde_json as _;
use solar_config::LspArgs;
use std::ops::ControlFlow;
use tower::ServiceBuilder;

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
mod inlay_hints;
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
    BenchmarkWorkspaceReports,
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

fn new_router_with_state(this: GlobalState) -> Router<GlobalState> {
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

fn new_server_service(
    client: ClientSocket,
) -> impl LspService<Response = serde_json::Value, Error = ResponseError, Future: Send + 'static> + Send
{
    let state = GlobalState::new(client.clone());
    let protocol_trace = state.protocol_trace();
    ServiceBuilder::new()
        .layer(TracingLayer::default())
        .layer(LifecycleLayer::default())
        .layer(protocol_trace::ProtocolTraceLayer::new(protocol_trace))
        .layer(request_layer(client.clone()))
        .layer(ClientProcessMonitorLayer::new(client))
        .service(new_router_with_state(state))
}

/// Start the LSP server over stdin/stdout.
///
/// This future is long running and will not stop until the server exits.
pub async fn run_server_stdio(_args: LspArgs) -> async_lsp::Result<()> {
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

    let (eloop, _) = async_lsp::MainLoop::new_server(new_server_service);

    eloop.run_buffered(stdin, stdout).await
}

#[cfg(test)]
#[path = "tests/router.rs"]
mod tests;

#[cfg(test)]
#[path = "tests/flycheck.rs"]
mod flycheck_tests;

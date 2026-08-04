use super::workspace_edit::{validated_code_actions, validated_rename_workspace_edit};
use crate::{
    diagnostics::PullReport,
    formatter::{self, FormatterError},
    global_state::GlobalState,
    natspec_completion::{self, NatSpecCompletionResult},
    progress::send_progress,
    symbols::{CompletionContext, CompletionItemData, SymbolTables},
    vfs::{Vfs, VfsPath},
};
use async_lsp::{ClientSocket, ErrorCode, ResponseError};
use crop::Rope;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeActionParams, CodeActionResponse, CodeLens, CodeLensParams, CompletionItem,
    CompletionParams, CompletionResponse, DocumentDiagnosticParams, DocumentDiagnosticReport,
    DocumentDiagnosticReportResult, DocumentFormattingParams, DocumentHighlight,
    DocumentHighlightParams, DocumentLink, DocumentLinkParams, DocumentSymbolParams,
    DocumentSymbolResponse, FoldingRange, FoldingRangeParams, FullDocumentDiagnosticReport,
    GotoDefinitionParams, GotoDefinitionResponse, Hover, HoverParams, InlayHint, InlayHintParams,
    Position, PrepareRenameResponse, ProgressToken, ReferenceParams,
    RelatedFullDocumentDiagnosticReport, RelatedUnchangedDocumentDiagnosticReport, RenameParams,
    SelectionRange, SelectionRangeParams, SignatureHelp, SignatureHelpParams,
    TextDocumentPositionParams, TextEdit, TypeHierarchyItem, TypeHierarchyPrepareParams,
    TypeHierarchySubtypesParams, TypeHierarchySupertypesParams, UnchangedDocumentDiagnosticReport,
    Url, WorkDoneProgress, WorkDoneProgressBegin, WorkDoneProgressEnd, WorkspaceDiagnosticParams,
    WorkspaceDiagnosticReport, WorkspaceDiagnosticReportPartialResult,
    WorkspaceDiagnosticReportResult, WorkspaceDocumentDiagnosticReport, WorkspaceEdit,
    WorkspaceFullDocumentDiagnosticReport, WorkspaceSymbolParams, WorkspaceSymbolResponse,
    WorkspaceUnchangedDocumentDiagnosticReport,
    notification::{Notification, Progress},
    request::GotoImplementationParams,
};
use serde::{Deserialize, Serialize};
use solar_interface::data_structures::sync::RwLock;
use solar_parse::lexer::is_ident;
use std::{future::ready, io, path::Path, sync::Arc};
use tracing::warn;

const WORKSPACE_DIAGNOSTIC_PARTIAL_BATCH_SIZE: usize = 64;
const WORKSPACE_DIAGNOSTIC_PROGRESS_TITLE: &str = "Workspace diagnostics";

#[derive(Debug)]
enum WorkspaceDiagnosticProgress {}

impl Notification for WorkspaceDiagnosticProgress {
    type Params = WorkspaceDiagnosticProgressParams;
    const METHOD: &'static str = Progress::METHOD;
}

#[derive(Deserialize, Serialize)]
struct WorkspaceDiagnosticProgressParams {
    token: ProgressToken,
    value: WorkspaceDiagnosticReportPartialResult,
}

struct RequestWorkDoneProgress {
    client: ClientSocket,
    token: Option<ProgressToken>,
}

impl RequestWorkDoneProgress {
    fn begin(client: ClientSocket, token: Option<ProgressToken>) -> Self {
        if let Some(token) = &token {
            send_progress(
                &client,
                token,
                WorkDoneProgress::Begin(WorkDoneProgressBegin {
                    title: WORKSPACE_DIAGNOSTIC_PROGRESS_TITLE.into(),
                    cancellable: Some(false),
                    message: None,
                    percentage: None,
                }),
            );
        }
        Self { client, token }
    }
}

impl Drop for RequestWorkDoneProgress {
    fn drop(&mut self) {
        let Some(token) = &self.token else { return };
        send_progress(&self.client, token, WorkDoneProgress::End(WorkDoneProgressEnd::default()));
    }
}

pub(crate) fn folding_range(
    state: &mut GlobalState,
    params: FoldingRangeParams,
) -> impl Future<Output = Result<Option<Vec<FoldingRange>>, ResponseError>> + use<> {
    let vfs = state.vfs.clone();
    let request = params
        .text_document
        .uri
        .to_file_path()
        .ok()
        .map(|path| (VfsPath::from(path.clone()), path));

    async move {
        let Some((vfs_path, path)) = request else { return Ok(None) };
        let source = match document_contents(&vfs, &vfs_path, &path).await {
            Ok(source) => source,
            Err(error) => {
                warn!(%error, "failed to read document");
                return Ok(None);
            }
        };
        let ranges =
            tokio::task::spawn_blocking(move || crate::folding_range::folding_ranges(source))
                .await
                .map_err(folding_range_task_failed)?;
        Ok(Some(ranges))
    }
}

pub(crate) fn selection_range(
    state: &mut GlobalState,
    params: SelectionRangeParams,
) -> impl Future<Output = Result<Option<Vec<SelectionRange>>, ResponseError>> + use<> {
    let vfs = state.vfs.clone();
    let request = params
        .text_document
        .uri
        .to_file_path()
        .map_err(|_| request_failed("document URI is not a file"))
        .map(|path| (VfsPath::from(path.clone()), path, params.positions));

    async move {
        let (vfs_path, path, positions) = request?;
        let source =
            document_contents(&vfs, &vfs_path, &path).await.map_err(document_read_failed)?;
        let ranges = tokio::task::spawn_blocking(move || {
            crate::selection_range::selection_ranges(source, &positions)
        })
        .await
        .map_err(selection_range_task_failed)?
        .ok_or_else(|| {
            ResponseError::new(ErrorCode::INVALID_PARAMS, "invalid selection range position")
        })?;
        Ok(Some(ranges))
    }
}

pub(crate) fn formatting(
    state: &mut GlobalState,
    params: DocumentFormattingParams,
) -> impl Future<Output = Result<Option<Vec<TextEdit>>, ResponseError>> + use<> {
    let vfs = state.vfs.clone();
    let request = params
        .text_document
        .uri
        .to_file_path()
        .map_err(|_| request_failed("document URI is not a file"))
        .and_then(|path| {
            let Some(root) = state.config.formatter_root_for_path(&path) else {
                return Err(request_failed("document has no parent directory"));
            };
            Ok((
                VfsPath::from(path.clone()),
                path,
                root,
                state.config.forge_path(),
                state.config.formatter_timeout(),
            ))
        });

    async move {
        let (vfs_path, path, root, forge, timeout) = request?;
        if formatter::is_ignored(&forge, &path, &root, timeout).await.map_err(formatter_failed)? {
            return Ok(None);
        }
        let source =
            document_contents(&vfs, &vfs_path, &path).await.map_err(document_read_failed)?;
        let formatted =
            formatter::run(&forge, &root, &source, timeout).await.map_err(formatter_failed)?;
        let is_current = document_is_current(&vfs, &vfs_path, &path, &source)
            .await
            .map_err(document_read_failed)?;
        if !is_current {
            return Err(ResponseError::new(
                ErrorCode::CONTENT_MODIFIED,
                "document changed during formatting",
            ));
        }

        Ok(formatting_edits(&source, formatted))
    }
}

async fn document_contents(
    vfs: &Arc<RwLock<Vfs>>,
    vfs_path: &VfsPath,
    path: &Path,
) -> io::Result<String> {
    let contents = { vfs.read().get_file_contents(vfs_path).cloned() };
    if let Some(contents) = contents {
        return Ok(rope_to_string(&contents));
    }

    tokio::fs::read_to_string(path).await
}

async fn document_is_current(
    vfs: &Arc<RwLock<Vfs>>,
    vfs_path: &VfsPath,
    path: &Path,
    source: &str,
) -> io::Result<bool> {
    let contents = { vfs.read().get_file_contents(vfs_path).cloned() };
    if let Some(contents) = contents {
        return Ok(contents == source);
    }

    Ok(tokio::fs::read_to_string(path).await? == source)
}

fn rope_to_string(contents: &Rope) -> String {
    let mut string = String::with_capacity(contents.byte_len());
    for chunk in contents.chunks() {
        string.push_str(chunk);
    }
    string
}

fn document_read_failed(error: io::Error) -> ResponseError {
    warn!(%error, "failed to read document");
    request_failed("failed to read document")
}

fn folding_range_task_failed(error: tokio::task::JoinError) -> ResponseError {
    warn!(%error, "folding-range task failed");
    ResponseError::new(ErrorCode::INTERNAL_ERROR, "folding-range task failed")
}

fn selection_range_task_failed(error: tokio::task::JoinError) -> ResponseError {
    warn!(%error, "selection-range task failed");
    ResponseError::new(ErrorCode::INTERNAL_ERROR, "selection-range task failed")
}

fn formatter_failed(error: FormatterError) -> ResponseError {
    warn!(%error, "document formatting failed");
    let message = match &error {
        FormatterError::Timeout => "Forge formatting timed out",
        FormatterError::ConfigTimeout => "Forge config resolution timed out",
        FormatterError::Io(error) if error.kind() == io::ErrorKind::NotFound => {
            "Forge executable was not found"
        }
        FormatterError::Io(_) => "failed to run Forge formatter",
        FormatterError::Failed { .. } => "Forge formatting failed",
        FormatterError::ConfigFailed { .. } => "Forge config resolution failed",
        FormatterError::InvalidConfig(_) => "Forge returned invalid config",
        FormatterError::InvalidUtf8(_) => "Forge returned invalid UTF-8",
        FormatterError::EmptyOutput => "Forge formatter returned empty output",
    };
    request_failed(message)
}

fn request_failed(message: &'static str) -> ResponseError {
    ResponseError::new(ErrorCode::REQUEST_FAILED, message)
}

fn latest_analysis_for_uri(
    state: &GlobalState,
    uri: &Url,
) -> Option<impl Future<Output = Result<Arc<RwLock<SymbolTables>>, ResponseError>> + use<>> {
    crate::proto::vfs_path(uri)?;
    Some(state.latest_analysis())
}

fn formatting_edits(source: &str, formatted: String) -> Option<Vec<TextEdit>> {
    if source == formatted {
        return None;
    }

    Some(vec![TextEdit {
        range: lsp_types::Range::new(Position::new(0, 0), end_position(source)),
        new_text: formatted,
    }])
}

fn end_position(source: &str) -> Position {
    let mut line = 0;
    let mut character = 0;
    let mut chars = source.chars().peekable();
    while let Some(char) = chars.next() {
        match char {
            '\r' => {
                if chars.peek() == Some(&'\n') {
                    chars.next();
                }
                line += 1;
                character = 0;
            }
            '\n' => {
                line += 1;
                character = 0;
            }
            char => character += char.len_utf16() as u32,
        }
    }
    Position::new(line, character)
}

pub(crate) fn document_symbol(
    state: &mut GlobalState,
    params: DocumentSymbolParams,
) -> impl Future<Output = Result<Option<DocumentSymbolResponse>, ResponseError>> + use<> {
    let hierarchical = state.config.supports_hierarchical_document_symbols();
    let uri = params.text_document.uri;
    let latest_analysis = latest_analysis_for_uri(state, &uri);
    async move {
        let Some(latest_analysis) = latest_analysis else {
            let response = if hierarchical {
                DocumentSymbolResponse::Nested(Vec::new())
            } else {
                DocumentSymbolResponse::Flat(Vec::new())
            };
            return Ok(Some(response));
        };
        let symbol_tables = latest_analysis.await?;
        let response = if hierarchical {
            DocumentSymbolResponse::Nested(symbol_tables.read().document_symbols(&uri))
        } else {
            DocumentSymbolResponse::Flat(symbol_tables.read().flat_document_symbols(&uri))
        };
        Ok(Some(response))
    }
}

pub(crate) fn document_links(
    state: &mut GlobalState,
    params: DocumentLinkParams,
) -> impl Future<Output = Result<Option<Vec<DocumentLink>>, ResponseError>> + use<> {
    let request =
        params.text_document.uri.to_file_path().ok().map(|path| (path, state.latest_analysis()));
    async move {
        let Some((path, latest_analysis)) = request else { return Ok(Some(Vec::new())) };
        let symbol_tables = latest_analysis.await?;
        let links = symbol_tables.read().document_links(&path);
        Ok(Some(links))
    }
}

pub(crate) fn code_actions(
    state: &mut GlobalState,
    mut params: CodeActionParams,
) -> impl Future<Output = Result<Option<CodeActionResponse>, ResponseError>> + use<> {
    params.text_document.uri = crate::diagnostics::normalize_file_uri(params.text_document.uri);
    let vfs = state.vfs.clone();
    let document_changes = state.config.supports_workspace_edit_document_changes();
    let literals = state.config.supports_code_action_literals();
    let is_preferred = state.config.supports_code_action_is_preferred();
    let diagnostic_data = state.config.supports_code_action_diagnostic_data();
    let diagnostics = state.code_action_diagnostics(params.text_document.uri.clone());
    async move {
        if !literals {
            return Ok(Some(Vec::new()));
        }
        let diagnostics = diagnostics.await?;
        let actions = tokio::task::spawn_blocking(move || {
            validated_code_actions(
                params,
                diagnostics,
                vfs,
                document_changes,
                is_preferred,
                diagnostic_data,
            )
        })
        .await
        .map_err(|error| {
            ResponseError::new(
                ErrorCode::INTERNAL_ERROR,
                format!("code-action task failed: {error}"),
            )
        })?;
        Ok(Some(actions))
    }
}

pub(crate) fn document_diagnostic(
    state: &mut GlobalState,
    params: DocumentDiagnosticParams,
) -> impl Future<Output = Result<DocumentDiagnosticReportResult, ResponseError>> + use<> {
    let report = state.pull_diagnostic_report(params.text_document.uri, params.previous_result_id);
    async move {
        let report = match report.await? {
            PullReport::Full { result_id, diagnostics } => {
                DocumentDiagnosticReport::Full(RelatedFullDocumentDiagnosticReport {
                    related_documents: None,
                    full_document_diagnostic_report: FullDocumentDiagnosticReport {
                        result_id: Some(result_id),
                        items: diagnostics,
                    },
                })
            }
            PullReport::Unchanged { result_id } => {
                DocumentDiagnosticReport::Unchanged(RelatedUnchangedDocumentDiagnosticReport {
                    related_documents: None,
                    unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                        result_id,
                    },
                })
            }
        };
        Ok(DocumentDiagnosticReportResult::Report(report))
    }
}

pub(crate) fn workspace_diagnostic(
    state: &mut GlobalState,
    params: WorkspaceDiagnosticParams,
) -> impl Future<Output = Result<WorkspaceDiagnosticReportResult, ResponseError>> + use<> {
    let client = state.client_socket();
    let reports = state.workspace_diagnostic_reports(params.previous_result_ids);
    let partial_result_token = params.partial_result_params.partial_result_token;
    let work_done_token = params.work_done_progress_params.work_done_token;
    async move {
        let _work_done = RequestWorkDoneProgress::begin(client.clone(), work_done_token);
        let items = reports.await?.into_iter().map(workspace_document_diagnostic_report);
        let items =
            stream_workspace_diagnostic_partials(&client, partial_result_token, items).await;
        Ok(WorkspaceDiagnosticReport { items }.into())
    }
}

fn workspace_document_diagnostic_report(
    report: crate::diagnostics::WorkspacePullReport,
) -> WorkspaceDocumentDiagnosticReport {
    match report.report {
        PullReport::Full { result_id, diagnostics } => {
            WorkspaceDocumentDiagnosticReport::Full(WorkspaceFullDocumentDiagnosticReport {
                uri: report.uri,
                version: report.version,
                full_document_diagnostic_report: FullDocumentDiagnosticReport {
                    result_id: Some(result_id),
                    items: diagnostics,
                },
            })
        }
        PullReport::Unchanged { result_id } => WorkspaceDocumentDiagnosticReport::Unchanged(
            WorkspaceUnchangedDocumentDiagnosticReport {
                uri: report.uri,
                version: report.version,
                unchanged_document_diagnostic_report: UnchangedDocumentDiagnosticReport {
                    result_id,
                },
            },
        ),
    }
}

async fn stream_workspace_diagnostic_partials(
    client: &ClientSocket,
    token: Option<ProgressToken>,
    items: impl Iterator<Item = WorkspaceDocumentDiagnosticReport>,
) -> Vec<WorkspaceDocumentDiagnosticReport> {
    let Some(token) = token else { return items.collect() };
    let mut items = items.peekable();
    loop {
        let batch =
            items.by_ref().take(WORKSPACE_DIAGNOSTIC_PARTIAL_BATCH_SIZE).collect::<Vec<_>>();
        if batch.is_empty() {
            break;
        }
        let _ = client.notify::<WorkspaceDiagnosticProgress>(WorkspaceDiagnosticProgressParams {
            token: token.clone(),
            value: WorkspaceDiagnosticReportPartialResult { items: batch },
        });
        if items.peek().is_some() {
            // Let the socket writer and request cancellation run between batches.
            tokio::task::yield_now().await;
        }
    }
    Vec::new()
}

pub(crate) fn workspace_symbol(
    state: &mut GlobalState,
    params: WorkspaceSymbolParams,
) -> impl Future<Output = Result<Option<WorkspaceSymbolResponse>, ResponseError>> + use<> {
    let symbols = state.symbol_tables.read().workspace_symbols(&params.query);
    ready(Ok(Some(WorkspaceSymbolResponse::Nested(symbols))))
}

pub(crate) fn prepare_type_hierarchy(
    state: &mut GlobalState,
    params: TypeHierarchyPrepareParams,
) -> impl Future<Output = Result<Option<Vec<TypeHierarchyItem>>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let uri = params.text_document.uri;
    let latest_analysis = latest_analysis_for_uri(state, &uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().prepare_type_hierarchy(&uri, params.position);
        Ok(response)
    }
}

pub(crate) fn type_hierarchy_supertypes(
    state: &mut GlobalState,
    params: TypeHierarchySupertypesParams,
) -> impl Future<Output = Result<Option<Vec<TypeHierarchyItem>>, ResponseError>> + use<> {
    let latest_analysis = latest_analysis_for_uri(state, &params.item.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().type_hierarchy_supertypes(&params.item);
        Ok(response)
    }
}

pub(crate) fn type_hierarchy_subtypes(
    state: &mut GlobalState,
    params: TypeHierarchySubtypesParams,
) -> impl Future<Output = Result<Option<Vec<TypeHierarchyItem>>, ResponseError>> + use<> {
    let latest_analysis = latest_analysis_for_uri(state, &params.item.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().type_hierarchy_subtypes(&params.item);
        Ok(response)
    }
}

pub(crate) fn goto_definition(
    state: &mut GlobalState,
    params: GotoDefinitionParams,
) -> impl Future<Output = Result<Option<GotoDefinitionResponse>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response =
            symbol_tables.read().goto_definition(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn goto_type_definition(
    state: &mut GlobalState,
    params: GotoDefinitionParams,
) -> impl Future<Output = Result<Option<GotoDefinitionResponse>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response =
            symbol_tables.read().goto_type_definition(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn goto_declaration(
    state: &mut GlobalState,
    params: GotoDefinitionParams,
) -> impl Future<Output = Result<Option<GotoDefinitionResponse>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response =
            symbol_tables.read().goto_declaration(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn goto_implementation(
    state: &mut GlobalState,
    params: GotoImplementationParams,
) -> impl Future<Output = Result<Option<GotoDefinitionResponse>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response =
            symbol_tables.read().goto_implementation(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn prepare_call_hierarchy(
    state: &mut GlobalState,
    params: CallHierarchyPrepareParams,
) -> impl Future<Output = Result<Option<Vec<CallHierarchyItem>>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response =
            symbol_tables.read().prepare_call_hierarchy(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn call_hierarchy_incoming(
    state: &mut GlobalState,
    params: CallHierarchyIncomingCallsParams,
) -> impl Future<Output = Result<Option<Vec<CallHierarchyIncomingCall>>, ResponseError>> + use<> {
    let item = params.item;
    let latest_analysis = latest_analysis_for_uri(state, &item.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().call_hierarchy_incoming(&item);
        Ok(response)
    }
}

pub(crate) fn call_hierarchy_outgoing(
    state: &mut GlobalState,
    params: CallHierarchyOutgoingCallsParams,
) -> impl Future<Output = Result<Option<Vec<CallHierarchyOutgoingCall>>, ResponseError>> + use<> {
    let item = params.item;
    let latest_analysis = latest_analysis_for_uri(state, &item.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().call_hierarchy_outgoing(&item);
        Ok(response)
    }
}

pub(crate) fn references(
    state: &mut GlobalState,
    params: ReferenceParams,
) -> impl Future<Output = Result<Option<Vec<lsp_types::Location>>, ResponseError>> + use<> {
    let include_declaration = params.context.include_declaration;
    let params = params.text_document_position;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().references(
            &params.text_document.uri,
            params.position,
            include_declaration,
        );
        Ok(response)
    }
}

pub(crate) fn code_lens(
    state: &mut GlobalState,
    params: CodeLensParams,
) -> impl Future<Output = Result<Option<Vec<CodeLens>>, ResponseError>> + use<> {
    let uri = params.text_document.uri;
    let options = state.config.code_lens_options();
    let latest_analysis =
        if options.is_active() { latest_analysis_for_uri(state, &uri) } else { None };
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(Some(Vec::new())) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().code_lenses(&uri, options);
        Ok(Some(response))
    }
}

pub(crate) fn document_highlight(
    state: &mut GlobalState,
    params: DocumentHighlightParams,
) -> impl Future<Output = Result<Option<Vec<DocumentHighlight>>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response =
            symbol_tables.read().document_highlights(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn hover(
    state: &mut GlobalState,
    params: HoverParams,
) -> impl Future<Output = Result<Option<Hover>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().hover(&params.text_document.uri, params.position);
        Ok(response)
    }
}

pub(crate) fn prepare_rename(
    state: &mut GlobalState,
    params: TextDocumentPositionParams,
) -> impl Future<Output = Result<Option<PrepareRenameResponse>, ResponseError>> + use<> {
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables
            .read()
            .rename_candidate(&params.text_document.uri, params.position)
            .map(|candidate| PrepareRenameResponse::Range(candidate.range));
        Ok(response)
    }
}

pub(crate) fn rename(
    state: &mut GlobalState,
    params: RenameParams,
) -> impl Future<Output = Result<Option<WorkspaceEdit>, ResponseError>> + use<> {
    let RenameParams { text_document_position: params_position, new_name, .. } = params;
    let (invalid_name, invalid_yul_name) = if is_ident(&new_name) {
        let name = state.sess.intern(&new_name);
        (name.is_reserved(false), name.is_reserved(true))
    } else {
        (true, true)
    };
    let latest_analysis = if invalid_name {
        None
    } else {
        latest_analysis_for_uri(state, &params_position.text_document.uri)
    };
    let vfs = state.vfs.clone();
    let document_changes = state.config.supports_workspace_edit_document_changes();
    async move {
        if invalid_name {
            return Err(ResponseError::new(ErrorCode::INVALID_PARAMS, "invalid rename name"));
        }

        let Some(latest_analysis) = latest_analysis else { return Ok(None) };
        let symbol_tables = latest_analysis.await?;
        let candidate = symbol_tables
            .read()
            .rename_candidate(&params_position.text_document.uri, params_position.position);
        let Some(candidate) = candidate else { return Ok(None) };
        if candidate.requires_yul_validation && invalid_yul_name {
            return Err(ResponseError::new(ErrorCode::INVALID_PARAMS, "invalid rename name"));
        }
        if candidate.old_name == new_name {
            return Ok(None);
        }

        tokio::task::spawn_blocking(move || {
            validated_rename_workspace_edit(candidate, new_name, vfs, document_changes)
        })
        .await
        .map_err(|error| {
            ResponseError::new(ErrorCode::INTERNAL_ERROR, format!("rename task failed: {error}"))
        })?
        .map(Some)
    }
}

pub(crate) fn inlay_hints(
    state: &mut GlobalState,
    params: InlayHintParams,
) -> impl Future<Output = Result<Option<Vec<InlayHint>>, ResponseError>> + use<> {
    let latest_analysis = latest_analysis_for_uri(state, &params.text_document.uri);
    async move {
        let Some(latest_analysis) = latest_analysis else { return Ok(Some(Vec::new())) };
        let symbol_tables = latest_analysis.await?;
        let response = symbol_tables.read().inlay_hints(&params.text_document.uri, params.range);
        Ok(Some(response))
    }
}

pub(crate) fn signature_help(
    state: &mut GlobalState,
    params: SignatureHelpParams,
) -> impl Future<Output = Result<Option<SignatureHelp>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let response = crate::proto::vfs_path(&params.text_document.uri).and_then(|path| {
        let contents = state.vfs.read().get_file_contents(&path)?.clone();
        state.symbol_tables.read().signature_help(
            &params.text_document.uri,
            params.position,
            &contents,
            state.config.signature_help_options(),
        )
    });
    ready(Ok(response))
}

pub(crate) fn completion(
    state: &mut GlobalState,
    params: CompletionParams,
) -> impl Future<Output = Result<Option<CompletionResponse>, ResponseError>> + use<> {
    let trigger_character =
        params.context.as_ref().and_then(|context| context.trigger_character.as_deref());
    let params = params.text_document_position;
    let contents = crate::proto::vfs_path(&params.text_document.uri)
        .and_then(|path| state.vfs.read().get_file_contents(&path).cloned());
    if let Some(contents) = contents {
        match natspec_completion::target(&contents, params.position) {
            NatSpecCompletionResult::Claimed(target) => {
                let items = target.map_or_else(Vec::new, |target| {
                    let semantics = state
                        .natspec_semantics_are_usable(&params.text_document.uri)
                        .then(|| {
                            state
                                .symbol_tables
                                .read()
                                .natspec_semantics(
                                    &params.text_document.uri,
                                    target.source_fingerprint(),
                                    target.key(),
                                )
                                .cloned()
                        })
                        .flatten();
                    target.completion_items(state.config.completion_options(), semantics.as_ref())
                });
                return ready(Ok(Some(CompletionResponse::Array(items))));
            }
            NatSpecCompletionResult::NotApplicable => {}
        }
    }
    if matches!(trigger_character, Some("/" | "*")) {
        return ready(Ok(Some(CompletionResponse::Array(Vec::new()))));
    }
    let input = completion_input(state, &params.text_document.uri, params.position);
    let context = input.as_ref().map(CompletionInput::context).unwrap_or_default();
    let options = state.config.completion_options();
    let symbol_tables = state.symbol_tables.read();
    let mut items =
        symbol_tables.completion_items(&params.text_document.uri, params.position, context);
    if !options.resolve_documentation {
        symbol_tables.resolve_completion_items(&mut items, options.markdown_documentation);
    }
    ready(Ok(Some(CompletionResponse::Array(items))))
}

pub(crate) fn resolve_completion_item(
    state: &mut GlobalState,
    item: CompletionItem,
) -> impl Future<Output = Result<CompletionItem, ResponseError>> + use<> {
    let options = state.config.completion_options();
    let request = if options.resolve_documentation {
        CompletionItemData::from_item(&item).and_then(|data| {
            let latest_analysis = latest_analysis_for_uri(state, data.uri())?;
            Some((data, latest_analysis))
        })
    } else {
        None
    };
    async move {
        let Some((data, latest_analysis)) = request else { return Ok(item) };
        let symbol_tables = latest_analysis.await?;
        let resolved = symbol_tables.read().resolve_completion_item(
            item,
            data,
            options.markdown_documentation,
        );
        Ok(resolved)
    }
}

struct CompletionInput {
    prefix: String,
    member_receiver: Option<String>,
}

impl CompletionInput {
    fn context(&self) -> CompletionContext<'_> {
        CompletionContext::new(&self.prefix, self.member_receiver.as_deref())
    }
}

fn completion_input(state: &GlobalState, uri: &Url, position: Position) -> Option<CompletionInput> {
    let path = crate::proto::vfs_path(uri)?;
    let vfs = state.vfs.read();
    let line = line_at(vfs.get_file_contents(&path)?, position.line as usize)?;
    let line_prefix = line_prefix_at(&line, position)?;
    Some(completion_input_from_line_prefix(line_prefix))
}

fn line_at(contents: &Rope, line: usize) -> Option<String> {
    (line < contents.line_len()).then(|| contents.line(line).to_string())
}

fn line_prefix_at(contents: &str, position: Position) -> Option<&str> {
    let line = contents.strip_suffix('\r').unwrap_or(contents);
    let target = position.character as usize;
    let mut utf16 = 0;
    for (idx, ch) in line.char_indices() {
        if utf16 >= target {
            return Some(&line[..idx]);
        }
        utf16 += ch.len_utf16();
    }
    Some(line)
}

fn completion_input_from_line_prefix(line_prefix: &str) -> CompletionInput {
    let prefix_start = start_of_trailing_ident(line_prefix);
    let prefix = line_prefix[prefix_start..].to_string();
    let before_prefix = &line_prefix[..prefix_start];
    let member_receiver = before_prefix.strip_suffix('.').and_then(|before_dot| {
        let receiver_start = start_of_trailing_ident(before_dot);
        let receiver = &before_dot[receiver_start..];
        (!receiver.is_empty()).then(|| receiver.to_string())
    });
    CompletionInput { prefix, member_receiver }
}

fn start_of_trailing_ident(s: &str) -> usize {
    s.char_indices()
        .rev()
        .find(|(_, ch)| !is_completion_ident_char(*ch))
        .map_or(0, |(idx, ch)| idx + ch.len_utf8())
}

fn is_completion_ident_char(ch: char) -> bool {
    ch == '_' || ch == '$' || ch.is_ascii_alphanumeric()
}

#[cfg(test)]
#[path = "../tests/requests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/formatting.rs"]
mod formatting_tests;

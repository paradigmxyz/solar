use crate::{
    formatter::{self, FormatterError},
    global_state::GlobalState,
    symbols::CompletionContext,
    vfs::{Vfs, VfsPath},
};
use async_lsp::{ErrorCode, ResponseError};
use crop::Rope;
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentFormattingParams, DocumentSymbolParams,
    DocumentSymbolResponse, GotoDefinitionParams, GotoDefinitionResponse, InlayHint,
    InlayHintParams, Position, ReferenceParams, SignatureHelp, SignatureHelpParams, TextEdit, Url,
    WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use solar_interface::data_structures::sync::RwLock;
use std::{future::ready, io, path::Path, sync::Arc};
use tracing::warn;

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
            Ok((VfsPath::from(path.clone()), path, root, state.config.forge_path()))
        });

    async move {
        let (vfs_path, path, root, forge) = request?;
        if formatter::is_ignored(&path, &root) {
            return Ok(None);
        }
        let source =
            document_contents(&vfs, &vfs_path, &path).await.map_err(document_read_failed)?;
        let formatted = formatter::run(&forge, &root, &source).await.map_err(formatter_failed)?;
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
    warn!(%error, "failed to read document for formatting");
    request_failed("failed to read document")
}

fn formatter_failed(error: FormatterError) -> ResponseError {
    warn!(%error, "document formatting failed");
    let message = match &error {
        FormatterError::Timeout => "Forge formatting timed out",
        FormatterError::Io(error) if error.kind() == io::ErrorKind::NotFound => {
            "Forge executable was not found"
        }
        FormatterError::Io(_) => "failed to run Forge formatter",
        FormatterError::Failed { .. } => "Forge formatting failed",
        FormatterError::InvalidUtf8(_) => "Forge returned invalid UTF-8",
        FormatterError::EmptyOutput => "Forge formatter returned empty output",
    };
    request_failed(message)
}

fn request_failed(message: &'static str) -> ResponseError {
    ResponseError::new(ErrorCode::REQUEST_FAILED, message)
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
    let symbol_tables = state.symbol_tables.read();
    let response = if state.config.supports_hierarchical_document_symbols() {
        DocumentSymbolResponse::Nested(symbol_tables.document_symbols(&params.text_document.uri))
    } else {
        DocumentSymbolResponse::Flat(symbol_tables.flat_document_symbols(&params.text_document.uri))
    };
    ready(Ok(Some(response)))
}

pub(crate) fn workspace_symbol(
    state: &mut GlobalState,
    params: WorkspaceSymbolParams,
) -> impl Future<Output = Result<Option<WorkspaceSymbolResponse>, ResponseError>> + use<> {
    let symbols = state.symbol_tables.read().workspace_symbols(&params.query);
    ready(Ok(Some(WorkspaceSymbolResponse::Nested(symbols))))
}

pub(crate) fn goto_definition(
    state: &mut GlobalState,
    params: GotoDefinitionParams,
) -> impl Future<Output = Result<Option<GotoDefinitionResponse>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let response =
        state.symbol_tables.read().goto_definition(&params.text_document.uri, params.position);
    ready(Ok(response))
}

pub(crate) fn goto_declaration(
    state: &mut GlobalState,
    params: GotoDefinitionParams,
) -> impl Future<Output = Result<Option<GotoDefinitionResponse>, ResponseError>> + use<> {
    let params = params.text_document_position_params;
    let response =
        state.symbol_tables.read().goto_declaration(&params.text_document.uri, params.position);
    ready(Ok(response))
}

pub(crate) fn references(
    state: &mut GlobalState,
    params: ReferenceParams,
) -> impl Future<Output = Result<Option<Vec<lsp_types::Location>>, ResponseError>> + use<> {
    let include_declaration = params.context.include_declaration;
    let params = params.text_document_position;
    let response = state.symbol_tables.read().references(
        &params.text_document.uri,
        params.position,
        include_declaration,
    );
    ready(Ok(response))
}

pub(crate) fn inlay_hints(
    state: &mut GlobalState,
    params: InlayHintParams,
) -> impl Future<Output = Result<Option<Vec<InlayHint>>, ResponseError>> + use<> {
    let response = state.symbol_tables.read().inlay_hints(&params.text_document.uri, params.range);
    ready(Ok(Some(response)))
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
    let params = params.text_document_position;
    let input = completion_input(state, &params.text_document.uri, params.position);
    let context = input.as_ref().map(CompletionInput::context).unwrap_or_default();
    let items = state.symbol_tables.read().completion_items(
        &params.text_document.uri,
        params.position,
        context,
    );
    ready(Ok(Some(CompletionResponse::Array(items))))
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
mod tests {
    use super::*;
    use crate::{
        config::negotiate_capabilities,
        symbols::{SymbolTables, push_symbol_for_test as push},
    };
    use async_lsp::ClientSocket;
    use lsp_types::{
        DocumentSymbolClientCapabilities, DocumentSymbolResponse, InitializeParams,
        PartialResultParams, SymbolKind, TextDocumentClientCapabilities, TextDocumentIdentifier,
        Url, WorkDoneProgressParams, WorkspaceSymbolResponse,
    };
    use std::{
        future::Future,
        sync::Arc,
        task::{Context, Poll, Waker},
    };

    #[test]
    fn completion_input_extracts_prefix_and_member_receiver() {
        assert_completion_input("        ms", "ms", None);
        assert_completion_input("        msg.", "", Some("msg"));
        assert_completion_input("        msg.s", "s", Some("msg"));
        assert_completion_input("        getToken().", "", None);
    }

    #[test]
    fn document_symbol_returns_flat_symbols_without_hierarchical_client_support() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state =
            state_with_symbols(symbol_tables(&uri, &other_uri), InitializeParams::default());

        let response = expect_ready(document_symbol(&mut state, document_symbol_params(uri)))
            .unwrap()
            .unwrap();

        let DocumentSymbolResponse::Flat(symbols) = response else {
            panic!("expected flat document symbols");
        };
        assert_eq!(
            symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["C", "x", "f"]
        );
        assert_eq!(symbols[0].container_name, None);
        assert_eq!(symbols[1].container_name.as_deref(), Some("C"));
    }

    #[test]
    fn document_symbol_returns_nested_symbols_with_hierarchical_client_support() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state = state_with_symbols(
            symbol_tables(&uri, &other_uri),
            initialize_params_with_hierarchical_document_symbols(),
        );

        let response = expect_ready(document_symbol(&mut state, document_symbol_params(uri)))
            .unwrap()
            .unwrap();

        let DocumentSymbolResponse::Nested(symbols) = response else {
            panic!("expected nested document symbols");
        };
        assert_eq!(symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["C"]);
        assert_eq!(
            symbols[0]
                .children
                .as_ref()
                .unwrap()
                .iter()
                .map(|symbol| symbol.name.as_str())
                .collect::<Vec<_>>(),
            ["x", "f"]
        );
    }

    #[test]
    fn workspace_symbol_returns_matching_symbols() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state =
            state_with_symbols(symbol_tables(&uri, &other_uri), InitializeParams::default());

        let response = expect_ready(workspace_symbol(
            &mut state,
            WorkspaceSymbolParams {
                query: "oth".into(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        ))
        .unwrap()
        .unwrap();

        let WorkspaceSymbolResponse::Nested(symbols) = response else {
            panic!("expected workspace symbols");
        };
        assert_eq!(
            symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
            ["Other"]
        );
    }

    fn state_with_symbols(symbol_tables: SymbolTables, params: InitializeParams) -> GlobalState {
        let (_, config) = negotiate_capabilities(params);
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        *state.symbol_tables.write() = symbol_tables;
        state
    }

    fn initialize_params_with_hierarchical_document_symbols() -> InitializeParams {
        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            document_symbol: Some(DocumentSymbolClientCapabilities {
                hierarchical_document_symbol_support: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });
        params
    }

    fn document_symbol_params(uri: Url) -> DocumentSymbolParams {
        DocumentSymbolParams {
            text_document: TextDocumentIdentifier { uri },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        }
    }

    fn symbol_tables(uri: &lsp_types::Url, other_uri: &lsp_types::Url) -> SymbolTables {
        let mut tables = SymbolTables::default();
        let contract = push(&mut tables, uri, "C", SymbolKind::CLASS, 0, 0, None);
        push(&mut tables, uri, "x", SymbolKind::PROPERTY, 1, 4, Some(contract));
        push(&mut tables, uri, "f", SymbolKind::METHOD, 2, 4, Some(contract));
        push(&mut tables, other_uri, "Other", SymbolKind::CLASS, 0, 0, None);
        tables
    }

    fn parse_uri(uri: &str) -> lsp_types::Url {
        lsp_types::Url::parse(uri).unwrap()
    }

    fn expect_ready<F: Future>(future: F) -> F::Output {
        let waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        let mut future = std::pin::pin!(future);
        match future.as_mut().poll(&mut cx) {
            Poll::Ready(output) => output,
            Poll::Pending => panic!("request handler future should complete immediately"),
        }
    }

    fn assert_completion_input(
        line_prefix: &str,
        expected_prefix: &str,
        expected_receiver: Option<&str>,
    ) {
        let input = completion_input_from_line_prefix(line_prefix);
        assert_eq!(input.prefix, expected_prefix);
        assert_eq!(input.member_receiver.as_deref(), expected_receiver);
    }
}

#[cfg(test)]
mod formatting_tests {
    use super::*;
    use crate::{config::negotiate_capabilities, test_support::TestProject};
    use async_lsp::ClientSocket;
    #[cfg(unix)]
    use lsp_types::{
        DidChangeTextDocumentParams, TextDocumentContentChangeEvent,
        VersionedTextDocumentIdentifier,
    };
    use lsp_types::{
        FormattingOptions, Position, Range, TextDocumentIdentifier, WorkDoneProgressParams,
    };
    #[cfg(unix)]
    use std::{fs, os::unix::fs::PermissionsExt, path::PathBuf};
    #[cfg(unix)]
    use std::{ops::ControlFlow, time::Duration};
    use std::{path::Path, sync::Arc};
    #[cfg(unix)]
    use tokio::time;

    #[test]
    fn unchanged_formatting_returns_none() {
        assert_eq!(formatting_edits("contract C {}", "contract C {}".into()), None);
    }

    #[test]
    fn changed_formatting_returns_one_full_document_edit() {
        let edits = formatting_edits("a\r\n🚀中\n", "formatted".into()).unwrap();

        assert_eq!(edits.len(), 1);
        assert_eq!(
            edits[0],
            TextEdit {
                range: Range::new(Position::new(0, 0), Position::new(2, 0)),
                new_text: "formatted".into(),
            }
        );
    }

    #[test]
    fn changed_formatting_covers_documents_with_bare_carriage_returns() {
        let edits =
            formatting_edits("contract First{}\rcontract Second{}\r", "formatted".into()).unwrap();

        assert_eq!(edits[0].range, Range::new(Position::new(0, 0), Position::new(2, 0)));
    }

    #[test]
    fn formatter_failures_map_to_concise_request_failed_errors() {
        let failures = [
            (FormatterError::Timeout, "Forge formatting timed out"),
            (
                FormatterError::Io(io::Error::new(io::ErrorKind::NotFound, "missing")),
                "Forge executable was not found",
            ),
            (FormatterError::Io(io::Error::other("pipe failed")), "failed to run Forge formatter"),
            (
                FormatterError::Failed { status: Some(1), stderr: "failed".into() },
                "Forge formatting failed",
            ),
            (
                FormatterError::InvalidUtf8(String::from_utf8(vec![0xff]).unwrap_err()),
                "Forge returned invalid UTF-8",
            ),
        ];

        for (failure, message) in failures {
            let response = formatter_failed(failure);
            assert_eq!(response.code, ErrorCode::REQUEST_FAILED);
            assert_eq!(response.message, message);
            assert!(!response.message.ends_with('.'));
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn missing_forge_returns_request_failed() {
        let mut project = TestProject::from_fixture(
            r#"
            //- /workspace/Test.sol
            contract Test {}
            "#,
        );
        project.open_file("/workspace/Test.sol", "contract Test{}");
        let mut state =
            formatting_state(&project, &project.path("/missing-forge"), &["/workspace"]);
        let uri = Url::from_file_path(project.path("/workspace/Test.sol")).unwrap();

        let error = formatting(&mut state, formatting_params(uri)).await.unwrap_err();

        assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
        assert_eq!(error.message, "Forge executable was not found");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn formatting_rejects_empty_output_for_non_whitespace_source() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/Test.sol
            contract Test {}
            "#,
        );
        let forge = write_executable(&project, "/fake-forge", "#!/bin/sh\ncat >/dev/null\n");
        let mut state = formatting_state(&project, &forge, &["/workspace"]);
        let path = project.path("/workspace/Test.sol");

        let error = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
            .await
            .unwrap_err();

        assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
        assert_eq!(error.message, "Forge formatter returned empty output");
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn formatting_uses_unsaved_vfs_source_and_most_specific_workspace() {
        let mut project = TestProject::from_fixture(
            r#"
            //- /workspace/A.sol
            contract A {}

            //- /workspace/nested/Test.sol
            contract Test {}
            "#,
        );
        let unsaved = "contract Test{string s=\"🚀\";}";
        project.open_file("/workspace/nested/Test.sol", unsaved);
        let forge = write_executable(
            &project,
            "/fake-forge",
            r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$0.args"
cat > "$0.stdin"
printf 'contract Test { string s = "🚀"; }'
"#,
        );
        let mut state = formatting_state(&project, &forge, &["/workspace", "/workspace/nested"]);
        let path = project.path("/workspace/nested/Test.sol");

        let edits = formatting(&mut state, formatting_params(Url::from_file_path(&path).unwrap()))
            .await
            .unwrap()
            .unwrap();

        assert_eq!(edits[0].new_text, "contract Test { string s = \"🚀\"; }");
        assert_eq!(project.read_file("/fake-forge.stdin"), unsaved);
        assert_eq!(
            project.read_file("/fake-forge.args"),
            format!("fmt\n--raw\n--root\n{}\n-\n", project.path("/workspace/nested").display())
        );
        assert_eq!(
            state
                .vfs
                .read()
                .get_file_contents(&crate::vfs::VfsPath::from(path))
                .unwrap()
                .to_string(),
            unsaved
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn formatting_skips_files_ignored_by_foundry_config() {
        let mut project = TestProject::from_fixture(
            r#"
            //- /workspace/foundry.toml
            [fmt]
            ignore = ["src/Ignored.sol"]

            //- /workspace/src/Ignored.sol
            contract Ignored {}
            "#,
        );
        let unsaved = "contract Ignored{uint value;}";
        project.open_file("/workspace/src/Ignored.sol", unsaved);
        let forge = write_executable(
            &project,
            "/fake-forge",
            r#"#!/bin/sh
set -eu
if [ "${1-}" = lint ]; then exit 1; fi
printf '%s\n' "$@" > "$0.called"
cat
"#,
        );
        let mut state = formatting_state(&project, &forge, &["/workspace"]);
        let path = project.path("/workspace/src/Ignored.sol");

        let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
            .await
            .unwrap();

        assert_eq!(edits, None);
        assert!(!project.path("/fake-forge.called").exists());
    }

    #[tokio::test(flavor = "current_thread")]
    async fn formatting_skips_ignored_files_before_reading_contents() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/foundry.toml
            [fmt]
            ignore = ["src/Ignored.sol"]
            "#,
        );
        let mut state =
            formatting_state(&project, &project.path("/missing-forge"), &["/workspace"]);
        let path = project.path("/workspace/src/Ignored.sol");

        let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
            .await
            .unwrap();

        assert_eq!(edits, None);
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn formatting_reads_disk_and_discovers_foundry_root_outside_workspaces() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/.keep

            //- /outside/foundry.toml
            [fmt]
            int_types = "short"

            //- /outside/src/Test.sol
            contract Test {}
            "#,
        );
        let forge = write_executable(
            &project,
            "/fake-forge",
            r#"#!/bin/sh
set -eu
printf '%s\n' "$@" > "$0.args"
cat > "$0.stdin"
cat "$0.stdin"
"#,
        );
        let mut state = formatting_state(&project, &forge, &["/workspace"]);
        let path = project.path("/outside/src/Test.sol");

        let edits = formatting(&mut state, formatting_params(Url::from_file_path(path).unwrap()))
            .await
            .unwrap();

        assert_eq!(edits, None);
        assert_eq!(project.read_file("/fake-forge.stdin"), "contract Test {}");
        assert_eq!(
            project.read_file("/fake-forge.args"),
            format!("fmt\n--raw\n--root\n{}\n-\n", project.path("/outside").display())
        );
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn formatting_rejects_results_after_document_change() {
        let mut project = TestProject::from_fixture(
            r#"
            //- /workspace/Test.sol
            contract Test {}
            "#,
        );
        project.open_file("/workspace/Test.sol", "contract Test{}");
        let forge = write_executable(
            &project,
            "/fake-forge",
            r#"#!/bin/sh
set -eu
cat > "$0.stdin"
: > "$0.ready.tmp"
mv "$0.ready.tmp" "$0.ready"
while [ ! -e "$0.release" ]; do sleep 0.01; done
printf 'contract Test {}'
"#,
        );
        let mut state = formatting_state(&project, &forge, &["/workspace"]);
        let uri = Url::from_file_path(project.path("/workspace/Test.sol")).unwrap();
        let request = formatting(&mut state, formatting_params(uri.clone()));
        let task = tokio::spawn(request);
        wait_for_path(&project.path("/fake-forge.ready")).await;

        let result = crate::handlers::did_change_text_document(
            &mut state,
            DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri, 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: "contract Changed {}".into(),
                }],
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
        project.write_file("/fake-forge.release", "");

        let error = task.await.unwrap().unwrap_err();

        assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
        assert_eq!(error.message, "document changed during formatting");
    }

    fn formatting_params(uri: Url) -> DocumentFormattingParams {
        DocumentFormattingParams {
            text_document: TextDocumentIdentifier { uri },
            options: FormattingOptions { tab_size: 99, insert_spaces: false, ..Default::default() },
            work_done_progress_params: WorkDoneProgressParams::default(),
        }
    }

    fn formatting_state(project: &TestProject, forge: &Path, roots: &[&str]) -> GlobalState {
        let mut params = project.initialize_params_with_roots(roots);
        params.initialization_options =
            Some(serde_json::json!({ "forgePath": forge.display().to_string() }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();

        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        *state.vfs.write() = project.vfs();
        state
    }

    #[cfg(unix)]
    fn write_executable(project: &TestProject, path: &str, contents: &str) -> PathBuf {
        project.write_file(path, contents);
        let path = project.path(path);
        let mut permissions = fs::metadata(&path).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&path, permissions).unwrap();
        path
    }

    #[cfg(unix)]
    async fn wait_for_path(path: &Path) {
        time::timeout(Duration::from_secs(5), async {
            while !path.exists() {
                tokio::task::yield_now().await;
            }
        })
        .await
        .unwrap();
    }
}

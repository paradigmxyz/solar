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
        if formatter::is_ignored(&forge, &path, &root).await.map_err(formatter_failed)? {
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
#[path = "../tests/requests.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/formatting.rs"]
mod formatting_tests;

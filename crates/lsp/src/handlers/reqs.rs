use crate::{global_state::GlobalState, symbols::CompletionContext};
use async_lsp::ResponseError;
use crop::Rope;
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, Position, ReferenceParams, Url,
    WorkspaceSymbolParams, WorkspaceSymbolResponse,
};
use std::future::ready;

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

use crate::{global_state::GlobalState, proto};
use async_lsp::ResponseError;
use lsp_types::{
    CompletionParams, CompletionResponse, DocumentSymbolParams, DocumentSymbolResponse,
    GotoDefinitionParams, GotoDefinitionResponse, ReferenceParams, WorkspaceSymbolParams,
    WorkspaceSymbolResponse,
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
    let source = proto::vfs_path(&params.text_document.uri).and_then(|path| {
        state.vfs.read().get_file_contents(&path).map(|contents| contents.to_string())
    });
    let symbol_tables = state.symbol_tables.read();
    let items = if let Some(source) = source.as_deref() {
        symbol_tables.completion_items_with_source(
            &params.text_document.uri,
            params.position,
            source,
        )
    } else {
        symbol_tables.completion_items(&params.text_document.uri, params.position)
    };
    ready(Ok(Some(CompletionResponse::Array(items))))
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
    use std::sync::Arc;

    #[tokio::test(flavor = "current_thread")]
    async fn document_symbol_returns_flat_symbols_without_hierarchical_client_support() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state =
            state_with_symbols(symbol_tables(&uri, &other_uri), InitializeParams::default());

        let response =
            document_symbol(&mut state, document_symbol_params(uri)).await.unwrap().unwrap();

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

    #[tokio::test(flavor = "current_thread")]
    async fn document_symbol_returns_nested_symbols_with_hierarchical_client_support() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state = state_with_symbols(
            symbol_tables(&uri, &other_uri),
            initialize_params_with_hierarchical_document_symbols(),
        );

        let response =
            document_symbol(&mut state, document_symbol_params(uri)).await.unwrap().unwrap();

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

    #[tokio::test(flavor = "current_thread")]
    async fn workspace_symbol_returns_matching_symbols() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state =
            state_with_symbols(symbol_tables(&uri, &other_uri), InitializeParams::default());

        let response = workspace_symbol(
            &mut state,
            WorkspaceSymbolParams {
                query: "oth".into(),
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
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
}

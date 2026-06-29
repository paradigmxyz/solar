use async_lsp::ResponseError;
use lsp_types::{
    DocumentSymbolParams, DocumentSymbolResponse, WorkspaceSymbolParams, WorkspaceSymbolResponse,
};

use crate::global_state::GlobalState;

pub(crate) fn document_symbol(
    state: &mut GlobalState,
    params: DocumentSymbolParams,
) -> impl Future<Output = Result<Option<DocumentSymbolResponse>, ResponseError>> + use<> {
    let symbols = state.symbol_tables.read().document_symbols(&params.text_document.uri);
    std::future::ready(Ok(Some(DocumentSymbolResponse::Nested(symbols))))
}

pub(crate) fn workspace_symbol(
    state: &mut GlobalState,
    params: WorkspaceSymbolParams,
) -> impl Future<Output = Result<Option<WorkspaceSymbolResponse>, ResponseError>> + use<> {
    let symbols = state.symbol_tables.read().workspace_symbols(&params.query);
    std::future::ready(Ok(Some(WorkspaceSymbolResponse::Nested(symbols))))
}

#[cfg(test)]
mod tests {
    use async_lsp::ClientSocket;
    use lsp_types::{
        DocumentSymbolResponse, PartialResultParams, TextDocumentIdentifier,
        WorkDoneProgressParams, WorkspaceSymbolResponse,
    };

    use crate::symbols::{DeclarationKind, SymbolTables, push_symbol_for_test as push};

    use super::*;

    #[tokio::test(flavor = "current_thread")]
    async fn document_symbol_returns_symbols_for_requested_document() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state = state_with_symbols(symbol_tables(&uri, &other_uri));

        let response = document_symbol(
            &mut state,
            DocumentSymbolParams {
                text_document: TextDocumentIdentifier { uri },
                work_done_progress_params: WorkDoneProgressParams::default(),
                partial_result_params: PartialResultParams::default(),
            },
        )
        .await
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

    #[tokio::test(flavor = "current_thread")]
    async fn workspace_symbol_returns_matching_symbols() {
        let uri = parse_uri("file:///workspace/src/Test.sol");
        let other_uri = parse_uri("file:///workspace/src/Other.sol");
        let mut state = state_with_symbols(symbol_tables(&uri, &other_uri));

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

    fn state_with_symbols(symbol_tables: SymbolTables) -> GlobalState {
        let state = GlobalState::new(ClientSocket::new_closed());
        *state.symbol_tables.write() = symbol_tables;
        state
    }

    fn symbol_tables(uri: &lsp_types::Url, other_uri: &lsp_types::Url) -> SymbolTables {
        let mut tables = SymbolTables::default();
        let contract = push(&mut tables, uri, "C", DeclarationKind::Contract, 0, 0, None);
        push(&mut tables, uri, "x", DeclarationKind::Variable, 1, 4, Some(contract));
        push(&mut tables, uri, "f", DeclarationKind::Function, 2, 4, Some(contract));
        push(&mut tables, other_uri, "Other", DeclarationKind::Contract, 0, 0, None);
        tables
    }

    fn parse_uri(uri: &str) -> lsp_types::Url {
        lsp_types::Url::parse(uri).unwrap()
    }
}

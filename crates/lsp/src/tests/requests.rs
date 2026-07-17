use super::*;
use crate::{
    config::negotiate_capabilities,
    symbols::{SymbolTables, push_symbol_for_test as push},
};
use async_lsp::ClientSocket;
use lsp_types::{
    DocumentSymbolClientCapabilities, DocumentSymbolResponse, InitializeParams,
    PartialResultParams, SymbolKind, TextDocumentClientCapabilities, TextDocumentIdentifier, Url,
    WorkDoneProgressParams, WorkspaceSymbolResponse,
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

    let response =
        expect_ready(document_symbol(&mut state, document_symbol_params(uri))).unwrap().unwrap();

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

    let response =
        expect_ready(document_symbol(&mut state, document_symbol_params(uri))).unwrap().unwrap();

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
    assert_eq!(symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["Other"]);
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

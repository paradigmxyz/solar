use super::*;
use crate::{
    config::negotiate_capabilities,
    symbols::{SymbolTables, push_symbol_for_test as push},
};
use async_lsp::ClientSocket;
use lsp_types::{
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    DocumentSymbolClientCapabilities, DocumentSymbolResponse, InitializeParams,
    PartialResultParams, ReferenceContext, SymbolKind, TextDocumentClientCapabilities,
    TextDocumentIdentifier, Url, WorkDoneProgressParams, WorkspaceSymbolResponse,
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
    let uri = file_uri("Test.sol");
    let other_uri = file_uri("Other.sol");
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
    let uri = file_uri("Test.sol");
    let other_uri = file_uri("Other.sol");
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
    let uri = file_uri("Test.sol");
    let other_uri = file_uri("Other.sol");
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

#[test]
fn document_diagnostic_skips_analysis_for_non_file_uris() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();

    let response = expect_ready(document_diagnostic(
        &mut state,
        document_diagnostic_params(parse_uri("untitled:Diagnostics.sol"), None),
    ))
    .unwrap();
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = response
    else {
        panic!("first diagnostic pull should return a full report");
    };
    assert!(report.full_document_diagnostic_report.items.is_empty());
}

#[test]
fn semantic_requests_wait_for_latest_analysis() {
    let uri = file_uri("Test.sol");
    let mut state = pending_analysis_state();

    assert_pending(document_symbol(&mut state, document_symbol_params(uri.clone())));
    assert_pending(document_links(&mut state, document_link_params(uri.clone())));
    assert_pending(goto_definition(&mut state, goto_params(uri.clone())));
    assert_pending(goto_type_definition(&mut state, goto_params(uri.clone())));
    assert_pending(goto_declaration(&mut state, goto_params(uri.clone())));
    assert_pending(goto_implementation(&mut state, goto_params(uri.clone())));
    assert_pending(references(&mut state, reference_params(uri.clone())));
    assert_pending(prepare_rename(&mut state, position_params(uri.clone())));
    assert_pending(rename(&mut state, rename_params(uri.clone(), "renamed")));
    assert_pending(inlay_hints(&mut state, inlay_hint_params(uri)));
}

#[test]
fn semantic_requests_skip_analysis_for_non_file_uris() {
    let uri = parse_uri("untitled:Test.sol");
    let mut state = pending_analysis_state();

    assert_ready(document_symbol(&mut state, document_symbol_params(uri.clone())));
    assert_ready(document_links(&mut state, document_link_params(uri.clone())));
    assert_ready(goto_definition(&mut state, goto_params(uri.clone())));
    assert_ready(goto_type_definition(&mut state, goto_params(uri.clone())));
    assert_ready(goto_declaration(&mut state, goto_params(uri.clone())));
    assert_ready(goto_implementation(&mut state, goto_params(uri.clone())));
    assert_ready(references(&mut state, reference_params(uri.clone())));
    assert_ready(prepare_rename(&mut state, position_params(uri.clone())));
    assert_ready(rename(&mut state, rename_params(uri.clone(), "renamed")));
    assert_ready(inlay_hints(&mut state, inlay_hint_params(uri)));
}

#[test]
fn invalid_rename_names_and_latency_sensitive_requests_do_not_wait_for_analysis() {
    let uri = file_uri("Test.sol");
    let mut state = pending_analysis_state();

    let error =
        expect_ready(rename(&mut state, rename_params(uri.clone(), "not a name"))).unwrap_err();
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert_ready(completion(&mut state, completion_params(uri.clone())));
    assert_ready(signature_help(&mut state, signature_help_params(uri)));
    assert_ready(workspace_symbol(
        &mut state,
        WorkspaceSymbolParams {
            query: String::new(),
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
        },
    ));
}

fn pending_analysis_state() -> GlobalState {
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    state
}

fn position_params(uri: Url) -> TextDocumentPositionParams {
    TextDocumentPositionParams {
        text_document: TextDocumentIdentifier::new(uri),
        position: Position::new(0, 0),
    }
}

fn goto_params(uri: Url) -> GotoDefinitionParams {
    GotoDefinitionParams {
        text_document_position_params: position_params(uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn reference_params(uri: Url) -> ReferenceParams {
    ReferenceParams {
        text_document_position: position_params(uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: ReferenceContext { include_declaration: true },
    }
}

fn rename_params(uri: Url, new_name: &str) -> RenameParams {
    RenameParams {
        text_document_position: position_params(uri),
        new_name: new_name.into(),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

fn document_link_params(uri: Url) -> DocumentLinkParams {
    DocumentLinkParams {
        text_document: TextDocumentIdentifier::new(uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn document_diagnostic_params(
    uri: Url,
    previous_result_id: Option<String>,
) -> DocumentDiagnosticParams {
    DocumentDiagnosticParams {
        text_document: TextDocumentIdentifier { uri },
        identifier: None,
        previous_result_id,
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

fn inlay_hint_params(uri: Url) -> InlayHintParams {
    InlayHintParams {
        text_document: TextDocumentIdentifier::new(uri),
        range: lsp_types::Range::new(Position::new(0, 0), Position::new(u32::MAX, u32::MAX)),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
}

fn completion_params(uri: Url) -> CompletionParams {
    CompletionParams {
        text_document_position: position_params(uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
        context: None,
    }
}

fn signature_help_params(uri: Url) -> SignatureHelpParams {
    SignatureHelpParams {
        context: None,
        text_document_position_params: position_params(uri),
        work_done_progress_params: WorkDoneProgressParams::default(),
    }
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

fn file_uri(path: &str) -> lsp_types::Url {
    lsp_types::Url::from_file_path(std::env::temp_dir().join(path)).unwrap()
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

fn assert_pending<F: Future>(future: F) {
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let mut future = std::pin::pin!(future);
    assert!(future.as_mut().poll(&mut context).is_pending());
}

fn assert_ready<F: Future>(future: F) {
    let _ = expect_ready(future);
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

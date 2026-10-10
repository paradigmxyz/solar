use super::*;
use crate::{
    config::negotiate_capabilities,
    symbols::{SymbolTables, push_symbol_for_test as push},
    test_support::{
        assert_polls, document_params, expect_ready, from_json, request_params, state_with,
        with_options,
    },
};
use lsp_types::{
    DocumentDiagnosticReport, DocumentDiagnosticReportResult, DocumentSymbolResponse,
    InitializeParams, SymbolKind, WorkspaceSymbolResponse,
};
use serde::de::DeserializeOwned;
use serde_json::{Value, json};
use std::sync::Arc;

#[test]
fn completion_input_extracts_prefix_and_member_receiver() {
    for (line_prefix, prefix, receiver) in [
        ("        ms", "ms", None),
        ("        msg.", "", Some("msg")),
        ("        msg.s", "s", Some("msg")),
        ("        getToken().", "", Some("")),
        ("        object.field.", "", Some("")),
        ("        object . field . tw", "tw", Some("")),
        ("        value . ", "", Some("value")),
        ("        value . tw", "tw", Some("value")),
    ] {
        let input = completion_input_from_line_prefix(line_prefix);
        assert_eq!(
            (input.prefix.as_str(), input.member_receiver.as_deref()),
            (prefix, receiver),
            "{line_prefix:?}"
        );
    }
}

#[test]
fn symbol_requests_read_the_latest_symbol_tables() {
    let uri = file_uri("Test.sol");
    let mut state = state_with_symbols(InitializeParams::default());

    let Some(DocumentSymbolResponse::Flat(symbols)) =
        expect_ready(document_symbol(&mut state, document_params(&uri))).unwrap()
    else {
        panic!("expected flat document symbols");
    };
    assert_eq!(
        symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
        ["C", "x", "f"]
    );
    assert_eq!(symbols[0].container_name, None);
    assert_eq!(symbols[1].container_name.as_deref(), Some("C"));

    let Some(WorkspaceSymbolResponse::Nested(symbols)) =
        expect_ready(workspace_symbol(&mut state, params(&uri, json!({ "query": "oth" }))))
            .unwrap()
    else {
        panic!("expected workspace symbols");
    };
    assert_eq!(symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["Other"]);

    let hierarchical = from_json(json!({ "capabilities": { "textDocument": {
        "documentSymbol": { "hierarchicalDocumentSymbolSupport": true },
    } } }));
    let mut state = state_with_symbols(hierarchical);
    let Some(DocumentSymbolResponse::Nested(symbols)) =
        expect_ready(document_symbol(&mut state, document_params(&uri))).unwrap()
    else {
        panic!("expected nested document symbols");
    };
    let [contract] = symbols.as_slice() else { panic!("expected one root symbol") };
    assert_eq!(contract.name, "C");
    let children = contract.children.as_ref().unwrap();
    assert_eq!(children.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(), ["x", "f"]);
}

#[test]
fn semantic_requests_wait_for_analysis_only_for_file_uris() {
    let file = file_uri("Test.sol");
    let untitled = Url::parse("untitled:Test.sol").unwrap();

    // Type hierarchy follow-ups use the item URI, not the URI stored in its data.
    for (uri, data_uri, pending) in [(&file, &untitled, true), (&untitled, &file, false)] {
        let mut state = pending_analysis_state();
        let item = json!({ "item": type_hierarchy_item(uri, data_uri) });
        assert_polls(pending, document_symbol(&mut state, document_params(uri)));
        assert_polls(pending, document_links(&mut state, document_params(uri)));
        assert_polls(pending, goto_definition(&mut state, document_params(uri)));
        assert_polls(pending, goto_type_definition(&mut state, document_params(uri)));
        assert_polls(pending, goto_declaration(&mut state, document_params(uri)));
        assert_polls(pending, goto_implementation(&mut state, document_params(uri)));
        let context = json!({ "context": { "includeDeclaration": true } });
        assert_polls(pending, references(&mut state, params(uri, context)));
        assert_polls(pending, prepare_rename(&mut state, document_params(uri)));
        assert_polls(pending, rename(&mut state, params(uri, json!({ "newName": "renamed" }))));
        assert_polls(pending, inlay_hints(&mut state, document_params(uri)));
        assert_polls(pending, code_lens(&mut state, document_params(uri)));
        assert_polls(pending, prepare_type_hierarchy(&mut state, document_params(uri)));
        assert_polls(pending, type_hierarchy_supertypes(&mut state, params(uri, item.clone())));
        assert_polls(pending, type_hierarchy_subtypes(&mut state, params(uri, item)));
    }

    let mut state = pending_analysis_state();
    let Ok(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report))) =
        expect_ready(document_diagnostic(&mut state, document_params(&untitled)))
    else {
        panic!("first diagnostic pull should return a full report");
    };
    assert!(report.full_document_diagnostic_report.items.is_empty());
}

#[test]
fn latency_sensitive_requests_without_source_do_not_wait_for_analysis() {
    let uri = file_uri("Test.sol");
    let mut state = pending_analysis_state();

    let error = expect_ready(rename(&mut state, params(&uri, json!({ "newName": "not a name" }))))
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
    assert_polls(false, completion(&mut state, document_params(&uri)));
    assert_polls(false, signature_help(&mut state, document_params(&uri)));
    assert_polls(false, workspace_symbol(&mut state, params(&uri, json!({ "query": "" }))));

    for code_lens_options in [
        json!({ "enable": false }),
        json!({
            "enable": true,
            "selectors": false,
            "references": false,
            "inheritance": false,
            "clientCommands": true,
        }),
    ] {
        let params = with_options(Default::default(), json!({ "codeLens": code_lens_options }));
        state.config = Arc::new(negotiate_capabilities(params).1);

        let response = expect_ready(code_lens(&mut state, document_params(&uri))).unwrap();
        assert!(response.is_some_and(|lenses| lenses.is_empty()));
    }
}

fn pending_analysis_state() -> GlobalState {
    let state = state_with(Config::default());
    state.mark_analysis_pending_for_test();
    state
}

/// Builds request params at the start of `uri`, extended with request-specific `extra` fields.
fn params<T: DeserializeOwned>(uri: &Url, extra: Value) -> T {
    request_params(uri, Position::default(), extra)
}

fn type_hierarchy_item(uri: &Url, data_uri: &Url) -> Value {
    let range = json!({
        "start": { "line": 0, "character": 0 },
        "end": { "line": 0, "character": 1 },
    });
    json!({
        "name": "C",
        "kind": SymbolKind::CLASS,
        "uri": uri,
        "range": range,
        "selectionRange": range,
        "data": { "version": 1, "uri": data_uri, "selectionRange": range },
    })
}

fn state_with_symbols(params: InitializeParams) -> GlobalState {
    let uri = file_uri("Test.sol");
    let mut tables = SymbolTables::default();
    let contract = push(&mut tables, &uri, "C", SymbolKind::CLASS, 0, 0, None);
    push(&mut tables, &uri, "x", SymbolKind::PROPERTY, 1, 4, Some(contract));
    push(&mut tables, &uri, "f", SymbolKind::METHOD, 2, 4, Some(contract));
    push(&mut tables, &file_uri("Other.sol"), "Other", SymbolKind::CLASS, 0, 0, None);

    let state = state_with(negotiate_capabilities(params).1);
    state.symbol_tables.store(Arc::new(tables));
    state
}

fn file_uri(path: &str) -> Url {
    Url::from_file_path(std::env::temp_dir().join(path)).unwrap()
}

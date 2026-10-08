use super::*;
use lsp_types::{CompletionParams, CompletionResponse, SignatureHelpParams};
use serde::de::DeserializeOwned;

const POSITION: Position = Position { line: 0, character: 9 };

fn fixture() -> (TestProject, GlobalState, Url) {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Before {}\n");
    let result = analyze_single_batch(&snapshot(&project));
    assert!(result.diagnostics.is_empty());
    let uri = project.uri("/Request.sol");
    let state = project.state();
    state.symbol_tables.store(Arc::new(result.symbol_tables));
    (project, state, uri)
}

/// Builds params for any request at the contract name.
fn params<T: DeserializeOwned>(uri: &Url) -> T {
    let extra = json!({ "newName": "Renamed", "context": { "includeDeclaration": true } });
    request_params(uri, POSITION, extra)
}

fn request_foreground(state: &mut GlobalState, uri: &Url, method: &str) {
    match method {
        "hover" => drop(crate::handlers::hover(state, params(uri))),
        "definition" => drop(crate::handlers::goto_definition(state, params(uri))),
        "typeDefinition" => drop(crate::handlers::goto_type_definition(state, params(uri))),
        "declaration" => drop(crate::handlers::goto_declaration(state, params(uri))),
        "implementation" => drop(crate::handlers::goto_implementation(state, params(uri))),
        "references" => drop(crate::handlers::references(state, params(uri))),
        "prepareRename" => drop(crate::handlers::prepare_rename(state, params(uri))),
        "rename" => drop(crate::handlers::rename(state, params(uri))),
        _ => unreachable!(),
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn foreground_requests_end_the_pending_source_change_debounce() {
    for method in [
        "hover",
        "definition",
        "typeDefinition",
        "declaration",
        "implementation",
        "references",
        "prepareRename",
        "rename",
    ] {
        let (_project, mut state, uri) = fixture();
        change(&mut state, &uri, 1, "contract After {}");
        let coordinator = analysis_coordinator(&state);
        state.analysis_scheduler.gate.close();
        tokio::time::sleep(state.config.source_change_debounce() / 2).await;
        let start = tokio::time::Instant::now();
        let tick = Duration::from_millis(1);

        request_foreground(&mut state, &uri, method);
        tokio::time::sleep(tick).await;

        assert_eq!(tokio::time::Instant::now(), start + tick);
        assert!(coordinator.is_finished(), "{method} should reach the scheduler immediately");
        assert_eq!(*state.published_analysis_version.borrow(), 0);
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn document_symbols_and_diagnostics_retain_the_source_change_debounce() {
    let (_project, mut state, uri) = fixture();
    change(&mut state, &uri, 1, "contract After {}");
    let _symbols = start_request(crate::handlers::document_symbol(&mut state, params(&uri)));
    let diagnostics = document_diagnostic_params(&uri, None);
    let _diagnostics = start_request(crate::handlers::document_diagnostic(&mut state, diagnostics));
    let coordinator = analysis_coordinator(&state);
    let tick = Duration::from_millis(1);
    state.analysis_scheduler.gate.close();

    tokio::time::sleep(state.config.source_change_debounce() - tick).await;
    assert!(!coordinator.is_finished(), "background requests should preserve the debounce");

    tokio::time::sleep(tick * 2).await;
    assert!(coordinator.is_finished());
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn repeated_foreground_requests_wait_for_one_fresh_analysis() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract After {}");
    let version = analysis_version(&state);
    let coordinator = analysis_coordinator(&state);
    let hover = start_request(crate::handlers::hover(&mut state, params(&uri)));
    for _ in 0..8 {
        request_foreground(&mut state, &uri, "definition");
    }
    tokio::time::sleep(Duration::from_millis(1)).await;
    let (waiting_at_gate, same_coordinator, debounce_ended) = {
        let tasks = state.analysis_scheduler.tasks.lock();
        (
            tasks.worker.is_none() && !coordinator.is_finished(),
            tasks.coordinator.as_ref().is_some_and(|(_, task)| task.id() == coordinator.id()),
            tasks.debounce.is_none(),
        )
    };
    let unpublished = *state.published_analysis_version.borrow() == 0;
    let old_symbols_visible = !state.symbol_tables.load().workspace_symbols("Before").is_empty();
    let unchanged_epoch = analysis_version(&state) == version;
    drop(gate);
    tokio::time::resume();

    let response = within("foreground analysis", hover).await.unwrap();
    assert!(waiting_at_gate && same_coordinator && debounce_ended);
    assert!(unpublished && old_symbols_visible && unchanged_epoch);
    assert!(response.is_some());
    let tables = state.symbol_tables.load();
    assert_eq!(workspace_symbol_names(&tables), ["After"]);
    assert_eq!(response, tables.hover(&uri, POSITION));
    assert_eq!(*state.published_analysis_version.borrow(), version);
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn edits_after_foreground_urgency_restart_the_full_debounce() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract Intermediate {}");
    let first_coordinator = analysis_coordinator(&state);
    let hover = crate::handlers::hover(&mut state, params(&uri));
    let tick = Duration::from_millis(1);
    tokio::time::sleep(tick).await;

    change(&mut state, &uri, 2, "contract Latest {}");
    let latest_coordinator = analysis_coordinator(&state);
    drop(gate);
    tokio::time::sleep(state.config.source_change_debounce() - tick).await;

    assert!(first_coordinator.is_finished());
    assert!(!latest_coordinator.is_finished(), "the new edit should receive a full debounce");
    assert!(state.analysis_scheduler.tasks.lock().worker.is_none());
    assert_eq!(*state.published_analysis_version.borrow(), 0);
    assert!(state.symbol_tables.load().workspace_symbols("Intermediate").is_empty());

    tokio::time::sleep(tick * 2).await;
    tokio::time::resume();
    let response = within("the invalidated hover", hover).await.unwrap_err();
    assert_eq!(response.code, ErrorCode::CONTENT_MODIFIED);
    let response =
        within("request", crate::handlers::hover(&mut state, params(&uri))).await.unwrap();
    assert!(response.is_some());
    let tables = state.symbol_tables.load();
    assert_eq!(workspace_symbol_names(&tables), ["Latest"]);
    assert_eq!(response, tables.hover(&uri, POSITION));
    assert_eq!(*state.published_analysis_version.borrow(), analysis_version(&state));
}

#[tokio::test(flavor = "current_thread")]
async fn requests_recheck_freshness_after_analysis_wakes_them() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract Intermediate {}");
    let mut hover = start_request(crate::handlers::hover(&mut state, params(&uri)));
    let diagnostics = document_diagnostic_params(&uri, None);
    let mut diagnostics =
        start_request(crate::handlers::document_diagnostic(&mut state, diagnostics));

    let mut snapshot = state.snapshot();
    let result = analyze(snapshot.analysis_batches(Vec::new()).pop().unwrap());
    assert!(snapshot.publish_analysis(analysis_version(&state), result));
    change(&mut state, &uri, 2, "contract Latest {}");

    let Err(error) = expect_ready(hover.as_mut()) else {
        panic!("a superseded hover must finish without waiting for more edits");
    };
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    let Err(error) = expect_ready(diagnostics.as_mut()) else {
        panic!("a superseded diagnostic request must finish without waiting for more edits");
    };
    assert_eq!(error.code, ErrorCode::SERVER_CANCELLED);
    let hover = crate::handlers::hover(&mut state, params(&uri));
    let diagnostics =
        crate::handlers::document_diagnostic(&mut state, document_diagnostic_params(&uri, None));
    drop(gate);
    state.prioritize_pending_analysis();
    let response = within("request", hover).await.unwrap();
    within("request", diagnostics).await.unwrap();
    assert!(response.is_some());
    assert_eq!(response, state.symbol_tables.load().hover(&uri, POSITION));
    assert!(state.symbol_tables.load().workspace_symbols("Intermediate").is_empty());
    assert_eq!(*state.published_analysis_version.borrow(), analysis_version(&state));
}

#[tokio::test(flavor = "current_thread")]
async fn rapid_edits_and_saves_in_a_large_workspace_publish_the_latest_analysis() {
    let mut source = String::from("//- /Request.sol open\ncontract Before {}\n");
    for index in 0..256 {
        source.push_str(&format!(
            "//- /Contract{index}.sol\ncontract Contract{index} {{ function value() external pure returns (uint) {{ return {index}; }} }}\n"
        ));
    }
    let project = TestProject::from_fixture(&source);
    let uri = project.uri("/Request.sol");
    let mut state = project.state();
    change(&mut state, &uri, 1, "contract Intermediate {}");
    state.prioritize_pending_analysis();
    settle(&state).await;

    // Keep the ready request unpolled while the editor sends another burst of changes.
    let diagnostics =
        crate::handlers::document_diagnostic(&mut state, document_diagnostic_params(&uri, None));
    for version in 2..=201 {
        let text = if version % 2 == 0 { "contract Broken {" } else { "contract Latest {}" };
        change(&mut state, &uri, version, text);
        project.write_file("/Request.sol", text);
        save(&mut state, &uri);
        if version % 10 == 0 {
            state.prioritize_pending_analysis();
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    }
    let Err(error) = expect_ready(diagnostics) else {
        panic!("the earlier request must finish despite continued edits");
    };
    assert_eq!(error.code, ErrorCode::SERVER_CANCELLED);
    let diagnostics =
        crate::handlers::document_diagnostic(&mut state, document_diagnostic_params(&uri, None));
    state.prioritize_pending_analysis();
    let report = within("request", diagnostics).await.unwrap();
    let DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(report)) = report
    else {
        panic!("expected a full diagnostic report");
    };
    assert!(report.full_document_diagnostic_report.items.is_empty());
    assert_eq!(*state.published_analysis_version.borrow(), analysis_version(&state));
    let tables = state.symbol_tables.load();
    assert!(tables.workspace_symbols("Intermediate").is_empty());
    assert!(tables.workspace_symbols("Broken").is_empty());
    assert!(tables.workspace_symbols("Latest").iter().any(|symbol| symbol.name == "Latest"));
    assert_eq!(tables.workspace_symbols("Contract").len(), 256);
}

#[tokio::test(flavor = "current_thread")]
async fn pending_rename_rejects_a_different_identifier_at_the_same_position() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract Original {}");
    let rename = start_request(crate::handlers::rename(&mut state, params(&uri)));
    let prepare = start_request(crate::handlers::prepare_rename(&mut state, params(&uri)));
    change(&mut state, &uri, 2, "contract Replaced {}");
    drop(gate);
    state.prioritize_pending_analysis();
    settle(&state).await;

    let error = within("request", rename).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
    let error = within("request", prepare).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_requests_wake_without_analysis_publication() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract First {}");
    let hover = tokio::spawn(crate::handlers::hover(&mut state, params(&uri)));
    tokio::task::yield_now().await;
    assert!(!hover.is_finished());

    change(&mut state, &uri, 2, "contract Second {}");
    let result = tokio::time::timeout(TIMEOUT, hover).await;
    assert_eq!(*state.published_analysis_version.borrow(), 0);
    drop(gate);
    let error = result
        .expect("invalidation must wake the request without publication")
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn pending_rename_rejects_disk_changes_after_analysis_catches_up() {
    let (project, mut state, uri) = fixture();
    let path = project.path("/Request.sol");
    remove_overlay(&state, &path);
    project.write_file("/Request.sol", "contract Original {}");
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    state.recompute_with_disk_files(vec![path.clone()]);
    let rename = crate::handlers::rename(&mut state, params(&uri));
    let revision = state.vfs.read().content_revision();
    project.write_file("/Request.sol", "contract Replaced {}");
    state.recompute_with_disk_files(vec![path]);
    drop(gate);
    state.prioritize_pending_analysis();
    settle(&state).await;
    assert_eq!(state.vfs.read().content_revision(), revision);
    let error = within("request", rename).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[tokio::test(flavor = "current_thread")]
async fn content_identical_edits_preserve_pending_requests() {
    let (_project, mut state, uri) = fixture();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 1, "contract Latest {}");
    let hover = crate::handlers::hover(&mut state, params(&uri));
    change(&mut state, &uri, 2, "contract Latest {}");
    drop(gate);
    let response = within("request", hover).await.unwrap();
    assert!(response.is_some());
}

const USING_SOURCE: &str = r#"library Math {
    function twice(uint256 value) internal pure returns (uint256) { return value * 2; }
    function triple(uint256 value) internal pure returns (uint256) { return value * 3; }
}
contract C {
    using {Math.twice} for uint256;
    function f() public pure {
        uint256 x;
        // completion
    }
}"#;

async fn using_fixture() -> (TestProject, GlobalState, Url) {
    let (project, mut state, uri) = fixture();
    change(&mut state, &uri, 1, USING_SOURCE);
    state.prioritize_pending_analysis();
    within("analysis", state.latest_analysis()).await.unwrap();
    (project, state, uri)
}

fn completion_params(uri: &Url, position: Position) -> CompletionParams {
    request_params(uri, position, json!({}))
}

fn signature_params(uri: &Url, position: Position) -> SignatureHelpParams {
    request_params(uri, position, json!({}))
}

fn completion_labels(response: Option<CompletionResponse>) -> Vec<String> {
    let Some(CompletionResponse::Array(items)) = response else {
        panic!("expected completion array")
    };
    items.into_iter().map(|item| item.label).collect()
}

#[tokio::test(flavor = "current_thread")]
async fn completion_refreshes_local_names_and_using_methods_after_edits() {
    for (changed, expression, expected) in [
        (
            USING_SOURCE.replace("uint256 x;", "uint256 x; uint256 addedLocal;"),
            "addedL",
            vec!["addedLocal"],
        ),
        (USING_SOURCE.replace("uint256 x;", "uint256 newLocal;"), "newL", vec!["newLocal"]),
        (USING_SOURCE.replace("{Math.twice}", "Math"), "x.", vec!["triple", "twice"]),
        (USING_SOURCE.replace("{Math.twice}", "{Math.triple}"), "x.", vec!["triple"]),
        (USING_SOURCE.replace("using {Math.twice} for uint256;", ""), "x.", vec![]),
    ] {
        let (_project, mut state, uri) = using_fixture().await;
        let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
        change(&mut state, &uri, 2, changed.replace("// completion", expression));
        let mut request = std::pin::pin!(crate::handlers::completion(
            &mut state,
            completion_params(&uri, Position::new(8, 8 + expression.len() as u32)),
        ));
        assert!(request.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
        drop(gate);
        let response = within("request", request).await.unwrap();
        assert_eq!(completion_labels(response), expected);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn signature_help_waits_for_a_new_attached_call() {
    let (_project, mut state, uri) = using_fixture().await;
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, USING_SOURCE.replace("// completion", "x.twice("));
    let mut request = std::pin::pin!(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ));
    assert!(request.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending());
    assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
    drop(gate);
    let response = within("request", request).await.unwrap().unwrap();
    snapbox::assert_data_eq!(
        response.signatures[0].label.as_str(),
        snapbox::str!["function twice() internal pure returns (uint256)"]
    );
}

const FOLLOWING_BLOCKS: [&str; 5] = [
    "if (true) { uint256 blockLocal; $2blockLocal; }",
    "for (uint256 i; i < 1; ++i) { uint256 blockLocal; $2blockLocal; }",
    "while (false) { uint256 blockLocal; $2blockLocal; }",
    "unchecked { uint256 blockLocal; $2blockLocal; }",
    "{ uint256 blockLocal; $2blockLocal; }",
];

#[tokio::test(flavor = "current_thread")]
async fn incomplete_members_before_blocks_preserve_interactive_analysis() {
    for following in FOLLOWING_BLOCKS {
        for expression in ["x.", "x.tw", "(x + 1)."] {
            let source = USING_SOURCE.replace(
                "// completion",
                &format!("{expression}$1\n        {following}\n        uint256 afterBlock;\n        $3afterBlock;"),
            );
            let opened = support::RequestFixture::new_allowing_diagnostics(
                &format!("//- /Request.sol open\n{source}"),
                "/Request.sol",
            );
            let (_, cursor) = opened.marker_location("$1");
            let source = opened.project_contents("/Request.sol");
            let (_project, mut state, uri) = using_fixture().await;
            let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
            change(&mut state, &uri, 2, &source);
            let mut params = completion_params(&uri, cursor);
            if expression.ends_with('.') {
                params.context = Some(lsp_types::CompletionContext {
                    trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
                    trigger_character: Some(".".into()),
                });
            }
            let request = start_request(crate::handlers::completion(&mut state, params));
            assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
            drop(gate);
            let response = within("request", request).await.unwrap();
            assert_eq!(completion_labels(response), ["twice"], "{expression} before {following}");
            let ready = expect_ready(crate::handlers::completion(
                &mut state,
                completion_params(&uri, cursor),
            ))
            .unwrap();
            assert_eq!(completion_labels(ready), ["twice"]);

            // Recovery must leave the following block and its lexical scope intact.
            for (marker, expected, excluded) in
                [("$2", "blockLocal", "afterBlock"), ("$3", "afterBlock", "blockLocal")]
            {
                let (_, cursor) = opened.marker_location(marker);
                let response = expect_ready(crate::handlers::completion(
                    &mut state,
                    completion_params(&uri, cursor),
                ))
                .unwrap();
                let labels = completion_labels(response);
                assert!(
                    labels.iter().any(|label| label == expected),
                    "{expression} before {following}: missing {expected}"
                );
                assert!(
                    !labels.iter().any(|label| label == excluded),
                    "{expression} before {following}: unexpected {excluded}"
                );
            }

            // Initially opening this incomplete source has no last-good snapshot to reuse.
            let (uri, cursor) = opened.marker_location("$1");
            let response = expect_ready(crate::handlers::completion(
                &mut opened.state(),
                completion_params(&uri, cursor),
            ))
            .unwrap();
            assert_eq!(completion_labels(response), ["twice"]);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn incomplete_calls_before_blocks_preserve_interactive_analysis() {
    for following in FOLLOWING_BLOCKS {
        for expression in ["add($1", "add($1)", "add(1 $1", "add(1,$1", "add(add($1", "x.twice($1"]
        {
            let source = USING_SOURCE
                .replace("uint256 x;", "uint256 x; uint256 addedLocal;")
                .replace(
                    "function f()",
                    "function add(uint256 amount) internal pure returns (uint256) { return amount; }\n    function f()",
                )
                .replace(
                    "// completion",
                    &format!("{expression}\n        {following}\n        uint256 afterBlock;\n        $3afterBlock;"),
                );
            let opened = support::RequestFixture::new_allowing_diagnostics(
                &format!("//- /Request.sol open\n{source}"),
                "/Request.sol",
            );
            let (_, cursor) = opened.marker_location("$1");
            let source = opened.project_contents("/Request.sol");
            let (_project, mut state, uri) = using_fixture().await;
            let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
            change(&mut state, &uri, 2, &source);
            let completion = start_request(crate::handlers::completion(
                &mut state,
                completion_params(&uri, cursor),
            ));
            let mut params = signature_params(&uri, cursor);
            let trigger = expression.split_once("$1").unwrap().0.chars().last().unwrap();
            if matches!(trigger, '(' | ',') {
                params.context = Some(lsp_types::SignatureHelpContext {
                    trigger_kind: lsp_types::SignatureHelpTriggerKind::TRIGGER_CHARACTER,
                    trigger_character: Some(trigger.to_string()),
                    is_retrigger: false,
                    active_signature_help: None,
                });
            }
            let signature = start_request(crate::handlers::signature_help(&mut state, params));
            assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
            drop(gate);
            let response = within("completion", completion).await.unwrap();
            let labels = completion_labels(response);
            let locals = labels
                .iter()
                .map(String::as_str)
                .filter(|label| matches!(*label, "x" | "addedLocal" | "blockLocal" | "afterBlock"))
                .collect::<Vec<_>>();
            assert_eq!(locals, ["addedLocal", "x"], "{expression} before {following}");
            let response = within("signature help", signature).await.unwrap().unwrap();
            assert_eq!(response.signatures.len(), 1);
            if expression.starts_with("x.") {
                snapbox::assert_data_eq!(
                    response.signatures[0].label.as_str(),
                    snapbox::str!["function twice() internal pure returns (uint256)"]
                );
            } else {
                snapbox::assert_data_eq!(
                    response.signatures[0].label.as_str(),
                    snapbox::str!["function add(uint256 amount) internal pure returns (uint256)"]
                );
            }
            let ready = expect_ready(crate::handlers::signature_help(
                &mut state,
                signature_params(&uri, cursor),
            ))
            .unwrap()
            .unwrap();
            assert_eq!(response, ready);
            let ready = expect_ready(crate::handlers::completion(
                &mut state,
                completion_params(&uri, cursor),
            ))
            .unwrap();
            assert_eq!(labels, completion_labels(ready));

            for (marker, expected, excluded) in
                [("$2", "blockLocal", "afterBlock"), ("$3", "afterBlock", "blockLocal")]
            {
                let (_, cursor) = opened.marker_location(marker);
                let response = expect_ready(crate::handlers::completion(
                    &mut state,
                    completion_params(&uri, cursor),
                ))
                .unwrap();
                let labels = completion_labels(response);
                assert!(
                    labels.iter().any(|label| label == expected),
                    "{expression} before {following}: missing {expected}"
                );
                assert!(
                    !labels.iter().any(|label| label == excluded),
                    "{expression} before {following}: unexpected {excluded}"
                );
            }

            // The callee and addedLocal were introduced in this edit, so an old table cannot
            // satisfy these requests. An initial open must provide the same semantic results.
            let (uri, cursor) = opened.marker_location("$1");
            let mut initial = opened.state();
            let initial_signature = expect_ready(crate::handlers::signature_help(
                &mut initial,
                signature_params(&uri, cursor),
            ))
            .unwrap()
            .unwrap();
            // The fixtures negotiate different label-offset capabilities.
            assert_eq!(response.signatures[0].label, initial_signature.signatures[0].label);
            let initial_completion = expect_ready(crate::handlers::completion(
                &mut initial,
                completion_params(&uri, cursor),
            ))
            .unwrap();
            assert_eq!(labels, completion_labels(initial_completion));
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_reject_failed_analysis() {
    let (_project, mut state, uri) = using_fixture().await;
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, USING_SOURCE.replace("// completion", "x.twice("));
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ));
    handle_analysis_failure(
        state.analysis_version.load(Ordering::Acquire),
        "test analysis failure",
        &state.analysis_version,
        &state.published_analysis_version,
        &state.analysis_commit,
    );
    assert_eq!(completion.await.unwrap_err().code, ErrorCode::REQUEST_FAILED);
    assert_eq!(signature.await.unwrap_err().code, ErrorCode::REQUEST_FAILED);
    for clear in [false, true] {
        if clear {
            // Clearing publishes the current epoch, but does not make its empty table usable.
            state.clear_analysis_cache();
        }
        let completion = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params(&uri, Position::new(8, 10)),
        ));
        let signature = expect_ready(crate::handlers::signature_help(
            &mut state,
            signature_params(&uri, Position::new(8, 16)),
        ));
        assert_eq!(completion.unwrap_err().code, ErrorCode::REQUEST_FAILED);
        assert_eq!(signature.unwrap_err().code, ErrorCode::REQUEST_FAILED);
    }
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_reject_superseded_inputs() {
    for configuration in [false, true] {
        for published in [false, true] {
            let (_project, mut state, uri) = using_fixture().await;
            let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
            change(&mut state, &uri, 2, USING_SOURCE.replace("// completion", "x.twice("));
            let completion = start_request(crate::handlers::completion(
                &mut state,
                completion_params(&uri, Position::new(8, 10)),
            ));
            let signature = start_request(crate::handlers::signature_help(
                &mut state,
                signature_params(&uri, Position::new(8, 16)),
            ));
            if published {
                let mut snapshot = state.snapshot();
                let result = analyze(snapshot.analysis_batches(Vec::new()).pop().unwrap());
                assert!(
                    snapshot
                        .publish_analysis(state.analysis_version.load(Ordering::Acquire), result)
                );
            }
            if configuration {
                let _ = crate::handlers::did_change_configuration(
                    &mut state,
                    DidChangeConfigurationParams { settings: serde_json::Value::Null },
                );
            } else {
                change(&mut state, &uri, 3, USING_SOURCE.replace("// completion", "x.triple("));
            }
            assert_eq!(expect_ready(completion).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
            assert_eq!(expect_ready(signature).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
            drop(gate);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_refresh_changed_imports() {
    let fixture = support::RequestFixture::new(
        r#"
        //- /Math.sol open
        library Math {
            function twice(uint256 value, uint256 amount) internal pure returns (uint256) {
                return value + amount;
            }
        }
        //- /Request.sol open
        import {Math} from "./Math.sol";
        contract C {
            using Math for uint256;
            function f(uint256 x) public pure {
                x.$1twice($2 1);
            }
        }
        "#,
        "/Request.sol",
    );
    let mut state = fixture.state();
    let (uri, completion_position) = fixture.marker_location("$1");
    let (_, signature_position) = fixture.marker_location("$2");
    let imported = Url::from_file_path(fixture.project_path("/Math.sol")).unwrap();
    let changed = fixture.project_contents("/Math.sol").replace("amount", "count").replace(
        "library Math {",
        "library Math { function triple(uint256 value) internal pure returns (uint256) { return value * 3; }",
    );
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &imported, 1, &changed);
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, completion_position),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, signature_position),
    ));
    change(&mut state, &imported, 2, changed.replace("count", "increment"));
    assert_eq!(expect_ready(completion).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    assert_eq!(expect_ready(signature).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, completion_position),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, signature_position),
    ));
    drop(gate);
    let response = within("completion", completion).await.unwrap();
    assert_eq!(completion_labels(response), ["triple", "twice"]);
    let response = within("signature help", signature).await.unwrap().unwrap();
    snapbox::assert_data_eq!(
        response.signatures[0].label.as_str(),
        snapbox::str!["function twice(uint256 increment) internal pure returns (uint256)"]
    );
}

#[tokio::test(flavor = "current_thread")]
async fn completion_and_signature_help_share_analysis_through_identical_edits() {
    let (_project, mut state, uri) = using_fixture().await;
    let source = USING_SOURCE.replace("// completion", "x.twice(");
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, &source);
    let coordinator = state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone();
    let completion = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ));
    let signature = start_request(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ));
    change(&mut state, &uri, 3, &source);
    let second = start_request(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ));
    assert_eq!(
        state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.id(),
        coordinator.id()
    );
    drop(gate);
    let response = within("completion", completion).await.unwrap();
    assert_eq!(completion_labels(response), ["twice"]);
    assert_eq!(completion_labels(second.await.unwrap()), ["twice"]);
    let response = within("signature help", signature).await.unwrap().unwrap();
    let ready_completion = expect_ready(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(8, 10)),
    ))
    .unwrap();
    assert_eq!(completion_labels(ready_completion), ["twice"]);
    let ready_signature = expect_ready(crate::handlers::signature_help(
        &mut state,
        signature_params(&uri, Position::new(8, 16)),
    ))
    .unwrap()
    .unwrap();
    assert_eq!(response, ready_signature);
}

#[tokio::test(flavor = "current_thread")]
async fn ready_completion_and_signature_help_recheck_inputs_when_polled() {
    let (_project, mut state, uri) = using_fixture().await;
    let completion =
        crate::handlers::completion(&mut state, completion_params(&uri, Position::new(8, 8)));
    let signature =
        crate::handlers::signature_help(&mut state, signature_params(&uri, Position::new(8, 8)));
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    change(&mut state, &uri, 2, USING_SOURCE.replace("uint256 x;", "uint256 other;"));
    assert_eq!(expect_ready(completion).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    assert_eq!(expect_ready(signature).unwrap_err().code, ErrorCode::CONTENT_MODIFIED);
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn completion_syntax_and_invalid_positions_do_not_wait_for_analysis() {
    let (_project, mut state, uri) = using_fixture().await;
    Arc::make_mut(&mut state.config).enable_completion_snippets();
    let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
    for (version, source, position) in [
        (2, "///\ncontract C {}", Position::new(0, 3)),
        (3, "import \"./\";", Position::new(0, 10)),
    ] {
        change(&mut state, &uri, version, source);
        let response = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params(&uri, position),
        ))
        .unwrap();
        assert!(!completion_labels(response).is_empty());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_some());
    }
    change(&mut state, &uri, 4, USING_SOURCE);
    for trigger in ["/", "*", "\"", "'"] {
        let mut params = completion_params(&uri, Position::new(7, 17));
        params.context = Some(lsp_types::CompletionContext {
            trigger_kind: lsp_types::CompletionTriggerKind::TRIGGER_CHARACTER,
            trigger_character: Some(trigger.into()),
        });
        let response = expect_ready(crate::handlers::completion(&mut state, params)).unwrap();
        assert!(completion_labels(response).is_empty());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_some());
    }
    let response = expect_ready(crate::handlers::completion(
        &mut state,
        completion_params(&uri, Position::new(u32::MAX, u32::MAX)),
    ))
    .unwrap();
    assert!(completion_labels(response).is_empty());
    change(&mut state, &uri, 5, format!("//😀\n{USING_SOURCE}"));
    for (request_uri, position) in [
        (uri.clone(), Position::new(0, 3)),
        (uri.join("Missing.sol").unwrap(), Position::new(0, 0)),
    ] {
        let response = expect_ready(crate::handlers::completion(
            &mut state,
            completion_params(&request_uri, position),
        ))
        .unwrap();
        assert!(completion_labels(response).is_empty());
        let response = expect_ready(crate::handlers::signature_help(
            &mut state,
            signature_params(&request_uri, position),
        ))
        .unwrap();
        assert!(response.is_none());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_some());
    }
    drop(gate);
}

#[tokio::test(flavor = "current_thread")]
async fn aliased_interactive_requests_refresh_analysis_and_clamp_signature_positions() {
    for spelling in ["%52equest.sol", "/Request.sol", "nested%2F..%2FRequest.sol"] {
        let (_project, mut state, uri) = using_fixture().await;
        let alias = Url::parse(&uri.as_str().replacen("Request.sol", spelling, 1)).unwrap();
        let source = USING_SOURCE
            .replace("{Math.twice}", "{Math.triple}")
            .replace("// completion\n    }\n}", "x.triple(");
        let gate = state.analysis_scheduler.gate.clone().acquire_owned().await.unwrap();
        change(&mut state, &uri, 2, &source);
        let mut completion = std::pin::pin!(crate::handlers::completion(
            &mut state,
            completion_params(&alias, Position::new(8, 10)),
        ));
        let mut signature = std::pin::pin!(crate::handlers::signature_help(
            &mut state,
            signature_params(&alias, Position::new(8, u32::MAX)),
        ));
        let mut eof_signature = std::pin::pin!(crate::handlers::signature_help(
            &mut state,
            signature_params(&alias, Position::new(u32::MAX, u32::MAX)),
        ));
        let mut context = Context::from_waker(Waker::noop());
        assert!(completion.as_mut().poll(&mut context).is_pending());
        assert!(signature.as_mut().poll(&mut context).is_pending());
        assert!(eof_signature.as_mut().poll(&mut context).is_pending());
        assert!(state.analysis_scheduler.tasks.lock().debounce.is_none());
        drop(gate);

        let response = within("completion", completion).await.unwrap();
        assert_eq!(completion_labels(response), ["triple"]);
        let response = within("signature help", signature).await.unwrap().unwrap();
        snapbox::assert_data_eq!(
            response.signatures[0].label.as_str(),
            snapbox::str!["function triple() internal pure returns (uint256)"]
        );
        assert_eq!(
            within("EOF signature help", eof_signature).await.unwrap(),
            Some(response.clone())
        );
        let ready = expect_ready(crate::handlers::signature_help(
            &mut state,
            signature_params(&alias, Position::new(8, u32::MAX)),
        ))
        .unwrap();
        assert_eq!(ready, Some(response));
    }
}

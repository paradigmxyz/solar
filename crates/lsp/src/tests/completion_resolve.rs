use super::{
    AnalysisBatch, AnalysisResultAccumulator, GlobalState, analyze, support::RequestFixture,
};
use crate::test_support::MarkedProject;
use async_lsp::{AnyRequest, ClientSocket, router::Router};
use lsp_types::{
    CompletionClientCapabilities, CompletionItem, CompletionItemCapability,
    CompletionItemCapabilityResolveSupport, CompletionItemKind, CompletionParams,
    CompletionResponse, Documentation, InitializeParams, MarkupKind, PartialResultParams,
    TextDocumentClientCapabilities, TextDocumentIdentifier, TextDocumentPositionParams,
    WorkDoneProgressParams, request, request::Request,
};
use solar_config::{CompileOpts, ImportRemapping};
use std::{
    future::Future,
    task::{Context, Poll, Waker},
};
use tower::Service;

const PLAIN_DOCUMENTATION: &str = r#"function documented(uint256 value) public pure returns (uint256 result)

Adds one to the provided value.

@param

value: The value to increment.

@return

result: The incremented value."#;

#[tokio::test(flavor = "current_thread")]
async fn source_completion_uses_compact_resolve_data() {
    let fixture = completion_resolve_fixture();
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let (uri, start) = fixture.marker_location("$2");
    let (_, end) = fixture.marker_location("$3");

    assert_eq!(
        item.data,
        Some(serde_json::json!([1, uri, start.line, start.character, end.line, end.character,])),
    );
}

#[tokio::test(flavor = "current_thread")]
async fn resolves_source_completion_documentation_without_changing_identity() {
    for (formats, resolve_properties, expected) in [
        (
            vec![MarkupKind::PlainText, MarkupKind::Markdown],
            Some(vec!["documentation".into()]),
            Documentation::String(PLAIN_DOCUMENTATION.to_string()),
        ),
        (
            vec![MarkupKind::Markdown, MarkupKind::PlainText],
            None,
            Documentation::MarkupContent(lsp_types::MarkupContent {
                kind: MarkupKind::Markdown,
                value: r#"```solidity
function documented(uint256 value) public pure returns (uint256 result)
```

Adds one to the provided value.

**@param**

- `value`: The value to increment.

**@return**

- `result`: The incremented value."#
                    .to_string(),
            }),
        ),
    ] {
        let fixture = completion_resolve_fixture();
        let mut router = crate::new_router_with_state(fixture.state());
        request_initialize_with_resolve_support(&mut router, formats, resolve_properties).await;
        let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;

        assert!(item.data.is_some(), "source completion should carry resolve data");
        assert!(item.documentation.is_none(), "documentation should be deferred");

        let original = item.clone();
        let resolved = request_resolve_item(&mut router, item).await;
        assert_eq!(resolved.documentation, Some(expected));

        let mut unresolved = resolved;
        unresolved.documentation = None;
        assert_eq!(unresolved, original);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn sends_documentation_eagerly_when_client_cannot_resolve_it() {
    let fixture = completion_resolve_fixture();
    let mut router = crate::new_router_with_state(fixture.state());
    request_initialize_with_resolve_support(
        &mut router,
        vec![MarkupKind::PlainText],
        Some(vec!["additionalTextEdits".into()]),
    )
    .await;

    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;

    assert!(item.data.is_some(), "source completion should carry resolve data");
    assert_eq!(item.documentation, Some(Documentation::String(PLAIN_DOCUMENTATION.to_string())));
    let resolved = request_resolve_item(&mut router, item.clone()).await;
    assert_eq!(resolved, item);
}

#[tokio::test(flavor = "current_thread")]
async fn resolves_imported_public_getter_member_completion_documentation() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Main.sol open
        import {Token} from "./Token.sol";

        contract C {
            function read(Token token) public view returns (uint256) {
                return token.bal$1();
            }
        }

        //- /Token.sol
        contract Token {
            /// @notice Returns the current balance.
            uint256 public balance;
        }
        "#,
        "/Main.sol",
    );
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "balance").await;
    let token_uri = lsp_types::Url::from_file_path(fixture.project_path("/Token.sol")).unwrap();

    assert_eq!(item.kind, Some(CompletionItemKind::METHOD));
    assert!(item.data.is_some(), "public getter completion should carry source resolve data");
    assert_eq!(item.data.as_ref().unwrap()[1], serde_json::json!(token_uri));
    assert!(item.documentation.is_none(), "documentation should be deferred");

    let original = item.clone();
    let resolved = request_resolve_item(&mut router, item).await;
    assert_eq!(
        resolved.documentation,
        Some(Documentation::String(
            "uint256 public balance\n\nReturns the current balance.".into(),
        )),
    );
    let mut unresolved = resolved;
    unresolved.documentation = None;
    assert_eq!(unresolved, original);
}

#[tokio::test(flavor = "current_thread")]
async fn returns_stale_completion_item_unchanged_when_symbol_is_deleted() {
    let fixture = completion_resolve_fixture();
    let state = fixture.state();
    let symbol_tables = state.symbol_tables.clone();
    let mut router = crate::new_router_with_state(state);
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let replacement = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(
            fixture.project_path("/Completion.sol"),
            "contract C { function use() public pure {} }".into(),
        )],
    ));
    assert!(replacement.diagnostics.is_empty());
    *symbol_tables.write() = replacement.symbol_tables;

    let resolved = request_resolve_item(&mut router, item.clone()).await;

    assert_eq!(resolved, item);
}

#[tokio::test(flavor = "current_thread")]
async fn validates_completion_data_before_waiting_and_uses_latest_analysis() {
    let fixture = completion_resolve_fixture();
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let replacement_contents = fixture.project_contents("/Completion.sol").replacen(
        "Adds one to the provided value.",
        "Uses documentation from the latest analysis.",
        1,
    );
    let replacement = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(fixture.project_path("/Completion.sol"), replacement_contents)],
    ));
    assert!(replacement.diagnostics.is_empty());
    let mut state = fixture.state();
    state.mark_analysis_pending_for_test();

    let mut malformed = item.clone();
    malformed.data = Some(serde_json::json!({ "version": "invalid" }));
    let mut malformed_request =
        std::pin::pin!(crate::handlers::resolve_completion_item(&mut state, malformed.clone(),));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);
    let Poll::Ready(response) = malformed_request.as_mut().poll(&mut context) else {
        panic!("malformed completion data should not wait for analysis");
    };
    assert_eq!(response.unwrap(), malformed);

    let mut wrong_coordinate_type = item.clone();
    wrong_coordinate_type.data.as_mut().unwrap()[2] = serde_json::json!("invalid");
    let mut wrong_coordinate_request = std::pin::pin!(crate::handlers::resolve_completion_item(
        &mut state,
        wrong_coordinate_type.clone(),
    ));
    let Poll::Ready(response) = wrong_coordinate_request.as_mut().poll(&mut context) else {
        panic!("invalid completion data coordinates should not wait for analysis");
    };
    assert_eq!(response.unwrap(), wrong_coordinate_type);

    let mut non_file = item.clone();
    non_file.data.as_mut().unwrap()[1] = serde_json::json!("untitled:Completion.sol");
    let mut non_file_request =
        std::pin::pin!(crate::handlers::resolve_completion_item(&mut state, non_file.clone(),));
    let Poll::Ready(response) = non_file_request.as_mut().poll(&mut context) else {
        panic!("non-file completion data should not wait for analysis");
    };
    assert_eq!(response.unwrap(), non_file);

    let mut request =
        std::pin::pin!(crate::handlers::resolve_completion_item(&mut state, item.clone()));
    assert!(request.as_mut().poll(&mut context).is_pending());

    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(1, replacement.symbol_tables));
    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("resolve should complete after analysis is published");
    };
    let mut resolved = response.unwrap();
    assert_eq!(
        resolved.documentation,
        Some(Documentation::String(PLAIN_DOCUMENTATION.replacen(
            "Adds one to the provided value.",
            "Uses documentation from the latest analysis.",
            1,
        ))),
    );
    resolved.documentation = None;
    assert_eq!(resolved, item);
}

#[tokio::test(flavor = "current_thread")]
async fn resolves_only_compatible_completion_items_across_analysis_batches() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Shared.sol open
        import {Base} from "@dep/Base.sol";
        contract C is Base {
            /// @inheritdoc Base
            function $2documented(uint256 value)
                public
                pure
                override
                returns (uint256 result)
            {
                return value + 1;
            }

            function use() public pure {
                documented$1(1);
            }
        }

        //- /left/Main.sol
        import "../Shared.sol";

        //- /equivalent/Base.sol
        abstract contract Base {
            /// @notice Shared documentation.
            /// @notice Second paragraph.
            function documented(uint256 value)
                public
                pure
                virtual
                returns (uint256 result);
        }

        //- /equivalent/Main.sol
        import "../Shared.sol";

        //- /right/Base.sol
        abstract contract Base {
            /// @notice Documentation from the right context.
            function documented(uint256 value)
                public
                pure
                virtual
                returns (uint256 result);
        }

        //- /right/Main.sol
        import "../Shared.sol";
        "#,
    );
    let project = marked.project();
    project.write_file(
        "/left/Base.sol",
        concat!(
            "abstract contract Base {\n",
            "    /** @notice Shared documentation.\n",
            "\n",
            "Second paragraph. */\n",
            "    function documented(uint256 value)\n",
            "        public\n",
            "        pure\n",
            "        virtual\n",
            "        returns (uint256 result);\n",
            "}\n",
        ),
    );
    let uri = lsp_types::Url::from_file_path(project.path("/Shared.sol")).unwrap();
    let analyze_context = |entry_directory: &str, dependency_directory: &str| {
        let opts = CompileOpts {
            base_path: Some(project.root().to_path_buf()),
            import_remappings: vec![ImportRemapping {
                context: String::new(),
                prefix: "@dep/".into(),
                path: project.path(dependency_directory).to_string_lossy().into_owned(),
            }],
            ..Default::default()
        };
        let entry = format!("{entry_directory}/Main.sol");
        analyze(AnalysisBatch::from_files(
            opts,
            [(project.path(&entry), project.read_file(&entry))],
        ))
    };
    let left = analyze_context("/left", "/left");
    let equivalent = analyze_context("/equivalent", "/equivalent");
    assert!(left.diagnostics.is_empty(), "{:#?}", left.diagnostics);
    assert!(equivalent.diagnostics.is_empty(), "{:#?}", equivalent.diagnostics);
    assert_eq!(
        left.symbol_tables.hover(&uri, marked.marker("$2").position()),
        equivalent.symbol_tables.hover(&uri, marked.marker("$2").position()),
        "structurally different NatSpec should render identically",
    );

    let state = GlobalState::new(ClientSocket::new_closed());
    *state.vfs.write() = project.vfs();
    let symbol_tables = state.symbol_tables.clone();
    *symbol_tables.write() = left.symbol_tables.clone();
    let mut router = crate::new_router_with_state(state);
    let item = request_completion_item_at(
        &mut router,
        uri.clone(),
        marked.marker("$1").position(),
        "documented",
    )
    .await;
    assert!(item.data.is_some(), "source completion should carry resolve data");
    assert!(item.documentation.is_none(), "documentation should be deferred");

    let mut results = AnalysisResultAccumulator::default();
    results.push(left);
    results.push(equivalent);
    *symbol_tables.write() = results.finish().symbol_tables;

    let resolved = request_resolve_item(&mut router, item.clone()).await;
    assert!(resolved.documentation.is_some());
    let mut unresolved = resolved;
    unresolved.documentation = None;
    assert_eq!(unresolved, item);

    let left = analyze_context("/left", "/left");
    let right = analyze_context("/right", "/right");
    assert!(left.diagnostics.is_empty(), "{:#?}", left.diagnostics);
    assert!(right.diagnostics.is_empty(), "{:#?}", right.diagnostics);
    assert_ne!(
        left.symbol_tables.hover(&uri, marked.marker("$2").position()),
        right.symbol_tables.hover(&uri, marked.marker("$2").position()),
        "incompatible analysis contexts should resolve different inherited documentation",
    );
    let mut results = AnalysisResultAccumulator::default();
    results.push(left);
    results.push(right);
    *symbol_tables.write() = results.finish().symbol_tables;

    let resolved = request_resolve_item(&mut router, item.clone()).await;

    assert_eq!(resolved, item);
}

#[tokio::test(flavor = "current_thread")]
async fn returns_completion_item_unchanged_for_conflicting_source_snapshots() {
    let fixture = completion_resolve_fixture();
    let state = fixture.state();
    let symbol_tables = state.symbol_tables.clone();
    let mut router = crate::new_router_with_state(state);
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let path = fixture.project_path("/Completion.sol");
    let contents = fixture.project_contents("/Completion.sol");
    let current = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path.clone(), contents.clone())],
    ));
    let shifted = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path, format!("\n{contents}"))],
    ));
    assert!(current.diagnostics.is_empty());
    assert!(shifted.diagnostics.is_empty());
    let mut results = AnalysisResultAccumulator::default();
    results.push(current);
    results.push(shifted);
    *symbol_tables.write() = results.finish().symbol_tables;

    let resolved = request_resolve_item(&mut router, item.clone()).await;

    assert_eq!(resolved, item);
}

#[tokio::test(flavor = "current_thread")]
async fn returns_untrusted_completion_items_unchanged() {
    let fixture = completion_resolve_fixture();
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let mut items = Vec::new();

    let mut missing = item.clone();
    missing.data = None;
    items.push(missing);

    let mut malformed = item.clone();
    malformed.data = Some(serde_json::json!({ "version": "invalid" }));
    malformed.documentation = Some(Documentation::String("client documentation".into()));
    items.push(malformed);

    let mut unknown_version = item.clone();
    unknown_version.data.as_mut().unwrap()[0] = serde_json::json!(2);
    items.push(unknown_version);

    let mut extra_field = item.clone();
    extra_field.data.as_mut().unwrap().as_array_mut().unwrap().push(serde_json::json!(true));
    items.push(extra_field);

    let mut missing_field = item.clone();
    missing_field.data.as_mut().unwrap().as_array_mut().unwrap().pop();
    items.push(missing_field);

    let mut non_file_uri = item.clone();
    non_file_uri.data.as_mut().unwrap()[1] = serde_json::json!("untitled:Completion.sol");
    items.push(non_file_uri);

    let mut wrong_kind = item.clone();
    wrong_kind.kind = Some(CompletionItemKind::TEXT);
    items.push(wrong_kind);

    let mut wrong_range = item.clone();
    wrong_range.data.as_mut().unwrap()[2] = serde_json::json!(999);
    items.push(wrong_range);

    let mut wrong_identity = item;
    wrong_identity.label = "replacement".into();
    items.push(wrong_identity);

    for item in items {
        let resolved = request_resolve_item(&mut router, item.clone()).await;
        assert_eq!(resolved, item);
    }
}

fn completion_resolve_fixture() -> RequestFixture {
    RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            /// @notice Adds one to the provided value.
            /// @param value The value to increment.
            /// @return result The incremented value.
            function $2documented$3(uint256 value) public pure returns (uint256 result) {
                return value + 1;
            }

            function use() public pure {
                documented$1(1);
            }
        }
        "#,
        "/Completion.sol",
    )
}

async fn request_initialize_with_resolve_support(
    router: &mut Router<GlobalState>,
    documentation_format: Vec<MarkupKind>,
    resolve_properties: Option<Vec<String>>,
) {
    let mut params = InitializeParams::default();
    params.capabilities.text_document = Some(TextDocumentClientCapabilities {
        completion: Some(CompletionClientCapabilities {
            completion_item: Some(CompletionItemCapability {
                documentation_format: Some(documentation_format),
                resolve_support: resolve_properties
                    .map(|properties| CompletionItemCapabilityResolveSupport { properties }),
                ..Default::default()
            }),
            ..Default::default()
        }),
        ..Default::default()
    });
    let initialize = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 0,
        "method": request::Initialize::METHOD,
        "params": params,
    }))
    .unwrap();
    router.call(initialize).await.unwrap();
}

async fn request_completion_item(
    router: &mut Router<GlobalState>,
    fixture: &RequestFixture,
    marker: &str,
    label: &str,
) -> CompletionItem {
    let (uri, position) = fixture.marker_location(marker);
    request_completion_item_at(router, uri, position, label).await
}

async fn request_completion_item_at(
    router: &mut Router<GlobalState>,
    uri: lsp_types::Url,
    position: lsp_types::Position,
    label: &str,
) -> CompletionItem {
    let completion = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 1,
        "method": request::Completion::METHOD,
        "params": CompletionParams {
            text_document_position: TextDocumentPositionParams {
                text_document: TextDocumentIdentifier::new(uri),
                position,
            },
            work_done_progress_params: WorkDoneProgressParams::default(),
            partial_result_params: PartialResultParams::default(),
            context: None,
        },
    }))
    .unwrap();
    let response = router.call(completion).await.unwrap();
    let Some(CompletionResponse::Array(items)) =
        serde_json::from_value::<Option<CompletionResponse>>(response).unwrap()
    else {
        panic!("expected completion items");
    };
    items.into_iter().find(|item| item.label == label).unwrap()
}

async fn request_resolve_item(
    router: &mut Router<GlobalState>,
    item: CompletionItem,
) -> CompletionItem {
    let resolve = serde_json::from_value::<AnyRequest>(serde_json::json!({
        "id": 2,
        "method": request::ResolveCompletionItem::METHOD,
        "params": item,
    }))
    .unwrap();
    let response = router.call(resolve).await.unwrap();
    serde_json::from_value(response).unwrap()
}

use super::*;
use async_lsp::AnyRequest;
use lsp_types::{
    CompletionItem, CompletionItemKind, CompletionResponse, Documentation, MarkupContent,
    MarkupKind,
    request::{Completion, Initialize, Request, ResolveCompletionItem},
};
use solar_config::ImportRemapping;
use std::pin::{Pin, pin};
use tower::Service;

const PLAIN_DOCUMENTATION: &str = r#"function documented(uint256 value) public pure returns (uint256 result)

Adds one to the provided value.

@param

value: The value to increment.

@return

result: The incremented value."#;

#[tokio::test(flavor = "current_thread")]
async fn resolves_source_completion_documentation_without_changing_identity() {
    let markdown = MarkupContent {
        kind: MarkupKind::Markdown,
        value: r#"```solidity
function documented(uint256 value) public pure returns (uint256 result)
```

Adds one to the provided value.

**@param**

- `value`: The value to increment.

**@return**

- `result`: The incremented value."#
            .into(),
    };
    for (formats, resolve_properties, expected, eager) in [
        (
            vec![MarkupKind::PlainText, MarkupKind::Markdown],
            Some(vec!["documentation"]),
            Documentation::String(PLAIN_DOCUMENTATION.into()),
            false,
        ),
        (
            vec![MarkupKind::Markdown, MarkupKind::PlainText],
            None,
            Documentation::MarkupContent(markdown),
            false,
        ),
        (
            vec![MarkupKind::PlainText],
            Some(vec!["additionalTextEdits"]),
            Documentation::String(PLAIN_DOCUMENTATION.into()),
            true,
        ),
    ] {
        let fixture = completion_resolve_fixture();
        let mut router = crate::new_router_with_state(fixture.state());
        let resolve_support =
            resolve_properties.map(|properties| json!({ "properties": properties }));
        let completion_item =
            json!({ "documentationFormat": formats, "resolveSupport": resolve_support });
        let params = from_json(json!({ "capabilities": { "textDocument": {
            "completion": { "completionItem": completion_item },
        } } }));
        request::<Initialize>(&mut router, params).await;
        let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;

        let (uri, start) = fixture.marker_location("$2");
        let (_, end) = fixture.marker_location("$3");
        let data = json!([1, uri, start.line, start.character, end.line, end.character]);
        assert_eq!(item.data, Some(data));
        if eager {
            assert_eq!(item.documentation, Some(expected));
            assert_eq!(request::<ResolveCompletionItem>(&mut router, item.clone()).await, item);
        } else {
            check_resolved_documentation(&mut router, item, expected).await;
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn resolves_imported_getter_and_alias_documentation() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Main.sol open
        import {Token} from "./Token.sol";
        import {Math as Numbers} from "./Math.sol";

        contract C {
            using Num$2bers for uint256;

            function read(Token token) public view returns (uint256) {
                return token.bal$1();
            }
        }

        //- /Token.sol
        contract Token {
            /// @notice Returns the current balance.
            uint256 public balance;
        }

        //- /Math.sol
        /// @notice Integer helpers.
        library Math {}
        "#,
        "/Main.sol",
    );
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "balance").await;
    let token_uri = fixture.project().uri("/Token.sol");
    assert_eq!(item.kind, Some(CompletionItemKind::METHOD));
    assert_eq!(item.data.as_ref().unwrap()[1], json!(token_uri));
    let documentation = "uint256 public balance\n\nReturns the current balance.";
    check_resolved_documentation(&mut router, item, Documentation::String(documentation.into()))
        .await;

    let item = request_completion_item(&mut router, &fixture, "$2", "Numbers").await;
    let documentation = Documentation::String("library Math\n\nInteger helpers.".into());
    check_resolved_documentation(&mut router, item, documentation).await;
}

#[tokio::test(flavor = "current_thread")]
async fn returns_items_unchanged_for_deleted_or_conflicting_symbols() {
    let fixture = completion_resolve_fixture();
    let state = fixture.state();
    let symbol_tables = state.symbol_tables.clone();
    let mut router = crate::new_router_with_state(state);
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let path = fixture.project_path("/Completion.sol");
    let contents = fixture.project_contents("/Completion.sol");

    let deleted =
        analyze_clean(path.clone(), "contract C { function use() public pure {} }".into());
    symbol_tables.store(Arc::new(deleted.symbol_tables));
    assert_eq!(request::<ResolveCompletionItem>(&mut router, item.clone()).await, item);

    let mut results = AnalysisResultAccumulator::default();
    results.push(analyze_clean(path.clone(), contents.clone()));
    results.push(analyze_clean(path, format!("\n{contents}")));
    symbol_tables.store(Arc::new(results.finish().symbol_tables));
    assert_eq!(request::<ResolveCompletionItem>(&mut router, item.clone()).await, item);
}

#[tokio::test(flavor = "current_thread")]
async fn validates_completion_data_before_waiting_and_uses_latest_analysis() {
    let fixture = completion_resolve_fixture();
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let replacement_documentation = "Uses documentation from the latest analysis.";
    let replacement_contents = fixture.project_contents("/Completion.sol").replacen(
        "Adds one to the provided value.",
        replacement_documentation,
        1,
    );
    let replacement = analyze_clean(fixture.project_path("/Completion.sol"), replacement_contents);
    let mut state = fixture.state();
    state.mark_analysis_pending_for_test();

    let with_data = |edit: fn(&mut Value)| {
        let mut item = item.clone();
        edit(item.data.as_mut().unwrap());
        item
    };
    let mut missing = item.clone();
    missing.data = None;
    let mut malformed = item.clone();
    malformed.data = Some(json!({ "version": "invalid" }));
    malformed.documentation = Some(Documentation::String("client documentation".into()));
    for invalid in [
        missing,
        malformed,
        with_data(|data| data[0] = json!(2)),
        with_data(|data| data.as_array_mut().unwrap().push(json!(true))),
        with_data(|data| {
            data.as_array_mut().unwrap().pop();
        }),
        with_data(|data| data[1] = json!("untitled:Completion.sol")),
        with_data(|data| data[2] = json!("invalid")),
    ] {
        let mut request =
            pin!(crate::handlers::resolve_completion_item(&mut state, invalid.clone()));
        let Poll::Ready(response) = poll_once(request.as_mut()) else {
            panic!("invalid completion data should not wait for analysis");
        };
        assert_eq!(response.unwrap(), invalid);
    }

    let mut wrong_kind = item.clone();
    wrong_kind.kind = Some(CompletionItemKind::TEXT);
    let mut wrong_label = item.clone();
    wrong_label.label = "replacement".into();
    let mut requests =
        [item.clone(), wrong_kind, wrong_label, with_data(|data| data[2] = json!(999))].map(
            |item| {
                (item.clone(), Box::pin(crate::handlers::resolve_completion_item(&mut state, item)))
            },
        );
    for (_, request) in &mut requests {
        assert!(poll_once(request.as_mut()).is_pending());
    }

    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(1, Arc::new(replacement.symbol_tables)));
    for (index, (item, request)) in requests.iter_mut().enumerate() {
        let Poll::Ready(response) = poll_once(request.as_mut()) else {
            panic!("resolve should complete after analysis is published");
        };
        let mut resolved = response.unwrap();
        if index == 0 {
            let documentation = PLAIN_DOCUMENTATION.replacen(
                "Adds one to the provided value.",
                replacement_documentation,
                1,
            );
            assert_eq!(resolved.documentation, Some(Documentation::String(documentation)));
            resolved.documentation = None;
        }
        assert_eq!(&resolved, item);
    }
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
    let uri = project.uri("/Shared.sol");
    let hover_position = marked.marker("$2").position();
    let analyze_context = |directory: &str| {
        let opts = CompileOpts {
            base_path: Some(project.root().to_path_buf()),
            import_remappings: vec![ImportRemapping {
                context: String::new(),
                prefix: "@dep/".into(),
                path: format!("{}/", project.path(directory).display()),
            }],
            ..Default::default()
        };
        let entry = format!("{directory}/Main.sol");
        let result = analyze(AnalysisBatch::from_files(
            opts,
            [(project.path(&entry), project.read_file(&entry))],
        ));
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        result
    };
    let left = analyze_context("/left");
    let equivalent = analyze_context("/equivalent");
    assert_eq!(
        left.symbol_tables.hover(&uri, hover_position),
        equivalent.symbol_tables.hover(&uri, hover_position),
        "structurally different NatSpec should render identically",
    );

    let state = GlobalState::new(ClientSocket::new_closed());
    *state.vfs.write() = project.vfs();
    let symbol_tables = state.symbol_tables.clone();
    symbol_tables.store(Arc::new(left.symbol_tables.clone()));
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
    symbol_tables.store(Arc::new(results.finish().symbol_tables));
    let mut resolved = request::<ResolveCompletionItem>(&mut router, item.clone()).await;
    assert!(resolved.documentation.take().is_some());
    assert_eq!(resolved, item);

    let left = analyze_context("/left");
    let right = analyze_context("/right");
    assert_ne!(
        left.symbol_tables.hover(&uri, hover_position),
        right.symbol_tables.hover(&uri, hover_position),
        "incompatible analysis contexts should resolve different inherited documentation",
    );
    let mut results = AnalysisResultAccumulator::default();
    results.push(left);
    results.push(right);
    symbol_tables.store(Arc::new(results.finish().symbol_tables));
    assert_eq!(request::<ResolveCompletionItem>(&mut router, item.clone()).await, item);
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

fn analyze_clean(path: PathBuf, contents: String) -> AnalysisResult {
    let result = analyze_source(path, contents);
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    result
}

fn poll_once<F: Future>(future: Pin<&mut F>) -> Poll<F::Output> {
    future.poll(&mut Context::from_waker(Waker::noop()))
}

async fn request<R: Request>(router: &mut Router<GlobalState>, params: R::Params) -> R::Result {
    let request = json!({ "id": 0, "method": R::METHOD, "params": params });
    let response = router.call(from_json::<AnyRequest>(request)).await;
    serde_json::from_value(response.unwrap()).unwrap()
}

/// Resolves a deferred item and checks that only its documentation changes.
async fn check_resolved_documentation(
    router: &mut Router<GlobalState>,
    item: CompletionItem,
    expected: Documentation,
) {
    assert!(item.data.is_some(), "source completion should carry resolve data");
    assert!(item.documentation.is_none(), "documentation should be deferred");
    let mut resolved = request::<ResolveCompletionItem>(router, item.clone()).await;
    assert_eq!(resolved.documentation.take(), Some(expected));
    assert_eq!(resolved, item);
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
    uri: Url,
    position: Position,
    label: &str,
) -> CompletionItem {
    let params = request_params(&uri, position, json!({}));
    let Some(CompletionResponse::Array(items)) = request::<Completion>(router, params).await else {
        panic!("expected completion items");
    };
    items.into_iter().find(|item| item.label == label).unwrap()
}

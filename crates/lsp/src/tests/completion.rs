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
use snapbox::str;
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
async fn resolves_source_completion_documentation_without_changing_identity() {
    for (formats, expected) in [
        (
            vec![MarkupKind::PlainText, MarkupKind::Markdown],
            Documentation::String(PLAIN_DOCUMENTATION.to_string()),
        ),
        (
            vec![MarkupKind::Markdown, MarkupKind::PlainText],
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
        request_initialize(&mut router, formats).await;
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
async fn resolves_public_getter_member_completion_documentation() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Getter.sol open
        contract Token {
            /// @notice Returns the current balance.
            uint256 public balance;
        }

        contract C {
            function read(Token token) public view returns (uint256) {
                return token.bal$1();
            }
        }
        "#,
        "/Getter.sol",
    );
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "balance").await;

    assert_eq!(item.kind, Some(CompletionItemKind::METHOD));
    assert!(item.data.is_some(), "public getter completion should carry source resolve data");
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
async fn validates_completion_data_before_waiting_for_latest_analysis() {
    let fixture = completion_resolve_fixture();
    let mut router = crate::new_router_with_state(fixture.state());
    let item = request_completion_item(&mut router, &fixture, "$1", "documented").await;
    let replacement = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(
            fixture.project_path("/Completion.sol"),
            "contract C { function use() public pure {} }".into(),
        )],
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

    let mut non_file = item.clone();
    non_file.data.as_mut().unwrap()["uri"] = serde_json::json!("untitled:Completion.sol");
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
    assert_eq!(response.unwrap(), item);
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

        //- /left/Base.sol
        abstract contract Base {
            /// @notice Documentation from the left context.
            function documented(uint256 value)
                public
                pure
                virtual
                returns (uint256 result);
        }

        //- /left/Main.sol
        import "../Shared.sol";

        //- /duplicate/Main.sol
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
    let duplicate = analyze_context("/duplicate", "/left");
    assert!(left.diagnostics.is_empty(), "{:#?}", left.diagnostics);
    assert!(duplicate.diagnostics.is_empty(), "{:#?}", duplicate.diagnostics);
    assert_eq!(
        left.symbol_tables.hover(&uri, marked.marker("$2").position()),
        duplicate.symbol_tables.hover(&uri, marked.marker("$2").position()),
        "equivalent analysis contexts should resolve the same inherited documentation",
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
    results.push(duplicate);
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
    unknown_version.data.as_mut().unwrap()["version"] = serde_json::json!(2);
    items.push(unknown_version);

    let mut unknown_field = item.clone();
    unknown_field.data.as_mut().unwrap()["unexpected"] = serde_json::json!(true);
    items.push(unknown_field);

    let mut non_file_uri = item.clone();
    non_file_uri.data.as_mut().unwrap()["uri"] = serde_json::json!("untitled:Completion.sol");
    items.push(non_file_uri);

    let mut wrong_kind = item.clone();
    wrong_kind.kind = Some(CompletionItemKind::TEXT);
    items.push(wrong_kind);

    let mut wrong_range = item.clone();
    wrong_range.data.as_mut().unwrap()["selectionRange"]["start"]["line"] = serde_json::json!(999);
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
            function documented(uint256 value) public pure returns (uint256 result) {
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

async fn request_initialize(
    router: &mut Router<GlobalState>,
    documentation_format: Vec<MarkupKind>,
) {
    request_initialize_with_resolve_support(router, documentation_format, None).await;
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

#[test]
fn uses_utf16_ranges_with_non_bmp_source_text() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        // 😀
        ///$1
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 1:0-1:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_contracts() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ///$1
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_named_function_parameters_and_return() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256 amount, uint256) external pure returns (uint256 total) {
                return amount;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @return total $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_unnamed_function_parameter_and_return() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256, address recipient) external pure returns (uint256) {
                return uint160(recipient);
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param recipient $2
    /// @return $3$0

"#]],
    );
}

#[test]
fn deduplicates_parameter_names_and_keeps_all_returns() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256 amount, uint256 amount)
                external
                pure
                returns (uint256 total, uint256)
            {
                return (amount, amount);
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @return total $3
    /// @return $4$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_contract_kinds() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ///$1
        abstract contract AbstractVault {}
        ///$2
        interface IVault {}
        ///$3
        library VaultMath {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec abstract contract documentation
kind=Snippet
detail=abstract contract AbstractVault
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec interface documentation
kind=Snippet
detail=interface IVault
sort_text=0
text_edit=edit 2:0-2:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec library documentation
kind=Snippet
detail=library VaultMath
sort_text=0
text_edit=edit 4:0-4:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_special_functions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            constructor(uint256 ownerSeed, address) {}
            ///$2
            fallback(bytes calldata input) external returns (bytes memory output) {
                output = input;
            }
            ///$3
            receive() external payable {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec constructor documentation
kind=Snippet
detail=constructor
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param ownerSeed $2$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec fallback documentation
kind=Snippet
detail=fallback
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param input $2
    /// @return output $3$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec receive documentation
kind=Snippet
detail=receive
sort_text=0
text_edit=edit 7:4-7:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_events_errors_structs_and_enums() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            event Transfer(address indexed from, address indexed, uint256 amount);
            ///$2
            error TransferFailed(uint256 code, address);
            ///$3
            struct Record {
                uint256 amount;
                address owner;
            }
            ///$4
            enum Status { Pending, Complete }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec event documentation
kind=Snippet
detail=event Transfer
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param from $2
    /// @param amount $3$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec error documentation
kind=Snippet
detail=error TransferFailed
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param code $2$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec struct documentation
kind=Snippet
detail=struct Record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @param owner $3$0

"#]],
    );
    fixture.check_completion_details(
        "$4",
        str![[r#"
label=NatSpec enum documentation
kind=Snippet
detail=enum Status
sort_text=0
text_edit=edit 10:4-10:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn completes_line_natspec_for_state_variables_and_getter_returns() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
                uint256[] samples;
                mapping(address account => uint256 balance) balances;
            }
            ///$1
            uint256 public total;
            ///$2
            Record public record;
            ///$3
            uint256 private secret;
            ///$4
            uint256 internal cached;
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable total
sort_text=0
text_edit=edit 7:4-7:7
insert_text_format=Snippet
new_text:
/// @notice $1
    /// @return $2$0

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 9:4-9:7
insert_text_format=Snippet
new_text:
/// @notice $1
    /// @return amount $2
    /// @return owner $3$0

"#]],
    );
    fixture.check_completion_details(
        "$3",
        str![[r#"
label=NatSpec private state variable documentation
kind=Snippet
detail=private state variable secret
sort_text=0
text_edit=edit 11:4-11:7
insert_text_format=Snippet
new_text:
/// @dev $1$0

"#]],
    );
    fixture.check_completion_details(
        "$4",
        str![[r#"
label=NatSpec internal state variable documentation
kind=Snippet
detail=internal state variable cached
sort_text=0
text_edit=edit 13:4-13:7
insert_text_format=Snippet
new_text:
/// @dev $1$0

"#]],
    );
}

#[test]
fn completes_full_and_inheritdoc_templates_for_multiple_bases() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface First { function value(uint256 amount) external view returns (uint256 total); }
        interface Second { function value(uint256 amount) external view returns (uint256 total); }
        contract Child is First, Second {
            ///$1
            function value(uint256 amount)
                external
                pure
                override(First, Second)
                returns (uint256 total)
            {
                total = amount;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param amount $2
    /// @return total $3$0

label=NatSpec @inheritdoc First
kind=Snippet
detail=Inherit documentation from First
sort_text=1:First
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// @inheritdoc First$0

label=NatSpec @inheritdoc Second
kind=Snippet
detail=Inherit documentation from Second
sort_text=1:Second
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Second$0

"#]],
    );
}

#[test]
fn completes_inheritdoc_for_overridden_fallback() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract Base { fallback() external virtual {} }
        contract Child is Base {
            ///$1
            fallback() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec fallback documentation
kind=Snippet
detail=fallback
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

label=NatSpec @inheritdoc Base
kind=Snippet
detail=Inherit documentation from Base
sort_text=1:Base
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Base$0

"#]],
    );
}

#[test]
fn completes_inheritdoc_with_a_named_import_alias() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /Base.sol
        interface Original {
            function value() external view returns (uint256 result);
        }

        //- /Completion.sol open
        import {Original as Alias} from "./Base.sol";
        contract Child is Alias {
            ///$1
            function value() external pure override returns (uint256 result) {
                result = 1;
            }
        }
        "#,
        &["/Completion.sol"],
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1
    /// @return result $2$0

label=NatSpec @inheritdoc Alias
kind=Snippet
detail=Inherit documentation from Alias
sort_text=1:Alias
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Alias$0

"#]],
    );
}

#[test]
fn completes_inheritdoc_with_a_reexported_import_alias() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        interface Original { function value() external; }

        //- /Middle.sol
        import {Original as Alias} from "./Base.sol";

        //- /Completion.sol open
        import "./Middle.sol";
        contract Child is Alias {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

label=NatSpec @inheritdoc Alias
kind=Snippet
detail=Inherit documentation from Alias
sort_text=1:Alias
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Alias$0

"#]],
    );
}

#[test]
fn omits_inheritdoc_for_a_base_function_with_a_different_signature() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        interface Base { function value(uint256 amount) external; }
        contract Child is Base {
            ///$1
            function value(address account) external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1
    /// @param account $2$0

"#]],
    );
}

#[test]
fn preserves_dollar_identifiers_in_plain_text_completion() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            function value(uint256 $amount) external pure returns (uint256 $result) {
                $result = $amount;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_snippets(
        "$1",
        false,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=PlainText
new_text:
///
    /// @param $amount
    /// @return $result

"#]],
    );
}

#[test]
fn completes_closed_and_unclosed_block_natspec() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        /**$1 */
        contract Vault {}
        /**$2
        contract OpenVault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:6
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
    fixture.check_completion_details(
        "$2",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract OpenVault
sort_text=0
text_edit=edit 2:0-2:3
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn falls_back_to_ordinary_completion_after_closed_block_natspec() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        /** docs */ contract C { function f() external pure { ret$1urn; } }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
revert Function

"#]],
    );
}

#[test]
fn completes_closed_block_natspec_before_same_line_declaration() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        /**$1 */ contract C {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract C
sort_text=0
text_edit=edit 0:0-0:6
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn completes_multiline_block_natspec_with_non_overlapping_edits() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        /**$1
         *
         */
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details(
        "$1",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:3
additional_text_edit=0:3-2:3 new_text=""
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn current_vfs_syntax_wins_over_stale_state_variable_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            ///$1
            uint256 public value;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("public", "private");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec private state variable documentation
kind=Snippet
detail=private state variable value
sort_text=0
text_edit=edit 1:4-1:7
insert_text_format=Snippet
new_text:
/// @dev $1$0

"#]],
    );
}

#[test]
fn pending_analysis_omits_stale_getter_returns_without_waiting() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
            }
            ///$1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("owner", "admin");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/// @notice $1$0

"#]],
    );
}

#[test]
fn pending_analysis_omits_stale_inheritdoc_without_waiting() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface First { function value() external; }
        interface Other { function value() external; }
        contract Child is First {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let changed =
        fixture.project_contents("/Completion.sol").replace("Child is First", "Child is Other");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 3:4-3:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn pending_context_change_omits_inheritdoc_without_waiting() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface Base { function value() external; }
        contract Child is Base {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_after_context_change(
        "$1",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn pending_trivia_only_change_keeps_getter_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
            }
            // $1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/// @notice $1
    /// @return amount $2
    /// @return owner $3$0

"#]],
    );
}

#[test]
fn pending_unclosed_block_keeps_getter_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            struct Record {
                uint256 amount;
                address owner;
            }
            // $1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "/**");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 5:4-5:7
insert_text_format=Snippet
new_text:
/**
     * @notice $1
     * @return amount $2
     * @return owner $3$0
     */

"#]],
    );
}

#[test]
fn pending_unclosed_block_keeps_inheritdoc_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface Base { function value() external; }
        contract Child is Base {
            // $1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "/**");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/**
     * $1$0
     */

label=NatSpec @inheritdoc Base
kind=Snippet
detail=Inherit documentation from Base
sort_text=1:Base
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/**
     * @inheritdoc Base$0
     */

"#]],
    );
}

#[test]
fn pending_trivia_only_change_keeps_inheritdoc_semantics() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        interface Base { function value() external; }
        contract Child is Base {
            // $1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let changed = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_change(
        "$1",
        "/Completion.sol",
        &changed,
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

label=NatSpec @inheritdoc Base
kind=Snippet
detail=Inherit documentation from Base
sort_text=1:Base
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @inheritdoc Base$0

"#]],
    );
}

#[test]
fn pending_imported_struct_change_omits_stale_getter_returns() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol open
        struct Record {
            uint256 amount;
            address owner;
        }

        //- /Completion.sol open
        import {Record} from "./Base.sol";
        contract C {
            // $1
            Record public record;
        }
        "#,
        "/Completion.sol",
    );
    let base = fixture.project_contents("/Base.sol").replace("owner", "admin");
    let completion = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_changes(
        "$1",
        "/Completion.sol",
        &[("/Base.sol", &base), ("/Completion.sol", &completion)],
        str![[r#"
label=NatSpec public state variable documentation
kind=Snippet
detail=public state variable record
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// @notice $1$0

"#]],
    );
}

#[test]
fn pending_base_signature_change_omits_stale_inheritdoc() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol open
        interface Base { function value() external; }

        //- /Completion.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            // $1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );
    let base = fixture.project_contents("/Base.sol").replace("value", "other");
    let completion = fixture.project_contents("/Completion.sol").replace("// ", "///");

    fixture.check_completion_details_after_changes(
        "$1",
        "/Completion.sol",
        &[("/Base.sol", &base), ("/Completion.sol", &completion)],
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn deleted_base_source_omits_stale_inheritdoc() {
    let fixture = RequestFixture::new(
        r#"
        //- /Base.sol
        interface Base { function value() external; }

        //- /Completion.sol open
        import {Base} from "./Base.sol";
        contract Child is Base {
            ///$1
            function value() external override {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_after_deleted_source(
        "$1",
        "/Completion.sol",
        "/Base.sol",
        str![[r#"
label=NatSpec function documentation
kind=Snippet
detail=function value
sort_text=0
text_edit=edit 2:4-2:7
insert_text_format=Snippet
new_text:
/// $1$0

"#]],
    );
}

#[test]
fn falls_back_to_plain_text_natspec_when_snippets_are_unsupported() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ///$1
        contract Vault {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_snippets(
        "$1",
        false,
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract Vault
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=PlainText
new_text:
/// @title
/// @author
/// @notice

"#]],
    );
}

#[test]
fn rejects_invalid_nonempty_separated_and_unsupported_natspec_targets() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        ////$1
        contract FourSlashes {}
        /**/$2
        contract EmptyBlock {}
        /***/$3
        contract ThreeStars {}
        /// existing documentation$4
        contract NonEmpty {}
        ///$5
        // intervening comment
        contract Separated {}
        contract C {
            ///$6
            modifier onlyOwner() { _; }
        }
        ///$7
        type Price is uint256;
        "#,
        "/Completion.sol",
    );

    for marker in ["$1", "$2", "$3", "$4", "$5", "$6", "$7"] {
        fixture.check_completion_details(marker, str![""]);
    }
}

#[test]
fn comment_triggers_complete_natspec_templates() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        ///$1
        contract LineDocs {}
        /**$2
        contract BlockDocs {}
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_trigger(
        "$1",
        "/",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract LineDocs
sort_text=0
text_edit=edit 0:0-0:3
insert_text_format=Snippet
new_text:
/// @title $1
/// @author $2
/// @notice $3$0

"#]],
    );
    fixture.check_completion_details_with_trigger(
        "$2",
        "*",
        str![[r#"
label=NatSpec contract documentation
kind=Snippet
detail=contract BlockDocs
sort_text=0
text_edit=edit 2:0-2:3
insert_text_format=Snippet
new_text:
/**
 * @title $1
 * @author $2
 * @notice $3$0
 */

"#]],
    );
}

#[test]
fn comment_triggers_outside_natspec_do_not_leak_symbol_completions() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            //$1
            function first() external {}
            //*$2
            function second() external {}
            /*$3 */
            function third() external {}
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion_details_with_trigger("$1", "/", str![""]);
    fixture.check_completion_details_with_trigger("$2", "*", str![""]);
    fixture.check_completion_details_with_trigger("$3", "*", str![""]);
}

#[test]
fn completes_symbols_in_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Symbols.sol open
        contract C {
            uint256 stateValue;

            function target(uint256 input) public view returns (uint256 output) {
                uint256 localValue = input + stateValue;
                output = $1localValue;
            }
        }
        "#,
        "/Symbols.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
gasleft Function
input Variable
keccak256 Function
localValue Variable
msg Module
mulmod Function
output Variable
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
stateValue Property
target Method
tx Module

"#]],
    );
}

#[test]
fn filters_locals_by_declaration_scope() {
    let fixture = RequestFixture::new(
        r#"
        //- /Completion.sol open
        contract C {
            function f(uint256 input) public pure {
                uint256 localValue = $1input + 1;
                uint256 nextValue = $2localValue;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
f Method
gasleft Function
input Variable
keccak256 Function
msg Module
mulmod Function
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
tx Module

"#]],
    );
    fixture.check_completion(
        "$2",
        str![[r#"
C Class
abi Module
addmod Function
assert Function
blobhash Function
block Module
blockhash Function
ecrecover Function
erc7201 Function
f Method
gasleft Function
input Variable
keccak256 Function
localValue Variable
msg Module
mulmod Function
require Function
revert Function
ripemd160 Function
selfdestruct Function
sha256 Function
tx Module

"#]],
    );
}

#[test]
fn completes_dirty_members_from_typed_receivers() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract Token {
            uint256 public balance;
        }

        contract C {
            Token[] tokens;
            Token public token;
            Token foo;

            function getToken() public view returns (Token) {
                return token;
            }

            function read(uint256 i) public view {
                getToken().$1;
                (this.token()).$2b;
                tokens[i].bal$3;
                foo.$4;
                foo
                    .bal$5;
            }
        }
        "#,
        "/Completion.sol",
    );
    let expected = str![[r#"
balance Method

"#]];

    fixture.check_completion("$1", expected.clone());
    fixture.check_completion("$2", expected.clone());
    fixture.check_completion("$3", expected.clone());
    fixture.check_completion("$4", expected.clone());
    fixture.check_completion("$5", expected);
}

#[test]
fn completes_builtin_members_and_filters_globals() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            function f() public view {
                msg.$1;
                tx.$2;
                tx.$3
                block.$4;
                abi.$5;
                ms$6;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
data Method
gas Method
sender Method
sig Method
value Method

"#]],
    );
    fixture.check_completion(
        "$2",
        str![[r#"
gasprice Method
origin Method

"#]],
    );
    fixture.check_completion(
        "$3",
        str![[r#"
gasprice Function
origin Function

"#]],
    );
    fixture.check_completion(
        "$4",
        str![[r#"
basefee Function
blobbasefee Function
chainid Function
coinbase Function
difficulty Function
gaslimit Function
number Function
prevrandao Function
timestamp Function

"#]],
    );
    fixture.check_completion(
        "$5",
        str![[r#"
decode Method
encode Method
encodeCall Method
encodePacked Method
encodeWithSelector Method
encodeWithSignature Method

"#]],
    );
    fixture.check_completion(
        "$6",
        str![[r#"
msg Module

"#]],
    );
}

#[test]
fn completes_partial_member_prefixes_from_vfs_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /Completion.sol open
        contract C {
            struct Data {
                uint256 field;
                uint256 other;
            }

            function f() public pure {
                Data memory data;
                data.$1;
                data.f$2;
            }
        }
        "#,
        "/Completion.sol",
    );

    fixture.check_completion(
        "$1",
        str![[r#"
field Property
other Property

"#]],
    );
    fixture.check_completion(
        "$2",
        str![[r#"
field Property

"#]],
    );
}

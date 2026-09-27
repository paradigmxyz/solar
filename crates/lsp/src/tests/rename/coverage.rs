use super::*;
use crate::{
    FoundryWorkspaceConfig, LaunchConfig, config::negotiate_capabilities_with_pull_diagnostic_data,
    handlers, test_support::MarkedProject,
};
use async_lsp::ClientSocket;
use lsp_types::{RenameParams, TextDocumentIdentifier, TextDocumentPositionParams};

#[tokio::test(flavor = "current_thread")]
async fn rejects_rename_with_unindexed_callers() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        //- /src/Counter.sol
        contract Counter {
            function $1increment() public {}
        }
        //- /test/Counter.t.sol
        import "../src/Counter.sol";
        contract CounterTest {
            function run(Counter counter) public { counter.increment(); }
        }
        "#,
    );
    let mut state = coverage_state(&marked, false);
    let params = rename_params(&marked, "$1", "increase");
    let position = params.text_document_position.clone();
    let error = tokio::time::timeout(
        super::super::ASYNC_TEST_TIMEOUT,
        handlers::rename(&mut state, params),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    snapbox::assert_data_eq!(
        error.message,
        "cannot rename this symbol because workspace indexing may omit source files",
    );
    let error = handlers::prepare_rename(&mut state, position).await.unwrap_err();
    assert_eq!(error.code, ErrorCode::REQUEST_FAILED);
    assert!(
        handlers::rename(&mut state, rename_params(&marked, "$1", "increment"))
            .await
            .unwrap()
            .is_none()
    );
}

#[tokio::test(flavor = "current_thread")]
async fn incomplete_coverage_distinguishes_locals_from_named_arguments() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        //- /src/Counter.sol
        contract Counter {
            mapping(address $1owner => uint256) public balances;
            function increase(uint256 $2amount) public pure returns (uint256 $3result) {
                uint256 $4local = amount;
                result = local;
            }
            function attempt() public returns (string memory) {
                try this.increase(1) {} catch Error(string memory $5reason) { return reason; }
                return "";
            }
        }
        //- /test/Counter.t.sol
        import "../src/Counter.sol";
        contract CounterTest {
            function run(Counter counter) public view returns (uint256) {
                return counter.increase({$6amount: 1}) + counter.balances({$7owner: address(this)});
            }
        }
        "#,
    );
    for complete in [false, true] {
        let mut state = coverage_state(&marked, complete);
        tokio::time::timeout(super::super::ASYNC_TEST_TIMEOUT, state.latest_analysis())
            .await
            .unwrap()
            .unwrap();
        for (marker, references) in [("$1", 1), ("$2", 2), ("$3", 2), ("$4", 2), ("$5", 2)] {
            let params = rename_params(&marked, marker, "renamed");
            let prepare =
                handlers::prepare_rename(&mut state, params.text_document_position.clone()).await;
            let renamed = handlers::rename(&mut state, params).await;
            if !complete && matches!(marker, "$1" | "$2") {
                assert_eq!(prepare.unwrap_err().code, ErrorCode::REQUEST_FAILED);
                assert_eq!(renamed.unwrap_err().code, ErrorCode::REQUEST_FAILED);
                continue;
            }
            assert!(prepare.unwrap().is_some());
            let changes = renamed.unwrap().unwrap().changes.unwrap();
            let source_uri =
                Url::from_file_path(marked.project().path("/src/Counter.sol")).unwrap();
            let source = &changes[&source_uri];
            assert_eq!(source.len(), references, "{marker}");
            assert!(source.iter().all(|edit| edit.new_text == "renamed"));
            if matches!(marker, "$1" | "$2") {
                let caller_uri =
                    Url::from_file_path(marked.project().path("/test/Counter.t.sol")).unwrap();
                let caller = &changes[&caller_uri];
                let reference = marked.marker(if marker == "$1" { "$7" } else { "$6" }).position();
                let length = if marker == "$1" { 5 } else { 6 };
                assert_eq!(
                    caller,
                    &[lsp_types::TextEdit::new(
                        lsp_types::Range::new(
                            reference,
                            lsp_types::Position::new(reference.line, reference.character + length)
                        ),
                        "renamed".into(),
                    )]
                );
                assert_eq!(changes.len(), 2);
            } else {
                assert_eq!(changes.len(), 1);
            }
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn rename_coverage_uses_the_analyzed_config_after_merging_batches() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /First.sol
        contract First {}
        //- /Shared.sol
        contract Shared {
            function $1run() public pure returns (uint256 $2result) {
                uint256 $3local = 1;
                result = local;
            }
        }
        //- /Caller.sol
        import "./Shared.sol";
        contract Caller { function use(Shared value) public pure { value.run(); } }
        "#,
        &["/First.sol", "/Shared.sol", "/Caller.sol"],
    );
    for marker in ["$1", "$2", "$3"] {
        let (mut state, params) = fixture.rename_state_and_params(marker, "renamed");
        assert!(!state.config.may_omit_source_files());
        let mut analyzed_config = (*state.config).clone();
        analyzed_config.mark_analysis_source_files_incomplete();
        state.analysis_commit.lock().analysis_config = Some(Arc::new(analyzed_config));
        let prepare =
            handlers::prepare_rename(&mut state, params.text_document_position.clone()).await;
        let renamed = handlers::rename(&mut state, params).await;
        if marker == "$1" {
            assert_eq!(prepare.unwrap_err().code, ErrorCode::REQUEST_FAILED);
            assert_eq!(renamed.unwrap_err().code, ErrorCode::REQUEST_FAILED);
        } else {
            assert!(prepare.unwrap().is_some());
            let changes = renamed.unwrap().unwrap().changes.unwrap();
            assert_eq!(changes.len(), 1);
            assert_eq!(changes.values().next().unwrap().len(), 2);
        }
    }
}

fn coverage_state(marked: &MarkedProject, complete: bool) -> GlobalState {
    let launch = LaunchConfig::default().with_foundry_workspace_config_loader(move |root| {
        let roots = if complete { &["src", "test", "script"][..] } else { &["src"][..] };
        Ok::<_, std::convert::Infallible>(
            FoundryWorkspaceConfig::new(root)
                .with_source_roots(roots.iter().copied())
                .with_flycheck_source_roots(["src", "test", "script"]),
        )
    });
    let (_, mut config) = negotiate_capabilities_with_pull_diagnostic_data(
        marked.project().initialize_params(),
        false,
        &launch,
    );
    config.rediscover_workspaces();
    assert_eq!(config.may_omit_source_files(), !complete);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.recompute_after_opening_source(Vec::new());
    state
}

fn rename_params(marked: &MarkedProject, marker: &str, new_name: &str) -> RenameParams {
    let marker = marked.marker(marker);
    RenameParams {
        text_document_position: TextDocumentPositionParams::new(
            TextDocumentIdentifier::new(
                Url::from_file_path(marked.project().path(marker.path())).unwrap(),
            ),
            marker.position(),
        ),
        new_name: new_name.into(),
        work_done_progress_params: Default::default(),
    }
}

use super::*;
use crate::{
    FoundryWorkspaceConfig, LaunchConfig, config::negotiate_capabilities_with_pull_diagnostic_data,
    handlers, test_support::MarkedProject,
};
use async_lsp::ClientSocket;
use lsp_types::{RenameParams, TextDocumentIdentifier, TextDocumentPositionParams};
use std::fmt::Write as _;

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
    let report = tokio::time::timeout(
        super::super::ASYNC_TEST_TIMEOUT,
        rename_report(&mut state, params, marked.project().root()),
    )
    .await
    .unwrap();
    snapbox::assert_data_eq!(
        report,
        "cannot rename this symbol because workspace indexing may omit source files\n"
    );
    let unchanged = rename_params(&marked, "$1", "increment");
    assert!(handlers::rename(&mut state, unchanged).await.unwrap().is_none());
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
                return counter.increase({amount: 1}) + counter.balances({owner: address(this)});
            }
        }
        "#,
    );
    for (complete, expected) in [
        (
            false,
            str![[r#"
$1:
cannot rename this symbol because workspace indexing may omit source files
$2:
cannot rename this symbol because workspace indexing may omit source files
$3:
/src/Counter.sol:2:67-2:73 -> renamed
/src/Counter.sol:4:8-4:14 -> renamed
$4:
/src/Counter.sol:3:16-3:21 -> renamed
/src/Counter.sol:4:17-4:22 -> renamed
$5:
/src/Counter.sol:7:58-7:64 -> renamed
/src/Counter.sol:7:75-7:81 -> renamed

"#]],
        ),
        (
            true,
            str![[r#"
$1:
/src/Counter.sol:1:20-1:25 -> renamed
/test/Counter.t.sol:3:65-3:70 -> renamed
$2:
/src/Counter.sol:2:30-2:36 -> renamed
/src/Counter.sol:3:24-3:30 -> renamed
/test/Counter.t.sol:3:33-3:39 -> renamed
$3:
/src/Counter.sol:2:67-2:73 -> renamed
/src/Counter.sol:4:8-4:14 -> renamed
$4:
/src/Counter.sol:3:16-3:21 -> renamed
/src/Counter.sol:4:17-4:22 -> renamed
$5:
/src/Counter.sol:7:58-7:64 -> renamed
/src/Counter.sol:7:75-7:81 -> renamed

"#]],
        ),
    ] {
        let mut state = coverage_state(&marked, complete);
        tokio::time::timeout(super::super::ASYNC_TEST_TIMEOUT, state.latest_analysis())
            .await
            .unwrap()
            .unwrap();
        let mut output = String::new();
        for marker in ["$1", "$2", "$3", "$4", "$5"] {
            let params = rename_params(&marked, marker, "renamed");
            let report = rename_report(&mut state, params, marked.project().root()).await;
            write!(output, "{marker}:\n{report}").unwrap();
        }
        snapbox::assert_data_eq!(output, expected);
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
    let mut output = String::new();
    for marker in ["$1", "$2", "$3"] {
        let (mut state, params) = fixture.rename_state_and_params(marker, "renamed");
        assert!(!state.config.may_omit_source_files());
        let mut analyzed_config = (*state.config).clone();
        analyzed_config.mark_analysis_source_files_incomplete();
        state.analysis_commit.lock().analysis_config = Some(Arc::new(analyzed_config));
        let report = rename_report(&mut state, params, &fixture.project_path("/")).await;
        write!(output, "{marker}:\n{report}").unwrap();
    }
    snapbox::assert_data_eq!(
        output,
        str![[r#"
$1:
cannot rename this symbol because workspace indexing may omit source files
$2:
/Shared.sol:1:48-1:54 -> renamed
/Shared.sol:3:8-3:14 -> renamed
$3:
/Shared.sol:2:16-2:21 -> renamed
/Shared.sol:3:17-3:22 -> renamed

"#]]
    );
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

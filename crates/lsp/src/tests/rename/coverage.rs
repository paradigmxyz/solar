use super::*;
use crate::{
    FoundryWorkspaceConfig, LaunchConfig, config::negotiate_capabilities_with_pull_diagnostic_data,
    handlers,
};
use std::fmt::Write as _;

#[tokio::test(flavor = "current_thread")]
async fn incomplete_coverage_distinguishes_locals_from_named_arguments() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        //- /src/Counter.sol
        contract Counter {
            mapping(address $1owner => uint256) public balances;
            function $6increase(uint256 $2amount) public pure returns (uint256 $3result) {
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
$6:
cannot rename this symbol because workspace indexing may omit source files

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
$6:
/src/Counter.sol:2:13-2:21 -> renamed
/src/Counter.sol:7:17-7:25 -> renamed
/test/Counter.t.sol:3:23-3:31 -> renamed

"#]],
        ),
    ] {
        let mut state = coverage_state(&marked, complete);
        // Renaming to the current name is a no-op even when callers may be unindexed.
        let unchanged = rename_at(&marked, "$6", "increase");
        assert!(handlers::rename(&mut state, unchanged).await.unwrap().is_none());
        settle(&state).await;
        let mut output = String::new();
        for marker in ["$1", "$2", "$3", "$4", "$5", "$6"] {
            let params = rename_at(&marked, marker, "renamed");
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
        publish_analysis_config(&state, false);
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
    let mut state = state_with(config);
    state.recompute_after_opening_source(Vec::new());
    state
}

fn rename_at(marked: &MarkedProject, marker: &str, new_name: &str) -> RenameParams {
    let (uri, position) = marked.location(marker);
    rename_params(&uri, position, new_name)
}

use super::{
    GlobalState,
    support::{Query, RequestFixture},
};
use crate::vfs::VfsPath;
use async_lsp::ErrorCode;
use crop::Rope;
use lsp_types::{
    DidChangeWatchedFilesParams, FileChangeType, FileEvent, InitializeParams, Url, WorkspaceFolder,
};
use snapbox::str;
use std::{
    sync::{Arc, atomic::Ordering},
    task::{Context, Poll, Waker},
    time::Duration,
};

#[tokio::test(flavor = "current_thread")]
async fn remappings_change_refreshes_import_definitions() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false

        //- /remappings.txt
        pkg/=lib/old/

        //- /src/Main.sol open
        import "pkg/$1Target.sol";

        //- /lib/old/Target.sol
        contract OldTarget {}

        //- /lib/new/Target.sol
        contract NewTarget {}
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    assert_eq!(
        fixture.query_in(&mut state, Query::Definition, "$1").await,
        "/lib/old/Target.sol:0:0 contract OldTarget {}\n"
    );

    std::fs::write(fixture.project_path("/remappings.txt"), "pkg/=lib/new/\n").unwrap();
    let remappings_uri = Url::from_file_path(fixture.project_path("/remappings.txt")).unwrap();
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: remappings_uri, typ: FileChangeType::CHANGED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after remappings change should finish")
        .unwrap();

    assert_eq!(
        fixture.query_in(&mut state, Query::Definition, "$1").await,
        "/lib/new/Target.sol:0:0 contract NewTarget {}\n"
    );
}

/// Starts a definition request at `$1` while analysis is pending, applies `update`, and returns
/// the completed response.
fn definition_after(
    fixture: &RequestFixture,
    update: impl FnOnce(&mut GlobalState),
) -> Result<String, ErrorCode> {
    let mut state = fixture.state();
    state.mark_analysis_pending_for_test();
    let (uri, position) = fixture.marker_location("$1");
    let mut request = Query::Definition.request(&mut state, uri, position);
    let mut context = Context::from_waker(Waker::noop());
    assert!(request.as_mut().poll(&mut context).is_pending());

    update(&mut state);
    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("definition request should complete after analysis settles");
    };
    response.map(|response| fixture.response_output(response)).map_err(|error| error.code)
}

fn open_import_fixture() -> RequestFixture {
    RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";

        //- /Target.sol
        contract Target {}

        //- /OtherX.sol
        contract OtherX {}
        "#,
        "/Main.sol",
    )
}

#[test]
fn import_definition_discards_a_stale_vfs_result() {
    let fixture = open_import_fixture();
    let response = definition_after(&fixture, |state| {
        let old_tables = state.symbol_tables.load_full();
        state.vfs.write().set_file_contents(
            VfsPath::from(fixture.project_path("/Main.sol")),
            Some(Rope::from("import \"./Other.sol\";")),
        );
        assert!(state.snapshot().publish_symbol_tables(1, old_tables));
    });
    assert_eq!(response.as_deref(), Ok("<none>\n"));
}

#[test]
fn import_definition_discards_a_fallback_from_an_old_analysis_epoch() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/"]

        //- /src/Main.sol open
        import "pkg/$1Target.sol";

        //- /lib/Target.sol
        contract Target {}
        "#,
        "/src/Main.sol",
    );
    let response = definition_after(&fixture, |state| {
        state.mark_context_analysis_pending_for_test();
        assert!(state.snapshot().publish_symbol_tables(2, Default::default()));
    });
    assert_eq!(response, Err(ErrorCode::CONTENT_MODIFIED));
}

#[tokio::test(flavor = "current_thread")]
async fn import_definition_discards_the_index_after_current_analysis_fails() {
    let fixture = open_import_fixture();
    let error = tokio::spawn(async { panic!("test import analysis failure") }).await.unwrap_err();
    let response = definition_after(&fixture, |state| {
        let failed_version = state.analysis_version.load(Ordering::Acquire);
        assert!(
            crate::global_state::handle_analysis_failure(
                failed_version,
                error,
                &state.analysis_version,
                &state.published_analysis_version,
                &state.analysis_commit,
            )
            .is_some()
        );
    });
    assert_eq!(response.as_deref(), Ok("<none>\n"));
}

#[tokio::test(flavor = "current_thread")]
async fn import_definition_does_not_use_the_index_for_changed_current_literals() {
    // An incomplete literal and a literal naming another file both miss the stale index.
    for contents in ["import \"./Target.sol", "import \"./OtherX.sol\";"] {
        let fixture = open_import_fixture();
        let mut state = fixture.state();
        state.vfs.write().set_file_contents(
            VfsPath::from(fixture.project_path("/Main.sol")),
            Some(Rope::from(contents)),
        );
        assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");
    }
}

#[test]
fn resolves_import_literals_from_the_analysis_index() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        import "./$1Target.sol";
        import $2"./Target.sol";
        import "./nes$4ted/\
        Tar$3get.sol";

        //- /Target.sol
        contract Target {}

        //- /nested/Target.sol
        contract NestedTarget {}
        "#,
        "/Imports.sol",
    );

    fixture.check_queries(
        &[Query::Definition],
        1..=4,
        str![[r#"
$1 /Target.sol:0:0 contract Target {}
$2 /Target.sol:0:0 contract Target {}
$3 /nested/Target.sol:0:0 contract NestedTarget {}
$4 /nested/Target.sol:0:0 contract NestedTarget {}

"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn import_only_source_watcher_changes_refresh_auto_detected_remappings() {
    check_import_only_watcher_refreshes_auto_detected_remappings("/lib/pkg/src/Target.sol").await;
}

#[tokio::test(flavor = "current_thread")]
async fn import_only_package_watcher_changes_refresh_auto_detected_remappings() {
    check_import_only_watcher_refreshes_auto_detected_remappings("/lib/pkg").await;
}

#[tokio::test(flavor = "current_thread")]
async fn external_compile_only_library_events_do_not_force_rediscovery() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        libs = ["../external/lib"]

        //- /workspace/src/Main.sol open
        import "pkg/$1Target.sol";
        "#,
        "/workspace/src/Main.sol",
    );
    let mut state = fixture.state_with_workspace_analysis();
    let workspace = fixture.project_path("/workspace");
    let (_, mut config) = crate::config::negotiate_capabilities(InitializeParams {
        workspace_folders: Some(vec![WorkspaceFolder {
            uri: Url::from_file_path(&workspace).unwrap(),
            name: "workspace".into(),
        }]),
        ..Default::default()
    });
    config.rediscover_workspaces();
    state.config = Arc::new(config);
    let target = fixture.project_path("/external/lib/pkg/src/Target.sol");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "contract Target {}\n").unwrap();
    let version = state.analysis_version.load(Ordering::Acquire);

    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(target).unwrap(),
                typ: FileChangeType::CREATED,
            }],
        },
    );

    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    state.analysis_scheduler.tasks.lock().cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn manual_reindex_refreshes_auto_detected_remappings() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "pkg/$1Target.sol";
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state_with_workspace_analysis();
    let package = fixture.project_path("/lib/pkg");
    let target = package.join("src/Target.sol");
    assert!(!state.config.supports_watched_file_dynamic_registration());
    assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");

    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "contract Target {}\n").unwrap();
    state.reindex();
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("manual analysis after package creation should finish")
        .unwrap();

    assert_eq!(
        fixture.query_in(&mut state, Query::Definition, "$1").await,
        "/lib/pkg/src/Target.sol:0:0 contract Target {}\n"
    );

    std::fs::remove_dir_all(package).unwrap();
    state.reindex();
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("manual analysis after package deletion should finish")
        .unwrap();

    assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");
}

async fn check_import_only_watcher_refreshes_auto_detected_remappings(created_path: &str) {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "pkg/$1Target.sol";
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state_with_workspace_analysis();
    let target = fixture.project_path("/lib/pkg/src/Target.sol");

    assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");

    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "contract Target {}\n").unwrap();
    let event_path = fixture.project_path(created_path);
    let event_uri = Url::from_file_path(&event_path).unwrap();
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: event_uri.clone(), typ: FileChangeType::CREATED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after import-only creation should finish")
        .unwrap();

    assert_eq!(
        fixture.query_in(&mut state, Query::Definition, "$1").await,
        "/lib/pkg/src/Target.sol:0:0 contract Target {}\n"
    );

    if event_path == target {
        std::fs::remove_file(&target).unwrap();
    } else {
        std::fs::remove_dir_all(&event_path).unwrap();
    }
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: event_uri, typ: FileChangeType::DELETED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after import-only deletion should finish")
        .unwrap();

    assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");
}

#[test]
fn unresolved_import_literals_use_the_deepest_foundry_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/outer/"]

        //- /lib/outer/Target.sol
        contract OuterTarget {}

        //- /packages/app/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/inner/"]

        //- /packages/app/src/Main.sol open
        import "pkg/$1Target.sol";

        //- /packages/app/lib/inner/Target.sol
        contract InnerTarget {}
        "#,
        "/packages/app/src/Main.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/packages/app/lib/inner/Target.sol:0:0 contract InnerTarget {}

"#]],
    );
}

#[test]
fn external_source_roots_inside_an_ancestor_base_use_the_nested_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /outer/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/outer/"]

        //- /outer/lib/outer/Target.sol
        contract OuterTarget {}

        //- /outer/packages/app/foundry.toml
        [profile.default]
        src = "../../shared"
        auto_detect_remappings = false
        remappings = ["pkg/=lib/inner/"]

        //- /outer/shared/Main.sol open
        import "pkg/$1Target.sol";

        //- /outer/packages/app/lib/inner/Target.sol
        contract InnerTarget {}
        "#,
        "/outer/shared/Main.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/outer/packages/app/lib/inner/Target.sol:0:0 contract InnerTarget {}

"#]],
    );
}

#[test]
fn out_of_base_remapping_targets_keep_their_workspace_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /project/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=../shared/"]

        //- /project/src/Main.sol
        import "pkg/Consumer.sol";

        //- /shared/Consumer.sol open
        import "./$1Target.sol";

        //- /shared/Target.sol
        contract Target {}
        "#,
        "/shared/Consumer.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/shared/Target.sol:0:0 contract Target {}

"#]],
    );
}

#[test]
fn unowned_import_definitions_do_not_use_the_first_workspace_context() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /owned/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/"]

        //- /owned/lib/Target.sol
        contract Target {}

        //- /unowned/Main.sol open
        import "pkg/$1Target.sol";
        "#,
        "/unowned/Main.sol",
    );

    fixture.check_goto_definition("$1", "<none>\n");
}

#[tokio::test(flavor = "current_thread")]
async fn unowned_indexed_import_definitions_do_not_bypass_context() {
    let fixture = RequestFixture::new_in_batches(
        r#"
        //- /owned/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/"]

        //- /owned/lib/Target.sol
        contract Target {}

        //- /unowned/Main.sol open
        import "../owned/lib/$1Target.sol";
        "#,
        &["/unowned/Main.sol", "/owned/lib/Target.sol"],
    );

    let mut state = fixture.state();
    let importer = fixture.project_path("/unowned/Main.sol");
    state.vfs.write().set_file_contents(
        VfsPath::from(importer.clone()),
        Some(Rope::from(fixture.project_contents("/unowned/Main.sol"))),
    );
    {
        let mut commit = state.analysis_commit.lock();
        commit.vfs_content_revision = state.vfs.read().content_revision();
        commit.symbol_tables_version = state.analysis_version.load(Ordering::Acquire);
    }
    assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");
}

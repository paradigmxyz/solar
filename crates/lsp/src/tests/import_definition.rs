use super::{SymbolTables, support::RequestFixture};
use crate::vfs::VfsPath;
use crop::Rope;
use lsp_types::{
    DidChangeWatchedFilesParams, FileChangeType, FileEvent, GotoDefinitionParams,
    GotoDefinitionResponse, PartialResultParams, Position, TextDocumentIdentifier,
    TextDocumentPositionParams, Url, WorkDoneProgressParams,
};
use snapbox::str;
use std::{
    future::Future,
    path::PathBuf,
    sync::atomic::Ordering,
    task::{Context, Waker},
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
    let (uri, position) = fixture.marker_location("$1");

    assert_eq!(
        definition_target(&mut state, uri.clone(), position).await,
        Some(fixture.project_path("/lib/old/Target.sol"))
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
        definition_target(&mut state, uri, position).await,
        Some(fixture.project_path("/lib/new/Target.sol"))
    );
}

async fn definition_target(
    state: &mut super::GlobalState,
    uri: Url,
    position: Position,
) -> Option<PathBuf> {
    let response =
        crate::handlers::goto_definition(state, goto_params(uri, position)).await.ok()??;
    let location = match response {
        GotoDefinitionResponse::Scalar(location) => location,
        GotoDefinitionResponse::Array(locations) => locations.into_iter().next()?,
        GotoDefinitionResponse::Link(links) => {
            let link = links.into_iter().next()?;
            return link.target_uri.to_file_path().ok();
        }
    };
    location.uri.to_file_path().ok()
}

fn goto_params(uri: Url, position: Position) -> GotoDefinitionParams {
    GotoDefinitionParams {
        text_document_position_params: TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri),
            position,
        },
        work_done_progress_params: WorkDoneProgressParams::default(),
        partial_result_params: PartialResultParams::default(),
    }
}

#[test]
fn import_definition_discards_a_stale_vfs_result() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );
    let mut state = fixture.state();
    let old_tables = state.symbol_tables.read().clone();
    state.mark_analysis_pending_for_test();
    let (uri, position) = fixture.marker_location("$1");
    let params = goto_params(uri, position);
    let mut request = std::pin::pin!(crate::handlers::goto_definition(&mut state, params));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(request.as_mut().poll(&mut context).is_pending());

    state.vfs.write().set_file_contents(
        VfsPath::from(fixture.project_path("/Main.sol")),
        Some(Rope::from("import \"./Other.sol\";")),
    );
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(1, old_tables));

    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("definition request should complete after analysis is published");
    };
    assert_eq!(response.unwrap(), None);
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
    let mut state = fixture.state();
    state.mark_analysis_pending_for_test();
    let (uri, position) = fixture.marker_location("$1");
    let mut request =
        std::pin::pin!(crate::handlers::goto_definition(&mut state, goto_params(uri, position)));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(request.as_mut().poll(&mut context).is_pending());

    state.mark_context_analysis_pending_for_test();
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(2, SymbolTables::default()));

    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("definition request should complete after analysis is published");
    };
    assert_eq!(response.unwrap(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn import_definition_discards_the_index_after_current_analysis_fails() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );
    let mut state = fixture.state();
    state.mark_analysis_pending_for_test();
    let failed_version = state.analysis_version.load(Ordering::Acquire);
    let (uri, position) = fixture.marker_location("$1");
    let mut request =
        std::pin::pin!(crate::handlers::goto_definition(&mut state, goto_params(uri, position)));
    let waker = Waker::noop();
    let mut context = Context::from_waker(waker);

    assert!(request.as_mut().poll(&mut context).is_pending());

    let error = tokio::spawn(async { panic!("test import analysis failure") }).await.unwrap_err();
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

    let std::task::Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("definition request should complete after analysis fails");
    };
    assert_eq!(response.unwrap(), None);
}

#[tokio::test(flavor = "current_thread")]
async fn import_definition_does_not_use_the_index_for_an_incomplete_current_literal() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
        "/Main.sol",
    );
    let mut state = fixture.state();
    state.vfs.write().set_file_contents(
        VfsPath::from(fixture.project_path("/Main.sol")),
        Some(Rope::from("import \"./Target.sol")),
    );
    let (uri, position) = fixture.marker_location("$1");

    let response =
        crate::handlers::goto_definition(&mut state, goto_params(uri, position)).await.unwrap();

    assert_eq!(response, None);
}

#[tokio::test(flavor = "current_thread")]
async fn import_definition_discards_an_index_from_an_older_vfs_revision() {
    let fixture = RequestFixture::new(
        r#"
        //- /Main.sol open
        import "./$1Target.sol";

        //- /Target.sol
        contract Target {}

        //- /OtherX.sol
        contract OtherX {}
        "#,
        "/Main.sol",
    );
    let mut state = fixture.state();
    state.vfs.write().set_file_contents(
        VfsPath::from(fixture.project_path("/Main.sol")),
        Some(Rope::from("import \"./OtherX.sol\";")),
    );
    let (uri, position) = fixture.marker_location("$1");

    let response =
        crate::handlers::goto_definition(&mut state, goto_params(uri, position)).await.unwrap();

    assert_eq!(response, None);
}

#[test]
fn resolves_import_literals_from_the_analysis_index() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        import "./$1Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
        "/Imports.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Target.sol:0:0 contract Target {}

"#]],
    );
}

#[test]
fn resolves_import_literals_from_the_opening_quote() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        import $1"./Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
        "/Imports.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/Target.sol:0:0 contract Target {}

"#]],
    );
}

#[test]
fn resolves_import_literals_across_escaped_line_continuations() {
    let fixture = RequestFixture::new(
        r#"
        //- /Imports.sol
        import "./nes$2ted/\
            Tar$1get.sol";

        //- /nested/Target.sol
        contract Target {}
        "#,
        "/Imports.sol",
    );

    fixture.check_goto_definition(
        "$1",
        str![[r#"
/nested/Target.sol:0:0 contract Target {}

"#]],
    );
    fixture.check_goto_definition(
        "$2",
        str![[r#"
/nested/Target.sol:0:0 contract Target {}

"#]],
    );
}

#[tokio::test(flavor = "current_thread")]
async fn import_only_watcher_changes_refresh_auto_detected_remappings() {
    let fixture = RequestFixture::new_allowing_diagnostics(
        r#"
        //- /foundry.toml

        //- /src/Main.sol open
        import "pkg/$1Target.sol";
        "#,
        "/src/Main.sol",
    );
    let mut state = fixture.state();
    let (uri, position) = fixture.marker_location("$1");
    let target = fixture.project_path("/lib/pkg/src/Target.sol");

    assert_eq!(definition_target(&mut state, uri.clone(), position).await, None);

    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "contract Target {}\n").unwrap();
    let target_uri = Url::from_file_path(&target).unwrap();
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: target_uri.clone(), typ: FileChangeType::CREATED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after import-only creation should finish")
        .unwrap();

    assert_eq!(definition_target(&mut state, uri.clone(), position).await, Some(target.clone()));

    std::fs::remove_file(&target).unwrap();
    let _ = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri: target_uri, typ: FileChangeType::DELETED }],
        },
    );
    tokio::time::timeout(Duration::from_secs(5), state.latest_analysis())
        .await
        .expect("analysis after import-only deletion should finish")
        .unwrap();

    assert_eq!(definition_target(&mut state, uri, position).await, None);
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

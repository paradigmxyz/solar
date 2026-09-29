use super::*;
use snapbox::str;

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

    let remappings = fixture.project_path("/remappings.txt");
    std::fs::write(&remappings, "pkg/=lib/new/\n").unwrap();
    refresh(&mut state, Some((remappings, FileChangeType::CHANGED))).await;

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
    let mut request = start_request(Query::Definition.request(&mut state, uri, position));

    update(&mut state);
    let response = expect_ready(request.as_mut());
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

#[tokio::test(flavor = "current_thread")]
async fn import_definition_discards_stale_vfs_and_failed_analysis_results() {
    let fixture = open_import_fixture();
    let stale = definition_after(&fixture, |state| {
        let old_tables = state.symbol_tables.load_full();
        set_overlay(state, &fixture.project_path("/Main.sol"), "import \"./Other.sol\";", None);
        assert!(state.snapshot().publish_symbol_tables(1, old_tables));
    });
    assert_eq!(stale.as_deref(), Ok("<none>\n"));

    // The index is discarded after the current analysis fails.
    let error = tokio::spawn(async { panic!("test import analysis failure") }).await.unwrap_err();
    let failed = definition_after(&fixture, |state| {
        let failed_version = analysis_version(state);
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
    assert_eq!(failed.as_deref(), Ok("<none>\n"));
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
async fn import_definition_does_not_use_the_index_for_changed_current_literals() {
    // An incomplete literal and a literal naming another file both miss the stale index.
    for contents in ["import \"./Target.sol", "import \"./OtherX.sol\";"] {
        let fixture = open_import_fixture();
        let mut state = fixture.state();
        set_overlay(&state, &fixture.project_path("/Main.sol"), contents, None);
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
async fn import_only_changes_refresh_auto_detected_remappings() {
    // Watch the created file or package directory, or reindex manually without an event.
    for event_path in [Some("/lib/pkg/src/Target.sol"), Some("/lib/pkg"), None] {
        let fixture = RequestFixture::new_allowing_diagnostics(
            r#"
            //- /foundry.toml

            //- /src/Main.sol open
            import "pkg/$1Target.sol";
            "#,
            "/src/Main.sol",
        );
        let mut state = fixture.state_with_workspace_analysis();
        assert!(!state.config.supports_watched_file_dynamic_registration());
        let target = fixture.project_path("/lib/pkg/src/Target.sol");
        let event_path = event_path.map(|path| fixture.project_path(path));
        let event = |typ| event_path.clone().map(|path| (path, typ));
        assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");

        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(&target, "contract Target {}\n").unwrap();
        refresh(&mut state, event(FileChangeType::CREATED)).await;
        assert_eq!(
            fixture.query_in(&mut state, Query::Definition, "$1").await,
            "/lib/pkg/src/Target.sol:0:0 contract Target {}\n"
        );

        if event_path.as_ref() == Some(&target) {
            std::fs::remove_file(&target).unwrap();
        } else {
            std::fs::remove_dir_all(fixture.project_path("/lib/pkg")).unwrap();
        }
        refresh(&mut state, event(FileChangeType::DELETED)).await;
        assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");
    }
}

/// Sends a watched-file event, or reindexes without one, and waits for the resulting analysis.
async fn refresh(state: &mut GlobalState, event: Option<(PathBuf, FileChangeType)>) {
    if let Some(event) = event {
        watch_files(state, [event]);
    } else {
        state.reindex();
    }
    settle(state).await;
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
    state.config = Arc::new(fixture.project().config_with_roots(&["/workspace"]));
    let target = fixture.project_path("/external/lib/pkg/src/Target.sol");
    std::fs::create_dir_all(target.parent().unwrap()).unwrap();
    std::fs::write(&target, "contract Target {}\n").unwrap();
    let version = analysis_version(&state);

    watch_files(&mut state, [(&target, FileChangeType::CREATED)]);

    assert_eq!(analysis_version(&state), version);
    cancel_analysis(&state);
}

#[test]
fn unresolved_import_literals_use_the_owning_foundry_context() {
    for (fixture, path, expected) in [
        // The deepest Foundry context wins.
        (
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
            "/packages/app/lib/inner/Target.sol:0:0 contract InnerTarget {}\n",
        ),
        // External source roots inside an ancestor base use the nested context.
        (
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
            "/outer/packages/app/lib/inner/Target.sol:0:0 contract InnerTarget {}\n",
        ),
        // Out-of-base remapping targets keep their workspace context.
        (
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
            "/shared/Target.sol:0:0 contract Target {}\n",
        ),
        // Unowned files do not use the first workspace context.
        (
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
            "<none>\n",
        ),
    ] {
        RequestFixture::new_allowing_diagnostics(fixture, path)
            .check_goto_definition("$1", expected);
    }
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
    set_overlay(&state, &importer, &fixture.project_contents("/unowned/Main.sol"), None);
    {
        let mut commit = state.analysis_commit.lock();
        commit.vfs_content_revision = state.vfs.read().content_revision();
        commit.symbol_tables_version = analysis_version(&state);
    }
    assert_eq!(fixture.query_in(&mut state, Query::Definition, "$1").await, "<none>\n");
}

use super::*;
use crate::{file_operations::WatchedFileAction, handlers, workspace::WorkspaceKind};
use lsp_types::{CreateFilesParams, DeleteFilesParams, FileCreate, FileDelete, InitializedParams};
use std::fs;

const CREATED: FileChangeType = FileChangeType::CREATED;
const DELETED: FileChangeType = FileChangeType::DELETED;
const EXISTING: &str = "//- /Existing.sol\ncontract Existing {}\n";
const UNSAVED: &str = "contract Unsaved {}";

async fn analysis(state: &GlobalState) -> Arc<SymbolTables> {
    settle(state).await.load_full()
}

fn has_symbol(tables: &Arc<SymbolTables>, name: &str) -> bool {
    tables.workspace_symbols(name).iter().any(|symbol| symbol.name == name)
}

fn exists(state: &GlobalState, path: impl AsRef<Path>) -> bool {
    state.vfs.read().exists(&VfsPath::from(path.as_ref().to_path_buf()))
}

fn assert_buffer(state: &GlobalState, path: impl AsRef<Path>, text: &str, version: i32) {
    let path = VfsPath::from(path.as_ref().to_path_buf());
    let vfs = state.vfs.read();
    let buffer = vfs
        .get_file_contents(&path)
        .map(|contents| (contents.to_string(), vfs.get_file_version(&path)));
    assert_eq!(buffer, Some((text.to_owned(), Some(version))));
}

fn did_create(state: &mut GlobalState, paths: impl IntoIterator<Item = impl AsRef<Path>>) {
    let files = paths.into_iter().map(|path| FileCreate { uri: uri(path) }).collect();
    assert!(handlers::did_create_files(state, CreateFilesParams { files }).is_continue());
}

fn did_delete(state: &mut GlobalState, paths: impl IntoIterator<Item = impl AsRef<Path>>) {
    let files = paths.into_iter().map(|path| FileDelete { uri: uri(path) }).collect();
    assert!(handlers::did_delete_files(state, DeleteFilesParams { files }).is_continue());
}

fn did_rename(state: &mut GlobalState, params: &RenameFilesParams) {
    assert!(handlers::did_rename_files(state, params.clone()).is_continue());
}

fn rename_watcher_events(
    old_root: &Path,
    new_root: &Path,
    paths: &[&str],
) -> Vec<(PathBuf, FileChangeType)> {
    paths
        .iter()
        .flat_map(|path| [(old_root.join(path), DELETED), (new_root.join(path), CREATED)])
        .collect()
}

/// Creates a Foundry project whose `lib` folder is only reachable through imports.
fn import_only_project(main: &str) -> TestProject {
    TestProject::from_fixture(&format!(
        "//- /foundry.toml\n[profile.default]\nauto_detect_remappings = false\nlibs = [\"lib\"]\n\n//- /src/Main.sol\n{main}\n"
    ))
}

/// Opens `/old/Open.sol` with an unsaved buffer, optionally prepares the `/old` -> `/new`
/// rename, and moves the folder on disk.
async fn folder_rename(
    prepare: bool,
) -> (TestProject, GlobalState, RenameFilesParams, PathBuf, PathBuf) {
    let project = TestProject::from_fixture("//- /old/Open.sol open\ncontract DiskVersion {}\n");
    let (old_folder, new_folder) = (project.path("/old"), project.path("/new"));
    let params = rename_params([(&old_folder, &new_folder)]);
    let mut state = state(&project);
    set_overlay(&state, &old_folder.join("Open.sol"), UNSAVED, 12);
    if prepare {
        handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    }
    fs::rename(&old_folder, &new_folder).unwrap();
    (project, state, params, old_folder.join("Open.sol"), new_folder.join("Open.sol"))
}

#[tokio::test(flavor = "current_thread")]
async fn initialized_indexes_workspace_before_the_first_file_operation() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let mut state = state_with(Config::default());
    state.on_initialize(project.initialize_params()).await.unwrap();
    assert!(state.config.workspaces().is_empty());
    assert!(state.on_initialized(InitializedParams {}).is_continue());

    let params =
        rename_params([(project.path("/src/Target.sol"), project.path("/src/Renamed.sol"))]);
    assert!(handlers::will_rename_files(&mut state, params).await.unwrap().is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn did_create_files_rediscovers_files_and_folder_descendants_once() {
    let project = TestProject::from_fixture(EXISTING);
    let mut state = state(&project);
    project.write_file("/Direct.sol", "contract Direct {}");
    project.write_file("/created.v2/Nested.sol", "contract Nested {}");
    let before = analysis_version(&state);

    did_create(&mut state, [project.path("/Direct.sol"), project.path("/created.v2")]);

    assert_eq!(analysis_version(&state), before + 1);
    let tables = analysis(&state).await;
    assert!(has_symbol(&tables, "Direct"));
    assert!(has_symbol(&tables, "Nested"));
}

#[tokio::test(flavor = "current_thread")]
async fn folder_create_echo_scan_skips_excluded_descendants() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol
        contract Main {}
        "#,
    );
    let mut state = state(&project);
    project.write_file("/src/created/Included.sol", "contract Included {}");
    project.write_file("/src/created/node_modules/Excluded.sol", "contract Excluded {}");

    did_create(&mut state, [project.path("/src/created")]);

    let excluded = project.path("/src/created/node_modules/Excluded.sol");
    let included = project.path("/src/created/Included.sol");
    let coordinator = &mut state.file_operations;
    assert_eq!(coordinator.observe_watcher_event(&excluded, CREATED), WatchedFileAction::Process);
    assert_eq!(coordinator.observe_watcher_event(&included, CREATED), WatchedFileAction::Ignore);
    analysis(&state).await;
}

#[tokio::test(flavor = "current_thread")]
async fn did_create_nested_manifest_discovers_the_project() {
    // The second package is nested under an existing source root.
    for (fixture, package) in [
        ("//- /Main.sol\ncontract Main {}\n", "/package"),
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "lib"

            //- /lib/Existing.sol
            contract Existing {}
            "#,
            "/lib/package",
        ),
    ] {
        let project = TestProject::from_fixture(fixture);
        let mut state = state(&project);
        let manifest = format!("{package}/foundry.toml");
        project.write_file(&manifest, "[profile.default]\nsrc = \"src\"\n");
        project.write_file(&format!("{package}/src/Nested.sol"), "contract Nested {}");

        did_create(&mut state, [project.path(&manifest)]);

        assert!(has_symbol(&analysis(&state).await, "Nested"));
        assert!(state.config.workspaces().iter().any(|workspace| {
            workspace.compile_opts().base_path.as_deref() == Some(project.path(package).as_path())
        }));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn deleting_manifest_directory_with_external_sources_rediscovers_workspace() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "../shared"

        //- /shared/Main.sol
        contract Main {}
        "#,
    );
    let mut state = state(&project);
    let before = analysis_version(&state);
    fs::remove_dir_all(project.path("/project")).unwrap();

    did_delete(&mut state, [project.path("/project")]);

    assert_eq!(analysis_version(&state), before + 1);
    assert!(has_symbol(&analysis(&state).await, "Main"));
}

#[tokio::test(flavor = "current_thread")]
async fn deleting_parent_of_missing_candidate_schedules_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Missing.sol";
        contract Main is Missing {}
        "#,
    );
    let mut state = state(&project);
    let missing = project.path("/generated/Missing.sol");
    state.analysis_commit.lock().analysis_paths.missing_candidates.insert(missing);
    let before = analysis_version(&state);

    did_delete(&mut state, [project.path("/generated")]);

    assert_eq!(analysis_version(&state), before + 1);
    analysis(&state).await;
}

#[tokio::test(flavor = "current_thread")]
async fn excluded_folder_rename_watcher_echo_does_not_schedule_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /node_modules/old/Ignored.sol
        contract Ignored {}
        "#,
    );
    let (old_root, new_root) =
        (project.path("/node_modules/old"), project.path("/node_modules/new"));
    let mut state = state(&project);
    let before = analysis_version(&state);

    let params = rename_params([(&old_root, &new_root)]);
    assert!(handlers::will_rename_files(&mut state, params).await.unwrap().is_none());
    fs::rename(&old_root, &new_root).unwrap();
    watch_files(&mut state, rename_watcher_events(&old_root, &new_root, &["Ignored.sol"]));

    assert_eq!(analysis_version(&state), before);
}

#[tokio::test(flavor = "current_thread")]
async fn delayed_create_after_empty_folder_did_create_is_processed() {
    let project = TestProject::from_fixture(EXISTING);
    let mut state = state(&project);
    fs::create_dir(project.path("/created")).unwrap();
    did_create(&mut state, [project.path("/created")]);
    analysis(&state).await;
    let before = analysis_version(&state);

    project.write_file("/created/Later.sol", "contract Later {}");
    watch_files(&mut state, [(project.path("/created/Later.sol"), CREATED)]);

    assert_eq!(analysis_version(&state), before + 1);
    assert!(has_symbol(&analysis(&state).await, "Later"));
}

#[tokio::test(flavor = "current_thread")]
async fn did_create_and_its_watcher_echo_start_one_epoch() {
    for (fixture, created, contents) in [
        (EXISTING, "/Created.sol", "contract Created {}"),
        (
            "//- /foundry.toml\n\n//- /src/Main.sol\ncontract Main {}\n",
            "/remappings.txt",
            "pkg/=lib/pkg/\n",
        ),
    ] {
        for watcher_first in [false, true] {
            let project = TestProject::from_fixture(fixture);
            let created = project.path(created);
            let mut state = state(&project);
            fs::write(&created, contents).unwrap();
            let before = analysis_version(&state);

            if watcher_first {
                watch_files(&mut state, [(&created, CREATED)]);
                did_create(&mut state, [&created]);
            } else {
                did_create(&mut state, [&created]);
                watch_files(&mut state, [(&created, CREATED)]);
            }

            assert_eq!(analysis_version(&state), before + 1);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn did_delete_and_its_watcher_echo_start_one_epoch() {
    for watcher_first in [false, true] {
        let project = TestProject::from_fixture("//- /Deleted.sol open\ncontract Deleted {}\n");
        let deleted = project.path("/Deleted.sol");
        let mut state = state(&project);
        fs::remove_file(&deleted).unwrap();
        let before = analysis_version(&state);

        if watcher_first {
            watch_files(&mut state, [(&deleted, DELETED)]);
            assert_eq!(analysis_version(&state), before);
            assert!(exists(&state, &deleted));
            analysis(&state).await;
            did_delete(&mut state, [&deleted]);
        } else {
            did_delete(&mut state, [&deleted]);
            watch_files(&mut state, [(&deleted, DELETED)]);
        }

        assert_eq!(analysis_version(&state), before + 1);
        assert!(!exists(&state, &deleted));
        assert!(analysis(&state).await.workspace_symbols("Deleted").is_empty());
        let vfs_revision = state.vfs.read().content_revision();
        assert!(state.analysis_revision().is_current(vfs_revision));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watcher_delete_preserves_open_file_until_the_editor_changes_or_closes_it() {
    for closed in [false, true] {
        let project = TestProject::from_fixture("//- /Deleted.sol open\ncontract Deleted {}\n");
        let deleted = project.path("/Deleted.sol");
        let uri = project.uri("/Deleted.sol");
        let mut state = state(&project);
        fs::remove_file(&deleted).unwrap();
        let before = analysis_version(&state);

        watch_files(&mut state, [(&deleted, DELETED)]);
        assert_eq!(analysis_version(&state), before);
        assert!(exists(&state, &deleted));

        if closed {
            close(&mut state, &uri);
            assert!(!exists(&state, &deleted));
            assert!(analysis(&state).await.workspace_symbols("Deleted").is_empty());
        } else {
            change(&mut state, &uri, 1, "contract AfterDelete {}");
            assert_buffer(&state, &deleted, "contract AfterDelete {}", 1);
            assert!(has_symbol(&analysis(&state).await, "AfterDelete"));
        }
        assert_eq!(analysis_version(&state), before + 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn mixed_watcher_batch_preserves_an_unrelated_deleted_open_file() {
    let project = TestProject::from_fixture("//- /Deleted.sol open\ncontract Deleted {}\n");
    let deleted = project.path("/Deleted.sol");
    let mut state = state(&project);
    fs::remove_file(&deleted).unwrap();
    project.write_file("/Created.sol", "contract Created {}");

    watch_files(&mut state, [(deleted.clone(), DELETED), (project.path("/Created.sol"), CREATED)]);

    assert!(exists(&state, &deleted));
    let tables = analysis(&state).await;
    assert!(has_symbol(&tables, "Deleted"));
    assert!(has_symbol(&tables, "Created"));
}

#[tokio::test(flavor = "current_thread")]
async fn folder_create_and_its_watcher_echoes_start_one_epoch() {
    // Watcher echoes arrive split after the notification, or batched before it.
    for (watcher_first, files) in [
        (
            false,
            &[
                ("foundry.toml", "[profile.default]\nsrc = \".\"\n"),
                ("remappings.txt", "pkg/=lib/pkg/\n"),
                ("First.sol", "contract First {}"),
                ("Second.sol", "contract Second {}"),
            ][..],
        ),
        (
            true,
            &[
                ("foundry.toml", "[profile.default]\nsrc = \"src\"\n"),
                ("src/First.sol", "contract First {}"),
                ("src/Second.sol", "contract Second {}"),
            ],
        ),
    ] {
        let project = TestProject::from_fixture(EXISTING);
        let folder = project.path("/created");
        let mut state = state(&project);
        for (path, contents) in files {
            project.write_file(&format!("/created/{path}"), contents);
        }
        let echoes = files.iter().map(|(path, _)| (folder.join(path), CREATED));
        let before = analysis_version(&state);

        if watcher_first {
            watch_files(&mut state, echoes);
            did_create(&mut state, [&folder]);
        } else {
            did_create(&mut state, [&folder]);
            for echo in echoes {
                watch_files(&mut state, [echo]);
            }
        }

        assert_eq!(analysis_version(&state), before + 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn folder_create_records_foundry_flycheck_descendants_for_watcher_echoes() {
    let project = TestProject::from_fixture(EXISTING);
    let mut state = state(&project);
    project.write_file("/created/foundry.toml", "[profile.default]\nsrc = \"src\"\n");
    project.write_file("/created/test/Only.t.sol", "contract OnlyTest {}");
    let before = analysis_version(&state);

    did_create(&mut state, [project.path("/created")]);
    watch_files(
        &mut state,
        [
            (project.path("/created/foundry.toml"), CREATED),
            (project.path("/created/test/Only.t.sol"), CREATED),
        ],
    );

    assert_eq!(analysis_version(&state), before + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn folder_create_and_import_only_watcher_echo_start_one_epoch() {
    for watcher_first in [false, true] {
        let project = import_only_project("import \"pkg/src/Target.sol\";");
        let folder = project.path("/lib/pkg");
        let target = project.path("/lib/pkg/src/Target.sol");
        let mut state = state(&project);
        project.write_file("/lib/pkg/src/Target.sol", "contract Target {}");
        let before = analysis_version(&state);

        if watcher_first {
            watch_files(&mut state, [(&target, CREATED)]);
            did_create(&mut state, [&folder]);
        } else {
            did_create(&mut state, [&folder]);
            watch_files(&mut state, [(&target, CREATED)]);
        }

        assert_eq!(analysis_version(&state), before + 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unrelated_import_only_folder_create_does_not_schedule_analysis() {
    let project = import_only_project("contract Main {}");
    let mut state = state(&project);
    project.write_file("/lib/pkg/src/Unrelated.sol", "contract Unrelated {}");
    let before = analysis_version(&state);

    did_create(&mut state, [project.path("/lib/pkg")]);

    assert_eq!(analysis_version(&state), before);
}

#[tokio::test(flavor = "current_thread")]
async fn folder_delete_split_watcher_echoes_do_not_start_another_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /deleted/foundry.toml
        [profile.default]
        src = "."

        //- /deleted/remappings.txt
        pkg/=lib/pkg/

        //- /deleted/First.sol
        contract First {}

        //- /deleted/Second.sol open
        contract Second {}
        "#,
    );
    let folder = project.path("/deleted");
    let mut state = state(&project);
    fs::remove_dir_all(&folder).unwrap();
    let before = analysis_version(&state);

    did_delete(&mut state, [&folder]);
    for path in ["foundry.toml", "remappings.txt", "First.sol", "Second.sol"] {
        watch_files(&mut state, [(folder.join(path), DELETED)]);
    }

    assert_eq!(analysis_version(&state), before + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn folder_watcher_delete_followed_by_did_delete_starts_one_epoch() {
    let project = TestProject::from_fixture(
        "//- /deleted/First.sol\ncontract First {}\n//- /deleted/Second.sol\ncontract Second {}\n",
    );
    let folder = project.path("/deleted");
    let mut state = state(&project);
    fs::remove_dir_all(&folder).unwrap();
    let before = analysis_version(&state);

    watch_files(
        &mut state,
        [(folder.join("First.sol"), DELETED), (folder.join("Second.sol"), DELETED)],
    );
    analysis(&state).await;
    did_delete(&mut state, [&folder]);

    assert_eq!(analysis_version(&state), before + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_or_watcher_migrates_open_buffers_before_one_reanalysis() {
    // Without a prepared rename, the did notification commits it and the watcher echoes are
    // ignored; with one, the watcher commits it and the did notification is a replay.
    for watcher_first in [false, true] {
        let (_project, mut state, params, old_file, new_file) = folder_rename(watcher_first).await;
        let events = [(&old_file, DELETED), (&new_file, CREATED)];
        let before = analysis_version(&state);

        if watcher_first {
            watch_files(&mut state, events);
        } else {
            did_rename(&mut state, &params);
        }
        assert_eq!(analysis_version(&state), before + 1);
        assert!(!exists(&state, &old_file));
        assert_buffer(&state, &new_file, UNSAVED, 12);
        if watcher_first {
            did_rename(&mut state, &params);
        } else {
            watch_files(&mut state, events);
        }

        assert_eq!(analysis_version(&state), before + 1);
        assert_buffer(&state, &new_file, UNSAVED, 12);
        let tables = analysis(&state).await;
        assert!(tables.workspace_symbols("DiskVersion").is_empty());
        assert!(has_symbol(&tables, "Unsaved"));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn split_watcher_echo_after_did_rename_is_ignored() {
    let (_project, mut state, params, old_file, new_file) = folder_rename(true).await;
    let before = analysis_version(&state);

    did_rename(&mut state, &params);
    for event in [(&old_file, DELETED), (&new_file, CREATED)] {
        for _ in 0..2 {
            watch_files(&mut state, [event]);
        }
    }

    assert_eq!(analysis_version(&state), before + 1);
    assert_buffer(&state, &new_file, UNSAVED, 12);
}

#[tokio::test(flavor = "current_thread")]
async fn unrelated_create_does_not_end_split_watcher_echo() {
    let (project, mut state, params, old_file, new_file) = folder_rename(true).await;
    let before = analysis_version(&state);

    did_rename(&mut state, &params);
    watch_files(&mut state, [(&old_file, DELETED)]);
    project.write_file("/other/New.sol", "contract New {}");
    did_create(&mut state, [project.path("/other/New.sol")]);
    assert_eq!(analysis_version(&state), before + 2);

    watch_files(&mut state, [(&new_file, CREATED)]);
    assert_eq!(analysis_version(&state), before + 2);
    did_rename(&mut state, &params);
    assert_eq!(analysis_version(&state), before + 2);
}

#[tokio::test(flavor = "current_thread")]
async fn split_watcher_events_commit_prepared_rename_once() {
    for reversed in [false, true] {
        let (_project, mut state, params, old_file, new_file) = folder_rename(true).await;
        let before = analysis_version(&state);
        let mut events = [(&old_file, DELETED), (&new_file, CREATED)];
        if reversed {
            events.reverse();
        }

        watch_files(&mut state, [events[0]]);
        assert_eq!(analysis_version(&state), before);
        assert!(exists(&state, &old_file));
        watch_files(&mut state, [events[1]]);
        assert_eq!(analysis_version(&state), before + 1);
        assert!(!exists(&state, &old_file));
        assert_buffer(&state, &new_file, UNSAVED, 12);

        did_rename(&mut state, &params);
        for _ in 0..2 {
            watch_files(&mut state, [(&new_file, CREATED)]);
        }
        assert_eq!(analysis_version(&state), before + 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn partial_watcher_batch_does_not_commit_prepared_rename() {
    let project = TestProject::from_fixture(
        r#"
        //- /A/Open.sol open
        contract A {}

        //- /X/Open.sol open
        contract X {}
        "#,
    );
    let [a, b, x, y] = ["/A", "/B", "/X", "/Y"].map(|path| project.path(path));
    let params = rename_params([(&a, &b), (&x, &y)]);
    let mut state = state(&project);
    handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&a, &b).unwrap();
    fs::rename(&x, &y).unwrap();
    let before = analysis_version(&state);

    watch_files(&mut state, [(a.join("Open.sol"), DELETED), (b.join("Open.sol"), CREATED)]);
    assert_eq!(analysis_version(&state), before);
    assert!(exists(&state, a.join("Open.sol")));
    assert!(exists(&state, x.join("Open.sol")));

    watch_files(&mut state, [(x.join("Open.sol"), DELETED), (y.join("Open.sol"), CREATED)]);
    assert_eq!(analysis_version(&state), before + 1);
    for (root, present) in [(&a, false), (&b, true), (&x, false), (&y, true)] {
        assert_eq!(exists(&state, root.join("Open.sol")), present);
    }

    did_rename(&mut state, &params);
    assert_eq!(analysis_version(&state), before + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_or_cancelled_will_rename_does_not_leave_a_watcher_transaction() {
    for cancel in [false, true] {
        let project = TestProject::from_fixture(
            r#"
            //- /src/Importer.sol open
            import "../old/Target.sol";

            //- /old/Target.sol open
            contract Target {}
            "#,
        );
        let params = rename_params([(project.path("/old"), project.path("/new"))]);
        let mut state = state(&project);
        if cancel {
            drop(handlers::will_rename_files(&mut state, params));
        } else {
            let importer = project.path("/src/Importer.sol");
            set_overlay(&state, &importer, "import \"../old/Other.sol\";", None);
            let error = handlers::will_rename_files(&mut state, params).await.unwrap_err();
            assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
        }
        let (old, new) = (project.path("/old/Target.sol"), project.path("/new/Target.sol"));
        let before = analysis_version(&state);

        watch_files(&mut state, [(&old, DELETED), (&new, CREATED)]);

        assert_eq!(analysis_version(&state), before + 1);
        assert!(exists(&state, &old));
        assert!(!exists(&state, &new));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cross_suffix_watcher_pair_does_not_commit_prepared_rename() {
    let project = TestProject::from_fixture(
        r#"
        //- /old/A.sol open
        contract A {}

        //- /old/B.sol open
        contract B {}
        "#,
    );
    let params = rename_params([(project.path("/old"), project.path("/new"))]);
    let mut state = state(&project);
    handlers::will_rename_files(&mut state, params).await.unwrap();
    let before = analysis_version(&state);

    watch_files(
        &mut state,
        [(project.path("/old/A.sol"), DELETED), (project.path("/new/B.sol"), CREATED)],
    );

    assert_eq!(analysis_version(&state), before);
    for (path, present) in
        [("/old/A.sol", true), ("/old/B.sol", true), ("/new/A.sol", false), ("/new/B.sol", false)]
    {
        assert_eq!(exists(&state, project.path(path)), present);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_replay_does_not_reapply_overlapping_moves() {
    // Covers a chain (`A -> B`, `B -> C`) and a swap (`A <-> B`).
    for swap in [false, true] {
        let project = TestProject::from_fixture(
            "//- /A.sol open\ncontract A {}\n//- /B.sol open\ncontract B {}\n",
        );
        let [a, b, c] = ["/A.sol", "/B.sol", "/C.sol"].map(|path| project.path(path));
        let params = if swap {
            rename_params([(&a, &b), (&b, &a)])
        } else {
            rename_params([(&a, &b), (&b, &c)])
        };
        let mut state = state(&project);
        set_overlay(&state, &a, "contract UnsavedA {}", 11);
        set_overlay(&state, &b, "contract UnsavedB {}", 22);
        handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
        if swap {
            fs::rename(&a, &c).unwrap();
            fs::rename(&b, &a).unwrap();
            fs::rename(&c, &b).unwrap();
        } else {
            fs::rename(&b, &c).unwrap();
            fs::rename(&a, &b).unwrap();
        }
        let before = analysis_version(&state);

        did_rename(&mut state, &params);
        did_rename(&mut state, &params);

        assert_eq!(analysis_version(&state), before + 1);
        assert_buffer(&state, &b, "contract UnsavedA {}", 11);
        assert_buffer(&state, if swap { &a } else { &c }, "contract UnsavedB {}", 22);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn file_rename_accepts_percent_encoded_file_uris() {
    let project =
        TestProject::from_fixture("//- /src/Importer.sol open\nimport \"./Target file.sol\";\n");
    project.write_file("/src/Target file.sol", "contract Target {}");
    let old_target = project.path("/src/Target file.sol");
    let new_target = project.path("/src/Renamed # file.sol");
    let params = rename_params([(&old_target, &new_target)]);
    assert!(params.files[0].old_uri.contains("%20"));
    assert!(params.files[0].new_uri.contains("%23"));
    let mut state = state(&project);
    set_overlay(&state, &old_target, "contract UnsavedTarget {}", 9);

    let edit = handlers::will_rename_files(&mut state, params.clone()).await.unwrap().unwrap();
    let importer = project.uri("/src/Importer.sol");
    assert_eq!(edit.changes.unwrap()[&importer][0].new_text, "\"./Renamed # file.sol\"");
    fs::rename(&old_target, &new_target).unwrap();
    did_rename(&mut state, &params);

    assert!(!exists(&state, &old_target));
    assert_buffer(&state, &new_target, "contract UnsavedTarget {}", 9);
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_migrates_case_only_file_move() {
    let project = TestProject::from_fixture("//- /Case.sol open\ncontract Case {}\n");
    let (old_path, new_path) = (project.path("/Case.sol"), project.path("/case.sol"));
    let params = rename_params([(&old_path, &new_path)]);
    let mut state = state(&project);
    set_overlay(&state, &old_path, "contract UnsavedCase {}", 4);
    handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_path, &new_path).unwrap();

    did_rename(&mut state, &params);

    assert!(!exists(&state, &old_path));
    assert_buffer(&state, &new_path, "contract UnsavedCase {}", 4);
}

#[tokio::test(flavor = "current_thread")]
async fn conflicting_rename_batches_are_rejected() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/First.sol open
        contract First {}

        //- /src/Second.sol open
        contract Second {}

        //- /A/x.sol open
        contract A {}

        //- /B/x.sol open
        contract B {}
        "#,
    );
    let mut state = state(&project);
    let before = analysis_version(&state);

    // Conflicting sources, conflicting destinations, and a destination collision that only
    // appears once the folder move expands over the VFS.
    for moves in [
        [("/src/First.sol", "/src/One.sol"), ("/src/First.sol", "/src/Two.sol")],
        [("/src/First.sol", "/src/Renamed.sol"), ("/src/Second.sol", "/src/Renamed.sol")],
        [("/A", "/out"), ("/B/x.sol", "/out/x.sol")],
    ] {
        let params = rename_params(moves.map(|(old, new)| (project.path(old), project.path(new))));
        let error = handlers::will_rename_files(&mut state, params.clone()).await.unwrap_err();
        assert_eq!(error.code, ErrorCode::INVALID_PARAMS);

        did_rename(&mut state, &params);

        for path in ["/src/First.sol", "/src/Second.sol", "/A/x.sol", "/B/x.sol"] {
            assert!(exists(&state, project.path(path)));
        }
        for path in ["/src/One.sol", "/src/Two.sol", "/src/Renamed.sol", "/out/x.sol"] {
            assert!(!exists(&state, project.path(path)));
        }
    }
    assert_eq!(analysis_version(&state), before);
}

#[tokio::test(flavor = "current_thread")]
async fn watcher_collision_does_not_suppress_later_did_rename() {
    let project = TestProject::from_fixture(
        r#"
        //- /A/x.sol open
        contract A {}

        //- /B/x.sol
        contract B {}
        "#,
    );
    let [a, b, destination] = ["/A/x.sol", "/B/x.sol", "/out/x.sol"].map(|path| project.path(path));
    let params = rename_params([
        (project.path("/A"), project.path("/out")),
        (b.clone(), destination.clone()),
    ]);
    let mut state = state(&project);
    handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    set_overlay(&state, &b, "contract UnsavedB {}", 22);
    let before = analysis_version(&state);

    watch_files(&mut state, [(&a, DELETED), (&b, DELETED), (&destination, CREATED)]);
    assert_eq!(analysis_version(&state), before);
    assert!(exists(&state, &a));
    assert!(exists(&state, &b));
    assert!(!exists(&state, &destination));

    remove_overlay(&state, &b);
    did_rename(&mut state, &params);

    assert_eq!(analysis_version(&state), before + 1);
    assert!(!exists(&state, &a));
    assert!(exists(&state, &destination));
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_workspace_root_preserves_foundry_configuration_and_closed_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /project/src/Main.sol
        import "@lib/Dependency.sol";
        contract Main {}

        //- /project/lib/Dependency.sol
        contract Dependency {}
        "#,
    );
    let (old_root, new_root) = (project.path("/project"), project.path("/renamed"));
    let params = rename_params([(&old_root, &new_root)]);
    let mut state = state_with(project.config_with_roots(&["/project"]));
    fs::rename(&old_root, &new_root).unwrap();
    let before = analysis_version(&state);

    did_rename(&mut state, &params);

    let tables = analysis(&state).await;
    let workspaces = state.config.workspaces();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].kind(), WorkspaceKind::Foundry);
    assert_eq!(workspaces[0].compile_opts().base_path.as_deref(), Some(new_root.as_path()));
    assert!(has_symbol(&tables, "Main"));
    assert_eq!(
        tables.document_links(&new_root.join("src/Main.sol"))[0].target,
        Some(Url::from_file_path(new_root.join("lib/Dependency.sol")).unwrap())
    );

    let files = ["foundry.toml", "src/Main.sol", "lib/Dependency.sol"];
    watch_files(&mut state, rename_watcher_events(&old_root, &new_root, &files));
    assert_eq!(analysis_version(&state), before + 1);
    did_rename(&mut state, &params);
    assert_eq!(analysis_version(&state), before + 1);
    assert_eq!(
        state.config.workspaces()[0].compile_opts().base_path.as_deref(),
        Some(new_root.as_path())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_root_rename_advances_epoch_before_watcher_reregistration() {
    let project = TestProject::new();
    let (old_root, new_root) = (project.path("/project"), project.path("/renamed"));
    fs::create_dir(&old_root).unwrap();
    let params = with_relative_watchers(project.initialize_params_with_roots(&["/project"]));
    let mut state = state_with(Config::default());
    state.on_initialize(params).await.unwrap();
    fs::rename(&old_root, &new_root).unwrap();
    let rename = rename_params([(old_root, new_root)]);
    assert_epoch_advances_before_reregistration(state, move |state| did_rename(state, &rename));
}

#[tokio::test(flavor = "current_thread")]
async fn watcher_can_commit_workspace_root_rename_once() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /project/src/Main.sol open
        import "@lib/Dependency.sol";
        contract Main {}

        //- /project/lib/Dependency.sol
        contract Dependency {}
        "#,
    );
    let (old_root, new_root) = (project.path("/project"), project.path("/renamed"));
    let (old_main, new_main) = (old_root.join("src/Main.sol"), new_root.join("src/Main.sol"));
    let unsaved = "import \"@lib/Dependency.sol\";\ncontract Unsaved {}";
    let params = rename_params([(&old_root, &new_root)]);
    let mut state = state(&project);
    set_overlay(&state, &old_main, unsaved, 12);
    handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_root, &new_root).unwrap();
    let before = analysis_version(&state);

    let files = ["foundry.toml", "src/Main.sol", "lib/Dependency.sol"];
    watch_files(&mut state, rename_watcher_events(&old_root, &new_root, &files));
    assert_eq!(analysis_version(&state), before + 1);
    assert_eq!(
        state.config.workspaces()[0].compile_opts().base_path.as_deref(),
        Some(new_root.as_path())
    );
    assert!(!exists(&state, &old_main));
    assert_buffer(&state, &new_main, unsaved, 12);

    did_rename(&mut state, &params);
    assert_eq!(analysis_version(&state), before + 1);
    let tables = analysis(&state).await;
    assert!(has_symbol(&tables, "Unsaved"));
    assert_eq!(
        tables.document_links(&new_main)[0].target,
        Some(Url::from_file_path(new_root.join("lib/Dependency.sol")).unwrap())
    );
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_replay_does_not_remap_workspace_root_again() {
    let project = TestProject::from_fixture(
        r#"
        //- /A/foundry.toml
        [profile.default]
        src = "src"

        //- /A/src/Main.sol
        contract Main {}
        "#,
    );
    let [a, b, c] = ["/A", "/B", "/C"].map(|path| project.path(path));
    let params = rename_params([(&a, &b), (&b, &c)]);
    let mut state = state_with(project.config_with_roots(&["/A"]));
    fs::rename(&a, &b).unwrap();
    let before = analysis_version(&state);

    did_rename(&mut state, &params);
    did_rename(&mut state, &params);

    let tables = analysis(&state).await;
    assert_eq!(analysis_version(&state), before + 1);
    assert_eq!(state.config.workspaces()[0].compile_opts().base_path.as_deref(), Some(b.as_path()));
    assert!(has_symbol(&tables, "Main"));
}

#[tokio::test(flavor = "current_thread")]
async fn did_only_rename_round_trip_reapplies_original_payload() {
    let project = TestProject::from_fixture("//- /A/Main.sol open\ncontract DiskVersion {}\n");
    let (a, b) = (project.path("/A"), project.path("/B"));
    let forward = rename_params([(&a, &b)]);
    let reverse = rename_params([(&b, &a)]);
    let mut state = state(&project);
    state.config = Arc::new(project.config_with_roots(&["/A"]));
    set_overlay(&state, &a.join("Main.sol"), UNSAVED, 12);
    let before = analysis_version(&state);

    fs::rename(&a, &b).unwrap();
    did_rename(&mut state, &forward);
    fs::rename(&b, &a).unwrap();
    did_rename(&mut state, &reverse);
    fs::rename(&a, &b).unwrap();
    did_rename(&mut state, &forward);
    did_rename(&mut state, &forward);

    assert_eq!(analysis_version(&state), before + 3);
    assert!(!exists(&state, a.join("Main.sol")));
    assert_buffer(&state, b.join("Main.sol"), UNSAVED, 12);
    assert_eq!(state.config.workspaces()[0].compile_opts().base_path.as_deref(), Some(b.as_path()));
}

#[tokio::test(flavor = "current_thread")]
async fn did_delete_folder_removes_open_descendants_but_not_prefix_siblings() {
    let project = TestProject::from_fixture(
        r#"
        //- /pkg/Deleted.sol open
        contract Deleted {}

        //- /pkg2/Keep.sol open
        contract Keep {}
        "#,
    );
    let mut state = state(&project);
    fs::remove_dir_all(project.path("/pkg")).unwrap();

    did_delete(&mut state, [project.path("/pkg")]);

    assert!(!exists(&state, project.path("/pkg/Deleted.sol")));
    assert!(exists(&state, project.path("/pkg2/Keep.sol")));
    let tables = analysis(&state).await;
    assert!(tables.workspace_symbols("Deleted").is_empty());
    assert!(has_symbol(&tables, "Keep"));
}

#[tokio::test(flavor = "current_thread")]
async fn did_delete_folder_clears_closed_dependency_diagnostics_by_prefix() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol open
        import "../lib/pkg/Dependency.sol";

        //- /lib/pkg/Dependency.sol
        contract Dependency {}

        //- /lib2/Keep.sol
        contract Keep {}
        "#,
    );
    let deleted_uri = project.uri("/lib/pkg/Dependency.sol");
    let sibling_uri = project.uri("/lib2/Keep.sol");
    let owner =
        DiagnosticOwner::Flycheck { id: "probe".into(), workspace: project.root().to_path_buf() };
    let mut state = state(&project);
    state.snapshot().publish_diagnostics(
        owner,
        DiagnosticMap::from_iter([
            (deleted_uri.clone(), vec![diagnostic("deleted")]),
            (sibling_uri.clone(), vec![diagnostic("sibling")]),
        ]),
    );
    fs::remove_dir_all(project.path("/lib")).unwrap();

    did_delete(&mut state, [project.path("/lib")]);

    analysis(&state).await;
    assert!(pulled_diagnostics(&state, &deleted_uri).is_empty());
    assert_eq!(pulled_diagnostics(&state, &sibling_uri), [diagnostic("sibling")]);
}

#[tokio::test(flavor = "current_thread")]
async fn did_delete_workspace_root_removes_configuration_and_closed_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"

        //- /project/src/Deleted.sol
        contract Deleted {}
        "#,
    );
    let mut state = state_with(project.config_with_roots(&["/project"]));
    fs::remove_dir_all(project.path("/project")).unwrap();

    did_delete(&mut state, [project.path("/project")]);

    let tables = analysis(&state).await;
    assert!(state.config.workspaces().is_empty());
    assert!(tables.workspace_symbols("Deleted").is_empty());
}

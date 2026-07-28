use super::{
    ASYNC_TEST_TIMEOUT, AnalysisResultAccumulator, GlobalState, SymbolTables, analyze, diagnostic,
    snapshot,
};
use crate::{
    diagnostics::{DiagnosticMap, DiagnosticOwner, PullReport},
    file_operations::FileMoveBatch,
    test_support::TestProject,
    vfs::VfsPath,
    workspace::WorkspaceKind,
};
use async_lsp::{ClientSocket, ErrorCode};
use crop::Rope;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileChangeType, FileCreate, FileDelete, FileEvent,
    FileRename, InitializedParams, Position, Range, RenameFilesParams, TextEdit, Url,
};
use std::{
    fs,
    future::Future,
    ops::ControlFlow,
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
};

fn analyze_project(project: &TestProject) -> SymbolTables {
    let mut results = AnalysisResultAccumulator::default();
    for batch in snapshot(project).analysis_batches(Vec::new()) {
        if !batch.files.is_empty() {
            results.push(analyze(batch));
        }
    }
    results.finish().symbol_tables
}

fn state(project: &TestProject) -> GlobalState {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();
    *state.symbol_tables.write() = analyze_project(project);
    state
}

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}

fn move_batch(moves: impl IntoIterator<Item = (PathBuf, PathBuf)>) -> FileMoveBatch {
    FileMoveBatch::new(moves).unwrap()
}

#[test]
fn rename_file_rewrites_relative_import_target() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let moves = move_batch([(project.path("/src/Target.sol"), project.path("/src/Renamed.sol"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 21)),
                "\"./Renamed.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn unrelated_rename_preserves_noncanonical_relative_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "./nested/../Target.sol";

        //- /src/Target.sol
        contract Target {}

        //- /src/Other.sol
        contract Other {}
        "#,
    );
    let tables = analyze_project(&project);
    let moves =
        move_batch([(project.path("/src/Other.sol"), project.path("/src/RenamedOther.sol"))]);
    let edits = tables.import_rename_edits(&moves);

    assert!(edits.changes.is_empty());
}

#[test]
fn rename_file_preserves_foundry_remapping() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /src/Importer.sol
        import "@lib/Target.sol";

        //- /lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let moves = move_batch([(project.path("/lib/Target.sol"), project.path("/lib/Renamed.sol"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"@lib/Renamed.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_workspace_folder_keeps_foundry_remapping_unchanged() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert!(edits.changes.is_empty());
}

#[test]
fn rename_workspace_with_absolute_remapping_rewrites_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let remapping = project.path("/project/lib").to_string_lossy().replace('\\', "/");
    project.write_file(
        "/project/foundry.toml",
        &format!("[profile.default]\nsrc = \"src\"\nremappings = [\"@lib/={remapping}/\"]\n"),
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_workspace_with_absolute_include_path_rewrites_remapping() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/vendor/pkg/Target.sol
        contract Target {}
        "#,
    );
    let include_path = project.path("/project/vendor").to_string_lossy().replace('\\', "/");
    project.write_file(
        "/project/foundry.toml",
        &format!(
            "[profile.default]\nsrc = \"src\"\nlibs = [\"{include_path}\"]\nremappings = [\"@lib/=pkg/\"]\n"
        ),
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([(project.path("/project"), project.path("/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../vendor/pkg/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_configuration_root_rewrites_import_when_nested_moves_leave_files_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/src"), project.path("/project/src")),
        (project.path("/project/lib"), project.path("/project/lib")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_configuration_root_rewrites_include_path_import_when_files_stay_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        libs = ["vendor"]
        remappings = ["@lib/=pkg/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/vendor/pkg/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/src"), project.path("/project/src")),
        (project.path("/project/vendor"), project.path("/project/vendor")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../vendor/pkg/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_configuration_root_rewrites_opaque_import_when_files_stay_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["Alias=lib/Target.sol"]

        //- /project/src/Importer.sol
        import "Alias";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/src"), project.path("/project/src")),
        (project.path("/project/lib"), project.path("/project/lib")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 14)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_target_falls_back_when_a_more_specific_remapping_would_capture_it() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/", "@lib/special/=shadow/"]

        //- /project/src/Importer.sol
        import "@lib/Target.sol";

        //- /project/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([
        (project.path("/project"), project.path("/renamed")),
        (project.path("/project/lib/Target.sol"), project.path("/renamed/lib/special/Target.sol")),
    ]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/special/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_target_falls_back_when_common_suffix_loses_the_remapping_prefix() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "src"
        remappings = ["foo/=vendor/foo/"]

        //- /project/src/Importer.sol
        import "foo/Target.sol";

        //- /project/vendor/foo/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/project/src/Importer.sol");
    let moves = move_batch([(
        project.path("/project/vendor/foo/Target.sol"),
        project.path("/project/vendor/other/Target.sol"),
    )]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 23)),
                "\"../vendor/other/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_remapped_subtree_rewrites_import_when_manifest_stays_put() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=src/pkg/lib/"]

        //- /src/pkg/contracts/Importer.sol
        import "@lib/Target.sol";

        //- /src/pkg/lib/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/pkg/contracts/Importer.sol");
    let moves = move_batch([(project.path("/src/pkg"), project.path("/src/renamed"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 24)),
                "\"../lib/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn rename_importer_folder_recalculates_relative_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "../shared/Target.sol";

        //- /shared/Target.sol
        contract Target {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let moves = move_batch([(project.path("/src"), project.path("/contracts/nested"))]);
    let edits = tables.import_rename_edits(&moves);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 7), Position::new(0, 29)),
                "\"../../shared/Target.sol\"".into(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn delete_file_removes_complete_import_directive() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import {Target} from "./Target.sol";
        import "./Keep.sol";

        //- /src/Target.sol
        contract Target {}

        //- /src/Keep.sol
        contract Keep {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let edits = tables.import_delete_edits(&[project.path("/src/Target.sol")]);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 0), Position::new(0, 36)),
                String::new(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn delete_folder_removes_descendant_imports_but_not_deleted_importers() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "../package/Target.sol";

        //- /package/Target.sol
        import "./Nested.sol";
        contract Target {}

        //- /package/Nested.sol
        contract Nested {}
        "#,
    );
    let tables = analyze_project(&project);
    let importer = project.path("/src/Importer.sol");
    let edits = tables.import_delete_edits(&[project.path("/package")]);

    assert_eq!(
        edits.changes,
        [(
            Url::from_file_path(importer).unwrap(),
            vec![TextEdit::new(
                Range::new(Position::new(0, 0), Position::new(0, 31)),
                String::new(),
            )],
        )]
        .into_iter()
        .collect()
    );
}

#[test]
fn will_create_returns_no_speculative_edits() {
    let mut state = GlobalState::new(ClientSocket::new_closed());

    let edit =
        block_on(crate::handlers::will_create_files(&mut state, CreateFilesParams::default()))
            .unwrap();

    assert!(edit.is_none());
}

#[test]
fn will_delete_returns_import_edits_without_mutating_state() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol open
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let target_uri = Url::from_file_path(project.path("/src/Target.sol")).unwrap();
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_delete_files(
        &mut state,
        DeleteFilesParams { files: vec![FileDelete { uri: target_uri.to_string() }] },
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        edit.changes,
        Some(
            [(
                Url::from_file_path(&importer).unwrap(),
                vec![TextEdit::new(
                    Range::new(Position::new(0, 0), Position::new(0, 22)),
                    String::new(),
                )],
            )]
            .into_iter()
            .collect()
        )
    );
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(importer)).unwrap().to_string(),
        "import \"./Target.sol\";"
    );
}

#[test]
fn will_rename_does_not_edit_open_foundry_dependencies() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /src/Main.sol
        import "@lib/Dependency.sol";

        //- /lib/Dependency.sol open
        import "./Target.sol";

        //- /lib/Target.sol
        contract Target {}
        "#,
    );
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(project.path("/lib/Target.sol")).unwrap().to_string(),
                new_uri: Url::from_file_path(project.path("/lib/Renamed.sol")).unwrap().to_string(),
            }],
        },
    ))
    .unwrap();

    assert!(edit.is_none());
}

#[test]
fn will_rename_returns_import_edits_without_mutating_state() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol open
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let importer_uri = Url::from_file_path(&importer).unwrap();
    let old_target = project.path("/src/Target.sol");
    let old_target_uri = Url::from_file_path(&old_target).unwrap();
    let new_target = project.path("/src/Renamed.sol");
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: old_target_uri.to_string(),
                new_uri: Url::from_file_path(&new_target).unwrap().to_string(),
            }],
        },
    ))
    .unwrap()
    .unwrap();

    assert_eq!(
        edit.changes,
        Some(
            [(
                importer_uri,
                vec![TextEdit::new(
                    Range::new(Position::new(0, 7), Position::new(0, 21)),
                    "\"./Renamed.sol\"".into(),
                )],
            )]
            .into_iter()
            .collect()
        )
    );
    assert!(edit.document_changes.is_none());
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(importer.clone())).unwrap().to_string(),
        "import \"./Target.sol\";"
    );
    assert_eq!(
        state.symbol_tables.read().document_links(&importer)[0].target,
        Some(old_target_uri)
    );
}

#[test]
fn will_rename_rejects_source_changed_since_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol open
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let mut state = state(&project);
    state
        .vfs
        .write()
        .set_file_contents(VfsPath::from(importer), Some(Rope::from("import \"./Other.sol\";")));

    let error = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(project.path("/src/Target.sol")).unwrap().to_string(),
                new_uri: Url::from_file_path(project.path("/src/Renamed.sol")).unwrap().to_string(),
            }],
        },
    ))
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[test]
fn will_rename_rejects_conflicting_moves_for_one_source() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol open
        import "./Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let target = Url::from_file_path(project.path("/src/Target.sol")).unwrap().to_string();
    let mut state = state(&project);

    let error = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: target.clone(),
                    new_uri: Url::from_file_path(project.path("/src/First.sol"))
                        .unwrap()
                        .to_string(),
                },
                FileRename {
                    old_uri: target,
                    new_uri: Url::from_file_path(project.path("/src/Second.sol"))
                        .unwrap()
                        .to_string(),
                },
            ],
        },
    ))
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
}

#[test]
fn will_rename_rejects_conflicting_moves_to_one_destination() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/First.sol open
        contract First {}

        //- /src/Second.sol open
        contract Second {}
        "#,
    );
    let destination = Url::from_file_path(project.path("/src/Renamed.sol")).unwrap().to_string();
    let mut state = state(&project);

    let error = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: Url::from_file_path(project.path("/src/First.sol"))
                        .unwrap()
                        .to_string(),
                    new_uri: destination.clone(),
                },
                FileRename {
                    old_uri: Url::from_file_path(project.path("/src/Second.sol"))
                        .unwrap()
                        .to_string(),
                    new_uri: destination,
                },
            ],
        },
    ))
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
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
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.on_initialize(project.initialize_params()).await.unwrap();
    assert!(matches!(state.on_initialized(InitializedParams {}), ControlFlow::Continue(())));

    let edit = crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(project.path("/src/Target.sol")).unwrap().to_string(),
                new_uri: Url::from_file_path(project.path("/src/Renamed.sol")).unwrap().to_string(),
            }],
        },
    )
    .await
    .unwrap();

    assert!(edit.is_some());
}

#[tokio::test(flavor = "current_thread")]
async fn did_create_files_rediscovers_files_and_folder_descendants_once() {
    let project = TestProject::from_fixture(
        r#"
        //- /Existing.sol
        contract Existing {}
        "#,
    );
    let mut state = state(&project);
    project.write_file("/Direct.sol", "contract Direct {}");
    project.write_file("/created/Nested.sol", "contract Nested {}");
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let result = crate::handlers::did_create_files(
        &mut state,
        CreateFilesParams {
            files: vec![
                FileCreate {
                    uri: Url::from_file_path(project.path("/Direct.sol")).unwrap().to_string(),
                },
                FileCreate {
                    uri: Url::from_file_path(project.path("/created")).unwrap().to_string(),
                },
            ],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("create-file analysis should finish")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("Direct").iter().any(|symbol| symbol.name == "Direct"));
    assert!(tables.workspace_symbols("Nested").iter().any(|symbol| symbol.name == "Nested"));
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_folder_migrates_open_buffers_before_one_reanalysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /old/Open.sol open
        contract DiskVersion {}
        "#,
    );
    let old_folder = project.path("/old");
    let new_folder = project.path("/new");
    let old_file = project.path("/old/Open.sol");
    let new_file = project.path("/new/Open.sol");
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_file.clone()),
        Some(Rope::from("contract Unsaved {}")),
        Some(12),
    );
    fs::rename(&old_folder, &new_folder).unwrap();

    let result = crate::handlers::did_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
                new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(!state.vfs.read().exists(&VfsPath::from(old_file.clone())));
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(new_file.clone())).unwrap().to_string(),
        "contract Unsaved {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_file.clone())), Some(12));

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(old_file).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(&new_file).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ],
        },
    );
    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_file)), Some(12));

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("rename-file analysis should finish")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("DiskVersion").is_empty());
    assert!(tables.workspace_symbols("Unsaved").iter().any(|symbol| symbol.name == "Unsaved"));
}

#[test]
fn did_rename_ignores_conflicting_moves_for_one_source() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Target.sol open
        contract Target {}
        "#,
    );
    let old_path = project.path("/src/Target.sol");
    let first_path = project.path("/src/First.sol");
    let second_path = project.path("/src/Second.sol");
    let mut state = state(&project);
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let result = crate::handlers::did_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: Url::from_file_path(&old_path).unwrap().to_string(),
                    new_uri: Url::from_file_path(&first_path).unwrap().to_string(),
                },
                FileRename {
                    old_uri: Url::from_file_path(&old_path).unwrap().to_string(),
                    new_uri: Url::from_file_path(&second_path).unwrap().to_string(),
                },
            ],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(state.vfs.read().exists(&VfsPath::from(old_path)));
    assert!(!state.vfs.read().exists(&VfsPath::from(first_path)));
    assert!(!state.vfs.read().exists(&VfsPath::from(second_path)));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_ignores_conflicting_moves_to_one_destination() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/First.sol open
        contract First {}

        //- /src/Second.sol open
        contract Second {}
        "#,
    );
    let first_path = project.path("/src/First.sol");
    let second_path = project.path("/src/Second.sol");
    let destination = project.path("/src/Renamed.sol");
    let mut state = state(&project);
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let result = crate::handlers::did_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: Url::from_file_path(&first_path).unwrap().to_string(),
                    new_uri: Url::from_file_path(&destination).unwrap().to_string(),
                },
                FileRename {
                    old_uri: Url::from_file_path(&second_path).unwrap().to_string(),
                    new_uri: Url::from_file_path(&destination).unwrap().to_string(),
                },
            ],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(state.vfs.read().exists(&VfsPath::from(first_path)));
    assert!(state.vfs.read().exists(&VfsPath::from(second_path)));
    assert!(!state.vfs.read().exists(&VfsPath::from(destination)));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
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
    let old_root = project.path("/project");
    let new_root = project.path("/renamed");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config_with_roots(&["/project"]));
    fs::rename(&old_root, &new_root).unwrap();

    let result = crate::handlers::did_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(old_root).unwrap().to_string(),
                new_uri: Url::from_file_path(&new_root).unwrap().to_string(),
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("workspace-root rename analysis should finish")
        .unwrap();
    let workspaces = state.config.workspaces();
    assert_eq!(workspaces.len(), 1);
    assert_eq!(workspaces[0].kind(), WorkspaceKind::Foundry);
    assert_eq!(workspaces[0].compile_opts().base_path.as_deref(), Some(new_root.as_path()));
    assert!(tables.read().workspace_symbols("Main").iter().any(|symbol| symbol.name == "Main"));
    assert_eq!(
        tables.read().document_links(&new_root.join("src/Main.sol"))[0].target,
        Some(Url::from_file_path(new_root.join("lib/Dependency.sol")).unwrap())
    );
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
    let deleted_folder = project.path("/pkg");
    let deleted_file = project.path("/pkg/Deleted.sol");
    let kept_file = project.path("/pkg2/Keep.sol");
    let mut state = state(&project);
    fs::remove_dir_all(&deleted_folder).unwrap();

    let result = crate::handlers::did_delete_files(
        &mut state,
        DeleteFilesParams {
            files: vec![FileDelete {
                uri: Url::from_file_path(deleted_folder).unwrap().to_string(),
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(!state.vfs.read().exists(&VfsPath::from(deleted_file)));
    assert!(state.vfs.read().exists(&VfsPath::from(kept_file)));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("delete-file analysis should finish")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("Deleted").is_empty());
    assert!(tables.workspace_symbols("Keep").iter().any(|symbol| symbol.name == "Keep"));
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
    let deleted_folder = project.path("/lib");
    let deleted_uri = Url::from_file_path(project.path("/lib/pkg/Dependency.sol")).unwrap();
    let sibling_uri = Url::from_file_path(project.path("/lib2/Keep.sol")).unwrap();
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
    fs::remove_dir_all(&deleted_folder).unwrap();

    let result = crate::handlers::did_delete_files(
        &mut state,
        DeleteFilesParams {
            files: vec![FileDelete {
                uri: Url::from_file_path(deleted_folder).unwrap().to_string(),
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("delete-file analysis should finish")
        .unwrap();
    let diagnostics = state.diagnostics.read();
    assert!(matches!(
        diagnostics.pull_report(&deleted_uri, None),
        PullReport::Full { diagnostics, .. } if diagnostics.is_empty()
    ));
    assert!(matches!(
        diagnostics.pull_report(&sibling_uri, None),
        PullReport::Full { diagnostics, .. }
            if diagnostics == vec![diagnostic("sibling")]
    ));
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
    let root = project.path("/project");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config_with_roots(&["/project"]));
    fs::remove_dir_all(&root).unwrap();

    let result = crate::handlers::did_delete_files(
        &mut state,
        DeleteFilesParams {
            files: vec![FileDelete { uri: Url::from_file_path(root).unwrap().to_string() }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("workspace-root delete analysis should finish")
        .unwrap();
    assert!(state.config.workspaces().is_empty());
    assert!(tables.read().workspace_symbols("Deleted").is_empty());
}

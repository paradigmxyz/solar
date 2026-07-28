use super::{GlobalState, state};
use crate::{test_support::TestProject, vfs::VfsPath};
use async_lsp::{ClientSocket, ErrorCode};
use crop::Rope;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileDelete, FileRename, Position, Range,
    RenameFilesParams, TextEdit, Url,
};
use std::future::Future;

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
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

use super::{
    super::{AnalysisOutputAccumulator, analyze_cancellable, snapshot_with_config},
    GlobalState, state,
};
use crate::{
    config::{Config, negotiate_capabilities},
    test_support::TestProject,
    vfs::VfsPath,
};
use async_lsp::{ClientSocket, ErrorCode};
use crop::Rope;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileDelete, FileRename, Position, Range,
    RenameFilesParams, TextEdit, Url,
};
use std::{fs, future::Future, sync::Arc};

fn block_on<F: Future>(future: F) -> F::Output {
    tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap().block_on(future)
}

fn state_with_config(project: &TestProject, config: Config) -> GlobalState {
    let mut outputs = AnalysisOutputAccumulator::default();
    for batch in snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new()) {
        if !batch.files.is_empty() {
            outputs.push(
                analyze_cancellable(batch, &Default::default())
                    .expect("fresh analysis cancellation cannot be cancelled"),
            );
        }
    }
    let output = outputs.finish();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    *state.vfs.write() = project.vfs();
    *state.symbol_tables.write() = output.result.symbol_tables;
    state.analysis_commit.lock().analysis_paths = output.analysis_paths;
    state
}

fn config_with_initialization_options(
    project: &TestProject,
    initialization_options: Option<serde_json::Value>,
) -> Config {
    let mut params = project.initialize_params();
    params.initialization_options = initialization_options;
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    config
}

fn assert_will_file_operations_refuse_pruned_importer(
    project: &TestProject,
    config: Config,
    importer: &str,
    target: &str,
) {
    let importer = project.path(importer);
    let target = project.path(target);
    let renamed = target.with_file_name("Renamed.sol");

    let mut delete_state = state_with_config(project, config.clone());
    assert!(
        delete_state.symbol_tables.read().document_links(&importer).is_empty(),
        "pruned importer was unexpectedly analyzed: {}",
        importer.display()
    );
    let delete = block_on(crate::handlers::will_delete_files(
        &mut delete_state,
        DeleteFilesParams {
            files: vec![FileDelete { uri: Url::from_file_path(&target).unwrap().to_string() }],
        },
    ))
    .unwrap();
    assert!(delete.is_none(), "delete returned a partial edit for {}", importer.display());

    let mut rename_state = state_with_config(project, config);
    assert!(
        rename_state.symbol_tables.read().document_links(&importer).is_empty(),
        "pruned importer was unexpectedly analyzed: {}",
        importer.display()
    );
    let rename = block_on(crate::handlers::will_rename_files(
        &mut rename_state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(target).unwrap().to_string(),
                new_uri: Url::from_file_path(renamed).unwrap().to_string(),
            }],
        },
    ))
    .unwrap();
    assert!(rename.is_none(), "rename returned a partial edit for {}", importer.display());
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
fn will_delete_refuses_partial_import_edits() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /src/Main.sol
        import "@lib/Target.sol";

        //- /lib/Dependency.sol open
        import "./Target.sol";

        //- /lib/Target.sol
        contract Target {}
        "#,
    );
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_delete_files(
        &mut state,
        DeleteFilesParams {
            files: vec![FileDelete {
                uri: Url::from_file_path(project.path("/lib/Target.sol")).unwrap().to_string(),
            }],
        },
    ))
    .unwrap();

    assert!(edit.is_none());
}

#[test]
fn will_delete_edits_closed_default_named_source_importers() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol
        import "./Target.sol";

        //- /src/lib/Library.sol
        import "../Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_delete_files(
        &mut state,
        DeleteFilesParams {
            files: vec![FileDelete {
                uri: Url::from_file_path(project.path("/src/Target.sol")).unwrap().to_string(),
            }],
        },
    ))
    .unwrap()
    .unwrap();

    let changes = edit.changes.unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes.contains_key(&Url::from_file_path(project.path("/src/Main.sol")).unwrap()));
    assert!(
        changes.contains_key(&Url::from_file_path(project.path("/src/lib/Library.sol")).unwrap())
    );
}

#[test]
fn will_delete_refuses_closed_excluded_source_importers() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."

        //- /Main.sol
        import "./Target.sol";

        //- /out/Generated.sol
        import "../Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
    );
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_delete_files(
        &mut state,
        DeleteFilesParams {
            files: vec![FileDelete {
                uri: Url::from_file_path(project.path("/Target.sol")).unwrap().to_string(),
            }],
        },
    ))
    .unwrap();

    assert!(edit.is_none());
}

#[test]
fn will_file_operations_refuse_closed_importers_omitted_by_source_pruning() {
    let cases = [
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Main.sol
            import "./Target.sol";

            //- /src/.hidden/Importer.sol
            import "../Target.sol";

            //- /src/Target.sol
            contract Target {}
            "#,
            None,
            "/src/.hidden/Importer.sol",
            "/src/Target.sol",
        ),
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Main.sol
            import "./Target.sol";

            //- /src/nested/.git
            gitdir: elsewhere

            //- /src/nested/Importer.sol
            import "../Target.sol";

            //- /src/Target.sol
            contract Target {}
            "#,
            None,
            "/src/nested/Importer.sol",
            "/src/Target.sol",
        ),
        (
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Main.sol
            import "./Target.sol";

            //- /src/generated/Importer.sol
            import "../Target.sol";

            //- /src/Target.sol
            contract Target {}
            "#,
            Some(serde_json::json!({
                "indexing": { "exclude": ["src/generated/**"] }
            })),
            "/src/generated/Importer.sol",
            "/src/Target.sol",
        ),
        (
            r#"
            //- /Main.sol
            import "./Target.sol";

            //- /node_modules/Importer.sol
            import "../Target.sol";

            //- /Target.sol
            contract Target {}
            "#,
            None,
            "/node_modules/Importer.sol",
            "/Target.sol",
        ),
    ];

    for (fixture, initialization_options, importer, target) in cases {
        let project = TestProject::from_fixture(fixture);
        let config = config_with_initialization_options(&project, initialization_options);
        assert_will_file_operations_refuse_pruned_importer(&project, config, importer, target);
    }
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
fn will_rename_refuses_partial_import_edits() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib/=lib/"]

        //- /src/Main.sol
        import "@lib/Target.sol";

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
fn will_rename_edits_closed_default_named_source_importers() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol
        import "./Target.sol";

        //- /src/lib/Library.sol
        import "../Target.sol";

        //- /src/Target.sol
        contract Target {}
        "#,
    );
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(project.path("/src/Target.sol")).unwrap().to_string(),
                new_uri: Url::from_file_path(project.path("/src/Renamed.sol")).unwrap().to_string(),
            }],
        },
    ))
    .unwrap()
    .unwrap();

    let changes = edit.changes.unwrap();
    assert_eq!(changes.len(), 2);
    assert!(changes.contains_key(&Url::from_file_path(project.path("/src/Main.sol")).unwrap()));
    assert!(
        changes.contains_key(&Url::from_file_path(project.path("/src/lib/Library.sol")).unwrap())
    );
}

#[test]
fn will_rename_refuses_closed_excluded_source_importers() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."

        //- /Main.sol
        import "./Target.sol";

        //- /out/Generated.sol
        import "../Target.sol";

        //- /Target.sol
        contract Target {}
        "#,
    );
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![FileRename {
                old_uri: Url::from_file_path(project.path("/Target.sol")).unwrap().to_string(),
                new_uri: Url::from_file_path(project.path("/Renamed.sol")).unwrap().to_string(),
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
fn will_rename_validates_closed_importer_on_disk_across_workspace_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /one/Importer.sol
        import "../two/Target.sol";

        //- /two/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/one/Importer.sol");
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(project.path("/two/Target.sol")).unwrap().to_string(),
            new_uri: Url::from_file_path(project.path("/two/Renamed.sol")).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.config = Arc::new(project.config_with_roots(&["/one", "/two"]));

    let edit =
        block_on(crate::handlers::will_rename_files(&mut state, params.clone())).unwrap().unwrap();

    let changes = edit.changes.unwrap();
    let edits = changes.get(&Url::from_file_path(&importer).unwrap()).unwrap();
    assert_eq!(edits[0].new_text, "\"../two/Renamed.sol\"");

    let old_target = project.path("/two/Target.sol");
    let new_target = project.path("/two/Renamed.sol");
    fs::write(&importer, format!("import {};\n", edits[0].new_text)).unwrap();
    fs::rename(&old_target, &new_target).unwrap();
    let tables = super::analyze_project(&project);
    let links = tables.document_links(&importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), new_target);

    fs::write(&importer, "import \"../two/Other.sol\";").unwrap();
    let error = block_on(crate::handlers::will_rename_files(&mut state, params)).unwrap_err();
    assert_eq!(error.code, ErrorCode::CONTENT_MODIFIED);
}

#[test]
fn will_rename_rewrites_independently_moved_importer_and_target() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol
        import "../deps/Target.sol";

        //- /deps/Target.sol
        contract Target {}
        "#,
    );
    let importer = project.path("/src/Importer.sol");
    let moved_importer = project.path("/contracts/nested/Importer.sol");
    let target = project.path("/deps/Target.sol");
    let moved_target = project.path("/vendor/pkg/Target.sol");
    let params = RenameFilesParams {
        files: vec![
            FileRename {
                old_uri: Url::from_file_path(&importer).unwrap().to_string(),
                new_uri: Url::from_file_path(&moved_importer).unwrap().to_string(),
            },
            FileRename {
                old_uri: Url::from_file_path(&target).unwrap().to_string(),
                new_uri: Url::from_file_path(&moved_target).unwrap().to_string(),
            },
        ],
    };
    let mut state = state(&project);

    let edit = block_on(crate::handlers::will_rename_files(&mut state, params)).unwrap().unwrap();

    let changes = edit.changes.unwrap();
    let edits = changes.get(&Url::from_file_path(&importer).unwrap()).unwrap();
    assert_eq!(edits[0].new_text, "\"../../vendor/pkg/Target.sol\"");

    fs::write(&importer, format!("import {};\n", edits[0].new_text)).unwrap();
    fs::create_dir_all(moved_importer.parent().unwrap()).unwrap();
    fs::create_dir_all(moved_target.parent().unwrap()).unwrap();
    fs::rename(importer, &moved_importer).unwrap();
    fs::rename(target, &moved_target).unwrap();
    let tables = super::analyze_project(&project);
    let links = tables.document_links(&moved_importer);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target.as_ref().unwrap().to_file_path().unwrap(), moved_target);
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

#[test]
fn will_rename_rejects_expanded_vfs_destination_collision() {
    let project = TestProject::from_fixture(
        r#"
        //- /A/x.sol open
        contract A {}

        //- /B/x.sol open
        contract B {}
        "#,
    );
    let mut state = state(&project);

    let error = block_on(crate::handlers::will_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: Url::from_file_path(project.path("/A")).unwrap().to_string(),
                    new_uri: Url::from_file_path(project.path("/out")).unwrap().to_string(),
                },
                FileRename {
                    old_uri: Url::from_file_path(project.path("/B/x.sol")).unwrap().to_string(),
                    new_uri: Url::from_file_path(project.path("/out/x.sol")).unwrap().to_string(),
                },
            ],
        },
    ))
    .unwrap_err();

    assert_eq!(error.code, ErrorCode::INVALID_PARAMS);
}

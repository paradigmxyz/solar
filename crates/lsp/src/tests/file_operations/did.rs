use super::{
    super::{ASYNC_TEST_TIMEOUT, diagnostic},
    GlobalState, state,
};
use crate::{
    diagnostics::{DiagnosticMap, DiagnosticOwner, PullReport},
    test_support::TestProject,
    vfs::VfsPath,
    workspace::WorkspaceKind,
};
use async_lsp::ClientSocket;
use crop::Rope;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileChangeType, FileCreate, FileDelete, FileEvent,
    FileRename, InitializedParams, RenameFilesParams, Url,
};
use std::{
    fs,
    ops::ControlFlow,
    path::Path,
    sync::{Arc, atomic::Ordering},
};

fn rename_watcher_events(old_root: &Path, new_root: &Path, paths: &[&str]) -> Vec<FileEvent> {
    paths
        .iter()
        .flat_map(|path| {
            [
                FileEvent {
                    uri: Url::from_file_path(old_root.join(path)).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(new_root.join(path)).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ]
        })
        .collect()
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
async fn did_create_watcher_echo_does_not_start_another_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /Existing.sol
        contract Existing {}
        "#,
    );
    let created = project.path("/Created.sol");
    let mut state = state(&project);
    project.write_file("/Created.sol", "contract Created {}");
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_create_files(
            &mut state,
            CreateFilesParams {
                files: vec![FileCreate { uri: Url::from_file_path(&created).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(created).unwrap(),
                    typ: FileChangeType::CREATED,
                }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn watcher_create_followed_by_did_create_starts_one_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /Existing.sol
        contract Existing {}
        "#,
    );
    let created = project.path("/Created.sol");
    let mut state = state(&project);
    project.write_file("/Created.sol", "contract Created {}");
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(&created).unwrap(),
                    typ: FileChangeType::CREATED,
                }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert!(matches!(
        crate::handlers::did_create_files(
            &mut state,
            CreateFilesParams {
                files: vec![FileCreate { uri: Url::from_file_path(created).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn did_delete_watcher_echo_does_not_start_another_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /Deleted.sol open
        contract Deleted {}
        "#,
    );
    let deleted = project.path("/Deleted.sol");
    let mut state = state(&project);
    fs::remove_file(&deleted).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_delete_files(
            &mut state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(&deleted).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(deleted).unwrap(),
                    typ: FileChangeType::DELETED,
                }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn watcher_delete_followed_by_did_delete_starts_one_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /Deleted.sol open
        contract Deleted {}
        "#,
    );
    let deleted = project.path("/Deleted.sol");
    let mut state = state(&project);
    fs::remove_file(&deleted).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: vec![FileEvent {
                    uri: Url::from_file_path(&deleted).unwrap(),
                    typ: FileChangeType::DELETED,
                }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert!(matches!(
        crate::handlers::did_delete_files(
            &mut state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(&deleted).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(!state.vfs.read().exists(&VfsPath::from(deleted)));
}

#[tokio::test(flavor = "current_thread")]
async fn folder_create_split_watcher_echoes_do_not_start_another_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /Existing.sol
        contract Existing {}
        "#,
    );
    let folder = project.path("/created");
    let first = project.path("/created/First.sol");
    let second = project.path("/created/Second.sol");
    let mut state = state(&project);
    project.write_file("/created/First.sol", "contract First {}");
    project.write_file("/created/Second.sol", "contract Second {}");
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_create_files(
            &mut state,
            CreateFilesParams {
                files: vec![FileCreate { uri: Url::from_file_path(folder).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    for path in [first, second] {
        assert!(matches!(
            crate::handlers::did_change_watched_files(
                &mut state,
                lsp_types::DidChangeWatchedFilesParams {
                    changes: vec![FileEvent {
                        uri: Url::from_file_path(path).unwrap(),
                        typ: FileChangeType::CREATED,
                    }],
                },
            ),
            ControlFlow::Continue(())
        ));
    }

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn folder_watcher_create_followed_by_did_create_starts_one_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /Existing.sol
        contract Existing {}
        "#,
    );
    let folder = project.path("/created");
    let first = project.path("/created/First.sol");
    let second = project.path("/created/Second.sol");
    let mut state = state(&project);
    project.write_file("/created/First.sol", "contract First {}");
    project.write_file("/created/Second.sol", "contract Second {}");
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: [first, second]
                    .into_iter()
                    .map(|path| FileEvent {
                        uri: Url::from_file_path(path).unwrap(),
                        typ: FileChangeType::CREATED,
                    })
                    .collect(),
            },
        ),
        ControlFlow::Continue(())
    ));
    assert!(matches!(
        crate::handlers::did_create_files(
            &mut state,
            CreateFilesParams {
                files: vec![FileCreate { uri: Url::from_file_path(folder).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn folder_delete_split_watcher_echoes_do_not_start_another_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /deleted/First.sol
        contract First {}

        //- /deleted/Second.sol open
        contract Second {}
        "#,
    );
    let folder = project.path("/deleted");
    let first = project.path("/deleted/First.sol");
    let second = project.path("/deleted/Second.sol");
    let mut state = state(&project);
    fs::remove_dir_all(&folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_delete_files(
            &mut state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(folder).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    for path in [first, second] {
        assert!(matches!(
            crate::handlers::did_change_watched_files(
                &mut state,
                lsp_types::DidChangeWatchedFilesParams {
                    changes: vec![FileEvent {
                        uri: Url::from_file_path(path).unwrap(),
                        typ: FileChangeType::DELETED,
                    }],
                },
            ),
            ControlFlow::Continue(())
        ));
    }

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn folder_watcher_delete_followed_by_did_delete_starts_one_epoch() {
    let project = TestProject::from_fixture(
        r#"
        //- /deleted/First.sol
        contract First {}

        //- /deleted/Second.sol
        contract Second {}
        "#,
    );
    let folder = project.path("/deleted");
    let first = project.path("/deleted/First.sol");
    let second = project.path("/deleted/Second.sol");
    let mut state = state(&project);
    fs::remove_dir_all(&folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: [first, second]
                    .into_iter()
                    .map(|path| FileEvent {
                        uri: Url::from_file_path(path).unwrap(),
                        typ: FileChangeType::DELETED,
                    })
                    .collect(),
            },
        ),
        ControlFlow::Continue(())
    ));
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("watcher delete analysis should finish")
        .unwrap();
    assert!(matches!(
        crate::handlers::did_delete_files(
            &mut state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(folder).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
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
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

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
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_file)), Some(12));

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("rename-file analysis should finish")
        .unwrap();
    let tables = tables.read();
    assert!(tables.workspace_symbols("DiskVersion").is_empty());
    assert!(tables.workspace_symbols("Unsaved").iter().any(|symbol| symbol.name == "Unsaved"));
}

#[tokio::test(flavor = "current_thread")]
async fn watcher_can_commit_prepared_rename_before_did_notification() {
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
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_file.clone()),
        Some(Rope::from("contract Unsaved {}")),
        Some(12),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_folder, &new_folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(&old_file).unwrap(),
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
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(!state.vfs.read().exists(&VfsPath::from(old_file)));
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(new_file.clone())).unwrap().to_string(),
        "contract Unsaved {}"
    );

    let result = crate::handlers::did_rename_files(&mut state, params);

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_file)), Some(12));
}

#[tokio::test(flavor = "current_thread")]
async fn split_watcher_echo_after_did_rename_is_ignored() {
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
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_file.clone()),
        Some(Rope::from("contract Unsaved {}")),
        Some(12),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_folder, &new_folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let renamed = crate::handlers::did_rename_files(&mut state, params.clone());
    assert!(matches!(renamed, ControlFlow::Continue(())));
    for event in [
        FileEvent { uri: Url::from_file_path(&old_file).unwrap(), typ: FileChangeType::DELETED },
        FileEvent { uri: Url::from_file_path(&new_file).unwrap(), typ: FileChangeType::CREATED },
    ] {
        for _ in 0..2 {
            let watched = crate::handlers::did_change_watched_files(
                &mut state,
                lsp_types::DidChangeWatchedFilesParams { changes: vec![event.clone()] },
            );
            assert!(matches!(watched, ControlFlow::Continue(())));
        }
    }

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_file)), Some(12));
}

#[tokio::test(flavor = "current_thread")]
async fn unrelated_create_does_not_end_split_watcher_echo() {
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
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_folder, &new_folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let renamed = crate::handlers::did_rename_files(&mut state, params.clone());
    assert!(matches!(renamed, ControlFlow::Continue(())));
    let deleted = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&old_file).unwrap(),
                typ: FileChangeType::DELETED,
            }],
        },
    );
    assert!(matches!(deleted, ControlFlow::Continue(())));

    project.write_file("/other/New.sol", "contract New {}");
    let created = crate::handlers::did_create_files(
        &mut state,
        CreateFilesParams {
            files: vec![FileCreate {
                uri: Url::from_file_path(project.path("/other/New.sol")).unwrap().to_string(),
            }],
        },
    );
    assert!(matches!(created, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 2);

    let echo = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&new_file).unwrap(),
                typ: FileChangeType::CREATED,
            }],
        },
    );
    assert!(matches!(echo, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 2);

    let replay = crate::handlers::did_rename_files(&mut state, params);
    assert!(matches!(replay, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 2);
}

#[tokio::test(flavor = "current_thread")]
async fn split_watcher_events_commit_prepared_rename_once() {
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
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_file.clone()),
        Some(Rope::from("contract Unsaved {}")),
        Some(12),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_folder, &new_folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let deleted = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&old_file).unwrap(),
                typ: FileChangeType::DELETED,
            }],
        },
    );
    assert!(matches!(deleted, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
    assert!(state.vfs.read().exists(&VfsPath::from(old_file.clone())));

    let created_event =
        FileEvent { uri: Url::from_file_path(&new_file).unwrap(), typ: FileChangeType::CREATED };
    let created = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams { changes: vec![created_event.clone()] },
    );
    assert!(matches!(created, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(!state.vfs.read().exists(&VfsPath::from(old_file)));
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_file)), Some(12));

    let replay = crate::handlers::did_rename_files(&mut state, params);
    assert!(matches!(replay, ControlFlow::Continue(())));
    for _ in 0..2 {
        let watched = crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams { changes: vec![created_event.clone()] },
        );
        assert!(matches!(watched, ControlFlow::Continue(())));
    }
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn reversed_split_watcher_events_commit_prepared_rename_once() {
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
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_folder, &new_folder).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    for (index, event) in [
        FileEvent { uri: Url::from_file_path(&new_file).unwrap(), typ: FileChangeType::CREATED },
        FileEvent { uri: Url::from_file_path(&old_file).unwrap(), typ: FileChangeType::DELETED },
    ]
    .into_iter()
    .enumerate()
    {
        let watched = crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams { changes: vec![event] },
        );
        assert!(matches!(watched, ControlFlow::Continue(())));
        assert_eq!(
            state.analysis_version.load(Ordering::Relaxed),
            previous_version + usize::from(index == 1)
        );
    }

    let replay = crate::handlers::did_rename_files(&mut state, params);
    assert!(matches!(replay, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(!state.vfs.read().exists(&VfsPath::from(old_file)));
    assert!(state.vfs.read().exists(&VfsPath::from(new_file)));
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
    let a = project.path("/A");
    let b = project.path("/B");
    let x = project.path("/X");
    let y = project.path("/Y");
    let params = RenameFilesParams {
        files: vec![
            FileRename {
                old_uri: Url::from_file_path(&a).unwrap().to_string(),
                new_uri: Url::from_file_path(&b).unwrap().to_string(),
            },
            FileRename {
                old_uri: Url::from_file_path(&x).unwrap().to_string(),
                new_uri: Url::from_file_path(&y).unwrap().to_string(),
            },
        ],
    };
    let mut state = state(&project);
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&a, &b).unwrap();
    fs::rename(&x, &y).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(project.path("/A/Open.sol")).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(project.path("/B/Open.sol")).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ],
        },
    );

    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/A/Open.sol"))));
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/X/Open.sol"))));

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(project.path("/X/Open.sol")).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(project.path("/Y/Open.sol")).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ],
        },
    );

    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(!state.vfs.read().exists(&VfsPath::from(project.path("/A/Open.sol"))));
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/B/Open.sol"))));
    assert!(!state.vfs.read().exists(&VfsPath::from(project.path("/X/Open.sol"))));
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/Y/Open.sol"))));

    let replay = crate::handlers::did_rename_files(&mut state, params);
    assert!(matches!(replay, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_will_rename_does_not_leave_a_watcher_transaction() {
    let project = TestProject::from_fixture(
        r#"
        //- /src/Importer.sol open
        import "../old/Target.sol";

        //- /old/Target.sol open
        contract Target {}
        "#,
    );
    let old_folder = project.path("/old");
    let new_folder = project.path("/new");
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents(
        VfsPath::from(project.path("/src/Importer.sol")),
        Some(Rope::from("import \"../old/Other.sol\";")),
    );
    let error = crate::handlers::will_rename_files(&mut state, params).await.unwrap_err();
    assert_eq!(error.code, async_lsp::ErrorCode::CONTENT_MODIFIED);
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(project.path("/old/Target.sol")).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(project.path("/new/Target.sol")).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ],
        },
    );

    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/old/Target.sol"))));
    assert!(!state.vfs.read().exists(&VfsPath::from(project.path("/new/Target.sol"))));
}

#[tokio::test(flavor = "current_thread")]
async fn cancelled_will_rename_does_not_leave_a_watcher_transaction() {
    let project = TestProject::from_fixture(
        r#"
        //- /old/Target.sol open
        contract Target {}
        "#,
    );
    let old_folder = project.path("/old");
    let new_folder = project.path("/new");
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    drop(crate::handlers::will_rename_files(&mut state, params));
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(project.path("/old/Target.sol")).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(project.path("/new/Target.sol")).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ],
        },
    );

    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/old/Target.sol"))));
    assert!(!state.vfs.read().exists(&VfsPath::from(project.path("/new/Target.sol"))));
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
    let old_folder = project.path("/old");
    let new_folder = project.path("/new");
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_folder).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_folder).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    crate::handlers::will_rename_files(&mut state, params).await.unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: vec![
                FileEvent {
                    uri: Url::from_file_path(project.path("/old/A.sol")).unwrap(),
                    typ: FileChangeType::DELETED,
                },
                FileEvent {
                    uri: Url::from_file_path(project.path("/new/B.sol")).unwrap(),
                    typ: FileChangeType::CREATED,
                },
            ],
        },
    );

    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/old/A.sol"))));
    assert!(state.vfs.read().exists(&VfsPath::from(project.path("/old/B.sol"))));
    assert!(!state.vfs.read().exists(&VfsPath::from(project.path("/new/A.sol"))));
    assert!(!state.vfs.read().exists(&VfsPath::from(project.path("/new/B.sol"))));
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_replay_does_not_reapply_overlapping_moves() {
    let project = TestProject::from_fixture(
        r#"
        //- /A.sol open
        contract A {}

        //- /B.sol open
        contract B {}
        "#,
    );
    let a = project.path("/A.sol");
    let b = project.path("/B.sol");
    let c = project.path("/C.sol");
    let params = RenameFilesParams {
        files: vec![
            FileRename {
                old_uri: Url::from_file_path(&a).unwrap().to_string(),
                new_uri: Url::from_file_path(&b).unwrap().to_string(),
            },
            FileRename {
                old_uri: Url::from_file_path(&b).unwrap().to_string(),
                new_uri: Url::from_file_path(&c).unwrap().to_string(),
            },
        ],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(a.clone()),
        Some(Rope::from("contract UnsavedA {}")),
        Some(11),
    );
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(b.clone()),
        Some(Rope::from("contract UnsavedB {}")),
        Some(22),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&b, &c).unwrap();
    fs::rename(&a, &b).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    for _ in 0..2 {
        let result = crate::handlers::did_rename_files(&mut state, params.clone());
        assert!(matches!(result, ControlFlow::Continue(())));
    }

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(b.clone())).unwrap().to_string(),
        "contract UnsavedA {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(b)), Some(11));
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(c.clone())).unwrap().to_string(),
        "contract UnsavedB {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(c)), Some(22));
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_replay_does_not_toggle_swap() {
    let project = TestProject::from_fixture(
        r#"
        //- /A.sol open
        contract A {}

        //- /B.sol open
        contract B {}
        "#,
    );
    let a = project.path("/A.sol");
    let b = project.path("/B.sol");
    let temporary = project.path("/Temporary.sol");
    let params = RenameFilesParams {
        files: vec![
            FileRename {
                old_uri: Url::from_file_path(&a).unwrap().to_string(),
                new_uri: Url::from_file_path(&b).unwrap().to_string(),
            },
            FileRename {
                old_uri: Url::from_file_path(&b).unwrap().to_string(),
                new_uri: Url::from_file_path(&a).unwrap().to_string(),
            },
        ],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(a.clone()),
        Some(Rope::from("contract UnsavedA {}")),
        Some(11),
    );
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(b.clone()),
        Some(Rope::from("contract UnsavedB {}")),
        Some(22),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&a, &temporary).unwrap();
    fs::rename(&b, &a).unwrap();
    fs::rename(&temporary, &b).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    for _ in 0..2 {
        let result = crate::handlers::did_rename_files(&mut state, params.clone());
        assert!(matches!(result, ControlFlow::Continue(())));
    }

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(a.clone())).unwrap().to_string(),
        "contract UnsavedB {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(a)), Some(22));
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(b.clone())).unwrap().to_string(),
        "contract UnsavedA {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(b)), Some(11));
}

#[tokio::test(flavor = "current_thread")]
async fn file_rename_accepts_percent_encoded_file_uris() {
    let project = TestProject::from_fixture(
        r##"
        //- /src/Importer.sol open
        import "./Target file.sol";
        "##,
    );
    project.write_file("/src/Target file.sol", "contract Target {}");
    let importer = project.path("/src/Importer.sol");
    let old_target = project.path("/src/Target file.sol");
    let new_target = project.path("/src/Renamed # file.sol");
    let old_uri = Url::from_file_path(&old_target).unwrap();
    let new_uri = Url::from_file_path(&new_target).unwrap();
    assert!(old_uri.as_str().contains("%20"));
    assert!(new_uri.as_str().contains("%23"));
    let params = RenameFilesParams {
        files: vec![FileRename { old_uri: old_uri.to_string(), new_uri: new_uri.to_string() }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_target.clone()),
        Some(Rope::from("contract UnsavedTarget {}")),
        Some(9),
    );

    let edit =
        crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap().unwrap();
    let edits = edit.changes.unwrap();
    assert_eq!(
        edits.get(&Url::from_file_path(importer).unwrap()).unwrap()[0].new_text,
        "\"./Renamed # file.sol\""
    );
    fs::rename(&old_target, &new_target).unwrap();
    let result = crate::handlers::did_rename_files(&mut state, params);

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(!state.vfs.read().exists(&VfsPath::from(old_target)));
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(new_target.clone())).unwrap().to_string(),
        "contract UnsavedTarget {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_target)), Some(9));
}

#[tokio::test(flavor = "current_thread")]
async fn did_rename_migrates_case_only_file_move() {
    let project = TestProject::from_fixture(
        r#"
        //- /Case.sol open
        contract Case {}
        "#,
    );
    let old_path = project.path("/Case.sol");
    let new_path = project.path("/case.sol");
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_path).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_path).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_path.clone()),
        Some(Rope::from("contract UnsavedCase {}")),
        Some(4),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_path, &new_path).unwrap();

    let result = crate::handlers::did_rename_files(&mut state, params);

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(!state.vfs.read().exists(&VfsPath::from(old_path)));
    assert_eq!(
        state.vfs.read().get_file_contents(&VfsPath::from(new_path.clone())).unwrap().to_string(),
        "contract UnsavedCase {}"
    );
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_path)), Some(4));
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

#[test]
fn did_rename_ignores_expanded_vfs_destination_collision() {
    let project = TestProject::from_fixture(
        r#"
        //- /A/x.sol open
        contract A {}

        //- /B/x.sol open
        contract B {}
        "#,
    );
    let a = project.path("/A/x.sol");
    let b = project.path("/B/x.sol");
    let destination = project.path("/out/x.sol");
    let mut state = state(&project);
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let result = crate::handlers::did_rename_files(
        &mut state,
        RenameFilesParams {
            files: vec![
                FileRename {
                    old_uri: Url::from_file_path(project.path("/A")).unwrap().to_string(),
                    new_uri: Url::from_file_path(project.path("/out")).unwrap().to_string(),
                },
                FileRename {
                    old_uri: Url::from_file_path(&b).unwrap().to_string(),
                    new_uri: Url::from_file_path(&destination).unwrap().to_string(),
                },
            ],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert!(state.vfs.read().exists(&VfsPath::from(a)));
    assert!(state.vfs.read().exists(&VfsPath::from(b)));
    assert!(!state.vfs.read().exists(&VfsPath::from(destination)));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
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
    let a = project.path("/A/x.sol");
    let b = project.path("/B/x.sol");
    let destination = project.path("/out/x.sol");
    let params = RenameFilesParams {
        files: vec![
            FileRename {
                old_uri: Url::from_file_path(project.path("/A")).unwrap().to_string(),
                new_uri: Url::from_file_path(project.path("/out")).unwrap().to_string(),
            },
            FileRename {
                old_uri: Url::from_file_path(&b).unwrap().to_string(),
                new_uri: Url::from_file_path(&destination).unwrap().to_string(),
            },
        ],
    };
    let mut state = state(&project);
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(b.clone()),
        Some(Rope::from("contract UnsavedB {}")),
        Some(22),
    );
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            lsp_types::DidChangeWatchedFilesParams {
                changes: vec![
                    FileEvent {
                        uri: Url::from_file_path(&a).unwrap(),
                        typ: FileChangeType::DELETED,
                    },
                    FileEvent {
                        uri: Url::from_file_path(&b).unwrap(),
                        typ: FileChangeType::DELETED,
                    },
                    FileEvent {
                        uri: Url::from_file_path(&destination).unwrap(),
                        typ: FileChangeType::CREATED,
                    },
                ],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version);
    assert!(state.vfs.read().exists(&VfsPath::from(a.clone())));
    assert!(state.vfs.read().exists(&VfsPath::from(b.clone())));
    assert!(!state.vfs.read().exists(&VfsPath::from(destination.clone())));

    state.vfs.write().set_file_contents(VfsPath::from(b), None);
    assert!(matches!(
        crate::handlers::did_rename_files(&mut state, params),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert!(!state.vfs.read().exists(&VfsPath::from(a)));
    assert!(state.vfs.read().exists(&VfsPath::from(destination)));
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

    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_root).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_root).unwrap().to_string(),
        }],
    };
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let result = crate::handlers::did_rename_files(&mut state, params.clone());

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

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: rename_watcher_events(
                &old_root,
                &new_root,
                &["foundry.toml", "src/Main.sol", "lib/Dependency.sol"],
            ),
        },
    );
    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);

    let replay = crate::handlers::did_rename_files(&mut state, params);
    assert!(matches!(replay, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(
        state.config.workspaces()[0].compile_opts().base_path.as_deref(),
        Some(new_root.as_path())
    );
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
    let old_root = project.path("/project");
    let new_root = project.path("/renamed");
    let old_main = old_root.join("src/Main.sol");
    let new_main = new_root.join("src/Main.sol");
    let params = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&old_root).unwrap().to_string(),
            new_uri: Url::from_file_path(&new_root).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(old_main.clone()),
        Some(Rope::from("import \"@lib/Dependency.sol\";\ncontract Unsaved {}")),
        Some(12),
    );
    crate::handlers::will_rename_files(&mut state, params.clone()).await.unwrap();
    fs::rename(&old_root, &new_root).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    let watched = crate::handlers::did_change_watched_files(
        &mut state,
        lsp_types::DidChangeWatchedFilesParams {
            changes: rename_watcher_events(
                &old_root,
                &new_root,
                &["foundry.toml", "src/Main.sol", "lib/Dependency.sol"],
            ),
        },
    );
    assert!(matches!(watched, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(
        state.config.workspaces()[0].compile_opts().base_path.as_deref(),
        Some(new_root.as_path())
    );
    assert!(!state.vfs.read().exists(&VfsPath::from(old_main)));
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(new_main.clone())), Some(12));

    let replay = crate::handlers::did_rename_files(&mut state, params);
    assert!(matches!(replay, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("workspace-root rename analysis should finish")
        .unwrap();
    assert!(
        tables.read().workspace_symbols("Unsaved").iter().any(|symbol| symbol.name == "Unsaved")
    );
    assert_eq!(
        tables.read().document_links(&new_main)[0].target,
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
    let old_root = project.path("/A");
    let moved_root = project.path("/B");
    let replay_target = project.path("/C");
    let params = RenameFilesParams {
        files: vec![
            FileRename {
                old_uri: Url::from_file_path(&old_root).unwrap().to_string(),
                new_uri: Url::from_file_path(&moved_root).unwrap().to_string(),
            },
            FileRename {
                old_uri: Url::from_file_path(&moved_root).unwrap().to_string(),
                new_uri: Url::from_file_path(&replay_target).unwrap().to_string(),
            },
        ],
    };
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config_with_roots(&["/A"]));
    fs::rename(&old_root, &moved_root).unwrap();
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    for _ in 0..2 {
        let result = crate::handlers::did_rename_files(&mut state, params.clone());
        assert!(matches!(result, ControlFlow::Continue(())));
    }

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("workspace-root rename analysis should finish")
        .unwrap();
    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 1);
    assert_eq!(
        state.config.workspaces()[0].compile_opts().base_path.as_deref(),
        Some(moved_root.as_path())
    );
    assert!(tables.read().workspace_symbols("Main").iter().any(|symbol| symbol.name == "Main"));
}

#[tokio::test(flavor = "current_thread")]
async fn did_only_rename_round_trip_reapplies_original_payload() {
    let project = TestProject::from_fixture(
        r#"
        //- /A/Main.sol open
        contract DiskVersion {}
        "#,
    );
    let a = project.path("/A");
    let b = project.path("/B");
    let a_main = a.join("Main.sol");
    let b_main = b.join("Main.sol");
    let forward = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&a).unwrap().to_string(),
            new_uri: Url::from_file_path(&b).unwrap().to_string(),
        }],
    };
    let reverse = RenameFilesParams {
        files: vec![FileRename {
            old_uri: Url::from_file_path(&b).unwrap().to_string(),
            new_uri: Url::from_file_path(&a).unwrap().to_string(),
        }],
    };
    let mut state = state(&project);
    state.config = Arc::new(project.config_with_roots(&["/A"]));
    state.vfs.write().set_file_contents_with_version(
        VfsPath::from(a_main.clone()),
        Some(Rope::from("contract Unsaved {}")),
        Some(12),
    );
    let previous_version = state.analysis_version.load(Ordering::Relaxed);

    fs::rename(&a, &b).unwrap();
    assert!(matches!(
        crate::handlers::did_rename_files(&mut state, forward.clone()),
        ControlFlow::Continue(())
    ));
    fs::rename(&b, &a).unwrap();
    assert!(matches!(
        crate::handlers::did_rename_files(&mut state, reverse),
        ControlFlow::Continue(())
    ));
    fs::rename(&a, &b).unwrap();
    assert!(matches!(
        crate::handlers::did_rename_files(&mut state, forward.clone()),
        ControlFlow::Continue(())
    ));
    assert!(matches!(
        crate::handlers::did_rename_files(&mut state, forward),
        ControlFlow::Continue(())
    ));

    assert_eq!(state.analysis_version.load(Ordering::Relaxed), previous_version + 3);
    assert!(!state.vfs.read().exists(&VfsPath::from(a_main)));
    assert_eq!(state.vfs.read().get_file_version(&VfsPath::from(b_main)), Some(12));
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

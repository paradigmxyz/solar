use super::*;
use lsp_types::{CreateFilesParams, DeleteFilesParams, FileCreate, FileDelete};

#[tokio::test(flavor = "current_thread")]
async fn watched_solidity_change_ignores_unrelated_excluded_file() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /generated/Unrelated.sol
        contract Unrelated {}
        "#,
    );
    let path = project.path("/generated/Unrelated.sol");
    let uri = Url::from_file_path(path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config_with_indexing_excludes(&project, &["generated/**"]));
    let version = state.analysis_version.load(Ordering::Acquire);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert!(state.analysis_scheduler.tasks.lock().coordinator.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn watched_excluded_manifest_events_do_not_schedule_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /node_modules/package/foundry.toml
        [profile.default]
        src = "src"
        "#,
    );

    for typ in [FileChangeType::CREATED, FileChangeType::CHANGED, FileChangeType::DELETED] {
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(project.config());
        let version = state.analysis_version.load(Ordering::Acquire);
        let uri = Url::from_file_path(project.path("/node_modules/package/foundry.toml")).unwrap();

        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri, typ }] },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
        assert!(state.analysis_scheduler.tasks.lock().coordinator.is_none());
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_nested_manifest_create_discovers_the_project() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml

        //- /packages/app/foundry.toml

        //- /packages/app/generated/.keep
        "#,
    );
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "indexing": { "exclude": ["packages/app/generated/**"] }
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    project.write_file(
        "/packages/app/generated/foundry.toml",
        r#"
        [profile.default]
        src = "src"
        "#,
    );
    project.write_file("/packages/app/generated/src/Nested.sol", "contract Nested {}");
    let uri = Url::from_file_path(project.path("/packages/app/generated/foundry.toml")).unwrap();

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CREATED }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("nested manifest analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Nested").iter().any(|symbol| symbol.name == "Nested"));
}

#[tokio::test(flavor = "current_thread")]
async fn created_directory_under_overlapping_source_root_schedules_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "lib"
        "#,
    );
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    let version = state.analysis_version.load(Ordering::Acquire);
    let path = project.path("/lib/generated");
    std::fs::create_dir_all(&path).unwrap();

    let result = crate::handlers::did_create_files(
        &mut state,
        CreateFilesParams {
            files: vec![FileCreate { uri: Url::from_file_path(path).unwrap().to_string() }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let actual_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    assert_eq!(actual_version, version + 1);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_created_source_under_overlapping_root_is_tracked() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "lib"
        "#,
    );
    let config = project.config();
    let path = project.path("/lib/Created.sol");
    project.write_file("/lib/Created.sol", "contract Created {}");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&path).unwrap(),
                typ: FileChangeType::CREATED,
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
    assert_eq!(state.config.tracked_source_files_under(&[project.path("/lib")]), [path]);
    state.analysis_scheduler.tasks.lock().cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn watched_created_directory_under_partitioned_root_is_rediscovered() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."

        //- /lib/Dependency.sol
        contract Dependency {}
        "#,
    );
    let config = project.config();
    let directory = project.path("/new");
    let source = project.path("/new/New.sol");
    let unrelated = project.path("/README.md");
    project.write_file("/README.md", "notes");
    project.write_file("/new/New.sol", "contract New {}");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(unrelated).unwrap(),
                typ: FileChangeType::CREATED,
            }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 0);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(directory).unwrap(),
                typ: FileChangeType::CREATED,
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
    assert_eq!(state.config.tracked_source_files_under(&[project.root().to_path_buf()]), [source]);
    state.analysis_scheduler.tasks.lock().cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn watched_deleted_directory_under_partitioned_root_is_rediscovered() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."

        //- /lib/Dependency.sol
        contract Dependency {}

        //- /old/Old.sol
        contract Old {}
        "#,
    );
    let config = project.config();
    let directory = project.path("/old");
    std::fs::remove_dir_all(&directory).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(directory).unwrap(),
                typ: FileChangeType::DELETED,
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
    assert!(state.config.tracked_source_files_under(&[project.root().to_path_buf()]).is_empty());
    state.analysis_scheduler.tasks.lock().cancel();
}

#[tokio::test(flavor = "current_thread")]
async fn created_directory_below_default_exclude_does_not_schedule_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}
        "#,
    );
    let path = project.path("/node_modules/package/generated");
    std::fs::create_dir_all(&path).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(project.config());
    let version = state.analysis_version.load(Ordering::Acquire);

    let result = crate::handlers::did_create_files(
        &mut state,
        CreateFilesParams {
            files: vec![FileCreate { uri: Url::from_file_path(path).unwrap().to_string() }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    let actual_version = state.analysis_version.load(Ordering::Acquire);
    state.analysis_scheduler.tasks.lock().cancel();
    assert_eq!(actual_version, version);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_excluded_dependency_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /Main.sol
            import "./generated/Dependency.sol";
            contract Main is Dependency {}

            //- /generated/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = config_with_indexing_excludes(&project, &["generated/**"]);
        let mut batches =
            snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
        let output =
            analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        state.snapshot().publish_analysis_output(0, output);
        let path = project.path("/generated/Dependency.sol");
        let uri = Url::from_file_path(path).unwrap();

        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri, typ }] },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
        state.analysis_scheduler.tasks.lock().cancel();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_external_source_create_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CREATED, FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /project/foundry.toml
            [profile.default]
            src = "../shared"
            "#,
        );
        let path = project.path("/shared/External.sol");
        if typ != FileChangeType::CREATED {
            project.write_file("/shared/External.sol", "contract External {}");
        }
        let config = project.config_with_roots(&["/project"]);
        if typ == FileChangeType::CREATED {
            project.write_file("/shared/External.sol", "contract External {}");
        } else if typ == FileChangeType::DELETED {
            std::fs::remove_file(&path).unwrap();
        }
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);

        assert!(matches!(
            crate::handlers::did_change_watched_files(
                &mut state,
                DidChangeWatchedFilesParams {
                    changes: vec![FileEvent { uri: Url::from_file_path(&path).unwrap(), typ }],
                },
            ),
            ControlFlow::Continue(())
        ));

        assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
        let tracked = state.config.tracked_source_files_under(&[project.path("/shared")]);
        if typ == FileChangeType::DELETED {
            assert!(tracked.is_empty());
        } else {
            assert_eq!(tracked, [path]);
        }
        state.analysis_scheduler.tasks.lock().cancel();
    }
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_dependency_event_is_deferred_while_analysis_is_pending() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#,
    );
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config_with_indexing_excludes(&project, &["generated/**"]));
    state.mark_analysis_pending_for_test();
    let path = project.path("/generated/Dependency.sol");

    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent {
                uri: Url::from_file_path(&path).unwrap(),
                typ: FileChangeType::CHANGED,
            }],
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::CHANGED)
    );
}

#[test]
fn did_create_defers_a_candidate_first_learned_by_pending_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let path = project.path("/generated/Dependency.sol");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    project.write_file("/generated/Dependency.sol", "contract Dependency {}");

    assert!(matches!(
        crate::handlers::did_create_files(
            &mut state,
            CreateFilesParams {
                files: vec![FileCreate { uri: Url::from_file_path(&path).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::CREATED)
    );
    assert!(!state.snapshot().publish_analysis_output(version, output));
}

#[test]
fn did_delete_defers_a_dependency_first_learned_by_pending_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let path = project.path("/generated/Dependency.sol");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    std::fs::remove_file(&path).unwrap();

    assert!(matches!(
        crate::handlers::did_delete_files(
            &mut state,
            DeleteFilesParams {
                files: vec![FileDelete { uri: Url::from_file_path(&path).unwrap().to_string() }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::DELETED)
    );
    assert!(!state.snapshot().publish_analysis_output(version, output));
}

#[tokio::test(flavor = "current_thread")]
async fn source_events_during_initial_discovery_are_replayed_after_policy_is_known() {
    let project = TestProject::from_fixture(
        r#"
        //- /project/foundry.toml
        [profile.default]
        src = "lib"
        libs = ["vendor"]
        "#,
    );
    let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&["/project"]));
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let discovery = state.config.discover_workspaces(&IndexingCancellation::default()).unwrap();

    project.write_file("/project/lib/Active.sol", "contract Active {}");
    project.write_file("/project/node_modules/Ignored.sol", "contract Ignored {}");
    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: [
                project.path("/project/lib/Active.sol"),
                project.path("/project/node_modules/Ignored.sol"),
            ]
            .into_iter()
            .map(|path| FileEvent {
                uri: Url::from_file_path(path).unwrap(),
                typ: FileChangeType::CREATED,
            })
            .collect(),
        },
    );

    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert!(matches!(
        state.on_workspace_discovery_ready(WorkspaceDiscoveryReady {
            version,
            result: discovery,
            disk_paths: Vec::new(),
            progress,
            cancellation: IndexingCancellation::default(),
        }),
        ControlFlow::Continue(())
    ));

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("initial discovery analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Active").iter().any(|symbol| symbol.name == "Active"));
    assert!(tables.read().workspace_symbols("Ignored").is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn source_events_during_discovery_are_deferred_with_existing_workspaces() {
    let project = TestProject::from_fixture(
        r#"
        //- /existing/Existing.sol
        contract Existing {}
        "#,
    );
    let new_root = project.path("/new");
    std::fs::create_dir(&new_root).unwrap();
    let (_, mut config) =
        negotiate_capabilities(project.initialize_params_with_roots(&["/existing"]));
    config.rediscover_workspaces();
    assert!(!config.workspaces().is_empty());
    config.add_workspaces([new_root.clone()]);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let discovery = state.config.discover_workspaces(&IndexingCancellation::default()).unwrap();
    let path = project.path("/new/Active.sol");
    project.write_file("/new/Active.sol", "contract Active {}");

    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![
                    FileEvent {
                        uri: Url::from_file_path(&path).unwrap(),
                        typ: FileChangeType::CREATED,
                    },
                    FileEvent {
                        uri: Url::from_file_path(&path).unwrap(),
                        typ: FileChangeType::CHANGED,
                    },
                ],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), version);
    assert_eq!(
        state.analysis_commit.lock().deferred_source_file_events.get(&path),
        Some(&FileChangeType::CHANGED)
    );

    assert!(matches!(
        state.on_workspace_discovery_ready(WorkspaceDiscoveryReady {
            version,
            result: discovery,
            disk_paths: Vec::new(),
            progress,
            cancellation: IndexingCancellation::default(),
        }),
        ControlFlow::Continue(())
    ));
    assert_eq!(
        state.config.tracked_source_files_under(std::slice::from_ref(&new_root)),
        std::slice::from_ref(&path)
    );
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("rediscovered source analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Active").iter().any(|symbol| symbol.name == "Active"));
    drop(tables);

    state.recompute_after_source_changes(vec![project.path("/existing/Existing.sol")]);
    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("subsequent analysis should finish")
        .unwrap();
    assert!(tables.read().workspace_symbols("Active").iter().any(|symbol| symbol.name == "Active"));
}

#[tokio::test(flavor = "current_thread")]
async fn watched_existing_unresolved_candidate_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            libs = ["lib-one", "lib-two"]

            //- /src/Main.sol
            import "Dependency.sol";
            contract Main is Dependency {}

            //- /lib-one/Dependency.sol
            contract Dependency {}

            //- /lib-two/Dependency.sol
            contract Dependency {}
            "#,
        );
        let config = project.config();
        let mut batches =
            snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
        let output =
            analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
        let path = project.path("/lib-one/Dependency.sol");
        match typ {
            FileChangeType::CHANGED => {
                std::fs::write(&path, "contract Dependency { uint x; }").unwrap()
            }
            FileChangeType::DELETED => std::fs::remove_file(&path).unwrap(),
            _ => unreachable!(),
        }
        let uri = Url::from_file_path(path).unwrap();
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        state.snapshot().publish_analysis_output(0, output);

        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri, typ }] },
        );

        assert!(matches!(result, ControlFlow::Continue(())));
        let actual_version = state.analysis_version.load(Ordering::Acquire);
        state.analysis_scheduler.tasks.lock().cancel();
        assert_eq!(actual_version, 1);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_missing_excluded_dependency_recovers_only_on_create() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Missing.sol";
        contract Main is Missing {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.snapshot().publish_analysis_output(0, output);
    let path = project.path("/generated/Missing.sol");
    let uri = Url::from_file_path(&path).unwrap();

    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        let result = crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams { changes: vec![FileEvent { uri: uri.clone(), typ }] },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
        assert_eq!(state.analysis_version.load(Ordering::Acquire), 0);
    }

    project.write_file("/generated/Missing.sol", "contract Missing {}");
    let result = crate::handlers::did_change_watched_files(
        &mut state,
        DidChangeWatchedFilesParams {
            changes: vec![FileEvent { uri, typ: FileChangeType::CREATED }],
        },
    );
    assert!(matches!(result, ControlFlow::Continue(())));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);

    let tables = tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("created import candidate should be analyzed")
        .unwrap();
    assert!(
        tables.read().workspace_symbols("Missing").iter().any(|symbol| symbol.name == "Missing")
    );
}

#[tokio::test(flavor = "current_thread")]
async fn watched_missing_candidate_change_supersedes_pending_create_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Missing.sol";
        contract Main is Missing {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    let output =
        analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.snapshot().publish_analysis_output(0, output);
    let path = project.path("/generated/Missing.sol");
    let uri = Url::from_file_path(&path).unwrap();

    project.write_file("/generated/Missing.sol", "contract Missing {}");
    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent { uri: uri.clone(), typ: FileChangeType::CREATED }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 1);

    project.write_file("/generated/Missing.sol", "contract Missing { uint latest; }");
    assert!(matches!(
        crate::handlers::did_change_watched_files(
            &mut state,
            DidChangeWatchedFilesParams {
                changes: vec![FileEvent { uri, typ: FileChangeType::CHANGED }],
            },
        ),
        ControlFlow::Continue(())
    ));
    assert_eq!(state.analysis_version.load(Ordering::Acquire), 2);

    tokio::time::timeout(ASYNC_TEST_TIMEOUT, state.latest_analysis())
        .await
        .expect("replacement dependency analysis should finish")
        .unwrap();
}

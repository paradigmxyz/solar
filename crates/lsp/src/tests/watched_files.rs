use super::*;
use async_lsp::{ErrorCode, ResponseError};
use lsp_types::{
    DidChangeWatchedFilesClientCapabilities, RegistrationParams, UnregistrationParams, WatchKind,
};

#[derive(Debug)]
enum WatchedFileClientEvent {
    Register(RegistrationParams),
    Unregister(UnregistrationParams),
}

async fn next_watched_file_client_event(
    events: &mut mpsc::UnboundedReceiver<WatchedFileClientEvent>,
) -> WatchedFileClientEvent {
    tokio::time::timeout(ASYNC_TEST_TIMEOUT, events.recv())
        .await
        .expect("watched-file client event should arrive")
        .expect("watched-file client event channel should stay open")
}

fn watched_file_registration_has_spec(
    params: &RegistrationParams,
    base: &Path,
    pattern: &str,
) -> bool {
    let base_uri = Url::from_file_path(base).unwrap().to_string();
    params.registrations.iter().any(|registration| {
        registration.register_options.as_ref().is_some_and(|options| {
            options["watchers"].as_array().is_some_and(|watchers| {
                watchers.iter().any(|watcher| {
                    watcher["globPattern"]["baseUri"].as_str() == Some(&base_uri)
                        && watcher["globPattern"]["pattern"].as_str() == Some(pattern)
                })
            })
        })
    })
}

fn watched_file_registration_spec_kind(
    params: &RegistrationParams,
    base: &Path,
    pattern: &str,
) -> Option<u64> {
    let base_uri = Url::from_file_path(base).unwrap().to_string();
    params.registrations.iter().find_map(|registration| {
        registration.register_options.as_ref().and_then(|options| {
            options["watchers"].as_array().and_then(|watchers| {
                watchers.iter().find_map(|watcher| {
                    (watcher["globPattern"]["baseUri"].as_str() == Some(&base_uri)
                        && watcher["globPattern"]["pattern"].as_str() == Some(pattern))
                    .then(|| watcher["kind"].as_u64())
                    .flatten()
                })
            })
        })
    })
}

fn watched_file_registration_has_recursive_spec_covering(
    params: &RegistrationParams,
    path: &Path,
) -> bool {
    params.registrations.iter().any(|registration| {
        registration.register_options.as_ref().is_some_and(|options| {
            options["watchers"].as_array().is_some_and(|watchers| {
                watchers.iter().any(|watcher| {
                    matches!(
                        watcher["globPattern"]["pattern"].as_str(),
                        Some("**/*.sol" | "**/foundry.toml")
                    ) && watcher["globPattern"]["baseUri"]
                        .as_str()
                        .and_then(|uri| Url::parse(uri).ok())
                        .and_then(|uri| uri.to_file_path().ok())
                        .is_some_and(|base| path.starts_with(base))
                })
            })
        })
    })
}
#[tokio::test(flavor = "current_thread")]
async fn watched_file_specs_are_prepared_after_the_analysis_commit_unlocks() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    state.mark_analysis_pending_for_test();
    let version = state.analysis_version.load(Ordering::Acquire);
    let mut snapshot = state.snapshot();
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([
                project.path("/workspace/deps/Dependency.sol")
            ]),
            ..Default::default()
        },
    };
    let desired_specs = state.watched_file_registration.desired_specs.lock();
    let runtime = tokio::runtime::Handle::current();

    std::thread::scope(|scope| {
        let publisher = scope.spawn(move || {
            let _runtime = runtime.enter();
            snapshot.publish_analysis_output(version, output)
        });

        let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
        while *state.published_analysis_version.borrow() != version && Instant::now() < deadline {
            std::thread::yield_now();
        }
        assert_eq!(*state.published_analysis_version.borrow(), version);

        let deadline = Instant::now() + Duration::from_millis(100);
        let mut commit_became_available = false;
        while Instant::now() < deadline {
            if state.analysis_commit.try_lock().is_some() {
                commit_became_available = true;
                break;
            }
            std::thread::yield_now();
        }

        drop(desired_specs);
        assert!(publisher.join().unwrap());
        assert!(
            commit_became_available,
            "watched-file registration preparation held the analysis commit lock"
        );
    });
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_folder_change_advances_epoch_before_watcher_reregistration() {
    let project = TestProject::new();
    let old_root = project.path("/old");
    let new_root = project.path("/new");
    std::fs::create_dir(&old_root).unwrap();
    std::fs::create_dir(&new_root).unwrap();
    let mut params = project.initialize_params_with_roots(&["/old"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let initial_version = state.analysis_version.load(Ordering::Acquire);
    let registration = state.watched_file_registration.clone();
    let desired_specs = registration.desired_specs.lock();
    let analysis_version = state.analysis_version.clone();
    let runtime = tokio::runtime::Handle::current();
    let worker = std::thread::spawn(move || {
        let _runtime = runtime.enter();
        let result = crate::handlers::did_change_workspace_folders(
            &mut state,
            DidChangeWorkspaceFoldersParams {
                event: WorkspaceFoldersChangeEvent {
                    added: vec![WorkspaceFolder {
                        uri: Url::from_file_path(new_root).unwrap(),
                        name: "new".into(),
                    }],
                    removed: vec![WorkspaceFolder {
                        uri: Url::from_file_path(old_root).unwrap(),
                        name: "old".into(),
                    }],
                },
            },
        );
        assert!(matches!(result, ControlFlow::Continue(())));
        state
    });

    let deadline = Instant::now() + Duration::from_millis(100);
    while analysis_version.load(Ordering::Acquire) == initial_version && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let advanced_before_reregistration =
        analysis_version.load(Ordering::Acquire) != initial_version;

    drop(desired_specs);
    let state = worker.join().unwrap();
    state.analysis_scheduler.tasks.lock().cancel();
    assert!(
        advanced_before_reregistration,
        "workspace-folder change queued watchers before invalidating the old analysis epoch"
    );
}

#[test]
fn watched_file_registration_has_global_fallback_patterns() {
    let [registration] =
        watched_file_registration_params(&Config::default()).registrations.try_into().unwrap();
    assert_eq!(registration.id, "solar-watched-files");
    assert_eq!(registration.method, lsp_types::notification::DidChangeWatchedFiles::METHOD);

    assert_eq!(
        registration.register_options,
        Some(serde_json::json!({
            "watchers": [
                { "globPattern": "**/*.sol", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                { "globPattern": "**/foundry.toml", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                { "globPattern": "**/remappings.txt", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                { "globPattern": "**/.git", "kind": WatchKind::Create | WatchKind::Delete },
            ],
        }))
    );
}

#[test]
fn relative_watched_file_registration_tracks_nested_repository_markers() {
    let clean = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "contracts"

        //- /workspace/contracts/Main.sol
        contract Main {}

        //- /workspace/.git/HEAD
        "#,
    );
    let mut params = clean.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let registration = watched_file_registration_params(&config);
    let workspace_root = clean.path("/workspace");
    let source_root = clean.path("/workspace/contracts");

    assert!(watched_file_registration_has_spec(&registration, &source_root, "**/.git"));
    assert!(!watched_file_registration_has_spec(&registration, &workspace_root, ".git"));
    assert_eq!(
        watched_file_registration_spec_kind(&registration, &source_root, "**/.git"),
        Some((WatchKind::Create | WatchKind::Delete).bits().into())
    );

    let pruned = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "contracts"

        //- /workspace/contracts/Main.sol
        contract Main {}

        //- /workspace/contracts/nested/.git
        gitdir: elsewhere

        //- /workspace/contracts/nested/Nested.sol
        contract Nested {}
        "#,
    );
    let mut params = pruned.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let registration = watched_file_registration_params(&config);
    let marker_root = pruned.path("/workspace/contracts/nested");

    assert!(watched_file_registration_has_spec(&registration, &marker_root, ".git"));
    assert_eq!(
        watched_file_registration_spec_kind(&registration, &marker_root, ".git"),
        Some((WatchKind::Create | WatchKind::Delete).bits().into())
    );
}

#[test]
fn relative_watched_file_registration_keeps_approved_parent_manifest_marker_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /repo/foundry.toml
        [profile.default]
        src = "src"

        //- /repo/src/Main.sol
        contract Main {}

        //- /repo/src/vendor/.git
        gitdir: elsewhere

        //- /repo/src/vendor/Vendor.sol
        contract Vendor {}

        //- /repo/member/.keep
        "#,
    );
    let mut params = project.initialize_params_with_roots(&["/repo/member"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let registration = watched_file_registration_params(&config);
    let marker_root = project.path("/repo/src/vendor");

    assert!(watched_file_registration_has_spec(&registration, &marker_root, ".git"));
    assert_eq!(
        watched_file_registration_spec_kind(&registration, &marker_root, ".git"),
        Some((WatchKind::Create | WatchKind::Delete).bits().into())
    );
}

#[test]
fn relative_watched_file_registration_uses_bounded_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "contracts"

        //- /workspace/contracts/Main.sol
        contract Main {}

        //- /workspace/node_modules/Dependency.sol
        contract Dependency {}

        //- /workspace/out/Generated.sol
        contract Generated {}

        //- /workspace/.hidden/Hidden.sol
        contract Hidden {}

        //- /workspace/nested/.git/HEAD

        //- /workspace/nested/Nested.sol
        contract Nested {}
        "#,
    );
    let workspace_root = project.path("/workspace");
    let source_root = project.path("/workspace/contracts");
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);

    let initial = watched_file_registration_params(&config);
    assert!(watched_file_registration_has_spec(&initial, &workspace_root, "foundry.toml"));
    assert!(watched_file_registration_has_spec(&initial, &workspace_root, "remappings.txt"));
    for pattern in ["**/*.sol", "**/foundry.toml", "**/remappings.txt"] {
        assert!(!watched_file_registration_has_spec(&initial, &workspace_root, pattern));
    }

    config.rediscover_workspaces();
    let discovered = watched_file_registration_params(&config);
    assert!(watched_file_registration_has_spec(&discovered, &source_root, "**/*.sol"));
    assert!(watched_file_registration_has_spec(&discovered, &source_root, "**/foundry.toml"));
    assert!(watched_file_registration_has_spec(&discovered, &workspace_root, "foundry.toml"));
    assert!(watched_file_registration_has_spec(&discovered, &workspace_root, "remappings.txt"));
    for pattern in ["**/*.sol", "**/foundry.toml", "**/remappings.txt"] {
        assert!(!watched_file_registration_has_spec(&discovered, &workspace_root, pattern));
    }
}

#[test]
fn relative_watched_file_registration_partitions_root_sources() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry/foundry.toml
        [profile.default]
        src = "."

        //- /foundry/Root.sol
        contract Root {}

        //- /foundry/contracts/Main.sol
        contract Main {}

        //- /foundry/contracts/core/Core.sol
        contract Core {}

        //- /foundry/contracts/node_modules/Dependency.sol
        contract Dependency {}

        //- /foundry/contracts/out/Generated.sol
        contract Generated {}

        //- /foundry/contracts/.hidden/Hidden.sol
        contract Hidden {}

        //- /foundry/contracts/vendor/.git/HEAD

        //- /foundry/contracts/vendor/Nested.sol
        contract Nested {}

        //- /foundry/lib/Dependency.sol
        contract Dependency {}

        //- /foundry/out/Generated.sol
        contract Generated {}

        //- /foundry/.hidden/Hidden.sol
        contract Hidden {}

        //- /foundry/nested/.git/HEAD

        //- /foundry/nested/Nested.sol
        contract Nested {}

        //- /naked/Root.sol
        contract Root {}

        //- /naked/contracts/Main.sol
        contract Main {}

        //- /naked/node_modules/Dependency.sol
        contract Dependency {}
        "#,
    );
    let foundry_root = project.path("/foundry");
    let naked_root = project.path("/naked");
    let mut params = project.initialize_params_with_roots(&["/foundry", "/naked"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();

    let registration = watched_file_registration_params(&config);
    for root in [&foundry_root, &naked_root] {
        assert!(watched_file_registration_has_spec(&registration, root, "*"));
        assert!(watched_file_registration_has_spec(&registration, root, "*.sol"));
        assert!(!watched_file_registration_has_spec(&registration, root, "**/*.sol"));
        assert_eq!(watched_file_registration_spec_kind(&registration, root, "*"), Some(5));
        assert_eq!(watched_file_registration_spec_kind(&registration, root, "*.sol"), Some(2));
    }
    assert!(watched_file_registration_has_spec(
        &registration,
        &foundry_root.join("contracts"),
        "*"
    ));
    assert!(watched_file_registration_has_spec(
        &registration,
        &foundry_root.join("contracts"),
        "*.sol"
    ));
    assert!(watched_file_registration_has_spec(
        &registration,
        &foundry_root.join("contracts"),
        "foundry.toml"
    ));
    assert!(watched_file_registration_has_spec(
        &registration,
        &foundry_root.join("contracts/core"),
        "**/*.sol"
    ));
    assert!(watched_file_registration_has_spec(
        &registration,
        &foundry_root.join("contracts/core"),
        "**/foundry.toml"
    ));
    assert!(watched_file_registration_has_spec(
        &registration,
        &naked_root.join("contracts"),
        "**/*.sol"
    ));
    for excluded in [
        foundry_root.join("lib"),
        foundry_root.join("out"),
        foundry_root.join(".hidden"),
        foundry_root.join("nested"),
        foundry_root.join("contracts/node_modules"),
        foundry_root.join("contracts/out"),
        foundry_root.join("contracts/.hidden"),
        foundry_root.join("contracts/vendor"),
        naked_root.join("node_modules"),
    ] {
        assert!(!watched_file_registration_has_recursive_spec_covering(&registration, &excluded));
    }
}

#[test]
fn relative_watched_file_registration_respects_nested_workspace_ownership() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "."

        //- /nested/foundry.toml
        [profile.default]
        src = "src"
        libs = ["src/vendor"]

        //- /nested/Outside.sol
        contract Outside {}

        //- /nested/src/Included.sol
        contract Included {}

        //- /nested/src/generated/Excluded.sol
        contract Excluded {}

        //- /nested/src/vendor/Dependency.sol
        contract Dependency {}
        "#,
    );
    let nested_root = project.path("/nested");
    let nested_source_root = project.path("/nested/src");
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "indexing": { "exclude": ["src/generated/**"] }
    }));
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();

    let registration = watched_file_registration_params(&config);
    assert!(watched_file_registration_has_spec(&registration, &nested_source_root, "*"));
    assert!(watched_file_registration_has_spec(&registration, &nested_source_root, "*.sol"));
    assert!(watched_file_registration_has_spec(&registration, &nested_source_root, "foundry.toml"));
    assert!(!watched_file_registration_has_spec(&registration, &nested_root, "*.sol"));
    for excluded in [
        project.path("/nested/Outside.sol"),
        project.path("/nested/src/generated"),
        project.path("/nested/src/vendor"),
    ] {
        assert!(!watched_file_registration_has_recursive_spec_covering(&registration, &excluded));
    }
}

#[test]
fn relative_watched_file_registration_omits_excluded_source_root() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "contracts"

        //- /contracts/Main.sol
        contract Main {}
        "#,
    );
    let source_root = project.path("/contracts");
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "indexing": { "exclude": ["contracts/**"] }
    }));
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();

    let registration = watched_file_registration_params(&config);
    for pattern in ["*.sol", "**/*.sol"] {
        assert!(!watched_file_registration_has_spec(&registration, &source_root, pattern));
    }
    assert!(!watched_file_registration_has_recursive_spec_covering(&registration, &source_root));
}

#[test]
fn watched_file_registration_includes_parent_config_and_external_source_specs() {
    let project = TestProject::from_fixture(
        r#"
        //- /repo/foundry.toml
        [profile.default]
        src = "../shared/contracts"

        //- /repo/workspace/.keep
        "#,
    );
    let mut params = project.initialize_params_with_roots(&["/repo/workspace", "/shared"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let [registration] =
        watched_file_registration_params(&config).registrations.try_into().unwrap();
    let options = registration.register_options.unwrap();
    let watchers = options["watchers"].as_array().unwrap();
    let parent_uri = Url::from_file_path(project.path("/repo")).unwrap().to_string();
    let external_uri = Url::from_file_path(project.path("/shared/contracts")).unwrap().to_string();

    assert!(watchers.iter().any(|watcher| {
        watcher["globPattern"]["baseUri"] == parent_uri
            && watcher["globPattern"]["pattern"] == "foundry.toml"
    }));
    assert!(watchers.iter().any(|watcher| {
        watcher["globPattern"]["baseUri"] == external_uri
            && watcher["globPattern"]["pattern"] == "**/*.sol"
    }));
}

#[test]
fn watched_file_specs_add_only_approved_dependency_parents() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "src"
        libs = ["../include"]
        remappings = ["@mapped/=../mapped/"]

        //- /workspace/src/Main.sol
        contract Main {}

        //- /include/pkg/Include.sol
        contract Include {}

        //- /mapped/pkg/Mapped.sol
        contract Mapped {}
        "#,
    );
    let (_, mut config) =
        negotiate_capabilities(project.initialize_params_with_roots(&["/workspace"]));
    config.apply_workspace_discovery(WorkspaceDiscoveryResult {
        workspaces: vec![
            crate::workspace::Workspace::load_foundry_bounded(
                project.path("/workspace/foundry.toml"),
                &[project.path("/workspace")],
            )
            .unwrap(),
        ],
        manifest_watch_roots: Vec::new(),
        git_marker_watch_roots: Vec::new(),
        metrics: Default::default(),
    });
    let workspace_parent = project.path("/workspace/deps");
    let include_parent = project.path("/include/pkg");
    let remapping_parent = project.path("/mapped/pkg");
    let outside_parent = project.path("/outside");
    let missing_parent = project.path("/workspace/missing");
    let analysis_paths = AnalysisPathIndex {
        resolved_dependencies: FxHashSet::from_iter([
            workspace_parent.join("Dependency.sol"),
            include_parent.join("Include.sol"),
            remapping_parent.join("Mapped.sol"),
            outside_parent.join("Outside.sol"),
        ]),
        existing_unresolved_candidates: FxHashSet::from_iter([
            include_parent.join("Candidate.sol"),
            outside_parent.join("Candidate.sol"),
        ]),
        missing_candidates: FxHashSet::from_iter([missing_parent.join("Missing.sol")]),
    };

    let specs = watched_file_specs(&config, &analysis_paths);

    for parent in [&workspace_parent, &include_parent, &remapping_parent] {
        assert_eq!(
            specs.iter().filter(|spec| spec.base == *parent && spec.pattern == "*.sol").count(),
            1
        );
    }
    assert!(!specs.iter().any(|spec| spec.base == outside_parent));
    assert!(!specs.iter().any(|spec| spec.base == missing_parent));
}

#[test]
fn watched_file_specs_use_indexed_recursive_coverage() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "src"

        //- /workspace/src/Main.sol
        contract Main {}
        "#,
    );
    let config = project.config_with_roots(&["/workspace"]);
    let dependency_parent = project.path("/workspace/src/nested");
    let analysis_paths = AnalysisPathIndex {
        resolved_dependencies: FxHashSet::from_iter([dependency_parent.join("Dependency.sol")]),
        ..Default::default()
    };

    let specs = watched_file_specs(&config, &analysis_paths);

    assert!(
        specs.iter().any(|spec| {
            spec.base == project.path("/workspace/src") && spec.pattern == "**/*.sol"
        })
    );
    assert!(!specs.iter().any(|spec| spec.base == dependency_parent && spec.pattern == "*.sol"));
}

#[test]
fn watched_file_specs_cap_dynamic_dependency_parents() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&["/workspace"]));
    let dependency_root = project.path("/workspace/deps");
    let analysis_paths = AnalysisPathIndex {
        resolved_dependencies: (0..MAX_DYNAMIC_WATCHED_FILE_SPECS + 32)
            .map(|index| dependency_root.join(index.to_string()).join("Dependency.sol"))
            .collect(),
        ..Default::default()
    };

    let specs = watched_file_specs(&config, &analysis_paths);

    assert_eq!(
        specs
            .iter()
            .filter(|spec| spec.pattern == "*.sol" && spec.base.starts_with(&dependency_root))
            .count(),
        MAX_DYNAMIC_WATCHED_FILE_SPECS
    );
}

#[test]
fn concurrent_watched_file_updates_keep_desired_specs_and_generation_in_sync() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let config = Arc::new(config);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let barrier = Arc::new(Barrier::new(3));
    let specs = [
        vec![WatchedFileSpec::new(project.path("/first"), "**/*.sol")],
        vec![WatchedFileSpec::new(project.path("/second"), "**/*.sol")],
    ];

    let updates = std::thread::scope(|scope| {
        let handles = specs.map(|specs| {
            let config = config.clone();
            let coordinator = coordinator.clone();
            let barrier = barrier.clone();
            scope.spawn(move || {
                barrier.wait();
                prepare_watched_file_registration_update(&config, &coordinator, specs).unwrap()
            })
        });
        barrier.wait();
        handles.map(|handle| handle.join().unwrap())
    });

    let desired_specs = coordinator.desired_specs.lock().clone().unwrap();
    let generation = coordinator.generation.load(Ordering::Acquire);
    let current = updates.iter().find(|update| update.desired_specs == desired_specs).unwrap();
    assert_eq!(current.generation, generation);
}

#[test]
fn global_fallback_watched_file_update_ignores_spec_changes() {
    let project = TestProject::new();
    let mut params = project.initialize_params();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(false),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = WatchedFileRegistrationCoordinator::default();
    let first_specs = vec![WatchedFileSpec::new(project.path("/first"), "**/*.sol")];
    let first =
        prepare_watched_file_registration_update(&config, &coordinator, first_specs).unwrap();

    assert!(first.desired_specs.is_empty());
    let generation = first.generation;
    let second_specs = vec![WatchedFileSpec::new(project.path("/second"), "**/*.sol")];
    assert!(
        prepare_watched_file_registration_update(&config, &coordinator, second_specs).is_none()
    );
    assert_eq!(coordinator.generation.load(Ordering::Acquire), generation);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_watched_file_registration_allows_the_same_specs_to_retry() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let specs = config.watched_file_specs();
    let update =
        prepare_watched_file_registration_update(&config, &coordinator, specs.clone()).unwrap();
    let first_generation = update.generation;

    spawn_watched_file_registration_update(&ClientSocket::new_closed(), &coordinator, Some(update));
    let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
    while coordinator.desired_specs.lock().is_some() && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(coordinator.desired_specs.lock().is_none());

    let retry = prepare_watched_file_registration_update(&config, &coordinator, specs).unwrap();
    assert!(retry.generation > first_generation);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_watched_file_replacement_keeps_the_previous_registration() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let (server_main, client_socket) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let attempts = Arc::new(AtomicUsize::new(0));
    let (client_main, server_socket) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new((events_tx, attempts));
        router.request::<request::RegisterCapability, _>(|(events, attempts), params| {
            events.send(WatchedFileClientEvent::Register(params)).unwrap();
            let attempt = attempts.fetch_add(1, Ordering::AcqRel);
            async move {
                if attempt == 1 {
                    Err(ResponseError::new(
                        ErrorCode::REQUEST_FAILED,
                        "replacement registration failed",
                    ))
                } else {
                    Ok(())
                }
            }
        });
        router.request::<request::UnregisterCapability, _>(|(events, _), params| {
            events.send(WatchedFileClientEvent::Unregister(params)).unwrap();
            async { Ok(()) }
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let first_specs = vec![WatchedFileSpec::new(project.path("/first"), "**/*.sol")];
    let first =
        prepare_watched_file_registration_update(&config, &coordinator, first_specs).unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(first));
    let WatchedFileClientEvent::Register(first_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected initial watched-file registration")
    };
    let first_id = first_registration.registrations[0].id.clone();

    let second_specs = vec![WatchedFileSpec::new(project.path("/second"), "**/*.sol")];
    let second =
        prepare_watched_file_registration_update(&config, &coordinator, second_specs.clone())
            .unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(second));
    let WatchedFileClientEvent::Register(second_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected replacement registration before any unregistration")
    };
    assert_ne!(second_registration.registrations[0].id, first_id);

    let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
    while coordinator.desired_specs.lock().is_some() && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(coordinator.desired_specs.lock().is_none());
    assert!(events_rx.try_recv().is_err());
    assert_eq!(
        coordinator.active_registration_ids.lock().as_slice(),
        std::slice::from_ref(&first_id)
    );

    let retry =
        prepare_watched_file_registration_update(&config, &coordinator, second_specs).unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(retry));
    let WatchedFileClientEvent::Register(retry_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected watched-file registration retry")
    };
    let retry_id = retry_registration.registrations[0].id.clone();
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == first_id
    ));
    let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
    while *coordinator.active_registration_ids.lock() != [retry_id.clone()]
        && Instant::now() < deadline
    {
        tokio::task::yield_now().await;
    }
    assert_eq!(*coordinator.active_registration_ids.lock(), [retry_id]);

    server_socket.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_replacement_preserves_previous_registration_until_latest_is_active() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let (server_main, client_socket) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let (replacement_ack_tx, replacement_ack_rx) = oneshot::channel();
    let (client_main, server_socket) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new((events_tx, Some(replacement_ack_rx), 0usize));
        router.request::<request::RegisterCapability, _>(
            |(events, replacement_ack, attempts), params| {
                events.send(WatchedFileClientEvent::Register(params)).unwrap();
                let ack = (*attempts == 1).then(|| replacement_ack.take().unwrap());
                *attempts += 1;
                async move {
                    if let Some(ack) = ack {
                        ack.await.map_err(|_| {
                            ResponseError::new(
                                ErrorCode::REQUEST_FAILED,
                                "test registration ack dropped",
                            )
                        })?;
                    }
                    Ok(())
                }
            },
        );
        router.request::<request::UnregisterCapability, _>(|(events, _, _), params| {
            events.send(WatchedFileClientEvent::Unregister(params)).unwrap();
            async { Ok(()) }
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let shared_root = project.path("/shared");
    let first_specs = vec![WatchedFileSpec::new(shared_root.clone(), "**/*.sol")];
    let first =
        prepare_watched_file_registration_update(&config, &coordinator, first_specs).unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(first));
    let WatchedFileClientEvent::Register(first_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected initial watched-file registration")
    };
    let first_id = first_registration.registrations[0].id.clone();

    let second_specs = vec![WatchedFileSpec::new(project.path("/stale"), "**/*.sol")];
    let second =
        prepare_watched_file_registration_update(&config, &coordinator, second_specs).unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(second));
    let WatchedFileClientEvent::Register(second_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected replacement watched-file registration")
    };
    let second_id = second_registration.registrations[0].id.clone();
    assert_ne!(second_id, first_id);

    let latest_root = project.path("/latest");
    let third = prepare_watched_file_registration_update(
        &config,
        &coordinator,
        vec![
            WatchedFileSpec::new(shared_root.clone(), "**/*.sol"),
            WatchedFileSpec::new(latest_root.clone(), "**/*.sol"),
        ],
    )
    .unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(third));
    replacement_ack_tx.send(()).unwrap();

    let WatchedFileClientEvent::Register(third_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected latest registration before any unregistration")
    };
    let third_id = third_registration.registrations[0].id.clone();
    assert!(watched_file_registration_has_spec(&third_registration, &shared_root, "**/*.sol"));
    assert!(watched_file_registration_has_spec(&third_registration, &latest_root, "**/*.sol"));
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == first_id
    ));
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == second_id
    ));
    let deadline = Instant::now() + ASYNC_TEST_TIMEOUT;
    while *coordinator.active_registration_ids.lock() != [third_id.clone()]
        && Instant::now() < deadline
    {
        tokio::task::yield_now().await;
    }
    assert_eq!(*coordinator.active_registration_ids.lock(), [third_id]);

    server_socket.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[tokio::test(flavor = "current_thread")]
async fn failed_unregistration_is_retried_after_the_next_replacement() {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    let mut params = project.initialize_params_with_roots(&["/workspace"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let (server_main, client_socket) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let (client_main, server_socket) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new((events_tx, 0usize));
        router.request::<request::RegisterCapability, _>(|(events, _), params| {
            events.send(WatchedFileClientEvent::Register(params)).unwrap();
            async { Ok(()) }
        });
        router.request::<request::UnregisterCapability, _>(|(events, attempts), params| {
            events.send(WatchedFileClientEvent::Unregister(params)).unwrap();
            let attempt = *attempts;
            *attempts += 1;
            async move {
                if attempt == 0 {
                    Err(ResponseError::new(
                        ErrorCode::REQUEST_FAILED,
                        "first unregistration failed",
                    ))
                } else {
                    Ok(())
                }
            }
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let first = prepare_watched_file_registration_update(
        &config,
        &coordinator,
        vec![WatchedFileSpec::new(project.path("/first"), "**/*.sol")],
    )
    .unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(first));
    let WatchedFileClientEvent::Register(first_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected initial watched-file registration")
    };
    let first_id = first_registration.registrations[0].id.clone();

    let second = prepare_watched_file_registration_update(
        &config,
        &coordinator,
        vec![WatchedFileSpec::new(project.path("/second"), "**/*.sol")],
    )
    .unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(second));
    let WatchedFileClientEvent::Register(second_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected second watched-file registration")
    };
    let second_id = second_registration.registrations[0].id.clone();
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == first_id
    ));

    let third = prepare_watched_file_registration_update(
        &config,
        &coordinator,
        vec![WatchedFileSpec::new(project.path("/third"), "**/*.sol")],
    )
    .unwrap();
    spawn_watched_file_registration_update(&client_socket, &coordinator, Some(third));
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Register(_)
    ));
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == first_id
    ));
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == second_id
    ));

    server_socket.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

#[tokio::test(flavor = "current_thread")]
async fn synchronous_discovery_refreshes_watched_file_specs_before_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "contracts"

        //- /contracts/Main.sol
        contract Main {}
        "#,
    );
    let mut params = project.initialize_params();
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let initial_specs = config.watched_file_specs();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    *state.watched_file_registration.desired_specs.lock() = Some(initial_specs);

    state.rediscover_workspaces();

    let specs = state.watched_file_registration.desired_specs.lock();
    assert!(
        specs
            .as_ref()
            .unwrap()
            .iter()
            .any(|spec| { spec.base == project.path("/contracts") && spec.pattern == "**/*.sol" })
    );
}

#[tokio::test(flavor = "current_thread")]
async fn discovery_and_analysis_refresh_bounded_watched_file_specs() {
    let project = TestProject::from_fixture(
        r#"
        //- /repo/foundry.toml
        [profile.default]
        src = "../shared/contracts"

        //- /repo/workspace/.keep
        "#,
    );
    let mut params = project.initialize_params_with_roots(&["/repo/workspace", "/shared"]);
    params.capabilities.workspace = Some(WorkspaceClientCapabilities {
        did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
            dynamic_registration: Some(true),
            relative_pattern_support: Some(true),
        }),
        ..Default::default()
    });
    let (_, config) = negotiate_capabilities(params);
    let discovery = config.discover_workspaces(&IndexingCancellation::default()).unwrap();
    let (server_main, client_socket) = async_lsp::MainLoop::new_server(|_| {
        let mut router = Router::new(());
        router.notification::<notification::Exit>(|_, ()| ControlFlow::Break(Ok(())));
        router
    });
    let (events_tx, mut events_rx) = mpsc::unbounded_channel();
    let (client_main, server_socket) = async_lsp::MainLoop::new_client(move |_| {
        let mut router = Router::new(events_tx);
        router.request::<request::RegisterCapability, _>(|events, params| {
            events.send(WatchedFileClientEvent::Register(params)).unwrap();
            async { Ok(()) }
        });
        router.request::<request::UnregisterCapability, _>(|events, params| {
            events.send(WatchedFileClientEvent::Unregister(params)).unwrap();
            async { Ok(()) }
        });
        router
    });
    let (server_stream, client_stream) = tokio::io::duplex(64 << 10);
    let (server_rx, server_tx) = tokio::io::split(server_stream);
    let server_task =
        tokio::spawn(server_main.run_buffered(server_rx.compat(), server_tx.compat_write()));
    let (client_rx, client_tx) = tokio::io::split(client_stream);
    let client_task =
        tokio::spawn(client_main.run_buffered(client_rx.compat(), client_tx.compat_write()));

    let mut state = GlobalState::new(client_socket);
    state.config = Arc::new(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();

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
    let discovered_specs = state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(
        discovered_specs
            .iter()
            .any(|spec| { spec.base == project.path("/repo") && spec.pattern == "foundry.toml" })
    );
    assert!(discovered_specs.iter().any(|spec| {
        spec.base == project.path("/shared/contracts") && spec.pattern == "**/*.sol"
    }));
    let WatchedFileClientEvent::Register(discovered_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected discovered watched-file registration")
    };
    let discovered_id = discovered_registration.registrations[0].id.clone();
    assert!(watched_file_registration_has_spec(
        &discovered_registration,
        &project.path("/repo"),
        "foundry.toml"
    ));
    assert!(watched_file_registration_has_spec(
        &discovered_registration,
        &project.path("/shared/contracts"),
        "**/*.sol"
    ));

    state.analysis_scheduler.tasks.lock().cancel();
    let dependency_parent = project.path("/repo/dependencies");
    let outside_parent = project.path("/outside");
    let missing_parent = project.path("/repo/missing");
    let output = AnalysisOutput {
        result: AnalysisResult {
            analyzed_documents: AnalyzedDocuments::default(),
            diagnostics: DiagnosticMap::default(),
            symbol_tables: SymbolTables::default(),
        },
        analysis_paths: AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([
                dependency_parent.join("Dependency.sol"),
                outside_parent.join("Outside.sol"),
            ]),
            missing_candidates: FxHashSet::from_iter([missing_parent.join("Missing.sol")]),
            ..Default::default()
        },
    };
    assert!(state.snapshot().publish_analysis_output(version, output));
    let published_specs = state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(
        published_specs
            .iter()
            .any(|spec| { spec.base == dependency_parent && spec.pattern == "*.sol" })
    );
    assert!(
        !published_specs
            .iter()
            .any(|spec| spec.base == dependency_parent && spec.pattern == "**/*.sol")
    );
    assert!(!published_specs.iter().any(|spec| spec.base == outside_parent));
    assert!(!published_specs.iter().any(|spec| spec.base == missing_parent));
    let WatchedFileClientEvent::Register(published_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected analysis watched-file registration")
    };
    let published_id = published_registration.registrations[0].id.clone();
    assert_ne!(published_id, discovered_id);
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == discovered_id
    ));
    assert!(watched_file_registration_has_spec(
        &published_registration,
        &dependency_parent,
        "*.sol"
    ));
    assert!(!watched_file_registration_has_spec(
        &published_registration,
        &dependency_parent,
        "**/*.sol"
    ));
    assert!(!watched_file_registration_has_spec(&published_registration, &outside_parent, "*.sol"));
    assert!(!watched_file_registration_has_spec(&published_registration, &missing_parent, "*.sol"));

    state.clear_analysis_cache();
    let cleared_specs = state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(!cleared_specs.iter().any(|spec| spec.base == dependency_parent));
    let WatchedFileClientEvent::Register(cleared_registration) =
        next_watched_file_client_event(&mut events_rx).await
    else {
        panic!("expected cache-clear watched-file registration")
    };
    assert_ne!(cleared_registration.registrations[0].id, published_id);
    assert!(matches!(
        next_watched_file_client_event(&mut events_rx).await,
        WatchedFileClientEvent::Unregister(params)
            if params.unregisterations[0].id == published_id
    ));
    assert!(!watched_file_registration_has_spec(
        &cleared_registration,
        &dependency_parent,
        "**/*.sol"
    ));

    server_socket.notify::<notification::Exit>(()).unwrap();
    assert!(server_task.await.unwrap().is_ok());
    assert!(matches!(client_task.await.unwrap(), Err(async_lsp::Error::Eof)));
}

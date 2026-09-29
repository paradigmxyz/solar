use super::{
    indexing::{
        analysis_version, analyze_project, cancel_analysis, settle, state_with, symbol_names, watch,
    },
    *,
};
use lsp_types::{CreateFilesParams, DeleteFilesParams, FileCreate, FileDelete};

fn create_files(state: &mut GlobalState, path: &Path) {
    let files = vec![FileCreate { uri: Url::from_file_path(path).unwrap().to_string() }];
    assert!(crate::handlers::did_create_files(state, CreateFilesParams { files }).is_continue());
}

fn deferred_event(state: &GlobalState, path: &Path) -> Option<FileChangeType> {
    state.analysis_commit.lock().deferred_source_file_events.get(path).copied()
}

fn discovery_ready(
    version: usize,
    result: WorkspaceDiscoveryResult,
    progress: ProgressTicket,
) -> WorkspaceDiscoveryReady {
    WorkspaceDiscoveryReady {
        version,
        result,
        disk_paths: Vec::new(),
        progress,
        cancellation: IndexingCancellation::default(),
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_unrelated_excluded_sources_and_manifests_do_not_schedule_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        contract Main {}

        //- /generated/Unrelated.sol
        contract Unrelated {}

        //- /node_modules/package/foundry.toml
        [profile.default]
        src = "src"
        "#,
    );
    let unrelated = project.path("/generated/Unrelated.sol");
    let manifest = project.path("/node_modules/package/foundry.toml");
    let excluded = || config_with_indexing_excludes(&project, &["generated/**"]);
    let cases = [
        (excluded(), unrelated.as_path(), FileChangeType::CHANGED),
        (project.config(), &manifest, FileChangeType::CREATED),
        (project.config(), &manifest, FileChangeType::CHANGED),
        (project.config(), &manifest, FileChangeType::DELETED),
    ];
    for (config, path, typ) in cases {
        let mut state = state_with(config);
        let version = analysis_version(&state);

        watch(&mut state, &[(path, typ)]);

        assert_eq!(analysis_version(&state), version);
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
    let mut state =
        state_with(config_with_indexing_excludes(&project, &["packages/app/generated/**"]));
    project
        .write_file("/packages/app/generated/foundry.toml", "[profile.default]\nsrc = \"src\"\n");
    project.write_file("/packages/app/generated/src/Nested.sol", "contract Nested {}");

    watch(
        &mut state,
        &[(&project.path("/packages/app/generated/foundry.toml"), FileChangeType::CREATED)],
    );

    assert_eq!(symbol_names(&settle(&state).await, "Nested"), ["Nested"]);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_nested_manifest_create_under_external_foundry_roots_discovers_projects() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "../shared/contracts"
        test = "../shared/checks"

        //- /shared/.keep
        "#,
    );
    let roots =
        ["/shared/contracts/deep/app", "/shared/checks/deep/app"].map(|root| project.path(root));
    let has_workspace = |config: &Config, root: &Path| {
        config
            .workspaces()
            .iter()
            .any(|workspace| workspace.compile_opts().base_path.as_deref() == Some(root))
    };
    let mut state = state_with(project.config());
    assert!(roots.iter().all(|root| !has_workspace(&state.config, root)));
    for (root, source) in
        [("/shared/contracts/deep/app", "Source"), ("/shared/checks/deep/app", "Check")]
    {
        project.write_file(&format!("{root}/foundry.toml"), "[profile.default]\nsrc = \"src\"\n");
        project.write_file(&format!("{root}/src/{source}.sol"), &format!("contract {source} {{}}"));
    }

    let manifests = roots.clone().map(|root| root.join("foundry.toml"));
    watch(
        &mut state,
        &[(&manifests[0], FileChangeType::CREATED), (&manifests[1], FileChangeType::CREATED)],
    );
    settle(&state).await;

    assert!(roots.iter().all(|root| has_workspace(&state.config, root)));
}

#[tokio::test(flavor = "current_thread")]
async fn watched_nested_repository_markers_prune_and_restore_nested_projects() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol
        contract Main {}

        //- /src/nested/Nested.sol
        contract Nested {}
        "#,
    );
    let nested_root = project.path("/src/nested");
    let nested_source = project.path("/src/nested/Nested.sol");
    let marker = project.path("/src/nested/.git");
    let tracked = |state: &GlobalState| {
        state.config.tracked_source_files_under(std::slice::from_ref(&nested_root))
    };
    let mut state = state_with(project.config());
    assert_eq!(tracked(&state), std::slice::from_ref(&nested_source));

    project.write_file("/src/nested/.git", "gitdir: elsewhere");
    watch(&mut state, &[(&marker, FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 1);
    assert!(tracked(&state).is_empty());

    std::fs::remove_file(&marker).unwrap();
    watch(&mut state, &[(&marker, FileChangeType::DELETED)]);
    assert_eq!(analysis_version(&state), 2);
    assert_eq!(tracked(&state), std::slice::from_ref(&nested_source));
    cancel_analysis(&state);

    // The marker events are ignored when nested repositories stay indexed.
    let mut params = project.initialize_params();
    params.initialization_options =
        Some(serde_json::json!({ "indexing": { "excludeNestedRepositories": false } }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    assert!(
        config
            .watched_file_specs()
            .iter()
            .all(|spec| spec.pattern != "**/.git" && spec.pattern != ".git")
    );
    project.write_file("/src/nested/.git", "gitdir: elsewhere");
    let mut state = state_with(config);
    watch(&mut state, &[(&marker, FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 0);
    assert_eq!(tracked(&state), [nested_source]);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_created_directories_under_shallow_roots_discover_projects_and_sources() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "contracts"
        test = "test"

        //- /contracts/Main.sol
        contract Main {}

        //- /node_modules/dependency/foundry.toml

        //- /test/node_modules/dependency/Skipped.t.sol
        contract Skipped {}
        "#,
    );
    let mut state = state_with(project.config());
    let nested_source = project.path("/packages/app/src/Nested.sol");
    let generated = project.path("/test/generated/Generated.t.sol");
    project.write_file("/packages/app/foundry.toml", "[profile.default]\nsrc = \"src\"\n");
    project.write_file("/packages/app/src/Nested.sol", "contract Nested {}");
    project.write_file("/test/generated/Generated.t.sol", "contract GeneratedTest {}");

    watch(&mut state, &[(&project.path("/packages"), FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 1);
    assert_eq!(
        state.config.tracked_source_files_under(&[project.path("/packages")]),
        [nested_source]
    );

    watch(&mut state, &[(&project.path("/test/generated"), FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 2);
    assert!(
        state
            .config
            .workspaces()
            .iter()
            .any(|workspace| workspace.flycheck_source_files().contains(&generated))
    );
    cancel_analysis(&state);
}

#[tokio::test(flavor = "current_thread")]
async fn created_directories_schedule_analysis_only_below_indexed_roots() {
    let overlapping =
        TestProject::from_fixture("//- /foundry.toml\n[profile.default]\nsrc = \"lib\"\n");
    let excluded = TestProject::from_fixture("//- /Main.sol\ncontract Main {}\n");
    for (project, directory, scheduled) in [
        (overlapping, "/lib/generated", true),
        (excluded, "/node_modules/package/generated", false),
    ] {
        let path = project.path(directory);
        std::fs::create_dir_all(&path).unwrap();
        let mut state = state_with(project.config());
        let version = analysis_version(&state);

        create_files(&mut state, &path);

        let actual_version = analysis_version(&state);
        cancel_analysis(&state);
        assert_eq!(actual_version, version + usize::from(scheduled));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_created_source_under_overlapping_root_is_tracked() {
    let project =
        TestProject::from_fixture("//- /foundry.toml\n[profile.default]\nsrc = \"lib\"\n");
    let mut state = state_with(project.config());
    let path = project.path("/lib/Created.sol");
    project.write_file("/lib/Created.sol", "contract Created {}");

    watch(&mut state, &[(&path, FileChangeType::CREATED)]);

    assert_eq!(analysis_version(&state), 1);
    assert_eq!(state.config.tracked_source_files_under(&[project.path("/lib")]), [path]);
    cancel_analysis(&state);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_directory_topology_under_partitioned_root_is_rediscovered() {
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
    let mut state = state_with(project.config());
    let root = [project.root().to_path_buf()];
    project.write_file("/README.md", "notes");
    project.write_file("/new/New.sol", "contract New {}");

    watch(&mut state, &[(&project.path("/README.md"), FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 0);

    watch(&mut state, &[(&project.path("/new"), FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 1);
    assert_eq!(
        state.config.tracked_source_files_under(&root),
        [project.path("/new/New.sol"), project.path("/old/Old.sol")]
    );

    std::fs::remove_dir_all(project.path("/old")).unwrap();
    watch(&mut state, &[(&project.path("/old"), FileChangeType::DELETED)]);
    assert_eq!(analysis_version(&state), 2);
    assert_eq!(state.config.tracked_source_files_under(&root), [project.path("/new/New.sol")]);
    cancel_analysis(&state);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_dependency_and_unresolved_candidate_changes_schedule_analysis() {
    let excluded_dependency = r#"
        //- /Main.sol
        import "./generated/Dependency.sol";
        contract Main is Dependency {}

        //- /generated/Dependency.sol
        contract Dependency {}
        "#;
    let unresolved_candidate = r#"
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
        "#;
    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        for (fixture, excludes, path) in [
            (excluded_dependency, &["generated/**"][..], "/generated/Dependency.sol"),
            (unresolved_candidate, &[], "/lib-one/Dependency.sol"),
        ] {
            let project = TestProject::from_fixture(fixture);
            let config = config_with_indexing_excludes(&project, excludes);
            let output = analyze_project(&project, &config);
            if typ == FileChangeType::CHANGED {
                project.write_file(path, "contract Dependency { uint x; }");
            } else {
                project.remove_file(path);
            }
            let mut state = state_with(config);
            state.snapshot().publish_analysis_output(0, output.into_shared());

            watch(&mut state, &[(&project.path(path), typ)]);

            let actual_version = analysis_version(&state);
            cancel_analysis(&state);
            assert_eq!(actual_version, 1);
        }
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_external_source_create_change_and_delete_schedule_analysis() {
    for typ in [FileChangeType::CREATED, FileChangeType::CHANGED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /project/foundry.toml
            [profile.default]
            src = "../shared/contracts"
            "#,
        );
        let path = project.path("/shared/contracts/External.sol");
        if typ != FileChangeType::CREATED {
            project.write_file("/shared/contracts/External.sol", "contract External {}");
        }
        let mut state = state_with(project.config_with_roots(&["/project", "/shared"]));
        if typ == FileChangeType::CREATED {
            project.write_file("/shared/contracts/External.sol", "contract External {}");
        } else if typ == FileChangeType::DELETED {
            std::fs::remove_file(&path).unwrap();
        }

        watch(&mut state, &[(&path, typ)]);

        assert_eq!(analysis_version(&state), 1);
        let tracked = state.config.tracked_source_files_under(&[project.path("/shared")]);
        if typ == FileChangeType::DELETED {
            assert!(tracked.is_empty());
        } else {
            assert_eq!(tracked, [path]);
        }
        cancel_analysis(&state);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn watched_flycheck_only_source_change_schedules_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        test = "test"

        //- /src/Main.sol
        contract Main {}

        //- /test/Main.t.sol
        contract MainTest {}
        "#,
    );
    let path = project.path("/test/Main.t.sol");
    let (_, mut config) = crate::config::negotiate_capabilities_with_pull_diagnostic_data(
        project.initialize_params(),
        false,
        &crate::LaunchConfig::default().with_foundry_workspace_configs([
            crate::FoundryWorkspaceConfig::new(project.root())
                .with_source_roots(["src"])
                .with_flycheck_source_roots(["src", "test"]),
        ]),
    );
    config.rediscover_workspaces();
    assert!(!config.tracks_source_file(&path));
    assert!(config.tracks_flycheck_file(&path));
    let mut state = state_with(config);

    watch(&mut state, &[(&path, FileChangeType::CHANGED)]);

    assert_eq!(analysis_version(&state), 1);
    cancel_analysis(&state);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_source_respects_the_most_specific_flycheck_owner() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        test = "."

        //- /src/Main.sol
        contract Main {}

        //- /packages/app/foundry.toml
        [profile.default]
        src = "src"
        test = "test"
        script = "script"
        "#,
    );
    let config = project.config();
    let path = project.path("/packages/app/Outside.sol");
    project.write_file("/packages/app/Outside.sol", "contract Outside {}");
    assert!(!config.tracks_flycheck_file(&path));
    let mut state = state_with(config);

    watch(&mut state, &[(&path, FileChangeType::CREATED)]);

    assert_eq!(analysis_version(&state), 1);
    assert!(
        state
            .config
            .workspaces()
            .iter()
            .all(|workspace| !workspace.flycheck_source_files().contains(&path))
    );
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
    let mut state = state_with(config_with_indexing_excludes(&project, &["generated/**"]));
    state.mark_analysis_pending_for_test();
    let path = project.path("/generated/Dependency.sol");

    watch(&mut state, &[(&path, FileChangeType::CHANGED)]);

    assert_eq!(analysis_version(&state), 1);
    assert_eq!(deferred_event(&state, &path), Some(FileChangeType::CHANGED));
}

#[test]
fn did_create_and_delete_defer_a_path_first_learned_by_pending_analysis() {
    for typ in [FileChangeType::CREATED, FileChangeType::DELETED] {
        let project = TestProject::from_fixture(
            r#"
            //- /Main.sol
            import "./generated/Dependency.sol";
            contract Main is Dependency {}

            //- /generated/Dependency.sol
            contract Dependency {}
            "#,
        );
        let path = project.path("/generated/Dependency.sol");
        if typ == FileChangeType::CREATED {
            project.remove_file("/generated/Dependency.sol");
        }
        let config = config_with_indexing_excludes(&project, &["generated/**"]);
        let output = analyze_project(&project, &config);
        let mut state = state_with(config);
        state.mark_analysis_pending_for_test();
        let version = analysis_version(&state);

        if typ == FileChangeType::CREATED {
            project.write_file("/generated/Dependency.sol", "contract Dependency {}");
            create_files(&mut state, &path);
        } else {
            std::fs::remove_file(&path).unwrap();
            let files = vec![FileDelete { uri: Url::from_file_path(&path).unwrap().to_string() }];
            let params = DeleteFilesParams { files };
            assert!(crate::handlers::did_delete_files(&mut state, params).is_continue());
        }

        assert_eq!(analysis_version(&state), version);
        assert_eq!(deferred_event(&state, &path), Some(typ));
        assert!(!state.snapshot().publish_analysis_output(version, output.into_shared()));
    }
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
    let mut state = state_with(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let discovery = state.config.discover_workspaces(&IndexingCancellation::default()).unwrap();

    project.write_file("/project/lib/Active.sol", "contract Active {}");
    project.write_file("/project/node_modules/Ignored.sol", "contract Ignored {}");
    let active = project.path("/project/lib/Active.sol");
    let ignored = project.path("/project/node_modules/Ignored.sol");
    watch(&mut state, &[(&active, FileChangeType::CREATED), (&ignored, FileChangeType::CREATED)]);

    assert_eq!(analysis_version(&state), version);
    let ready = discovery_ready(version, discovery, progress);
    assert!(state.on_workspace_discovery_ready(ready).is_continue());

    let tables = settle(&state).await;
    assert_eq!(symbol_names(&tables, "Active"), ["Active"]);
    assert!(symbol_names(&tables, "Ignored").is_empty());
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
    let mut config = project.config_with_roots(&["/existing"]);
    assert!(!config.workspaces().is_empty());
    config.add_workspaces([new_root.clone()]);
    let mut state = state_with(config);
    let (version, progress) = state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap();
    let discovery = state.config.discover_workspaces(&IndexingCancellation::default()).unwrap();
    let path = project.path("/new/Active.sol");
    project.write_file("/new/Active.sol", "contract Active {}");

    watch(&mut state, &[(&path, FileChangeType::CREATED), (&path, FileChangeType::CHANGED)]);
    assert_eq!(analysis_version(&state), version);
    assert_eq!(deferred_event(&state, &path), Some(FileChangeType::CHANGED));

    let ready = discovery_ready(version, discovery, progress);
    assert!(state.on_workspace_discovery_ready(ready).is_continue());
    assert_eq!(
        state.config.tracked_source_files_under(std::slice::from_ref(&new_root)),
        std::slice::from_ref(&path)
    );
    assert_eq!(symbol_names(&settle(&state).await, "Active"), ["Active"]);

    state.recompute_after_source_changes(vec![project.path("/existing/Existing.sol")]);
    assert_eq!(symbol_names(&settle(&state).await, "Active"), ["Active"]);
}

#[tokio::test(flavor = "current_thread")]
async fn watched_missing_excluded_dependency_recovers_on_create_and_later_changes() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/Missing.sol";
        contract Main is Missing {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let output = analyze_project(&project, &config);
    let mut state = state_with(config);
    state.snapshot().publish_analysis_output(0, output.into_shared());
    let path = project.path("/generated/Missing.sol");

    for typ in [FileChangeType::CHANGED, FileChangeType::DELETED] {
        watch(&mut state, &[(&path, typ)]);
        assert_eq!(analysis_version(&state), 0);
    }

    project.write_file("/generated/Missing.sol", "contract Missing {}");
    watch(&mut state, &[(&path, FileChangeType::CREATED)]);
    assert_eq!(analysis_version(&state), 1);

    // A change to the created candidate supersedes the pending create analysis.
    project.write_file("/generated/Missing.sol", "contract Missing { uint latest; }");
    watch(&mut state, &[(&path, FileChangeType::CHANGED)]);
    assert_eq!(analysis_version(&state), 2);

    let tables = settle(&state).await;
    assert_eq!(symbol_names(&tables, "Missing"), ["Missing"]);
    assert_eq!(symbol_names(&tables, "latest"), ["latest"]);
}

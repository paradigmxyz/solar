use super::{
    indexing::{change_workspace_folders, path_output, resolved_paths},
    *,
};
use lsp_types::{RegistrationParams, UnregistrationParams};

#[derive(Debug)]
enum WatchedFileClientEvent {
    Register(RegistrationParams),
    Unregister(UnregistrationParams),
}

/// Scripts client responses to watched-file registration requests by attempt index.
#[derive(Default)]
struct ClientScript {
    fail_register: Option<usize>,
    fail_unregister: Option<usize>,
    delay_register: Option<(usize, oneshot::Receiver<()>)>,
}

struct RegistrationHarness {
    pair: LspPair,
    coordinator: Arc<WatchedFileRegistrationCoordinator>,
    config: Config,
    events: mpsc::UnboundedReceiver<WatchedFileClientEvent>,
}

impl RegistrationHarness {
    fn new(config: Config, script: ClientScript) -> Self {
        let (events_tx, events) = mpsc::unbounded_channel();
        let server = |_| {
            let mut router = exit_router(());
            router.event::<WatchedFileRegistrationReady>(|_, _| ControlFlow::Continue(()));
            router
        };
        let pair = LspPair::spawn(server, move |_| {
            let failed = || ResponseError::new(ErrorCode::REQUEST_FAILED, "scripted failure");
            let mut router = Router::new((events_tx, script, 0, 0));
            router.request::<request::RegisterCapability, _>(
                move |(events, script, attempts, _), params| {
                    events.send(WatchedFileClientEvent::Register(params)).unwrap();
                    let fail = script.fail_register == Some(*attempts);
                    let delay = script.delay_register.take_if(|(attempt, _)| attempt == attempts);
                    *attempts += 1;
                    async move {
                        if let Some((_, ack)) = delay {
                            ack.await.map_err(|_| failed())?;
                        }
                        if fail { Err(failed()) } else { Ok(()) }
                    }
                },
            );
            router.request::<request::UnregisterCapability, _>(
                move |(events, script, _, attempts), params| {
                    events.send(WatchedFileClientEvent::Unregister(params)).unwrap();
                    let fail = script.fail_unregister == Some(*attempts);
                    *attempts += 1;
                    async move { if fail { Err(failed()) } else { Ok(()) } }
                },
            );
            router
        });
        Self { pair, coordinator: Arc::default(), config, events }
    }

    /// Sends a prepared registration update.
    fn send(&self, update: Option<WatchedFileRegistrationUpdate>) {
        spawn_watched_file_registration_update(&self.pair.client, &self.coordinator, update);
    }

    /// Prepares and sends a registration update, returning the ID the client received.
    async fn register(&mut self, specs: Vec<WatchedFileSpec>) -> (String, RegistrationParams) {
        self.send(prepare_watched_file_registration_update(&self.config, &self.coordinator, specs));
        self.next_registration().await
    }

    async fn next_event(&mut self) -> WatchedFileClientEvent {
        within("watched-file registration", self.events.recv()).await.unwrap()
    }

    async fn next_registration(&mut self) -> (String, RegistrationParams) {
        let WatchedFileClientEvent::Register(params) = self.next_event().await else {
            panic!("expected watched-file registration")
        };
        (params.registrations[0].id.clone(), params)
    }

    async fn expect_unregistration(&mut self, id: &str) {
        let WatchedFileClientEvent::Unregister(params) = self.next_event().await else {
            panic!("expected watched-file unregistration")
        };
        assert_eq!(params.unregisterations[0].id, id);
    }

    async fn wait_for_active(&self, ids: &[&str]) {
        let deadline = Instant::now() + TIMEOUT;
        while *self.coordinator.active_registration_ids.lock() != ids && Instant::now() < deadline {
            tokio::task::yield_now().await;
        }
        assert_eq!(*self.coordinator.active_registration_ids.lock(), ids);
    }
}

async fn wait_until_idle(coordinator: &WatchedFileRegistrationCoordinator) {
    let deadline = Instant::now() + TIMEOUT;
    while coordinator.desired_specs.lock().is_some() && Instant::now() < deadline {
        tokio::task::yield_now().await;
    }
    assert!(coordinator.desired_specs.lock().is_none());
}

fn relative_watch_config(project: &TestProject, roots: &[&str], excludes: &[&str]) -> Config {
    let mut params = with_relative_watchers(project.initialize_params_with_roots(roots));
    if !excludes.is_empty() {
        params = with_options(params, json!({ "indexing": { "exclude": excludes } }));
    }
    negotiate_capabilities(params).1
}

fn discovered_registration(
    project: &TestProject,
    roots: &[&str],
    excludes: &[&str],
) -> RegistrationParams {
    let mut config = relative_watch_config(project, roots, excludes);
    config.rediscover_workspaces();
    watched_file_registration_params(&config)
}

fn watchers(params: &RegistrationParams) -> impl Iterator<Item = &serde_json::Value> {
    params.registrations.iter().flat_map(|registration| {
        let options = registration.register_options.as_ref();
        options.and_then(|options| options["watchers"].as_array()).into_iter().flatten()
    })
}

/// Returns the watch kind of the relative watcher for `pattern` below `base`, if any.
fn spec_kind(params: &RegistrationParams, base: &Path, pattern: &str) -> Option<u64> {
    let base_uri = Url::from_file_path(base).unwrap().to_string();
    watchers(params).find_map(|watcher| {
        (watcher["globPattern"]["baseUri"].as_str() == Some(&base_uri)
            && watcher["globPattern"]["pattern"].as_str() == Some(pattern))
        .then(|| watcher["kind"].as_u64().unwrap())
    })
}

fn has_spec(params: &RegistrationParams, base: &Path, pattern: &str) -> bool {
    spec_kind(params, base, pattern).is_some()
}

fn has_recursive_spec_covering(params: &RegistrationParams, path: &Path) -> bool {
    watchers(params).any(|watcher| {
        matches!(watcher["globPattern"]["pattern"].as_str(), Some("**/*.sol" | "**/foundry.toml"))
            && watcher["globPattern"]["baseUri"]
                .as_str()
                .and_then(|uri| Url::parse(uri).ok())
                .and_then(|uri| uri.to_file_path().ok())
                .is_some_and(|base| path.starts_with(base))
    })
}

fn has_desired_spec(specs: &[WatchedFileSpec], base: &Path, pattern: &str) -> bool {
    specs.iter().any(|spec| spec.base == base && spec.pattern == pattern)
}

fn sol_spec(project: &TestProject, root: &str) -> Vec<WatchedFileSpec> {
    vec![WatchedFileSpec::new(project.path(root), "**/*.sol")]
}

fn workspace_project() -> TestProject {
    let project = TestProject::new();
    std::fs::create_dir(project.path("/workspace")).unwrap();
    project
}

/// A `/workspace` project and its undiscovered config with relative watchers.
fn workspace_watch_config() -> (TestProject, Config) {
    let project = workspace_project();
    let config = relative_watch_config(&project, &["/workspace"], &[]);
    (project, config)
}

const CREATE_DELETE: u64 = 5;

#[tokio::test(flavor = "current_thread")]
async fn watched_file_specs_are_prepared_after_the_analysis_commit_unlocks() {
    let (project, config) = workspace_watch_config();
    let state = state_with(config);
    state.mark_analysis_pending_for_test();
    let version = analysis_version(&state);
    let mut snapshot = state.snapshot();
    let output = path_output(resolved_paths([project.path("/workspace/deps/Dependency.sol")]));
    let desired_specs = state.watched_file_registration.desired_specs.lock();
    let runtime = tokio::runtime::Handle::current();

    std::thread::scope(|scope| {
        let publisher = scope.spawn(move || {
            let _runtime = runtime.enter();
            snapshot.publish_analysis_output(version, output.into_shared())
        });

        let deadline = Instant::now() + TIMEOUT;
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
async fn workspace_folder_changes_normalize_equivalent_uris() {
    let project = TestProject::from_fixture(
        r#"
        //- /old/Old.sol
        contract Old {}
        //- /new/New.sol
        contract New {}
        "#,
    );
    let mut state =
        state_with(negotiate_capabilities(project.initialize_params_with_roots(&["/old"])).1);
    let root = Url::from_file_path(project.root()).unwrap();
    let equivalent = |name| Url::parse(&format!("{root}/missing%2F..%2F{name}")).unwrap();

    change_workspace_folders(&mut state, &[equivalent("new")], &[equivalent("old")]);

    assert_eq!(state.config.workspace_roots(), [project.path("/new")]);
    let tables = settle(&state).await;
    let tables = tables.load();
    assert!(tables.workspace_symbols("Old").is_empty());
    let new_uri = project.uri("/new/New.sol");
    assert_eq!(tables.document_symbols(&new_uri)[0].name, "New");
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_folder_change_advances_epoch_before_watcher_reregistration() {
    let project = TestProject::new();
    let [old_root, new_root] = ["/old", "/new"].map(|root| project.uri(root));
    for root in ["/old", "/new"] {
        std::fs::create_dir(project.path(root)).unwrap();
    }
    let state = state_with(relative_watch_config(&project, &["/old"], &[]));
    assert_epoch_advances_before_reregistration(state, move |state| {
        change_workspace_folders(state, &[new_root], &[old_root]);
    });
}

#[test]
fn watched_file_registration_has_global_fallback_patterns() {
    // Without workspace roots, and for an empty workspace root.
    for config in [Config::default(), TestProject::new().config()] {
        let [registration] =
            watched_file_registration_params(&config).registrations.try_into().unwrap();
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
}

#[test]
fn relative_watched_file_registration_tracks_nested_repository_markers() {
    let workspace = "//- /workspace/foundry.toml\n[profile.default]\nsrc = \"contracts\"\n\
                     //- /workspace/contracts/Main.sol\n";
    let clean = TestProject::from_fixture(&format!("{workspace}//- /workspace/.git/HEAD\n"));
    let registration = discovered_registration(&clean, &["/workspace"], &[]);
    let source_root = clean.path("/workspace/contracts");
    assert_eq!(spec_kind(&registration, &source_root, "**/.git"), Some(CREATE_DELETE));
    assert!(!has_spec(&registration, &clean.path("/workspace"), ".git"));

    let pruned = TestProject::from_fixture(&format!(
        "{workspace}//- /workspace/contracts/nested/.git\ngitdir: elsewhere\n\
         //- /workspace/contracts/nested/Nested.sol\n"
    ));
    let registration = discovered_registration(&pruned, &["/workspace"], &[]);
    let marker_root = pruned.path("/workspace/contracts/nested");
    assert_eq!(spec_kind(&registration, &marker_root, ".git"), Some(CREATE_DELETE));

    // Markers below a parent manifest approved for a member workspace are watched too.
    let parent = TestProject::from_fixture(
        r#"
        //- /repo/foundry.toml
        [profile.default]
        src = "src"

        //- /repo/src/Main.sol
        //- /repo/src/vendor/.git
        gitdir: elsewhere

        //- /repo/src/vendor/Vendor.sol
        //- /repo/member/.keep
        "#,
    );
    let registration = discovered_registration(&parent, &["/repo/member"], &[]);
    let marker_root = parent.path("/repo/src/vendor");
    assert_eq!(spec_kind(&registration, &marker_root, ".git"), Some(CREATE_DELETE));
}

#[test]
fn relative_watched_file_registration_uses_bounded_roots() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "contracts"

        //- /workspace/contracts/Main.sol
        //- /workspace/node_modules/Dependency.sol
        //- /workspace/out/Generated.sol
        //- /workspace/.hidden/Hidden.sol
        //- /workspace/nested/.git/HEAD
        //- /workspace/nested/Nested.sol
        "#,
    );
    let workspace_root = project.path("/workspace");
    let source_root = project.path("/workspace/contracts");
    let mut config = relative_watch_config(&project, &["/workspace"], &[]);
    let assert_bounded = |registration: &RegistrationParams| {
        assert!(has_spec(registration, &workspace_root, "foundry.toml"));
        assert!(has_spec(registration, &workspace_root, "remappings.txt"));
        for pattern in ["**/*.sol", "**/foundry.toml", "**/remappings.txt"] {
            assert!(!has_spec(registration, &workspace_root, pattern));
        }
    };

    assert_bounded(&watched_file_registration_params(&config));

    config.rediscover_workspaces();
    let discovered = watched_file_registration_params(&config);
    assert_bounded(&discovered);
    assert!(has_spec(&discovered, &source_root, "**/*.sol"));
    assert!(has_spec(&discovered, &source_root, "**/foundry.toml"));
}

#[test]
fn relative_watched_file_registration_partitions_root_sources() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry/foundry.toml
        [profile.default]
        src = "."

        //- /foundry/Root.sol
        //- /foundry/contracts/Main.sol
        //- /foundry/contracts/core/Core.sol
        //- /foundry/contracts/node_modules/Dependency.sol
        //- /foundry/contracts/out/Generated.sol
        //- /foundry/contracts/.hidden/Hidden.sol
        //- /foundry/contracts/vendor/.git/HEAD
        //- /foundry/contracts/vendor/Nested.sol
        //- /foundry/lib/Dependency.sol
        //- /foundry/out/Generated.sol
        //- /foundry/.hidden/Hidden.sol
        //- /foundry/nested/.git/HEAD
        //- /foundry/nested/Nested.sol
        //- /naked/Root.sol
        //- /naked/contracts/Main.sol
        //- /naked/node_modules/Dependency.sol
        "#,
    );
    let foundry = project.path("/foundry");
    let naked = project.path("/naked");
    let registration = discovered_registration(&project, &["/foundry", "/naked"], &[]);

    for root in [&foundry, &naked] {
        assert_eq!(spec_kind(&registration, root, "*"), Some(CREATE_DELETE));
        assert_eq!(spec_kind(&registration, root, "*.sol"), Some(2));
        assert!(!has_spec(&registration, root, "**/*.sol"));
    }
    for (base, pattern) in [
        ("contracts", "*"),
        ("contracts", "*.sol"),
        ("contracts", "foundry.toml"),
        ("contracts/core", "**/*.sol"),
        ("contracts/core", "**/foundry.toml"),
    ] {
        assert!(has_spec(&registration, &foundry.join(base), pattern));
    }
    assert!(has_spec(&registration, &naked.join("contracts"), "**/*.sol"));
    for excluded in [
        "lib",
        "out",
        ".hidden",
        "nested",
        "contracts/node_modules",
        "contracts/out",
        "contracts/.hidden",
        "contracts/vendor",
    ] {
        assert!(!has_recursive_spec_covering(&registration, &foundry.join(excluded)));
    }
    assert!(!has_recursive_spec_covering(&registration, &naked.join("node_modules")));
}

#[test]
fn nested_workspace_policy_owns_discovered_sources_batches_and_watchers() {
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
    let mut config = relative_watch_config(&project, &["/"], &["src/generated/**"]);
    config.rediscover_workspaces();
    let nested_root = project.path("/nested");
    assert!(workspace_at(&config, project.root()).source_files().is_empty());
    assert_eq!(
        workspace_at(&config, &nested_root).source_files(),
        [project.path("/nested/Outside.sol"), project.path("/nested/src/Included.sol")]
    );
    assert_eq!(config.index_metrics().eager, 2);

    let registration = watched_file_registration_params(&config);
    let nested_source_root = project.path("/nested/src");
    for pattern in ["*", "*.sol", "foundry.toml"] {
        assert!(has_spec(&registration, &nested_source_root, pattern));
    }
    assert!(has_spec(&registration, &nested_root, "*.sol"));
    for excluded in ["/nested/Outside.sol", "/nested/src/generated", "/nested/src/vendor"] {
        assert!(!has_recursive_spec_covering(&registration, &project.path(excluded)));
    }

    let batches = snapshot_with_config(config, Vfs::default()).analysis_batches(Vec::new());
    let batch_at = |root: &Path| {
        batches.iter().find(|batch| batch.opts.base_path.as_deref() == Some(root)).unwrap()
    };
    assert!(batch_at(project.root()).files.iter().all(|(path, _)| !path.starts_with(&nested_root)));
    assert_eq!(
        batch_at(&nested_root).files,
        vec![
            (project.path("/nested/Outside.sol"), Arc::new("contract Outside {}".into())),
            (project.path("/nested/src/Included.sol"), Arc::new("contract Included {}".into()))
        ]
    );
}

#[test]
fn relative_watched_file_registration_omits_excluded_source_root() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "contracts"

        //- /contracts/Main.sol
        "#,
    );
    let source_root = project.path("/contracts");
    let registration = discovered_registration(&project, &["/"], &["contracts/**"]);

    for pattern in ["*.sol", "**/*.sol"] {
        assert!(!has_spec(&registration, &source_root, pattern));
    }
    assert!(!has_recursive_spec_covering(&registration, &source_root));
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
        //- /include/pkg/Include.sol
        //- /mapped/pkg/Mapped.sol
        "#,
    );
    let config = project.config_with_roots(&["/workspace"]);
    let workspace_parent = project.path("/workspace/deps");
    let include_root = project.path("/include");
    let include_parent = project.path("/include/pkg");
    let remapping_parent = project.path("/mapped/pkg");
    let outside_parent = project.path("/outside");
    let missing_parent = project.path("/include/missing");
    let missing_nested_parent = project.path("/include/pkg/src");
    std::fs::create_dir_all(&missing_parent).unwrap();
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
        missing_candidates: FxHashSet::from_iter([
            missing_parent.join("Missing.sol"),
            missing_nested_parent.join("Missing.sol"),
        ]),
    };

    let specs = watched_file_specs(&config, &analysis_paths);

    let count = |base: &Path, pattern: &str| {
        specs.iter().filter(|spec| spec.base == base && spec.pattern == pattern).count()
    };
    for parent in [&workspace_parent, &include_parent, &remapping_parent, &missing_parent] {
        assert_eq!(count(parent, "*.sol"), 1);
    }
    assert!(!specs.iter().any(|spec| spec.base == outside_parent));
    assert_eq!(count(&include_parent, "*"), 1);
    assert_eq!(count(&include_root, "*"), 1);
}

#[test]
fn watched_file_specs_use_indexed_recursive_coverage() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "src"

        //- /workspace/src/Main.sol
        "#,
    );
    let config = project.config_with_roots(&["/workspace"]);
    let dependency_parent = project.path("/workspace/src/nested");
    let analysis_paths = resolved_paths([dependency_parent.join("Dependency.sol")]);

    let specs = watched_file_specs(&config, &analysis_paths);

    assert!(has_desired_spec(&specs, &project.path("/workspace/src"), "**/*.sol"));
    assert!(!has_desired_spec(&specs, &dependency_parent, "*.sol"));
}

#[test]
fn watched_file_specs_cap_dynamic_dependency_parents() {
    let project = workspace_project();
    let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&["/workspace"]));
    let dependency_root = project.path("/workspace/deps");
    let analysis_paths = resolved_paths(
        (0..MAX_DYNAMIC_WATCHED_FILE_SPECS + 32)
            .map(|index| dependency_root.join(index.to_string()).join("Dependency.sol")),
    );

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
fn watched_file_specs_prioritize_specific_dependency_parents() {
    let project = workspace_project();
    let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&["/workspace"]));
    let fallback_root = project.path("/workspace/missing");
    std::fs::create_dir(&fallback_root).unwrap();
    let mut missing_candidates = FxHashSet::default();
    for index in 0..MAX_DYNAMIC_WATCHED_FILE_SPECS + 32 {
        let existing_parent = fallback_root.join(index.to_string());
        std::fs::create_dir(&existing_parent).unwrap();
        missing_candidates.insert(existing_parent.join("nested/Missing.sol"));
    }
    let resolved_parent = project.path("/workspace/resolved");
    let analysis_paths = AnalysisPathIndex {
        missing_candidates,
        ..resolved_paths([resolved_parent.join("Dependency.sol")])
    };

    let specs = watched_file_specs(&config, &analysis_paths);

    assert!(
        has_desired_spec(&specs, &resolved_parent, "*.sol"),
        "fallback recovery watchers displaced a specific dependency watcher"
    );
    assert!(
        specs
            .iter()
            .filter(|spec| {
                spec.pattern == "*" || spec.base == resolved_parent && spec.pattern == "*.sol"
            })
            .count()
            <= MAX_DYNAMIC_WATCHED_FILE_SPECS
    );
}

#[test]
fn concurrent_watched_file_updates_keep_desired_specs_and_generation_in_sync() {
    let (project, config) = workspace_watch_config();
    let config = Arc::new(config);
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let barrier = Arc::new(Barrier::new(3));
    let specs = [sol_spec(&project, "/first"), sol_spec(&project, "/second")];

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
    let watched_files = json!({ "dynamicRegistration": true, "relativePatternSupport": false });
    let capabilities = json!({ "workspace": { "didChangeWatchedFiles": watched_files } });
    let (_, config) =
        negotiate_capabilities(with_capabilities(project.initialize_params(), capabilities));
    let coordinator = WatchedFileRegistrationCoordinator::default();
    let prepare = |root| {
        prepare_watched_file_registration_update(&config, &coordinator, sol_spec(&project, root))
    };
    let first = prepare("/first").unwrap();

    assert!(first.desired_specs.is_empty());
    assert!(prepare("/second").is_none());
    assert_eq!(coordinator.generation.load(Ordering::Acquire), first.generation);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_watched_file_registration_allows_the_same_specs_to_retry() {
    let (project, config) = workspace_watch_config();
    let coordinator = Arc::new(WatchedFileRegistrationCoordinator::default());
    let client = ClientSocket::new_closed();
    let specs = config.watched_file_specs();
    // The superseded update exits without touching the latest desired specs.
    let stale = prepare_watched_file_registration_update(
        &config,
        &coordinator,
        sol_spec(&project, "/stale"),
    );
    let update =
        prepare_watched_file_registration_update(&config, &coordinator, specs.clone()).unwrap();
    let first_generation = update.generation;

    spawn_watched_file_registration_update(&client, &coordinator, stale);
    spawn_watched_file_registration_update(&client, &coordinator, Some(update));
    wait_until_idle(&coordinator).await;

    let retry = prepare_watched_file_registration_update(&config, &coordinator, specs).unwrap();
    assert!(retry.generation > first_generation);
}

#[tokio::test(flavor = "current_thread")]
async fn failed_watched_file_replacement_keeps_the_previous_registration() {
    let (project, config) = workspace_watch_config();
    let script = ClientScript { fail_register: Some(1), ..Default::default() };
    let mut harness = RegistrationHarness::new(config, script);

    let (first_id, _) = harness.register(sol_spec(&project, "/first")).await;
    let (second_id, _) = harness.register(sol_spec(&project, "/second")).await;
    assert_ne!(second_id, first_id);

    wait_until_idle(&harness.coordinator).await;
    assert!(harness.events.try_recv().is_err());
    assert_eq!(*harness.coordinator.active_registration_ids.lock(), [first_id.as_str()]);

    let (retry_id, _) = harness.register(sol_spec(&project, "/second")).await;
    harness.expect_unregistration(&first_id).await;
    harness.wait_for_active(&[&retry_id]).await;
    harness.pair.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_replacement_preserves_previous_registration_until_latest_is_active() {
    let (project, config) = workspace_watch_config();
    let (replacement_ack_tx, replacement_ack_rx) = oneshot::channel();
    let script =
        ClientScript { delay_register: Some((1, replacement_ack_rx)), ..Default::default() };
    let mut harness = RegistrationHarness::new(config, script);
    let shared_root = project.path("/shared");
    let latest_root = project.path("/latest");

    let (first_id, _) = harness.register(sol_spec(&project, "/shared")).await;
    let (second_id, _) = harness.register(sol_spec(&project, "/stale")).await;
    assert_ne!(second_id, first_id);

    let third_specs = [&shared_root, &latest_root]
        .map(|root| WatchedFileSpec::new(root.clone(), "**/*.sol"))
        .to_vec();
    harness.send(prepare_watched_file_registration_update(
        &harness.config,
        &harness.coordinator,
        third_specs,
    ));
    replacement_ack_tx.send(()).unwrap();

    let (third_id, third_registration) = harness.next_registration().await;
    assert!(has_spec(&third_registration, &shared_root, "**/*.sol"));
    assert!(has_spec(&third_registration, &latest_root, "**/*.sol"));
    harness.expect_unregistration(&first_id).await;
    harness.expect_unregistration(&second_id).await;
    harness.wait_for_active(&[&third_id]).await;
    harness.pair.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn failed_unregistration_is_retried_after_the_next_replacement() {
    let (project, config) = workspace_watch_config();
    let script = ClientScript { fail_unregister: Some(0), ..Default::default() };
    let mut harness = RegistrationHarness::new(config, script);

    let (first_id, _) = harness.register(sol_spec(&project, "/first")).await;
    let (second_id, _) = harness.register(sol_spec(&project, "/second")).await;
    harness.expect_unregistration(&first_id).await;

    harness.register(sol_spec(&project, "/third")).await;
    harness.expect_unregistration(&first_id).await;
    harness.expect_unregistration(&second_id).await;
    harness.pair.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn discovery_refreshes_watched_file_specs_before_analysis() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "contracts"

        //- /contracts/Main.sol
        //- /out/generated/.keep
        "#,
    );
    let config = relative_watch_config(&project, &["/"], &[]);
    let initial_specs = config.watched_file_specs();
    let mut state = state_with(config);
    *state.watched_file_registration.desired_specs.lock() = Some(initial_specs);

    state.rediscover_workspaces().unwrap();

    let desired_specs = || state.watched_file_registration.desired_specs.lock().clone().unwrap();
    assert!(has_desired_spec(&desired_specs(), &project.path("/contracts"), "**/*.sol"));

    // Reregistration keeps watchers for missing import candidates.
    let missing_parent = project.path("/out/generated");
    let mut commit = state.analysis_commit.lock();
    commit.analysis_paths.missing_candidates.insert(missing_parent.join("Missing.sol"));
    drop(commit);
    state.reregister_watched_files();
    assert!(has_desired_spec(&desired_specs(), &missing_parent, "*.sol"));
}

#[tokio::test(flavor = "current_thread")]
async fn completed_watcher_registration_rechecks_missing_candidates() {
    for symlinked in [false, cfg!(unix)] {
        let project = TestProject::from_fixture(
            r#"
            //- /Main.sol
            import "./generated/Missing.sol";
            contract Main is Missing {}

            //- /generated/Target.sol
            contract Missing {}
            "#,
        );
        let mut state = state_with(project.config());
        let missing = project.path("/generated/Missing.sol");
        state.analysis_commit.lock().analysis_paths.missing_candidates.insert(missing.clone());
        if symlinked {
            #[cfg(unix)]
            symlink(project.path("/generated/Target.sol"), &missing).unwrap();
        } else {
            project.write_file("/generated/Missing.sol", "contract Missing {}");
        }
        let previous_version = analysis_version(&state);

        assert!(
            state.on_watched_file_registration_ready(WatchedFileRegistrationReady).is_continue()
        );

        assert_eq!(analysis_version(&state), previous_version + 1);
        cancel_analysis(&state);
    }
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
    let config = relative_watch_config(&project, &["/repo/workspace", "/shared"], &[]);
    let discovery = config.discover_workspaces(&IndexingCancellation::default()).unwrap();
    let mut harness = RegistrationHarness::new(config.clone(), ClientScript::default());
    let mut state = GlobalState::new(harness.pair.client.clone());
    state.config = Arc::new(config);
    let desired_specs =
        |state: &GlobalState| state.watched_file_registration.desired_specs.lock().clone().unwrap();
    let (version, progress) = begin_rediscovery(&mut state);

    assert!(
        state
            .on_workspace_discovery_ready(discovery_ready(version, discovery, progress))
            .is_continue()
    );
    cancel_analysis(&state);
    let repo = project.path("/repo");
    let shared_contracts = project.path("/shared/contracts");
    let specs = desired_specs(&state);
    assert!(has_desired_spec(&specs, &repo, "foundry.toml"));
    assert!(has_desired_spec(&specs, &shared_contracts, "**/*.sol"));
    let (discovered_id, registration) = harness.next_registration().await;
    assert!(has_spec(&registration, &repo, "foundry.toml"));
    assert!(has_spec(&registration, &shared_contracts, "**/*.sol"));

    let dependency_parent = project.path("/repo/dependencies");
    let outside_parent = project.path("/outside");
    let missing_parent = project.path("/repo/missing");
    let output = path_output(AnalysisPathIndex {
        missing_candidates: FxHashSet::from_iter([missing_parent.join("Missing.sol")]),
        ..resolved_paths([
            dependency_parent.join("Dependency.sol"),
            outside_parent.join("Outside.sol"),
        ])
    });
    assert!(state.snapshot().publish_analysis_output(version, output.into_shared()));
    let specs = desired_specs(&state);
    assert!(has_desired_spec(&specs, &dependency_parent, "*.sol"));
    assert!(!has_desired_spec(&specs, &dependency_parent, "**/*.sol"));
    assert!(!specs.iter().any(|spec| spec.base == outside_parent));
    assert!(!specs.iter().any(|spec| spec.base == missing_parent));
    assert!(has_desired_spec(&specs, &repo, "*"));
    let (published_id, registration) = harness.next_registration().await;
    assert_ne!(published_id, discovered_id);
    harness.expect_unregistration(&discovered_id).await;
    assert!(has_spec(&registration, &dependency_parent, "*.sol"));
    assert!(!has_spec(&registration, &dependency_parent, "**/*.sol"));
    assert!(!has_spec(&registration, &outside_parent, "*.sol"));
    assert!(!has_spec(&registration, &missing_parent, "*.sol"));
    assert!(has_spec(&registration, &repo, "*"));

    state.clear_analysis_cache();
    assert!(!desired_specs(&state).iter().any(|spec| spec.base == dependency_parent));
    let (cleared_id, registration) = harness.next_registration().await;
    assert_ne!(cleared_id, published_id);
    harness.expect_unregistration(&published_id).await;
    assert!(!has_spec(&registration, &dependency_parent, "**/*.sol"));
    harness.pair.exit().await;
}

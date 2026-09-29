use super::*;
use std::sync::atomic::AtomicBool;

/// A project whose main source imports a dependency below `/generated`.
pub(super) const GENERATED_DEPENDENCY: &str = r#"
    //- /Main.sol
    import "./generated/Dependency.sol";
    contract Main is Dependency {}

    //- /generated/Dependency.sol
    contract Dependency {}
    "#;

async fn reanalyze(state: &mut GlobalState, changed_paths: Vec<PathBuf>) {
    state.recompute_after_opening_source(changed_paths);
    settle(state).await;
}

fn assert_workspace_diagnostics_clean(state: &GlobalState) {
    for report in state.diagnostics.read().workspace_pull_reports(Vec::new()) {
        let PullReport::Full { diagnostics, .. } = report.report else {
            unreachable!("no previous result IDs")
        };
        assert!(diagnostics.is_empty());
    }
}

fn report_version(state: &GlobalState, uri: &Url) -> Option<i64> {
    let reports = state.diagnostics.read().workspace_pull_reports(Vec::new());
    reports.into_iter().find(|report| report.uri == *uri).unwrap().version
}

pub(super) fn analysis_result(
    documents: impl IntoIterator<Item = (Url, Option<i64>)>,
    diagnostics: impl IntoIterator<Item = (Url, Vec<Diagnostic>)>,
) -> AnalysisResult {
    AnalysisResult {
        analyzed_documents: documents.into_iter().collect(),
        diagnostics: diagnostics.into_iter().collect(),
        symbol_tables: Default::default(),
    }
}

pub(super) fn path_output(analysis_paths: AnalysisPathIndex) -> AnalysisOutput {
    AnalysisOutput { result: analysis_result([], []), analysis_paths }
}

pub(super) fn workspace_bases(config: &Config) -> Vec<PathBuf> {
    let workspaces = config.workspaces().iter();
    workspaces.filter_map(|workspace| workspace.compile_opts().base_path.clone()).collect()
}

pub(super) fn resolved_paths(paths: impl IntoIterator<Item = PathBuf>) -> AnalysisPathIndex {
    AnalysisPathIndex { resolved_dependencies: paths.into_iter().collect(), ..Default::default() }
}

/// Builds a workspace folder change that adds the `added` folders and removes the `removed` ones.
pub(super) fn workspace_folders_change(
    added: &[Url],
    removed: &[Url],
) -> DidChangeWorkspaceFoldersParams {
    let folders = |uris: &[Url]| {
        uris.iter().map(|uri| WorkspaceFolder { uri: uri.clone(), name: "folder".into() }).collect()
    };
    let event = WorkspaceFoldersChangeEvent { added: folders(added), removed: folders(removed) };
    DidChangeWorkspaceFoldersParams { event }
}

pub(super) fn change_workspace_folders(state: &mut GlobalState, added: &[Url], removed: &[Url]) {
    let params = workspace_folders_change(added, removed);
    assert!(crate::handlers::did_change_workspace_folders(state, params).is_continue());
}

/// Negotiates `params` with a host `launch_config` and discovers its workspaces.
pub(super) fn host_config(params: InitializeParams, launch_config: &crate::LaunchConfig) -> Config {
    let (_, mut config) = crate::config::negotiate_capabilities_with_pull_diagnostic_data(
        params,
        false,
        launch_config,
    );
    config.rediscover_workspaces();
    config
}

/// A config whose host Foundry loader uses `src` sources and fails for roots where `fails` holds.
fn failing_loader_config(
    params: InitializeParams,
    fails: impl Fn(&Path) -> bool + Send + Sync + 'static,
) -> Config {
    let launch_config =
        crate::LaunchConfig::default().with_foundry_workspace_config_loader(move |root| {
            if fails(root) {
                return Err("host config unavailable");
            }
            Ok(crate::FoundryWorkspaceConfig::new(root).with_source_roots(["src"]))
        });
    host_config(params, &launch_config)
}

/// Starts a rediscovery that fails in the host loader and waits for the recovery analysis.
async fn fail_rediscovery(state: &mut GlobalState) {
    let (version, progress) = begin_rediscovery(state);
    assert!(state.analysis_commit.lock().discovery_pending);
    let Err(error) = state.config.try_discover_workspaces(&IndexingCancellation::default()) else {
        panic!("host loader should fail workspace discovery")
    };
    let failed = WorkspaceDiscoveryFailed { version, error: error.to_string(), progress };
    assert!(state.on_workspace_discovery_failed(failed).is_continue());
    settle(state).await;
}

/// Serves a state that uses `config` to a quiet client. `setup` runs on the state before `route`
/// adds test handlers to its router.
fn serve<T>(
    config: Config,
    setup: impl FnOnce(&mut GlobalState) -> T,
    route: impl FnOnce(&mut Router<GlobalState>),
) -> (LspPair, T) {
    let mut output = None;
    let server = |client| {
        let mut state = GlobalState::new(client);
        state.config = Arc::new(config);
        output = Some(setup(&mut state));
        let mut router = crate::new_router_with_state(state);
        route(&mut router);
        router
    };
    let pair = LspPair::spawn(server, |_| quiet_client());
    (pair, output.unwrap())
}

/// Analyzes the only batch of a fixture as a previous analysis would have.
pub(super) fn analyze_project(project: &TestProject, config: &Config) -> AnalysisOutput {
    let mut batches =
        snapshot_with_config(config.clone(), project.vfs()).analysis_batches(Vec::new());
    analyze_cancellable(batches.pop().unwrap(), &IndexingCancellation::default()).unwrap()
}

pub(super) fn fail_analysis(state: &GlobalState, error: &str) {
    cancel_analysis(state);
    assert!(
        handle_analysis_failure(
            analysis_version(state),
            error,
            &state.analysis_version,
            &state.published_analysis_version,
            &state.analysis_commit,
        )
        .is_some()
    );
}

pub(super) fn symbol_names(tables: &Arc<ArcSwap<SymbolTables>>, query: &str) -> Vec<String> {
    tables.load().workspace_symbols(query).into_iter().map(|symbol| symbol.name).collect()
}

async fn wait_published(published: &mut watch::Receiver<usize>, done: impl Fn(usize) -> bool) {
    within("publication", async {
        while !done(*published.borrow()) {
            published.changed().await.unwrap();
        }
    })
    .await;
}

#[tokio::test(flavor = "current_thread")]
async fn dependency_references_survive_closing_arbitrary_project_sources() {
    for (foundry, created_later) in [(false, false), (true, false), (false, true), (true, true)] {
        let marked = MarkedProject::from_fixture(
            r#"
        //- /foundry.toml
        [profile.default]
        //- /src/Main.sol
        contract Main {}
        //- /lib/forge-std/src/Base.sol
        abstract contract Base { uint internal constant $1vm = 1; }
        //- /checks/Main.sol
        import "../lib/forge-std/src/Base.sol";
        contract Test is Base { function run() public pure returns (uint) { return $2vm; } }
        //- /examples/Main.sol
        import "../lib/forge-std/src/Base.sol";
        contract Script is Base { function run() public pure returns (uint) { return $3vm; } }
        "#,
        );
        let project = marked.project();
        if !foundry {
            project.remove_file("/foundry.toml");
        }
        let caller_source = project.read_file("/checks/Main.sol");
        if created_later {
            project.remove_file("/checks/Main.sol");
            std::fs::remove_dir(project.path("/checks")).unwrap();
        }
        let uri = project.uri("/lib/forge-std/src/Base.sol");
        let test_uri = project.uri("/checks/Main.sol");
        let mut state = state_with(project.config());
        if created_later {
            // No watcher notification: the editor is how we discover this new file.
            project.write_file("/checks/Main.sol", &caller_source);
        }
        let references = |state: &GlobalState, uri: &Url, marker: &str| {
            let position = marked.marker(marker).position();
            state.symbol_tables.load().references(uri, position, false).unwrap()
        };
        let expected = ["$2", "$3"].map(|name| {
            let marker = marked.marker(name);
            let position = marker.position();
            lsp_types::Location::new(
                project.uri(marker.path()),
                Range::new(position, Position::new(position.line, position.character + 2)),
            )
        });

        open(&mut state, &uri, 1, project.read_file("/lib/forge-std/src/Base.sol"));
        settle(&state).await;
        assert_eq!(
            references(&state, &uri, "$1"),
            if created_later { &expected[1..] } else { &expected[..] }
        );

        open(&mut state, &test_uri, 1, project.read_file("/checks/Main.sol"));
        settle(&state).await;
        assert_eq!(references(&state, &uri, "$1"), expected);
        assert_eq!(references(&state, &test_uri, "$2"), expected);

        let edited = project.read_file("/checks/Main.sol").replace("return vm", "return 0");
        change(&mut state, &test_uri, 2, edited);
        settle(&state).await;
        assert_eq!(references(&state, &uri, "$1"), expected[1..]);

        for closed_uri in [&test_uri, &uri] {
            close(&mut state, closed_uri);
            settle(&state).await;
            assert_eq!(references(&state, &uri, "$1"), expected);
        }
        assert!(
            state
                .config
                .watched_file_specs()
                .iter()
                .any(|spec| spec.base == project.path("/checks") && spec.pattern == "**/*.sol")
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn identical_sources_and_reverted_edits_reuse_analysis_and_initialized_queries() {
    let caller = "{ function target() internal {} function $1caller() external { target(); } }";
    let single = format!("//- /Main.sol\ncontract Main {caller}\n");
    let with_dependency = format!(
        "//- /Main.sol\nimport \"./lib/Dep.sol\";\ncontract Main is Dep {caller}\n\
         //- /lib/Dep.sol\ncontract Dep {{}}\n"
    );
    let workspaces =
        format!("//- /a/Main.sol\ncontract Main {caller}\n//- /b/Other.sol\ncontract Other {{}}\n");
    for (fixture, main, roots) in [
        (single, "/Main.sol", &["/"][..]),
        (with_dependency, "/Main.sol", &["/"]),
        (workspaces, "/a/Main.sol", &["/a", "/b"]),
    ] {
        let marked = MarkedProject::from_fixture(&fixture);
        let project = marked.project();
        let path = project.path(main);
        let uri = Url::from_file_path(&path).unwrap();
        let source = project.read_file(main);
        let mut state = state_with(project.config_with_roots(roots));
        reanalyze(&mut state, Vec::new()).await;
        let published = state.symbol_tables.load_full();
        let caller =
            published.prepare_call_hierarchy(&uri, marked.marker("$1").position()).unwrap().pop();
        let caller = caller.unwrap();
        assert_eq!(published.call_hierarchy_outgoing(&caller).unwrap().len(), 1);
        assert!(published.call_hierarchy_is_initialized());
        let reused = |state: &GlobalState| {
            let current = state.symbol_tables.load();
            Arc::ptr_eq(&published, &current) && current.call_hierarchy_is_initialized()
        };

        reanalyze(&mut state, Vec::new()).await;
        assert!(reused(&state));

        open(&mut state, &uri, 7, source.clone());
        settle(&state).await;
        assert!(reused(&state));
        assert_eq!(report_version(&state, &uri), Some(7));

        set_overlay(&state, &path, "contract Edited {}", 8);
        set_overlay(&state, &path, &source, 9);
        reanalyze(&mut state, vec![path.clone()]).await;
        assert!(reused(&state));
        assert_eq!(report_version(&state, &uri), Some(9));

        // Removing an identical overlay changes its version without changing compiler inputs.
        // The didClose handler explicitly invalidates the cache before reaching this path.
        remove_overlay(&state, &path);
        reanalyze(&mut state, Vec::new()).await;
        assert!(reused(&state));
        assert_eq!(state.symbol_tables.load().call_hierarchy_outgoing(&caller).unwrap().len(), 1);
        assert_eq!(report_version(&state, &uri), None);
        // An unchanged epoch must retain the refreshed versions in the aggregate cache too.
        reanalyze(&mut state, Vec::new()).await;
        assert_eq!(report_version(&state, &uri), None);

        set_overlay(&state, &path, "contract Edited {}", 10);
        reanalyze(&mut state, vec![path]).await;
        assert!(!Arc::ptr_eq(&published, &state.symbol_tables.load()));
        assert_eq!(symbol_names(&state.symbol_tables, "Edited"), ["Edited"]);

        let edited = state.symbol_tables.load_full();
        state.config = Arc::new((*state.config).clone());
        reanalyze(&mut state, Vec::new()).await;
        assert!(!Arc::ptr_eq(&edited, &state.symbol_tables.load()));
    }
}

/// Analyzes `/a/Main.sol` and `/b/Other.sol` as separate workspaces, returning both paths.
async fn two_workspaces() -> (TestProject, GlobalState, PathBuf, PathBuf) {
    let project = TestProject::from_fixture(
        r#"
        //- /a/Main.sol
        contract Main { uint public original; }
        //- /b/Other.sol
        contract Other { uint public stable; }
        "#,
    );
    let mut state = state_with(project.config_with_roots(&["/a", "/b"]));
    reanalyze(&mut state, Vec::new()).await;
    let (main, other) = (project.path("/a/Main.sol"), project.path("/b/Other.sol"));
    (project, state, main, other)
}

fn cached_batch_for_path(state: &GlobalState, path: &Path) -> Option<Arc<CachedAnalysisBatch>> {
    let commit = state.analysis_commit.lock();
    let cached = commit.cached_output.as_ref()?;
    let idx = cached
        .inputs
        .iter()
        .position(|inputs| inputs.files.iter().any(|(input_path, _)| input_path == path))?;
    cached.batches.get(idx)?.clone()
}

#[tokio::test(flavor = "current_thread")]
async fn removing_workspace_batch_inputs_invalidates_the_aggregate() {
    let project = TestProject::new();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    for (root, name) in [("/a", "First"), ("/b", "Second"), ("/c", "Removed")] {
        std::fs::create_dir_all(project.path(root)).unwrap();
        set_overlay(
            &state,
            &project.path(&format!("{root}/Main.sol")),
            &format!("contract {name} {{}}"),
            1,
        );
    }
    state.config = Arc::new(project.config_with_roots(&["/a", "/b", "/c"]));
    reanalyze(&mut state, Vec::new()).await;
    let published = state.symbol_tables.load_full();
    assert_eq!(published.workspace_symbols("").len(), 3);

    // The remaining batches are reusable, but the newly empty batch changes the aggregate.
    remove_overlay(&state, &project.path("/c/Main.sol"));
    reanalyze(&mut state, Vec::new()).await;
    let current = state.symbol_tables.load_full();
    assert!(!Arc::ptr_eq(&published, &current));
    assert!(current.workspace_symbols("Removed").is_empty());
    assert_eq!(current.workspace_symbols("").len(), 2);

    reanalyze(&mut state, Vec::new()).await;
    assert!(Arc::ptr_eq(&current, &state.symbol_tables.load()));
}

#[tokio::test(flavor = "current_thread")]
async fn editing_one_workspace_reuses_other_workspace_and_current_document_versions() {
    let (project, mut state, main, other) = two_workspaces().await;
    let main_uri = Url::from_file_path(&main).unwrap();
    let other_uri = Url::from_file_path(&other).unwrap();
    let original_main = cached_batch_for_path(&state, &main).unwrap();
    let original_other = cached_batch_for_path(&state, &other).unwrap();

    for (uri, path) in [(&main_uri, "/a/Main.sol"), (&other_uri, "/b/Other.sol")] {
        open(&mut state, uri, 7, project.read_file(path));
        settle(&state).await;
    }

    change(&mut state, &main_uri, 8, "contract Main { uint public edited; }");
    settle(&state).await;
    assert!(!Arc::ptr_eq(&original_main, &cached_batch_for_path(&state, &main).unwrap()));
    assert!(Arc::ptr_eq(&original_other, &cached_batch_for_path(&state, &other).unwrap()));
    assert!(symbol_names(&state.symbol_tables, "original").is_empty());
    assert_eq!(symbol_names(&state.symbol_tables, "edited"), ["edited"]);
    assert_eq!(symbol_names(&state.symbol_tables, "stable"), ["stable"]);
    assert_eq!(report_version(&state, &main_uri), Some(8));
    assert_eq!(report_version(&state, &other_uri), Some(7));

    close(&mut state, &main_uri);
    settle(&state).await;
    assert!(symbol_names(&state.symbol_tables, "edited").is_empty());
    assert_eq!(symbol_names(&state.symbol_tables, "original"), ["original"]);
    assert_eq!(symbol_names(&state.symbol_tables, "stable"), ["stable"]);
    assert_eq!(report_version(&state, &main_uri), None);
    assert_eq!(report_version(&state, &other_uri), Some(7));
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_batch_cache_revalidates_config_and_disk_sources() {
    let (project, mut state, main, other) = two_workspaces().await;
    let original = cached_batch_for_path(&state, &other).unwrap();

    state.config = Arc::new((*state.config).clone());
    reanalyze(&mut state, Vec::new()).await;
    let reconfigured = cached_batch_for_path(&state, &other).unwrap();
    assert!(!Arc::ptr_eq(&original, &reconfigured));

    // A source edit also observes changed disk roots without a watcher notification.
    project.write_file("/b/Other.sol", "contract Other { uint public diskChanged; }");
    set_overlay(&state, &main, "contract Main { uint public edited; }", 1);
    reanalyze(&mut state, vec![main.clone()]).await;
    assert!(!Arc::ptr_eq(&reconfigured, &cached_batch_for_path(&state, &other).unwrap()));
    assert_eq!(symbol_names(&state.symbol_tables, "diskChanged"), ["diskChanged"]);

    // Reanalysis without a VFS revision must still observe an unnotified disk root change.
    project.write_file("/b/Other.sol", "contract Other { uint public diskOnly; }");
    reanalyze(&mut state, Vec::new()).await;
    assert_eq!(symbol_names(&state.symbol_tables, "diskOnly"), ["diskOnly"]);

    project.write_file("/b/Other.sol", "contract Other { uint public notified; }");
    state.recompute_for_file_changes(vec![other], Vec::new(), false);
    // A new document request may cancel the pending disk worker; invalidation must survive it.
    reanalyze(&mut state, vec![main]).await;
    assert!(symbol_names(&state.symbol_tables, "diskChanged").is_empty());
    assert_eq!(symbol_names(&state.symbol_tables, "notified"), ["notified"]);
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_batch_cache_rechecks_disk_imports_and_resolver_probes() {
    for (initially_missing, edit_main) in
        [(false, false), (true, false), (false, true), (true, true)]
    {
        let project = TestProject::from_fixture(
            r#"
            //- /a/Main.sol
            contract Main {}
            //- /b/Other.sol
            import "./lib/Dep.sol";
            contract Other is Dep {}
            //- /b/lib/Dep.sol
            contract Dep {}
            "#,
        );
        let main = project.path("/a/Main.sol");
        let other = project.path("/b/Other.sol");
        if initially_missing {
            project.remove_file("/b/lib/Dep.sol");
        }
        let mut state = state_with(project.config_with_roots(&["/a", "/b"]));
        reanalyze(&mut state, Vec::new()).await;
        let published = state.symbol_tables.load_full();
        let original_main = cached_batch_for_path(&state, &main).unwrap();
        let original_other = cached_batch_for_path(&state, &other).unwrap();
        let vfs_revision = state.vfs.read().content_revision();

        // Neither path signals this disk change: the previously observed import must be checked
        // again whether or not other batch inputs change.
        project.write_file("/b/lib/Dep.sol", "contract Dep { uint public dependencyChanged; }");
        if edit_main {
            set_overlay(&state, &main, "contract Main { uint public edited; }", 1);
            state.recompute_after_opening_source(vec![main.clone()]);
        } else {
            state.recompute_after_opening_source(Vec::new());
        }
        settle(&state).await;
        assert_eq!(state.vfs.read().content_revision() == vfs_revision, !edit_main);
        assert!(!Arc::ptr_eq(&published, &state.symbol_tables.load()));
        let current_main = cached_batch_for_path(&state, &main).unwrap();
        assert_eq!(Arc::ptr_eq(&original_main, &current_main), !edit_main);
        assert!(!Arc::ptr_eq(&original_other, &cached_batch_for_path(&state, &other).unwrap()));
        assert_eq!(symbol_names(&state.symbol_tables, "dependencyChanged"), ["dependencyChanged"]);
        assert_workspace_diagnostics_clean(&state);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn opening_identical_source_rechecks_single_workspace_disk_imports() {
    for initially_missing in [false, true] {
        let project = TestProject::from_fixture(
            r#"
            //- /Main.sol
            import "./lib/Dep.sol";
            contract Main is Dep {}
            //- /lib/Dep.sol
            contract Dep { uint public original; }
            "#,
        );
        let main = project.path("/Main.sol");
        let main_uri = Url::from_file_path(&main).unwrap();
        let dep_uri = project.uri("/lib/Dep.sol");
        let dependency = project.read_file("/lib/Dep.sol");
        if initially_missing {
            project.remove_file("/lib/Dep.sol");
        }
        let mut state = state_with(project.config());
        reanalyze(&mut state, Vec::new()).await;
        let published = state.symbol_tables.load_full();
        let vfs_revision = state.vfs.read().content_revision();
        let previous_report = state.diagnostics.read().pull_report(&main_uri, None);

        // A missing import that remains missing is a reusable observation too.
        set_overlay(&state, &main, &project.read_file("/Main.sol"), 6);
        reanalyze(&mut state, vec![main.clone()]).await;
        assert!(Arc::ptr_eq(&published, &state.symbol_tables.load()));
        assert_eq!(state.diagnostics.read().pull_report(&main_uri, None), previous_report);
        // Remove the overlay so opening it again advances the content revision below.
        remove_overlay(&state, &main);

        // Root text still matches. The import changed without any watcher notification,
        // and equal byte lengths must not substitute for comparing dependency contents.
        let changed_dependency = dependency.replace("original", "modified");
        assert_eq!(dependency.len(), changed_dependency.len());
        project.write_file("/lib/Dep.sol", &changed_dependency);
        open(&mut state, &main_uri, 7, project.read_file("/Main.sol"));
        settle(&state).await;
        assert_ne!(state.vfs.read().content_revision(), vfs_revision);
        assert!(!Arc::ptr_eq(&published, &state.symbol_tables.load()));
        assert!(symbol_names(&state.symbol_tables, "original").is_empty());
        assert_eq!(symbol_names(&state.symbol_tables, "modified"), ["modified"]);
        assert_eq!(report_version(&state, &main_uri), Some(7));
        assert_eq!(report_version(&state, &dep_uri), None);
        assert_workspace_diagnostics_clean(&state);
    }
}

#[tokio::test(flavor = "current_thread")]
#[cfg(unix)]
async fn opening_identical_source_rechecks_retargeted_single_workspace_import() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./lib/Dep.sol";
        contract Main is Dep {}
        //- /lib/First.sol
        contract Dep {}
        //- /lib/Second.sol
        contract Dep {}
        "#,
    );
    let link = project.path("/lib/Dep.sol");
    let import_uri = Url::from_file_path(&link).unwrap();
    symlink(project.path("/lib/First.sol"), &link).unwrap();
    let mut state = state_with(project.config());
    reanalyze(&mut state, Vec::new()).await;
    let published = state.symbol_tables.load_full();
    let declaration = published.declarations().iter().find(|symbol| symbol.name == "Dep").unwrap();
    assert_eq!(declaration.location.uri, import_uri);

    // Identical text at a retargeted symlink still invalidates recorded path resolution.
    project.remove_file("/lib/Dep.sol");
    symlink(project.path("/lib/Second.sol"), &link).unwrap();
    let main_uri = project.uri("/Main.sol");
    open(&mut state, &main_uri, 7, project.read_file("/Main.sol"));
    settle(&state).await;
    let current = state.symbol_tables.load_full();
    assert!(!Arc::ptr_eq(&published, &current));
    let declaration = current.declarations().iter().find(|symbol| symbol.name == "Dep").unwrap();
    assert_eq!(declaration.location.uri, import_uri);
}

#[tokio::test(flavor = "current_thread")]
async fn disk_dependency_changes_survive_identical_inputs_and_cache_reuse_attempts() {
    #[derive(Clone, Copy)]
    enum Trigger {
        DiskFiles,
        Notification,
        RevertedEdit,
    }

    for trigger in [Trigger::DiskFiles, Trigger::Notification, Trigger::RevertedEdit] {
        let project = TestProject::from_fixture(
            r#"
            //- /Main.sol
            import "./lib/Dep.sol";
            contract Main is Dep {}
            //- /lib/Dep.sol
            contract Dep {}
        "#,
        );
        let main = project.path("/Main.sol");
        let dep = project.path("/lib/Dep.sol");
        let source = project.read_file("/Main.sol");
        let mut state = state_with(project.config());
        set_overlay(&state, &main, &source, 1);
        reanalyze(&mut state, Vec::new()).await;
        let published = state.symbol_tables.load_full();

        std::fs::write(&dep, "contract Dep { uint public changed; }").unwrap();
        match trigger {
            Trigger::DiskFiles => state.recompute_with_disk_files(vec![dep]),
            // Supersede the debounced disk request without changing the VFS revision.
            Trigger::Notification => {
                state.recompute_for_file_changes(vec![dep], Vec::new(), false);
                state.recompute_after_opening_source(vec![main]);
            }
            // A reverted edit must reload disk imports even without a watcher notification.
            Trigger::RevertedEdit => {
                set_overlay(&state, &main, "contract Edited {}", 2);
                set_overlay(&state, &main, &source, 3);
                state.recompute_after_opening_source(vec![main]);
            }
        }
        settle(&state).await;
        assert!(!Arc::ptr_eq(&published, &state.symbol_tables.load()));
        assert_eq!(symbol_names(&state.symbol_tables, "changed"), ["changed"]);
    }
}

#[tokio::test(flavor = "current_thread")]
async fn cached_published_and_retained_symbol_tables_share_storage() {
    let project = TestProject::from_fixture("//- /Main.sol\ncontract Main {}\n");
    let mut state = project.state();
    reanalyze(&mut state, vec![project.path("/Main.sol")]).await;

    let published = state.symbol_tables.load_full();
    {
        let commit = state.analysis_commit.lock();
        let cached = commit.cached_output.as_ref().unwrap();
        assert!(Arc::ptr_eq(&published, &cached.output.result.symbol_tables));
    }
    reanalyze(&mut state, Vec::new()).await;
    assert!(Arc::ptr_eq(&published, &state.symbol_tables.load()));

    // Publication precedes worker cleanup; wait until it releases its symbol references.
    let _permit =
        within("the worker", state.analysis_scheduler.gate.clone().acquire_owned()).await.unwrap();
    let tables = state.symbol_tables.clone();
    let retained = tables.load();
    let old = Arc::downgrade(&published);
    drop(published);
    let mut snapshot = state.snapshot();
    assert!(snapshot.publish_symbol_tables(analysis_version(&state), Arc::default()));
    assert!(tables.load().workspace_symbols("").is_empty());
    assert!(!retained.workspace_symbols("Main").is_empty());

    // Neither publication nor clearing waits for a retained reader snapshot.
    state.clear_analysis_cache();
    assert!(old.upgrade().is_some());
    drop(retained);
    assert!(old.upgrade().is_none());
}

#[test]
fn analysis_tracks_excluded_transitive_dependencies_and_normalized_missing_candidates() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol
        import "./generated/B.sol";
        contract Main is B {}

        //- /generated/B.sol
        import "./nested/../C.sol";
        contract B is C {}

        //- /generated/C.sol
        import "./missing/../Missing.sol";
        contract C {}

        //- /generated/Unrelated.sol
        contract Unrelated {}
        "#,
    );
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let mut batches = snapshot_with_config(config, project.vfs()).analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert!(batches.is_empty());
    assert_eq!(batch.seen_paths, FxHashSet::from_iter([project.path("/Main.sol")]));

    let output = analyze_cancellable(batch, &IndexingCancellation::default()).unwrap();

    assert_eq!(
        output.analysis_paths.resolved_dependencies,
        FxHashSet::from_iter([project.path("/generated/B.sol"), project.path("/generated/C.sol")])
    );
    assert_eq!(
        output.analysis_paths.missing_candidates,
        FxHashSet::from_iter([project.path("/generated/Missing.sol")])
    );
    assert!(output.analysis_paths.existing_unresolved_candidates.is_empty());
    let tables = Arc::new(ArcSwap::from_pointee(output.result.symbol_tables));
    assert_eq!(symbol_names(&tables, "B"), ["B"]);
    assert_eq!(symbol_names(&tables, "C"), ["C"]);
    assert!(symbol_names(&tables, "Unrelated").is_empty());
}

#[test]
fn analysis_output_accumulator_resolved_path_wins_across_batches() {
    let path = PathBuf::from("Dependency.sol");
    let mut accumulator = AnalysisOutputAccumulator::default();
    accumulator.push(path_output(AnalysisPathIndex {
        missing_candidates: FxHashSet::from_iter([path.clone()]),
        ..Default::default()
    }));
    accumulator.push(path_output(resolved_paths([path.clone()])));

    let output = accumulator.finish();

    assert_eq!(output.analysis_paths.resolved_dependencies, FxHashSet::from_iter([path]));
    assert!(output.analysis_paths.existing_unresolved_candidates.is_empty());
    assert!(output.analysis_paths.missing_candidates.is_empty());
}

#[test]
fn stale_analysis_does_not_replace_published_path_index() {
    let output = |path: &str| path_output(resolved_paths([PathBuf::from(path)])).into_shared();
    let state = GlobalState::new(ClientSocket::new_closed());
    assert!(state.snapshot().publish_analysis_output(0, output("Current.sol")));
    let mut stale_snapshot = state.snapshot();
    state.mark_analysis_pending_for_test();

    assert!(!stale_snapshot.publish_analysis_output(0, output("Stale.sol")));
    assert_eq!(
        state.analysis_commit.lock().analysis_paths.resolved_dependencies,
        FxHashSet::from_iter([PathBuf::from("Current.sol")])
    );
}

#[test]
fn deferred_source_events_block_only_analysis_that_observed_the_path() {
    let project = TestProject::new();
    project.write_file("/Missing.sol", "contract Missing {}");
    let dependency = PathBuf::from("Dependency.sol");
    let missing = project.path("/Missing.sol");
    let cases = [
        (dependency.clone(), resolved_paths([dependency])),
        (
            missing.clone(),
            AnalysisPathIndex {
                missing_candidates: FxHashSet::from_iter([missing]),
                ..Default::default()
            },
        ),
        (PathBuf::from("Unrelated.sol"), AnalysisPathIndex::default()),
    ];
    for (path, analysis_paths) in cases {
        let blocks = !analysis_paths.resolved_dependencies.is_empty()
            || !analysis_paths.missing_candidates.is_empty();
        let state = GlobalState::new(ClientSocket::new_closed());
        state.mark_analysis_pending_for_test();
        let version = analysis_version(&state);
        assert_eq!(
            state.classify_source_file_event(&path, FileChangeType::CHANGED),
            SourceFileEventDisposition::Deferred
        );

        let published = state
            .snapshot()
            .publish_analysis_output(version, path_output(analysis_paths).into_shared());

        assert_eq!(published, !blocks);
        assert_eq!(*state.published_analysis_version.borrow(), if blocks { 0 } else { version });
        assert!(state.analysis_commit.lock().deferred_source_file_events.is_empty());
    }
}

#[test]
fn source_event_classification_observes_path_index_committed_before_its_lock() {
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let version = analysis_version(&state);
    let path = PathBuf::from("Dependency.sol");
    let mut commit = state.analysis_commit.lock();

    std::thread::scope(|scope| {
        let (started_tx, started_rx) = std_mpsc::sync_channel(1);
        let (result_tx, result_rx) = std_mpsc::sync_channel(1);
        let event_state = &state;
        let event_path = path.clone();
        scope.spawn(move || {
            started_tx.send(()).unwrap();
            result_tx
                .send(event_state.classify_source_file_event(&event_path, FileChangeType::CHANGED))
                .unwrap();
        });
        started_rx.recv_timeout(TIMEOUT).unwrap();

        commit.analysis_paths.resolved_dependencies.insert(path.clone());
        state.published_analysis_version.send_replace(version);
        drop(commit);

        assert_eq!(result_rx.recv_timeout(TIMEOUT).unwrap(), SourceFileEventDisposition::Relevant);
    });
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_clears_path_index_and_rejects_stale_deferred_replay() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let stale_version = analysis_version(&state);
    {
        let mut commit = state.analysis_commit.lock();
        commit.discovery_pending = true;
        commit.analysis_paths = AnalysisPathIndex {
            resolved_dependencies: FxHashSet::from_iter([PathBuf::from("Dependency.sol")]),
            existing_unresolved_candidates: FxHashSet::from_iter([PathBuf::from("Unreadable.sol")]),
            missing_candidates: FxHashSet::from_iter([PathBuf::from("Missing.sol")]),
        };
        commit
            .deferred_source_file_events
            .insert(PathBuf::from("Pending.sol"), FileChangeType::CHANGED);
    }

    state.clear_analysis_cache();
    let cleared_version = analysis_version(&state);
    assert!(
        state
            .on_deferred_source_file_events_ready(DeferredSourceFileEventsReady {
                version: stale_version,
                events: vec![(PathBuf::from("Dependency.sol"), FileChangeType::CHANGED)],
            })
            .is_continue()
    );

    assert_eq!(analysis_version(&state), cleared_version);
    assert_eq!(*state.published_analysis_version.borrow(), cleared_version);
    let commit = state.analysis_commit.lock();
    assert!(commit.cache_invalidated);
    assert!(!commit.discovery_pending);
    assert!(commit.analysis_paths.resolved_dependencies.is_empty());
    assert!(commit.analysis_paths.existing_unresolved_candidates.is_empty());
    assert!(commit.analysis_paths.missing_candidates.is_empty());
    assert!(commit.deferred_source_file_events.is_empty());
}

#[tokio::test(flavor = "current_thread")]
async fn latest_analysis_uses_the_config_published_with_the_analysis() {
    let project = TestProject::new();
    let published_config = Arc::new(project.config_with_roots(&["/published"]));
    let state = state_with(project.config_with_roots(&["/fallback"]));
    state.analysis_version.store(1, Ordering::Release);

    let latest = state.latest_analysis_with_config();
    {
        let mut commit = state.analysis_commit.lock();
        commit.symbol_tables_version = 1;
        commit.analysis_config = Some(published_config.clone());
    }
    state.published_analysis_version.send_replace(1);

    let (_, config) = latest.await.unwrap();
    assert!(Arc::ptr_eq(&config, &published_config));
}

#[tokio::test(flavor = "current_thread")]
async fn unknown_dependency_event_after_cache_clear_starts_recovery() {
    let project = TestProject::from_fixture(GENERATED_DEPENDENCY);
    let mut state = state_with(config_with_indexing_excludes(&project, &["generated/**"]));
    state.clear_analysis_cache();
    let cleared_version = analysis_version(&state);
    project.write_file("/generated/Dependency.sol", "contract Dependency {} contract Latest {}");

    watch_files(
        &mut state,
        [(&project.path("/generated/Dependency.sol"), FileChangeType::CHANGED)],
    );

    assert_eq!(analysis_version(&state), cleared_version + 1);
    let tables = settle(&state).await;
    assert!(!state.analysis_cache_invalidated());
    assert_eq!(symbol_names(&tables, "Latest"), ["Latest"]);
}

#[tokio::test(flavor = "current_thread")]
async fn discovery_cleanup_does_not_remove_analysis_handles_for_the_same_epoch() {
    let version = 1;
    let discovery = AnalysisTaskKey { version, stage: AnalysisTaskStage::Discovery };
    let analysis = AnalysisTaskKey { version, stage: AnalysisTaskStage::Analysis };
    let coordinator = tokio::spawn(std::future::pending::<()>());
    let worker = tokio::spawn(std::future::pending::<()>());
    let mut tasks = AnalysisTasks {
        coordinator: Some((analysis, coordinator.abort_handle())),
        worker: Some((analysis, worker.abort_handle())),
        ..Default::default()
    };

    tasks.clear_worker(discovery);
    tasks.clear_coordinator(discovery);

    assert!(tasks.worker.as_ref().is_some_and(|(key, _)| *key == analysis));
    assert!(tasks.coordinator.as_ref().is_some_and(|(key, _)| *key == analysis));
    tasks.cancel();
    let _ = coordinator.await;
    let _ = worker.await;
}

#[tokio::test(flavor = "current_thread")]
async fn workspace_discovery_router_rejects_stale_and_cancelled_ready_events() {
    struct DiscoveryStateProbe(oneshot::Sender<(Vec<PathBuf>, bool)>);

    let project = TestProject::from_fixture(
        r#"
        //- /stale/Stale.sol
        contract Stale {}

        //- /latest/Latest.sol
        contract Latest {}
        "#,
    );
    let discovery_result = |root| {
        let (_, config) = negotiate_capabilities(project.initialize_params_with_roots(&[root]));
        config.discover_workspaces(&IndexingCancellation::default()).unwrap()
    };
    let stale_result = discovery_result("/stale");
    let cancelled_result = discovery_result("/stale");
    let latest_result = discovery_result("/latest");
    let (_, latest_config) =
        negotiate_capabilities(project.initialize_params_with_roots(&["/latest"]));

    let setup = |state: &mut GlobalState| {
        let stale = begin_rediscovery(state);
        let latest = begin_rediscovery(state);
        (stale, latest, state.published_analysis_version.subscribe(), state.symbol_tables.clone())
    };
    let route = |router: &mut Router<GlobalState>| {
        router.event::<DiscoveryStateProbe>(|state, probe| {
            let pending = state.analysis_commit.lock().discovery_pending;
            probe.0.send((workspace_bases(&state.config), pending)).unwrap();
            ControlFlow::Continue(())
        });
    };
    let (pair, setup) = serve(latest_config, setup, route);
    let ((stale_version, stale_progress), (latest_version, latest_progress), mut published, tables) =
        setup;
    let ready = |version, result, progress, cancellation| WorkspaceDiscoveryReady {
        cancellation,
        ..discovery_ready(version, result, progress)
    };
    let probe = || async {
        let (probe_tx, probe_rx) = oneshot::channel();
        pair.client.emit(DiscoveryStateProbe(probe_tx)).unwrap();
        probe_rx.await.unwrap()
    };

    let event = ready(stale_version, stale_result, stale_progress, Default::default());
    pair.client.emit(event).unwrap();
    assert_eq!(probe().await, (Vec::new(), true));

    let cancelled = IndexingCancellation::default();
    cancelled.cancel();
    let event = ready(latest_version, cancelled_result, latest_progress.clone(), cancelled);
    pair.client.emit(event).unwrap();
    assert_eq!(probe().await, (Vec::new(), true));

    let event = ready(latest_version, latest_result, latest_progress, Default::default());
    pair.client.emit(event).unwrap();
    wait_published(&mut published, |published| published == latest_version).await;

    assert!(symbol_names(&tables, "Stale").is_empty());
    assert_eq!(symbol_names(&tables, "Latest"), ["Latest"]);
    pair.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn deferred_dependency_change_router_publishes_replacement_analysis() {
    struct PublishAnalysis {
        version: usize,
        output: AnalysisOutput,
    }

    let project = TestProject::from_fixture(GENERATED_DEPENDENCY);
    let config = config_with_indexing_excludes(&project, &["generated/**"]);
    let old_output = analyze_project(&project, &config);
    let dependency = project.path("/generated/Dependency.sol");
    project.write_file("/generated/Dependency.sol", "contract Dependency {} contract Latest {}");

    let setup = |state: &mut GlobalState| {
        state.mark_analysis_pending_for_test();
        assert_eq!(
            state.classify_source_file_event(&dependency, FileChangeType::CHANGED),
            SourceFileEventDisposition::Deferred
        );
        let published = state.published_analysis_version.subscribe();
        (analysis_version(state), published, state.symbol_tables.clone())
    };
    let route = |router: &mut Router<GlobalState>| {
        router.event::<PublishAnalysis>(|state, event| {
            let output = event.output.into_shared();
            assert!(!state.snapshot().publish_analysis_output(event.version, output));
            ControlFlow::Continue(())
        });
    };
    let (pair, (version, mut published, tables)) = serve(config, setup, route);

    pair.client.emit(PublishAnalysis { version, output: old_output }).unwrap();
    wait_published(&mut published, |published| published > version).await;

    assert_eq!(*published.borrow(), version + 1);
    assert_eq!(symbol_names(&tables, "Latest"), ["Latest"]);
    pair.shutdown().await;
}

#[tokio::test(flavor = "current_thread")]
async fn host_loader_failure_terminates_background_discovery_with_last_good_config() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Existing.sol
        contract Existing {}
        "#,
    );
    let fail = Arc::new(AtomicBool::new(false));
    let loader_fail = fail.clone();
    let config = failing_loader_config(project.initialize_params(), move |_| {
        loader_fail.load(Ordering::Relaxed)
    });
    assert_eq!(config.workspaces()[0].source_roots(), &[project.path("/src")]);

    let mut state = state_with(config);
    fail.store(true, Ordering::Relaxed);
    fail_rediscovery(&mut state).await;
    assert_eq!(state.config.workspaces()[0].source_roots(), &[project.path("/src")]);
    let commit = state.analysis_commit.lock();
    assert!(commit.cache_invalidated);
    assert!(!commit.discovery_pending);
}

fn workspace_folder_failure_fixture() -> (TestProject, Config) {
    let project = TestProject::from_fixture(
        r#"
        //- /a/foundry.toml
        [profile.default]
        src = "src"

        //- /a/src/A.sol
        contract A {}

        //- /b/foundry.toml
        [profile.default]
        src = "src"

        //- /b/src/B.sol
        contract B {}
        "#,
    );
    let rejected = project.path("/b");
    let params = project.initialize_params_with_roots(&["/a"]);
    let config = failing_loader_config(params, move |root| root == rejected);
    (project, config)
}

fn assert_failed_workspace_folder_change_rolled_back(
    state: &GlobalState,
    old_root: &Path,
    new_root: &Path,
) {
    assert_eq!(state.config.workspace_roots(), [old_root.to_path_buf()]);
    assert_eq!(workspace_bases(&state.config), [old_root.to_path_buf()]);
    assert!(state.config.tracks_source_file(&old_root.join("src/A.sol")));
    assert!(!state.config.tracks_source_file(&new_root.join("src/B.sol")));
    assert!(!state.analysis_commit.lock().discovery_pending);
}

#[tokio::test(flavor = "current_thread")]
async fn synchronous_workspace_folder_loader_failure_rolls_back_roots() {
    let (project, config) = workspace_folder_failure_fixture();
    let mut state = state_with(config);
    let (old_root, new_root) = (project.path("/a"), project.path("/b"));

    change_workspace_folders(&mut state, &[project.uri("/b")], &[project.uri("/a")]);

    settle(&state).await;
    assert_failed_workspace_folder_change_rolled_back(&state, &old_root, &new_root);
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_cache_discards_workspace_root_rollback_checkpoint() {
    let (project, config) = workspace_folder_failure_fixture();
    let mut state = state_with(config);
    let old_root = project.path("/a");
    let new_root = project.path("/b");
    let previous_roots = state.config.workspace_roots().to_vec();
    {
        let config = Arc::make_mut(&mut state.config);
        config.remove_workspace(&old_root);
        config.add_workspaces([new_root.clone()]);
    }
    state.record_workspace_root_change(previous_roots);
    begin_rediscovery(&mut state);
    assert_eq!(
        state.analysis_commit.lock().workspace_roots_before_change.as_deref(),
        Some(std::slice::from_ref(&old_root))
    );

    state.clear_analysis_cache();
    assert!(state.analysis_commit.lock().workspace_roots_before_change.is_none());
    assert_eq!(state.config.workspace_roots(), std::slice::from_ref(&new_root));

    fail_rediscovery(&mut state).await;
    assert_eq!(state.config.workspace_roots(), [new_root]);
}

#[tokio::test(flavor = "current_thread")]
async fn background_workspace_folder_loader_failure_rolls_back_roots() {
    struct WorkspaceStateProbe(PathBuf, PathBuf, oneshot::Sender<()>);

    let (project, config) = workspace_folder_failure_fixture();
    let route = |router: &mut Router<GlobalState>| {
        router.event::<WorkspaceStateProbe>(|state, WorkspaceStateProbe(old, new, done)| {
            assert_failed_workspace_folder_change_rolled_back(state, &old, &new);
            done.send(()).unwrap();
            ControlFlow::Continue(())
        });
    };
    let (pair, mut published) =
        serve(config, |state| state.published_analysis_version.subscribe(), route);

    let params = workspace_folders_change(&[project.uri("/b")], &[project.uri("/a")]);
    pair.server.notify::<notification::DidChangeWorkspaceFolders>(params).unwrap();
    wait_published(&mut published, |published| published != 0).await;

    let (done_tx, done_rx) = oneshot::channel();
    let probe = WorkspaceStateProbe(project.path("/a"), project.path("/b"), done_tx);
    pair.client.emit(probe).unwrap();
    done_rx.await.unwrap();
    pair.shutdown().await;
}

#[test]
fn analysis_batches_index_sources_below_overlapping_library_and_manifest_corridors() {
    let overlapping = r#"
        //- /foundry.toml
        [profile.default]
        src = "lib"

        //- /lib/Main.sol
        contract Main {}
        "#;
    let corridor = r#"
        //- /foundry.toml
        [profile.default]
        src = "lib/contracts"

        //- /lib/foundry.toml
        [profile.default]
        src = "other"

        //- /lib/contracts/Main.sol
        contract Main {}

        //- /lib/dependency/Dependency.sol
        contract Dependency {}
        "#;
    for (fixture, main) in [(overlapping, "/lib/Main.sol"), (corridor, "/lib/contracts/Main.sol")] {
        let project = TestProject::from_fixture(fixture);
        let config = project.config();
        let lib = project.path("/lib");
        assert!(!workspace_bases(&config).contains(&lib));

        let mut batches = snapshot_with_config(config, Vfs::default()).analysis_batches(Vec::new());

        let expected = vec![(project.path(main), Arc::new("contract Main {}".into()))];
        assert_eq!(batches.pop().unwrap().files, expected);
    }
}

#[test]
fn nested_external_source_and_flycheck_roots_outrank_an_outer_workspace_base() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Outer.sol
        //- /packages/app/foundry.toml
        [profile.default]
        src = "../../shared"
        test = "../../checks"

        //- /shared/Shared.sol
        //- /checks/Nested.t.sol
        "#,
    );
    let config = project.config();
    let shared = project.path("/shared/Shared.sol");
    let check = project.path("/checks/Nested.t.sol");
    let nested = workspace_at(&config, &project.path("/packages/app"));

    assert_eq!(
        nested.source_roots(),
        [
            project.path("/packages/app"),
            project.path("/shared"),
            project.path("/checks"),
            project.path("/packages/app/script")
        ]
    );
    assert!(nested.source_files().contains(&shared));
    assert!(config.tracks_source_file(&shared));
    assert!(nested.flycheck_source_files().contains(&check));
    assert!(config.tracks_flycheck_file(&check));
}

#[test]
fn discovery_finds_nested_projects_under_flycheck_roots_and_nested_manifests() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        test = "out/checks"

        //- /out/checks/deep/app/foundry.toml
        [profile.default]
        src = "src"

        //- /out/checks/deep/app/src/Check.sol
        "#,
    );
    assert_eq!(
        workspace_at(&project.config(), &project.path("/out/checks/deep/app")).source_files(),
        [project.path("/out/checks/deep/app/src/Check.sol")]
    );

    // Manifests found below a discovered source root are discovered again until a fixed point.
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "../shared/contracts"

        //- /shared/contracts/first/foundry.toml
        [profile.default]
        src = "src"

        //- /shared/contracts/first/src/First.sol
        //- /shared/contracts/first/src/second/foundry.toml
        [profile.default]
        src = "src"

        //- /shared/contracts/first/src/second/src/Second.sol
        "#,
    );
    let config = project.config();
    workspace_at(&config, &project.path("/shared/contracts/first"));
    assert_eq!(
        workspace_at(&config, &project.path("/shared/contracts/first/src/second")).source_files(),
        [project.path("/shared/contracts/first/src/second/src/Second.sol")]
    );
}

#[test]
fn discovery_and_updates_share_the_most_specific_flycheck_owner() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /packages/app/foundry.toml
        [profile.default]
        src = "src"
        test = "../../src/shared"

        //- /src/shared/Shared.t.sol
        "#,
    );
    let mut config = project.config();
    let path = project.path("/src/shared/Shared.t.sol");
    let nested_root = project.path("/packages/app");
    let owners = |config: &Config| {
        let [outer, nested] = [project.root(), &nested_root].map(|root| workspace_at(config, root));
        [
            outer.source_files().contains(&path),
            nested.source_files().contains(&path),
            outer.flycheck_source_files().contains(&path),
            nested.flycheck_source_files().contains(&path),
        ]
    };

    assert_eq!(owners(&config), [false, true, false, true]);
    config.remove_source_file(&path);
    assert_eq!(owners(&config), [false, false, false, false]);
    config.add_source_file(path.clone());
    assert_eq!(owners(&config), [false, true, false, true]);
}

use super::*;
#[cfg(unix)]
use crate::test_support::process_exists;
use crate::{
    config::negotiate_capabilities,
    test_support::{MarkedProject, TestProject},
};
use async_lsp::ClientSocket;
use lsp_types::{
    DocumentSymbol, SymbolKind, WatchKind, WorkspaceSymbol, notification::Notification,
};
use std::{path::Path, time::Duration};

mod completion;
mod document_highlight;
mod document_link;
mod goto_definition;
mod hover;
mod implementation;
mod inlay_hint;
mod references;
mod rename;
mod signature_help;
mod support;
mod type_definition;

fn snapshot(project: &TestProject) -> GlobalStateSnapshot {
    snapshot_with_config(project.config(), project.vfs())
}

fn snapshot_with_config(config: Config, vfs: Vfs) -> GlobalStateSnapshot {
    let (published_analysis_version, _) = watch::channel(1);
    GlobalStateSnapshot {
        client: ClientSocket::new_closed(),
        vfs: Arc::new(RwLock::new(vfs)),
        config: Arc::new(config),
        analysis_version: Arc::new(AtomicUsize::new(1)),
        published_analysis_version,
        flycheck_versions: Arc::new(Default::default()),
        symbol_tables: Arc::new(Default::default()),
        diagnostics: Arc::new(Default::default()),
    }
}

fn flycheck_owner(workspace: impl Into<PathBuf>) -> DiagnosticOwner {
    DiagnosticOwner::Flycheck { id: "slow".into(), workspace: workspace.into() }
}

#[test]
fn watched_file_registration_watches_solidity_and_foundry_manifests() {
    let [registration] = watched_file_registration_params().registrations.try_into().unwrap();
    assert_eq!(registration.id, "solar-watched-files");
    assert_eq!(registration.method, lsp_types::notification::DidChangeWatchedFiles::METHOD);

    assert_eq!(
        registration.register_options,
        Some(serde_json::json!({
            "watchers": [
                { "globPattern": "**/*.sol", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
                { "globPattern": "**/foundry.toml", "kind": WatchKind::Create | WatchKind::Change | WatchKind::Delete },
            ],
        }))
    );
}

#[test]
fn saving_without_matching_flychecks_keeps_previous_flycheck_results_current() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let snapshot = state.snapshot();
    let owner = flycheck_owner("/workspace");

    assert!(snapshot.is_current_flycheck(&owner, 0));

    state.run_flychecks_on_save(PathBuf::from("/workspace/Untracked.sol"));

    assert!(snapshot.is_current_flycheck(&owner, 0));
}

#[test]
fn clearing_removed_flychecks_without_owners_keeps_previous_flycheck_results_current() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let snapshot = state.snapshot();
    let owner = flycheck_owner("/workspace");

    assert!(snapshot.is_current_flycheck(&owner, 0));

    state.clear_removed_flycheck_diagnostics(Vec::new());

    assert!(snapshot.is_current_flycheck(&owner, 0));
}

#[test]
fn clearing_removed_flychecks_stales_removed_owner_results() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let snapshot = state.snapshot();
    let owner = flycheck_owner("/workspace");

    assert!(snapshot.is_current_flycheck(&owner, 0));

    state.clear_removed_flycheck_diagnostics([owner.clone()]);

    assert!(!snapshot.is_current_flycheck(&owner, 0));
}

#[test]
fn clearing_removed_file_diagnostics_stales_matching_flycheck_owner_only() {
    let project = TestProject::from_fixture(
        r#"
        //- /first/foundry.toml
        [profile.default]
        src = "src"
        //- /second/foundry.toml
        [profile.default]
        src = "src"
        "#,
    );
    let mut params = project.initialize_params_with_roots(&["/first", "/second"]);
    params.initialization_options = Some(serde_json::json!({
        "flychecks": [{
            "id": "slow",
            "command": "slow",
        }],
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);
    let snapshot = state.snapshot();
    let first_owner = flycheck_owner(project.path("/first"));
    let second_owner = flycheck_owner(project.path("/second"));

    assert!(snapshot.is_current_flycheck(&first_owner, 0));
    assert!(snapshot.is_current_flycheck(&second_owner, 0));

    state.clear_removed_file_diagnostics([project.path("/first/src/Deleted.sol")]);

    assert!(!snapshot.is_current_flycheck(&first_owner, 0));
    assert!(snapshot.is_current_flycheck(&second_owner, 0));
}

#[test]
fn beginning_flycheck_epoch_keeps_other_owner_cancel_pending() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let first_owner = flycheck_owner("/first");
    let second_owner = flycheck_owner("/second");
    let (cancel, mut cancelled) = oneshot::channel();
    state.flycheck_cancels.insert(first_owner, cancel);

    state.begin_flycheck_epoch(&second_owner);

    assert!(matches!(cancelled.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
}

#[cfg(unix)]
#[tokio::test(flavor = "current_thread")]
async fn saving_again_cancels_in_flight_flychecks() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        //- /src/Test.sol
        contract Test {}
        "#,
    );
    let first_pid_path = project.path("/first-flycheck-pid.txt");
    let second_pid_path = project.path("/second-flycheck-pid.txt");
    let mut params = project.initialize_params();
    params.initialization_options = Some(serde_json::json!({
        "flychecks": [{
            "id": "slow",
            "command": "/bin/sh",
            "args": [
                "-c",
                "if [ ! -f \"$1\" ]; then printf '%s' \"$$\" > \"$1\"; exec sleep 120; fi; printf '%s' \"$$\" > \"$2\"; printf '{}\n'",
                "sh",
                first_pid_path.display().to_string(),
                second_pid_path.display().to_string(),
            ],
        }],
    }));
    let (_, mut config) = negotiate_capabilities(params);
    config.rediscover_workspaces();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.config = Arc::new(config);

    state.run_flychecks_on_save(project.path("/src/Test.sol"));
    wait_for_path(&first_pid_path).await;
    let first_pid = project.read_file("/first-flycheck-pid.txt").parse().unwrap();

    state.run_flychecks_on_save(project.path("/src/Test.sol"));
    wait_for_path(&second_pid_path).await;
    wait_for_process_exit(first_pid).await;

    assert!(!process_exists(first_pid));
}

#[test]
fn analysis_batches_read_tracked_disk_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Saved.sol
        contract C { function f() public { number+; } }
        "#,
    );
    let path = project.path("/src/Saved.sol");
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(vec![path.clone()]);
    let batch = batches.pop().unwrap();

    assert_eq!(batch.files, vec![(path, "contract C { function f() public { number+; } }".into())]);
}

#[test]
fn goto_implementation_finds_unopened_naked_workspace_files() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Base.sol open
        interface Runner {
            function $1run(uint256 input) external returns (uint256);
        }

        //- /First.sol
        import {Runner} from "./Base.sol";

        contract First is Runner {
            function run(uint256 input) external pure override returns (uint256) {
                return input + 1;
            }
        }

        //- /Second.sol
        import {Runner} from "./Base.sol";

        contract Second is Runner {
            function run(uint256 input) external pure override returns (uint256) {
                return input + 2;
            }
        }
        "#,
    );
    let snapshot = snapshot(marked.project());
    let mut symbol_tables = SymbolTables::default();
    for batch in snapshot.analysis_batches(Vec::new()) {
        let result = analyze(batch);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
        symbol_tables.extend(result.symbol_tables);
    }

    let marker = marked.marker("$1");
    let uri = Url::from_file_path(marked.project().path(marker.path())).unwrap();
    let Some(lsp_types::GotoDefinitionResponse::Array(locations)) =
        symbol_tables.goto_implementation(&uri, marker.position())
    else {
        panic!("expected implementation locations");
    };
    let paths = locations
        .into_iter()
        .map(|location| {
            location.uri.to_file_path().unwrap().file_name().unwrap().to_str().unwrap().to_owned()
        })
        .collect::<Vec<_>>();

    assert_eq!(paths, ["First.sol", "Second.sol"]);
}

#[cfg(unix)]
async fn wait_for_path(path: &Path) {
    for _ in 0..100 {
        if path.exists() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {}", path.display());
}

#[cfg(unix)]
async fn wait_for_process_exit(pid: u32) {
    for _ in 0..100 {
        if !process_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
}

#[test]
fn analysis_batches_include_created_naked_workspace_disk_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /Open.sol open
        contract Open { function f() public { number+; } }
        "#,
    );
    let mut config = project.config();
    project.write_file("/Disk.sol", "contract Disk {}");
    let disk_path = project.path("/Disk.sol");
    let open_path = project.path("/Open.sol");
    config.add_source_file(disk_path.clone());
    let snapshot = snapshot_with_config(config, project.vfs());

    let mut batches = snapshot.analysis_batches(vec![disk_path.clone()]);
    let batch = batches.pop().unwrap();

    assert_eq!(
        batch.files,
        vec![
            (disk_path, "contract Disk {}".into()),
            (open_path, "contract Open { function f() public { number+; } }".into()),
        ]
    );
}

#[test]
fn analysis_batches_scan_workspace_source_roots_and_apply_vfs_overlay() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        contract A {}

        //- /src/ignored.txt
        not solidity
        "#,
    );
    project.open_file("/src/A.sol", "contract A { function f() public { number+; } }");
    let source_path = project.path("/src/A.sol");
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    assert_eq!(batches.len(), 1);
    let batch = batches.pop().unwrap();

    assert_eq!(
        batch.files,
        vec![(source_path, "contract A { function f() public { number+; } }".into())]
    );
    assert_eq!(batch.opts.base_path.as_deref(), Some(project.root()));
}

#[test]
fn document_links_use_vfs_overlay() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        import "./Old.sol";

        //- /src/Old.sol
        contract Old {}

        //- /src/New.sol
        contract New {}
        "#,
    );
    project.open_file("/src/A.sol", "import \"./New.sol\";");
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    let result = analyze(batches.pop().unwrap());

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let path = project.path("/src/A.sol");
    let links = result.symbol_tables.document_links(&path);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(Url::from_file_path(project.path("/src/New.sol")).unwrap()));
}

#[test]
fn analysis_batches_use_cached_workspace_source_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Cached.sol
        contract Cached {}
        "#,
    );
    let cached_path = project.path("/src/Cached.sol");
    let created_after_discovery = project.path("/src/CreatedAfterDiscovery.sol");
    let mut config = project.config();
    project.write_file("/src/CreatedAfterDiscovery.sol", "contract CreatedAfterDiscovery {}");

    let snapshot = snapshot_with_config(config.clone(), Vfs::default());

    let mut batches = snapshot.analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert_eq!(batch.files, vec![(cached_path, "contract Cached {}".into())]);

    config.add_source_file(created_after_discovery.clone());
    let outside_source_root = project.path("/test/Outside.sol");
    project.write_file("/test/Outside.sol", "contract Outside {}");
    config.add_source_file(outside_source_root.clone());
    let snapshot = snapshot_with_config(config, Vfs::default());

    let mut batches = snapshot.analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert!(batch.files.iter().any(|(path, _)| path == &created_after_discovery));
    assert!(!batch.files.iter().any(|(path, _)| path == &outside_source_root));
}

#[test]
fn analysis_batches_assign_open_files_to_most_specific_workspace() {
    let project = TestProject::from_fixture(
        r#"
        //- /nested/A.sol open
        contract A {}
        "#,
    );
    let source_path = project.path("/nested/A.sol");
    let nested = project.path("/nested");
    let config = project.config_with_roots(&["/", "/nested"]);
    let snapshot = snapshot_with_config(config, project.vfs());

    let batches = snapshot.analysis_batches(Vec::new());
    let outer_batch = batches
        .iter()
        .find(|batch| batch.opts.base_path.as_deref() == Some(project.root()))
        .unwrap();
    let inner_batch = batches
        .iter()
        .find(|batch| batch.opts.base_path.as_deref() == Some(nested.as_path()))
        .unwrap();

    assert!(!outer_batch.files.iter().any(|(path, _)| path == &source_path));
    assert_eq!(inner_batch.files, vec![(source_path, "contract A {}".into())]);
}

#[test]
fn analysis_uses_workspace_remappings_for_import_resolution() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib=lib/"]

        //- /src/A.sol
        import "@lib/B.sol"; contract A is B {}

        //- /lib/B.sol
        contract B {}
        "#,
    );
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    assert_eq!(batches.len(), 1);
    let result = analyze(batches.pop().unwrap());

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let path = project.path("/src/A.sol");
    let links = result.symbol_tables.document_links(&path);
    assert_eq!(links.len(), 1);
    assert_eq!(links[0].target, Some(Url::from_file_path(project.path("/lib/B.sol")).unwrap()));
}

#[test]
fn analysis_resolves_relative_imports_when_cwd_differs_from_workspace_root() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        import "./B.sol"; contract A is B {}

        //- /src/B.sol
        contract B {}
        "#,
    );
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    assert_eq!(batches.len(), 1);
    let result = analyze(batches.pop().unwrap());

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
}

#[test]
fn analysis_uses_foundry_auto_remappings_for_import_resolution() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/A.sol
        import "forge-std/Test.sol"; contract A is Test {}

        //- /lib/forge-std/src/Test.sol
        contract Test {}
        "#,
    );
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(Vec::new());
    assert_eq!(batches.len(), 1);
    let result = analyze(batches.pop().unwrap());

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
}

#[test]
fn analysis_batches_skip_unreadable_disk_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/.keep
        "#,
    );
    let path = project.path("/src/Missing.sol");
    let snapshot = snapshot(&project);

    let mut batches = snapshot.analysis_batches(vec![path]);
    let batch = batches.pop().unwrap();

    assert!(batch.files.is_empty());
}

#[test]
fn analyze_builds_declaration_symbol_table() {
    let project = TestProject::from_fixture(
        r#"
        //- /Symbols.sol
        uint256 constant TOP = 1;
        contract C {
            uint256 public x;
            uint256 public constant K = 1;
            struct S { uint256 field; }
            struct GetterValue {
                uint256 visible;
                uint256 other;
                mapping(uint256 => uint256) hidden;
            }
            mapping(uint256 key => uint256 value) public getterMap;
            mapping(uint256 key => GetterValue value) public getterValues;
            constructor() {}
            fallback() external {}
            receive() external payable {}
            function f(uint256 y) public view returns (uint256 z) {
                uint256 local = x + y;
                return local;
            }
        }
        enum E { A }
        "#,
    );
    let path = project.path("/Symbols.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let result = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path, project.read_file("/Symbols.sol"))],
    ));

    assert!(result.diagnostics.is_empty());

    let declarations = result.symbol_tables.file_declarations(&uri).collect::<Vec<_>>();
    assert_declaration(&declarations, "TOP", SymbolKind::CONSTANT);
    assert_declaration(&declarations, "C", SymbolKind::CLASS);
    assert_declaration(&declarations, "x", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "K", SymbolKind::CONSTANT);
    assert_declaration(&declarations, "S", SymbolKind::STRUCT);
    assert_declaration(&declarations, "field", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "GetterValue", SymbolKind::STRUCT);
    assert_declaration(&declarations, "visible", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "other", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "hidden", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "getterMap", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "getterValues", SymbolKind::PROPERTY);
    assert_declaration(&declarations, "constructor", SymbolKind::CONSTRUCTOR);
    assert_declaration(&declarations, "fallback", SymbolKind::FUNCTION);
    assert_declaration(&declarations, "receive", SymbolKind::FUNCTION);
    assert_declaration(&declarations, "f", SymbolKind::METHOD);
    assert_declaration(&declarations, "y", SymbolKind::VARIABLE);
    assert_declaration(&declarations, "z", SymbolKind::VARIABLE);
    assert_declaration(&declarations, "local", SymbolKind::VARIABLE);
    assert_declaration(&declarations, "E", SymbolKind::ENUM);
    assert_declaration(&declarations, "A", SymbolKind::ENUM_MEMBER);

    assert_parent(&declarations, "x", "C");
    assert_parent(&declarations, "K", "C");
    assert_parent(&declarations, "field", "S");
    assert_parent(&declarations, "visible", "GetterValue");
    assert_parent(&declarations, "other", "GetterValue");
    assert_parent(&declarations, "hidden", "GetterValue");
    assert_parent(&declarations, "getterMap", "C");
    assert_parent(&declarations, "getterValues", "C");
    assert_parent(&declarations, "constructor", "C");
    assert_parent(&declarations, "y", "f");
    assert_parent(&declarations, "z", "f");
    assert_parent(&declarations, "local", "f");
    assert_parent(&declarations, "A", "E");

    assert_declaration_count(&declarations, "x", SymbolKind::PROPERTY, 1);
    assert_declaration_count(&declarations, "visible", SymbolKind::PROPERTY, 1);
    assert_declaration_count(&declarations, "other", SymbolKind::PROPERTY, 1);
    assert_no_declaration(&declarations, "key");
    assert_no_declaration(&declarations, "value");
    assert_no_declaration(&declarations, "__tmp_struct");
    assert_eq!(declarations.len(), result.symbol_tables.declarations().len());
}

#[test]
fn analyze_builds_lsp_symbol_responses() {
    let project = TestProject::from_fixture(
        r#"
        //- /Symbols.sol
        interface I {
            function iface(uint256 value) external;
        }
        library L {
            event Logged(uint256 value);
            function helper(uint256 value) internal pure returns (uint256 result) {
                return value;
            }
        }
        contract C {
            enum E { A, B }
            struct S { uint256 field; }
            uint256 public x;
            constructor() {}
            function f(uint256 y) public pure returns (uint256 z) {
                uint256 local = y;
                return local;
            }
        }
        "#,
    );
    let path = project.path("/Symbols.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let result = analyze(AnalysisBatch::from_files(
        CompileOpts::default(),
        [(path, project.read_file("/Symbols.sol"))],
    ));

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    let document_symbols = result.symbol_tables.document_symbols(&uri);
    assert_eq!(
        document_symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
        ["I", "L", "C"]
    );
    assert_eq!(document_symbols[0].kind, SymbolKind::INTERFACE);
    assert_eq!(document_symbols[1].kind, SymbolKind::MODULE);
    assert_eq!(document_symbols[2].kind, SymbolKind::CLASS);

    let contract = find_document_symbol(&document_symbols, "C");
    assert_eq!(child_names(contract), ["E", "S", "x", "constructor", "f"]);

    let enumm = find_document_child(contract, "E");
    assert_eq!(enumm.kind, SymbolKind::ENUM);
    assert_eq!(child_names(enumm), ["A", "B"]);

    let function = find_document_child(contract, "f");
    assert_eq!(function.kind, SymbolKind::METHOD);
    assert_eq!(child_names(function), ["y", "z", "local"]);

    let workspace_symbols = result.symbol_tables.workspace_symbols("helper");
    assert_eq!(
        workspace_symbols.iter().map(|symbol| symbol.name.as_str()).collect::<Vec<_>>(),
        ["helper"]
    );
    assert_eq!(workspace_symbols[0].kind, SymbolKind::METHOD);
    assert_eq!(workspace_symbols[0].container_name.as_deref(), Some("L"));

    let all_workspace_symbols = result.symbol_tables.workspace_symbols("");
    assert_eq!(find_workspace_symbol(&all_workspace_symbols, "I").kind, SymbolKind::INTERFACE);
    assert_eq!(find_workspace_symbol(&all_workspace_symbols, "L").kind, SymbolKind::MODULE);
    assert_eq!(find_workspace_symbol(&all_workspace_symbols, "C").kind, SymbolKind::CLASS);
}

fn assert_parent(declarations: &[&crate::symbols::DeclarationSymbol], name: &str, parent: &str) {
    let declaration = find_declaration(declarations, name);
    let parent_id = declaration.parent.unwrap_or_else(|| {
        panic!("declaration `{name}` has no parent in {declarations:#?}");
    });
    let parent_declaration = declarations
        .iter()
        .find(|candidate| candidate.id == parent_id)
        .unwrap_or_else(|| panic!("parent {parent_id:?} for `{name}` not found"));
    assert_eq!(parent_declaration.name, parent);
}

fn assert_declaration(
    declarations: &[&crate::symbols::DeclarationSymbol],
    name: &str,
    kind: SymbolKind,
) {
    assert!(
        declarations.iter().any(|symbol| symbol.name == name && symbol.kind == kind),
        "missing {kind:?} declaration `{name}` in {declarations:#?}"
    );
}

fn assert_declaration_count(
    declarations: &[&crate::symbols::DeclarationSymbol],
    name: &str,
    kind: SymbolKind,
    expected: usize,
) {
    assert_eq!(
        declarations.iter().filter(|symbol| symbol.name == name && symbol.kind == kind).count(),
        expected,
        "unexpected count for {kind:?} declaration `{name}` in {declarations:#?}",
    );
}

fn assert_no_declaration(declarations: &[&crate::symbols::DeclarationSymbol], name: &str) {
    assert!(
        declarations.iter().all(|symbol| symbol.name != name),
        "unexpected declaration `{name}` in {declarations:#?}",
    );
}

fn find_declaration<'a>(
    declarations: &'a [&crate::symbols::DeclarationSymbol],
    name: &str,
) -> &'a crate::symbols::DeclarationSymbol {
    declarations
        .iter()
        .copied()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("missing declaration `{name}` in {declarations:#?}"))
}

fn find_document_symbol<'a>(symbols: &'a [DocumentSymbol], name: &str) -> &'a DocumentSymbol {
    symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("missing document symbol `{name}` in {symbols:#?}"))
}

fn find_document_child<'a>(symbol: &'a DocumentSymbol, child_name: &str) -> &'a DocumentSymbol {
    let children = symbol.children.as_deref().unwrap_or_else(|| {
        panic!("document symbol `{}` has no children", symbol.name);
    });
    find_document_symbol(children, child_name)
}

fn child_names(symbol: &DocumentSymbol) -> Vec<&str> {
    symbol.children.as_deref().unwrap_or_default().iter().map(|child| child.name.as_str()).collect()
}

fn find_workspace_symbol<'a>(symbols: &'a [WorkspaceSymbol], name: &str) -> &'a WorkspaceSymbol {
    symbols
        .iter()
        .find(|symbol| symbol.name == name)
        .unwrap_or_else(|| panic!("missing workspace symbol `{name}` in {symbols:#?}"))
}

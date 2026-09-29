use super::*;
use crate::{config::negotiate_capabilities, test_support::*};
use async_lsp::{ClientSocket, ErrorCode, ResponseError, router::Router};
use lsp_types::{
    Diagnostic, DidChangeConfigurationParams, DidChangeWorkspaceFoldersParams,
    DocumentDiagnosticParams, DocumentDiagnosticReport, DocumentDiagnosticReportResult,
    DocumentSymbol, FileChangeType, Position, Range, RenameParams, TextDocumentContentChangeEvent,
    WatchKind, WorkDoneProgress, WorkspaceFolder, WorkspaceFoldersChangeEvent, notification,
    notification::Notification, request,
};
use serde_json::{Value, json};
use std::{
    future::Future,
    path::Path,
    sync::{Barrier, mpsc as std_mpsc},
    task::{Context, Poll, Waker},
    time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot};

#[cfg(unix)]
use std::os::unix::fs::symlink;

mod call_hierarchy;
mod code_action;
mod code_lens;
mod compatibility_sessions;
mod completion;
mod completion_resolve;
mod definition_fast_path;
mod document_highlight;
mod document_link;
mod file_operations;
mod flycheck_freshness;
mod folding_range;
mod goto_definition;
mod hover;
mod implementation;
mod import_completion;
mod import_definition;
mod indexing;
mod inlay_hint;
mod interactive_analysis;
mod point_queries;
#[path = "protocol_trace.rs"]
mod protocol_trace_tests;
mod qualified_path_edges;
mod qualified_paths;
mod references;
mod refresh;
mod rename;
mod selection_range;
mod signature_help;
mod source_events;
mod support;
mod type_definition;
mod type_hierarchy;
mod watched_files;
mod workspace_diagnostic;

use support::{Query, RequestFixture, rename_output};

/// A state whose analysis progress starts without delay.
fn progress_state(harness: &ClientHarness) -> GlobalState {
    let mut state = GlobalState::new(harness.client().clone());
    state.analysis_progress = ProgressCoordinator::with_timing(
        harness.client().clone(),
        true,
        Duration::ZERO,
        Duration::from_secs(1),
    );
    state
}

fn snapshot(project: &TestProject) -> GlobalStateSnapshot {
    snapshot_with_config(project.config(), project.vfs())
}

fn config_with_indexing_excludes(project: &TestProject, excludes: &[&str]) -> Config {
    config_with_options(project.initialize_params(), json!({ "indexing": { "exclude": excludes } }))
}

fn config_with_options(params: InitializeParams, options: Value) -> Config {
    rediscovered_config(with_options(params, options))
}

fn snapshot_with_config(config: Config, vfs: Vfs) -> GlobalStateSnapshot {
    let (published_analysis_version, _) = watch::channel(1);
    GlobalStateSnapshot {
        client: ClientSocket::new_closed(),
        vfs: Arc::new(RwLock::new(vfs)),
        config: Arc::new(config),
        analysis_version: Arc::new(AtomicUsize::new(1)),
        published_analysis_version,
        analysis_commit: Arc::new(Default::default()),
        watched_file_registration: Arc::new(Default::default()),
        flycheck_versions: Arc::new(Default::default()),
        symbol_tables: Arc::new(Default::default()),
        diagnostics: Arc::new(Default::default()),
        #[cfg(any(test, feature = "bench"))]
        benchmark_threads: None,
    }
}

/// Analyzes `source` as the only file of a batch.
fn analyze_source(path: impl Into<PathBuf>, source: impl Into<String>) -> AnalysisResult {
    analyze(AnalysisBatch::from_files(CompileOpts::default(), [(path.into(), source.into())]))
}

/// Analyzes every non-empty batch of `snapshot` and merges the outputs.
fn analyze_workspace(snapshot: &GlobalStateSnapshot) -> AnalysisOutput {
    let mut outputs = AnalysisOutputAccumulator::default();
    for batch in snapshot.analysis_batches(Vec::new()) {
        if !batch.files.is_empty() {
            outputs.push(analyze_cancellable(batch, &Default::default()).unwrap());
        }
    }
    outputs.finish()
}

fn analyze_single_batch(snapshot: &GlobalStateSnapshot) -> AnalysisResult {
    let mut batches = snapshot.analysis_batches(Vec::new());
    assert_eq!(batches.len(), 1);
    analyze(batches.pop().unwrap())
}

fn workspace_symbol_names(tables: &SymbolTables) -> Vec<String> {
    let mut names =
        tables.workspace_symbols("").into_iter().map(|symbol| symbol.name).collect::<Vec<_>>();
    names.sort_unstable();
    names
}

fn analysis_version(state: &GlobalState) -> usize {
    state.analysis_version.load(Ordering::Acquire)
}

fn cancel_analysis(state: &GlobalState) {
    state.analysis_scheduler.tasks.lock().cancel();
}

/// Runs `notify` on another thread while the watched-file specs are locked, and checks that it
/// advances the analysis epoch before it waits to reregister watchers.
fn assert_epoch_advances_before_reregistration(
    mut state: GlobalState,
    notify: impl FnOnce(&mut GlobalState) + Send + 'static,
) {
    let initial_version = analysis_version(&state);
    let registration = state.watched_file_registration.clone();
    let desired_specs = registration.desired_specs.lock();
    let version = state.analysis_version.clone();
    let runtime = tokio::runtime::Handle::current();
    let worker = std::thread::spawn(move || {
        let _runtime = runtime.enter();
        notify(&mut state);
        state
    });

    let deadline = Instant::now() + Duration::from_millis(100);
    while version.load(Ordering::Acquire) == initial_version && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let advanced_before_reregistration = version.load(Ordering::Acquire) != initial_version;

    drop(desired_specs);
    cancel_analysis(&worker.join().unwrap());
    assert!(
        advanced_before_reregistration,
        "watchers were queued before the old analysis epoch was invalidated"
    );
}

fn begin_recompute(
    state: &mut GlobalState,
    removed_paths: Vec<PathBuf>,
    trigger: AnalysisTrigger,
) -> (usize, ProgressTicket) {
    state.begin_analysis(AnalysisMode::Recompute, removed_paths, Vec::new(), trigger).unwrap()
}

fn begin_rediscovery(state: &mut GlobalState) -> (usize, ProgressTicket) {
    state
        .begin_analysis(AnalysisMode::Rediscover, Vec::new(), Vec::new(), AnalysisTrigger::External)
        .unwrap()
}

fn discovery_ready(
    version: usize,
    result: WorkspaceDiscoveryResult,
    progress: ProgressTicket,
) -> WorkspaceDiscoveryReady {
    let cancellation = IndexingCancellation::default();
    WorkspaceDiscoveryReady { version, result, disk_paths: Vec::new(), progress, cancellation }
}

fn analysis_coordinator(state: &GlobalState) -> AbortHandle {
    state.analysis_scheduler.tasks.lock().coordinator.as_ref().unwrap().1.clone()
}

fn rename_params(uri: &Url, position: Position, new_name: &str) -> RenameParams {
    request_params(uri, position, json!({ "newName": new_name }))
}

fn range_edit(range: Range, text: &str) -> TextDocumentContentChangeEvent {
    TextDocumentContentChangeEvent { range: Some(range), range_length: None, text: text.into() }
}

fn diagnostic(message: &str) -> Diagnostic {
    Diagnostic::new_simple(Range::new(Position::new(0, 0), Position::new(0, 1)), message.into())
}

fn diagnostics_for(uri: &Url, message: &str) -> DiagnosticMap {
    DiagnosticMap::from_iter([(uri.clone(), vec![diagnostic(message)])])
}

fn diagnostic_messages(diagnostics: &[Diagnostic]) -> Vec<&str> {
    diagnostics.iter().map(|diagnostic| diagnostic.message.as_str()).collect()
}

/// Returns the stored diagnostics of `uri`, which must have a full pull report.
fn pulled_diagnostics(state: &GlobalState, uri: &Url) -> Vec<Diagnostic> {
    let PullReport::Full { diagnostics, .. } = state.diagnostics.read().pull_report(uri, None)
    else {
        panic!("expected a full diagnostic report");
    };
    diagnostics
}

fn diagnostic_uri() -> Url {
    Url::from_file_path(std::env::temp_dir().join("Diagnostics.sol")).unwrap()
}

fn document_diagnostic_params(
    uri: &Url,
    previous_result_id: Option<String>,
) -> DocumentDiagnosticParams {
    request_params(uri, Position::default(), json!({ "previousResultId": previous_result_id }))
}

fn pull_document_diagnostics(
    state: &mut GlobalState,
    uri: &Url,
    previous_result_id: Option<String>,
) -> DocumentDiagnosticReport {
    let params = document_diagnostic_params(uri, previous_result_id);
    let DocumentDiagnosticReportResult::Report(report) =
        expect_ready(crate::handlers::document_diagnostic(state, params)).unwrap()
    else {
        panic!("expected a document diagnostic report");
    };
    report
}

fn flycheck_owner(workspace: impl Into<PathBuf>) -> DiagnosticOwner {
    DiagnosticOwner::Flycheck { id: "slow".into(), workspace: workspace.into() }
}

fn document_symbol_output(symbols: &[DocumentSymbol], depth: usize, output: &mut String) {
    for symbol in symbols {
        output.push_str(&format!("{:depth$}{} {:?}\n", "", symbol.name, symbol.kind));
        document_symbol_output(symbol.children.as_deref().unwrap_or_default(), depth + 2, output);
    }
}

#[test]
fn analysis_result_accumulator_merges_multiple_batches() {
    let one_path = std::env::temp_dir().join("One.sol");
    let two_path = std::env::temp_dir().join("Two.sol");
    let one_uri = Url::from_file_path(&one_path).unwrap();
    let two_uri = Url::from_file_path(&two_path).unwrap();
    let mut first = analyze_source(one_path, "contract One {}");
    let mut second = analyze_source(two_path, "contract Two {}");
    second.analyzed_documents.insert(one_uri.clone(), Some(7));
    let uri = diagnostic_uri();
    first.diagnostics = diagnostics_for(&uri, "first");
    second.diagnostics = diagnostics_for(&uri, "second");
    let mut results = AnalysisResultAccumulator::default();

    results.push(first);
    results.push(second);
    let result = results.finish();

    assert_eq!(diagnostic_messages(&result.diagnostics[&uri]), ["first", "second"]);
    assert_eq!(workspace_symbol_names(&result.symbol_tables), ["One", "Two"]);
    assert_eq!(
        result.analyzed_documents,
        AnalyzedDocuments::from_iter([(one_uri, Some(7)), (two_uri, None)])
    );
}

#[test]
fn analysis_batch_from_files_tracks_unique_sorted_paths() {
    let a = PathBuf::from("a.sol");
    let b = PathBuf::from("b.sol");
    let batch = AnalysisBatch::from_files(
        CompileOpts::default(),
        [
            (b.clone(), "contract B {}".into()),
            (a.clone(), "contract A {}".into()),
            (b.clone(), "contract Duplicate {}".into()),
        ],
    );

    assert_eq!(
        batch.files,
        [
            (a.clone(), Arc::new("contract A {}".into())),
            (b.clone(), Arc::new("contract B {}".into()))
        ]
    );
    assert_eq!(batch.seen_paths, FxHashSet::from_iter([a, b]));
}

#[test]
fn analysis_collects_open_versions_and_loaded_dependencies() {
    let project = TestProject::from_fixture(
        r#"
        //- /Main.sol open
        import "./Dependency.sol";
        contract Main is Dependency {}

        //- /Dependency.sol
        contract Dependency {}
        "#,
    );
    let main_uri = project.uri("/Main.sol");
    let dependency_uri = project.uri("/Dependency.sol");

    let result = analyze_single_batch(&snapshot_with_config(Config::default(), project.vfs()));

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    assert_eq!(
        result.analyzed_documents,
        AnalyzedDocuments::from_iter([(main_uri, Some(0)), (dependency_uri, None)])
    );
}

#[tokio::test(flavor = "current_thread")]
async fn analysis_updates_refresh_code_lenses_only_when_active() {
    let mut harness = ClientHarness::new();
    let mut state = GlobalState::new(harness.client().clone());
    for enable in [true, false] {
        let params = from_json(json!({
            "capabilities": { "workspace": { "codeLens": { "refreshSupport": true } } },
            "initializationOptions": { "codeLens": { "enable": enable } },
        }));
        state.config = Arc::new(negotiate_capabilities(params).1);
        let version = analysis_version(&state);

        assert!(state.snapshot().publish_analysis(version, AnalysisResult::default()));
        if enable {
            assert_eq!(harness.next_event().await, ClientEvent::CodeLensRefresh);
        }
        state.clear_analysis_cache();
        if enable {
            assert_eq!(harness.next_event().await, ClientEvent::CodeLensRefresh);
        }
        harness.expect_no_event().await;
    }
    harness.exit().await;
}

#[test]
fn document_diagnostic_merges_owners_and_canonicalizes_file_uris() {
    let canonical_uri = diagnostic_uri();
    for spelling in ["Diagnostics.sol", "%44iagnostics.sol", "nested%2F..%2FDiagnostics.sol"] {
        let uri =
            Url::parse(&canonical_uri.as_str().replacen("Diagnostics.sol", spelling, 1)).unwrap();
        assert_eq!(crate::proto::vfs_path(&canonical_uri), crate::proto::vfs_path(&uri));
        let mut state = GlobalState::new(ClientSocket::new_closed());
        let mut snapshot = state.snapshot();
        snapshot.publish_diagnostics(
            DiagnosticOwner::Compiler,
            diagnostics_for(&canonical_uri, "compiler"),
        );
        snapshot.publish_diagnostics(
            flycheck_owner("/workspace"),
            diagnostics_for(&canonical_uri, "lint"),
        );

        let DocumentDiagnosticReport::Full(report) =
            pull_document_diagnostics(&mut state, &uri, None)
        else {
            panic!("first diagnostic pull should return a full report");
        };
        assert_eq!(report.related_documents, None);
        let report = report.full_document_diagnostic_report;
        assert_eq!(diagnostic_messages(&report.items), ["compiler", "lint"]);
        let result_id = report.result_id.unwrap();

        let DocumentDiagnosticReport::Unchanged(report) =
            pull_document_diagnostics(&mut state, &canonical_uri, Some(result_id.clone()))
        else {
            panic!("equivalent URI should share the cached result ID");
        };
        assert_eq!(report.related_documents, None);
        assert_eq!(report.unchanged_document_diagnostic_report.result_id, result_id);
    }
}

#[test]
fn document_diagnostic_waits_for_committed_analysis_diagnostics() {
    let uri = diagnostic_uri();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();

    let mut request = std::pin::pin!(crate::handlers::document_diagnostic(
        &mut state,
        document_diagnostic_params(&uri, None),
    ));
    let mut context = Context::from_waker(Waker::noop());
    assert!(request.as_mut().poll(&mut context).is_pending());

    let result =
        AnalysisResult { diagnostics: diagnostics_for(&uri, "current"), ..Default::default() };
    assert!(state.snapshot().publish_analysis(1, result));

    let Poll::Ready(Ok(DocumentDiagnosticReportResult::Report(DocumentDiagnosticReport::Full(
        report,
    )))) = request.as_mut().poll(&mut context)
    else {
        panic!("diagnostic pull should return a full report after analysis is published");
    };
    assert_eq!(report.full_document_diagnostic_report.items, vec![diagnostic("current")]);
}

fn assert_analysis_stale_before_diagnostic_publication(
    mut state: GlobalState,
    stale_version: usize,
    advance_epoch: impl FnOnce(&mut GlobalState) + Send + 'static,
) {
    let stale_snapshot = state.snapshot();
    assert!(stale_snapshot.is_current(stale_version));

    let diagnostics = state.diagnostics.clone();
    let diagnostics_guard = diagnostics.write();
    let start = Arc::new(Barrier::new(2));
    let worker_start = start.clone();
    let (finished_tx, finished_rx) = std_mpsc::sync_channel(1);
    let worker = std::thread::spawn(move || {
        worker_start.wait();
        advance_epoch(&mut state);
        finished_tx.send(()).unwrap();
    });

    start.wait();
    let deadline = Instant::now() + TIMEOUT;
    while stale_snapshot.is_current(stale_version) && Instant::now() < deadline {
        std::thread::yield_now();
    }
    let stale_while_diagnostics_locked = !stale_snapshot.is_current(stale_version);
    let finished_while_diagnostics_locked =
        !matches!(finished_rx.try_recv(), Err(std_mpsc::TryRecvError::Empty));

    drop(diagnostics_guard);
    finished_rx
        .recv_timeout(TIMEOUT)
        .expect("epoch advance should finish after diagnostic publication is unblocked");
    worker.join().unwrap();

    assert!(!finished_while_diagnostics_locked, "diagnostic lock should block publication");
    assert!(
        stale_while_diagnostics_locked,
        "old analysis should be stale before diagnostic publication"
    );
}

#[test]
fn replacement_analysis_invalidates_old_worker_before_removed_diagnostics_publish() {
    let uri = diagnostic_uri();
    let path = uri.to_file_path().unwrap();
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let (stale_version, _stale_progress) =
        begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
    state
        .snapshot()
        .publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "removed"));

    assert_analysis_stale_before_diagnostic_publication(state, stale_version, move |state| {
        state
            .begin_analysis(
                AnalysisMode::Recompute,
                vec![path],
                Vec::new(),
                AnalysisTrigger::Document,
            )
            .expect("replacement analysis should start");
    });
}

#[test]
fn clearing_analysis_cache_invalidates_old_worker_before_diagnostics_publish() {
    let uri = diagnostic_uri();
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let stale_version = analysis_version(&state);
    state
        .snapshot()
        .publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "cleared"));

    assert_analysis_stale_before_diagnostic_publication(
        state,
        stale_version,
        GlobalState::clear_analysis_cache,
    );
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_publishes_an_empty_snapshot_before_ending_progress() {
    let project = TestProject::from_fixture("//- /Cached.sol\ncontract Cached {}\n");
    let old_tables = analyze_single_batch(&snapshot(&project)).symbol_tables;
    assert_eq!(workspace_symbol_names(&old_tables), ["Cached"]);
    let mut harness = ClientHarness::new();
    let mut state = progress_state(&harness);
    state.symbol_tables.store(Arc::new(old_tables));
    let compiler_only = Url::parse("file:///workspace/CompilerOnly.sol").unwrap();
    let shared = Url::parse("file:///workspace/Shared.sol").unwrap();
    let mut snapshot = state.snapshot();
    snapshot.publish_diagnostics(
        DiagnosticOwner::Compiler,
        DiagnosticMap::from_iter([
            (compiler_only.clone(), vec![diagnostic("compiler only")]),
            (shared.clone(), vec![diagnostic("compiler shared")]),
        ]),
    );
    snapshot
        .publish_diagnostics(flycheck_owner("/workspace"), diagnostics_for(&shared, "flycheck"));
    for _ in 0..3 {
        harness.next_published().await;
    }

    let (_, progress) = begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
    progress.begin();
    progress.report("Analyzing workspace");
    let token = harness.expect_progress_begin(Some("Analyzing workspace")).await;

    state.clear_analysis_cache();

    let mut cleared = [harness.next_published().await, harness.next_published().await];
    cleared.sort_by(|lhs, rhs| lhs.uri.as_str().cmp(rhs.uri.as_str()));
    assert_eq!(cleared[0].uri, compiler_only);
    assert!(cleared[0].diagnostics.is_empty());
    assert_eq!(cleared[1].uri, shared);
    assert_eq!(diagnostic_messages(&cleared[1].diagnostics), ["flycheck"]);
    harness.expect_progress_end(&token, "Workspace index cleared").await;
    assert!(settle(&state).await.load().workspace_symbols("").is_empty());
    assert!(state.analysis_cache_invalidated());

    harness.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn clearing_analysis_cache_suppresses_progress_pending_creation() {
    let uri = diagnostic_uri();
    let mut harness = ClientHarness::new();
    let mut state = progress_state(&harness);
    state.snapshot().publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "old"));
    harness.next_published().await;

    let (_, progress) = begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
    progress.begin();
    harness.expect_create().await;
    progress.report("obsolete analysis");
    progress.finish("obsolete completion");

    state.clear_analysis_cache();

    let cleared = harness.next_published().await;
    assert_eq!(cleared.uri, uri);
    assert!(cleared.diagnostics.is_empty());
    harness.acknowledge_create();
    harness.assert_silent().await;

    harness.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn superseded_analysis_cannot_publish_or_end_latest_progress() {
    let uri = diagnostic_uri();
    let mut harness = ClientHarness::new();
    let mut state = progress_state(&harness);

    let (stale_version, stale_progress) =
        begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
    let mut stale_snapshot = state.snapshot();
    stale_progress.begin();
    let token = harness.expect_progress_begin(None).await;

    let (latest_version, latest_progress) =
        begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
    let mut latest_snapshot = state.snapshot();
    let WorkDoneProgress::Report(report) = harness.expect_progress(&token).await else {
        panic!("expected replacement report");
    };
    assert_eq!(report.message.as_deref(), Some("Workspace changed, restarting analysis"));

    stale_progress.report("stale report");
    stale_progress.finish("stale end");
    let stale_result =
        AnalysisResult { diagnostics: diagnostics_for(&uri, "stale"), ..Default::default() };
    assert!(!stale_snapshot.publish_analysis(stale_version, stale_result));
    harness.assert_silent().await;

    let latest_result =
        AnalysisResult { diagnostics: diagnostics_for(&uri, "current"), ..Default::default() };
    assert!(latest_snapshot.publish_analysis(latest_version, latest_result));
    let published = harness.next_published().await;
    assert_eq!(published.uri, uri);
    assert_eq!(diagnostic_messages(&published.diagnostics), ["current"]);

    finish_analysis_progress_if_current(
        latest_version,
        &state.analysis_version,
        &state.analysis_commit,
        &latest_progress,
        "Workspace index ready",
    );
    harness.expect_progress_end(&token, "Workspace index ready").await;

    harness.exit().await;
}

#[test]
fn clearing_analysis_cache_rejects_older_analysis_results() {
    let project = TestProject::from_fixture("//- /Stale.sol\ncontract Stale {}\n");
    let mut stale_result = analyze_single_batch(&snapshot(&project));
    let uri = project.uri("/Stale.sol");
    stale_result.diagnostics = diagnostics_for(&uri, "stale compiler");
    let mut state = GlobalState::new(ClientSocket::new_closed());
    state.mark_analysis_pending_for_test();
    let mut stale_snapshot = state.snapshot();

    state.clear_analysis_cache();

    assert!(!stale_snapshot.publish_analysis(1, stale_result));
    assert!(state.symbol_tables.load().workspace_symbols("").is_empty());
    assert!(pulled_diagnostics(&state, &uri).is_empty());
}

#[test]
fn reindex_if_invalidated_is_a_no_op_for_a_current_cache() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let version = analysis_version(&state);

    state.reindex_if_invalidated();

    assert_eq!(analysis_version(&state), version);
    assert!(!state.analysis_cache_invalidated());
}

#[tokio::test(flavor = "current_thread")]
async fn failed_current_analysis_ends_visible_progress() {
    let mut harness = ClientHarness::new();
    let mut state = progress_state(&harness);
    let (version, progress) = begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
    progress.begin();
    let token = harness.expect_progress_begin(None).await;

    let task = tokio::spawn(async { panic!("test analysis failure") });
    state.monitor_analysis_task(version, task, progress);
    settle(&state).await;
    assert!(state.analysis_cache_invalidated());
    harness.expect_progress_end(&token, "Workspace indexing failed").await;

    harness.exit().await;
}

#[tokio::test(flavor = "current_thread")]
async fn failed_or_cancelled_analysis_keeps_results_until_save_recovers() {
    for cancelled in [false, true] {
        let project = TestProject::from_fixture("//- /Old.sol\ncontract Old {}\n");
        let old_tables = analyze_single_batch(&snapshot(&project)).symbol_tables;
        let uri = project.uri("/Old.sol");
        let mut state = state_with(project.config());
        state.symbol_tables.store(Arc::new(old_tables));
        state
            .snapshot()
            .publish_diagnostics(DiagnosticOwner::Compiler, diagnostics_for(&uri, "old compiler"));

        let (version, progress) =
            begin_recompute(&mut state, Vec::new(), AnalysisTrigger::Document);
        let task = if cancelled {
            let task = tokio::spawn(std::future::pending::<AnalysisTaskOutcome>());
            task.abort();
            task
        } else {
            tokio::spawn(async { panic!("test analysis failure") })
        };
        state.monitor_analysis_task(version, task, progress);

        assert_eq!(workspace_symbol_names(&settle(&state).await.load()), ["Old"]);
        assert!(state.analysis_cache_invalidated());
        assert!(!state.natspec_semantics_are_usable(&uri));
        assert_eq!(pulled_diagnostics(&state, &uri), [diagnostic("old compiler")]);

        project.write_file("/Old.sol", "contract Recovered {}");
        save(&mut state, &uri);

        assert_eq!(workspace_symbol_names(&settle(&state).await.load()), ["Recovered"]);
        assert!(!state.analysis_cache_invalidated());
        assert!(state.natspec_semantics_are_usable(&uri));
    }
}

#[tokio::test(flavor = "current_thread")]
async fn reindex_rediscovers_disk_files_without_preclearing_the_old_index() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Old.sol
        contract Old {}
        "#,
    );
    let config = project.config();
    let old_tables =
        analyze_single_batch(&snapshot_with_config(config.clone(), Vfs::default())).symbol_tables;
    project.remove_file("/src/Old.sol");
    project.write_file("/src/New.sol", "contract New {}");

    let mut state = state_with(config);
    state.symbol_tables.store(Arc::new(old_tables));
    state.reindex();

    assert!(state.analysis_commit.lock().external_refresh.is_some());
    assert_eq!(workspace_symbol_names(&state.symbol_tables.load()), ["Old"]);
    assert_eq!(workspace_symbol_names(&settle(&state).await.load()), ["New"]);
}

#[tokio::test(flavor = "current_thread")]
async fn source_notification_after_clear_rediscovers_disk_files_and_preserves_overlays() {
    for saved in [true, false] {
        let mut project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Open.sol
            contract DiskVersion {}
            "#,
        );
        project.open_file("/src/Open.sol", "contract Unsaved {}");
        let mut state = project.state();
        project.write_file("/src/New.sol", "contract New {}");
        let uri = project.uri("/src/Open.sol");
        state.clear_analysis_cache();

        if saved {
            save(&mut state, &uri);
        } else {
            let no_op = TextDocumentContentChangeEvent {
                range_length: Some(0),
                ..range_edit(Range::default(), "")
            };
            edit(&mut state, &uri, 1, vec![no_op]);
        }

        assert_eq!(workspace_symbol_names(&settle(&state).await.load()), ["New", "Unsaved"]);
    }
}

/// Checks that a source notification keeps only its own NatSpec usable until analysis publishes.
fn check_source_notification(
    open: bool,
    external_refresh: bool,
    notify: impl FnOnce(&mut GlobalState, Url),
) {
    let project = TestProject::from_fixture(&format!(
        "//- /Request.sol{}\ncontract Request {{}}\n",
        if open { " open" } else { "" }
    ));
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();
    let other_uri = project.uri("/OtherRequest.sol");
    with_paused_blocking_pool(|release_worker| async move {
        let mut state = project.state();
        notify(&mut state, uri.clone());
        {
            let commit = state.analysis_commit.lock();
            assert_eq!(commit.external_refresh.is_some(), external_refresh);
            assert!(commit.natspec_pending_source_changes.contains(&path));
        }
        assert!(state.natspec_semantics_are_usable(&uri));
        assert!(!state.natspec_semantics_are_usable(&other_uri));

        release_worker.send(()).unwrap();
        settle(&state).await;
        assert!(state.analysis_commit.lock().natspec_pending_source_changes.is_empty());
        assert!(state.natspec_semantics_are_usable(&other_uri));
    });
}

#[test]
fn source_notifications_track_the_source_until_analysis_publishes() {
    check_source_notification(false, false, |state, uri| {
        open(state, &uri, 1, "contract Request {}");
    });
    check_source_notification(true, false, |state, uri| close(state, &uri));
    check_source_notification(true, false, |state, uri| {
        change(state, &uri, 1, "contract After {}");
    });
    check_source_notification(false, true, |state, uri| {
        watch_files(state, [(&uri.to_file_path().unwrap(), FileChangeType::CHANGED)]);
    });
}

/// Checks that a context notification refreshes externally and blocks NatSpec until published.
fn check_context_notification(project: &TestProject, notify: impl FnOnce(&mut GlobalState)) {
    let request_uri = project.uri("/Request.sol");
    with_paused_blocking_pool(|release_worker| async move {
        let mut state = project.state();
        notify(&mut state);
        assert!(state.analysis_commit.lock().external_refresh.is_some());
        assert!(!state.natspec_semantics_are_usable(&request_uri));

        release_worker.send(()).unwrap();
        settle(&state).await;
        assert!(state.analysis_commit.lock().external_refresh.is_none());
        assert!(state.natspec_semantics_are_usable(&request_uri));
    });
}

#[test]
fn context_notifications_track_external_refresh_until_analysis_publishes() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Request.sol
        contract Request {}
        "#,
    );
    let manifest = project.path("/foundry.toml");
    check_context_notification(&project, |state| {
        watch_files(state, [(&manifest, FileChangeType::CHANGED)]);
    });

    let added_path = project.path("/added");
    std::fs::create_dir(&added_path).unwrap();
    let added =
        WorkspaceFolder { uri: Url::from_file_path(added_path).unwrap(), name: "added".into() };
    check_context_notification(&project, |state| {
        let event = WorkspaceFoldersChangeEvent { added: vec![added], removed: Vec::new() };
        let params = DidChangeWorkspaceFoldersParams { event };
        assert!(crate::handlers::did_change_workspace_folders(state, params).is_continue());
    });

    check_context_notification(&project, |state| {
        let params = DidChangeConfigurationParams { settings: Value::Null };
        assert!(crate::handlers::did_change_configuration(state, params).is_continue());
    });
}

#[test]
fn watched_solidity_change_ignores_open_document() {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Request {}\n");
    let mut state = project.state();
    let version = analysis_version(&state);

    watch_files(&mut state, [(&project.path("/Request.sol"), FileChangeType::CHANGED)]);

    assert_eq!(analysis_version(&state), version);
    assert!(state.analysis_scheduler.tasks.lock().coordinator.is_none());
}

#[tokio::test(flavor = "current_thread")]
async fn did_change_clamps_positions_before_analysis_and_rename() {
    // Position clamping itself is unit-tested in `proto`; cover the first-line and indexed paths.
    for (position, text, expected, rename_position) in [
        (
            Position::new(0, u32::MAX),
            "\n// inserted",
            "//😀\n// inserted\ncontract C {}",
            Position::new(2, 9),
        ),
        (
            Position::new(u32::MAX, u32::MAX),
            "\ncontract Added {}",
            "//😀\ncontract C {}\ncontract Added {}",
            Position::new(1, 9),
        ),
    ] {
        let fixture = support::RequestFixture::new(
            "//- /Clamped.sol open\n//😀\ncontract $1C {}",
            "/Clamped.sol",
        );
        let (mut state, mut rename_params) = fixture.rename_state_and_params("$1", "Renamed");
        let uri = rename_params.text_document_position.text_document.uri.clone();
        let path = VfsPath::from(fixture.project_path("/Clamped.sol"));
        edit(&mut state, &uri, 2, vec![range_edit(Range::new(position, position), text)]);
        {
            let vfs = state.vfs.read();
            snapbox::assert_data_eq!(vfs.get_file_contents(&path).unwrap().to_string(), expected);
            assert_eq!(vfs.get_file_version(&path), Some(2));
        }

        rename_params.text_document_position.position = rename_position;
        let edit = within("rename", crate::handlers::rename(&mut state, rename_params))
            .await
            .unwrap()
            .expect("the current contract should remain renamable");
        assert_eq!(
            edit.changes.unwrap()[&uri],
            [lsp_types::TextEdit::new(
                Range::new(
                    rename_position,
                    Position::new(rename_position.line, rename_position.character + 1),
                ),
                "Renamed".into(),
            )]
        );
    }
}

#[tokio::test(flavor = "current_thread")]
async fn did_change_rejects_invalid_ranges_without_applying_a_partial_batch() {
    for invalid_range in [
        Range::new(Position::new(0, 3), Position::new(0, 3)),
        Range::new(Position::new(0, 4), Position::new(0, 2)),
    ] {
        let fixture = support::RequestFixture::new(
            "//- /Invalid.sol open\n//😀\ncontract $1C {}",
            "/Invalid.sol",
        );
        let mut state = fixture.state();
        let (uri, _) = fixture.marker_location("$1");
        let path = VfsPath::from(fixture.project_path("/Invalid.sol"));
        let valid_range = Range::new(Position::new(1, 9), Position::new(1, 10));
        let changes = vec![range_edit(valid_range, "D"), range_edit(invalid_range, "invalid")];
        edit(&mut state, &uri, 2, changes);
        let vfs = state.vfs.read();
        snapbox::assert_data_eq!(
            vfs.get_file_contents(&path).unwrap().to_string(),
            "//😀\ncontract C {}"
        );
        assert_eq!(vfs.get_file_version(&path), Some(0));
    }
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn did_open_schedules_analysis_without_source_change_debounce() {
    let project = TestProject::from_fixture("//- /Request.sol\ncontract Request {}\n");
    let uri = project.uri("/Request.sol");
    let mut state = project.state();
    let start = tokio::time::Instant::now();
    let scheduler_tick = Duration::from_millis(1);

    open(&mut state, &uri, 1, project.read_file("/Request.sol"));
    let coordinator = analysis_coordinator(&state);
    state.analysis_scheduler.gate.close();

    tokio::time::sleep(scheduler_tick).await;

    assert_eq!(tokio::time::Instant::now(), start + scheduler_tick);
    assert!(coordinator.is_finished(), "open analysis should reach the scheduler immediately");
}

#[tokio::test(flavor = "current_thread")]
async fn source_change_debounce_does_not_count_toward_progress_delay() {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Before {}\n");
    let uri = project.uri("/Request.sol");
    let mut harness = ClientHarness::new();
    let mut state = progress_state(&harness);
    state.config = Arc::new(project.config());
    *state.vfs.write() = project.vfs();

    change(&mut state, &uri, 1, "contract After {}");

    tokio::time::sleep(state.config.source_change_debounce() / 2).await;
    harness.assert_silent().await;

    state.clear_analysis_cache();
    harness.exit().await;
}

#[tokio::test(flavor = "current_thread", start_paused = true)]
async fn rapid_did_changes_restart_the_full_source_change_debounce() {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Before {}\n");
    let uri = project.uri("/Request.sol");
    let mut state = project.state();
    state.config = Arc::new(config_with_options(
        project.initialize_params(),
        json!({ "sourceChangeDebounce": 75 }),
    ));
    let debounce = Duration::from_millis(75);
    let final_tick = Duration::from_millis(1);
    let almost_debounce = debounce - final_tick;

    change(&mut state, &uri, 1, "contract First {}");
    let first_coordinator = analysis_coordinator(&state);
    tokio::time::sleep(almost_debounce).await;

    change(&mut state, &uri, 2, "contract Latest {}");
    let latest_coordinator = analysis_coordinator(&state);
    state.analysis_scheduler.gate.close();

    tokio::time::sleep(almost_debounce).await;
    assert!(first_coordinator.is_finished());
    assert!(!latest_coordinator.is_finished(), "latest change should wait for the full debounce");

    tokio::time::sleep(final_tick * 2).await;
    assert!(latest_coordinator.is_finished());
}

#[tokio::test(flavor = "current_thread")]
async fn rapid_did_changes_debounce_to_the_latest_source() {
    let project = TestProject::from_fixture("//- /Request.sol open\ncontract Before {}\n");
    let uri = project.uri("/Request.sol");
    let mut state = project.state();

    change(&mut state, &uri, 1, "contract Intermediate {}");
    let first_coordinator = analysis_coordinator(&state);
    change(&mut state, &uri, 2, "contract Latest {}");
    let debounce = state.config.source_change_debounce();

    assert!(
        tokio::time::timeout(debounce / 2, state.latest_analysis()).await.is_err(),
        "analysis should remain pending during the debounce window"
    );
    within("the replaced coordinator", async {
        while !first_coordinator.is_finished() {
            tokio::task::yield_now().await;
        }
    })
    .await;

    assert_eq!(workspace_symbol_names(&settle(&state).await.load()), ["Latest"]);
}

#[test]
fn pending_changes_limit_natspec_semantics() {
    let project = TestProject::new();
    let path = project.path("/Request.sol");
    let uri = Url::from_file_path(&path).unwrap();

    // A pending request source defers to the target-specific lookup, even for an equivalent URI.
    let equivalent_uri =
        Url::parse(&uri.as_str().replacen("Request.sol", "%52equest.sol", 1)).unwrap();
    assert_ne!(uri, equivalent_uri);
    assert_eq!(uri.to_file_path(), equivalent_uri.to_file_path());
    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_source_analysis_pending_for_test(path);
    assert!(state.natspec_semantics_are_usable(&equivalent_uri));

    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_source_analysis_pending_for_test(project.path("/Missing.sol"));
    assert!(!state.natspec_semantics_are_usable(&uri));

    let state = GlobalState::new(ClientSocket::new_closed());
    state.mark_context_analysis_pending_for_test();
    assert!(!state.natspec_semantics_are_usable(&uri));
}

#[test]
fn publishing_current_epoch_clears_pending_source_changes() {
    let project = TestProject::new();
    let mut snapshot = snapshot(&project);
    snapshot
        .analysis_commit
        .lock()
        .natspec_pending_source_changes
        .extend([project.path("/First.sol"), project.path("/Second.sol")]);

    assert!(snapshot.publish_symbol_tables(1, Default::default()));

    let commit = snapshot.analysis_commit.lock();
    assert_eq!(commit.symbol_tables_version, 1);
    assert!(commit.natspec_pending_source_changes.is_empty());
}

#[test]
fn beginning_analysis_epoch_waits_for_analysis_commit() {
    let state = GlobalState::new(ClientSocket::new_closed());
    let commit = state.analysis_commit.lock();
    let (started_tx, started_rx) = std::sync::mpsc::channel();
    let (finished_tx, finished_rx) = std::sync::mpsc::channel();

    std::thread::scope(|scope| {
        scope.spawn(|| {
            started_tx.send(()).unwrap();
            state.mark_analysis_pending_for_test();
            finished_tx.send(()).unwrap();
        });
        started_rx.recv().unwrap();
        let finished_while_locked = finished_rx.recv_timeout(Duration::from_millis(100)).is_ok();
        drop(commit);
        if !finished_while_locked {
            finished_rx
                .recv_timeout(Duration::from_secs(5))
                .expect("analysis epoch should begin after commit unlocks");
        }
        assert!(!finished_while_locked, "analysis epoch bypassed analysis commit");
    });
}

#[test]
fn flycheck_epochs_advance_only_for_affected_owners() {
    let mut state = GlobalState::new(ClientSocket::new_closed());
    let snapshot = state.snapshot();
    let owner = flycheck_owner("/workspace");
    let (cancel, mut cancelled) = oneshot::channel();
    state.flycheck_cancels.insert(flycheck_owner("/other"), cancel);

    state.run_flychecks_on_save(PathBuf::from("/workspace/Untracked.sol"));
    state.clear_removed_flycheck_diagnostics(Vec::new());
    assert!(snapshot.is_current_flycheck(&owner, 0));

    state.clear_removed_flycheck_diagnostics([owner.clone()]);
    assert!(!snapshot.is_current_flycheck(&owner, 0));
    assert!(matches!(cancelled.try_recv(), Err(oneshot::error::TryRecvError::Empty)));
}

#[tokio::test(flavor = "current_thread")]
async fn saving_equivalent_file_uri_selects_workspace_flycheck() {
    let project = TestProject::from_fixture(
        r#"
        //- /workspace/foundry.toml
        [profile.default]
        src = "src"
        //- /workspace/src/Test.sol
        contract Test {}
        "#,
    );
    let config = config_with_options(
        project.initialize_params_with_roots(&["/workspace"]),
        json!({
            "flychecks": [{
                "id": "save",
                "command": std::env::current_exe().unwrap(),
                "args": ["--list"]
            }]
        }),
    );
    let [owner] = config.flycheck_owners().collect::<Vec<_>>().try_into().unwrap();
    let mut state = state_with(config);
    let snapshot = state.snapshot();
    let uri = Url::parse(&format!(
        "{}/missing%2F..%2Fworkspace/src/Test.sol",
        Url::from_file_path(project.root()).unwrap()
    ))
    .unwrap();

    save(&mut state, &uri);

    assert!(!snapshot.is_current_flycheck(&owner, 0));
}

#[tokio::test(flavor = "current_thread")]
async fn recomputing_for_removed_files_stales_all_flycheck_owners() {
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
    let mut state = state_with(config_with_options(
        project.initialize_params_with_roots(&["/first", "/second"]),
        json!({ "flychecks": [{ "id": "slow", "command": "slow" }] }),
    ));
    let mut snapshot = state.snapshot();
    let first_owner = flycheck_owner(project.path("/first"));
    let second_owner = flycheck_owner(project.path("/second"));
    let deleted_path = project.path("/first/src/Deleted.sol");
    let uri = project.uri("/first/src/Deleted.sol");
    snapshot.publish_flycheck_diagnostics(
        second_owner.clone(),
        0,
        diagnostics_for(&uri, "existing"),
    );
    assert_eq!(diagnostic_messages(&pulled_diagnostics(&state, &uri)), ["existing"]);

    state.recompute_for_file_changes(vec![deleted_path.clone()], vec![deleted_path], false);

    assert!(!snapshot.is_current_flycheck(&first_owner, 0));
    assert!(!snapshot.is_current_flycheck(&second_owner, 0));
    assert!(pulled_diagnostics(&state, &uri).is_empty());
    snapshot.publish_flycheck_diagnostics(first_owner, 0, diagnostics_for(&uri, "stale"));
    snapshot.publish_flycheck_diagnostics(second_owner, 0, diagnostics_for(&uri, "stale other"));
    assert!(pulled_diagnostics(&state, &uri).is_empty());
    settle(&state).await;
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
    let mut state = state_with(config_with_options(
        project.initialize_params(),
        json!({
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
        }),
    ));

    state.run_flychecks_on_save(project.path("/src/Test.sol"));
    wait_for_path(&first_pid_path).await;
    let first_pid = project.read_file("/first-flycheck-pid.txt").parse().unwrap();

    state.run_flychecks_on_save(project.path("/src/Test.sol"));
    wait_for_path(&second_pid_path).await;
    for _ in 0..100 {
        if !process_exists(first_pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }

    assert!(!process_exists(first_pid));
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

/// Returns the first workspace's sources and the flycheck inputs for a file saved after discovery.
fn flycheck_source_paths(project: &TestProject, saved: &str) -> (Vec<PathBuf>, Vec<PathBuf>) {
    let config = config_with_options(
        project.initialize_params(),
        json!({ "flychecks": [{ "id": "custom", "command": "custom-lint" }] }),
    );
    let saved_path = project.path(saved);
    project.write_file(saved, "contract SavedAfterDiscovery {}\n");
    let [flycheck] = config.flychecks_for_path(&saved_path).try_into().unwrap();
    let source_files = config.workspaces()[0].source_files().to_vec();
    let state = state_with(config);
    (source_files, state.snapshot().flycheck_source_paths(&flycheck, &saved_path))
}

#[test]
fn flycheck_snapshot_includes_saved_file_default_inputs_and_nested_workspaces() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        //- /src/Tracked.sol
        contract Tracked {}
        //- /test/Tracked.t.sol
        contract TrackedTest {}
        //- /script/Tracked.s.sol
        contract TrackedScript {}
        //- /nested/foundry.toml
        [profile.default]
        src = "src"
        //- /nested/src/Nested.sol
        contract Nested {}
        "#,
    );

    let (_, paths) = flycheck_source_paths(&project, "/src/SavedAfterDiscovery.sol");

    assert_eq!(
        paths,
        [
            "/nested/src/Nested.sol",
            "/script/Tracked.s.sol",
            "/src/SavedAfterDiscovery.sol",
            "/src/Tracked.sol",
            "/test/Tracked.t.sol",
        ]
        .map(|path| project.path(path))
    );
}

#[test]
fn flycheck_snapshot_includes_all_configured_foundry_input_roots() {
    let project = TestProject::new();
    project.write_file(
        "/workspace/foundry.toml",
        &format!(
            "[profile.default]\nsrc = \"contracts\"\ntest = '{}'\nscript = \"deployments\"\n",
            project.path("/checks").display()
        ),
    );
    project.write_file("/workspace/contracts/Tracked.sol", "contract Tracked {}");
    project.write_file("/checks/Tracked.t.sol", "contract TrackedTest {}");
    project.write_file("/workspace/deployments/Tracked.s.sol", "contract TrackedScript {}");

    let (source_files, paths) =
        flycheck_source_paths(&project, "/checks/SavedAfterDiscovery.t.sol");

    let tracked = [
        "/checks/Tracked.t.sol",
        "/workspace/contracts/Tracked.sol",
        "/workspace/deployments/Tracked.s.sol",
    ]
    .map(|path| project.path(path));
    assert_eq!(source_files, tracked);
    let mut expected = tracked.to_vec();
    expected.insert(0, project.path("/checks/SavedAfterDiscovery.t.sol"));
    assert_eq!(paths, expected);
}

#[test]
fn analysis_batches_read_tracked_disk_files_and_skip_missing_ones() {
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

    let [batch] = snapshot(&project)
        .analysis_batches(vec![path.clone(), project.path("/src/Missing.sol")])
        .try_into()
        .ok()
        .unwrap();

    assert_eq!(
        batch.files,
        vec![(path, Arc::new("contract C { function f() public { number+; } }".into()))]
    );
}

#[test]
fn goto_implementation_finds_unopened_naked_workspace_files() {
    let marked = MarkedProject::from_fixture(
        r#"
        //- /Base.sol open
        interface Runner { function $1run() external; }
        //- /First.sol
        import "./Base.sol";
        contract First is Runner { function run() external {} }
        //- /Second.sol
        import "./Base.sol";
        contract Second is Runner { function run() external {} }
        "#,
    );
    let result = analyze_single_batch(&snapshot(marked.project()));
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    let marker = marked.marker("$1");
    let uri = marked.project().uri(marker.path());
    let Some(lsp_types::GotoDefinitionResponse::Array(locations)) =
        result.symbol_tables.goto_implementation(&uri, marker.position())
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
            (disk_path, Arc::new("contract Disk {}".into())),
            (open_path, Arc::new("contract Open { function f() public { number+; } }".into()),),
        ]
    );
}

#[cfg(unix)]
#[test]
fn analysis_batches_ignore_symlinked_disk_sources() {
    let project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"

        //- /src/Main.sol
        contract Main {}

        //- /target/Target.sol
        contract Target {}
        "#,
    );
    let mut config = project.config();
    let symlink_path = project.path("/src/Link.sol");
    symlink(project.path("/target/Target.sol"), &symlink_path).unwrap();
    config.add_source_file(symlink_path.clone());

    let snapshot = snapshot_with_config(config, Vfs::default());
    let (batches, source_files_complete) = snapshot
        .analysis_batches_cancellable(vec![symlink_path.clone()], &IndexingCancellation::default())
        .unwrap();

    assert!(batches.iter().flat_map(|batch| &batch.files).all(|(path, _)| path != &symlink_path));
    assert!(!source_files_complete);
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

    let [batch] = snapshot(&project).analysis_batches(Vec::new()).try_into().ok().unwrap();

    assert_eq!(
        batch.files,
        vec![(source_path, Arc::new("contract A { function f() public { number+; } }".into()))]
    );
    assert_eq!(batch.opts.base_path.as_deref(), Some(project.root()));
}

#[test]
fn analysis_resolves_overlay_remapped_and_auto_remapped_imports() {
    let mut project = TestProject::from_fixture(
        r#"
        //- /foundry.toml
        [profile.default]
        src = "src"
        remappings = ["@lib=lib/"]

        //- /src/A.sol
        import "./Old.sol";

        //- /src/Old.sol
        contract Old {}

        //- /src/New.sol
        contract New {}

        //- /lib/B.sol
        contract B {}

        //- /lib/forge-std/src/Test.sol
        contract Test {}
        "#,
    );
    project.open_file(
        "/src/A.sol",
        "import \"./New.sol\";\nimport \"@lib/B.sol\";\nimport \"forge-std/Test.sol\";\ncontract A is New, B, Test {}",
    );

    let result = analyze_single_batch(&snapshot(&project));

    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    let targets = result
        .symbol_tables
        .document_links(&project.path("/src/A.sol"))
        .into_iter()
        .map(|link| link.target.unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        targets,
        ["/src/New.sol", "/lib/B.sol", "/lib/forge-std/src/Test.sol"].map(|path| project.uri(path))
    );
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

    let mut batches =
        snapshot_with_config(config.clone(), Vfs::default()).analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert_eq!(batch.files, vec![(cached_path, Arc::new("contract Cached {}".into()))]);

    config.add_source_file(created_after_discovery.clone());
    let outside_source_root = project.path("/other/Outside.sol");
    project.write_file("/other/Outside.sol", "contract Outside {}");
    config.add_source_file(outside_source_root.clone());

    let mut batches = snapshot_with_config(config, Vfs::default()).analysis_batches(Vec::new());
    let batch = batches.pop().unwrap();
    assert!(batch.files.iter().any(|(path, _)| path == &created_after_discovery));
    assert!(batch.files.iter().any(|(path, _)| path == &outside_source_root));
}

#[test]
fn analysis_batches_assign_open_files_to_most_specific_workspace() {
    let project = TestProject::from_fixture("//- /nested/A.sol open\ncontract A {}\n");
    let source_path = project.path("/nested/A.sol");
    let nested = project.path("/nested");
    let config = project.config_with_roots(&["/", "/nested"]);

    let batches = snapshot_with_config(config, project.vfs()).analysis_batches(Vec::new());
    let batch_for = |base: &Path| {
        batches.iter().find(|batch| batch.opts.base_path.as_deref() == Some(base)).unwrap()
    };

    assert!(!batch_for(project.root()).files.iter().any(|(path, _)| path == &source_path));
    assert_eq!(batch_for(&nested).files, vec![(source_path, Arc::new("contract A {}".into()))]);
}

#[test]
fn analysis_batches_use_import_ownership_for_external_files() {
    let project = TestProject::from_fixture(
        r#"
        //- /first/foundry.toml
        [profile.default]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/first/"]

        //- /second/foundry.toml
        [profile.default]
        src = "../external/src"
        libs = ["../external/include"]
        auto_detect_remappings = false
        remappings = ["pkg/=lib/second/"]

        //- /external/src/Main.sol open
        import "pkg/Target.sol";

        //- /external/include/Overlay.sol open
        import "pkg/Target.sol";

        //- /first/lib/first/Target.sol
        contract FirstTarget {}

        //- /second/lib/second/Target.sol
        contract SecondTarget {}
        "#,
    );
    let mut config = project.config_with_roots(&["/"]);
    let main_path = project.path("/external/src/Main.sol");
    let overlay_path = project.path("/external/include/Overlay.sol");
    let cached_path = project.path("/external/src/Cached.sol");
    let disk_path = project.path("/external/src/Disk.sol");
    project.write_file("/external/src/Cached.sol", "contract Cached {}");
    project.write_file("/external/src/Disk.sol", "contract Disk {}");
    config.add_source_file(cached_path.clone());
    let snapshot = snapshot_with_config(config, project.vfs());

    let mut batches = snapshot.analysis_batches(vec![disk_path.clone()]);
    let second_idx = batches
        .iter()
        .position(|batch| {
            batch.opts.base_path.as_deref() == Some(project.path("/second").as_path())
        })
        .unwrap();
    let second_batch = batches.remove(second_idx);

    let mut expected_paths = vec![&main_path, &overlay_path, &cached_path, &disk_path];
    expected_paths.sort_unstable();
    assert_eq!(second_batch.files.iter().map(|(path, _)| path).collect::<Vec<_>>(), expected_paths);
    assert!(batches.iter().all(|batch| {
        batch
            .files
            .iter()
            .all(|(path, _)| ![&main_path, &overlay_path, &cached_path, &disk_path].contains(&path))
    }));
    let result = analyze(second_batch);
    let expected = Some(project.uri("/second/lib/second/Target.sol"));
    for path in [main_path, overlay_path] {
        let links = result.symbol_tables.document_links(&path);
        assert_eq!(links.len(), 1);
        assert_eq!(links[0].target, expected);
    }
}

#[test]
fn analysis_batches_share_external_open_files_across_matching_and_overlapping_contexts() {
    for (second_libs, first_import, shared) in [
        ("../shared", "Overlay.sol", "/shared"),
        ("../shared/nested", "nested/Overlay.sol", "/shared/nested"),
    ] {
        let project = TestProject::from_fixture(&format!(
            r#"
            //- /first/foundry.toml
            [profile.default]
            libs = ["../shared"]
            auto_detect_remappings = false

            //- /second/foundry.toml
            [profile.default]
            libs = ["{second_libs}"]
            auto_detect_remappings = false

            //- /first/src/Main.sol
            import "{first_import}";
            contract First is Overlay {{}}

            //- /second/src/Main.sol
            import "Overlay.sol";
            contract Second is Overlay {{}}

            //- {shared}/Overlay.sol open
            contract DiskOverlay {{}}

            //- {shared}/Dependency.sol
            contract Dependency {{}}
            "#
        ));
        let overlay_contents = "import \"./Dependency.sol\"; contract Overlay is Dependency {}";
        let overlay = project.path(&format!("{shared}/Overlay.sol"));
        let mut vfs = project.vfs();
        vfs.set_file_contents(VfsPath::from(overlay.clone()), Some(overlay_contents.into()));
        let snapshot = snapshot_with_config(project.config_with_roots(&["/"]), vfs);

        let batches = snapshot.analysis_batches(Vec::new());
        let primary = batches
            .iter()
            .position(|batch| batch.files.iter().any(|(path, _)| path == &overlay))
            .unwrap();
        let overlay_file = vec![(overlay.clone(), Arc::new(overlay_contents.into()))];
        assert_eq!(
            batches[primary]
                .files
                .iter()
                .filter(|(path, _)| path == &overlay)
                .cloned()
                .collect::<Vec<_>>(),
            overlay_file
        );
        assert!(batches[primary].preloaded_files.iter().all(|(path, _)| path != &overlay));
        let secondary = 1 - primary;
        assert!(batches[secondary].files.iter().all(|(path, _)| path != &overlay));
        assert_eq!(batches[secondary].preloaded_files, overlay_file);

        let mut results = AnalysisResultAccumulator::default();
        for batch in batches {
            results.push(analyze(batch));
        }
        let result = results.finish();
        assert!(result.diagnostics.values().all(Vec::is_empty));
        let [link] = result.symbol_tables.document_links(&overlay).try_into().unwrap();
        assert_eq!(link.target, Some(project.uri(&format!("{shared}/Dependency.sol"))));
    }
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
    let result = analyze_source(path, project.read_file("/Symbols.sol"));
    assert!(result.diagnostics.is_empty());

    let declarations = result.symbol_tables.file_declarations(&uri).collect::<Vec<_>>();
    assert_eq!(declarations.len(), result.symbol_tables.declarations().len());
    let output = declarations
        .iter()
        .map(|declaration| {
            let parent = declaration.parent.map(|parent| {
                &declarations.iter().find(|candidate| candidate.id == parent).unwrap().name
            });
            format!("{} {:?} in {parent:?}\n", declaration.name, declaration.kind)
        })
        .collect::<String>();
    snapbox::assert_data_eq!(
        output,
        snapbox::str![[r#"
TOP Constant in None
C Class in None
x Property in Some("C")
K Constant in Some("C")
S Struct in Some("C")
field Property in Some("S")
GetterValue Struct in Some("C")
visible Property in Some("GetterValue")
other Property in Some("GetterValue")
hidden Property in Some("GetterValue")
getterMap Property in Some("C")
getterValues Property in Some("C")
constructor Constructor in Some("C")
fallback Function in Some("C")
receive Function in Some("C")
f Method in Some("C")
y Variable in Some("f")
z Variable in Some("f")
local Variable in Some("f")
E Enum in None
A EnumMember in Some("E")

"#]]
    );
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
    let result = analyze_source(path, project.read_file("/Symbols.sol"));
    assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);

    let mut output = String::new();
    document_symbol_output(&result.symbol_tables.document_symbols(&uri), 0, &mut output);
    snapbox::assert_data_eq!(
        output,
        snapbox::str![[r#"
I Interface
  iface Method
    value Variable
L Module
  Logged Event
    value Variable
  helper Method
    value Variable
    result Variable
C Class
  E Enum
    A EnumMember
    B EnumMember
  S Struct
    field Property
  x Property
  constructor Constructor
  f Method
    y Variable
    z Variable
    local Variable

"#]]
    );

    let workspace_symbols = |query, top_level_only: bool| {
        result
            .symbol_tables
            .workspace_symbols(query)
            .into_iter()
            .filter(|symbol| !top_level_only || symbol.container_name.is_none())
            .map(|symbol| {
                format!("{} {:?} in {:?}\n", symbol.name, symbol.kind, symbol.container_name)
            })
            .collect::<String>()
    };
    snapbox::assert_data_eq!(
        workspace_symbols("helper", false),
        snapbox::str![[r#"
helper Method in Some("L")

"#]]
    );
    snapbox::assert_data_eq!(
        workspace_symbols("", true),
        snapbox::str![[r#"
I Interface in None
L Module in None
C Class in None

"#]]
    );
}

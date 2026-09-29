//! Benchmark-only LSP analysis and request support.
//!
//! Compiler sessions use one thread, including repeated workspace-analysis epochs. Rayon pool
//! shutdown does not join worker threads, so teardown from fixture setup can otherwise enter the
//! next measured operation. Rename and quick-fix workloads call the production validation and
//! edit-building functions synchronously; these CPU benchmarks exclude Tokio task scheduling.
//! The separate `lsp_pending` target uses production scheduling for in-process request latency.
//! Protocol and process latency belongs in the session benchmarks under `benches/lsp/`.

use super::{
    AnalysisBatch, AnalysisResult, AnalysisResultAccumulator, AnalysisTaskOutcome, DiagnosticMap,
    SymbolTables, analyze, analyze_with_source_map, run_analysis,
};
use crate::{
    config::{Config, negotiate_capabilities},
    diagnostics::{AnalyzedDocuments, DiagnosticOwner, DiagnosticStore, PullReport},
    handlers,
    symbols::CompletionContext,
    utils::apply_document_changes,
    vfs::VfsPath,
    workspace::{
        Workspace, WorkspacePathIndex,
        index_policy::{IndexingCancellation, WorkspaceIndexMetrics, WorkspaceIndexPolicy},
        workspace_idx_containing_path,
    },
};
use async_lsp::{ClientSocket, ResponseError};
use crop::Rope;
use lsp_types::{
    CallHierarchyIncomingCall, CallHierarchyIncomingCallsParams, CallHierarchyItem,
    CallHierarchyOutgoingCall, CallHierarchyOutgoingCallsParams, CallHierarchyPrepareParams,
    CodeLens, CompletionItem, Diagnostic, DidChangeTextDocumentParams, DocumentSymbol,
    GotoDefinitionResponse, Hover, HoverContents, Location, Position, PreviousResultId, Range,
    RenameParams, SignatureHelp, SignatureHelpParams, TextDocumentContentChangeEvent,
    TextDocumentIdentifier, TextDocumentPositionParams, TypeHierarchyItem, Url,
    VersionedTextDocumentIdentifier, WorkspaceEdit, WorkspaceFolder, WorkspaceSymbol,
};
use normalize_path::NormalizePath;
use solar_config::{CompileOpts, Threads};
use solar_interface::{
    data_structures::map::{FxHashMap, FxHashSet},
    source_map::{FileLoader, SourceMap},
};
use std::{
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
    task::{Context, Poll, Waker},
};

mod pending;
#[cfg(feature = "bench")]
pub use pending::BenchmarkPendingRequests;

/// An opaque error returned while preparing an LSP benchmark project.
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct BenchmarkError {
    message: String,
}

/// A summary of one bounded workspace-discovery run.
#[doc(hidden)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BenchmarkWorkspaceDiscovery {
    visited: usize,
    pruned: usize,
    eager: usize,
    source_file_count: usize,
}

impl BenchmarkWorkspaceDiscovery {
    /// Run discovery for `root` using the production indexing policy.
    #[doc(hidden)]
    pub fn run(root: impl Into<PathBuf>) -> Self {
        let result = workspace_config(&[root.into()])
            .discover_workspaces(&IndexingCancellation::default())
            .expect("benchmark discovery should not be cancelled");
        Self {
            visited: result.metrics.visited,
            pruned: result.metrics.pruned,
            eager: result.metrics.eager,
            source_file_count: result
                .workspaces
                .iter()
                .map(|workspace| workspace.source_files().len())
                .sum(),
        }
    }

    /// Number of filesystem entries inspected before pruning.
    pub fn visited(self) -> usize {
        self.visited
    }

    /// Number of directories pruned before reading descendants.
    pub fn pruned(self) -> usize {
        self.pruned
    }

    /// Number of actively indexed Solidity files.
    pub fn eager(self) -> usize {
        self.eager
    }

    /// Number of actively indexed source files.
    pub fn source_file_count(self) -> usize {
        self.source_file_count
    }
}

impl BenchmarkError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

/// A prepared workload for workspace path ownership and overlay fan-out queries.
#[doc(hidden)]
pub struct BenchmarkWorkspacePathQueries {
    workspaces: Vec<Workspace>,
    paths: Vec<PathBuf>,
    path_cache: Arc<crate::workspace::WorkspacePathIndexCache>,
}

impl BenchmarkWorkspacePathQueries {
    /// Prepare `query_count` paths below `workspace_count` nested workspace roots.
    pub fn new(workspace_count: usize, query_count: usize) -> Self {
        assert!(workspace_count > 0, "workspace path benchmark needs a workspace");
        assert!(query_count > 0, "workspace path benchmark needs a query");
        let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/path-index");
        let mut workspaces = Vec::with_capacity(workspace_count);
        for index in 0..workspace_count {
            root.push(format!("nested-{index}"));
            workspaces.push(Workspace::naked(root.clone()));
        }
        let paths = (0..query_count).map(|index| root.join(format!("Query-{index}.sol"))).collect();
        let path_cache = Arc::new(WorkspacePathIndex::cache(&workspaces));
        Self { workspaces, paths, path_cache }
    }

    /// Execute ownership and overlay-recipient queries for every prepared path.
    pub fn run(&self) -> usize {
        self.query(false, &self.paths)
    }

    /// Construct the index and execute one ownership and overlay-recipient query.
    pub fn run_one(&self) -> usize {
        self.query(false, &self.paths[..1])
    }

    /// Execute the same queries with a prebuilt workspace path index.
    pub fn run_cached(&self) -> usize {
        self.query(true, &self.paths)
    }

    /// Execute one query with a prebuilt workspace path index.
    pub fn run_cached_one(&self) -> usize {
        self.query(true, &self.paths[..1])
    }

    fn query(&self, cached: bool, paths: &[PathBuf]) -> usize {
        let index = if cached {
            WorkspacePathIndex::with_cache(&self.workspaces, Arc::clone(&self.path_cache))
        } else {
            WorkspacePathIndex::new(&self.workspaces)
        };
        paths.iter().fold(0, |fingerprint, path| {
            let query = index.query(path);
            let primary = query.workspace_idx_for_path();
            let owner = query.workspace_idx_for_import_path().unwrap_or_default();
            let overlays = query.workspace_idxs_for_import_path().fold(0, usize::wrapping_add);
            fingerprint.wrapping_add(primary).wrapping_add(owner).wrapping_add(overlays)
        })
    }

    /// Construct the index and execute one base-path containment query.
    pub fn run_containment_one(&self) -> usize {
        workspace_idx_containing_path(&self.workspaces, &self.paths[0]).unwrap_or_default()
    }
}

/// A prepared, entirely in-memory LSP benchmark project.
#[doc(hidden)]
#[derive(Clone)]
pub struct BenchmarkProject {
    root: PathBuf,
    opts: CompileOpts,
    files: Vec<(PathBuf, String)>,
    loader: InMemoryFileLoader,
}

impl BenchmarkProject {
    /// Prepare the historical single-source benchmark project.
    pub fn from_source(source: String) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches");
        let opts = CompileOpts { base_path: Some(root.clone()), ..Default::default() };
        Self::from_sources(opts, [(root.join("benchmark.sol"), source)])
            .expect("the built-in benchmark source path should be valid")
    }

    /// Prepare ordered primary sources and compiler options for repeated analysis.
    pub fn from_sources(
        mut opts: CompileOpts,
        sources: impl IntoIterator<Item = (PathBuf, String)>,
    ) -> Result<Self, BenchmarkError> {
        let root = opts
            .base_path
            .take()
            .ok_or_else(|| BenchmarkError::new("benchmark compiler options need a base path"))?;
        let root = absolute_normalized(&root)?;
        opts.base_path = Some(root.clone());

        let mut files = sources
            .into_iter()
            .map(|(path, source)| {
                let path = if path.is_absolute() { path } else { root.join(path) };
                let path = path.normalize();
                if !path.starts_with(&root) {
                    return Err(BenchmarkError::new(format!(
                        "benchmark source `{}` is outside project root `{}`",
                        path.display(),
                        root.display()
                    )));
                }
                Ok((path, source))
            })
            .collect::<Result<Vec<_>, _>>()?;
        files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
        if files.is_empty() {
            return Err(BenchmarkError::new("benchmark project has no primary sources"));
        }
        if files.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(BenchmarkError::new("benchmark project contains duplicate source paths"));
        }

        let loader_sources = files.iter().cloned().collect();
        let loader = InMemoryFileLoader::new(root.clone(), loader_sources);
        Ok(Self { root, opts, files, loader })
    }

    /// Load a Foundry project's manifest, primary sources, and import dependencies.
    ///
    /// All filesystem access happens during this constructor, before benchmark timing starts.
    pub fn from_foundry_manifest(path: impl AsRef<Path>) -> Result<Self, BenchmarkError> {
        let preparation_source_map = SourceMap::empty();
        let file_loader = preparation_source_map.file_loader();
        let manifest = file_loader.canonicalize_path(path.as_ref()).map_err(|error| {
            BenchmarkError::new(format!(
                "failed to resolve Foundry manifest `{}`: {error}",
                path.as_ref().display()
            ))
        })?;
        let mut workspace = Workspace::load_foundry(manifest.clone()).map_err(|error| {
            BenchmarkError::new(format!(
                "failed to load Foundry manifest `{}`: {error}",
                manifest.display()
            ))
        })?;
        let mut metrics = WorkspaceIndexMetrics::default();
        assert!(workspace.refresh_source_files(
            &WorkspaceIndexPolicy::default(),
            &IndexingCancellation::default(),
            &mut metrics,
        ));

        let opts = workspace.compile_opts().clone();
        let root = opts
            .base_path
            .clone()
            .ok_or_else(|| BenchmarkError::new("Foundry benchmark project has no base path"))?;
        let mut loader_sources = FxHashMap::default();
        let mut files = Vec::with_capacity(workspace.source_files().len());
        for path in workspace.source_files() {
            let source = read_source(file_loader, path)?;
            loader_sources.insert(path.normalize(), source.clone());
            files.push((path.clone(), source));
        }
        let mut dependency_roots = opts.include_paths.clone();
        for remapping in &opts.import_remappings {
            let target = Path::new(&remapping.path);
            let target =
                if target.is_absolute() { target.to_path_buf() } else { root.join(target) };
            dependency_roots.push(target);
        }
        for dependency_root in deduplicate_dependency_roots(dependency_roots) {
            collect_dependency_sources(
                file_loader,
                &dependency_root,
                &mut loader_sources,
                &mut FxHashSet::default(),
            )?;
        }
        if files.is_empty() {
            return Err(BenchmarkError::new(format!(
                "Foundry benchmark project `{}` has no Solidity sources",
                root.display()
            )));
        }
        files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));

        let root = root.normalize();
        let loader = InMemoryFileLoader::new(root.clone(), loader_sources);
        Ok(Self { root, opts, files, loader })
    }

    /// The number of primary Solidity source files in this project.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The total number of bytes in all prepared primary and dependency sources.
    pub fn source_bytes(&self) -> usize {
        let mut bytes = self.loader.corpus.sources.values().map(String::len).sum::<usize>();
        for (path, source) in &self.loader.overlays {
            if let Some(original) = self.loader.corpus.sources.get(path) {
                bytes -= original.len();
            }
            bytes += source.len();
        }
        bytes
    }

    /// Resolve a unique source substring to its LSP URI and UTF-16 position.
    pub fn unique_anchor(
        &self,
        relative_path: impl AsRef<Path>,
        needle: &str,
    ) -> Result<(Url, Position), BenchmarkError> {
        let (path, source) = self.source(relative_path.as_ref())?;
        let start = unique_offset(source, needle, path)?;
        Ok((file_url(path)?, position_at(source, start)))
    }

    /// Create a validated LSP range edit replacing one unique source substring.
    pub fn replacement_edit(
        &self,
        relative_path: impl AsRef<Path>,
        needle: &str,
        replacement: impl Into<String>,
    ) -> Result<BenchmarkEdit, BenchmarkError> {
        let (path, source) = self.source(relative_path.as_ref())?;
        let start = unique_offset(source, needle, path)?;
        let end = start + needle.len();
        Ok(BenchmarkEdit {
            path: path.clone(),
            change: TextDocumentContentChangeEvent {
                range: Some(Range::new(position_at(source, start), position_at(source, end))),
                range_length: None,
                text: replacement.into(),
            },
        })
    }

    /// Apply an edit with the same UTF-16 range logic used by document-change notifications.
    pub fn apply_edit(&mut self, edit: &BenchmarkEdit) -> Result<(), BenchmarkError> {
        let index = self.edit_source_index(edit)?;
        let source = &mut self.files[index].1;
        let updated =
            apply_document_changes(&Rope::from(source.as_str()), vec![edit.change.clone()])
                .ok_or_else(|| BenchmarkError::new("invalid document change range"))?
                .to_string();
        *source = updated.clone();
        self.loader.overlays.insert(edit.path.clone(), updated);
        Ok(())
    }

    /// Prepare one production document change without including project harness conversions.
    pub fn document_change(
        &self,
        edit: &BenchmarkEdit,
    ) -> Result<BenchmarkDocumentChange, BenchmarkError> {
        let index = self.edit_source_index(edit)?;
        Ok(BenchmarkDocumentChange {
            contents: Rope::from(self.files[index].1.as_str()),
            changes: vec![edit.change.clone()],
        })
    }

    fn edit_source_index(&self, edit: &BenchmarkEdit) -> Result<usize, BenchmarkError> {
        self.files.binary_search_by(|(path, _)| path.cmp(&edit.path)).map_err(|_| {
            BenchmarkError::new(format!(
                "benchmark edit targets unknown source `{}`",
                edit.path.display()
            ))
        })
    }

    /// Consume this prepared project and run the production compiler analysis pipeline.
    pub fn analyze(self) -> BenchmarkAnalysis {
        let Self { root, opts, files, loader } = self;
        let default_uri = files.first().and_then(|(path, _)| Url::from_file_path(path).ok());
        analyze_files(root, opts, files, loader, default_uri)
    }

    /// Analyze each primary file as an independent production analysis batch.
    pub fn analyze_file_batches(self) -> Vec<BenchmarkAnalysis> {
        let Self { root, opts, files, loader } = self;
        files
            .into_iter()
            .map(|(path, source)| {
                let default_uri = Url::from_file_path(&path).ok();
                analyze_files(
                    root.clone(),
                    opts.clone(),
                    [(path, source)],
                    loader.clone(),
                    default_uri,
                )
            })
            .collect()
    }

    fn source(&self, relative_path: &Path) -> Result<(&PathBuf, &str), BenchmarkError> {
        let path = resolve_relative_path(&self.root, relative_path)?;
        self.files
            .binary_search_by(|(candidate, _)| candidate.cmp(&path))
            .ok()
            .map(|index| (&self.files[index].0, self.files[index].1.as_str()))
            .ok_or_else(|| {
                BenchmarkError::new(format!(
                    "benchmark source `{}` is not a primary project file",
                    relative_path.display()
                ))
            })
    }
}

/// A validated document edit for a prepared benchmark project.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct BenchmarkEdit {
    path: PathBuf,
    change: TextDocumentContentChangeEvent,
}

/// An opaque document change input using the same Rope path as production notifications.
#[doc(hidden)]
#[derive(Clone)]
pub struct BenchmarkDocumentChange {
    contents: Rope,
    changes: Vec<TextDocumentContentChangeEvent>,
}

impl BenchmarkDocumentChange {
    /// Prepare incoming document edits without including source construction in timing.
    pub fn from_changes(contents: Rope, changes: Vec<TextDocumentContentChangeEvent>) -> Self {
        Self { contents, changes }
    }

    /// The complete document contents, for verifying prepared edits outside timing.
    pub fn contents(&self) -> &Rope {
        &self.contents
    }

    /// Apply this prepared document change and return the edited document.
    #[inline(never)]
    pub fn apply(self) -> Self {
        let Self { contents, changes } = self;
        Self {
            contents: apply_document_changes(&contents, changes).expect("valid benchmark edits"),
            changes: Vec::new(),
        }
    }
}

/// A prepared content-identical document update through the production notification handler.
#[doc(hidden)]
pub struct BenchmarkDocumentUpdate {
    state: super::GlobalState,
    params: DidChangeTextDocumentParams,
}

/// A production analysis state used to compare a cold run with an unchanged-snapshot reuse.
#[doc(hidden)]
pub struct BenchmarkRepeatedAnalysis {
    state: super::GlobalState,
}

impl BenchmarkRepeatedAnalysis {
    /// Prepare one open document and reserve a stable analysis epoch.
    pub fn new(source: String) -> Self {
        let (state, _) = open_benchmark_document(&source, "repeated-analysis.sol", 1);
        let version = 1;
        state.analysis_version.store(version, std::sync::atomic::Ordering::Release);
        state.analysis_commit.lock().vfs_content_revision = state.vfs.read().content_revision();
        Self { state }
    }

    /// Prepare one open document in each independently configured workspace.
    ///
    /// The caller keeps these roots and their disk dependencies alive for the workload.
    pub fn from_workspaces(roots: &[PathBuf], source: &str) -> Self {
        let mut config = workspace_config(roots);
        config.try_rediscover_workspaces().expect("benchmark workspace discovery should succeed");
        assert_eq!(config.workspaces().len(), roots.len());
        assert!(config.workspaces().iter().all(|workspace| {
            workspace.source_files().len() == 1
                && workspace.source_files()[0].file_name().is_some_and(|name| name == "Main.sol")
        }));
        let mut state = new_state();
        state.config = Arc::new(config);
        for root in roots {
            open_document(&state, root.join("Main.sol"), Rope::from(source), 1);
        }
        Self { state }
    }

    fn snapshot(&self) -> super::GlobalStateSnapshot {
        let mut snapshot = self.state.snapshot();
        snapshot.benchmark_threads = Some(Threads::resolve(1));
        snapshot
    }

    /// Check that the published workspace has no compiler diagnostics.
    pub fn assert_no_diagnostics(&self) {
        for report in self.state.diagnostics.read().workspace_pull_reports(Vec::new()) {
            let PullReport::Full { diagnostics, .. } = report.report else {
                unreachable!("a first pull always includes a full report");
            };
            assert!(
                diagnostics.is_empty(),
                "unexpected diagnostics for {}: {diagnostics:?}",
                report.uri
            );
        }
    }

    /// Open or replace one source while leaving the other workspaces unchanged.
    pub fn replace_source(&mut self, path: &Path, source: &str) {
        let path = VfsPath::from(path.to_path_buf());
        let mut vfs = self.state.vfs.write();
        let version = vfs.get_file_version(&path).unwrap_or_default() + 1;
        vfs.set_file_contents_with_version(path, Some(Rope::from(source)), Some(version));
    }

    /// Remove all document overlays before preparing an initial disk-only analysis.
    pub fn clear_open_documents(&mut self) {
        *self.state.vfs.write() = Default::default();
    }

    /// Advance and synchronously run a production document-analysis epoch.
    #[inline(never)]
    pub fn run_epoch(&mut self) -> bool {
        let version = self.state.next_analysis_version();
        self.state.commit_analysis_epoch(
            &mut self.state.analysis_commit.lock(),
            version,
            Vec::new(),
            false,
        );
        let mut snapshot = self.snapshot();
        let progress = self.state.analysis_progress.reserve(version);
        matches!(
            run_analysis(
                &mut snapshot,
                version,
                Vec::new(),
                &progress,
                &IndexingCancellation::default(),
            ),
            AnalysisTaskOutcome::Published
        )
    }

    /// Prepare call hierarchy against the latest published snapshot.
    pub fn prepare_call_hierarchy(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<Vec<CallHierarchyItem>> {
        self.state.symbol_tables.load().prepare_call_hierarchy(uri, position)
    }

    /// Advance the VFS revision through an edit and undo before analysis begins.
    pub fn edit_and_revert(&mut self) {
        let mut vfs = self.state.vfs.write();
        let (path, source) =
            vfs.iter().next().map(|(path, source)| (path.clone(), source.clone())).unwrap();
        let mut edited = source.clone();
        edited.insert(0, " ");
        vfs.set_file_contents(path.clone(), Some(edited));
        vfs.set_file_contents(path, Some(source));
    }

    /// Run one production analysis epoch, returning whether it published successfully.
    #[inline(never)]
    pub fn run(&mut self) -> bool {
        let mut snapshot = self.snapshot();
        let progress = self.state.analysis_progress.reserve(1);
        matches!(
            run_analysis(&mut snapshot, 1, Vec::new(), &progress, &IndexingCancellation::default()),
            AnalysisTaskOutcome::Published
        )
    }
}

impl BenchmarkDocumentUpdate {
    /// Prepare one open document and a full-content update with the same source text.
    pub fn from_source(source: String) -> Self {
        let (state, path) = open_benchmark_document(&source, "benchmark.sol", 0);
        let uri = Url::from_file_path(path.as_path().unwrap())
            .expect("benchmark path should be a file URL");
        let params = DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier::new(uri, 1),
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: source,
            }],
        };
        Self { state, params }
    }

    /// Apply the prepared update and return the resulting VFS content revision.
    #[inline(never)]
    pub fn apply(mut self) -> u64 {
        assert!(handlers::did_change_text_document(&mut self.state, self.params).is_continue());
        self.state.vfs.read().content_revision()
    }
}

/// A prepared VFS snapshot containing several open documents.
#[doc(hidden)]
pub struct BenchmarkOpenDocuments {
    snapshot: super::GlobalStateSnapshot,
    source_bytes: usize,
}

/// A prepared open-document selection-range request workload.
#[doc(hidden)]
pub struct BenchmarkSelectionRangeRequests {
    state: super::GlobalState,
    path: VfsPath,
    positions: Vec<Position>,
}

/// A prepared open-document folding-range request workload.
#[doc(hidden)]
pub struct BenchmarkFoldingRangeRequests {
    state: super::GlobalState,
    path: VfsPath,
}

/// A prepared open-document signature-help request using production analysis and its handler.
#[doc(hidden)]
pub struct BenchmarkSignatureHelpRequests {
    state: super::GlobalState,
    params: SignatureHelpParams,
}

/// Prepared call-hierarchy requests using the production handlers and a completed analysis.
#[doc(hidden)]
pub struct BenchmarkCallHierarchyRequests {
    state: super::GlobalState,
}

impl BenchmarkCallHierarchyRequests {
    /// Publish an analyzed project without initializing the lazy call-hierarchy query index.
    pub fn new(analysis: BenchmarkAnalysis) -> Self {
        Self::from_symbol_tables(analysis.symbol_tables)
    }

    /// Clone semantic facts into a fresh snapshot outside the first-request timing.
    pub fn before_first_request(&self) -> Self {
        Self::from_symbol_tables(self.state.symbol_tables.load().as_ref().clone())
    }

    fn from_symbol_tables(symbol_tables: SymbolTables) -> Self {
        let state = new_state();
        state.symbol_tables.store(Arc::new(symbol_tables));
        Self { state }
    }

    /// Prepare a callable through the production handler, including analysis snapshot lookup.
    #[inline(never)]
    pub fn prepare(&mut self, uri: &Url, position: Position) -> Option<Vec<CallHierarchyItem>> {
        complete_request(handlers::prepare_call_hierarchy(
            &mut self.state,
            CallHierarchyPrepareParams {
                text_document_position_params: position_params(uri.clone(), position),
                work_done_progress_params: Default::default(),
            },
        ))
    }

    /// Expand one incoming-call item through the production handler.
    #[inline(never)]
    pub fn incoming(&mut self, item: &CallHierarchyItem) -> Option<Vec<CallHierarchyIncomingCall>> {
        complete_request(handlers::call_hierarchy_incoming(
            &mut self.state,
            CallHierarchyIncomingCallsParams {
                item: item.clone(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        ))
    }

    /// Expand one outgoing-call item through the production handler.
    #[inline(never)]
    pub fn outgoing(&mut self, item: &CallHierarchyItem) -> Option<Vec<CallHierarchyOutgoingCall>> {
        complete_request(handlers::call_hierarchy_outgoing(
            &mut self.state,
            CallHierarchyOutgoingCallsParams {
                item: item.clone(),
                work_done_progress_params: Default::default(),
                partial_result_params: Default::default(),
            },
        ))
    }
}

fn complete_request<T>(request: impl Future<Output = Result<T, ResponseError>>) -> T {
    let mut request = std::pin::pin!(request);
    let mut context = Context::from_waker(Waker::noop());
    let Poll::Ready(response) = request.as_mut().poll(&mut context) else {
        panic!("benchmark request should complete immediately");
    };
    response.expect("benchmark request should succeed")
}

/// A prepared quick-fix request using diagnostics from a real compiler analysis.
#[doc(hidden)]
pub struct BenchmarkCodeActionRequests {
    state: super::GlobalState,
    params: lsp_types::CodeActionParams,
}

impl BenchmarkCodeActionRequests {
    /// Analyze the source and select either its whole range or its first mutability warning.
    pub fn new(source: String, whole_document: bool) -> Self {
        let analysis = BenchmarkAnalysis::from_source(source.clone());
        let (mut state, path) = open_benchmark_document(&source, "benchmark.sol", 1);
        let uri = Url::from_file_path(path.as_path().unwrap()).unwrap();
        let mut initialize = lsp_types::InitializeParams::default();
        initialize.capabilities.text_document.get_or_insert_default().code_action =
            Some(lsp_types::CodeActionClientCapabilities {
                code_action_literal_support: Some(lsp_types::CodeActionLiteralSupport {
                    code_action_kind: lsp_types::CodeActionKindLiteralSupport {
                        value_set: vec![lsp_types::CodeActionKind::QUICKFIX.as_str().into()],
                    },
                }),
                ..Default::default()
            });
        state.config = Arc::new(negotiate_capabilities(initialize).1);
        let range = if whole_document {
            let rope = Rope::from(source);
            Range::new(
                Position::default(),
                crate::proto::position_at_byte(&rope, rope.byte_len()).unwrap(),
            )
        } else {
            analysis.diagnostics[&uri]
                .iter()
                .find(|diagnostic| {
                    diagnostic.code == Some(lsp_types::NumberOrString::String("2018".into()))
                })
                .expect("source should emit a mutability warning")
                .range
        };
        state
            .diagnostics
            .write()
            .replace_and_publish_batches(DiagnosticOwner::Compiler, analysis.diagnostics);
        state.symbol_tables.store(Arc::new(analysis.symbol_tables));
        let params = lsp_types::CodeActionParams {
            text_document: TextDocumentIdentifier { uri },
            range,
            context: lsp_types::CodeActionContext {
                diagnostics: Vec::new(),
                only: Some(vec![lsp_types::CodeActionKind::QUICKFIX]),
                trigger_kind: None,
            },
            work_done_progress_params: Default::default(),
            partial_result_params: Default::default(),
        };
        Self { state, params }
    }

    /// Retrieve diagnostics, validate sources, and build quick fixes without task dispatch.
    #[inline(never)]
    pub fn run(&mut self) -> lsp_types::CodeActionResponse {
        let diagnostics = complete_request(
            self.state
                .code_action_diagnostics(self.params.text_document.uri.clone(), self.params.range),
        );
        handlers::validated_code_actions(
            self.params.clone(),
            diagnostics,
            self.state.vfs.clone(),
            self.state.config.supports_workspace_edit_document_changes(),
            self.state.config.supports_code_action_is_preferred(),
            self.state.config.supports_code_action_diagnostic_data(),
        )
    }
}

/// A prepared rename request including source validation and workspace-edit construction.
#[doc(hidden)]
pub struct BenchmarkRenameRequests {
    state: super::GlobalState,
    params: RenameParams,
}

impl BenchmarkRenameRequests {
    /// Analyze the project and retain all source documents as versioned VFS snapshots.
    pub fn new(project: BenchmarkProject, uri: Url, position: Position) -> Self {
        let state = new_state();
        for (path, contents) in &project.files {
            open_document(&state, path.clone(), Rope::from(contents.as_str()), 1);
        }
        state.symbol_tables.store(Arc::new(project.analyze().symbol_tables));
        let params = RenameParams {
            text_document_position: position_params(uri, position),
            new_name: "renamed".into(),
            work_done_progress_params: Default::default(),
        };
        Self { state, params }
    }

    /// Resolve the target, validate sources, and build rename edits without task dispatch.
    ///
    /// The fixture supplies a valid replacement name and a published analysis snapshot.
    #[inline(never)]
    pub fn run(&mut self) -> Option<WorkspaceEdit> {
        let position = &self.params.text_document_position;
        let candidate = self
            .state
            .symbol_tables
            .load()
            .rename_candidate(&position.text_document.uri, position.position)?;
        Some(
            handlers::validated_rename_workspace_edit(
                candidate,
                self.params.new_name.clone(),
                self.state.vfs.clone(),
                self.state.config.supports_workspace_edit_document_changes(),
            )
            .expect("rename benchmark request should succeed"),
        )
    }
}

impl BenchmarkSignatureHelpRequests {
    /// Analyze a project and open the document containing the requested call argument.
    pub fn new(project: BenchmarkProject, uri: Url, position: Position) -> Self {
        let path = uri.to_file_path().expect("signature-help benchmark URI should be a file");
        let (_, contents) = project
            .files
            .iter()
            .find(|(source_path, _)| *source_path == path)
            .expect("signature-help benchmark document should belong to the project");
        let state = new_state();
        open_document(&state, path, Rope::from(contents.as_str()), 1);
        state.symbol_tables.store(Arc::new(project.analyze().symbol_tables));
        let params = SignatureHelpParams {
            text_document_position_params: position_params(uri, position),
            work_done_progress_params: Default::default(),
            context: None,
        };
        Self { state, params }
    }

    /// Prepare an edited document with unchanged analysis, outside the request timing.
    pub fn after_edit(&self) -> Self {
        self.fresh_document(true)
    }

    /// Prepare the same analyzed document with no request caches, outside the request timing.
    pub fn before_first_request(&self) -> Self {
        self.fresh_document(false)
    }

    fn fresh_document(&self, edit: bool) -> Self {
        let path =
            crate::proto::vfs_path(&self.params.text_document_position_params.text_document.uri)
                .expect("signature-help benchmark URI should be a file");
        let mut contents = self.state.vfs.read().get_file_contents(&path).unwrap().clone();
        if edit {
            contents.insert(contents.byte_len(), " ");
        }
        let state = new_state();
        open_document(&state, path, contents, if edit { 2 } else { 1 });
        state.symbol_tables.store(self.state.symbol_tables.load_full());
        Self { state, params: self.params.clone() }
    }

    /// Move the cursor and execute a complete request against the same open document.
    pub fn run_at(&mut self, position: Position) -> Option<SignatureHelp> {
        self.params.text_document_position_params.position = position;
        self.run()
    }

    /// Execute one synchronous signature-help request through the production handler.
    #[inline(never)]
    pub fn run(&mut self) -> Option<SignatureHelp> {
        complete_request(handlers::signature_help(&mut self.state, self.params.clone()))
    }
}

fn new_state() -> super::GlobalState {
    super::GlobalState::new(ClientSocket::new_closed())
}

fn open_document(
    state: &super::GlobalState,
    path: impl Into<VfsPath>,
    contents: Rope,
    version: i32,
) {
    state.vfs.write().set_file_contents_with_version(path.into(), Some(contents), Some(version));
}

fn open_benchmark_document(
    source: &str,
    name: &str,
    version: i32,
) -> (super::GlobalState, VfsPath) {
    let state = new_state();
    let path = VfsPath::from(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches").join(name));
    open_document(&state, path.clone(), Rope::from(source), version);
    (state, path)
}

fn position_params(uri: Url, position: Position) -> TextDocumentPositionParams {
    TextDocumentPositionParams::new(TextDocumentIdentifier::new(uri), position)
}

/// Negotiate a configuration with one workspace folder per root.
fn workspace_config(roots: &[PathBuf]) -> Config {
    let folders = roots
        .iter()
        .enumerate()
        .map(|(index, root)| WorkspaceFolder {
            uri: Url::from_file_path(root).expect("benchmark root should be absolute"),
            name: format!("workspace-{index}"),
        })
        .collect();
    let params =
        lsp_types::InitializeParams { workspace_folders: Some(folders), ..Default::default() };
    negotiate_capabilities(params).1
}

impl BenchmarkFoldingRangeRequests {
    /// Prepare one immutable open document for repeated folding-range requests.
    pub fn new(source: String) -> Self {
        let (state, path) = open_benchmark_document(&source, "open-folding-range.sol", 1);
        Self { state, path }
    }

    /// Run one folding-range request through the open-document source path.
    #[inline(never)]
    pub fn run(&self) -> Vec<lsp_types::FoldingRange> {
        self.state
            .vfs
            .read()
            .get_file_folding_range_source(&self.path)
            .expect("the benchmark document should be open")
            .folding_ranges()
    }
}

impl BenchmarkSelectionRangeRequests {
    /// Prepare one immutable open document and the positions queried on every request.
    pub fn new(source: String, positions: impl IntoIterator<Item = Position>) -> Self {
        let (state, path) = open_benchmark_document(&source, "open-selection-range.sol", 1);
        let positions = positions.into_iter().collect();
        Self { state, path, positions }
    }

    /// Run one selection-range request through the open-document source path.
    #[inline(never)]
    pub fn run(&self) -> Option<Vec<lsp_types::SelectionRange>> {
        let source = { self.state.vfs.read().get_file_selection_range_source(&self.path)? };
        source.selection_ranges(&self.positions)
    }
}

impl BenchmarkOpenDocuments {
    /// Prepare `document_count` equally sized open-document overlays.
    pub fn new(document_count: usize, bytes_per_document: usize) -> Self {
        assert!(document_count > 0);
        assert!(bytes_per_document > 0);

        let state = new_state();
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/open-documents");
        for index in 0..document_count {
            let source = "x".repeat(bytes_per_document);
            open_document(
                &state,
                root.join(format!("Document-{index}.sol")),
                Rope::from(source.as_str()),
                1,
            );
        }

        Self { snapshot: state.snapshot(), source_bytes: document_count * bytes_per_document }
    }

    /// The total source bytes represented by the open documents.
    pub fn source_bytes(&self) -> usize {
        self.source_bytes
    }

    /// Build production analysis batches from the prepared open documents.
    #[inline(never)]
    pub fn build_analysis_batches(&self) -> usize {
        self.snapshot
            .analysis_batches(Vec::new())
            .into_iter()
            .flat_map(|batch| batch.files)
            .map(|(path, source)| path.as_os_str().len() + source.len())
            .sum()
    }
}

/// A diagnostic store prepared with overlapping current, reported, and previous document sets.
#[doc(hidden)]
pub struct BenchmarkWorkspaceReports {
    store: DiagnosticStore,
    previous: Vec<PreviousResultId>,
}

impl BenchmarkWorkspaceReports {
    /// Prepare a representative workspace report request for `document_count` current documents.
    pub fn new(document_count: usize) -> Self {
        assert!(document_count > 0);
        let temp_uri = |name: String| {
            Url::from_file_path(std::env::temp_dir().join(name))
                .expect("benchmark path should be a file URL")
        };
        let uris = (0..document_count)
            .map(|index| temp_uri(format!("solar-lsp-benchmark-{index:05}.sol")))
            .collect::<Vec<_>>();
        let analyzed_documents =
            uris.iter().cloned().map(|uri| (uri, None)).collect::<AnalyzedDocuments>();
        let diagnostics = uris
            .iter()
            .step_by(4)
            .cloned()
            .map(|uri| (uri, vec![Diagnostic::new_simple(Range::default(), "benchmark".into())]))
            .collect::<DiagnosticMap>();
        let mut store = DiagnosticStore::default();
        store.replace_compiler_snapshot_and_publish_batches(diagnostics, analyzed_documents);
        let mut previous = store
            .workspace_pull_reports(Vec::new())
            .into_iter()
            .map(|report| PreviousResultId {
                uri: report.uri,
                value: match report.report {
                    PullReport::Full { result_id, .. } | PullReport::Unchanged { result_id } => {
                        result_id
                    }
                },
            })
            .collect::<Vec<_>>();
        previous.extend((0..document_count / 4).map(|index| PreviousResultId {
            uri: temp_uri(format!("solar-lsp-stale-{index:05}.sol")),
            value: format!("stale-{index}"),
        }));
        Self { store, previous }
    }

    /// Generate workspace reports through the production diagnostic-store path.
    #[inline(never)]
    pub fn generate(self) -> usize {
        self.store.workspace_pull_reports(self.previous).len()
    }
}

/// A synchronous LSP query against an analyzed benchmark project.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum BenchmarkRequest {
    Hover { uri: Url, position: Position },
    GotoDefinition { uri: Url, position: Position },
    References { uri: Url, position: Position, include_declaration: bool },
    WorkspaceSymbols { query: String },
}

/// The typed response to a [`BenchmarkRequest`].
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum BenchmarkResponse {
    Hover(Option<Hover>),
    GotoDefinition(Option<GotoDefinitionResponse>),
    References(Option<Vec<Location>>),
    WorkspaceSymbols(Vec<WorkspaceSymbol>),
}

/// An opaque analysis snapshot used by the LSP Criterion benchmarks.
#[doc(hidden)]
#[derive(Clone)]
pub struct BenchmarkAnalysis {
    root: PathBuf,
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTables,
    default_uri: Option<Url>,
}

impl BenchmarkAnalysis {
    /// Analyze one in-memory Solidity source with the historical benchmark workload.
    pub fn from_source(source: String) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches");
        let path = root.join("benchmark.sol");
        let uri = Url::from_file_path(&path).expect("benchmark path should be a file URL");
        let opts = CompileOpts { threads: Threads::resolve(1), ..Default::default() };
        let mut batch = AnalysisBatch::new(opts);
        batch.push_file(path, source);
        let result = analyze(batch);
        Self {
            root,
            diagnostics: result.diagnostics,
            symbol_tables: result.symbol_tables,
            default_uri: Some(uri),
        }
    }

    /// The number of diagnostics emitted for this analysis.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.values().map(Vec::len).sum()
    }

    /// A stable, project-relative representation of all diagnostics.
    pub fn diagnostic_fingerprint(&self) -> String {
        let mut diagnostics = self
            .diagnostics
            .iter()
            .flat_map(|(uri, diagnostics)| {
                diagnostics.iter().map(|diagnostic| diagnostic_line(&self.root, uri, diagnostic))
            })
            .collect::<Vec<_>>();
        diagnostics.sort();
        diagnostics.join("\n")
    }

    /// Merge independently analyzed batches through the production symbol-table aggregation path.
    pub fn merge(batches: Vec<Self>) -> Self {
        let mut batches = batches.into_iter();
        let first = batches.next().expect("benchmark analysis needs at least one batch");
        let Self { root, diagnostics, symbol_tables, default_uri } = first;
        let mut accumulator = AnalysisResultAccumulator::default();
        let rest = batches.map(|batch| (batch.diagnostics, batch.symbol_tables));
        for (diagnostics, symbol_tables) in
            std::iter::once((diagnostics, symbol_tables)).chain(rest)
        {
            accumulator.push(AnalysisResult {
                analyzed_documents: Default::default(),
                diagnostics,
                symbol_tables,
            });
        }
        let AnalysisResult { diagnostics, symbol_tables, .. } = accumulator.finish();
        Self { root, diagnostics, symbol_tables, default_uri }
    }

    /// Execute one synchronous query against the analyzed symbol tables.
    #[inline(never)]
    pub fn execute(&self, request: &BenchmarkRequest) -> BenchmarkResponse {
        match request {
            BenchmarkRequest::Hover { uri, position } => {
                BenchmarkResponse::Hover(self.symbol_tables.hover(uri, *position))
            }
            BenchmarkRequest::GotoDefinition { uri, position } => {
                BenchmarkResponse::GotoDefinition(
                    self.symbol_tables.goto_definition(uri, *position),
                )
            }
            BenchmarkRequest::References { uri, position, include_declaration } => {
                BenchmarkResponse::References(self.symbol_tables.references(
                    uri,
                    *position,
                    *include_declaration,
                ))
            }
            BenchmarkRequest::WorkspaceSymbols { query } => {
                BenchmarkResponse::WorkspaceSymbols(self.symbol_tables.workspace_symbols(query))
            }
        }
    }

    /// Resolve hover and definition responses at one position.
    pub fn navigation(
        &self,
        uri: &Url,
        position: Position,
    ) -> (Option<Hover>, Option<GotoDefinitionResponse>) {
        (self.symbol_tables.hover(uri, position), self.symbol_tables.goto_definition(uri, position))
    }

    /// Prepare a callable and query its incoming calls.
    #[inline(never)]
    pub fn incoming_calls(&self, uri: &Url, position: Position) -> Vec<CallHierarchyIncomingCall> {
        let items = self.symbol_tables.prepare_call_hierarchy(uri, position).unwrap();
        self.symbol_tables.call_hierarchy_incoming(&items[0]).unwrap()
    }

    /// Prepare one call hierarchy item at a source position.
    #[inline(never)]
    pub fn prepare_call_hierarchy(
        &self,
        uri: &Url,
        position: Position,
    ) -> Option<Vec<CallHierarchyItem>> {
        self.symbol_tables.prepare_call_hierarchy(uri, position)
    }

    /// Return the selected range and edit count from a complete rename candidate lookup.
    #[inline(never)]
    pub fn rename_candidate(&self, uri: &Url, position: Position) -> Option<(Range, usize)> {
        self.symbol_tables
            .rename_candidate(uri, position)
            .map(|candidate| (candidate.range, candidate.locations.len()))
    }

    /// Prepare a hierarchy item and query its direct subtypes.
    #[inline(never)]
    pub fn type_hierarchy(&self, uri: &Url, position: Position) -> Vec<TypeHierarchyItem> {
        let items = self.symbol_tables.prepare_type_hierarchy(uri, position).unwrap();
        self.symbol_tables.type_hierarchy_subtypes(&items[0]).unwrap()
    }

    /// Render CodeLens annotations with the VS Code client commands enabled.
    #[inline(never)]
    pub fn code_lenses(&self, uri: &Url) -> Vec<CodeLens> {
        self.symbol_tables.code_lenses(
            uri,
            crate::config::CodeLensConfig { client_commands: true, ..Default::default() },
        )
    }

    /// Build hierarchical document symbols for one analyzed source file.
    #[inline(never)]
    pub fn document_symbols(&self, uri: &Url) -> Vec<DocumentSymbol> {
        self.symbol_tables.document_symbols(uri)
    }

    /// Complete names at a source position without protocol transport or parsing.
    #[inline(never)]
    pub fn completions(&self, uri: &Url, position: Position, prefix: &str) -> Vec<CompletionItem> {
        self.symbol_tables.completion_items(uri, position, CompletionContext::new(prefix, None))
    }

    /// Resolve one declaration or reference position synchronously.
    #[inline(never)]
    pub fn hover(&self, line: u32, character: u32) -> Option<usize> {
        let uri = self.default_uri.as_ref()?;
        let hover =
            std::hint::black_box(self.symbol_tables.hover(uri, Position::new(line, character)))?;
        let HoverContents::Markup(content) = hover.contents else { return None };
        Some(content.value.len())
    }
}

#[derive(Clone)]
struct InMemoryFileLoader {
    root: PathBuf,
    corpus: Arc<InMemoryCorpus>,
    overlays: FxHashMap<PathBuf, String>,
}

struct InMemoryCorpus {
    sources: FxHashMap<PathBuf, String>,
    directories: FxHashSet<PathBuf>,
}

impl InMemoryFileLoader {
    fn new(root: PathBuf, sources: FxHashMap<PathBuf, String>) -> Self {
        let mut directories = FxHashSet::default();
        directories.insert(root.clone());
        for path in sources.keys() {
            let mut current = path.parent();
            while let Some(directory) = current {
                directories.insert(directory.to_path_buf());
                if directory == root {
                    break;
                }
                current = directory.parent();
            }
        }
        Self {
            root,
            corpus: Arc::new(InMemoryCorpus { sources, directories }),
            overlays: FxHashMap::default(),
        }
    }

    fn normalized(&self, path: &Path) -> PathBuf {
        if path.is_absolute() { path.normalize() } else { self.root.join(path).normalize() }
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("benchmark file `{}` was not prepared", path.display()),
        )
    }
}

impl FileLoader for InMemoryFileLoader {
    fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
        let path = self.normalized(path);
        if self.overlays.contains_key(&path)
            || self.corpus.sources.contains_key(&path)
            || self.corpus.directories.contains(&path)
        {
            Ok(path)
        } else {
            Err(Self::not_found(&path))
        }
    }

    fn load_stdin(&self) -> io::Result<String> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "stdin is unavailable in an in-memory LSP benchmark",
        ))
    }

    fn load_file(&self, path: &Path) -> io::Result<String> {
        let path = self.normalized(path);
        self.overlays
            .get(&path)
            .or_else(|| self.corpus.sources.get(&path))
            .cloned()
            .ok_or_else(|| Self::not_found(&path))
    }

    fn load_binary_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "binary file `{}` is unavailable in an in-memory LSP benchmark",
                path.display()
            ),
        ))
    }
}

fn absolute_normalized(path: &Path) -> Result<PathBuf, BenchmarkError> {
    if !path.is_absolute() {
        return Err(BenchmarkError::new(format!(
            "benchmark base path `{}` is not absolute",
            path.display()
        )));
    }
    Ok(path.normalize())
}

fn resolve_relative_path(root: &Path, relative_path: &Path) -> Result<PathBuf, BenchmarkError> {
    if relative_path.components().any(|component| {
        matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    }) {
        return Err(BenchmarkError::new(format!(
            "benchmark source path `{}` is not project-relative",
            relative_path.display()
        )));
    }
    Ok(root.join(relative_path).normalize())
}

fn unique_offset(source: &str, needle: &str, path: &Path) -> Result<usize, BenchmarkError> {
    if needle.is_empty() {
        return Err(BenchmarkError::new("benchmark anchor cannot be empty"));
    }
    let Some(start) = source.find(needle) else {
        return Err(BenchmarkError::new(format!(
            "benchmark anchor `{needle}` was not found in `{}`",
            path.display()
        )));
    };
    if source[start + needle.len()..].contains(needle) {
        return Err(BenchmarkError::new(format!(
            "benchmark anchor `{needle}` is not unique in `{}`",
            path.display()
        )));
    }
    Ok(start)
}

fn position_at(source: &str, offset: usize) -> Position {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    Position::new(line, character)
}

fn file_url(path: &Path) -> Result<Url, BenchmarkError> {
    Url::from_file_path(path).map_err(|()| {
        BenchmarkError::new(format!(
            "benchmark source `{}` cannot be represented as a file URI",
            path.display()
        ))
    })
}

fn read_source(file_loader: &dyn FileLoader, path: &Path) -> Result<String, BenchmarkError> {
    file_loader.load_file(path).map_err(|error| {
        BenchmarkError::new(format!(
            "failed to read benchmark source `{}`: {error}",
            path.display()
        ))
    })
}

fn deduplicate_dependency_roots(mut roots: Vec<PathBuf>) -> Vec<PathBuf> {
    roots.iter_mut().for_each(|path| *path = path.normalize());
    roots.sort();

    let mut deduplicated = Vec::<PathBuf>::new();
    for root in roots {
        if !deduplicated.iter().any(|parent| root.starts_with(parent)) {
            deduplicated.push(root);
        }
    }
    deduplicated
}

fn collect_dependency_sources(
    file_loader: &dyn FileLoader,
    path: &Path,
    sources: &mut FxHashMap<PathBuf, String>,
    directory_stack: &mut FxHashSet<PathBuf>,
) -> Result<(), BenchmarkError> {
    let Ok(metadata) = std::fs::metadata(path) else { return Ok(()) };
    if metadata.is_file() {
        if path.extension().is_some_and(|extension| extension == "sol") {
            sources.insert(path.normalize(), read_source(file_loader, path)?);
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let canonical = file_loader.canonicalize_path(path).map_err(|error| {
        BenchmarkError::new(format!(
            "failed to resolve benchmark dependency directory `{}`: {error}",
            path.display()
        ))
    })?;
    if !directory_stack.insert(canonical.clone()) {
        return Ok(());
    }
    let enumerate_error = |error: io::Error| {
        BenchmarkError::new(format!(
            "failed to enumerate benchmark dependency directory `{}`: {error}",
            path.display()
        ))
    };
    for entry in std::fs::read_dir(path).map_err(enumerate_error)? {
        let entry = entry.map_err(enumerate_error)?;
        collect_dependency_sources(file_loader, &entry.path(), sources, directory_stack)?;
    }
    directory_stack.remove(&canonical);
    Ok(())
}

fn analyze_files(
    root: PathBuf,
    mut opts: CompileOpts,
    files: impl IntoIterator<Item = (PathBuf, String)>,
    loader: InMemoryFileLoader,
    default_uri: Option<Url>,
) -> BenchmarkAnalysis {
    opts.threads = Threads::resolve(1);
    let source_map = Arc::new(SourceMap::empty());
    source_map.set_file_loader(loader);
    let result = analyze_with_source_map(AnalysisBatch::from_files(opts, files), source_map);
    BenchmarkAnalysis {
        root,
        diagnostics: result.diagnostics,
        symbol_tables: result.symbol_tables,
        default_uri,
    }
}

fn diagnostic_line(root: &Path, uri: &Url, diagnostic: &Diagnostic) -> String {
    let path = uri
        .to_file_path()
        .ok()
        .and_then(|path| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| uri.as_str().to_string());
    let range = diagnostic.range;
    let root = root.to_string_lossy();
    let message = diagnostic.message.replace(root.as_ref(), ".").replace('\n', "\\n");
    format!(
        "{path}:{}:{}:{}:{}:{:?}:{:?}:{}",
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
        diagnostic.severity,
        diagnostic.code,
        message
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_support::TestProject;

    #[cfg(unix)]
    use std::{fs, os::unix::fs::symlink};

    #[tokio::test]
    async fn rename_workload_matches_handler() {
        for reference_count in [0, 64] {
            let source = format!(
                "contract C {{ function target() internal pure {{}} function caller() public pure {{ {} }} }}",
                "target();".repeat(reference_count),
            );
            let project = BenchmarkProject::from_source(source);
            let (uri, position) =
                project.unique_anchor("benchmark.sol", "target() internal").unwrap();
            let mut requests = BenchmarkRenameRequests::new(project, uri, position);
            let expected =
                handlers::rename(&mut requests.state, requests.params.clone()).await.unwrap();
            assert!(expected.is_some());
            assert_eq!(requests.run(), expected);
        }
    }

    #[tokio::test]
    async fn code_action_workload_matches_handler() {
        let source = "contract C { function first() public returns (uint) { return 1; } function second() public returns (uint) { return 2; } }";
        for whole_document in [false, true] {
            let mut requests = BenchmarkCodeActionRequests::new(source.into(), whole_document);
            let expected = handlers::code_actions(&mut requests.state, requests.params.clone())
                .await
                .unwrap()
                .unwrap();
            assert_eq!(expected.len(), if whole_document { 2 } else { 1 });
            assert_eq!(requests.run(), expected);
        }
    }

    #[test]
    fn repeated_analysis_limits_only_benchmark_snapshot_threads() {
        let source = "contract C {}";
        let project = TestProject::new();
        let roots = ["/workspace-0", "/workspace-1"].map(|root| {
            project.write_file(&format!("{root}/Main.sol"), source);
            project.path(root)
        });
        for analysis in [
            BenchmarkRepeatedAnalysis::new(source.into()),
            BenchmarkRepeatedAnalysis::from_workspaces(&roots[..1], source),
            BenchmarkRepeatedAnalysis::from_workspaces(&roots, source),
        ] {
            let batches = analysis.snapshot().analysis_batches(Vec::new());
            assert!(!batches.is_empty());
            assert!(batches.iter().all(|batch| batch.opts.threads().get() == 1));
            let production_batches = analysis.state.snapshot().analysis_batches(Vec::new());
            assert!(
                production_batches.iter().all(|batch| batch.opts.threads() == Threads::default().0)
            );
        }
    }

    #[cfg(unix)]
    #[test]
    fn dependency_collection_follows_directory_symlinks_without_cycles() {
        let project = TestProject::new();
        project.write_file("/vendor/package/Dependency.sol", "contract Dependency {}");
        let dependency = project.path("/vendor/package");
        symlink(&dependency, dependency.join("cycle")).unwrap();

        let alias = project.path("/lib/package");
        fs::create_dir(project.path("/lib")).unwrap();
        symlink(&dependency, &alias).unwrap();

        let source_map = SourceMap::empty();
        let mut sources = FxHashMap::default();
        collect_dependency_sources(
            source_map.file_loader(),
            &alias,
            &mut sources,
            &mut FxHashSet::default(),
        )
        .unwrap();

        assert_eq!(sources.get(&alias.join("Dependency.sol")).unwrap(), "contract Dependency {}");
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn dependency_roots_are_normalized_and_covered_children_are_removed() {
        let root = PathBuf::from("/workspace");
        let roots = vec![
            root.join("vendor/package"),
            root.join("lib/package/src"),
            root.join("lib/./package"),
            root.join("lib"),
            root.join("vendor/package"),
        ];

        assert_eq!(
            deduplicate_dependency_roots(roots),
            [root.join("lib"), root.join("vendor/package")]
        );
    }

    #[test]
    fn anchors_use_utf16_positions_and_require_uniqueness() {
        let project = BenchmarkProject::from_source("contract C { string s = \"中😀x\"; }".into());
        let (_, position) = project.unique_anchor("benchmark.sol", "x").unwrap();
        assert_eq!(position, Position::new(0, 28));

        let duplicate = BenchmarkProject::from_source("contract C { uint x; uint x; }".into());
        assert!(duplicate.unique_anchor("benchmark.sol", "x").is_err());
        assert!(duplicate.unique_anchor("missing.sol", "x").is_err());
    }

    #[test]
    fn edits_overlay_a_shared_corpus_and_feed_analysis() {
        let (original, updated) = ("contract C { uint x; }", "contract C { address x; }");
        let project = BenchmarkProject::from_source(original.into());
        let edit = project.replacement_edit("benchmark.sol", "uint x", "address x").unwrap();
        assert_eq!(project.document_change(&edit).unwrap().apply().contents.to_string(), updated);

        let mut edited = project.clone();
        assert!(Arc::ptr_eq(&project.loader.corpus, &edited.loader.corpus));
        edited.apply_edit(&edit).unwrap();
        for (project, source) in [(&project, original), (&edited, updated)] {
            assert_eq!(project.source(Path::new("benchmark.sol")).unwrap().1, source);
            assert_eq!(project.loader.load_file(Path::new("benchmark.sol")).unwrap(), source);
        }
        assert_eq!(edited.source_bytes(), project.source_bytes() + updated.len() - original.len());
        assert_eq!(edited.analyze().diagnostic_count(), 0);
    }

    #[test]
    fn foundry_benchmark_corpus_resolves_from_memory() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/foundry/unifap-v2/foundry.toml");
        let project = BenchmarkProject::from_foundry_manifest(manifest).unwrap();
        let hover = project.unique_anchor("src/UnifapV2Pair.sol", "SELECTOR").unwrap();
        assert_eq!(project.file_count(), 14);
        assert_eq!(project.source_bytes(), 290_811);

        let analysis = project.analyze();
        assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
        assert!(matches!(
            analysis.execute(&BenchmarkRequest::Hover { uri: hover.0, position: hover.1 }),
            BenchmarkResponse::Hover(Some(_))
        ));
    }

    #[test]
    fn foundry_benchmark_corpus_loads_explicit_remapping_targets() {
        let fixture = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            libs = []
            auto_detect_remappings = false
            remappings = ["@dep/=vendor/dep/src/"]

            //- /src/Main.sol
            import "@dep/Dependency.sol"; contract Main is Dependency {}

            //- /vendor/dep/src/Dependency.sol
            contract Dependency {}
            "#,
        );

        let project =
            BenchmarkProject::from_foundry_manifest(fixture.path("/foundry.toml")).unwrap();
        let analysis = project.analyze();

        assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
    }

    #[test]
    fn repeated_analysis_reuses_and_invalidates_snapshot() {
        let mut analysis = BenchmarkRepeatedAnalysis::new("contract Cached {}".into());
        let run = |analysis: &mut BenchmarkRepeatedAnalysis| {
            assert!(analysis.run());
            let commit = analysis.state.analysis_commit.lock();
            commit.cached_output.as_ref().unwrap().vfs_content_revision
        };
        let first_revision = run(&mut analysis);
        assert_eq!(run(&mut analysis), first_revision);
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/repeated-analysis.sol");
        analysis.replace_source(&path, "contract Cached { uint value; }");
        assert_ne!(run(&mut analysis), first_revision);
    }
}

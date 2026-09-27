//! Walltime measurements through the production edit, scheduler and request paths.
//!
//! A current-thread runtime makes the first request poll deterministically pending: the edit
//! schedules analysis, but its coordinator cannot run until the request yields. The timer stops
//! at the response, before validation and worker cleanup. Each sample drains the coordinator so
//! its teardown cannot leak into the next sample. Protocol transport is outside this boundary.

use super::super::GlobalState;
use crate::{config::negotiate_capabilities, handlers};
use async_lsp::{ClientSocket, ResponseError};
use lsp_types::{
    DidChangeTextDocumentParams, DidOpenTextDocumentParams, GotoDefinitionParams,
    GotoDefinitionResponse, Hover, HoverParams, InitializeParams, TextDocumentItem, Url,
    WorkspaceFolder,
};
use solar_config::Threads;
use std::{
    path::PathBuf,
    sync::Arc,
    task::{Context, Waker},
    time::{Duration, Instant},
};

const TIMEOUT: Duration = Duration::from_secs(30);

/// A warm workspace for real pending hover and definition request measurements.
#[doc(hidden)]
pub struct BenchmarkPendingRequests {
    state: GlobalState,
}

impl BenchmarkPendingRequests {
    /// Discover a disk workspace and complete initial analysis outside measurement.
    ///
    /// The caller owns the workspace and must use a current-thread Tokio runtime. Compiler
    /// analyses use one thread so Rayon pool teardown cannot overlap successive samples.
    pub async fn new(root: PathBuf, uri: Url, source: String) -> Self {
        assert_eq!(
            tokio::runtime::Handle::current().runtime_flavor(),
            tokio::runtime::RuntimeFlavor::CurrentThread,
            "pending benchmarks require a current-thread runtime",
        );
        let (_, mut config) = negotiate_capabilities(InitializeParams {
            workspace_folders: Some(vec![WorkspaceFolder {
                uri: Url::from_file_path(root).expect("benchmark root should be absolute"),
                name: "benchmark".into(),
            }]),
            ..Default::default()
        });
        config.try_rediscover_workspaces().expect("benchmark discovery should succeed");
        let mut state = GlobalState::new(ClientSocket::new_closed());
        state.config = Arc::new(config);
        state.benchmark_threads = Some(Threads::resolve(1));
        assert!(
            handlers::did_open_text_document(
                &mut state,
                DidOpenTextDocumentParams {
                    text_document: TextDocumentItem::new(uri, "solidity".into(), 1, source),
                },
            )
            .is_continue()
        );
        tokio::time::timeout(TIMEOUT, state.latest_analysis())
            .await
            .expect("initial benchmark analysis timed out")
            .expect("initial benchmark analysis failed");
        let mut requests = Self { state };
        requests.settle().await;
        requests
    }

    /// The configured debounce, before a navigation request asks analysis to start immediately.
    pub fn source_change_debounce(&self) -> Duration {
        self.state.config.source_change_debounce()
    }

    /// Measure an edit through a pending hover response, then drain untimed worker cleanup.
    pub async fn hover(
        &mut self,
        change: DidChangeTextDocumentParams,
        params: HoverParams,
    ) -> (Duration, Option<Hover>) {
        self.measure(change, |state| handlers::hover(state, params)).await
    }

    /// Measure an edit through a pending definition response, then drain untimed worker cleanup.
    pub async fn definition(
        &mut self,
        change: DidChangeTextDocumentParams,
        params: GotoDefinitionParams,
    ) -> (Duration, Option<GotoDefinitionResponse>) {
        self.measure(change, |state| handlers::goto_definition(state, params)).await
    }

    async fn measure<T, F>(
        &mut self,
        change: DidChangeTextDocumentParams,
        request: impl FnOnce(&mut GlobalState) -> F,
    ) -> (Duration, T)
    where
        F: Future<Output = Result<T, ResponseError>>,
    {
        let start = Instant::now();
        assert!(handlers::did_change_text_document(&mut self.state, change).is_continue());
        let mut request = std::pin::pin!(request(&mut self.state));
        // On this runtime the coordinator cannot publish until we first yield to it.
        assert!(
            request.as_mut().poll(&mut Context::from_waker(Waker::noop())).is_pending(),
            "benchmark request must be pending after the edit",
        );
        let response = tokio::time::timeout(TIMEOUT, request)
            .await
            .expect("pending benchmark request timed out")
            .expect("pending benchmark request failed");
        let elapsed = start.elapsed();
        self.settle().await;
        (elapsed, response)
    }

    async fn settle(&mut self) {
        tokio::time::timeout(TIMEOUT, async {
            let coordinator = self
                .state
                .analysis_scheduler
                .tasks
                .lock()
                .coordinator
                .as_ref()
                .map(|(_, task)| task.clone());
            let permit = self.state.analysis_scheduler.gate.acquire().await.unwrap();
            drop(permit);
            if let Some(coordinator) = coordinator {
                while !coordinator.is_finished() {
                    tokio::task::yield_now().await;
                }
            }
        })
        .await
        .expect("benchmark analysis cleanup timed out");
        let commit = self.state.analysis_commit.lock();
        assert!(!commit.cache_invalidated, "benchmark analysis failed");
        assert_eq!(commit.symbol_tables_version, *self.state.published_analysis_version.borrow());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{global_state::benchmark::BenchmarkProject, test_support::TestProject};
    use lsp_types::{
        TextDocumentContentChangeEvent, TextDocumentIdentifier, TextDocumentPositionParams,
        VersionedTextDocumentIdentifier,
    };
    use solar_config::CompileOpts;

    #[tokio::test(flavor = "current_thread")]
    async fn pending_benchmark_returns_edited_responses_and_drains_workers() {
        let project = TestProject::new();
        let before = "contract C {\nfunction before() public pure returns (uint256) { return 1; }\nfunction after_() public pure returns (uint256) { return 2; }\nfunction call() public pure returns (uint256) { return before(); }\n}\n";
        let after = before.replace("return before();", "return after_();");
        project.write_file("/Main.sol", before);
        let baseline = BenchmarkProject::from_sources(
            CompileOpts { base_path: Some(project.root().to_path_buf()), ..Default::default() },
            [(PathBuf::from("Main.sol"), before.into())],
        )
        .unwrap();
        let (uri, position) = baseline.unique_anchor("Main.sol", "before();").unwrap();
        let mut requests =
            BenchmarkPendingRequests::new(project.root().to_path_buf(), uri.clone(), before.into())
                .await;
        let params = TextDocumentPositionParams {
            text_document: TextDocumentIdentifier::new(uri.clone()),
            position,
        };
        let baseline = baseline.analyze();
        let old_hover = baseline.symbol_tables.hover(&uri, position);
        let old_definition = baseline.symbol_tables.goto_definition(&uri, position);
        let updated = BenchmarkProject::from_sources(
            CompileOpts { base_path: Some(project.root().to_path_buf()), ..Default::default() },
            [(PathBuf::from("Main.sol"), after.clone())],
        )
        .unwrap()
        .analyze();
        let new_hover = updated.symbol_tables.hover(&uri, position);
        let new_definition = updated.symbol_tables.goto_definition(&uri, position);
        assert!(new_hover.is_some());
        assert!(new_definition.is_some());
        assert_ne!(old_hover, new_hover);
        assert_ne!(old_definition, new_definition);

        for (index, source) in [after.as_str(), before, after.as_str()].into_iter().enumerate() {
            let change = DidChangeTextDocumentParams {
                text_document: VersionedTextDocumentIdentifier::new(uri.clone(), index as i32 + 2),
                content_changes: vec![TextDocumentContentChangeEvent {
                    range: None,
                    range_length: None,
                    text: source.into(),
                }],
            };
            if index == 1 {
                let (elapsed, response) = requests
                    .definition(
                        change,
                        GotoDefinitionParams {
                            text_document_position_params: params.clone(),
                            work_done_progress_params: Default::default(),
                            partial_result_params: Default::default(),
                        },
                    )
                    .await;
                assert!(!elapsed.is_zero());
                assert_eq!(response, old_definition);
            } else {
                let (elapsed, response) = requests
                    .hover(
                        change,
                        HoverParams {
                            text_document_position_params: params.clone(),
                            work_done_progress_params: Default::default(),
                        },
                    )
                    .await;
                assert!(!elapsed.is_zero());
                assert_eq!(response, new_hover);
            }
            let tasks = requests.state.analysis_scheduler.tasks.lock();
            assert!(tasks.worker.is_none());
            assert!(tasks.coordinator.is_none());
            assert!(tasks.debounce.is_none());
        }
    }

    #[tokio::test(flavor = "current_thread")]
    #[should_panic(expected = "benchmark request must be pending after the edit")]
    async fn pending_benchmark_rejects_content_identical_edits() {
        let project = TestProject::new();
        let source = "contract C {}";
        project.write_file("/Main.sol", source);
        let uri = Url::from_file_path(project.path("/Main.sol")).unwrap();
        let mut requests =
            BenchmarkPendingRequests::new(project.root().to_path_buf(), uri.clone(), source.into())
                .await;
        requests
            .hover(
                DidChangeTextDocumentParams {
                    text_document: VersionedTextDocumentIdentifier::new(uri.clone(), 2),
                    content_changes: vec![TextDocumentContentChangeEvent {
                        range: None,
                        range_length: None,
                        text: source.into(),
                    }],
                },
                HoverParams {
                    text_document_position_params: TextDocumentPositionParams {
                        text_document: TextDocumentIdentifier::new(uri),
                        position: lsp_types::Position::new(0, 9),
                    },
                    work_done_progress_params: Default::default(),
                },
            )
            .await;
    }
}

//! Fresh semantic snapshots for requests that only need one file and its imports.
//!
//! These snapshots never publish workspace diagnostics or replace the full index. They use
//! captured overlays and compiler options, and reject results if the input revision changes.

use super::*;
use async_lsp::ErrorCode;

struct CancelOnDrop(IndexingCancellation);

impl Drop for CancelOnDrop {
    fn drop(&mut self) {
        self.0.cancel();
    }
}

impl GlobalState {
    pub(crate) fn analysis_for_request(
        &self,
        uri: &Url,
        source_override: Option<Arc<String>>,
    ) -> impl Future<Output = Result<Arc<SymbolTables>, ResponseError>> + use<> {
        let path = uri.to_file_path().ok();
        let revision = self.analysis_revision();
        let vfs_revision = self.vfs.read().content_revision();
        let current = (source_override.is_none()
            && revision.is_current(vfs_revision)
            && !self.analysis_commit.lock().cache_invalidated)
            .then(|| self.symbol_tables.load_full());
        // Rediscovery and failed epochs require a validated configuration before local analysis.
        let workspace_analysis = {
            let commit = self.analysis_commit.lock();
            (commit.discovery_pending || commit.cache_invalidated).then(|| self.latest_analysis())
        };
        let snapshot = self.snapshot();
        let mut invalidated = self.analysis_invalidated.subscribe();
        let overlays = if current.is_none() && workspace_analysis.is_none() {
            let vfs = self.vfs.read();
            vfs.iter()
                .filter_map(|(path, _)| {
                    Some((
                        path.as_path()?.to_path_buf(),
                        vfs.get_file_analysis_source(path)?,
                        vfs.get_file_version(path),
                    ))
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        };
        async move {
            let changed = || {
                ResponseError::new(
                    ErrorCode::CONTENT_MODIFIED,
                    "analysis inputs changed since request",
                )
            };
            if !revision.inputs_are_current(vfs_revision) {
                return Err(changed());
            }
            if let Some(current) = current {
                return Ok(current);
            }
            if let Some(workspace_analysis) = workspace_analysis {
                let tables = workspace_analysis.await?;
                return if revision.is_current(vfs_revision)
                    && snapshot
                        .analysis_commit
                        .lock()
                        .analysis_config
                        .as_ref()
                        .is_some_and(|config| Arc::ptr_eq(&snapshot.config, config))
                {
                    Ok(tables.load_full())
                } else {
                    Err(changed())
                };
            }
            let Some(path) = path else { return Ok(Arc::new(SymbolTables::default())) };
            let cancellation = CancelOnDrop(IndexingCancellation::default());
            let worker_cancellation = cancellation.0.clone();
            let worker = tokio::task::spawn_blocking(move || {
                let workspaces = snapshot.analysis_workspaces();
                let index = WorkspacePathIndex::new(&workspaces);
                let workspace = index
                    .workspace_idx_for_source_path(snapshot.config.index_policy(), &path)
                    .unwrap_or_else(|| index.query(&path).workspace_idx_for_path());
                let mut batch = AnalysisBatch::new(workspaces[workspace].compile_opts().clone());
                let mut source_override = source_override;
                for (overlay, source, version) in overlays {
                    if overlay == path {
                        batch.push_open_file(
                            overlay,
                            source_override.take().unwrap_or(source),
                            version,
                        );
                    } else {
                        batch.push_preloaded_file(overlay, source, version);
                    }
                }
                if batch.files.is_empty() {
                    let source = RealFileLoader.load_file(&path).map_err(|error| {
                        ResponseError::new(
                            ErrorCode::REQUEST_FAILED,
                            format!("cannot read request source: {error}"),
                        )
                    })?;
                    batch.push_file(path, source);
                }
                batch.finish();
                let result = analyze_recording_dependencies(batch, &worker_cancellation, None);
                result.map(|result| Arc::new(result.result.symbol_tables)).ok_or_else(changed)
            });
            let result = tokio::select! {
                biased;
                _ = invalidated.changed() => return Err(changed()),
                result = worker => result.map_err(|error| {
                    ResponseError::new(ErrorCode::INTERNAL_ERROR, format!("request analysis failed: {error}"))
                })??,
            };
            if !revision.inputs_are_current(vfs_revision) {
                return Err(changed());
            }
            Ok(result)
        }
    }
}

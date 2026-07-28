use super::workspace_edit::validated_import_workspace_edit;
use crate::{
    document_links::ImportEditPlan,
    file_operations::{FileMoveBatch, parse_file_uri},
    global_state::GlobalState,
};
use async_lsp::{ErrorCode, ResponseError};
use lsp_types::{CreateFilesParams, DeleteFilesParams, RenameFilesParams, Url, WorkspaceEdit};
use std::future::ready;

pub(crate) fn will_create_files(
    _: &mut GlobalState,
    _: CreateFilesParams,
) -> impl Future<Output = Result<Option<WorkspaceEdit>, ResponseError>> + use<> {
    ready(Ok(None))
}

pub(crate) fn will_rename_files(
    state: &mut GlobalState,
    params: RenameFilesParams,
) -> impl Future<Output = Result<Option<WorkspaceEdit>, ResponseError>> + use<> {
    let moves = FileMoveBatch::try_from(params);
    let request = moves.as_ref().is_ok_and(|moves| !moves.is_empty()).then(|| {
        (
            state.latest_analysis(),
            state.config.clone(),
            state.vfs.clone(),
            state.config.supports_workspace_edit_document_changes(),
        )
    });
    async move {
        let moves = moves
            .map_err(|error| ResponseError::new(ErrorCode::INVALID_PARAMS, error.to_string()))?;
        let Some((latest_analysis, config, vfs, document_changes)) = request else {
            return Ok(None);
        };
        let symbol_tables = latest_analysis.await?;
        let mut plan = symbol_tables.read().import_rename_edits(&moves);
        retain_workspace_source_edits(&mut plan, &config);
        if plan.changes.is_empty() {
            return Ok(None);
        }
        tokio::task::spawn_blocking(move || {
            validated_import_workspace_edit(plan, vfs, document_changes)
        })
        .await
        .map_err(file_operation_task_failed)?
        .map(Some)
    }
}

pub(crate) fn will_delete_files(
    state: &mut GlobalState,
    params: DeleteFilesParams,
) -> impl Future<Output = Result<Option<WorkspaceEdit>, ResponseError>> + use<> {
    let deleted_paths =
        params.files.into_iter().filter_map(|file| parse_file_uri(&file.uri)).collect::<Vec<_>>();
    let request = (!deleted_paths.is_empty()).then(|| {
        (
            state.latest_analysis(),
            state.config.clone(),
            state.vfs.clone(),
            state.config.supports_workspace_edit_document_changes(),
        )
    });
    async move {
        let Some((latest_analysis, config, vfs, document_changes)) = request else {
            return Ok(None);
        };
        let symbol_tables = latest_analysis.await?;
        let mut plan = symbol_tables.read().import_delete_edits(&deleted_paths);
        retain_workspace_source_edits(&mut plan, &config);
        if plan.changes.is_empty() {
            return Ok(None);
        }
        tokio::task::spawn_blocking(move || {
            validated_import_workspace_edit(plan, vfs, document_changes)
        })
        .await
        .map_err(file_operation_task_failed)?
        .map(Some)
    }
}

fn retain_workspace_source_edits(plan: &mut ImportEditPlan, config: &crate::config::Config) {
    let is_workspace_source =
        |uri: &Url| uri.to_file_path().is_ok_and(|path| config.tracks_source_file(&path));
    plan.changes.retain(|uri, _| is_workspace_source(uri));
    plan.analyzed_contents.retain(|uri, _| is_workspace_source(uri));
}

fn file_operation_task_failed(error: tokio::task::JoinError) -> ResponseError {
    ResponseError::new(ErrorCode::INTERNAL_ERROR, format!("file-operation task failed: {error}"))
}

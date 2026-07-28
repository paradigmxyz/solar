use super::workspace_edit::validated_import_workspace_edit;
use crate::{
    document_links::ImportEditPlan,
    file_operations::{FileMoveBatch, parse_file_uri},
    global_state::GlobalState,
};
use async_lsp::{ErrorCode, ResponseError};
use lsp_types::{CreateFilesParams, DeleteFilesParams, RenameFilesParams, Url, WorkspaceEdit};
use std::{
    fs,
    future::ready,
    path::{Path, PathBuf},
};

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
    let preparation = if let Ok(moves) = &moves
        && !moves.is_empty()
    {
        let watched_paths = rename_watched_paths(state, moves);
        Some(state.file_operations.prepare_rename(moves.clone(), watched_paths))
    } else {
        None
    };
    let request = moves.as_ref().is_ok_and(|moves| !moves.is_empty()).then(|| {
        (
            state.latest_analysis(),
            state.config.clone(),
            state.vfs.clone(),
            state.config.supports_workspace_edit_document_changes(),
        )
    });
    async move {
        let result = async {
            let moves = moves.map_err(|error| {
                ResponseError::new(ErrorCode::INVALID_PARAMS, error.to_string())
            })?;
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
        .await;
        if result.is_ok()
            && let Some(preparation) = preparation
        {
            preparation.activate();
        }
        result
    }
}

pub(super) fn rename_watched_paths(state: &GlobalState, moves: &FileMoveBatch) -> Vec<PathBuf> {
    let roots = moves.old_paths().map(ToOwned::to_owned).collect::<Vec<_>>();
    let mut paths = state.config.tracked_source_files_under(&roots);
    paths.extend(state.vfs.read().iter().filter_map(|(path, _)| {
        let path = path.as_path()?;
        roots.iter().any(|root| path.starts_with(root)).then(|| path.to_path_buf())
    }));
    for root in &roots {
        collect_rename_watched_paths(root, &mut paths);
    }
    for root in moves.new_paths() {
        let mut moved_paths = Vec::new();
        collect_rename_watched_paths(root, &mut moved_paths);
        paths.extend(
            moved_paths
                .into_iter()
                .filter_map(|path| moves.reverse_map_path(&path).map(|(_, path)| path)),
        );
    }
    paths.sort();
    paths.dedup();
    paths
}

fn collect_rename_watched_paths(path: &Path, paths: &mut Vec<PathBuf>) {
    let Ok(metadata) = fs::symlink_metadata(path) else { return };
    if !metadata.is_dir() {
        if is_rename_watched_path(path) {
            paths.push(path.to_path_buf());
        }
        return;
    }

    let Ok(entries) = fs::read_dir(path) else { return };
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else { continue };
        if file_type.is_dir() {
            collect_rename_watched_paths(&path, paths);
        } else if is_rename_watched_path(&path) {
            paths.push(path);
        }
    }
}

fn is_rename_watched_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "sol")
        || path.file_name().is_some_and(|name| name == "foundry.toml")
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

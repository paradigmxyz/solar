use crate::{
    NotifyResult,
    file_operations::{FileMoveBatch, parse_file_uri},
    global_state::GlobalState,
    proto,
    utils::apply_document_changes,
};
use crop::Rope;
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, DidChangeConfigurationParams,
    DidChangeTextDocumentParams, DidChangeWatchedFilesParams, DidChangeWorkspaceFoldersParams,
    DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
    FileChangeType, RenameFilesParams, WillSaveTextDocumentParams,
};
use std::{
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
};
use tracing::{debug, error};

pub(crate) fn did_open_text_document(
    state: &mut GlobalState,
    params: DidOpenTextDocumentParams,
) -> NotifyResult {
    if let Some(path) = proto::vfs_path(&params.text_document.uri) {
        let disk_path = path.as_path().map(ToOwned::to_owned);
        let already_exists = state.vfs.read().exists(&path);
        if already_exists {
            error!(?path, "duplicate DidOpenTextDocument");
        }

        let mut vfs = state.vfs.write();
        vfs.set_file_contents_with_version(
            path,
            Some(Rope::from(params.text_document.text)),
            Some(params.text_document.version),
        );
        let changed = vfs.mark_clean();
        drop(vfs);
        if changed {
            state.recompute_after_source_changes(disk_path.into_iter().collect());
        } else {
            state.reindex_if_invalidated();
        }
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_change_text_document(
    state: &mut GlobalState,
    params: DidChangeTextDocumentParams,
) -> NotifyResult {
    if let Some(path) = proto::vfs_path(&params.text_document.uri) {
        let disk_path = path.as_path().map(ToOwned::to_owned);
        let (changed, new_contents) = {
            let _guard = state.vfs.read();
            let Some(contents) = _guard.get_file_contents(&path) else {
                error!(?path, "orphan DidChangeTextDocument");
                return ControlFlow::Continue(());
            };
            let new_contents = apply_document_changes(contents, params.content_changes);

            (contents != &new_contents, new_contents)
        };

        state.vfs.write().set_file_contents_with_version(
            path,
            Some(new_contents),
            Some(params.text_document.version),
        );
        if changed {
            state.recompute_after_source_changes(disk_path.into_iter().collect());
        } else {
            state.reindex_if_invalidated();
        }
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_close_text_document(
    state: &mut GlobalState,
    params: DidCloseTextDocumentParams,
) -> NotifyResult {
    if let Some(path) = proto::vfs_path(&params.text_document.uri) {
        if !state.vfs.read().exists(&path) {
            error!(?path, "orphan DidCloseTextDocument");
        }

        let disk_path = path.as_path().map(ToOwned::to_owned);
        state.vfs.write().set_file_contents(path, None);
        state.recompute_with_disk_files(disk_path.into_iter().collect());
    }

    ControlFlow::Continue(())
}

pub(crate) fn will_save_text_document(
    _: &mut GlobalState,
    params: WillSaveTextDocumentParams,
) -> NotifyResult {
    debug!(
        uri = %params.text_document.uri,
        reason = ?params.reason,
        "text document will save"
    );
    ControlFlow::Continue(())
}

pub(crate) fn did_save_text_document(
    state: &mut GlobalState,
    params: DidSaveTextDocumentParams,
) -> NotifyResult {
    state.reindex_if_invalidated();
    if let Ok(path) = params.text_document.uri.to_file_path() {
        state.run_flychecks_on_save(path);
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_change_configuration(
    state: &mut GlobalState,
    _: DidChangeConfigurationParams,
) -> NotifyResult {
    // As stated in https://github.com/microsoft/language-server-protocol/issues/676,
    // this notification's parameters should be ignored and the actual config queried separately.
    state.reindex();
    ControlFlow::Continue(())
}

pub(crate) fn did_change_watched_files(
    state: &mut GlobalState,
    params: DidChangeWatchedFilesParams,
) -> NotifyResult {
    let mut should_rediscover = false;
    let mut disk_paths = Vec::new();
    let mut removed_paths = Vec::new();

    for event in params.changes {
        let Ok(path) = event.uri.to_file_path() else {
            continue;
        };

        match path.file_name().and_then(|name| name.to_str()) {
            Some("foundry.toml") => {
                should_rediscover = true;
            }
            Some(_) if path.extension().is_some_and(|ext| ext == "sol") => {
                if event.typ == FileChangeType::CREATED {
                    Arc::make_mut(&mut state.config).add_source_file(path.clone());
                } else if event.typ == FileChangeType::DELETED {
                    Arc::make_mut(&mut state.config).remove_source_file(&path);
                    removed_paths.push(path.clone());
                }
                disk_paths.push(path);
            }
            _ => {}
        }
    }

    if should_rediscover || !disk_paths.is_empty() {
        state.recompute_for_file_changes(disk_paths, removed_paths, should_rediscover);
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_create_files(state: &mut GlobalState, params: CreateFilesParams) -> NotifyResult {
    let created_paths =
        params.files.into_iter().filter_map(|file| parse_file_uri(&file.uri)).collect();
    reconcile_workspace_file_operations(state, created_paths, FileMoveBatch::default(), Vec::new());
    ControlFlow::Continue(())
}

pub(crate) fn did_rename_files(state: &mut GlobalState, params: RenameFilesParams) -> NotifyResult {
    let moves = match FileMoveBatch::try_from(params) {
        Ok(moves) => moves,
        Err(error) => {
            tracing::warn!(%error, "ignoring conflicting file rename batch");
            return ControlFlow::Continue(());
        }
    };
    reconcile_workspace_file_operations(state, Vec::new(), moves, Vec::new());
    ControlFlow::Continue(())
}

pub(crate) fn did_delete_files(state: &mut GlobalState, params: DeleteFilesParams) -> NotifyResult {
    let deleted_paths =
        params.files.into_iter().filter_map(|file| parse_file_uri(&file.uri)).collect();
    reconcile_workspace_file_operations(state, Vec::new(), FileMoveBatch::default(), deleted_paths);
    ControlFlow::Continue(())
}

fn reconcile_workspace_file_operations(
    state: &mut GlobalState,
    created_paths: Vec<PathBuf>,
    moves: FileMoveBatch,
    deleted_paths: Vec<PathBuf>,
) {
    if created_paths.is_empty() && moves.is_empty() && deleted_paths.is_empty() {
        return;
    }

    let removed_roots = moves
        .old_paths()
        .map(Path::to_path_buf)
        .chain(deleted_paths.iter().cloned())
        .collect::<Vec<_>>();
    let mut removed_paths = state.config.tracked_source_files_under(&removed_roots);
    {
        let mut vfs = state.vfs.write();
        removed_paths.extend(vfs.iter().filter_map(|(path, _)| {
            let path = path.as_path()?;
            removed_roots.iter().any(|root| path.starts_with(root)).then(|| path.to_path_buf())
        }));
        vfs.rename_file_prefixes(&moves);
        vfs.remove_file_prefixes(&deleted_paths);
    }
    Arc::make_mut(&mut state.config).reconcile_workspace_roots(&moves, &deleted_paths);
    removed_paths.extend(removed_roots);
    removed_paths.sort();
    removed_paths.dedup();

    let mut disk_paths = created_paths;
    disk_paths.extend(moves.new_paths().map(Path::to_path_buf));
    disk_paths.sort();
    disk_paths.dedup();
    state.recompute_for_file_changes(disk_paths, removed_paths, true);
}

pub(crate) fn did_change_workspace_folders(
    state: &mut GlobalState,
    params: DidChangeWorkspaceFoldersParams,
) -> NotifyResult {
    let config = Arc::make_mut(&mut state.config);

    for workspace in params.event.removed {
        let Ok(path) = workspace.uri.to_file_path() else {
            continue;
        };
        config.remove_workspace(&path);
    }

    let added = params.event.added.into_iter().filter_map(|it| it.uri.to_file_path().ok());
    config.add_workspaces(added);

    state.reindex();

    ControlFlow::Continue(())
}

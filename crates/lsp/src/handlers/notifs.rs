use crate::{
    NotifyResult,
    global_state::{GlobalState, SourceFileEventDisposition},
    proto,
    utils::apply_document_changes,
};
use crop::Rope;
use lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, FileChangeType, WillSaveTextDocumentParams,
};
use std::{ops::ControlFlow, path::PathBuf, sync::Arc};
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
        vfs.mark_clean();
        drop(vfs);
        state.recompute_after_opening_source(disk_path.into_iter().collect());
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_change_text_document(
    state: &mut GlobalState,
    params: DidChangeTextDocumentParams,
) -> NotifyResult {
    if let Some(path) = proto::vfs_path(&params.text_document.uri) {
        let disk_path = path.as_path().map(ToOwned::to_owned);
        let new_contents = {
            let _guard = state.vfs.read();
            let Some(contents) = _guard.get_file_contents(&path) else {
                error!(?path, "orphan DidChangeTextDocument");
                return ControlFlow::Continue(());
            };
            apply_document_changes(contents, params.content_changes)
        };

        let changed = state.vfs.write().set_file_contents_with_version(
            path,
            Some(new_contents),
            Some(params.text_document.version),
        );
        if changed {
            state.recompute_after_source_changes(disk_path.into_iter().collect());
        } else {
            state.update_analyzed_document_version(
                params.text_document.uri,
                params.text_document.version,
            );
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

fn push_watched_event_batch(
    batches: &mut Vec<(FileChangeType, Vec<PathBuf>)>,
    typ: FileChangeType,
    path: PathBuf,
) {
    if let Some((last_typ, paths)) = batches.last_mut()
        && *last_typ == typ
    {
        paths.push(path);
    } else {
        batches.push((typ, vec![path]));
    }
}

pub(crate) fn did_change_watched_files(
    state: &mut GlobalState,
    params: DidChangeWatchedFilesParams,
) -> NotifyResult {
    let changes = super::file_operations::reconcile_watched_file_events(state, params.changes);

    let mut should_rediscover = false;
    let mut disk_paths = Vec::new();
    let mut removed_paths = Vec::new();
    let mut watched_event_batches = Vec::new();

    for event in changes {
        let Some(vfs_path) = proto::vfs_path(&event.uri) else {
            continue;
        };
        let Some(path) = vfs_path.as_path().map(ToOwned::to_owned) else {
            continue;
        };

        match path.file_name().and_then(|name| name.to_str()) {
            Some(".git")
                if matches!(event.typ, FileChangeType::CREATED | FileChangeType::DELETED) =>
            {
                if state.config.nested_repository_marker_event_is_relevant(&path) {
                    should_rediscover = true;
                }
            }
            Some("foundry.toml" | "remappings.txt") => {
                if !state.config.workspace_config_event_is_relevant(&path) {
                    continue;
                }
                should_rediscover = true;
                if matches!(event.typ, FileChangeType::CREATED | FileChangeType::DELETED) {
                    push_watched_event_batch(&mut watched_event_batches, event.typ, path);
                }
            }
            Some(_) if path.extension().is_some_and(|ext| ext == "sol") => {
                let is_open = state.vfs.read().exists(&vfs_path);
                // Open documents are sourced from the VFS. A save echo is redundant, while a
                // watcher-only delete only removes the disk copy; `didClose` or `didDelete` ends
                // the open-document overlay.
                if is_open && matches!(event.typ, FileChangeType::CHANGED | FileChangeType::DELETED)
                {
                    if event.typ == FileChangeType::DELETED {
                        Arc::make_mut(&mut state.config).remove_source_file(&path);
                    }
                    continue;
                }
                let import_only_topology_changed =
                    matches!(event.typ, FileChangeType::CREATED | FileChangeType::DELETED)
                        && state.config.is_index_import_only_path(&path);
                if !import_only_topology_changed {
                    match state.classify_source_file_event(&path, event.typ) {
                        SourceFileEventDisposition::Relevant => {}
                        SourceFileEventDisposition::Deferred
                        | SourceFileEventDisposition::Irrelevant => continue,
                        SourceFileEventDisposition::Recover => {
                            should_rediscover = true;
                            if event.typ == FileChangeType::DELETED {
                                removed_paths.push(path.clone());
                            }
                            disk_paths.push(path);
                            continue;
                        }
                    }
                }
                if import_only_topology_changed {
                    should_rediscover = true;
                }
                if event.typ == FileChangeType::CREATED {
                    Arc::make_mut(&mut state.config).add_source_file(path.clone());
                } else if event.typ == FileChangeType::DELETED {
                    Arc::make_mut(&mut state.config).remove_source_file(&path);
                    removed_paths.push(path.clone());
                }
                if matches!(event.typ, FileChangeType::CREATED | FileChangeType::DELETED) {
                    push_watched_event_batch(&mut watched_event_batches, event.typ, path.clone());
                }
                disk_paths.push(path);
            }
            _ if matches!(event.typ, FileChangeType::CREATED | FileChangeType::DELETED) => {
                let shallow_watch = state.config.shallow_watch_event_is_relevant(&path);
                let relevant = if event.typ == FileChangeType::CREATED {
                    std::fs::symlink_metadata(&path).is_ok_and(|metadata| metadata.is_dir())
                        && (shallow_watch || state.created_file_operation_path_is_relevant(&path))
                } else {
                    shallow_watch || state.deleted_file_operation_path_is_relevant(&path)
                };
                if !relevant {
                    continue;
                }
                should_rediscover = true;
                if event.typ == FileChangeType::DELETED {
                    removed_paths.push(path.clone());
                }
                push_watched_event_batch(&mut watched_event_batches, event.typ, path);
            }
            _ => {}
        }
    }

    for (typ, paths) in watched_event_batches {
        state.file_operations.record_watched_events(typ, paths);
    }

    if should_rediscover || !disk_paths.is_empty() {
        state.recompute_for_file_changes(disk_paths, removed_paths, should_rediscover);
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_change_workspace_folders(
    state: &mut GlobalState,
    params: DidChangeWorkspaceFoldersParams,
) -> NotifyResult {
    let removed_paths = params
        .event
        .removed
        .into_iter()
        .filter_map(|workspace| workspace.uri.to_file_path().ok())
        .collect::<Vec<_>>();
    let added_paths =
        params.event.added.into_iter().filter_map(|workspace| workspace.uri.to_file_path().ok());

    let previous_roots = state.config.workspace_roots().to_vec();
    {
        let config = Arc::make_mut(&mut state.config);
        for path in &removed_paths {
            config.remove_workspace(path);
        }
        config.add_workspaces(added_paths);
    }
    if state.config.workspace_roots() != previous_roots {
        state.record_workspace_root_change(previous_roots);
    }

    state.reindex_after_removing_paths(removed_paths);
    state.reregister_watched_files();

    ControlFlow::Continue(())
}

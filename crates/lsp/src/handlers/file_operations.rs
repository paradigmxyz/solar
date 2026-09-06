use super::workspace_edit::validated_import_workspace_edit;
use crate::{
    NotifyResult,
    config::Config,
    document_links::ImportEditPlan,
    file_operations::{FileMoveBatch, WatchedFileAction, parse_file_uri},
    global_state::{GlobalState, SourceFileEventDisposition},
    proto,
    symbols::SymbolTables,
    vfs::Vfs,
};
use async_lsp::{ErrorCode, ResponseError};
use lsp_types::{
    CreateFilesParams, DeleteFilesParams, FileChangeType, FileEvent, RenameFilesParams, Url,
    WorkspaceEdit,
};
use solar_interface::data_structures::sync::RwLock;
use std::{
    future::ready,
    ops::ControlFlow,
    path::{Path, PathBuf},
    sync::Arc,
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
    let moves = FileMoveBatch::try_from(params).and_then(|moves| {
        state.vfs.read().validate_rename_file_prefixes(&moves)?;
        Ok(moves)
    });
    let relevant = moves.as_ref().is_ok_and(|moves| {
        moves.old_paths().any(|path| state.deleted_file_operation_path_is_relevant(path))
            || moves.new_paths().any(|path| state.created_file_operation_path_is_relevant(path))
    });
    let preparation = if let Ok(moves) = &moves
        && relevant
    {
        Some(state.file_operations.prepare_rename(moves.clone()))
    } else {
        None
    };
    let request = relevant.then(|| {
        (
            state.latest_analysis_with_config(),
            state.vfs.clone(),
            state.config.supports_workspace_edit_document_changes(),
        )
    });
    async move {
        let result = async {
            let moves = moves.map_err(|error| {
                ResponseError::new(ErrorCode::INVALID_PARAMS, error.to_string())
            })?;
            let Some((latest_analysis, vfs, document_changes)) = request else {
                return Ok(None);
            };
            let (symbol_tables, config) = latest_analysis.await?;
            let plan = symbol_tables.read().import_rename_edits(&moves);
            if plan.is_empty() || !workspace_source_edits_are_complete(&plan, &config) {
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

fn watched_paths_under(
    config: &Config,
    vfs: &RwLock<Vfs>,
    symbol_tables: &RwLock<SymbolTables>,
    roots: &[PathBuf],
) -> Vec<PathBuf> {
    let mut paths = config.file_operation_paths_under(roots);
    paths.extend(symbol_tables.read().file_operation_paths_under(roots));
    paths.extend(vfs.read().iter().filter_map(|(path, _)| {
        let path = path.as_path()?;
        roots.iter().any(|root| path.starts_with(root)).then(|| path.to_path_buf())
    }));
    paths.extend(roots.iter().filter(|path| is_watched_path(path)).cloned());
    paths
}

fn is_watched_path(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "sol")
        || path
            .file_name()
            .is_some_and(|name| matches!(name.to_str(), Some("foundry.toml" | "remappings.txt")))
}

fn completed_file_operation_path_is_relevant(
    state: &GlobalState,
    path: &Path,
    typ: FileChangeType,
) -> bool {
    let known = if typ == FileChangeType::CREATED {
        state.created_file_operation_path_is_relevant(path)
    } else {
        state.deleted_file_operation_path_is_relevant(path)
    };
    if known {
        return true;
    }
    if path.extension().is_none_or(|extension| extension != "sol") {
        return false;
    }
    matches!(
        state.classify_source_file_event(path, typ),
        SourceFileEventDisposition::Relevant | SourceFileEventDisposition::Recover
    )
}

fn classify_completed_file_operation_paths<'a>(
    state: &GlobalState,
    paths: impl IntoIterator<Item = &'a Path>,
    typ: FileChangeType,
) -> bool {
    let mut relevant = false;
    for path in paths {
        relevant |= completed_file_operation_path_is_relevant(state, path, typ);
    }
    relevant
}

const CREATED_DIRECTORY_ECHO_SCAN_LIMIT: usize = 4096;

fn collect_watched_disk_paths(state: &GlobalState, roots: &[PathBuf], paths: &mut Vec<PathBuf>) {
    let mut pending =
        roots.iter().take(CREATED_DIRECTORY_ECHO_SCAN_LIMIT).cloned().collect::<Vec<_>>();
    let mut visited = 0;

    while let Some(path) = pending.pop() {
        if visited == CREATED_DIRECTORY_ECHO_SCAN_LIMIT {
            break;
        }
        visited += 1;

        let Ok(metadata) = std::fs::symlink_metadata(&path) else { continue };
        if is_watched_path(&path) && state.created_file_operation_path_is_relevant(&path) {
            paths.push(path.clone());
        }
        if !metadata.is_dir() || !state.created_file_operation_path_is_relevant(&path) {
            continue;
        }

        let remaining = CREATED_DIRECTORY_ECHO_SCAN_LIMIT.saturating_sub(visited + pending.len());
        if remaining == 0 {
            continue;
        }
        let Ok(entries) = std::fs::read_dir(path) else { continue };
        pending.extend(entries.take(remaining).filter_map(Result::ok).map(|entry| entry.path()));
    }
}

pub(crate) fn did_create_files(state: &mut GlobalState, params: CreateFilesParams) -> NotifyResult {
    let created_paths = params
        .files
        .into_iter()
        .filter_map(|file| parse_file_uri(&file.uri))
        .filter(|path| {
            completed_file_operation_path_is_relevant(state, path, FileChangeType::CREATED)
        })
        .collect::<Vec<_>>();
    let mut watched_paths =
        watched_paths_under(&state.config, &state.vfs, &state.symbol_tables, &created_paths);
    // Snapshot only bounded, policy-relevant echoes. Events beyond the bound are harmless: they
    // may schedule a redundant analysis instead of blocking the router on an unbounded scan.
    collect_watched_disk_paths(state, &created_paths, &mut watched_paths);
    watched_paths.extend(
        state.file_operations.watched_event_paths_under(FileChangeType::CREATED, &created_paths),
    );
    watched_paths.sort_unstable();
    watched_paths.dedup();
    // Workspace configuration includes the optional `remappings.txt`; a missing path cannot
    // produce a create watcher echo and must not prevent matching the paths that did.
    watched_paths.retain(|path| path.exists());
    let schedule_analysis =
        !state.file_operations.consume_watched_events(FileChangeType::CREATED, &watched_paths);
    reconcile_workspace_file_operations(
        state,
        created_paths.clone(),
        FileMoveBatch::default(),
        Vec::new(),
        schedule_analysis,
    );
    if schedule_analysis {
        watched_paths.extend(watched_paths_under(
            &state.config,
            &state.vfs,
            &state.symbol_tables,
            &created_paths,
        ));
        watched_paths.sort_unstable();
        watched_paths.dedup();
        watched_paths.retain(|path| path.exists());
    }
    state.file_operations.record_direct_create_events(watched_paths);
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
    if let Err(error) = state.vfs.read().validate_rename_file_prefixes(&moves) {
        tracing::warn!(%error, "ignoring file rename with colliding VFS destinations");
        return ControlFlow::Continue(());
    }
    let deleted_relevant =
        classify_completed_file_operation_paths(state, moves.old_paths(), FileChangeType::DELETED);
    let created_relevant =
        classify_completed_file_operation_paths(state, moves.new_paths(), FileChangeType::CREATED);
    if !deleted_relevant && !created_relevant {
        return ControlFlow::Continue(());
    }
    if !state.file_operations.apply_rename(&moves) {
        return ControlFlow::Continue(());
    }
    reconcile_workspace_file_operations(state, Vec::new(), moves, Vec::new(), true);
    ControlFlow::Continue(())
}

pub(crate) fn did_delete_files(state: &mut GlobalState, params: DeleteFilesParams) -> NotifyResult {
    let deleted_paths = params
        .files
        .into_iter()
        .filter_map(|file| parse_file_uri(&file.uri))
        .filter(|path| {
            completed_file_operation_path_is_relevant(state, path, FileChangeType::DELETED)
        })
        .collect::<Vec<_>>();
    let mut watched_paths =
        watched_paths_under(&state.config, &state.vfs, &state.symbol_tables, &deleted_paths);
    watched_paths.extend(
        state.file_operations.watched_event_paths_under(FileChangeType::DELETED, &deleted_paths),
    );
    let schedule_analysis =
        !state.file_operations.consume_watched_events(FileChangeType::DELETED, &watched_paths);
    reconcile_workspace_file_operations(
        state,
        Vec::new(),
        FileMoveBatch::default(),
        deleted_paths.clone(),
        schedule_analysis,
    );
    state.file_operations.record_direct_delete_events(watched_paths, deleted_paths);
    ControlFlow::Continue(())
}

pub(super) fn reconcile_watched_file_events(
    state: &mut GlobalState,
    events: Vec<FileEvent>,
) -> Vec<FileEvent> {
    let mut changes = Vec::with_capacity(events.len());
    for event in events {
        let action = proto::vfs_path(&event.uri)
            .and_then(|path| path.as_path().map(ToOwned::to_owned))
            .map_or(WatchedFileAction::Process, |path| {
                state.file_operations.observe_watcher_event(&path, event.typ)
            });
        match action {
            WatchedFileAction::ApplyRenames(moves) => {
                for moves in moves {
                    if let Err(error) = state.vfs.read().validate_rename_file_prefixes(&moves) {
                        tracing::warn!(%error, "deferring watched rename with colliding VFS destinations");
                        continue;
                    }
                    let deleted_relevant = classify_completed_file_operation_paths(
                        state,
                        moves.old_paths(),
                        FileChangeType::DELETED,
                    );
                    let created_relevant = classify_completed_file_operation_paths(
                        state,
                        moves.new_paths(),
                        FileChangeType::CREATED,
                    );
                    let relevant = deleted_relevant || created_relevant;
                    if relevant && state.file_operations.claim_watched_rename(&moves) {
                        reconcile_workspace_file_operations(
                            state,
                            Vec::new(),
                            moves,
                            Vec::new(),
                            true,
                        );
                    }
                }
            }
            WatchedFileAction::Ignore => {}
            WatchedFileAction::Process => changes.push(event),
        }
    }
    changes
}

fn reconcile_workspace_file_operations(
    state: &mut GlobalState,
    created_paths: Vec<PathBuf>,
    moves: FileMoveBatch,
    deleted_paths: Vec<PathBuf>,
    schedule_analysis: bool,
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
        if let Err(error) = vfs.rename_file_prefixes(&moves) {
            tracing::warn!(%error, "ignoring file rename with colliding VFS destinations");
            return;
        }
        vfs.remove_file_prefixes(&deleted_paths);
    }
    let previous_workspace_roots = state.config.workspace_roots().to_vec();
    let workspace_roots_changed =
        Arc::make_mut(&mut state.config).reconcile_workspace_roots(&moves, &deleted_paths);
    if workspace_roots_changed {
        state.record_workspace_root_change(previous_workspace_roots);
    }
    removed_paths.extend(removed_roots);
    removed_paths.sort();
    removed_paths.dedup();

    let mut disk_paths = created_paths;
    disk_paths.extend(moves.new_paths().map(Path::to_path_buf));
    disk_paths.sort();
    disk_paths.dedup();
    if schedule_analysis || workspace_roots_changed {
        state.recompute_for_file_changes(disk_paths, removed_paths, true);
    }
    if workspace_roots_changed {
        state.reregister_watched_files();
    }
}

pub(crate) fn will_delete_files(
    state: &mut GlobalState,
    params: DeleteFilesParams,
) -> impl Future<Output = Result<Option<WorkspaceEdit>, ResponseError>> + use<> {
    let deleted_paths = params
        .files
        .into_iter()
        .filter_map(|file| parse_file_uri(&file.uri))
        .filter(|path| state.deleted_file_operation_path_is_relevant(path))
        .collect::<Vec<_>>();
    let request = (!deleted_paths.is_empty()).then(|| {
        (
            state.latest_analysis_with_config(),
            state.vfs.clone(),
            state.config.supports_workspace_edit_document_changes(),
        )
    });
    async move {
        let Some((latest_analysis, vfs, document_changes)) = request else {
            return Ok(None);
        };
        let (symbol_tables, config) = latest_analysis.await?;
        let plan = symbol_tables.read().import_delete_edits(&deleted_paths);
        if plan.is_empty() || !workspace_source_edits_are_complete(&plan, &config) {
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

fn workspace_source_edits_are_complete(
    plan: &ImportEditPlan,
    config: &crate::config::Config,
) -> bool {
    if config.may_omit_source_files() {
        return false;
    }
    let is_workspace_source =
        |uri: &Url| uri.to_file_path().is_ok_and(|path| config.tracks_source_file(&path));
    plan.all_files(is_workspace_source)
}

fn file_operation_task_failed(error: tokio::task::JoinError) -> ResponseError {
    ResponseError::new(ErrorCode::INTERNAL_ERROR, format!("file-operation task failed: {error}"))
}

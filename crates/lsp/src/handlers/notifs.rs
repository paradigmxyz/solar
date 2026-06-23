use std::{ops::ControlFlow, sync::Arc};

use crop::Rope;
use lsp_types::{
    DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
    DidChangeWorkspaceFoldersParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
};
use tracing::{error, info};

use crate::{NotifyResult, global_state::GlobalState, proto, utils::apply_document_changes};

pub(crate) fn did_open_text_document(
    state: &mut GlobalState,
    params: DidOpenTextDocumentParams,
) -> NotifyResult {
    info!("config: {:?}", state.config);
    if let Some(path) = proto::vfs_path(&params.text_document.uri) {
        let already_exists = state.vfs.read().exists(&path);
        if already_exists {
            error!(?path, "duplicate DidOpenTextDocument");
        }

        let mut vfs = state.vfs.write();
        vfs.set_file_contents(path, Some(Rope::from(params.text_document.text)));
        if vfs.mark_clean() {
            drop(vfs);
            state.recompute();
        }
    }

    ControlFlow::Continue(())
}

pub(crate) fn did_change_text_document(
    state: &mut GlobalState,
    params: DidChangeTextDocumentParams,
) -> NotifyResult {
    if let Some(path) = proto::vfs_path(&params.text_document.uri) {
        let (changed, new_contents) = {
            let _guard = state.vfs.read();
            let Some(contents) = _guard.get_file_contents(&path) else {
                error!(?path, "orphan DidChangeTextDocument");
                return ControlFlow::Continue(());
            };
            let new_contents = apply_document_changes(contents, params.content_changes);

            (contents != &new_contents, new_contents)
        };

        if changed {
            state.vfs.write().set_file_contents(path, Some(new_contents));
            state.recompute();
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

pub(crate) fn did_change_configuration(
    _: &mut GlobalState,
    _: DidChangeConfigurationParams,
) -> NotifyResult {
    // As stated in https://github.com/microsoft/language-server-protocol/issues/676,
    // this notification's parameters should be ignored and the actual config queried separately.
    //
    // For now this is just a stub.
    ControlFlow::Continue(())
}

pub(crate) fn did_change_watched_files(
    state: &mut GlobalState,
    params: DidChangeWatchedFilesParams,
) -> NotifyResult {
    let mut should_recompute = false;
    let mut disk_paths = Vec::new();

    for event in params.changes {
        let Ok(path) = event.uri.to_file_path() else {
            continue;
        };

        match path.file_name().and_then(|name| name.to_str()) {
            Some("foundry.toml") => {
                Arc::make_mut(&mut state.config).rediscover_workspaces();
                should_recompute = true;
            }
            Some(_) if path.extension().is_some_and(|ext| ext == "sol") => {
                disk_paths.push(path);
                should_recompute = true;
            }
            _ => {}
        }
    }

    if should_recompute {
        state.recompute_with_disk_files(disk_paths);
    }

    ControlFlow::Continue(())
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

    // todo: rediscover workspaces & refetch configs

    ControlFlow::Continue(())
}

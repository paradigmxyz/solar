use std::{env, path::PathBuf};

use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use tracing::info;

use crate::workspace::manifest::ProjectManifest;

/// The LSP config.
///
/// This struct is internal only and should not be serialized or deserialized. Instead, values in
/// this struct are the full view of all merged config sources, such as `initialization_opts`,
/// on-disk config files (e.g. `foundry.toml`).
#[derive(Default, Clone, Debug)]
pub(crate) struct Config {
    workspace_roots: Vec<PathBuf>,
    discovered_projects: Vec<ProjectManifest>,
    watched_file_dynamic_registration: bool,
}

impl Config {
    pub(crate) fn supports_watched_file_dynamic_registration(&self) -> bool {
        self.watched_file_dynamic_registration
    }

    pub(crate) fn rediscover_workspaces(&mut self) {
        let discovered = ProjectManifest::discover_all(&self.workspace_roots);
        info!("discovered projects: {:?}", discovered);
        if discovered.is_empty() {
            info!("no project manifests found in {:?}", &self.workspace_roots);
        }
        self.discovered_projects = discovered;
    }

    pub(crate) fn remove_workspace(&mut self, path: &PathBuf) {
        if let Some(pos) = self.workspace_roots.iter().position(|it| it == path) {
            self.workspace_roots.remove(pos);
        }
    }

    pub(crate) fn add_workspaces(&mut self, paths: impl Iterator<Item = PathBuf>) {
        self.workspace_roots.extend(paths);
    }
}

pub(crate) fn negotiate_capabilities(params: InitializeParams) -> (ServerCapabilities, Config) {
    // todo: make this absolute guaranteed
    #[allow(deprecated)]
    let root_path = match params.root_uri.and_then(|it| it.to_file_path().ok()) {
        Some(it) => it,
        None => {
            // todo: unwrap
            env::current_dir().unwrap()
        }
    };

    // todo: make this absolute guaranteed
    // The latest LSP spec mandates clients report `workspace_folders`, but some might still report
    // `root_uri`.
    let watched_file_dynamic_registration = params
        .capabilities
        .workspace
        .and_then(|workspace| workspace.did_change_watched_files)
        .and_then(|capabilities| capabilities.dynamic_registration)
        .unwrap_or(false);

    let workspace_roots = params
        .workspace_folders
        .map(|workspaces| {
            workspaces.into_iter().filter_map(|it| it.uri.to_file_path().ok()).collect::<Vec<_>>()
        })
        .filter(|workspaces| !workspaces.is_empty())
        .unwrap_or_else(|| vec![root_path]);

    (
        ServerCapabilities {
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: None,
                    will_save_wait_until: None,
                    ..Default::default()
                },
            )),
            ..Default::default()
        },
        Config { workspace_roots, watched_file_dynamic_registration, ..Default::default() },
    )
}

#[cfg(test)]
mod tests {
    use lsp_types::{DidChangeWatchedFilesClientCapabilities, WorkspaceClientCapabilities};

    use super::*;

    #[test]
    fn negotiate_capabilities_records_watched_file_dynamic_registration_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_watched_file_dynamic_registration());

        let mut params = InitializeParams::default();
        params.capabilities.workspace = Some(WorkspaceClientCapabilities {
            did_change_watched_files: Some(DidChangeWatchedFilesClientCapabilities {
                dynamic_registration: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.supports_watched_file_dynamic_registration());
    }
}

use std::{
    env,
    path::{Path, PathBuf},
};

use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use tracing::{error, info};

use crate::workspace::manifest::ProjectManifest;

/// The LSP config.
///
/// This struct is internal only and should not be serialized or deserialized. Instead, values in
/// this struct are the full view of all merged config sources, such as `initialization_opts`,
/// on-disk config files (e.g. `foundry.toml`).
#[derive(Default, Clone, Debug)]
pub(crate) struct Config {
    root_path: PathBuf,
    workspace_roots: Vec<PathBuf>,
    discovered_projects: Vec<ProjectManifest>,
}

impl Config {
    pub(crate) fn new(root_path: PathBuf, workspace_roots: Vec<PathBuf>) -> Self {
        Config { root_path, workspace_roots, discovered_projects: Default::default() }
    }

    pub(crate) fn rediscover_workspaces(&mut self) {
        let discovered = ProjectManifest::discover_all(&self.workspace_roots);
        info!("discovered projects: {:?}", discovered);
        if discovered.is_empty() {
            error!("failed to find any projects in {:?}", &self.workspace_roots);
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

    pub(crate) fn root_path(&self) -> &Path {
        self.root_path.as_path()
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
    let workspace_roots = params
        .workspace_folders
        .map(|workspaces| {
            workspaces.into_iter().filter_map(|it| it.uri.to_file_path().ok()).collect::<Vec<_>>()
        })
        .filter(|workspaces| !workspaces.is_empty())
        .unwrap_or_else(|| vec![root_path.clone()]);

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
        Config::new(root_path, workspace_roots),
    )
}

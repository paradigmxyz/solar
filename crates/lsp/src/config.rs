use std::{
    env,
    path::{Path, PathBuf},
};

use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use solar_interface::data_structures::map::FxHashSet;
use tracing::{info, warn};

use crate::workspace::{Workspace, manifest::ProjectManifest, workspace_idx_for_path};

/// The LSP config.
///
/// This struct is internal only and should not be serialized or deserialized. Instead, values in
/// this struct are the full view of all merged config sources, such as `initialization_opts`,
/// on-disk config files (e.g. `foundry.toml`).
#[derive(Default, Clone, Debug)]
pub(crate) struct Config {
    workspace_roots: Vec<PathBuf>,
    workspaces: Vec<Workspace>,
    watched_file_dynamic_registration: bool,
}

impl Config {
    pub(crate) fn supports_watched_file_dynamic_registration(&self) -> bool {
        self.watched_file_dynamic_registration
    }

    pub(crate) fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub(crate) fn rediscover_workspaces(&mut self) {
        let mut workspaces = Vec::new();
        let mut seen_manifests = FxHashSet::default();
        for root in &self.workspace_roots {
            let discovered = ProjectManifest::discover(root);
            info!(?root, ?discovered, "discovered projects");
            if discovered.is_empty() {
                info!(?root, "no project manifests found");
                push_workspace(&mut workspaces, Workspace::naked(root.clone()));
                continue;
            }

            for manifest in discovered {
                if !seen_manifests.insert(manifest.clone()) {
                    continue;
                }
                let fallback_root = manifest.root().map(Path::to_path_buf);
                match Workspace::load_manifest(manifest) {
                    Ok(workspace) => push_workspace(&mut workspaces, workspace),
                    Err(error) => {
                        warn!(%error, "failed to load workspace");
                        if let Some(root) = fallback_root {
                            push_workspace(&mut workspaces, Workspace::naked(root));
                        }
                    }
                }
            }
        }
        if workspaces.is_empty() {
            for root in &self.workspace_roots {
                push_workspace(&mut workspaces, Workspace::naked(root.clone()));
            }
        }

        info!(
            workspaces = ?workspaces
                .iter()
                .map(|workspace| (workspace.kind(), workspace.manifest_path()))
                .collect::<Vec<_>>(),
            "loaded workspaces",
        );
        self.workspaces = workspaces;
    }

    pub(crate) fn remove_workspace(&mut self, path: &Path) {
        if let Some(pos) = self.workspace_roots.iter().position(|it| it == path) {
            self.workspace_roots.remove(pos);
        }
    }

    pub(crate) fn add_workspaces(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.workspace_roots.extend(paths);
    }

    pub(crate) fn add_source_file(&mut self, path: PathBuf) {
        if self.workspaces.is_empty() {
            return;
        }
        let idx = workspace_idx_for_path(&self.workspaces, &path);
        self.workspaces[idx].add_source_file(path);
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        if self.workspaces.is_empty() {
            return;
        }
        let idx = workspace_idx_for_path(&self.workspaces, path);
        self.workspaces[idx].remove_source_file(path);
    }
}

fn push_workspace(workspaces: &mut Vec<Workspace>, mut workspace: Workspace) {
    workspace.refresh_source_files();
    workspaces.push(workspace);
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
    use std::fs;

    use lsp_types::{
        DidChangeWatchedFilesClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
    };
    use tempfile::TempDir;

    use super::*;
    use crate::workspace::WorkspaceKind;

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

    #[test]
    fn rediscover_workspaces_loads_manifests_and_falls_back_to_naked_roots() {
        let configured = TempDir::new().unwrap();
        fs::write(
            configured.path().join("solar.toml"),
            r#"
                [compiler]
                source_paths = ["contracts"]
            "#,
        )
        .unwrap();
        let naked = TempDir::new().unwrap();

        let params = InitializeParams {
            workspace_folders: Some(vec![
                WorkspaceFolder {
                    uri: lsp_types::Url::from_file_path(configured.path()).unwrap(),
                    name: "configured".into(),
                },
                WorkspaceFolder {
                    uri: lsp_types::Url::from_file_path(naked.path()).unwrap(),
                    name: "naked".into(),
                },
            ]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();

        assert_eq!(config.workspaces().len(), 2);
        let solar = config
            .workspaces()
            .iter()
            .find(|workspace| workspace.kind() == WorkspaceKind::Solar)
            .unwrap();
        assert_eq!(solar.source_roots(), &[configured.path().join("contracts")]);

        fs::remove_file(configured.path().join("solar.toml")).unwrap();
        config.rediscover_workspaces();

        assert_eq!(config.workspaces().len(), 2);
        assert!(
            config.workspaces().iter().all(|workspace| workspace.kind() == WorkspaceKind::Naked)
        );
    }

    #[test]
    fn rediscover_workspaces_keeps_naked_root_after_manifest_load_error() {
        let broken = TempDir::new().unwrap();
        fs::write(broken.path().join("solar.toml"), "not valid toml =").unwrap();
        let configured = TempDir::new().unwrap();
        fs::write(
            configured.path().join("solar.toml"),
            r#"
                [compiler]
                source_paths = ["contracts"]
            "#,
        )
        .unwrap();

        let params = InitializeParams {
            workspace_folders: Some(vec![
                WorkspaceFolder {
                    uri: lsp_types::Url::from_file_path(broken.path()).unwrap(),
                    name: "broken".into(),
                },
                WorkspaceFolder {
                    uri: lsp_types::Url::from_file_path(configured.path()).unwrap(),
                    name: "configured".into(),
                },
            ]),
            ..Default::default()
        };
        let (_, mut config) = negotiate_capabilities(params);

        config.rediscover_workspaces();

        assert_eq!(config.workspaces().len(), 2);
        assert!(config.workspaces().iter().any(|workspace| {
            workspace.kind() == WorkspaceKind::Naked
                && workspace.compile_opts().base_path.as_deref() == Some(broken.path())
        }));
        assert!(config.workspaces().iter().any(|workspace| {
            workspace.kind() == WorkspaceKind::Solar
                && workspace.source_roots() == [configured.path().join("contracts")]
        }));
    }
}

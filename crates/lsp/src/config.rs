use std::{collections::HashSet, env, path::PathBuf};

use lsp_types::{
    InitializeParams, ServerCapabilities, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions,
};
use tracing::{info, warn};

use crate::workspace::{Workspace, manifest::ProjectManifest};

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
        let mut seen_manifests = HashSet::new();
        for root in &self.workspace_roots {
            let discovered = ProjectManifest::discover_all(std::slice::from_ref(root));
            info!(?root, ?discovered, "discovered projects");
            if discovered.is_empty() {
                info!(?root, "no project manifests found");
                workspaces.push(Workspace::naked(root.clone()));
                continue;
            }

            for manifest in discovered {
                if !seen_manifests.insert(manifest.clone()) {
                    continue;
                }
                match Workspace::load_manifest(manifest) {
                    Ok(workspace) => workspaces.push(workspace),
                    Err(error) => warn!(%error, "failed to load workspace"),
                }
            }
        }
        if workspaces.is_empty() {
            workspaces.extend(self.workspace_roots.iter().cloned().map(Workspace::naked));
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
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use lsp_types::{
        DidChangeWatchedFilesClientCapabilities, WorkspaceClientCapabilities, WorkspaceFolder,
    };

    use super::*;
    use crate::workspace::WorkspaceKind;

    struct TempProject {
        root: PathBuf,
    }

    impl TempProject {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir()
                .join(format!("solar-lsp-config-{name}-{}-{nanos}", std::process::id()));
            fs::create_dir_all(&root).unwrap();
            Self { root }
        }

        fn root(&self) -> &Path {
            &self.root
        }
    }

    impl Drop for TempProject {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.root);
        }
    }

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
        let configured = TempProject::new("configured");
        fs::write(
            configured.root().join("solar.toml"),
            r#"
                [compiler]
                source_paths = ["contracts"]
            "#,
        )
        .unwrap();
        let naked = TempProject::new("naked");

        let params = InitializeParams {
            workspace_folders: Some(vec![
                WorkspaceFolder {
                    uri: lsp_types::Url::from_file_path(configured.root()).unwrap(),
                    name: "configured".into(),
                },
                WorkspaceFolder {
                    uri: lsp_types::Url::from_file_path(naked.root()).unwrap(),
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
        assert_eq!(solar.source_roots(), &[configured.root().join("contracts")]);

        fs::remove_file(configured.root().join("solar.toml")).unwrap();
        config.rediscover_workspaces();

        assert_eq!(config.workspaces().len(), 2);
        assert!(
            config.workspaces().iter().all(|workspace| workspace.kind() == WorkspaceKind::Naked)
        );
    }
}

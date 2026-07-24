use crate::{
    commands,
    diagnostics::DiagnosticOwner,
    flycheck::{FlycheckConfig, FlycheckInitializationOptions},
    workspace::{Workspace, WorkspacePathIndex, manifest::ProjectManifest},
};
use lsp_types::{
    CompletionOptions, DeclarationCapability, DiagnosticOptions, DiagnosticServerCapabilities,
    DocumentLinkOptions, ExecuteCommandOptions, HoverProviderCapability,
    ImplementationProviderCapability, InitializeParams, OneOf, RenameOptions, SaveOptions,
    SelectionRangeProviderCapability, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TypeDefinitionProviderCapability, WorkDoneProgressOptions,
};
use solar_interface::data_structures::map::FxHashSet;
use std::{
    env,
    path::{Path, PathBuf},
};
use tracing::{info, warn};

/// The LSP config.
///
/// This struct is internal only and should not be serialized or deserialized. Instead, values in
/// this struct are the full view of all merged config sources, such as `initialization_opts`,
/// on-disk config files (e.g. `foundry.toml`).
#[derive(Default, Clone, Debug)]
pub(crate) struct Config {
    workspace_roots: Vec<PathBuf>,
    workspaces: Vec<Workspace>,
    flycheck_options: FlycheckInitializationOptions,
    flychecks: Vec<FlycheckConfig>,
    watched_file_dynamic_registration: bool,
    workspace_edit_document_changes: bool,
    work_done_progress: bool,
    hierarchical_document_symbol_support: bool,
    completion: CompletionClientOptions,
    signature_help: SignatureHelpClientOptions,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct CompletionClientOptions {
    pub(crate) snippet_support: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SignatureHelpClientOptions {
    pub(crate) label_offsets: bool,
    pub(crate) markdown_documentation: bool,
    pub(crate) signature_active_parameter: bool,
}

impl Config {
    pub(crate) fn supports_watched_file_dynamic_registration(&self) -> bool {
        self.watched_file_dynamic_registration
    }

    pub(crate) fn supports_workspace_edit_document_changes(&self) -> bool {
        self.workspace_edit_document_changes
    }

    pub(crate) fn supports_work_done_progress(&self) -> bool {
        self.work_done_progress
    }

    pub(crate) fn supports_hierarchical_document_symbols(&self) -> bool {
        self.hierarchical_document_symbol_support
    }

    pub(crate) fn completion_options(&self) -> CompletionClientOptions {
        self.completion
    }

    #[cfg(test)]
    pub(crate) fn enable_completion_snippets(&mut self) {
        self.completion.snippet_support = true;
    }

    pub(crate) fn signature_help_options(&self) -> SignatureHelpClientOptions {
        self.signature_help
    }

    #[cfg(test)]
    pub(crate) fn enable_signature_help_label_offsets(&mut self) {
        self.signature_help.label_offsets = true;
    }

    pub(crate) fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub(crate) fn forge_path(&self) -> PathBuf {
        self.flycheck_options.forge_path()
    }

    pub(crate) fn formatter_root_for_path(&self, path: &Path) -> Option<PathBuf> {
        ProjectManifest::discover_in_parents(path)
            .and_then(|manifest| match manifest {
                ProjectManifest::Foundry(path) => path.parent().map(Path::to_path_buf),
            })
            .or_else(|| {
                WorkspacePathIndex::new(&self.workspaces)
                    .workspace_idx_containing_path(path)
                    .and_then(|idx| self.workspaces[idx].compile_opts().base_path.clone())
            })
            .or_else(|| path.parent().map(Path::to_path_buf))
    }

    pub(crate) fn flychecks_for_path(&self, path: &Path) -> Vec<FlycheckConfig> {
        self.flychecks.iter().filter(|flycheck| flycheck.applies_to(path)).cloned().collect()
    }

    pub(crate) fn rediscover_workspaces(&mut self) -> Vec<DiagnosticOwner> {
        let mut workspaces = Vec::new();
        let mut seen_manifests = FxHashSet::default();
        for root in &self.workspace_roots {
            let discovered = ProjectManifest::discover_all(std::slice::from_ref(root));
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
                match manifest {
                    ProjectManifest::Foundry(path) => {
                        let fallback_root = path.parent().map(PathBuf::from);
                        match Workspace::load_foundry(path) {
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
            }
        }
        info!(workspaces = ?workspaces.iter().map(Workspace::kind).collect::<Vec<_>>(), "loaded workspaces");
        self.workspaces = workspaces;
        self.refresh_flychecks()
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
        let idx = WorkspacePathIndex::new(&self.workspaces).workspace_idx_for_path(&path);
        self.workspaces[idx].add_source_file(path);
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        if self.workspaces.is_empty() {
            return;
        }
        let idx = WorkspacePathIndex::new(&self.workspaces).workspace_idx_for_path(path);
        self.workspaces[idx].remove_source_file(path);
    }

    fn refresh_flychecks(&mut self) -> Vec<DiagnosticOwner> {
        let mut removed_owners =
            self.flychecks.iter().map(FlycheckConfig::owner).collect::<FxHashSet<_>>();
        self.flychecks = self.flycheck_options.configs(&self.workspaces);

        for owner in self.flychecks.iter().map(FlycheckConfig::owner) {
            removed_owners.remove(&owner);
        }

        let mut removed_owners = removed_owners.into_iter().collect::<Vec<_>>();
        removed_owners.sort();
        info!(flychecks = ?self.flychecks.iter().map(|it| &it.id).collect::<Vec<_>>(), "loaded flychecks");
        removed_owners
    }
}

fn push_workspace(workspaces: &mut Vec<Workspace>, mut workspace: Workspace) {
    workspace.refresh_source_files();
    workspaces.push(workspace);
}

pub(crate) fn negotiate_capabilities(params: InitializeParams) -> (ServerCapabilities, Config) {
    let capabilities = params.capabilities;
    let initialization_options = params.initialization_options;
    #[allow(deprecated)]
    let root_uri = params.root_uri;
    let workspace_folders = params.workspace_folders;
    let flycheck_options = FlycheckInitializationOptions::from_json(initialization_options);

    // todo: make this absolute guaranteed
    let root_path = match root_uri.and_then(|it| it.to_file_path().ok()) {
        Some(it) => it,
        None => {
            // todo: unwrap
            env::current_dir().unwrap()
        }
    };

    // todo: make this absolute guaranteed
    // The latest LSP spec mandates clients report `workspace_folders`, but some might still report
    // `root_uri`.
    let watched_file_dynamic_registration = capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.did_change_watched_files.as_ref())
        .and_then(|capabilities| capabilities.dynamic_registration)
        .unwrap_or(false);
    let workspace_edit_document_changes = capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.workspace_edit.as_ref())
        .and_then(|capabilities| capabilities.document_changes)
        .unwrap_or(false);
    let work_done_progress =
        capabilities.window.as_ref().and_then(|window| window.work_done_progress).unwrap_or(false);
    let hierarchical_document_symbol_support = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.document_symbol.as_ref())
        .and_then(|capabilities| capabilities.hierarchical_document_symbol_support)
        .unwrap_or(false);
    let completion = CompletionClientOptions {
        snippet_support: capabilities
            .text_document
            .as_ref()
            .and_then(|text_document| text_document.completion.as_ref())
            .and_then(|capabilities| capabilities.completion_item.as_ref())
            .and_then(|capabilities| capabilities.snippet_support)
            .unwrap_or(false),
    };
    let signature_information = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.signature_help.as_ref())
        .and_then(|capabilities| capabilities.signature_information.as_ref());
    let signature_help = SignatureHelpClientOptions {
        label_offsets: signature_information
            .and_then(|settings| settings.parameter_information.as_ref())
            .and_then(|settings| settings.label_offset_support)
            .unwrap_or(false),
        markdown_documentation: signature_information
            .and_then(|settings| settings.documentation_format.as_ref())
            .is_some_and(|formats| {
                formats.iter().find(|format| {
                    matches!(
                        **format,
                        lsp_types::MarkupKind::Markdown | lsp_types::MarkupKind::PlainText
                    )
                }) == Some(&lsp_types::MarkupKind::Markdown)
            }),
        signature_active_parameter: signature_information
            .and_then(|settings| settings.active_parameter_support)
            .unwrap_or(false),
    };

    let workspace_roots = workspace_folders
        .map(|workspaces| {
            workspaces.into_iter().filter_map(|it| it.uri.to_file_path().ok()).collect::<Vec<_>>()
        })
        .filter(|workspaces| !workspaces.is_empty())
        .unwrap_or_else(|| vec![root_path]);

    (
        ServerCapabilities {
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".into(), "/".into(), "*".into()]),
                ..Default::default()
            }),
            declaration_provider: Some(DeclarationCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: None,
                inter_file_dependencies: true,
                workspace_diagnostics: false,
                ..Default::default()
            })),
            document_link_provider: Some(DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: commands::ALL.into_iter().map(str::to_owned).collect(),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            document_symbol_provider: Some(OneOf::Left(true)),
            document_highlight_provider: Some(OneOf::Left(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            inlay_hint_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            selection_range_provider: Some(SelectionRangeProviderCapability::Simple(true)),
            rename_provider: Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            })),
            signature_help_provider: Some(SignatureHelpOptions {
                trigger_characters: Some(vec!["(".into(), ",".into()]),
                retrigger_characters: Some(vec![",".into()]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            text_document_sync: Some(TextDocumentSyncCapability::Options(
                TextDocumentSyncOptions {
                    open_close: Some(true),
                    change: Some(TextDocumentSyncKind::INCREMENTAL),
                    will_save: Some(true),
                    save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                        include_text: Some(false),
                    })),
                    ..Default::default()
                },
            )),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            ..Default::default()
        },
        Config {
            workspace_roots,
            flycheck_options,
            watched_file_dynamic_registration,
            workspace_edit_document_changes,
            work_done_progress,
            hierarchical_document_symbol_support,
            completion,
            signature_help,
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::TestProject, workspace::WorkspaceKind};
    use lsp_types::{
        CompletionClientCapabilities, CompletionItemCapability,
        DidChangeWatchedFilesClientCapabilities, DocumentSymbolClientCapabilities, MarkupKind,
        OneOf, ParameterInformationSettings, RenameOptions, SignatureHelpClientCapabilities,
        SignatureInformationSettings, TextDocumentClientCapabilities, TextDocumentSyncCapability,
        TextDocumentSyncSaveOptions, TypeDefinitionProviderCapability, WindowClientCapabilities,
        WorkspaceClientCapabilities, WorkspaceEditClientCapabilities,
    };

    #[test]
    fn negotiate_capabilities_records_work_done_progress_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_work_done_progress());

        let mut params = InitializeParams::default();
        params.capabilities.window = Some(WindowClientCapabilities {
            work_done_progress: Some(false),
            ..Default::default()
        });
        let (_, config) = negotiate_capabilities(params.clone());
        assert!(!config.supports_work_done_progress());

        params.capabilities.window.as_mut().unwrap().work_done_progress = Some(true);
        let (_, config) = negotiate_capabilities(params);
        assert!(config.supports_work_done_progress());
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
    fn negotiate_capabilities_records_document_changes_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_workspace_edit_document_changes());

        let mut params = InitializeParams::default();
        params.capabilities.workspace = Some(WorkspaceClientCapabilities {
            workspace_edit: Some(WorkspaceEditClientCapabilities {
                document_changes: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.supports_workspace_edit_document_changes());
    }

    #[test]
    fn negotiate_capabilities_advertises_symbol_providers() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());

        let completion_provider = capabilities.completion_provider.unwrap();
        assert_eq!(
            completion_provider.trigger_characters,
            Some(vec![".".to_string(), "/".to_string(), "*".to_string()])
        );
        assert_eq!(capabilities.declaration_provider, Some(DeclarationCapability::Simple(true)));
        assert_eq!(capabilities.definition_provider, Some(OneOf::Left(true)));
        assert_eq!(
            capabilities.implementation_provider,
            Some(ImplementationProviderCapability::Simple(true))
        );
        assert_eq!(
            capabilities.type_definition_provider,
            Some(TypeDefinitionProviderCapability::Simple(true))
        );
        assert_eq!(capabilities.document_formatting_provider, Some(OneOf::Left(true)));
        assert_eq!(capabilities.document_symbol_provider, Some(OneOf::Left(true)));
        assert_eq!(capabilities.hover_provider, Some(HoverProviderCapability::Simple(true)));
        let document_link_provider = capabilities.document_link_provider.unwrap();
        assert_eq!(document_link_provider.resolve_provider, Some(false));
        assert_eq!(capabilities.inlay_hint_provider, Some(OneOf::Left(true)));
        assert_eq!(capabilities.document_highlight_provider, Some(OneOf::Left(true)));
        assert_eq!(capabilities.references_provider, Some(OneOf::Left(true)));
        assert_eq!(
            capabilities.selection_range_provider,
            Some(SelectionRangeProviderCapability::Simple(true))
        );
        assert_eq!(
            capabilities.rename_provider,
            Some(OneOf::Right(RenameOptions {
                prepare_provider: Some(true),
                work_done_progress_options: Default::default(),
            }))
        );
        let signature_help_provider = capabilities.signature_help_provider.unwrap();
        assert_eq!(
            signature_help_provider.trigger_characters,
            Some(vec!["(".to_string(), ",".to_string()])
        );
        assert_eq!(signature_help_provider.retrigger_characters, Some(vec![",".to_string()]));
        assert_eq!(capabilities.workspace_symbol_provider, Some(OneOf::Left(true)));

        let TextDocumentSyncCapability::Options(sync_options) =
            capabilities.text_document_sync.unwrap()
        else {
            panic!("expected text document sync options");
        };
        assert_eq!(sync_options.will_save, Some(true));
        assert_eq!(sync_options.will_save_wait_until, None);
        let TextDocumentSyncSaveOptions::SaveOptions(save_options) = sync_options.save.unwrap()
        else {
            panic!("expected save options");
        };
        assert_eq!(save_options.include_text, Some(false));
    }

    #[test]
    fn negotiate_capabilities_advertises_document_diagnostics() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());

        assert_eq!(
            capabilities.diagnostic_provider,
            Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: None,
                inter_file_dependencies: true,
                workspace_diagnostics: false,
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }))
        );
    }

    #[test]
    fn negotiate_capabilities_advertises_cache_commands() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());

        assert_eq!(
            capabilities.execute_command_provider,
            Some(ExecuteCommandOptions {
                commands: vec!["solar.clearCache".into(), "solar.reindex".into()],
                work_done_progress_options: WorkDoneProgressOptions::default(),
            })
        );
    }

    #[test]
    fn negotiate_capabilities_records_hierarchical_document_symbol_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_hierarchical_document_symbols());

        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            document_symbol: Some(DocumentSymbolClientCapabilities {
                hierarchical_document_symbol_support: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.supports_hierarchical_document_symbols());
    }

    #[test]
    fn negotiate_capabilities_defaults_completion_snippet_support_to_false() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());

        assert!(!config.completion_options().snippet_support);
    }

    #[test]
    fn negotiate_capabilities_records_completion_snippet_support() {
        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    snippet_support: Some(true),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.completion_options().snippet_support);
    }

    #[test]
    fn negotiate_capabilities_records_signature_help_label_offset_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        let options = config.signature_help_options();
        assert!(!options.label_offsets);
        assert!(!options.markdown_documentation);
        assert!(!options.signature_active_parameter);

        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            signature_help: Some(SignatureHelpClientCapabilities {
                signature_information: Some(SignatureInformationSettings {
                    documentation_format: Some(vec![MarkupKind::Markdown]),
                    parameter_information: Some(ParameterInformationSettings {
                        label_offset_support: Some(true),
                    }),
                    active_parameter_support: Some(true),
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params.clone());
        let options = config.signature_help_options();
        assert!(options.label_offsets);
        assert!(options.markdown_documentation);
        assert!(options.signature_active_parameter);

        params
            .capabilities
            .text_document
            .as_mut()
            .unwrap()
            .signature_help
            .as_mut()
            .unwrap()
            .signature_information
            .as_mut()
            .unwrap()
            .documentation_format = Some(vec![MarkupKind::PlainText, MarkupKind::Markdown]);
        let (_, config) = negotiate_capabilities(params);
        assert!(!config.signature_help_options().markdown_documentation);
    }

    #[test]
    fn negotiate_capabilities_records_configured_flychecks() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "custom",
                "command": "custom-lint",
                "args": ["--json"],
                "output": "solc-json"
            }]
        }));
        let (_, mut config) = negotiate_capabilities(params);
        config.rediscover_workspaces();

        let flychecks = config.flychecks_for_path(&project.path("/src/Test.sol"));

        assert_eq!(flychecks.len(), 1);
        assert_eq!(flychecks[0].id, "custom");
        assert_eq!(flychecks[0].command, PathBuf::from("custom-lint"));
        assert_eq!(flychecks[0].args, ["--json"]);
        assert_eq!(flychecks[0].cwd, project.root());
    }

    #[test]
    fn negotiate_capabilities_records_configured_forge_path() {
        let (_, default_config) = negotiate_capabilities(InitializeParams::default());
        assert_eq!(default_config.forge_path(), PathBuf::from("forge"));

        let params = InitializeParams {
            initialization_options: Some(serde_json::json!({
                "forgePath": "/tools/forge"
            })),
            ..Default::default()
        };

        let (_, config) = negotiate_capabilities(params);

        assert_eq!(config.forge_path(), PathBuf::from("/tools/forge"));
    }

    #[test]
    fn formatter_root_uses_nearest_foundry_project_workspace_or_file_parent() {
        let project = TestProject::from_fixture(
            r#"
            //- /workspace/A.sol
            contract A {}

            //- /workspace/nested/B.sol
            contract B {}

            //- /outside/foundry.toml

            //- /outside/src/C.sol
            contract C {}

            //- /standalone/D.sol
            contract D {}
            "#,
        );
        let config = project.config_with_roots(&["/workspace", "/workspace/nested"]);

        assert_eq!(
            config.formatter_root_for_path(&project.path("/workspace/nested/B.sol")),
            Some(project.path("/workspace/nested"))
        );
        assert_eq!(
            config.formatter_root_for_path(&project.path("/workspace/A.sol")),
            Some(project.path("/workspace"))
        );
        assert_eq!(
            config.formatter_root_for_path(&project.path("/outside/src/C.sol")),
            Some(project.path("/outside"))
        );
        assert_eq!(
            config.formatter_root_for_path(&project.path("/standalone/D.sol")),
            Some(project.path("/standalone"))
        );
    }

    #[test]
    fn rediscover_workspaces_loads_nested_discovered_project() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml

            //- /packages/token/foundry.toml
            [profile.default]
            src = "contracts"
            "#,
        );

        let config = project.config();
        let nested = config
            .workspaces()
            .iter()
            .find(|workspace| {
                workspace.compile_opts().base_path.as_deref()
                    == Some(project.path("/packages/token").as_path())
            })
            .unwrap();

        assert_eq!(config.workspaces().len(), 2);
        assert!(
            config.workspaces().iter().all(|workspace| workspace.kind() == WorkspaceKind::Foundry)
        );
        assert_eq!(nested.source_roots(), &[project.path("/packages/token/contracts")]);
    }

    #[test]
    fn rediscover_workspaces_reports_removed_flycheck_owners() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Test.sol
            contract Test {}
            "#,
        );
        let mut params = project.initialize_params();
        params.initialization_options = Some(serde_json::json!({
            "flychecks": [{
                "id": "custom",
                "command": "custom-lint",
                "output": "solc-json"
            }]
        }));
        let (_, mut config) = negotiate_capabilities(params);
        assert!(config.rediscover_workspaces().is_empty());

        config.remove_workspace(project.root());
        let removed_owners = config.rediscover_workspaces();

        assert_eq!(
            removed_owners,
            vec![DiagnosticOwner::Flycheck {
                id: "custom".into(),
                workspace: project.root().to_path_buf()
            }]
        );
    }

    #[test]
    fn rediscover_workspaces_loads_manifests_and_falls_back_to_naked_roots() {
        let project = TestProject::from_fixture(
            r#"
            //- /configured/foundry.toml
            [profile.default]
            src = "contracts"

            //- /naked/.keep
            "#,
        );
        let mut config = project.config_with_roots(&["/configured", "/naked"]);

        assert_eq!(config.workspaces().len(), 2);
        let foundry = config
            .workspaces()
            .iter()
            .find(|workspace| workspace.kind() == WorkspaceKind::Foundry)
            .unwrap();
        assert_eq!(foundry.source_roots(), &[project.path("/configured/contracts")]);

        project.remove_file("/configured/foundry.toml");
        config.rediscover_workspaces();

        assert_eq!(config.workspaces().len(), 2);
        assert!(
            config.workspaces().iter().all(|workspace| workspace.kind() == WorkspaceKind::Naked)
        );
    }

    #[test]
    fn rediscover_workspaces_keeps_naked_root_after_manifest_load_error() {
        let project = TestProject::from_fixture(
            r#"
            //- /broken/foundry.toml
            not valid toml =

            //- /configured/foundry.toml
            [profile.default]
            src = "contracts"
            "#,
        );
        let config = project.config_with_roots(&["/broken", "/configured"]);

        assert_eq!(config.workspaces().len(), 2);
        assert!(config.workspaces().iter().any(|workspace| {
            workspace.kind() == WorkspaceKind::Naked
                && workspace.compile_opts().base_path.as_deref()
                    == Some(project.path("/broken").as_path())
        }));
        assert!(config.workspaces().iter().any(|workspace| {
            workspace.kind() == WorkspaceKind::Foundry
                && workspace.source_roots() == [project.path("/configured/contracts")]
        }));
    }
}

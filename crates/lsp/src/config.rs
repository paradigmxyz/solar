use crate::{
    commands,
    diagnostics::DiagnosticOwner,
    file_operations::FileMoveBatch,
    flycheck::{FlycheckConfig, FlycheckInitializationOptions},
    workspace::{Workspace, WorkspaceKind, WorkspacePathIndex, manifest::ProjectManifest},
};
use lsp_types::{
    CallHierarchyServerCapability, CodeActionKind, CodeActionOptions, CodeActionProviderCapability,
    CodeLensOptions as CodeLensServerOptions, CompletionOptions, DeclarationCapability,
    DiagnosticOptions, DiagnosticServerCapabilities, DocumentLinkOptions, ExecuteCommandOptions,
    FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
    FileOperationRegistrationOptions, FoldingRangeProviderCapability, HoverProviderCapability,
    ImplementationProviderCapability, InitializeParams, MarkupKind, OneOf, RenameOptions,
    SaveOptions, SelectionRangeProviderCapability, ServerCapabilities, SignatureHelpOptions,
    TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions, TypeDefinitionProviderCapability, Url, WorkDoneProgressOptions,
    WorkspaceFileOperationsServerCapabilities, WorkspaceFolder, WorkspaceServerCapabilities,
};
use serde::Deserialize;
use solar_interface::data_structures::map::FxHashSet;
use std::{
    env,
    path::{Path, PathBuf},
    time::Duration,
};
use tracing::{info, warn};

/// The LSP config.
///
/// This struct is internal only and should not be serialized or deserialized. Instead, values in
/// this struct are the full view of all merged config sources, such as `initialization_opts`,
/// on-disk config files (e.g. `foundry.toml`).
#[derive(Clone, Debug)]
pub(crate) struct Config {
    workspace_roots: Vec<PathBuf>,
    workspaces: Vec<Workspace>,
    flycheck_options: FlycheckInitializationOptions,
    flychecks: Vec<FlycheckConfig>,
    watched_file_dynamic_registration: bool,
    workspace_edit_document_changes: bool,
    code_action_literals: bool,
    code_action_is_preferred: bool,
    publish_diagnostics_data: bool,
    pull_diagnostics_data: bool,
    code_lens_refresh_support: bool,
    diagnostic_refresh_support: bool,
    inlay_hint_refresh_support: bool,
    work_done_progress: bool,
    hierarchical_document_symbol_support: bool,
    completion: CompletionClientOptions,
    signature_help: SignatureHelpClientOptions,
    source_change_debounce: Duration,
    progress_delay: Duration,
    progress_create_timeout: Duration,
    formatter_timeout: Duration,
    flycheck_timeout: Duration,
    code_lens: CodeLensConfig,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            workspace_roots: Vec::new(),
            workspaces: Vec::new(),
            flycheck_options: FlycheckInitializationOptions::default(),
            flychecks: Vec::new(),
            watched_file_dynamic_registration: false,
            workspace_edit_document_changes: false,
            code_action_literals: false,
            code_action_is_preferred: false,
            publish_diagnostics_data: false,
            pull_diagnostics_data: false,
            code_lens_refresh_support: false,
            diagnostic_refresh_support: false,
            inlay_hint_refresh_support: false,
            work_done_progress: false,
            hierarchical_document_symbol_support: false,
            completion: CompletionClientOptions::default(),
            signature_help: SignatureHelpClientOptions::default(),
            source_change_debounce: Duration::from_millis(250),
            progress_delay: Duration::from_millis(250),
            progress_create_timeout: Duration::from_secs(1),
            formatter_timeout: Duration::from_secs(30),
            flycheck_timeout: Duration::from_secs(30),
            code_lens: CodeLensConfig::default(),
        }
    }
}

#[derive(Clone, Copy, Debug)]
pub(crate) struct CompletionClientOptions {
    pub(crate) snippet_support: bool,
    pub(crate) markdown_documentation: bool,
    pub(crate) resolve_documentation: bool,
}

impl Default for CompletionClientOptions {
    fn default() -> Self {
        Self { snippet_support: false, markdown_documentation: false, resolve_documentation: true }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct SignatureHelpClientOptions {
    pub(crate) label_offsets: bool,
    pub(crate) markdown_documentation: bool,
    pub(crate) signature_active_parameter: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub(crate) struct CodeLensConfig {
    pub(crate) enable: bool,
    pub(crate) selectors: bool,
    pub(crate) references: bool,
    pub(crate) inheritance: bool,
    pub(crate) client_commands: bool,
}

impl Default for CodeLensConfig {
    fn default() -> Self {
        Self {
            enable: true,
            selectors: true,
            references: true,
            inheritance: true,
            client_commands: false,
        }
    }
}

impl CodeLensConfig {
    pub(crate) fn is_active(self) -> bool {
        self.enable && (self.selectors || self.references || self.inheritance)
    }

    fn from_json(value: Option<serde_json::Value>) -> Self {
        value
            .and_then(|value| {
                value.get("codeLens").cloned().and_then(|value| serde_json::from_value(value).ok())
            })
            .unwrap_or_default()
    }
}

impl Config {
    pub(crate) fn supports_watched_file_dynamic_registration(&self) -> bool {
        self.watched_file_dynamic_registration
    }

    pub(crate) fn supports_workspace_edit_document_changes(&self) -> bool {
        self.workspace_edit_document_changes
    }

    pub(crate) fn supports_code_action_literals(&self) -> bool {
        self.code_action_literals
    }

    pub(crate) fn supports_code_action_is_preferred(&self) -> bool {
        self.code_action_is_preferred
    }

    pub(crate) fn supports_publish_diagnostics_data(&self) -> bool {
        self.publish_diagnostics_data
    }

    pub(crate) fn supports_pull_diagnostics_data(&self) -> bool {
        self.pull_diagnostics_data
    }

    pub(crate) fn supports_code_action_diagnostic_data(&self) -> bool {
        self.publish_diagnostics_data || self.pull_diagnostics_data
    }

    pub(crate) fn supports_code_lens_refresh(&self) -> bool {
        self.code_lens_refresh_support
    }

    pub(crate) fn supports_diagnostic_refresh(&self) -> bool {
        self.diagnostic_refresh_support
    }

    pub(crate) fn supports_inlay_hint_refresh(&self) -> bool {
        self.inlay_hint_refresh_support
    }

    pub(crate) fn supports_work_done_progress(&self) -> bool {
        self.work_done_progress
    }

    pub(crate) fn supports_hierarchical_document_symbols(&self) -> bool {
        self.hierarchical_document_symbol_support
    }

    pub(crate) fn source_change_debounce(&self) -> Duration {
        self.source_change_debounce
    }

    pub(crate) fn progress_delay(&self) -> Duration {
        self.progress_delay
    }

    pub(crate) fn progress_create_timeout(&self) -> Duration {
        self.progress_create_timeout
    }

    pub(crate) fn formatter_timeout(&self) -> Duration {
        self.formatter_timeout
    }

    pub(crate) fn flycheck_timeout(&self) -> Duration {
        self.flycheck_timeout
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

    pub(crate) fn code_lens_options(&self) -> CodeLensConfig {
        self.code_lens
    }

    #[cfg(test)]
    pub(crate) fn enable_signature_help_label_offsets(&mut self) {
        self.signature_help.label_offsets = true;
    }

    #[cfg(test)]
    pub(crate) fn enable_code_lens_client_commands(&mut self) {
        self.code_lens.client_commands = true;
    }

    pub(crate) fn workspaces(&self) -> &[Workspace] {
        &self.workspaces
    }

    pub(crate) fn workspace_roots(&self) -> &[PathBuf] {
        &self.workspace_roots
    }

    pub(crate) fn tracks_source_file(&self, path: &Path) -> bool {
        self.workspaces.iter().any(|workspace| workspace.tracks_disk_file(path))
    }

    pub(crate) fn tracked_source_files_under(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut files = self
            .workspaces
            .iter()
            .flat_map(Workspace::source_files)
            .filter(|path| roots.iter().any(|root| path.starts_with(root)))
            .cloned()
            .collect::<Vec<_>>();
        files.sort();
        files.dedup();
        files
    }

    pub(crate) fn file_operation_paths_under(&self, roots: &[PathBuf]) -> Vec<PathBuf> {
        let mut paths = self.tracked_source_files_under(roots);
        paths.extend(self.workspaces.iter().filter_map(|workspace| {
            if workspace.kind() != WorkspaceKind::Foundry {
                return None;
            }
            let manifest = workspace.compile_opts().base_path.as_ref()?.join("foundry.toml");
            roots.iter().any(|root| manifest.starts_with(root)).then_some(manifest)
        }));
        paths.sort();
        paths.dedup();
        paths
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
        self.flychecks
            .iter()
            .filter(|flycheck| {
                flycheck.applies_to(path)
                    || self.workspaces.iter().any(|workspace| {
                        workspace.compile_opts().base_path.as_deref()
                            == Some(flycheck.workspace_root.as_path())
                            && workspace.tracks_flycheck_file(path)
                    })
            })
            .cloned()
            .collect()
    }

    pub(crate) fn flycheck_owners(&self) -> impl Iterator<Item = DiagnosticOwner> + '_ {
        self.flychecks.iter().map(FlycheckConfig::owner)
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
        for path in paths {
            if !self.workspace_roots.contains(&path) {
                self.workspace_roots.push(path);
            }
        }
    }

    pub(crate) fn reconcile_workspace_roots(
        &mut self,
        moves: &FileMoveBatch,
        deleted_paths: &[PathBuf],
    ) {
        self.workspace_roots
            .retain(|root| !deleted_paths.iter().any(|deleted| root.starts_with(deleted)));
        for root in &mut self.workspace_roots {
            if let Some((_, new_root)) = moves.map_path(root) {
                *root = new_root;
            }
        }
        let mut seen = FxHashSet::default();
        self.workspace_roots.retain(|root| seen.insert(root.clone()));
    }

    pub(crate) fn add_source_file(&mut self, path: PathBuf) {
        if self.workspaces.is_empty() {
            return;
        }
        let idx = WorkspacePathIndex::new(&self.workspaces).workspace_idx_for_path(&path);
        for (workspace_idx, workspace) in self.workspaces.iter_mut().enumerate() {
            if workspace_idx != idx {
                workspace.add_flycheck_source_file(&path);
            }
        }
        self.workspaces[idx].add_source_file(path);
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        if self.workspaces.is_empty() {
            return;
        }
        let idx = WorkspacePathIndex::new(&self.workspaces).workspace_idx_for_path(path);
        for (workspace_idx, workspace) in self.workspaces.iter_mut().enumerate() {
            if workspace_idx != idx {
                workspace.remove_flycheck_source_file(path);
            }
        }
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

fn workspace_file_operation_options() -> FileOperationRegistrationOptions {
    FileOperationRegistrationOptions {
        filters: vec![
            FileOperationFilter {
                scheme: Some("file".into()),
                pattern: FileOperationPattern {
                    glob: "**/*.sol".into(),
                    matches: Some(FileOperationPatternKind::File),
                    options: None,
                },
            },
            FileOperationFilter {
                scheme: Some("file".into()),
                pattern: FileOperationPattern {
                    glob: "**".into(),
                    matches: Some(FileOperationPatternKind::Folder),
                    options: None,
                },
            },
        ],
    }
}

fn workspace_roots_from_initialize(
    workspace_folders: Option<Vec<WorkspaceFolder>>,
    root_uri: Option<Url>,
    fallback_root: impl FnOnce() -> Option<PathBuf>,
) -> Vec<PathBuf> {
    let workspace_roots = workspace_folders
        .map(|workspaces| {
            workspaces.into_iter().filter_map(|it| it.uri.to_file_path().ok()).collect::<Vec<_>>()
        })
        .unwrap_or_default();
    if !workspace_roots.is_empty() {
        return workspace_roots;
    }

    root_uri.and_then(|uri| uri.to_file_path().ok()).or_else(fallback_root).into_iter().collect()
}

#[cfg(test)]
pub(crate) fn negotiate_capabilities(params: InitializeParams) -> (ServerCapabilities, Config) {
    negotiate_capabilities_with_pull_diagnostic_data(params, false)
}

pub(crate) fn negotiate_capabilities_with_pull_diagnostic_data(
    params: InitializeParams,
    pull_diagnostics_data: bool,
) -> (ServerCapabilities, Config) {
    let capabilities = params.capabilities;
    let initialization_options = params.initialization_options;
    #[allow(deprecated)]
    let root_uri = params.root_uri;
    let workspace_folders = params.workspace_folders;
    let flycheck_options = FlycheckInitializationOptions::from_json(initialization_options.clone());
    let code_lens = CodeLensConfig::from_json(initialization_options);

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
    let code_lens_refresh_support = capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.code_lens.as_ref())
        .and_then(|capabilities| capabilities.refresh_support)
        .unwrap_or(false);
    let diagnostic_refresh_support = capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.diagnostic.as_ref())
        .and_then(|capabilities| capabilities.refresh_support)
        .unwrap_or(false);
    let inlay_hint_refresh_support = capabilities
        .workspace
        .as_ref()
        .and_then(|workspace| workspace.inlay_hint.as_ref())
        .and_then(|capabilities| capabilities.refresh_support)
        .unwrap_or(false);
    let code_action = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.code_action.as_ref());
    let code_action_literals = code_action
        .and_then(|capabilities| capabilities.code_action_literal_support.as_ref())
        .is_some();
    let code_action_is_preferred =
        code_action.and_then(|capabilities| capabilities.is_preferred_support).unwrap_or(false);
    let publish_diagnostics_data = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.publish_diagnostics.as_ref())
        .and_then(|capabilities| capabilities.data_support)
        .unwrap_or(false);
    let work_done_progress =
        capabilities.window.as_ref().and_then(|window| window.work_done_progress).unwrap_or(false);
    let hierarchical_document_symbol_support = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.document_symbol.as_ref())
        .and_then(|capabilities| capabilities.hierarchical_document_symbol_support)
        .unwrap_or(false);
    let completion_item = capabilities
        .text_document
        .as_ref()
        .and_then(|text_document| text_document.completion.as_ref())
        .and_then(|capabilities| capabilities.completion_item.as_ref());
    let completion = CompletionClientOptions {
        snippet_support: completion_item
            .and_then(|capabilities| capabilities.snippet_support)
            .unwrap_or(false),
        markdown_documentation: prefers_markdown_documentation(
            completion_item.and_then(|capabilities| capabilities.documentation_format.as_deref()),
        ),
        resolve_documentation: completion_item
            .and_then(|capabilities| capabilities.resolve_support.as_ref())
            .is_none_or(|support| {
                support.properties.iter().any(|property| property == "documentation")
            }),
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
        markdown_documentation: prefers_markdown_documentation(
            signature_information.and_then(|settings| settings.documentation_format.as_deref()),
        ),
        signature_active_parameter: signature_information
            .and_then(|settings| settings.active_parameter_support)
            .unwrap_or(false),
    };

    let workspace_roots =
        workspace_roots_from_initialize(workspace_folders, root_uri, || env::current_dir().ok());
    let file_operations = workspace_file_operation_options();

    (
        ServerCapabilities {
            completion_provider: Some(CompletionOptions {
                trigger_characters: Some(vec![".".into(), "/".into(), "*".into()]),
                resolve_provider: Some(true),
                ..Default::default()
            }),
            declaration_provider: Some(DeclarationCapability::Simple(true)),
            definition_provider: Some(OneOf::Left(true)),
            implementation_provider: Some(ImplementationProviderCapability::Simple(true)),
            type_definition_provider: Some(TypeDefinitionProviderCapability::Simple(true)),
            document_formatting_provider: Some(OneOf::Left(true)),
            folding_range_provider: Some(FoldingRangeProviderCapability::Simple(true)),
            diagnostic_provider: Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: None,
                inter_file_dependencies: true,
                workspace_diagnostics: true,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: Some(true),
                },
            })),
            document_link_provider: Some(DocumentLinkOptions {
                resolve_provider: Some(false),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            code_action_provider: code_action_literals.then(|| {
                CodeActionProviderCapability::Options(CodeActionOptions {
                    code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                    work_done_progress_options: WorkDoneProgressOptions::default(),
                    resolve_provider: Some(false),
                })
            }),
            execute_command_provider: Some(ExecuteCommandOptions {
                commands: commands::ALL.into_iter().map(str::to_owned).collect(),
                work_done_progress_options: WorkDoneProgressOptions::default(),
            }),
            document_symbol_provider: Some(OneOf::Left(true)),
            code_lens_provider: Some(CodeLensServerOptions { resolve_provider: Some(false) }),
            document_highlight_provider: Some(OneOf::Left(true)),
            hover_provider: Some(HoverProviderCapability::Simple(true)),
            inlay_hint_provider: Some(OneOf::Left(true)),
            references_provider: Some(OneOf::Left(true)),
            call_hierarchy_provider: Some(CallHierarchyServerCapability::Simple(true)),
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
            workspace: Some(WorkspaceServerCapabilities {
                file_operations: Some(WorkspaceFileOperationsServerCapabilities {
                    did_create: Some(file_operations.clone()),
                    will_create: Some(file_operations.clone()),
                    did_rename: Some(file_operations.clone()),
                    will_rename: Some(file_operations.clone()),
                    did_delete: Some(file_operations.clone()),
                    will_delete: Some(file_operations),
                }),
                ..Default::default()
            }),
            workspace_symbol_provider: Some(OneOf::Left(true)),
            ..Default::default()
        },
        Config {
            workspace_roots,
            flycheck_options,
            watched_file_dynamic_registration,
            workspace_edit_document_changes,
            code_action_literals,
            code_action_is_preferred,
            publish_diagnostics_data,
            pull_diagnostics_data,
            code_lens_refresh_support,
            diagnostic_refresh_support,
            inlay_hint_refresh_support,
            work_done_progress,
            hierarchical_document_symbol_support,
            completion,
            signature_help,
            code_lens,
            ..Default::default()
        },
    )
}

fn prefers_markdown_documentation(formats: Option<&[MarkupKind]>) -> bool {
    formats.is_some_and(|formats| {
        formats.iter().find(|format| matches!(format, MarkupKind::Markdown | MarkupKind::PlainText))
            == Some(&MarkupKind::Markdown)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{test_support::TestProject, workspace::WorkspaceKind};
    use lsp_types::{
        CallHierarchyServerCapability, CodeActionClientCapabilities, CodeActionKind,
        CodeActionKindLiteralSupport, CodeActionLiteralSupport, CodeActionOptions,
        CodeActionProviderCapability, CodeLensWorkspaceClientCapabilities,
        CompletionClientCapabilities, CompletionItemCapability,
        CompletionItemCapabilityResolveSupport, DiagnosticWorkspaceClientCapabilities,
        DidChangeWatchedFilesClientCapabilities, DocumentSymbolClientCapabilities,
        FileOperationFilter, FileOperationPattern, FileOperationPatternKind,
        FileOperationRegistrationOptions, InlayHintWorkspaceClientCapabilities, MarkupKind, OneOf,
        ParameterInformationSettings, PublishDiagnosticsClientCapabilities, RenameOptions,
        SignatureHelpClientCapabilities, SignatureInformationSettings,
        TextDocumentClientCapabilities, TextDocumentSyncCapability, TextDocumentSyncSaveOptions,
        TypeDefinitionProviderCapability, WindowClientCapabilities, WorkspaceClientCapabilities,
        WorkspaceEditClientCapabilities,
    };

    #[test]
    fn workspace_folders_skip_root_fallback() {
        let workspace_root = env::temp_dir().join("solar-lsp-workspace");
        let workspace_folders = Some(vec![WorkspaceFolder {
            uri: Url::from_file_path(&workspace_root).unwrap(),
            name: "workspace".into(),
        }]);

        let roots = workspace_roots_from_initialize(workspace_folders, None, || {
            panic!("root fallback should not be evaluated")
        });

        assert_eq!(roots, [workspace_root]);
    }

    #[test]
    fn unavailable_root_fallback_leaves_workspace_roots_empty() {
        let roots = workspace_roots_from_initialize(None, None, || None);

        assert!(roots.is_empty());
    }

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
    fn negotiate_capabilities_records_code_lens_refresh_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_code_lens_refresh());

        let mut params = InitializeParams::default();
        params.capabilities.workspace = Some(WorkspaceClientCapabilities {
            code_lens: Some(CodeLensWorkspaceClientCapabilities { refresh_support: Some(true) }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.supports_code_lens_refresh());
    }

    #[test]
    fn negotiate_capabilities_records_pull_refresh_support_independently() {
        for (diagnostic, inlay_hint) in [(false, false), (true, false), (false, true), (true, true)]
        {
            let mut params = InitializeParams::default();
            params.capabilities.workspace = Some(WorkspaceClientCapabilities {
                diagnostic: Some(DiagnosticWorkspaceClientCapabilities {
                    refresh_support: Some(diagnostic),
                }),
                inlay_hint: Some(InlayHintWorkspaceClientCapabilities {
                    refresh_support: Some(inlay_hint),
                }),
                ..Default::default()
            });

            let (_, config) = negotiate_capabilities(params);

            assert_eq!(config.supports_diagnostic_refresh(), diagnostic);
            assert_eq!(config.supports_inlay_hint_refresh(), inlay_hint);
        }

        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_diagnostic_refresh());
        assert!(!config.supports_inlay_hint_refresh());
    }

    #[test]
    fn negotiate_capabilities_reads_code_lens_initialization_options() {
        let params = InitializeParams {
            initialization_options: Some(serde_json::json!({
                "codeLens": {
                    "enable": false,
                    "selectors": false,
                    "references": true,
                    "inheritance": false,
                    "clientCommands": true,
                }
            })),
            ..Default::default()
        };

        let (_, config) = negotiate_capabilities(params);

        assert_eq!(
            config.code_lens_options(),
            CodeLensConfig {
                enable: false,
                selectors: false,
                references: true,
                inheritance: false,
                client_commands: true,
            }
        );
    }

    #[test]
    fn code_lens_activity_requires_an_enabled_lens_kind() {
        let inactive = CodeLensConfig {
            enable: true,
            selectors: false,
            references: false,
            inheritance: false,
            client_commands: false,
        };
        assert!(!inactive.is_active());
        assert!(!CodeLensConfig { enable: false, selectors: true, ..inactive }.is_active());
        assert!(!CodeLensConfig { client_commands: true, ..inactive }.is_active());

        for active in [
            CodeLensConfig { selectors: true, ..inactive },
            CodeLensConfig { references: true, ..inactive },
            CodeLensConfig { inheritance: true, ..inactive },
        ] {
            assert!(active.is_active());
        }
    }

    #[test]
    fn negotiate_capabilities_advertises_symbol_providers() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());

        let completion_provider = capabilities.completion_provider.unwrap();
        assert_eq!(
            completion_provider.trigger_characters,
            Some(vec![".".to_string(), "/".to_string(), "*".to_string()])
        );
        assert_eq!(completion_provider.resolve_provider, Some(true));
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
        assert_eq!(
            capabilities.folding_range_provider,
            Some(FoldingRangeProviderCapability::Simple(true))
        );
        assert_eq!(capabilities.document_symbol_provider, Some(OneOf::Left(true)));
        assert_eq!(
            capabilities.code_lens_provider,
            Some(CodeLensServerOptions { resolve_provider: Some(false) })
        );
        assert_eq!(capabilities.hover_provider, Some(HoverProviderCapability::Simple(true)));
        let document_link_provider = capabilities.document_link_provider.unwrap();
        assert_eq!(document_link_provider.resolve_provider, Some(false));
        assert_eq!(capabilities.inlay_hint_provider, Some(OneOf::Left(true)));
        assert_eq!(capabilities.document_highlight_provider, Some(OneOf::Left(true)));
        assert_eq!(capabilities.references_provider, Some(OneOf::Left(true)));
        assert_eq!(
            capabilities.call_hierarchy_provider,
            Some(CallHierarchyServerCapability::Simple(true))
        );
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
    fn negotiate_capabilities_advertises_workspace_file_operations() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());
        let operations = capabilities.workspace.unwrap().file_operations.unwrap();
        let options = FileOperationRegistrationOptions {
            filters: vec![
                FileOperationFilter {
                    scheme: Some("file".into()),
                    pattern: FileOperationPattern {
                        glob: "**/*.sol".into(),
                        matches: Some(FileOperationPatternKind::File),
                        options: None,
                    },
                },
                FileOperationFilter {
                    scheme: Some("file".into()),
                    pattern: FileOperationPattern {
                        glob: "**".into(),
                        matches: Some(FileOperationPatternKind::Folder),
                        options: None,
                    },
                },
            ],
        };

        assert_eq!(operations.did_create, Some(options.clone()));
        assert_eq!(operations.will_create, Some(options.clone()));
        assert_eq!(operations.did_rename, Some(options.clone()));
        assert_eq!(operations.will_rename, Some(options.clone()));
        assert_eq!(operations.did_delete, Some(options.clone()));
        assert_eq!(operations.will_delete, Some(options));
    }

    #[test]
    fn negotiate_capabilities_advertises_document_diagnostics() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());

        assert_eq!(
            capabilities.diagnostic_provider,
            Some(DiagnosticServerCapabilities::Options(DiagnosticOptions {
                identifier: None,
                inter_file_dependencies: true,
                workspace_diagnostics: true,
                work_done_progress_options: WorkDoneProgressOptions {
                    work_done_progress: Some(true),
                },
            }))
        );
    }

    #[test]
    fn negotiate_capabilities_omits_code_actions_without_literal_support() {
        let (capabilities, _) = negotiate_capabilities(InitializeParams::default());

        assert_eq!(capabilities.code_action_provider, None);
    }

    #[test]
    fn negotiate_capabilities_advertises_code_actions_with_other_literal_kinds() {
        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            code_action: Some(CodeActionClientCapabilities {
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: vec![CodeActionKind::REFACTOR.as_str().into()],
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (capabilities, config) = negotiate_capabilities(params);

        assert!(config.supports_code_action_literals());
        assert_eq!(
            capabilities.code_action_provider,
            Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
                resolve_provider: Some(false),
            }))
        );
    }

    #[test]
    fn negotiate_capabilities_advertises_and_records_code_action_literal_support() {
        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            code_action: Some(CodeActionClientCapabilities {
                code_action_literal_support: Some(CodeActionLiteralSupport {
                    code_action_kind: CodeActionKindLiteralSupport {
                        value_set: vec![CodeActionKind::QUICKFIX.as_str().into()],
                    },
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (capabilities, config) = negotiate_capabilities(params);

        assert!(config.supports_code_action_literals());
        assert_eq!(
            capabilities.code_action_provider,
            Some(CodeActionProviderCapability::Options(CodeActionOptions {
                code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
                work_done_progress_options: WorkDoneProgressOptions::default(),
                resolve_provider: Some(false),
            }))
        );
    }

    #[test]
    fn negotiate_capabilities_records_code_action_is_preferred_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_code_action_is_preferred());

        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            code_action: Some(CodeActionClientCapabilities {
                is_preferred_support: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.supports_code_action_is_preferred());
    }

    #[test]
    fn negotiate_capabilities_records_publish_diagnostics_data_support() {
        let (_, config) = negotiate_capabilities(InitializeParams::default());
        assert!(!config.supports_publish_diagnostics_data());

        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                data_support: Some(true),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params);

        assert!(config.supports_publish_diagnostics_data());
    }

    #[test]
    fn negotiate_capabilities_records_push_and_pull_diagnostic_data_independently() {
        for (publish, pull) in [(false, false), (false, true), (true, false), (true, true)] {
            let mut params = InitializeParams::default();
            params.capabilities.text_document = Some(TextDocumentClientCapabilities {
                publish_diagnostics: Some(PublishDiagnosticsClientCapabilities {
                    data_support: Some(publish),
                    ..Default::default()
                }),
                ..Default::default()
            });

            let (_, config) = negotiate_capabilities_with_pull_diagnostic_data(params, pull);

            assert_eq!(config.supports_publish_diagnostics_data(), publish);
            assert_eq!(config.supports_pull_diagnostics_data(), pull);
            assert_eq!(config.supports_code_action_diagnostic_data(), publish || pull);
        }
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
        assert!(!config.completion_options().markdown_documentation);
        assert!(config.completion_options().resolve_documentation);
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
    fn negotiate_capabilities_records_completion_documentation_preference() {
        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    documentation_format: Some(vec![MarkupKind::Markdown, MarkupKind::PlainText]),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params.clone());
        assert!(config.completion_options().markdown_documentation);

        params
            .capabilities
            .text_document
            .as_mut()
            .unwrap()
            .completion
            .as_mut()
            .unwrap()
            .completion_item
            .as_mut()
            .unwrap()
            .documentation_format = Some(vec![MarkupKind::PlainText, MarkupKind::Markdown]);
        let (_, config) = negotiate_capabilities(params);
        assert!(!config.completion_options().markdown_documentation);
    }

    #[test]
    fn negotiate_capabilities_records_completion_documentation_resolve_support() {
        let mut params = InitializeParams::default();
        params.capabilities.text_document = Some(TextDocumentClientCapabilities {
            completion: Some(CompletionClientCapabilities {
                completion_item: Some(CompletionItemCapability {
                    resolve_support: Some(CompletionItemCapabilityResolveSupport {
                        properties: vec!["additionalTextEdits".into()],
                    }),
                    ..Default::default()
                }),
                ..Default::default()
            }),
            ..Default::default()
        });

        let (_, config) = negotiate_capabilities(params.clone());
        assert!(!config.completion_options().resolve_documentation);

        params
            .capabilities
            .text_document
            .as_mut()
            .unwrap()
            .completion
            .as_mut()
            .unwrap()
            .completion_item
            .as_mut()
            .unwrap()
            .resolve_support
            .as_mut()
            .unwrap()
            .properties
            .push("documentation".into());
        let (_, config) = negotiate_capabilities(params);
        assert!(config.completion_options().resolve_documentation);
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

            //- /lib/Dependency.sol
            contract Dependency {}
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
        assert_eq!(config.flychecks_for_path(&project.path("/lib/Dependency.sol")).len(), 1);
    }

    #[test]
    fn source_file_updates_follow_external_foundry_flycheck_roots() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            src = "src"

            //- /second/foundry.toml
            [profile.default]
            src = "src"
            test = "../checks"
            "#,
        );
        let mut config = project.config_with_roots(&["/first", "/second"]);
        let path = project.path("/checks/New.t.sol");
        project.write_file("/checks/New.t.sol", "contract NewTest {}\n");

        config.add_source_file(path.clone());

        let second = config
            .workspaces()
            .iter()
            .find(|workspace| {
                workspace.compile_opts().base_path.as_deref()
                    == Some(project.path("/second").as_path())
            })
            .unwrap();
        assert!(second.flycheck_source_files().contains(&path));
        let first = config
            .workspaces()
            .iter()
            .find(|workspace| {
                workspace.compile_opts().base_path.as_deref()
                    == Some(project.path("/first").as_path())
            })
            .unwrap();
        assert!(!first.flycheck_source_files().contains(&path));

        config.remove_source_file(&path);

        let second = config
            .workspaces()
            .iter()
            .find(|workspace| {
                workspace.compile_opts().base_path.as_deref()
                    == Some(project.path("/second").as_path())
            })
            .unwrap();
        assert!(!second.flycheck_source_files().contains(&path));
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

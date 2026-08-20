use crate::{
    diagnostics::DiagnosticOwner,
    workspace::{Workspace, WorkspaceKind},
};
use serde::Deserialize;
use std::{
    path::{Path, PathBuf},
    process::{Command, Stdio},
};

#[derive(Clone, Debug)]
pub(crate) struct FlycheckConfig {
    pub(crate) id: String,
    pub(crate) command: PathBuf,
    pub(crate) args: Vec<String>,
    pub(crate) cwd: PathBuf,
    pub(crate) workspace_root: PathBuf,
    pub(super) output: FlycheckOutput,
}

impl FlycheckConfig {
    pub(crate) fn applies_to(&self, path: &Path) -> bool {
        path.starts_with(&self.workspace_root)
    }

    pub(crate) fn owner(&self) -> DiagnosticOwner {
        DiagnosticOwner::Flycheck { id: self.id.clone(), workspace: self.workspace_root.clone() }
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct FlycheckInitializationOptions {
    forge_path: Option<PathBuf>,
    flychecks: Option<Vec<FlycheckTemplate>>,
}

impl FlycheckInitializationOptions {
    pub(crate) fn from_json(
        value: Option<serde_json::Value>,
        default_forge_path: Option<&Path>,
    ) -> Self {
        let mut options =
            value.and_then(|value| serde_json::from_value::<Self>(value).ok()).unwrap_or_default();
        if options.forge_path.is_none() {
            options.forge_path = default_forge_path.map(Path::to_path_buf);
        }
        options
    }

    pub(crate) fn configs(&self, workspaces: &[Workspace]) -> Vec<FlycheckConfig> {
        match &self.flychecks {
            Some(templates) => expand_templates(templates, workspaces),
            None => default_flychecks(workspaces, self.forge_path()),
        }
    }

    pub(crate) fn forge_path(&self) -> PathBuf {
        self.forge_path.clone().unwrap_or_else(|| PathBuf::from("forge"))
    }
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct FlycheckTemplate {
    id: String,
    command: PathBuf,
    #[serde(default)]
    args: Vec<String>,
    cwd: Option<PathBuf>,
    #[serde(default)]
    output: FlycheckOutput,
}

#[derive(Clone, Copy, Debug, Default, Deserialize, Eq, PartialEq)]
#[serde(rename_all = "kebab-case")]
pub(super) enum FlycheckOutput {
    #[default]
    SolcJson,
    ForgeLintJson,
}

fn expand_templates(
    templates: &[FlycheckTemplate],
    workspaces: &[Workspace],
) -> Vec<FlycheckConfig> {
    workspaces
        .iter()
        .filter_map(workspace_root)
        .flat_map(|workspace_root| {
            templates.iter().map(move |template| {
                let cwd = template.cwd.as_ref().map_or_else(
                    || workspace_root.clone(),
                    |cwd| resolve_workspace_path(&workspace_root, cwd),
                );
                FlycheckConfig {
                    id: template.id.clone(),
                    command: template.command.clone(),
                    args: template.args.clone(),
                    cwd,
                    workspace_root: workspace_root.clone(),
                    output: template.output,
                }
            })
        })
        .collect()
}

fn default_flychecks(workspaces: &[Workspace], forge_path: PathBuf) -> Vec<FlycheckConfig> {
    workspaces
        .iter()
        .filter(|workspace| workspace.kind() == WorkspaceKind::Foundry)
        .filter_map(workspace_root)
        .filter(|root| forge_lint_available(&forge_path, root))
        .map(|workspace_root| FlycheckConfig {
            id: "forge-lint".into(),
            command: forge_path.clone(),
            args: vec!["lint".into(), "--json".into()],
            cwd: workspace_root.clone(),
            workspace_root,
            output: FlycheckOutput::ForgeLintJson,
        })
        .collect()
}

fn workspace_root(workspace: &Workspace) -> Option<PathBuf> {
    workspace.compile_opts().base_path.clone()
}

fn resolve_workspace_path(workspace_root: &Path, path: &Path) -> PathBuf {
    if path.is_absolute() { path.to_path_buf() } else { workspace_root.join(path) }
}

fn forge_lint_available(command: &Path, cwd: &Path) -> bool {
    Command::new(command)
        .args(["lint", "--help"])
        .current_dir(cwd)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|status| status.success())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LaunchConfig, global_state::GlobalState, test_support::TestProject};
    #[cfg(unix)]
    use solar_interface::source_map::{FileLoader, RealFileLoader};
    #[cfg(unix)]
    use std::{fs, os::unix::fs::PermissionsExt, sync::Arc, time::Duration};
    #[cfg(unix)]
    use tokio::sync::oneshot;

    #[test]
    fn configured_flychecks_expand_per_workspace() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            "#,
        );
        let options = FlycheckInitializationOptions::from_json(
            Some(serde_json::json!({
                "flychecks": [{
                    "id": "custom",
                    "command": "custom-lint",
                    "args": ["--json"],
                    "cwd": "tools",
                    "output": "solc-json"
                }]
            })),
            Some(Path::new("/embedded/forge")),
        );

        let configs = options.configs(project.config().workspaces());

        assert_eq!(options.forge_path(), PathBuf::from("/embedded/forge"));
        assert_eq!(configs.len(), 1);
        assert_eq!(configs[0].id, "custom");
        assert_eq!(configs[0].command, PathBuf::from("custom-lint"));
        assert_eq!(configs[0].args, ["--json"]);
        assert_eq!(configs[0].cwd, project.path("/tools"));
        assert_eq!(configs[0].workspace_root, project.root());
    }

    #[test]
    fn explicit_empty_flychecks_disable_default_detection() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            "#,
        );
        let options =
            FlycheckInitializationOptions { forge_path: None, flychecks: Some(Vec::new()) };

        assert!(options.configs(project.config().workspaces()).is_empty());
    }

    #[cfg(unix)]
    #[tokio::test(flavor = "current_thread")]
    async fn launch_default_drives_forge_lint_discovery_and_execution() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"

            //- /src/Test.sol
            contract Test {}
            "#,
        );
        project.write_file(
            "/embedded-forge",
            r#"#!/bin/sh
set -eu
printf '%s\n' "$@" >> "$0.args"
printf '%s\n' -- >> "$0.args"
printf '%s\n' "$PWD" >> "$0.cwd"
"#,
        );
        let forge = project.path("/embedded-forge");
        let mut permissions = fs::metadata(&forge).unwrap().permissions();
        permissions.set_mode(0o755);
        fs::set_permissions(&forge, permissions).unwrap();

        let mut state = GlobalState::with_launch_config(
            async_lsp::ClientSocket::new_closed(),
            LaunchConfig::default().with_default_forge_path(&forge),
        );
        state.on_initialize(project.initialize_params()).await.unwrap();
        let _ = Arc::make_mut(&mut state.config).rediscover_workspaces();
        let [config] =
            state.config.flychecks_for_path(&project.path("/src/Test.sol")).try_into().unwrap();
        let expected_cwd = RealFileLoader.canonicalize_path(&config.cwd).unwrap();

        assert_eq!(config.command, forge);
        assert_eq!(config.args, ["lint", "--json"]);
        let (_cancel, cancelled) = oneshot::channel();
        let diagnostics = crate::flycheck::run(
            config,
            Duration::from_secs(5),
            cancelled,
            vec![project.path("/src/Test.sol")],
        )
        .await
        .unwrap();

        assert!(diagnostics.is_empty());
        assert_eq!(
            project.read_file("/embedded-forge.args"),
            "lint\n--help\n--\nlint\n--json\n--\n"
        );
        assert_eq!(
            project.read_file("/embedded-forge.cwd"),
            format!("{0}\n{0}\n", expected_cwd.display())
        );
    }
}

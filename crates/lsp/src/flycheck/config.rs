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
    flychecks: Option<Vec<FlycheckTemplate>>,
}

impl FlycheckInitializationOptions {
    pub(crate) fn from_json(value: Option<serde_json::Value>) -> Self {
        value.and_then(|value| serde_json::from_value(value).ok()).unwrap_or_default()
    }

    pub(crate) fn configs(
        &self,
        workspaces: &[Workspace],
        forge_path: &Path,
        selected_profile: Option<&str>,
    ) -> Vec<FlycheckConfig> {
        match &self.flychecks {
            Some(templates) => expand_templates(templates, workspaces),
            None => default_flychecks(workspaces, forge_path.to_path_buf(), selected_profile),
        }
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

fn default_flychecks(
    workspaces: &[Workspace],
    forge_path: PathBuf,
    selected_profile: Option<&str>,
) -> Vec<FlycheckConfig> {
    workspaces
        .iter()
        .filter(|workspace| workspace.kind() == WorkspaceKind::Foundry)
        .filter_map(workspace_root)
        .filter(|root| forge_lint_available(&forge_path, root))
        .map(|workspace_root| FlycheckConfig {
            id: "forge-lint".into(),
            command: forge_path.clone(),
            args: forge_lint_args(selected_profile),
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

fn forge_lint_args(selected_profile: Option<&str>) -> Vec<String> {
    let mut args = vec!["lint".into(), "--json".into()];
    if let Some(profile) = selected_profile {
        args.extend(["--profile".into(), profile.into()]);
    }
    args
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
    use crate::test_support::TestProject;

    #[test]
    fn configured_flychecks_expand_per_workspace() {
        let project = TestProject::from_fixture(
            r#"
            //- /foundry.toml
            [profile.default]
            src = "src"
            "#,
        );
        let options = FlycheckInitializationOptions {
            flychecks: Some(vec![FlycheckTemplate {
                id: "custom".into(),
                command: "custom-lint".into(),
                args: vec!["--json".into()],
                cwd: Some("tools".into()),
                output: FlycheckOutput::SolcJson,
            }]),
        };

        let configs = options.configs(project.config().workspaces(), Path::new("forge"), None);

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
        let options = FlycheckInitializationOptions { flychecks: Some(Vec::new()) };

        assert!(
            options.configs(project.config().workspaces(), Path::new("forge"), None).is_empty()
        );
    }

    #[test]
    fn default_forge_lint_args_omit_unselected_profile() {
        assert_eq!(forge_lint_args(None), ["lint", "--json"]);
    }
}

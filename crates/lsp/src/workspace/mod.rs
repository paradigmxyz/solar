//! Workspace models.
//!
//! Solar LSP supports multiple workspace models that are configured in different ways.
//!
//! This module contains a generic workspace concept, as well as implementations of different
//! project models (e.g. Foundry projects), and a project discovery algorithm to try and determine
//! what kind of project the LSP is dealing with based on different heuristics.
//!
//! Once a project type is identified, the configuration for that project model is merged into the
//! overall LSP config.

use std::{
    io,
    path::{Path, PathBuf},
};

use solar_config::CompileOpts;

use crate::workspace::{foundry::FoundryDocument, manifest::ProjectManifest, solar::SolarDocument};

mod foundry;
pub(crate) mod manifest;
mod solar;

#[derive(Clone, Debug)]
pub(crate) struct Workspace {
    kind: WorkspaceKind,
    manifest_path: Option<PathBuf>,
    compile_opts: CompileOpts,
    source_roots: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceKind {
    Solar,
    Foundry,
    /// A naked workspace is a workspace with no specific configuration.
    ///
    /// Naked workspaces have no remappings or toolchain-style dependencies, so all imports are
    /// assumed to be relative to the file being parsed.
    Naked,
}

impl Workspace {
    pub(crate) fn load_manifest(manifest: ProjectManifest) -> Result<Self, WorkspaceError> {
        match manifest {
            ProjectManifest::Solar(path) => Self::load_solar(path),
            ProjectManifest::Foundry(path) => Self::load_foundry(path),
        }
    }

    pub(crate) fn naked(root: PathBuf) -> Self {
        let source_roots = vec![root.clone()];
        Self {
            kind: WorkspaceKind::Naked,
            manifest_path: None,
            compile_opts: CompileOpts { base_path: Some(root), ..Default::default() },
            source_roots,
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
            manifest_path: None,
            compile_opts: CompileOpts::default(),
            source_roots: Vec::new(),
        }
    }

    pub(crate) fn kind(&self) -> WorkspaceKind {
        self.kind
    }

    pub(crate) fn manifest_path(&self) -> Option<&Path> {
        self.manifest_path.as_deref()
    }

    pub(crate) fn compile_opts(&self) -> &CompileOpts {
        &self.compile_opts
    }

    pub(crate) fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }

    fn load_foundry(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?;
        let profile = FoundryDocument::load(&path)?.default_profile();
        let mut compile_opts = CompileOpts {
            base_path: Some(root.clone()),
            include_paths: profile.include_paths(&root),
            import_remappings: profile.remappings(),
            ..Default::default()
        };
        if let Some(evm_version) = profile.evm_version() {
            compile_opts.evm_version = evm_version;
        }

        Ok(Self {
            kind: WorkspaceKind::Foundry,
            manifest_path: Some(path),
            source_roots: profile.source_roots(&root),
            compile_opts,
        })
    }

    fn load_solar(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?;
        let config = SolarDocument::load(&path)?.compiler();
        let base_path = config.base_path(&root);
        let mut compile_opts = CompileOpts {
            base_path: Some(base_path.clone()),
            include_paths: config.include_paths(&base_path),
            import_remappings: config.remappings(),
            ..Default::default()
        };
        if let Some(evm_version) = config.evm_version() {
            compile_opts.evm_version = evm_version;
        }

        Ok(Self {
            kind: WorkspaceKind::Solar,
            manifest_path: Some(path),
            source_roots: config.source_roots(&base_path),
            compile_opts,
        })
    }
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum WorkspaceError {
    #[error("workspace manifest `{}` has no parent directory", .0.display())]
    MissingManifestParent(PathBuf),
    #[error("failed to read workspace manifest `{}`: {source}", path.display())]
    Read {
        path: PathBuf,
        #[source]
        source: io::Error,
    },
    #[error("failed to parse workspace manifest `{}`: {source}", path.display())]
    ParseToml {
        path: PathBuf,
        #[source]
        source: toml_edit::de::Error,
    },
}

fn manifest_root(path: &Path) -> Result<PathBuf, WorkspaceError> {
    path.parent()
        .map(Path::to_path_buf)
        .ok_or_else(|| WorkspaceError::MissingManifestParent(path.to_path_buf()))
}

#[cfg(test)]
mod tests {
    use std::{
        fs,
        path::{Path, PathBuf},
        time::{SystemTime, UNIX_EPOCH},
    };

    use solar_config::EvmVersion;

    use super::*;
    use crate::workspace::manifest::ProjectManifest;

    struct TempProject {
        root: PathBuf,
    }

    impl TempProject {
        fn new(name: &str) -> Self {
            let nanos = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
            let root = std::env::temp_dir()
                .join(format!("solar-lsp-{name}-{}-{nanos}", std::process::id()));
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
    fn foundry_workspace_loads_manifest_compile_config() {
        let project = TempProject::new("foundry-config");
        fs::write(
            project.root().join("foundry.toml"),
            r#"
                [profile.default]
                src = "contracts"
                libs = ["lib", "vendor"]
                evm_version = "cancun"
                remappings = [
                    "@oz=lib/openzeppelin-contracts/contracts/",
                    "ds-test=lib/ds-test/src/",
                ]
            "#,
        )
        .unwrap();

        let workspace =
            Workspace::load_manifest(ProjectManifest::Foundry(project.root().join("foundry.toml")))
                .unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.root()));
        assert_eq!(
            opts.include_paths,
            vec![project.root().join("lib"), project.root().join("vendor")]
        );
        assert_eq!(opts.evm_version, EvmVersion::Cancun);
        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["@oz=lib/openzeppelin-contracts/contracts/", "ds-test=lib/ds-test/src/",]
        );
        assert_eq!(workspace.source_roots(), &[project.root().join("contracts")]);
    }

    #[test]
    fn solar_workspace_loads_manifest_compile_config() {
        let project = TempProject::new("solar-config");
        fs::write(
            project.root().join("solar.toml"),
            r#"
                [compiler]
                base_path = "."
                source_paths = ["src", "contracts"]
                include_paths = ["lib"]
                evm_version = "osaka"
                remappings = ["@pkg=lib/pkg/src/"]
            "#,
        )
        .unwrap();

        let workspace =
            Workspace::load_manifest(ProjectManifest::Solar(project.root().join("solar.toml")))
                .unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.root()));
        assert_eq!(opts.include_paths, vec![project.root().join("lib")]);
        assert_eq!(opts.evm_version, EvmVersion::Osaka);
        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["@pkg=lib/pkg/src/"]
        );
        assert_eq!(
            workspace.source_roots(),
            &[project.root().join("src"), project.root().join("contracts")]
        );
    }
}

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

use serde::de::DeserializeOwned;
use solar_config::{CompileOpts, EvmVersion, ImportRemapping};
use solar_interface::source_map::SourceMap;

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
    source_files: Vec<PathBuf>,
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
            source_files: Vec::new(),
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
            manifest_path: None,
            compile_opts: CompileOpts::default(),
            source_roots: Vec::new(),
            source_files: Vec::new(),
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

    #[cfg(test)]
    pub(crate) fn source_roots(&self) -> &[PathBuf] {
        &self.source_roots
    }

    pub(crate) fn source_files(&self) -> &[PathBuf] {
        &self.source_files
    }

    pub(crate) fn refresh_source_files(&mut self) {
        self.source_files.clear();
        let skip_heavy_dirs = self.kind == WorkspaceKind::Naked;
        for root in &self.source_roots {
            collect_solidity_files(root, &mut self.source_files, skip_heavy_dirs, true);
        }
        self.source_files.sort();
        self.source_files.dedup();
    }

    pub(crate) fn add_source_file(&mut self, path: PathBuf) {
        if !is_solidity_file(&path) || !self.source_roots.iter().any(|root| path.starts_with(root))
        {
            return;
        }
        match self.source_files.binary_search(&path) {
            Ok(_) => {}
            Err(pos) => self.source_files.insert(pos, path),
        }
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        if let Ok(pos) =
            self.source_files.binary_search_by(|candidate| candidate.as_path().cmp(path))
        {
            self.source_files.remove(pos);
        }
    }

    fn load_foundry(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?;
        let profile = load_manifest_document::<FoundryDocument>(&path)?.default_profile();
        let compile_opts = compile_opts(
            root.clone(),
            profile.include_paths(&root),
            profile.remappings(),
            profile.evm_version(),
        );

        Ok(Self {
            kind: WorkspaceKind::Foundry,
            manifest_path: Some(path),
            source_roots: profile.source_roots(&root),
            compile_opts,
            source_files: Vec::new(),
        })
    }

    fn load_solar(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?;
        let config = load_manifest_document::<SolarDocument>(&path)?.compiler();
        let base_path = config.base_path(&root);
        let compile_opts = compile_opts(
            base_path.clone(),
            config.include_paths(&base_path),
            config.remappings(),
            config.evm_version(),
        );

        Ok(Self {
            kind: WorkspaceKind::Solar,
            manifest_path: Some(path),
            source_roots: config.source_roots(&base_path),
            compile_opts,
            source_files: Vec::new(),
        })
    }
}

pub(crate) fn workspace_idx_for_path(workspaces: &[Workspace], path: &Path) -> usize {
    workspaces
        .iter()
        .enumerate()
        .filter_map(|(idx, workspace)| {
            let base_path = workspace.compile_opts().base_path.as_ref()?;
            path.starts_with(base_path).then_some((idx, base_path.components().count()))
        })
        .max_by_key(|&(_, components)| components)
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn collect_solidity_files(
    path: &Path,
    files: &mut Vec<PathBuf>,
    skip_heavy_dirs: bool,
    is_root: bool,
) {
    let Ok(metadata) = std::fs::symlink_metadata(path) else {
        return;
    };
    if metadata.is_file() {
        if is_solidity_file(path) {
            files.push(path.to_path_buf());
        }
        return;
    }
    if metadata.is_dir() {
        if !is_root && skip_heavy_dirs && is_heavy_dir(path) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            collect_solidity_files(&entry.path(), files, skip_heavy_dirs, false);
        }
    }
}

fn is_solidity_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "sol")
}

fn is_heavy_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "cache" | "lib" | "node_modules" | "out")
    )
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

fn compile_opts(
    base_path: PathBuf,
    include_paths: Vec<PathBuf>,
    import_remappings: Vec<ImportRemapping>,
    evm_version: Option<EvmVersion>,
) -> CompileOpts {
    let mut opts = CompileOpts {
        base_path: Some(base_path),
        include_paths,
        import_remappings,
        ..Default::default()
    };
    if let Some(evm_version) = evm_version {
        opts.evm_version = evm_version;
    }
    opts
}

fn load_manifest_document<T: DeserializeOwned>(path: &Path) -> Result<T, WorkspaceError> {
    let source_map = SourceMap::empty();
    let contents = source_map
        .file_loader()
        .load_file(path)
        .map_err(|source| WorkspaceError::Read { path: path.to_path_buf(), source })?;
    toml_edit::de::from_str(&contents)
        .map_err(|source| WorkspaceError::ParseToml { path: path.to_path_buf(), source })
}

#[cfg(test)]
mod tests {
    use std::fs;

    use solar_config::EvmVersion;
    use tempfile::TempDir;

    use super::*;
    use crate::workspace::manifest::ProjectManifest;

    #[test]
    fn foundry_workspace_loads_manifest_compile_config() {
        let project = TempDir::new().unwrap();
        fs::write(
            project.path().join("foundry.toml"),
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
            Workspace::load_manifest(ProjectManifest::Foundry(project.path().join("foundry.toml")))
                .unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.path()));
        assert_eq!(
            opts.include_paths,
            vec![project.path().join("lib"), project.path().join("vendor")]
        );
        assert_eq!(opts.evm_version, EvmVersion::Cancun);
        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["@oz=lib/openzeppelin-contracts/contracts/", "ds-test=lib/ds-test/src/",]
        );
        assert_eq!(workspace.source_roots(), &[project.path().join("contracts")]);
    }

    #[test]
    fn solar_workspace_loads_manifest_compile_config() {
        let project = TempDir::new().unwrap();
        fs::write(
            project.path().join("solar.toml"),
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
            Workspace::load_manifest(ProjectManifest::Solar(project.path().join("solar.toml")))
                .unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.path()));
        assert_eq!(opts.include_paths, vec![project.path().join("lib")]);
        assert_eq!(opts.evm_version, EvmVersion::Osaka);
        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["@pkg=lib/pkg/src/"]
        );
        assert_eq!(
            workspace.source_roots(),
            &[project.path().join("src"), project.path().join("contracts")]
        );
    }

    #[test]
    fn naked_workspace_skips_common_heavy_dirs() {
        let project = TempDir::new().unwrap();
        let source = project.path().join("src");
        fs::create_dir(&source).unwrap();
        fs::write(source.join("A.sol"), "contract A {}").unwrap();

        for dir in [".git", "cache", "lib", "node_modules", "out"] {
            let dir = project.path().join(dir);
            fs::create_dir(&dir).unwrap();
            fs::write(dir.join("Ignored.sol"), "contract Ignored {}").unwrap();
        }

        let mut workspace = Workspace::naked(project.path().to_path_buf());
        workspace.refresh_source_files();

        assert_eq!(workspace.source_files(), &[source.join("A.sol")]);
    }

    #[test]
    fn naked_workspace_does_not_skip_root_named_like_heavy_dir() {
        let project = TempDir::new().unwrap();
        let root = project.path().join("lib");
        fs::create_dir(&root).unwrap();
        fs::write(root.join("A.sol"), "contract A {}").unwrap();

        let mut workspace = Workspace::naked(root.clone());
        workspace.refresh_source_files();

        assert_eq!(workspace.source_files(), &[root.join("A.sol")]);
    }
}

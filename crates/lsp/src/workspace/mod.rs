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

use crate::workspace::foundry::FoundryDocument;
use solar_config::{CompileOpts, EvmVersion, ImportRemapping};
use solar_interface::source_map::SourceMap;
use std::{
    io,
    path::{Path, PathBuf},
};

mod foundry;
pub(crate) mod manifest;

#[derive(Clone, Debug)]
pub(crate) struct Workspace {
    kind: WorkspaceKind,
    compile_opts: CompileOpts,
    source_roots: Vec<PathBuf>,
    source_files: Vec<PathBuf>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum WorkspaceKind {
    Foundry,
    /// A naked workspace is a workspace with no specific configuration.
    ///
    /// Naked workspaces have no remappings or toolchain-style dependencies, so all imports are
    /// assumed to be relative to the file being parsed.
    Naked,
}

impl Workspace {
    pub(crate) fn naked(root: PathBuf) -> Self {
        let source_roots = vec![root.clone()];
        Self {
            kind: WorkspaceKind::Naked,
            compile_opts: CompileOpts { base_path: Some(root), ..Default::default() },
            source_roots,
            source_files: Vec::new(),
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
            compile_opts: CompileOpts::default(),
            source_roots: Vec::new(),
            source_files: Vec::new(),
        }
    }

    pub(crate) fn kind(&self) -> WorkspaceKind {
        self.kind
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
        // Naked roots need workspace-wide symbols for reverse navigation, but have no manifest
        // boundary to keep dependency and build directories out of the scan.
        let skip_heavy_dirs = self.kind == WorkspaceKind::Naked;
        for root in &self.source_roots {
            collect_solidity_files(root, root, &mut self.source_files, skip_heavy_dirs);
        }
        self.source_files.sort();
        self.source_files.dedup();
    }

    pub(crate) fn add_source_file(&mut self, path: PathBuf) {
        if !self.tracks_disk_file(&path) {
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

    pub(crate) fn tracks_disk_file(&self, path: &Path) -> bool {
        is_solidity_file(path)
            && self.source_roots.iter().any(|root| {
                path.starts_with(root)
                    && (self.kind != WorkspaceKind::Naked
                        || !is_in_ignored_naked_directory(root, path))
            })
    }

    pub(crate) fn load_foundry(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?;
        let profile = load_foundry_document(&path)?.default_profile();
        let compile_opts = compile_opts(
            root.clone(),
            profile.include_paths(&root),
            profile.remappings(&root),
            profile.evm_version(),
        );

        Ok(Self {
            kind: WorkspaceKind::Foundry,
            source_roots: profile.source_roots(&root),
            compile_opts,
            source_files: Vec::new(),
        })
    }
}

pub(crate) struct WorkspacePathIndex<'a> {
    entries: Vec<WorkspacePathIndexEntry<'a>>,
}

struct WorkspacePathIndexEntry<'a> {
    idx: usize,
    base_path: &'a Path,
    depth: usize,
}

impl<'a> WorkspacePathIndex<'a> {
    pub(crate) fn new(workspaces: &'a [Workspace]) -> Self {
        let mut entries = workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                let base_path = workspace.compile_opts().base_path.as_deref()?;
                Some(WorkspacePathIndexEntry {
                    idx,
                    base_path,
                    depth: base_path.components().count(),
                })
            })
            .collect::<Vec<_>>();
        entries.sort_by(|lhs, rhs| rhs.depth.cmp(&lhs.depth).then_with(|| rhs.idx.cmp(&lhs.idx)));
        Self { entries }
    }

    pub(crate) fn workspace_idx_for_path(&self, path: &Path) -> usize {
        self.workspace_idx_containing_path(path).unwrap_or(0)
    }

    pub(crate) fn workspace_idx_containing_path(&self, path: &Path) -> Option<usize> {
        self.entries.iter().find(|entry| path.starts_with(entry.base_path)).map(|entry| entry.idx)
    }
}

fn collect_solidity_files(
    root: &Path,
    path: &Path,
    files: &mut Vec<PathBuf>,
    skip_heavy_dirs: bool,
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
        if skip_heavy_dirs && is_ignored_naked_directory(root, path) {
            return;
        }
        let Ok(entries) = std::fs::read_dir(path) else {
            return;
        };
        for entry in entries.filter_map(Result::ok) {
            collect_solidity_files(root, &entry.path(), files, skip_heavy_dirs);
        }
    }
}

fn is_solidity_file(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension == "sol")
}

fn is_heavy_dir(path: &Path) -> bool {
    matches!(
        path.file_name().and_then(|name| name.to_str()),
        Some(".git" | "cache" | "lib" | "node_modules" | "out" | "target")
    )
}

fn is_ignored_naked_directory(root: &Path, directory: &Path) -> bool {
    directory != root && (is_heavy_dir(directory) || directory.join(".git").exists())
}

fn is_in_ignored_naked_directory(root: &Path, path: &Path) -> bool {
    path.parent().is_some_and(|parent| {
        parent
            .ancestors()
            .take_while(|directory| directory.starts_with(root))
            .any(|directory| is_ignored_naked_directory(root, directory))
    })
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

fn load_foundry_document(path: &Path) -> Result<FoundryDocument, WorkspaceError> {
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
    use super::*;
    use crate::test_support::TestProject;
    use solar_config::EvmVersion;

    #[test]
    fn foundry_workspace_loads_manifest_compile_config() {
        let project = TestProject::from_fixture(
            r#"
            //- /lib/forge-std/src/Test.sol
            contract Test {}

            //- /vendor/ds-test/src/Test.sol
            contract Test {}

            //- /remappings.txt
            solmate/=lib/solmate/src/

            //- /foundry.toml
            [profile.default]
            src = "contracts"
            libs = ["lib", "vendor"]
            evm_version = "cancun"
            remappings = [
                "@oz=lib/openzeppelin-contracts/contracts/",
                "ds-test=lib/ds-test/src/",
            ]
            "#,
        );

        let workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(opts.base_path.as_deref(), Some(project.root()));
        assert_eq!(opts.include_paths, vec![project.path("/lib"), project.path("/vendor")]);
        assert_eq!(opts.evm_version, EvmVersion::Cancun);
        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec![
                "ds-test/=vendor/ds-test/src/",
                "forge-std/=lib/forge-std/src/",
                "solmate/=lib/solmate/src/",
                "@oz=lib/openzeppelin-contracts/contracts/",
                "ds-test=lib/ds-test/src/",
            ]
        );
        assert_eq!(workspace.source_roots(), &[project.path("/contracts")]);
    }

    #[test]
    fn foundry_workspace_respects_disabled_auto_detect_remappings() {
        let project = TestProject::from_fixture(
            r#"
            //- /lib/forge-std/src/Test.sol
            contract Test {}

            //- /remappings.txt
            solmate/=lib/solmate/src/

            //- /foundry.toml
            [profile.default]
            auto_detect_remappings = false
            remappings = ["@oz=lib/openzeppelin-contracts/contracts/"]
            "#,
        );

        let workspace = Workspace::load_foundry(project.path("/foundry.toml")).unwrap();
        let opts = workspace.compile_opts();

        assert_eq!(
            opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["solmate/=lib/solmate/src/", "@oz=lib/openzeppelin-contracts/contracts/"]
        );
    }

    #[test]
    fn workspace_path_index_uses_most_specific_base_path() {
        let project = TestProject::new();
        let nested = project.path("/nested");

        let outer = Workspace::naked(project.root().to_path_buf());
        let inner = Workspace::naked(nested);
        let workspaces = vec![outer, inner];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(index.workspace_idx_for_path(&project.path("/nested/A.sol")), 1);
        assert_eq!(index.workspace_idx_for_path(&project.path("/B.sol")), 0);
    }

    #[test]
    fn naked_workspace_collects_disk_source_files_and_skips_heavy_dirs() {
        let project = TestProject::new();
        project.write_file("/src/A.sol", "contract A {}");
        for dir in [".git", "cache", "lib", "node_modules", "out", "target"] {
            project.write_file(&format!("/{dir}/Ignored.sol"), "contract Ignored {}");
        }
        project.write_file("/nested/.git", "gitdir: elsewhere");
        project.write_file("/nested/Ignored.sol", "contract Ignored {}");

        let mut workspace = Workspace::naked(project.root().to_path_buf());
        workspace.refresh_source_files();

        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);
    }

    #[test]
    fn naked_workspace_adds_created_disk_source_files_outside_heavy_dirs() {
        let project = TestProject::from_fixture(
            r#"
            //- /src/A.sol
            contract A {}

            //- /node_modules/Ignored.sol
            contract Ignored {}

            //- /nested/.git
            gitdir: elsewhere

            //- /nested/Ignored.sol
            contract Ignored {}
            "#,
        );

        let mut workspace = Workspace::naked(project.root().to_path_buf());
        workspace.add_source_file(project.path("/src/A.sol"));
        workspace.add_source_file(project.path("/node_modules/Ignored.sol"));
        workspace.add_source_file(project.path("/nested/Ignored.sol"));

        assert_eq!(workspace.source_files(), &[project.path("/src/A.sol")]);
    }
}

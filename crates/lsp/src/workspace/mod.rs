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
use normalize_path::NormalizePath;
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
    flycheck_source_roots: Vec<PathBuf>,
    flycheck_source_files: Vec<PathBuf>,
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
            flycheck_source_roots: source_roots.clone(),
            source_roots,
            source_files: Vec::new(),
            flycheck_source_files: Vec::new(),
        }
    }

    pub(crate) fn unconfigured() -> Self {
        Self {
            kind: WorkspaceKind::Naked,
            compile_opts: CompileOpts::default(),
            source_roots: Vec::new(),
            source_files: Vec::new(),
            flycheck_source_roots: Vec::new(),
            flycheck_source_files: Vec::new(),
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

    pub(crate) fn import_source_roots(&self) -> &[PathBuf] {
        &self.flycheck_source_roots
    }

    pub(crate) fn import_only_roots(&self) -> &[PathBuf] {
        &self.compile_opts.include_paths
    }

    pub(crate) fn source_files(&self) -> &[PathBuf] {
        &self.source_files
    }

    pub(crate) fn flycheck_source_files(&self) -> &[PathBuf] {
        &self.flycheck_source_files
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

        self.flycheck_source_files.clone_from(&self.source_files);
        for root in &self.flycheck_source_roots {
            if !self.source_roots.contains(root) {
                collect_solidity_files(
                    root,
                    root,
                    &mut self.flycheck_source_files,
                    skip_heavy_dirs,
                );
            }
        }
        self.flycheck_source_files.sort();
        self.flycheck_source_files.dedup();
    }

    pub(crate) fn add_source_file(&mut self, path: PathBuf) {
        if self.tracks_disk_file(&path) {
            insert_sorted(&mut self.source_files, path.clone());
        }
        self.add_flycheck_source_file(&path);
    }

    pub(crate) fn remove_source_file(&mut self, path: &Path) {
        remove_sorted(&mut self.source_files, path);
        self.remove_flycheck_source_file(path);
    }

    pub(crate) fn add_flycheck_source_file(&mut self, path: &Path) {
        if self.tracks_flycheck_file(path) {
            insert_sorted(&mut self.flycheck_source_files, path.to_path_buf());
        }
    }

    pub(crate) fn remove_flycheck_source_file(&mut self, path: &Path) {
        remove_sorted(&mut self.flycheck_source_files, path);
    }

    pub(crate) fn tracks_disk_file(&self, path: &Path) -> bool {
        self.tracks_file_in_roots(path, &self.source_roots)
    }

    pub(crate) fn tracks_flycheck_file(&self, path: &Path) -> bool {
        self.tracks_file_in_roots(path, &self.flycheck_source_roots)
    }

    fn tracks_file_in_roots(&self, path: &Path, roots: &[PathBuf]) -> bool {
        is_solidity_file(path)
            && roots.iter().any(|root| {
                path.starts_with(root)
                    && (self.kind != WorkspaceKind::Naked
                        || !is_in_ignored_naked_directory(root, path))
            })
    }

    pub(crate) fn load_foundry(path: PathBuf) -> Result<Self, WorkspaceError> {
        let root = manifest_root(&path)?.normalize();
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
            flycheck_source_roots: profile.flycheck_source_roots(&root),
            compile_opts,
            source_files: Vec::new(),
            flycheck_source_files: Vec::new(),
        })
    }
}

fn insert_sorted(files: &mut Vec<PathBuf>, path: PathBuf) {
    if let Err(pos) = files.binary_search(&path) {
        files.insert(pos, path);
    }
}

fn remove_sorted(files: &mut Vec<PathBuf>, path: &Path) {
    if let Ok(pos) = files.binary_search_by(|candidate| candidate.as_path().cmp(path)) {
        files.remove(pos);
    }
}

pub(crate) struct WorkspacePathIndex<'a> {
    workspaces: &'a [Workspace],
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
        Self { workspaces, entries }
    }

    pub(crate) fn workspace_idx_for_path(&self, path: &Path) -> usize {
        self.import_path_matches(path)
            .into_iter()
            .max_by_key(|&(idx, root_depth, root_kind, base_depth)| {
                (root_depth, root_kind, base_depth, idx)
            })
            .map_or(0, |(idx, _, _, _)| idx)
    }

    pub(crate) fn workspace_idx_containing_path(&self, path: &Path) -> Option<usize> {
        self.entries.iter().find(|entry| path.starts_with(entry.base_path)).map(|entry| entry.idx)
    }

    /// Returns the workspace whose import configuration owns `path`.
    ///
    /// The deepest matching root wins. At the same depth, base paths take precedence over
    /// external source roots, which take precedence over import-only roots. A tie across
    /// workspaces at both levels has no unique owner.
    pub(crate) fn workspace_idx_for_import_path(&self, path: &Path) -> Option<usize> {
        let matches = self.import_path_matches(path);
        let root_depth = matches.iter().map(|&(_, depth, _, _)| depth).max()?;
        let root_kind = matches
            .iter()
            .filter(|&&(_, depth, _, _)| depth == root_depth)
            .map(|&(_, _, kind, _)| kind)
            .max()?;
        let mut owners = matches
            .into_iter()
            .filter(|&(_, depth, kind, _)| depth == root_depth && kind == root_kind)
            .map(|(idx, _, _, _)| idx);
        let owner = owners.next()?;
        owners.next().is_none().then_some(owner)
    }

    /// Returns every workspace that needs an overlay at the deepest matching import root.
    pub(crate) fn workspace_idxs_for_import_path(&self, path: &Path) -> Vec<usize> {
        let matches = self.import_path_matches(path);
        let Some(root_depth) = matches.iter().map(|&(_, depth, _, _)| depth).max() else {
            return Vec::new();
        };
        matches
            .into_iter()
            .filter_map(|(idx, depth, _, _)| (depth == root_depth).then_some(idx))
            .collect()
    }

    fn import_path_matches(&self, path: &Path) -> Vec<(usize, usize, u8, usize)> {
        let path = path.normalize();
        self.workspaces
            .iter()
            .enumerate()
            .filter_map(|(idx, workspace)| {
                let (root_depth, root_kind) = import_root_score(workspace, &path)?;
                let base_depth = workspace
                    .compile_opts()
                    .base_path
                    .as_deref()
                    .map_or(0, |base_path| base_path.normalize().components().count());
                Some((idx, root_depth, root_kind, base_depth))
            })
            .collect()
    }
}

fn import_root_score(workspace: &Workspace, path: &Path) -> Option<(usize, u8)> {
    const IMPORT_ONLY: u8 = 0;
    const SOURCE: u8 = 1;
    const BASE: u8 = 2;

    let mut best = None;
    let mut consider = |root: &Path, kind| {
        let root = root.normalize();
        if path.starts_with(&root) {
            best = best.max(Some((root.components().count(), kind)));
        }
    };

    let base_path = workspace.compile_opts().base_path.as_deref();
    if let Some(base_path) = base_path {
        consider(base_path, BASE);
    }
    for root in workspace.import_source_roots() {
        consider(root, SOURCE);
    }
    for root in workspace.import_only_roots() {
        consider(root, IMPORT_ONLY);
    }
    for remapping in &workspace.compile_opts().import_remappings {
        let target = Path::new(&remapping.path);
        if target.is_absolute() {
            consider(target, IMPORT_ONLY);
        } else if let Some(base_path) = base_path {
            consider(&base_path.join(target), IMPORT_ONLY);
        }
    }

    best
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
    fn foundry_workspace_auto_detects_remappings_from_absolute_library_roots() {
        let project = TestProject::new();
        project.write_file("/shared/lib/pkg/src/Target.sol", "contract Target {}");
        let library = project.path("/shared/lib").to_string_lossy().replace('\\', "/");
        project.write_file(
            "/workspace/foundry.toml",
            &format!("[profile.default]\nlibs = [\"{library}\"]\n"),
        );

        let workspace = Workspace::load_foundry(project.path("/workspace/foundry.toml")).unwrap();
        let remappings = workspace
            .compile_opts()
            .import_remappings
            .iter()
            .map(ToString::to_string)
            .collect::<Vec<_>>();
        let target = project.path("/shared/lib/pkg/src").to_string_lossy().replace('\\', "/");

        assert_eq!(remappings, [format!("pkg/={target}/")]);
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
    fn workspace_path_index_selects_import_owners_by_root_kind_and_specificity() {
        let project = TestProject::from_fixture(
            r#"
            //- /first/foundry.toml
            [profile.default]
            src = "../external/priority"
            libs = ["../external/include", "../external/tie"]
            auto_detect_remappings = false

            //- /nested/second/foundry.toml
            [profile.default]
            src = "../../external/source/nested"
            test = "../../external/tests"
            script = "../../external/scripts"
            libs = [
                "../../external/include/nested",
                "../../external/priority",
                "../../external/tie",
                "../../external/stable",
            ]
            auto_detect_remappings = false

            //- /nested/third/foundry.toml
            [profile.default]
            libs = ["../../external/stable"]
            auto_detect_remappings = false

            //- /source/foundry.toml
            [profile.default]
            src = "../external/source"
            auto_detect_remappings = false
            "#,
        );
        let workspaces = vec![
            Workspace::load_foundry(project.path("/first/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/nested/second/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/nested/third/foundry.toml")).unwrap(),
            Workspace::load_foundry(project.path("/source/foundry.toml")).unwrap(),
        ];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(index.workspace_idx_for_import_path(&project.path("/first/Owned.sol")), Some(0));
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/source/Owned.sol")),
            Some(3)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/source/nested/Owned.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/tests/Owned.t.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/scripts/Owned.s.sol")),
            Some(1)
        );
        assert_eq!(
            index
                .workspace_idx_for_import_path(&project.path("/external/include/nested/Owned.sol")),
            Some(1)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/priority/Owned.sol")),
            Some(0)
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/tie/Owned.sol")),
            None
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/external/stable/Owned.sol")),
            None
        );
        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/unowned/Overlay.sol")),
            None
        );
    }

    #[test]
    fn workspace_path_index_normalizes_import_base_paths() {
        let project = TestProject::from_fixture(
            r#"
            //- /container/.keep

            //- /project/foundry.toml
            "#,
        );
        let manifest = project.path("/container/../project/foundry.toml");
        let workspaces = vec![Workspace::load_foundry(manifest).unwrap()];
        let index = WorkspacePathIndex::new(&workspaces);

        assert_eq!(
            index.workspace_idx_for_import_path(&project.path("/project/test/Owned.t.sol")),
            Some(0)
        );
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

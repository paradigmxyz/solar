//! Project ownership rules for edits that must update every semantic reference.
//!
//! Client workspace roots and configured source roots admit project files. Library roots,
//! conventional dependency directories, and nonlocal remapping targets restrict that admission.
//! Source exceptions retain their declaring workspace: automatically discovered dependency
//! manifests cannot lift an enclosing project's restrictions, but independent projects can
//! explicitly own shared sources. Multiple restrictions on the same directory remain cumulative.
//!
//! Construct this policy from configuration once, establishing exceptions with lexical paths.
//! A request can then map those paths through filesystem resolution without recomputing their
//! relationships, which would otherwise let a source symlink authorize its dependency target.

use super::{Workspace, is_import_only_path_in_root};
use normalize_path::NormalizePath;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(crate) struct WorkspaceEditScope {
    unrestricted: bool,
    sources: Vec<PathBuf>,
    dependencies: Vec<DependencyRoot>,
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord)]
struct DependencyRoot {
    path: PathBuf,
    sources: Vec<PathBuf>,
}

struct ProjectSources {
    base: PathBuf,
    sources: Vec<PathBuf>,
    libraries: Vec<PathBuf>,
    remappings: Vec<PathBuf>,
}

impl WorkspaceEditScope {
    pub(crate) fn new(workspace_roots: &[PathBuf], workspaces: &[Workspace]) -> Self {
        let unrestricted = workspace_roots.is_empty()
            && workspaces.iter().all(|workspace| workspace.compile_opts().base_path.is_none());
        let mut client_roots = workspace_roots.iter().map(|path| path.normalize()).collect();
        dedup_paths(&mut client_roots);
        let mut sources = client_roots.clone();
        let mut dependencies = Vec::new();
        let projects = workspaces
            .iter()
            .filter_map(|workspace| {
                let base = workspace.compile_opts().base_path.as_ref()?.normalize();
                sources.push(base.clone());
                sources.extend(workspace.source_roots().iter().map(|path| path.normalize()));
                sources.extend(workspace.import_source_roots().iter().map(|path| path.normalize()));
                // The index's implicit base grants no dependency exception. An explicitly
                // configured build source equal to the base (src = .) still does.
                let mut explicit_sources = workspace
                    .source_roots()
                    .iter()
                    .map(|path| path.normalize())
                    .filter(|path| *path != base)
                    .chain(workspace.import_source_roots().iter().map(|path| path.normalize()))
                    .collect();
                dedup_paths(&mut explicit_sources);
                let mut libraries = workspace
                    .import_only_roots()
                    .iter()
                    .map(|path| path.normalize())
                    .chain(dependency_directories(&base))
                    .collect();
                dedup_paths(&mut libraries);
                let mut remappings = workspace.import_remapping_paths().collect();
                dedup_paths(&mut remappings);
                Some(ProjectSources { base, sources: explicit_sources, libraries, remappings })
            })
            .collect::<Vec<_>>();
        // A dependency manifest cannot authorize edits through a different project's rules.
        // Classify donors from declarations alone, before incorporating cross-project grants.
        let source_projects = projects
            .iter()
            .filter(|project| {
                !projects.iter().any(|owner| {
                    owner.base != project.base
                        && owner.protects_project(&project.base, &client_roots)
                })
            })
            .collect::<Vec<_>>();

        for project in &projects {
            let ProjectSources { base, sources: explicit_sources, libraries, remappings } = project;
            // A nested manifest is not independent authorization to edit its own dependency.
            let independent_projects = source_projects
                .iter()
                .filter(|other| !other.base.starts_with(base))
                .collect::<Vec<_>>();
            for path in libraries {
                let grants = explicit_sources.iter().chain(
                    independent_projects
                        .iter()
                        .filter(|other| !other.base.starts_with(path))
                        .flat_map(|other| &other.sources),
                );
                dependencies
                    .push(DependencyRoot::new(path.clone(), grants.chain(&client_roots).cloned()));
            }

            // These candidates do not depend on the remapping target. A separate project may
            // configure a source outside its own base, including a shared source directory.
            let mut local_sources = explicit_sources.clone();
            local_sources
                .extend(independent_projects.iter().flat_map(|other| &other.sources).cloned());
            local_sources.extend(
                client_roots
                    .iter()
                    .filter(|root| libraries.iter().any(|library| root.starts_with(library)))
                    .cloned(),
            );
            dedup_paths(&mut local_sources);
            for path in remappings {
                if path == base || local_sources.iter().any(|source| path.starts_with(source)) {
                    continue;
                }
                dependencies.push(DependencyRoot::new(
                    path.clone(),
                    local_sources.iter().chain(&client_roots).cloned(),
                ));
            }
        }

        // A client root may contain files outside every discovered project. Default dependency
        // directories still require a grant from outside that directory, or an explicit client
        // root within it; a manifest discovered inside cannot grant itself an exception.
        for root in &client_roots {
            for path in dependency_directories(root) {
                let grants = source_projects
                    .iter()
                    .filter(|project| !project.base.starts_with(&path))
                    .flat_map(|project| &project.sources);
                dependencies
                    .push(DependencyRoot::new(path.clone(), grants.chain(&client_roots).cloned()));
            }
        }

        dedup_paths(&mut sources);
        // Restrictions intersect. Equal paths with different exceptions cannot be merged by
        // taking the union of their source roots without granting additional edit permissions.
        dependencies.sort_unstable();
        dependencies.dedup();
        Self { unrestricted, sources, dependencies }
    }

    pub(crate) fn is_unrestricted(&self) -> bool {
        self.unrestricted
    }

    pub(crate) fn map_paths(&self, mut map_path: impl FnMut(&Path) -> PathBuf) -> Self {
        let sources = self.sources.iter().map(|path| map_path(path)).collect();
        let dependencies = self
            .dependencies
            .iter()
            .map(|dependency| DependencyRoot {
                path: map_path(&dependency.path),
                sources: dependency.sources.iter().map(|path| map_path(path)).collect(),
            })
            .collect();
        Self { unrestricted: self.unrestricted, sources, dependencies }
    }

    pub(crate) fn allows(&self, path: &Path) -> bool {
        self.unrestricted
            || (self.sources.iter().any(|root| path.starts_with(root))
                && !self.dependencies.iter().any(|dependency| {
                    is_import_only_path_in_root(path, &dependency.path, dependency.sources.iter())
                }))
    }
}

impl DependencyRoot {
    fn new(path: PathBuf, sources: impl Iterator<Item = PathBuf>) -> Self {
        let mut sources = sources.filter(|source| source.starts_with(&path)).collect();
        dedup_paths(&mut sources);
        Self { path, sources }
    }
}

impl ProjectSources {
    fn protects_project(&self, base: &Path, client_roots: &[PathBuf]) -> bool {
        let protected = |path: &Path| {
            let boundary = if base.starts_with(path) {
                path
            } else if base.starts_with(&self.base) && path.starts_with(base) {
                // Library roots and remappings can name a dependency's src directory while
                // its manifest sits above it. Conservatively require an explicit client root
                // before that nested manifest can grant sources to a different project.
                base
            } else {
                return false;
            };
            !client_roots.iter().any(|root| root.starts_with(boundary) && base.starts_with(root))
        };
        // Sibling project remappings are also first-party import aliases. Unlike a library
        // declaration, a remapping alone does not make an independent project a dependency.
        self.libraries.iter().any(|path| {
            protected(path)
                && !self
                    .sources
                    .iter()
                    .any(|source| source.starts_with(path) && base.starts_with(source))
        }) || (base.starts_with(&self.base)
            && self.remappings.iter().any(|path| {
                path != &self.base
                    && !self.sources.iter().any(|source| path.starts_with(source))
                    && protected(path)
            }))
    }
}

fn dependency_directories(base: &Path) -> impl Iterator<Item = PathBuf> {
    ["lib", "node_modules", "dependencies"].into_iter().map(|name| base.join(name))
}

fn dedup_paths(paths: &mut Vec<PathBuf>) {
    paths.sort_unstable();
    paths.dedup();
}

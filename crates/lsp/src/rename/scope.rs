//! Restricts semantic renames to project-owned files without changing the semantic index.
//!
//! All required edits must be writable: filtering individual locations would break override
//! families and imported declarations. Workspace roots explicitly opened by the client and
//! configured source roots establish ownership; library roots and dependency remappings do not.
//! Automatically discovered dependency manifests cannot override their parent's restrictions.
//! Check both lexical and resolved paths so a source symlink cannot grant access to a dependency.
//! This validation performs filesystem I/O and runs on the request's blocking worker.

use super::RenameCandidate;
use crate::{config::Config, proto};
use async_lsp::{ErrorCode, ResponseError};
use normalize_path::NormalizePath;
use solar_interface::source_map::{FileLoader, RealFileLoader};
use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) fn validate_rename_scope(
    candidate: &RenameCandidate,
    config: &Config,
) -> Result<(), ResponseError> {
    // Standalone sessions without workspace configuration retain their existing edit scope.
    if config.workspace_roots().is_empty()
        && config.workspaces().iter().all(|workspace| workspace.compile_opts().base_path.is_none())
    {
        return Ok(());
    }
    let lexical = EditScope::new(config, Path::normalize);
    let resolved =
        EditScope::new(config, |path| resolve_path(path).unwrap_or_else(|| path.normalize()));
    for locations in candidate.locations.chunk_by(|a, b| a.uri == b.uri) {
        let allowed = proto::vfs_path(&locations[0].uri).is_some_and(|path| {
            path.as_path().is_some_and(|path| {
                lexical.allows(path)
                    && resolve_path(path).is_some_and(|path| resolved.allows(&path))
            })
        });
        if !allowed {
            return Err(ResponseError::new(
                ErrorCode::REQUEST_FAILED,
                "cannot rename this symbol because it would modify dependency files",
            ));
        }
    }
    Ok(())
}

struct EditScope {
    sources: Vec<PathBuf>,
    dependencies: Vec<DependencyRoot>,
}

struct DependencyRoot {
    path: PathBuf,
    sources: Vec<PathBuf>,
}

impl EditScope {
    fn new(config: &Config, map_path: impl Fn(&Path) -> PathBuf) -> Self {
        let mut sources = config.workspace_roots().to_vec();
        let mut dependencies = Vec::new();
        for workspace in config.workspaces() {
            let opts = workspace.compile_opts();
            let Some(base) = &opts.base_path else { continue };
            sources.push(base.clone());
            sources.extend_from_slice(workspace.source_roots());
            sources.extend_from_slice(workspace.import_source_roots());
            let explicit_sources = workspace
                .source_roots()
                .iter()
                .filter(|root| *root != base)
                .chain(workspace.import_source_roots());
            let libraries =
                workspace.import_only_roots().iter().cloned().chain(dependency_directories(base));
            for path in libraries {
                dependencies.push(DependencyRoot {
                    sources: explicit_sources
                        .clone()
                        .filter(|source| source.starts_with(&path))
                        .cloned()
                        .collect(),
                    path,
                });
            }
            for remapping in &opts.import_remappings {
                let path = base.join(&remapping.path).normalize();
                // A remapping into configured project sources is a local alias. Otherwise the
                // target is import-only, unless a separate first-party project owns it.
                let mut local_sources = explicit_sources.clone().cloned().collect::<Vec<_>>();
                local_sources.extend(
                    config
                        .workspace_roots()
                        .iter()
                        .filter(|root| {
                            workspace
                                .import_only_roots()
                                .iter()
                                .cloned()
                                .chain(dependency_directories(base))
                                .any(|library| root.starts_with(library))
                        })
                        .cloned(),
                );
                local_sources.extend(
                    config
                        .workspaces()
                        .iter()
                        .filter(|other| {
                            other.compile_opts().base_path.as_ref().is_some_and(|other_base| {
                                !other_base.starts_with(base) && path.starts_with(other_base)
                            })
                        })
                        .flat_map(|other| other.import_source_roots())
                        .cloned(),
                );
                if path == *base || local_sources.iter().any(|source| path.starts_with(source)) {
                    continue;
                }
                dependencies.push(DependencyRoot {
                    sources: local_sources
                        .into_iter()
                        .filter(|source| source.starts_with(&path))
                        .collect(),
                    path,
                });
            }
        }
        // Client roots can include files outside every discovered project. Only source roots
        // configured outside a dependency may grant an exception inside it; a dependency's own
        // automatically discovered manifest must not grant itself permission.
        for root in config.workspace_roots() {
            for path in dependency_directories(root) {
                let sources = config
                    .workspaces()
                    .iter()
                    .filter(|workspace| {
                        workspace
                            .compile_opts()
                            .base_path
                            .as_ref()
                            .is_some_and(|base| !base.starts_with(&path))
                    })
                    .flat_map(|workspace| {
                        workspace.source_roots().iter().chain(workspace.import_source_roots())
                    })
                    .filter(|source| source.starts_with(&path))
                    .cloned()
                    .collect();
                dependencies.push(DependencyRoot { path, sources });
            }
        }
        for dependency in &mut dependencies {
            dependency.sources.extend(
                config
                    .workspace_roots()
                    .iter()
                    .filter(|root| root.starts_with(&dependency.path))
                    .cloned(),
            );
            // Resolve only after establishing exceptions lexically. Resolving src -> lib/dep
            // first would incorrectly turn that symlink into an explicit dependency exception.
            dependency.path = map_path(&dependency.path);
            for source in &mut dependency.sources {
                *source = map_path(source);
            }
        }
        sources = sources.into_iter().map(|source| map_path(&source)).collect();
        Self { sources, dependencies }
    }

    fn allows(&self, path: &Path) -> bool {
        self.sources.iter().any(|root| path.starts_with(root))
            && self.dependencies.iter().all(|dependency| {
                !path.starts_with(&dependency.path)
                    || dependency.sources.iter().any(|source| path.starts_with(source))
            })
    }
}

fn dependency_directories(base: &Path) -> impl Iterator<Item = PathBuf> {
    ["lib", "node_modules", "dependencies"].into_iter().map(|name| base.join(name))
}

fn resolve_path(path: &Path) -> Option<PathBuf> {
    // Unsaved files can still have symlinked parents. Resolve the nearest existing ancestor,
    // retaining the suffix, rather than requiring every candidate to exist on disk.
    for ancestor in path.ancestors() {
        match RealFileLoader.canonicalize_path(ancestor) {
            Ok(resolved) => {
                return Some(resolved.join(path.strip_prefix(ancestor).unwrap()).normalize());
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return None,
        }
        // Missing unsaved files are safe to append to a resolved parent. A dangling symlink
        // is not: VFS contents can outlive its target and must not authorize a lexical fallback.
        match std::fs::symlink_metadata(ancestor) {
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            _ => return None,
        }
    }
    None
}

//! Validates complete rename candidates against the workspace's edit ownership policy.
//!
//! Filtering individual locations would break override families and imported declarations, so
//! every required edit must pass both lexical and resolved-path checks. Filesystem observations
//! are shared only within this request, on its blocking worker, to keep symlink changes visible.

use super::RenameCandidate;
use crate::{config::Config, proto};
use async_lsp::{ErrorCode, ResponseError};
use normalize_path::NormalizePath;
use solar_interface::{
    data_structures::map::FxHashMap,
    source_map::{FileLoader, RealFileLoader},
};
use std::{
    io,
    path::{Path, PathBuf},
};

pub(crate) fn validate_rename_scope(
    candidate: &RenameCandidate,
    config: &Config,
) -> Result<(), ResponseError> {
    let lexical = config.workspace_edit_scope();
    // Standalone sessions without workspace configuration retain their existing edit scope.
    if lexical.is_unrestricted() {
        return Ok(());
    }
    // Symlinks can change without a new Config. Reuse filesystem observations only within this
    // request, including failed resolutions and roots shared by several dependency exceptions.
    let mut paths = FxHashMap::default();
    let mut resolve = |path: &Path| {
        paths.entry(path.to_path_buf()).or_insert_with_key(|path| resolve_path(path)).clone()
    };
    let resolved = lexical.map_paths(|path| resolve(path).unwrap_or_else(|| path.to_path_buf()));
    for locations in candidate.locations.chunk_by(|a, b| a.uri == b.uri) {
        let allowed = proto::vfs_path(&locations[0].uri).is_some_and(|path| {
            path.as_path().is_some_and(|path| {
                lexical.allows(path) && resolve(path).is_some_and(|path| resolved.allows(&path))
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

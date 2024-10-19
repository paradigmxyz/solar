//! File resolver.
//!
//! Modified from [`solang`](https://github.com/hyperledger/solang/blob/0f032dcec2c6e96797fd66fa0175a02be0aba71c/src/file_resolver.rs).

use super::SourceFile;
use crate::SourceMap;
use itertools::Itertools;
use normalize_path::NormalizePath;
use std::{
    borrow::Cow,
    io,
    path::{Path, PathBuf},
    sync::Arc,
};

/// An error that occurred while resolving a path.
#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("couldn't read stdin: {0}")]
    ReadStdin(#[source] io::Error),
    #[error("couldn't read {0}: {1}")]
    ReadFile(PathBuf, #[source] io::Error),
    #[error("file not found: {0}")]
    NotFound(PathBuf),
    #[error("multiple files match {0}: {}", _1.iter().map(|f| f.name.display()).format(", "))]
    MultipleMatches(PathBuf, Vec<Arc<SourceFile>>),
}

pub struct FileResolver<'a> {
    source_map: &'a SourceMap,
    import_paths: Vec<(Option<PathBuf>, PathBuf)>,
}

impl<'a> FileResolver<'a> {
    /// Creates a new file resolver.
    pub fn new(source_map: &'a SourceMap) -> Self {
        Self { source_map, import_paths: Vec::new() }
    }

    /// Returns the source map.
    pub fn source_map(&self) -> &'a SourceMap {
        self.source_map
    }

    /// Adds an import path. Returns `true` if the path is newly inserted.
    pub fn add_import_path(&mut self, path: PathBuf) -> bool {
        let entry = (None, path);
        let new = !self.import_paths.contains(&entry);
        if new {
            self.import_paths.push(entry);
        }
        new
    }

    /// Adds an import map.
    pub fn add_import_map(&mut self, map: PathBuf, path: PathBuf) {
        let map = Some(map);
        if let Some((_, e)) = self.import_paths.iter_mut().find(|(k, _)| *k == map) {
            *e = path;
        } else {
            self.import_paths.push((map, path));
        }
    }

    /// Get the import path and the optional mapping corresponding to `import_no`.
    pub fn get_import_path(&self, import_no: usize) -> Option<&(Option<PathBuf>, PathBuf)> {
        self.import_paths.get(import_no)
    }

    /// Get the import paths
    pub fn get_import_paths(&self) -> &[(Option<PathBuf>, PathBuf)] {
        self.import_paths.as_slice()
    }

    /// Get the import path corresponding to a map
    pub fn get_import_map(&self, map: &Path) -> Option<&PathBuf> {
        self.import_paths.iter().find(|(m, _)| m.as_deref() == Some(map)).map(|(_, pb)| pb)
    }

    /// Resolves an import path. `parent` is the path of the file that contains the import, if any.
    #[instrument(level = "debug", skip_all, fields(path = %path.display()))]
    pub fn resolve_file(
        &self,
        path: &Path,
        parent: Option<&Path>,
    ) -> Result<Arc<SourceFile>, ResolveError> {
        // https://docs.soliditylang.org/en/latest/path-resolution.html
        // Only when the path starts with ./ or ../ are relative paths considered; this means
        // that `import "b.sol";` will check the import paths for b.sol, while `import "./b.sol";`
        // will only the path relative to the current file.
        if path.starts_with("./") || path.starts_with("../") {
            if let Some(parent) = parent {
                let base = parent.parent().unwrap_or(Path::new("."));
                let path = base.join(path);
                if let Some(file) = self.try_file(&path)? {
                    // No ambiguity possible, so just return
                    return Ok(file);
                }
            }

            return Err(ResolveError::NotFound(path.into()));
        }

        if parent.is_none() {
            if let Some(file) = self.try_file(path)? {
                return Ok(file);
            }
            if path.is_absolute() {
                return Err(ResolveError::NotFound(path.into()));
            }
        }

        let original_path = path;
        let path = self.remap_path(path);
        let mut result = Vec::with_capacity(1);

        // Walk over the import paths until we find one that resolves.
        for import in &self.import_paths {
            if let (None, import_path) = import {
                let path = import_path.join(&path);
                if let Some(file) = self.try_file(&path)? {
                    result.push(file);
                }
            }
        }

        // If there was no defined import path, then try the file directly. See
        // https://docs.soliditylang.org/en/latest/path-resolution.html#base-path-and-include-paths
        // "By default the base path is empty, which leaves the source unit name unchanged."
        if !self.import_paths.iter().any(|(m, _)| m.is_none()) {
            if let Some(file) = self.try_file(&path)? {
                result.push(file);
            }
        }

        match result.len() {
            0 => Err(ResolveError::NotFound(original_path.into())),
            1 => Ok(result.pop().unwrap()),
            _ => Err(ResolveError::MultipleMatches(original_path.into(), result)),
        }
    }

    /// Applies the import path mappings to `path`.
    #[instrument(level = "trace", skip_all, ret)]
    pub fn remap_path<'b>(&self, path: &'b Path) -> Cow<'b, Path> {
        let orig = path;
        let mut remapped = Cow::Borrowed(path);
        for import_path in &self.import_paths {
            if let (Some(mapping), target) = import_path {
                if let Ok(relpath) = orig.strip_prefix(mapping) {
                    remapped = Cow::Owned(target.join(relpath));
                }
            }
        }
        remapped
    }

    /// Loads stdin into the source map.
    pub fn load_stdin(&self) -> Result<Arc<SourceFile>, ResolveError> {
        self.source_map().load_stdin().map_err(ResolveError::ReadStdin)
    }

    /// Loads `path` into the source map. Returns `None` if the file doesn't exist.
    #[instrument(level = "debug", skip_all)]
    pub fn try_file(&self, path: &Path) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        let cache_path = path.normalize();
        if let Ok(file) = self.source_map().load_file(&cache_path) {
            trace!("loaded from cache");
            return Ok(Some(file));
        }

        if let Ok(path) = crate::canonicalize(path) {
            // TODO: avoids loading the same file twice by canonicalizing,
            // and then not displaying the full path in the error message
            let mut path = path.as_path();
            if let Ok(curdir) = std::env::current_dir() {
                if let Ok(p) = path.strip_prefix(curdir) {
                    path = p;
                }
            }
            trace!("canonicalized to {}", path.display());
            return self
                .source_map()
                .load_file(path)
                .map(Some)
                .map_err(|e| ResolveError::ReadFile(path.into(), e));
        }

        trace!("not found");
        Ok(None)
    }
}

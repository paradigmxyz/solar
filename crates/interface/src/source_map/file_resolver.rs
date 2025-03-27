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
    #[error("file {0} not found")]
    NotFound(PathBuf),
    #[error("multiple files match {}: {}", .0.display(), .1.iter().map(|f| f.name.display()).format(", "))]
    MultipleMatches(PathBuf, Vec<Arc<SourceFile>>),
}

/// Performs file resolution by applying import paths and mappings.
#[derive(derive_more::Debug)]
pub struct FileResolver<'a> {
    #[debug(skip)]
    source_map: &'a SourceMap,

    /// Import paths and mappings.
    ///
    /// `(None, path)` is an import path.
    /// `(Some(map), path)` is a remapping.
    import_paths: Vec<(Option<PathBuf>, PathBuf)>,

    /// [`std::env::current_dir`] cache. Unused if the current directory is set manually.
    env_current_dir: Option<PathBuf>,
    /// Custom current directory.
    custom_current_dir: Option<PathBuf>,
}

impl<'a> FileResolver<'a> {
    /// Creates a new file resolver.
    ///
    /// If `current_dir` is `None`, the current directory is set to [`std::env::current_dir`].
    ///
    /// # Panics
    ///
    /// Panics if `current_dir` is `Some` and not an absolute path.
    #[track_caller]
    pub fn new(source_map: &'a SourceMap, current_dir: Option<&Path>) -> Self {
        let mut this = Self {
            source_map,
            import_paths: Vec::new(),
            env_current_dir: std::env::current_dir()
                .inspect_err(|e| debug!("failed to get current_dir: {e}"))
                .ok(),
            custom_current_dir: None,
        };
        if let Some(current_dir) = current_dir {
            this.set_current_dir(current_dir);
        }
        this
    }

    /// Returns the source map.
    pub fn source_map(&self) -> &'a SourceMap {
        self.source_map
    }

    /// Returns the current directory.
    pub fn current_dir(&self) -> &Path {
        self.custom_current_dir
            .as_deref()
            .or(self.env_current_dir.as_deref())
            .unwrap_or(Path::new("."))
    }

    /// Canonicalizes a path using [`Self::current_dir`].
    pub fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        let path = if path.is_absolute() {
            path
        } else if let Some(current_dir) = &self.custom_current_dir {
            &current_dir.join(path)
        } else {
            path
        };
        crate::canonicalize(path)
    }

    /// Sets the current directory.
    ///
    /// # Panics
    ///
    /// Panics if `current_dir` is not an absolute path.
    #[track_caller]
    pub fn set_current_dir(&mut self, current_dir: &Path) {
        if !current_dir.is_absolute() {
            panic!("current_dir must be an absolute path");
        }
        self.custom_current_dir = Some(current_dir.to_path_buf());
    }

    /// Adds an import path, AKA base path in solc.
    /// Returns `true` if the path is newly inserted.
    pub fn add_import_path(&mut self, path: PathBuf) -> bool {
        let entry = (None, path);
        let new = !self.import_paths.contains(&entry);
        if new {
            self.import_paths.push(entry);
        }
        new
    }

    /// Adds an import map, AKA remapping.
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

    /// Resolves an import path.
    ///
    /// `parent` is the path of the file that contains the import, if any.
    #[instrument(level = "debug", skip_all, fields(path = %path.display()))]
    pub fn resolve_file(
        &self,
        path: &Path,
        parent: Option<&Path>,
    ) -> Result<Arc<SourceFile>, ResolveError> {
        // https://docs.soliditylang.org/en/latest/path-resolution.html
        // Only when the path starts with ./ or ../ are relative paths considered; this means
        // that `import "b.sol";` will check the import paths for b.sol, while `import "./b.sol";`
        // will only check the path relative to the current file.
        let is_relative = path.starts_with("./") || path.starts_with("../");
        if (is_relative && parent.is_some()) || parent.is_none() {
            let try_path = if let Some(base) = parent.filter(|_| is_relative).and_then(Path::parent)
            {
                &base.join(path)
            } else {
                path
            };
            if let Some(file) = self.try_file(try_path)? {
                return Ok(file);
            }
            // See above.
            if is_relative {
                return Err(ResolveError::NotFound(path.into()));
            }
        }

        let original_path = path;
        let path = &*self.remap_path(path);
        let mut result = Vec::with_capacity(1);

        // Walk over the import paths until we find one that resolves.
        for import in &self.import_paths {
            if let (None, import_path) = import {
                let path = import_path.join(path);
                if let Some(file) = self.try_file(&path)? {
                    result.push(file);
                }
            }
        }

        // If there was no defined import path, then try the file directly. See
        // https://docs.soliditylang.org/en/latest/path-resolution.html#base-path-and-include-paths
        // "By default the base path is empty, which leaves the source unit name unchanged."
        if !self.import_paths.iter().any(|(m, _)| m.is_none()) {
            if let Some(file) = self.try_file(path)? {
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
        let path = &*path.normalize();
        if let Some(file) = self.source_map().get_file(path) {
            trace!("loaded from cache");
            return Ok(Some(file));
        }

        if let Ok(path) = self.canonicalize(path) {
            // Save the file name relative to the current directory.
            let mut relpath = path.as_path();
            if let Ok(p) = relpath.strip_prefix(self.current_dir()) {
                relpath = p;
            }
            trace!("canonicalized to {}", relpath.display());
            return self
                .source_map()
                // Can't use `load_file` with `rel_path` as a custom `current_dir` may be set.
                .load_file_with_name(relpath.to_path_buf().into(), &path)
                .map(Some)
                .map_err(|e| ResolveError::ReadFile(relpath.into(), e));
        }

        trace!("not found");
        Ok(None)
    }
}

//! File resolver.
//!
//! Modified from [`solang`](https://github.com/hyperledger/solang/blob/0f032dcec2c6e96797fd66fa0175a02be0aba71c/src/file_resolver.rs).

use super::SourceFile;
use crate::SourceMap;
use itertools::Itertools;
use normalize_path::NormalizePath;
use solar_config::ImportRemapping;
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

    /// Include paths.
    include_paths: Vec<PathBuf>,
    /// Import remappings.
    remappings: Vec<ImportRemapping>,

    /// [`std::env::current_dir`] cache. Unused if the current directory is set manually.
    env_current_dir: Option<PathBuf>,
    /// Custom current directory.
    custom_current_dir: Option<PathBuf>,
}

impl<'a> FileResolver<'a> {
    /// Creates a new file resolver.
    pub fn new(source_map: &'a SourceMap) -> Self {
        Self {
            source_map,
            include_paths: Vec::new(),
            remappings: Vec::new(),
            env_current_dir: std::env::current_dir()
                .inspect_err(|e| debug!("failed to get current_dir: {e}"))
                .ok(),
            custom_current_dir: None,
        }
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

    /// Adds include paths.
    pub fn add_include_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        self.include_paths.extend(paths);
    }

    /// Adds an include path.
    pub fn add_include_path(&mut self, path: PathBuf) {
        self.include_paths.push(path)
    }

    /// Adds import remappings.
    pub fn add_import_remappings(&mut self, remappings: impl IntoIterator<Item = ImportRemapping>) {
        self.remappings.extend(remappings);
    }

    /// Adds an import remapping.
    pub fn add_import_remapping(&mut self, remapping: ImportRemapping) {
        self.remappings.push(remapping);
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
        //
        // `parent.is_none()` only happens when resolving imports from a custom/stdin file, or when
        // manually resolving a file, like from CLI arguments. In these cases, the file is
        // considered to be in the current directory.
        // Technically, this behavior allows the latter, the manual case, to also be resolved using
        // remappings, which is not the case in solc, but this simplifies the implementation.
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
        let path = &*self.remap_path(path, parent);
        let mut result = Vec::with_capacity(1);

        // Walk over the import paths until we find one that resolves.
        for include_path in &self.include_paths {
            let path = include_path.join(path);
            if let Some(file) = self.try_file(&path)? {
                result.push(file);
            }
        }

        // If there are no include paths, then try the file directly. See
        // https://docs.soliditylang.org/en/latest/path-resolution.html#base-path-and-include-paths
        // "By default the base path is empty, which leaves the source unit name unchanged."
        if self.include_paths.is_empty() {
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
    // Reference: <https://github.com/ethereum/solidity/blob/e202d30db8e7e4211ee973237ecbe485048aae97/libsolidity/interface/ImportRemapper.cpp#L32>
    #[instrument(level = "trace", skip_all, ret)]
    pub fn remap_path<'b>(&self, path: &'b Path, parent: Option<&Path>) -> Cow<'b, Path> {
        let _context = &*parent.map(|p| p.to_string_lossy()).unwrap_or_default();

        let mut longest_prefix = 0;
        let mut longest_context = 0;
        let mut best_match_target = None;
        let mut unprefixed_path = path;
        for ImportRemapping { context, prefix, path: target } in &self.remappings {
            let context = &*sanitize_path(context);
            let prefix = &*sanitize_path(prefix);

            if context.len() < longest_context {
                continue;
            }
            if !_context.starts_with(context) {
                continue;
            }
            if prefix.len() < longest_prefix {
                continue;
            }
            let Ok(up) = path.strip_prefix(prefix) else {
                continue;
            };
            longest_context = context.len();
            longest_prefix = prefix.len();
            best_match_target = Some(sanitize_path(target));
            unprefixed_path = up;
        }
        if let Some(best_match_target) = best_match_target {
            let mut out = PathBuf::from(&*best_match_target);
            out.push(unprefixed_path);
            out.into()
        } else {
            Cow::Borrowed(unprefixed_path)
        }
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

fn sanitize_path(s: &str) -> impl std::ops::Deref<Target = str> + '_ {
    // TODO: Equivalent of: `boost::filesystem::path(_path).generic_string()`
    s
}

//! File resolver.
//!
//! Modified from [`solang`](https://github.com/hyperledger/solang/blob/0f032dcec2c6e96797fd66fa0175a02be0aba71c/src/file_resolver.rs).

use super::SourceFile;
use crate::{Session, SourceMap};
use itertools::Itertools;
use normalize_path::NormalizePath;
use solar_config::ImportRemapping;
use solar_data_structures::smallvec::SmallVec;
use std::{
    borrow::Cow,
    io,
    path::{Path, PathBuf},
    sync::{Arc, OnceLock},
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

    /// Custom current directory.
    custom_current_dir: Option<PathBuf>,
    /// [`std::env::current_dir`] cache. Unused if the current directory is set manually.
    env_current_dir: OnceLock<Option<PathBuf>>,
}

impl<'a> FileResolver<'a> {
    /// Creates a new file resolver.
    pub fn new(source_map: &'a SourceMap) -> Self {
        Self {
            source_map,
            include_paths: Vec::new(),
            remappings: Vec::new(),
            custom_current_dir: None,
            env_current_dir: OnceLock::new(),
        }
    }

    /// Configures the file resolver from a session.
    pub fn configure_from_sess(&mut self, sess: &Session) {
        self.add_include_paths(sess.opts.include_paths.iter().cloned());
        self.add_import_remappings(sess.opts.import_remappings.iter().cloned());
        'b: {
            if let Some(base_path) = &sess.opts.base_path {
                let base_path = if base_path.is_absolute() {
                    base_path.as_path()
                } else {
                    &if let Ok(path) = self.canonicalize_unchecked(base_path) {
                        path
                    } else {
                        break 'b;
                    }
                };
                self.set_current_dir(base_path);
            }
        }
    }

    /// Clears the internal state.
    pub fn clear(&mut self) {
        self.include_paths.clear();
        self.remappings.clear();
        self.custom_current_dir = None;
        self.env_current_dir.take();
    }

    /// Sets the current directory.
    ///
    /// # Panics
    ///
    /// Panics if `current_dir` is not an absolute path.
    #[track_caller]
    #[doc(alias = "set_base_path")]
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

    /// Returns the current directory, or `.` if it could not be resolved.
    #[doc(alias = "base_path")]
    pub fn current_dir(&self) -> &Path {
        self.try_current_dir().unwrap_or(Path::new("."))
    }

    /// Returns the current directory, if resolved successfully.
    #[doc(alias = "try_base_path")]
    pub fn try_current_dir(&self) -> Option<&Path> {
        self.custom_current_dir.as_deref().or_else(|| self.env_current_dir())
    }

    fn env_current_dir(&self) -> Option<&Path> {
        self.env_current_dir
            .get_or_init(|| {
                std::env::current_dir()
                    .inspect_err(|e| debug!("failed to get current_dir: {e}"))
                    .ok()
            })
            .as_deref()
    }

    /// Canonicalizes a path using [`Self::current_dir`].
    pub fn canonicalize(&self, path: &Path) -> io::Result<PathBuf> {
        self.canonicalize_unchecked(&self.make_absolute(path))
    }

    fn canonicalize_unchecked(&self, path: &Path) -> io::Result<PathBuf> {
        self.source_map.file_loader().canonicalize_path(path)
    }

    /// Normalizes a path removing unnecessary components.
    ///
    /// Does not perform I/O.
    pub fn normalize<'b>(&self, path: &'b Path) -> Cow<'b, Path> {
        // NOTE: checking `is_normalized` will not produce the correct result since it won't
        // consider `./` segments. See its documentation.
        Cow::Owned(path.normalize())
    }

    /// Makes the path absolute by joining it with the current directory.
    ///
    /// Does not perform I/O.
    pub fn make_absolute<'b>(&self, path: &'b Path) -> Cow<'b, Path> {
        if path.is_absolute() {
            Cow::Borrowed(path)
        } else if let Some(current_dir) = self.try_current_dir() {
            Cow::Owned(current_dir.join(path))
        } else {
            Cow::Borrowed(path)
        }
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

        let mut candidates = SmallVec::<[_; 1]>::new();
        // Quick deduplication when include paths are duplicated.
        let mut push_candidate = |file: Arc<SourceFile>| {
            if !candidates.iter().any(|f| Arc::ptr_eq(f, &file)) {
                candidates.push(file);
            }
        };

        // If there are no include paths, then try the file directly. See
        // https://docs.soliditylang.org/en/latest/path-resolution.html#base-path-and-include-paths
        // "By default the base path is empty, which leaves the source unit name unchanged."
        if self.include_paths.is_empty() || path.is_absolute() {
            if let Some(file) = self.try_file(path)? {
                push_candidate(file);
            }
        } else {
            // Try all the include paths.
            let base_path = self.try_current_dir().into_iter();
            for include_path in base_path.chain(self.include_paths.iter().map(|p| p.as_path())) {
                let path = include_path.join(path);
                if let Some(file) = self.try_file(&path)? {
                    push_candidate(file);
                }
            }
        }

        match candidates.len() {
            0 => Err(ResolveError::NotFound(original_path.into())),
            1 => Ok(candidates.pop().unwrap()),
            _ => Err(ResolveError::MultipleMatches(original_path.into(), candidates.into_vec())),
        }
    }

    /// Applies the import path mappings to `path`.
    // Reference: <https://github.com/argotorg/solidity/blob/e202d30db8e7e4211ee973237ecbe485048aae97/libsolidity/interface/ImportRemapper.cpp#L32>
    pub fn remap_path<'b>(&self, path: &'b Path, parent: Option<&Path>) -> Cow<'b, Path> {
        let remapped = self.remap_path_(path, parent);
        if remapped != path {
            trace!(remapped=%remapped.display());
        }
        remapped
    }

    fn remap_path_<'b>(&self, path: &'b Path, parent: Option<&Path>) -> Cow<'b, Path> {
        let _context = &*parent.map(|p| p.to_string_lossy()).unwrap_or_default();

        let mut longest_prefix = 0;
        let mut longest_context = 0;
        let mut best_match_target = None;
        let mut unprefixed_path = path;
        for ImportRemapping { context, prefix, path: target } in &self.remappings {
            let context = &*sanitize_path(context);
            let prefix = &*sanitize_path(prefix);

            // Skip if current context is closer.
            if context.len() < longest_context {
                continue;
            }
            // Skip if current context is not a prefix of the context.
            if !_context.starts_with(context) {
                continue;
            }
            // Skip if we already have a closer prefix match.
            if prefix.len() < longest_prefix && context.len() == longest_context {
                continue;
            }
            // Skip if the prefix does not match.
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
            Cow::Owned(out)
        } else {
            Cow::Borrowed(unprefixed_path)
        }
    }

    /// Loads stdin into the source map.
    pub fn load_stdin(&self) -> Result<Arc<SourceFile>, ResolveError> {
        self.source_map().load_stdin().map_err(ResolveError::ReadStdin)
    }

    /// Returns the source file with the given path, if it exists, without loading it.
    pub fn get_file(&self, path: &Path) -> Option<Arc<SourceFile>> {
        self.get_file_inner(path, false).ok().flatten()
    }

    /// Loads `path` into the source map. Returns `None` if the file doesn't exist.
    #[instrument(level = "debug", skip_all, fields(path = %path.display()))]
    pub fn try_file(&self, path: &Path) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        self.get_file_inner(path, true)
    }

    fn get_file_inner<'b>(
        &self,
        path: &'b Path,
        load: bool,
    ) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        // Normalize unnecessary components.
        let rpath = &*self.normalize(path);
        if let Some(file) = self.source_map().get_file(rpath) {
            trace!("loaded from cache 1");
            return Ok(Some(file));
        }

        // Make the path absolute with the current directory.
        let apath = &*self.make_absolute(rpath);
        if apath != rpath
            && let Some(file) = self.source_map().get_file(apath)
        {
            trace!("loaded from cache 2");
            return Ok(Some(file));
        }

        // Canonicalize, checking symlinks and if it exists.
        if load && let Ok(path) = self.canonicalize_unchecked(apath) {
            return self
                .source_map()
                // Store the file with `apath` as the name instead of `path`.
                // In case of symlinks we want to reference the symlink path, not the target path.
                .load_file_with_name(apath.to_path_buf().into(), &path)
                .map(Some)
                .map_err(|e| ResolveError::ReadFile(path, e));
        }

        trace!("not found");
        Ok(None)
    }
}

fn sanitize_path(s: &str) -> impl std::ops::Deref<Target = str> + '_ {
    // TODO: Equivalent of: `boost::filesystem::path(_path).generic_string()`
    s
}

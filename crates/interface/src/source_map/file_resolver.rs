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
    /// Base path for source unit names.
    base_path: Option<PathBuf>,

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
            base_path: source_map.base_path(),
            custom_current_dir: source_map.base_path(),
            env_current_dir: OnceLock::new(),
        }
    }

    /// Configures the file resolver from a session.
    pub fn configure_from_sess(&mut self, sess: &Session) {
        self.add_include_paths(sess.opts.include_paths.iter().cloned());
        self.add_import_remappings(sess.opts.import_remappings.iter().cloned());
        if let Ok(current_dir) = std::env::current_dir() {
            self.set_current_dir(&current_dir);
        }
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
                self.set_base_path(base_path);
                // Source unit names are relative to the base path after parent paths are stripped.
                self.set_current_dir(base_path);
            }
        }
    }

    /// Clears the internal state.
    pub fn clear(&mut self) {
        self.include_paths.clear();
        self.remappings.clear();
        self.base_path = None;
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

    /// Sets the base path.
    ///
    /// # Panics
    ///
    /// Panics if `base_path` is not an absolute path.
    #[track_caller]
    pub fn set_base_path(&mut self, base_path: &Path) {
        if !base_path.is_absolute() {
            panic!("base_path must be an absolute path");
        }
        self.base_path = Some(base_path.to_path_buf());
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

    /// Returns the base path for import resolution.
    pub fn try_base_path(&self) -> Option<&Path> {
        self.base_path.as_deref().or_else(|| self.try_current_dir())
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
        mut parent: Option<&Path>,
    ) -> Result<Arc<SourceFile>, ResolveError> {
        // `parent` comes from `FileName::Real` so it should be an absolute path.
        // Make it relative to the base path.
        if let Some(parent) = &mut parent
            && let Some(base_path) = self.try_base_path()
        {
            if let Ok(new_parent) = parent.strip_prefix(base_path) {
                *parent = new_parent;
            } else {
                trace!(?parent, ?base_path, "parent is not a subpath of the base path");
            }
        }

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
            let try_path = if is_relative
                && let Some(parent) = parent
                && let Some(parent_dir) = parent.parent()
            {
                &parent_dir.join(path)
            } else {
                path
            };
            if is_relative
                && let Some(file) = self.source_map().get_file(&*self.normalize(try_path))
            {
                return Ok(file);
            }
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

        if path.is_absolute() {
            if let Some(file) = self.try_file(path)? {
                push_candidate(file);
            }
        } else if let Some(file) = self.get_source_unit_file(path) {
            return Ok(file);
        } else {
            // Try the base path and all include paths.
            let base_path = self.try_base_path().into_iter();
            let mut searched = false;
            for include_path in base_path.chain(self.include_paths.iter().map(|p| p.as_path())) {
                searched = true;
                let path = include_path.join(path);
                if let Some(file) = self.try_file(&path)? {
                    push_candidate(file);
                }
            }
            if !searched && let Some(file) = self.try_file(path)? {
                push_candidate(file);
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

    fn get_source_unit_file(&self, path: &Path) -> Option<Arc<SourceFile>> {
        if path.is_absolute() {
            return None;
        }
        if let Some(file) = self.source_map().get_file(path) {
            return Some(file);
        }

        let rpath = &*self.normalize(path);
        if rpath != path { self.source_map().get_file(rpath) } else { None }
    }

    /// Loads `path` into the source map. Returns `None` if the file doesn't exist.
    #[instrument(level = "debug", skip_all, fields(path = %path.display()))]
    pub fn try_file(&self, path: &Path) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        self.get_file_inner(path, true)
    }

    fn get_file_inner(
        &self,
        path: &Path,
        load: bool,
    ) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        if let Some(file) = self.source_map().get_file(path) {
            trace!("loaded from cache 1");
            return Ok(Some(file));
        }

        // Make the path absolute before normalizing so leading `..` components are resolved
        // against the current directory instead of being discarded from a relative path.
        let apath = &*self.make_absolute(path);
        let rpath = &*self.normalize(apath);
        if rpath != path
            && let Some(file) = self.source_map().get_file(rpath)
        {
            trace!("loaded from cache 2");
            return Ok(Some(file));
        }

        // Canonicalize, checking symlinks and if it exists.
        if load && let Ok(path) = self.canonicalize_unchecked(rpath) {
            return self
                .source_map()
                // Store the file with `rpath` as the name instead of `path`.
                // In case of symlinks we want to reference the symlink path, not the target path.
                .load_file_with_name(rpath.to_path_buf().into(), &path)
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

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCase<'a> {
        remappings: &'a [&'a str],
        sources: &'a [Source<'a>],
    }
    struct Source<'a> {
        path: &'a str,
        // `<import string> => <resolved path>`
        imports: &'a [&'a str],
    }

    fn run(test_case: &TestCase<'_>) {
        static ONCE: std::sync::Once = std::sync::Once::new();
        ONCE.call_once(|| {
            tracing_subscriber::fmt::fmt()
                .with_test_writer()
                .with_max_level(tracing::level_filters::LevelFilter::TRACE)
                .init();
        });

        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();

        let sm = SourceMap::empty();
        sm.set_base_path(Some(base_path.clone()));
        for source in test_case.sources {
            let path = base_path.join(source.path);
            if let Some(parent) = path.parent() {
                let parent = parent.to_str().unwrap();
                std::fs::create_dir_all(parent).expect(parent);
            }
            std::fs::write(&path, "").expect(source.path);
            sm.load_file(&path).expect(source.path);
        }

        let mut file_resolver = FileResolver::new(&sm);
        for &remapping in test_case.remappings {
            file_resolver.add_import_remapping(remapping.parse().expect(remapping));
        }
        for &Source { path, imports } in test_case.sources {
            for (i, &import) in imports.iter().enumerate() {
                let res = (|| -> Result<(), Box<dyn std::error::Error>> {
                    let (import, expected) = import
                        .split_once(" => ")
                        .ok_or("import is not in the format <import string> => <resolved path>")?;
                    let parent = base_path.join(path);
                    let resolved = file_resolver.resolve_file(import.as_ref(), Some(&parent))?;
                    let actual_full = resolved.name.as_real().ok_or("resolved file has no path")?;
                    let actual = actual_full.strip_prefix(&base_path).ok().ok_or(
                        "resolved file path is not a subpath of the base path (not absolute?)",
                    )?;
                    let actual =
                        actual.to_str().ok_or("resolved file path is not a valid string")?;
                    if actual != expected {
                        return Err(format!(
                            "did not resolve to the expected path ({actual} != {expected})",
                        )
                        .into());
                    }
                    Ok(())
                })();
                match res {
                    Ok(()) => {}
                    Err(e) => panic!("{path}:{i}: [{import}] {e}"),
                }
            }
        }
    }

    // Taken from: https://github.com/argotorg/solidity/blob/32c8f080c4cc939df5a3c7ca5ad6b6144ee9aa66/test/libsolidity/Imports.cpp
    #[test]
    fn remappings() {
        run(&TestCase {
            remappings: &["s=s_1.4.6", "t=Tee"],
            sources: &[
                Source { path: "a", imports: &["s/s.sol => s_1.4.6/s.sol"] },
                Source { path: "b", imports: &["t/tee.sol => Tee/tee.sol"] },
                Source { path: "s_1.4.6/s.sol", imports: &[] },
                Source { path: "Tee/tee.sol", imports: &[] },
            ],
        })
    }

    #[test]
    fn context_dependent_remappings() {
        run(&TestCase {
            remappings: &["a:s=s_1.4.6", "b:s=s_1.4.7"],
            sources: &[
                Source { path: "a/a.sol", imports: &["s/s.sol => s_1.4.6/s.sol"] },
                Source { path: "b/b.sol", imports: &["s/s.sol => s_1.4.7/s.sol"] },
                Source { path: "s_1.4.6/s.sol", imports: &[] },
                Source { path: "s_1.4.7/s.sol", imports: &[] },
            ],
        })
    }

    #[test]
    fn context_dependent_remappings_ensure_default_and_module_preserved() {
        run(&TestCase {
            remappings: &[
                "foo=vendor/foo_2.0.0",
                "vendor/bar:foo=vendor/foo_1.0.0",
                "bar=vendor/bar",
            ],
            sources: &[
                Source {
                    path: "main.sol",
                    imports: &[
                        "foo/foo.sol => vendor/foo_2.0.0/foo.sol",
                        "bar/bar.sol => vendor/bar/bar.sol",
                    ],
                },
                Source {
                    path: "vendor/bar/bar.sol",
                    imports: &["foo/foo.sol => vendor/foo_1.0.0/foo.sol"],
                },
                Source { path: "vendor/foo_1.0.0/foo.sol", imports: &[] },
                Source { path: "vendor/foo_2.0.0/foo.sol", imports: &[] },
            ],
        })
    }

    #[test]
    fn context_dependent_remappings_order_independent() {
        let sources = &[
            Source { path: "a/main.sol", imports: &["x/y/z/z.sol => d/z.sol"] },
            Source { path: "a/b/main.sol", imports: &["x/y/z/z.sol => e/y/z/z.sol"] },
            Source { path: "d/z.sol", imports: &[] },
            Source { path: "e/y/z/z.sol", imports: &[] },
        ];
        run(&TestCase { remappings: &["a:x/y/z=d", "a/b:x=e"], sources });
        run(&TestCase { remappings: &["a/b:x=e", "a:x/y/z=d"], sources });
    }

    #[test]
    fn top_level_relative_path_uses_current_dir() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let cwd = tmp.path().join("cwd");
        let sibling = tmp.path().join("sibling");
        let source = sibling.join("a.sol");
        let import = sibling.join("b.sol");
        std::fs::create_dir_all(&cwd).unwrap();
        std::fs::create_dir_all(&sibling).unwrap();
        std::fs::write(&source, "").unwrap();
        std::fs::write(&import, "").unwrap();

        let sm = SourceMap::empty();
        sm.set_base_path(Some(cwd));
        let file_resolver = FileResolver::new(&sm);
        let resolved = file_resolver.resolve_file(Path::new("../sibling/a.sol"), None).unwrap();

        assert_eq!(resolved.name.as_real(), Some(source.as_path()));

        let parent = resolved.name.as_real().unwrap();
        let relative_import =
            file_resolver.resolve_file(Path::new("./b.sol"), Some(parent)).unwrap();
        assert_eq!(relative_import.name.as_real(), Some(import.as_path()));

        let direct_import = file_resolver.resolve_file(Path::new("b.sol"), Some(parent));
        assert!(
            matches!(direct_import, Err(ResolveError::NotFound(path)) if path == Path::new("b.sol"))
        );
    }

    #[test]
    fn relative_import_from_virtual_source_uses_source_unit_name() {
        let sm = SourceMap::empty();
        sm.set_base_path(Some(PathBuf::new()));
        let mut file_resolver = FileResolver::new(&sm);
        file_resolver.set_current_dir(&std::env::current_dir().unwrap());
        let imported = sm.new_source_file(PathBuf::from("B.sol"), "").unwrap();

        let resolved = file_resolver.resolve_file(Path::new("./B.sol"), Some(Path::new("A.sol")));

        assert!(Arc::ptr_eq(&resolved.unwrap(), &imported));
    }

    #[test]
    fn direct_import_without_current_dir_uses_source_unit_name() {
        use crate::source_map::FileLoader;

        struct Loader;

        impl FileLoader for Loader {
            fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
                Ok(path.to_path_buf())
            }

            fn load_stdin(&self) -> io::Result<String> {
                unreachable!()
            }

            fn load_file(&self, path: &Path) -> io::Result<String> {
                assert_eq!(path, Path::new("B.sol"));
                Ok(String::new())
            }

            fn load_binary_file(&self, _path: &Path) -> io::Result<Vec<u8>> {
                unreachable!()
            }
        }

        let sm = SourceMap::empty();
        sm.set_file_loader(Loader);
        let file_resolver = FileResolver::new(&sm);
        file_resolver.env_current_dir.set(None).unwrap();

        let resolved = file_resolver.resolve_file(Path::new("B.sol"), Some(Path::new("A.sol")));

        assert_eq!(resolved.unwrap().name.as_real(), Some(Path::new("B.sol")));
    }

    #[test]
    fn direct_import_reuses_preloaded_source_unit_name() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();
        let source_path = base_path.join("src/B.sol");
        std::fs::create_dir_all(source_path.parent().unwrap()).unwrap();
        std::fs::write(&source_path, "contract B {}").unwrap();

        let sm = SourceMap::empty();
        sm.set_base_path(Some(base_path.clone()));
        let imported = sm.new_source_file(PathBuf::from("src/B.sol"), "contract B {}").unwrap();
        let mut file_resolver = FileResolver::new(&sm);
        file_resolver.set_current_dir(&base_path);

        let resolved =
            file_resolver.resolve_file(Path::new("src/B.sol"), Some(Path::new("test/A.sol")));

        assert!(Arc::ptr_eq(&resolved.unwrap(), &imported));
    }
}

// ported-from: https://github.com/BenTheKush/solc_remapping_behavior_test
#[cfg(test)]
mod solang_import_resolution {
    use super::*;
    use std::collections::{HashMap, HashSet};

    struct FixtureFile {
        path: &'static str,
        imports: &'static [&'static str],
    }

    struct Scenario {
        name: &'static str,
        input: &'static str,
        remappings: &'static [&'static str],
        base_path: Option<&'static str>,
        include_paths: &'static [&'static str],
        should_resolve: bool,
    }

    struct Harness<'a> {
        _tmp: tempfile::TempDir,
        root: PathBuf,
        cwd: PathBuf,
        imports: HashMap<PathBuf, &'a [&'a str]>,
    }

    impl<'a> Harness<'a> {
        fn new(cwd: &str, files: &'a [FixtureFile]) -> Self {
            let tmp = tempfile::Builder::new()
                .prefix("solar-solang-import-resolution-test")
                .tempdir()
                .unwrap();
            let root = tmp.path().to_path_buf();
            let cwd = root.join(cwd);
            let mut imports = HashMap::default();
            for FixtureFile { path, imports: file_imports } in files {
                let path_on_disk = cwd.join(path);
                if let Some(parent) = path_on_disk.parent() {
                    std::fs::create_dir_all(parent).unwrap();
                }
                std::fs::write(path_on_disk, "").unwrap();
                let key = cwd.join(path).strip_prefix(&root).unwrap().to_path_buf();
                imports.insert(key, *file_imports);
            }
            Self { _tmp: tmp, root, cwd, imports }
        }

        fn run(&self, scenario: &Scenario) -> Result<(), ResolveError> {
            let sm = SourceMap::empty();
            sm.set_base_path(Some(self.cwd.clone()));
            let mut file_resolver = FileResolver::new(&sm);
            file_resolver.set_current_dir(&self.cwd);
            if let Some(base_path) = scenario.base_path {
                file_resolver.set_base_path(&self.cwd.join(base_path));
            }
            for include_path in scenario.include_paths {
                file_resolver.add_include_path(self.cwd.join(include_path));
            }
            for remapping in scenario.remappings {
                file_resolver.add_import_remapping(remapping.parse().unwrap());
            }

            let root_file = file_resolver.resolve_file(Path::new(scenario.input), None)?;
            let mut stack = vec![root_file];
            let mut seen = HashSet::new();
            while let Some(file) = stack.pop() {
                let path = file.name.as_real().unwrap();
                if !seen.insert(path.to_path_buf()) {
                    continue;
                }

                let key = path.strip_prefix(&self.root).unwrap();
                for import in self.imports.get(key).copied().unwrap_or_default() {
                    stack.push(file_resolver.resolve_file(Path::new(import), Some(path))?);
                }
            }
            Ok(())
        }
    }

    fn check_solang_import_resolution_scenarios(
        cwd: &str,
        files: &[FixtureFile],
        scenarios: &[Scenario],
    ) {
        let harness = Harness::new(cwd, files);
        for scenario in scenarios {
            let result = harness.run(scenario);
            assert_eq!(
                result.is_ok(),
                scenario.should_resolve,
                "{}: expected should_resolve={}, got {result:?}",
                scenario.name,
                scenario.should_resolve
            );
        }
    }

    #[test]
    fn solang_import_resolution_corpus() {
        check_solang_import_resolution_scenarios(
            "01_solang_remap_target",
            &[
                FixtureFile { path: "contracts/Contract.sol", imports: &["lib/Lib.sol"] },
                FixtureFile { path: "resources/node_modules/lib/Lib.sol", imports: &[] },
            ],
            &[
                Scenario {
                    name: "01.1 no remapping",
                    input: "contracts/Contract.sol",
                    remappings: &[],
                    base_path: None,
                    include_paths: &[],
                    should_resolve: false,
                },
                Scenario {
                    name: "01.2 no base path or include path",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=node_modules/lib"],
                    base_path: None,
                    include_paths: &[],
                    should_resolve: false,
                },
                Scenario {
                    name: "01.3 incomplete include paths",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=node_modules/lib"],
                    base_path: Some("."),
                    include_paths: &[],
                    should_resolve: false,
                },
                Scenario {
                    name: "01.4 incorrect include paths",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=node_modules/lib"],
                    base_path: Some("."),
                    include_paths: &["resources/node_modules"],
                    should_resolve: false,
                },
                Scenario {
                    name: "01.5 correct configuration",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=node_modules/lib"],
                    base_path: Some("."),
                    include_paths: &["resources"],
                    should_resolve: true,
                },
            ],
        );

        check_solang_import_resolution_scenarios(
            "02_solang_incorrect_direct_imports",
            &[
                FixtureFile { path: "Ambiguous.sol", imports: &[] },
                FixtureFile {
                    path: "contracts/Ambiguous.sol",
                    imports: &["Error: contracts/Ambiguous.sol should not be imported"],
                },
                FixtureFile { path: "contracts/Contract.sol", imports: &["Ambiguous.sol"] },
                FixtureFile {
                    path: "resources/node_modules/lib/Ambiguous.sol",
                    imports: &[
                        "Error: resources/node_modules/lib/Ambiguous.sol should not be imported",
                    ],
                },
                FixtureFile { path: "resources/node_modules/lib/Lib.sol", imports: &[] },
            ],
            &[
                Scenario {
                    name: "02.1 direct import default base path",
                    input: "contracts/Contract.sol",
                    remappings: &[],
                    base_path: None,
                    include_paths: &[],
                    should_resolve: true,
                },
                Scenario {
                    name: "02.2 direct import explicit base path",
                    input: "contracts/Contract.sol",
                    remappings: &[],
                    base_path: Some("."),
                    include_paths: &[],
                    should_resolve: true,
                },
            ],
        );

        check_solang_import_resolution_scenarios(
            "03_ambiguous_imports_should_fail",
            &[
                FixtureFile { path: "Ambiguous.sol", imports: &["This should not be imported"] },
                FixtureFile { path: "contracts/Ambiguous.sol", imports: &[] },
                FixtureFile {
                    path: "contracts/Contract.sol",
                    imports: &["lib/Lib.sol", "Ambiguous.sol"],
                },
                FixtureFile { path: "resources/node_modules/lib/Ambiguous.sol", imports: &[] },
                FixtureFile { path: "resources/node_modules/lib/Lib.sol", imports: &[] },
            ],
            &[
                Scenario {
                    name: "03.1 ambiguous imports should fail",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=resources/node_modules/lib"],
                    base_path: Some("."),
                    include_paths: &["contracts"],
                    should_resolve: false,
                },
                Scenario {
                    name: "03.2 import order resources then root",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=resources/node_modules/lib"],
                    base_path: Some("."),
                    include_paths: &["resources/node_modules/lib", "."],
                    should_resolve: false,
                },
                Scenario {
                    name: "03.3 import order root then resources",
                    input: "contracts/Contract.sol",
                    remappings: &["lib=resources/node_modules/lib"],
                    base_path: Some("."),
                    include_paths: &[".", "resources/node_modules/lib"],
                    should_resolve: false,
                },
            ],
        );

        check_solang_import_resolution_scenarios(
            "04_multiple_map_path_segments",
            &[
                FixtureFile { path: "contracts/Contract.sol", imports: &["lib/nested/Lib.sol"] },
                FixtureFile { path: "resources/node_modules/lib/nested/Lib.sol", imports: &[] },
            ],
            &[Scenario {
                name: "04.1 multiple import mapping segments",
                input: "contracts/Contract.sol",
                remappings: &["lib/nested=resources/node_modules/lib/nested"],
                base_path: Some("."),
                include_paths: &[],
                should_resolve: true,
            }],
        );

        check_solang_import_resolution_scenarios(
            "05_import_path_order_should_not_matter",
            &[
                FixtureFile { path: "contracts/Contract.sol", imports: &["A.sol"] },
                FixtureFile { path: "contracts/nested1/A.sol", imports: &[] },
                FixtureFile { path: "contracts/nested2/A.sol", imports: &[] },
            ],
            &[
                Scenario {
                    name: "05.1 include order nested1 then nested2",
                    input: "contracts/Contract.sol",
                    remappings: &[],
                    base_path: None,
                    include_paths: &["contracts/nested1", "contracts/nested2"],
                    should_resolve: false,
                },
                Scenario {
                    name: "05.2 include order nested2 then nested1",
                    input: "contracts/Contract.sol",
                    remappings: &[],
                    base_path: None,
                    include_paths: &["contracts/nested2", "contracts/nested1"],
                    should_resolve: false,
                },
            ],
        );

        check_solang_import_resolution_scenarios(
            "06_redundant_remaps",
            &[
                FixtureFile {
                    path: "contracts/Contract.sol",
                    imports: &["node_modules/lib/Lib.sol"],
                },
                FixtureFile { path: "resources/node_modules/lib/Lib.sol", imports: &[] },
            ],
            &[
                Scenario {
                    name: "06.1 multiple remappings",
                    input: "contracts/Contract.sol",
                    remappings: &[
                        "node_modules=resources/node_modules",
                        "node_modules=node_modules",
                    ],
                    base_path: Some("resources"),
                    include_paths: &[],
                    should_resolve: true,
                },
                Scenario {
                    name: "06.2 multiple remappings reversed",
                    input: "contracts/Contract.sol",
                    remappings: &[
                        "node_modules=node_modules",
                        "node_modules=resources/node_modules",
                    ],
                    base_path: Some("resources"),
                    include_paths: &[],
                    should_resolve: false,
                },
                Scenario {
                    name: "06.3 multiple remappings last wins",
                    input: "contracts/Contract.sol",
                    remappings: &[
                        "node_modules=node_modules",
                        "node_modules=resources/node_modules",
                        "node_modules=node_modules",
                    ],
                    base_path: Some("resources"),
                    include_paths: &[],
                    should_resolve: true,
                },
            ],
        );
    }
}

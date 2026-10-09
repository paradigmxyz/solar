//! File resolver.
//!
//! Modified from [`solang`](https://github.com/hyperledger/solang/blob/0f032dcec2c6e96797fd66fa0175a02be0aba71c/src/file_resolver.rs).
//!
//! Follows solc's [import path resolution](https://docs.soliditylang.org/en/latest/path-resolution.html):
//! an import path becomes a source unit name, which is looked up among the loaded sources and then
//! on disk under the base path and the include paths.

use super::SourceFile;
use crate::{Session, SourceMap};
use itertools::Itertools;
use normalize_path::NormalizePath;
use solar_config::{CompileOpts, ImportRemapping};
use solar_data_structures::smallvec::{SmallVec, smallvec};
use std::{
    borrow::Cow,
    io,
    path::{Component, Path, PathBuf},
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
            base_path: source_map.roots().as_ref().and_then(|roots| roots.base_path.clone()),
            custom_current_dir: None,
            env_current_dir: OnceLock::new(),
        }
    }

    /// Configures the file resolver from a session.
    pub fn configure_from_sess(&mut self, sess: &Session) {
        self.configure_from_opts(&sess.opts);
    }

    /// Configures the file resolver from compiler options.
    ///
    /// Relative base and include paths are relative to the current directory.
    pub fn configure_from_opts(&mut self, opts: &CompileOpts) {
        self.add_include_paths(opts.include_paths.iter().cloned());
        self.add_import_remappings(opts.import_remappings.iter().cloned());
        if let Some(base_path) = &opts.base_path {
            self.base_path = Some(self.absolute(base_path));
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
    ///
    /// Relative paths are relative to the current directory.
    pub fn add_include_paths(&mut self, paths: impl IntoIterator<Item = PathBuf>) {
        for path in paths {
            self.add_include_path(path);
        }
    }

    /// Adds an include path.
    ///
    /// Relative paths are relative to the current directory.
    pub fn add_include_path(&mut self, path: PathBuf) {
        let path = self.absolute(&path);
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
    pub fn current_dir(&self) -> &Path {
        self.try_current_dir().unwrap_or(Path::new("."))
    }

    /// Returns the current directory, if resolved successfully.
    pub fn try_current_dir(&self) -> Option<&Path> {
        self.custom_current_dir.as_deref().or_else(|| self.env_current_dir())
    }

    /// Returns the base path for import resolution, which defaults to the current directory.
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

    fn absolute(&self, path: &Path) -> PathBuf {
        absolute_path(self.try_current_dir(), path)
    }

    /// Returns the normalized filesystem paths that may satisfy `path`, in resolution order.
    ///
    /// This does not perform I/O or consult the source map. A returned path may remain relative if
    /// the resolver has no current directory. Use [`Self::resolve_file`] for exact resolution,
    /// including preloaded source-unit names and ambiguity detection.
    pub fn candidate_paths(&self, path: &Path, parent: Option<&Path>) -> Vec<PathBuf> {
        let Some(parent) = parent else { return vec![self.absolute(path)] };
        let unit = self.import_source_unit_name(path, parent);
        self.search_paths(&unit).iter().map(|candidate| self.absolute(candidate)).unique().collect()
    }

    /// Applies import remappings in the context of the source unit name of `parent`.
    pub fn remap_import_path<'b>(&self, path: &'b Path, parent: Option<&Path>) -> Cow<'b, Path> {
        self.remap_path(path, parent.map(|parent| self.source_unit_name(parent)))
    }

    /// Resolves an import path.
    ///
    /// `parent` is the path of the file that contains the import. Pass an empty path for a source
    /// without one, like standard input. Without a parent, `path` is loaded directly, like a path
    /// on the command line, relative to the current directory.
    #[instrument(level = "trace", skip_all, fields(path = %path.display()))]
    pub fn resolve_file(
        &self,
        path: &Path,
        parent: Option<&Path>,
    ) -> Result<Arc<SourceFile>, ResolveError> {
        let file = match parent {
            Some(parent) => self.resolve_source_unit(path, parent)?,
            None => self.try_file(path)?,
        };
        file.ok_or_else(|| ResolveError::NotFound(path.into()))
    }

    fn resolve_source_unit(
        &self,
        path: &Path,
        parent: &Path,
    ) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        let unit = self.import_source_unit_name(path, parent);
        if let Some(file) = self.get_source_unit(&unit) {
            return Ok(Some(file));
        }

        let mut found = SmallVec::<[Arc<SourceFile>; 1]>::new();
        for candidate in self.search_paths(&unit) {
            if let Some(file) = self.load(&candidate)?
                && !found.iter().any(|f| Arc::ptr_eq(f, &file))
            {
                found.push(file);
            }
        }
        match found.len() {
            0 | 1 => Ok(found.pop()),
            _ => Err(ResolveError::MultipleMatches(path.into(), found.into_vec())),
        }
    }

    /// Returns the source unit name that `path` refers to when imported from `parent`.
    fn import_source_unit_name<'b>(&self, path: &'b Path, parent: &Path) -> Cow<'b, Path> {
        let parent = self.source_unit_name(parent);
        // Only paths starting with `./` or `../` are relative to the importing source unit;
        // `import "b.sol";` is looked up in the base path and include paths.
        let path = if path.starts_with("./") || path.starts_with("../") {
            Cow::Owned(join_relative_import(parent, path))
        } else {
            Cow::Borrowed(path)
        };
        match self.remap_path(&path, Some(parent)) {
            Cow::Owned(remapped) => Cow::Owned(remapped),
            Cow::Borrowed(_) => path,
        }
    }

    /// Returns the base path, or an empty path without one, followed by the include paths.
    fn roots(&self) -> impl Iterator<Item = &Path> {
        let base_path = self.try_base_path().unwrap_or(Path::new(""));
        std::iter::once(base_path).chain(self.include_paths.iter().map(PathBuf::as_path))
    }

    /// Returns the source unit name of a loaded file, like solc does for command-line paths.
    fn source_unit_name<'b>(&self, path: &'b Path) -> &'b Path {
        strip_root(path, self.roots())
    }

    /// Returns the loaded source with the given source unit name.
    fn get_source_unit(&self, unit: &Path) -> Option<Arc<SourceFile>> {
        // An absolute name inside the base path may refer to a source loaded under its relative
        // name, like the ones a build tool preloads.
        self.source_map().get_file(unit).or_else(|| {
            self.source_map().get_file(self.source_unit_name(&self.rooted(unit).normalize()))
        })
    }

    /// Returns the paths that the host filesystem loader looks up for a source unit name.
    fn search_paths<'b>(&self, unit: &'b Path) -> SmallVec<[Cow<'b, Path>; 2]> {
        // Like solc, accept `file://` URLs.
        let unit = unit.to_str().and_then(|s| s.strip_prefix("file://")).map_or(unit, Path::new);
        // Unlike solc, look up absolute paths as is, even with a base path.
        if unit.has_root() {
            return smallvec![self.rooted(unit)];
        }
        self.roots().map(|root| Cow::Owned(root.join(unit))).collect()
    }

    /// Gives a path with a root but no drive, like `/a.sol` on Windows, the base path's drive.
    fn rooted<'b>(&self, path: &'b Path) -> Cow<'b, Path> {
        match self.try_base_path() {
            Some(base_path) if path.has_root() && !path.is_absolute() => {
                Cow::Owned(base_path.join(path))
            }
            _ => Cow::Borrowed(path),
        }
    }

    /// Applies the import path mappings to `path`.
    // Reference: <https://github.com/argotorg/solidity/blob/e202d30db8e7e4211ee973237ecbe485048aae97/libsolidity/interface/ImportRemapper.cpp#L32>
    pub fn remap_path<'b>(&self, path: &'b Path, parent: Option<&Path>) -> Cow<'b, Path> {
        let remapped = apply_import_remappings(&self.remappings, path, parent);
        if remapped != path {
            trace!(remapped=%remapped.display());
        }
        remapped
    }

    /// Loads stdin into the source map.
    pub fn load_stdin(&self) -> Result<Arc<SourceFile>, ResolveError> {
        self.source_map().load_stdin().map_err(ResolveError::ReadStdin)
    }

    /// Returns the source file with the given path, if it exists, without loading it.
    pub fn get_file(&self, path: &Path) -> Option<Arc<SourceFile>> {
        self.source_map()
            .get_file(path)
            .or_else(|| self.source_map().get_file(generic_path(self.absolute(path))))
    }

    /// Loads `path` into the source map. Returns `None` if the file doesn't exist.
    ///
    /// Relative paths are relative to the current directory.
    #[instrument(level = "debug", skip_all, fields(path = %path.display()))]
    pub fn try_file(&self, path: &Path) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        if let Some(file) = self.source_map().get_file(path) {
            return Ok(Some(file));
        }
        self.load(&self.make_absolute(path))
    }

    /// Loads the file at `path`, named by its normalized path.
    fn load(&self, path: &Path) -> Result<Option<Arc<SourceFile>>, ResolveError> {
        let path = generic_path(path.normalize());
        if let Some(file) = self.source_map().get_file(&path) {
            return Ok(Some(file));
        }
        // Check that the file exists. Its name keeps symbolic links, like solc.
        let Ok(canonical) = self.canonicalize_unchecked(&path) else {
            trace!(path=%path.display(), "not found");
            return Ok(None);
        };
        self.source_map()
            .load_file_with_name(path.into(), &canonical)
            .map(Some)
            .map_err(|e| ResolveError::ReadFile(canonical, e))
    }
}

/// Applies `remappings` to `path` using the same selection rules as [`FileResolver`].
pub fn apply_import_remappings<'a>(
    remappings: &[ImportRemapping],
    path: &'a Path,
    parent: Option<&Path>,
) -> Cow<'a, Path> {
    let context_path = parent.map(Path::to_string_lossy).unwrap_or_default();
    let context_path = sanitize_path(&context_path);

    let mut longest_prefix = 0;
    let mut longest_context = 0;
    let mut best_match_target = None;
    let path_text = path.to_string_lossy();
    let path_text = sanitize_path(&path_text);
    let mut unprefixed_path = &*path_text;
    for ImportRemapping { context, prefix, path: target } in remappings {
        let context = &*sanitize_path(context);
        let prefix = &*sanitize_path(prefix);

        // Skip if current context is closer.
        if context.len() < longest_context {
            continue;
        }
        // Skip if current context is not a prefix of the context.
        if !context_path.starts_with(context) {
            continue;
        }
        // Skip if we already have a closer prefix match.
        if prefix.len() < longest_prefix && context.len() == longest_context {
            continue;
        }
        // Skip if the prefix does not match.
        let Some(up) = path_text.strip_prefix(prefix) else {
            continue;
        };
        longest_context = context.len();
        longest_prefix = prefix.len();
        best_match_target = Some(sanitize_path(target));
        unprefixed_path = up;
    }
    if let Some(best_match_target) = best_match_target {
        Cow::Owned(PathBuf::from(format!("{best_match_target}{unprefixed_path}")))
    } else {
        Cow::Borrowed(path)
    }
}

/// Joins `path` with the current directory, if any, and normalizes it.
pub(crate) fn absolute_path(current_dir: Option<&Path>, path: &Path) -> PathBuf {
    match current_dir {
        Some(current_dir) if !path.is_absolute() => current_dir.join(path).normalize(),
        _ => path.normalize(),
    }
}

/// Strips the first root that strictly contains `path`.
pub(crate) fn strip_root(path: &Path, roots: impl IntoIterator<Item: AsRef<Path>>) -> &Path {
    roots
        .into_iter()
        .filter(|root| !root.as_ref().as_os_str().is_empty())
        .find_map(|root| path.strip_prefix(root).ok().filter(|p| !p.as_os_str().is_empty()))
        .unwrap_or(path)
}

/// Resolves a relative import against the source unit name of the importing file.
///
/// Only the import path is normalized; the importing name may contain `..` segments, or `//` in
/// URLs, that are part of its identity.
// Reference: <https://github.com/argotorg/solidity/blob/e202d30db8e7e4211ee973237ecbe485048aae97/libsolutil/CommonIO.cpp#L140>
fn join_relative_import(parent: &Path, path: &Path) -> PathBuf {
    let mut unit = parent.parent().unwrap_or(Path::new("")).to_path_buf();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir => {
                unit.pop();
            }
            component => unit.push(component),
        }
    }
    unit
}

/// Joins the components of `path` with `/`, like boost's `generic_string`.
fn generic_path(path: PathBuf) -> PathBuf {
    // `/` is not a separator after a verbatim `\\?\` prefix.
    #[cfg(windows)]
    if !matches!(path.components().next(), Some(Component::Prefix(p)) if p.kind().is_verbatim())
        && let Some(s) = path.to_str()
        && let Cow::Owned(s) = sanitize_path(s)
    {
        return s.into();
    }
    path
}

fn sanitize_path(s: &str) -> Cow<'_, str> {
    #[cfg(windows)]
    {
        if s.contains('\\') {
            return Cow::Owned(s.replace('\\', "/"));
        }
    }
    Cow::Borrowed(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn remapping_contexts_use_generic_windows_separators() {
        let remappings = ["src:pkg/=lib/source/".parse().unwrap()];
        let remapped = apply_import_remappings(
            &remappings,
            Path::new("pkg/Target.sol"),
            Some(Path::new(r"src\nested\Main.sol")),
        );

        assert_eq!(remapped, Path::new("lib/source/Target.sol"));
    }

    #[cfg(windows)]
    #[test]
    fn resolved_names_use_generic_windows_separators() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();
        std::fs::create_dir_all(base_path.join("src")).unwrap();
        for path in ["src/A.sol", "src/B.sol"] {
            std::fs::write(base_path.join(path), "").unwrap();
        }

        let sm = SourceMap::empty();
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(&base_path);
        let a = resolver.resolve_file(Path::new(r"src\A.sol"), None).unwrap();
        let b = resolver.resolve_file(Path::new("./B.sol"), a.name.as_real()).unwrap();

        for (file, name) in [(a, "src/A.sol"), (b, "src/B.sol")] {
            let path = file.name.as_real().unwrap().strip_prefix(&base_path).unwrap();
            assert_eq!(path.to_str(), Some(name));
        }
    }

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
        sm.set_roots(Some(base_path.clone()), Vec::new());
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
        sm.set_roots(Some(cwd.clone()), Vec::new());
        let mut file_resolver = FileResolver::new(&sm);
        file_resolver.set_current_dir(&cwd);
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
    fn top_level_path_takes_precedence_over_remapping() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();
        let direct = base_path.join("src/A.sol");
        let remapped = base_path.join("lib/A.sol");
        for path in [&direct, &remapped] {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }

        let sm = SourceMap::empty();
        sm.set_roots(Some(base_path.clone()), Vec::new());
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(&base_path);
        resolver.add_import_remapping("src/=lib/".parse().unwrap());

        let resolved = resolver.resolve_file(Path::new("src/A.sol"), None).unwrap();

        assert_eq!(resolved.name.as_real(), Some(direct.as_path()));
        assert_eq!(resolver.candidate_paths(Path::new("src/A.sol"), None), vec![direct]);
    }

    #[test]
    fn relative_import_from_virtual_source_uses_source_unit_name() {
        let sm = SourceMap::empty();
        sm.set_roots(Some(PathBuf::new()), Vec::new());
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
        sm.set_roots(Some(base_path.clone()), Vec::new());
        let imported = sm.new_source_file(PathBuf::from("src/B.sol"), "contract B {}").unwrap();
        let mut file_resolver = FileResolver::new(&sm);
        file_resolver.set_current_dir(&base_path);

        let resolved =
            file_resolver.resolve_file(Path::new("src/B.sol"), Some(Path::new("test/A.sol")));

        assert!(Arc::ptr_eq(&resolved.unwrap(), &imported));
    }

    #[test]
    fn absolute_remapping_reuses_preloaded_source_unit_name() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();
        for path in ["lib/dep/Test.sol", "lib/dep/Base.sol"] {
            let path = base_path.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }

        let sm = SourceMap::empty();
        sm.set_roots(Some(base_path.clone()), Vec::new());
        let test = sm.new_source_file(PathBuf::from("lib/dep/Test.sol"), "").unwrap();
        let base = sm.new_source_file(PathBuf::from("lib/dep/Base.sol"), "").unwrap();
        let mut file_resolver = FileResolver::new(&sm);
        file_resolver.set_current_dir(&base_path);
        file_resolver.add_import_remapping(
            format!("dep/={}/", base_path.join("lib/dep").display()).parse().unwrap(),
        );

        // `dep/Test.sol` remaps to an absolute path that names the preloaded `lib/dep/Test.sol`.
        let resolved = file_resolver
            .resolve_file(Path::new("dep/Test.sol"), Some(Path::new("test/A.t.sol")))
            .unwrap();
        assert!(Arc::ptr_eq(&resolved, &test));

        // Its relative imports then resolve to the same copies as direct imports do.
        let parent = resolved.name.as_real().unwrap();
        let relative = file_resolver.resolve_file(Path::new("./Base.sol"), Some(parent)).unwrap();
        let direct = file_resolver
            .resolve_file(Path::new("dep/Base.sol"), Some(Path::new("test/A.t.sol")))
            .unwrap();
        assert!(Arc::ptr_eq(&relative, &base));
        assert!(Arc::ptr_eq(&direct, &base));
    }

    #[test]
    fn candidate_paths_follow_remapping_and_search_order() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();
        let source = base_path.join("src/Main.sol");

        let sm = SourceMap::empty();
        sm.set_roots(Some(base_path.clone()), Vec::new());
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(&base_path);
        resolver.add_include_path(base_path.join("vendor"));
        resolver.add_import_remapping("oz=packages/openzeppelin".parse().unwrap());

        assert_eq!(
            resolver.candidate_paths(Path::new("oz/contracts/Token.sol"), Some(&source)),
            vec![
                base_path.join("packages/openzeppelin/contracts/Token.sol"),
                base_path.join("vendor/packages/openzeppelin/contracts/Token.sol"),
            ]
        );
    }

    #[test]
    fn candidate_paths_resolve_relative_imports_from_importer() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();
        let source = base_path.join("src/nested/Main.sol");

        let sm = SourceMap::empty();
        sm.set_roots(Some(base_path.clone()), Vec::new());
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(&base_path);
        resolver.add_include_path(base_path.join("vendor"));
        resolver.add_import_remapping("src:pkg/=packages/pkg/".parse().unwrap());

        // Like solc, the include paths are searched for the source unit name too.
        assert_eq!(
            resolver.candidate_paths(Path::new("../Shared.sol"), Some(&source)),
            vec![base_path.join("src/Shared.sol"), base_path.join("vendor/src/Shared.sol")]
        );
    }

    #[test]
    fn candidate_paths_apply_context_dependent_remappings() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let base_path = tmp.path().to_path_buf();

        let sm = SourceMap::empty();
        sm.set_roots(Some(base_path.clone()), Vec::new());
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(&base_path);
        resolver.add_import_remapping("src:pkg/=lib/source/".parse().unwrap());
        resolver.add_import_remapping("test:pkg/=lib/test/".parse().unwrap());

        assert_eq!(
            resolver.candidate_paths(
                Path::new("pkg/Dependency.sol"),
                Some(&base_path.join("src/Main.sol")),
            ),
            vec![base_path.join("lib/source/Dependency.sol")]
        );
        assert_eq!(
            resolver.candidate_paths(
                Path::new("pkg/Dependency.sol"),
                Some(&base_path.join("test/Main.t.sol")),
            ),
            vec![base_path.join("lib/test/Dependency.sol")]
        );
    }

    fn write_files(root: &Path, paths: &[&str]) {
        for path in paths {
            let path = root.join(path);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(path, "").unwrap();
        }
    }

    #[test]
    fn top_level_path_ignores_base_path() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let root = tmp.path();
        write_files(root, &["src/A.sol", "src/B.sol"]);

        let sm = SourceMap::empty();
        sm.set_roots(Some(root.join("src")), Vec::new());
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(root);

        let a = resolver.resolve_file(Path::new("src/A.sol"), None).unwrap();
        let b = resolver.resolve_file(Path::new("B.sol"), a.name.as_real()).unwrap();
        assert_eq!(a.name.as_real(), Some(root.join("src/A.sol").as_path()));
        assert_eq!(b.name.as_real(), Some(root.join("src/B.sol").as_path()));
        assert_eq!(sm.filename_for_diagnostics(&a.name).to_string(), "A.sol");
    }

    #[test]
    fn include_paths_name_source_units() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let root = tmp.path();
        write_files(root, &["src/A.sol", "lib/dep/C.sol", "lib/dep/D.sol"]);

        let sm = SourceMap::empty();
        sm.set_roots(Some(root.join("src")), vec![root.join("lib")]);
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(root);
        // Relative base and include paths are relative to the current directory.
        resolver.configure_from_opts(&CompileOpts {
            base_path: Some("src".into()),
            include_paths: vec!["lib".into()],
            ..Default::default()
        });

        let c = resolver.resolve_file(Path::new("dep/C.sol"), Some(&root.join("src/A.sol")));
        let c = c.unwrap();
        // The source unit name `dep/C.sol` resolves relative imports in the include path.
        let d = resolver.resolve_file(Path::new("./D.sol"), c.name.as_real()).unwrap();
        for (file, name) in [(c, "dep/C.sol"), (d, "dep/D.sol")] {
            assert_eq!(file.name.as_real(), Some(root.join("lib").join(name).as_path()));
            assert_eq!(sm.filename_for_diagnostics(&file.name).to_string(), name);
        }
    }

    #[test]
    fn nested_include_paths_name_by_base_path() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let root = tmp.path();
        write_files(root, &["src/A.sol", "lib/dep/C.sol"]);

        let sm = SourceMap::empty();
        sm.set_roots(Some(root.to_path_buf()), vec![root.join("lib")]);
        let mut resolver = FileResolver::new(&sm);
        resolver.add_include_path(root.join("lib"));

        // solc names this file `dep/C.sol`, by its import path.
        let c = resolver.resolve_file(Path::new("dep/C.sol"), Some(&root.join("src/A.sol")));
        assert_eq!(sm.filename_for_diagnostics(&c.unwrap().name).to_string(), "lib/dep/C.sol");
    }

    #[test]
    fn unnamed_source_imports_use_source_unit_names() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let root = tmp.path();
        write_files(root, &["A.sol", "src/A.sol", "lib/A.sol"]);

        let sm = SourceMap::empty();
        let mut resolver = FileResolver::new(&sm);
        resolver.set_current_dir(root);
        resolver.add_import_remapping("src/=lib/".parse().unwrap());

        // Imports from standard input are remapped, unlike paths on the command line.
        for (path, expected) in [("src/A.sol", "lib/A.sol"), ("./A.sol", "A.sol")] {
            let resolved = resolver.resolve_file(Path::new(path), Some(Path::new(""))).unwrap();
            assert_eq!(resolved.name.as_real(), Some(root.join(expected).as_path()), "{path}");
        }
        let resolved = resolver.resolve_file(Path::new("src/A.sol"), None).unwrap();
        assert_eq!(resolved.name.as_real(), Some(root.join("src/A.sol").as_path()));
    }

    #[test]
    fn file_url_imports_load_from_disk() {
        let tmp = tempfile::Builder::new().prefix("solar-file-resolver-test").tempdir().unwrap();
        let root = tmp.path();
        write_files(root, &["A.sol"]);

        let sm = SourceMap::empty();
        let resolver = FileResolver::new(&sm);
        let url = format!("file://{}", root.join("A.sol").display());
        let resolved = resolver.resolve_file(Path::new(&url), Some(Path::new("B.sol"))).unwrap();
        assert_eq!(resolved.name.as_real(), Some(root.join("A.sol").as_path()));
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
            sm.set_roots(Some(self.cwd.clone()), Vec::new());
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

    #[test]
    fn snapshots_base_path() {
        let sm = SourceMap::empty();
        sm.set_roots(Some(PathBuf::from("base")), Vec::new());
        let resolver = FileResolver::new(&sm);
        sm.set_roots(None, Vec::new());
        assert_eq!(resolver.try_base_path(), Some(Path::new("base")));
    }
}

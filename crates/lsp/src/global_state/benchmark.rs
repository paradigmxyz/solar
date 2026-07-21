//! Benchmark-only, in-memory LSP analysis support.

use super::{AnalysisBatch, DiagnosticMap, SymbolTables, analyze_with_source_map};
use crate::{project_fixture::ProjectFixture, utils::apply_document_changes, workspace::Workspace};
use crop::Rope;
use lsp_types::{
    Diagnostic, GotoDefinitionResponse, Hover, HoverContents, Location, Position, Range,
    TextDocumentContentChangeEvent, Url, WorkspaceSymbol,
};
use normalize_path::NormalizePath;
use solar_config::CompileOpts;
use solar_interface::{
    data_structures::map::{FxHashMap, FxHashSet},
    source_map::{FileLoader, SourceMap},
};
use std::{
    collections::BTreeMap,
    io,
    path::{Component, Path, PathBuf},
    sync::Arc,
};

/// An opaque error returned while preparing an LSP benchmark project.
#[doc(hidden)]
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct BenchmarkError {
    message: String,
}

impl BenchmarkError {
    fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

/// A prepared, entirely in-memory LSP benchmark project.
#[doc(hidden)]
#[derive(Clone)]
pub struct BenchmarkProject {
    root: PathBuf,
    opts: CompileOpts,
    files: Vec<(PathBuf, String)>,
    loader: InMemoryFileLoader,
    markers: BTreeMap<String, Vec<(PathBuf, Position)>>,
}

impl BenchmarkProject {
    /// Prepare the historical single-source benchmark project.
    pub fn from_source(source: String) -> Self {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches");
        let opts = CompileOpts { base_path: Some(root.clone()), ..Default::default() };
        Self::from_sources(opts, [(root.join("benchmark.sol"), source)])
            .expect("the built-in benchmark source path should be valid")
    }

    /// Prepare ordered primary sources and compiler options for repeated analysis.
    pub fn from_sources(
        mut opts: CompileOpts,
        sources: impl IntoIterator<Item = (PathBuf, String)>,
    ) -> Result<Self, BenchmarkError> {
        let root = opts
            .base_path
            .take()
            .ok_or_else(|| BenchmarkError::new("benchmark compiler options need a base path"))?;
        let root = absolute_normalized(&root)?;
        opts.base_path = Some(root.clone());

        let mut files = sources
            .into_iter()
            .map(|(path, source)| {
                let path = if path.is_absolute() { path } else { root.join(path) };
                let path = path.normalize();
                if !path.starts_with(&root) {
                    return Err(BenchmarkError::new(format!(
                        "benchmark source `{}` is outside project root `{}`",
                        path.display(),
                        root.display()
                    )));
                }
                Ok((path, source))
            })
            .collect::<Result<Vec<_>, _>>()?;
        files.sort_by(|(lhs, _), (rhs, _)| lhs.cmp(rhs));
        if files.is_empty() {
            return Err(BenchmarkError::new("benchmark project has no primary sources"));
        }
        if files.windows(2).any(|pair| pair[0].0 == pair[1].0) {
            return Err(BenchmarkError::new("benchmark project contains duplicate source paths"));
        }

        let loader_sources = files.iter().cloned().collect();
        let loader = InMemoryFileLoader::new(root.clone(), loader_sources);
        Ok(Self { root, opts, files, loader, markers: BTreeMap::new() })
    }

    /// Prepare a stable multi-file project from the fixture format shared with LSP tests.
    pub fn from_fixture(name: &str, fixture: &str) -> Result<Self, BenchmarkError> {
        if name.is_empty() {
            return Err(BenchmarkError::new("benchmark fixture name cannot be empty"));
        }
        let fixture_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("benches/fixtures");
        let root = resolve_relative_path(&fixture_root, Path::new(name))?;
        let fixture = ProjectFixture::try_parse(fixture)
            .map_err(|error| BenchmarkError::new(error.to_string()))?;
        let sources = fixture
            .files()
            .iter()
            .filter_map(|file| {
                let path = Path::new(file.path());
                (path.extension().is_some_and(|extension| extension == "sol")).then(|| {
                    let relative = path.strip_prefix("/").expect("fixture paths start with `/`");
                    (relative.to_path_buf(), file.text().to_string())
                })
            })
            .collect::<Vec<_>>();
        let opts = CompileOpts { base_path: Some(root), ..Default::default() };
        let mut project = Self::from_sources(opts, sources)?;
        for (name, markers) in fixture.markers() {
            let resolved = markers
                .iter()
                .map(|marker| {
                    let relative = Path::new(marker.path())
                        .strip_prefix("/")
                        .expect("fixture paths start with `/`");
                    let (path, _) = project.source(relative)?;
                    Ok((path.clone(), marker.position()))
                })
                .collect::<Result<Vec<_>, BenchmarkError>>()?;
            project.markers.insert(name.clone(), resolved);
        }
        Ok(project)
    }

    /// Load a Foundry project's manifest, primary sources, and import dependencies.
    ///
    /// All filesystem access happens during this constructor, before benchmark timing starts.
    pub fn from_foundry_manifest(path: impl AsRef<Path>) -> Result<Self, BenchmarkError> {
        let preparation_source_map = SourceMap::empty();
        let file_loader = preparation_source_map.file_loader();
        let manifest = file_loader.canonicalize_path(path.as_ref()).map_err(|error| {
            BenchmarkError::new(format!(
                "failed to resolve Foundry manifest `{}`: {error}",
                path.as_ref().display()
            ))
        })?;
        let mut workspace = Workspace::load_foundry(manifest.clone()).map_err(|error| {
            BenchmarkError::new(format!(
                "failed to load Foundry manifest `{}`: {error}",
                manifest.display()
            ))
        })?;
        workspace.refresh_source_files();

        let opts = workspace.compile_opts().clone();
        let root = opts
            .base_path
            .clone()
            .ok_or_else(|| BenchmarkError::new("Foundry benchmark project has no base path"))?;
        let mut loader_sources = FxHashMap::default();
        let mut files = Vec::with_capacity(workspace.source_files().len());
        for path in workspace.source_files() {
            let source = read_source(file_loader, path)?;
            loader_sources.insert(path.normalize(), source.clone());
            files.push((path.clone(), source));
        }
        for include_path in &opts.include_paths {
            collect_dependency_sources(file_loader, include_path, &mut loader_sources)?;
        }
        if files.is_empty() {
            return Err(BenchmarkError::new(format!(
                "Foundry benchmark project `{}` has no Solidity sources",
                root.display()
            )));
        }

        let root = root.normalize();
        let loader = InMemoryFileLoader::new(root.clone(), loader_sources);
        Ok(Self { root, opts, files, loader, markers: BTreeMap::new() })
    }

    /// The number of primary Solidity source files in this project.
    pub fn file_count(&self) -> usize {
        self.files.len()
    }

    /// The total number of bytes in all prepared primary and dependency sources.
    pub fn source_bytes(&self) -> usize {
        self.loader.sources.values().map(String::len).sum()
    }

    /// Resolve a unique source substring to its LSP URI and UTF-16 position.
    pub fn unique_anchor(
        &self,
        relative_path: impl AsRef<Path>,
        needle: &str,
    ) -> Result<(Url, Position), BenchmarkError> {
        let (path, source) = self.source(relative_path.as_ref())?;
        let start = unique_offset(source, needle, path)?;
        Ok((file_url(path)?, position_at(source, start)))
    }

    /// Resolve one fixture `$N` marker to its LSP URI and UTF-16 position.
    pub fn marker(&self, name: &str) -> Result<(Url, Position), BenchmarkError> {
        let name = name.strip_prefix('$').unwrap_or(name);
        let markers = self
            .markers
            .get(name)
            .ok_or_else(|| BenchmarkError::new(format!("missing benchmark marker `${name}`")))?;
        if markers.len() != 1 {
            return Err(BenchmarkError::new(format!("benchmark marker `${name}` is ambiguous")));
        }
        let (path, position) = &markers[0];
        Ok((file_url(path)?, *position))
    }

    /// Create a validated LSP range edit replacing one unique source substring.
    pub fn replacement_edit(
        &self,
        relative_path: impl AsRef<Path>,
        needle: &str,
        replacement: impl Into<String>,
    ) -> Result<BenchmarkEdit, BenchmarkError> {
        let (path, source) = self.source(relative_path.as_ref())?;
        let start = unique_offset(source, needle, path)?;
        let end = start + needle.len();
        Ok(BenchmarkEdit {
            path: path.clone(),
            change: TextDocumentContentChangeEvent {
                range: Some(Range::new(position_at(source, start), position_at(source, end))),
                range_length: None,
                text: replacement.into(),
            },
        })
    }

    /// Apply an edit with the same UTF-16 range logic used by document-change notifications.
    pub fn apply_edit(&mut self, edit: &BenchmarkEdit) -> Result<(), BenchmarkError> {
        let (_, source) =
            self.files.iter_mut().find(|(path, _)| path == &edit.path).ok_or_else(|| {
                BenchmarkError::new(format!(
                    "benchmark edit targets unknown source `{}`",
                    edit.path.display()
                ))
            })?;
        let updated =
            apply_document_changes(&Rope::from(source.as_str()), vec![edit.change.clone()])
                .to_string();
        *source = updated.clone();
        self.loader.sources.insert(edit.path.clone(), updated);
        Ok(())
    }

    /// Consume this prepared project and run the production compiler analysis pipeline.
    pub fn analyze(self) -> BenchmarkAnalysis {
        let Self { root, opts, files, loader, markers: _ } = self;
        let default_uri = files.first().and_then(|(path, _)| Url::from_file_path(path).ok());
        let source_map = Arc::new(SourceMap::empty());
        source_map.set_file_loader(loader);
        let result = analyze_with_source_map(
            AnalysisBatch { opts, files, seen_paths: FxHashSet::default() },
            source_map,
        );
        BenchmarkAnalysis {
            root,
            diagnostics: result.diagnostics,
            symbol_tables: result.symbol_tables,
            default_uri,
        }
    }

    fn source(&self, relative_path: &Path) -> Result<(&PathBuf, &str), BenchmarkError> {
        let path = resolve_relative_path(&self.root, relative_path)?;
        self.files
            .binary_search_by(|(candidate, _)| candidate.cmp(&path))
            .ok()
            .map(|index| (&self.files[index].0, self.files[index].1.as_str()))
            .ok_or_else(|| {
                BenchmarkError::new(format!(
                    "benchmark source `{}` is not a primary project file",
                    relative_path.display()
                ))
            })
    }
}

/// A validated document edit for a prepared benchmark project.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub struct BenchmarkEdit {
    path: PathBuf,
    change: TextDocumentContentChangeEvent,
}

/// A synchronous LSP query against an analyzed benchmark project.
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum BenchmarkRequest {
    Hover { uri: Url, position: Position },
    GotoDefinition { uri: Url, position: Position },
    References { uri: Url, position: Position, include_declaration: bool },
    WorkspaceSymbols { query: String },
}

/// The typed response to a [`BenchmarkRequest`].
#[doc(hidden)]
#[derive(Clone, Debug)]
pub enum BenchmarkResponse {
    Hover(Option<Hover>),
    GotoDefinition(Option<GotoDefinitionResponse>),
    References(Option<Vec<Location>>),
    WorkspaceSymbols(Vec<WorkspaceSymbol>),
}

/// An opaque analysis snapshot used by the LSP Criterion benchmarks.
#[doc(hidden)]
pub struct BenchmarkAnalysis {
    root: PathBuf,
    diagnostics: DiagnosticMap,
    symbol_tables: SymbolTables,
    default_uri: Option<Url>,
}

impl BenchmarkAnalysis {
    /// Analyze one in-memory Solidity source without touching the filesystem.
    pub fn from_source(source: String) -> Self {
        BenchmarkProject::from_source(source).analyze()
    }

    /// The number of diagnostics emitted for this analysis.
    pub fn diagnostic_count(&self) -> usize {
        self.diagnostics.values().map(Vec::len).sum()
    }

    /// A stable, project-relative representation of all diagnostics.
    pub fn diagnostic_fingerprint(&self) -> String {
        let mut diagnostics = self
            .diagnostics
            .iter()
            .flat_map(|(uri, diagnostics)| {
                diagnostics.iter().map(|diagnostic| diagnostic_line(&self.root, uri, diagnostic))
            })
            .collect::<Vec<_>>();
        diagnostics.sort();
        diagnostics.join("\n")
    }

    /// Execute one synchronous query against the analyzed symbol tables.
    #[inline(never)]
    pub fn execute(&self, request: &BenchmarkRequest) -> BenchmarkResponse {
        match request {
            BenchmarkRequest::Hover { uri, position } => {
                BenchmarkResponse::Hover(self.symbol_tables.hover(uri, *position))
            }
            BenchmarkRequest::GotoDefinition { uri, position } => {
                BenchmarkResponse::GotoDefinition(
                    self.symbol_tables.goto_definition(uri, *position),
                )
            }
            BenchmarkRequest::References { uri, position, include_declaration } => {
                BenchmarkResponse::References(self.symbol_tables.references(
                    uri,
                    *position,
                    *include_declaration,
                ))
            }
            BenchmarkRequest::WorkspaceSymbols { query } => {
                BenchmarkResponse::WorkspaceSymbols(self.symbol_tables.workspace_symbols(query))
            }
        }
    }

    /// Resolve one declaration or reference position synchronously.
    #[inline(never)]
    pub fn hover(&self, line: u32, character: u32) -> Option<usize> {
        let uri = self.default_uri.as_ref()?;
        let hover =
            std::hint::black_box(self.symbol_tables.hover(uri, Position::new(line, character)))?;
        let HoverContents::Markup(content) = hover.contents else { return None };
        Some(content.value.len())
    }
}

#[derive(Clone)]
struct InMemoryFileLoader {
    root: PathBuf,
    sources: FxHashMap<PathBuf, String>,
    directories: FxHashSet<PathBuf>,
}

impl InMemoryFileLoader {
    fn new(root: PathBuf, sources: FxHashMap<PathBuf, String>) -> Self {
        let mut directories = FxHashSet::default();
        directories.insert(root.clone());
        for path in sources.keys() {
            let mut current = path.parent();
            while let Some(directory) = current {
                directories.insert(directory.to_path_buf());
                if directory == root {
                    break;
                }
                current = directory.parent();
            }
        }
        Self { root, sources, directories }
    }

    fn normalized(&self, path: &Path) -> PathBuf {
        self.root.join(path).normalize()
    }

    fn not_found(path: &Path) -> io::Error {
        io::Error::new(
            io::ErrorKind::NotFound,
            format!("benchmark file `{}` was not prepared", path.display()),
        )
    }
}

impl FileLoader for InMemoryFileLoader {
    fn canonicalize_path(&self, path: &Path) -> io::Result<PathBuf> {
        let path = self.normalized(path);
        if self.sources.contains_key(&path) || self.directories.contains(&path) {
            Ok(path)
        } else {
            Err(Self::not_found(&path))
        }
    }

    fn load_stdin(&self) -> io::Result<String> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            "stdin is unavailable in an in-memory LSP benchmark",
        ))
    }

    fn load_file(&self, path: &Path) -> io::Result<String> {
        let path = self.normalized(path);
        self.sources.get(&path).cloned().ok_or_else(|| Self::not_found(&path))
    }

    fn load_binary_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        Err(io::Error::new(
            io::ErrorKind::Unsupported,
            format!(
                "binary file `{}` is unavailable in an in-memory LSP benchmark",
                path.display()
            ),
        ))
    }
}

fn absolute_normalized(path: &Path) -> Result<PathBuf, BenchmarkError> {
    if !path.is_absolute() {
        return Err(BenchmarkError::new(format!(
            "benchmark base path `{}` is not absolute",
            path.display()
        )));
    }
    Ok(path.normalize())
}

fn resolve_relative_path(root: &Path, relative_path: &Path) -> Result<PathBuf, BenchmarkError> {
    if relative_path.components().any(|component| {
        matches!(component, Component::ParentDir | Component::RootDir | Component::Prefix(_))
    }) {
        return Err(BenchmarkError::new(format!(
            "benchmark source path `{}` is not project-relative",
            relative_path.display()
        )));
    }
    Ok(root.join(relative_path).normalize())
}

fn unique_offset(source: &str, needle: &str, path: &Path) -> Result<usize, BenchmarkError> {
    if needle.is_empty() {
        return Err(BenchmarkError::new("benchmark anchor cannot be empty"));
    }
    let Some(start) = source.find(needle) else {
        return Err(BenchmarkError::new(format!(
            "benchmark anchor `{needle}` was not found in `{}`",
            path.display()
        )));
    };
    if source[start + needle.len()..].contains(needle) {
        return Err(BenchmarkError::new(format!(
            "benchmark anchor `{needle}` is not unique in `{}`",
            path.display()
        )));
    }
    Ok(start)
}

fn position_at(source: &str, offset: usize) -> Position {
    let prefix = &source[..offset];
    let line = prefix.bytes().filter(|&byte| byte == b'\n').count() as u32;
    let line_start = prefix.rfind('\n').map_or(0, |index| index + 1);
    let character = prefix[line_start..].encode_utf16().count() as u32;
    Position::new(line, character)
}

fn file_url(path: &Path) -> Result<Url, BenchmarkError> {
    Url::from_file_path(path).map_err(|()| {
        BenchmarkError::new(format!(
            "benchmark source `{}` cannot be represented as a file URI",
            path.display()
        ))
    })
}

fn read_source(file_loader: &dyn FileLoader, path: &Path) -> Result<String, BenchmarkError> {
    file_loader.load_file(path).map_err(|error| {
        BenchmarkError::new(format!(
            "failed to read benchmark source `{}`: {error}",
            path.display()
        ))
    })
}

fn collect_dependency_sources(
    file_loader: &dyn FileLoader,
    path: &Path,
    sources: &mut FxHashMap<PathBuf, String>,
) -> Result<(), BenchmarkError> {
    let mut directory_stack = FxHashSet::default();
    collect_dependency_sources_inner(file_loader, path, sources, &mut directory_stack)
}

fn collect_dependency_sources_inner(
    file_loader: &dyn FileLoader,
    path: &Path,
    sources: &mut FxHashMap<PathBuf, String>,
    directory_stack: &mut FxHashSet<PathBuf>,
) -> Result<(), BenchmarkError> {
    let Ok(metadata) = std::fs::metadata(path) else { return Ok(()) };
    if metadata.is_file() {
        if path.extension().is_some_and(|extension| extension == "sol") {
            sources.insert(path.normalize(), read_source(file_loader, path)?);
        }
        return Ok(());
    }
    if !metadata.is_dir() {
        return Ok(());
    }
    let canonical = file_loader.canonicalize_path(path).map_err(|error| {
        BenchmarkError::new(format!(
            "failed to resolve benchmark dependency directory `{}`: {error}",
            path.display()
        ))
    })?;
    if !directory_stack.insert(canonical.clone()) {
        return Ok(());
    }
    let entries = std::fs::read_dir(path).map_err(|error| {
        BenchmarkError::new(format!(
            "failed to enumerate benchmark dependency directory `{}`: {error}",
            path.display()
        ))
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| {
            BenchmarkError::new(format!(
                "failed to enumerate benchmark dependency directory `{}`: {error}",
                path.display()
            ))
        })?;
        collect_dependency_sources_inner(file_loader, &entry.path(), sources, directory_stack)?;
    }
    directory_stack.remove(&canonical);
    Ok(())
}

fn diagnostic_line(root: &Path, uri: &Url, diagnostic: &Diagnostic) -> String {
    let path = uri
        .to_file_path()
        .ok()
        .and_then(|path| path.strip_prefix(root).ok().map(Path::to_path_buf))
        .map(|path| path.to_string_lossy().replace('\\', "/"))
        .unwrap_or_else(|| uri.as_str().to_string());
    let range = diagnostic.range;
    let root = root.to_string_lossy();
    let message = diagnostic.message.replace(root.as_ref(), ".").replace('\n', "\\n");
    format!(
        "{path}:{}:{}:{}:{}:{:?}:{:?}:{}",
        range.start.line,
        range.start.character,
        range.end.line,
        range.end.character,
        diagnostic.severity,
        diagnostic.code,
        message
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    use std::{fs, os::unix::fs::symlink};

    #[cfg(unix)]
    #[test]
    fn dependency_collection_follows_directory_symlinks_without_cycles() {
        let temp = tempfile::tempdir().unwrap();
        let dependency = temp.path().join("vendor/package");
        fs::create_dir_all(&dependency).unwrap();
        fs::write(dependency.join("Dependency.sol"), "contract Dependency {}").unwrap();
        symlink(&dependency, dependency.join("cycle")).unwrap();

        let alias = temp.path().join("lib/package");
        fs::create_dir_all(alias.parent().unwrap()).unwrap();
        symlink(&dependency, &alias).unwrap();

        let source_map = SourceMap::empty();
        let mut sources = FxHashMap::default();
        collect_dependency_sources(source_map.file_loader(), &alias, &mut sources).unwrap();

        assert_eq!(sources.get(&alias.join("Dependency.sol")).unwrap(), "contract Dependency {}");
        assert_eq!(sources.len(), 1);
    }

    #[test]
    fn anchors_use_utf16_positions_and_require_uniqueness() {
        let project = BenchmarkProject::from_source("contract C { string s = \"中😀x\"; }".into());
        let (_, position) = project.unique_anchor("benchmark.sol", "x").unwrap();
        assert_eq!(position, Position::new(0, 28));

        let duplicate = BenchmarkProject::from_source("contract C { uint x; uint x; }".into());
        assert!(duplicate.unique_anchor("benchmark.sol", "x").is_err());
    }

    #[test]
    fn replacement_edits_feed_the_next_analysis() {
        let mut project = BenchmarkProject::from_source("contract C { uint x; }".into());
        let edit = project.replacement_edit("benchmark.sol", "uint x", "address x").unwrap();
        project.apply_edit(&edit).unwrap();
        let analysis = project.analyze();
        assert_eq!(analysis.diagnostic_count(), 0);
    }

    #[test]
    fn foundry_benchmark_corpus_resolves_from_memory() {
        let manifest = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../..")
            .join("tests/foundry/unifap-v2/foundry.toml");
        let project = BenchmarkProject::from_foundry_manifest(manifest).unwrap();
        let hover = project.unique_anchor("src/UnifapV2Pair.sol", "SELECTOR").unwrap();
        assert_eq!(project.file_count(), 14);
        assert_eq!(project.source_bytes(), 290_811);

        let analysis = project.analyze();
        assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
        assert!(matches!(
            analysis.execute(&BenchmarkRequest::Hover { uri: hover.0, position: hover.1 }),
            BenchmarkResponse::Hover(Some(_))
        ));
    }

    #[test]
    fn fixture_projects_support_relative_imports() {
        let project = BenchmarkProject::from_fixture(
            "relative-imports",
            r#"
                    //- /src/Imported.sol
                    contract Imported {}

                    //- /src/Main.sol
                    import "./Imported.sol";
                    contract Main is Imported {}
                "#,
        )
        .unwrap();
        assert_eq!(project.file_count(), 2);
        let analysis = project.analyze();
        assert_eq!(analysis.diagnostic_count(), 0, "{}", analysis.diagnostic_fingerprint());
    }

    #[test]
    fn fixture_projects_expose_markers() {
        let project = BenchmarkProject::from_fixture(
            "markers",
            r#"
                //- /src/Main.sol
                contract Main {
                    function $12run() external {}
                }
            "#,
        )
        .unwrap();

        let (uri, position) = project.marker("$12").unwrap();
        assert!(uri.path().ends_with("/src/Main.sol"));
        assert_eq!(position, Position::new(1, 13));
    }

    #[test]
    fn fixture_markers_must_target_primary_sources() {
        let result = BenchmarkProject::from_fixture(
            "non-source-marker",
            concat!(
                "//- /src/Main.sol\n",
                "contract Main {}\n",
                "//- /foundry.toml\n",
                "$0[profile.default]",
            ),
        );

        assert!(result.is_err());
    }

    #[test]
    fn malformed_fixtures_return_errors() {
        for fixture in [
            "contract BeforeMarker {}",
            "//-\ncontract MissingPath {}",
            "//- /Unknown.sol unsupported\ncontract Unknown {}",
        ] {
            assert!(BenchmarkProject::from_fixture("malformed", fixture).is_err());
        }
    }
}

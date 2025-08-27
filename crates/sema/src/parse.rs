use crate::{Gcx, hir::SourceId, ty::GcxMut};
use rayon::prelude::*;
use solar_ast::{self as ast, Span};
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashSet,
    sync::Lock,
};
use solar_interface::{
    Result, Session,
    config::CompilerStage,
    diagnostics::{DiagCtxt, ErrorGuaranteed},
    source_map::{FileName, FileResolver, ResolveError, SourceFile},
};
use solar_parse::{Lexer, Parser, unescape};
use std::{fmt, path::Path, sync::Arc};
use thread_local::ThreadLocal;

/// Builder for parsing sources into a [`Compiler`](crate::Compiler).
///
/// Created from [`CompilerRef::parse`](crate::CompilerRef::parse).
///
/// # Examples
///
/// ```
/// # let mut compiler = solar_sema::Compiler::new(solar_interface::Session::builder().with_stderr_emitter().build());
/// compiler.enter_mut(|compiler| {
///     let mut pcx = compiler.parse();
///     pcx.set_resolve_imports(false);
///     pcx.load_stdin();
///     pcx.parse();
/// });
/// ```
#[must_use = "`ParsingContext::parse` must be called to parse the sources"]
pub struct ParsingContext<'gcx> {
    /// The compiler session.
    pub sess: &'gcx Session,
    /// The file resolver.
    pub file_resolver: FileResolver<'gcx>,
    /// The loaded sources. Consumed once `parse` is called.
    pub(crate) sources: &'gcx mut Sources<'gcx>,
    /// The AST arenas.
    pub(crate) arenas: &'gcx ThreadLocal<ast::Arena>,
    /// Whether to recursively resolve and parse imports.
    resolve_imports: bool,
    /// Whether `parse` has been called.
    parsed: bool,
    gcx: Gcx<'gcx>,
}

impl<'gcx> ParsingContext<'gcx> {
    /// Creates a new parser context.
    pub(crate) fn new(mut gcx_: GcxMut<'gcx>) -> Self {
        let gcx = gcx_.get_mut();
        let sess = gcx.sess;
        let mut file_resolver = FileResolver::new(sess.source_map());
        file_resolver.configure_from_sess(sess);
        Self {
            sess,
            file_resolver,
            sources: &mut gcx.sources,
            arenas: &gcx.ast_arenas,
            resolve_imports: !sess.opts.unstable.no_resolve_imports,
            parsed: false,
            gcx: gcx_.get(),
        }
    }

    /// Returns the diagnostics context.
    #[inline]
    pub fn dcx(&self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    /// Sets whether to recursively resolve and parse imports.
    ///
    /// Default: `!sess.opts.unstable.no_resolve_imports`, `true`.
    pub fn set_resolve_imports(&mut self, resolve_imports: bool) {
        self.resolve_imports = resolve_imports;
    }

    /// Resolves a file.
    pub fn resolve_file(&self, path: impl AsRef<Path>) -> Result<Arc<SourceFile>> {
        self.file_resolver.resolve_file(path.as_ref(), None).map_err(self.map_resolve_error())
    }

    /// Resolves a list of files.
    pub fn resolve_files(
        &self,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> impl Iterator<Item = Result<Arc<SourceFile>>> {
        paths.into_iter().map(|path| self.resolve_file(path))
    }

    /// Resolves a list of files in parallel.
    pub fn par_resolve_files(
        &self,
        paths: impl IntoParallelIterator<Item = impl AsRef<Path>>,
    ) -> impl ParallelIterator<Item = Result<Arc<SourceFile>>> {
        paths.into_par_iter().map(|path| self.resolve_file(path))
    }

    /// Loads `stdin` into the context.
    #[instrument(level = "debug", skip_all)]
    pub fn load_stdin(&mut self) -> Result<()> {
        let file = self.file_resolver.load_stdin().map_err(self.map_resolve_error())?;
        self.add_file(file);
        Ok(())
    }

    /// Loads files into the context.
    #[instrument(level = "debug", skip_all)]
    pub fn load_files(&mut self, paths: impl IntoIterator<Item = impl AsRef<Path>>) -> Result<()> {
        for path in paths {
            self.load_file(path.as_ref())?;
        }
        Ok(())
    }

    /// Loads files into the context in parallel.
    pub fn par_load_files(
        &mut self,
        paths: impl IntoParallelIterator<Item = impl AsRef<Path>>,
    ) -> Result<()> {
        let resolved = self.par_resolve_files(paths).collect::<Result<Vec<_>>>()?;
        self.add_files(resolved);
        Ok(())
    }

    /// Loads a file into the context.
    #[instrument(level = "debug", skip_all)]
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let file = self.resolve_file(path)?;
        self.add_file(file);
        Ok(())
    }

    /// Adds a preloaded file to the resolver.
    pub fn add_files(&mut self, files: impl IntoIterator<Item = Arc<SourceFile>>) {
        for file in files {
            self.add_file(file);
        }
    }

    /// Adds a preloaded file to the resolver.
    pub fn add_file(&mut self, file: Arc<SourceFile>) {
        self.sources.get_or_insert_file(file);
    }

    /// Parses all the loaded sources, recursing into imports if specified.
    ///
    /// Sources are not guaranteed to be in any particular order, as they may be parsed in parallel.
    #[instrument(level = "debug", skip_all)]
    pub fn parse(mut self) {
        self.parsed = true;
        let _ = self.gcx.advance_stage(CompilerStage::Parsing);
        let mut sources = std::mem::take(self.sources);
        if !sources.is_empty() {
            if self.sess.is_sequential() || (sources.len() == 1 && !self.resolve_imports) {
                self.parse_sequential(&mut sources, self.arenas.get_or_default());
            } else {
                self.parse_parallel(&mut sources, self.arenas);
            }
            debug!(
                num_sources = sources.len(),
                num_contracts = sources.iter().map(|s| s.count_contracts()).sum::<usize>(),
                total_bytes = %crate::fmt_bytes(sources.iter().map(|s| s.file.src.len()).sum::<usize>()),
                total_lines = sources.iter().map(|s| s.file.count_lines()).sum::<usize>(),
                "parsed all sources",
            );
        }
        sources.assert_unique();
        *self.sources = sources;
    }

    fn parse_sequential<'ast>(&self, sources: &mut Sources<'ast>, arena: &'ast ast::Arena) {
        for i in 0.. {
            let id = SourceId::from_usize(i);
            let Some(source) = sources.get(id) else { break };
            if source.ast.is_some() {
                continue;
            }

            let ast = self.parse_one(&source.file, arena);
            let _guard = debug_span!("resolve_imports").entered();
            for (import_item_id, import_file) in
                self.resolve_imports(&source.file.clone(), ast.as_ref())
            {
                sources.add_import(id, import_item_id, import_file);
            }
            sources[id].ast = ast;
        }
    }

    fn parse_parallel<'ast>(
        &self,
        sources: &mut Sources<'ast>,
        arenas: &'ast ThreadLocal<ast::Arena>,
    ) {
        let lock = Lock::new(std::mem::take(sources));
        rayon::scope(|scope| {
            let sources = &*lock.lock();
            for (id, source) in sources.iter_enumerated() {
                if source.ast.is_some() {
                    continue;
                }
                let file = source.file.clone();
                self.spawn_parse_job(&lock, id, file, arenas, scope);
            }
        });
        *sources = lock.into_inner();
    }

    fn spawn_parse_job<'ast, 'scope>(
        &'scope self,
        lock: &'scope Lock<Sources<'ast>>,
        id: SourceId,
        file: Arc<SourceFile>,
        arenas: &'ast ThreadLocal<ast::Arena>,
        scope: &rayon::Scope<'scope>,
    ) {
        scope.spawn(move |scope| self.parse_job(lock, id, file, arenas, scope));
    }

    #[instrument(level = "debug", skip_all)]
    fn parse_job<'ast, 'scope>(
        &'scope self,
        lock: &'scope Lock<Sources<'ast>>,
        id: SourceId,
        file: Arc<SourceFile>,
        arenas: &'ast ThreadLocal<ast::Arena>,
        scope: &rayon::Scope<'scope>,
    ) {
        // Parse and resolve imports.
        let ast = self.parse_one(&file, arenas.get_or_default());
        let imports = {
            let _guard = debug_span!("resolve_imports").entered();
            self.resolve_imports(&file, ast.as_ref()).collect::<Vec<_>>()
        };

        // Set AST, add imports and recursively spawn jobs for parsing them if necessary.
        let _guard = debug_span!("add_imports").entered();
        let sources = &mut *lock.lock();
        assert!(sources[id].ast.is_none());
        sources[id].ast = ast;
        for (import_item_id, import_file) in imports {
            let (import_id, is_new) = sources.add_import(id, import_item_id, import_file.clone());
            if is_new {
                self.spawn_parse_job(lock, import_id, import_file, arenas, scope);
            }
        }
    }

    /// Parses a single file.
    #[instrument(level = "debug", skip_all, fields(file = %file.name.display()))]
    fn parse_one<'ast>(
        &self,
        file: &SourceFile,
        arena: &'ast ast::Arena,
    ) -> Option<ast::SourceUnit<'ast>> {
        let lexer = Lexer::from_source_file(self.sess, file);
        let mut parser = Parser::from_lexer(arena, lexer);
        if self.sess.opts.language.is_yul() {
            let _file = parser.parse_yul_file_object().map_err(|e| e.emit());
            None
        } else {
            parser.parse_file().map_err(|e| e.emit()).ok()
        }
    }

    /// Resolves the imports of the given file, returning an iterator over all the imported files
    /// that were successfully resolved.
    fn resolve_imports(
        &self,
        file: &SourceFile,
        ast: Option<&ast::SourceUnit<'_>>,
    ) -> impl Iterator<Item = (ast::ItemId, Arc<SourceFile>)> {
        let parent = match &file.name {
            FileName::Real(path) => Some(path.as_path()),
            FileName::Stdin | FileName::Custom(_) => None,
        };
        let items =
            ast.filter(|_| self.resolve_imports).map(|ast| &ast.items[..]).unwrap_or_default();
        items
            .iter_enumerated()
            .filter_map(move |(id, item)| self.resolve_import(item, parent).map(|file| (id, file)))
    }

    fn resolve_import(
        &self,
        item: &ast::Item<'_>,
        parent: Option<&Path>,
    ) -> Option<Arc<SourceFile>> {
        let ast::ItemKind::Import(import) = &item.kind else { return None };
        let span = import.path.span;
        let path_str = import.path.value.as_str();
        let (path_bytes, any_error) =
            unescape::parse_string_literal(path_str, unescape::StrKind::Str, span, self.sess);
        if any_error {
            return None;
        }
        let Some(path) = path_from_bytes(&path_bytes[..]) else {
            self.dcx().err("import path is not a valid UTF-8 string").span(span).emit();
            return None;
        };
        self.file_resolver
            .resolve_file(path, parent)
            .map_err(self.map_resolve_error_with(Some(span)))
            .ok()
    }

    fn map_resolve_error(&self) -> impl FnOnce(ResolveError) -> ErrorGuaranteed {
        self.map_resolve_error_with(None)
    }

    fn map_resolve_error_with(
        &self,
        span: Option<Span>,
    ) -> impl FnOnce(ResolveError) -> ErrorGuaranteed {
        move |e| {
            let mut err = self.dcx().err(e.to_string());
            if let Some(span) = span {
                err = err.span(span);
            }
            err.emit()
        }
    }
}

impl Drop for ParsingContext<'_> {
    fn drop(&mut self) {
        if self.parsed {
            return;
        }
        // This used to be a call to `bug` but it can be hit legitimately for example when there is
        // an error returned with `?` in between calls to `parse`.
        warn!("`ParsingContext::parse` not called");
    }
}

#[cfg(unix)]
fn path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    use std::os::unix::ffi::OsStrExt;
    Some(Path::new(std::ffi::OsStr::from_bytes(bytes)))
}

#[cfg(not(unix))]
fn path_from_bytes(bytes: &[u8]) -> Option<&Path> {
    std::str::from_utf8(bytes).ok().map(Path::new)
}

/// Sources.
#[derive(Default)]
pub struct Sources<'ast> {
    pub sources: IndexVec<SourceId, Source<'ast>>,
}

impl fmt::Debug for Sources<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ParsedSources")?;
        self.sources.fmt(f)
    }
}

impl<'ast> Sources<'ast> {
    /// Creates a new empty list of parsed sources.
    pub fn new() -> Self {
        Self { sources: IndexVec::new() }
    }

    /// Returns the ID of the imported file, and whether it was newly added.
    fn add_import(
        &mut self,
        current: SourceId,
        import_item_id: ast::ItemId,
        import: Arc<SourceFile>,
    ) -> (SourceId, bool) {
        let (import_id, new) = self.get_or_insert_file(import);
        self.sources[current].imports.push((import_item_id, import_id));
        (import_id, new)
    }

    #[instrument(level = "debug", skip_all)]
    fn get_or_insert_file(&mut self, file: Arc<SourceFile>) -> (SourceId, bool) {
        if let Some((id, _)) = self.get_file(&file) {
            return (id, false);
        }
        (self.sources.push(Source::new(file)), true)
    }

    /// Returns the ID of the source file, if it exists.
    pub fn get_file(&self, file: &Arc<SourceFile>) -> Option<(SourceId, &Source<'ast>)> {
        self.sources.iter_enumerated().find(|(_, source)| Arc::ptr_eq(&source.file, file))
    }

    /// Returns the ID of the source file, if it exists.
    pub fn get_file_mut(
        &mut self,
        file: &Arc<SourceFile>,
    ) -> Option<(SourceId, &mut Source<'ast>)> {
        let (id, _) = self.get_file(file)?;
        Some((id, &mut self.sources[id]))
    }

    /// Asserts that all sources are unique.
    fn assert_unique(&self) {
        if self.sources.len() <= 1 {
            return;
        }

        debug_assert_eq!(
            self.sources.iter().map(|s| &*s.file).collect::<FxHashSet<_>>().len(),
            self.sources.len(),
            "parsing produced duplicate source files"
        );
    }

    /// Returns an iterator over all the ASTs.
    pub fn asts(&self) -> impl DoubleEndedIterator<Item = &ast::SourceUnit<'ast>> {
        self.sources.iter().filter_map(|source| source.ast.as_ref())
    }

    /// Returns a parallel iterator over all the ASTs.
    pub fn par_asts(&self) -> impl ParallelIterator<Item = &ast::SourceUnit<'ast>> {
        self.sources.as_raw_slice().par_iter().filter_map(|source| source.ast.as_ref())
    }

    /// Sorts the sources topologically in-place. Invalidates all source IDs.
    #[instrument(level = "debug", skip_all)]
    pub fn topo_sort(&mut self) {
        let len = self.len();
        if len <= 1 {
            return;
        }

        let mut order = Vec::with_capacity(len);
        let mut seen = FxHashSet::with_capacity_and_hasher(len, Default::default());
        debug_span!("topo_order").in_scope(|| {
            for id in self.sources.indices() {
                self.topo_order(id, &mut order, &mut seen);
            }
        });

        debug_span!("remap_imports").in_scope(|| {
            for source in &mut self.sources {
                for (_, import) in &mut source.imports {
                    *import =
                        SourceId::from_usize(order.iter().position(|id| id == import).unwrap());
                }
            }
        });

        debug_span!("sort_by_indices").in_scope(|| {
            sort_by_indices(&mut self.sources, order);
        });
    }

    fn topo_order(&self, id: SourceId, order: &mut Vec<SourceId>, seen: &mut FxHashSet<SourceId>) {
        if !seen.insert(id) {
            return;
        }
        for &(_, import_id) in &self.sources[id].imports {
            self.topo_order(import_id, order, seen);
        }
        order.push(id);
    }
}

impl<'ast> std::ops::Deref for Sources<'ast> {
    type Target = IndexVec<SourceId, Source<'ast>>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.sources
    }
}

impl std::ops::DerefMut for Sources<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sources
    }
}

/// A single source.
pub struct Source<'ast> {
    /// The source file.
    pub file: Arc<SourceFile>,
    /// The AST IDs and source IDs of all the imports.
    pub imports: Vec<(ast::ItemId, SourceId)>,
    /// The AST.
    ///
    /// `None` if:
    /// - not yet parsed
    /// - an error occurred during parsing
    /// - the source is a Yul file
    /// - manually dropped to free memory
    pub ast: Option<ast::SourceUnit<'ast>>,
}

impl fmt::Debug for Source<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Source")
            .field("file", &self.file.name)
            .field("imports", &self.imports)
            .field("ast", &self.ast.as_ref().map(|ast| format!("{} items", ast.items.len())))
            .finish()
    }
}

impl Source<'_> {
    /// Creates a new empty source.
    pub fn new(file: Arc<SourceFile>) -> Self {
        Self { file, ast: None, imports: Vec::new() }
    }

    fn count_contracts(&self) -> usize {
        self.ast.as_ref().map(|ast| ast.count_contracts()).unwrap_or(0)
    }
}

/// Sorts `data` according to `indices`.
///
/// Adapted from: <https://stackoverflow.com/a/69774341>
fn sort_by_indices<I: Idx, T>(data: &mut IndexVec<I, T>, mut indices: Vec<I>) {
    assert_eq!(data.len(), indices.len());
    for idx in data.indices() {
        if indices[idx.index()] != idx {
            let mut current_idx = idx;
            loop {
                let target_idx = indices[current_idx.index()];
                indices[current_idx.index()] = current_idx;
                if indices[target_idx.index()] == target_idx {
                    break;
                }
                data.swap(current_idx, target_idx);
                current_idx = target_idx;
            }
        }
    }
}

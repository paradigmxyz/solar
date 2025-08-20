use crate::{hir::SourceId, ty::GcxMut};
use rayon::prelude::*;
use solar_ast as ast;
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashSet,
};
use solar_interface::{
    Result, Session,
    diagnostics::DiagCtxt,
    pluralize,
    source_map::{FileName, FileResolver, SourceFile},
};
use solar_parse::{Lexer, Parser, unescape};
use std::{fmt, path::Path, sync::Arc};
use thread_local::ThreadLocal;

#[must_use]
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
    /// Whether the context has been parsed.
    parsed: bool,
}

impl<'gcx> ParsingContext<'gcx> {
    /// Creates a new parser context.
    pub(crate) fn new(mut gcx: GcxMut<'gcx>) -> Self {
        let gcx = gcx.get_mut();
        let sess = gcx.sess;
        Self {
            sess,
            file_resolver: FileResolver::new(sess.source_map()),
            sources: &mut gcx.sources,
            arenas: &gcx.ast_arenas,
            resolve_imports: true,
            parsed: false,
        }
    }

    /// Returns the diagnostics context.
    #[inline]
    pub fn dcx(&self) -> &'gcx DiagCtxt {
        &self.sess.dcx
    }

    /// Sets whether to recursively resolve and parse imports.
    ///
    /// Default: `true`.
    pub fn set_resolve_imports(&mut self, resolve_imports: bool) {
        self.resolve_imports = resolve_imports;
    }

    /// Loads `stdin` into the context.
    #[instrument(level = "debug", skip_all)]
    pub fn load_stdin(&mut self) -> Result<()> {
        let file =
            self.file_resolver.load_stdin().map_err(|e| self.dcx().err(e.to_string()).emit())?;
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

    /// Loads a file into the context.
    #[instrument(level = "debug", skip_all)]
    pub fn load_file(&mut self, path: &Path) -> Result<()> {
        let file = self
            .file_resolver
            .resolve_file(path, None)
            .map_err(|e| self.dcx().err(e.to_string()).emit())?;
        self.add_file(file);
        Ok(())
    }

    /// Adds a preloaded file to the resolver.
    pub fn add_file(&mut self, file: Arc<SourceFile>) {
        self.sources.add_file(file);
    }

    /// Parses all the loaded sources, recursing into imports if specified.
    ///
    /// Sources are not guaranteed to be in any particular order, as they may be parsed in parallel.
    #[instrument(level = "debug", skip_all)]
    pub fn parse(mut self) {
        self.parsed = true;
        let mut sources = std::mem::take(self.sources);
        if !sources.is_empty() {
            let arenas = self.arenas;
            if self.sess.is_sequential() {
                self.parse_sequential(&mut sources, arenas.get_or_default());
            } else {
                self.parse_parallel(&mut sources, arenas);
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
            let current_file = SourceId::from_usize(i);
            let Some(source) = sources.get(current_file) else { break };
            if source.ast.is_some() {
                continue;
            }

            let ast = self.parse_one(&source.file, arena);
            let n_sources = sources.len();
            for (import_item_id, import) in self.resolve_imports(&source.file, ast.as_ref()) {
                sources.add_import(current_file, import_item_id, import);
            }
            let new_files = sources.len() - n_sources;
            if new_files > 0 {
                trace!(new_files);
            }
            sources[current_file].ast = ast;
        }
    }

    fn parse_parallel<'ast>(
        &self,
        sources: &mut Sources<'ast>,
        arenas: &'ast ThreadLocal<ast::Arena>,
    ) {
        let mut start = 0;
        loop {
            let base = start;
            let to_parse = &mut sources.raw[start..];
            if to_parse.is_empty() {
                break;
            }
            debug!(start, "parsing {} file{}", to_parse.len(), pluralize!(to_parse.len()));
            start += to_parse.len();
            let imports = to_parse
                .par_iter_mut()
                .enumerate()
                .filter(|(_, source)| source.ast.is_none())
                .flat_map_iter(|(i, source)| {
                    source.ast = self.parse_one(&source.file, arenas.get_or_default());
                    self.resolve_imports(&source.file, source.ast.as_ref())
                        .map(move |import| (i, import))
                })
                .collect_vec_list();
            let n_sources = sources.len();
            for (i, (import_item_id, import)) in imports.into_iter().flatten() {
                sources.add_import(SourceId::from_usize(base + i), import_item_id, import);
            }
            let new_files = sources.len() - n_sources;
            if new_files > 0 {
                trace!(new_files);
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
    fn resolve_imports<'a, 'b, 'c>(
        &'a self,
        file: &SourceFile,
        ast: Option<&'b ast::SourceUnit<'c>>,
    ) -> impl Iterator<Item = (ast::ItemId, Arc<SourceFile>)> + use<'a, 'b, 'c, 'gcx> {
        let parent = match &file.name {
            FileName::Real(path) => Some(path.to_path_buf()),
            FileName::Stdin | FileName::Custom(_) => None,
        };
        let items =
            ast.filter(|_| self.resolve_imports).map(|ast| &ast.items[..]).unwrap_or_default();
        items
            .iter_enumerated()
            .filter_map(|(id, item)| {
                if let ast::ItemKind::Import(import) = &item.kind {
                    Some((id, import))
                } else {
                    None
                }
            })
            .filter_map(move |(id, import)| {
                let span = import.path.span;
                let path_str = import.path.value.as_str();
                let (path_bytes, any_error) = unescape::parse_string_literal(
                    path_str,
                    unescape::StrKind::Str,
                    span,
                    self.sess,
                );
                if any_error {
                    return None;
                }
                let Some(path) = path_from_bytes(&path_bytes[..]) else {
                    self.dcx().err("import path is not a valid UTF-8 string").span(span).emit();
                    return None;
                };
                self.file_resolver
                    .resolve_file(path, parent.as_deref())
                    .map_err(|e| self.dcx().err(e.to_string()).span(span).emit())
                    .ok()
                    .map(|file| (id, file))
            })
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

impl Sources<'_> {
    /// Creates a new empty list of parsed sources.
    pub fn new() -> Self {
        Self { sources: IndexVec::new() }
    }

    fn add_import(
        &mut self,
        current: SourceId,
        import_item_id: ast::ItemId,
        import: Arc<SourceFile>,
    ) {
        let import_id = self.add_file(import);
        self.sources[current].imports.push((import_item_id, import_id));
    }

    #[instrument(level = "debug", skip_all)]
    fn add_file(&mut self, file: Arc<SourceFile>) -> SourceId {
        if let Some((id, _)) =
            self.sources.iter_enumerated().find(|(_, source)| Arc::ptr_eq(&source.file, &file))
        {
            return id;
        }
        self.sources.push(Source::new(file))
    }

    /// Asserts that all sources are unique.
    fn assert_unique(&self) {
        if self.sources.len() <= 1 {
            return;
        }

        debug_assert_eq!(
            self.sources.iter().map(|s| s.file.stable_id).collect::<FxHashSet<_>>().len(),
            self.sources.len(),
            "parsing produced duplicate source files"
        );
    }
}

impl<'ast> Sources<'ast> {
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

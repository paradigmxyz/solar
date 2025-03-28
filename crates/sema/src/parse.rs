use crate::{
    hir::{Arena, SourceId},
    GcxWrapper,
};
use rayon::prelude::*;
use solar_ast as ast;
use solar_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashSet,
};
use solar_interface::{
    diagnostics::DiagCtxt,
    source_map::{FileName, FileResolver, SourceFile},
    Result, Session,
};
use solar_parse::{unescape, Lexer, Parser};
use std::{borrow::Cow, fmt, path::Path, sync::Arc};
use thread_local::ThreadLocal;

pub struct ParsingContext<'sess> {
    /// The compiler session.
    pub sess: &'sess Session,
    /// The file resolver.
    pub file_resolver: FileResolver<'sess>,
    /// The loaded sources. Consumed once `parse` is called.
    /// The `'static` lifetime is a lie, as nothing borrowed is ever stored in this field.
    pub(crate) sources: ParsedSources<'static>,
}

impl<'sess> ParsingContext<'sess> {
    /// Creates a new parser context.
    pub fn new(sess: &'sess Session) -> Self {
        Self {
            sess,
            file_resolver: FileResolver::new(sess.source_map()),
            sources: ParsedSources::new(),
        }
    }

    /// Returns the diagnostics context.
    #[inline]
    pub fn dcx(&self) -> &'sess DiagCtxt {
        &self.sess.dcx
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

    /// Parses and semantically analyzes all the loaded sources, recursing into imports.
    pub fn parse_and_resolve(self) -> Result<()> {
        crate::parse_and_resolve(self)
    }

    /// Parses and lowers the entire program to HIR.
    /// Returns the global context if successful and if lowering was requested (default).
    pub fn parse_and_lower<'hir>(
        self,
        hir_arena: &'hir ThreadLocal<Arena>,
    ) -> Result<Option<GcxWrapper<'hir>>>
    where
        'sess: 'hir,
    {
        crate::parse_and_lower(self, hir_arena)
    }

    /// Parses all the loaded sources, recursing into imports.
    ///
    /// Sources are not guaranteed to be in any particular order, as they may be parsed in parallel.
    #[instrument(level = "debug", skip_all)]
    pub fn parse<'ast>(mut self, arenas: &'ast ThreadLocal<ast::Arena>) -> ParsedSources<'ast> {
        // SAFETY: The `'static` lifetime on `self.sources` is a lie since none of the asts are
        // populated, so this is safe.
        let sources: ParsedSources<'static> = std::mem::take(&mut self.sources);
        let mut sources: ParsedSources<'ast> =
            unsafe { std::mem::transmute::<ParsedSources<'static>, ParsedSources<'ast>>(sources) };
        if !sources.is_empty() {
            if self.sess.is_sequential() {
                self.parse_sequential(&mut sources, arenas.get_or_default());
            } else {
                self.parse_parallel(&mut sources, arenas);
            }
            debug!(
                num_sources = sources.len(),
                total_bytes = sources.iter().map(|s| s.file.src.len()).sum::<usize>(),
                total_lines = sources.iter().map(|s| s.file.count_lines()).sum::<usize>(),
                "parsed",
            );
        }
        sources.assert_unique();
        sources
    }

    fn parse_sequential<'ast>(&self, sources: &mut ParsedSources<'ast>, arena: &'ast ast::Arena) {
        for i in 0.. {
            let current_file = SourceId::from_usize(i);
            let Some(source) = sources.get(current_file) else { break };
            debug_assert!(source.ast.is_none(), "source already parsed");

            let ast = self.parse_one(&source.file, arena);
            let n_sources = sources.len();
            for (import_item_id, import) in resolve_imports!(self, &source.file, ast.as_ref()) {
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
        sources: &mut ParsedSources<'ast>,
        arenas: &'ast ThreadLocal<ast::Arena>,
    ) {
        let mut start = 0;
        loop {
            let base = start;
            let to_parse = &mut sources.raw[start..];
            if to_parse.is_empty() {
                break;
            }
            trace!(start, "parsing {} files", to_parse.len());
            start += to_parse.len();
            let imports = to_parse
                .par_iter_mut()
                .enumerate()
                .flat_map_iter(|(i, source)| {
                    debug_assert!(source.ast.is_none(), "source already parsed");
                    source.ast = self.parse_one(&source.file, arenas.get_or_default());
                    resolve_imports!(self, &source.file, source.ast.as_ref())
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
        let r = if self.sess.opts.language.is_yul() {
            let _file = parser.parse_yul_file_object().map_err(|e| e.emit());
            None
        } else {
            parser.parse_file().map_err(|e| e.emit()).ok()
        };
        trace!(allocated = arena.allocated_bytes(), used = arena.used_bytes(), "AST arena stats");
        r
    }
}

/// Resolves the imports of the given file, returning an iterator over all the imported files.
///
/// This is currently a macro as I have not figured out how to win against the borrow checker to
/// return `impl Iterator` instead of having to collect, since it obviously isn't necessary given
/// this macro.
macro_rules! resolve_imports {
    ($self:expr, $file:expr, $ast:expr) => {{
        let this = $self;
        let file = $file;
        let ast = $ast;
        let parent = match &file.name {
            FileName::Real(path) => Some(path.to_path_buf()),
            FileName::Stdin | FileName::Custom(_) => None,
        };
        let items = ast.map(|ast| &ast.items[..]).unwrap_or_default();
        items
            .iter_enumerated()
            .filter_map(|(id, item)| {
                if let ast::ItemKind::Import(import) = &item.kind {
                    Some((id, import, item.span))
                } else {
                    None
                }
            })
            .filter_map(move |(id, import, span)| {
                let path_bytes = escape_import_path(import.path.value.as_str())?;
                let Some(path) = path_from_bytes(&path_bytes[..]) else {
                    this.dcx().err("import path is not a valid UTF-8 string").span(span).emit();
                    return None;
                };
                this.file_resolver
                    .resolve_file(path, parent.as_deref())
                    .map_err(|e| this.dcx().err(e.to_string()).span(span).emit())
                    .ok()
                    .map(|file| (id, file))
            })
    }};
}
use resolve_imports;

fn escape_import_path(path_str: &str) -> Option<Cow<'_, [u8]>> {
    let mut any_error = false;
    let path_str =
        unescape::try_parse_string_literal(path_str, unescape::Mode::Str, |_, _| any_error = true);
    if any_error {
        return None;
    }
    Some(path_str)
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

/// Parsed sources, returned by [`ParsingContext::parse`].
#[derive(Default)]
pub struct ParsedSources<'ast> {
    /// The list of parsed sources.
    pub sources: IndexVec<SourceId, ParsedSource<'ast>>,
}

impl fmt::Debug for ParsedSources<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("ParsedSources ")?;
        self.sources.fmt(f)
    }
}

impl ParsedSources<'_> {
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
            trace!(file = %file.name.display(), "skipping duplicate source file");
            return id;
        }
        self.sources.push(ParsedSource::new(file))
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

impl<'ast> ParsedSources<'ast> {
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

impl<'ast> std::ops::Deref for ParsedSources<'ast> {
    type Target = IndexVec<SourceId, ParsedSource<'ast>>;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.sources
    }
}

impl std::ops::DerefMut for ParsedSources<'_> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sources
    }
}

/// A single parsed source.
pub struct ParsedSource<'ast> {
    /// The source file.
    pub file: Arc<SourceFile>,
    /// The AST IDs and source IDs of all the imports.
    pub imports: Vec<(ast::ItemId, SourceId)>,
    /// The AST. `None` if an error occurred during parsing, or if the source is a Yul file.
    pub ast: Option<ast::SourceUnit<'ast>>,
}

impl fmt::Debug for ParsedSource<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut dbg = f.debug_struct("ParsedSource");
        dbg.field("file", &self.file.name).field("imports", &self.imports);
        if let Some(ast) = &self.ast {
            dbg.field("ast", &ast);
        }
        dbg.finish()
    }
}

impl ParsedSource<'_> {
    /// Creates a new empty source.
    pub fn new(file: Arc<SourceFile>) -> Self {
        Self { file, ast: None, imports: Vec::new() }
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

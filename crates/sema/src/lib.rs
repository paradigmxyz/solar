//! Semantic analysis.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use rayon::prelude::*;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};
use sulk_ast::ast;
use sulk_data_structures::{
    index::{Idx, IndexVec},
    map::FxHashSet,
};
use sulk_interface::{
    diagnostics::DiagCtxt,
    source_map::{FileName, FileResolver, SourceFile},
    Result, Session,
};
use sulk_parse::{Lexer, Parser};

mod ast_passes;

pub mod hir;
use hir::{Source, SourceId};

mod ast_lowering;

mod staging;
pub use staging::SymbolCollector;

#[derive(Default)]
pub(crate) struct Sources {
    pub(crate) sources: IndexVec<SourceId, Source<'static>>,
}

#[allow(dead_code)]
impl Sources {
    fn new() -> Self {
        Self { sources: IndexVec::new() }
    }

    fn asts(&self) -> impl DoubleEndedIterator<Item = &ast::SourceUnit> {
        self.sources.iter().filter_map(|source| source.ast.as_ref())
    }

    fn par_asts(&self) -> impl ParallelIterator<Item = &ast::SourceUnit> {
        self.sources.as_raw_slice().par_iter().filter_map(|source| source.ast.as_ref())
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
        self.sources.push(Source::new(file))
    }

    #[cfg(debug_assertions)]
    fn debug_assert_unique(&self) {
        assert_eq!(
            self.sources.iter().map(|s| s.file.stable_id).collect::<FxHashSet<_>>().len(),
            self.sources.len(),
            "parsing produced duplicate source files"
        );
    }

    #[instrument(level = "debug", skip_all)]
    fn topo_sort(&mut self) {
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

        // Re-map imports.
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

impl std::ops::Deref for Sources {
    type Target = IndexVec<SourceId, Source<'static>>;

    fn deref(&self) -> &Self::Target {
        &self.sources
    }
}

impl std::ops::DerefMut for Sources {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.sources
    }
}

/// Semantic analysis context.
pub struct Sema<'a> {
    /// The file resolver.
    pub file_resolver: FileResolver<'a>,
    /// The session.
    pub sess: &'a Session,
    sources: Sources,
}

impl<'a> Sema<'a> {
    /// Creates a new context.
    pub fn new(sess: &'a Session) -> Self {
        Self { file_resolver: FileResolver::new(sess.source_map()), sess, sources: Sources::new() }
    }

    /// Returns the diagnostic context.
    pub fn dcx(&self) -> &'a DiagCtxt {
        &self.sess.dcx
    }

    /// Loads `stdin` into the resolver.
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
        // Paths must be canonicalized before passing to the resolver.
        let path = match path.canonicalize() {
            Ok(path) => {
                // Base paths from arguments to the current directory for shorter diagnostics
                // output.
                match path.strip_prefix(std::env::current_dir().unwrap_or(PathBuf::from(""))) {
                    Ok(path) => path.to_path_buf(),
                    Err(_) => path,
                }
            }
            Err(_) => path.to_path_buf(),
        };
        let file = self
            .file_resolver
            .resolve_file(&path, None)
            .map_err(|e| self.dcx().err(e.to_string()).emit())?;
        self.add_file(file);
        Ok(())
    }

    /// Adds a pre-loaded file to the resolver.
    pub fn add_file(&mut self, file: Arc<SourceFile>) {
        self.sources.add_file(file);
    }

    /// Parses and semantically analyzes all the loaded sources, recursing into imports.
    pub fn parse_and_resolve(&mut self) -> Result<()> {
        self.ensure_sources()?;

        self.parse();
        #[cfg(debug_assertions)]
        self.sources.debug_assert_unique();
        if self.sess.language.is_yul() || self.sess.stop_after.is_some_and(|s| s.is_parsing()) {
            return Ok(());
        }

        debug_span!("all_ast_passes").in_scope(|| {
            self.sources.par_asts().for_each(|ast| {
                ast_passes::run(self.sess, ast);
            });
        });

        self.dcx().has_errors()?;

        self.sources.topo_sort();

        let arena = bumpalo::Bump::new();
        let hir = ast_lowering::lower(self.sess, std::mem::take(&mut self.sources), &arena);

        self.dcx().has_errors()?;

        drop(hir);

        Ok(())
    }

    fn ensure_sources(&mut self) -> Result<()> {
        if self.sources.is_empty() {
            let msg = "no files found";
            let note = "if you wish to use the standard input, please specify `-` explicitly";
            return Err(self.dcx().err(msg).note(note).emit());
        }
        Ok(())
    }

    /// Parses all the loaded sources, recursing into imports.
    #[instrument(level = "debug", skip_all)]
    fn parse(&mut self) {
        let mut sources = std::mem::take(&mut self.sources);
        if self.sess.is_sequential() {
            self.parse_sequential(&mut sources);
        } else {
            self.parse_parallel(&mut sources);
        }
        debug!("parsed {} files", sources.len());
        self.sources = sources;
    }

    fn parse_sequential(&self, sources: &mut Sources) {
        for i in 0.. {
            let current_file = SourceId::from_usize(i);
            let Some(source) = sources.get(current_file) else { break };
            debug_assert!(source.ast.is_none(), "source already parsed");

            let ast = self.parse_one(&source.file);
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

    fn parse_parallel(&self, sources: &mut Sources) {
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
                    source.ast = self.parse_one(&source.file);
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
    fn parse_one(&self, file: &SourceFile) -> Option<ast::SourceUnit> {
        let lexer = Lexer::from_source_file(self.sess, file);
        let mut parser = Parser::from_lexer(lexer);
        if self.sess.language.is_yul() {
            let _file = parser.parse_yul_file_object().map_err(|e| e.emit());
            None
        } else {
            parser.parse_file().map_err(|e| e.emit()).ok()
        }
    }

    /// Resolves the imports of the given file, returning an iterator over all the imported files.
    fn resolve_imports<'b>(
        &'b self,
        file: &SourceFile,
        ast: Option<&'b ast::SourceUnit>,
    ) -> impl Iterator<Item = (ast::ItemId, Arc<SourceFile>)> + 'b {
        let parent = match &file.name {
            FileName::Real(path) => Some(path.clone()),
            // Use current directory for stdin.
            FileName::Stdin => Some(PathBuf::from("")),
            FileName::Custom(_) => None,
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
                // TODO: Unescape
                let path_str = import.path.value.as_str();
                let path = Path::new(path_str);
                self.file_resolver
                    .resolve_file(path, parent.as_deref())
                    .map_err(|e| self.dcx().err(e.to_string()).span(span).emit())
                    .ok()
                    .map(|file| (id, file))
            })
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

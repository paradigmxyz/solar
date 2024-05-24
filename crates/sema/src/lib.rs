//! Semantic analysis.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use rayon::iter::{IntoParallelRefIterator, ParallelIterator};
use std::path::{Path, PathBuf};
use sulk_ast::ast;
use sulk_data_structures::{index::IndexVec, newtype_index, sync::Lrc};
use sulk_interface::{
    debug_time,
    diagnostics::DiagCtxt,
    source_map::{FileName, FileResolver, ResolveError, SourceFile},
    trace_time, Result, Session,
};
use sulk_parse::{Lexer, Parser};

// pub mod hir;

mod ast_validation;
pub use ast_validation::AstValidator;

newtype_index! {
    /// A source index.
    pub(crate) struct SourceId
}

struct Sources(IndexVec<SourceId, Source>);

impl Sources {
    fn new() -> Self {
        Self(IndexVec::new())
    }

    fn get(&self, id: SourceId) -> Option<&Source> {
        self.0.get(id)
    }

    fn push_import(&mut self, current: SourceId, import: SourceId) {
        self.0[current].imports.push(import);
    }

    fn add_file(&mut self, file: Lrc<SourceFile>) -> SourceId {
        if let Some((id, _)) =
            self.0.iter_enumerated().find(|(_, source)| Lrc::ptr_eq(&source.file, &file))
        {
            trace!(file = %file.name.display(), "skipping duplicate source file");
            return id;
        }
        self.0.push(Source { file, ast: None, imports: IndexVec::new() })
    }

    #[allow(dead_code)]
    fn asts(&self) -> impl DoubleEndedIterator<Item = &ast::SourceUnit> {
        self.0.iter().filter_map(|source| source.ast.as_ref())
    }

    #[allow(dead_code)]
    fn par_asts(&self) -> impl ParallelIterator<Item = &ast::SourceUnit> {
        self.0.as_raw_slice().par_iter().filter_map(|source| source.ast.as_ref())
    }
}

struct Source {
    file: Lrc<SourceFile>,
    /// The AST of the source. None if Yul or parsing failed.
    ast: Option<ast::SourceUnit>,
    imports: IndexVec<SourceId, SourceId>,
}

/// Semantic analysis context.
pub struct Resolver<'a> {
    /// The file resolver.
    pub file_resolver: FileResolver<'a>,
    /// The session.
    pub sess: &'a Session,
    sources: Sources,
}

impl<'a> Resolver<'a> {
    /// Creates a new resolver.
    pub fn new(sess: &'a Session) -> Self {
        Self { file_resolver: FileResolver::new(sess.source_map()), sess, sources: Sources::new() }
    }

    /// Returns the diagnostic context.
    pub fn dcx(&self) -> &'a DiagCtxt {
        &self.sess.dcx
    }

    pub fn add_files_from_args(
        &mut self,
        stdin: bool,
        paths: impl IntoIterator<Item = impl AsRef<Path>>,
    ) -> Result<()> {
        let dcx = self.dcx();
        let emit_resolve_error = |e: ResolveError| dcx.err(e.to_string()).emit();

        if stdin {
            let file = self.file_resolver.load_stdin().map_err(emit_resolve_error)?;
            self.sources.add_file(file);
        }
        for path in paths {
            let path = path.as_ref();
            // Base paths from arguments to the current directory for shorter diagnostics output.
            let path = match path.canonicalize() {
                Ok(path) => {
                    match path.strip_prefix(std::env::current_dir().unwrap_or(PathBuf::from(""))) {
                        Ok(path) => path.to_path_buf(),
                        Err(_) => path,
                    }
                }
                Err(_) => path.to_path_buf(),
            };
            let file = self.file_resolver.resolve_file(&path, None).map_err(emit_resolve_error)?;
            self.sources.add_file(file);
        }

        if self.sources.0.is_empty() {
            let msg = "no files found";
            let note = "if you wish to use the standard input, please specify `-` explicitly";
            return Err(dcx.err(msg).note(note).emit());
        }

        Ok(())
    }

    pub fn parse_and_resolve(&mut self) -> Result<()> {
        debug_time!("parse all files", || self.parse_all_files());

        if self.sess.stop_after.is_some_and(|s| s.is_parsing()) {
            return Ok(());
        }

        debug_time!("validate ASTs", || self.validate_asts());

        Ok(())
    }

    fn parse_all_files(&mut self) {
        for i in 0.. {
            let current_file = SourceId::new(i);
            let Some(source) = self.sources.get(current_file) else { break };
            let Source { file, ast: source_ast, imports: _ } = source;
            debug_assert!(source_ast.is_none(), "already parsed a file");

            let _guard = debug_span!("parse", file = %file.name.display()).entered();

            let lexer = Lexer::from_source_file(self.sess, file);
            let tokens = trace_time!("lex file", || lexer.into_tokens());
            let mut parser = Parser::new(self.sess, tokens);
            let ast = trace_time!("parse file", || {
                if self.sess.language.is_yul() {
                    // TODO
                    let _file = parser.parse_yul_file_object().map_err(|e| e.emit());
                    None
                } else {
                    parser.parse_file().map_err(|e| e.emit()).ok()
                }
            });

            if let Some(ast) = &ast {
                let parent = match &file.name {
                    FileName::Real(path) => Some(path.clone()),
                    // Use current directory for stdin.
                    FileName::Stdin => Some(PathBuf::from("")),
                    FileName::Custom(_) => None,
                };
                for item in &ast.items {
                    if let ast::ItemKind::Import(import) = &item.kind {
                        // TODO: Unescape
                        let path_str = import.path.value.as_str();
                        let path = Path::new(path_str);
                        if let Ok(file) = self
                            .file_resolver
                            .resolve_file(path, parent.as_deref())
                            .map_err(|e| self.dcx().err(e.to_string()).span(item.span).emit())
                        {
                            let import = self.sources.add_file(file);
                            self.sources.push_import(current_file, import);
                        }
                    }
                }
            }

            self.sources.0[current_file].ast = ast;
        }

        debug!("parsed {} files", self.sources.0.len());
    }

    fn validate_asts(&self) {
        self.sources.par_asts().for_each(|ast| AstValidator::validate(self.sess, ast));
    }
}

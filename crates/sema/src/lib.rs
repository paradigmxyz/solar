//! Semantic analysis.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use std::path::{Path, PathBuf};
use sulk_ast::ast;
use sulk_data_structures::sync::Lrc;
use sulk_interface::{
    debug_time,
    diagnostics::DiagCtxt,
    source_map::{FileName, FileResolver, ResolveError, SourceFile},
    sym, trace_time, Result, Session, Span,
};
use sulk_parse::{Lexer, Parser};

struct Sources(Vec<Source>);

impl Sources {
    fn new() -> Self {
        Self(Vec::new())
    }

    fn add_file(&mut self, file: Lrc<SourceFile>) {
        if self.0.iter().any(|source| Lrc::ptr_eq(&source.file, &file)) {
            trace!(file = %file.name.display(), "skipping duplicate source file");
            return;
        }
        self.0.push(Source { file, ast: None });
    }

    #[allow(dead_code)]
    fn asts(&self) -> impl DoubleEndedIterator<Item = &ast::SourceUnit> {
        self.0.iter().filter_map(|source| source.ast.as_ref())
    }
}

struct Source {
    file: Lrc<SourceFile>,
    ast: Option<ast::SourceUnit>,
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

        for ast in self.sources.asts() {
            for item in &ast.items {
                if let ast::ItemKind::Pragma(pragma) = &item.kind {
                    self.check_pragma(item.span, pragma);
                }
            }
        }

        Ok(())
    }

    fn parse_all_files(&mut self) {
        for i in 0.. {
            let Some(Source { file, ast: source_ast }) = self.sources.0.get(i) else { break };
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
                            self.sources.add_file(file);
                        }
                    }
                }
            }

            self.sources.0[i].ast = ast;
        }

        debug!("parsed {} files", self.sources.0.len());
    }

    fn check_pragma(&self, span: Span, pragma: &ast::PragmaDirective) {
        match &pragma.tokens {
            ast::PragmaTokens::Version(name, _version) => {
                if name.name != sym::solidity {
                    let msg = "only `solidity` is supported as a version pragma";
                    self.dcx().err(msg).span(name.span).emit();
                    // return;
                }
                // TODO: Check version
            }
            ast::PragmaTokens::Custom(name, value) => {
                let name = name.as_str();
                let value = value.as_ref().map(ast::IdentOrStrLit::as_str);
                match (name, value) {
                    ("abicoder", Some("v1" | "v2")) => {}
                    ("experimental", Some("ABIEncoderV2")) => {}
                    ("experimental", Some("SMTChecker")) => {}
                    ("experimental", Some("solidity")) => {
                        let msg = "experimental solidity features are not supported";
                        self.dcx().err(msg).span(span).emit();
                    }
                    _ => {
                        self.dcx().err("unknown pragma").span(span).emit();
                    }
                }
            }
            ast::PragmaTokens::Verbatim(_) => {
                self.dcx().err("unknown pragma").span(span).emit();
            }
        }
    }
}

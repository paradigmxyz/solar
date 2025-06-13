#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use rayon::prelude::*;
use solar_data_structures::{trustme, OnDrop};
use solar_interface::{config::CompilerStage, Result, Session};
use thread_local::ThreadLocal;
use ty::Gcx;

// Convenience re-exports.
pub use ::thread_local;
pub use bumpalo;
pub use solar_ast as ast;
pub use solar_interface as interface;

mod ast_lowering;
mod ast_passes;

mod parse;
pub use parse::{ParsedSource, ParsedSources, ParsingContext};

pub mod builtins;
pub mod eval;

pub mod hir;
pub use hir::Hir;

pub mod ty;

mod typeck;

mod emit;

pub mod stats;

/// Thin wrapper around the global context to ensure it is accessed and dropped correctly.
pub struct GcxWrapper<'gcx>(std::mem::ManuallyDrop<ty::GlobalCtxt<'gcx>>);

impl<'gcx> GcxWrapper<'gcx> {
    fn new(gcx: ty::GlobalCtxt<'gcx>) -> Self {
        Self(std::mem::ManuallyDrop::new(gcx))
    }

    /// Get a reference to the global context.
    pub fn get(&self) -> Gcx<'gcx> {
        Gcx::new(unsafe { trustme::decouple_lt(&self.0) })
    }
}

impl Drop for GcxWrapper<'_> {
    fn drop(&mut self) {
        debug_span!("drop_gcx").in_scope(|| unsafe { std::mem::ManuallyDrop::drop(&mut self.0) });
    }
}

/// Parses and semantically analyzes all the loaded sources, recursing into imports.
pub(crate) fn parse_and_resolve(pcx: ParsingContext<'_>) -> Result<()> {
    let hir_arena = OnDrop::new(ThreadLocal::<hir::Arena>::new(), |hir_arena| {
        let _guard = debug_span!("dropping_hir_arena").entered();
        debug!(hir_allocated = %fmt_bytes(hir_arena.get_or_default().allocated_bytes()));
        drop(hir_arena);
    });
    if let Some(gcx) = parse_and_lower(pcx, &hir_arena)? {
        analysis(gcx.get())?;
    }
    Ok(())
}

/// Parses and lowers the entire program to HIR.
/// Returns the global context if successful and if lowering was requested (default).
pub(crate) fn parse_and_lower<'hir, 'sess: 'hir>(
    pcx: ParsingContext<'sess>,
    hir_arena: &'hir ThreadLocal<hir::Arena>,
) -> Result<Option<GcxWrapper<'hir>>> {
    let sess = pcx.sess;

    if pcx.sources.is_empty() {
        let msg = "no files found";
        let note = "if you wish to use the standard input, please specify `-` explicitly";
        return Err(sess.dcx.err(msg).note(note).emit());
    }

    let ast_arenas = OnDrop::new(ThreadLocal::<ast::Arena>::new(), |mut arenas| {
        let _guard = debug_span!("dropping_ast_arenas").entered();
        debug!(asts_allocated = %fmt_bytes(arenas.iter_mut().map(|a| a.allocated_bytes()).sum::<usize>()));
        drop(arenas);
    });
    let mut sources = pcx.parse(&ast_arenas);

    if let Some(dump) = &sess.opts.unstable.dump {
        if dump.kind.is_ast() {
            dump_ast(sess, &sources, dump.paths.as_deref())?;
        }
    }

    if sess.opts.unstable.ast_stats {
        for source in sources.asts() {
            stats::print_ast_stats(source, "AST STATS", "ast-stats");
        }
    }

    if sess.opts.unstable.span_visitor {
        use ast::span_visitor::SpanVisitor;
        use ast::visit::Visit;
        for source in sources.asts() {
            let mut visitor = SpanVisitor::new(sess);
            let _ = visitor.visit_source_unit(source);
            debug!(spans_visited = visitor.count(), "span visitor completed");
        }
    }

    if sess.opts.language.is_yul() || sess.stop_after(CompilerStage::Parsed) {
        return Ok(None);
    }

    sources.topo_sort();

    let (hir, symbol_resolver) = lower(sess, &sources, hir_arena.get_or_default())?;

    // Drop the ASTs and AST arenas in a separate thread.
    sess.spawn({
        // TODO: The transmute is required because `sources` borrows from `ast_arenas`,
        // even though both are moved in the closure.
        let sources =
            unsafe { std::mem::transmute::<ParsedSources<'_>, ParsedSources<'static>>(sources) };
        move || {
            debug_span!("drop_asts").in_scope(|| drop(sources));
            drop(ast_arenas);
        }
    });

    Ok(Some(GcxWrapper::new(ty::GlobalCtxt::new(sess, hir_arena, hir, symbol_resolver))))
}

/// Lowers the parsed ASTs into the HIR.
fn lower<'sess, 'hir>(
    sess: &'sess Session,
    sources: &ParsedSources<'_>,
    arena: &'hir hir::Arena,
) -> Result<(hir::Hir<'hir>, ast_lowering::SymbolResolver<'sess>)> {
    debug_span!("all_ast_passes").in_scope(|| {
        sources.par_asts().for_each(|ast| {
            ast_passes::run(sess, ast);
        });
    });

    sess.dcx.has_errors()?;

    Ok(ast_lowering::lower(sess, sources, arena))
}

/// Performs the analysis phase.
///
/// This is not yet exposed publicly as it is not yet fully implemented.
#[instrument(level = "debug", skip_all)]
fn analysis(gcx: Gcx<'_>) -> Result<()> {
    if let Some(dump) = &gcx.sess.opts.unstable.dump {
        if dump.kind.is_hir() {
            dump_hir(gcx, dump.paths.as_deref())?;
        }
    }

    // Lower HIR types.
    gcx.hir.par_item_ids().for_each(|id| {
        let _ = gcx.type_of_item(id);
        match id {
            hir::ItemId::Struct(id) => _ = gcx.struct_field_types(id),
            hir::ItemId::Contract(id) => _ = gcx.interface_functions(id),
            _ => {}
        }
    });
    gcx.sess.dcx.has_errors()?;

    typeck::check(gcx);
    gcx.sess.dcx.has_errors()?;

    if !gcx.sess.opts.emit.is_empty() {
        emit::emit(gcx);
        gcx.sess.dcx.has_errors()?;
    }

    Ok(())
}

fn dump_ast(sess: &Session, sources: &ParsedSources<'_>, paths: Option<&[String]>) -> Result<()> {
    if let Some(paths) = paths {
        for path in paths {
            if let Some(source) = sources.iter().find(|&s| match_file_name(&s.file.name, path)) {
                println!("{source:#?}");
            } else {
                let msg = format!("`-Zdump=ast={path:?}` did not match any source file");
                let note = format!(
                    "available source files: {}",
                    sources
                        .iter()
                        .map(|s| s.file.name.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                return Err(sess.dcx.err(msg).note(note).emit());
            }
        }
    } else {
        println!("{sources:#?}");
    }

    Ok(())
}

fn dump_hir(gcx: Gcx<'_>, paths: Option<&[String]>) -> Result<()> {
    println!("{:#?}", gcx.hir);
    if let Some(paths) = paths {
        println!("\nPaths not yet implemented: {paths:#?}");
    }
    Ok(())
}

fn match_file_name(name: &solar_interface::source_map::FileName, path: &str) -> bool {
    match name {
        solar_interface::source_map::FileName::Real(path_buf) => {
            path_buf.as_os_str() == path || path_buf.file_stem() == Some(path.as_ref())
        }
        solar_interface::source_map::FileName::Stdin => path == "stdin" || path == "<stdin>",
        solar_interface::source_map::FileName::Custom(name) => path == name,
    }
}

fn fmt_bytes(bytes: usize) -> impl std::fmt::Display {
    solar_data_structures::fmt::from_fn(move |f| {
        let mut size = bytes as f64;
        let mut suffix = "B";
        if size >= 1024.0 {
            size /= 1024.0;
            suffix = "KiB";
        }
        if size >= 1024.0 {
            size /= 1024.0;
            suffix = "MiB";
        }
        if size >= 1024.0 {
            size /= 1024.0;
            suffix = "GiB";
        }

        let precision = if size.fract() != 0.0 { 2 } else { 0 };
        write!(f, "{size:.precision$} {suffix}")?;
        if suffix != "B" {
            write!(f, " ({bytes} B)")?;
        }
        Ok(())
    })
}

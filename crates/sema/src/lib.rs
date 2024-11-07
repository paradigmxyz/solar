#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.jpg",
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
pub use solar_ast::ast;
pub use solar_interface as interface;

mod ast_lowering;
mod ast_passes;

mod parse;
pub use parse::{ParsedSource, ParsedSources, ParsingContext};

pub mod builtins;
pub mod eval;
pub mod hir;
pub mod ty;

mod typeck;

mod emit;

/// Parses and semantically analyzes all the loaded sources, recursing into imports.
pub fn parse_and_resolve(pcx: ParsingContext<'_>) -> Result<()> {
    let sess = pcx.sess;

    if pcx.sources.is_empty() {
        let msg = "no files found";
        let note = "if you wish to use the standard input, please specify `-` explicitly";
        return Err(sess.dcx.err(msg).note(note).emit());
    }

    let ast_arenas = OnDrop::new(ThreadLocal::<ast::Arena>::new(), |mut arenas| {
        debug!(asts_allocated = arenas.iter_mut().map(|a| a.allocated_bytes()).sum::<usize>());
        debug_span!("dropping_ast_arenas").in_scope(|| drop(arenas));
    });
    let mut sources = pcx.parse(&ast_arenas);

    if let Some(dump) = &sess.dump {
        if dump.kind.is_ast() {
            dump_ast(sess, &sources, dump.paths.as_deref())?;
        }
    }

    if sess.language.is_yul() || sess.stop_after(CompilerStage::Parsed) {
        return Ok(());
    }

    sources.topo_sort();

    let hir_arena = OnDrop::new(ThreadLocal::<hir::Arena>::new(), |hir_arena| {
        debug!(hir_allocated = hir_arena.get_or_default().allocated_bytes());
        debug_span!("dropping_hir_arena").in_scope(|| drop(hir_arena));
    });
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

    let global_context =
        OnDrop::new(ty::GlobalCtxt::new(sess, &hir_arena, hir, symbol_resolver), |gcx| {
            debug_span!("drop_gcx").in_scope(|| drop(gcx));
        });
    let gcx = ty::Gcx::new(unsafe { trustme::decouple_lt(&global_context) });
    analysis(gcx)?;

    Ok(())
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

#[instrument(level = "debug", skip_all)]
fn analysis(gcx: Gcx<'_>) -> Result<()> {
    if let Some(dump) = &gcx.sess.dump {
        if dump.kind.is_hir() {
            dump_hir(gcx, dump.paths.as_deref())?;
        }
    }

    // Collect the types first to check and fail on recursive types.
    gcx.hir.par_item_ids().for_each(|id| {
        let _ = gcx.type_of_item(id);
        if let hir::ItemId::Struct(id) = id {
            let _ = gcx.struct_field_types(id);
        }
    });
    gcx.sess.dcx.has_errors()?;

    gcx.hir.par_contract_ids().for_each(|id| {
        let _ = gcx.interface_functions(id);
    });
    gcx.sess.dcx.has_errors()?;

    typeck::check(gcx);
    gcx.sess.dcx.has_errors()?;

    if !gcx.sess.emit.is_empty() {
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

//! Semantic analysis.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use rayon::prelude::*;
use solar_data_structures::OnDrop;
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
pub mod hir;
pub mod ty;

mod typeck;

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
    let hir = resolve(sess, &sources, hir_arena.get_or_default())?;

    // TODO: The transmute is required because `sources` borrows from `ast_arenas`,
    // even though both are moved in the closure.
    let sources =
        unsafe { std::mem::transmute::<ParsedSources<'_>, ParsedSources<'static>>(sources) };
    sess.spawn(move || {
        debug_span!("drop_asts").in_scope(|| drop(sources));
        drop(ast_arenas);
    });

    let global_context = ty::GlobalCtxt::new(sess, &hir_arena, hir);
    // TODO: Leaks `hir`
    let gcx = ty::Gcx::new(hir_arena.get_or_default().alloc(global_context));

    if let Some(dump) = &gcx.sess.dump {
        if dump.kind.is_hir() {
            dump_hir(gcx, dump.paths.as_deref())?;
        }
    }

    gcx.sess.dcx.has_errors()?;

    check_type_cycles(gcx);
    gcx.sess.dcx.has_errors()?;

    gcx.hir.par_item_ids().for_each(|id| {
        if let hir::ItemId::Contract(id) = id {
            let _ = gcx.interface_functions(id);
        }
        let _ = gcx.type_of_item(id);
        if let hir::ItemId::Struct(id) = id {
            let _ = gcx.struct_field_types(id);
        }
    });

    Ok(())
}

/// Semantically analyzes the given sources and returns the resulting HIR.
pub fn resolve<'hir>(
    sess: &Session,
    sources: &ParsedSources<'_>,
    arena: &'hir hir::Arena,
) -> Result<hir::Hir<'hir>> {
    debug_span!("all_ast_passes").in_scope(|| {
        sources.par_asts().for_each(|ast| {
            ast_passes::run(sess, ast);
        });
    });

    sess.dcx.has_errors()?;

    let hir = ast_lowering::lower(sess, sources, arena);

    Ok(hir)
}

fn check_type_cycles(gcx: Gcx<'_>) {
    let _ = gcx;
    // TODO
    // gcx.hir.par_structs().for_each(|s| {
    //     s.fields
    // });
}

fn dump_ast(sess: &Session, sources: &ParsedSources<'_>, paths: Option<&[String]>) -> Result<()> {
    if let Some(paths) = paths {
        for path in paths {
            if let Some(source) = sources.iter().find(|s| match_file_name(&s.file.name, path)) {
                println!("{source:#?}");
            } else {
                return Err(sess
                    .dcx
                    .err("`-Zdump=ast` paths must match exactly a single input file")
                    .emit());
            }
        }
    } else {
        println!("{sources:#?}");
    }

    Ok(())
}

fn dump_hir(gcx: Gcx<'_>, paths: Option<&[String]>) -> Result<()> {
    if let Some(paths) = paths {
        todo!("{paths:#?}")
    } else {
        println!("{:#?}", gcx.hir);
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

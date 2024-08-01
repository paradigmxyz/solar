//! Semantic analysis.

#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/sulk/main/assets/logo.jpg",
    html_favicon_url = "https://avatars0.githubusercontent.com/u/97369466?s=256"
)]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use rayon::prelude::*;
use sulk_data_structures::OnDrop;
use sulk_interface::{Result, Session};
use thread_local::ThreadLocal;

// Convenience re-exports.
pub use ::thread_local;
pub use bumpalo;
pub use sulk_ast::ast;
pub use sulk_interface as interface;

mod ast_lowering;

mod ast_passes;

mod parse;
pub use parse::{ParsedSource, ParsedSources, ParsingContext};

pub mod hir;

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

    if sess.language.is_yul() || sess.stop_after.is_some_and(|s| s.is_parsing()) {
        return Ok(());
    }

    sources.topo_sort();

    let hir_arena = OnDrop::new(hir::Arena::new(), |hir_arena| {
        debug!(hir_allocated = hir_arena.allocated_bytes());
        debug_span!("dropping_hir_arena").in_scope(|| drop(hir_arena));
    });
    let hir = resolve(sess, &sources, &hir_arena)?;

    // TODO: The transmute is required because `sources` borrows from `ast_arenas`,
    // even though both are moved in the closure.
    let sources =
        unsafe { std::mem::transmute::<ParsedSources<'_>, ParsedSources<'static>>(sources) };
    sess.spawn(move || {
        debug_span!("drop_asts").in_scope(|| drop(sources));
        drop(ast_arenas);
    });

    debug_span!("drop_hir").in_scope(|| drop(hir));

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

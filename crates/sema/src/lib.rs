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
use solar_interface::{Result, Session, config::CompilerStage};
use std::ops::ControlFlow;

// Convenience re-exports.
pub use ::thread_local;
pub use bumpalo;
pub use solar_ast as ast;
pub use solar_interface as interface;

mod ast_lowering;
mod ast_passes;

mod compiler;
pub use compiler::{Compiler, CompilerRef};

mod parse;
pub use parse::{ParsingContext, Source, Sources};

pub mod builtins;
pub mod eval;

pub mod hir;
pub use hir::Hir;

pub mod ty;
pub use ty::{Gcx, Ty};

mod typeck;

mod emit;

pub mod stats;

mod span_visitor;

pub(crate) fn lower(compiler: &mut CompilerRef<'_>) -> Result<ControlFlow<()>> {
    let gcx = compiler.gcx();
    let sess = gcx.sess;

    if gcx.sources.is_empty() {
        let msg = "no files found";
        let note = "if you wish to use the standard input, please specify `-` explicitly";
        return Err(sess.dcx.err(msg).note(note).emit());
    }

    if let Some(dump) = &sess.opts.unstable.dump
        && dump.kind.is_ast()
    {
        dump_ast(sess, &gcx.sources, dump.paths.as_deref())?;
    }

    if sess.opts.unstable.ast_stats {
        for source in gcx.sources.asts() {
            stats::print_ast_stats(source, "AST STATS", "ast-stats");
        }
    }

    if sess.opts.unstable.span_visitor {
        use crate::span_visitor::SpanVisitor;
        use ast::visit::Visit;
        for source in gcx.sources.asts() {
            let mut visitor = SpanVisitor::new(sess);
            let _ = visitor.visit_source_unit(source);
            debug!(spans_visited = visitor.count(), "span visitor completed");
        }
    }

    if sess.opts.language.is_yul() || gcx.advance_stage(CompilerStage::Lowering).is_break() {
        return Ok(ControlFlow::Break(()));
    }

    compiler.gcx_mut().sources.topo_sort();

    debug_span!("all_ast_passes").in_scope(|| {
        gcx.sources.par_asts().for_each(|ast| {
            ast_passes::run(gcx.sess, ast);
        });
    });

    gcx.sess.dcx.has_errors()?;

    ast_lowering::lower(compiler.gcx_mut());

    Ok(ControlFlow::Continue(()))
}

#[instrument(level = "debug", skip_all)]
fn analysis(gcx: Gcx<'_>) -> Result<ControlFlow<()>> {
    if let ControlFlow::Break(()) = gcx.advance_stage(CompilerStage::Analysis) {
        return Ok(ControlFlow::Break(()));
    }

    if let Some(dump) = &gcx.sess.opts.unstable.dump
        && dump.kind.is_hir()
    {
        dump_hir(gcx, dump.paths.as_deref())?;
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

    Ok(ControlFlow::Continue(()))
}

fn dump_ast(sess: &Session, sources: &Sources<'_>, paths: Option<&[String]>) -> Result<()> {
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

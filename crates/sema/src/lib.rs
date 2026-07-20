#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]

#[macro_use]
extern crate tracing;

use indexmap as _;
use rayon::prelude::*;
use solar_interface::{Result, Session, config::CompilerStage};
use std::ops::ControlFlow;

// Convenience re-exports.
#[doc(no_inline)]
pub use ::thread_local;
#[doc(no_inline)]
pub use bumpalo;
pub use solar_ast as ast;
pub use solar_interface as interface;

mod ast_lowering;
mod ast_passes;
mod natspec;

mod compiler;
pub use compiler::{Compiler, CompilerRef};

mod parse;
pub use parse::{ParsingContext, Source, Sources};

pub mod builtins;
pub mod eval;

pub mod output;

pub mod hir;
pub use hir::Hir;

pub mod ty;
pub use ty::{Gcx, NatSpecView, Ty};

mod typeck;

pub mod stats;

mod span_visitor;

pub(crate) fn lower(compiler: &mut CompilerRef<'_>) -> Result<ControlFlow<()>> {
    let gcx = compiler.gcx();
    let sess = gcx.sess;

    if gcx.sources.is_empty() {
        debug!("no files found");
        return Ok(ControlFlow::Break(()));
    }

    if let Some(dump) = &sess.opts.unstable.dump
        && dump.kind.is_ast()
    {
        dump_ast(sess, &gcx.sources, dump.paths.as_deref())?;
    }

    if sess.opts.unstable.ast_stats {
        for source in gcx.sources.asts() {
            stats::print_ast_stats(source, "AST STATS");
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

    if gcx.sess.opts.unstable.hir_stats {
        stats::print_hir_stats(&gcx.hir, "HIR STATS");
    }

    // Lower HIR types.
    gcx.hir.par_item_ids().for_each(|id| {
        let _ = gcx.type_of_item(id);
        match id {
            hir::ItemId::Struct(id) => {
                let _ = gcx.struct_recursiveness(id);
                let _ = gcx.struct_field_types(id);
            }
            hir::ItemId::Contract(id) => _ = gcx.interface_functions(id),
            _ => {}
        }
        natspec::validate_item_docs(gcx, id);
    });

    typeck::check(gcx);

    Ok(ControlFlow::Continue(()))
}

fn dump_ast(sess: &Session, sources: &Sources<'_>, paths: Option<&[String]>) -> Result<()> {
    if let Some(paths) = paths {
        for path in paths {
            let sm = sess.source_map();
            if let Some(file) = sm.get_file(sm.parse_file_name(path))
                && let Some((_, source)) = sources.get_file(&file)
            {
                println!("{source:#?}");
            } else {
                let msg = format!("`-Zdump=ast={path}` did not match any source file");
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
    if let Some(paths) = paths {
        let mut printer = hir::HirPrinter::new(gcx);
        for path in paths {
            if let Some((id, source)) = gcx.get_hir_source(path.clone()) {
                printer.print_source(id, source);
            } else {
                let msg = format!("`-Zdump=hir={path}` did not match any source file");
                let note = format!(
                    "available source files: {}",
                    gcx.hir
                        .sources()
                        .map(|s| s.file.name.display().to_string())
                        .collect::<Vec<_>>()
                        .join(", ")
                );
                return Err(gcx.sess.dcx.err(msg).note(note).emit());
            }
        }
        print!("{}", printer.finish());
    } else {
        print!("{}", hir::HirPrinter::new(gcx).print_all());
    }
    Ok(())
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

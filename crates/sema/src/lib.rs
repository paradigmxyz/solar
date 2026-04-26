#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg))]

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

    // Most AST validation is fused into lowering/resolution below so successful builds only walk
    // the tree once. If parsing already emitted an error, lowering may not be safe to enter; run
    // the standalone validator on that error path to preserve additional diagnostics before
    // returning.
    if gcx.sess.dcx.has_errors().is_err() {
        debug_span!("all_ast_passes").in_scope(|| {
            gcx.sources.par_asts().for_each(|ast| {
                ast_passes::run(gcx.sess, ast);
            });
        });
        gcx.sess.dcx.has_errors()?;
    }

    ast_lowering::lower(compiler.gcx_mut());

    Ok(ControlFlow::Continue(()))
}

fn analysis(gcx: Gcx<'_>) -> Result<ControlFlow<()>> {
    if let ControlFlow::Break(()) = gcx.advance_stage(CompilerStage::Analysis) {
        return Ok(ControlFlow::Break(()));
    }

    if let Some(dump) = &gcx.sess.opts.unstable.dump
        && dump.kind.is_hir()
    {
        dump_hir(gcx, dump.paths.as_deref())?;
    }

    let lower_hir_ty = |id| match id {
        hir::ItemId::Contract(id) => {
            if has_external_interface_functions(gcx, id) {
                let _ = gcx.interface_functions(id);
            }
        }
        hir::ItemId::Struct(id) => {
            let _ = gcx.type_of_item(id.into());
            let _ = gcx.struct_field_types(id);
        }
        hir::ItemId::Function(_)
        | hir::ItemId::Variable(_)
        | hir::ItemId::Udvt(_)
        | hir::ItemId::Error(_)
        | hir::ItemId::Event(_) => _ = gcx.type_of_item(id),
        hir::ItemId::Enum(_) => {}
    };

    // Force the type queries that emit standalone diagnostics. Simple namespace item types
    // (contracts/enums/struct wrappers) are left lazy.
    if gcx.sess.is_sequential() {
        for id in gcx.hir.contract_ids() {
            lower_hir_ty(id.into());
        }
        for id in gcx.hir.function_ids() {
            lower_hir_ty(id.into());
        }
        for id in gcx.hir.variable_ids() {
            lower_hir_ty(id.into());
        }
        for id in gcx.hir.strukt_ids() {
            lower_hir_ty(id.into());
        }
        for id in gcx.hir.udvt_ids() {
            lower_hir_ty(id.into());
        }
        for id in gcx.hir.error_ids() {
            lower_hir_ty(id.into());
        }
        for id in gcx.hir.event_ids() {
            lower_hir_ty(id.into());
        }
    } else {
        gcx.hir.par_item_ids().for_each(lower_hir_ty);
    }
    gcx.sess.dcx.has_errors()?;

    typeck::check(gcx);
    gcx.sess.dcx.has_errors()?;

    if !gcx.sess.opts.emit.is_empty() {
        emit::emit(gcx);
        gcx.sess.dcx.has_errors()?;
    }

    Ok(ControlFlow::Continue(()))
}

fn has_external_interface_functions(gcx: Gcx<'_>, contract_id: hir::ContractId) -> bool {
    gcx.hir.contract(contract_id).linearized_bases.iter().any(|&base| {
        gcx.hir
            .contract(base)
            .functions()
            .any(|f| gcx.hir.function(f).is_part_of_external_interface())
    })
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
    println!("{:#?}", gcx.hir);
    if let Some(paths) = paths {
        println!("\nPaths not yet implemented: {paths:#?}");
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

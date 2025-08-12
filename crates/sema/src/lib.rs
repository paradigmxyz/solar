#![doc = include_str!("../README.md")]
#![doc(
    html_logo_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/logo.png",
    html_favicon_url = "https://raw.githubusercontent.com/paradigmxyz/solar/main/assets/favicon.ico"
)]
#![cfg_attr(feature = "nightly", feature(rustc_attrs), allow(internal_features))]
#![cfg_attr(docsrs, feature(doc_cfg, doc_auto_cfg))]

#[macro_use]
extern crate tracing;

use crate::ty::{GcxMut, GlobalCtxt};
use rayon::prelude::*;
use solar_data_structures::trustme;
use solar_interface::{Result, Session, config::CompilerStage};
use std::{
    marker::PhantomPinned,
    mem::{ManuallyDrop, MaybeUninit},
    pin::Pin,
};
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
pub use parse::{ParsingContext, Source, Sources};

pub mod builtins;
pub mod eval;

pub mod hir;
pub use hir::Hir;

pub mod ty;

mod typeck;

mod emit;

pub mod stats;

mod span_visitor;

pub struct Compiler(ManuallyDrop<Pin<Box<CompilerInner<'static>>>>);

struct CompilerInner<'a> {
    sess: Session,
    gcx: GlobalCtxt<'a>,
    /// Lifetimes in this struct are self-referential.
    _pinned: PhantomPinned,
}

/// `$x->$y`
macro_rules! project_ptr {
    ($x:ident -> $y:ident : $xty:ty => $ty:ty) => {
        $x.byte_add(std::mem::offset_of!($xty, $y)).cast::<$ty>()
    };
}

impl Compiler {
    /// Creates a new compiler.
    #[expect(clippy::missing_transmute_annotations)]
    pub fn new(sess: Session) -> Self {
        let mut inner = Box::pin(MaybeUninit::<CompilerInner<'_>>::uninit());

        // SAFETY: Valid pointer, `init` initializes all fields.
        unsafe {
            let inner = Pin::get_unchecked_mut(Pin::as_mut(&mut inner));
            let inner = inner.as_mut_ptr();
            CompilerInner::init(inner, sess);
        }

        // SAFETY: `inner` has been initialized, `MaybeUninit<T>` is transmuted to `T`.
        Self(ManuallyDrop::new(unsafe { std::mem::transmute(inner) }))
    }

    /// Returns a reference to the compiler session.
    #[inline]
    pub fn sess(&self) -> &Session {
        &self.0.sess
    }

    /// Enters the compiler context.
    pub fn enter<T: Send>(&self, f: impl FnOnce(&CompilerRef<'_>) -> T + Send) -> T {
        self.0.sess.enter_parallel(|| f(CompilerRef::new(&self.0)))
    }

    /// Enters the compiler context with mutable access.
    ///
    /// This is only necessary on the first access to parse sources.
    pub fn enter_mut<T: Send>(&mut self, f: impl FnOnce(&mut CompilerRef<'_>) -> T + Send) -> T {
        // SAFETY: `sess` is not modified.
        let sess = unsafe { trustme::decouple_lt(&self.0.sess) };
        sess.enter_parallel(|| f(self.as_mut()))
    }

    fn as_mut(&mut self) -> &mut CompilerRef<'_> {
        // SAFETY: CompilerRef does not invalidate the Pin.
        let inner = unsafe { Pin::get_unchecked_mut(Pin::as_mut(&mut self.0)) };
        let inner = unsafe {
            std::mem::transmute::<&mut CompilerInner<'static>, &mut CompilerInner<'_>>(inner)
        };
        CompilerRef::new_mut(inner)
    }
}

impl CompilerInner<'_> {
    #[inline]
    #[allow(elided_lifetimes_in_paths)]
    unsafe fn init(this: *mut Self, sess: Session) {
        use CompilerInner as C;

        unsafe {
            let sess_p = project_ptr!(this->sess: C=>Session);
            sess_p.write(sess);

            let sess = &*sess_p;
            project_ptr!(this->gcx: C=>GlobalCtxt<'static>).write(GlobalCtxt::new(sess));
        }
    }
}

impl Drop for CompilerInner<'_> {
    fn drop(&mut self) {
        log_ast_arenas_stats(&mut self.gcx.ast_arenas);
        debug!(hir_allocated = %fmt_bytes(self.gcx.hir_arenas.iter_mut().map(|a| a.allocated_bytes()).sum::<usize>()));
    }
}

impl Drop for Compiler {
    fn drop(&mut self) {
        let _guard = debug_span!("Compiler::drop").entered();
        unsafe { std::mem::ManuallyDrop::drop(&mut self.0) };
    }
}

/// A reference to the compiler.
///
/// This is only available inside the [`Compiler::enter`] closure, and has access to the global
/// context.
#[repr(transparent)]
pub struct CompilerRef<'c> {
    pub(crate) inner: CompilerInner<'c>,
}

impl<'c> CompilerRef<'c> {
    #[inline]
    fn new<'a>(inner: &'a CompilerInner<'c>) -> &'a Self {
        // SAFETY: `repr(transparent)`
        unsafe { std::mem::transmute(inner) }
    }

    #[inline]
    fn new_mut<'a>(inner: &'a mut CompilerInner<'c>) -> &'a mut Self {
        // SAFETY: `repr(transparent)`
        unsafe { std::mem::transmute(inner) }
    }

    /// Returns a reference to the global context.
    #[inline]
    pub fn gcx(&self) -> Gcx<'c> {
        Gcx::new(unsafe { trustme::decouple_lt(&self.inner.gcx) })
    }

    #[inline]
    pub(crate) fn gcx_mut(&mut self) -> GcxMut<'c> {
        GcxMut::new(&mut self.inner.gcx)
    }

    /// Drops the ASTs and AST arenas in a separate thread.
    pub fn drop_asts(&mut self) {
        let sources = std::mem::take(&mut self.inner.gcx.sources);
        let sources = unsafe { std::mem::transmute::<Sources<'_>, Sources<'static>>(sources) };
        let mut ast_arenas = std::mem::take(&mut self.inner.gcx.ast_arenas);
        self.inner.gcx.sess.spawn(move || {
            let _guard = debug_span!("drop_asts").entered();
            log_ast_arenas_stats(&mut ast_arenas);
            drop(sources);
            drop(ast_arenas);
        });
    }

    pub fn parse(&mut self) -> ParsingContext<'c> {
        ParsingContext::new(self.gcx_mut())
    }

    pub fn lower_asts(&mut self) -> Result<()> {
        lower(self)
    }

    pub fn analysis(&self) -> Result<()> {
        analysis(self.gcx())
    }
}

fn log_ast_arenas_stats(arenas: &mut ThreadLocal<ast::Arena>) {
    if arenas.iter_mut().len() == 0 {
        return;
    }
    debug!(asts_allocated = %fmt_bytes(arenas.iter_mut().map(|a| a.allocated_bytes()).sum::<usize>()));
}

/// Parses and lowers the entire program to HIR.
/// Returns the global context if successful and if lowering was requested (default).
pub(crate) fn lower(compiler: &mut CompilerRef<'_>) -> Result<()> {
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

    if sess.opts.language.is_yul() || sess.stop_after(CompilerStage::Parsed) {
        return Ok(());
    }

    compiler.inner.gcx.sources.topo_sort();

    debug_span!("all_ast_passes").in_scope(|| {
        gcx.sources.par_asts().for_each(|ast| {
            ast_passes::run(gcx.sess, ast);
        });
    });

    gcx.sess.dcx.has_errors()?;

    ast_lowering::lower(compiler.gcx_mut());

    compiler.drop_asts();

    Ok(())
}

/// Performs the analysis phase.
///
/// This is not yet exposed publicly as it is not yet fully implemented.
#[instrument(level = "debug", skip_all)]
fn analysis(gcx: Gcx<'_>) -> Result<()> {
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

    Ok(())
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

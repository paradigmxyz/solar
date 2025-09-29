use crate::{
    ParsingContext, Sources, fmt_bytes,
    ty::{Gcx, GcxMut, GlobalCtxt},
};
use solar_data_structures::trustme;
use solar_interface::{Result, Session, diagnostics::DiagCtxt};
use std::{
    fmt,
    marker::PhantomPinned,
    mem::{ManuallyDrop, MaybeUninit},
    ops::ControlFlow,
    pin::Pin,
};
use thread_local::ThreadLocal;

/// The compiler.
///
/// This is the main entry point and driver for the compiler.
///
/// It must be [`enter`ed](Self::enter) to perform most operations, as it makes use of thread-local
/// storage, which is only available inside of a closure.
/// [`enter_mut`](Self::enter_mut) is only necessary when parsing sources and lowering the ASTs. All
/// accesses after can make use of `gcx`, passed by immutable reference.
///
/// Once a stage-advancing operation is performed, such as `parse`, `lower`, etc., the compiler may
/// not perform the same or a previous operation again, with the exception of `parse`.
///
/// # Examples
///
/// ```
/// # mod solar { pub use {solar_interface as interface, solar_sema as sema}; }
/// # fn main() {}
#[doc = include_str!("../doc-examples/hir.rs")]
/// ```
pub struct Compiler(ManuallyDrop<Pin<Box<CompilerInner<'static>>>>);

impl fmt::Debug for Compiler {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.enter_sequential(|compiler| compiler.debug_fmt("Compiler", f))
    }
}

struct CompilerInner<'a> {
    sess: Session,
    gcx: GlobalCtxt<'a>,
    /// Lifetimes in this struct are self-referential.
    _pinned: PhantomPinned,
}

/// `$x->$y` like in C.
macro_rules! project_ptr {
    ($x:ident -> $y:ident) => {
        &raw mut (*$x).$y
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

    /// Returns a mutable reference to the compiler session.
    #[inline]
    pub fn sess_mut(&mut self) -> &mut Session {
        self.as_mut().sess_mut()
    }

    /// Returns a reference to the diagnostics context.
    #[inline]
    pub fn dcx(&self) -> &DiagCtxt {
        &self.sess().dcx
    }

    /// Returns a mutable reference to the diagnostics context.
    #[inline]
    pub fn dcx_mut(&mut self) -> &mut DiagCtxt {
        &mut self.sess_mut().dcx
    }

    /// Enters the compiler context.
    ///
    /// See [`Session::enter`](Session::enter) for more details.
    pub fn enter<T: Send>(&self, f: impl FnOnce(&CompilerRef<'_>) -> T + Send) -> T {
        self.0.sess.enter(|| f(CompilerRef::new(&self.0)))
    }

    /// Enters the compiler context with mutable access.
    ///
    /// This is currently only necessary when parsing sources and lowering the ASTs.
    /// All accesses after can make use of `gcx`, passed by immutable reference.
    ///
    /// See [`Session::enter`](Session::enter) for more details.
    pub fn enter_mut<T: Send>(&mut self, f: impl FnOnce(&mut CompilerRef<'_>) -> T + Send) -> T {
        // SAFETY: `CompilerRef` does not allow mutable access to the session.
        let sess = unsafe { trustme::decouple_lt(&self.0.sess) };
        sess.enter(|| f(self.as_mut()))
    }

    /// Enters the compiler context.
    ///
    /// Note that this does not set up the rayon thread pool. This is only useful when parsing
    /// sequentially, like manually using `Parser`. Otherwise, it might cause panics later on if a
    /// thread pool is expected to be set up correctly.
    ///
    /// See [`enter`](Self::enter) for more details.
    pub fn enter_sequential<T>(&self, f: impl FnOnce(&CompilerRef<'_>) -> T) -> T {
        self.0.sess.enter_sequential(|| f(CompilerRef::new(&self.0)))
    }

    /// Enters the compiler context with mutable access.
    ///
    /// Note that this does not set up the rayon thread pool. This is only useful when parsing
    /// sequentially, like manually using `Parser`. Otherwise, it might cause panics later on if a
    /// thread pool is expected to be set up correctly.
    ///
    /// See [`enter_mut`](Self::enter_mut) for more details.
    pub fn enter_sequential_mut<T>(&mut self, f: impl FnOnce(&mut CompilerRef<'_>) -> T) -> T {
        // SAFETY: `CompilerRef` does not allow mutable access to the session.
        let sess = unsafe { trustme::decouple_lt(&self.0.sess) };
        sess.enter_sequential(|| f(self.as_mut()))
    }

    fn as_mut(&mut self) -> &mut CompilerRef<'_> {
        // SAFETY: `CompilerRef` does not allow invalidating the `Pin`.
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
        unsafe {
            let sess_p = project_ptr!(this->sess);
            sess_p.write(sess);

            let sess = &*sess_p;
            project_ptr!(this->gcx).write(GlobalCtxt::new(sess));
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
        unsafe { ManuallyDrop::drop(&mut self.0) };
    }
}

/// A reference to the compiler.
///
/// This is only available inside the [`Compiler::enter`] closure, and has access to the global
/// context.
#[repr(transparent)]
pub struct CompilerRef<'c> {
    inner: CompilerInner<'c>,
}

impl fmt::Debug for CompilerRef<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.debug_fmt("CompilerRef", f)
    }
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

    /// Returns a reference to the compiler session.
    #[inline]
    pub fn sess(&self) -> &'c Session {
        self.gcx().sess
    }

    // NOTE: Do not expose mutable access to the session directly! See `replace_entered_session`.
    /// Returns a mutable reference to the compiler session.
    #[inline]
    fn sess_mut(&mut self) -> &mut Session {
        &mut self.inner.sess
    }

    /// Returns a reference to the diagnostics context.
    #[inline]
    pub fn dcx(&self) -> &'c DiagCtxt {
        &self.sess().dcx
    }

    /// Returns a mutable reference to the diagnostics context.
    #[inline]
    pub fn dcx_mut(&mut self) -> &mut DiagCtxt {
        &mut self.sess_mut().dcx
    }

    /// Returns a reference to the sources.
    #[inline]
    pub fn sources(&self) -> &'c Sources<'c> {
        &self.gcx().sources
    }

    /// Returns a mutable reference to the sources.
    #[inline]
    pub fn sources_mut(&mut self) -> &mut Sources<'c> {
        &mut self.gcx_mut().get_mut().sources
    }

    /// Returns a reference to the global context.
    #[inline]
    pub fn gcx(&self) -> Gcx<'c> {
        // SAFETY: `CompilerRef` is only accessible in the `Compiler::enter` closure.
        Gcx::new(unsafe { trustme::decouple_lt(&self.inner.gcx) })
    }

    #[inline]
    pub(crate) fn gcx_mut(&mut self) -> GcxMut<'c> {
        // SAFETY: `CompilerRef` is only accessible in the `Compiler::enter` closure.
        GcxMut::new(&mut self.inner.gcx)
    }

    /// Drops the sources, ASTs, and AST arenas in a separate thread.
    ///
    /// This is not done by default in the pipeline, but it can be called after `lower_asts` to
    /// free up memory.
    pub fn drop_asts(&mut self) {
        // TODO: Do we want to drop all the sources instead of just the ASTs?
        let sources = std::mem::take(&mut self.inner.gcx.sources);
        // SAFETY: `sources` points into `ast_arenas`, which we move together into the closure.
        let sources = unsafe { std::mem::transmute::<Sources<'_>, Sources<'static>>(sources) };
        let mut ast_arenas = std::mem::take(&mut self.inner.gcx.ast_arenas);
        self.inner.gcx.sess.spawn(move || {
            let _guard = debug_span!("drop_asts").entered();
            log_ast_arenas_stats(&mut ast_arenas);
            drop(sources);
            drop(ast_arenas);
        });
    }

    /// Returns a builder for parsing sources.
    ///
    /// [`ParsingContext::parse`](ParsingContext::parse) must be called at the end to actually parse
    /// the sources.
    pub fn parse(&mut self) -> ParsingContext<'c> {
        ParsingContext::new(self.gcx_mut())
    }

    /// Performs AST lowering.
    ///
    /// Lowers the entire program to HIR, populating `gcx.hir`.
    pub fn lower_asts(&mut self) -> Result<ControlFlow<()>> {
        crate::lower(self)
    }

    pub fn analysis(&self) -> Result<ControlFlow<()>> {
        crate::analysis(self.gcx())
    }

    fn debug_fmt(&self, name: &str, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct(name).field("gcx", &self.gcx()).finish_non_exhaustive()
    }
}

fn log_ast_arenas_stats(arenas: &mut ThreadLocal<solar_ast::Arena>) {
    if arenas.iter_mut().len() == 0 {
        return;
    }
    debug!(asts_allocated = %fmt_bytes(arenas.iter_mut().map(|a| a.allocated_bytes()).sum::<usize>()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // --- copy from `crates/interface/src/session.rs`
    use solar_ast::{Span, Symbol};
    use solar_interface::{BytePos, ColorChoice};

    /// Session to test `enter`.
    fn enter_tests_session() -> Session {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.source_map().new_source_file(PathBuf::from("test"), "abcd").unwrap();
        sess
    }

    #[track_caller]
    fn use_globals_parallel(sess: &Session) {
        use rayon::prelude::*;

        use_globals();
        sess.spawn(|| use_globals());
        sess.join(|| use_globals(), || use_globals());
        [1, 2, 3].par_iter().for_each(|_| use_globals());
        use_globals();
    }

    #[track_caller]
    fn use_globals() {
        use_globals_no_sm();

        let span = Span::new(BytePos(0), BytePos(1));
        assert_eq!(format!("{span:?}"), "test:1:1: 1:2");
        assert_eq!(format!("{span:#?}"), "test:1:1: 1:2");
    }

    #[track_caller]
    fn use_globals_no_sm() {
        let s = "hello";
        let sym = Symbol::intern(s);
        assert_eq!(sym.as_str(), s);
    }
    // --- end copy

    #[test]
    fn parse_multiple_times() {
        let sess = Session::builder().with_test_emitter().build();
        let mut compiler = Compiler::new(sess);

        assert!(compiler.enter(|c| c.gcx().sources.is_empty()));
        compiler.enter_mut(|c| {
            let pcx = c.parse();
            pcx.parse();
        });
        assert!(compiler.enter(|c| c.gcx().sources.is_empty()));

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            pcx.add_file(
                c.sess().source_map().new_source_file(PathBuf::from("test.sol"), "").unwrap(),
            );
            pcx.parse();
        });
        assert_eq!(compiler.enter(|c| c.gcx().sources.len()), 1);
        assert_eq!(compiler.enter(|c| c.gcx().sources.asts().count()), 1);

        compiler.enter_mut(|c| {
            let mut pcx = c.parse();
            pcx.add_file(
                c.sess().source_map().new_source_file(PathBuf::from("test2.sol"), "").unwrap(),
            );
            pcx.parse();
        });
        assert_eq!(compiler.enter(|c| c.gcx().sources.len()), 2);
        assert_eq!(compiler.enter(|c| c.gcx().sources.asts().count()), 2);

        compiler.enter_mut(|c| c.drop_asts());
        assert_eq!(compiler.enter(|c| c.gcx().sources.len()), 0);
        assert_eq!(compiler.enter(|c| c.gcx().sources.asts().count()), 0);
    }

    fn stage_test(expected: Result<(), &str>, f: fn(&mut CompilerRef<'_>)) {
        let sess =
            Session::builder().with_buffer_emitter(solar_interface::ColorChoice::Never).build();
        let mut compiler = Compiler::new(sess);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| compiler.enter_mut(f)));
        let errs = compiler.sess().dcx.emitted_errors().unwrap();
        match expected {
            Ok(()) => assert!(r.is_ok(), "panicked: {errs:#?}"),
            Err(e) => {
                assert!(r.is_err(), "didn't panic: {errs:#?}");
                let errs = errs.unwrap_err();
                let d = errs.to_string();
                assert!(d.contains("invalid compiler stage transition:"), "{d}");
                assert!(d.contains(e), "{d}");
                assert!(d.contains("stages must be advanced sequentially"), "{d}");
            }
        }
    }

    fn parse_dummy_file(c: &mut CompilerRef<'_>) {
        let mut pcx = c.parse();
        pcx.add_file(c.sess().source_map().new_source_file(PathBuf::from("test.sol"), "").unwrap());
        pcx.parse();
    }

    #[test]
    fn stage_tests() {
        // Backwards.
        stage_test(Err("from `lowering` to `parsing`"), |c| {
            parse_dummy_file(c);
            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            parse_dummy_file(c);
        });

        // Too far ahead.
        stage_test(Err("from `none` to `analysis`"), |c| {
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });

        // Same stage.
        stage_test(Err("from `lowering` to `lowering`"), |c| {
            parse_dummy_file(c);
            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });
        stage_test(Err("from `analysis` to `analysis`"), |c| {
            parse_dummy_file(c);
            assert_eq!(c.lower_asts(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
            assert_eq!(c.analysis(), Ok(ControlFlow::Continue(())));
        });
        // Parsing is special cased.
        stage_test(Ok(()), |c| {
            parse_dummy_file(c);
            parse_dummy_file(c);
        });
    }

    #[test]
    fn replace_session() {
        let mut compiler = Compiler::new(Session::builder().with_test_emitter().build());
        compiler.dcx().err("test").emit();
        assert!(compiler.sess().dcx.has_errors().is_err());
        *compiler.sess_mut() = Session::builder().with_test_emitter().build();
        assert!(compiler.sess().dcx.has_errors().is_ok());
    }

    #[test]
    fn replace_entered_session() {
        let mut compiler = Compiler::new(enter_tests_session());
        compiler.enter_mut(|compiler| {
            use_globals_parallel(compiler.sess());

            compiler.dcx().err("test").emit();
            assert!(compiler.sess().dcx.has_errors().is_err());

            // Replacing `Session` here drops the internal thread pool, which is currently in use,
            // so we must not expose mutable access to the session.
            *compiler.dcx_mut() = enter_tests_session().dcx;
            assert!(compiler.sess().dcx.has_errors().is_ok());

            use_globals_parallel(compiler.sess());
        });
        assert!(compiler.sess().dcx.has_errors().is_ok());
    }
}

use crate::{
    ColorChoice, SessionGlobals, SourceMap,
    diagnostics::{DiagCtxt, EmittedDiagnostics},
};
use solar_config::{CompilerOutput, CompilerStage, Opts, SINGLE_THREADED_TARGET, UnstableOpts};
use std::{path::Path, sync::Arc};

/// Information about the current compiler session.
pub struct Session {
    /// The compiler options.
    pub opts: Opts,

    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The globals.
    globals: SessionGlobals,
}

/// [`Session`] builder.
#[derive(Default)]
#[must_use = "builders don't do anything unless you call `build`"]
pub struct SessionBuilder {
    dcx: Option<DiagCtxt>,
    globals: Option<SessionGlobals>,
    opts: Option<Opts>,
}

impl SessionBuilder {
    /// Sets the diagnostic context. This is required.
    ///
    /// See also the `with_*_emitter*` methods.
    pub fn dcx(mut self, dcx: DiagCtxt) -> Self {
        self.dcx = Some(dcx);
        self
    }

    /// Sets the source map.
    pub fn source_map(mut self, source_map: Arc<SourceMap>) -> Self {
        self.get_globals().source_map = source_map;
        self
    }

    /// Sets the compiler options.
    pub fn opts(mut self, opts: Opts) -> Self {
        self.opts = Some(opts);
        self
    }

    /// Sets the diagnostic context to a test emitter.
    #[inline]
    pub fn with_test_emitter(mut self) -> Self {
        let sm = self.get_source_map();
        self.dcx(DiagCtxt::with_test_emitter(Some(sm)))
    }

    /// Sets the diagnostic context to a stderr emitter.
    #[inline]
    pub fn with_stderr_emitter(self) -> Self {
        self.with_stderr_emitter_and_color(ColorChoice::Auto)
    }

    /// Sets the diagnostic context to a stderr emitter and a color choice.
    #[inline]
    pub fn with_stderr_emitter_and_color(mut self, color_choice: ColorChoice) -> Self {
        let sm = self.get_source_map();
        self.dcx(DiagCtxt::with_stderr_emitter_and_color(Some(sm), color_choice))
    }

    /// Sets the diagnostic context to a human emitter that emits diagnostics to a local buffer.
    #[inline]
    pub fn with_buffer_emitter(mut self, color_choice: ColorChoice) -> Self {
        let sm = self.get_source_map();
        self.dcx(DiagCtxt::with_buffer_emitter(Some(sm), color_choice))
    }

    /// Sets the diagnostic context to a silent emitter.
    #[inline]
    pub fn with_silent_emitter(self, fatal_note: Option<String>) -> Self {
        self.dcx(DiagCtxt::with_silent_emitter(fatal_note))
    }

    /// Sets the number of threads to use for parallelism to 1.
    #[inline]
    pub fn single_threaded(self) -> Self {
        self.threads(1)
    }

    /// Sets the number of threads to use for parallelism. Zero specifies the number of logical
    /// cores.
    #[inline]
    pub fn threads(mut self, threads: usize) -> Self {
        self.opts_mut().threads = threads.into();
        self
    }

    /// Gets the source map from the diagnostics context.
    fn get_source_map(&mut self) -> Arc<SourceMap> {
        self.get_globals().source_map.clone()
    }

    fn get_globals(&mut self) -> &mut SessionGlobals {
        self.globals.get_or_insert_default()
    }

    /// Returns a mutable reference to the options.
    fn opts_mut(&mut self) -> &mut Opts {
        self.opts.get_or_insert_default()
    }

    /// Consumes the builder to create a new session.
    ///
    /// The diagnostics context must be set before calling this method, either by calling
    /// [`dcx`](Self::dcx) or by using one of the provided helper methods, like
    /// [`with_stderr_emitter`](Self::with_stderr_emitter).
    ///
    /// # Panics
    ///
    /// Panics if:
    /// - the diagnostics context is not set
    /// - the source map in the diagnostics context does not match the one set in the builder
    #[track_caller]
    pub fn build(mut self) -> Session {
        let mut dcx = self.dcx.take().unwrap_or_else(|| panic!("diagnostics context not set"));
        Session {
            globals: match self.globals.take() {
                Some(globals) => {
                    // Check that the source map matches the one in the diagnostics context.
                    if let Some(sm) = dcx.source_map_mut() {
                        assert!(
                            Arc::ptr_eq(&globals.source_map, sm),
                            "session source map does not match the one in the diagnostics context"
                        );
                    }
                    globals
                }
                None => {
                    // Set the source map from the diagnostics context.
                    let sm = dcx.source_map_mut().cloned().unwrap_or_default();
                    SessionGlobals::new(sm)
                }
            },
            dcx,
            opts: self.opts.take().unwrap_or_default(),
        }
    }
}

impl Session {
    /// Creates a new session with the given diagnostics context and source map.
    pub fn new(dcx: DiagCtxt, source_map: Arc<SourceMap>) -> Self {
        Self::builder().dcx(dcx).source_map(source_map).build()
    }

    /// Creates a new session with the given diagnostics context and an empty source map.
    pub fn empty(dcx: DiagCtxt) -> Self {
        Self::builder().dcx(dcx).build()
    }

    /// Creates a new session builder.
    #[inline]
    pub fn builder() -> SessionBuilder {
        SessionBuilder::default()
    }

    /// Infers the language from the input files.
    pub fn infer_language(&mut self) {
        if !self.opts.input.is_empty()
            && self.opts.input.iter().all(|arg| Path::new(arg).extension() == Some("yul".as_ref()))
        {
            self.opts.language = solar_config::Language::Yul;
        }
    }

    /// Validates the session options.
    pub fn validate(&self) -> crate::Result<()> {
        let mut result = Ok(());
        result = result.and(self.check_unique("emit", &self.opts.emit));
        result
    }

    fn check_unique<T: Eq + std::hash::Hash + std::fmt::Display>(
        &self,
        name: &str,
        list: &[T],
    ) -> crate::Result<()> {
        let mut result = Ok(());
        let mut seen = std::collections::HashSet::new();
        for item in list {
            if !seen.insert(item) {
                let msg = format!("cannot specify `--{name} {item}` twice");
                result = Err(self.dcx.err(msg).emit());
            }
        }
        result
    }

    /// Returns the unstable options.
    #[inline]
    pub fn unstable(&self) -> &UnstableOpts {
        &self.opts.unstable
    }

    /// Returns the emitted diagnostics. Can be empty.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](SessionBuilder::with_buffer_emitter).
    #[inline]
    pub fn emitted_diagnostics(&self) -> Option<EmittedDiagnostics> {
        self.dcx.emitted_diagnostics()
    }

    /// Returns `Err` with the printed diagnostics if any errors have been emitted.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](SessionBuilder::with_buffer_emitter).
    #[inline]
    pub fn emitted_errors(&self) -> Option<Result<(), EmittedDiagnostics>> {
        self.dcx.emitted_errors()
    }

    /// Returns a reference to the source map.
    #[inline]
    pub fn source_map(&self) -> &SourceMap {
        &self.globals.source_map
    }

    /// Clones the source map.
    #[inline]
    pub fn clone_source_map(&self) -> Arc<SourceMap> {
        self.globals.source_map.clone()
    }

    /// Returns `true` if compilation should stop after the given stage.
    #[inline]
    pub fn stop_after(&self, stage: CompilerStage) -> bool {
        self.opts.stop_after >= Some(stage)
    }

    /// Returns the number of threads to use for parallelism.
    #[inline]
    pub fn threads(&self) -> usize {
        self.opts.threads().get()
    }

    /// Returns `true` if parallelism is not enabled.
    #[inline]
    pub fn is_sequential(&self) -> bool {
        self.threads() == 1
    }

    /// Returns `true` if parallelism is enabled.
    #[inline]
    pub fn is_parallel(&self) -> bool {
        !self.is_sequential()
    }

    /// Returns `true` if the given output should be emitted.
    #[inline]
    pub fn do_emit(&self, output: CompilerOutput) -> bool {
        self.opts.emit.contains(&output)
    }

    /// Spawns the given closure on the thread pool or executes it immediately if parallelism is not
    /// enabled.
    // NOTE: This only exists because on a `use_current_thread` thread pool `rayon::spawn` will
    // never execute.
    #[inline]
    pub fn spawn(&self, f: impl FnOnce() + Send + 'static) {
        if self.is_sequential() {
            f();
        } else {
            rayon::spawn(f);
        }
    }

    /// Takes two closures and potentially runs them in parallel. It returns a pair of the results
    /// from those closures.
    #[inline]
    pub fn join<A, B, RA, RB>(&self, oper_a: A, oper_b: B) -> (RA, RB)
    where
        A: FnOnce() -> RA + Send,
        B: FnOnce() -> RB + Send,
        RA: Send,
        RB: Send,
    {
        if self.is_sequential() { (oper_a(), oper_b()) } else { rayon::join(oper_a, oper_b) }
    }

    /// Executes the given closure in a fork-join scope.
    ///
    /// See [`rayon::scope`] for more details.
    #[inline]
    pub fn scope<'scope, OP, R>(&self, op: OP) -> R
    where
        OP: FnOnce(solar_data_structures::sync::Scope<'_, 'scope>) -> R + Send,
        R: Send,
    {
        solar_data_structures::sync::scope(self.is_parallel(), op)
    }

    /// Sets up the session globals in the current thread, then executes the given closure.
    ///
    /// The globals are stored in this [`Session`] itself, meaning multiple consecutive calls to
    /// [`enter`](Self::enter) will share the same globals.
    ///
    /// Note that this does not set up the rayon thread pool. This is only useful when parsing
    /// sequentially, like manually using `Parser`.
    #[inline]
    #[track_caller]
    pub fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        self.globals.set(f)
    }

    /// Sets up a thread pool if parallelism is enabled, setting up the session globals on every
    /// thread.
    ///
    /// See [`enter`](Self::enter) for more details.
    #[inline]
    #[track_caller]
    pub fn enter_parallel<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        self.enter(|| enter_thread_pool(self, f))
    }
}

/// Runs the given closure in a thread pool with the given number of threads.
#[track_caller]
fn enter_thread_pool<R: Send>(sess: &Session, f: impl FnOnce() -> R + Send) -> R {
    // Avoid panicking below if this is a recursive call.
    if rayon::current_thread_index().is_some() {
        debug!(
            "running in the current thread's rayon thread pool; \
             this could cause panics later on if it was created without setting the session globals!"
        );
        return f();
    }

    let threads = sess.threads();
    debug_assert!(threads > 0, "number of threads must already be resolved");
    let mut builder =
        rayon::ThreadPoolBuilder::new().thread_name(|i| format!("solar-{i}")).num_threads(threads);
    // We still want to use a rayon thread pool with 1 thread so that `ParallelIterator`s don't
    // install and run in the default global thread pool.
    if threads == 1 {
        builder = builder.use_current_thread();
    }
    match builder.build_scoped(
        // Initialize each new worker thread when created.
        // Note that this is not called on the current thread, so `SessionGlobals::set` can't
        // panic.
        move |thread| sess.enter(|| thread.run()),
        // Run `f` on the first thread in the thread pool.
        move |pool| pool.install(f),
    ) {
        Ok(r) => r,
        Err(e) => {
            let mut err = sess.dcx.fatal(format!("failed to build the rayon thread pool: {e}"));
            if threads > 1 {
                if SINGLE_THREADED_TARGET {
                    err = err.note("the current target might not support multi-threaded execution");
                }
                err = err.help("try running with `--threads 1` / `-j1` to disable parallelism");
            }
            err.emit();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    #[should_panic = "diagnostics context not set"]
    fn no_dcx() {
        Session::builder().build();
    }

    #[test]
    #[should_panic = "session source map does not match the one in the diagnostics context"]
    fn sm_mismatch() {
        let sm1 = Arc::<SourceMap>::default();
        let sm2 = Arc::<SourceMap>::default();
        assert!(!Arc::ptr_eq(&sm1, &sm2));
        Session::builder().source_map(sm1).dcx(DiagCtxt::with_stderr_emitter(Some(sm2))).build();
    }

    #[test]
    #[should_panic = "session source map does not match the one in the diagnostics context"]
    fn sm_mismatch_non_builder() {
        let sm1 = Arc::<SourceMap>::default();
        let sm2 = Arc::<SourceMap>::default();
        assert!(!Arc::ptr_eq(&sm1, &sm2));
        Session::new(DiagCtxt::with_stderr_emitter(Some(sm2)), sm1);
    }

    #[test]
    fn builder() {
        let _ = Session::builder().with_stderr_emitter().build();
    }

    #[test]
    fn empty() {
        let _ = Session::empty(DiagCtxt::with_stderr_emitter(None));
        let _ = Session::empty(DiagCtxt::with_stderr_emitter(Some(Default::default())));
    }

    #[test]
    fn local() {
        let sess = Session::builder().with_stderr_emitter().build();
        assert!(sess.emitted_diagnostics().is_none());
        assert!(sess.emitted_errors().is_none());

        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.err("test").emit();
        let err = sess.dcx.emitted_errors().unwrap().unwrap_err();
        let err = Box::new(err) as Box<dyn std::error::Error>;
        assert!(err.to_string().contains("error: test"), "{err:?}");
    }

    #[test]
    fn enter() {
        #[track_caller]
        fn use_globals_no_sm() {
            SessionGlobals::with(|_globals| {});

            let s = "hello";
            let sym = crate::Symbol::intern(s);
            assert_eq!(sym.as_str(), s);
        }

        #[track_caller]
        fn use_globals() {
            use_globals_no_sm();

            let span = crate::Span::new(crate::BytePos(0), crate::BytePos(1));
            assert_eq!(format!("{span:?}"), "test:1:1: 1:2");
            assert_eq!(format!("{span:#?}"), "test:1:1: 1:2");

            assert!(rayon::current_thread_index().is_some());
        }

        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.source_map().new_source_file(PathBuf::from("test"), "abcd").unwrap();
        sess.enter_parallel(|| use_globals());
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
        sess.enter_parallel(|| {
            use_globals();
            sess.enter_parallel(use_globals);
            use_globals();
        });
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());

        sess.enter(|| {
            use_globals_no_sm();
            sess.enter_parallel(|| {
                use_globals();
                sess.enter_parallel(|| use_globals());
                use_globals();
            });
            use_globals_no_sm();
        });
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
    }

    #[test]
    fn enter_diags() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
        sess.enter_parallel(|| {
            sess.dcx.err("test1").emit();
            assert!(sess.dcx.emitted_errors().unwrap().is_err());
        });
        assert!(sess.dcx.emitted_errors().unwrap().unwrap_err().to_string().contains("test1"));
        sess.enter_parallel(|| {
            sess.dcx.err("test2").emit();
            assert!(sess.dcx.emitted_errors().unwrap().is_err());
        });
        assert!(sess.dcx.emitted_errors().unwrap().unwrap_err().to_string().contains("test1"));
        assert!(sess.dcx.emitted_errors().unwrap().unwrap_err().to_string().contains("test2"));
    }

    #[test]
    fn set_opts() {
        let _ = Session::builder()
            .with_test_emitter()
            .opts(Opts {
                evm_version: solar_config::EvmVersion::Berlin,
                unstable: UnstableOpts { ast_stats: false, ..Default::default() },
                ..Default::default()
            })
            .build();
    }
}

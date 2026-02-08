use crate::{
    ByteSymbol, ColorChoice, SessionGlobals, SourceMap, Symbol,
    diagnostics::{DiagCtxt, EmittedDiagnostics},
};
use solar_config::{CompilerOutput, CompilerStage, Opts, SINGLE_THREADED_TARGET, UnstableOpts};
use std::{
    fmt,
    path::Path,
    sync::{Arc, OnceLock},
};

/// Information about the current compiler session.
pub struct Session {
    /// The compiler options.
    pub opts: Opts,

    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The globals.
    globals: Arc<SessionGlobals>,
    /// The rayon thread pool. This is spawned lazily on first use, rather than always constructing
    /// one with `SessionBuilder`.
    thread_pool: OnceLock<rayon::ThreadPool>,
}

impl Default for Session {
    fn default() -> Self {
        Self::new(Opts::default())
    }
}

impl fmt::Debug for Session {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Session")
            .field("opts", &self.opts)
            .field("dcx", &self.dcx)
            .finish_non_exhaustive()
    }
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
    /// Sets the diagnostic context.
    ///
    /// If `opts` is set this will default to [`DiagCtxt::from_opts`], otherwise this is required.
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
        let opts = self.opts.take();
        let mut dcx = self.dcx.take().unwrap_or_else(|| {
            opts.as_ref()
                .map(DiagCtxt::from_opts)
                .unwrap_or_else(|| panic!("either diagnostics context or options must be set"))
        });
        let sess = Session {
            globals: Arc::new(match self.globals.take() {
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
            }),
            dcx,
            opts: opts.unwrap_or_default(),
            thread_pool: OnceLock::new(),
        };
        sess.reconfigure();
        debug!(version = %solar_config::version::SEMVER_VERSION, "created new session");
        sess
    }
}

impl Session {
    /// Creates a new session builder.
    #[inline]
    pub fn builder() -> SessionBuilder {
        SessionBuilder::default()
    }

    /// Creates a new session from the given options.
    ///
    /// See [`builder`](Self::builder) for a more flexible way to create a session.
    pub fn new(opts: Opts) -> Self {
        Self::builder().opts(opts).build()
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

    /// Reconfigures inner state to match any new options.
    ///
    /// Call this after updating options.
    pub fn reconfigure(&self) {
        'bp: {
            let new_base_path = if self.opts.unstable.ui_testing {
                // `ui_test` relies on absolute paths.
                None
            } else if let Some(base_path) =
                self.opts.base_path.clone().or_else(|| std::env::current_dir().ok())
                && let Ok(base_path) = self.source_map().file_loader().canonicalize_path(&base_path)
            {
                Some(base_path)
            } else {
                break 'bp;
            };
            self.source_map().set_base_path(new_base_path);
        }
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

    /// Returns the emitted diagnostics as a result. Can be empty.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](SessionBuilder::with_buffer_emitter).
    ///
    /// Results `Ok` if there are no errors, `Err` otherwise.
    ///
    /// # Examples
    ///
    /// Print diagnostics to `stdout` if there are no errors, otherwise propagate with `?`:
    ///
    /// ```no_run
    /// # fn f(dcx: solar_interface::diagnostics::DiagCtxt) -> Result<(), Box<dyn std::error::Error>> {
    /// println!("{}", dcx.emitted_diagnostics_result().unwrap()?);
    /// # Ok(())
    /// # }
    /// ```
    #[inline]
    pub fn emitted_diagnostics_result(
        &self,
    ) -> Option<Result<EmittedDiagnostics, EmittedDiagnostics>> {
        self.dcx.emitted_diagnostics_result()
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
    ///
    /// NOTE: on a `use_current_thread` thread pool `rayon::spawn` will never execute without
    /// yielding to rayon, so prefer using this method over `rayon::spawn`.
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
    ///
    /// NOTE: on a `use_current_thread` thread pool `rayon::join` will never execute without
    /// yielding to rayon, so prefer using this method over `rayon::join`.
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

    /// Sets up the session globals and executes the given closure in the thread pool.
    ///
    /// The thread pool and globals are stored in this [`Session`] itself, meaning multiple
    /// consecutive calls to [`enter`](Self::enter) will share the same globals and resources.
    #[track_caller]
    pub fn enter<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        if in_rayon() {
            // Avoid panicking if we were to build a `current_thread` thread pool.
            if self.is_sequential() {
                reentrant_log();
                return self.enter_sequential(f);
            }
            // Avoid creating a new thread pool if it's already set up with the same globals.
            if self.is_entered() {
                // No need to set again.
                return f();
            }
        }

        self.enter_sequential(|| self.thread_pool().install(f))
    }

    /// Sets up the session globals and executes the given closure in the current thread.
    ///
    /// Note that this does not set up the rayon thread pool. This is only useful when parsing
    /// sequentially, like manually using `Parser`. Otherwise, it might cause panics later on if a
    /// thread pool is expected to be set up correctly.
    ///
    /// See [`enter`](Self::enter) for more details.
    #[inline]
    #[track_caller]
    pub fn enter_sequential<R>(&self, f: impl FnOnce() -> R) -> R {
        self.globals.set(f)
    }

    /// Interns a string in this session's symbol interner.
    ///
    /// The symbol may not be usable on its own if the session has not been [entered](Self::enter).
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn intern(&self, s: &str) -> Symbol {
        self.globals.symbol_interner.intern(s)
    }

    /// Resolves a symbol to its string representation.
    ///
    /// The given symbol must have been interned in this session.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn resolve_symbol(&self, s: Symbol) -> &str {
        self.globals.symbol_interner.get(s)
    }

    /// Interns a byte string in this session's symbol interner.
    ///
    /// The symbol may not be usable on its own if the session has not been [entered](Self::enter).
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn intern_byte_str(&self, s: &[u8]) -> ByteSymbol {
        self.globals.symbol_interner.intern_byte_str(s)
    }

    /// Resolves a byte symbol to its string representation.
    ///
    /// The given symbol must have been interned in this session.
    #[inline]
    #[cfg_attr(debug_assertions, track_caller)]
    pub fn resolve_byte_str(&self, s: ByteSymbol) -> &[u8] {
        self.globals.symbol_interner.get_byte_str(s)
    }

    /// Returns `true` if this session has been entered.
    pub fn is_entered(&self) -> bool {
        SessionGlobals::try_with(|g| g.is_some_and(|g| g.maybe_eq(&self.globals)))
    }

    fn thread_pool(&self) -> &rayon::ThreadPool {
        self.thread_pool.get_or_init(|| {
            trace!(threads = self.threads(), "building rayon thread pool");
            self.thread_pool_builder()
                .spawn_handler(|thread| {
                    let mut builder = std::thread::Builder::new();
                    if let Some(name) = thread.name() {
                        builder = builder.name(name.to_string());
                    }
                    if let Some(size) = thread.stack_size() {
                        builder = builder.stack_size(size);
                    }
                    let globals = self.globals.clone();
                    builder.spawn(move || globals.set(|| thread.run()))?;
                    Ok(())
                })
                .build()
                .unwrap_or_else(|e| self.handle_thread_pool_build_error(e))
        })
    }

    fn thread_pool_builder(&self) -> rayon::ThreadPoolBuilder {
        let threads = self.threads();
        debug_assert!(threads > 0, "number of threads must already be resolved");
        let mut builder = rayon::ThreadPoolBuilder::new()
            .thread_name(|i| format!("solar-{i}"))
            .num_threads(threads);
        // We still want to use a rayon thread pool with 1 thread so that `ParallelIterator`s don't
        // install and run in the default global thread pool.
        if threads == 1 {
            builder = builder.use_current_thread();
        }
        builder
    }

    #[cold]
    fn handle_thread_pool_build_error(&self, e: rayon::ThreadPoolBuildError) -> ! {
        let mut err = self.dcx.fatal(format!("failed to build the rayon thread pool: {e}"));
        if self.is_parallel() {
            if SINGLE_THREADED_TARGET {
                err = err.note("the current target might not support multi-threaded execution");
            }
            err = err.help("try running with `--threads 1` / `-j1` to disable parallelism");
        }
        err.emit()
    }
}

fn reentrant_log() {
    debug!(
        "running in the current thread's rayon thread pool; \
         this could cause panics later on if it was created without setting the session globals!"
    );
}

#[inline]
fn in_rayon() -> bool {
    rayon::current_thread_index().is_some()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

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

        let span = crate::Span::new(crate::BytePos(0), crate::BytePos(1));
        assert_eq!(format!("{span:?}"), "test:1:1: 1:2");
        assert_eq!(format!("{span:#?}"), "test:1:1: 1:2");
    }

    #[track_caller]
    fn use_globals_no_sm() {
        SessionGlobals::with(|_globals| {});

        let s = "hello";
        let sym = crate::Symbol::intern(s);
        assert_eq!(sym.as_str(), s);
    }

    #[track_caller]
    fn cant_use_globals() {
        std::panic::catch_unwind(|| use_globals()).unwrap_err();
    }

    #[test]
    fn builder() {
        let _ = Session::builder().with_stderr_emitter().build();
    }

    #[test]
    fn not_builder() {
        let _ = Session::new(Opts::default());
        let _ = Session::default();
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
    #[should_panic = "either diagnostics context or options must be set"]
    fn no_dcx() {
        Session::builder().build();
    }

    #[test]
    fn dcx() {
        let _ = Session::builder().dcx(DiagCtxt::with_stderr_emitter(None)).build();
        let _ =
            Session::builder().dcx(DiagCtxt::with_stderr_emitter(Some(Default::default()))).build();
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
        crate::enter(|| {
            use_globals_no_sm();
            cant_use_globals();
        });

        let sess = enter_tests_session();
        sess.enter(|| use_globals());
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
        sess.enter(|| {
            use_globals();
            sess.enter(|| use_globals());
            use_globals();
        });
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());

        sess.enter_sequential(|| {
            use_globals();
            sess.enter(|| {
                use_globals_parallel(&sess);
                sess.enter(|| use_globals_parallel(&sess));
                use_globals_parallel(&sess);
            });
            use_globals();
        });
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
    }

    #[test]
    fn enter_diags() {
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
        sess.enter(|| {
            sess.dcx.err("test1").emit();
            assert!(sess.dcx.emitted_errors().unwrap().is_err());
        });
        assert!(sess.dcx.emitted_errors().unwrap().unwrap_err().to_string().contains("test1"));
        sess.enter(|| {
            sess.dcx.err("test2").emit();
            assert!(sess.dcx.emitted_errors().unwrap().is_err());
        });
        assert!(sess.dcx.emitted_errors().unwrap().unwrap_err().to_string().contains("test1"));
        assert!(sess.dcx.emitted_errors().unwrap().unwrap_err().to_string().contains("test2"));
    }

    #[test]
    fn enter_thread_pool() {
        let sess = enter_tests_session();

        assert!(!in_rayon());
        sess.enter(|| {
            assert!(in_rayon());
            sess.enter(|| {
                assert!(in_rayon());
            });
            assert!(in_rayon());
        });
        sess.enter_sequential(|| {
            assert!(!in_rayon());
            sess.enter(|| {
                assert!(in_rayon());
            });
            assert!(!in_rayon());
            sess.enter_sequential(|| {
                assert!(!in_rayon());
            });
            assert!(!in_rayon());
        });
        assert!(!in_rayon());

        let pool = rayon::ThreadPoolBuilder::new().build().unwrap();
        pool.install(|| {
            assert!(in_rayon());
            cant_use_globals();
            sess.enter(|| use_globals_parallel(&sess));
            assert!(in_rayon());
            cant_use_globals();
        });
        assert!(!in_rayon());
    }

    #[test]
    fn enter_different_nested_sessions() {
        let sess1 = enter_tests_session();
        let sess2 = enter_tests_session();
        assert!(!sess1.globals.maybe_eq(&sess2.globals));
        sess1.enter(|| {
            SessionGlobals::with(|g| assert!(g.maybe_eq(&sess1.globals)));
            use_globals();
            sess2.enter(|| {
                SessionGlobals::with(|g| assert!(g.maybe_eq(&sess2.globals)));
                use_globals();
            });
            use_globals();
        });
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

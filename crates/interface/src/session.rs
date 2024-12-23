use crate::{
    diagnostics::{DiagCtxt, EmittedDiagnostics},
    ColorChoice, SessionGlobals, SourceMap,
};
use solar_config::{CompilerOutput, CompilerStage, Opts, UnstableOpts};
use std::sync::Arc;

/// Information about the current compiler session.
#[derive(derive_builder::Builder)]
#[builder(pattern = "owned", build_fn(name = "try_build", private), setter(strip_option))]
pub struct Session {
    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The source map.
    #[builder(default)]
    source_map: Arc<SourceMap>,

    /// The compiler options.
    #[builder(default)]
    pub opts: Opts,
}

impl SessionBuilder {
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

    /// Sets the number of threads to use for parallelism.
    #[inline]
    pub fn threads(mut self, threads: usize) -> Self {
        self.opts_mut().threads = threads.into();
        self
    }

    /// Gets the source map from the diagnostics context.
    fn get_source_map(&mut self) -> Arc<SourceMap> {
        self.source_map.get_or_insert_default().clone()
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
        // Set the source map from the diagnostics context if it's not set.
        let dcx = self.dcx.as_mut().unwrap_or_else(|| panic!("diagnostics context not set"));
        if self.source_map.is_none() {
            self.source_map = dcx.source_map_mut().cloned();
        }

        let mut sess = self.try_build().unwrap();
        if let Some(sm) = sess.dcx.source_map_mut() {
            assert!(
                Arc::ptr_eq(&sess.source_map, sm),
                "session source map does not match the one in the diagnostics context"
            );
        }
        sess
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
            && self.opts.input.iter().all(|arg| arg.extension() == Some("yul".as_ref()))
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
        &self.source_map
    }

    /// Clones the source map.
    #[inline]
    pub fn clone_source_map(&self) -> Arc<SourceMap> {
        self.source_map.clone()
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
        if self.is_sequential() {
            (oper_a(), oper_b())
        } else {
            rayon::join(oper_a, oper_b)
        }
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

    /// Sets up the thread pool and session globals if they doesn't exist already and then
    /// executes the given closure.
    ///
    /// This also calls [`SessionGlobals::with_source_map`].
    #[inline]
    pub fn enter<R: Send>(&self, f: impl FnOnce() -> R + Send) -> R {
        SessionGlobals::with_or_default(|session_globals| {
            SessionGlobals::with_source_map(self.clone_source_map(), || {
                run_in_thread_pool_with_globals(self.threads(), session_globals, f)
            })
        })
    }
}

/// Runs the given closure in a thread pool with the given number of threads.
fn run_in_thread_pool_with_globals<R: Send>(
    threads: usize,
    session_globals: &SessionGlobals,
    f: impl FnOnce() -> R + Send,
) -> R {
    // Avoid panicking below if this is a recursive call.
    if rayon::current_thread_index().is_some() {
        return f();
    }

    let mut builder =
        rayon::ThreadPoolBuilder::new().thread_name(|i| format!("solar-{i}")).num_threads(threads);
    // We still want to use a rayon thread pool with 1 thread so that `ParallelIterator`s don't
    // install and run in the default global thread pool.
    if threads == 1 {
        builder = builder.use_current_thread();
    }
    builder
        .build_scoped(
            // Initialize each new worker thread when created.
            // Note that this is not called on the current thread, so `set` can't panic.
            move |thread| session_globals.set(|| thread.run()),
            // Run `f` on the first thread in the thread pool.
            move |pool| pool.install(f),
        )
        .unwrap()
}

#[cfg(test)]
mod tests {
    use super::*;

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
            let s = format!("{span:?}");
            assert!(!s.contains("Span("), "{s}");
            let s = format!("{span:#?}");
            assert!(!s.contains("Span("), "{s}");
        }

        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.enter(use_globals);
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());
        sess.enter(|| {
            use_globals();
            sess.enter(use_globals);
            use_globals();
        });
        assert!(sess.dcx.emitted_diagnostics().unwrap().is_empty());
        assert!(sess.dcx.emitted_errors().unwrap().is_ok());

        SessionGlobals::new().set(|| {
            use_globals_no_sm();
            sess.enter(|| {
                use_globals();
                sess.enter(use_globals);
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
}

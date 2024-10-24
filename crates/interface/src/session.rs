use crate::{
    diagnostics::{DiagCtxt, EmittedDiagnostics},
    ColorChoice, SessionGlobals, SourceMap,
};
use solar_config::{CompilerOutput, CompilerStage, Dump, EvmVersion, Language};
use std::{collections::BTreeSet, num::NonZeroUsize, path::PathBuf, sync::Arc};

/// Information about the current compiler session.
#[derive(derive_builder::Builder)]
#[builder(
    pattern = "owned",
    build_fn(name = "try_build", private, error = "SessionBuilderError"),
    setter(strip_option)
)]
pub struct Session {
    /// The diagnostics context.
    pub dcx: DiagCtxt,
    /// The source map.
    #[builder(default)]
    source_map: Arc<SourceMap>,

    /// EVM version.
    #[builder(default)]
    pub evm_version: EvmVersion,
    /// Source code language.
    #[builder(default)]
    pub language: Language,
    /// Stop execution after the given compiler stage.
    #[builder(default)]
    pub stop_after: Option<CompilerStage>,
    /// Types of output to emit.
    #[builder(default)]
    pub emit: BTreeSet<CompilerOutput>,
    /// Output directory.
    #[builder(default)]
    pub out_dir: Option<PathBuf>,
    /// Internal state to dump to stdout.
    #[builder(default)]
    pub dump: Option<Dump>,
    /// Pretty-print any JSON output.
    #[builder(default)]
    pub pretty_json: bool,
    /// Number of threads to use. Already resolved to a non-zero value.
    #[builder(default = "NonZeroUsize::MIN")]
    pub jobs: NonZeroUsize,
}

#[derive(Debug)]
struct SessionBuilderError;
impl From<derive_builder::UninitializedFieldError> for SessionBuilderError {
    fn from(_value: derive_builder::UninitializedFieldError) -> Self {
        Self
    }
}

impl SessionBuilder {
    /// Sets the diagnostic context to a test emitter.
    #[inline]
    pub fn with_test_emitter(self) -> Self {
        self.dcx(DiagCtxt::with_test_emitter())
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

    /// Gets the source map from the diagnostics context.
    fn get_source_map(&mut self) -> Arc<SourceMap> {
        self.source_map.get_or_insert_with(Default::default).clone()
    }

    /// Consumes the builder to create a new session.
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

    /// Returns `Err` with the printed diagnostics if any errors have been emitted.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](SessionBuilder::with_buffer_emitter).
    #[inline]
    pub fn emitted_diagnostics(&self) -> Option<Result<(), EmittedDiagnostics>> {
        self.dcx.emitted_diagnostics()
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
        self.stop_after >= Some(stage)
    }

    /// Returns `true` if parallelism is not enabled.
    #[inline]
    pub fn is_sequential(&self) -> bool {
        self.jobs.get() == 1
    }

    /// Returns `true` if parallelism is enabled.
    #[inline]
    pub fn is_parallel(&self) -> bool {
        !self.is_sequential()
    }

    /// Returns `true` if the given output should be emitted.
    #[inline]
    pub fn do_emit(&self, output: CompilerOutput) -> bool {
        self.emit.contains(&output)
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

    /// Sets up session globals on the current thread if they doesn't exist already and then
    /// executes the given closure.
    ///
    /// This also calls [`SessionGlobals::with_source_map`].
    #[inline]
    pub fn enter<R>(&self, f: impl FnOnce() -> R) -> R {
        SessionGlobals::with_or_default(|_| {
            SessionGlobals::with_source_map(self.clone_source_map(), f)
        })
    }
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
        let sess = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        sess.dcx.err("test").emit();
        let err = sess.dcx.emitted_diagnostics().unwrap().unwrap_err();
        let err = Box::new(err) as Box<dyn std::error::Error>;
        assert!(err.to_string().contains("error: test"), "{err:?}");
    }
}

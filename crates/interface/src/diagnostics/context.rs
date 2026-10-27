//! Diagnostic collection, deduplication, counting, and emission.
//!
//! Modified from rustc's [`DiagCtxt`](https://github.com/rust-lang/rust/blob/3b58636b30eb364ac72aeaf03d46347084ed87d1/compiler/rustc_errors/src/lib.rs).

use super::{
    BugAbort, Diag, DiagBuilder, DiagMsg, DynEmitter, EmissionGuarantee, EmittedDiagnostics,
    ErrorGuaranteed, FatalAbort, HumanBufferEmitter, Level, MultiSpan, SilentEmitter,
    emitter::HumanEmitter,
};
use crate::{Result, SourceMap, Span};
use anstream::ColorChoice;
use rayon::prelude::*;
use solar_config::{CompileOpts, ErrorFormat};
use solar_data_structures::{map::FxHashSet, sync::Mutex};
use std::{borrow::Cow, cell::RefCell, fmt, hash::BuildHasher, num::NonZeroUsize, sync::Arc};

thread_local! {
    /// The diagnostics deferred by the innermost [`DiagCtxt::defer`] running on this thread.
    static DEFERRED: RefCell<Option<Deferral>> = const { RefCell::new(None) };
}

struct Deferral {
    dcx: *const DiagCtxt,
    diagnostics: Vec<Diag>,
}

/// Diagnostics withheld by [`DiagCtxt::defer`], to be emitted with [`DiagCtxt::emit_deferred`].
#[derive(Debug, Default)]
#[must_use = "deferred diagnostics are lost unless passed to `DiagCtxt::emit_deferred`"]
pub struct DeferredDiagnostics(Vec<Diag>);

impl DeferredDiagnostics {
    /// Moves the diagnostics of `other` after these.
    pub fn append(&mut self, mut other: Self) {
        self.0.append(&mut other.0);
    }

    /// Returns whether there are no diagnostics.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Stably sorts the diagnostics by the key `f` computes for each.
    pub fn sort_by_cached_key<K: Ord>(&mut self, f: impl FnMut(&Diag) -> K) {
        self.0.sort_by_cached_key(f);
    }
}

/// Flags that control the behaviour of a [`DiagCtxt`].
#[derive(Clone, Copy, Debug)]
pub struct DiagCtxtFlags {
    /// If false, warning-level lints are suppressed.
    pub can_emit_warnings: bool,
    /// If Some, the Nth error-level diagnostic is upgraded to bug-level.
    pub treat_err_as_bug: Option<NonZeroUsize>,
    /// If true, identical diagnostics are reported only once.
    pub deduplicate_diagnostics: bool,
    /// Track where errors are created. Enabled with `-Ztrack-diagnostics`, and by default in debug
    /// builds.
    pub track_diagnostics: bool,
}

impl Default for DiagCtxtFlags {
    fn default() -> Self {
        Self {
            can_emit_warnings: true,
            treat_err_as_bug: None,
            deduplicate_diagnostics: true,
            track_diagnostics: cfg!(debug_assertions),
        }
    }
}

impl DiagCtxtFlags {
    /// Updates the flags from the given options.
    ///
    /// Looks at the following options:
    /// - `unstable.ui_testing`
    /// - `unstable.track_diagnostics`
    /// - `no_warnings`
    pub fn update_from_opts(&mut self, opts: &CompileOpts) {
        self.deduplicate_diagnostics &= !opts.unstable.ui_testing;
        self.track_diagnostics &= !opts.unstable.ui_testing;
        self.track_diagnostics |= opts.unstable.track_diagnostics;
        self.can_emit_warnings &= !opts.no_warnings;
    }
}

/// A handler that deals with errors and other compiler output.
///
/// Certain errors (fatal, bug) may cause immediate exit, others log errors for later reporting.
pub struct DiagCtxt {
    inner: Mutex<DiagCtxtInner>,
}

impl fmt::Debug for DiagCtxt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut s = f.debug_struct("DiagCtxt");
        if let Some(inner) = self.inner.try_lock() {
            s.field("flags", &inner.flags)
                .field("err_count", &inner.err_count)
                .field("warn_count", &inner.warn_count)
                .field("note_count", &inner.note_count);
        } else {
            s.field("inner", &format_args!("<locked>"));
        }
        s.finish_non_exhaustive()
    }
}

struct DiagCtxtInner {
    emitter: Box<DynEmitter>,

    flags: DiagCtxtFlags,
    allowed_diagnostic_codes: FxHashSet<String>,

    /// The number of errors that have been emitted, including duplicates.
    ///
    /// This is not necessarily the count that's reported to the user once
    /// compilation ends.
    err_count: usize,
    deduplicated_err_count: usize,
    /// The warning count, used for a recap upon finishing
    warn_count: usize,
    deduplicated_warn_count: usize,
    /// The note count, used for a recap upon finishing
    note_count: usize,
    deduplicated_note_count: usize,

    /// This set contains a hash of every diagnostic that has been emitted by this `DiagCtxt`.
    /// These hashes are used to avoid emitting the same error twice.
    emitted_diagnostics: FxHashSet<u64>,
}

impl DiagCtxt {
    /// Creates a new `DiagCtxt` with the given diagnostics emitter.
    pub fn new(emitter: Box<DynEmitter>) -> Self {
        Self {
            inner: Mutex::new(DiagCtxtInner {
                emitter,
                flags: DiagCtxtFlags::default(),
                allowed_diagnostic_codes: FxHashSet::default(),
                err_count: 0,
                deduplicated_err_count: 0,
                warn_count: 0,
                deduplicated_warn_count: 0,
                note_count: 0,
                deduplicated_note_count: 0,
                emitted_diagnostics: FxHashSet::default(),
            }),
        }
    }

    /// Creates a new `DiagCtxt` with a stderr emitter for emitting one-off/early fatal errors that
    /// contain no source information.
    pub fn new_early() -> Self {
        Self::with_stderr_emitter(None).with_flags(|flags| flags.track_diagnostics = false)
    }

    /// Creates a new `DiagCtxt` with a test emitter.
    pub fn with_test_emitter(source_map: Option<Arc<SourceMap>>) -> Self {
        Self::new(Box::new(HumanEmitter::test().source_map(source_map)))
    }

    /// Creates a new `DiagCtxt` with a stderr emitter.
    pub fn with_stderr_emitter(source_map: Option<Arc<SourceMap>>) -> Self {
        Self::with_stderr_emitter_and_color(source_map, ColorChoice::Auto)
    }

    /// Creates a new `DiagCtxt` with a stderr emitter and a color choice.
    pub fn with_stderr_emitter_and_color(
        source_map: Option<Arc<SourceMap>>,
        color_choice: ColorChoice,
    ) -> Self {
        Self::new(Box::new(HumanEmitter::stderr(color_choice).source_map(source_map)))
    }

    /// Creates a new `DiagCtxt` with a silent emitter.
    ///
    /// Fatal diagnostics will still be emitted, optionally with the given note.
    pub fn with_silent_emitter(fatal_note: Option<String>) -> Self {
        let fatal_emitter = HumanEmitter::stderr(Default::default());
        Self::new(Box::new(SilentEmitter::new(fatal_emitter).with_note(fatal_note)))
            .disable_warnings()
    }

    /// Creates a new `DiagCtxt` with a human emitter that emits diagnostics to a local buffer.
    pub fn with_buffer_emitter(
        source_map: Option<Arc<SourceMap>>,
        color_choice: ColorChoice,
    ) -> Self {
        Self::new(Box::new(HumanBufferEmitter::new(color_choice).source_map(source_map)))
    }

    /// Creates a new `DiagCtxt` from the given options.
    ///
    /// This is the default `DiagCtxt` used by the `Session` if one is not provided manually.
    /// It looks at the following options:
    /// - `error_format`
    /// - `color`
    /// - `unstable.ui_testing`
    /// - `unstable.track_diagnostics`
    /// - `no_warnings`
    /// - `error_format_human`
    /// - `diagnostic_width`
    ///
    /// The default is human emitter to stderr.
    ///
    /// See also [`DiagCtxtFlags::update_from_opts`].
    pub fn from_opts(opts: &solar_config::CompileOpts) -> Self {
        let source_map = Arc::new(SourceMap::empty());
        let emitter: Box<DynEmitter> = match opts.error_format {
            ErrorFormat::Human => {
                let human = HumanEmitter::stderr(opts.color)
                    .source_map(Some(source_map))
                    .ui_testing(opts.unstable.ui_testing)
                    .human_kind(opts.error_format_human)
                    .terminal_width(opts.diagnostic_width);
                Box::new(human)
            }
            #[cfg(feature = "json")]
            ErrorFormat::Json | ErrorFormat::RustcJson => {
                // `io::Stderr` is not buffered.
                let writer = Box::new(std::io::BufWriter::new(std::io::stderr()));
                let json = crate::diagnostics::JsonEmitter::new(writer, source_map, opts.color)
                    .pretty(opts.pretty_json_err)
                    .rustc_like(matches!(opts.error_format, ErrorFormat::RustcJson))
                    .ui_testing(opts.unstable.ui_testing)
                    .human_kind(opts.error_format_human)
                    .terminal_width(opts.diagnostic_width);
                Box::new(json)
            }
            format => unimplemented!("{format:?}"),
        };
        Self::new(emitter)
            .with_flags(|flags| flags.update_from_opts(opts))
            .with_allowed_diagnostic_codes(opts.allow.iter().cloned())
    }

    /// Adds diagnostic codes that should be allowed.
    pub fn with_allowed_diagnostic_codes(
        mut self,
        codes: impl IntoIterator<Item = String>,
    ) -> Self {
        self.set_allowed_diagnostic_codes_mut(codes);
        self
    }

    /// Sets the emitter to [`SilentEmitter`].
    pub fn make_silent(&self, fatal_note: Option<String>, emit_fatal: bool) {
        self.wrap_emitter(|prev| {
            Box::new(SilentEmitter::new_boxed(emit_fatal.then_some(prev)).with_note(fatal_note))
        });
    }

    /// Sets the inner emitter. Returns the previous emitter.
    pub fn set_emitter(&self, emitter: Box<DynEmitter>) -> Box<DynEmitter> {
        std::mem::replace(&mut self.inner.lock().emitter, emitter)
    }

    /// Wraps the current emitter with the given closure.
    pub fn wrap_emitter(&self, f: impl FnOnce(Box<DynEmitter>) -> Box<DynEmitter>) {
        struct FakeEmitter;
        impl crate::diagnostics::Emitter for FakeEmitter {
            fn emit_diagnostic(&mut self, _diagnostic: &mut Diag) {}
        }

        let mut inner = self.inner.lock();
        let prev = std::mem::replace(&mut inner.emitter, Box::new(FakeEmitter));
        inner.emitter = f(prev);
    }

    /// Gets the source map associated with this context.
    pub fn source_map(&self) -> Option<Arc<SourceMap>> {
        self.inner.lock().emitter.source_map().cloned()
    }

    /// Gets the source map associated with this context.
    pub fn source_map_mut(&mut self) -> Option<&Arc<SourceMap>> {
        self.inner.get_mut().emitter.source_map()
    }

    /// Sets flags.
    pub fn with_flags(mut self, f: impl FnOnce(&mut DiagCtxtFlags)) -> Self {
        self.set_flags_mut(f);
        self
    }

    /// Sets flags.
    pub fn set_flags(&self, f: impl FnOnce(&mut DiagCtxtFlags)) {
        f(&mut self.inner.lock().flags);
    }

    /// Sets flags.
    pub fn set_flags_mut(&mut self, f: impl FnOnce(&mut DiagCtxtFlags)) {
        f(&mut self.inner.get_mut().flags);
    }

    /// Adds diagnostic codes that should be allowed.
    pub fn set_allowed_diagnostic_codes(&self, codes: impl IntoIterator<Item = String>) {
        self.inner.lock().allowed_diagnostic_codes.extend(codes);
    }

    /// Adds diagnostic codes that should be allowed.
    pub fn set_allowed_diagnostic_codes_mut(&mut self, codes: impl IntoIterator<Item = String>) {
        self.inner.get_mut().allowed_diagnostic_codes.extend(codes);
    }

    /// Disables emitting warnings.
    pub fn disable_warnings(self) -> Self {
        self.with_flags(|f| f.can_emit_warnings = false)
    }

    /// Returns whether warning diagnostics can be emitted.
    pub fn can_emit_warnings(&self) -> bool {
        self.inner.lock().flags.can_emit_warnings
    }

    /// Resets diagnostic counts and the deduplication cache.
    ///
    /// This is intended for tools that reuse a diagnostic context for multiple independent runs.
    pub fn reset_err_count(&self) {
        let mut inner = self.inner.lock();
        inner.err_count = 0;
        inner.deduplicated_err_count = 0;
        inner.warn_count = 0;
        inner.deduplicated_warn_count = 0;
        inner.note_count = 0;
        inner.deduplicated_note_count = 0;
        inner.emitted_diagnostics.clear();
    }

    /// Returns `true` if diagnostics are being tracked.
    pub fn track_diagnostics(&self) -> bool {
        self.inner.lock().flags.track_diagnostics
    }

    /// Emits the given diagnostic with this context.
    #[inline]
    pub fn emit_diagnostic(&self, mut diagnostic: Diag) -> Result<(), ErrorGuaranteed> {
        self.emit_diagnostic_without_consuming(&mut diagnostic)
    }

    /// Emits the given diagnostic with this context, without consuming the diagnostic.
    ///
    /// **Note:** This function is intended to be used only internally in `DiagBuilder`.
    /// Use [`emit_diagnostic`](Self::emit_diagnostic) instead.
    pub(super) fn emit_diagnostic_without_consuming(
        &self,
        diagnostic: &mut Diag,
    ) -> Result<(), ErrorGuaranteed> {
        DEFERRED.with_borrow_mut(|deferral| {
            let deferred = deferral
                .as_mut()
                .filter(|deferral| std::ptr::eq(deferral.dcx, self))
                .map(|deferral| &mut deferral.diagnostics);
            self.inner.lock().emit_diagnostic_without_consuming(diagnostic, deferred)
        })
    }

    /// Runs `f`, withholding the diagnostics that it emits on this thread.
    ///
    /// The withheld diagnostics count towards [`err_count`](Self::err_count) and the other counts
    /// at once, but reach the emitter only through [`emit_deferred`](Self::emit_deferred), which
    /// also deduplicates them. Calls nest, and diagnostics emitted by work that `f` hands to other
    /// threads are not withheld.
    pub fn defer<R>(&self, f: impl FnOnce() -> R) -> (R, DeferredDiagnostics) {
        /// Restores the enclosing deferral. If `f` unwinds, it first emits the diagnostics that it
        /// withheld, so that they are not lost to the panic.
        struct Restore<'a> {
            dcx: &'a DiagCtxt,
            outer: Option<Deferral>,
        }
        impl Drop for Restore<'_> {
            fn drop(&mut self) {
                let current = DEFERRED.replace(self.outer.take());
                if std::thread::panicking()
                    && let Some(current) = current
                {
                    self.dcx.emit_deferred(DeferredDiagnostics(current.diagnostics));
                }
            }
        }

        let outer = DEFERRED.replace(Some(Deferral { dcx: self, diagnostics: vec![] }));
        let restore = Restore { dcx: self, outer };
        let result = f();
        let deferral = DEFERRED.take();
        drop(restore);
        (
            result,
            DeferredDiagnostics(deferral.map(|deferral| deferral.diagnostics).unwrap_or_default()),
        )
    }

    /// Emits the diagnostics withheld by [`defer`](Self::defer), in the order they were emitted.
    ///
    /// Within another [`defer`](Self::defer), they join its diagnostics instead.
    pub fn emit_deferred(&self, deferred: DeferredDiagnostics) {
        if deferred.0.is_empty() {
            return;
        }
        DEFERRED.with_borrow_mut(|deferral| {
            if let Some(deferral) =
                deferral.as_mut().filter(|deferral| std::ptr::eq(deferral.dcx, self))
            {
                deferral.diagnostics.extend(deferred.0);
                return;
            }
            let mut inner = self.inner.lock();
            for mut diagnostic in deferred.0 {
                inner.deliver_diagnostic(&mut diagnostic);
            }
        });
    }

    /// Runs `f` for every item of `iter` in parallel, and returns its results and the diagnostics
    /// of each call in the order of the items, as a sequential loop would emit them.
    ///
    /// `iter` must keep the order of its items when collected, as every Rayon iterator does.
    pub fn par_defer<I, R>(
        &self,
        iter: I,
        f: impl Fn(I::Item) -> R + Sync + Send,
    ) -> (Vec<R>, DeferredDiagnostics)
    where
        I: ParallelIterator,
        R: Send,
    {
        let outputs = iter.map(|item| self.defer(|| f(item))).collect::<Vec<_>>();
        let mut results = Vec::with_capacity(outputs.len());
        let mut deferred = DeferredDiagnostics::default();
        for (result, batch) in outputs {
            results.push(result);
            deferred.append(batch);
        }
        (results, deferred)
    }

    /// Runs `f` for every item of `iter` in parallel, and emits the diagnostics of each call in the
    /// order of the items. See [`par_defer`](Self::par_defer).
    pub fn par_for_each<I>(&self, iter: I, f: impl Fn(I::Item) + Sync + Send)
    where
        I: ParallelIterator,
    {
        self.emit_deferred(self.par_defer(iter, f).1);
    }

    /// Returns the number of errors that have been emitted, including duplicates.
    pub fn err_count(&self) -> usize {
        self.inner.lock().err_count
    }

    /// Returns `Err` if any errors have been emitted.
    pub fn has_errors(&self) -> Result<(), ErrorGuaranteed> {
        if self.inner.lock().has_errors() { Err(ErrorGuaranteed::new_unchecked()) } else { Ok(()) }
    }

    /// Returns the number of warnings that have been emitted, including duplicates.
    pub fn warn_count(&self) -> usize {
        self.inner.lock().warn_count
    }

    /// Returns the number of notes that have been emitted, including duplicates.
    pub fn note_count(&self) -> usize {
        self.inner.lock().note_count
    }

    /// Returns the emitted diagnostics as a result. Can be empty.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](Self::with_buffer_emitter).
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
        let inner = self.inner.lock();
        let diags = EmittedDiagnostics(inner.emitter.local_buffer()?.to_string());
        Some(if inner.has_errors() { Err(diags) } else { Ok(diags) })
    }

    /// Returns the emitted diagnostics. Can be empty.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](Self::with_buffer_emitter).
    pub fn emitted_diagnostics(&self) -> Option<EmittedDiagnostics> {
        let inner = self.inner.lock();
        Some(EmittedDiagnostics(inner.emitter.local_buffer()?.to_string()))
    }

    /// Returns `Err` with the printed diagnostics if any errors have been emitted.
    ///
    /// Returns `None` if the underlying emitter is not a human buffer emitter created with
    /// [`with_buffer_emitter`](Self::with_buffer_emitter).
    pub fn emitted_errors(&self) -> Option<Result<(), EmittedDiagnostics>> {
        let inner = self.inner.lock();
        let buffer = inner.emitter.local_buffer()?;
        Some(if inner.has_errors() { Err(EmittedDiagnostics(buffer.to_string())) } else { Ok(()) })
    }

    /// Emits a diagnostic if any warnings or errors have been emitted.
    pub fn print_error_count(&self) -> Result {
        self.inner.lock().print_error_count()
    }
}

/// Diag constructors.
///
/// Note that methods returning a [`DiagBuilder`] must also marked with `#[track_caller]`.
impl DiagCtxt {
    /// Creates a builder at the given `level` with the given `msg`.
    #[track_caller]
    pub fn diag<G: EmissionGuarantee>(
        &self,
        level: Level,
        msg: impl Into<DiagMsg>,
    ) -> DiagBuilder<'_, G> {
        DiagBuilder::new(self, level, msg)
    }

    /// Creates a builder at the `Bug` level with the given `msg`.
    #[track_caller]
    #[cold]
    pub fn bug(&self, msg: impl Into<DiagMsg>) -> DiagBuilder<'_, BugAbort> {
        self.diag(Level::Bug, msg)
    }

    /// Creates a builder at the `Fatal` level with the given `msg`.
    #[track_caller]
    #[cold]
    pub fn fatal(&self, msg: impl Into<DiagMsg>) -> DiagBuilder<'_, FatalAbort> {
        self.diag(Level::Fatal, msg)
    }

    /// Creates a builder at the `Error` level with the given `msg`.
    #[track_caller]
    pub fn err(&self, msg: impl Into<DiagMsg>) -> DiagBuilder<'_, ErrorGuaranteed> {
        self.diag(Level::Error, msg)
    }

    /// Emits an error at `span` with the given `msg`.
    #[track_caller]
    pub fn emit_err(&self, span: impl Into<MultiSpan>, msg: impl Into<DiagMsg>) -> ErrorGuaranteed {
        self.err(msg).span(span).emit()
    }

    /// Emits an error at `span` with an attached note.
    #[track_caller]
    pub fn emit_err_note(
        &self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagMsg>,
        note: impl Into<DiagMsg>,
    ) -> ErrorGuaranteed {
        self.err(msg).span(span).note(note).emit()
    }

    /// Emits an error at `span` with an attached span note.
    #[track_caller]
    pub fn emit_err_span_note(
        &self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagMsg>,
        note_span: impl Into<MultiSpan>,
        note: impl Into<DiagMsg>,
    ) -> ErrorGuaranteed {
        self.err(msg).span(span).span_note(note_span, note).emit()
    }

    /// Emits an error at `span` with an attached span label.
    #[track_caller]
    pub fn emit_err_label(
        &self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagMsg>,
        label_span: Span,
        label: impl Into<DiagMsg>,
    ) -> ErrorGuaranteed {
        self.err(msg).span(span).span_label(label_span, label).emit()
    }

    /// Creates a builder at the `Warning` level with the given `msg`.
    ///
    /// Attempting to `.emit()` the builder will only emit if `can_emit_warnings` is `true`.
    #[track_caller]
    pub fn warn(&self, msg: impl Into<DiagMsg>) -> DiagBuilder<'_, ()> {
        self.diag(Level::Warning, msg)
    }

    /// Creates a builder at the `Help` level with the given `msg`.
    #[track_caller]
    pub fn help(&self, msg: impl Into<DiagMsg>) -> DiagBuilder<'_, ()> {
        self.diag(Level::Help, msg)
    }

    /// Creates a builder at the `Note` level with the given `msg`.
    #[track_caller]
    pub fn note(&self, msg: impl Into<DiagMsg>) -> DiagBuilder<'_, ()> {
        self.diag(Level::Note, msg)
    }
}

impl DiagCtxtInner {
    fn emit_diagnostic(&mut self, mut diagnostic: Diag) -> Result<(), ErrorGuaranteed> {
        self.emit_diagnostic_without_consuming(&mut diagnostic, None)
    }

    /// Counts the diagnostic, and emits it, or pushes it to `deferred` if given.
    fn emit_diagnostic_without_consuming(
        &mut self,
        diagnostic: &mut Diag,
        deferred: Option<&mut Vec<Diag>>,
    ) -> Result<(), ErrorGuaranteed> {
        if self.is_allowed_diagnostic(diagnostic) {
            return Ok(());
        }

        if diagnostic.level == Level::Warning && !self.flags.can_emit_warnings {
            return Ok(());
        }

        if diagnostic.level == Level::Allow {
            return Ok(());
        }

        if matches!(diagnostic.level, Level::Error | Level::Fatal) && self.treat_next_err_as_bug() {
            diagnostic.level = Level::Bug;
        }

        match deferred {
            Some(deferred) if diagnostic.level != Level::Bug => deferred.push(diagnostic.clone()),
            deferred => {
                // A bug panics when counted, so it is emitted first, after the diagnostics that
                // precede it.
                for mut withheld in deferred.into_iter().flat_map(|deferred| deferred.drain(..)) {
                    self.deliver_diagnostic(&mut withheld);
                }
                self.deliver_diagnostic(diagnostic);
            }
        }

        if diagnostic.is_error() {
            self.bump_err_count();
            Err(ErrorGuaranteed::new_unchecked())
        } else {
            if diagnostic.level == Level::Warning {
                self.bump_warn_count();
            } else if diagnostic.is_note() {
                self.bump_note_count();
            }
            // Don't bump any counters for `Help`, `OnceHelp`, or `Allow`
            Ok(())
        }
    }

    /// Emits the diagnostic unless it duplicates one that was emitted.
    fn deliver_diagnostic(&mut self, diagnostic: &mut Diag) {
        let already_emitted = self.insert_diagnostic(diagnostic);
        if self.flags.deduplicate_diagnostics && already_emitted {
            return;
        }

        // Remove duplicate `Once*` subdiagnostics.
        diagnostic.children.retain(|sub| {
            if !matches!(sub.level, Level::OnceNote | Level::OnceHelp) {
                return true;
            }
            let sub_already_emitted = self.insert_diagnostic(sub);
            !sub_already_emitted
        });

        // Unlike rustc, deduplication is only disabled internally for UI testing, so do not
        // attach rustc's `-Z deduplicate-diagnostics=no` note.

        self.emitter.emit_diagnostic(diagnostic);
        if diagnostic.is_error() {
            self.deduplicated_err_count += 1;
        } else if diagnostic.level == Level::Warning {
            self.deduplicated_warn_count += 1;
        } else if diagnostic.is_note() {
            self.deduplicated_note_count += 1;
        }
    }

    fn print_error_count(&mut self) -> Result {
        // self.emit_stashed_diagnostics();

        if self.treat_err_as_bug() {
            return Ok(());
        }

        let errors = match self.deduplicated_err_count {
            0 => None,
            1 => Some(Cow::from("aborting due to 1 previous error")),
            count => Some(Cow::from(format!("aborting due to {count} previous errors"))),
        };

        let mut others = Vec::with_capacity(2);
        match self.deduplicated_warn_count {
            1 => others.push(Cow::from("1 warning emitted")),
            count if count > 1 => others.push(Cow::from(format!("{count} warnings emitted"))),
            _ => {}
        }
        match self.deduplicated_note_count {
            1 => others.push(Cow::from("1 note emitted")),
            count if count > 1 => others.push(Cow::from(format!("{count} notes emitted"))),
            _ => {}
        }

        match (errors, others.is_empty()) {
            (None, true) => Ok(()),
            (None, false) => {
                // TODO: Don't emit in tests since it's not handled by `ui_test`: https://github.com/oli-obk/ui_test/issues/324
                if self.flags.track_diagnostics {
                    let msg = others.join(", ");
                    self.emitter.emit_diagnostic(&mut Diag::new(Level::Warning, msg));
                }
                Ok(())
            }
            (Some(e), true) => self.emit_diagnostic(Diag::new(Level::Error, e)),
            (Some(e), false) => self
                .emit_diagnostic(Diag::new(Level::Error, format!("{}; {}", e, others.join(", ")))),
        }
    }

    /// Inserts the given diagnostic into the set of emitted diagnostics.
    /// Returns `true` if the diagnostic was already emitted.
    fn insert_diagnostic<H: std::hash::Hash>(&mut self, diag: &H) -> bool {
        let hash = solar_data_structures::map::rustc_hash::FxBuildHasher.hash_one(diag);
        !self.emitted_diagnostics.insert(hash)
    }

    fn treat_err_as_bug(&self) -> bool {
        self.flags.treat_err_as_bug.is_some_and(|c| self.err_count >= c.get())
    }

    /// Use before incrementing `err_count`.
    fn treat_next_err_as_bug(&self) -> bool {
        self.flags.treat_err_as_bug.is_some_and(|c| self.err_count + 1 >= c.get())
    }

    fn is_allowed_diagnostic(&self, diagnostic: &Diag) -> bool {
        diagnostic.level == Level::Warning
            && diagnostic.id().is_some_and(|id| self.allowed_diagnostic_codes.contains(id))
    }

    fn bump_err_count(&mut self) {
        self.err_count += 1;
        self.panic_if_treat_err_as_bug();
    }

    fn bump_warn_count(&mut self) {
        self.warn_count += 1;
    }

    fn bump_note_count(&mut self) {
        self.note_count += 1;
    }

    fn has_errors(&self) -> bool {
        self.err_count > 0
    }

    fn panic_if_treat_err_as_bug(&self) {
        if self.treat_err_as_bug() {
            match (self.err_count, self.flags.treat_err_as_bug.unwrap().get()) {
                (1, 1) => panic!("aborting due to `-Z treat-err-as-bug=1`"),
                (count, val) => {
                    panic!("aborting after {count} errors due to `-Z treat-err-as-bug={val}`")
                }
            }
        }
    }
}

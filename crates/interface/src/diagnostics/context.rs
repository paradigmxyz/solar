use super::{
    emitter::HumanEmitter, BugAbort, Diagnostic, DiagnosticBuilder, DiagnosticMessage, DynEmitter,
    EmissionGuarantee, ErrorGuaranteed, Level, SilentEmitter,
};
use crate::{Result, SourceMap};
use anstream::ColorChoice;
use std::{borrow::Cow, num::NonZeroUsize, sync::Arc};
use sulk_data_structures::{map::FxHashSet, sync::Lock};

/// Flags that control the behaviour of a [`DiagCtxt`].
#[derive(Clone, Copy)]
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

/// A handler deals with errors and other compiler output.
/// Certain errors (fatal, bug, unimpl) may cause immediate exit,
/// others log errors for later reporting.
pub struct DiagCtxt {
    inner: Lock<DiagCtxtInner>,
}

struct DiagCtxtInner {
    emitter: Box<DynEmitter>,

    flags: DiagCtxtFlags,

    /// The number of errors that have been emitted, including duplicates.
    ///
    /// This is not necessarily the count that's reported to the user once
    /// compilation ends.
    err_count: usize,
    deduplicated_err_count: usize,
    warn_count: usize,
    /// The warning count, used for a recap upon finishing
    deduplicated_warn_count: usize,

    /// This set contains a hash of every diagnostic that has been emitted by this `DiagCtxt`.
    /// These hashes are used to avoid emitting the same error twice.
    emitted_diagnostics: FxHashSet<u64>,
}

impl DiagCtxt {
    /// Creates a new `DiagCtxt` with the given diagnostics emitter.
    pub fn new(emitter: Box<DynEmitter>) -> Self {
        Self {
            inner: Lock::new(DiagCtxtInner {
                emitter,
                flags: DiagCtxtFlags::default(),
                err_count: 0,
                deduplicated_err_count: 0,
                warn_count: 0,
                deduplicated_warn_count: 0,
                emitted_diagnostics: FxHashSet::default(),
            }),
        }
    }

    /// Creates a new `DiagCtxt` with a test emitter.
    pub fn with_test_emitter() -> Self {
        Self::new(Box::new(HumanEmitter::test()))
    }

    /// Creates a new `DiagCtxt` with a TTY emitter.
    pub fn with_tty_emitter(source_map: Option<Arc<SourceMap>>) -> Self {
        Self::with_tty_emitter_and_color(source_map, ColorChoice::Auto)
    }

    /// Creates a new `DiagCtxt` with a TTY emitter and a color choice.
    pub fn with_tty_emitter_and_color(
        source_map: Option<Arc<SourceMap>>,
        color_choice: ColorChoice,
    ) -> Self {
        Self::new(Box::new(HumanEmitter::stderr(color_choice).source_map(source_map)))
    }

    /// Creates a new `DiagCtxt` with a silent emitter.
    pub fn with_silent_emitter(fatal_note: Option<String>) -> Self {
        let fatal_dcx = Self::with_tty_emitter(None).disable_warnings();
        Self::new(Box::new(SilentEmitter::new(fatal_dcx).with_note(fatal_note))).disable_warnings()
    }

    /// Sets whether to include created and emitted locations in diagnostics.
    pub fn set_flags(mut self, f: impl FnOnce(&mut DiagCtxtFlags)) -> Self {
        f(&mut self.inner.get_mut().flags);
        self
    }

    /// Disables emitting warnings.
    pub fn disable_warnings(self) -> Self {
        self.set_flags(|f| f.can_emit_warnings = false)
    }

    /// Returns `true` if diagnostics are being tracked.
    pub fn track_diagnostics(&self) -> bool {
        self.inner.lock().flags.track_diagnostics
    }

    /// Emits the given diagnostic with this context.
    #[inline]
    pub fn emit_diagnostic(&self, mut diagnostic: Diagnostic) -> Result<(), ErrorGuaranteed> {
        self.emit_diagnostic_without_consuming(&mut diagnostic)
    }

    /// Emits the given diagnostic with this context, without consuming the diagnostic.
    ///
    /// **Note:** This function is intended to be used only internally in `DiagnosticBuilder`.
    /// Use [`emit_diagnostic`](Self::emit_diagnostic) instead.
    pub(super) fn emit_diagnostic_without_consuming(
        &self,
        diagnostic: &mut Diagnostic,
    ) -> Result<(), ErrorGuaranteed> {
        self.inner.lock().emit_diagnostic_without_consuming(diagnostic)
    }

    /// Returns the number of errors that have been emitted, including duplicates.
    #[inline]
    pub fn err_count(&self) -> usize {
        self.inner.lock().err_count
    }

    /// Returns `Err` if any errors have been emitted.
    pub fn has_errors(&self) -> Result<(), ErrorGuaranteed> {
        if self.inner.lock().has_errors() {
            #[allow(deprecated)]
            Err(ErrorGuaranteed::new_unchecked())
        } else {
            Ok(())
        }
    }

    /// Emits a diagnostic if any warnings or errors have been emitted.
    pub fn print_error_count(&self) -> Result {
        self.inner.lock().print_error_count()
    }
}

/// Diagnostic constructors.
///
/// Note that methods returning a [`DiagnosticBuilder`] must also marked with `#[track_caller]`.
impl DiagCtxt {
    /// Creates a builder at the given `level` with the given `msg`.
    #[track_caller]
    pub fn diag<G: EmissionGuarantee>(
        &self,
        level: Level,
        msg: impl Into<DiagnosticMessage>,
    ) -> DiagnosticBuilder<'_, G> {
        DiagnosticBuilder::new(self, level, msg)
    }

    /// Creates a builder at the `Bug` level with the given `msg`.
    #[track_caller]
    pub fn bug(&self, msg: impl Into<DiagnosticMessage>) -> DiagnosticBuilder<'_, BugAbort> {
        self.diag(Level::Bug, msg)
    }

    /// Creates a builder at the `Error` level with the given `msg`.
    #[track_caller]
    pub fn err(&self, msg: impl Into<DiagnosticMessage>) -> DiagnosticBuilder<'_, ErrorGuaranteed> {
        self.diag(Level::Error, msg)
    }

    /// Creates a builder at the `Warning` level with the given `msg`.
    ///
    /// Attempting to `.emit()` the builder will only emit if `can_emit_warnings` is `true`.
    #[track_caller]
    pub fn warn(&self, msg: impl Into<DiagnosticMessage>) -> DiagnosticBuilder<'_, ()> {
        self.diag(Level::Warning, msg)
    }

    /// Creates a builder at the `Help` level with the given `msg`.
    #[track_caller]
    pub fn help(&self, msg: impl Into<DiagnosticMessage>) -> DiagnosticBuilder<'_, ()> {
        self.diag(Level::Help, msg)
    }

    /// Creates a builder at the `Note` level with the given `msg`.
    #[track_caller]
    pub fn note(&self, msg: impl Into<DiagnosticMessage>) -> DiagnosticBuilder<'_, ()> {
        self.diag(Level::Note, msg)
    }
}

impl DiagCtxtInner {
    fn emit_diagnostic(&mut self, mut diagnostic: Diagnostic) -> Result<(), ErrorGuaranteed> {
        self.emit_diagnostic_without_consuming(&mut diagnostic)
    }

    fn emit_diagnostic_without_consuming(
        &mut self,
        diagnostic: &mut Diagnostic,
    ) -> Result<(), ErrorGuaranteed> {
        if diagnostic.level == Level::Warning && !self.flags.can_emit_warnings {
            return Ok(());
        }

        if diagnostic.level == Level::Allow {
            return Ok(());
        }

        if matches!(diagnostic.level, Level::Error) && self.treat_err_as_bug() {
            diagnostic.level = Level::Bug;
        }

        let already_emitted = self.insert_diagnostic(diagnostic);
        if !(self.flags.deduplicate_diagnostics && already_emitted) {
            // Remove duplicate `Once*` subdiagnostics.
            diagnostic.children.retain(|sub| {
                if !matches!(sub.level, Level::OnceNote | Level::OnceHelp) {
                    return true;
                }
                let sub_already_emitted = self.insert_diagnostic(sub);
                !sub_already_emitted
            });

            // if already_emitted {
            //     diagnostic.note(
            //         "duplicate diagnostic emitted due to `-Z deduplicate-diagnostics=no`",
            //     );
            // }

            self.emitter.emit_diagnostic(diagnostic);
            if diagnostic.is_error() {
                self.deduplicated_err_count += 1;
            } else if diagnostic.level == Level::Warning {
                self.deduplicated_warn_count += 1;
            }
        }

        if diagnostic.is_error() {
            self.bump_err_count();
            #[allow(deprecated)]
            Err(ErrorGuaranteed::new_unchecked())
        } else {
            self.bump_warn_count();
            Ok(())
        }
    }

    fn print_error_count(&mut self) -> Result {
        // self.emit_stashed_diagnostics();

        if self.treat_err_as_bug() {
            return Ok(());
        }

        let warnings = |count| match count {
            0 => unreachable!(),
            1 => Cow::from("1 warning emitted"),
            count => Cow::from(format!("{count} warnings emitted")),
        };
        let errors = |count| match count {
            0 => unreachable!(),
            1 => Cow::from("aborting due to 1 previous error"),
            count => Cow::from(format!("aborting due to {count} previous errors")),
        };

        match (self.deduplicated_err_count, self.deduplicated_warn_count) {
            (0, 0) => Ok(()),
            (0, w) => {
                self.emitter.emit_diagnostic(&Diagnostic::new(Level::Warning, warnings(w)));
                Ok(())
            }
            (e, 0) => self.emit_diagnostic(Diagnostic::new(Level::Error, errors(e))),
            (e, w) => self.emit_diagnostic(Diagnostic::new(
                Level::Error,
                format!("{}; {}", errors(e), warnings(w)),
            )),
        }
    }

    /// Inserts the given diagnostic into the set of emitted diagnostics.
    /// Returns `true` if the diagnostic was already emitted.
    fn insert_diagnostic<H: std::hash::Hash>(&mut self, diag: &H) -> bool {
        let hash = sulk_data_structures::map::ahash::RandomState::new().hash_one(diag);
        !self.emitted_diagnostics.insert(hash)
    }

    fn treat_err_as_bug(&self) -> bool {
        self.flags.treat_err_as_bug.is_some_and(|c| self.err_count >= c.get())
    }

    fn bump_err_count(&mut self) {
        self.err_count += 1;
        self.panic_if_treat_err_as_bug();
    }

    fn bump_warn_count(&mut self) {
        self.warn_count += 1;
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

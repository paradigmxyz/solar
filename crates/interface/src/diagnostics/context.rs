use super::{
    emitter::EmitterWriter, Diagnostic, DiagnosticBuilder, DiagnosticMessage, DynEmitter,
    EmissionGuarantee, ErrorGuaranteed, FatalAbort, Level, SilentEmitter,
};
use crate::SourceMap;
use anstream::ColorChoice;
use sulk_data_structures::{
    map::FxHashSet,
    sync::{Lock, Lrc},
};

/// A handler deals with errors and other compiler output.
/// Certain errors (fatal, bug, unimpl) may cause immediate exit,
/// others log errors for later reporting.
pub struct DiagCtxt {
    inner: Lock<DiagCtxtInner>,
}

struct DiagCtxtInner {
    emitter: Box<DynEmitter>,

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

    can_emit_warnings: bool,
}

impl DiagCtxt {
    /// Creates a new `DiagCtxt` with the given diagnostics emitter.
    pub fn new(emitter: Box<DynEmitter>) -> Self {
        Self {
            inner: Lock::new(DiagCtxtInner {
                emitter,
                err_count: 0,
                deduplicated_err_count: 0,
                warn_count: 0,
                deduplicated_warn_count: 0,
                emitted_diagnostics: FxHashSet::default(),
                can_emit_warnings: true,
            }),
        }
    }

    /// Creates a new `DiagCtxt` with a test emitter.
    pub fn with_test_emitter(ui: bool) -> Self {
        Self::new(Box::new(EmitterWriter::test(ui)))
    }

    /// Creates a new `DiagCtxt` with a TTY emitter.
    pub fn with_tty_emitter(source_map: Option<Lrc<SourceMap>>) -> Self {
        Self::new(Box::new(EmitterWriter::stderr(ColorChoice::Auto).source_map(source_map)))
    }

    /// Creates a new `DiagCtxt` with a silent emitter.
    pub fn with_silent_emitter(fatal_note: Option<String>) -> Self {
        let fatal_dcx = Self::with_tty_emitter(None).disable_warnings();
        Self::new(Box::new(SilentEmitter::new(fatal_dcx).with_note(fatal_note))).disable_warnings()
    }

    /// Disables emitting warnings.
    pub fn disable_warnings(mut self) -> Self {
        self.inner.get_mut().can_emit_warnings = false;
        self
    }

    /// Emits the given diagnostic with this context.
    #[inline]
    pub fn emit_diagnostic(&self, mut diagnostic: Diagnostic) -> Option<ErrorGuaranteed> {
        self.emit_diagnostic_without_consuming(&mut diagnostic)
    }

    /// Emits the given diagnostic with this context, without consuming the diagnostic.
    ///
    /// **Note:** This function is intended to be used only internally in `DiagnosticBuilder`.
    /// Use [`emit_diagnostic`](Self::emit_diagnostic) instead.
    pub(super) fn emit_diagnostic_without_consuming(
        &self,
        diagnostic: &mut Diagnostic,
    ) -> Option<ErrorGuaranteed> {
        self.inner.lock().emit_diagnostic_without_consuming(diagnostic)
    }

    /// Returns the number of errors that have been emitted, including duplicates.
    #[inline]
    pub fn err_count(&self) -> usize {
        self.inner.lock().err_count
    }

    /// Returns `true` if any errors have been emitted.
    pub fn has_errors(&self) -> Result<(), ErrorGuaranteed> {
        if self.inner.lock().has_errors() {
            #[allow(deprecated)]
            Err(ErrorGuaranteed::new_unchecked())
        } else {
            Ok(())
        }
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

    /// Creates a builder at the `Fatal` level with the given `msg`.
    #[track_caller]
    pub fn fatal(&self, msg: impl Into<DiagnosticMessage>) -> DiagnosticBuilder<'_, FatalAbort> {
        self.diag(Level::Fatal, msg)
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
    fn emit_diagnostic_without_consuming(
        &mut self,
        diagnostic: &mut Diagnostic,
    ) -> Option<ErrorGuaranteed> {
        if diagnostic.level == Level::Warning && !self.can_emit_warnings {
            return None;
        }

        if diagnostic.level == Level::Allow {
            return None;
        }

        let already_emitted = self.insert_diagnostic(diagnostic);
        if !already_emitted {
            // Remove duplicate `Once*` subdiagnostics.
            diagnostic.children.retain_mut(|sub| {
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
            Some(ErrorGuaranteed::new_unchecked())
        } else {
            self.bump_warn_count();
            None
        }
    }

    /// Inserts the given diagnostic into the set of emitted diagnostics.
    /// Returns `true` if the diagnostic was already emitted.
    fn insert_diagnostic<H: std::hash::Hash>(&mut self, diag: &H) -> bool {
        let hash = sulk_data_structures::map::ahash::RandomState::new().hash_one(diag);
        !self.emitted_diagnostics.insert(hash)
    }

    fn bump_err_count(&mut self) {
        self.err_count += 1;
        // self.panic_if_treat_err_as_bug();
    }

    fn bump_warn_count(&mut self) {
        self.warn_count += 1;
    }

    fn has_errors(&self) -> bool {
        self.err_count > 0
    }
}

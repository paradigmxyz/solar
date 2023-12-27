use super::{
    DiagCtxt, Diagnostic, DiagnosticId, DiagnosticMessage, ErrorGuaranteed, FatalAbort, FatalError,
    Level, MultiSpan, Style,
};
use core::fmt;
use std::{
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
};
use sulk_data_structures::Never;

/// Trait for types that `DiagnosticBuilder::emit` can return as a "guarantee" (or "proof") token
/// that the emission happened.
pub trait EmissionGuarantee: Sized {
    /// This exists so that bugs and fatal errors can both result in `!` (an abort) when emitted,
    /// but have different aborting behaviour.
    type EmitResult;

    /// Implementation of `DiagnosticBuilder::emit`, fully controlled by each `impl` of
    /// `EmissionGuarantee`, to make it impossible to create a value of `Self::EmitResult` without
    /// actually performing the emission.
    #[track_caller]
    fn emit_producing_guarantee(db: &mut DiagnosticBuilder<'_, Self>) -> Self::EmitResult;
}

impl EmissionGuarantee for ErrorGuaranteed {
    type EmitResult = Self;

    fn emit_producing_guarantee(db: &mut DiagnosticBuilder<'_, Self>) -> Self::EmitResult {
        let guar = db.emit_producing_error_guaranteed();

        // Only allow a guarantee if the `level` wasn't switched to a
        // non-error - the field isn't `pub`, but the whole `Diagnostic`
        // can be overwritten with a new one, thanks to `DerefMut`.
        assert!(
            db.diagnostic.is_error(),
            "emitted non-error ({:?}) diagnostic from `DiagnosticBuilder<ErrorGuaranteed>`",
            db.diagnostic.level,
        );

        guar.unwrap()
    }
}

impl EmissionGuarantee for () {
    type EmitResult = Self;

    fn emit_producing_guarantee(db: &mut DiagnosticBuilder<'_, Self>) -> Self::EmitResult {
        db.emit_producing_nothing();
    }
}

impl EmissionGuarantee for FatalAbort {
    type EmitResult = Never;

    fn emit_producing_guarantee(db: &mut DiagnosticBuilder<'_, Self>) -> Self::EmitResult {
        db.emit_producing_nothing();
        FatalError.raise()
    }
}

impl EmissionGuarantee for FatalError {
    type EmitResult = Self;

    fn emit_producing_guarantee(db: &mut DiagnosticBuilder<'_, Self>) -> Self::EmitResult {
        db.emit_producing_nothing();
        Self
    }
}

/// Used for emitting structured error messages and other diagnostic information.
///
/// **Note:** Incorrect usage of this type results in a panic when dropped.
/// This is to ensure that all errors are either emitted or cancelled.
#[must_use = "diagnostics must be emitted or cancelled"]
#[derive(Clone)]
pub struct DiagnosticBuilder<'a, G: EmissionGuarantee> {
    dcx: &'a DiagCtxt,

    /// `Diagnostic` is a large type, and `DiagnosticBuilder` is often used as a
    /// return value, especially within the frequently-used `PResult` type.
    /// In theory, return value optimization (RVO) should avoid unnecessary
    /// copying. In practice, it does not (at the time of writing).
    diagnostic: Box<Diagnostic>,

    _marker: PhantomData<G>,
}

impl<G: EmissionGuarantee> fmt::Debug for DiagnosticBuilder<'_, G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic.fmt(f)
    }
}

impl<'a, G: EmissionGuarantee> Deref for DiagnosticBuilder<'a, G> {
    type Target = Diagnostic;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.diagnostic
    }
}

impl<'a, G: EmissionGuarantee> DerefMut for DiagnosticBuilder<'a, G> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.diagnostic
    }
}

impl<'a, G: EmissionGuarantee> Drop for DiagnosticBuilder<'a, G> {
    #[track_caller]
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }

        self.dcx.emit_diagnostic(Diagnostic::new(
            Level::Fatal,
            "the following error was constructed but not emitted",
        ));
        self.dcx.emit_diagnostic_without_consuming(&mut self.diagnostic);
        panic!("error was constructed but not emitted");
    }
}

impl<'a, G: EmissionGuarantee> DiagnosticBuilder<'a, G> {
    /// Creates a new `DiagnosticBuilder`.
    #[track_caller]
    pub fn new<M: Into<DiagnosticMessage>>(dcx: &'a DiagCtxt, level: Level, msg: M) -> Self {
        Self::new_diagnostic(dcx, Diagnostic::new(level, msg))
    }

    /// Creates a new `DiagnosticBuilder` with an already constructed diagnostic.
    pub(crate) fn new_diagnostic(dcx: &'a DiagCtxt, diagnostic: Diagnostic) -> Self {
        Self { dcx, diagnostic: Box::new(diagnostic), _marker: PhantomData }
    }

    /// Returns the [`DiagCtxt`].
    #[inline]
    pub fn dcx(&self) -> &DiagCtxt {
        self.dcx
    }

    /// Emits the diagnostic.
    #[track_caller]
    pub fn emit(self) -> G::EmitResult {
        self.consume_no_panic(G::emit_producing_guarantee)
    }

    fn emit_producing_nothing(&mut self) {
        let _ = self.emit_producing_error_guaranteed();
    }

    fn emit_producing_error_guaranteed(&mut self) -> Option<ErrorGuaranteed> {
        self.dcx.emit_diagnostic_without_consuming(&mut self.diagnostic)
    }

    /// Cancel the diagnostic (a structured diagnostic must either be emitted or cancelled or it
    /// will panic when dropped).
    #[inline]
    pub fn cancel(self) {
        self.consume_no_panic(|_| {});
    }

    #[track_caller]
    fn consume_no_panic<R>(self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut this = ManuallyDrop::new(self);
        let r = f(&mut *this);
        unsafe { std::ptr::drop_in_place(&mut this.diagnostic) };
        r
    }
}

/// Forwards methods to `Diagnostic`.
macro_rules! forward {
    (
        $(
            $(#[$attrs:meta])*
            $vis:vis fn $n:ident($($name:ident: $ty:ty),* $(,)?);
        )*
    ) => {
        $(
            $(#[$attrs])*
            #[doc = concat!("See [`Diagnostic::", stringify!($n), "()`].")]
            $vis fn $n(mut self, $($name: $ty),*) -> Self {
                self.diagnostic.$n($($name),*);
                self
            }
        )*
    };
}

impl<'a, G: EmissionGuarantee> DiagnosticBuilder<'a, G> {
    forward! {
        pub fn span(span: impl Into<MultiSpan>);
        pub fn code(code: impl Into<DiagnosticId>);

        pub fn warn(msg: impl Into<DiagnosticMessage>);
        pub fn span_warn(span: impl Into<MultiSpan>, msg: impl Into<DiagnosticMessage>);

        pub fn note(msg: impl Into<DiagnosticMessage>);
        pub fn span_note(span: impl Into<MultiSpan>, msg: impl Into<DiagnosticMessage>);
        pub fn highlighted_note(messages: Vec<(impl Into<DiagnosticMessage>, Style)>);
        pub fn note_once(msg: impl Into<DiagnosticMessage>);
        pub fn span_note_once(span: impl Into<MultiSpan>, msg: impl Into<DiagnosticMessage>);

        pub fn help(msg: impl Into<DiagnosticMessage>);
        pub fn help_once(msg: impl Into<DiagnosticMessage>);
        pub fn highlighted_help(messages: Vec<(impl Into<DiagnosticMessage>, Style)>);
        pub fn span_help(span: impl Into<MultiSpan>, msg: impl Into<DiagnosticMessage>);
    }
}

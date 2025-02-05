use super::{
    BugAbort, Diag, DiagCtxt, DiagId, DiagMsg, ErrorGuaranteed, ExplicitBug, FatalAbort, Level,
    MultiSpan, Style,
};
use crate::Span;
use solar_data_structures::Never;
use std::{
    fmt,
    marker::PhantomData,
    mem::ManuallyDrop,
    ops::{Deref, DerefMut},
    panic::Location,
};

/// Trait for types that `DiagBuilder::emit` can return as a "guarantee" (or "proof") token
/// that the emission happened.
pub trait EmissionGuarantee: Sized {
    /// This exists so that bugs and fatal errors can both result in `!` (an abort) when emitted,
    /// but have different aborting behaviour.
    type EmitResult;

    /// Implementation of `DiagBuilder::emit`, fully controlled by each `impl` of
    /// `EmissionGuarantee`, to make it impossible to create a value of `Self::EmitResult` without
    /// actually performing the emission.
    #[track_caller]
    fn emit_producing_guarantee(db: &mut DiagBuilder<'_, Self>) -> Self::EmitResult;
}

impl EmissionGuarantee for ErrorGuaranteed {
    type EmitResult = Self;

    fn emit_producing_guarantee(db: &mut DiagBuilder<'_, Self>) -> Self::EmitResult {
        let guar = db.emit_producing_error_guaranteed();

        // Only allow a guarantee if the `level` wasn't switched to a
        // non-error - the field isn't `pub`, but the whole `Diag`
        // can be overwritten with a new one, thanks to `DerefMut`.
        assert!(
            db.diagnostic.is_error(),
            "emitted non-error ({:?}) diagnostic from `DiagBuilder<ErrorGuaranteed>`",
            db.diagnostic.level,
        );

        guar.unwrap_err()
    }
}

impl EmissionGuarantee for () {
    type EmitResult = Self;

    fn emit_producing_guarantee(db: &mut DiagBuilder<'_, Self>) -> Self::EmitResult {
        db.emit_producing_nothing();
    }
}

impl EmissionGuarantee for BugAbort {
    type EmitResult = Never;

    fn emit_producing_guarantee(db: &mut DiagBuilder<'_, Self>) -> Self::EmitResult {
        db.emit_producing_nothing();
        std::panic::panic_any(ExplicitBug);
    }
}

impl EmissionGuarantee for FatalAbort {
    type EmitResult = Never;

    fn emit_producing_guarantee(db: &mut DiagBuilder<'_, Self>) -> Self::EmitResult {
        db.emit_producing_nothing();
        std::panic::panic_any(Self);
    }
}

/// Used for emitting structured error messages and other diagnostic information.
///
/// **Note:** Incorrect usage of this type results in a panic when dropped.
/// This is to ensure that all errors are either emitted or cancelled.
#[must_use = "diagnostics must be emitted or cancelled"]
pub struct DiagBuilder<'a, G: EmissionGuarantee> {
    dcx: &'a DiagCtxt,

    /// `Diag` is a large type, and `DiagBuilder` is often used as a
    /// return value, especially within the frequently-used `PResult` type.
    /// In theory, return value optimization (RVO) should avoid unnecessary
    /// copying. In practice, it does not (at the time of writing).
    diagnostic: Box<Diag>,

    _marker: PhantomData<G>,
}

impl<G: EmissionGuarantee> Clone for DiagBuilder<'_, G> {
    #[inline]
    fn clone(&self) -> Self {
        Self { dcx: self.dcx, diagnostic: self.diagnostic.clone(), _marker: PhantomData }
    }
}

impl<G: EmissionGuarantee> fmt::Debug for DiagBuilder<'_, G> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.diagnostic.fmt(f)
    }
}

impl<G: EmissionGuarantee> Deref for DiagBuilder<'_, G> {
    type Target = Diag;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.diagnostic
    }
}

impl<G: EmissionGuarantee> DerefMut for DiagBuilder<'_, G> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.diagnostic
    }
}

impl<G: EmissionGuarantee> Drop for DiagBuilder<'_, G> {
    #[track_caller]
    fn drop(&mut self) {
        if std::thread::panicking() {
            return;
        }

        let _ = self.dcx.emit_diagnostic(Diag::new(
            Level::Bug,
            "the following error was constructed but not emitted",
        ));
        let _ = self.dcx.emit_diagnostic_without_consuming(&mut self.diagnostic);
        panic!("error was constructed but not emitted");
    }
}

impl<'a, G: EmissionGuarantee> DiagBuilder<'a, G> {
    /// Creates a new `DiagBuilder`.
    #[track_caller]
    pub fn new<M: Into<DiagMsg>>(dcx: &'a DiagCtxt, level: Level, msg: M) -> Self {
        Self { dcx, diagnostic: Box::new(Diag::new(level, msg)), _marker: PhantomData }
    }

    /// Returns the [`DiagCtxt`].
    #[inline]
    pub fn dcx(&self) -> &DiagCtxt {
        self.dcx
    }

    /// Emits the diagnostic.
    #[track_caller]
    pub fn emit(mut self) -> G::EmitResult {
        if self.dcx.track_diagnostics() {
            self.diagnostic.locations_note(Location::caller());
        }
        self.consume_no_panic(G::emit_producing_guarantee)
    }

    fn emit_producing_nothing(&mut self) {
        let _ = self.emit_producing_error_guaranteed();
    }

    fn emit_producing_error_guaranteed(&mut self) -> Result<(), ErrorGuaranteed> {
        self.dcx.emit_diagnostic_without_consuming(&mut self.diagnostic)
    }

    /// Cancel the diagnostic (a structured diagnostic must either be emitted or cancelled or it
    /// will panic when dropped).
    #[inline]
    pub fn cancel(self) {
        self.consume_no_panic(|_| {});
    }

    fn consume_no_panic<R>(self, f: impl FnOnce(&mut Self) -> R) -> R {
        let mut this = ManuallyDrop::new(self);
        let r = f(&mut *this);
        unsafe { std::ptr::drop_in_place(&mut this.diagnostic) };
        r
    }
}

/// Forwards methods to [`Diag`].
macro_rules! forward {
    (
        $(
            $(#[$attrs:meta])*
            $vis:vis fn $n:ident($($name:ident: $ty:ty),* $(,)?);
        )*
    ) => {
        $(
            $(#[$attrs])*
            #[doc = concat!("See [`Diag::", stringify!($n), "()`].")]
            $vis fn $n(mut self, $($name: $ty),*) -> Self {
                self.diagnostic.$n($($name),*);
                self
            }
        )*
    };
}

/// Forwarded methods to [`Diag`].
impl<G: EmissionGuarantee> DiagBuilder<'_, G> {
    forward! {
        pub fn span(span: impl Into<MultiSpan>);
        pub fn code(code: impl Into<DiagId>);

        pub fn span_label(span: Span, label: impl Into<DiagMsg>);
        pub fn span_labels(spans: impl IntoIterator<Item = Span>, label: impl Into<DiagMsg>);

        pub fn warn(msg: impl Into<DiagMsg>);
        pub fn span_warn(span: impl Into<MultiSpan>, msg: impl Into<DiagMsg>);

        pub fn note(msg: impl Into<DiagMsg>);
        pub fn span_note(span: impl Into<MultiSpan>, msg: impl Into<DiagMsg>);
        pub fn highlighted_note(messages: Vec<(impl Into<DiagMsg>, Style)>);
        pub fn note_once(msg: impl Into<DiagMsg>);
        pub fn span_note_once(span: impl Into<MultiSpan>, msg: impl Into<DiagMsg>);

        pub fn help(msg: impl Into<DiagMsg>);
        pub fn help_once(msg: impl Into<DiagMsg>);
        pub fn highlighted_help(messages: Vec<(impl Into<DiagMsg>, Style)>);
        pub fn span_help(span: impl Into<MultiSpan>, msg: impl Into<DiagMsg>);
    }
}

//! Diagnostics implementation.
//!
//! Modified from [`rustc_errors`](https://github.com/rust-lang/rust/blob/520e30be83b4ed57b609d33166c988d1512bf4f3/compiler/rustc_errors/src/diagnostic.rs).

use crate::Span;
use anstyle::{Ansi256Color, AnsiColor, Color};
use std::{
    borrow::Cow,
    panic::{self, Location},
    process::ExitCode,
};

mod builder;
pub use builder::{DiagnosticBuilder, EmissionGuarantee};

mod context;
pub use context::DiagCtxt;

mod emitter;
#[cfg(feature = "json")]
pub use emitter::JsonEmitter;
pub use emitter::{DynEmitter, Emitter, HumanEmitter, LocalEmitter, SilentEmitter};

mod message;
pub use message::{DiagnosticMessage, MultiSpan, SpanLabel};

/// Useful type to use with [`Result`] indicate that an error has already been reported to the user,
/// so no need to continue checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ErrorGuaranteed(());

impl ErrorGuaranteed {
    /// Creates a new `ErrorGuaranteed`.
    ///
    /// Use of this method is discouraged.
    #[inline]
    #[allow(deprecated)]
    pub const fn new_unchecked() -> Self {
        Self(())
    }
}

/// Marker type which enables implementation of `create_bug` and `emit_bug` functions for
/// bug diagnostics.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BugAbort;

/// Marker type which enables implementation of fatal diagnostics.
pub struct FatalAbort(());

/// Used as a return value to signify that a fatal error occurred.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[must_use]
pub struct FatalError;

impl FatalError {
    /// Raises a fatal error that can be caught by [`catch`](Self::catch).
    pub fn raise(self) -> ! {
        panic::resume_unwind(Box::new(self))
    }

    /// Catches a fatal error that was raised by [`raise`](Self::raise).
    pub fn catch<R>(f: impl FnOnce() -> R) -> Result<R, ErrorGuaranteed> {
        panic::catch_unwind(panic::AssertUnwindSafe(f)).map_err(|value| {
            if value.is::<Self>() {
                #[allow(deprecated)]
                ErrorGuaranteed::new_unchecked()
            } else {
                panic::resume_unwind(value)
            }
        })
    }

    /// Catches a fatal error that was raised by [`raise`](Self::raise).
    ///
    /// Returns [`FAILURE`](ExitCode::FAILURE) if an error was caught,
    /// [`SUCCESS`](ExitCode::SUCCESS) otherwise.
    pub fn catch_with_exit_code(f: impl FnOnce() -> Result<(), ErrorGuaranteed>) -> ExitCode {
        match Self::catch(f).and_then(std::convert::identity) {
            Ok(()) => ExitCode::SUCCESS,
            Err(_) => ExitCode::FAILURE,
        }
    }
}

/// Diagnostic ID.
///
/// Use [`error_code!`](crate::error_code) to create an error code diagnostic ID.
///
/// # Examples
///
/// ```
/// # use sulk_interface::{diagnostics::DiagnosticId, error_code};
/// let id: DiagnosticId = error_code!(E1234);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticId {
    id: &'static str,
}

impl DiagnosticId {
    /// Creates an error code diagnostic ID.
    ///
    /// Use [`error_code!`](crate::error_code) instead.
    #[doc(hidden)]
    #[track_caller]
    pub const fn new_from_macro(id: &'static str) -> Self {
        let [b'E', bytes @ ..] = id.as_bytes() else { panic!("error codes must start with 'E'") };
        assert!(bytes.len() == 4, "error codes must be exactly 4 digits long");

        let mut bytes = bytes;
        while let [byte, rest @ ..] = bytes {
            assert!(byte.is_ascii_digit(), "error codes must be decimal");
            bytes = rest;
        }

        Self { id }
    }
}

/// Used for creating an error code. The input must be exactly one 'E' character followed by four
/// decimal digits.
///
/// # Examples
///
/// ```
/// # use sulk_interface::{diagnostics::DiagnosticId, error_code};
/// let code: DiagnosticId = error_code!(E1234);
/// ```
#[macro_export]
macro_rules! error_code {
    ($id:ident) => {{
        const $id: $crate::diagnostics::DiagnosticId =
            $crate::diagnostics::DiagnosticId::new_from_macro(stringify!($id));
        $id
    }};
}

/// Diagnostic level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Level {
    /// An error that causes an immediate abort. Used for things like configuration errors,
    /// internal overflows, some file operation errors.
    ///
    /// Its `EmissionGuarantee` is `FatalAbort`, except in the non-aborting "almost fatal" case
    /// that is occasionaly used, where it is `FatalError`.
    Fatal,

    /// An error in the code being compiled, which prevents compilation from finishing. This is the
    /// most common case.
    ///
    /// Its `EmissionGuarantee` is `ErrorGuaranteed`.
    Error,

    /// A warning about the code being compiled. Does not prevent compilation from finishing.
    ///
    /// Its `EmissionGuarantee` is `()`.
    Warning,

    /// A message giving additional context. Rare, because notes are more commonly attached to
    /// other diagnostics such as errors.
    ///
    /// Its `EmissionGuarantee` is `()`.
    Note,

    /// A note that is only emitted once. Rare, mostly used in circumstances relating to lints.
    ///
    /// Its `EmissionGuarantee` is `()`.
    OnceNote,

    /// A message suggesting how to fix something. Rare, because help messages are more commonly
    /// attached to other diagnostics such as errors.
    ///
    /// Its `EmissionGuarantee` is `()`.
    Help,

    /// A help that is only emitted once. Rare.
    ///
    /// Its `EmissionGuarantee` is `()`.
    OnceHelp,

    /// Similar to `Note`, but used in cases where compilation has failed. Rare.
    ///
    /// Its `EmissionGuarantee` is `()`.
    FailureNote,

    /// Only used for lints.
    ///
    /// Its `EmissionGuarantee` is `()`.
    Allow,
}

impl Level {
    /// Returns the string representation of the level.
    pub fn to_str(self) -> &'static str {
        match self {
            Self::Fatal | Self::Error => "error",
            Self::Warning => "warning",
            Self::Note | Self::OnceNote => "note",
            Self::Help | Self::OnceHelp => "help",
            Self::FailureNote => "failure-note",
            Self::Allow
            // | Self::Expect(_)
            => unreachable!(),
        }
    }

    /// Returns `true` if this level is an error.
    #[inline]
    pub fn is_error(self) -> bool {
        match self {
            Self::Fatal | Self::Error | Self::FailureNote => true,

            Self::Warning
            | Self::Note
            | Self::OnceNote
            | Self::Help
            | Self::OnceHelp
            | Self::Allow => false,
        }
    }

    /// Returns the style of this level.
    #[inline]
    pub const fn style(self) -> anstyle::Style {
        anstyle::Style::new().fg_color(self.color()).bold()
    }

    /// Returns the color of this level.
    #[inline]
    pub const fn color(self) -> Option<Color> {
        match self.ansi_color() {
            Some(c) => Some(intense(c)),
            None => None,
        }
    }

    /// Returns the ANSI color of this level.
    #[inline]
    pub const fn ansi_color(self) -> Option<AnsiColor> {
        // https://github.com/rust-lang/rust/blob/99472c7049783605444ab888a97059d0cce93a12/compiler/rustc_errors/src/lib.rs#L1768
        match self {
            Self::Fatal | Self::Error => Some(AnsiColor::Red),
            Self::Warning => Some(AnsiColor::Yellow),
            Self::Note | Self::OnceNote => Some(AnsiColor::Green),
            Self::Help | Self::OnceHelp => Some(AnsiColor::Cyan),
            Self::FailureNote | Self::Allow => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Style {
    MainHeaderMsg,
    HeaderMsg,
    LineAndColumn,
    LineNumber,
    Quotation,
    UnderlinePrimary,
    UnderlineSecondary,
    LabelPrimary,
    LabelSecondary,
    NoStyle,
    Level(Level),
    Highlight,
    Addition,
    Removal,
}

impl Style {
    /// Converts the style to an [`anstyle::Style`].
    pub const fn to_color_spec(self, level: Level) -> anstyle::Style {
        use AnsiColor::*;

        /// On Windows, BRIGHT_BLUE is hard to read on black. Use cyan instead.
        ///
        /// See [rust-lang/rust#36178](https://github.com/rust-lang/rust/pull/36178).
        const BRIGHT_BLUE: Color = intense(if cfg!(windows) { Cyan } else { Blue });
        const GREEN: Color = intense(Green);
        const MAGENTA: Color = intense(Magenta);
        const RED: Color = intense(Red);
        const WHITE: Color = intense(White);

        let s = anstyle::Style::new();
        match self {
            Self::Addition => s.fg_color(Some(GREEN)),
            Self::Removal => s.fg_color(Some(RED)),
            Self::LineAndColumn => s,
            Self::LineNumber => s.fg_color(Some(BRIGHT_BLUE)).bold(),
            Self::Quotation => s,
            Self::MainHeaderMsg => if cfg!(windows) { s.fg_color(Some(WHITE)) } else { s }.bold(),
            Self::UnderlinePrimary | Self::LabelPrimary => s.fg_color(level.color()).bold(),
            Self::UnderlineSecondary | Self::LabelSecondary => s.fg_color(Some(BRIGHT_BLUE)).bold(),
            Self::HeaderMsg | Self::NoStyle => s,
            Self::Level(level2) => s.fg_color(level2.color()).bold(),
            Self::Highlight => s.fg_color(Some(MAGENTA)).bold(),
        }
    }
}

/// A "sub"-diagnostic attached to a parent diagnostic.
/// For example, a note attached to an error.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct SubDiagnostic {
    pub level: Level,
    pub messages: Vec<(DiagnosticMessage, Style)>,
    pub span: MultiSpan,
}

impl SubDiagnostic {
    /// Formats the diagnostic messages into a single string.
    pub fn label(&self) -> Cow<'_, str> {
        flatten_messages(&self.messages)
    }
}

/// A compiler diagnostic.
#[must_use]
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub(crate) level: Level,

    pub messages: Vec<(DiagnosticMessage, Style)>,
    pub span: MultiSpan,
    pub children: Vec<SubDiagnostic>,
    pub code: Option<DiagnosticId>,

    pub created_at: &'static Location<'static>,
}

impl PartialEq for Diagnostic {
    fn eq(&self, other: &Self) -> bool {
        self.keys() == other.keys()
    }
}

impl std::hash::Hash for Diagnostic {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.keys().hash(state);
    }
}

impl Diagnostic {
    /// Creates a new `Diagnostic` with a single message.
    #[track_caller]
    pub fn new<M: Into<DiagnosticMessage>>(level: Level, msg: M) -> Self {
        Self::new_with_messages(level, vec![(msg.into(), Style::NoStyle)])
    }

    /// Creates a new `Diagnostic` with multiple messages.
    #[track_caller]
    pub fn new_with_messages(level: Level, messages: Vec<(DiagnosticMessage, Style)>) -> Self {
        Self {
            level,
            messages,
            code: None,
            span: MultiSpan::new(),
            children: vec![],
            // suggestions: Ok(vec![]),
            // args: Default::default(),
            // sort_span: DUMMY_SP,
            // is_lint: false,
            created_at: Location::caller(),
        }
    }

    /// Returns `true` if this diagnostic is an error.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.level.is_error()
    }

    /// Formats the diagnostic messages into a single string.
    pub fn label(&self) -> Cow<'_, str> {
        flatten_messages(&self.messages)
    }

    /// Returns the messages of this diagnostic.
    pub fn messages(&self) -> &[(DiagnosticMessage, Style)] {
        &self.messages
    }

    /// Returns the level of this diagnostic.
    pub fn level(&self) -> Level {
        self.level
    }

    /// Fields used for `PartialEq` and `Hash` implementations.
    fn keys(&self) -> impl PartialEq + std::hash::Hash + '_ {
        (
            &self.level,
            &self.messages,
            // self.args().collect(),
            &self.code,
            &self.span,
            // &self.suggestions,
            // (if self.is_lint { None } else { Some(&self.children) }),
            &self.children,
        )
    }
}

/// Setters.
impl Diagnostic {
    /// Sets the span of this diagnostic.
    pub fn span(&mut self, span: impl Into<MultiSpan>) -> &mut Self {
        self.span = span.into();
        self
    }

    /// Sets the code of this diagnostic.
    pub fn code(&mut self, code: impl Into<DiagnosticId>) -> &mut Self {
        self.code = Some(code.into());
        self
    }

    /// Adds a span/label to be included in the resulting snippet.
    ///
    /// This is pushed onto the [`MultiSpan`] that was created when the diagnostic
    /// was first built. That means it will be shown together with the original
    /// span/label, *not* a span added by one of the `span_{note,warn,help,suggestions}` methods.
    ///
    /// This span is *not* considered a ["primary span"][`MultiSpan`]; only
    /// the `Span` supplied when creating the diagnostic is primary.
    pub fn span_label(&mut self, span: Span, label: impl Into<DiagnosticMessage>) -> &mut Self {
        self.span.push_span_label(span, label);
        self
    }

    /// Labels all the given spans with the provided label.
    /// See [`Self::span_label()`] for more information.
    pub fn span_labels(
        &mut self,
        spans: impl IntoIterator<Item = Span>,
        label: impl Into<DiagnosticMessage>,
    ) -> &mut Self {
        let label = label.into();
        for span in spans {
            self.span_label(span, label.clone());
        }
        self
    }

    /// Adds a note with the location where this diagnostic was created and emitted.
    pub(crate) fn locations_note(&mut self, emitted_at: &Location<'_>) -> &mut Self {
        let msg = format!(
            "created at {},\n\
             emitted at {}",
            self.created_at, emitted_at
        );
        self.note(msg)
    }
}

/// Sub-diagnostics.
impl Diagnostic {
    /// Add a warning attached to this diagnostic.
    pub fn warn(&mut self, msg: impl Into<DiagnosticMessage>) -> &mut Self {
        self.sub(Level::Warning, msg, MultiSpan::new())
    }

    /// Prints the span with a warning above it.
    /// This is like [`Diagnostic::warn()`], but it gets its own span.
    pub fn span_warn(
        &mut self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagnosticMessage>,
    ) -> &mut Self {
        self.sub(Level::Warning, msg, span)
    }

    /// Add a note to this diagnostic.
    pub fn note(&mut self, msg: impl Into<DiagnosticMessage>) -> &mut Self {
        self.sub(Level::Note, msg, MultiSpan::new())
    }

    /// Prints the span with a note above it.
    /// This is like [`Diagnostic::note()`], but it gets its own span.
    pub fn span_note(
        &mut self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagnosticMessage>,
    ) -> &mut Self {
        self.sub(Level::Note, msg, span)
    }

    pub fn highlighted_note(
        &mut self,
        messages: Vec<(impl Into<DiagnosticMessage>, Style)>,
    ) -> &mut Self {
        self.sub_with_highlights(Level::Note, messages, MultiSpan::new())
    }

    /// Prints the span with a note above it.
    /// This is like [`Diagnostic::note()`], but it gets emitted only once.
    pub fn note_once(&mut self, msg: impl Into<DiagnosticMessage>) -> &mut Self {
        self.sub(Level::OnceNote, msg, MultiSpan::new())
    }

    /// Prints the span with a note above it.
    /// This is like [`Diagnostic::note_once()`], but it gets its own span.
    pub fn span_note_once(
        &mut self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagnosticMessage>,
    ) -> &mut Self {
        self.sub(Level::OnceNote, msg, span)
    }

    /// Add a help message attached to this diagnostic.
    pub fn help(&mut self, msg: impl Into<DiagnosticMessage>) -> &mut Self {
        self.sub(Level::Help, msg, MultiSpan::new())
    }

    /// Prints the span with a help above it.
    /// This is like [`Diagnostic::help()`], but it gets its own span.
    pub fn help_once(&mut self, msg: impl Into<DiagnosticMessage>) -> &mut Self {
        self.sub(Level::OnceHelp, msg, MultiSpan::new())
    }

    /// Add a help message attached to this diagnostic with a customizable highlighted message.
    pub fn highlighted_help(
        &mut self,
        msgs: Vec<(impl Into<DiagnosticMessage>, Style)>,
    ) -> &mut Self {
        self.sub_with_highlights(Level::Help, msgs, MultiSpan::new())
    }

    /// Prints the span with some help above it.
    /// This is like [`Diagnostic::help()`], but it gets its own span.
    pub fn span_help(
        &mut self,
        span: impl Into<MultiSpan>,
        msg: impl Into<DiagnosticMessage>,
    ) -> &mut Self {
        self.sub(Level::Help, msg, span)
    }

    fn sub(
        &mut self,
        level: Level,
        msg: impl Into<DiagnosticMessage>,
        span: impl Into<MultiSpan>,
    ) -> &mut Self {
        self.children.push(SubDiagnostic {
            level,
            messages: vec![(msg.into(), Style::NoStyle)],
            span: span.into(),
        });
        self
    }

    fn sub_with_highlights(
        &mut self,
        level: Level,
        messages: Vec<(impl Into<DiagnosticMessage>, Style)>,
        span: MultiSpan,
    ) -> &mut Self {
        let messages = messages.into_iter().map(|(m, s)| (m.into(), s)).collect();
        self.children.push(SubDiagnostic { level, messages, span });
        self
    }
}

// TODO: Styles?
fn flatten_messages(messages: &[(DiagnosticMessage, Style)]) -> Cow<'_, str> {
    match messages {
        [] => Cow::Borrowed(""),
        [(message, _)] => Cow::Borrowed(message.as_str()),
        messages => messages.iter().map(|(msg, _)| msg.as_str()).collect(),
    }
}

// https://github.com/rust-lang/rust/blob/ce0f703554f3828f2d470679cd1e83b52667bf20/compiler/rustc_errors/src/emitter.rs#L2682
/// Ansi -> Ansi256
const fn intense(ansi: AnsiColor) -> Color {
    Color::Ansi256(Ansi256Color::from_ansi(ansi))
}

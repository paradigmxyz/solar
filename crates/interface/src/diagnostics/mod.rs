//! Diagnostics implementation.
//!
//! Modified from [`rustc_errors`](https://github.com/rust-lang/rust/blob/520e30be83b4ed57b609d33166c988d1512bf4f3/compiler/rustc_errors/src/diagnostic.rs).

use std::{borrow::Cow, fmt, panic};

mod builder;
pub use builder::{DiagnosticBuilder, EmissionGuarantee};

mod context;
pub use context::DiagCtxt;

mod emitter;
pub use emitter::{DynEmitter, Emitter, LocalEmitter, SilentEmitter};

mod message;
pub use message::{DiagnosticMessage, MultiSpan, SpanLabel};

/// Useful type to use with [`Result`] indicate that an error has already been reported to the user,
/// so no need to continue checking.
#[derive(Clone, Copy, Debug, Hash, PartialEq, Eq, PartialOrd, Ord)]
pub struct ErrorGuaranteed(());

/// Marker type which enables implementation of `create_bug` and `emit_bug` functions for
/// bug diagnostics.
#[derive(Copy, Clone)]
pub struct BugAbort;

/// Marker type which enables implementation of fatal diagnostics.
pub struct FatalAbort(());

/// Used as a return value to signify that a fatal error occurred.
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
                ErrorGuaranteed(())
            } else {
                panic::resume_unwind(value)
            }
        })
    }
}

/// Diagnostic ID.
///
/// Use [`error_code!`](crate::error_code) to create an error code diagnostic ID.
///
/// # Examples
///
/// ```
/// # use sulk_interface::error_code;
/// assert_eq!(error_code!(E1234).id(), 1234);
/// ```
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticId {
    /// The ID of the diagnostic.
    id: u32,
}

impl DiagnosticId {
    /// Creates an error code diagnostic ID.
    ///
    /// Use [`error_code!`](crate::error_code) instead.
    #[doc(hidden)]
    #[track_caller]
    pub const fn new_from_macro(s: &'static str) -> Self {
        let [b'E', bytes @ ..] = s.as_bytes() else { panic!("error codes must start with 'E'") };
        assert!(bytes.len() == 4, "error codes must be exactly 4 digits long");

        let mut bytes = bytes;
        let mut id = 0;
        while let &[byte, ref rest @ ..] = bytes {
            assert!(byte.is_ascii_digit(), "error codes must be decimal");
            id = id * 10 + (byte - b'0') as u32;
            bytes = rest;
        }
        Self { id }
    }

    /// Returns the internal ID.
    #[inline]
    pub const fn id(&self) -> u32 {
        self.id
    }
}

/// Used for creating an error code.
#[macro_export]
macro_rules! error_code {
    ($id:ident) => {{
        const $id: $crate::diagnostics::DiagnosticId =
            $crate::diagnostics::DiagnosticId::new_from_macro(stringify!($id));
        $id
    }};
}

/// Diagnostic level.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
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

#[allow(clippy::match_like_matches_macro)]
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

    /// Returns whether this level is an error.
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

#[must_use]
#[derive(Clone, Debug)]
pub struct Diagnostic {
    pub(crate) level: Level,

    pub messages: Vec<(DiagnosticMessage, Style)>,
    pub span: MultiSpan,
    pub children: Vec<SubDiagnostic>,
    pub code: Option<DiagnosticId>,

    pub emitted_at: DiagnosticLocation,
}

/// A "sub"-diagnostic attached to a parent diagnostic.
/// For example, a note attached to an error.
#[derive(Clone, Debug, PartialEq, Hash)]
pub struct SubDiagnostic {
    pub level: Level,
    pub messages: Vec<(DiagnosticMessage, Style)>,
    pub span: MultiSpan,
}

#[derive(Clone, Debug)]
pub struct DiagnosticLocation {
    file: Cow<'static, str>,
    line: u32,
    col: u32,
}

impl fmt::Display for DiagnosticLocation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}:{}:{}", self.file, self.line, self.col)
    }
}

impl DiagnosticLocation {
    #[track_caller]
    fn caller() -> Self {
        let loc = std::panic::Location::caller();
        DiagnosticLocation { file: loc.file().into(), line: loc.line(), col: loc.column() }
    }
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
            emitted_at: DiagnosticLocation::caller(),
        }
    }

    /// Returns whether this diagnostic is an error.
    #[inline]
    pub fn is_error(&self) -> bool {
        self.level.is_error()
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

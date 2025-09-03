use super::{CodeSuggestion, Diag, Level, MultiSpan, SuggestionStyle};
use crate::{SourceMap, diagnostics::Suggestions};
use std::{any::Any, sync::Arc};

mod human;
pub use human::{HumanBufferEmitter, HumanEmitter};

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "json")]
pub use json::JsonEmitter;

mod mem;
pub use mem::InMemoryEmitter;

mod rustc;

/// Maximum number of suggestions to emit per diagnostic.

/// Maximum number of lines to highlight in a suggestion.
#[allow(dead_code)]
pub(super) const MAX_SUGGESTION_HIGHLIGHT_LINES: usize = 6;

/// Dynamic diagnostic emitter. See [`Emitter`].
pub type DynEmitter = dyn Emitter + Send;

/// Diagnostic emitter.
pub trait Emitter: Any {
    /// Emits a diagnostic.
    fn emit_diagnostic(&mut self, diagnostic: &mut Diag);

    /// Returns a reference to the source map, if any.
    #[inline]
    fn source_map(&self) -> Option<&Arc<SourceMap>> {
        None
    }

    /// Returns `true` if we can use colors in the current output stream.
    #[inline]
    fn supports_color(&self) -> bool {
        false
    }

    /// Formats the substitutions of the primary_span
    ///
    /// There are a lot of conditions to this method, but in short:
    ///
    /// * If the current `DiagInner` has only one visible `CodeSuggestion`, we format the `help`
    ///   suggestion depending on the content of the substitutions. In that case, we modify the span
    ///   and clear the suggestions.
    ///
    /// * If the current `DiagInner` has multiple suggestions, we leave `primary_span` and the
    ///   suggestions untouched.
    fn primary_span_formatted(&self, primary_span: &mut MultiSpan, suggestions: &mut Suggestions) {
        if let Some((sugg, rest)) = &suggestions.split_first()
            // if there are multiple suggestions, print them all in full
            // to be consistent.
            && rest.is_empty()
            // don't display multi-suggestions as labels
            && let [substitution] = sugg.substitutions.as_slice()
            // don't display multipart suggestions as labels
            && let [part] = substitution.parts.as_slice()
            // don't display long messages as labels
            && sugg.msg.as_str().split_whitespace().count() < 10
            // don't display multiline suggestions as labels
            && !part.snippet.contains('\n')
            && ![
                // when this style is set we want the suggestion to be a message, not inline
                SuggestionStyle::HideCodeAlways,
                // trivial suggestion for tooling's sake, never shown
                SuggestionStyle::CompletelyHidden,
                // subtle suggestion, never shown inline
                SuggestionStyle::ShowAlways,
            ].contains(&sugg.style)
        {
            let snippet = part.snippet.trim();
            let msg = if snippet.is_empty() || sugg.style.hide_inline() {
                // This substitution is only removal OR we explicitly don't want to show the
                // code inline (`hide_inline`). Therefore, we don't show the substitution.
                format!("help: {}", sugg.msg.as_str())
            } else {
                format!("help: {}: `{}`", sugg.msg.as_str(), snippet,)
            };
            primary_span.push_span_label(part.span, msg);

            // Since we only return the modified primary_span, we disable suggestions.
            *suggestions = Suggestions::Disabled;
        } else {
            // Do nothing.
        }
    }
}

impl DynEmitter {
    pub(crate) fn local_buffer(&self) -> Option<&str> {
        (self as &dyn Any).downcast_ref::<HumanBufferEmitter>().map(HumanBufferEmitter::buffer)
    }
}

/// Diagnostic emitter.
///
/// Emits fatal diagnostics by default, with `note` if set.
pub struct SilentEmitter {
    fatal_emitter: Option<Box<DynEmitter>>,
    note: Option<String>,
}

impl SilentEmitter {
    /// Creates a new `SilentEmitter`. Emits fatal diagnostics with `fatal_emitter`.
    pub fn new(fatal_emitter: impl Emitter + Send) -> Self {
        Self::new_boxed(Some(Box::new(fatal_emitter)))
    }

    /// Creates a new `SilentEmitter`. Emits fatal diagnostics with `fatal_emitter` if `Some`.
    pub fn new_boxed(fatal_emitter: Option<Box<DynEmitter>>) -> Self {
        Self { fatal_emitter, note: None }
    }

    /// Creates a new `SilentEmitter` that does not emit any diagnostics at all.
    ///
    /// Same as `new_boxed(None)`.
    pub fn new_silent() -> Self {
        Self::new_boxed(None)
    }

    /// Sets the note to be emitted for fatal diagnostics.
    pub fn with_note(mut self, note: Option<String>) -> Self {
        self.note = note;
        self
    }
}

impl Emitter for SilentEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &mut Diag) {
        let Some(fatal_emitter) = self.fatal_emitter.as_deref_mut() else { return };
        if diagnostic.level != Level::Fatal {
            return;
        }

        if let Some(note) = &self.note {
            let mut diagnostic = diagnostic.clone();
            diagnostic.note(note.clone());
            fatal_emitter.emit_diagnostic(&mut diagnostic);
        } else {
            fatal_emitter.emit_diagnostic(diagnostic);
        }
    }
}

/// Diagnostic emitter that only stores emitted diagnostics.
#[derive(Clone, Debug)]
pub struct LocalEmitter {
    diagnostics: Vec<Diag>,
}

impl Default for LocalEmitter {
    fn default() -> Self {
        Self::new()
    }
}

impl LocalEmitter {
    /// Creates a new `LocalEmitter`.
    pub fn new() -> Self {
        Self { diagnostics: Vec::new() }
    }

    /// Returns a reference to the emitted diagnostics.
    pub fn diagnostics(&self) -> &[Diag] {
        &self.diagnostics
    }

    /// Consumes the emitter and returns the emitted diagnostics.
    pub fn into_diagnostics(self) -> Vec<Diag> {
        self.diagnostics
    }
}

impl Emitter for LocalEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &mut Diag) {
        self.diagnostics.push(diagnostic.clone());
    }
}

/// Determines whether a suggestion should be displayed inline.
/// Based on rustc's implementation in compiler/rustc_errors/src/emitter.rs.
pub(crate) fn should_inline_suggestion(suggestion: &CodeSuggestion) -> bool {
    // Match rustc's conditions exactly:
    // 1. Must be HideCodeInline style
    // 2. Single substitution
    // 3. Single part in that substitution
    // 4. Message is short (< 10 words)
    // 5. Not multiline

    if !matches!(suggestion.style, SuggestionStyle::HideCodeInline) {
        return false;
    }

    suggestion.substitutions.len() == 1
        && suggestion.substitutions[0].parts.len() == 1
        && suggestion.msg.as_str().split_whitespace().count() < 10
        && !suggestion.substitutions[0].parts[0].snippet.contains('\n')
}

/// Formats a suggestion for inline display.
/// Based on rustc's implementation.
pub(crate) fn format_inline_help(message: &str, replacement: &str) -> String {
    let snippet = replacement.trim();

    if snippet.is_empty() || matches!(snippet, "..." | "…") {
        // For removal or placeholder suggestions, just show the help message
        format!("help: {message}")
    } else {
        // Include the suggestion inline
        if message.ends_with(':') {
            format!("help: {message} `{snippet}`")
        } else {
            format!("help: {message}: `{snippet}`")
        }
    }
}

#[cold]
#[inline(never)]
fn io_panic(error: std::io::Error) -> ! {
    panic!("failed to emit diagnostic: {error}");
}

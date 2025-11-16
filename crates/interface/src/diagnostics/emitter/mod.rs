use super::{Diag, Level, MultiSpan, SuggestionStyle};
use crate::{SourceMap, diagnostics::Suggestions};
use std::{any::Any, borrow::Cow, sync::Arc};

mod human;
pub use human::{HumanBufferEmitter, HumanEmitter};

#[cfg(feature = "json")]
mod json;
#[cfg(feature = "json")]
pub use json::JsonEmitter;

mod mem;
pub use mem::InMemoryEmitter;

mod rustc;

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
    fn primary_span_formatted<'a>(
        &self,
        primary_span: &mut Cow<'a, MultiSpan>,
        suggestions: &mut Suggestions,
    ) {
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
                format!("help: {}: `{}`", sugg.msg.as_str(), snippet)
            };
            primary_span.to_mut().push_span_label(part.span, msg);

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

#[cold]
#[inline(never)]
fn io_panic(error: std::io::Error) -> ! {
    panic!("failed to emit diagnostic: {error}");
}

// We replace some characters so the CLI output is always consistent and underlines aligned.
// Keep the following list in sync with `rustc_span::char_width`.
const OUTPUT_REPLACEMENTS: &[(char, &str)] = &[
    // In terminals without Unicode support the following will be garbled, but in *all* terminals
    // the underlying codepoint will be as well. We could gate this replacement behind a "unicode
    // support" gate.
    ('\0', "␀"),
    ('\u{0001}', "␁"),
    ('\u{0002}', "␂"),
    ('\u{0003}', "␃"),
    ('\u{0004}', "␄"),
    ('\u{0005}', "␅"),
    ('\u{0006}', "␆"),
    ('\u{0007}', "␇"),
    ('\u{0008}', "␈"),
    ('\t', "    "), // We do our own tab replacement
    ('\u{000b}', "␋"),
    ('\u{000c}', "␌"),
    ('\u{000d}', "␍"),
    ('\u{000e}', "␎"),
    ('\u{000f}', "␏"),
    ('\u{0010}', "␐"),
    ('\u{0011}', "␑"),
    ('\u{0012}', "␒"),
    ('\u{0013}', "␓"),
    ('\u{0014}', "␔"),
    ('\u{0015}', "␕"),
    ('\u{0016}', "␖"),
    ('\u{0017}', "␗"),
    ('\u{0018}', "␘"),
    ('\u{0019}', "␙"),
    ('\u{001a}', "␚"),
    ('\u{001b}', "␛"),
    ('\u{001c}', "␜"),
    ('\u{001d}', "␝"),
    ('\u{001e}', "␞"),
    ('\u{001f}', "␟"),
    ('\u{007f}', "␡"),
    ('\u{200d}', ""), // Replace ZWJ for consistent terminal output of grapheme clusters.
    ('\u{202a}', "�"), // The following unicode text flow control characters are inconsistently
    ('\u{202b}', "�"), // supported across CLIs and can cause confusion due to the bytes on disk
    ('\u{202c}', "�"), // not corresponding to the visible source code, so we replace them always.
    ('\u{202d}', "�"),
    ('\u{202e}', "�"),
    ('\u{2066}', "�"),
    ('\u{2067}', "�"),
    ('\u{2068}', "�"),
    ('\u{2069}', "�"),
];

pub(crate) fn normalize_whitespace(s: &str) -> String {
    const {
        let mut i = 1;
        while i < OUTPUT_REPLACEMENTS.len() {
            assert!(
                OUTPUT_REPLACEMENTS[i - 1].0 < OUTPUT_REPLACEMENTS[i].0,
                "The OUTPUT_REPLACEMENTS array must be sorted (for binary search to work) \
                and must contain no duplicate entries"
            );
            i += 1;
        }
    }
    // Scan the input string for a character in the ordered table above.
    // If it's present, replace it with its alternative string (it can be more than 1 char!).
    // Otherwise, retain the input char.
    s.chars().fold(String::with_capacity(s.len()), |mut s, c| {
        match OUTPUT_REPLACEMENTS.binary_search_by_key(&c, |(k, _)| *k) {
            Ok(i) => s.push_str(OUTPUT_REPLACEMENTS[i].1),
            _ => s.push(c),
        }
        s
    })
}

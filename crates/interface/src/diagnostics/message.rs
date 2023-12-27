//! Modified from [`rustc_error_messages`](https://github.com/rust-lang/rust/blob/520e30be83b4ed57b609d33166c988d1512bf4f3/compiler/rustc_error_messages/src/lib.rs).

use crate::Span;
use std::borrow::Cow;

#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct DiagnosticMessage {
    inner: Cow<'static, str>,
}

impl From<&'static str> for DiagnosticMessage {
    fn from(value: &'static str) -> Self {
        Self { inner: Cow::Borrowed(value) }
    }
}

impl From<String> for DiagnosticMessage {
    fn from(value: String) -> Self {
        Self { inner: Cow::Owned(value) }
    }
}

impl From<Cow<'static, str>> for DiagnosticMessage {
    fn from(value: Cow<'static, str>) -> Self {
        Self { inner: value }
    }
}

impl DiagnosticMessage {
    /// Returns the message as a string.
    #[inline]
    pub fn as_str(&self) -> &str {
        &self.inner
    }
}

/// A span together with some additional data.
#[derive(Clone, Debug)]
pub struct SpanLabel {
    /// The span we are going to include in the final snippet.
    pub span: Span,

    /// Is this a primary span? This is the "locus" of the message,
    /// and is indicated with a `^^^^` underline, versus `----`.
    pub is_primary: bool,

    /// What label should we attach to this span (if any)?
    pub label: Option<DiagnosticMessage>,
}

/// A collection of `Span`s.
///
/// Spans have two orthogonal attributes:
/// - They can be *primary spans*. In this case they are the locus of the error, and would be
///   rendered with `^^^`.
/// - They can have a *label*. In this case, the label is written next to the mark in the snippet
///   when we render.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct MultiSpan {
    primary_spans: Vec<Span>,
    span_labels: Vec<(Span, DiagnosticMessage)>,
}

impl MultiSpan {
    #[inline]
    pub fn new() -> Self {
        Self { primary_spans: vec![], span_labels: vec![] }
    }

    pub fn from_span(primary_span: Span) -> Self {
        Self { primary_spans: vec![primary_span], span_labels: vec![] }
    }

    pub fn from_spans(mut vec: Vec<Span>) -> Self {
        vec.sort();
        Self { primary_spans: vec, span_labels: vec![] }
    }

    pub fn push_span_label(&mut self, span: Span, label: impl Into<DiagnosticMessage>) {
        self.span_labels.push((span, label.into()));
    }

    /// Selects the first primary span (if any).
    pub fn primary_span(&self) -> Option<Span> {
        self.primary_spans.first().copied()
    }

    /// Returns all primary spans.
    pub fn primary_spans(&self) -> &[Span] {
        &self.primary_spans
    }

    /// Returns `true` if any of the primary spans are displayable.
    pub fn has_primary_spans(&self) -> bool {
        !self.is_dummy()
    }

    /// Returns `true` if this contains only a dummy primary span with any hygienic context.
    pub fn is_dummy(&self) -> bool {
        self.primary_spans.iter().all(|sp| sp.is_dummy())
    }

    /// Replaces all occurrences of one Span with another. Used to move `Span`s in areas that don't
    /// display well (like std macros). Returns whether replacements occurred.
    pub fn replace(&mut self, before: Span, after: Span) -> bool {
        let mut replacements_occurred = false;
        for primary_span in &mut self.primary_spans {
            if *primary_span == before {
                *primary_span = after;
                replacements_occurred = true;
            }
        }
        for span_label in &mut self.span_labels {
            if span_label.0 == before {
                span_label.0 = after;
                replacements_occurred = true;
            }
        }
        replacements_occurred
    }

    pub fn pop_span_label(&mut self) -> Option<(Span, DiagnosticMessage)> {
        self.span_labels.pop()
    }

    /// Returns the strings to highlight. We always ensure that there
    /// is an entry for each of the primary spans -- for each primary
    /// span `P`, if there is at least one label with span `P`, we return
    /// those labels (marked as primary). But otherwise we return
    /// `SpanLabel` instances with empty labels.
    pub fn span_labels(&self) -> Vec<SpanLabel> {
        let is_primary = |span| self.primary_spans.contains(&span);

        let mut span_labels = self
            .span_labels
            .iter()
            .map(|&(span, ref label)| SpanLabel {
                span,
                is_primary: is_primary(span),
                label: Some(label.clone()),
            })
            .collect::<Vec<_>>();

        for &span in &self.primary_spans {
            if !span_labels.iter().any(|sl| sl.span == span) {
                span_labels.push(SpanLabel { span, is_primary: true, label: None });
            }
        }

        span_labels
    }

    /// Returns `true` if any of the span labels is displayable.
    pub fn has_span_labels(&self) -> bool {
        self.span_labels.iter().any(|(sp, _)| !sp.is_dummy())
    }

    /// Clone this `MultiSpan` without keeping any of the span labels - sometimes a `MultiSpan` is
    /// to be re-used in another diagnostic, but includes `span_labels` which have translated
    /// messages. These translated messages would fail to translate without their diagnostic
    /// arguments which are unlikely to be cloned alongside the `Span`.
    pub fn clone_ignoring_labels(&self) -> Self {
        Self { primary_spans: self.primary_spans.clone(), ..Self::new() }
    }
}

impl Default for MultiSpan {
    fn default() -> Self {
        Self::new()
    }
}

impl From<Span> for MultiSpan {
    fn from(span: Span) -> Self {
        Self::from_span(span)
    }
}

impl From<Vec<Span>> for MultiSpan {
    fn from(spans: Vec<Span>) -> Self {
        Self::from_spans(spans)
    }
}

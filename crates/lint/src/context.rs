use solar_interface::{
    Session, Span,
    diagnostics::{
        Applicability, DiagBuilder, DiagId, DiagMsg, Level, MultiSpan, Style, SuggestionStyle,
    },
    source_map::SourceFile,
};
use std::{cell::RefCell, collections::HashSet, sync::Arc};

/// Static metadata shared by a lint's diagnostics.
pub trait Lint {
    /// Stable lint identifier.
    fn id(&self) -> &'static str;
    /// Diagnostic level emitted by this lint.
    fn level(&self) -> Level;
    /// Default diagnostic message.
    fn description(&self) -> &'static str;
    /// Help URL associated with the lint.
    fn help(&self) -> &'static str;
}

/// Toolchain policy applied while emitting lint diagnostics.
pub trait LintPolicy: Send + Sync {
    /// Returns whether this lint is enabled for the current source.
    fn is_lint_enabled(&self, id: &str) -> bool;

    /// Returns whether an otherwise enabled lint is suppressed at `span`.
    fn is_lint_suppressed(&self, id: &str, span: Span) -> bool;
}

/// Context supplied to AST and HIR lint passes.
pub struct LintContext<'s, 'p> {
    sess: &'s Session,
    policy: &'p dyn LintPolicy,
    with_description: bool,
    with_ansi_help: bool,
    source_file: Option<Arc<SourceFile>>,
    emitted: RefCell<HashSet<(&'static str, Span)>>,
}

impl<'s, 'p> LintContext<'s, 'p> {
    /// Creates a lint context.
    pub fn new(
        sess: &'s Session,
        policy: &'p dyn LintPolicy,
        with_description: bool,
        with_ansi_help: bool,
        source_file: Option<Arc<SourceFile>>,
    ) -> Self {
        Self {
            sess,
            policy,
            with_description,
            with_ansi_help,
            source_file,
            emitted: RefCell::default(),
        }
    }

    /// Returns the compiler session.
    pub const fn session(&self) -> &'s Session {
        self.sess
    }

    /// Returns the source file currently being linted, when known.
    pub const fn source_file(&self) -> Option<&Arc<SourceFile>> {
        self.source_file.as_ref()
    }

    /// Returns whether a lint is active for the current source.
    pub fn is_lint_enabled(&self, id: &str) -> bool {
        self.policy.is_lint_enabled(id)
    }

    fn should_emit<L: Lint>(&self, lint: &'static L, span: Span) -> bool {
        self.policy.is_lint_enabled(lint.id()) && !self.policy.is_lint_suppressed(lint.id(), span)
    }

    fn add_help<'a>(&self, diag: DiagBuilder<'a, ()>, help: &'static str) -> DiagBuilder<'a, ()> {
        if self.with_ansi_help { diag.help(hyperlink(help)) } else { diag.help(help) }
    }

    /// Emits a lint's default diagnostic.
    pub fn emit<L: Lint>(&self, lint: &'static L, span: Span) {
        if !self.should_emit(lint, span) || !self.emitted.borrow_mut().insert((lint.id(), span)) {
            return;
        }

        let message = if self.with_description { lint.description() } else { "" };
        let diag = self
            .sess
            .dcx
            .diag(lint.level(), message)
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span));
        self.add_help(diag, lint.help()).emit();
    }

    /// Emits a lint diagnostic with a caller-provided message.
    pub fn emit_with_msg<L: Lint>(&self, lint: &'static L, span: Span, msg: impl Into<DiagMsg>) {
        if !self.should_emit(lint, span) {
            return;
        }

        let diag = self
            .sess
            .dcx
            .diag(lint.level(), msg.into())
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span));
        self.add_help(diag, lint.help()).emit();
    }

    /// Emits a lint diagnostic with a suggestion.
    pub fn emit_with_suggestion<L: Lint>(
        &self,
        lint: &'static L,
        span: Span,
        suggestion: Suggestion,
    ) {
        if !self.should_emit(lint, span) {
            return;
        }

        let message = if self.with_description { lint.description() } else { "" };
        let mut diag = self
            .sess
            .dcx
            .diag(lint.level(), message)
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span));

        diag = match suggestion.kind {
            SuggestionKind::Fix { span: fix_span, applicability, style } => diag
                .span_suggestion_with_style(
                    fix_span.unwrap_or(span),
                    suggestion.description.unwrap_or_default(),
                    suggestion.content,
                    applicability,
                    style,
                ),
            SuggestionKind::Example => {
                if let Some(note) = suggestion.to_note() {
                    diag.note(note.iter().map(|line| line.0.as_str()).collect::<String>())
                } else {
                    diag
                }
            }
        };

        self.add_help(diag, lint.help()).emit();
    }

    /// Returns the source snippet covered by `span`.
    pub fn span_to_snippet(&self, span: Span) -> Option<String> {
        self.sess.source_map().span_to_snippet(span).ok()
    }

    /// Returns the number of leading whitespace bytes on the span's line.
    pub fn get_span_indentation(&self, span: Span) -> usize {
        if !span.is_dummy() {
            let loc = self.sess.source_map().lookup_char_pos(span.lo());
            if let Some(line_text) = loc.file.get_line(loc.line) {
                let col_offset = loc.col.to_usize();
                if col_offset <= line_text.len() {
                    let previous = &line_text[..col_offset];
                    return previous.len() - previous.trim().len();
                }
            }
        }
        0
    }
}

/// The presentation form of a lint suggestion.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SuggestionKind {
    /// A standalone example emitted as a note.
    Example,
    /// A source replacement.
    Fix {
        /// Replacement span, defaulting to the lint span.
        span: Option<Span>,
        /// Applicability of the replacement.
        applicability: Applicability,
        /// Presentation style.
        style: SuggestionStyle,
    },
}

/// A diagnostic suggestion emitted by a lint.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Suggestion {
    description: Option<&'static str>,
    content: String,
    kind: SuggestionKind,
}

impl Suggestion {
    /// Creates a standalone example.
    pub const fn example(content: String) -> Self {
        Self { description: None, content, kind: SuggestionKind::Example }
    }

    /// Creates a source replacement.
    pub const fn fix(content: String, applicability: Applicability) -> Self {
        Self {
            description: None,
            content,
            kind: SuggestionKind::Fix {
                span: None,
                applicability,
                style: SuggestionStyle::ShowCode,
            },
        }
    }

    /// Sets the suggestion description.
    pub const fn with_desc(mut self, description: &'static str) -> Self {
        self.description = Some(description);
        self
    }

    /// Sets the replacement span.
    pub const fn with_span(mut self, span: Span) -> Self {
        if let SuggestionKind::Fix { span: target, .. } = &mut self.kind {
            *target = Some(span);
        }
        self
    }

    /// Sets the suggestion presentation style.
    pub const fn with_style(mut self, style: SuggestionStyle) -> Self {
        if let SuggestionKind::Fix { style: target, .. } = &mut self.kind {
            *target = style;
        }
        self
    }

    fn to_note(&self) -> Option<Vec<(DiagMsg, Style)>> {
        if matches!(self.kind, SuggestionKind::Fix { .. }) {
            return None;
        }

        let mut output = if let Some(description) = self.description {
            vec![
                (DiagMsg::from(description), Style::NoStyle),
                (DiagMsg::from("\n\n"), Style::NoStyle),
            ]
        } else {
            vec![(DiagMsg::from(" \n"), Style::NoStyle)]
        };
        output.extend(
            self.content.lines().map(|line| (DiagMsg::from(format!("{line}\n")), Style::NoStyle)),
        );
        output.push((DiagMsg::from("\n"), Style::NoStyle));
        Some(output)
    }
}

fn hyperlink(url: &'static str) -> String {
    format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\")
}

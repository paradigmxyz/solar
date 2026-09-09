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
    /// Actionable advice attached to diagnostics, separate from the help URL.
    fn diagnostic_help(&self) -> Option<&'static str> {
        None
    }
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

    fn add_help<'a, L: Lint>(
        &self,
        mut diag: DiagBuilder<'a, ()>,
        lint: &'static L,
        help: Option<DiagMsg>,
    ) -> DiagBuilder<'a, ()> {
        if let Some(help) = help.or_else(|| lint.diagnostic_help().map(Into::into)) {
            diag = diag.help(help);
        }
        if self.with_ansi_help { diag.help(hyperlink(lint.help())) } else { diag.help(lint.help()) }
    }

    /// Emits a lint's default diagnostic.
    pub fn emit<L: Lint>(&self, lint: &'static L, span: Span) {
        self.emit_with_optional_help(lint, span, None);
    }

    /// Emits a lint's default diagnostic with caller-provided advice.
    ///
    /// The advice replaces the lint's default advice. Description visibility and deduplication
    /// are the same as for [`Self::emit`], and the help URL remains attached separately.
    pub fn emit_with_help<L: Lint>(&self, lint: &'static L, span: Span, help: impl Into<DiagMsg>) {
        self.emit_with_optional_help(lint, span, Some(help.into()));
    }

    fn emit_with_optional_help<L: Lint>(
        &self,
        lint: &'static L,
        span: Span,
        help: Option<DiagMsg>,
    ) {
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
        self.add_help(diag, lint, help).emit();
    }

    /// Emits a lint diagnostic with a caller-provided message.
    pub fn emit_with_msg<L: Lint>(&self, lint: &'static L, span: Span, msg: impl Into<DiagMsg>) {
        self.emit_with_msg_and_optional_help(lint, span, msg.into(), None);
    }

    /// Emits a caller-provided message and advice, replacing the lint's default advice.
    ///
    /// The lint's help URL is still attached separately.
    pub fn emit_with_msg_and_help<L: Lint>(
        &self,
        lint: &'static L,
        span: Span,
        msg: impl Into<DiagMsg>,
        help: impl Into<DiagMsg>,
    ) {
        self.emit_with_msg_and_optional_help(lint, span, msg.into(), Some(help.into()));
    }

    fn emit_with_msg_and_optional_help<L: Lint>(
        &self,
        lint: &'static L,
        span: Span,
        msg: DiagMsg,
        help: Option<DiagMsg>,
    ) {
        if !self.should_emit(lint, span) {
            return;
        }

        let diag = self
            .sess
            .dcx
            .diag(lint.level(), msg)
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span));
        self.add_help(diag, lint, help).emit();
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
                if let Some(help) = suggestion.to_help() {
                    diag.help(help.iter().map(|line| line.0.as_str()).collect::<String>())
                } else {
                    diag
                }
            }
        };

        self.add_help(diag, lint, None).emit();
    }

    /// Returns the source snippet covered by `span`.
    pub fn span_to_snippet(&self, span: Span) -> Option<String> {
        self.sess.source_map().span_to_snippet(span).ok()
    }

    /// Returns the number of leading whitespace bytes on the span's line.
    pub fn get_span_indentation(&self, span: Span) -> usize {
        if !span.is_dummy() {
            let loc = self.sess.source_map().lookup_char_pos(span.lo());
            if let Some(line_index) = loc.line.checked_sub(1)
                && let Some(line_text) = loc.file.get_line(line_index)
            {
                let col_offset = loc.col.to_usize();
                let byte_offset = line_text
                    .char_indices()
                    .nth(col_offset)
                    .map_or(line_text.len(), |(offset, _)| offset);
                let previous = &line_text[..byte_offset];
                return previous.len() - previous.trim_start().len();
            }
        }
        0
    }
}

/// The presentation form of a lint suggestion.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum SuggestionKind {
    /// A standalone example emitted as help.
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

    fn to_help(&self) -> Option<Vec<(DiagMsg, Style)>> {
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

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{IntoData as _, assert_data_eq, str};
    use solar_interface::{
        BytePos, ColorChoice,
        diagnostics::{DiagCtxt, InMemoryEmitter, JsonEmitter},
        source_map::SourceMap,
    };
    use std::{io, path::PathBuf, sync::Mutex};

    struct TestLint;

    impl Lint for TestLint {
        fn id(&self) -> &'static str {
            "test-lint"
        }

        fn level(&self) -> Level {
            Level::Warning
        }

        fn description(&self) -> &'static str {
            "test lint message"
        }

        fn help(&self) -> &'static str {
            "https://example.com/lint"
        }
    }

    struct HelpLint;

    impl Lint for HelpLint {
        fn id(&self) -> &'static str {
            TestLint.id()
        }

        fn level(&self) -> Level {
            TestLint.level()
        }

        fn description(&self) -> &'static str {
            TestLint.description()
        }

        fn help(&self) -> &'static str {
            TestLint.help()
        }

        fn diagnostic_help(&self) -> Option<&'static str> {
            Some("use a supported value")
        }
    }

    fn emit_help_diagnostics(session: &Session, policy: &dyn LintPolicy, with_description: bool) {
        session.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let file =
            session.source_map().new_source_file(PathBuf::from("test.sol"), "value\n").unwrap();
        let span = Span::new(file.start_pos, file.start_pos + BytePos(5));
        let ctx = LintContext::new(session, policy, with_description, false, Some(file));
        ctx.emit(&HelpLint, span);
        ctx.emit(&HelpLint, span);
        ctx.emit_with_msg(&HelpLint, span, "custom lint message");
        ctx.emit_with_msg_and_help(
            &HelpLint,
            span,
            "conditional lint message",
            "use another value",
        );
        ctx.emit_with_suggestion(
            &HelpLint,
            span,
            Suggestion::fix("replacement".into(), Applicability::MaybeIncorrect)
                .with_desc("replace this value"),
        );
        ctx.emit_with_suggestion(
            &HelpLint,
            span,
            Suggestion::example("replacement".into()).with_desc("consider this example"),
        );
    }

    #[test]
    fn diagnostic_help_text() {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        emit_help_diagnostics(&session, &TestPolicy, true);
        assert_data_eq!(
            trim_line_ends(&session.dcx.emitted_diagnostics().unwrap().to_string()),
            str![[r#"
warning[test-lint]: test lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

warning[test-lint]: custom lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

warning[test-lint]: conditional lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: use another value
  ╰ help: https://example.com/lint

warning[test-lint]: test lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━ help: replace this value: `replacement`
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

warning[test-lint]: test lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: consider this example
  │
  │       replacement
  │
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

"#]]
        );
    }

    #[test]
    fn diagnostic_help_without_description() {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        emit_help_diagnostics(&session, &TestPolicy, false);
        assert_data_eq!(
            trim_line_ends(&session.dcx.emitted_diagnostics().unwrap().to_string()),
            str![[r#"
warning[test-lint]:
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

warning[test-lint]: custom lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

warning[test-lint]: conditional lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: use another value
  ╰ help: https://example.com/lint

warning[test-lint]:
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━ help: replace this value: `replacement`
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

warning[test-lint]:
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ├ help: consider this example
  │
  │       replacement
  │
  │
  ├ help: use a supported value
  ╰ help: https://example.com/lint

"#]]
        );
    }

    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    fn trim_line_ends(text: &str) -> String {
        text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
    }

    impl io::Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }

        fn flush(&mut self) -> io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    #[test]
    fn diagnostic_help_json() {
        let source_map = Arc::new(SourceMap::empty());
        let writer = Arc::new(Mutex::new(Vec::new()));
        let emitter = JsonEmitter::new(
            Box::new(SharedWriter(writer.clone())),
            source_map.clone(),
            ColorChoice::Never,
        )
        .rustc_like(true);
        let session =
            Session::builder().source_map(source_map).dcx(DiagCtxt::new(Box::new(emitter))).build();
        emit_help_diagnostics(&session, &TestPolicy, true);
        let output = String::from_utf8(writer.lock().unwrap().clone()).unwrap();
        let diagnostics = output
            .lines()
            .map(|line| {
                let diagnostic = serde_json::from_str::<serde_json::Value>(line).unwrap();
                serde_json::json!({
                    "message": diagnostic["message"],
                    "code": diagnostic["code"],
                    "children": diagnostic["children"],
                })
            })
            .collect::<Vec<_>>();
        assert_data_eq!(
            serde_json::to_string_pretty(&diagnostics).unwrap(),
            str![[r#"
[
  {
    "children": [
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "use a supported value",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "https://example.com/lint",
        "rendered": null,
        "spans": []
      }
    ],
    "code": {
      "code": "test-lint",
      "explanation": null
    },
    "message": "test lint message"
  },
  {
    "children": [
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "use a supported value",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "https://example.com/lint",
        "rendered": null,
        "spans": []
      }
    ],
    "code": {
      "code": "test-lint",
      "explanation": null
    },
    "message": "custom lint message"
  },
  {
    "children": [
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "use another value",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "https://example.com/lint",
        "rendered": null,
        "spans": []
      }
    ],
    "code": {
      "code": "test-lint",
      "explanation": null
    },
    "message": "conditional lint message"
  },
  {
    "children": [
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "use a supported value",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "https://example.com/lint",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "replace this value",
        "rendered": null,
        "spans": [
          {
            "byte_end": 5,
            "byte_start": 0,
            "column_end": 6,
            "column_start": 1,
            "expansion": null,
            "file_name": "test.sol",
            "is_primary": true,
            "label": null,
            "line_end": 1,
            "line_start": 1,
            "suggested_replacement": "replacement",
            "suggestion_applicability": "MaybeIncorrect",
            "text": [
              {
                "highlight_end": 6,
                "highlight_start": 1,
                "text": "value"
              }
            ]
          }
        ]
      }
    ],
    "code": {
      "code": "test-lint",
      "explanation": null
    },
    "message": "test lint message"
  },
  {
    "children": [
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "consider this example\n\nreplacement\n\n",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "use a supported value",
        "rendered": null,
        "spans": []
      },
      {
        "children": [],
        "code": null,
        "level": "help",
        "message": "https://example.com/lint",
        "rendered": null,
        "spans": []
      }
    ],
    "code": {
      "code": "test-lint",
      "explanation": null
    },
    "message": "test lint message"
  }
]
"#]]
            .is_json()
        );
    }

    struct FilteringPolicy {
        enabled: bool,
        suppressed: bool,
    }

    impl LintPolicy for FilteringPolicy {
        fn is_lint_enabled(&self, _id: &str) -> bool {
            self.enabled
        }

        fn is_lint_suppressed(&self, _id: &str, _span: Span) -> bool {
            self.suppressed
        }
    }

    #[test]
    fn diagnostic_help_respects_policy() {
        for policy in [
            FilteringPolicy { enabled: false, suppressed: false },
            FilteringPolicy { enabled: true, suppressed: true },
        ] {
            let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            emit_help_diagnostics(&session, &policy, true);
            assert_data_eq!(session.dcx.emitted_diagnostics().unwrap().to_string(), str![""]);
            assert_eq!(session.dcx.warn_count(), 0);
        }
    }

    #[test]
    fn diagnostic_help_defaults_to_none() {
        assert_eq!(TestLint.diagnostic_help(), None);
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        session.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let file =
            session.source_map().new_source_file(PathBuf::from("test.sol"), "value").unwrap();
        let span = Span::new(file.start_pos, file.start_pos + BytePos(5));
        let ctx = LintContext::new(&session, &TestPolicy, true, false, Some(file));
        ctx.emit(&TestLint, span);
        assert_data_eq!(
            session.dcx.emitted_diagnostics().unwrap().to_string(),
            str![[r#"
warning[test-lint]: test lint message
  ╭▸ test.sol:1:1
  │
1 │ value
  │ ━━━━━
  │
  ╰ help: https://example.com/lint


"#]]
        );
    }

    #[test]
    fn diagnostic_help_keeps_ansi_links_separate() {
        let (emitter, diagnostics) = InMemoryEmitter::new();
        let session = Session::builder()
            .dcx(
                DiagCtxt::new(Box::new(emitter))
                    .with_flags(|flags| flags.track_diagnostics = false),
            )
            .build();
        let ctx = LintContext::new(&session, &TestPolicy, true, true, None);
        ctx.emit(&HelpLint, Span::DUMMY);
        let diagnostics = diagnostics.read();
        assert_eq!(diagnostics.len(), 1);
        let children = &diagnostics[0].children;
        assert_eq!(children.len(), 2);
        assert_eq!(children[0].level, Level::Help);
        assert_eq!(children[0].label(), "use a supported value");
        assert_eq!(children[1].level, Level::Help);
        assert_eq!(children[1].label(), hyperlink(TestLint.help()));
    }

    #[test]
    fn diagnostic_help_preserves_default_deduplication() {
        let (emitter, diagnostics) = InMemoryEmitter::new();
        let session = Session::builder()
            .dcx(
                DiagCtxt::new(Box::new(emitter))
                    .with_flags(|flags| flags.track_diagnostics = false),
            )
            .build();
        let ctx = LintContext::new(&session, &TestPolicy, false, false, None);
        ctx.emit_with_help(&HelpLint, Span::DUMMY, "first advice");
        ctx.emit_with_help(&HelpLint, Span::DUMMY, "duplicate advice");
        ctx.emit(&HelpLint, Span::DUMMY);
        let second_span = Span::new(BytePos(1), BytePos(2));
        ctx.emit(&HelpLint, second_span);
        ctx.emit_with_help(&HelpLint, second_span, "duplicate advice");
        let diagnostics = diagnostics.read();
        assert_eq!(diagnostics.len(), 2);
        for diagnostic in diagnostics.iter() {
            assert_eq!(diagnostic.label(), "");
            assert_eq!(diagnostic.children.len(), 2);
            assert_eq!(diagnostic.children[1].label(), TestLint.help());
        }
        assert_eq!(diagnostics[0].children[0].label(), "first advice");
        assert_eq!(diagnostics[1].children[0].label(), "use a supported value");
    }

    struct TestPolicy;

    impl LintPolicy for TestPolicy {
        fn is_lint_enabled(&self, _id: &str) -> bool {
            true
        }

        fn is_lint_suppressed(&self, _id: &str, _span: Span) -> bool {
            false
        }
    }

    fn indentation(source: &str, needle: &str) -> usize {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let file = session.source_map().new_source_file(PathBuf::from("test.sol"), source).unwrap();
        let offset = source.find(needle).unwrap();
        let pos = BytePos(file.start_pos.0 + u32::try_from(offset).unwrap());
        let policy = TestPolicy;
        let context = LintContext::new(&session, &policy, false, false, Some(file));
        context.get_span_indentation(Span::new(pos, pos))
    }

    #[test]
    fn indentation_on_final_line() {
        assert_eq!(indentation("first line\n    target", "target"), 4);
    }

    #[test]
    fn indentation_with_utf8_before_span() {
        assert_eq!(indentation("  étarget", "target"), 2);
    }

    #[test]
    fn indentation_ignores_inline_whitespace() {
        assert_eq!(indentation("  item   target", "target"), 2);
    }
}

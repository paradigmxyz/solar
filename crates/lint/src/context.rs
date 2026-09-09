use solar_interface::{
    Session, Span,
    diagnostics::{Diag, DiagId, Level, MultiSpan},
    source_map::SourceFile,
};
use std::{
    cell::RefCell,
    collections::HashSet,
    hash::{DefaultHasher, Hash, Hasher},
    sync::Arc,
};

/// Static metadata shared by a lint's diagnostics.
pub trait Lint {
    /// Stable lint identifier.
    fn id(&self) -> &'static str;
    /// Diagnostic level emitted by this lint.
    fn level(&self) -> Level;
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
    emitted: RefCell<HashSet<u64>>,
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

    /// Decorates and emits a lint diagnostic at `span`.
    ///
    /// The callback runs only if the source policy enables the lint and does not suppress it.
    /// It supplies the primary message and any help, notes, labels, or suggestions. This context
    /// appends the documentation URL, applies description visibility, and emits the diagnostic.
    /// Exact duplicate decorated diagnostics are suppressed, including in UI-testing mode.
    pub fn span_lint<L: Lint>(
        &self,
        lint: &'static L,
        span: Span,
        decorate: impl FnOnce(&mut Diag),
    ) {
        self.span_lint_with_dedup(lint, span, decorate, &self.emitted);
    }

    pub(crate) fn span_lint_with_dedup<L: Lint>(
        &self,
        lint: &'static L,
        span: Span,
        decorate: impl FnOnce(&mut Diag),
        emitted: &RefCell<HashSet<u64>>,
    ) {
        if !self.policy.is_lint_enabled(lint.id())
            || self.policy.is_lint_suppressed(lint.id(), span)
        {
            return;
        }

        let mut diag = self
            .sess
            .dcx
            .diag::<()>(lint.level(), "")
            .code(DiagId::new_str(lint.id()))
            .span(MultiSpan::from_span(span));
        decorate(&mut diag);
        diag = if self.with_ansi_help {
            diag.help(hyperlink(lint.help()))
        } else {
            diag.help(lint.help())
        };
        if !self.with_description {
            diag = diag.primary_message("");
        }

        let mut hasher = DefaultHasher::new();
        (*diag).hash(&mut hasher);
        if !emitted.borrow_mut().insert(hasher.finish()) {
            diag.cancel();
            return;
        }
        diag.emit();
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

fn hyperlink(url: &'static str) -> String {
    format!("\x1b]8;;{url}\x1b\\{url}\x1b]8;;\x1b\\")
}

#[cfg(test)]
mod tests {
    use super::*;
    use snapbox::{IntoData as _, assert_data_eq, str};
    use solar_interface::{
        BytePos, ColorChoice,
        diagnostics::{Applicability, DiagCtxt, InMemoryEmitter, JsonEmitter},
        source_map::SourceMap,
    };
    use std::{cell::Cell, io, path::PathBuf, sync::Mutex};

    struct TestLint;

    impl Lint for TestLint {
        fn id(&self) -> &'static str {
            "test-lint"
        }
        fn level(&self) -> Level {
            Level::Warning
        }
        fn help(&self) -> &'static str {
            "https://example.com/lint"
        }
    }

    struct Policy {
        enabled: bool,
        suppressed: bool,
    }

    impl LintPolicy for Policy {
        fn is_lint_enabled(&self, _id: &str) -> bool {
            self.enabled
        }
        fn is_lint_suppressed(&self, _id: &str, _span: Span) -> bool {
            self.suppressed
        }
    }

    const ENABLED: Policy = Policy { enabled: true, suppressed: false };

    fn emit_decorated_diagnostics(session: &Session, with_description: bool) {
        session.dcx.set_flags(|flags| flags.track_diagnostics = false);
        let file = session
            .source_map()
            .new_source_file(PathBuf::from("test.sol"), "value other\n")
            .unwrap();
        let span = Span::new(file.start_pos, file.start_pos + BytePos(5));
        let other = Span::new(file.start_pos + BytePos(6), file.start_pos + BytePos(11));
        let ctx = LintContext::new(session, &ENABLED, with_description, false, Some(file));
        ctx.span_lint(&TestLint, span, |diag| {
            diag.primary_message("test lint message");
            diag.help("use a supported value");
            diag.note("the other value is related");
            diag.span_label(other, "related value");
            diag.multipart_suggestion(
                "replace both values",
                vec![(span, "first".into()), (other, "second".into())],
                Applicability::MaybeIncorrect,
            );
        });
        ctx.span_lint(&TestLint, span, |diag| {
            diag.primary_message("another lint message");
            diag.span_suggestion(
                span,
                "replace this value",
                "replacement",
                Applicability::MachineApplicable,
            );
        });
    }

    fn trim_line_ends(text: &str) -> String {
        text.lines().map(str::trim_end).collect::<Vec<_>>().join("\n")
    }

    #[test]
    fn decorated_diagnostic_text() {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        emit_decorated_diagnostics(&session, true);
        assert_data_eq!(
            trim_line_ends(&session.dcx.emitted_diagnostics().unwrap().to_string()),
            str![[r#"
warning[test-lint]: test lint message
  ╭▸ test.sol:1:1
  │
1 │ value other
  │ ━━━━━ ───── related value
  │
  ├ help: use a supported value
  ├ note: the other value is related
  ╰ help: https://example.com/lint
help: replace both values
  ╭╴
1 - value other
1 + first second
  ╰╴

warning[test-lint]: another lint message
  ╭▸ test.sol:1:1
  │
1 │ value other
  │ ━━━━━ help: replace this value: `replacement`
  │
  ╰ help: https://example.com/lint

"#]]
        );
    }

    #[test]
    fn decorated_diagnostic_without_description() {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        emit_decorated_diagnostics(&session, false);
        assert_data_eq!(
            trim_line_ends(&session.dcx.emitted_diagnostics().unwrap().to_string()),
            str![[r#"
warning[test-lint]:
  ╭▸ test.sol:1:1
  │
1 │ value other
  │ ━━━━━ ───── related value
  │
  ├ help: use a supported value
  ├ note: the other value is related
  ╰ help: https://example.com/lint
help: replace both values
  ╭╴
1 - value other
1 + first second
  ╰╴

warning[test-lint]:
  ╭▸ test.sol:1:1
  │
1 │ value other
  │ ━━━━━ help: replace this value: `replacement`
  │
  ╰ help: https://example.com/lint

"#]]
        );
    }

    struct SharedWriter(Arc<Mutex<Vec<u8>>>);

    impl io::Write for SharedWriter {
        fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
            self.0.lock().unwrap().write(buf)
        }
        fn flush(&mut self) -> io::Result<()> {
            self.0.lock().unwrap().flush()
        }
    }

    #[test]
    fn decorated_diagnostic_json() {
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
        emit_decorated_diagnostics(&session, true);
        let output = String::from_utf8(writer.lock().unwrap().clone()).unwrap();
        let diagnostics = output
            .lines()
            .map(|line| {
                let mut diagnostic = serde_json::from_str::<serde_json::Value>(line).unwrap();
                diagnostic.as_object_mut().unwrap().remove("rendered");
                diagnostic
            })
            .collect::<Vec<_>>();
        assert_data_eq!(
            serde_json::to_string_pretty(&diagnostics).unwrap(),
            str![[r#"
[
  {
    "$message_type": "diagnostic",
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
        "level": "note",
        "message": "the other value is related",
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
        "message": "replace both values",
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
            "suggested_replacement": "first",
            "suggestion_applicability": "MaybeIncorrect",
            "text": [
              {
                "highlight_end": 6,
                "highlight_start": 1,
                "text": "value other"
              }
            ]
          },
          {
            "byte_end": 11,
            "byte_start": 6,
            "column_end": 12,
            "column_start": 7,
            "expansion": null,
            "file_name": "test.sol",
            "is_primary": true,
            "label": null,
            "line_end": 1,
            "line_start": 1,
            "suggested_replacement": "second",
            "suggestion_applicability": "MaybeIncorrect",
            "text": [
              {
                "highlight_end": 12,
                "highlight_start": 7,
                "text": "value other"
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
    "level": "warning",
    "message": "test lint message",
    "spans": [
      {
        "byte_end": 11,
        "byte_start": 6,
        "column_end": 12,
        "column_start": 7,
        "expansion": null,
        "file_name": "test.sol",
        "is_primary": false,
        "label": "related value",
        "line_end": 1,
        "line_start": 1,
        "suggested_replacement": null,
        "suggestion_applicability": null,
        "text": [
          {
            "highlight_end": 12,
            "highlight_start": 7,
            "text": "value other"
          }
        ]
      },
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
        "suggested_replacement": null,
        "suggestion_applicability": null,
        "text": [
          {
            "highlight_end": 6,
            "highlight_start": 1,
            "text": "value other"
          }
        ]
      }
    ]
  },
  {
    "$message_type": "diagnostic",
    "children": [
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
            "suggestion_applicability": "MachineApplicable",
            "text": [
              {
                "highlight_end": 6,
                "highlight_start": 1,
                "text": "value other"
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
    "level": "warning",
    "message": "another lint message",
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
        "suggested_replacement": null,
        "suggestion_applicability": null,
        "text": [
          {
            "highlight_end": 6,
            "highlight_start": 1,
            "text": "value other"
          }
        ]
      }
    ]
  }
]
"#]]
            .is_json()
        );
    }

    #[test]
    fn source_policy_prevents_decoration() {
        for policy in [
            Policy { enabled: false, suppressed: false },
            Policy { enabled: true, suppressed: true },
        ] {
            let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
            let ctx = LintContext::new(&session, &policy, true, false, None);
            ctx.span_lint(&TestLint, Span::DUMMY, |_| panic!("suppressed decorator ran"));
            assert_eq!(session.dcx.warn_count(), 0);
            assert_data_eq!(session.dcx.emitted_diagnostics().unwrap().to_string(), str![""]);
        }
    }

    #[test]
    fn keeps_ansi_url_separate() {
        let (emitter, diagnostics) = InMemoryEmitter::new();
        let session = Session::builder()
            .dcx(
                DiagCtxt::new(Box::new(emitter))
                    .with_flags(|flags| flags.track_diagnostics = false),
            )
            .build();
        let ctx = LintContext::new(&session, &ENABLED, true, true, None);
        ctx.span_lint(&TestLint, Span::DUMMY, |diag| {
            diag.primary_message("test message");
            diag.help("first advice");
        });
        let diagnostics = diagnostics.read();
        assert_eq!(diagnostics.len(), 1);
        assert_eq!(diagnostics[0].children.len(), 2);
        assert_eq!(diagnostics[0].children[0].label(), "first advice");
        assert_eq!(diagnostics[0].children[1].label(), hyperlink(TestLint.help()));
    }

    fn check_deduplication(ui_testing: bool) {
        let (emitter, diagnostics) = InMemoryEmitter::new();
        let session = Session::builder()
            .dcx(DiagCtxt::new(Box::new(emitter)).with_flags(|flags| {
                flags.track_diagnostics = false;
                flags.deduplicate_diagnostics = !ui_testing;
            }))
            .build();
        let ctx = LintContext::new(&session, &ENABLED, true, false, None);
        let calls = Cell::new(0);
        for _ in 0..2 {
            ctx.span_lint(&TestLint, Span::DUMMY, |diag| {
                calls.set(calls.get() + 1);
                diag.primary_message("first message");
                diag.help("first advice");
            });
        }
        ctx.span_lint(&TestLint, Span::DUMMY, |diag| {
            diag.primary_message("second message");
            diag.help("first advice");
        });
        ctx.span_lint(&TestLint, Span::DUMMY, |diag| {
            diag.primary_message("first message");
            diag.help("second advice");
        });
        for replacement in ["one", "two", "two"] {
            ctx.span_lint(&TestLint, Span::DUMMY, |diag| {
                diag.primary_message("first message");
                diag.span_suggestion(
                    Span::DUMMY,
                    "replace",
                    replacement,
                    Applicability::MaybeIncorrect,
                );
            });
        }
        assert_eq!(calls.get(), 2);
        assert_eq!(diagnostics.read().len(), 5);
        assert_eq!(session.dcx.warn_count(), 5);
    }

    #[test]
    fn exact_duplicate_diagnostics() {
        check_deduplication(false);
    }

    #[test]
    fn exact_duplicate_diagnostics_in_ui_mode() {
        check_deduplication(true);
    }

    #[test]
    fn hidden_messages_deduplicate_after_decoration() {
        let (emitter, diagnostics) = InMemoryEmitter::new();
        let session = Session::builder()
            .dcx(DiagCtxt::new(Box::new(emitter)).with_flags(|flags| {
                flags.track_diagnostics = false;
                flags.deduplicate_diagnostics = false;
            }))
            .build();
        let ctx = LintContext::new(&session, &ENABLED, false, false, None);
        for message in ["first message", "second message"] {
            ctx.span_lint(&TestLint, Span::DUMMY, |diag| {
                diag.primary_message(message);
                diag.help("same advice");
            });
        }
        assert_eq!(diagnostics.read().len(), 1);
        assert_eq!(diagnostics.read()[0].label(), "");
    }

    fn indentation(source: &str, needle: &str) -> usize {
        let session = Session::builder().with_buffer_emitter(ColorChoice::Never).build();
        let file = session.source_map().new_source_file(PathBuf::from("test.sol"), source).unwrap();
        let offset = source.find(needle).unwrap();
        let pos = BytePos(file.start_pos.0 + u32::try_from(offset).unwrap());
        let context = LintContext::new(&session, &ENABLED, false, false, Some(file));
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

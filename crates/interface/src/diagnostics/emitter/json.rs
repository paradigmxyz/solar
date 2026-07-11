//! JSON diagnostics, including rustc-compatible and solc-compatible representations.
//!
//! The rustc-compatible representation is modified from rustc's [`JsonEmitter`](https://github.com/rust-lang/rust/blob/3b58636b30eb364ac72aeaf03d46347084ed87d1/compiler/rustc_errors/src/json.rs).
//! It preserves this codebase's source-map model, which has no macro expansion metadata.

use super::{Emitter, human::HumanBufferEmitter, io_panic};
use crate::{
    Span,
    diagnostics::{
        Applicability, CodeSuggestion, Diag, Level, MultiSpan, SpanLabel, SubDiagnostic,
    },
    source_map::{LineInfo, SourceFile, SourceMap},
};
use anstream::ColorChoice;
use serde::{Deserialize, Serialize};
use solar_config::HumanEmitterKind;
use std::{borrow::Cow, io, sync::Arc};

/// Diagnostic emitter that emits diagnostics as JSON.
pub struct JsonEmitter {
    writer: Box<dyn io::Write + Send>,
    pretty: bool,
    rustc_like: bool,

    human_emitter: HumanBufferEmitter,
}

impl Emitter for JsonEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &mut Diag) {
        self.emit_diagnostic_ref(diagnostic);
    }

    fn emit_diagnostic_ref(&mut self, diagnostic: &Diag) {
        if self.rustc_like {
            let diagnostic = self.diagnostic(diagnostic);
            self.emit(&JsonDiagnosticMessage::Diagnostic(diagnostic))
        } else {
            let diagnostic = self.solc_diagnostic(diagnostic);
            self.emit(&diagnostic)
        }
        .unwrap_or_else(|e| io_panic(e));
    }

    fn source_map(&self) -> Option<&Arc<SourceMap>> {
        Emitter::source_map(&self.human_emitter)
    }
}

impl JsonEmitter {
    /// Creates a new `JsonEmitter` that writes to given writer.
    pub fn new(
        writer: Box<dyn io::Write + Send>,
        source_map: Arc<SourceMap>,
        color_choice: ColorChoice,
    ) -> Self {
        let color_choice =
            if color_choice == ColorChoice::Auto { ColorChoice::Never } else { color_choice };
        Self {
            writer,
            pretty: false,
            rustc_like: false,
            human_emitter: HumanBufferEmitter::new(color_choice).source_map(Some(source_map)),
        }
    }

    /// Sets whether to pretty print the JSON.
    pub fn pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    /// Sets whether to emit diagnostics in a format that is compatible with rustc.
    ///
    /// Mainly used in UI testing.
    pub fn rustc_like(mut self, yes: bool) -> Self {
        self.rustc_like = yes;
        self
    }

    /// Sets whether to emit diagnostics in a way that is suitable for UI testing.
    pub fn ui_testing(mut self, yes: bool) -> Self {
        self.human_emitter = self.human_emitter.ui_testing(yes);
        self
    }

    /// Sets the human emitter kind for rendered messages.
    pub fn human_kind(mut self, kind: HumanEmitterKind) -> Self {
        self.human_emitter = self.human_emitter.human_kind(kind);
        self
    }

    /// Sets the terminal width for formatting.
    pub fn terminal_width(mut self, width: Option<usize>) -> Self {
        self.human_emitter = self.human_emitter.terminal_width(width);
        self
    }

    fn source_map(&self) -> &Arc<SourceMap> {
        Emitter::source_map(self).unwrap()
    }

    fn diagnostic(&mut self, diagnostic: &Diag) -> JsonDiagnostic<'static> {
        // Unlike the human emitter, all suggestions are preserved as separate diagnostic children.
        let children = diagnostic
            .children
            .iter()
            .map(|sub| self.sub_diagnostic(sub))
            .chain(diagnostic.suggestions.iter().map(|sugg| self.suggestion_to_diagnostic(sugg)))
            .collect();

        JsonDiagnostic {
            message: Cow::Owned(diagnostic.label().into_owned()),
            code: diagnostic.id().map(|code| JsonDiagnosticCode {
                code: Cow::Owned(code.to_string()),
                explanation: None,
            }),
            level: Cow::Borrowed(diagnostic.level.to_str()),
            spans: self.spans(&diagnostic.span),
            children,
            rendered: Some(Cow::Owned(self.emit_diagnostic_to_buffer(diagnostic))),
        }
    }

    fn sub_diagnostic(&self, diagnostic: &SubDiagnostic) -> JsonDiagnostic<'static> {
        JsonDiagnostic {
            message: Cow::Owned(diagnostic.label().into_owned()),
            code: None,
            level: Cow::Borrowed(diagnostic.level.to_str()),
            spans: self.spans(&diagnostic.span),
            children: vec![],
            rendered: None,
        }
    }

    fn suggestion_to_diagnostic(&self, sugg: &CodeSuggestion) -> JsonDiagnostic<'static> {
        // Collect all spans from all substitutions
        let spans = sugg
            .substitutions
            .iter()
            .flat_map(|sub| sub.parts.iter())
            .map(|part| {
                self.span_with_suggestion(part.span, part.snippet.to_string(), sugg.applicability)
            })
            .collect();

        JsonDiagnostic {
            message: Cow::Owned(sugg.msg.as_str().to_string()),
            code: None,
            level: Cow::Borrowed("help"),
            spans,
            children: vec![],
            rendered: None,
        }
    }

    fn spans(&self, msp: &MultiSpan) -> Vec<JsonDiagnosticSpan<'static>> {
        msp.span_labels().iter().map(|label| self.span(label)).collect()
    }

    fn span(&self, label: &SpanLabel) -> JsonDiagnosticSpan<'static> {
        let sm = &**self.source_map();
        let span = label.span;
        let start = sm.lookup_char_pos(span.lo());
        let end = sm.lookup_char_pos(span.hi());
        JsonDiagnosticSpan {
            file_name: Cow::Owned(sm.filename_for_diagnostics(&start.file.name).to_string()),
            byte_start: start.file.original_relative_byte_pos(span.lo()).0,
            byte_end: start.file.original_relative_byte_pos(span.hi()).0,
            line_start: start.line,
            line_end: end.line,
            column_start: start.col.0 + 1,
            column_end: end.col.0 + 1,
            is_primary: label.is_primary,
            text: self.span_lines(span),
            label: label.label.as_ref().map(|msg| Cow::Owned(msg.as_str().to_string())),
            suggested_replacement: None,
            suggestion_applicability: None,
            expansion: None,
        }
    }

    fn span_with_suggestion(
        &self,
        span: Span,
        replacement: String,
        applicability: Applicability,
    ) -> JsonDiagnosticSpan<'static> {
        let sm = &**self.source_map();
        let start = sm.lookup_char_pos(span.lo());
        let span = if start.col.0 == 0
            && replacement.is_empty()
            && span.hi() < start.file.end_position()
            && start.file.contains(span.hi())
            && start
                .file
                .src
                .get(start.file.relative_position(span.hi()).to_usize()..)
                .is_some_and(|after| after.starts_with('\n'))
        {
            span.with_hi(span.hi() + crate::BytePos(1))
        } else {
            span
        };
        let start = sm.lookup_char_pos(span.lo());
        let end = sm.lookup_char_pos(span.hi());
        JsonDiagnosticSpan {
            file_name: Cow::Owned(sm.filename_for_diagnostics(&start.file.name).to_string()),
            byte_start: start.file.original_relative_byte_pos(span.lo()).0,
            byte_end: start.file.original_relative_byte_pos(span.hi()).0,
            line_start: start.line,
            line_end: end.line,
            column_start: start.col.0 + 1,
            column_end: end.col.0 + 1,
            is_primary: true,
            text: self.span_lines(span),
            label: None,
            suggested_replacement: Some(Cow::Owned(replacement)),
            suggestion_applicability: Some(applicability),
            expansion: None,
        }
    }

    fn span_lines(&self, span: Span) -> Vec<JsonDiagnosticSpanLine<'static>> {
        let Ok(f) = self.source_map().span_to_lines(span) else { return Vec::new() };
        let sf = &*f.file;
        f.data.iter().map(|line| self.span_line(sf, line)).collect()
    }

    fn span_line(&self, sf: &SourceFile, line: &LineInfo) -> JsonDiagnosticSpanLine<'static> {
        JsonDiagnosticSpanLine {
            text: Cow::Owned(
                sf.get_line(line.line_index).map_or_else(String::new, |l| l.to_string()),
            ),
            highlight_start: line.start_col.0 + 1,
            highlight_end: line.end_col.0 + 1,
        }
    }

    /// Converts a diagnostic to a solc-compatible JSON diagnostic.
    pub fn solc_diagnostic<'a>(
        &mut self,
        diagnostic: &'a crate::diagnostics::Diag,
    ) -> SolcDiagnostic<'a> {
        let primary = diagnostic.span.primary_span();
        let file = primary
            .map(|span| {
                let sm = &**self.source_map();
                let start = sm.lookup_char_pos(span.lo());
                sm.filename_for_diagnostics(&start.file.name).to_string()
            })
            .unwrap_or_default();

        let severity = to_severity(diagnostic.level);

        SolcDiagnostic {
            source_location: primary
                .is_some()
                .then(|| self.solc_span(&diagnostic.span, &file, None)),
            secondary_source_locations: diagnostic
                .children
                .iter()
                .map(|sub| self.solc_span(&sub.span, &file, Some(sub.label())))
                .collect(),
            r#type: Cow::Borrowed(match severity {
                Severity::Error => match diagnostic.level {
                    Level::Bug => "InternalCompilerError",
                    Level::Fatal => "FatalError",
                    Level::Error => "Exception",
                    _ => unreachable!(),
                },
                Severity::Warning => "Warning",
                Severity::Info => "Info",
            }),
            component: Cow::Borrowed("general"),
            severity,
            error_code: diagnostic.id().map(Cow::Borrowed),
            message: diagnostic.label(),
            formatted_message: Some(Cow::Owned(self.emit_diagnostic_to_buffer(diagnostic))),
        }
    }

    fn solc_span<'a>(
        &self,
        span: &MultiSpan,
        file: &str,
        message: Option<Cow<'a, str>>,
    ) -> SourceLocation<'a> {
        let sm = &**self.source_map();
        let sp = span.primary_span();
        SourceLocation {
            file: sp
                .map(|span| {
                    let start = sm.lookup_char_pos(span.lo());
                    Cow::Owned(sm.filename_for_diagnostics(&start.file.name).to_string())
                })
                .unwrap_or_else(|| Cow::Owned(file.to_owned())),
            start: sp
                .map(|span| {
                    let start = sm.lookup_char_pos(span.lo());
                    start.file.original_relative_byte_pos(span.lo()).0
                })
                .unwrap_or(0),
            end: sp
                .map(|span| {
                    let end = sm.lookup_char_pos(span.hi());
                    end.file.original_relative_byte_pos(span.hi()).0
                })
                .unwrap_or(0),
            message,
        }
    }

    fn emit_diagnostic_to_buffer(&mut self, diagnostic: &crate::diagnostics::Diag) -> String {
        self.human_emitter.emit_diagnostic_ref(diagnostic);
        std::mem::take(self.human_emitter.buffer_mut())
    }

    fn emit<T: ?Sized + Serialize>(&mut self, value: &T) -> io::Result<()> {
        if self.pretty {
            serde_json::to_writer_pretty(&mut *self.writer, value)
        } else {
            serde_json::to_writer(&mut *self.writer, value)
        }?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

// Rustc-like JSON format.

/// A rustc-like JSON message emitted by [`JsonEmitter::rustc_like`].
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "$message_type", rename_all = "snake_case")]
pub enum JsonDiagnosticMessage<'a> {
    /// A diagnostic message.
    Diagnostic(#[serde(borrow)] JsonDiagnostic<'a>),
}

/// A rustc-like JSON diagnostic emitted by [`JsonEmitter`].
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonDiagnostic<'a> {
    /// The primary error message.
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    /// The diagnostic code.
    #[serde(borrow)]
    pub code: Option<JsonDiagnosticCode<'a>>,
    /// "error: internal compiler error", "error", "warning", "note", or "help".
    #[serde(borrow)]
    pub level: Cow<'a, str>,
    /// The diagnostic spans.
    #[serde(borrow)]
    pub spans: Vec<JsonDiagnosticSpan<'a>>,
    /// Associated diagnostic messages.
    #[serde(borrow)]
    pub children: Vec<Self>,
    /// The message as the compiler would render it.
    #[serde(borrow)]
    pub rendered: Option<Cow<'a, str>>,
}

/// A source span in a rustc-like JSON diagnostic.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonDiagnosticSpan<'a> {
    /// The diagnostic file name.
    #[serde(borrow)]
    pub file_name: Cow<'a, str>,
    /// The start byte offset.
    pub byte_start: u32,
    /// The end byte offset.
    pub byte_end: u32,
    /// 1-based.
    pub line_start: usize,
    /// 1-based.
    pub line_end: usize,
    /// 1-based, character offset.
    pub column_start: usize,
    /// 1-based, character offset.
    pub column_end: usize,
    /// Is this a "primary" span -- meaning the point, or one of the points,
    /// where the error occurred?
    pub is_primary: bool,
    /// Source text from the start of line_start to the end of line_end.
    #[serde(borrow)]
    pub text: Vec<JsonDiagnosticSpanLine<'a>>,
    /// Label that should be placed at this location, if any.
    #[serde(borrow)]
    pub label: Option<Cow<'a, str>>,
    /// If we are suggesting a replacement, this will contain text
    /// that should be sliced in atop this span.
    #[serde(borrow)]
    pub suggested_replacement: Option<Cow<'a, str>>,
    /// How confidently the suggested replacement can be applied.
    pub suggestion_applicability: Option<Applicability>,
    /// Macro expansion that produced this span, if available.
    #[serde(borrow)]
    pub expansion: Option<Box<JsonDiagnosticSpanMacroExpansion<'a>>>,
}

/// A macro expansion represented in a rustc-like JSON diagnostic span.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonDiagnosticSpanMacroExpansion<'a> {
    /// The span where the macro was invoked.
    #[serde(borrow)]
    pub span: JsonDiagnosticSpan<'a>,
    /// The macro declaration name.
    #[serde(borrow)]
    pub macro_decl_name: Cow<'a, str>,
    /// The span where the macro was defined.
    #[serde(borrow)]
    pub def_site_span: JsonDiagnosticSpan<'a>,
}

/// A source line in a rustc-like JSON diagnostic span.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonDiagnosticSpanLine<'a> {
    /// The source text.
    #[serde(borrow)]
    pub text: Cow<'a, str>,

    /// 1-based, character offset in self.text.
    pub highlight_start: usize,

    /// 1-based, character offset in self.text.
    pub highlight_end: usize,
}

/// A diagnostic code in a rustc-like JSON diagnostic.
#[derive(Debug, Serialize, Deserialize)]
pub struct JsonDiagnosticCode<'a> {
    /// The code itself.
    #[serde(borrow)]
    pub code: Cow<'a, str>,
    /// An explanation for the code.
    #[serde(borrow)]
    pub explanation: Option<Cow<'a, str>>,
}

// Solc JSON format.

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SolcDiagnostic<'a> {
    #[serde(borrow)]
    pub source_location: Option<SourceLocation<'a>>,
    #[serde(borrow)]
    pub secondary_source_locations: Vec<SourceLocation<'a>>,
    #[serde(borrow)]
    pub r#type: Cow<'a, str>,
    #[serde(borrow)]
    pub component: Cow<'a, str>,
    pub severity: Severity,
    #[serde(borrow)]
    pub error_code: Option<Cow<'a, str>>,
    #[serde(borrow)]
    pub message: Cow<'a, str>,
    #[serde(borrow)]
    pub formatted_message: Option<Cow<'a, str>>,
}

impl SolcDiagnostic<'_> {
    /// Returns `true` if this diagnostic has error severity.
    pub fn is_error(&self) -> bool {
        matches!(self.severity, Severity::Error)
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SourceLocation<'a> {
    #[serde(borrow)]
    pub file: Cow<'a, str>,
    pub start: u32,
    pub end: u32,
    // Some if it's a secondary source location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    #[serde(borrow)]
    pub message: Option<Cow<'a, str>>,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

fn to_severity(level: Level) -> Severity {
    match level {
        Level::Bug | Level::Fatal | Level::Error => Severity::Error,
        Level::Warning => Severity::Warning,
        Level::Note
        | Level::OnceNote
        | Level::FailureNote
        | Level::Help
        | Level::OnceHelp
        | Level::Allow => Severity::Info,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_line_deletion_includes_newline() {
        let source_map = Arc::new(SourceMap::empty());
        source_map
            .new_source_file(crate::source_map::FileName::custom("test.sol"), "foo\nbar\n")
            .unwrap();
        let emitter = JsonEmitter::new(Box::new(io::sink()), source_map);

        let span = emitter.span_with_suggestion(
            Span::new(crate::BytePos(0), crate::BytePos(3)),
            String::new(),
            Applicability::MachineApplicable,
        );

        assert_eq!(span.byte_start, 0);
        assert_eq!(span.byte_end, 4);
        assert_eq!(span.line_start, 1);
        assert_eq!(span.line_end, 2);
        assert_eq!(span.column_end, 1);
        assert!(span.expansion.is_none());
    }

    #[test]
    fn solc_diagnostic_serializes_borrowed_strings() {
        let diagnostic = SolcDiagnostic {
            source_location: Some(SourceLocation {
                file: Cow::Borrowed("input.sol"),
                start: 0,
                end: 1,
                message: Some(Cow::Borrowed("borrowed \"message\"")),
            }),
            secondary_source_locations: Vec::new(),
            r#type: Cow::Borrowed("Exception\nquoted"),
            component: Cow::Borrowed("general"),
            severity: Severity::Error,
            error_code: Some(Cow::Borrowed("1234")),
            message: Cow::Borrowed("borrowed message"),
            formatted_message: None,
        };

        let json = serde_json::to_string(&diagnostic).unwrap();
        assert!(json.contains(r#""type":"Exception\nquoted""#));
        assert!(json.contains(r#""message":"borrowed \"message\"""#));

        assert!(matches!(diagnostic.r#type, Cow::Borrowed(_)));
        assert!(matches!(diagnostic.component, Cow::Borrowed("general")));
    }
}

use super::{io_panic, Emitter, HumanEmitter};
use crate::{
    diagnostics::{Level, MultiSpan, SpanLabel},
    source_map::{LineInfo, SourceFile},
    SourceMap, Span,
};
use anstream::ColorChoice;
use serde::Serialize;
use std::{
    io,
    sync::{Arc, Mutex, PoisonError},
};
use sulk_data_structures::sync::Lrc;

/// Diagnostic emitter that emits diagnostics as JSON.
pub struct JsonEmitter {
    writer: Box<dyn io::Write>,
    pretty: bool,
    rustc_like: bool,

    human_emitter: HumanEmitter,
    buffer: LocalBuffer,
}

impl Emitter for JsonEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &crate::diagnostics::Diagnostic) {
        if self.rustc_like {
            let diagnostic = self.diagnostic(diagnostic);
            self.emit(&EmitTyped::Diagnostic(diagnostic))
        } else {
            let diagnostic = self.solc_diagnostic(diagnostic);
            self.emit(&diagnostic)
        }
        .unwrap_or_else(|e| io_panic(e));
    }

    fn source_map(&self) -> Option<&Lrc<SourceMap>> {
        Emitter::source_map(&self.human_emitter)
    }
}

impl JsonEmitter {
    /// Creates a new `JsonEmitter` that writes to given writer.
    pub fn new(writer: Box<dyn io::Write>, source_map: Lrc<SourceMap>) -> Self {
        let buffer = LocalBuffer::new();
        Self {
            writer,
            pretty: false,
            rustc_like: false,
            human_emitter: HumanEmitter::new(Box::new(buffer.clone()), ColorChoice::Never)
                .source_map(Some(source_map)),
            buffer,
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

    fn source_map(&self) -> &Lrc<SourceMap> {
        Emitter::source_map(self).unwrap()
    }

    fn diagnostic(&mut self, diagnostic: &crate::diagnostics::Diagnostic) -> Diagnostic {
        Diagnostic {
            message: diagnostic.label().into_owned(),
            code: diagnostic.id().map(|code| DiagnosticCode { code, explanation: None }),
            level: diagnostic.level.to_str(),
            spans: self.spans(&diagnostic.span),
            children: diagnostic.children.iter().map(|sub| self.sub_diagnostic(sub)).collect(),
            rendered: Some(self.emit_diagnostic_to_buffer(diagnostic)),
        }
    }

    fn sub_diagnostic(&self, diagnostic: &crate::diagnostics::SubDiagnostic) -> Diagnostic {
        Diagnostic {
            message: diagnostic.label().into_owned(),
            code: None,
            level: diagnostic.level.to_str(),
            spans: self.spans(&diagnostic.span),
            children: vec![],
            rendered: None,
        }
    }

    fn spans(&self, msp: &MultiSpan) -> Vec<DiagnosticSpan> {
        msp.span_labels().iter().map(|label| self.span(label)).collect()
    }

    fn span(&self, label: &SpanLabel) -> DiagnosticSpan {
        let sm = &**self.source_map();
        let span = label.span;
        let start = sm.lookup_char_pos(span.lo());
        let end = sm.lookup_char_pos(span.hi());
        DiagnosticSpan {
            file_name: sm.filename_for_diagnostics(&start.file.name).to_string(),
            byte_start: start.file.original_relative_byte_pos(span.lo()).0,
            byte_end: start.file.original_relative_byte_pos(span.hi()).0,
            line_start: start.line,
            line_end: end.line,
            column_start: start.col.0 + 1,
            column_end: end.col.0 + 1,
            is_primary: label.is_primary,
            text: self.span_lines(span),
            label: label.label.as_ref().map(|msg| msg.as_str().into()),
        }
    }

    fn span_lines(&self, span: Span) -> Vec<DiagnosticSpanLine> {
        let Ok(f) = self.source_map().span_to_lines(span) else { return Vec::new() };
        let sf = &*f.file;
        f.lines.iter().map(|line| self.span_line(sf, line)).collect()
    }

    fn span_line(&self, sf: &SourceFile, line: &LineInfo) -> DiagnosticSpanLine {
        DiagnosticSpanLine {
            text: sf.get_line(line.line_index).map_or_else(String::new, |l| l.to_string()),
            highlight_start: line.start_col.0 + 1,
            highlight_end: line.end_col.0 + 1,
        }
    }

    fn solc_diagnostic(&mut self, diagnostic: &crate::diagnostics::Diagnostic) -> SolcDiagnostic {
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
                .map(|sub| self.solc_span(&sub.span, &file, Some(sub.label().into_owned())))
                .collect(),
            r#type: match severity {
                Severity::Error => match diagnostic.level {
                    Level::Bug => "InternalCompilerError",
                    Level::Fatal => "FatalError",
                    Level::Error => "Exception",
                    _ => unreachable!(),
                },
                Severity::Warning => "Warning",
                Severity::Info => "Info",
            }
            .into(),
            component: "general".into(),
            severity,
            error_code: diagnostic.id(),
            message: diagnostic.label().into_owned(),
            formatted_message: Some(self.emit_diagnostic_to_buffer(diagnostic)),
        }
    }

    fn solc_span(&self, span: &MultiSpan, file: &str, message: Option<String>) -> SourceLocation {
        let sm = &**self.source_map();
        let sp = span.primary_span();
        SourceLocation {
            file: sp
                .map(|span| {
                    let start = sm.lookup_char_pos(span.lo());
                    sm.filename_for_diagnostics(&start.file.name).to_string()
                })
                .unwrap_or_else(|| file.into()),
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

    fn emit_diagnostic_to_buffer(&mut self, diagnostic: &crate::diagnostics::Diagnostic) -> String {
        self.human_emitter.emit_diagnostic(diagnostic);
        let bytes = std::mem::take(&mut *self.buffer.0.lock().unwrap());
        String::from_utf8(bytes).expect("HumanEmitter wrote invalid UTF-8")
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

#[derive(Clone)]
struct LocalBuffer(Arc<Mutex<Vec<u8>>>);

impl io::Write for LocalBuffer {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).write(buf)
    }

    fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).write_vectored(bufs)
    }

    fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner).write_all(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

impl LocalBuffer {
    fn new() -> Self {
        Self(Arc::new(Mutex::new(Vec::new())))
    }
}

// Rustc-like JSON format.

#[derive(Serialize)]
#[serde(tag = "$message_type", rename_all = "snake_case")]
enum EmitTyped {
    Diagnostic(Diagnostic),
}

#[derive(Serialize)]
struct Diagnostic {
    /// The primary error message.
    message: String,
    code: Option<DiagnosticCode>,
    /// "error", "warning", "note", "help".
    level: &'static str,
    spans: Vec<DiagnosticSpan>,
    /// Associated diagnostic messages.
    children: Vec<Diagnostic>,
    /// The message as the compiler would render it.
    rendered: Option<String>,
}

#[derive(Serialize)]
struct DiagnosticSpan {
    file_name: String,
    byte_start: u32,
    byte_end: u32,
    /// 1-based.
    line_start: usize,
    line_end: usize,
    /// 1-based, character offset.
    column_start: usize,
    column_end: usize,
    /// Is this a "primary" span -- meaning the point, or one of the points,
    /// where the error occurred?
    is_primary: bool,
    /// Source text from the start of line_start to the end of line_end.
    text: Vec<DiagnosticSpanLine>,
    /// Label that should be placed at this location (if any)
    label: Option<String>,
    // /// If we are suggesting a replacement, this will contain text
    // /// that should be sliced in atop this span.
    // suggested_replacement: Option<String>,
}

#[derive(Serialize)]
struct DiagnosticSpanLine {
    text: String,

    /// 1-based, character offset in self.text.
    highlight_start: usize,

    highlight_end: usize,
}

#[derive(Serialize)]
struct DiagnosticCode {
    /// The code itself.
    code: String,
    /// An explanation for the code.
    explanation: Option<&'static str>,
}

// Solc JSON format.

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct SolcDiagnostic {
    source_location: Option<SourceLocation>,
    secondary_source_locations: Vec<SourceLocation>,
    r#type: String,
    component: String,
    severity: Severity,
    error_code: Option<String>,
    message: String,
    formatted_message: Option<String>,
}

#[derive(Serialize)]
struct SourceLocation {
    file: String,
    start: u32,
    end: u32,
    // Some if it's a secondary source location.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    message: Option<String>,
}

#[derive(Serialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    Error,
    Warning,
    Info,
}

fn to_severity(level: Level) -> Severity {
    match level {
        Level::Bug | Level::Fatal | Level::Error => Severity::Error,
        Level::Warning => Severity::Warning,
        #[rustfmt::skip]
        Level::Note | Level::OnceNote | Level::FailureNote |
        Level::Help | Level::OnceHelp |
        Level::Allow => Severity::Info,
    }
}

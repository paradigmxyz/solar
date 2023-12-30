#![allow(dead_code)]

use super::{Emitter, EmitterWriter};
use crate::{
    diagnostics::{ColorConfig, MultiSpan, SpanLabel},
    SourceMap, Span,
};
use serde::Serialize;
use std::{cell::RefCell, io, rc::Rc};
use sulk_data_structures::sync::Lrc;

/// Diagnostic emitter that emits diagnostics as JSON.
pub struct JsonEmitter {
    writer: Box<dyn io::Write>,
    source_map: Lrc<SourceMap>,
    pretty: bool,
}

impl Emitter for JsonEmitter {
    fn emit_diagnostic(&mut self, diagnostic: &crate::diagnostics::Diagnostic) {
        if let Err(e) = self.emit(&EmitTyped::Diagnostic(self.diagnostic(diagnostic))) {
            panic!("failed to emit diagnostic: {e}");
        }
    }

    fn source_map(&self) -> Option<&Lrc<SourceMap>> {
        Some(&self.source_map)
    }
}

impl JsonEmitter {
    /// Creates a new `JsonEmitter` that writes to given writer.
    pub fn new(writer: Box<dyn io::Write>, source_map: Lrc<SourceMap>) -> Self {
        Self { writer, source_map, pretty: false }
    }

    /// Sets whether to pretty print the JSON.
    pub fn pretty(mut self, pretty: bool) -> Self {
        self.pretty = pretty;
        self
    }

    fn diagnostic(&self, diagnostic: &crate::diagnostics::Diagnostic) -> Diagnostic {
        Diagnostic {
            message: diagnostic.label().into_owned(),
            code: diagnostic
                .code
                .as_ref()
                .map(|code| DiagnosticCode { code: code.id.to_string(), explanation: None }),
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
        msp.span_labels().into_iter().map(|label| self.span(&label)).collect()
    }

    fn span(&self, label: &SpanLabel) -> DiagnosticSpan {
        // TODO
        let _ = label;
        todo!()
        /*
        let span = label.span;
        DiagnosticSpan {
            file_name: je.sm.filename_for_diagnostics(&start.file.name).to_string(),
            byte_start: start.file.original_relative_byte_pos(span.lo()).0,
            byte_end: start.file.original_relative_byte_pos(span.hi()).0,
            line_start: start.line,
            line_end: end.line,
            column_start: start.col.0 + 1,
            column_end: end.col.0 + 1,
            is_primary: label.is_primary,
            text: self.span_line(span),
            label: label.label.as_ref().map(|msg| msg.as_str().into()),
        }
        */
    }

    fn span_line(&self, span: Span) -> DiagnosticSpanLine {
        // TODO
        let _ = span;
        todo!()
        /*
        DiagnosticSpanLine {
            text: sf.get_line(index).map_or_else(String::new, |l| l.into_owned()),
            highlight_start: h_start,
            highlight_end: h_end,
        }
        */
    }

    fn emit_diagnostic_to_buffer(&self, diagnostic: &crate::diagnostics::Diagnostic) -> String {
        #[derive(Clone)]
        struct LocalBuffer(Rc<RefCell<Vec<u8>>>);

        impl io::Write for LocalBuffer {
            fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
                self.0.borrow_mut().write(buf)
            }

            fn write_vectored(&mut self, bufs: &[io::IoSlice<'_>]) -> io::Result<usize> {
                self.0.borrow_mut().write_vectored(bufs)
            }

            fn write_all(&mut self, buf: &[u8]) -> io::Result<()> {
                self.0.borrow_mut().write_all(buf)
            }

            fn flush(&mut self) -> io::Result<()> {
                Ok(())
            }
        }

        impl LocalBuffer {
            fn new() -> Self {
                Self(Rc::new(RefCell::new(Vec::with_capacity(64))))
            }

            #[track_caller]
            fn unwrap(self) -> Vec<u8> {
                match Rc::try_unwrap(self.0) {
                    Ok(cell) => cell.into_inner(),
                    Err(_) => panic!("LocalBuffer::unwrap called with multiple references"),
                }
            }
        }

        let buffer = LocalBuffer::new();
        EmitterWriter::new(Box::new(buffer.clone()), ColorConfig::Never)
            .source_map(Some(self.source_map.clone()))
            .emit_diagnostic(diagnostic);
        let buffer = buffer.unwrap();
        String::from_utf8(buffer).expect("EmitterWriter wrote invalid UTF-8")
    }

    fn emit(&mut self, value: &EmitTyped) -> io::Result<()> {
        if self.pretty {
            serde_json::to_writer_pretty(&mut *self.writer, value)
        } else {
            serde_json::to_writer(&mut *self.writer, value)
        }?;
        self.writer.write_all(b"\n")?;
        self.writer.flush()
    }
}

// JSON format.

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
    /// The message as rustc would render it.
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
    /// If we are suggesting a replacement, this will contain text
    /// that should be sliced in atop this span.
    suggested_replacement: Option<String>,
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

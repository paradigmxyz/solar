//! These structs are a subset of the ones found in `rustc_errors::json`.
//! They are only used for deserialization of JSON output provided by libtest.

use crate::{
    errors::{Error, ErrorKind},
    ProcRes,
};
use serde::Deserialize;
use std::str::FromStr;

#[derive(Debug, Deserialize)]
struct Diagnostic {
    message: String,
    code: Option<DiagnosticCode>,
    level: String,
    spans: Vec<DiagnosticSpan>,
    children: Vec<Diagnostic>,
    rendered: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticSpan {
    file_name: String,
    line_start: usize,
    line_end: usize,
    column_start: usize,
    column_end: usize,
    is_primary: bool,
    label: Option<String>,
    suggested_replacement: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DiagnosticCode {
    /// The code itself.
    code: String,
}

pub fn extract_rendered(output: &str) -> String {
    output
        .lines()
        .filter_map(|line| {
            if line.starts_with('{') {
                if let Ok(diagnostic) = serde_json::from_str::<Diagnostic>(line) {
                    diagnostic.rendered
                } else {
                    panic!(
                        "failed to decode compiler output as json: line: {line}\noutput: {output}"
                    );
                }
            } else {
                // preserve non-JSON lines, such as ICEs
                Some(format!("{line}\n"))
            }
        })
        .collect()
}

pub fn parse_output(file_name: &str, output: &str, proc_res: &ProcRes) -> Vec<Error> {
    output.lines().flat_map(|line| parse_line(file_name, line, output, proc_res)).collect()
}

fn parse_line(file_name: &str, line: &str, output: &str, proc_res: &ProcRes) -> Vec<Error> {
    // The compiler sometimes intermingles non-JSON stuff into the
    // output.  This hack just skips over such lines. Yuck.
    if line.starts_with('{') {
        match serde_json::from_str::<Diagnostic>(line) {
            Ok(diagnostic) => {
                let mut expected_errors = vec![];
                push_expected_errors(&mut expected_errors, &diagnostic, &[], file_name);
                expected_errors
            }
            Err(error) => {
                proc_res.fatal(Some(&format!(
                    "failed to decode compiler output as json: \
                     `{error}`\nline: {line}\noutput: {output}"
                )));
            }
        }
    } else {
        vec![]
    }
}

fn push_expected_errors(
    expected_errors: &mut Vec<Error>,
    diagnostic: &Diagnostic,
    default_spans: &[&DiagnosticSpan],
    file_name: &str,
) {
    // In case of macro expansions, we need to get the span of the callsite
    let spans_info_in_this_file: Vec<_> = diagnostic
        .spans
        .iter()
        .map(|span| (span.is_primary, span))
        .filter(|(_, span)| file_name.contains(&span.file_name))
        .collect();

    let spans_in_this_file: Vec<_> =
        spans_info_in_this_file.iter().map(|(_, span)| *span).collect();

    let primary_spans: Vec<_> = spans_info_in_this_file
        .iter()
        .copied()
        .filter(|(is_primary, _)| *is_primary)
        .map(|(_, span)| span)
        .take(1) // sometimes we have more than one showing up in the json; pick first
        .collect();
    let primary_spans = if primary_spans.is_empty() {
        // subdiagnostics often don't have a span of their own;
        // inherit the span from the parent in that case
        default_spans
    } else {
        &primary_spans
    };

    // We break the output into multiple lines, and then append the
    // [E123] to every line in the output. This may be overkill.  The
    // intention was to match existing tests that do things like "//|
    // found `i32` [E123]" and expect to match that somewhere, and yet
    // also ensure that `//~ ERROR E123` *always* works. The
    // assumption is that these multi-line error messages are on their
    // way out anyhow.
    let with_code = |span: &DiagnosticSpan, text: &str| {
        match diagnostic.code {
            Some(ref code) =>
            // FIXME(#33000) -- it'd be better to use a dedicated
            // UI harness than to include the line/col number like
            // this, but some current tests rely on it.
            //
            // Note: Do NOT include the filename. These can easily
            // cause false matches where the expected message
            // appears in the filename, and hence the message
            // changes but the test still passes.
            {
                format!(
                    "{}:{}: {}:{}: {} [{}]",
                    span.line_start,
                    span.column_start,
                    span.line_end,
                    span.column_end,
                    text,
                    code.code.clone()
                )
            }
            None =>
            // FIXME(#33000) -- it'd be better to use a dedicated UI harness
            {
                format!(
                    "{}:{}: {}:{}: {}",
                    span.line_start, span.column_start, span.line_end, span.column_end, text
                )
            }
        }
    };

    // Convert multi-line messages into multiple expected
    // errors. We expect to replace these with something
    // more structured shortly anyhow.
    let mut message_lines = diagnostic.message.lines();
    if let Some(first_line) = message_lines.next() {
        for span in primary_spans {
            let msg = with_code(span, first_line);
            let kind = ErrorKind::from_str(&diagnostic.level).ok();
            expected_errors.push(Error { line_num: span.line_start, kind, msg, solc_kind: None });
        }
    }
    for next_line in message_lines {
        for span in primary_spans {
            expected_errors.push(Error {
                line_num: span.line_start,
                kind: None,
                msg: with_code(span, next_line),
                solc_kind: None,
            });
        }
    }

    // If the message has a suggestion, register that.
    for span in primary_spans {
        if let Some(ref suggested_replacement) = span.suggested_replacement {
            for (index, line) in suggested_replacement.lines().enumerate() {
                expected_errors.push(Error {
                    line_num: span.line_start + index,
                    kind: Some(ErrorKind::Suggestion),
                    msg: line.to_string(),
                    solc_kind: None,
                });
            }
        }
    }

    // Add notes for any labels that appear in the message.
    for span in spans_in_this_file.iter().filter(|span| span.label.is_some()) {
        expected_errors.push(Error {
            line_num: span.line_start,
            kind: Some(ErrorKind::Note),
            msg: span.label.clone().unwrap(),
            solc_kind: None,
        });
    }

    // Flatten out the children.
    for child in &diagnostic.children {
        push_expected_errors(expected_errors, child, primary_spans, file_name);
    }
}
